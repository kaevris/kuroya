use crate::{
    KuroyaApp,
    background_image::{BackgroundImageState, LoadedBackgroundImage, load_background_image},
    background_image_animation::{BackgroundGifAnimation, BackgroundGifAnimationUpdate},
    image_preview::path_is_image_preview,
    path_display::display_error_label_cow,
    ui_event_channel::send_critical_ui_event,
    ui_events::UiEvent,
    workspace_state::paths_match_exact_or_lexically,
};
use eframe::egui::{Context, LayerId, Painter, Rect, Ui};
use kuroya_core::{EditorBackgroundImageScope, EditorSettings};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const BACKGROUND_IMAGE_DECODE_CONCURRENCY: usize = 1;

const BACKGROUND_IMAGE_FAILURE_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingBackgroundImageLoad {
    request_id: u64,
    path: PathBuf,
}

pub(crate) struct BackgroundImageRuntime {
    state: Option<BackgroundImageState>,
    animation: Option<BackgroundGifAnimation>,
    pending: Option<PendingBackgroundImageLoad>,
    failed: Option<(PathBuf, Instant)>,
    configuration_error: Option<BackgroundImageConfigurationError>,
    request_generation: Arc<AtomicU64>,
    decode_gate: Arc<Semaphore>,
    repaint_context: Option<Context>,
}

impl Default for BackgroundImageRuntime {
    fn default() -> Self {
        Self {
            state: None,
            animation: None,
            pending: None,
            failed: None,
            configuration_error: None,
            request_generation: Arc::new(AtomicU64::new(0)),
            decode_gate: Arc::new(Semaphore::new(BACKGROUND_IMAGE_DECODE_CONCURRENCY)),
            repaint_context: None,
        }
    }
}

impl BackgroundImageRuntime {
    fn set_repaint_context(&mut self, ctx: &Context) {
        self.repaint_context = Some(ctx.clone());
    }

    fn invalidate(&mut self, clear_state: bool) -> bool {
        if self.pending.is_none() && (!clear_state || self.state.is_none()) {
            return false;
        }
        self.request_generation.fetch_add(1, Ordering::AcqRel);
        self.pending = None;
        if clear_state {
            self.state = None;
            self.animation = None;
        }
        true
    }

    fn loaded_path_matches(&self, path: &Path) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| state.matches_source_path(path))
    }

    fn pending_path_matches(&self, path: &Path) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| paths_match_exact_or_lexically(&pending.path, path))
    }

    fn accepts_event(&self, request_id: u64, path: &Path) -> bool {
        self.request_generation.load(Ordering::Acquire) == request_id
            && self.pending.as_ref().is_some_and(|pending| {
                pending.request_id == request_id
                    && paths_match_exact_or_lexically(&pending.path, path)
            })
    }

    fn note_load_failure(&mut self, path: &Path) {
        self.failed = Some((path.to_path_buf(), Instant::now()));
    }

    fn clear_load_failure(&mut self) {
        self.failed = None;
    }

    fn load_failure_cooldown_active(&self, path: &Path) -> bool {
        self.failed
            .as_ref()
            .is_some_and(|(failed_path, failed_at)| {
                paths_match_exact_or_lexically(failed_path, path)
                    && failed_at.elapsed() < BACKGROUND_IMAGE_FAILURE_COOLDOWN
            })
    }
}

impl KuroyaApp {
    pub(crate) fn set_background_image_repaint_context(&mut self, ctx: &Context) {
        self.background_image_runtime.set_repaint_context(ctx);
    }

    pub(crate) fn sync_background_image(&mut self, force: bool) {
        let configured_path =
            match configured_background_image_path(self.effective_background_image_settings()) {
                Ok(Some(path)) => path.to_path_buf(),
                Ok(None) => {
                    self.background_image_runtime.configuration_error = None;
                    self.background_image_runtime.clear_load_failure();
                    if !self.settings_panel_open {
                        self.background_image_runtime.invalidate(true);
                    }
                    return;
                }
                Err(error) => {
                    self.background_image_runtime.clear_load_failure();
                    if !self.settings_panel_open {
                        self.background_image_runtime.invalidate(true);
                    }
                    if self.background_image_runtime.configuration_error != Some(error) {
                        self.status = error.status().to_owned();
                    }
                    self.background_image_runtime.configuration_error = Some(error);
                    return;
                }
            };
        self.background_image_runtime.configuration_error = None;
        self.advance_background_image_animation();

        if !force
            && self
                .background_image_runtime
                .loaded_path_matches(&configured_path)
        {
            if self.background_image_runtime.pending.is_some() {
                self.background_image_runtime.invalidate(false);
            }
            return;
        }
        if !force
            && self
                .background_image_runtime
                .pending_path_matches(&configured_path)
        {
            return;
        }

        if !force
            && self
                .background_image_runtime
                .load_failure_cooldown_active(&configured_path)
        {
            return;
        }

        if !self
            .background_image_runtime
            .loaded_path_matches(&configured_path)
        {
            self.background_image_runtime.state = None;
        }

        let request_id = self
            .background_image_runtime
            .request_generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        self.background_image_runtime.animation = None;
        self.background_image_runtime.pending = Some(PendingBackgroundImageLoad {
            request_id,
            path: configured_path.clone(),
        });
        self.background_image_runtime.clear_load_failure();

        let tx = self.tx.clone();
        let request_generation = Arc::clone(&self.background_image_runtime.request_generation);
        let decode_gate = Arc::clone(&self.background_image_runtime.decode_gate);
        let repaint_context = self.background_image_runtime.repaint_context.clone();
        let loop_enabled = self
            .effective_background_image_settings()
            .background_image_loop;
        self.runtime.spawn(async move {
            let Ok(_permit) = decode_gate.acquire_owned().await else {
                return;
            };
            if request_generation.load(Ordering::Acquire) != request_id {
                return;
            }

            let result =
                load_background_image(&configured_path, repaint_context.clone(), loop_enabled)
                    .await;
            if request_generation.load(Ordering::Acquire) != request_id {
                return;
            }

            let event = match result {
                Ok(loaded) => UiEvent::EditorBackgroundImageLoaded {
                    request_id,
                    path: configured_path,
                    loaded,
                },
                Err(error) => UiEvent::EditorBackgroundImageLoadFailed {
                    request_id,
                    path: configured_path,
                    error,
                },
            };
            if send_critical_ui_event(&tx, event) {
                if let Some(ctx) = repaint_context {
                    ctx.request_repaint();
                }
            }
        });
    }

    pub(crate) fn apply_background_image_loaded(
        &mut self,
        request_id: u64,
        path: PathBuf,
        loaded: LoadedBackgroundImage,
    ) -> bool {
        if !self
            .background_image_runtime
            .accepts_event(request_id, &path)
            || !configured_background_image_path(self.effective_background_image_settings())
                .ok()
                .flatten()
                .is_some_and(|configured| paths_match_exact_or_lexically(configured, &path))
        {
            return false;
        }

        let (preview, animation) = match loaded {
            LoadedBackgroundImage::Static(preview) => (preview, None),
            LoadedBackgroundImage::AnimatedGif(loaded) => {
                let mut animation = loaded.animation;
                let paused = self.background_image_animation_paused();
                animation.activate(
                    loaded.first_frame_delay,
                    self.background_image_runtime.repaint_context.as_ref(),
                    paused,
                );
                (loaded.first_frame, Some(animation))
            }
        };
        self.background_image_runtime.state =
            Some(BackgroundImageState::from_loaded(path, preview));
        self.background_image_runtime.animation = animation;
        self.background_image_runtime.pending = None;
        self.background_image_runtime.clear_load_failure();
        true
    }

    fn advance_background_image_animation(&mut self) {
        let Some(ctx) = self.background_image_runtime.repaint_context.clone() else {
            return;
        };
        let paused = ctx.input(|input| input.viewport().minimized.unwrap_or(false));
        let loop_enabled = self
            .effective_background_image_settings()
            .background_image_loop;
        let update = self
            .background_image_runtime
            .animation
            .as_mut()
            .and_then(|animation| animation.poll(&ctx, paused, loop_enabled));
        match update {
            Some(BackgroundGifAnimationUpdate::Frame(preview)) => {
                if !self
                    .background_image_runtime
                    .state
                    .as_mut()
                    .is_some_and(|state| state.replace_loaded(&ctx, preview))
                {
                    self.background_image_runtime.animation = None;
                    self.status = background_image_load_failure_status(
                        "animated GIF produced an invalid frame",
                    );
                }
            }
            Some(BackgroundGifAnimationUpdate::Failed(error)) => {
                self.background_image_runtime.animation = None;
                self.status = background_image_load_failure_status(&error);
            }
            Some(BackgroundGifAnimationUpdate::Finished) => {}
            None => {}
        }
    }

    fn background_image_animation_paused(&self) -> bool {
        self.background_image_runtime
            .repaint_context
            .as_ref()
            .is_some_and(|ctx| ctx.input(|input| input.viewport().minimized.unwrap_or(false)))
    }

    pub(crate) fn apply_background_image_load_failed(
        &mut self,
        request_id: u64,
        path: &Path,
        error: &str,
    ) -> bool {
        if !self
            .background_image_runtime
            .accepts_event(request_id, path)
            || !configured_background_image_path(self.effective_background_image_settings())
                .ok()
                .flatten()
                .is_some_and(|configured| paths_match_exact_or_lexically(configured, path))
        {
            return false;
        }

        self.background_image_runtime.pending = None;
        self.background_image_runtime.note_load_failure(path);
        self.status = background_image_load_failure_status(error);
        true
    }

    pub(crate) fn background_image_is_ready(&self) -> bool {
        background_image_runtime_is_ready(
            &self.background_image_runtime,
            self.effective_background_image_settings(),
        )
    }

    pub(crate) fn render_background_image(&mut self, ctx: &Context) {
        self.background_image_runtime.set_repaint_context(ctx);
        if !self.full_app_background_image_is_ready() {
            return;
        }

        let bounds = ctx.content_rect();
        let painter = ctx.layer_painter(LayerId::background());
        self.paint_background_image(ctx, &painter, bounds);
    }

    pub(crate) fn render_editor_background_image(&mut self, ui: &Ui) {
        self.background_image_runtime.set_repaint_context(ui.ctx());
        if !self.editor_background_image_is_ready() {
            return;
        }

        self.paint_background_image(ui.ctx(), ui.painter(), ui.max_rect());
    }

    pub(crate) fn full_app_background_image_is_ready(&self) -> bool {
        self.background_image_is_ready()
            && self
                .effective_background_image_settings()
                .background_image_scope
                == EditorBackgroundImageScope::FullApp
    }

    fn editor_background_image_is_ready(&self) -> bool {
        self.background_image_is_ready()
            && self
                .effective_background_image_settings()
                .background_image_scope
                == EditorBackgroundImageScope::Editor
    }

    fn paint_background_image(&mut self, ctx: &Context, painter: &Painter, bounds: Rect) {
        if !bounds.is_positive() {
            return;
        }
        let base_color = ctx.style().visuals.code_bg_color;
        let (fit, position, dim) = {
            let settings = self.effective_background_image_settings();
            (
                settings.background_image_fit,
                settings.background_image_position,
                settings.background_image_dim,
            )
        };
        painter.rect_filled(bounds, 0.0, base_color);
        if let Some(state) = self.background_image_runtime.state.as_mut() {
            state.paint(ctx, painter, bounds, fit, position, dim, base_color);
        }
    }

    fn effective_background_image_settings(&self) -> &EditorSettings {
        if self.settings_panel_open {
            &self.settings_panel_draft
        } else {
            &self.settings
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackgroundImageConfigurationError {
    RelativePath,
    UnsupportedFormat,
}

impl BackgroundImageConfigurationError {
    fn status(self) -> &'static str {
        match self {
            Self::RelativePath => "Editor background image path must be absolute",
            Self::UnsupportedFormat => "Editor background image format is not supported",
        }
    }
}

fn configured_background_image_path(
    settings: &EditorSettings,
) -> Result<Option<&Path>, BackgroundImageConfigurationError> {
    if !settings.background_image_enabled {
        return Ok(None);
    }
    let Some(path) = settings
        .background_image_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(Path::new)
    else {
        return Ok(None);
    };
    if !path.is_absolute() {
        return Err(BackgroundImageConfigurationError::RelativePath);
    }
    if !path_is_image_preview(path) {
        return Err(BackgroundImageConfigurationError::UnsupportedFormat);
    }
    Ok(Some(path))
}

fn background_image_runtime_is_ready(
    runtime: &BackgroundImageRuntime,
    settings: &EditorSettings,
) -> bool {
    configured_background_image_path(settings)
        .ok()
        .flatten()
        .is_some_and(|path| runtime.loaded_path_matches(path))
}

fn background_image_load_failure_status(error: &str) -> String {
    let error = display_error_label_cow(error);
    format!("Could not load editor background image: {}", error.as_ref())
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundImageRuntime, PendingBackgroundImageLoad, background_image_load_failure_status,
        background_image_runtime_is_ready, configured_background_image_path,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, background_image::BackgroundImageState,
        image_preview::LoadedImagePreview, terminal::TerminalPane,
    };
    use image::{Rgba, RgbaImage};
    use kuroya_core::{
        EditorBackgroundImageFit, EditorBackgroundImagePosition, EditorBackgroundImageScope,
        EditorSettings, Workspace,
    };
    use std::{
        path::{Path, PathBuf},
        sync::atomic::Ordering,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn configured_path_requires_enablement_absolute_path_and_supported_format() {
        let mut settings = EditorSettings::default();
        assert_eq!(configured_background_image_path(&settings), Ok(None));

        settings.background_image_enabled = true;
        settings.background_image_path = Some("relative/background.png".to_owned());
        assert!(configured_background_image_path(&settings).is_err());

        let absolute = absolute_test_path("background.txt");
        settings.background_image_path = Some(absolute.to_string_lossy().into_owned());
        assert!(configured_background_image_path(&settings).is_err());

        let absolute = absolute_test_path("background.png");
        settings.background_image_path = Some(absolute.to_string_lossy().into_owned());
        assert_eq!(
            configured_background_image_path(&settings),
            Ok(Some(absolute.as_path()))
        );
    }

    #[test]
    fn stale_or_mismatched_events_are_rejected() {
        let path = absolute_test_path("background.png");
        let other = absolute_test_path("other.png");
        let mut runtime = BackgroundImageRuntime::default();
        assert!(!runtime.invalidate(true));
        assert_eq!(runtime.request_generation.load(Ordering::Acquire), 0);
        runtime.request_generation.store(7, Ordering::Release);
        runtime.pending = Some(PendingBackgroundImageLoad {
            request_id: 7,
            path: path.clone(),
        });

        assert!(runtime.accepts_event(7, &path));
        assert!(!runtime.accepts_event(6, &path));
        assert!(!runtime.accepts_event(7, &other));
        runtime.invalidate(false);
        assert!(!runtime.accepts_event(7, &path));
    }

    #[test]
    fn readiness_requires_enabled_matching_loaded_path() {
        let path = absolute_test_path("background.png");
        let mut settings = EditorSettings {
            background_image_enabled: true,
            background_image_path: Some(path.to_string_lossy().into_owned()),
            ..EditorSettings::default()
        };
        let runtime = BackgroundImageRuntime {
            state: Some(BackgroundImageState::from_loaded(
                path.clone(),
                LoadedImagePreview {
                    width: 1,
                    height: 1,
                    rgba: Some(vec![255; 4]),
                    byte_len: 4,
                },
            )),
            ..BackgroundImageRuntime::default()
        };

        assert!(background_image_runtime_is_ready(&runtime, &settings));
        settings.background_image_enabled = false;
        assert!(!background_image_runtime_is_ready(&runtime, &settings));
        settings.background_image_enabled = true;
        settings.background_image_path =
            Some(absolute_test_path("other.png").display().to_string());
        assert!(!background_image_runtime_is_ready(&runtime, &settings));
    }

    #[test]
    fn load_failure_status_is_sanitized() {
        let status = background_image_load_failure_status("decode failed\nsecret detail\u{202e}");
        assert_eq!(
            status,
            "Could not load editor background image: decode failed secret detail"
        );
    }

    #[test]
    fn failed_load_latches_and_does_not_respawn_until_path_changes() {
        let root = temp_root("failure-latch");
        std::fs::create_dir_all(&root).unwrap();
        let missing = root.join("missing.png");
        let replacement = root.join("replacement.png");
        write_test_image(&replacement, [10, 180, 60, 255]);

        let settings = EditorSettings {
            background_image_enabled: true,
            background_image_path: Some(missing.display().to_string()),
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings);

        app.sync_background_image(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.background_image_runtime.pending.is_some() && Instant::now() < deadline {
            app.handle_events();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.handle_events();

        assert!(app.background_image_runtime.pending.is_none());
        assert!(
            app.background_image_runtime
                .failed
                .as_ref()
                .is_some_and(|(path, _)| *path == missing),
            "a failed load must latch the configured path"
        );

        for _ in 0..3 {
            app.sync_background_image(false);
            assert!(app.background_image_runtime.pending.is_none());
        }

        app.settings.background_image_path = Some(replacement.display().to_string());
        app.sync_background_image(false);
        assert!(app.background_image_runtime.failed.is_none());

        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.background_image_is_ready() && Instant::now() < deadline {
            app.handle_events();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.handle_events();

        assert!(app.background_image_is_ready());
        assert!(app.background_image_runtime.failed.is_none());
        assert!(app.background_image_runtime.pending.is_none());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_accepts_only_latest_image_and_releases_state_when_disabled() {
        let root = temp_root("latest-load");
        std::fs::create_dir_all(&root).unwrap();
        let first = root.join("first.png");
        let second = root.join("second.png");
        write_test_image(&first, [220, 20, 30, 255]);
        write_test_image(&second, [10, 180, 60, 255]);

        let settings = EditorSettings {
            background_image_enabled: true,
            background_image_path: Some(first.display().to_string()),
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings);
        app.sync_background_image(false);
        app.settings.background_image_path = Some(second.display().to_string());
        app.sync_background_image(false);

        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.background_image_is_ready() && Instant::now() < deadline {
            app.handle_events();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.handle_events();

        assert!(app.background_image_is_ready());
        let state = app
            .background_image_runtime
            .state
            .as_ref()
            .expect("latest image should load");
        assert!(state.matches_source_path(&second));
        assert!(!state.matches_source_path(&first));
        assert!(app.background_image_runtime.pending.is_none());

        app.settings.background_image_enabled = false;
        app.sync_background_image(false);
        assert!(!app.background_image_is_ready());
        assert!(app.background_image_runtime.state.is_none());
        assert!(app.background_image_runtime.pending.is_none());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn open_settings_draft_previews_without_persisting_and_cancel_restores_saved_state() {
        let root = temp_root("settings-draft-preview");
        std::fs::create_dir_all(&root).unwrap();
        let image_path = root.join("preview.png");
        write_test_image(&image_path, [40, 90, 180, 255]);
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        let saved_dim = app.settings.background_image_dim;
        app.settings_panel_open = true;
        app.settings_panel_draft.background_image_enabled = true;
        app.settings_panel_draft.background_image_path = Some(image_path.display().to_string());
        app.settings_panel_draft.background_image_dim = 0.83;
        app.settings_panel_draft.background_image_fit = EditorBackgroundImageFit::Stretch;
        app.settings_panel_draft.background_image_position = EditorBackgroundImagePosition::Top;
        app.settings_panel_draft.background_image_scope = EditorBackgroundImageScope::FullApp;

        app.sync_background_image(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.background_image_is_ready() && Instant::now() < deadline {
            app.handle_events();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.handle_events();

        assert!(app.background_image_is_ready());
        assert!(app.full_app_background_image_is_ready());
        assert!(!app.editor_background_image_is_ready());
        assert!(!app.settings.background_image_enabled);
        assert!(app.settings.background_image_path.is_none());
        assert_eq!(app.settings.background_image_dim, saved_dim);
        let effective = app.effective_background_image_settings();
        assert_eq!(effective.background_image_dim, 0.83);
        assert_eq!(
            effective.background_image_fit,
            EditorBackgroundImageFit::Stretch
        );
        assert_eq!(
            effective.background_image_position,
            EditorBackgroundImagePosition::Top
        );
        assert_eq!(
            effective.background_image_scope,
            EditorBackgroundImageScope::FullApp
        );

        app.settings_panel_draft.background_image_scope = EditorBackgroundImageScope::Editor;
        assert!(!app.full_app_background_image_is_ready());
        assert!(app.editor_background_image_is_ready());
        assert!(app.background_image_runtime.pending.is_none());

        app.settings_panel_open = false;
        app.sync_background_image(false);

        assert!(!app.background_image_is_ready());
        assert!(app.background_image_runtime.state.is_none());
        assert!(app.background_image_runtime.pending.is_none());
        assert!(!app.settings.background_image_enabled);
        assert!(app.settings.background_image_path.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reset_preview_keeps_loaded_image_while_settings_panel_is_open() {
        let root = temp_root("reset-preview-keeps-image");
        std::fs::create_dir_all(&root).unwrap();
        let image_path = root.join("wallpaper.png");
        write_test_image(&image_path, [200, 120, 40, 255]);
        let settings = EditorSettings {
            background_image_enabled: true,
            background_image_path: Some(image_path.display().to_string()),
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings);
        app.sync_background_image(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.background_image_is_ready() && Instant::now() < deadline {
            app.handle_events();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.handle_events();

        assert!(app.background_image_is_ready());

        app.settings_panel_open = true;
        app.settings_panel_draft.background_image_enabled = false;
        app.sync_background_image(false);
        assert!(!app.background_image_is_ready());
        assert!(app.background_image_runtime.state.is_some());
        assert!(app.background_image_runtime.pending.is_none());

        app.settings_panel_open = false;
        app.sync_settings_panel_inputs();
        app.sync_background_image(false);
        assert!(app.background_image_is_ready());
        let state = app
            .background_image_runtime
            .state
            .as_ref()
            .expect("loaded image should survive the reset preview");
        assert!(state.matches_source_path(&image_path));
        assert!(app.background_image_runtime.pending.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    fn absolute_test_path(file_name: &str) -> PathBuf {
        if cfg!(windows) {
            Path::new(r"C:\images").join(file_name)
        } else {
            Path::new("/images").join(file_name)
        }
    }

    fn write_test_image(path: &Path, color: [u8; 4]) {
        RgbaImage::from_pixel(3, 2, Rgba(color))
            .save(path)
            .expect("test image should save");
    }

    fn app_for_test(root: PathBuf, settings: EditorSettings) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-background-image-runtime-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}

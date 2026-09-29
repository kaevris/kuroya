use crate::{
    KuroyaApp,
    status_bar::{status_bar_message, status_message_is_persistent},
    ui_icons::IconKind,
};
use eframe::egui::{
    self, Align2, Area, Color32, Context, CornerRadius, CursorIcon, FontId, Id, Order, Rect, Sense,
    Stroke, Vec2,
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const TOAST_TTL: Duration = Duration::from_secs(6);
const TOAST_ERROR_TTL: Duration = Duration::from_secs(30);
const TOAST_FADE_SECS: f32 = 0.4;
const MAX_TOASTS: usize = 5;
const TOAST_WIDTH: f32 = 340.0;
const TOAST_PADDING: f32 = 10.0;
const TOAST_GAP: f32 = 8.0;
const TOAST_ACCENT_WIDTH: f32 = 3.0;
const TOAST_ICON_SLOT: f32 = 24.0;
const TOAST_TEXT_FONT_SIZE: f32 = 13.5;

/// Toast category ids. This is the single authoritative list; the values are
/// persisted in `EditorSettings::muted_notifications`, so they must stay stable.
/// Every toast carries exactly one category; toasts emitted without an explicit
/// category default to [`TOAST_CATEGORY_GENERAL`]. Muting is display-only: the
/// status bar text still updates, only the toast popup is suppressed.
pub(crate) const TOAST_CATEGORY_GENERAL: &str = "general";
pub(crate) const TOAST_CATEGORY_LSP: &str = "lsp";
pub(crate) const TOAST_CATEGORY_LSP_INSTALL: &str = "lsp-install";
pub(crate) const TOAST_CATEGORY_UPDATE: &str = "update";
pub(crate) const TOAST_CATEGORY_BACKGROUND_IMAGE: &str = "background-image";
pub(crate) const TOAST_CATEGORY_INDEXING: &str = "indexing";
pub(crate) const TOAST_CATEGORY_GIT: &str = "git";
pub(crate) const TOAST_CATEGORY_SLOW_FRAMES: &str = "slow-frames";
pub(crate) const TOAST_CATEGORY_PLUGINS: &str = "plugins";

fn toast_category_is_muted(muted_notifications: &[String], category: &str) -> bool {
    muted_notifications.iter().any(|muted| muted == category)
}

#[derive(Debug, Clone)]
pub(crate) struct StatusToast {
    pub(crate) id: u64,
    pub(crate) message: String,
    pub(crate) error: bool,
    pub(crate) created: Instant,
    pub(crate) category: &'static str,
}

static NEXT_TOAST_ID: AtomicU64 = AtomicU64::new(1);

fn next_toast_id() -> u64 {
    NEXT_TOAST_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn toast_ttl(error: bool) -> Duration {
    if error { TOAST_ERROR_TTL } else { TOAST_TTL }
}

pub(crate) fn toast_is_alive(created: Instant, now: Instant, error: bool) -> bool {
    now.duration_since(created) < toast_ttl(error)
}

fn toast_alpha(created: Instant, error: bool) -> f32 {
    let elapsed = created.elapsed().as_secs_f32();
    let ttl = toast_ttl(error).as_secs_f32();
    if elapsed >= ttl {
        return 0.0;
    }
    let fade = ((ttl - elapsed) / TOAST_FADE_SECS).clamp(0.0, 1.0);
    let slide_in = (elapsed / 0.16).clamp(0.0, 1.0);
    fade * slide_in
}

fn alpha_color(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (color.a() as f32 * alpha).clamp(0.0, 255.0) as u8,
    )
}

fn toast_text_wrap_width() -> f32 {
    TOAST_WIDTH - TOAST_PADDING * 2.0 - TOAST_ACCENT_WIDTH - TOAST_ICON_SLOT
}

impl KuroyaApp {
    pub(crate) fn set_status_with_toast(&mut self, status: impl Into<String>) {
        self.set_status_with_toast_in_category(TOAST_CATEGORY_GENERAL, status);
    }

    /// Sets the status text and shows it as a toast tagged with `category`.
    /// The toast is suppressed when the category is muted in
    /// `settings.muted_notifications`; the status bar text always updates.
    pub(crate) fn set_status_with_toast_in_category(
        &mut self,
        category: &'static str,
        status: impl Into<String>,
    ) {
        self.status = status.into();
        self.pending_status_category = Some(category);
        self.ingest_status_toast();
    }

    pub(crate) fn ingest_status_toast(&mut self) {
        if self.status == self.last_status {
            // A pending category always belongs to the current status value;
            // if that value produced no toast, the tag must not leak into the
            // next, unrelated status change.
            self.pending_status_category = None;
            return;
        }
        self.last_status = self.status.clone();
        self.status_shown_since = Instant::now();

        let category = self
            .pending_status_category
            .take()
            .unwrap_or(TOAST_CATEGORY_GENERAL);
        let message = status_bar_message(&self.status).to_string();
        if message.is_empty() {
            return;
        }
        if let Some(head) = self.status_toasts.first_mut()
            && head.message == message
        {
            head.created = Instant::now();
            return;
        }
        let error = status_message_is_persistent(&self.status);
        if toast_category_is_muted(&self.settings.muted_notifications, category) {
            self.drop_toasts_in_category(category);
            return;
        }
        self.status_toasts.insert(
            0,
            StatusToast {
                id: next_toast_id(),
                message,
                error,
                created: Instant::now(),
                category,
            },
        );
        self.status_toasts.truncate(MAX_TOASTS);
    }

    /// Updates (or creates) a single in-place progress toast tagged with
    /// `category`, e.g. a download progress toast. Muted categories neither
    /// create nor refresh toasts, and remove any toast already showing for
    /// that category; the status bar text still updates.
    pub(crate) fn set_status_updating_toast_in_category(
        &mut self,
        category: &'static str,
        toast_prefix: &str,
        status: String,
    ) {
        self.status = status.clone();
        self.last_status = status.clone();
        self.status_shown_since = Instant::now();

        let message = status_bar_message(&status).to_string();
        if message.is_empty() {
            return;
        }
        let error = status_message_is_persistent(&status);
        if toast_category_is_muted(&self.settings.muted_notifications, category) {
            self.drop_toasts_in_category(category);
            return;
        }
        let existing = self
            .status_toasts
            .iter_mut()
            .find(|toast| toast.message.starts_with(toast_prefix));
        if let Some(toast) = existing {
            toast.message = message;
            toast.error = error;
            toast.created = Instant::now();
            return;
        }
        self.status_toasts.insert(
            0,
            StatusToast {
                id: next_toast_id(),
                message,
                error,
                created: Instant::now(),
                category,
            },
        );
        self.status_toasts.truncate(MAX_TOASTS);
    }

    fn drop_toasts_in_category(&mut self, category: &str) {
        self.status_toasts
            .retain(|toast| toast.category != category);
    }

    pub(crate) fn render_status_toasts(&mut self, ctx: &Context) {
        let now = Instant::now();
        self.status_toasts
            .retain(|toast| toast_is_alive(toast.created, now, toast.error));
        if self.status_toasts.is_empty() {
            return;
        }

        let visuals = ctx.style().visuals.clone();
        let mut dismissed_ids: Vec<u64> = Vec::new();

        Area::new(Id::new("status-toast-stack"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_TOP, [-16.0, 16.0])
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_width(TOAST_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, TOAST_GAP);

                for index in 0..self.status_toasts.len() {
                    let Some(toast) = self.status_toasts.get(index).cloned() else {
                        break;
                    };
                    let alpha = toast_alpha(toast.created, toast.error);
                    if alpha <= 0.0 {
                        continue;
                    }

                    let accent = if toast.error {
                        visuals.warn_fg_color
                    } else {
                        visuals.selection.stroke.color
                    };
                    let text_color = visuals.text_color();
                    let bg = alpha_color(visuals.window_fill, alpha);
                    let stroke = Stroke::new(
                        1.0_f32,
                        alpha_color(visuals.widgets.noninteractive.bg_stroke.color, alpha),
                    );

                    let galley = ui.painter().layout(
                        toast.message.clone(),
                        FontId::proportional(TOAST_TEXT_FONT_SIZE),
                        text_color,
                        toast_text_wrap_width(),
                    );
                    let height = galley.size().y.max(18.0) + TOAST_PADDING * 2.0;

                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(TOAST_WIDTH, height), Sense::hover());
                    let response =
                        ui.interact(rect, Id::new(("status-toast", toast.id)), Sense::click());
                    if response.clicked() {
                        dismissed_ids.push(toast.id);
                    }
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }

                    ui.painter().rect_filled(rect, CornerRadius::same(8), bg);
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(8),
                        stroke,
                        egui::StrokeKind::Inside,
                    );

                    let accent_rect = Rect::from_min_max(
                        rect.left_top() + Vec2::new(TOAST_PADDING * 0.4, TOAST_PADDING * 0.5),
                        rect.left_bottom()
                            + Vec2::new(
                                TOAST_PADDING * 0.4 + TOAST_ACCENT_WIDTH,
                                -TOAST_PADDING * 0.5,
                            ),
                    );
                    ui.painter()
                        .rect_filled(accent_rect, CornerRadius::same(2), accent);

                    let icon_rect = Rect::from_center_size(
                        rect.left_center()
                            + Vec2::new(
                                TOAST_ACCENT_WIDTH + TOAST_PADDING + TOAST_ICON_SLOT * 0.5,
                                0.0,
                            ),
                        Vec2::splat(15.0),
                    );
                    crate::ui_icon_shapes::draw_icon(
                        ui,
                        icon_rect,
                        if toast.error {
                            IconKind::Diagnostics
                        } else {
                            IconKind::Code
                        },
                        accent,
                    );

                    ui.painter().galley(
                        rect.left_top()
                            + Vec2::new(
                                TOAST_ACCENT_WIDTH + TOAST_PADDING + TOAST_ICON_SLOT,
                                TOAST_PADDING,
                            ),
                        galley,
                        text_color,
                    );
                }
            });

        if !dismissed_ids.is_empty() {
            self.status_toasts
                .retain(|toast| !dismissed_ids.contains(&toast.id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TOAST_CATEGORY_BACKGROUND_IMAGE, TOAST_CATEGORY_GENERAL, TOAST_CATEGORY_GIT,
        TOAST_CATEGORY_INDEXING, TOAST_CATEGORY_LSP, TOAST_CATEGORY_LSP_INSTALL,
        TOAST_CATEGORY_UPDATE,
    };
    use crate::{KuroyaApp, app_startup_context::AppStartupContext, terminal::TerminalPane};
    use kuroya_core::{EditorSettings, Workspace};
    use std::{path::PathBuf, time::Duration};
    use tokio::runtime::Runtime;

    #[test]
    fn set_status_with_toast_enqueues_toast_without_render() {
        let mut app = app_for_test();

        app.set_status_with_toast("Saved settings".to_owned());

        assert_eq!(app.status, "Saved settings");
        assert_eq!(app.status_toasts.len(), 1);
        assert_eq!(app.status_toasts[0].message, "Saved settings");
        assert!(!app.status_toasts[0].error);
    }

    #[test]
    fn same_frame_error_then_benign_statuses_each_produce_a_toast() {
        let mut app = app_for_test();

        app.set_status_with_toast("Could not save main.rs: disk full".to_owned());
        app.set_status_with_toast("Saved settings".to_owned());

        assert_eq!(app.status, "Saved settings");
        assert_eq!(app.status_toasts.len(), 2);
        assert_eq!(app.status_toasts[0].message, "Saved settings");
        assert!(!app.status_toasts[0].error);
        assert_eq!(
            app.status_toasts[1].message,
            "Could not save main.rs: disk full"
        );
        assert!(app.status_toasts[1].error);
    }

    #[test]
    fn duplicate_head_toast_refreshes_created_timestamp() {
        let mut app = app_for_test();
        app.set_status_with_toast("Could not save main.rs: disk full".to_owned());
        let first_id = app.status_toasts[0].id;
        let first_created = app.status_toasts[0].created;

        std::thread::sleep(Duration::from_millis(5));
        app.set_status_with_toast("  Could not save main.rs: disk full  ".to_owned());

        assert_eq!(app.status_toasts.len(), 1);
        assert_eq!(app.status_toasts[0].id, first_id);
        assert!(
            app.status_toasts[0].created > first_created,
            "duplicate head toast should refresh its created timestamp"
        );
    }

    #[test]
    fn ingest_ignores_unchanged_status_between_frames() {
        let mut app = app_for_test();
        app.set_status_with_toast("Saved settings".to_owned());

        app.ingest_status_toast();

        assert_eq!(app.status_toasts.len(), 1);
    }

    #[test]
    fn progress_statuses_update_one_download_toast_in_place() {
        let mut app = app_for_test();

        app.set_status_updating_toast_in_category(
            TOAST_CATEGORY_LSP_INSTALL,
            "Downloading marksman…",
            "Downloading marksman… 815 KB".to_owned(),
        );
        app.set_status_updating_toast_in_category(
            TOAST_CATEGORY_LSP_INSTALL,
            "Downloading marksman…",
            "Downloading marksman… 1.3 MB".to_owned(),
        );
        app.ingest_status_toast();

        assert_eq!(app.status, "Downloading marksman… 1.3 MB");
        assert_eq!(
            app.status_toasts.len(),
            1,
            "download progress must update a single toast instead of stacking"
        );
        assert_eq!(app.status_toasts[0].message, "Downloading marksman… 1.3 MB");
        assert_eq!(app.status_toasts[0].category, TOAST_CATEGORY_LSP_INSTALL);
    }

    #[test]
    fn progress_statuses_keep_other_toasts_and_classify_errors() {
        let mut app = app_for_test();
        app.set_status_with_toast("Saved settings".to_owned());

        app.set_status_updating_toast_in_category(
            TOAST_CATEGORY_LSP_INSTALL,
            "Could not download",
            "Could not download marksman: connection reset".to_owned(),
        );
        app.set_status_updating_toast_in_category(
            TOAST_CATEGORY_LSP_INSTALL,
            "Could not download",
            "Could not download marksman: server unreachable".to_owned(),
        );

        assert_eq!(app.status_toasts.len(), 2);
        assert_eq!(
            app.status,
            "Could not download marksman: server unreachable"
        );
        assert_eq!(
            app.status_toasts[0].message,
            "Could not download marksman: server unreachable"
        );
        assert!(app.status_toasts[0].error);
        assert_eq!(app.status_toasts[1].message, "Saved settings");
    }

    #[test]
    fn categorized_statuses_carry_their_category_on_the_toast() {
        let mut app = app_for_test();

        app.set_status_with_toast_in_category(TOAST_CATEGORY_LSP, "rust LSP stopped");
        app.set_status_with_toast_in_category(TOAST_CATEGORY_UPDATE, "Kuroya v9 is available");
        app.set_status_with_toast_in_category(TOAST_CATEGORY_GENERAL, "Saved settings");

        assert_eq!(app.status_toasts.len(), 3);
        assert_eq!(app.status_toasts[0].category, TOAST_CATEGORY_GENERAL);
        assert_eq!(app.status_toasts[0].message, "Saved settings");
        assert_eq!(app.status_toasts[1].category, TOAST_CATEGORY_UPDATE);
        assert_eq!(app.status_toasts[2].category, TOAST_CATEGORY_LSP);
    }

    #[test]
    fn muted_category_suppresses_the_toast_but_still_updates_status_text() {
        let mut app = app_for_test();
        app.settings.muted_notifications = vec![TOAST_CATEGORY_LSP.to_owned()];

        app.set_status_with_toast_in_category(TOAST_CATEGORY_LSP, "rust LSP stopped");

        assert_eq!(
            app.status, "rust LSP stopped",
            "muting is display-only; the status bar text must still update"
        );
        assert!(
            app.status_toasts.is_empty(),
            "a muted category must not create a toast"
        );
        assert_eq!(
            app.last_status, "rust LSP stopped",
            "muted statuses must not queue up for a later frame"
        );
    }

    #[test]
    fn muted_category_toast_does_not_push_existing_toasts_out_of_the_stack() {
        let mut app = app_for_test();
        app.set_status_with_toast("Saved settings".to_owned());
        app.settings.muted_notifications = vec![TOAST_CATEGORY_BACKGROUND_IMAGE.to_owned()];

        app.set_status_with_toast_in_category(
            TOAST_CATEGORY_BACKGROUND_IMAGE,
            "Could not load background image",
        );

        assert_eq!(app.status_toasts.len(), 1);
        assert_eq!(app.status_toasts[0].message, "Saved settings");
    }

    #[test]
    fn unmuting_a_category_restores_future_toasts_without_replaying_suppressed_ones() {
        let mut app = app_for_test();
        app.settings.muted_notifications = vec![TOAST_CATEGORY_UPDATE.to_owned()];
        app.set_status_with_toast_in_category(TOAST_CATEGORY_UPDATE, "Kuroya v9 is available");
        assert!(app.status_toasts.is_empty());

        app.settings.muted_notifications.clear();
        app.set_status_with_toast_in_category(TOAST_CATEGORY_UPDATE, "Kuroya v10 is available");

        assert_eq!(app.status_toasts.len(), 1);
        assert_eq!(app.status_toasts[0].message, "Kuroya v10 is available");
        assert_eq!(app.status_toasts[0].category, TOAST_CATEGORY_UPDATE);
    }

    #[test]
    fn empty_or_unknown_muted_lists_show_every_toast() {
        let mut app = app_for_test();
        app.set_status_with_toast_in_category(TOAST_CATEGORY_GIT, "No git repository");
        assert_eq!(app.status_toasts.len(), 1);

        app.settings.muted_notifications = vec!["not-a-category".to_owned()];
        app.set_status_with_toast_in_category(TOAST_CATEGORY_UPDATE, "Kuroya v9 is available");

        assert_eq!(app.status_toasts.len(), 2);
        assert_eq!(app.status_toasts[0].category, TOAST_CATEGORY_UPDATE);
    }

    #[test]
    fn muting_mid_download_removes_the_in_flight_progress_toast_and_keeps_status_fresh() {
        let mut app = app_for_test();
        app.set_status_updating_toast_in_category(
            TOAST_CATEGORY_LSP_INSTALL,
            "Downloading marksman…",
            "Downloading marksman… 815 KB".to_owned(),
        );
        assert_eq!(app.status_toasts.len(), 1);

        app.settings.muted_notifications = vec![TOAST_CATEGORY_LSP_INSTALL.to_owned()];
        app.set_status_updating_toast_in_category(
            TOAST_CATEGORY_LSP_INSTALL,
            "Downloading marksman…",
            "Downloading marksman… 1.3 MB".to_owned(),
        );

        assert_eq!(app.status, "Downloading marksman… 1.3 MB");
        assert!(
            app.status_toasts.is_empty(),
            "muting mid-download must retire the progress toast"
        );
    }

    #[test]
    fn muted_categorized_status_does_not_leak_its_category_into_later_statuses() {
        let mut app = app_for_test();
        app.settings.muted_notifications = vec![TOAST_CATEGORY_LSP.to_owned()];

        // Same LSP status twice: the second ingest early-returns and must
        // consume the pending category instead of leaking it.
        app.set_status_with_toast_in_category(TOAST_CATEGORY_LSP, "rust LSP stopped");
        app.set_status_with_toast_in_category(TOAST_CATEGORY_LSP, "rust LSP stopped");
        assert!(app.status_toasts.is_empty());

        app.settings.muted_notifications.clear();
        app.set_status_with_toast("Saved settings".to_owned());

        assert_eq!(app.status_toasts.len(), 1);
        assert_eq!(
            app.status_toasts[0].category, TOAST_CATEGORY_GENERAL,
            "an unrelated status must not inherit a stale muted category"
        );
    }

    #[test]
    fn direct_status_assignments_ingest_as_general_toasts() {
        let mut app = app_for_test();
        // Frame 1 ingests the startup status, mirroring the running app.
        app.ingest_status_toast();

        app.status = "Watcher noticed 3 changes".to_owned();
        app.ingest_status_toast();

        assert_eq!(app.status_toasts.len(), 2);
        assert_eq!(app.status_toasts[0].category, TOAST_CATEGORY_GENERAL);
    }

    #[test]
    fn startup_status_toasts_as_indexing_when_a_workspace_is_open() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        let mut app = KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(PathBuf::from("workspace")),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(PathBuf::from("workspace"), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![PathBuf::from("workspace")],
            now: std::time::Instant::now(),
            startup_timings: Vec::new(),
        });

        assert_eq!(app.status, "Indexing workspace");
        app.ingest_status_toast();

        assert_eq!(app.status_toasts.len(), 1);
        assert_eq!(app.status_toasts[0].category, TOAST_CATEGORY_INDEXING);

        // Muting indexing suppresses the startup toast entirely.
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let draft = EditorSettings::default();
        let muted = EditorSettings {
            muted_notifications: vec![TOAST_CATEGORY_INDEXING.to_owned()],
            ..EditorSettings::default()
        };
        let mut app = KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(PathBuf::from("workspace")),
            settings: muted,
            settings_panel_draft: draft,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(PathBuf::from("workspace"), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![PathBuf::from("workspace")],
            now: std::time::Instant::now(),
            startup_timings: Vec::new(),
        });
        app.ingest_status_toast();

        assert!(
            app.status_toasts.is_empty(),
            "muting indexing must suppress the startup indexing toast"
        );
    }

    /// Emitter-level checks: the real app flows must attach the right
    /// category to the toasts they produce.
    mod emitter_tags {
        use super::{
            TOAST_CATEGORY_BACKGROUND_IMAGE, TOAST_CATEGORY_INDEXING, TOAST_CATEGORY_LSP,
            TOAST_CATEGORY_LSP_INSTALL, TOAST_CATEGORY_UPDATE,
        };
        use crate::{KuroyaApp, app_startup_context::AppStartupContext, terminal::TerminalPane};
        use kuroya_core::{EditorSettings, Workspace};
        use std::{path::PathBuf, time::Instant};
        use tokio::runtime::Runtime;

        fn app_with_settings(settings: EditorSettings) -> KuroyaApp {
            let (tx, rx) = crate::ui_event_channel::ui_event_channel();
            KuroyaApp::from_startup_context(AppStartupContext {
                runtime: Runtime::new().expect("test runtime"),
                tx,
                rx,
                workspace: Workspace::new(PathBuf::from("workspace")),
                settings: settings.clone(),
                settings_panel_draft: settings,
                settings_editor_font_path: String::new(),
                settings_ui_font_path: String::new(),
                theme_picker_selected: 0,
                saved_session: None,
                terminal: TerminalPane::new(PathBuf::from("workspace"), 100, 12.0, 1.2),
                watcher: None,
                recent_projects: Vec::new(),
                trusted_workspaces: vec![PathBuf::from("workspace")],
                now: Instant::now(),
                startup_timings: Vec::new(),
            })
        }

        fn single_toast_category(app: &KuroyaApp) -> &'static str {
            assert_eq!(
                app.status_toasts.len(),
                1,
                "emitter test expects exactly one toast, got {:?}",
                app.status_toasts
                    .iter()
                    .map(|toast| toast.message.as_str())
                    .collect::<Vec<_>>()
            );
            app.status_toasts[0].category
        }

        #[test]
        fn lsp_work_done_progress_toasts_are_tagged_lsp() {
            let mut app = app_with_settings(EditorSettings::default());

            app.handle_lsp_work_done_progress(
                "rust".to_owned(),
                PathBuf::from("workspace"),
                1,
                kuroya_core::LspWorkDoneProgress {
                    token: "token-1".to_owned(),
                    kind: kuroya_core::LspWorkDoneProgressKind::Begin,
                    title: Some("Indexing".to_owned()),
                    message: Some("src/lib.rs".to_owned()),
                    percentage: None,
                },
            );

            assert!(app.status.contains("Indexing"));
            assert_eq!(single_toast_category(&app), TOAST_CATEGORY_LSP);
        }

        #[test]
        fn lsp_download_progress_toasts_are_tagged_lsp_install() {
            let mut app = app_with_settings(EditorSettings::default());
            app.lsp_installs_in_flight.push("rust".to_owned());

            app.apply_lsp_install_progress("rust", "rust-analyzer", 815_000);

            assert!(app.status.starts_with("Downloading rust-analyzer"));
            assert_eq!(single_toast_category(&app), TOAST_CATEGORY_LSP_INSTALL);
        }

        #[test]
        fn update_available_toasts_are_tagged_update() {
            let mut app = app_with_settings(EditorSettings::default());

            app.apply_update_check_finished(
                crate::update_checker::UpdateCheckOutcome::UpdateAvailable(
                    crate::update_checker::AvailableUpdate {
                        current_version: "0.1.7".to_owned(),
                        latest_version: "v0.2.0".to_owned(),
                        asset: crate::update_checker::UpdateInstallerAsset {
                            name: "Kuroya-Setup-0.2.0.exe".to_owned(),
                            browser_download_url:
                                "https://github.com/owner/repo/releases/download/v0.2.0/setup.exe"
                                    .to_owned(),
                            checksum_sidecar_url: None,
                        },
                    },
                ),
            );

            assert!(app.status.contains("v0.2.0 is available"));
            assert_eq!(single_toast_category(&app), TOAST_CATEGORY_UPDATE);
        }

        #[test]
        fn background_image_configuration_error_toasts_are_tagged_background_image() {
            // A relative path is a configuration error, reported synchronously.
            let settings = EditorSettings {
                background_image_enabled: true,
                background_image_path: Some("relative/background.png".to_owned()),
                ..EditorSettings::default()
            };
            let mut app = app_with_settings(settings);

            app.sync_background_image(false);

            assert!(app.status.starts_with("Editor background image"));
            assert_eq!(single_toast_category(&app), TOAST_CATEGORY_BACKGROUND_IMAGE);
        }

        #[test]
        fn project_search_indexing_deferral_toasts_are_tagged_indexing() {
            let mut app = app_with_settings(EditorSettings::default());
            app.project_search_query = "needle".to_owned();
            app.workspace_index_in_flight_request_id = Some(1);

            app.spawn_project_search();

            assert_eq!(app.status, "Indexing workspace before search");
            assert_eq!(single_toast_category(&app), TOAST_CATEGORY_INDEXING);
        }

        #[test]
        fn muting_update_silences_the_update_available_toast_but_keeps_the_status() {
            let settings = EditorSettings {
                muted_notifications: vec![TOAST_CATEGORY_UPDATE.to_owned()],
                ..EditorSettings::default()
            };
            let mut app = app_with_settings(settings);

            app.apply_update_check_finished(
                crate::update_checker::UpdateCheckOutcome::UpdateAvailable(
                    crate::update_checker::AvailableUpdate {
                        current_version: "0.1.7".to_owned(),
                        latest_version: "v0.2.0".to_owned(),
                        asset: crate::update_checker::UpdateInstallerAsset {
                            name: "Kuroya-Setup-0.2.0.exe".to_owned(),
                            browser_download_url:
                                "https://github.com/owner/repo/releases/download/v0.2.0/setup.exe"
                                    .to_owned(),
                            checksum_sidecar_url: None,
                        },
                    },
                ),
            );

            assert!(app.status.contains("v0.2.0 is available"));
            assert!(
                app.status_toasts.is_empty(),
                "muted update toasts must not be displayed"
            );
            assert!(
                app.available_update.is_some(),
                "muting is display-only; the update offer must stay available"
            );
        }
    }

    fn app_for_test() -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(PathBuf::from("workspace")),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(PathBuf::from("workspace"), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![PathBuf::from("workspace")],
            now: std::time::Instant::now(),
            startup_timings: Vec::new(),
        })
    }
}

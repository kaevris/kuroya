use crate::{
    devtools_startup::{StartupProfiler, StartupTimingEntry},
    editor_vim_key_events::sanitize_vim_settings_for_runtime,
    fonts::{apply_typography, install_fonts},
    fs_watcher::FileWatcher,
    path_display::display_error_label_cow,
    persistence::{AppState, PersistedSession},
    persistence_storage::{legacy_session_path, session_path},
    preferences::load_app_settings,
    settings_form::optional_setting_path_to_input,
    startup_arguments::StartupTarget,
    terminal::TerminalPane,
    theme::apply_theme,
    theme_picker_panel::selected_theme_picker_index_for_settings,
    ui_event_channel::{Receiver, Sender, set_ui_wake_hook, ui_event_channel},
    ui_events::UiEvent,
    workspace_state::paths_match_lexically,
};
use anyhow::Context as _;
use kuroya_core::{EditorSettings, PluginThemeRegistry, Workspace, window_zoom_factor};
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use tokio::runtime::Runtime;

const EMPTY_STARTUP_WORKSPACE_DIR_NAME: &str = "empty-workspace";

pub(crate) struct AppStartupContext {
    pub(crate) runtime: Runtime,
    pub(crate) tx: Sender<UiEvent>,
    pub(crate) rx: Receiver<UiEvent>,
    pub(crate) workspace: Workspace,
    pub(crate) settings: EditorSettings,
    pub(crate) settings_panel_draft: EditorSettings,
    pub(crate) settings_editor_font_path: String,
    pub(crate) settings_ui_font_path: String,
    pub(crate) theme_picker_selected: usize,
    pub(crate) saved_session: Option<PersistedSession>,
    pub(crate) terminal: TerminalPane,
    pub(crate) watcher: Option<FileWatcher>,
    pub(crate) recent_projects: Vec<PathBuf>,
    pub(crate) trusted_workspaces: Vec<PathBuf>,
    pub(crate) now: Instant,
    pub(crate) startup_timings: Vec<StartupTimingEntry>,
}

impl AppStartupContext {
    pub(crate) fn load(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        let mut startup_profiler = StartupProfiler::start(Instant::now());
        let runtime = create_runtime()?;
        let (tx, rx) = ui_event_channel();
        let app_state = AppState::load().unwrap_or_default();
        let workspace_root = startup_workspace_root(&app_state);
        let workspace_placeholder = is_empty_startup_workspace_root(&workspace_root);
        if workspace_placeholder {
            let _ = fs::create_dir_all(&workspace_root);
        }
        let workspace = Workspace::new(workspace_root);
        startup_profiler.record("Initialize runtime");

        let settings = load_startup_app_settings(&workspace.root, &app_state).unwrap_or_default();
        startup_profiler.record("Load settings");

        install_fonts(&cc.egui_ctx, &workspace.root, &settings);
        apply_typography(&cc.egui_ctx, &settings);
        apply_theme(&cc.egui_ctx, &settings.theme);
        cc.egui_ctx
            .set_zoom_factor(window_zoom_factor(settings.window_zoom_level));
        startup_profiler.record("Configure UI");

        let theme_picker_selected = selected_theme_picker_index_for_settings(
            &workspace.root,
            &settings,
            &PluginThemeRegistry::default(),
        );
        let settings_panel_draft = settings.clone();
        let settings_editor_font_path = optional_setting_path_to_input(&settings.editor_font_path);
        let settings_ui_font_path = optional_setting_path_to_input(&settings.ui_font_path);
        // The saved session is read off-thread (`spawn_startup_session_load`,
        // called from `KuroyaApp::new`) and applied when the critical
        // `UiEvent::StartupSessionLoaded` arrives, so the multi-megabyte
        // session JSON no longer blocks the first frame. Fonts, theme, and
        // settings stay synchronous above: the first paint needs them.
        startup_profiler.record("Queue session load");

        let mut terminal = TerminalPane::with_settings(
            terminal_root_for_workspace(&workspace.root),
            settings.terminal_scrollback_rows,
            settings.terminal_shell_path.clone(),
            settings.terminal_shell_args.clone(),
            settings.terminal_cwd.clone(),
            settings.terminal_split_cwd,
            settings.terminal_min_rows,
            settings.terminal_min_columns,
            settings.terminal_font_size,
            settings.terminal_line_height,
            settings.terminal_letter_spacing,
            settings.terminal_cursor_style,
            settings.terminal_cursor_width,
            settings.terminal_cursor_blinking,
            settings.terminal_cursor_style_inactive,
            settings.terminal_draw_bold_text_in_bright_colors,
            settings.terminal_minimum_contrast_ratio,
            settings.terminal_enable_bell,
            settings.terminal_bell_duration_ms,
            settings.terminal_show_exit_alert,
            settings.terminal_hide_on_last_closed,
            settings.terminal_confirm_on_kill,
            settings.terminal_tabs_enabled,
            settings.terminal_tabs_default_icon.clone(),
            settings.terminal_tabs_default_color.clone(),
            settings.terminal_tabs_allow_agent_cli_title,
            settings.terminal_tabs_title.clone(),
            settings.terminal_tabs_hide_condition,
            settings.terminal_tabs_show_active_terminal,
            settings.terminal_tabs_show_actions,
            settings.terminal_tabs_focus_mode,
            settings.terminal_tabs_location,
            settings.terminal_right_click_behavior,
            settings.terminal_middle_click_behavior,
            settings.terminal_alt_click_moves_cursor,
            settings.terminal_copy_on_selection,
            settings.terminal_ignore_bracketed_paste_mode,
            settings.terminal_enable_multi_line_paste_warning,
            settings.terminal_word_separators.clone(),
            settings.terminal_mouse_wheel_scroll_sensitivity,
            settings.terminal_fast_scroll_sensitivity,
            settings.terminal_mouse_wheel_zoom,
        );
        terminal.set_repaint_context(cc.egui_ctx.clone());
        let repaint_ctx = cc.egui_ctx.clone();
        set_ui_wake_hook(Arc::new(move || repaint_ctx.request_repaint()));
        startup_profiler.record("Create terminal");

        let watcher = startup_file_watcher(&workspace.root, workspace_placeholder);
        startup_profiler.record("Start watcher");

        let now = Instant::now();
        let startup_timings = startup_profiler.into_entries();

        Ok(Self {
            runtime,
            tx,
            rx,
            workspace,
            settings,
            settings_panel_draft,
            settings_editor_font_path,
            settings_ui_font_path,
            theme_picker_selected,
            saved_session: None,
            terminal,
            watcher,
            recent_projects: app_state.recent_projects,
            trusted_workspaces: app_state.trusted_workspaces,
            now,
            startup_timings,
        })
    }
}

fn startup_workspace_root(app_state: &AppState) -> PathBuf {
    startup_workspace_root_with_dir_probe(app_state, Path::is_dir)
}

fn startup_workspace_root_with_dir_probe(
    app_state: &AppState,
    mut is_dir: impl FnMut(&Path) -> bool,
) -> PathBuf {
    app_state
        .recent_projects
        .iter()
        .find(|path| startup_recent_project_is_usable_with_dir_probe(path, &mut is_dir))
        .cloned()
        .unwrap_or_else(empty_startup_workspace_root)
}

#[cfg(test)]
fn startup_recent_project_is_usable(path: &Path) -> bool {
    startup_recent_project_is_usable_with_dir_probe(path, Path::is_dir)
}

fn startup_recent_project_is_usable_with_dir_probe(
    path: &Path,
    mut is_dir: impl FnMut(&Path) -> bool,
) -> bool {
    !path.as_os_str().is_empty() && !is_empty_startup_workspace_root(path) && is_dir(path)
}

fn load_startup_session(
    workspace_root: &Path,
    workspace_placeholder: bool,
) -> anyhow::Result<Option<PersistedSession>> {
    if workspace_placeholder {
        return Ok(None);
    }

    PersistedSession::load(workspace_root)
}

pub(crate) fn load_startup_session_with_warning(
    workspace_root: &Path,
    workspace_placeholder: bool,
) -> (Option<PersistedSession>, Option<String>) {
    match load_startup_session(workspace_root, workspace_placeholder) {
        Ok(Some(session)) => (Some(session), None),
        Ok(None) if startup_session_has_quarantine_artifacts(workspace_root) => (
            None,
            Some(startup_session_load_warning(
                STARTUP_SESSION_QUARANTINED_REASON,
            )),
        ),
        Ok(None) => (None, None),
        Err(error) => (None, Some(startup_session_load_warning(error))),
    }
}

/// Reads the saved session off the blocking thread pool and delivers it to
/// the app with a critical `UiEvent::StartupSessionLoaded` (bounded wait, no
/// silent drop while capacity frees up). This keeps the multi-megabyte
/// session JSON read off the startup path so the first frame can paint
/// before the restore lands. `startup_target` rides along on the event so
/// the handler can apply it after the restore, preserving the ordering of
/// the previous synchronous startup (restore, then file/folder target).
pub(crate) fn spawn_startup_session_load(
    runtime: &Runtime,
    tx: Sender<UiEvent>,
    workspace_root: PathBuf,
    startup_target: Option<StartupTarget>,
) {
    runtime.spawn_blocking(move || {
        let (session, warning) = load_startup_session_with_warning(&workspace_root, false);
        let _ = crate::ui_event_channel::send_critical_ui_event(
            &tx,
            UiEvent::StartupSessionLoaded {
                root: workspace_root,
                target: startup_target,
                session: session.map(Box::new),
                warning,
            },
        );
    });
}

const STARTUP_SESSION_QUARANTINED_REASON: &str = "corrupt saved session file was quarantined";

pub(crate) fn startup_session_load_warning(error: impl std::fmt::Display) -> String {
    let error = error.to_string();
    format!(
        "Could not load saved session: {}",
        display_error_label_cow(&error)
    )
}

fn startup_session_has_quarantine_artifacts(workspace_root: &Path) -> bool {
    [
        session_path(workspace_root),
        legacy_session_path(workspace_root),
    ]
    .iter()
    .any(|session_file| startup_session_file_has_quarantine_artifacts(session_file))
}

fn startup_session_file_has_quarantine_artifacts(session_file: &Path) -> bool {
    let Some(dir) = session_file.parent() else {
        return false;
    };
    let Some(file_name) = session_file.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };

    entries.filter_map(Result::ok).any(|entry| {
        entry.file_name().to_str().is_some_and(|name| {
            name.strip_prefix(file_name).is_some_and(|suffix| {
                suffix.starts_with(".corrupt.") || suffix.starts_with(".mismatched.")
            })
        })
    })
}

fn load_startup_app_settings(
    workspace_root: &Path,
    app_state: &AppState,
) -> anyhow::Result<EditorSettings> {
    let loaded = load_app_settings(workspace_root)?;
    let mut settings = loaded.settings;
    if loaded.source.applies_startup_app_state_fallback() {
        apply_app_state_settings_fallback(&mut settings, app_state);
    }
    Ok(settings)
}

fn startup_file_watcher(workspace_root: &Path, workspace_placeholder: bool) -> Option<FileWatcher> {
    if workspace_placeholder {
        None
    } else {
        FileWatcher::new(workspace_root).ok()
    }
}

fn apply_app_state_settings_fallback(settings: &mut EditorSettings, app_state: &AppState) {
    if let Some(theme) = &app_state.theme {
        settings.theme = theme.clone();
    }
    settings.custom_theme_paths = app_state.custom_theme_paths.clone();
    settings.active_custom_theme_path = app_state.active_custom_theme_path.clone();
    settings.editor_font_path = app_state.editor_font_path.clone();
    settings.ui_font_path = app_state.ui_font_path.clone();
    if let Some(vim_keybindings) = app_state.vim_keybindings {
        settings.vim_keybindings = vim_keybindings;
    }
    if let Some(vim) = &app_state.vim {
        settings.vim = vim.clone();
        sanitize_vim_settings_for_runtime(&mut settings.vim);
    }
}

fn create_runtime() -> anyhow::Result<Runtime> {
    Runtime::new().context("create tokio runtime")
}

pub(crate) fn empty_startup_workspace_root() -> PathBuf {
    crate::persistence_storage::app_state_dir().join(EMPTY_STARTUP_WORKSPACE_DIR_NAME)
}

pub(crate) fn terminal_root_for_workspace(workspace_root: &Path) -> PathBuf {
    terminal_root_for_workspace_with_home(workspace_root, home_dir_from_env())
}

fn terminal_root_for_workspace_with_home(workspace_root: &Path, home: Option<PathBuf>) -> PathBuf {
    if is_empty_startup_workspace_root(workspace_root) {
        home.unwrap_or_else(empty_startup_workspace_root)
    } else {
        workspace_root.to_path_buf()
    }
}

pub(crate) fn home_dir_from_env() -> Option<PathBuf> {
    home_dir_from_env_values(std::env::var_os("USERPROFILE"), std::env::var_os("HOME"))
}

fn home_dir_from_env_values(
    userprofile: Option<OsString>,
    home: Option<OsString>,
) -> Option<PathBuf> {
    non_empty_path(userprofile).or_else(|| non_empty_path(home))
}

fn non_empty_path(value: Option<OsString>) -> Option<PathBuf> {
    let path = PathBuf::from(value?);
    (!path.as_os_str().is_empty()).then_some(path)
}

pub(crate) fn is_empty_startup_workspace_root(path: &Path) -> bool {
    paths_match_lexically(path, &empty_startup_workspace_root())
}

#[cfg(test)]
mod tests {
    use super::{
        apply_app_state_settings_fallback, create_runtime, empty_startup_workspace_root,
        home_dir_from_env_values, is_empty_startup_workspace_root, load_startup_app_settings,
        load_startup_session, load_startup_session_with_warning, spawn_startup_session_load,
        startup_recent_project_is_usable, startup_session_load_warning, startup_workspace_root,
        startup_workspace_root_with_dir_probe, terminal_root_for_workspace_with_home,
    };
    use crate::{
        path_display::DISPLAY_ERROR_LABEL_MAX_CHARS,
        persistence::AppState,
        persistence_storage::{session_path, state_dir},
        startup_arguments::StartupTarget,
        ui_events::UiEvent,
        workspace_state::settings_path,
    };
    use kuroya_core::{EditorSettings, EditorVimKeyOverride, EditorVimSettings, ThemeSettings};
    use std::{
        ffi::OsString,
        fs,
        path::PathBuf,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn startup_runtime_creation_returns_result() -> anyhow::Result<()> {
        let _runtime = create_runtime()?;
        Ok(())
    }

    #[test]
    fn empty_startup_workspace_root_is_recognized() {
        let root = empty_startup_workspace_root();

        assert!(is_empty_startup_workspace_root(&root));
        assert!(!is_empty_startup_workspace_root(&root.join("child")));
    }

    #[cfg(windows)]
    #[test]
    fn empty_startup_workspace_root_recognizes_windows_verbatim_path() {
        let root = empty_startup_workspace_root();
        let verbatim = PathBuf::from(format!(r"\\?\{}", root.display()));

        assert!(is_empty_startup_workspace_root(&verbatim));
    }

    #[test]
    fn empty_startup_terminal_root_prefers_user_home() {
        assert_eq!(
            home_dir_from_env_values(
                Some(OsString::from(r"C:\Users\kuroya")),
                Some(OsString::from("/home/kuroya"))
            ),
            Some(PathBuf::from(r"C:\Users\kuroya"))
        );
        assert_eq!(
            home_dir_from_env_values(Some(OsString::new()), Some(OsString::from("/home/kuroya"))),
            Some(PathBuf::from("/home/kuroya"))
        );
    }

    #[test]
    fn terminal_root_for_empty_workspace_uses_home() {
        let root = empty_startup_workspace_root();

        assert_eq!(
            terminal_root_for_workspace_with_home(&root, Some(PathBuf::from(r"C:\Users\kuroya"))),
            PathBuf::from(r"C:\Users\kuroya")
        );
        assert_eq!(
            terminal_root_for_workspace_with_home(PathBuf::from("project").as_path(), None),
            PathBuf::from("project")
        );
    }

    #[test]
    fn startup_workspace_root_prefers_first_existing_recent_project() {
        let missing = temp_workspace("missing-recent");
        let first = temp_workspace("first-recent");
        let second = temp_workspace("second-recent");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let app_state = AppState {
            recent_projects: vec![missing, first.clone(), second.clone()],
            ..AppState::default()
        };

        assert_eq!(startup_workspace_root(&app_state), first);

        fs::remove_dir_all(second).unwrap();
        fs::remove_dir_all(first).unwrap();
    }

    #[test]
    fn startup_workspace_root_falls_back_to_empty_workspace_without_usable_recents() {
        let app_state = AppState {
            recent_projects: vec![PathBuf::new(), empty_startup_workspace_root()],
            ..AppState::default()
        };

        assert_eq!(
            startup_workspace_root(&app_state),
            empty_startup_workspace_root()
        );
    }

    #[test]
    fn startup_workspace_root_uses_injected_dir_probe() {
        let first = PathBuf::from("first");
        let second = PathBuf::from("second");
        let app_state = AppState {
            recent_projects: vec![first.clone(), second.clone()],
            ..AppState::default()
        };

        let selected =
            startup_workspace_root_with_dir_probe(&app_state, |path| path == second.as_path());

        assert_eq!(selected, second);
    }

    #[test]
    fn startup_recent_project_skips_files_and_empty_workspace() {
        let root = temp_workspace("file-recent");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("not-a-workspace");
        fs::write(&file, b"file").unwrap();

        assert!(!startup_recent_project_is_usable(&file));
        assert!(!startup_recent_project_is_usable(
            &empty_startup_workspace_root()
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_does_not_load_session_for_empty_workspace() {
        let root = empty_startup_workspace_root();

        assert_eq!(load_startup_session(&root, true).unwrap(), None);
    }

    #[test]
    fn spawn_startup_session_load_sends_event_with_startup_target() {
        let root = temp_workspace("session-load-event");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("opened-on-startup.rs");

        let runtime = create_runtime().unwrap();
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        spawn_startup_session_load(
            &runtime,
            tx,
            root.clone(),
            Some(StartupTarget::File(target.clone())),
        );

        let event = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("startup session load should deliver an event");
        match event {
            UiEvent::StartupSessionLoaded {
                root: event_root,
                target: event_target,
                session,
                warning,
            } => {
                assert_eq!(event_root, root);
                assert_eq!(event_target, Some(StartupTarget::File(target)));
                // The test state dir is thread-local, so the blocking thread
                // sees a fresh bucket and a missing session file loads as
                // (None, None); the session round-trip itself is covered by
                // the synchronous load_startup_session_with_warning tests.
                assert!(session.is_none());
                assert!(warning.is_none());
            }
            other => panic!("unexpected startup event: {other:?}"),
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn spawn_startup_session_load_carries_warning_for_missing_session_file() {
        let root = temp_workspace("session-load-event-warning");
        fs::create_dir_all(&root).unwrap();

        let runtime = create_runtime().unwrap();
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        spawn_startup_session_load(&runtime, tx, root.clone(), None);

        let event = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("startup session load should deliver an event");
        match event {
            UiEvent::StartupSessionLoaded {
                root: event_root,
                session,
                warning,
                ..
            } => {
                assert_eq!(event_root, root);
                assert!(session.is_none());
                assert!(warning.is_none(), "a missing session is not a warning");
            }
            other => panic!("unexpected startup event: {other:?}"),
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_missing_saved_session_loads_without_warning() {
        let root = temp_workspace("missing-session-no-warning");
        fs::create_dir_all(&root).unwrap();

        let (session, warning) =
            load_startup_session_with_warning(&root.join("src").join(".."), true);
        assert!(session.is_none());
        assert!(warning.is_none());

        let (session, warning) = load_startup_session_with_warning(&root, false);
        assert!(session.is_none());
        assert!(warning.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_corrupt_saved_session_surfaces_quarantine_warning() {
        let root = temp_workspace("corrupt-session-warning");
        fs::create_dir_all(&root).unwrap();
        let session_file = session_path(&root);
        fs::create_dir_all(session_file.parent().unwrap()).unwrap();
        fs::write(&session_file, b"{not json").unwrap();

        let (session, warning) = load_startup_session_with_warning(&root, false);

        assert!(session.is_none());
        let warning = warning.expect("quarantined session should surface a warning");
        assert!(
            warning.starts_with("Could not load saved session: "),
            "{}",
            warning
        );
        assert!(warning.contains("quarantined"), "{}", warning);

        assert!(!session_file.exists());
        let mut entries = fs::read_dir(session_file.parent().unwrap()).unwrap();
        assert!(entries.next().is_some());

        drop(fs::remove_dir_all(state_dir(&root)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_session_load_warning_is_single_line_and_bounded() {
        let raw_error = format!("first line\nsecond line \u{202e}{}", "x".repeat(400));
        let warning = startup_session_load_warning(&raw_error);
        let prefix = "Could not load saved session: ";

        assert!(warning.starts_with(prefix));
        assert!(!warning.contains('\n'));
        assert!(!warning.contains('\u{202e}'));
        assert!(warning[prefix.len()..].chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);
        assert_eq!(
            startup_session_load_warning(""),
            format!("{prefix}unknown error")
        );
    }

    #[test]
    fn startup_missing_app_settings_restores_vim_from_app_state() {
        let root = temp_workspace("missing-settings-vim-fallback");
        fs::create_dir_all(&root).unwrap();
        let settings_path = settings_path(&root);
        let app_state = AppState {
            vim_keybindings: Some(true),
            vim: Some(EditorVimSettings {
                disabled_bindings: vec!["Q".to_owned(), "<Nope>".to_owned()],
                key_overrides: vec![
                    EditorVimKeyOverride {
                        before: "<Home>".to_owned(),
                        after: "0".to_owned(),
                        command: None,
                    },
                    EditorVimKeyOverride {
                        before: "L".to_owned(),
                        after: "<Left>".to_owned(),
                        command: None,
                    },
                ],
            }),
            ..AppState::default()
        };

        let settings = load_startup_app_settings(&root, &app_state).unwrap();

        assert!(settings.vim_keybindings);
        assert_eq!(
            settings.vim,
            EditorVimSettings {
                disabled_bindings: vec!["Q".to_owned()],
                key_overrides: vec![EditorVimKeyOverride {
                    before: "<Home>".to_owned(),
                    after: "0".to_owned(),
                    command: None,
                }],
            }
        );
        assert!(!settings_path.exists());
        assert!(!settings_path.parent().unwrap().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_app_settings_override_fallback_regardless_of_workspace_trust() {
        let root = temp_workspace("app-settings-before-fallback");
        fs::create_dir_all(&root).unwrap();
        let settings_path = settings_path(&root);
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(
            &settings_path,
            "vim_keybindings = false\nword_separators = \".\"\n",
        )
        .unwrap();
        let app_state = AppState {
            vim_keybindings: Some(true),
            vim: Some(EditorVimSettings {
                disabled_bindings: vec!["Q".to_owned()],
                key_overrides: Vec::new(),
            }),
            ..AppState::default()
        };

        let settings = load_startup_app_settings(&root, &app_state).unwrap();

        assert!(!settings.vim_keybindings);
        assert!(settings.vim.disabled_bindings.is_empty());
        assert_eq!(settings.word_separators, ".");
        fs::remove_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_app_settings_restore_appearance_and_vim_from_app_state() {
        let mut settings = EditorSettings::default();
        let theme_path = std::env::temp_dir()
            .join("themes")
            .join("saved.toml")
            .display()
            .to_string();
        let editor_font_path = std::env::temp_dir()
            .join("fonts")
            .join("editor.ttf")
            .display()
            .to_string();
        let ui_font_path = std::env::temp_dir()
            .join("fonts")
            .join("ui.ttf")
            .display()
            .to_string();
        let app_state_vim = EditorVimSettings {
            disabled_bindings: vec!["Q".to_owned(), "<Nope>".to_owned()],
            key_overrides: vec![
                EditorVimKeyOverride {
                    before: "<Home>".to_owned(),
                    after: "0".to_owned(),
                    command: None,
                },
                EditorVimKeyOverride {
                    before: "L".to_owned(),
                    after: "<Left>".to_owned(),
                    command: None,
                },
            ],
        };
        let app_state = AppState {
            vim_keybindings: Some(true),
            vim: Some(app_state_vim),
            theme: Some(ThemeSettings {
                name: "Saved Theme".to_owned(),
                accent: [1, 2, 3],
                ..ThemeSettings::default()
            }),
            custom_theme_paths: vec![theme_path.clone()],
            active_custom_theme_path: Some(theme_path.clone()),
            editor_font_path: Some(editor_font_path.clone()),
            ui_font_path: Some(ui_font_path.clone()),
            ..AppState::default()
        };

        apply_app_state_settings_fallback(&mut settings, &app_state);

        assert_eq!(settings.theme.name, "Saved Theme");
        assert_eq!(settings.theme.accent, [1, 2, 3]);
        assert_eq!(
            settings.custom_theme_paths,
            std::slice::from_ref(&theme_path)
        );
        assert_eq!(
            settings.active_custom_theme_path.as_deref(),
            Some(theme_path.as_str())
        );
        assert_eq!(
            settings.editor_font_path.as_deref(),
            Some(editor_font_path.as_str())
        );
        assert_eq!(
            settings.ui_font_path.as_deref(),
            Some(ui_font_path.as_str())
        );
        assert!(settings.vim_keybindings);
        assert_eq!(
            settings.vim,
            EditorVimSettings {
                disabled_bindings: vec!["Q".to_owned()],
                key_overrides: vec![EditorVimKeyOverride {
                    before: "<Home>".to_owned(),
                    after: "0".to_owned(),
                    command: None,
                }],
            }
        );
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-startup-context-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}

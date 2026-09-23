use crate::{
    KuroyaApp,
    path_display::display_error_label_cow,
    settings_form::optional_setting_path_to_input,
    ui_event_channel::send_critical_ui_event,
    ui_events::{SettingsFontTarget, UiEvent},
};
use kuroya_core::EditorSettings;

use super::{background_files::choose_background_image_file, font_files::choose_font_file};

#[derive(Default)]
pub(super) struct PendingSettingsPanelActions {
    pub(super) close: bool,
    pub(super) apply: bool,
    pub(super) reset: bool,
    pub(super) reload: bool,
    pub(super) choose_editor_font: bool,
    pub(super) clear_editor_font: bool,
    pub(super) choose_ui_font: bool,
    pub(super) clear_ui_font: bool,
    pub(super) choose_background_image: bool,
    pub(super) clear_background_image: bool,
    pub(super) status: Option<String>,
}

impl KuroyaApp {
    pub(crate) fn settings_panel_has_pending_inputs(&self) -> bool {
        self.settings_panel_open && self.settings_panel_draft_validation().has_pending_inputs()
    }

    pub(super) fn apply_settings_panel_actions(&mut self, actions: PendingSettingsPanelActions) {
        if actions.apply {
            self.apply_settings_panel();
        } else if actions.close {
            self.settings_panel_open = false;
            self.sync_settings_panel_inputs();
            self.status = "Closed settings".to_owned();
        } else if actions.reset {
            self.reset_settings_panel_draft();
        } else if actions.reload {
            self.reload_settings();
        } else if actions.choose_editor_font {
            self.choose_settings_font(SettingsFontTarget::Editor);
        } else if actions.clear_editor_font {
            self.settings_editor_font_path.clear();
            self.status = "Cleared editor font file".to_owned();
        } else if actions.choose_ui_font {
            self.choose_settings_font(SettingsFontTarget::Ui);
        } else if actions.clear_ui_font {
            self.settings_ui_font_path.clear();
            self.status = "Cleared UI font file".to_owned();
        } else if actions.choose_background_image {
            self.choose_settings_background_image();
        } else if actions.clear_background_image {
            self.settings_panel_draft.background_image_path = None;
            self.status = "Cleared editor background image".to_owned();
        } else if let Some(status) = actions.status {
            self.status = status;
        }
    }

    fn reset_settings_panel_draft(&mut self) {
        if !self.settings_panel_open {
            self.status = "Open settings before resetting draft".to_owned();
            return;
        }

        let validation = self.settings_panel_draft_validation();
        let had_pending_inputs = validation.has_pending_inputs();
        let mut default_candidate = self.settings_panel_default_candidate();

        default_candidate.active_custom_theme_path =
            EditorSettings::default().active_custom_theme_path;
        let already_default = !had_pending_inputs && default_candidate == self.settings;
        self.settings_panel_draft = default_candidate;
        self.settings_editor_font_path =
            optional_setting_path_to_input(&self.settings_panel_draft.editor_font_path);
        self.settings_ui_font_path =
            optional_setting_path_to_input(&self.settings_panel_draft.ui_font_path);
        self.status = if already_default {
            "Settings already match defaults".to_owned()
        } else {
            "Reset settings draft to defaults; apply to save".to_owned()
        };
    }

    fn choose_settings_font(&mut self, target: SettingsFontTarget) {
        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        let current = match target {
            SettingsFontTarget::Editor => self.settings_editor_font_path.clone(),
            SettingsFontTarget::Ui => self.settings_ui_font_path.clone(),
        };
        let tx = self.tx.clone();
        self.status = match target {
            SettingsFontTarget::Editor => "Choose an editor font file".to_owned(),
            SettingsFontTarget::Ui => "Choose a UI font file".to_owned(),
        };
        self.runtime.spawn_blocking(move || {
            let event = match choose_font_file(&root, &current) {
                Ok(Some(path)) => UiEvent::SettingsFontPicked {
                    root,
                    generation,
                    target,
                    path,
                },
                Ok(None) => UiEvent::SettingsFontPickerCanceled {
                    root,
                    generation,
                    target,
                },
                Err(error) => UiEvent::SettingsFontPickerFailed {
                    root,
                    generation,
                    target,
                    error,
                },
            };
            let _ = send_critical_ui_event(&tx, event);
        });
    }

    fn choose_settings_background_image(&mut self) {
        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        let current = self.settings_panel_draft.background_image_path.clone();
        let tx = self.tx.clone();
        self.status = "Choose an editor background image".to_owned();
        self.runtime.spawn_blocking(move || {
            let event = match choose_background_image_file(&root, current.as_deref()) {
                Ok(Some(path)) => UiEvent::SettingsBackgroundImagePicked {
                    root,
                    generation,
                    path,
                },
                Ok(None) => UiEvent::SettingsBackgroundImagePickerCanceled { root, generation },
                Err(error) => UiEvent::SettingsBackgroundImagePickerFailed {
                    root,
                    generation,
                    error,
                },
            };
            let _ = send_critical_ui_event(&tx, event);
        });
    }

    pub(crate) fn apply_settings_font_picked(&mut self, target: SettingsFontTarget, path: String) {
        if !self.settings_panel_open {
            self.status = "Settings panel is closed; font selection discarded".to_owned();
            return;
        }
        match target {
            SettingsFontTarget::Editor => {
                self.settings_editor_font_path = path;
                self.status = "Selected editor font file".to_owned();
            }
            SettingsFontTarget::Ui => {
                self.settings_ui_font_path = path;
                self.status = "Selected UI font file".to_owned();
            }
        }
    }

    pub(crate) fn apply_settings_font_picker_canceled(&mut self, target: SettingsFontTarget) {
        self.status = match target {
            SettingsFontTarget::Editor => "Editor font selection canceled".to_owned(),
            SettingsFontTarget::Ui => "UI font selection canceled".to_owned(),
        };
    }

    pub(crate) fn apply_settings_font_picker_failed(
        &mut self,
        target: SettingsFontTarget,
        error: String,
    ) {
        self.status = font_selection_failure_status(target, &error);
    }

    pub(crate) fn apply_settings_background_image_picked(&mut self, path: String) {
        if !self.settings_panel_open {
            self.status =
                "Settings panel is closed; background image selection discarded".to_owned();
            return;
        }
        self.settings_panel_draft.background_image_path = Some(path);
        self.settings_panel_draft.background_image_enabled = true;
        self.status = "Selected editor background image".to_owned();
    }

    pub(crate) fn apply_settings_background_image_picker_canceled(&mut self) {
        self.status = "Editor background image selection canceled".to_owned();
    }

    pub(crate) fn apply_settings_background_image_picker_failed(&mut self, error: String) {
        self.status = background_image_selection_failure_status(&error);
    }
}

fn font_selection_failure_status(target: SettingsFontTarget, error: &str) -> String {
    let error = display_error_label_cow(error);
    let label = match target {
        SettingsFontTarget::Editor => "editor",
        SettingsFontTarget::Ui => "UI",
    };
    format!("Could not select {label} font file: {}", error.as_ref())
}

fn background_image_selection_failure_status(error: &str) -> String {
    let error = display_error_label_cow(error);
    format!(
        "Could not select editor background image: {}",
        error.as_ref()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        PendingSettingsPanelActions, background_image_selection_failure_status,
        font_selection_failure_status,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext,
        path_display::DISPLAY_ERROR_LABEL_MAX_CHARS, persistence::AppState, terminal::TerminalPane,
        ui_events::SettingsFontTarget, workspace_state::settings_path,
    };
    use kuroya_core::{EditorSettings, EditorVimKeyOverride, EditorVimSettings, Workspace};
    use std::{
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn preferences_font_selection_failure_status_sanitizes_and_bounds_error_detail() {
        let status = font_selection_failure_status(
            SettingsFontTarget::Editor,
            &format!(
                "first line\nsecond line \u{202e}{}",
                "font-error-".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
            ),
        );

        assert!(status.starts_with("Could not select editor font file: first line "));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Could not select editor font file: ".chars().count()
                    + DISPLAY_ERROR_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn preferences_font_selection_failure_status_falls_back_for_blank_error_detail() {
        assert_eq!(
            font_selection_failure_status(SettingsFontTarget::Ui, "\n\u{202e}\u{0007}"),
            "Could not select UI font file: unknown error"
        );
    }

    #[test]
    fn preferences_background_image_selection_updates_only_the_draft() {
        let root = temp_root("background-image-picked");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_open = true;
        app.settings_panel_draft.font_size = 19.0;

        app.apply_settings_background_image_picked("C:/images/background.png".to_owned());

        assert!(app.settings_panel_draft.background_image_enabled);
        assert_eq!(
            app.settings_panel_draft.background_image_path.as_deref(),
            Some("C:/images/background.png")
        );
        assert_eq!(app.settings_panel_draft.font_size, 19.0);
        assert!(app.settings.background_image_path.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preferences_background_image_clear_keeps_enablement_in_the_draft() {
        let root = temp_root("background-image-clear");
        let mut app = app_for_test(
            root.clone(),
            EditorSettings {
                background_image_enabled: true,
                background_image_path: Some("C:/images/background.png".to_owned()),
                ..EditorSettings::default()
            },
        );
        app.settings_panel_open = true;

        app.apply_settings_panel_actions(PendingSettingsPanelActions {
            clear_background_image: true,
            ..PendingSettingsPanelActions::default()
        });

        assert!(app.settings_panel_draft.background_image_enabled);
        assert!(app.settings_panel_draft.background_image_path.is_none());
        assert!(app.settings.background_image_path.is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preferences_background_image_failure_status_sanitizes_error_detail() {
        let status = background_image_selection_failure_status(&format!(
            "first line\nsecond line \u{202e}{}",
            "image-error-".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
        ));

        assert!(status.starts_with("Could not select editor background image: first line "));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
    }

    #[test]
    fn reset_settings_panel_draft_rejects_closed_panel_draft() {
        let root = temp_root("closed-reset-draft");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_open = false;
        app.settings_panel_draft.font_size = 18.0;

        app.reset_settings_panel_draft();

        assert_eq!(app.settings_panel_draft.font_size, 18.0);
        assert_eq!(app.status, "Open settings before resetting draft");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reset_settings_panel_draft_uses_defaults_after_apply() {
        let root = temp_root("reset-applied-settings");
        let settings = EditorSettings {
            font_size: 18.0,
            ui_font_size: 17.0,
            editor_font_path: Some("fonts/current-editor.ttf".to_owned()),
            ui_font_path: Some("fonts/current-ui.ttf".to_owned()),
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings.clone());
        app.settings_panel_open = true;
        app.sync_settings_panel_inputs();

        app.reset_settings_panel_draft();

        let defaults = EditorSettings::default();
        assert_eq!(app.settings.font_size, settings.font_size);
        assert_eq!(app.settings.editor_font_path, settings.editor_font_path);
        assert_eq!(app.settings_panel_draft.font_size, defaults.font_size);
        assert_eq!(app.settings_panel_draft.ui_font_size, defaults.ui_font_size);
        assert_eq!(
            app.settings_panel_draft.editor_font_path,
            defaults.editor_font_path
        );
        assert_eq!(app.settings_panel_draft.ui_font_path, defaults.ui_font_path);
        assert!(app.settings_panel_draft_validation().has_pending_inputs());
        assert_eq!(
            app.status,
            "Reset settings draft to defaults; apply to save"
        );

        app.apply_settings_panel_actions(PendingSettingsPanelActions {
            apply: true,
            ..PendingSettingsPanelActions::default()
        });

        assert_eq!(app.settings.font_size, defaults.font_size);
        assert_eq!(app.settings.ui_font_size, defaults.ui_font_size);
        assert_eq!(app.settings.editor_font_path, defaults.editor_font_path);
        assert_eq!(app.settings.ui_font_path, defaults.ui_font_path);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reset_settings_panel_draft_resets_vim_settings_after_apply() {
        let root = temp_root("reset-vim-settings");
        let settings = EditorSettings {
            vim_keybindings: true,
            vim: EditorVimSettings {
                disabled_bindings: vec!["Q".to_owned()],
                key_overrides: vec![EditorVimKeyOverride {
                    before: "<Home>".to_owned(),
                    after: "0".to_owned(),
                    command: None,
                }],
            },
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings);
        let app_state_path = root.join("app-state.json");
        app.app_state_path_override = Some(app_state_path.clone());
        app.settings_panel_open = true;
        app.sync_settings_panel_inputs();

        app.reset_settings_panel_draft();
        app.apply_settings_panel_actions(PendingSettingsPanelActions {
            apply: true,
            ..PendingSettingsPanelActions::default()
        });

        let defaults = EditorSettings::default();
        assert_eq!(app.settings.vim_keybindings, defaults.vim_keybindings);
        assert_eq!(app.settings.vim, defaults.vim);
        let saved = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
            .unwrap()
            .settings;
        assert_eq!(saved.vim_keybindings, defaults.vim_keybindings);
        assert_eq!(saved.vim, defaults.vim);
        let app_state: AppState =
            serde_json::from_str(&std::fs::read_to_string(app_state_path).unwrap()).unwrap();
        assert_eq!(app_state.vim_keybindings, Some(defaults.vim_keybindings));
        assert_eq!(app_state.vim, Some(defaults.vim));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reset_settings_panel_draft_reports_when_already_default() {
        let root = temp_root("reset-already-default");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_open = true;
        app.sync_settings_panel_inputs();

        app.reset_settings_panel_draft();

        assert!(!app.settings_panel_draft_validation().has_pending_inputs());
        assert_eq!(app.status, "Settings already match defaults");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn closing_settings_panel_discards_stale_draft_inputs() {
        let root = temp_root("close-discards-draft");
        let settings = EditorSettings {
            font_size: 14.0,
            editor_font_path: Some("fonts/current.ttf".to_owned()),
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings.clone());
        app.settings_panel_open = true;
        app.settings_panel_draft.font_size = 22.0;
        app.settings_editor_font_path = "fonts/stale.ttf".to_owned();

        app.apply_settings_panel_actions(PendingSettingsPanelActions {
            close: true,
            ..PendingSettingsPanelActions::default()
        });

        assert!(!app.settings_panel_open);
        assert_eq!(app.settings_panel_draft.font_size, settings.font_size);
        assert_eq!(app.settings_editor_font_path, "fonts/current.ttf");
        assert_eq!(app.status, "Closed settings");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn apply_wins_when_apply_and_close_are_requested_in_the_same_frame() {
        let root = temp_root("apply-beats-close");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_open = true;
        app.settings_panel_draft.font_size = 22.0;

        app.apply_settings_panel_actions(PendingSettingsPanelActions {
            apply: true,
            close: true,
            ..PendingSettingsPanelActions::default()
        });

        assert_eq!(app.settings.font_size, 22.0);
        assert!(app.settings_panel_open);
        assert!(app.status.starts_with("Saved settings"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn font_picks_after_the_settings_panel_closed_are_discarded() {
        let root = temp_root("font-pick-after-close");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_open = false;

        app.apply_settings_font_picked(SettingsFontTarget::Editor, "fonts/picked.ttf".to_owned());

        assert_eq!(app.settings_editor_font_path, "");
        assert_eq!(app.settings_panel_draft.editor_font_path, None);
        assert_eq!(
            app.status,
            "Settings panel is closed; font selection discarded"
        );

        app.apply_settings_font_picked(SettingsFontTarget::Ui, "fonts/picked-ui.ttf".to_owned());

        assert_eq!(app.settings_ui_font_path, "");
        assert_eq!(app.settings_panel_draft.ui_font_path, None);
        assert_eq!(
            app.status,
            "Settings panel is closed; font selection discarded"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn font_picks_while_the_settings_panel_is_open_still_land_in_the_inputs() {
        let root = temp_root("font-pick-panel-open");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_open = true;

        app.apply_settings_font_picked(SettingsFontTarget::Editor, "fonts/picked.ttf".to_owned());
        app.apply_settings_font_picked(SettingsFontTarget::Ui, "fonts/picked-ui.ttf".to_owned());

        assert_eq!(app.settings_editor_font_path, "fonts/picked.ttf");
        assert_eq!(app.settings_ui_font_path, "fonts/picked-ui.ttf");
        assert_eq!(app.status, "Selected UI font file");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn background_image_picks_after_the_settings_panel_closed_are_discarded() {
        let root = temp_root("background-pick-after-close");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_open = false;

        app.apply_settings_background_image_picked("C:/images/late.png".to_owned());

        assert!(app.settings_panel_draft.background_image_path.is_none());
        assert!(!app.settings_panel_draft.background_image_enabled);
        assert_eq!(
            app.status,
            "Settings panel is closed; background image selection discarded"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reset_settings_panel_draft_clears_a_stale_active_custom_theme_path() {
        let root = temp_root("reset-clears-active-theme");
        let theme_path = "themes/custom.toml";
        let settings = EditorSettings {
            custom_theme_paths: vec![theme_path.to_owned()],
            active_custom_theme_path: Some(theme_path.to_owned()),
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings.clone());
        app.settings_panel_open = true;
        app.sync_settings_panel_inputs();

        app.reset_settings_panel_draft();

        assert_eq!(
            app.settings_panel_draft.active_custom_theme_path,
            EditorSettings::default().active_custom_theme_path
        );
        assert_eq!(
            app.settings.active_custom_theme_path.as_deref(),
            Some(theme_path)
        );
        assert!(app.settings_panel_draft_validation().has_pending_inputs());
        assert_eq!(
            app.status,
            "Reset settings draft to defaults; apply to save"
        );

        app.apply_settings_panel_actions(PendingSettingsPanelActions {
            apply: true,
            ..PendingSettingsPanelActions::default()
        });

        assert!(app.settings.active_custom_theme_path.is_none());
        assert!(app.settings.custom_theme_paths.is_empty());
        let _ = std::fs::remove_dir_all(root);
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
            "kuroya-actions-settings-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}

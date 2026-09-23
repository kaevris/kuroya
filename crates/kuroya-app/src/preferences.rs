use crate::{
    KuroyaApp,
    editor_vim_key_events::sanitize_vim_settings_for_runtime,
    path_display::{
        DISPLAY_PATH_LABEL_MAX_CHARS, display_error_label_cow, display_path_label_cow,
        sanitized_display_label_cow,
    },
    settings_form::optional_setting_path_to_input,
    workspace_state::settings_path,
};
use kuroya_core::{EditorSettings, EditorSettingsLoad};
use std::{
    borrow::Cow,
    fmt::Display,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppSettingsLoadSource {
    Loaded,
    MissingDefault,
}

impl AppSettingsLoadSource {
    pub(crate) fn applies_startup_app_state_fallback(self) -> bool {
        matches!(self, Self::MissingDefault)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AppSettingsLoad {
    pub(crate) settings: EditorSettings,
    pub(crate) quarantined_path: Option<PathBuf>,
    pub(crate) source: AppSettingsLoadSource,
}

impl AppSettingsLoad {
    fn loaded(loaded: EditorSettingsLoad) -> Self {
        Self {
            settings: loaded.settings,
            quarantined_path: loaded.quarantined_path,
            source: AppSettingsLoadSource::Loaded,
        }
    }
}

pub(crate) fn load_app_settings(root: &Path) -> anyhow::Result<AppSettingsLoad> {
    let path = settings_path(root);
    if !path.try_exists()? {
        return Ok(default_app_settings_load(
            AppSettingsLoadSource::MissingDefault,
        ));
    }

    let mut loaded = EditorSettings::load_or_create_with_recovery(&path)?;
    if sanitize_vim_settings_for_runtime(&mut loaded.settings.vim) {
        let _ = loaded.settings.save(&path);
    }
    Ok(AppSettingsLoad::loaded(loaded))
}

fn default_app_settings_load(source: AppSettingsLoadSource) -> AppSettingsLoad {
    AppSettingsLoad {
        settings: EditorSettings::default(),
        quarantined_path: None,
        source,
    }
}

impl KuroyaApp {
    pub(crate) fn reload_settings(&mut self) {
        let path = settings_path(&self.workspace.root);
        match load_app_settings(&self.workspace.root) {
            Ok(loaded) => {
                let source = loaded.source;

                let keep_settings_panel_draft = self.settings_panel_has_pending_inputs();
                let previous_settings = std::mem::replace(&mut self.settings, loaded.settings);
                for buffer in &mut self.buffers {
                    buffer.set_word_separators(self.settings.word_separators.clone());
                }
                self.terminal
                    .set_scrollback_rows(self.settings.terminal_scrollback_rows);
                self.terminal.set_shell_profile(
                    self.settings.terminal_shell_path.clone(),
                    self.settings.terminal_shell_args.clone(),
                );
                self.terminal
                    .set_terminal_cwd(self.settings.terminal_cwd.clone());
                self.terminal
                    .set_split_cwd(self.settings.terminal_split_cwd);
                self.terminal.set_minimum_size(
                    self.settings.terminal_min_rows,
                    self.settings.terminal_min_columns,
                );
                self.terminal.set_font_metrics(
                    self.settings.terminal_font_size,
                    self.settings.terminal_line_height,
                    self.settings.terminal_letter_spacing,
                );
                self.terminal.set_cursor_settings(
                    self.settings.terminal_cursor_style,
                    self.settings.terminal_cursor_width,
                    self.settings.terminal_cursor_blinking,
                    self.settings.terminal_cursor_style_inactive,
                );
                self.terminal.set_draw_bold_text_in_bright_colors(
                    self.settings.terminal_draw_bold_text_in_bright_colors,
                );
                self.terminal
                    .set_minimum_contrast_ratio(self.settings.terminal_minimum_contrast_ratio);
                self.terminal.set_bell_settings(
                    self.settings.terminal_enable_bell,
                    self.settings.terminal_bell_duration_ms,
                );
                self.terminal
                    .set_show_exit_alert(self.settings.terminal_show_exit_alert);
                self.terminal
                    .set_hide_on_last_closed(self.settings.terminal_hide_on_last_closed);
                self.terminal
                    .set_confirm_on_kill(self.settings.terminal_confirm_on_kill);
                self.terminal
                    .set_tabs_enabled(self.settings.terminal_tabs_enabled);
                self.terminal
                    .set_tabs_default_icon(&self.settings.terminal_tabs_default_icon);
                self.terminal
                    .set_tabs_default_color(self.settings.terminal_tabs_default_color.clone());
                self.terminal.set_tabs_allow_agent_cli_title(
                    self.settings.terminal_tabs_allow_agent_cli_title,
                );
                self.terminal
                    .set_tabs_title_template(&self.settings.terminal_tabs_title);
                self.terminal
                    .set_tabs_hide_condition(self.settings.terminal_tabs_hide_condition);
                self.terminal.set_tabs_show_active_terminal(
                    self.settings.terminal_tabs_show_active_terminal,
                );
                self.terminal
                    .set_tabs_show_actions(self.settings.terminal_tabs_show_actions);
                self.terminal
                    .set_tabs_focus_mode(self.settings.terminal_tabs_focus_mode);
                self.terminal
                    .set_tabs_location(self.settings.terminal_tabs_location);
                self.terminal
                    .set_right_click_behavior(self.settings.terminal_right_click_behavior);
                self.terminal
                    .set_middle_click_behavior(self.settings.terminal_middle_click_behavior);
                self.terminal
                    .set_alt_click_moves_cursor(self.settings.terminal_alt_click_moves_cursor);
                self.terminal
                    .set_copy_on_selection(self.settings.terminal_copy_on_selection);
                self.terminal.set_ignore_bracketed_paste_mode(
                    self.settings.terminal_ignore_bracketed_paste_mode,
                );
                self.terminal.set_multi_line_paste_warning(
                    self.settings.terminal_enable_multi_line_paste_warning,
                );
                self.terminal
                    .set_word_separators(self.settings.terminal_word_separators.clone());
                self.terminal.set_scroll_sensitivity(
                    self.settings.terminal_mouse_wheel_scroll_sensitivity,
                    self.settings.terminal_fast_scroll_sensitivity,
                );
                self.terminal
                    .set_mouse_wheel_zoom(self.settings.terminal_mouse_wheel_zoom);
                if keep_settings_panel_draft {
                } else {
                    self.sync_settings_panel_inputs();
                }
                self.theme_picker_selected = self.selected_theme_picker_index();
                match source {
                    AppSettingsLoadSource::Loaded => {
                        self.status =
                            settings_reload_status(&path, loaded.quarantined_path.as_deref());
                        self.sync_app_state_settings_after_reload(&previous_settings);
                    }
                    AppSettingsLoadSource::MissingDefault => {
                        self.status = settings_reload_missing_default_status(&path);
                    }
                }
                if keep_settings_panel_draft {
                    self.status.push_str(SETTINGS_RELOAD_KEPT_DRAFT_NOTE);
                }
                self.theme_dirty = true;
                self.fonts_dirty = true;
                if previous_settings.vim_keybindings != self.settings.vim_keybindings
                    || previous_settings.vim != self.settings.vim
                {
                    self.vim_reset_session_state();
                }
                if previous_settings.read_only != self.settings.read_only {
                    self.sync_global_read_only_buffers();
                }
                if previous_settings.plugins != self.settings.plugins {
                    self.sync_plugin_settings_state();
                }
                if previous_settings.discord != self.settings.discord {
                    self.sync_discord_presence_runtime();
                }
                self.sync_lsp_server_settings_after_reload(&previous_settings);
                let project_index_settings_changed = previous_settings.project_index_max_files
                    != self.settings.project_index_max_files
                    || previous_settings.project_index_exclude_globs
                        != self.settings.project_index_exclude_globs
                    || previous_settings.project_index_include_hidden_dirs
                        != self.settings.project_index_include_hidden_dirs;
                let project_search_settings_changed = previous_settings
                    .project_search_exclude_globs
                    != self.settings.project_search_exclude_globs
                    || previous_settings.project_search_max_file_size_mb
                        != self.settings.project_search_max_file_size_mb
                    || previous_settings.project_search_max_results
                        != self.settings.project_search_max_results;
                if project_search_settings_changed {
                    self.invalidate_project_search_requests();
                    self.project_search_metadata_cache.clear();
                    self.sync_project_search_after_settings_change();
                }
                if project_index_settings_changed {
                    self.spawn_index();
                }
                let git_repository_scan_settings_changed = previous_settings
                    .git_auto_repository_detection
                    != self.settings.git_auto_repository_detection
                    || previous_settings.git_ignore_submodules
                        != self.settings.git_ignore_submodules
                    || previous_settings.git_repository_scan_ignored_folders
                        != self.settings.git_repository_scan_ignored_folders
                    || previous_settings.git_open_repository_in_parent_folders
                        != self.settings.git_open_repository_in_parent_folders
                    || previous_settings.git_detect_submodules
                        != self.settings.git_detect_submodules
                    || previous_settings.git_detect_submodules_limit
                        != self.settings.git_detect_submodules_limit
                    || previous_settings.git_repository_scan_max_depth
                        != self.settings.git_repository_scan_max_depth
                    || previous_settings.git_detect_worktrees != self.settings.git_detect_worktrees
                    || previous_settings.git_detect_worktrees_limit
                        != self.settings.git_detect_worktrees_limit
                    || previous_settings.git_worktree_include_files
                        != self.settings.git_worktree_include_files
                    || previous_settings.git_similarity_threshold
                        != self.settings.git_similarity_threshold;
                let git_ignored_repositories_changed = previous_settings.git_ignored_repositories
                    != self.settings.git_ignored_repositories;
                if previous_settings.diff_max_file_size_mb != self.settings.diff_max_file_size_mb
                    || previous_settings.git_enabled != self.settings.git_enabled
                    || git_ignored_repositories_changed
                    || git_repository_scan_settings_changed
                {
                    self.invalidate_virtual_source_control_open_requests();
                }
                if previous_settings.git_blame_ignore_whitespace
                    != self.settings.git_blame_ignore_whitespace
                {
                    self.sync_source_control_blame_settings();
                }
                if previous_settings.git_enabled != self.settings.git_enabled {
                    self.sync_git_enabled_state();
                } else if previous_settings.git_autorefresh != self.settings.git_autorefresh {
                    self.sync_git_autorefresh_state(previous_settings.git_autorefresh);
                } else if git_ignored_repositories_changed {
                    self.sync_git_repository_filters_state();
                } else if git_repository_scan_settings_changed {
                    self.spawn_git_scan();
                }
                self.sync_background_image(true);
            }
            Err(error) => {
                self.status = settings_load_failed_status(error);
            }
        }
    }

    pub(crate) fn sync_settings_panel_inputs(&mut self) {
        self.settings_panel_draft = self.settings.clone();
        self.settings_editor_font_path =
            optional_setting_path_to_input(&self.settings.editor_font_path);
        self.settings_ui_font_path = optional_setting_path_to_input(&self.settings.ui_font_path);
    }

    pub(crate) fn open_settings_file(&mut self) {
        let path = settings_path(&self.workspace.root);
        if !path.exists() {
            if let Err(error) = self.settings.save(&path) {
                self.status = settings_create_failed_status(error);
                return;
            }
        }
        self.spawn_open_file(path.clone());
        self.status = settings_opening_status(&path);
    }

    fn sync_app_state_settings_after_reload(&mut self, previous_settings: &EditorSettings) {
        let vim_changed = self.app_state_vim_keybindings != self.settings.vim_keybindings
            || self.app_state_vim != self.settings.vim;
        let appearance_changed =
            app_state_appearance_settings_changed(previous_settings, &self.settings);
        if !vim_changed && !appearance_changed {
            return;
        }

        if vim_changed {
            self.app_state_vim_keybindings = self.settings.vim_keybindings;
            self.app_state_vim = self.settings.vim.clone();
        }
        if let Err(error) = self.save_app_state() {
            push_app_state_reload_save_failed_status(&mut self.status, error);
        }
    }
}

const SETTINGS_RELOAD_KEPT_DRAFT_NOTE: &str =
    "; kept unsaved settings changes; external changes loaded for other settings";

fn app_state_appearance_settings_changed(
    previous: &EditorSettings,
    current: &EditorSettings,
) -> bool {
    previous.theme != current.theme
        || previous.custom_theme_paths != current.custom_theme_paths
        || previous.active_custom_theme_path != current.active_custom_theme_path
        || previous.editor_font_path != current.editor_font_path
        || previous.ui_font_path != current.ui_font_path
}

fn push_app_state_reload_save_failed_status(status: &mut String, error: impl Display) {
    let error = error.to_string();
    let error = display_error_label_cow(&error);
    status.push_str("; app preference save failed: ");
    status.push_str(error.as_ref());
}

pub(crate) fn settings_reload_status(path: &Path, quarantined_path: Option<&Path>) -> String {
    let path = settings_path_status_label(path);
    match quarantined_path {
        Some(quarantined_path) => {
            let quarantined_path = display_path_label_cow(quarantined_path);
            format!(
                "Recovered settings from {}; corrupt file moved to {}",
                path.as_ref(),
                quarantined_path.as_ref()
            )
        }
        None => format!("Loaded settings from {}", path.as_ref()),
    }
}

fn settings_reload_missing_default_status(path: &Path) -> String {
    let path = settings_path_status_label(path);
    format!(
        "App settings not found at {}; using defaults",
        path.as_ref()
    )
}

fn settings_load_failed_status(error: impl Display) -> String {
    let error = error.to_string();
    let error = settings_error_status_label(&error);
    format!("Could not load settings: {}", error.as_ref())
}

fn settings_create_failed_status(error: impl Display) -> String {
    let error = error.to_string();
    let error = settings_error_status_label(&error);
    format!("Could not create settings file: {}", error.as_ref())
}

fn settings_opening_status(path: &Path) -> String {
    let path = display_path_label_cow(path);
    format!("Opening settings {}", path.as_ref())
}

fn settings_path_status_label(path: &Path) -> Cow<'_, str> {
    if let Some(path) = path.to_str() {
        return sanitized_display_label_cow(path, DISPLAY_PATH_LABEL_MAX_CHARS, ".");
    }

    Cow::Owned(settings_path_owned_display_label(
        path.display().to_string(),
    ))
}

fn settings_path_owned_display_label(path: String) -> String {
    match sanitized_display_label_cow(&path, DISPLAY_PATH_LABEL_MAX_CHARS, ".") {
        Cow::Borrowed(label) if label.as_ptr() == path.as_ptr() && label.len() == path.len() => {
            path
        }
        Cow::Borrowed(label) => label.to_owned(),
        Cow::Owned(label) => label,
    }
}

fn settings_error_status_label(error: &str) -> Cow<'_, str> {
    display_error_label_cow(error)
}

#[cfg(test)]
mod tests {
    use super::{
        AppSettingsLoadSource, KuroyaApp, load_app_settings, settings_create_failed_status,
        settings_load_failed_status, settings_opening_status, settings_path_status_label,
        settings_reload_missing_default_status, settings_reload_status,
    };
    use crate::{
        app_startup_context::AppStartupContext,
        lsp_client::LspClientHandle,
        path_display::{DISPLAY_ERROR_LABEL_MAX_CHARS, DISPLAY_PATH_LABEL_MAX_CHARS},
        terminal::TerminalPane,
        workspace_state::settings_path,
    };
    use kuroya_core::{
        EditorSettings, EditorVimKeyOverride, EditorVimSettings, PluginCapabilities,
        PluginContributions, PluginDescriptor, PluginManifest, TextBuffer, ThemeSettings,
        Workspace,
    };
    use std::{
        borrow::Cow,
        fs,
        path::{Path, PathBuf},
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn settings_reload_status_reports_recovered_corrupt_file() {
        assert_eq!(
            settings_reload_status(
                Path::new("workspace/.kuroya/settings.toml"),
                Some(Path::new("workspace/.kuroya/settings.toml.corrupt.42"))
            ),
            "Recovered settings from workspace/.kuroya/settings.toml; corrupt file moved to settings.toml.corrupt.42"
        );
    }

    #[test]
    fn settings_reload_status_sanitizes_and_bounds_paths() {
        let path = Path::new("workspace/.kuroya")
            .join(format!("bad\n{}\u{202e}.toml", "segment-".repeat(40)));
        let quarantined_path = Path::new("workspace/.kuroya")
            .join(format!("corrupt\n{}\u{202e}.toml", "tail-".repeat(40)));

        let status = settings_reload_status(&path, Some(&quarantined_path));

        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Recovered settings from ; corrupt file moved to "
                    .chars()
                    .count()
                    + DISPLAY_PATH_LABEL_MAX_CHARS
                    + DISPLAY_PATH_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn settings_path_status_label_borrows_clean_ascii_and_unicode_paths() {
        for path in [
            Path::new("workspace/.kuroya/settings.toml"),
            Path::new("workspace/.kuroya/settings-\u{03bb}.toml"),
        ] {
            let expected = path.to_str().expect("test path is UTF-8");

            match settings_path_status_label(path) {
                Cow::Borrowed(label) => assert_eq!(label, expected),
                Cow::Owned(label) => panic!("expected borrowed settings path, got {label:?}"),
            }
        }
    }

    #[test]
    fn settings_path_status_label_owns_dirty_truncated_and_fallback_paths() {
        let long = Path::new("workspace/.kuroya").join(format!(
            "{}settings.toml",
            "very-long-segment-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));
        for path in [
            Path::new("workspace/.kuroya/bad\nsettings\u{202e}.toml"),
            long.as_path(),
            Path::new("\n\t\u{202e}"),
        ] {
            let label = settings_path_status_label(path);

            assert!(matches!(&label, Cow::Owned(_)));
            assert!(!label.contains('\n'));
            assert!(!label.contains('\u{202e}'));
            assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        }
        assert_eq!(
            settings_path_status_label(Path::new("\n\t\u{202e}")).as_ref(),
            "."
        );
    }

    #[test]
    fn settings_error_statuses_sanitize_and_bound_errors() {
        let error = format!(
            "first line\nsecond line \u{202e}{}",
            "x".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
        );

        let load_status = settings_load_failed_status(&error);
        let create_status = settings_create_failed_status(&error);

        for status in [load_status, create_status] {
            assert!(!status.contains('\n'));
            assert!(!status.contains('\u{202e}'));
            assert!(status.contains("..."));
            assert!(
                status.chars().count()
                    <= "Could not create settings file: ".chars().count()
                        + DISPLAY_ERROR_LABEL_MAX_CHARS
            );
        }
    }

    #[test]
    fn settings_opening_status_sanitizes_and_bounds_path() {
        let path = Path::new("workspace/.kuroya")
            .join(format!("bad\n{}\u{202e}.toml", "segment-".repeat(40)));

        let status = settings_opening_status(&path);

        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Opening settings ".chars().count() + DISPLAY_PATH_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn reload_settings_keeps_pending_draft_while_settings_panel_is_open() {
        let root = temp_root("reload-keeps-pending-draft");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, "word_separators = \".\"\n").unwrap();

        let mut app = app_for_test(root.clone());
        app.settings_panel_open = true;
        app.settings_panel_draft.font_size = 19.0;

        app.reload_settings();

        assert_eq!(app.settings_panel_draft.font_size, 19.0);
        assert_eq!(app.settings.word_separators, ".");
        assert!(
            app.status.contains("kept unsaved settings changes"),
            "unexpected status: {}",
            app.status
        );

        drop(app);
        remove_root(&root);
    }

    #[test]
    fn reload_settings_resyncs_clean_draft_while_settings_panel_is_open() {
        let root = temp_root("reload-resyncs-clean-draft");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, "word_separators = \".\"\n").unwrap();

        let mut app = app_for_test(root.clone());
        app.settings_panel_open = true;

        app.reload_settings();

        assert_eq!(app.settings_panel_draft, app.settings);
        assert_eq!(app.status, settings_reload_status(&settings, None));

        drop(app);
        remove_root(&root);
    }

    #[test]
    fn reload_settings_resyncs_pending_draft_when_settings_panel_is_closed() {
        let root = temp_root("reload-resyncs-closed-panel-draft");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, "word_separators = \".\"\n").unwrap();

        let mut app = app_for_test(root.clone());
        app.settings_panel_open = false;
        app.settings_panel_draft.font_size = 19.0;

        app.reload_settings();

        assert_eq!(app.settings_panel_draft, app.settings);
        assert_eq!(app.settings_panel_draft.font_size, app.settings.font_size);
        assert_eq!(app.status, settings_reload_status(&settings, None));

        drop(app);
        remove_root(&root);
    }

    #[test]
    fn reload_settings_updates_open_buffer_word_separators() {
        let root = temp_root("word-separators");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, "word_separators = \".\"\n").unwrap();

        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(root.join("src/main.rs")),
            "alpha.beta".to_owned(),
        ));

        app.reload_settings();

        assert_eq!(app.settings.word_separators, ".");
        assert_eq!(app.buffer(1).unwrap().word_separators(), ".");

        remove_root(&root);
    }

    #[test]
    fn reload_settings_applies_app_vim_keybindings() {
        let root = temp_root("apply-vim-keybindings");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            "vim_keybindings = false\n\
             word_separators = \".\"\n\
             [vim]\n\
             disabled_bindings = [\"<C-n>\"]\n\
             [[vim.key_overrides]]\n\
             before = \"<Home>\"\n\
             after = \"<C-r>\"\n\
             [[vim.key_overrides]]\n\
             before = \"L\"\n\
             after = \"<Left>\"\n",
        )
        .unwrap();

        let mut app = app_for_test(root.clone());
        app.settings.vim_keybindings = true;
        app.editor_vim_mode = crate::editor_vim_key_events::EditorVimMode::Insert;

        app.reload_settings();

        assert!(!app.settings.vim_keybindings);
        assert_eq!(app.settings.vim.disabled_bindings, ["<C-n>"]);
        assert_eq!(
            app.settings.vim.key_overrides,
            [EditorVimKeyOverride {
                before: "<Home>".to_owned(),
                after: "<C-r>".to_owned(),
                command: None,
            }]
        );
        assert_eq!(
            app.editor_vim_mode,
            crate::editor_vim_key_events::EditorVimMode::Normal
        );
        assert_eq!(app.settings.word_separators, ".");
        remove_root(&root);
    }

    #[test]
    fn reload_updates_app_state_vim_copy() {
        let root = temp_root("reload-vim-keeps-app-state-fallback");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            "vim_keybindings = false\n\
             [vim]\n\
             disabled_bindings = [\"<C-n>\"]\n",
        )
        .unwrap();

        let mut app = app_for_test(root.clone());
        app.app_state_vim_keybindings = true;
        app.app_state_vim = EditorVimSettings {
            disabled_bindings: vec!["Q".to_owned()],
            key_overrides: Vec::new(),
        };

        app.reload_settings();

        assert!(!app.settings.vim_keybindings);
        assert_eq!(app.settings.vim.disabled_bindings, ["<C-n>"]);
        assert!(!app.app_state_vim_keybindings);
        assert_eq!(app.app_state_vim.disabled_bindings, ["<C-n>"]);
        let app_state: crate::persistence::AppState =
            serde_json::from_str(&fs::read_to_string(root.join("app-state.json")).unwrap())
                .unwrap();
        assert_eq!(app_state.vim_keybindings, Some(false));
        assert_eq!(app_state.vim, Some(app.settings.vim.clone()));
        remove_root(&root);
    }

    #[test]
    fn load_app_settings_persists_runtime_sanitized_vim_settings() {
        let root = temp_root("load-vim-persists-runtime-sanitize");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            "vim_keybindings = true\n\
             [vim]\n\
             disabled_bindings = [\"<C-n>\", \"<Missing>\"]\n\
             [[vim.key_overrides]]\n\
             before = \"Q\"\n\
             after = \"<Esc>\"\n\
             [[vim.key_overrides]]\n\
             before = \"Z\"\n\
             after = \"<Missing>\"\n",
        )
        .unwrap();

        let loaded = load_app_settings(&root).unwrap();

        assert_eq!(loaded.source, AppSettingsLoadSource::Loaded);
        assert_eq!(loaded.settings.vim.disabled_bindings, ["<C-n>"]);
        assert_eq!(
            loaded.settings.vim.key_overrides,
            [EditorVimKeyOverride {
                before: "Q".to_owned(),
                after: "<Esc>".to_owned(),
                command: None,
            }]
        );
        let saved = fs::read_to_string(settings).unwrap();
        assert!(saved.contains("<C-n>"));
        assert!(saved.contains("before = \"Q\""));
        assert!(!saved.contains("<Missing>"));
        assert!(!saved.contains("before = \"Z\""));
        remove_root(&root);
    }

    #[test]
    fn reload_settings_uses_global_app_settings_in_untrusted_workspace() {
        let root = temp_root("untrusted-loads-app-settings");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            "vim_keybindings = false\nword_separators = \".\"\n",
        )
        .unwrap();

        let mut app = app_for_test_with_trust(root.clone(), false);
        app.app_state_vim_keybindings = true;
        app.app_state_vim.disabled_bindings = vec!["Q".to_owned(), "<C-n>".to_owned()];
        app.app_state_vim.key_overrides = vec![EditorVimKeyOverride {
            before: "<Home>".to_owned(),
            after: "<C-r>".to_owned(),
            command: None,
        }];

        app.reload_settings();

        assert!(!app.workspace_trusted);
        assert!(!app.settings.vim_keybindings);
        assert!(app.settings.vim.disabled_bindings.is_empty());
        assert!(app.settings.vim.key_overrides.is_empty());
        assert_eq!(app.settings.word_separators, ".");
        assert_eq!(app.status, settings_reload_status(&settings, None));
        remove_root(&root);
    }

    #[test]
    fn load_app_settings_uses_defaults_without_creating_missing_file() {
        let root = temp_root("missing-app-settings-defaults");
        fs::create_dir_all(&root).unwrap();
        let settings = settings_path(&root);

        let loaded = load_app_settings(&root).unwrap();

        assert_eq!(loaded.source, AppSettingsLoadSource::MissingDefault);
        assert_eq!(loaded.settings, EditorSettings::default());
        assert_eq!(loaded.quarantined_path, None);
        assert!(!settings.exists());
        assert!(!settings.parent().unwrap().exists());
        remove_root(&root);
    }

    #[test]
    fn reload_settings_uses_defaults_without_creating_missing_app_settings() {
        let root = temp_root("missing-app-settings-reload-defaults");
        fs::create_dir_all(&root).unwrap();
        let settings = settings_path(&root);
        let mut app = app_for_test(root.clone());
        app.settings.word_separators = ".".to_owned();

        app.reload_settings();

        assert_eq!(
            app.settings.word_separators,
            EditorSettings::default().word_separators
        );
        assert_eq!(
            app.status,
            settings_reload_missing_default_status(&settings)
        );
        assert!(!settings.exists());
        assert!(!settings.parent().unwrap().exists());
        remove_root(&root);
    }

    #[test]
    fn reload_settings_uses_saved_appearance_in_untrusted_workspace() {
        let root = temp_root("untrusted-loads-saved-appearance");
        let settings_path = settings_path(&root);
        let saved_theme = ThemeSettings {
            name: "Saved Global Theme".to_owned(),
            background: [248, 249, 250],
            panel: [255, 255, 255],
            panel_alt: [238, 241, 245],
            text: [32, 36, 42],
            muted_text: [95, 104, 118],
            accent: [47, 111, 237],
            selection: None,
            warning: [161, 104, 24],
            error: [190, 50, 50],
        };
        let custom_theme_path = root.join("themes/saved.toml").display().to_string();
        let editor_font_path = root.join("fonts/editor.ttf").display().to_string();
        let ui_font_path = root.join("fonts/ui.ttf").display().to_string();
        let saved_settings = EditorSettings {
            theme: saved_theme.clone(),
            custom_theme_paths: vec![custom_theme_path.clone()],
            active_custom_theme_path: Some(custom_theme_path.clone()),
            editor_font_path: Some(editor_font_path.clone()),
            ui_font_path: Some(ui_font_path.clone()),
            ..EditorSettings::default()
        };
        saved_settings.save(&settings_path).unwrap();

        let mut app = app_for_test_with_trust(root.clone(), false);

        app.reload_settings();

        assert!(!app.workspace_trusted);
        assert_eq!(app.settings.theme, saved_theme);
        assert_eq!(
            app.settings.custom_theme_paths,
            std::slice::from_ref(&custom_theme_path)
        );
        assert_eq!(
            app.settings.active_custom_theme_path.as_deref(),
            Some(custom_theme_path.as_str())
        );
        assert_eq!(
            app.settings.editor_font_path.as_deref(),
            Some(editor_font_path.as_str())
        );
        assert_eq!(
            app.settings.ui_font_path.as_deref(),
            Some(ui_font_path.as_str())
        );
        assert_eq!(app.settings_panel_draft.theme, app.settings.theme);
        assert_eq!(app.status, settings_reload_status(&settings_path, None));
        remove_root(&root);
    }

    #[test]
    fn reload_settings_restores_saved_appearance_selection() {
        let root = temp_root("reload-saved-appearance");
        let settings_path = settings_path(&root);
        let saved_theme = ThemeSettings {
            name: "Saved Theme".to_owned(),
            background: [16, 18, 22],
            panel: [24, 28, 34],
            panel_alt: [34, 40, 48],
            text: [230, 236, 244],
            muted_text: [135, 146, 160],
            accent: [88, 166, 255],
            selection: None,
            warning: [210, 153, 34],
            error: [248, 81, 73],
        };
        let custom_theme_path = root.join("themes").join("saved.toml").display().to_string();
        let custom_theme_paths = vec![
            custom_theme_path.clone(),
            ".kuroya/themes/relative.toml".to_owned(),
        ];
        let active_custom_theme_path = Some(custom_theme_path.clone());
        let editor_font_path = Some(
            root.join("fonts")
                .join("saved-editor.ttf")
                .display()
                .to_string(),
        );
        let ui_font_path = Some(
            root.join("fonts")
                .join("saved-ui.ttf")
                .display()
                .to_string(),
        );
        let saved_settings = EditorSettings {
            theme: saved_theme.clone(),
            custom_theme_paths: custom_theme_paths.clone(),
            active_custom_theme_path: active_custom_theme_path.clone(),
            editor_font_path: editor_font_path.clone(),
            ui_font_path: ui_font_path.clone(),
            ..EditorSettings::default()
        };
        saved_settings.save(&settings_path).unwrap();

        let mut app = app_for_test(root.clone());

        app.reload_settings();

        assert_eq!(app.settings.theme, saved_theme);
        assert_eq!(app.settings.custom_theme_paths, custom_theme_paths);
        assert_eq!(
            app.settings.active_custom_theme_path,
            active_custom_theme_path
        );
        assert_eq!(app.settings.editor_font_path, editor_font_path);
        assert_eq!(app.settings.ui_font_path, ui_font_path);
        assert_eq!(app.settings_panel_draft.theme, app.settings.theme);
        let app_state: crate::persistence::AppState =
            serde_json::from_str(&fs::read_to_string(root.join("app-state.json")).unwrap())
                .unwrap();
        assert_eq!(app_state.theme, Some(saved_theme));
        assert_eq!(app_state.custom_theme_paths, vec![custom_theme_path]);
        assert_eq!(app_state.active_custom_theme_path, active_custom_theme_path);
        assert_eq!(app_state.editor_font_path, editor_font_path);
        assert_eq!(app_state.ui_font_path, ui_font_path);
        remove_root(&root);
    }

    #[test]
    fn reload_settings_adds_configured_lsp_server_and_retries_unavailable_language() {
        let root = temp_root("reload-lsp-configured-server");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            r#"
            [[lsp_servers]]
            language = "go"
            command = "gopls"
            root_markers = ["go.mod", "go.work"]
            "#,
        )
        .unwrap();

        let mut app = app_for_test(root.clone());
        app.lsp_unavailable.insert("go".to_owned());
        app.lsp_restart_attempts.insert("go".to_owned(), 2);
        app.pending_lsp_restarts
            .insert("go".to_owned(), Instant::now());

        app.reload_settings();

        let go = app
            .settings
            .lsp_server_configs()
            .into_iter()
            .find(|config| config.language == "go")
            .expect("configured Go LSP should be available");
        assert_eq!(go.command, "gopls");
        assert!(!app.lsp_unavailable.contains("go"));
        assert!(!app.lsp_restart_attempts.contains_key("go"));
        assert!(!app.pending_lsp_restarts.contains_key("go"));
        remove_root(&root);
    }

    #[test]
    fn reload_settings_stops_active_lsp_clients_when_server_config_changes() {
        let root = temp_root("reload-lsp-config-change");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            r#"
            [[lsp_servers]]
            language = "rust"
            command = "rust-analyzer-custom"
            root_markers = ["Cargo.toml"]
            "#,
        )
        .unwrap();

        let mut app = app_for_test(root.clone());
        app.lsp_clients
            .insert("rust".to_owned(), LspClientHandle::accepting_for_test());
        app.lsp_unavailable.insert("rust".to_owned());
        app.lsp_restart_attempts.insert("rust".to_owned(), 2);
        app.pending_lsp_restarts
            .insert("rust".to_owned(), Instant::now());

        app.reload_settings();

        assert!(app.lsp_clients.is_empty());
        assert!(app.lsp_unavailable.is_empty());
        assert!(app.lsp_restart_attempts.is_empty());
        assert!(app.pending_lsp_restarts.is_empty());
        assert_eq!(
            app.settings
                .lsp_server_configs()
                .into_iter()
                .find(|config| config.language == "rust")
                .expect("rust config should be present")
                .command,
            "rust-analyzer-custom"
        );
        remove_root(&root);
    }

    #[test]
    fn reload_settings_applies_external_plugin_edits_and_resyncs_discovery() {
        let root = temp_root("reload-plugin-settings");
        let settings = settings_path(&root);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            "[plugins]\n\
             enabled = false\n\
             disabled_ids = [\"external.plugin\"]\n",
        )
        .unwrap();
        let mut app = app_for_test(root.clone());
        app.plugins.push(test_plugin_descriptor("loaded.plugin"));

        app.reload_settings();

        assert!(!app.settings.plugins.enabled);
        assert_eq!(app.settings.plugins.disabled_ids, ["external.plugin"]);
        assert!(app.plugins.is_empty());
        assert_eq!(app.workspace_plugins_in_flight_request_id, None);
        assert_eq!(app.settings_panel_draft, app.settings);

        fs::write(
            &settings,
            "[plugins]\n\
             enabled = true\n\
             disabled_ids = [\"external.plugin\"]\n",
        )
        .unwrap();

        app.reload_settings();

        assert!(app.settings.plugins.enabled);
        assert_eq!(app.workspace_plugins_in_flight_request_id, Some(2));
        remove_root(&root);
    }

    #[test]
    fn open_settings_file_creates_missing_settings_and_opens_it() {
        let root = temp_root("open-settings-missing");
        let settings_path = settings_path(&root);
        let mut app = app_for_test(root.clone());
        app.settings.word_separators = ".".to_owned();

        app.open_settings_file();

        assert!(settings_path.is_file());
        assert_eq!(app.status, settings_opening_status(&settings_path));
        assert!(app.pending_open_paths.contains(&settings_path));
        assert_eq!(
            EditorSettings::load_or_create(&settings_path)
                .unwrap()
                .word_separators,
            "."
        );

        drop(app);
        remove_root(&root);
    }

    #[test]
    fn open_settings_file_preserves_existing_settings_and_opens_it() {
        let root = temp_root("open-settings-existing");
        let settings_path = settings_path(&root);
        let existing_settings = "word_separators = \":\"\n";
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(&settings_path, existing_settings).unwrap();
        let mut app = app_for_test(root.clone());
        app.settings.word_separators = ".".to_owned();

        app.open_settings_file();

        assert_eq!(
            fs::read_to_string(&settings_path).unwrap(),
            existing_settings
        );
        assert_eq!(app.status, settings_opening_status(&settings_path));
        assert!(app.pending_open_paths.contains(&settings_path));

        drop(app);
        remove_root(&root);
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        app_for_test_with_trust(root, true)
    }

    fn test_plugin_descriptor(id: &str) -> PluginDescriptor {
        PluginDescriptor {
            root: PathBuf::from(format!(".kuroya/plugins/{id}")),
            manifest: PluginManifest {
                api_version: "1".to_owned(),
                id: id.to_owned(),
                name: id.to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities::default(),
                contributes: PluginContributions::default(),
            },
        }
    }

    fn app_for_test_with_trust(root: PathBuf, trusted: bool) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        let trusted_workspaces = trusted.then(|| root.clone()).into_iter().collect();
        let mut app = KuroyaApp::from_startup_context(AppStartupContext {
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
            trusted_workspaces,
            now: Instant::now(),
            startup_timings: Vec::new(),
        });
        app.app_state_path_override = Some(root.join("app-state.json"));
        app
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-settings-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn remove_root(root: &Path) {
        match fs::remove_dir_all(root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("failed to remove temp root: {error}"),
        }
    }
}

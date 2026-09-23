use crate::{
    KuroyaApp,
    fs_watcher::note_app_write,
    path_display::display_error_label_cow,
    source_control_panel::{
        source_control_sort_mode_from_setting, source_control_view_mode_from_setting,
    },
    workspace_state::settings_path,
};
use kuroya_core::EditorSettings;
use std::fmt::Display;

mod draft;
mod terminal;
mod validation;

use terminal::sync_terminal_settings;
use validation::{SettingsPanelDraftValidation, validate_settings_panel_draft};

impl KuroyaApp {
    pub(super) fn settings_panel_default_candidate(&self) -> EditorSettings {
        let defaults = EditorSettings::default();
        let mut candidate = self.settings.clone();
        draft::apply_settings_panel_draft_with_font_paths(
            &mut candidate,
            &defaults,
            defaults.editor_font_path.clone(),
            defaults.ui_font_path.clone(),
        );
        candidate
    }

    pub(super) fn settings_panel_draft_validation(&self) -> SettingsPanelDraftValidation {
        validate_settings_panel_draft(
            &self.settings,
            &self.settings_panel_draft,
            &self.settings_editor_font_path,
            &self.settings_ui_font_path,
        )
    }

    pub(super) fn apply_settings_panel(&mut self) {
        if !self.settings_panel_open {
            self.status = "Open settings before applying changes".to_owned();
            return;
        }

        let validation = self.settings_panel_draft_validation();
        if !validation.has_pending_inputs() {
            self.status = "No pending settings changes".to_owned();
            return;
        }

        let apply_note = validation.apply_note();
        let next_settings = validation.into_candidate();
        if next_settings == self.settings {
            self.sync_settings_panel_inputs();
            self.status = match apply_note {
                Some(note) => format!("Settings draft {note}; no saved changes"),
                None => "Settings already match the current configuration".to_owned(),
            };
            return;
        }

        let previous_settings = self.settings.clone();
        let previous_fonts = (
            self.settings.font_size,
            self.settings.ui_font_size,
            self.settings.editor_font_path.clone(),
            self.settings.ui_font_path.clone(),
        );
        let previous_app_state_appearance = (
            self.settings.theme.clone(),
            self.settings.custom_theme_paths.clone(),
            self.settings.active_custom_theme_path.clone(),
            self.settings.editor_font_path.clone(),
            self.settings.ui_font_path.clone(),
        );
        let previous_theme = (
            self.settings.theme.clone(),
            self.settings.active_custom_theme_path.clone(),
        );
        let previous_read_only = self.settings.read_only;
        let previous_vim_settings = (self.settings.vim_keybindings, self.settings.vim.clone());
        let previous_discord_settings = self.settings.discord.clone();
        let previous_inline_annotations = (self.settings.code_lens, self.settings.inlay_hints);
        let previous_navigation_annotations = (
            self.settings.hover_enabled,
            self.settings.document_highlights_enabled,
        );
        let previous_parameter_hints = self.settings.parameter_hints_enabled;
        let previous_source_control_defaults = (
            self.settings.scm_default_view_mode,
            self.settings.scm_default_view_sort_key,
        );
        let previous_terminal_shell_profile = (
            self.settings.terminal_shell_path.clone(),
            self.settings.terminal_shell_args.clone(),
        );
        let previous_blame_ignore_whitespace = self.settings.git_blame_ignore_whitespace;
        let previous_git_enabled = self.settings.git_enabled;
        let previous_git_autorefresh = self.settings.git_autorefresh;
        let previous_plugin_settings = self.settings.plugins.clone();
        let previous_git_ignored_repositories_key =
            git_string_list_setting_key(&self.settings.git_ignored_repositories);
        let previous_git_repository_scan_settings =
            GitRepositoryScanSettings::from_settings(&self.settings);
        let previous_diff_max_file_size_mb = self.settings.diff_max_file_size_mb;
        let previous_project_index_settings = (
            self.settings.project_index_max_files,
            self.settings.project_index_exclude_globs.clone(),
        );
        let previous_project_search_settings = (
            self.settings.project_search_exclude_globs.clone(),
            self.settings.project_search_max_file_size_mb,
            self.settings.project_search_max_results,
        );
        let path = settings_path(&self.workspace.root);
        if let Err(error) = next_settings.save(&path) {
            self.status = settings_save_failed_status(error);
            return;
        }

        note_app_write(&path);

        let lsp_server_configs_changed =
            previous_settings.lsp_server_configs() != next_settings.lsp_server_configs();
        self.settings = next_settings;
        let terminal_shell_profile_changed = previous_terminal_shell_profile
            != (
                self.settings.terminal_shell_path.clone(),
                self.settings.terminal_shell_args.clone(),
            );
        for buffer in &mut self.buffers {
            buffer.set_word_separators(self.settings.word_separators.clone());
        }
        sync_terminal_settings(&mut self.terminal, &self.settings);
        let restarted_terminal_sessions = if terminal_shell_profile_changed {
            self.terminal.restart_shell_sessions_for_profile_change()
        } else {
            0
        };
        let reopened_lsp_buffers = self.sync_lsp_server_settings_after_reload(&previous_settings);
        self.sync_settings_panel_inputs();

        let current_fonts = (
            self.settings.font_size,
            self.settings.ui_font_size,
            self.settings.editor_font_path.clone(),
            self.settings.ui_font_path.clone(),
        );
        if previous_fonts != current_fonts {
            self.fonts_dirty = true;
        }
        if previous_theme
            != (
                self.settings.theme.clone(),
                self.settings.active_custom_theme_path.clone(),
            )
        {
            self.theme_dirty = true;
            self.theme_picker_selected = self.selected_theme_picker_index();
        }
        if previous_read_only != self.settings.read_only {
            self.sync_global_read_only_buffers();
        }
        if previous_vim_settings != (self.settings.vim_keybindings, self.settings.vim.clone()) {
            self.vim_reset_session_state();
        }
        if previous_discord_settings != self.settings.discord {
            self.sync_discord_presence_runtime();
        }
        if previous_inline_annotations.0 && !self.settings.code_lens {
            self.code_lenses.clear();
        }
        if previous_inline_annotations.1 && !self.settings.inlay_hints {
            self.inlay_hints.clear();
        }
        if (!previous_inline_annotations.0 && self.settings.code_lens)
            || (!previous_inline_annotations.1 && self.settings.inlay_hints)
        {
            self.schedule_lsp_symbol_refreshes_for_open_buffers();
        }
        if previous_navigation_annotations.0 && !self.settings.hover_enabled {
            self.pending_lsp_hover = None;
            self.lsp_hover_request = None;
            self.lsp_hover = None;
        }
        if previous_navigation_annotations.1 && !self.settings.document_highlights_enabled {
            self.document_highlights_path = None;
            self.document_highlights.clear();
        }
        if previous_parameter_hints && !self.settings.parameter_hints_enabled {
            self.pending_signature_help_requests.clear();
            self.signature_help = None;
        }
        if previous_source_control_defaults
            != (
                self.settings.scm_default_view_mode,
                self.settings.scm_default_view_sort_key,
            )
        {
            self.source_control_view =
                source_control_view_mode_from_setting(self.settings.scm_default_view_mode);
            self.source_control_sort =
                source_control_sort_mode_from_setting(self.settings.scm_default_view_sort_key);
            self.source_control_selected = 0;
        }
        if previous_blame_ignore_whitespace != self.settings.git_blame_ignore_whitespace {
            self.sync_source_control_blame_settings();
        }
        let git_ignored_repositories_key =
            git_string_list_setting_key(&self.settings.git_ignored_repositories);
        let git_ignored_repositories_changed =
            previous_git_ignored_repositories_key != git_ignored_repositories_key;
        let git_repository_scan_settings_changed = previous_git_repository_scan_settings
            != GitRepositoryScanSettings::from_settings(&self.settings);
        let project_index_settings_changed = previous_project_index_settings
            != (
                self.settings.project_index_max_files,
                self.settings.project_index_exclude_globs.clone(),
            );
        let project_search_settings_changed = previous_project_search_settings
            != (
                self.settings.project_search_exclude_globs.clone(),
                self.settings.project_search_max_file_size_mb,
                self.settings.project_search_max_results,
            );
        if project_search_settings_changed {
            self.invalidate_project_search_requests();
            self.project_search_metadata_cache.clear();
            self.sync_project_search_after_settings_change();
        }
        if project_index_settings_changed {
            self.spawn_index();
        }
        if previous_diff_max_file_size_mb != self.settings.diff_max_file_size_mb
            || previous_git_enabled != self.settings.git_enabled
            || git_ignored_repositories_changed
            || git_repository_scan_settings_changed
        {
            self.invalidate_virtual_source_control_open_requests();
        }
        if previous_git_enabled != self.settings.git_enabled {
            self.sync_git_enabled_state();
        } else if previous_git_autorefresh != self.settings.git_autorefresh {
            self.sync_git_autorefresh_state(previous_git_autorefresh);
        } else if git_ignored_repositories_changed {
            self.sync_git_repository_filters_state();
        } else if git_repository_scan_settings_changed {
            self.spawn_git_scan();
        }
        if previous_plugin_settings != self.settings.plugins {
            self.sync_plugin_settings_state();
        }

        let app_state_vim_changed =
            previous_vim_settings != (self.settings.vim_keybindings, self.settings.vim.clone());
        let app_state_appearance_changed = previous_app_state_appearance
            != (
                self.settings.theme.clone(),
                self.settings.custom_theme_paths.clone(),
                self.settings.active_custom_theme_path.clone(),
                self.settings.editor_font_path.clone(),
                self.settings.ui_font_path.clone(),
            );
        let app_state_save_error = if app_state_vim_changed || app_state_appearance_changed {
            self.app_state_vim_keybindings = self.settings.vim_keybindings;
            self.app_state_vim = self.settings.vim.clone();
            self.save_app_state().err()
        } else {
            None
        };
        self.status = settings_save_success_status(
            apply_note,
            terminal_shell_profile_changed,
            restarted_terminal_sessions,
            lsp_server_configs_changed,
            reopened_lsp_buffers,
        );
        if let Some(error) = app_state_save_error {
            push_app_preference_save_failed_status(&mut self.status, error);
        }
        self.sync_background_image(false);
    }
}

#[derive(Debug, PartialEq)]
struct GitRepositoryScanSettings {
    auto_repository_detection: kuroya_core::GitAutoRepositoryDetection,
    ignore_submodules: bool,
    repository_scan_ignored_folders: Vec<String>,
    open_repository_in_parent_folders: kuroya_core::GitOpenRepositoryInParentFolders,
    detect_submodules: bool,
    detect_submodules_limit: usize,
    repository_scan_max_depth: usize,
    detect_worktrees: bool,
    detect_worktrees_limit: usize,
    worktree_include_files: Vec<String>,
    similarity_threshold: usize,
}

impl GitRepositoryScanSettings {
    fn from_settings(settings: &kuroya_core::EditorSettings) -> Self {
        GitRepositoryScanSettings {
            auto_repository_detection: settings.git_auto_repository_detection,
            ignore_submodules: settings.git_ignore_submodules,
            repository_scan_ignored_folders: git_string_list_setting_key(
                &settings.git_repository_scan_ignored_folders,
            ),
            open_repository_in_parent_folders: settings.git_open_repository_in_parent_folders,
            detect_submodules: settings.git_detect_submodules,
            detect_submodules_limit: settings.git_detect_submodules_limit,
            repository_scan_max_depth: settings.git_repository_scan_max_depth,
            detect_worktrees: settings.git_detect_worktrees,
            detect_worktrees_limit: settings.git_detect_worktrees_limit,
            worktree_include_files: git_string_list_setting_key(
                &settings.git_worktree_include_files,
            ),
            similarity_threshold: settings.git_similarity_threshold,
        }
    }
}

fn git_string_list_setting_key(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn settings_save_success_status(
    apply_note: Option<&str>,
    terminal_shell_profile_changed: bool,
    restarted_terminal_sessions: usize,
    lsp_server_configs_changed: bool,
    reopened_lsp_buffers: usize,
) -> String {
    let mut status = "Saved settings".to_owned();
    if let Some(note) = apply_note {
        status.push_str("; ");
        status.push_str(note);
    }
    if restarted_terminal_sessions > 0 {
        status.push_str("; restarted ");
        status.push_str(&restarted_terminal_sessions.to_string());
        status.push_str(if restarted_terminal_sessions == 1 {
            " terminal with the selected provider"
        } else {
            " terminals with the selected provider"
        });
    } else if terminal_shell_profile_changed {
        status.push_str("; new terminals use the selected provider");
    }
    if lsp_server_configs_changed {
        status.push_str("; LSP servers updated");
        if reopened_lsp_buffers > 0 {
            status.push_str("; reopened ");
            status.push_str(&reopened_lsp_buffers.to_string());
            status.push_str(if reopened_lsp_buffers == 1 {
                " LSP buffer"
            } else {
                " LSP buffers"
            });
        }
    } else if reopened_lsp_buffers > 0 {
        status.push_str("; retried ");
        status.push_str(&reopened_lsp_buffers.to_string());
        status.push_str(if reopened_lsp_buffers == 1 {
            " LSP buffer"
        } else {
            " LSP buffers"
        });
    }
    status
}

fn settings_save_failed_status(error: impl Display) -> String {
    let error = error.to_string();
    let error = display_error_label_cow(&error);
    format!("Could not save settings: {}", error.as_ref())
}

fn push_app_preference_save_failed_status(status: &mut String, error: impl Display) {
    let error = error.to_string();
    let error = display_error_label_cow(&error);
    status.push_str("; app preference save failed: ");
    status.push_str(error.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_startup_context::AppStartupContext, lsp_client::LspClientHandle,
        lsp_runtime::LSP_SYMBOL_REFRESH_DEBOUNCE, lsp_runtime::due_lsp_symbol_refresh_ids,
        path_display::DISPLAY_ERROR_LABEL_MAX_CHARS, terminal::TerminalPane,
        transient_state::LspSignatureHelpPopup, workspace_state::settings_path,
    };
    use image::{Rgba, RgbaImage};
    use kuroya_core::{
        EditorBackgroundImageFit, EditorBackgroundImagePosition, EditorBackgroundImageScope,
        EditorScrollbarVisibility, EditorSettings, LspServerConfig, LspSignatureHelp,
        PluginCapabilities, PluginContributions, PluginDescriptor, PluginManifest, TextBuffer,
        ThemeSettings, Workspace,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn apply_settings_panel_schedules_refresh_when_code_lens_is_reenabled() {
        let root = temp_root("code-lens-reenabled");
        let settings = EditorSettings {
            code_lens: false,
            inlay_hints: false,
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src").join("main.rs")),
            "fn main() {}\n".to_owned(),
        ));

        app.settings_panel_draft.code_lens = true;
        app.settings_panel_draft.inlay_hints = false;
        app.apply_settings_panel();

        assert_due_refresh_ids(&app, &[7]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_schedules_refresh_when_inlay_hints_are_reenabled() {
        let root = temp_root("inlay-hints-reenabled");
        let settings = EditorSettings {
            code_lens: false,
            inlay_hints: false,
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src").join("main.rs")),
            "fn main() {}\n".to_owned(),
        ));

        app.settings_panel_draft.code_lens = false;
        app.settings_panel_draft.inlay_hints = true;
        app.apply_settings_panel();

        assert_due_refresh_ids(&app, &[7]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_normalizes_invalid_numeric_draft_without_saving_when_unchanged() {
        let root = temp_root("invalid-numeric-draft");
        let mut app = app_for_test(root.clone(), EditorSettings::default());

        app.settings_panel_draft.font_size = f32::NAN;
        app.settings_panel_draft.terminal_line_height = f32::INFINITY;
        app.apply_settings_panel();

        assert!(app.settings.font_size.is_finite());
        assert!(app.settings.terminal_line_height.is_finite());
        assert_eq!(app.settings_panel_draft.font_size, app.settings.font_size);
        assert_eq!(
            app.settings_panel_draft.terminal_line_height,
            app.settings.terminal_line_height
        );
        assert!(app.status.contains("normalized invalid draft values"));
        assert!(!settings_path(&root).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_rejects_closed_panel_draft() {
        let root = temp_root("closed-panel-draft");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_open = false;
        app.settings_panel_draft.font_size = 18.0;

        app.apply_settings_panel();

        assert_eq!(app.settings.font_size, EditorSettings::default().font_size);
        assert_eq!(app.status, "Open settings before applying changes");
        assert!(!settings_path(&root).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_records_settings_save_as_recent_app_write() {
        let root = temp_root("apply-notes-settings-write");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_draft.font_size = 22.0;

        app.apply_settings_panel();

        assert!(app.status.starts_with("Saved settings"));
        assert!(crate::fs_watcher::is_recent_app_write(&settings_path(
            &root
        )));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_does_not_apply_or_save_vim_app_state_when_settings_save_fails() {
        let root = temp_root("vim-app-state-after-settings-save-fail");
        fs::create_dir_all(&root).unwrap();
        let settings_path = settings_path(&root);
        fs::create_dir_all(settings_path.parent().unwrap().parent().unwrap()).unwrap();
        fs::write(settings_path.parent().unwrap(), "not a settings directory").unwrap();
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        let app_state_path = root.join("app-state.json");
        app.app_state_path_override = Some(app_state_path.clone());

        app.settings_panel_draft.vim_keybindings = true;
        app.apply_settings_panel();

        assert!(!app.settings.vim_keybindings);
        assert!(app.settings_panel_draft.vim_keybindings);
        assert!(app.status.starts_with("Could not save settings:"));
        assert!(!app_state_path.exists());
        app.save_app_state().unwrap();
        let app_state = fs::read_to_string(&app_state_path).unwrap();
        assert!(app_state.contains("\"vim_keybindings\": false"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_reports_vim_app_state_save_failure_after_settings_save() {
        let root = temp_root("vim-app-state-save-fail-after-settings-save");
        fs::create_dir_all(&root).unwrap();
        let blocked_parent = root.join("blocked-app-state");
        fs::write(&blocked_parent, "not a directory").unwrap();
        let app_state_path = blocked_parent.join("state.json");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.app_state_path_override = Some(app_state_path.clone());

        app.settings_panel_draft.vim_keybindings = true;
        app.apply_settings_panel();

        assert!(app.settings.vim_keybindings);
        assert!(app.app_state_vim_keybindings);
        let saved = fs::read_to_string(settings_path(&root)).unwrap();
        assert!(saved.contains("vim_keybindings = true"));
        assert!(
            app.status
                .starts_with("Saved settings; app preference save failed:")
        );
        assert!(!app_state_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_persists_custom_vim_settings_to_settings_and_app_state() {
        let root = temp_root("vim-custom-settings-persist");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        let app_state_path = root.join("app-state.json");
        app.app_state_path_override = Some(app_state_path.clone());
        app.settings_panel_draft.vim_keybindings = true;
        app.settings_panel_draft.vim.disabled_bindings = vec!["Q".to_owned()];
        app.settings_panel_draft.vim.key_overrides = vec![kuroya_core::EditorVimKeyOverride {
            before: "K".to_owned(),
            after: "0".to_owned(),
            command: None,
        }];

        app.apply_settings_panel();

        assert!(app.settings.vim_keybindings);
        assert_eq!(app.settings.vim.disabled_bindings, ["Q"]);
        assert_eq!(app.app_state_vim_keybindings, app.settings.vim_keybindings);
        assert_eq!(app.app_state_vim, app.settings.vim);
        let saved = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
            .unwrap()
            .settings;
        assert_eq!(saved.vim_keybindings, app.settings.vim_keybindings);
        assert_eq!(saved.vim, app.settings.vim);
        let app_state: crate::persistence::AppState =
            serde_json::from_str(&fs::read_to_string(app_state_path).unwrap()).unwrap();
        assert_eq!(
            app_state.vim_keybindings,
            Some(app.settings.vim_keybindings)
        );
        assert_eq!(app_state.vim, Some(app.settings.vim.clone()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_persists_appearance_settings_to_app_state() {
        let root = temp_root("appearance-app-state-persist");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        let app_state_path = root.join("app-state.json");
        let theme_path = root.join("themes").join("panel.toml");
        let editor_font_path = root.join("fonts").join("editor.ttf");
        let ui_font_path = root.join("fonts").join("ui.ttf");
        let theme_path = theme_path.display().to_string();
        let editor_font_path = editor_font_path.display().to_string();
        let ui_font_path = ui_font_path.display().to_string();
        app.app_state_path_override = Some(app_state_path.clone());
        app.settings_panel_draft.theme = ThemeSettings {
            name: "Panel Theme".to_owned(),
            accent: [10, 20, 30],
            ..ThemeSettings::default()
        };
        app.settings_panel_draft.custom_theme_paths =
            vec![theme_path.clone(), "themes/relative.toml".to_owned()];
        app.settings_panel_draft.active_custom_theme_path = Some(theme_path.clone());
        app.settings_editor_font_path = editor_font_path.clone();
        app.settings_ui_font_path = ui_font_path.clone();

        app.apply_settings_panel();

        let app_state: crate::persistence::AppState =
            serde_json::from_str(&fs::read_to_string(app_state_path).unwrap()).unwrap();
        assert_eq!(app_state.theme, Some(app.settings.theme.clone()));
        assert_eq!(app_state.custom_theme_paths, vec![theme_path.clone()]);
        assert_eq!(
            app_state.active_custom_theme_path.as_deref(),
            Some(theme_path.as_str())
        );
        assert_eq!(
            app_state.editor_font_path.as_deref(),
            Some(editor_font_path.as_str())
        );
        assert_eq!(
            app_state.ui_font_path.as_deref(),
            Some(ui_font_path.as_str())
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_persists_scrollbar_visibility_settings() {
        let root = temp_root("scrollbar-visibility-persist");
        let mut app = app_for_test(root.clone(), EditorSettings::default());

        app.settings_panel_draft.scrollbar_vertical = EditorScrollbarVisibility::Visible;
        app.settings_panel_draft.scrollbar_horizontal = EditorScrollbarVisibility::Auto;
        app.settings_panel_draft.explorer_scrollbar = EditorScrollbarVisibility::Visible;

        app.apply_settings_panel();

        assert_eq!(
            app.settings.scrollbar_vertical,
            EditorScrollbarVisibility::Visible
        );
        assert_eq!(
            app.settings.scrollbar_horizontal,
            EditorScrollbarVisibility::Auto
        );
        assert_eq!(
            app.settings.explorer_scrollbar,
            EditorScrollbarVisibility::Visible
        );
        assert_eq!(app.settings_panel_draft, app.settings);

        let saved = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
            .unwrap()
            .settings;
        assert_eq!(saved.scrollbar_vertical, EditorScrollbarVisibility::Visible);
        assert_eq!(saved.scrollbar_horizontal, EditorScrollbarVisibility::Auto);
        assert_eq!(saved.explorer_scrollbar, EditorScrollbarVisibility::Visible);
        assert!(app.status.starts_with("Saved settings"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_general_settings_survive_untrusted_workspace_reload() {
        let root = temp_root("general-settings-untrusted-reload");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.trusted_workspaces.clear();
        app.workspace_trusted = false;
        app.settings_panel_draft.window_zoom_level = 1.5;
        app.settings_panel_draft.ui_font_size = 16.0;
        app.settings_panel_draft.minimap = true;
        app.settings_panel_draft.smooth_scrolling = false;
        app.settings_panel_draft.scroll_beyond_last_line = false;
        app.settings_panel_draft.status_bar_visible = false;

        app.apply_settings_panel();

        assert!(app.status.starts_with("Saved settings"));
        app.reload_settings();

        assert!(!app.workspace_trusted);
        assert_eq!(app.settings.window_zoom_level, 1.5);
        assert_eq!(app.settings.ui_font_size, 16.0);
        assert!(app.settings.minimap);
        assert!(!app.settings.smooth_scrolling);
        assert!(!app.settings.scroll_beyond_last_line);
        assert!(!app.settings.status_bar_visible);
        assert_eq!(app.settings_panel_draft, app.settings);

        let saved = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
            .unwrap()
            .settings;
        assert_eq!(saved.window_zoom_level, 1.5);
        assert_eq!(saved.ui_font_size, 16.0);
        assert!(saved.minimap);
        assert!(!saved.smooth_scrolling);
        assert!(!saved.scroll_beyond_last_line);
        assert!(!saved.status_bar_visible);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_and_reload_manage_persisted_editor_background_image() {
        let root = temp_root("editor-background-image");
        fs::create_dir_all(&root).unwrap();
        let image_path = root.join("background.png");
        RgbaImage::from_pixel(4, 3, Rgba([20, 80, 160, 255]))
            .save(&image_path)
            .unwrap();
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.settings_panel_draft.background_image_enabled = true;
        app.settings_panel_draft.background_image_path = Some(image_path.display().to_string());
        app.settings_panel_draft.background_image_dim = 0.72;
        app.settings_panel_draft.background_image_fit = EditorBackgroundImageFit::Contain;
        app.settings_panel_draft.background_image_position = EditorBackgroundImagePosition::Bottom;
        app.settings_panel_draft.background_image_scope = EditorBackgroundImageScope::FullApp;

        app.apply_settings_panel();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !app.background_image_is_ready() && Instant::now() < deadline {
            app.handle_events();
            std::thread::sleep(Duration::from_millis(10));
        }
        app.handle_events();

        assert!(app.background_image_is_ready());
        assert_eq!(app.settings.background_image_dim, 0.72);
        assert_eq!(
            app.settings.background_image_fit,
            EditorBackgroundImageFit::Contain
        );
        assert_eq!(
            app.settings.background_image_position,
            EditorBackgroundImagePosition::Bottom
        );
        assert_eq!(
            app.settings.background_image_scope,
            EditorBackgroundImageScope::FullApp
        );
        let mut saved = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
            .unwrap()
            .settings;
        assert!(saved.background_image_enabled);
        assert_eq!(
            saved.background_image_path.as_deref(),
            Some(image_path.to_string_lossy().as_ref())
        );

        saved.background_image_enabled = false;
        saved.save(&settings_path(&root)).unwrap();
        app.reload_settings();

        assert!(!app.settings.background_image_enabled);
        assert!(!app.background_image_is_ready());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_restarts_lsp_clients_when_server_config_changes() {
        let root = temp_root("lsp-config-change");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.lsp_clients
            .insert("rust".to_owned(), LspClientHandle::accepting_for_test());
        app.lsp_unavailable.insert("rust".to_owned());
        app.lsp_restart_attempts.insert("rust".to_owned(), 2);
        app.pending_lsp_restarts
            .insert("rust".to_owned(), Instant::now());
        app.settings_panel_draft.lsp_servers = vec![LspServerConfig {
            language: "rust".to_owned(),
            command: "rust-analyzer-custom".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: vec!["Cargo.toml".to_owned()],
            enabled: true,
        }];

        app.apply_settings_panel();

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
        let saved = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
            .unwrap()
            .settings;
        assert_eq!(saved.lsp_servers, app.settings.lsp_servers);
        assert!(app.status.contains("LSP servers updated"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_clears_signature_help_when_parameter_hints_are_disabled() {
        let root = temp_root("parameter-hints-disabled");
        let path = root.join("src").join("main.rs");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        app.pending_signature_help_requests
            .insert(7, Instant::now());
        app.signature_help = Some(signature_popup(7, path));

        app.settings_panel_draft.parameter_hints_enabled = false;
        app.apply_settings_panel();

        assert!(!app.settings.parameter_hints_enabled);
        assert!(app.pending_signature_help_requests.is_empty());
        assert!(app.signature_help.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_does_not_scan_for_whitespace_only_scan_setting_changes() {
        let root = temp_root("scan-whitespace");
        let settings = EditorSettings {
            git_enabled: true,
            git_repository_scan_ignored_folders: vec!["node_modules".to_owned()],
            git_worktree_include_files: vec!["packages/app".to_owned()],
            ..EditorSettings::default()
        };
        let mut app = app_for_test(root.clone(), settings);

        app.settings_panel_draft.status_bar_visible = !app.settings.status_bar_visible;
        app.settings_panel_draft.git_repository_scan_ignored_folders =
            vec![" node_modules ".to_owned()];
        app.settings_panel_draft.git_worktree_include_files = vec![" packages/app ".to_owned()];

        app.apply_settings_panel();

        assert_eq!(
            app.settings.git_repository_scan_ignored_folders,
            [" node_modules ".to_owned()]
        );
        assert_eq!(
            app.settings.git_worktree_include_files,
            [" packages/app ".to_owned()]
        );
        assert_eq!(app.git_scan_active_request_id, 0);
        assert_eq!(app.git_scan_in_flight_request_id, None);
        assert!(app.active_async_tasks.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn settings_save_success_status_mentions_terminal_provider_change() {
        let status = settings_save_success_status(None, true, 0, false, 0);

        assert!(status.contains("new terminals use the selected provider"));
    }

    #[test]
    fn settings_save_success_status_mentions_restarted_provider_sessions() {
        let status = settings_save_success_status(None, true, 2, false, 0);

        assert!(status.contains("restarted 2 terminals with the selected provider"));
        assert!(!status.contains("new terminals use the selected provider"));
    }

    #[test]
    fn settings_save_success_status_mentions_lsp_updates() {
        let status = settings_save_success_status(None, false, 0, true, 2);

        assert!(status.contains("LSP servers updated"));
        assert!(status.contains("reopened 2 LSP buffers"));
    }

    #[test]
    fn settings_save_failed_status_sanitizes_and_bounds_error() {
        let error = format!(
            "first line\nsecond line \u{202e}{}",
            "x".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
        );

        let status = settings_save_failed_status(&error);

        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Could not save settings: ".chars().count() + DISPLAY_ERROR_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn apply_settings_panel_persists_plugin_settings_and_resyncs_discovery_state() {
        let root = temp_root("plugin-settings-persist");
        let mut app = app_for_test(root.clone(), EditorSettings::default());
        let app_state_path = root.join("app-state.json");
        app.app_state_path_override = Some(app_state_path.clone());
        app.plugins.push(test_plugin_descriptor("loaded.plugin"));

        app.settings_panel_draft.plugins.enabled = false;
        app.apply_settings_panel();

        assert!(!app.settings.plugins.enabled);
        assert!(app.plugins.is_empty());
        assert_eq!(app.workspace_plugins_in_flight_request_id, None);
        let saved = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
            .unwrap()
            .settings;
        assert!(!saved.plugins.enabled);
        assert!(app.status.starts_with("Saved settings"));

        let next_request_id = app.workspace_plugins_next_request_id;
        app.settings_panel_draft.plugins.enabled = true;
        app.apply_settings_panel();

        assert!(app.settings.plugins.enabled);
        assert_eq!(
            app.workspace_plugins_in_flight_request_id,
            Some(next_request_id + 1)
        );
        let saved = EditorSettings::load_or_create_with_recovery(&settings_path(&root))
            .unwrap()
            .settings;
        assert!(saved.plugins.enabled);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn apply_settings_panel_updates_plugin_disabled_ids_from_draft() {
        let root = temp_root("plugin-disabled-ids");
        let mut app = app_for_test(root.clone(), EditorSettings::default());

        app.settings_panel_draft.plugins.disabled_ids = vec!["alpha.plugin".to_owned()];
        app.apply_settings_panel();

        assert_eq!(
            app.settings.plugins.disabled_ids,
            ["alpha.plugin".to_owned()]
        );
        assert_eq!(
            EditorSettings::load_or_create_with_recovery(&settings_path(&root))
                .unwrap()
                .settings
                .plugins
                .disabled_ids,
            ["alpha.plugin".to_owned()]
        );
        assert_eq!(app.settings_panel_draft, app.settings);

        app.sync_settings_panel_inputs();
        app.settings_panel_draft.plugins.disabled_ids.clear();
        app.apply_settings_panel();

        assert!(app.settings.plugins.disabled_ids.is_empty());
        assert_eq!(app.settings_panel_draft, app.settings);
        let _ = fs::remove_dir_all(root);
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

    fn assert_due_refresh_ids(app: &KuroyaApp, expected: &[u64]) {
        assert_eq!(
            due_lsp_symbol_refresh_ids(
                &app.pending_lsp_symbol_refreshes,
                Instant::now(),
                LSP_SYMBOL_REFRESH_DEBOUNCE,
            ),
            expected
        );
    }

    fn signature_popup(id: u64, path: PathBuf) -> LspSignatureHelpPopup {
        LspSignatureHelpPopup {
            id,
            path,
            line: 1,
            column: 1,
            help: LspSignatureHelp {
                signatures: Vec::new(),
                active_signature: 0,
                active_parameter: None,
            },
        }
    }

    fn app_for_test(root: PathBuf, settings: EditorSettings) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
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
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        });
        app.settings_panel_open = true;
        app
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-apply-settings-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}

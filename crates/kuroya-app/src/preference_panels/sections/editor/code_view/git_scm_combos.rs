use crate::preference_panels::sections::{
    SETTINGS_TEXT_INPUT_MAX_CHARS, bounded_settings_multiline_input,
    bounded_settings_multiline_join, bounded_settings_singleline_input,
    bounded_settings_text_edit_width,
};
use crate::ui_switch::ui_switch;
use eframe::egui;
use kuroya_core::{
    EditorSettings, GitAddAiCoAuthor, GitAutoFetch, GitAutoRepositoryDetection,
    GitBranchProtectionPrompt, GitBranchSortOrder, GitCheckoutType, GitCountBadge,
    GitInputValidationSubjectLength, GitOpenAfterClone, GitOpenRepositoryInParentFolders,
    GitPostCommitCommand, GitPromptToSaveFilesBeforeCommit, GitSmartCommitChanges, GitTimelineDate,
    GitUntrackedChanges, MAX_GIT_INPUT_VALIDATION_LENGTH, MIN_GIT_INPUT_VALIDATION_LENGTH,
    ScmCountBadge, ScmDefaultViewMode, ScmDefaultViewSortKey, ScmDiffDecorations,
    ScmDiffDecorationsGutterAction, ScmDiffDecorationsGutterVisibility,
    ScmDiffDecorationsIgnoreTrimWhitespace, ScmGraphBadges, ScmProviderCountBadge,
};

pub(super) fn scm_default_view_mode_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmDefaultViewMode,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_default_view_mode_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmDefaultViewMode::List, "List");
            ui.selectable_value(value, ScmDefaultViewMode::Tree, "Tree");
        });
}

fn scm_default_view_mode_label(mode: ScmDefaultViewMode) -> &'static str {
    match mode {
        ScmDefaultViewMode::List => "List",
        ScmDefaultViewMode::Tree => "Tree",
    }
}

pub(super) fn scm_default_view_sort_key_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmDefaultViewSortKey,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_default_view_sort_key_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmDefaultViewSortKey::Path, "Path");
            ui.selectable_value(value, ScmDefaultViewSortKey::Name, "Name");
            ui.selectable_value(value, ScmDefaultViewSortKey::Status, "Status");
        });
}

fn scm_default_view_sort_key_label(mode: ScmDefaultViewSortKey) -> &'static str {
    match mode {
        ScmDefaultViewSortKey::Path => "Path",
        ScmDefaultViewSortKey::Name => "Name",
        ScmDefaultViewSortKey::Status => "Status",
    }
}

pub(super) fn scm_count_badge_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmCountBadge,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_count_badge_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmCountBadge::All, "All");
            ui.selectable_value(value, ScmCountBadge::Focused, "Focused");
            ui.selectable_value(value, ScmCountBadge::Off, "Off");
        });
}

fn scm_count_badge_label(mode: ScmCountBadge) -> &'static str {
    match mode {
        ScmCountBadge::All => "All",
        ScmCountBadge::Focused => "Focused",
        ScmCountBadge::Off => "Off",
    }
}

pub(super) fn scm_provider_count_badge_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmProviderCountBadge,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_provider_count_badge_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmProviderCountBadge::Hidden, "Hidden");
            ui.selectable_value(value, ScmProviderCountBadge::Auto, "Auto");
            ui.selectable_value(value, ScmProviderCountBadge::Visible, "Visible");
        });
}

fn scm_provider_count_badge_label(mode: ScmProviderCountBadge) -> &'static str {
    match mode {
        ScmProviderCountBadge::Hidden => "Hidden",
        ScmProviderCountBadge::Auto => "Auto",
        ScmProviderCountBadge::Visible => "Visible",
    }
}

pub(super) fn git_count_badge_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitCountBadge,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_count_badge_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitCountBadge::All, "All");
            ui.selectable_value(value, GitCountBadge::Tracked, "Tracked");
            ui.selectable_value(value, GitCountBadge::Off, "Off");
        });
}

pub(super) fn render_git_checkout_type(ui: &mut egui::Ui, draft: &mut EditorSettings) {
    ui.horizontal(|ui| {
        for (kind, label) in [
            (GitCheckoutType::Local, "Local"),
            (GitCheckoutType::Remote, "Remote"),
            (GitCheckoutType::Tags, "Tags"),
        ] {
            let mut enabled = draft.git_checkout_type.contains(&kind);
            if ui
                .horizontal(|ui| {
                    let r = ui_switch(ui, &mut enabled);
                    ui.label(egui::RichText::new(label).small().weak());
                    r
                })
                .inner
                .changed()
            {
                if enabled {
                    draft.git_checkout_type.push(kind);
                } else {
                    draft.git_checkout_type.retain(|entry| *entry != kind);
                }
            }
        }
    })
    .response
    .on_hover_text("Git refs shown by Checkout to");
}

fn git_count_badge_label(mode: GitCountBadge) -> &'static str {
    match mode {
        GitCountBadge::All => "All",
        GitCountBadge::Tracked => "Tracked",
        GitCountBadge::Off => "Off",
    }
}

pub(super) fn git_untracked_changes_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitUntrackedChanges,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_untracked_changes_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitUntrackedChanges::Mixed, "Mixed");
            ui.selectable_value(value, GitUntrackedChanges::Separate, "Separate");
            ui.selectable_value(value, GitUntrackedChanges::Hidden, "Hidden");
        });
}

fn git_untracked_changes_label(mode: GitUntrackedChanges) -> &'static str {
    match mode {
        GitUntrackedChanges::Mixed => "Mixed",
        GitUntrackedChanges::Separate => "Separate",
        GitUntrackedChanges::Hidden => "Hidden",
    }
}

pub(super) fn git_auto_repository_detection_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitAutoRepositoryDetection,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_auto_repository_detection_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitAutoRepositoryDetection::True, "On");
            ui.selectable_value(value, GitAutoRepositoryDetection::False, "Off");
            ui.selectable_value(value, GitAutoRepositoryDetection::SubFolders, "Subfolders");
            ui.selectable_value(
                value,
                GitAutoRepositoryDetection::OpenEditors,
                "Open editors",
            );
        })
        .response
        .on_hover_text("Controls which folders can be detected as Git repositories");
}

fn git_auto_repository_detection_label(mode: GitAutoRepositoryDetection) -> &'static str {
    match mode {
        GitAutoRepositoryDetection::True => "On",
        GitAutoRepositoryDetection::False => "Off",
        GitAutoRepositoryDetection::SubFolders => "Subfolders",
        GitAutoRepositoryDetection::OpenEditors => "Open editors",
    }
}

pub(super) fn git_add_ai_co_author_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitAddAiCoAuthor,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_add_ai_co_author_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitAddAiCoAuthor::Off, "Off");
            ui.selectable_value(value, GitAddAiCoAuthor::ChatAndAgent, "Chat and agent");
            ui.selectable_value(value, GitAddAiCoAuthor::All, "All");
        });
}

fn git_add_ai_co_author_label(mode: GitAddAiCoAuthor) -> &'static str {
    match mode {
        GitAddAiCoAuthor::Off => "Off",
        GitAddAiCoAuthor::ChatAndAgent => "Chat and agent",
        GitAddAiCoAuthor::All => "All",
    }
}

pub(super) fn git_autofetch_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitAutoFetch,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_autofetch_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitAutoFetch::False, "Off");
            ui.selectable_value(value, GitAutoFetch::True, "Current repository");
            ui.selectable_value(value, GitAutoFetch::All, "All repositories");
        });
}

fn git_autofetch_label(mode: GitAutoFetch) -> &'static str {
    match mode {
        GitAutoFetch::True => "Current repository",
        GitAutoFetch::False => "Off",
        GitAutoFetch::All => "All repositories",
    }
}

pub(super) fn git_open_repository_in_parent_folders_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitOpenRepositoryInParentFolders,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_open_repository_in_parent_folders_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitOpenRepositoryInParentFolders::Prompt, "Prompt");
            ui.selectable_value(value, GitOpenRepositoryInParentFolders::Always, "Always");
            ui.selectable_value(value, GitOpenRepositoryInParentFolders::Never, "Never");
        })
        .response
        .on_hover_text(
            "Controls whether Source Control can use a Git repository above the workspace folder",
        );
}

fn git_open_repository_in_parent_folders_label(
    mode: GitOpenRepositoryInParentFolders,
) -> &'static str {
    match mode {
        GitOpenRepositoryInParentFolders::Always => "Always",
        GitOpenRepositoryInParentFolders::Never => "Never",
        GitOpenRepositoryInParentFolders::Prompt => "Prompt",
    }
}

pub(super) fn git_open_after_clone_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitOpenAfterClone,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_open_after_clone_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitOpenAfterClone::Prompt, "Prompt");
            ui.selectable_value(value, GitOpenAfterClone::Always, "Always");
            ui.selectable_value(value, GitOpenAfterClone::AlwaysNewWindow, "New window");
            ui.selectable_value(
                value,
                GitOpenAfterClone::WhenNoFolderOpen,
                "When no folder open",
            );
        });
}

fn git_open_after_clone_label(mode: GitOpenAfterClone) -> &'static str {
    match mode {
        GitOpenAfterClone::Always => "Always",
        GitOpenAfterClone::AlwaysNewWindow => "New window",
        GitOpenAfterClone::WhenNoFolderOpen => "When no folder open",
        GitOpenAfterClone::Prompt => "Prompt",
    }
}

pub(super) fn git_post_commit_command_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitPostCommitCommand,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_post_commit_command_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitPostCommitCommand::None, "None");
            ui.selectable_value(value, GitPostCommitCommand::Push, "Push");
            ui.selectable_value(value, GitPostCommitCommand::Sync, "Sync");
        });
}

fn git_post_commit_command_label(mode: GitPostCommitCommand) -> &'static str {
    match mode {
        GitPostCommitCommand::None => "None",
        GitPostCommitCommand::Push => "Push",
        GitPostCommitCommand::Sync => "Sync",
    }
}

pub(super) fn git_branch_sort_order_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitBranchSortOrder,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_branch_sort_order_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitBranchSortOrder::CommitterDate, "Committer date");
            ui.selectable_value(value, GitBranchSortOrder::Alphabetically, "Alphabetically");
        });
}

fn git_branch_sort_order_label(mode: GitBranchSortOrder) -> &'static str {
    match mode {
        GitBranchSortOrder::CommitterDate => "Committer date",
        GitBranchSortOrder::Alphabetically => "Alphabetically",
    }
}

pub(super) fn render_string_list_input(
    ui: &mut egui::Ui,
    values: &mut Vec<String>,
    hint_text: &'static str,
    rows: usize,
) {
    let mut value = bounded_settings_multiline_join(values.iter().map(String::as_str));
    let response = ui.add_sized(
        [
            bounded_settings_text_edit_width(ui.available_width(), 420.0),
            (rows as f32 * 24.0).max(48.0),
        ],
        egui::TextEdit::multiline(&mut value)
            .desired_rows(rows)
            .hint_text(hint_text),
    );
    if response.changed() {
        *values = parse_string_list_input_value(&value);
    }
}

pub(super) fn parse_string_list_input_value(value: &str) -> Vec<String> {
    value.lines().map(ToOwned::to_owned).collect()
}

pub(super) fn render_string_map_input(
    ui: &mut egui::Ui,
    values: &mut std::collections::BTreeMap<String, String>,
    hint_text: &'static str,
    rows: usize,
) {
    let mut value = String::new();
    for (index, (key, severity)) in values.iter().enumerate() {
        if index > 0 {
            value.push('\n');
        }
        value.push_str(&bounded_settings_singleline_input(key));
        value.push('=');
        value.push_str(&bounded_settings_singleline_input(severity));
        if value.chars().count() >= SETTINGS_TEXT_INPUT_MAX_CHARS {
            break;
        }
    }
    value = bounded_settings_multiline_input(&value);
    let response = ui.add_sized(
        [
            bounded_settings_text_edit_width(ui.available_width(), 420.0),
            (rows as f32 * 24.0).max(48.0),
        ],
        egui::TextEdit::multiline(&mut value)
            .desired_rows(rows)
            .hint_text(hint_text),
    );
    if response.changed() {
        *values = value
            .lines()
            .filter_map(|line| {
                let (key, severity) = line.split_once('=')?;
                let key = key.trim();
                let severity = severity.trim();
                (!key.is_empty() && !severity.is_empty())
                    .then(|| (key.to_owned(), severity.to_owned()))
            })
            .collect();
    }
}

pub(super) fn render_optional_string_input(
    ui: &mut egui::Ui,
    value: &mut Option<String>,
    hint_text: &'static str,
) {
    let mut text = bounded_settings_singleline_input(value.as_deref().unwrap_or_default());
    if ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(hint_text)
                .desired_width(bounded_settings_text_edit_width(
                    ui.available_width(),
                    420.0,
                ))
                .clip_text(true),
        )
        .changed()
    {
        *value = if text.is_empty() { None } else { Some(text) };
    }
}

pub(super) fn render_git_branch_protection_input(ui: &mut egui::Ui, draft: &mut EditorSettings) {
    render_string_list_input(ui, &mut draft.git_branch_protection, "main\nrelease/*", 3);
}

pub(super) fn render_git_ignored_repositories_input(ui: &mut egui::Ui, draft: &mut EditorSettings) {
    render_string_list_input(
        ui,
        &mut draft.git_ignored_repositories,
        "C:/repo/ignored\n../other",
        3,
    );
}

pub(super) fn render_git_repository_scan_ignored_folders_input(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
) {
    render_string_list_input(
        ui,
        &mut draft.git_repository_scan_ignored_folders,
        "node_modules\ndist",
        3,
    );
}

pub(super) fn git_branch_protection_prompt_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitBranchProtectionPrompt,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_branch_protection_prompt_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(
                value,
                GitBranchProtectionPrompt::AlwaysPrompt,
                "Always prompt",
            );
            ui.selectable_value(
                value,
                GitBranchProtectionPrompt::AlwaysCommit,
                "Always commit",
            );
            ui.selectable_value(
                value,
                GitBranchProtectionPrompt::AlwaysCommitToNewBranch,
                "Commit to new branch",
            );
        });
}

fn git_branch_protection_prompt_label(mode: GitBranchProtectionPrompt) -> &'static str {
    match mode {
        GitBranchProtectionPrompt::AlwaysCommit => "Always commit",
        GitBranchProtectionPrompt::AlwaysCommitToNewBranch => "Commit to new branch",
        GitBranchProtectionPrompt::AlwaysPrompt => "Always prompt",
    }
}

pub(super) fn git_timeline_date_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitTimelineDate,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_timeline_date_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitTimelineDate::Committed, "Committed");
            ui.selectable_value(value, GitTimelineDate::Authored, "Authored");
        });
}

fn git_timeline_date_label(mode: GitTimelineDate) -> &'static str {
    match mode {
        GitTimelineDate::Committed => "Committed",
        GitTimelineDate::Authored => "Authored",
    }
}

pub(super) fn scm_graph_badges_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmGraphBadges,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_graph_badges_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmGraphBadges::All, "All");
            ui.selectable_value(value, ScmGraphBadges::Filter, "Filter");
        });
}

fn scm_graph_badges_label(mode: ScmGraphBadges) -> &'static str {
    match mode {
        ScmGraphBadges::All => "All",
        ScmGraphBadges::Filter => "Filter",
    }
}

pub(super) fn git_smart_commit_changes_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitSmartCommitChanges,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_smart_commit_changes_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitSmartCommitChanges::All, "All");
            ui.selectable_value(value, GitSmartCommitChanges::Tracked, "Tracked");
        });
}

fn git_smart_commit_changes_label(mode: GitSmartCommitChanges) -> &'static str {
    match mode {
        GitSmartCommitChanges::All => "All",
        GitSmartCommitChanges::Tracked => "Tracked",
    }
}

pub(super) fn git_prompt_to_save_files_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut GitPromptToSaveFilesBeforeCommit,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(git_prompt_to_save_files_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, GitPromptToSaveFilesBeforeCommit::Always, "Always");
            ui.selectable_value(value, GitPromptToSaveFilesBeforeCommit::Staged, "Staged");
            ui.selectable_value(value, GitPromptToSaveFilesBeforeCommit::Never, "Never");
        });
}

fn git_prompt_to_save_files_label(mode: GitPromptToSaveFilesBeforeCommit) -> &'static str {
    match mode {
        GitPromptToSaveFilesBeforeCommit::Always => "Always",
        GitPromptToSaveFilesBeforeCommit::Staged => "Staged",
        GitPromptToSaveFilesBeforeCommit::Never => "Never",
    }
}

pub(super) fn render_git_input_validation_subject_length(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
) {
    ui.horizontal(|ui| {
        let mut inherit =
            draft.git_input_validation_subject_length == GitInputValidationSubjectLength::Inherit;
        if ui
            .horizontal(|ui| {
                let r = ui_switch(ui, &mut inherit);
                ui.label(egui::RichText::new("Inherit").small().weak());
                r
            })
            .inner
            .changed()
        {
            draft.git_input_validation_subject_length = if inherit {
                GitInputValidationSubjectLength::Inherit
            } else {
                GitInputValidationSubjectLength::default()
            };
        }
        if let GitInputValidationSubjectLength::Chars(length) =
            &mut draft.git_input_validation_subject_length
        {
            ui.add(
                egui::DragValue::new(length)
                    .range(MIN_GIT_INPUT_VALIDATION_LENGTH..=MAX_GIT_INPUT_VALIDATION_LENGTH),
            );
        }
    });
}

pub(super) fn scm_diff_decorations_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmDiffDecorations,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_diff_decorations_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmDiffDecorations::All, "All");
            ui.selectable_value(value, ScmDiffDecorations::Gutter, "Gutter");
            ui.selectable_value(value, ScmDiffDecorations::Overview, "Overview");
            ui.selectable_value(value, ScmDiffDecorations::Minimap, "Minimap");
            ui.selectable_value(value, ScmDiffDecorations::None, "None");
        });
}

fn scm_diff_decorations_label(mode: ScmDiffDecorations) -> &'static str {
    match mode {
        ScmDiffDecorations::All => "All",
        ScmDiffDecorations::Gutter => "Gutter",
        ScmDiffDecorations::Overview => "Overview",
        ScmDiffDecorations::Minimap => "Minimap",
        ScmDiffDecorations::None => "None",
    }
}

pub(super) fn scm_diff_decorations_gutter_visibility_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmDiffDecorationsGutterVisibility,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_diff_decorations_gutter_visibility_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmDiffDecorationsGutterVisibility::Always, "Always");
            ui.selectable_value(value, ScmDiffDecorationsGutterVisibility::Hover, "Hover");
        });
}

fn scm_diff_decorations_gutter_visibility_label(
    mode: ScmDiffDecorationsGutterVisibility,
) -> &'static str {
    match mode {
        ScmDiffDecorationsGutterVisibility::Always => "Always",
        ScmDiffDecorationsGutterVisibility::Hover => "Hover",
    }
}

pub(super) fn scm_diff_decorations_gutter_action_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmDiffDecorationsGutterAction,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_diff_decorations_gutter_action_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmDiffDecorationsGutterAction::Diff, "Open diff");
            ui.selectable_value(value, ScmDiffDecorationsGutterAction::None, "None");
        });
}

fn scm_diff_decorations_gutter_action_label(
    action: ScmDiffDecorationsGutterAction,
) -> &'static str {
    match action {
        ScmDiffDecorationsGutterAction::Diff => "Open diff",
        ScmDiffDecorationsGutterAction::None => "None",
    }
}

pub(super) fn scm_diff_decorations_ignore_trim_whitespace_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut ScmDiffDecorationsIgnoreTrimWhitespace,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(scm_diff_decorations_ignore_trim_whitespace_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ScmDiffDecorationsIgnoreTrimWhitespace::True, "True");
            ui.selectable_value(
                value,
                ScmDiffDecorationsIgnoreTrimWhitespace::False,
                "False",
            );
            ui.selectable_value(
                value,
                ScmDiffDecorationsIgnoreTrimWhitespace::Inherit,
                "Inherit",
            );
        });
}

fn scm_diff_decorations_ignore_trim_whitespace_label(
    value: ScmDiffDecorationsIgnoreTrimWhitespace,
) -> &'static str {
    match value {
        ScmDiffDecorationsIgnoreTrimWhitespace::True => "True",
        ScmDiffDecorationsIgnoreTrimWhitespace::False => "False",
        ScmDiffDecorationsIgnoreTrimWhitespace::Inherit => "Inherit",
    }
}

use crate::{
    KuroyaApp,
    startup_tasks::git_repository_ignored,
    status_bar::items::{git_status_counts_label, source_control_provider_count_badge_label},
    ui_icons::{IconKind, draw_icon, icon_button, icon_label},
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
};
use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Key, Rect, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, TextEdit, TextStyle, pos2, vec2,
};
use kuroya_core::{
    Command, EditorSettings, GitChangeStage, GitFileStatus, GitSmartCommitChanges, GitStatusEntry,
    GitUntrackedChanges, ScmProviderCountBadge, TextBuffer,
};
use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

mod actions;
mod commit;
mod entries;
mod labels;

#[cfg(test)]
use actions::SourceControlRowActionKind;
pub(crate) use actions::source_control_hunks_available;
use actions::{
    SourceControlRowActionTarget, SourceControlRowOpenability, render_source_control_row_actions,
    source_control_cached_row_openability, source_control_has_head_revision,
    source_control_has_index_revision, source_control_keyboard_action,
    source_control_keyboard_action_command, source_control_path_exists_cached,
    source_control_row_action_count, source_control_row_action_strip_width,
    source_control_row_openability, source_control_validated_row_action_command,
};
#[cfg(test)]
pub(crate) use actions::{
    source_control_keyboard_action_labels, source_control_row_action_label_commands,
    source_control_row_action_labels,
};
pub(crate) use commit::{
    normalize_source_control_commit_history, record_source_control_commit_history,
    source_control_clear_commit_input, source_control_commit_action_button_visible,
    source_control_commit_history_message, source_control_commit_input_font,
    source_control_commit_input_rows_for_mode, source_control_commit_input_validation_diagnostics,
    source_control_commit_input_visible, source_control_commit_tooltip,
    source_control_empty_changes_commit_input_visible, source_control_stage_label,
    source_control_stage_section_label, source_control_status_label, source_control_status_marker,
    source_control_verbose_commit_preview, source_control_view_action_button_visible,
};
#[cfg(test)]
pub(crate) use commit::{
    source_control_commit_enabled, source_control_commit_input_rows,
    source_control_smart_commit_count,
};
use commit::{
    source_control_commit_enabled_from_stats, source_control_commit_stats,
    source_control_status_color,
};
#[cfg(test)]
use entries::{
    SourceControlFilterScope, SourceControlFilterTerm, source_control_filter_terms,
    source_control_visible_entry_index_for_selection,
};
pub(crate) use entries::{
    SourceControlSortMode, SourceControlStageSection, SourceControlStageSectionKind,
    SourceControlViewMode, SourceControlVisibleRow, source_control_auto_reveal_selection,
    source_control_filtered_entries, source_control_reveal_selection,
    source_control_sort_mode_from_setting, source_control_sort_mode_label,
    source_control_sorted_entries, source_control_tree_row_indent,
    source_control_view_mode_from_setting, source_control_view_mode_label,
    source_control_visible_rows,
};
#[cfg(test)]
pub(crate) use entries::{
    source_control_entries_for_untracked_changes, source_control_stage_sections,
    source_control_visible_entries, source_control_visible_row_index_for_selection,
};
use entries::{
    source_control_entries_for_untracked_changes_from_slice, source_control_filter_visible_entries,
    source_control_next_sort_mode, source_control_next_view_mode, source_control_section_collapsed,
    source_control_visible_entry_count, source_control_visible_entry_for_selection,
};
#[cfg(test)]
pub(crate) use labels::SOURCE_CONTROL_REF_LABEL_MAX_CHARS;
#[cfg(test)]
pub(crate) use labels::source_control_display_path_label;
pub(crate) use labels::{
    source_control_branch_display_label, source_control_empty_changes_label,
    source_control_filter_empty_label, source_control_git_error_label,
    source_control_repository_label, source_control_result_count_label,
};
#[cfg(test)]
use labels::{
    source_control_branch_display_label_cow, source_control_ref_display_label,
    source_control_ref_display_label_cow, source_control_sanitized_path_label,
    source_control_sanitized_path_label_cow, source_control_sanitized_path_label_owned,
};
use labels::{
    source_control_display_path_label_cow, source_control_display_tree_path_label,
    source_control_path_label, source_control_status_path_label, source_control_tree_path_label,
};

pub(crate) const SOURCE_CONTROL_COMMIT_HISTORY_LIMIT: usize = 50;
const SOURCE_CONTROL_ROW_HEIGHT: f32 = 30.0;

impl KuroyaApp {
    pub(crate) fn render_source_control_panel(&mut self, ui: &mut egui::Ui) {
        render_source_control_header(ui, &mut self.source_control, &mut self.command_bus);
        ui.separator();

        if !self.settings.git_enabled {
            render_empty_source_control_state(ui, "Git is disabled");
            return;
        }

        if git_repository_ignored(
            &self.workspace.root,
            &self.settings.git_ignored_repositories,
        ) {
            render_empty_source_control_state(ui, "Git repository ignored");
            return;
        }

        let git_scan_in_progress = self
            .active_async_tasks
            .iter()
            .any(|task| task.name == "Git Scan");

        if !self.git.has_repository() && git_scan_in_progress {
            render_empty_source_control_state(ui, "Loading git status");
            return;
        }

        let Some(root) = self.git.root().map(Path::to_path_buf) else {
            render_empty_source_control_state(ui, "No git repository");
            return;
        };
        if !source_control_git_root_matches_workspace(&root, &self.workspace.root) {
            render_empty_source_control_state(
                ui,
                source_control_stale_git_state_label(git_scan_in_progress),
            );
            return;
        }
        if let Some(error) = self.git.scan_error() {
            render_empty_source_control_state(ui, &source_control_git_error_label(error));
            return;
        }

        let repositories_visible = source_control_repositories_section_visible(
            self.settings.scm_always_show_repositories,
            self.settings.scm_repositories_visible,
        );
        if repositories_visible {
            render_source_control_repositories(
                ui,
                &root,
                self.git.branch(),
                self.git.counts(),
                self.settings.scm_provider_count_badge,
                self.settings.git_show_reference_details,
            );
            ui.separator();
        }

        let mut selection_changed = false;
        if render_source_control_summary(
            ui,
            self.git.branch(),
            self.git.counts(),
            if repositories_visible {
                ScmProviderCountBadge::Hidden
            } else {
                self.settings.scm_provider_count_badge
            },
            &mut self.source_control_view,
            &mut self.source_control_sort,
            &mut self.command_bus,
        ) {
            self.source_control_selected = 0;
            selection_changed = true;
        }
        ui.separator();

        let raw_entries = self.git.entries_slice();
        let raw_entry_count = raw_entries.len();
        let visible_entries = source_control_entries_for_untracked_changes_from_slice(
            raw_entries,
            self.settings.git_untracked_changes,
        );
        if visible_entries.is_empty() {
            if source_control_empty_changes_commit_input_visible(
                self.settings.git_show_commit_input,
                raw_entry_count,
                visible_entries.as_ref(),
            ) {
                render_source_control_commit_input(
                    ui,
                    visible_entries.as_ref(),
                    &self.settings,
                    &mut self.source_control_commit_message,
                    &self.source_control_commit_history,
                    &mut self.source_control_commit_history_index,
                    &mut self.command_bus,
                );
            }
            render_empty_source_control_state(
                ui,
                source_control_empty_changes_label(
                    raw_entry_count,
                    self.settings.git_untracked_changes,
                ),
            );
            return;
        }
        let entry_count = visible_entries.len();

        ui.horizontal(|ui| {
            if source_control_view_action_button_visible(self.settings.scm_show_action_button) {
                if icon_button(ui, IconKind::Plus, "Stage all changes").clicked() {
                    self.command_bus.push(Command::StageAllChanges);
                }
                if icon_button(ui, IconKind::Minus, "Unstage all changes").clicked() {
                    self.command_bus.push(Command::UnstageAllChanges);
                }
                if icon_button(ui, IconKind::Trash, "Discard all changes").clicked() {
                    self.command_bus.push(Command::DiscardAllChanges);
                }
                if icon_button(ui, IconKind::Code, "Open all changes").clicked() {
                    self.command_bus.push(Command::OpenAllChanges);
                }
                if icon_button(ui, IconKind::Copy, "Copy all changes patch").clicked() {
                    self.command_bus.push(Command::CopyAllChangesPatch);
                }
                if icon_button(ui, IconKind::GitBranch, "Switch branch").clicked() {
                    self.command_bus.push(Command::ToggleGitBranchSwitcher);
                }
                if icon_button(ui, IconKind::Search, "Git history").clicked() {
                    self.command_bus.push(Command::ToggleGitHistory);
                }
                if icon_button(ui, IconKind::Code, "Git stashes").clicked() {
                    self.command_bus.push(Command::ToggleGitStashes);
                }
            }
            ui.label(RichText::new(format!("Changes ({entry_count})")).small());
        });
        let commit_focused = render_source_control_commit_input(
            ui,
            visible_entries.as_ref(),
            &self.settings,
            &mut self.source_control_commit_message,
            &self.source_control_commit_history,
            &mut self.source_control_commit_history_index,
            &mut self.command_bus,
        );

        let filter = ui.add(
            TextEdit::singleline(&mut self.source_control_query)
                .hint_text("Filter changes")
                .desired_width(f32::INFINITY),
        );
        let filter_focused = filter.has_focus();
        if filter.changed() {
            self.source_control_selected = 0;
            selection_changed = true;
        }

        let cached_rows = source_control_rows_cache_entry(
            SourceControlRowsCacheInputs {
                git_revision: self.git.revision(),
                root: &root,
                query: &self.source_control_query,
                sort: self.source_control_sort,
                untracked_changes: self.settings.git_untracked_changes,
                always_show_staged: self.settings.git_always_show_staged_changes_resource_group,
                unstaged_collapsed: self.source_control_unstaged_collapsed,
                untracked_collapsed: self.source_control_untracked_collapsed,
                staged_collapsed: self.source_control_staged_collapsed,
            },
            visible_entries,
            &mut self.source_control_rows_cache,
        );
        let entries: &[GitStatusEntry] = &cached_rows.entries;
        let rows: &[SourceControlVisibleRow] = &cached_rows.rows;
        let render_rows: &[SourceControlRenderRow] = &cached_rows.render_rows;
        let visible_entry_count = source_control_visible_entry_count(rows);
        clamp_selection(&mut self.source_control_selected, visible_entry_count);
        let viewport_height = ui.available_height();
        let change_list_focus_id = ui.make_persistent_id("source-control-change-list");
        let change_list_focused = ui.memory(|memory| memory.has_focus(change_list_focus_id));
        let mut path_exists_cache = HashMap::new();
        selection_changed |= handle_source_control_keyboard(
            ui,
            entries,
            rows,
            visible_entry_count,
            &self.buffers,
            self.index.files(),
            &mut self.source_control_selected,
            &mut self.source_control_query,
            &mut self.source_control,
            &mut self.status,
            &mut self.command_bus,
            &mut path_exists_cache,
            Path::exists,
            filter_focused,
            commit_focused,
            change_list_focused,
            viewport_height,
        );

        if let Some(label) = source_control_result_count_label(
            entry_count,
            entries.len(),
            &self.source_control_query,
        ) {
            ui.label(
                RichText::new(label)
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }

        if entries.is_empty() {
            render_empty_source_control_state(
                ui,
                &source_control_filter_empty_label(&self.source_control_query),
            );
            return;
        }

        let selected_row = source_control_render_row_index_for_selection(
            render_rows,
            self.source_control_selected,
        );
        let mut scroll_area = ScrollArea::vertical().auto_shrink([false, false]);
        if selection_changed && let Some(selected_row) = selected_row {
            scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                selected_row,
                render_rows.len(),
                SOURCE_CONTROL_ROW_HEIGHT,
                viewport_height,
            ));
        }
        scroll_area.show_rows(
            ui,
            SOURCE_CONTROL_ROW_HEIGHT,
            render_rows.len(),
            |ui, visible_rows| {
                for row_index in visible_rows {
                    match &render_rows[row_index] {
                        SourceControlRenderRow::Header(section) => {
                            let section = *section;
                            let collapsed = source_control_section_collapsed(
                                section.kind,
                                self.source_control_unstaged_collapsed,
                                self.source_control_untracked_collapsed,
                                self.source_control_staged_collapsed,
                            );
                            let header = render_source_control_stage_header(ui, section, collapsed);
                            if header.toggled {
                                match section.kind {
                                    SourceControlStageSectionKind::StagedChanges => {
                                        self.source_control_staged_collapsed =
                                            !self.source_control_staged_collapsed;
                                    }
                                    SourceControlStageSectionKind::UntrackedChanges => {
                                        self.source_control_untracked_collapsed =
                                            !self.source_control_untracked_collapsed;
                                    }
                                    SourceControlStageSectionKind::Changes
                                    | SourceControlStageSectionKind::TrackedChanges => {
                                        self.source_control_unstaged_collapsed =
                                            !self.source_control_unstaged_collapsed;
                                    }
                                }
                                self.source_control_selected = 0;
                            }
                            if let Some(command) = header.command {
                                self.command_bus.push(command);
                            }
                        }
                        SourceControlRenderRow::Entry {
                            entry_index,
                            visible_index,
                        } => {
                            let entry = &entries[*entry_index];
                            let selected = *visible_index == self.source_control_selected;
                            let mut row_openability = None;
                            let display = source_control_row_display(
                                &root,
                                entry,
                                self.source_control_view,
                                self.settings.scm_compact_folders,
                            );
                            let row = render_source_control_row(
                                ui,
                                entry,
                                display,
                                self.settings.scm_always_show_actions,
                                selected,
                                *visible_index,
                                || {
                                    source_control_cached_row_openability(
                                        &mut row_openability,
                                        || {
                                            source_control_row_openability(
                                                &mut path_exists_cache,
                                                &self.buffers,
                                                self.index.files(),
                                                &entry.path,
                                                self.explorer_compare_path.as_deref(),
                                                Path::exists,
                                            )
                                        },
                                    )
                                },
                                self.settings.git_show_inline_open_file_action,
                            );
                            if let Some(action) = row.action {
                                ui.memory_mut(|memory| memory.request_focus(change_list_focus_id));
                                self.source_control_selected = *visible_index;
                                let openability = source_control_cached_row_openability(
                                    &mut row_openability,
                                    || {
                                        source_control_row_openability(
                                            &mut path_exists_cache,
                                            &self.buffers,
                                            self.index.files(),
                                            &entry.path,
                                            self.explorer_compare_path.as_deref(),
                                            Path::exists,
                                        )
                                    },
                                );
                                if let Some(command) = source_control_validated_row_action_command(
                                    entry,
                                    action,
                                    openability,
                                    self.settings.git_show_inline_open_file_action,
                                ) {
                                    self.command_bus.push(command);
                                }
                            } else if row.response.clicked() {
                                ui.memory_mut(|memory| memory.request_focus(change_list_focus_id));
                                self.source_control_selected = *visible_index;
                                if let Some(command) = source_control_row_click_command(
                                    self.settings.git_open_diff_on_click,
                                    entry,
                                ) {
                                    self.command_bus.push(command);
                                }
                            }
                            if row.response.secondary_clicked() {
                                ui.memory_mut(|memory| memory.request_focus(change_list_focus_id));
                            }
                            row.response.context_menu(|ui| {
                                let openability = source_control_cached_row_openability(
                                    &mut row_openability,
                                    || {
                                        source_control_row_openability(
                                            &mut path_exists_cache,
                                            &self.buffers,
                                            self.index.files(),
                                            &entry.path,
                                            self.explorer_compare_path.as_deref(),
                                            Path::exists,
                                        )
                                    },
                                );
                                render_source_control_row_context_menu(
                                    ui,
                                    &mut self.command_bus,
                                    &mut self.status,
                                    entry,
                                    openability.source_exists,
                                    openability.can_compare_with_selected,
                                );
                            });
                        }
                    }
                }
            },
        );
    }
}

fn render_source_control_commit_input(
    ui: &mut egui::Ui,
    entries: &[GitStatusEntry],
    settings: &EditorSettings,
    message: &mut String,
    commit_history: &[String],
    commit_history_index: &mut Option<usize>,
    command_bus: &mut kuroya_core::CommandBus,
) -> bool {
    if !source_control_commit_input_visible(settings.git_show_commit_input) {
        return false;
    }

    render_source_control_commit(
        ui,
        entries,
        message,
        settings.scm_show_input_action_button,
        settings.git_use_editor_as_commit_input,
        settings.git_verbose_commit,
        settings.git_show_action_button_commit,
        settings.scm_input_min_line_count,
        settings.scm_input_max_line_count,
        &settings.scm_input_font_family,
        settings.scm_input_font_size,
        settings.font_size,
        settings.ui_font_size,
        settings.git_enable_smart_commit,
        settings.git_suggest_smart_commit,
        settings.git_smart_commit_changes,
        settings.git_confirm_empty_commits,
        settings.git_input_validation,
        settings.git_input_validation_length,
        settings
            .git_input_validation_subject_length
            .resolve(settings.git_input_validation_length),
        commit_history,
        commit_history_index,
        command_bus,
    )
}

fn render_source_control_header(
    ui: &mut egui::Ui,
    source_control: &mut bool,
    command_bus: &mut kuroya_core::CommandBus,
) {
    ui.horizontal(|ui| {
        icon_label(
            ui,
            IconKind::GitBranch,
            ui.visuals().widgets.inactive.fg_stroke.color,
            "Source Control",
        );
        ui.label(RichText::new("Source Control").strong());
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, IconKind::Close, "Close source control").clicked() {
                *source_control = false;
            }
            if icon_button(ui, IconKind::Refresh, "Refresh workspace").clicked() {
                command_bus.push(Command::RefreshWorkspace);
            }
        });
    });
}

fn render_source_control_stage_header(
    ui: &mut egui::Ui,
    section: SourceControlStageSection,
    collapsed: bool,
) -> SourceControlStageHeaderResponse {
    let mut command = None;
    let mut toggled = false;
    ui.allocate_ui_with_layout(
        vec2(ui.available_width().max(160.0), SOURCE_CONTROL_ROW_HEIGHT),
        egui::Layout::left_to_right(Align::Center),
        |ui| {
            if source_control_stage_toggle_button(ui, section.kind, collapsed).clicked() {
                toggled = true;
            }
            let label = ui
                .add(
                    egui::Label::new(
                        RichText::new(source_control_stage_section_header_label(
                            section.kind,
                            section.count,
                        ))
                        .small()
                        .strong(),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_text(source_control_stage_section_collapse_tooltip(
                    section.kind,
                    collapsed,
                ));
            if label.clicked() {
                toggled = true;
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                for action in source_control_stage_section_header_actions(section.kind) {
                    let enabled = source_control_stage_header_action_enabled(section);
                    let tooltip = if enabled {
                        action.tooltip
                    } else {
                        source_control_stage_header_action_disabled_tooltip(section.kind)
                    };
                    let response = ui
                        .add_enabled_ui(enabled, |ui| icon_button(ui, action.icon, tooltip))
                        .inner;
                    if enabled && response.clicked() {
                        command = Some(action.command.clone());
                    }
                }
            });
        },
    );
    SourceControlStageHeaderResponse { command, toggled }
}

struct SourceControlStageHeaderResponse {
    command: Option<Command>,
    toggled: bool,
}

fn source_control_stage_toggle_button(
    ui: &mut egui::Ui,
    kind: SourceControlStageSectionKind,
    collapsed: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
    let visuals = ui.visuals();
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 4.0, visuals.widgets.hovered.bg_fill);
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, visuals.widgets.hovered.bg_stroke.color),
            StrokeKind::Inside,
        );
    }
    draw_icon(
        ui,
        Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
        if collapsed {
            IconKind::ChevronRight
        } else {
            IconKind::ChevronDown
        },
        visuals.widgets.inactive.fg_stroke.color,
    );
    response.on_hover_text(source_control_stage_section_collapse_tooltip(
        kind, collapsed,
    ))
}

struct SourceControlStageHeaderAction {
    icon: IconKind,
    tooltip: &'static str,
    command: Command,
}

const EMPTY_STAGE_HEADER_ACTIONS: &[SourceControlStageHeaderAction] = &[];
const STAGED_STAGE_HEADER_ACTIONS: &[SourceControlStageHeaderAction] = &[
    SourceControlStageHeaderAction {
        icon: IconKind::Code,
        tooltip: "Open all staged changes",
        command: Command::OpenAllStagedChanges,
    },
    SourceControlStageHeaderAction {
        icon: IconKind::Copy,
        tooltip: "Copy staged patch",
        command: Command::CopyStagedChangesPatch,
    },
    SourceControlStageHeaderAction {
        icon: IconKind::Minus,
        tooltip: "Unstage all changes",
        command: Command::UnstageAllChanges,
    },
];
const UNSTAGED_STAGE_HEADER_ACTIONS: &[SourceControlStageHeaderAction] = &[
    SourceControlStageHeaderAction {
        icon: IconKind::Code,
        tooltip: "Open all unstaged changes",
        command: Command::OpenAllUnstagedChanges,
    },
    SourceControlStageHeaderAction {
        icon: IconKind::Copy,
        tooltip: "Copy unstaged patch",
        command: Command::CopyUnstagedChangesPatch,
    },
    SourceControlStageHeaderAction {
        icon: IconKind::Plus,
        tooltip: "Stage all changes",
        command: Command::StageAllChanges,
    },
    SourceControlStageHeaderAction {
        icon: IconKind::Trash,
        tooltip: "Discard all changes",
        command: Command::DiscardAllChanges,
    },
];

fn source_control_stage_header_action_enabled(section: SourceControlStageSection) -> bool {
    section.count > 0
}

fn source_control_stage_header_action_disabled_tooltip(
    kind: SourceControlStageSectionKind,
) -> &'static str {
    match kind {
        SourceControlStageSectionKind::StagedChanges => "No staged changes",
        SourceControlStageSectionKind::Changes
        | SourceControlStageSectionKind::TrackedChanges
        | SourceControlStageSectionKind::UntrackedChanges => "No unstaged changes",
    }
}

#[cfg(test)]
fn source_control_stage_header_actions(
    stage: GitChangeStage,
) -> &'static [SourceControlStageHeaderAction] {
    source_control_stage_section_header_actions(match stage {
        GitChangeStage::Unstaged => SourceControlStageSectionKind::Changes,
        GitChangeStage::Staged => SourceControlStageSectionKind::StagedChanges,
    })
}

fn source_control_stage_section_header_actions(
    kind: SourceControlStageSectionKind,
) -> &'static [SourceControlStageHeaderAction] {
    match kind {
        SourceControlStageSectionKind::StagedChanges => STAGED_STAGE_HEADER_ACTIONS,
        SourceControlStageSectionKind::Changes => UNSTAGED_STAGE_HEADER_ACTIONS,
        SourceControlStageSectionKind::TrackedChanges
        | SourceControlStageSectionKind::UntrackedChanges => EMPTY_STAGE_HEADER_ACTIONS,
    }
}

#[cfg(test)]
pub(crate) fn source_control_stage_header_label(stage: GitChangeStage, count: usize) -> String {
    format!("{} ({count})", source_control_stage_label(stage))
}

pub(crate) fn source_control_stage_section_header_label(
    kind: SourceControlStageSectionKind,
    count: usize,
) -> String {
    format!("{} ({count})", source_control_stage_section_label(kind))
}

#[cfg(test)]
pub(crate) fn source_control_stage_collapse_tooltip(
    stage: GitChangeStage,
    collapsed: bool,
) -> String {
    let action = if collapsed { "Expand" } else { "Collapse" };
    format!("{action} {}", source_control_stage_label(stage))
}

fn source_control_stage_section_collapse_tooltip(
    kind: SourceControlStageSectionKind,
    collapsed: bool,
) -> String {
    let action = if collapsed { "Expand" } else { "Collapse" };
    format!("{action} {}", source_control_stage_section_label(kind))
}

#[cfg(test)]
pub(crate) fn source_control_stage_header_action_labels(
    stage: GitChangeStage,
) -> Vec<&'static str> {
    source_control_stage_header_actions(stage)
        .iter()
        .map(|action| action.tooltip)
        .collect()
}

fn handle_source_control_keyboard(
    ui: &mut egui::Ui,
    entries: &[GitStatusEntry],
    rows: &[SourceControlVisibleRow],
    visible_entry_count: usize,
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    selected: &mut usize,
    query: &mut String,
    source_control: &mut bool,
    status: &mut String,
    command_bus: &mut kuroya_core::CommandBus,
    path_exists_cache: &mut HashMap<PathBuf, bool>,
    mut path_exists: impl FnMut(&Path) -> bool,
    filter_focused: bool,
    commit_focused: bool,
    change_list_focused: bool,
    viewport_height: f32,
) -> bool {
    if filter_focused {
        if ui.input(|input| input.key_pressed(Key::Escape)) && !query.is_empty() {
            query.clear();
            *selected = 0;
            return true;
        }
        return false;
    }
    if !source_control_change_list_keyboard_active(commit_focused, change_list_focused) {
        return false;
    }

    let mut selection_changed = false;
    if ui.input(|input| input.key_pressed(Key::Escape)) {
        if query.is_empty() {
            *source_control = false;
        } else {
            query.clear();
            *selected = 0;
            selection_changed = true;
        }
    }
    selection_changed |= ui.input(|input| {
        handle_list_navigation_keys(
            input,
            selected,
            visible_entry_count,
            selection_page_step(SOURCE_CONTROL_ROW_HEIGHT, viewport_height),
        )
    });
    if let Some(entry) = source_control_visible_entry_for_selection(entries, rows, *selected)
        && let Some(action) = ui.input(|input| {
            source_control_keyboard_action(input, entry, || {
                source_control_path_exists_cached(
                    path_exists_cache,
                    buffers,
                    indexed_files,
                    &entry.path,
                    |path| path_exists(path),
                )
            })
        })
    {
        if let Some(command) = source_control_keyboard_action_command(entry, action) {
            command_bus.push(command);
        } else {
            *status = source_control_status_path_label(&entry.path);
        }
    }
    selection_changed
}

fn source_control_change_list_keyboard_active(
    commit_focused: bool,
    change_list_focused: bool,
) -> bool {
    change_list_focused && !commit_focused
}

fn open_changes_command_for_entry(entry: &GitStatusEntry) -> Command {
    match entry.stage {
        GitChangeStage::Staged => Command::OpenStagedFileChanges(entry.path.clone()),
        GitChangeStage::Unstaged => Command::OpenFileChanges(entry.path.clone()),
    }
}

pub(crate) fn source_control_row_click_command(
    open_diff_on_click: bool,
    entry: &GitStatusEntry,
) -> Option<Command> {
    open_diff_on_click.then(|| open_changes_command_for_entry(entry))
}

fn copy_patch_command_for_entry(entry: &GitStatusEntry) -> Command {
    match entry.stage {
        GitChangeStage::Staged => Command::CopyStagedFilePatch(entry.path.clone()),
        GitChangeStage::Unstaged => Command::CopyFilePatch(entry.path.clone()),
    }
}

fn render_source_control_row_context_menu(
    ui: &mut egui::Ui,
    command_bus: &mut kuroya_core::CommandBus,
    status: &mut String,
    entry: &GitStatusEntry,
    source_exists: bool,
    can_compare_with_selected: bool,
) {
    let path = &entry.path;
    if ui
        .button(open_changes_label_for_stage(entry.stage))
        .clicked()
    {
        command_bus.push(open_changes_command_for_entry(entry));
        ui.close();
    }
    if ui.button("Copy Patch").clicked() {
        command_bus.push(copy_patch_command_for_entry(entry));
        ui.close();
    }
    if ui.button("Compare with HEAD").clicked() {
        command_bus.push(Command::OpenFileHeadChanges(path.clone()));
        ui.close();
    }
    if source_control_has_head_revision(entry.status) && ui.button("Open File at HEAD").clicked() {
        command_bus.push(Command::OpenFileHeadRevision(path.clone()));
        ui.close();
    }
    if source_control_has_index_revision(entry.stage, entry.status)
        && ui.button("Open File at Index").clicked()
    {
        command_bus.push(Command::OpenFileIndexRevision(path.clone()));
        ui.close();
    }
    if source_exists && ui.button("Select for Compare").clicked() {
        command_bus.push(Command::SelectFileForCompare(path.clone()));
        ui.close();
    }
    if can_compare_with_selected && ui.button("Compare with Selected").clicked() {
        command_bus.push(Command::CompareFileWithSelected(path.clone()));
        ui.close();
    }
    if entry.stage == GitChangeStage::Unstaged && ui.button("Stage Changes").clicked() {
        command_bus.push(Command::StageFileChange(path.clone()));
        ui.close();
    }
    if entry.stage == GitChangeStage::Unstaged
        && source_control_hunks_available(entry.stage, entry.status, source_exists)
        && ui.button("Open Hunks").clicked()
    {
        command_bus.push(Command::OpenFileHunks(path.clone()));
        ui.close();
    }
    if entry.stage == GitChangeStage::Staged
        && source_control_hunks_available(entry.stage, entry.status, source_exists)
        && ui.button("Open Staged Hunks").clicked()
    {
        command_bus.push(Command::OpenStagedFileHunks(path.clone()));
        ui.close();
    }
    if entry.stage == GitChangeStage::Staged && ui.button("Unstage Changes").clicked() {
        command_bus.push(Command::UnstageFileChange(path.clone()));
        ui.close();
    }
    if ui.button("Discard Changes").clicked() {
        command_bus.push(Command::DiscardFileChanges(path.clone()));
        ui.close();
    }
    if source_exists
        && ui
            .button(open_file_label_for_status(entry.status))
            .clicked()
    {
        command_bus.push(Command::OpenFile(path.clone()));
        ui.close();
    }
    if ui.button("Reveal in Explorer").clicked() {
        command_bus.push(Command::RevealFileInExplorer(path.clone()));
        ui.close();
    }
    if source_exists && ui.button("Open Blame").clicked() {
        command_bus.push(Command::OpenFileBlame(path.clone()));
        ui.close();
    }
    if ui.button("Copy Path").clicked() {
        command_bus.push(Command::CopyFilePath(path.clone()));
        ui.close();
    }
    if ui.button("Copy Relative Path").clicked() {
        command_bus.push(Command::CopyFileRelativePath(path.clone()));
        ui.close();
    }
    if ui.button("Show Path").clicked() {
        *status = source_control_status_path_label(path);
        ui.close();
    }
}

fn open_changes_label_for_stage(stage: GitChangeStage) -> &'static str {
    match stage {
        GitChangeStage::Staged => "Open Staged Changes",
        GitChangeStage::Unstaged => "Open Changes",
    }
}

fn open_file_label_for_status(status: GitFileStatus) -> &'static str {
    match status {
        GitFileStatus::Conflicted => "Open File to Resolve",
        _ => "Open File",
    }
}

fn render_source_control_summary(
    ui: &mut egui::Ui,
    branch: Option<&str>,
    counts: kuroya_core::GitStatusCounts,
    provider_count_badge: ScmProviderCountBadge,
    view_mode: &mut SourceControlViewMode,
    sort_mode: &mut SourceControlSortMode,
    command_bus: &mut kuroya_core::CommandBus,
) -> bool {
    let count_label = git_status_counts_label(counts);
    let provider_badge = source_control_provider_count_badge_label(counts, provider_count_badge);
    let branch_label = source_control_branch_display_label(branch);
    let mut selection_changed = false;
    ui.horizontal(|ui| {
        if ui
            .button(RichText::new(&branch_label).monospace())
            .on_hover_text("Switch git branch")
            .clicked()
        {
            command_bus.push(Command::ToggleGitBranchSwitcher);
        }
        if let Some(provider_badge) = provider_badge {
            ui.label(RichText::new(provider_badge).small().strong())
                .on_hover_text("Source Control Provider count badge");
        }
        if ui.button("History").clicked() {
            command_bus.push(Command::ToggleGitHistory);
        }
        if ui.button("Stashes").clicked() {
            command_bus.push(Command::ToggleGitStashes);
        }
        if ui
            .button("Terminal")
            .on_hover_text("Open Source Control in Integrated Terminal")
            .clicked()
        {
            command_bus.push(Command::OpenSourceControlInIntegratedTerminal);
        }
        if ui
            .button(format!(
                "View: {}",
                source_control_view_mode_label(*view_mode)
            ))
            .on_hover_text(format!(
                "Show source control as {}; click for {}",
                source_control_view_mode_label(*view_mode).to_ascii_lowercase(),
                source_control_view_mode_label(source_control_next_view_mode(*view_mode))
                    .to_ascii_lowercase()
            ))
            .clicked()
        {
            *view_mode = source_control_next_view_mode(*view_mode);
            selection_changed = true;
        }
        if ui
            .button(format!(
                "Sort: {}",
                source_control_sort_mode_label(*sort_mode)
            ))
            .on_hover_text(format!(
                "Sort source control by {}; click for {}",
                source_control_sort_mode_label(*sort_mode).to_ascii_lowercase(),
                source_control_sort_mode_label(source_control_next_sort_mode(*sort_mode))
                    .to_ascii_lowercase()
            ))
            .clicked()
        {
            *sort_mode = source_control_next_sort_mode(*sort_mode);
            selection_changed = true;
        }
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            let summary = if count_label.is_empty() {
                "clean".to_owned()
            } else {
                count_label
            };
            ui.label(RichText::new(summary).small());
        });
    });
    selection_changed
}

fn render_source_control_repositories(
    ui: &mut egui::Ui,
    root: &Path,
    branch: Option<&str>,
    counts: kuroya_core::GitStatusCounts,
    provider_count_badge: ScmProviderCountBadge,
    show_reference_details: bool,
) {
    let provider_badge = source_control_provider_count_badge_label(counts, provider_count_badge);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Repositories").small().strong());
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            if let Some(provider_badge) = provider_badge {
                ui.label(RichText::new(provider_badge).small().strong())
                    .on_hover_text("Source Control Provider count badge");
            }
        });
    });
    ui.horizontal(|ui| {
        icon_label(
            ui,
            IconKind::GitBranch,
            ui.visuals().widgets.inactive.fg_stroke.color,
            "Source Control Repository",
        );
        ui.label(
            RichText::new(source_control_repository_label(
                root,
                branch,
                show_reference_details,
            ))
            .small(),
        );
    });
}

pub(crate) fn source_control_repositories_section_visible(
    always_show_repositories: bool,
    repositories_visible: usize,
) -> bool {
    always_show_repositories && repositories_visible > 0
}

fn render_source_control_commit(
    ui: &mut egui::Ui,
    entries: &[GitStatusEntry],
    message: &mut String,
    show_input_action_button: bool,
    use_editor_as_commit_input: bool,
    verbose_commit: bool,
    show_commit_action_button: bool,
    min_line_count: usize,
    max_line_count: usize,
    input_font_family: &str,
    input_font_size: f32,
    editor_font_size: f32,
    ui_font_size: f32,
    smart_commit_enabled: bool,
    suggest_smart_commit: bool,
    smart_commit_changes: GitSmartCommitChanges,
    confirm_empty_commits: bool,
    input_validation: bool,
    input_validation_length: usize,
    input_validation_subject_length: usize,
    commit_history: &[String],
    commit_history_index: &mut Option<usize>,
    command_bus: &mut kuroya_core::CommandBus,
) -> bool {
    let commit_stats = source_control_commit_stats(entries, smart_commit_changes);
    let staged_count = commit_stats.staged_count;
    let input_rows = source_control_commit_input_rows_for_mode(
        use_editor_as_commit_input,
        message,
        min_line_count,
        max_line_count,
    );
    let input_font = source_control_commit_input_font(
        input_font_family,
        input_font_size,
        editor_font_size,
        ui_font_size,
    );
    let input = if use_editor_as_commit_input {
        TextEdit::multiline(message).desired_rows(input_rows)
    } else {
        TextEdit::singleline(message)
    };
    let response = ui.add(
        input
            .hint_text("Commit message")
            .font(input_font.clone())
            .desired_width(f32::INFINITY),
    );
    let has_conflicts = commit_stats.has_conflicts;
    let enabled = source_control_commit_enabled_from_stats(
        message,
        smart_commit_enabled,
        suggest_smart_commit,
        confirm_empty_commits,
        commit_stats,
    );
    let tooltip = source_control_commit_tooltip(
        staged_count,
        message,
        has_conflicts,
        smart_commit_enabled,
        suggest_smart_commit,
        commit_stats.smart_commit_count,
        confirm_empty_commits,
    );
    for diagnostic in source_control_commit_input_validation_diagnostics(
        message,
        input_validation,
        input_validation_length,
        input_validation_subject_length,
    ) {
        ui.label(
            RichText::new(diagnostic)
                .small()
                .color(Color32::from_rgb(220, 170, 80)),
        );
    }
    if let Some(mut preview) =
        source_control_verbose_commit_preview(entries, verbose_commit, use_editor_as_commit_input)
    {
        let preview_rows = preview.lines().count().clamp(3, 8);
        ui.add_enabled(
            false,
            TextEdit::multiline(&mut preview)
                .font(FontId::new(input_font.size, FontFamily::Monospace))
                .desired_rows(preview_rows)
                .desired_width(f32::INFINITY),
        );
    }
    let keyboard_commit = response.has_focus()
        && enabled
        && ui.input(|input| {
            input.key_pressed(Key::Enter) && (input.modifiers.command || input.modifiers.ctrl)
        });
    let previous_history = response.has_focus()
        && ui.input(|input| input.key_pressed(Key::ArrowUp) && input.modifiers.alt);
    let next_history = response.has_focus()
        && ui.input(|input| input.key_pressed(Key::ArrowDown) && input.modifiers.alt);
    let clear_input = response.has_focus() && ui.input(|input| input.key_pressed(Key::Escape));
    if source_control_clear_commit_input(message, clear_input) {
        *commit_history_index = None;
    } else if previous_history {
        if let Some(history_message) =
            source_control_commit_history_message(commit_history, commit_history_index, -1)
        {
            *message = history_message;
        }
    } else if next_history
        && let Some(history_message) =
            source_control_commit_history_message(commit_history, commit_history_index, 1)
    {
        *message = history_message;
    } else if response.changed() {
        *commit_history_index = None;
    }
    let button_commit = source_control_commit_action_button_visible(
        show_input_action_button,
        show_commit_action_button,
    ) && ui
        .add_enabled(enabled, egui::Button::new("Commit"))
        .on_hover_text(tooltip)
        .clicked();
    if button_commit || keyboard_commit {
        command_bus.push(Command::CommitStagedChanges);
    }
    response.has_focus()
}

fn render_empty_source_control_state(ui: &mut egui::Ui, label: &str) {
    ui.add_space(24.0);
    ui.centered_and_justified(|ui| {
        icon_label(
            ui,
            IconKind::GitBranch,
            ui.visuals().widgets.inactive.fg_stroke.color,
            label,
        );
        ui.label(RichText::new(label).small());
    });
}

fn source_control_stale_git_state_label(git_scan_in_progress: bool) -> &'static str {
    if git_scan_in_progress {
        "Refreshing git status"
    } else {
        "Git status is stale"
    }
}

pub(crate) fn source_control_git_root_matches_workspace(
    git_root: &Path,
    workspace_root: &Path,
) -> bool {
    let git_root = source_control_path_components(git_root);
    let workspace_root = source_control_path_components(workspace_root);
    source_control_component_prefix(&git_root, &workspace_root)
        || source_control_component_prefix(&workspace_root, &git_root)
}

fn source_control_component_prefix(prefix: &[String], path: &[String]) -> bool {
    if prefix.is_empty() {
        return path
            .first()
            .is_none_or(|component| component.starts_with("normal:"));
    }
    !prefix.is_empty() && prefix.len() <= path.len() && path.starts_with(prefix)
}

fn source_control_path_components(path: &Path) -> Vec<String> {
    let path_components = path.components();
    let mut components: Vec<String> = Vec::with_capacity(path_components.size_hint().0);
    for component in path_components {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if components
                    .last()
                    .is_some_and(|component| component.starts_with("normal:"))
                {
                    components.pop();
                } else {
                    components.push("parent:".to_owned());
                }
            }
            Component::Prefix(prefix) => {
                components.push(format!(
                    "prefix:{}",
                    source_control_path_component_key(prefix.as_os_str())
                ));
            }
            Component::RootDir => components.push("root:".to_owned()),
            Component::Normal(component) => {
                components.push(format!(
                    "normal:{}",
                    source_control_path_component_key(component)
                ));
            }
        }
    }
    components
}

#[cfg(windows)]
fn source_control_path_component_key(component: &OsStr) -> String {
    component.to_string_lossy().to_lowercase()
}

#[cfg(not(windows))]
fn source_control_path_component_key(component: &OsStr) -> String {
    component.to_string_lossy().into_owned()
}

fn render_source_control_row(
    ui: &mut egui::Ui,
    entry: &GitStatusEntry,
    display: SourceControlRowDisplay,
    always_show_actions: bool,
    selected: bool,
    row_index: usize,
    openability: impl FnMut() -> SourceControlRowOpenability,
    show_inline_open_file_action: bool,
) -> SourceControlRowResponse {
    let SourceControlRowDisplay {
        text,
        hover_path_label,
        indent,
    } = display;
    let row_height = 30.0;
    let width = ui.available_width().max(160.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, row_height), Sense::click());
    let visuals = ui.visuals();
    let show_actions =
        source_control_row_actions_visible(always_show_actions, selected, response.hovered());
    let action_openability = show_actions.then(openability);

    let fill = if response.is_pointer_button_down_on() {
        visuals.widgets.active.bg_fill
    } else if selected {
        visuals.widgets.active.weak_bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(rect.shrink2(vec2(2.0, 1.0)), 4.0, fill);
    }
    if selected || response.hovered() {
        ui.painter().rect_stroke(
            rect.shrink2(vec2(2.0, 1.0)),
            4.0,
            Stroke::new(
                1.0,
                if selected {
                    visuals.widgets.active.bg_stroke.color
                } else {
                    visuals.widgets.hovered.bg_stroke.color
                },
            ),
            StrokeKind::Inside,
        );
    }

    let marker = source_control_status_marker(entry.status);
    let marker_color = source_control_status_color(entry.status);
    let center_y = rect.center().y;
    ui.painter().text(
        pos2(rect.left() + 14.0, center_y),
        egui::Align2::CENTER_CENTER,
        marker,
        FontId::monospace(10.5),
        marker_color,
    );

    let text_color = visuals.widgets.inactive.fg_stroke.color;
    let font_id = TextStyle::Body.resolve(ui.style());
    let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(text, font_id, text_color));
    let text_x = rect.left() + 32.0 + indent;
    let action_count = action_openability.as_ref().map_or(0, |openability| {
        source_control_row_action_count(
            entry.stage,
            entry.status,
            openability.source_exists,
            openability.can_compare_with_selected,
            show_inline_open_file_action,
        )
    });
    let action_width = source_control_row_action_strip_width(action_count);
    let clip_right = (rect.right() - 8.0 - action_width).max(text_x);
    let clip_rect = Rect::from_min_max(pos2(text_x, rect.top()), pos2(clip_right, rect.bottom()));
    ui.painter().with_clip_rect(clip_rect).galley(
        pos2(text_x, center_y - galley.rect.height() / 2.0),
        galley,
        text_color,
    );

    let action = action_openability.and_then(|openability| {
        render_source_control_row_actions(
            ui,
            rect,
            row_index,
            entry,
            openability.source_exists,
            openability.can_compare_with_selected,
            show_inline_open_file_action,
            action_count,
        )
    });
    let response = if response.hovered() {
        response.on_hover_text(hover_path_label)
    } else {
        response
    };
    SourceControlRowResponse { response, action }
}

pub(crate) fn source_control_row_actions_visible(
    always_show_actions: bool,
    selected: bool,
    hovered: bool,
) -> bool {
    always_show_actions || selected || hovered
}

struct SourceControlRowResponse {
    response: egui::Response,
    action: Option<SourceControlRowActionTarget>,
}

#[derive(Debug, Clone, PartialEq)]
struct SourceControlRowDisplay {
    text: String,
    hover_path_label: String,
    indent: f32,
}

fn source_control_row_display(
    root: &Path,
    entry: &GitStatusEntry,
    view_mode: SourceControlViewMode,
    compact_folders: bool,
) -> SourceControlRowDisplay {
    let path_label =
        source_control_display_path_label_cow(root, &entry.path, view_mode, compact_folders);
    let status_label = source_control_status_label(entry.status);
    SourceControlRowDisplay {
        text: format!("{}  {status_label}", path_label.as_ref()),
        hover_path_label: source_control_display_tree_path_label(root, &entry.path, true),
        indent: source_control_tree_row_indent(root, &entry.path, view_mode, compact_folders),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum SourceControlRenderRow {
    Header(SourceControlStageSection),
    Entry {
        entry_index: usize,
        visible_index: usize,
    },
}

fn source_control_prepare_render_rows(
    entries: &[GitStatusEntry],
    rows: &[SourceControlVisibleRow],
) -> Vec<SourceControlRenderRow> {
    let mut render_rows = Vec::with_capacity(rows.len());
    for row in rows {
        match *row {
            SourceControlVisibleRow::Header(section) => {
                render_rows.push(SourceControlRenderRow::Header(section));
            }
            SourceControlVisibleRow::Entry {
                entry_index,
                visible_index,
            } => {
                if entries.get(entry_index).is_some() {
                    render_rows.push(SourceControlRenderRow::Entry {
                        entry_index,
                        visible_index,
                    });
                }
            }
        }
    }
    render_rows
}

fn source_control_render_row_index_for_selection(
    rows: &[SourceControlRenderRow],
    selected: usize,
) -> Option<usize> {
    rows.iter().position(|row| {
        matches!(
            row,
            SourceControlRenderRow::Entry { visible_index, .. } if *visible_index == selected
        )
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceControlRowsCache {
    key: SourceControlRowsCacheKey,
    entries: Vec<GitStatusEntry>,
    rows: Vec<SourceControlVisibleRow>,
    render_rows: Vec<SourceControlRenderRow>,
}

#[derive(Debug, Clone, PartialEq)]
struct SourceControlRowsCacheKey {
    git_revision: u64,
    root: PathBuf,
    query: String,
    sort: SourceControlSortMode,
    untracked_changes: GitUntrackedChanges,
    always_show_staged: bool,
    unstaged_collapsed: bool,
    untracked_collapsed: bool,
    staged_collapsed: bool,
}

#[derive(Debug, Clone, Copy)]
struct SourceControlRowsCacheInputs<'a> {
    git_revision: u64,
    root: &'a Path,
    query: &'a str,
    sort: SourceControlSortMode,
    untracked_changes: GitUntrackedChanges,
    always_show_staged: bool,
    unstaged_collapsed: bool,
    untracked_collapsed: bool,
    staged_collapsed: bool,
}

impl SourceControlRowsCacheKey {
    fn matches(&self, inputs: SourceControlRowsCacheInputs<'_>) -> bool {
        self.git_revision == inputs.git_revision
            && self.root == inputs.root
            && self.query == inputs.query
            && self.sort == inputs.sort
            && self.untracked_changes == inputs.untracked_changes
            && self.always_show_staged == inputs.always_show_staged
            && self.unstaged_collapsed == inputs.unstaged_collapsed
            && self.untracked_collapsed == inputs.untracked_collapsed
            && self.staged_collapsed == inputs.staged_collapsed
    }
}

fn source_control_rows_cache_entry<'cache>(
    inputs: SourceControlRowsCacheInputs<'_>,
    visible_entries: Cow<'_, [GitStatusEntry]>,
    cache: &'cache mut Option<SourceControlRowsCache>,
) -> &'cache SourceControlRowsCache {
    let rebuild = !cache
        .as_ref()
        .is_some_and(|cached| cached.key.matches(inputs));
    if rebuild {
        let entries = source_control_sorted_entries(
            inputs.root,
            source_control_filter_visible_entries(inputs.root, visible_entries, inputs.query),
            inputs.sort,
        );
        let rows = source_control_visible_rows(
            &entries,
            inputs.always_show_staged,
            inputs.untracked_changes,
            inputs.unstaged_collapsed,
            inputs.untracked_collapsed,
            inputs.staged_collapsed,
        );
        let render_rows = source_control_prepare_render_rows(&entries, &rows);
        *cache = Some(SourceControlRowsCache {
            key: SourceControlRowsCacheKey {
                git_revision: inputs.git_revision,
                root: inputs.root.to_path_buf(),
                query: inputs.query.to_owned(),
                sort: inputs.sort,
                untracked_changes: inputs.untracked_changes,
                always_show_staged: inputs.always_show_staged,
                unstaged_collapsed: inputs.unstaged_collapsed,
                untracked_collapsed: inputs.untracked_collapsed,
                staged_collapsed: inputs.staged_collapsed,
            },
            entries,
            rows,
            render_rows,
        });
    }
    cache
        .as_ref()
        .expect("source control rows cache is populated")
}

#[cfg(test)]
mod tests;

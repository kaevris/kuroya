use crate::{
    KuroyaApp,
    path_display::sanitized_display_label_cow,
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
};
use eframe::egui::{self, Context, InputState, Key, RichText, ScrollArea, TextEdit};
use kuroya_core::{
    GitBranch, GitBranchSortOrder, GitCheckoutType, git_branch_validation_error,
    text_match::ascii_case_insensitive_contains,
};
use std::{borrow::Cow, ops::Range};

const SOURCE_CONTROL_BRANCH_ROW_HEIGHT: f32 = 24.0;
const SOURCE_CONTROL_BRANCH_DISPLAY_MAX_CHARS: usize = 160;
const SOURCE_CONTROL_BRANCH_LABEL_MAX_CHARS: usize = 180;
const SOURCE_CONTROL_BRANCH_STATUS_DETAIL_MAX_CHARS: usize = 240;
const SOURCE_CONTROL_BRANCH_TOOLTIP_MAX_CHARS: usize = 240;

impl KuroyaApp {
    pub(crate) fn render_git_branch_switcher(&mut self, ctx: &Context) {
        let mut selected_branch: Option<(String, GitCheckoutType)> = None;
        let mut copied_branch = None;
        let mut created_branch = None;
        let mut deleted_branch = None;
        let mut renamed_branch = None;
        let mut begin_rename = None;
        let mut cancel_rename = false;
        let rename_from = self.source_control_branch_rename_from.clone();

        egui::Window::new(if rename_from.is_some() {
            "Rename Git Branch"
        } else {
            "Switch Git Branch"
        })
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
        .max_size(crate::layout::popup_window_max_size_with_top_margin(
            ctx, 96.0,
        ))
        .fixed_size([520.0, 360.0])
        .show(ctx, |ui| {
            let renaming = rename_from.as_deref();
            let response = ui.add(
                TextEdit::singleline(&mut self.source_control_branch_query)
                    .hint_text(if renaming.is_some() {
                        "New branch name"
                    } else {
                        "Select branch"
                    })
                    .desired_width(f32::INFINITY),
            );
            response.request_focus();
            let query_changed = response.changed();
            if query_changed {
                self.source_control_branch_selected = 0;
            }

            if ui.input(|input| input.key_pressed(Key::Escape)) {
                if renaming.is_some() {
                    cancel_rename = true;
                } else {
                    self.source_control_branch_picker_open = false;
                }
            }

            let sorted_branches = source_control_sorted_branch_refs(
                &self.source_control_branches,
                self.settings.git_branch_sort_order,
            );
            let branches =
                if renaming.is_some() || self.source_control_branch_query.trim().is_empty() {
                    sorted_branches
                } else {
                    source_control_filtered_branch_refs(
                        &sorted_branches,
                        &self.source_control_branch_query,
                    )
                };
            let row_count = branches.len();
            if let Some(branch) = renaming
                && let Some(index) = source_control_branch_ref_index_by_name(&branches, branch)
            {
                self.source_control_branch_selected = index;
            }
            let create_branch_name = renaming.is_none().then(|| {
                source_control_branch_create_name(
                    &self.source_control_branch_query,
                    &self.source_control_branches,
                    &self.settings.git_branch_prefix,
                    &self.settings.git_branch_validation_regex,
                    &self.settings.git_branch_whitespace_char,
                )
            });
            let create_branch_name = create_branch_name.flatten();
            let create_branch_blocked_reason = (renaming.is_none() && create_branch_name.is_none())
                .then(|| {
                    source_control_branch_create_blocked_reason(
                        &self.source_control_branch_query,
                        &self.source_control_branches,
                        &self.settings.git_branch_prefix,
                        &self.settings.git_branch_validation_regex,
                        &self.settings.git_branch_whitespace_char,
                    )
                })
                .flatten();
            let rename_target = renaming.and_then(|branch| {
                source_control_branch_rename_target(
                    &self.source_control_branch_query,
                    branch,
                    &self.source_control_branches,
                    &self.settings.git_branch_validation_regex,
                )
            });
            let rename_blocked_reason =
                renaming
                    .filter(|_| rename_target.is_none())
                    .and_then(|branch| {
                        source_control_branch_rename_blocked_reason(
                            &self.source_control_branch_query,
                            branch,
                            &self.source_control_branches,
                            &self.settings.git_branch_validation_regex,
                        )
                    });
            clamp_selection(&mut self.source_control_branch_selected, row_count);

            let viewport_height = ui.available_height();
            let selection_changed = ui.input(|input| {
                handle_list_navigation_keys(
                    input,
                    &mut self.source_control_branch_selected,
                    row_count,
                    selection_page_step(SOURCE_CONTROL_BRANCH_ROW_HEIGHT, viewport_height),
                )
            }) || query_changed;
            let selected_row = source_control_branch_prepared_row_at(
                &branches,
                self.source_control_branch_selected,
                renaming,
            );
            if let Some(branch) = renaming
                && let Some(target) = rename_target.clone()
                && ui.input(|input| input.key_pressed(Key::Enter))
            {
                renamed_branch = Some((branch.to_owned(), target));
            } else if ui.input(|input| input.key_pressed(Key::Enter))
                && let Some(row) = selected_row.as_ref()
                && row.can_switch()
            {
                selected_branch = Some(row.checkout_target());
            } else if row_count == 0
                && let Some(branch) = create_branch_name.clone()
                && ui.input(|input| input.key_pressed(Key::Enter))
            {
                created_branch = Some(branch);
            }
            if renaming.is_none()
                && let Some(row) = selected_row.as_ref()
                && let Some(action) = ui.input(source_control_branch_keyboard_action)
            {
                match action {
                    SourceControlBranchKeyboardActionKind::CopyName => {
                        copied_branch = Some(row.copy_name());
                    }
                    SourceControlBranchKeyboardActionKind::Rename => {
                        if row.can_rename() {
                            begin_rename = Some(row.raw_name().to_owned());
                        } else {
                            self.status = row.rename_action_tooltip();
                        }
                    }
                    SourceControlBranchKeyboardActionKind::Delete => {
                        if row.can_delete() {
                            deleted_branch = Some(row.raw_name().to_owned());
                        } else {
                            self.status = row.delete_tooltip().to_owned();
                        }
                    }
                }
            }

            if row_count == 0 {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(20.0);
                    ui.centered_and_justified(|ui| {
                        ui.label(source_control_branch_empty_label(
                            &self.source_control_branch_query,
                            self.source_control_branch_in_flight_request_id.is_some(),
                        ));
                    });
                });
            } else {
                let mut scroll_area = ScrollArea::vertical();
                if selection_changed {
                    scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                        self.source_control_branch_selected,
                        row_count,
                        SOURCE_CONTROL_BRANCH_ROW_HEIGHT,
                        viewport_height,
                    ));
                }
                scroll_area.show_rows(
                    ui,
                    SOURCE_CONTROL_BRANCH_ROW_HEIGHT,
                    row_count,
                    |ui, visible_rows| {
                        for row in source_control_branch_prepared_visible_rows(
                            &branches,
                            visible_rows,
                            renaming,
                        ) {
                            let index = row.row_index();
                            let selected = index == self.source_control_branch_selected;
                            let text = if row.is_current() {
                                RichText::new(row.label()).strong()
                            } else {
                                RichText::new(row.label())
                            };
                            let response = ui.selectable_label(selected, text);
                            if response.clicked() {
                                self.source_control_branch_selected = index;
                                if renaming.is_none() && row.can_switch() {
                                    selected_branch = Some(row.checkout_target());
                                }
                            }
                            response.context_menu(|ui| {
                                if ui.button("Copy Branch Name").clicked() {
                                    copied_branch = Some(row.copy_name());
                                    ui.close();
                                }
                                if row.can_switch() && ui.button("Switch to Branch").clicked() {
                                    selected_branch = Some(row.checkout_target());
                                    ui.close();
                                }
                                if row.can_rename()
                                    && renaming.is_none()
                                    && ui.button("Rename Branch").clicked()
                                {
                                    begin_rename = Some(row.raw_name().to_owned());
                                    ui.close();
                                }
                                if row.can_delete()
                                    && renaming.is_none()
                                    && ui.button("Delete Branch").clicked()
                                {
                                    deleted_branch = Some(row.raw_name().to_owned());
                                    ui.close();
                                }
                            });
                        }
                    },
                );
            }

            ui.separator();
            ui.horizontal(|ui| {
                if let Some(branch) = renaming {
                    ui.label(
                        RichText::new(format!(
                            "Rename {}",
                            source_control_branch_display_name(branch)
                        ))
                        .small(),
                    );
                    if ui
                        .add_enabled(rename_target.is_some(), egui::Button::new("Rename"))
                        .on_hover_text(source_control_branch_rename_tooltip(
                            branch,
                            rename_target.as_deref(),
                            rename_blocked_reason.as_deref(),
                        ))
                        .clicked()
                        && let Some(target) = rename_target.clone()
                    {
                        renamed_branch = Some((branch.to_owned(), target));
                    }
                    if ui.button("Cancel").clicked() {
                        cancel_rename = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{row_count} branches"))
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    });
                    return;
                }
                let selected = selected_row.as_ref();
                let can_switch = selected.is_some_and(|row| row.can_switch());
                if ui
                    .add_enabled(can_switch, egui::Button::new("Switch"))
                    .on_hover_text("Switch to Branch")
                    .clicked()
                    && let Some(row) = selected
                {
                    selected_branch = Some(row.checkout_target());
                }
                if ui
                    .add_enabled(selected.is_some(), egui::Button::new("Copy Name"))
                    .on_hover_text("Copy Branch Name (Alt+C)")
                    .clicked()
                    && let Some(row) = selected
                {
                    copied_branch = Some(row.copy_name());
                }
                if ui
                    .add_enabled(
                        selected.is_some_and(|row| row.can_rename()),
                        egui::Button::new("Rename"),
                    )
                    .on_hover_text(
                        selected
                            .map(SourceControlBranchPreparedRow::rename_action_tooltip)
                            .unwrap_or_else(|| "Select a local branch to rename".to_owned()),
                    )
                    .clicked()
                    && let Some(row) = selected
                {
                    begin_rename = Some(row.raw_name().to_owned());
                }
                if ui
                    .add_enabled(
                        selected.is_some_and(|row| row.can_delete()),
                        egui::Button::new("Delete"),
                    )
                    .on_hover_text(
                        selected
                            .map(SourceControlBranchPreparedRow::delete_tooltip)
                            .unwrap_or("Select a branch to delete"),
                    )
                    .clicked()
                    && let Some(row) = selected
                {
                    deleted_branch = Some(row.raw_name().to_owned());
                }
                if ui
                    .add_enabled(create_branch_name.is_some(), egui::Button::new("Create"))
                    .on_hover_text(
                        create_branch_name
                            .as_deref()
                            .map(source_control_branch_create_tooltip)
                            .unwrap_or_else(|| {
                                create_branch_blocked_reason
                                    .clone()
                                    .unwrap_or_else(|| "Type a new branch name".to_owned())
                            }),
                    )
                    .clicked()
                    && let Some(branch) = create_branch_name.clone()
                {
                    created_branch = Some(branch);
                }
                ui.label(
                    RichText::new(format!("{row_count} branches"))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            });
        });

        if let Some(branch) = copied_branch {
            self.status = copy_branch_name_to_clipboard(ctx, &branch);
        }
        if let Some(branch) = created_branch {
            self.create_git_branch(branch);
        }
        if let Some(branch) = deleted_branch {
            self.delete_git_branch(branch);
        }
        if let Some((old_branch, new_branch)) = renamed_branch {
            self.rename_git_branch(old_branch, new_branch);
        }
        if let Some(branch) = begin_rename {
            self.source_control_branch_rename_from = Some(branch.clone());
            self.source_control_branch_query = branch.clone();
            self.source_control_branch_selected = source_control_branch_index_by_name_and_kind(
                &self.source_control_branches,
                &branch,
                GitCheckoutType::Local,
            )
            .unwrap_or(0);
            self.status = format!(
                "Renaming branch {}",
                source_control_branch_display_name(&branch)
            );
        }
        if cancel_rename {
            self.source_control_branch_rename_from = None;
            self.source_control_branch_query.clear();
            self.source_control_branch_selected = 0;
            self.status = "Canceled branch rename".to_owned();
        }
        if let Some((branch, kind)) = selected_branch {
            self.switch_git_branch(branch, kind);
        }
    }
}

#[cfg(test)]
pub(crate) fn source_control_filtered_branches(
    branches: &[GitBranch],
    query: &str,
) -> Vec<GitBranch> {
    if query.trim().is_empty() {
        return branches.to_vec();
    }

    let terms = query.split_whitespace().collect::<Vec<_>>();
    branches
        .iter()
        .filter(|branch| source_control_branch_matches_query(branch, &terms))
        .cloned()
        .collect()
}

fn source_control_filtered_branch_refs<'a>(
    branches: &[&'a GitBranch],
    query: &str,
) -> Vec<&'a GitBranch> {
    if query.trim().is_empty() {
        return branches.to_vec();
    }

    let terms = query.split_whitespace().collect::<Vec<_>>();
    let mut filtered = Vec::with_capacity(branches.len());
    filtered.extend(
        branches
            .iter()
            .copied()
            .filter(|branch| source_control_branch_matches_query(branch, &terms)),
    );
    filtered
}

pub(crate) fn source_control_branch_selected_identity(
    branches: &[GitBranch],
    query: &str,
    rename_from: Option<&str>,
    sort_order: GitBranchSortOrder,
    selected: usize,
) -> Option<(String, GitCheckoutType)> {
    let displayed = source_control_displayed_branch_refs(branches, query, rename_from, sort_order);
    source_control_branch_ref_at_clamped_index(&displayed, selected)
        .map(|branch| (branch.name.clone(), branch.kind))
}

pub(crate) fn source_control_branch_selection_after_reload(
    branches: &[GitBranch],
    query: &str,
    rename_from: Option<&str>,
    sort_order: GitBranchSortOrder,
    previous_selected: usize,
    selected_branch: Option<(&str, GitCheckoutType)>,
) -> usize {
    let displayed = source_control_displayed_branch_refs(branches, query, rename_from, sort_order);
    if displayed.is_empty() {
        return 0;
    }
    if let Some(rename_from) = rename_from
        && let Some(index) = source_control_branch_ref_index_by_name_and_kind(
            &displayed,
            rename_from,
            GitCheckoutType::Local,
        )
    {
        return index;
    }
    if let Some((selected_name, selected_kind)) = selected_branch
        && let Some(index) = displayed
            .iter()
            .position(|branch| branch.name == selected_name && branch.kind == selected_kind)
    {
        return index;
    }
    previous_selected.min(displayed.len() - 1)
}

fn source_control_branch_ref_at_clamped_index<'a>(
    branches: &[&'a GitBranch],
    selected: usize,
) -> Option<&'a GitBranch> {
    branches
        .get(source_control_branch_clamped_index(
            branches.len(),
            selected,
        )?)
        .copied()
}

fn source_control_branch_clamped_index(len: usize, selected: usize) -> Option<usize> {
    if len == 0 {
        None
    } else {
        Some(selected.min(len - 1))
    }
}

fn source_control_displayed_branch_refs<'a>(
    branches: &'a [GitBranch],
    query: &str,
    rename_from: Option<&str>,
    sort_order: GitBranchSortOrder,
) -> Vec<&'a GitBranch> {
    let sorted_branches = source_control_sorted_branch_refs(branches, sort_order);
    if rename_from.is_some() || query.trim().is_empty() {
        sorted_branches
    } else {
        source_control_filtered_branch_refs(&sorted_branches, query)
    }
}

fn source_control_branch_prepared_row_at<'a>(
    branches: &[&'a GitBranch],
    index: usize,
    renaming: Option<&str>,
) -> Option<SourceControlBranchPreparedRow<'a>> {
    branches
        .get(index)
        .copied()
        .map(|branch| SourceControlBranchPreparedRow::new(index, branch, renaming))
}

fn source_control_branch_prepared_visible_rows<'a, 'b>(
    branches: &'b [&'a GitBranch],
    visible_rows: Range<usize>,
    renaming: Option<&'b str>,
) -> impl ExactSizeIterator<Item = SourceControlBranchPreparedRow<'a>> + 'b
where
    'a: 'b,
{
    let (start, end) = source_control_branch_visible_row_bounds(branches.len(), visible_rows);
    branches[start..end]
        .iter()
        .enumerate()
        .map(move |(offset, branch)| {
            SourceControlBranchPreparedRow::new(start + offset, branch, renaming)
        })
}

fn source_control_branch_visible_row_bounds(
    branch_count: usize,
    rows: Range<usize>,
) -> (usize, usize) {
    let start = rows.start.min(branch_count);
    let end = rows.end.min(branch_count).max(start);
    (start, end)
}

fn source_control_branch_ref_index_by_name(branches: &[&GitBranch], name: &str) -> Option<usize> {
    branches
        .iter()
        .position(|branch| branch.name == name && source_control_branch_can_rename(branch))
}

fn source_control_branch_matches_query(branch: &GitBranch, terms: &[&str]) -> bool {
    terms
        .iter()
        .all(|term| ascii_case_insensitive_contains(&branch.name, term))
}

#[cfg(test)]
pub(crate) fn source_control_sorted_branches(
    branches: &[GitBranch],
    sort_order: GitBranchSortOrder,
) -> Vec<GitBranch> {
    source_control_sorted_branch_refs(branches, sort_order)
        .into_iter()
        .cloned()
        .collect()
}

fn source_control_sorted_branch_refs(
    branches: &[GitBranch],
    sort_order: GitBranchSortOrder,
) -> Vec<&GitBranch> {
    let mut sorted = Vec::with_capacity(branches.len());
    sorted.extend(branches.iter());
    sorted.sort_by(|left, right| {
        right
            .is_current
            .cmp(&left.is_current)
            .then_with(|| match sort_order {
                GitBranchSortOrder::CommitterDate => right
                    .committer_time_seconds
                    .cmp(&left.committer_time_seconds)
                    .then_with(|| left.name.cmp(&right.name)),
                GitBranchSortOrder::Alphabetically => left.name.cmp(&right.name),
            })
    });
    sorted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceControlBranchKeyboardActionKind {
    CopyName,
    Rename,
    Delete,
}

const SOURCE_CONTROL_BRANCH_KEYBOARD_ACTION_KINDS: &[SourceControlBranchKeyboardActionKind] = &[
    SourceControlBranchKeyboardActionKind::CopyName,
    SourceControlBranchKeyboardActionKind::Rename,
    SourceControlBranchKeyboardActionKind::Delete,
];

fn source_control_branch_keyboard_action(
    input: &InputState,
) -> Option<SourceControlBranchKeyboardActionKind> {
    SOURCE_CONTROL_BRANCH_KEYBOARD_ACTION_KINDS
        .iter()
        .copied()
        .find(|action| source_control_branch_keyboard_action_pressed(input, *action))
}

fn source_control_branch_keyboard_action_pressed(
    input: &InputState,
    action: SourceControlBranchKeyboardActionKind,
) -> bool {
    let only_alt = input.modifiers.alt
        && !input.modifiers.ctrl
        && !input.modifiers.command
        && !input.modifiers.mac_cmd
        && !input.modifiers.shift;
    only_alt
        && input.key_pressed(match action {
            SourceControlBranchKeyboardActionKind::CopyName => Key::C,
            SourceControlBranchKeyboardActionKind::Rename => Key::R,
            SourceControlBranchKeyboardActionKind::Delete => Key::D,
        })
}

#[cfg(test)]
pub(crate) fn source_control_branch_keyboard_action_labels() -> Vec<&'static str> {
    SOURCE_CONTROL_BRANCH_KEYBOARD_ACTION_KINDS
        .iter()
        .copied()
        .map(source_control_branch_keyboard_action_label)
        .collect()
}

pub(crate) fn source_control_branch_empty_label(query: &str, loading: bool) -> &'static str {
    if loading {
        "Loading git branches"
    } else if query.trim().is_empty() {
        "No branches found"
    } else {
        "No matching branches"
    }
}

#[cfg(test)]
fn source_control_branch_keyboard_action_label(
    action: SourceControlBranchKeyboardActionKind,
) -> &'static str {
    match action {
        SourceControlBranchKeyboardActionKind::CopyName => "Alt+C Copy Branch Name",
        SourceControlBranchKeyboardActionKind::Rename => "Alt+R Rename Branch",
        SourceControlBranchKeyboardActionKind::Delete => "Alt+D Delete Branch",
    }
}

pub(crate) fn source_control_branch_create_name(
    query: &str,
    branches: &[GitBranch],
    branch_prefix: &str,
    branch_validation_regex: &str,
    branch_whitespace_char: &str,
) -> Option<String> {
    let name = source_control_new_branch_name(query, branch_prefix, branch_whitespace_char)?;
    if branches.iter().any(|branch| branch.name == name)
        || git_branch_validation_error(&name, branch_validation_regex).is_some()
    {
        None
    } else {
        Some(name)
    }
}

pub(crate) fn source_control_branch_create_blocked_reason(
    query: &str,
    branches: &[GitBranch],
    branch_prefix: &str,
    branch_validation_regex: &str,
    branch_whitespace_char: &str,
) -> Option<String> {
    let name = source_control_new_branch_name(query, branch_prefix, branch_whitespace_char)?;
    if branches.iter().any(|branch| branch.name == name) {
        return Some(format!(
            "Branch {} already exists",
            source_control_branch_display_name(&name)
        ));
    }
    git_branch_validation_error(&name, branch_validation_regex)
        .map(|error| source_control_branch_status_detail(&error))
}

pub(crate) fn source_control_new_branch_name(
    query: &str,
    branch_prefix: &str,
    branch_whitespace_char: &str,
) -> Option<String> {
    let query = query.trim();
    let mut terms = query.split_whitespace();
    let first = terms.next()?;
    let whitespace_replacement = branch_whitespace_char.trim();
    let prefix = branch_prefix.trim();
    let mut name = String::with_capacity(query.len() + prefix.len());
    name.push_str(first);
    for term in terms {
        name.push_str(whitespace_replacement);
        name.push_str(term);
    }
    if !prefix.is_empty() && !name.starts_with(prefix) {
        name.insert_str(0, prefix);
    }
    Some(name)
}

fn source_control_branch_single_name_tooltip(prefix: &str, branch: &str) -> String {
    let branch = source_control_branch_tooltip_fragment_cow(branch, prefix.chars().count());
    let mut tooltip = String::with_capacity(prefix.len() + branch.len());
    tooltip.push_str(prefix);
    tooltip.push_str(&branch);
    tooltip
}

fn source_control_branch_pair_tooltip(
    prefix: &str,
    first_branch: &str,
    infix: &str,
    second_branch: &str,
) -> String {
    let fixed_chars = prefix.chars().count() + infix.chars().count();
    let available_chars = SOURCE_CONTROL_BRANCH_TOOLTIP_MAX_CHARS.saturating_sub(fixed_chars);
    let first_max_chars = (available_chars / 2).min(SOURCE_CONTROL_BRANCH_DISPLAY_MAX_CHARS);
    let second_max_chars = available_chars
        .saturating_sub(first_max_chars)
        .min(SOURCE_CONTROL_BRANCH_DISPLAY_MAX_CHARS);
    let first =
        source_control_branch_display_fragment_cow(first_branch, first_max_chars, "unnamed branch");
    let second = source_control_branch_display_fragment_cow(
        second_branch,
        second_max_chars,
        "unnamed branch",
    );
    let mut tooltip =
        String::with_capacity(prefix.len() + first.len() + infix.len() + second.len());
    tooltip.push_str(prefix);
    tooltip.push_str(&first);
    tooltip.push_str(infix);
    tooltip.push_str(&second);
    tooltip
}

fn source_control_branch_tooltip_fragment_cow<'a>(
    branch: &'a str,
    reserved_chars: usize,
) -> Cow<'a, str> {
    source_control_branch_display_fragment_cow(
        branch,
        SOURCE_CONTROL_BRANCH_TOOLTIP_MAX_CHARS
            .saturating_sub(reserved_chars)
            .min(SOURCE_CONTROL_BRANCH_DISPLAY_MAX_CHARS),
        "unnamed branch",
    )
}

pub(crate) fn source_control_branch_create_tooltip(branch: &str) -> String {
    source_control_branch_single_name_tooltip("Create Branch ", branch)
}

pub(crate) fn source_control_branch_rename_target(
    query: &str,
    old_branch: &str,
    branches: &[GitBranch],
    branch_validation_regex: &str,
) -> Option<String> {
    let name = query.trim();
    if !source_control_branch_rename_source_is_valid(old_branch, branches)
        || name.is_empty()
        || name == old_branch
        || branches.iter().any(|branch| branch.name == name)
        || git_branch_validation_error(name, branch_validation_regex).is_some()
    {
        None
    } else {
        Some(name.to_owned())
    }
}

pub(crate) fn source_control_branch_rename_blocked_reason(
    query: &str,
    old_branch: &str,
    branches: &[GitBranch],
    branch_validation_regex: &str,
) -> Option<String> {
    if let Some(reason) = source_control_branch_rename_source_blocked_reason(old_branch, branches) {
        return Some(reason);
    }
    let name = query.trim();
    if name.is_empty() || name == old_branch {
        return None;
    }
    if branches.iter().any(|branch| branch.name == name) {
        return Some(format!(
            "Branch {} already exists",
            source_control_branch_display_name(name)
        ));
    }
    git_branch_validation_error(name, branch_validation_regex)
        .map(|error| source_control_branch_status_detail(&error))
}

pub(crate) fn source_control_branch_rename_tooltip(
    old_branch: &str,
    target: Option<&str>,
    blocked_reason: Option<&str>,
) -> String {
    if let Some(new_branch) = target {
        source_control_branch_pair_tooltip("Rename Branch ", old_branch, " to ", new_branch)
    } else if let Some(blocked_reason) = blocked_reason {
        source_control_branch_status_detail(blocked_reason)
    } else {
        source_control_branch_single_name_tooltip("Type a new branch name for ", old_branch)
    }
}

fn source_control_branch_rename_source_is_valid(old_branch: &str, branches: &[GitBranch]) -> bool {
    branches
        .iter()
        .any(|branch| branch.name == old_branch && source_control_branch_can_rename(branch))
}

fn source_control_branch_rename_source_blocked_reason(
    old_branch: &str,
    branches: &[GitBranch],
) -> Option<String> {
    if source_control_branch_rename_source_is_valid(old_branch, branches) {
        return None;
    }
    if let Some(branch) = branches.iter().find(|branch| branch.name == old_branch) {
        return Some(source_control_branch_rename_action_tooltip(branch));
    }
    Some(format!(
        "Branch {} is no longer available",
        source_control_branch_display_name(old_branch)
    ))
}

pub(crate) fn source_control_branch_can_delete(branch: &GitBranch) -> bool {
    branch.kind == GitCheckoutType::Local && !branch.is_current
}

pub(crate) fn source_control_branch_delete_tooltip(branch: &GitBranch) -> &'static str {
    if branch.kind != GitCheckoutType::Local {
        "Can only delete local branches"
    } else if source_control_branch_can_delete(branch) {
        "Delete Branch (Alt+D)"
    } else {
        "Cannot delete the current branch"
    }
}

pub(crate) fn source_control_branch_can_rename(branch: &GitBranch) -> bool {
    branch.kind == GitCheckoutType::Local
}

pub(crate) fn source_control_branch_can_switch(branch: &GitBranch) -> bool {
    !branch.is_current
}

pub(crate) fn source_control_branch_rename_action_tooltip(branch: &GitBranch) -> String {
    if source_control_branch_can_rename(branch) {
        "Rename Branch (Alt+R)".to_owned()
    } else {
        format!(
            "Can only rename local branches, not {}",
            source_control_branch_kind_label(branch.kind)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceControlBranchNameCopy {
    raw_name: String,
    display_name: String,
}

impl SourceControlBranchNameCopy {
    fn from_branch(branch: &GitBranch) -> Self {
        Self {
            raw_name: branch.name.clone(),
            display_name: source_control_branch_display_name(&branch.name),
        }
    }

    fn status(&self) -> String {
        format!("Copied branch name {}", self.display_name)
    }

    #[cfg(test)]
    fn raw_name(&self) -> &str {
        &self.raw_name
    }
}

#[derive(Debug)]
struct SourceControlBranchPreparedRow<'a> {
    row_index: usize,
    branch: &'a GitBranch,
    label: String,
}

impl<'a> SourceControlBranchPreparedRow<'a> {
    fn new(row_index: usize, branch: &'a GitBranch, renaming: Option<&str>) -> Self {
        Self {
            row_index,
            branch,
            label: source_control_branch_row_label(branch, renaming),
        }
    }

    fn row_index(&self) -> usize {
        self.row_index
    }

    fn raw_name(&self) -> &str {
        &self.branch.name
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn is_current(&self) -> bool {
        self.branch.is_current
    }

    fn can_switch(&self) -> bool {
        source_control_branch_can_switch(self.branch)
    }

    fn can_rename(&self) -> bool {
        source_control_branch_can_rename(self.branch)
    }

    fn can_delete(&self) -> bool {
        source_control_branch_can_delete(self.branch)
    }

    fn checkout_target(&self) -> (String, GitCheckoutType) {
        (self.raw_name().to_owned(), self.branch.kind)
    }

    fn rename_action_tooltip(&self) -> String {
        source_control_branch_rename_action_tooltip(self.branch)
    }

    fn delete_tooltip(&self) -> &'static str {
        source_control_branch_delete_tooltip(self.branch)
    }

    fn copy_name(&self) -> SourceControlBranchNameCopy {
        SourceControlBranchNameCopy::from_branch(self.branch)
    }
}

#[cfg(test)]
pub(crate) fn source_control_branch_label(branch: &GitBranch) -> String {
    SourceControlBranchPreparedRow::new(0, branch, None)
        .label()
        .to_owned()
}

fn source_control_branch_row_label(branch: &GitBranch, renaming: Option<&str>) -> String {
    let suffix = source_control_branch_label_suffix(
        branch,
        renaming == Some(branch.name.as_str()) && source_control_branch_can_rename(branch),
    );
    source_control_branch_label_with_suffix(&branch.name, &suffix)
}

fn source_control_branch_label_with_suffix(branch_name: &str, suffix: &str) -> String {
    let branch_name_max_chars = SOURCE_CONTROL_BRANCH_LABEL_MAX_CHARS
        .saturating_sub(suffix.chars().count())
        .min(SOURCE_CONTROL_BRANCH_DISPLAY_MAX_CHARS);
    let branch_name = source_control_branch_display_fragment_cow(
        branch_name,
        branch_name_max_chars,
        "unnamed branch",
    );
    let mut label = match branch_name {
        Cow::Borrowed(branch_name) => {
            let mut label = String::with_capacity(branch_name.len() + suffix.len());
            label.push_str(branch_name);
            label
        }
        Cow::Owned(label) => label,
    };
    label.push_str(suffix);
    label
}

fn source_control_branch_label_suffix(branch: &GitBranch, renaming: bool) -> String {
    let mut suffix = source_control_branch_status_suffix(branch);
    if renaming {
        suffix.push_str("  renaming");
    }
    suffix
}

fn source_control_branch_status_suffix(branch: &GitBranch) -> String {
    let mut suffix = String::new();
    match (branch.is_current, branch.kind) {
        (false, GitCheckoutType::Local) => {}
        (true, GitCheckoutType::Local) => {
            suffix.push_str("  current");
        }
        (false, kind) => {
            suffix.push_str("  ");
            suffix.push_str(source_control_branch_kind_label(kind));
        }
        (true, kind) => {
            suffix.push_str("  current  ");
            suffix.push_str(source_control_branch_kind_label(kind));
        }
    }
    suffix
}

fn source_control_branch_kind_label(kind: GitCheckoutType) -> &'static str {
    match kind {
        GitCheckoutType::Local => "local",
        GitCheckoutType::Remote => "remote",
        GitCheckoutType::Tags => "tag",
    }
}

fn source_control_branch_index_by_name_and_kind(
    branches: &[GitBranch],
    name: &str,
    kind: GitCheckoutType,
) -> Option<usize> {
    branches
        .iter()
        .position(|branch| branch.name == name && branch.kind == kind)
}

fn source_control_branch_ref_index_by_name_and_kind(
    branches: &[&GitBranch],
    name: &str,
    kind: GitCheckoutType,
) -> Option<usize> {
    branches
        .iter()
        .position(|branch| branch.name == name && branch.kind == kind)
}

fn copy_branch_name_to_clipboard(ctx: &Context, branch: &SourceControlBranchNameCopy) -> String {
    ctx.copy_text(branch.raw_name.clone());
    branch.status()
}

#[cfg(test)]
pub(crate) fn source_control_branch_copy_text(branch: &GitBranch) -> String {
    branch.name.clone()
}

#[cfg(test)]
pub(crate) fn source_control_branch_copy_status(branch: &GitBranch) -> String {
    SourceControlBranchNameCopy::from_branch(branch).status()
}

pub(crate) fn source_control_branch_display_name(value: &str) -> String {
    source_control_branch_display_fragment(
        value,
        SOURCE_CONTROL_BRANCH_DISPLAY_MAX_CHARS,
        "unnamed branch",
    )
}

pub(crate) fn source_control_branch_status_detail(value: &str) -> String {
    source_control_branch_display_fragment(
        value,
        SOURCE_CONTROL_BRANCH_STATUS_DETAIL_MAX_CHARS,
        "unknown error",
    )
}

fn source_control_branch_display_fragment(value: &str, max_chars: usize, fallback: &str) -> String {
    source_control_branch_display_fragment_cow(value, max_chars, fallback).into_owned()
}

fn source_control_branch_display_fragment_cow<'a>(
    value: &'a str,
    max_chars: usize,
    fallback: &str,
) -> Cow<'a, str> {
    sanitized_display_label_cow(value, max_chars, fallback)
}

#[cfg(test)]
mod tests;

pub(crate) use crate::source_control_history_runtime::source_control_commit_age_label_at;

mod labels;

use crate::source_control_history_runtime::{
    source_control_filtered_history_indices, source_control_history_can_load_more,
    source_control_history_should_page_on_scroll,
};
use crate::{
    KuroyaApp,
    source_control_git_panel_ui::{
        SOURCE_CONTROL_GIT_ROW_HEIGHT, apply_git_panel_spacing, render_git_panel_row,
    },
    ui_state::{handle_list_navigation_keys, selected_row_scroll_offset, selection_page_step},
};
use eframe::egui::{self, Context, InputState, Key, RichText, ScrollArea, TextEdit};
use kuroya_core::GitCommitSummary;
use labels::SourceControlHistoryRowDisplay;
#[cfg(test)]
use labels::{
    SOURCE_CONTROL_HISTORY_COMMIT_ID_DISPLAY_MAX_CHARS,
    SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS,
    SOURCE_CONTROL_HISTORY_ROW_LABEL_MAX_CHARS, source_control_commit_display_field,
    source_control_commit_display_field_text, source_control_commit_id_display,
    source_control_commit_id_display_text, source_control_commit_summary_display,
    source_control_history_display_sample,
};
pub(crate) use labels::{
    SourceControlCommitCopyKind, source_control_commit_copy_status,
    source_control_commit_copy_text_at, source_control_graph_divergence_label,
};
#[cfg(test)]
pub(crate) use labels::{source_control_commit_copy_text, source_control_commit_label};
use std::{
    ops::Range,
    time::{SystemTime, UNIX_EPOCH},
};

impl KuroyaApp {
    pub(crate) fn render_git_history_panel(&mut self, ctx: &Context) {
        let mut close = false;
        let mut open_commit_target = None;
        let mut copy_patch_target = None;
        let now_seconds = current_unix_seconds();
        let commit_indices = source_control_filtered_history_indices(
            &self.source_control_history,
            &self.source_control_history_query,
            now_seconds,
        );
        if self.source_control_history_selected >= commit_indices.len() {
            self.source_control_history_selected = commit_indices.len().saturating_sub(1);
        }

        // Full-page layout: the panel fills the viewport (inside margins) so
        // long commit lists stay readable. The filter field and the commit
        // list expand to the fixed window; the list scrolls internally.
        egui::Window::new("Git History")
            .id(egui::Id::new((
                "git_history_panel",
                self.git_panel_open_generation,
            )))
            .fixed_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 84.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 84.0])
            .show(ctx, |ui| {
                apply_git_panel_spacing(ui);
                ui.horizontal(|ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.source_control_history_query)
                            .hint_text("Filter commits")
                            .desired_width(f32::INFINITY),
                    );
                    response.request_focus();
                    if response.changed() {
                        self.source_control_history_selected = 0;
                    }
                    if let Some(label) = source_control_graph_divergence_label(
                        self.git.remote_divergence(),
                        self.settings.scm_graph_show_incoming_changes,
                        self.settings.scm_graph_show_outgoing_changes,
                    ) {
                        ui.label(RichText::new(label).small()).on_hover_text(
                            "Incoming and outgoing changes against the upstream branch",
                        );
                    }
                    if ui.button("Refresh").clicked() {
                        self.begin_git_history_panel();
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
                let viewport_height = ui.available_height();
                let selection_changed = ui.input(|input| {
                    handle_list_navigation_keys(
                        input,
                        &mut self.source_control_history_selected,
                        commit_indices.len(),
                        selection_page_step(SOURCE_CONTROL_GIT_ROW_HEIGHT, viewport_height),
                    )
                });
                let selected_entry = selected_history_entry(
                    &self.source_control_history,
                    &commit_indices,
                    self.source_control_history_selected,
                );
                if ui.input(|input| input.key_pressed(Key::Enter))
                    && let Some(selected) = selected_entry.as_ref()
                {
                    open_commit_target = Some(selected.target());
                }
                if let Some(action) = ui.input(source_control_history_keyboard_action) {
                    match action {
                        SourceControlHistoryKeyboardActionKind::Patch => {
                            if let Some(selected) = selected_entry.as_ref() {
                                copy_patch_target = Some(selected.target());
                            }
                        }
                        SourceControlHistoryKeyboardActionKind::CommitId => {
                            if let Some(selected) = selected_entry.as_ref() {
                                self.status = copy_commit_to_clipboard(
                                    ui.ctx(),
                                    selected.commit(),
                                    SourceControlCommitCopyKind::Oid,
                                    now_seconds,
                                );
                            }
                        }
                        SourceControlHistoryKeyboardActionKind::ShortCommitId => {
                            if let Some(selected) = selected_entry.as_ref() {
                                self.status = copy_commit_to_clipboard(
                                    ui.ctx(),
                                    selected.commit(),
                                    SourceControlCommitCopyKind::ShortOid,
                                    now_seconds,
                                );
                            }
                        }
                        SourceControlHistoryKeyboardActionKind::CommitMessage => {
                            if let Some(selected) = selected_entry.as_ref() {
                                self.status = copy_commit_to_clipboard(
                                    ui.ctx(),
                                    selected.commit(),
                                    SourceControlCommitCopyKind::Summary,
                                    now_seconds,
                                );
                            }
                        }
                        SourceControlHistoryKeyboardActionKind::CommitAuthor => {
                            if let Some(selected) = selected_entry.as_ref() {
                                self.status = copy_commit_to_clipboard(
                                    ui.ctx(),
                                    selected.commit(),
                                    SourceControlCommitCopyKind::Author,
                                    now_seconds,
                                );
                            }
                        }
                        SourceControlHistoryKeyboardActionKind::CommitAge => {
                            if let Some(selected) = selected_entry.as_ref() {
                                self.status = copy_commit_to_clipboard(
                                    ui.ctx(),
                                    selected.commit(),
                                    SourceControlCommitCopyKind::Age,
                                    now_seconds,
                                );
                            }
                        }
                    }
                }

                ui.separator();
                if commit_indices.is_empty() {
                    ui.label(RichText::new("No matching commits").small());
                } else {
                    let mut scroll_area = ScrollArea::vertical().auto_shrink([false, false]);
                    if selection_changed {
                        scroll_area =
                            scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                                self.source_control_history_selected,
                                commit_indices.len(),
                                SOURCE_CONTROL_GIT_ROW_HEIGHT,
                                viewport_height,
                            ));
                    }
                    let history_scroll = scroll_area.show_rows(
                        ui,
                        SOURCE_CONTROL_GIT_ROW_HEIGHT,
                        commit_indices.len(),
                        |ui, rows| {
                            let visible_rows = source_control_history_prepared_visible_rows(
                                &self.source_control_history,
                                &commit_indices,
                                rows,
                                now_seconds,
                                self.settings.git_timeline_show_author,
                            );
                            for mut row in visible_rows {
                                let index = row.row_index();
                                let selected = index == self.source_control_history_selected;
                                let response = render_git_panel_row(
                                    ui,
                                    selected,
                                    row.short_oid(),
                                    row.summary(),
                                    row.detail(),
                                )
                                .on_hover_ui(|ui| {
                                    ui.label(row.tooltip());
                                });
                                if response.clicked() {
                                    self.source_control_history_selected = index;
                                }
                                if response.double_clicked() {
                                    open_commit_target = Some(row.target());
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Open Changes").clicked() {
                                        open_commit_target = Some(row.target());
                                        ui.close();
                                    }
                                    if ui.button("Copy Patch").clicked() {
                                        copy_patch_target = Some(row.target());
                                        ui.close();
                                    }
                                    if ui.button("Copy Commit ID").clicked() {
                                        self.status = copy_commit_to_clipboard_with_prepared_row(
                                            ui.ctx(),
                                            &row,
                                            SourceControlCommitCopyKind::Oid,
                                            now_seconds,
                                        );
                                        ui.close();
                                    }
                                    if ui.button("Copy Short Commit ID").clicked() {
                                        self.status = copy_commit_to_clipboard_with_prepared_row(
                                            ui.ctx(),
                                            &row,
                                            SourceControlCommitCopyKind::ShortOid,
                                            now_seconds,
                                        );
                                        ui.close();
                                    }
                                    if ui.button("Copy Commit Message").clicked() {
                                        self.status = copy_commit_to_clipboard_with_prepared_row(
                                            ui.ctx(),
                                            &row,
                                            SourceControlCommitCopyKind::Summary,
                                            now_seconds,
                                        );
                                        ui.close();
                                    }
                                    if ui.button("Copy Commit Author").clicked() {
                                        self.status = copy_commit_to_clipboard_with_prepared_row(
                                            ui.ctx(),
                                            &row,
                                            SourceControlCommitCopyKind::Author,
                                            now_seconds,
                                        );
                                        ui.close();
                                    }
                                    if ui.button("Copy Commit Age").clicked() {
                                        self.status = copy_commit_to_clipboard_with_prepared_row(
                                            ui.ctx(),
                                            &row,
                                            SourceControlCommitCopyKind::Age,
                                            now_seconds,
                                        );
                                        ui.close();
                                    }
                                });
                            }
                        },
                    );
                    if source_control_history_should_page_on_scroll(
                        self.settings.scm_graph_page_on_scroll,
                        self.source_control_history_loading,
                        self.source_control_history_has_more,
                        history_scroll.state.offset.y,
                        history_scroll.inner_rect.height(),
                        history_scroll.content_size.y,
                    ) {
                        self.request_more_git_history();
                    }
                }

                ui.horizontal(|ui| {
                    let selected_entry = selected_history_entry(
                        &self.source_control_history,
                        &commit_indices,
                        self.source_control_history_selected,
                    );
                    if ui
                        .add_enabled(
                            !commit_indices.is_empty(),
                            egui::Button::new("Open Changes"),
                        )
                        .clicked()
                        && let Some(selected) = selected_entry.as_ref()
                    {
                        open_commit_target = Some(selected.target());
                    }
                    if ui
                        .add_enabled(selected_entry.is_some(), egui::Button::new("Copy Patch"))
                        .on_hover_text("Copy Commit Patch (Alt+P)")
                        .clicked()
                        && let Some(selected) = selected_entry.as_ref()
                    {
                        copy_patch_target = Some(selected.target());
                    }
                    if ui
                        .add_enabled(selected_entry.is_some(), egui::Button::new("Copy ID"))
                        .on_hover_text("Copy Commit ID (Alt+I)")
                        .clicked()
                        && let Some(selected) = selected_entry.as_ref()
                    {
                        self.status = copy_commit_to_clipboard(
                            ui.ctx(),
                            selected.commit(),
                            SourceControlCommitCopyKind::Oid,
                            now_seconds,
                        );
                    }
                    if ui
                        .add_enabled(selected_entry.is_some(), egui::Button::new("Copy Short ID"))
                        .on_hover_text("Copy Short Commit ID (Alt+S)")
                        .clicked()
                        && let Some(selected) = selected_entry.as_ref()
                    {
                        self.status = copy_commit_to_clipboard(
                            ui.ctx(),
                            selected.commit(),
                            SourceControlCommitCopyKind::ShortOid,
                            now_seconds,
                        );
                    }
                    if ui
                        .add_enabled(selected_entry.is_some(), egui::Button::new("Copy Message"))
                        .on_hover_text("Copy Commit Message (Alt+M)")
                        .clicked()
                        && let Some(selected) = selected_entry.as_ref()
                    {
                        self.status = copy_commit_to_clipboard(
                            ui.ctx(),
                            selected.commit(),
                            SourceControlCommitCopyKind::Summary,
                            now_seconds,
                        );
                    }
                    if ui
                        .add_enabled(selected_entry.is_some(), egui::Button::new("Copy Author"))
                        .on_hover_text("Copy Commit Author (Alt+A)")
                        .clicked()
                        && let Some(selected) = selected_entry.as_ref()
                    {
                        self.status = copy_commit_to_clipboard(
                            ui.ctx(),
                            selected.commit(),
                            SourceControlCommitCopyKind::Author,
                            now_seconds,
                        );
                    }
                    if ui
                        .add_enabled(selected_entry.is_some(), egui::Button::new("Copy Age"))
                        .on_hover_text("Copy Commit Age (Alt+T)")
                        .clicked()
                        && let Some(selected) = selected_entry.as_ref()
                    {
                        self.status = copy_commit_to_clipboard(
                            ui.ctx(),
                            selected.commit(),
                            SourceControlCommitCopyKind::Age,
                            now_seconds,
                        );
                    }
                    ui.label(
                        RichText::new(format!("{} commits", commit_indices.len()))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                    if self.source_control_history_loading {
                        ui.label(
                            RichText::new("Loading")
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                    } else if !self.settings.scm_graph_page_on_scroll
                        && source_control_history_can_load_more(
                            self.source_control_history_loading,
                            self.source_control_history_has_more,
                        )
                        && ui.button("Load More").clicked()
                    {
                        self.request_more_git_history();
                    }
                });
            });

        if close {
            self.source_control_history_open = false;
            self.source_control_history_query.clear();
            self.status = "Closed git history".to_owned();
        }
        if let Some(commit) = copy_patch_target
            .and_then(|target| {
                source_control_history_commit_for_action_target(
                    &self.source_control_history,
                    &target,
                )
            })
            .cloned()
        {
            self.copy_commit_patch(ctx, &commit);
        }
        if let Some(commit) = open_commit_target
            .and_then(|target| {
                source_control_history_commit_for_action_target(
                    &self.source_control_history,
                    &target,
                )
            })
            .cloned()
        {
            self.open_commit_changes(commit);
        }
    }
}

#[cfg(test)]
pub(crate) fn source_control_filtered_history(
    commits: &[GitCommitSummary],
    query: &str,
    now_seconds: i64,
) -> Vec<GitCommitSummary> {
    source_control_filtered_history_indices(commits, query, now_seconds)
        .into_iter()
        .filter_map(|index| commits.get(index))
        .cloned()
        .collect()
}

#[cfg(test)]
fn selected_history_commit<'a>(
    commits: &'a [GitCommitSummary],
    indices: &[usize],
    selected: usize,
) -> Option<&'a GitCommitSummary> {
    selected_history_entry(commits, indices, selected).map(|entry| entry.commit())
}

fn selected_history_commit_index(indices: &[usize], selected: usize) -> Option<usize> {
    indices.get(selected).copied()
}

fn selected_history_entry<'a>(
    commits: &'a [GitCommitSummary],
    indices: &[usize],
    selected: usize,
) -> Option<SourceControlHistorySelectedEntry<'a>> {
    let history_index = selected_history_commit_index(indices, selected)?;
    let commit = commits.get(history_index)?;
    Some(SourceControlHistorySelectedEntry::new(
        history_index,
        commit,
    ))
}

#[cfg(test)]
fn selected_history_action_target(
    commits: &[GitCommitSummary],
    indices: &[usize],
    selected: usize,
) -> Option<SourceControlHistoryActionTarget> {
    selected_history_entry(commits, indices, selected).map(|entry| entry.target())
}

#[derive(Debug, Clone, Copy)]
struct SourceControlHistorySelectedEntry<'a> {
    history_index: usize,
    commit: &'a GitCommitSummary,
}

impl<'a> SourceControlHistorySelectedEntry<'a> {
    fn new(history_index: usize, commit: &'a GitCommitSummary) -> Self {
        Self {
            history_index,
            commit,
        }
    }

    #[cfg(test)]
    fn history_index(&self) -> usize {
        self.history_index
    }

    fn commit(&self) -> &'a GitCommitSummary {
        self.commit
    }

    fn target(&self) -> SourceControlHistoryActionTarget {
        SourceControlHistoryActionTarget::new(self.history_index, self.commit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceControlHistoryActionTarget {
    history_index: usize,
    oid: String,
}

impl SourceControlHistoryActionTarget {
    fn new(history_index: usize, commit: &GitCommitSummary) -> Self {
        Self {
            history_index,
            oid: commit.oid.clone(),
        }
    }
}

fn source_control_history_commit_for_action_target<'a>(
    commits: &'a [GitCommitSummary],
    target: &SourceControlHistoryActionTarget,
) -> Option<&'a GitCommitSummary> {
    if let Some(commit) = commits
        .get(target.history_index)
        .filter(|commit| source_control_history_commit_matches_action_target(commit, target))
    {
        if source_control_history_action_target_has_duplicate_match(
            commits,
            target,
            target.history_index,
        ) {
            return None;
        }
        return Some(commit);
    }
    source_control_history_unique_commit_for_action_target(commits, target)
}

fn source_control_history_commit_matches_action_target(
    commit: &GitCommitSummary,
    target: &SourceControlHistoryActionTarget,
) -> bool {
    commit.oid.as_str() == target.oid.as_str()
}

fn source_control_history_action_target_has_duplicate_match(
    commits: &[GitCommitSummary],
    target: &SourceControlHistoryActionTarget,
    matched_index: usize,
) -> bool {
    commits.iter().enumerate().any(|(index, commit)| {
        index != matched_index
            && source_control_history_commit_matches_action_target(commit, target)
    })
}

fn source_control_history_unique_commit_for_action_target<'a>(
    commits: &'a [GitCommitSummary],
    target: &SourceControlHistoryActionTarget,
) -> Option<&'a GitCommitSummary> {
    let mut matches = commits
        .iter()
        .filter(|commit| source_control_history_commit_matches_action_target(commit, target));
    let commit = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(commit)
}

fn source_control_history_prepared_visible_rows<'a>(
    commits: &'a [GitCommitSummary],
    indices: &'a [usize],
    rows: Range<usize>,
    now_seconds: i64,
    show_author: bool,
) -> impl Iterator<Item = SourceControlHistoryPreparedRow<'a>> + 'a {
    let start = rows.start.min(indices.len());
    let end = rows.end.min(indices.len()).max(start);
    indices[start..end]
        .iter()
        .copied()
        .enumerate()
        .filter_map(move |(offset, history_index)| {
            let row_index = start + offset;
            let commit = commits.get(history_index)?;
            Some(SourceControlHistoryPreparedRow::new(
                row_index,
                history_index,
                commit,
                now_seconds,
                show_author,
            ))
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceControlHistoryKeyboardActionKind {
    Patch,
    CommitId,
    ShortCommitId,
    CommitMessage,
    CommitAuthor,
    CommitAge,
}

fn source_control_history_keyboard_action(
    input: &InputState,
) -> Option<SourceControlHistoryKeyboardActionKind> {
    source_control_history_keyboard_action_kinds()
        .iter()
        .copied()
        .find(|action| source_control_history_keyboard_action_pressed(input, *action))
}

fn source_control_history_keyboard_action_pressed(
    input: &InputState,
    action: SourceControlHistoryKeyboardActionKind,
) -> bool {
    let only_alt = input.modifiers.alt
        && !input.modifiers.ctrl
        && !input.modifiers.command
        && !input.modifiers.mac_cmd
        && !input.modifiers.shift;
    only_alt
        && input.key_pressed(match action {
            SourceControlHistoryKeyboardActionKind::Patch => Key::P,
            SourceControlHistoryKeyboardActionKind::CommitId => Key::I,
            SourceControlHistoryKeyboardActionKind::ShortCommitId => Key::S,
            SourceControlHistoryKeyboardActionKind::CommitMessage => Key::M,
            SourceControlHistoryKeyboardActionKind::CommitAuthor => Key::A,
            SourceControlHistoryKeyboardActionKind::CommitAge => Key::T,
        })
}

fn source_control_history_keyboard_action_kinds()
-> &'static [SourceControlHistoryKeyboardActionKind] {
    &[
        SourceControlHistoryKeyboardActionKind::Patch,
        SourceControlHistoryKeyboardActionKind::CommitId,
        SourceControlHistoryKeyboardActionKind::ShortCommitId,
        SourceControlHistoryKeyboardActionKind::CommitMessage,
        SourceControlHistoryKeyboardActionKind::CommitAuthor,
        SourceControlHistoryKeyboardActionKind::CommitAge,
    ]
}

#[cfg(test)]
pub(crate) fn source_control_history_keyboard_action_labels() -> Vec<&'static str> {
    source_control_history_keyboard_action_kinds()
        .iter()
        .copied()
        .map(source_control_history_keyboard_action_label)
        .collect()
}

#[cfg(test)]
fn source_control_history_keyboard_action_label(
    action: SourceControlHistoryKeyboardActionKind,
) -> &'static str {
    match action {
        SourceControlHistoryKeyboardActionKind::Patch => "Alt+P Copy Patch",
        SourceControlHistoryKeyboardActionKind::CommitId => "Alt+I Copy Commit ID",
        SourceControlHistoryKeyboardActionKind::ShortCommitId => "Alt+S Copy Short Commit ID",
        SourceControlHistoryKeyboardActionKind::CommitMessage => "Alt+M Copy Commit Message",
        SourceControlHistoryKeyboardActionKind::CommitAuthor => "Alt+A Copy Commit Author",
        SourceControlHistoryKeyboardActionKind::CommitAge => "Alt+T Copy Commit Age",
    }
}

#[derive(Debug, Clone)]
struct SourceControlHistoryPreparedRow<'a> {
    row_index: usize,
    history_index: usize,
    commit: &'a GitCommitSummary,
    display: SourceControlHistoryRowDisplay<'a>,
}

impl<'a> SourceControlHistoryPreparedRow<'a> {
    fn new(
        row_index: usize,
        history_index: usize,
        commit: &'a GitCommitSummary,
        now_seconds: i64,
        show_author: bool,
    ) -> Self {
        Self {
            row_index,
            history_index,
            commit,
            display: SourceControlHistoryRowDisplay::new(commit, now_seconds, show_author),
        }
    }

    fn row_index(&self) -> usize {
        self.row_index
    }

    #[cfg(test)]
    fn history_index(&self) -> usize {
        self.history_index
    }

    fn target(&self) -> SourceControlHistoryActionTarget {
        SourceControlHistoryActionTarget::new(self.history_index, self.commit)
    }

    fn commit(&self) -> &'a GitCommitSummary {
        self.commit
    }

    #[cfg(test)]
    fn label(&self) -> &str {
        self.display.label()
    }

    fn short_oid(&self) -> &str {
        self.display.short_oid()
    }

    fn summary(&self) -> &str {
        self.display.summary()
    }

    fn detail(&self) -> &str {
        self.display.detail()
    }

    fn tooltip(&mut self) -> &str {
        self.display.tooltip(self.commit)
    }

    fn copy_status(&self, kind: SourceControlCommitCopyKind) -> String {
        self.display.copy_status(kind)
    }
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

fn copy_commit_to_clipboard(
    ctx: &Context,
    commit: &GitCommitSummary,
    kind: SourceControlCommitCopyKind,
    now_seconds: i64,
) -> String {
    let text = source_control_commit_copy_text_at(commit, kind, now_seconds);
    ctx.copy_text(text);
    source_control_commit_copy_status(commit, kind)
}

fn copy_commit_to_clipboard_with_prepared_row(
    ctx: &Context,
    row: &SourceControlHistoryPreparedRow<'_>,
    kind: SourceControlCommitCopyKind,
    now_seconds: i64,
) -> String {
    let commit = row.commit();
    let text = source_control_commit_copy_text_at(commit, kind, now_seconds);
    ctx.copy_text(text);
    row.copy_status(kind)
}

#[cfg(test)]
mod tests {
    use super::{
        SOURCE_CONTROL_HISTORY_COMMIT_ID_DISPLAY_MAX_CHARS,
        SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS,
        SOURCE_CONTROL_HISTORY_ROW_LABEL_MAX_CHARS, SourceControlCommitCopyKind,
        SourceControlHistoryActionTarget, SourceControlHistoryRowDisplay,
        selected_history_action_target, selected_history_commit, selected_history_entry,
        source_control_commit_copy_status, source_control_commit_copy_text,
        source_control_commit_display_field, source_control_commit_display_field_text,
        source_control_commit_id_display, source_control_commit_id_display_text,
        source_control_commit_label, source_control_commit_summary_display,
        source_control_filtered_history, source_control_filtered_history_indices,
        source_control_history_commit_for_action_target, source_control_history_display_sample,
        source_control_history_prepared_visible_rows,
    };
    use kuroya_core::GitCommitSummary;
    use std::borrow::Cow;

    fn commit(short_oid: &str, summary: &str, author: &str, time_seconds: i64) -> GitCommitSummary {
        GitCommitSummary {
            oid: format!("{short_oid}aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            short_oid: short_oid.to_owned(),
            summary: summary.to_owned(),
            author: author.to_owned(),
            time_seconds,
        }
    }

    #[test]
    fn history_panel_filters_to_indices_without_cloning_commits() {
        let commits = vec![
            commit("12345678", "Add search panel", "Kuroya Test", 10),
            commit(
                "abcdef12",
                "Fix terminal scrollback",
                "Another Author",
                2000,
            ),
        ];

        let filtered = source_control_filtered_history_indices(&commits, "search kuroya", 3700);

        assert_eq!(filtered, vec![0]);
        assert_eq!(
            source_control_filtered_history(&commits, "search kuroya", 3700)[0].short_oid,
            "12345678"
        );
    }

    fn assert_history_display_text_is_safe(value: &str) {
        assert!(
            !value.chars().any(is_unsafe_history_display_char),
            "display text contains unsafe characters: {value:?}"
        );
    }

    fn is_unsafe_history_display_char(ch: char) -> bool {
        ch.is_control()
            || matches!(
                ch,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{2028}'
                    | '\u{2029}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    }

    #[test]
    fn history_commit_label_sanitizes_and_bounds_display_fields() {
        let commit = commit(
            &format!("12\u{202e}\n34{}", "a".repeat(120)),
            &format!("Add\u{2066}\nsearch{}", "b".repeat(240)),
            &format!("Kuroya\u{200f}\rTest{}", "c".repeat(180)),
            1000,
        );

        let label = source_control_commit_label(&commit, 3700, true);

        assert!(label.starts_with("12 34"));
        assert!(label.contains("Add search"));
        assert!(label.contains("Kuroya Test"));
        assert!(label.contains("..."));
        assert_history_display_text_is_safe(&label);
        assert!(label.chars().count() <= SOURCE_CONTROL_HISTORY_ROW_LABEL_MAX_CHARS);
    }

    #[test]
    fn history_row_display_lazily_reuses_sanitized_fields_for_tooltip_and_status() {
        let commit = commit(
            &format!("12\u{202e}\n34{}", "a".repeat(120)),
            &format!("Add\u{2066}\nsearch{}", "b".repeat(240)),
            &format!("Kuroya\u{200f}\rTest{}", "c".repeat(180)),
            1000,
        );

        let mut display = SourceControlHistoryRowDisplay::new(&commit, 3700, false);
        assert!(display.tooltip.is_none());
        assert_eq!(display.label_age(), "45m ago");
        assert_eq!(
            display
                .label()
                .get(display.label_age_start.expect("label age start")..),
            Some("45m ago")
        );
        let status = display.copy_status(SourceControlCommitCopyKind::Summary);
        assert!(display.tooltip.is_none());

        assert!(display.label().starts_with("12 34"));
        assert!(display.label().contains("Add search"));
        assert!(!display.label().contains("Kuroya Test"));
        assert_history_display_text_is_safe(display.label());

        let tooltip = display.tooltip(&commit);
        let tooltip_ptr = tooltip.as_ptr();
        let tooltip_len = tooltip.len();
        let tooltip = tooltip.to_owned();
        let tooltip_again = display.tooltip(&commit);
        assert!(tooltip.contains("Kuroya Test"));
        assert!(tooltip.contains("..."));
        assert_history_display_text_is_safe(&tooltip);
        assert!(tooltip.chars().count() <= SOURCE_CONTROL_HISTORY_ROW_LABEL_MAX_CHARS);
        assert!(std::ptr::eq(tooltip_ptr, tooltip_again.as_ptr()));
        assert_eq!(tooltip_len, tooltip_again.len());
        assert_eq!(
            status,
            source_control_commit_copy_status(&commit, SourceControlCommitCopyKind::Summary)
        );
        assert_history_display_text_is_safe(&status);
        assert_eq!(
            source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::Oid),
            commit.oid.as_str()
        );
        assert_eq!(
            source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::ShortOid),
            commit.short_oid.as_str()
        );
    }

    #[test]
    fn history_row_display_uses_label_as_tooltip_when_author_is_visible() {
        let commit = commit("12345678", "Add search panel", "Kuroya Test", 1000);

        let mut display = SourceControlHistoryRowDisplay::new(&commit, 3700, true);
        let label = display.label().to_owned();
        let label_ptr = display.label().as_ptr();

        assert!(display.label_age_start.is_none());
        assert_eq!(display.label_age(), "");
        assert_eq!(display.tooltip(&commit), label);
        assert!(std::ptr::eq(display.tooltip(&commit).as_ptr(), label_ptr));
    }

    #[test]
    fn history_row_display_borrows_clean_cached_fields() {
        let commit = commit("12345678", "Add search panel", "Kuroya Test", 1000);

        let display = SourceControlHistoryRowDisplay::new(&commit, 3700, false);

        assert!(matches!(&display.short_oid, Cow::Borrowed("12345678")));
        assert!(matches!(
            &display.summary,
            Cow::Borrowed("Add search panel")
        ));
        assert!(display.label().contains("12345678"));
        assert!(display.label().contains("Add search panel"));
    }

    #[test]
    fn history_prepared_visible_rows_skip_invalid_indices_and_cache_labels() {
        let commits = vec![
            commit("12345678", "Add search panel", "Kuroya Test", 1000),
            commit(
                "abcdef12",
                "Fix terminal scrollback",
                "Another Author",
                2000,
            ),
        ];
        let commit_indices = vec![1, usize::MAX, 0, 99];

        let rows: Vec<_> = source_control_history_prepared_visible_rows(
            &commits,
            &commit_indices,
            0..4,
            3700,
            true,
        )
        .collect();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].row_index(), 0);
        assert_eq!(rows[0].history_index(), 1);
        assert_eq!(rows[0].commit().short_oid, "abcdef12");
        assert_eq!(
            rows[0].label(),
            "abcdef12  Fix terminal scrollback  Another Author  28m ago"
        );
        assert_eq!(rows[1].row_index(), 2);
        assert_eq!(rows[1].history_index(), 0);
        assert_eq!(rows[1].commit().short_oid, "12345678");

        let label = rows[0].label();
        let label_ptr = label.as_ptr();
        let label_len = label.len();
        assert_eq!(rows[0].label().as_ptr(), label_ptr);
        assert_eq!(rows[0].label().len(), label_len);
    }

    #[test]
    fn history_prepared_visible_rows_clamps_stale_ranges() {
        let commits = vec![
            commit("12345678", "Add search panel", "Kuroya Test", 1000),
            commit(
                "abcdef12",
                "Fix terminal scrollback",
                "Another Author",
                2000,
            ),
        ];
        let commit_indices = vec![0, 1];

        let rows: Vec<_> = source_control_history_prepared_visible_rows(
            &commits,
            &commit_indices,
            1..usize::MAX,
            3700,
            true,
        )
        .collect();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].row_index(), 1);
        assert_eq!(rows[0].history_index(), 1);
        assert_eq!(rows[0].commit().short_oid, "abcdef12");
        let stale_start = commit_indices.len();
        let stale_end = stale_start.saturating_sub(1);
        assert!(
            source_control_history_prepared_visible_rows(
                &commits,
                &commit_indices,
                stale_start..stale_end,
                3700,
                true,
            )
            .next()
            .is_none()
        );
    }

    #[test]
    fn history_action_target_rejects_stale_commit_at_same_index() {
        let current = commit("12345678", "Current commit", "Kuroya Test", 1000);
        let replaced = commit("abcdef12", "Replacement commit", "Kuroya Test", 2000);
        let commits = vec![current.clone()];
        let target = SourceControlHistoryActionTarget::new(0, &current);

        assert_eq!(
            source_control_history_commit_for_action_target(&commits, &target)
                .map(|commit| commit.short_oid.as_str()),
            Some("12345678")
        );
        assert!(source_control_history_commit_for_action_target(&[replaced], &target).is_none());
    }

    #[test]
    fn history_action_target_resolves_unique_commit_after_index_shift() {
        let current = commit("12345678", "Current commit", "Kuroya Test", 1000);
        let inserted = commit("abcdef12", "Inserted commit", "Another Author", 2000);
        let target = SourceControlHistoryActionTarget::new(0, &current);
        let refreshed = vec![inserted, current];

        assert_eq!(
            source_control_history_commit_for_action_target(&refreshed, &target)
                .map(|commit| commit.short_oid.as_str()),
            Some("12345678")
        );
    }

    #[test]
    fn history_action_target_rejects_ambiguous_commit_after_index_shift() {
        let current = commit("12345678", "Current commit", "Kuroya Test", 1000);
        let duplicate = commit("12345678", "Duplicate commit", "Another Author", 2000);
        let inserted = commit("abcdef12", "Inserted commit", "Another Author", 3000);
        let target = SourceControlHistoryActionTarget::new(0, &current);
        let refreshed = vec![inserted, current, duplicate];

        assert!(source_control_history_commit_for_action_target(&refreshed, &target).is_none());
    }

    #[test]
    fn history_action_target_rejects_duplicated_oid_at_original_index() {
        let current = commit("12345678", "Current commit", "Kuroya Test", 1000);
        let duplicate = commit("12345678", "Duplicate commit", "Another Author", 2000);
        let target = SourceControlHistoryActionTarget::new(0, &current);
        let refreshed = vec![current, duplicate];

        assert!(source_control_history_commit_for_action_target(&refreshed, &target).is_none());
    }

    #[test]
    fn selected_history_action_target_preserves_raw_commit_fields() {
        let raw_short_oid = format!("12\u{202e}\n34{}", "a".repeat(120));
        let raw_summary = format!("Add\u{2066}\nsearch{}", "b".repeat(240));
        let raw_author = format!("Kuroya\u{200f}\rTest{}", "c".repeat(180));
        let commit = commit(&raw_short_oid, &raw_summary, &raw_author, 1000);
        let commits = vec![commit.clone()];

        let entry = selected_history_entry(&commits, &[0], 0).expect("selected entry");
        let target = selected_history_action_target(&commits, &[0], 0).expect("selected commit");
        let selected = selected_history_commit(&commits, &[0], 0).expect("selected commit");
        let entry_target = entry.target();
        let resolved =
            source_control_history_commit_for_action_target(&commits, &target).expect("resolved");

        assert_eq!(entry.history_index(), 0);
        assert_eq!(entry.commit().short_oid, raw_short_oid);
        assert_eq!(entry_target, target);
        assert_eq!(selected.short_oid, raw_short_oid);
        assert_eq!(resolved.oid, commit.oid);
        assert_eq!(resolved.short_oid, raw_short_oid);
        assert_eq!(resolved.summary, raw_summary);
        assert_eq!(resolved.author, raw_author);
    }

    #[test]
    fn history_prepared_visible_row_preserves_raw_commit_fields_for_actions() {
        let raw_short_oid = format!("12\u{202e}\n34{}", "a".repeat(120));
        let raw_summary = format!("Add\u{2066}\nsearch{}", "b".repeat(240));
        let raw_author = format!("Kuroya\u{200f}\rTest{}", "c".repeat(180));
        let commit = commit(&raw_short_oid, &raw_summary, &raw_author, 1000);
        let mut rows: Vec<_> = source_control_history_prepared_visible_rows(
            std::slice::from_ref(&commit),
            &[0],
            0..1,
            3700,
            false,
        )
        .collect();

        let row = rows.first_mut().expect("prepared row");

        assert_eq!(row.commit().oid, commit.oid);
        assert_eq!(row.commit().short_oid, raw_short_oid);
        assert_eq!(row.commit().summary, raw_summary);
        assert_eq!(row.commit().author, raw_author);
        assert_eq!(
            source_control_commit_copy_text(row.commit(), SourceControlCommitCopyKind::ShortOid),
            raw_short_oid
        );
        assert_eq!(
            source_control_commit_copy_text(row.commit(), SourceControlCommitCopyKind::Summary),
            raw_summary
        );
        assert_eq!(
            source_control_commit_copy_text(row.commit(), SourceControlCommitCopyKind::Author),
            raw_author
        );

        assert!(row.label().starts_with("12 34"));
        assert!(row.label().contains("Add search"));
        assert!(!row.label().contains("Kuroya Test"));
        assert!(row.label().contains("..."));
        assert_history_display_text_is_safe(row.label());
        assert!(row.label().chars().count() <= SOURCE_CONTROL_HISTORY_ROW_LABEL_MAX_CHARS);

        let tooltip = row.tooltip().to_owned();
        assert!(tooltip.contains("Kuroya Test"));
        assert!(tooltip.contains("..."));
        assert_history_display_text_is_safe(&tooltip);
        assert!(tooltip.chars().count() <= SOURCE_CONTROL_HISTORY_ROW_LABEL_MAX_CHARS);

        let status = row.copy_status(SourceControlCommitCopyKind::Summary);
        assert!(status.starts_with("Copied commit message for 12 34"));
        assert!(status.contains("..."));
        assert_history_display_text_is_safe(&status);
        assert!(
            status.chars().count()
                <= "Copied commit message for ".chars().count()
                    + SOURCE_CONTROL_HISTORY_COMMIT_ID_DISPLAY_MAX_CHARS
        );
    }

    #[test]
    fn history_commit_copy_status_sanitizes_and_bounds_display_id() {
        let commit = commit(
            &format!("12\u{202e}\n34{}", "a".repeat(120)),
            "Add search panel",
            "Kuroya Test",
            1000,
        );

        let status = source_control_commit_copy_status(&commit, SourceControlCommitCopyKind::Oid);

        assert!(status.starts_with("Copied commit ID 12 34"));
        assert!(status.contains("..."));
        assert_history_display_text_is_safe(&status);
        assert!(
            status.chars().count()
                <= "Copied commit ID ".chars().count()
                    + SOURCE_CONTROL_HISTORY_COMMIT_ID_DISPLAY_MAX_CHARS
        );
        assert_eq!(
            source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::Oid),
            commit.oid.as_str()
        );
        assert_eq!(
            source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::ShortOid),
            commit.short_oid.as_str()
        );
    }

    #[test]
    fn history_commit_display_field_text_borrows_clean_ascii_and_unicode() {
        let ascii = "12345678";
        let unicode = "feature-\u{03c0}";

        let ascii_display = source_control_commit_id_display_text(ascii);
        let unicode_display = source_control_commit_display_field_text(unicode, 32, "unknown");

        assert!(matches!(ascii_display, Cow::Borrowed("12345678")));
        assert!(matches!(unicode_display, Cow::Borrowed("feature-\u{03c0}")));
    }

    #[test]
    fn history_commit_display_field_text_owns_dirty_truncated_and_fallback_outputs() {
        let dirty = source_control_commit_display_field_text("12\u{202e}\n34", 64, "unknown");
        let truncated =
            source_control_commit_display_field_text("abcdefghijklmnopqrstuvwxyz", 12, "unknown");
        let fallback = source_control_commit_display_field_text(" \u{202e}\n ", 64, "unknown");

        assert!(matches!(&dirty, Cow::Owned(_)));
        assert_eq!(dirty, "12 34");
        assert!(matches!(&truncated, Cow::Owned(_)));
        assert!(truncated.contains("..."));
        assert_eq!(truncated.chars().count(), 12);
        assert!(matches!(&fallback, Cow::Owned(_)));
        assert_eq!(fallback, "unknown");
    }

    #[test]
    fn history_commit_display_field_text_matches_string_wrappers() {
        for value in [
            "12345678",
            "feature-\u{03c0}",
            "12\u{202e}\n34",
            "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
            "",
        ] {
            assert_eq!(
                source_control_commit_id_display_text(value).as_ref(),
                source_control_commit_id_display(value)
            );
            assert_eq!(
                source_control_commit_display_field_text(value, 16, "unknown").as_ref(),
                source_control_commit_display_field(value, 16, "unknown")
            );
        }
    }

    #[test]
    fn history_commit_display_field_text_owns_huge_sampled_fields() {
        let field = format!(
            "{}{}{}",
            "prefix-".repeat(512),
            "middle-".repeat(2048),
            "suffix-".repeat(512)
        );

        let display = source_control_commit_display_field_text(
            &field,
            SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS,
            "No message",
        );

        assert!(matches!(&display, Cow::Owned(_)));
        assert!(display.starts_with("prefix-"));
        assert!(display.contains("suffix-"));
        assert!(display.contains("..."));
        assert_history_display_text_is_safe(display.as_ref());
        assert!(display.chars().count() <= SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS);
    }

    #[test]
    fn history_commit_display_sanitizing_preserves_filter_and_copy_payloads() {
        let summary = "Add\u{202e}\nsearch panel";
        let author = "Kuroya\u{2066}\nTest";
        let commit = commit("12345678", summary, author, 1000);

        assert_eq!(
            source_control_filtered_history_indices(
                std::slice::from_ref(&commit),
                "search kuroya",
                3700
            ),
            vec![0]
        );
        assert_eq!(
            source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::Summary),
            summary
        );
        assert_eq!(
            source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::Author),
            author
        );
    }

    #[test]
    fn history_commit_display_samples_huge_fields_before_sanitizing() {
        let summary = format!(
            "{}{}{}",
            "prefix-".repeat(80),
            "\u{202e}\n".repeat(4096),
            "suffix-".repeat(80)
        );

        let sample = source_control_history_display_sample(
            &summary,
            SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS,
        );
        let display = source_control_commit_summary_display(&summary);

        assert!(sample.len() < summary.len() / 8);
        assert!(display.starts_with("prefix-"));
        assert!(display.contains("suffix-"));
        assert!(display.contains("..."));
        assert_history_display_text_is_safe(&display);
        assert!(display.chars().count() <= SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS);
    }

    #[test]
    fn history_commit_display_samples_huge_ascii_fields_from_head_and_tail() {
        let summary = format!(
            "{}{}{}",
            "prefix-".repeat(512),
            "middle-".repeat(2048),
            "suffix-".repeat(512)
        );

        let sample = source_control_history_display_sample(
            &summary,
            SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS,
        );
        let display = source_control_commit_summary_display(&summary);

        assert!(sample.len() < summary.len() / 8);
        assert!(display.starts_with("prefix-"));
        assert!(display.contains("suffix-"));
        assert!(display.contains("..."));
        assert_history_display_text_is_safe(&display);
        assert!(display.chars().count() <= SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS);
    }
}

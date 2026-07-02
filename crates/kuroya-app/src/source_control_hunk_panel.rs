use crate::{
    KuroyaApp,
    file_runtime::file_path_open_buffer_or_known_openable,
    path_display::{display_path_label_cow, sanitized_display_label_cow},
    source_control_git_panel_ui::{
        SOURCE_CONTROL_GIT_HUNK_PANEL_DEFAULT_SIZE, SOURCE_CONTROL_GIT_ROW_HEIGHT,
        apply_git_panel_spacing, render_git_panel_row,
    },
    ui_state::{
        clamp_selection, handle_list_navigation_keys, plain_key_pressed,
        selected_row_scroll_offset, selection_page_step,
    },
};
use eframe::egui::{self, Context, InputState, Key, RichText, ScrollArea};
use kuroya_core::{Command, GitChangeStage, GitDiffHunk};
use std::{borrow::Cow, fmt::Write as _, path::Path};

const SOURCE_CONTROL_HUNK_HEADER_LABEL_MAX_CHARS: usize = 140;

impl KuroyaApp {
    pub(crate) fn render_git_hunks_panel(&mut self, ctx: &Context) {
        let mut close = false;
        let mut pending_hunk_actions = SourceControlHunkPendingActions::default();
        clamp_selection(
            &mut self.source_control_hunk_selected,
            self.source_control_hunks.len(),
        );

        egui::Window::new("Git Hunks")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 108.0])
            .default_size(SOURCE_CONTROL_GIT_HUNK_PANEL_DEFAULT_SIZE)
            .show(ctx, |ui| {
                apply_git_panel_spacing(ui);
                ui.horizontal(|ui| {
                    let title = self
                        .source_control_hunk_path
                        .as_ref()
                        .map(|path| {
                            format!(
                                "{} - {}",
                                display_path_label_cow(path),
                                source_control_hunk_stage_title(self.source_control_hunk_stage)
                            )
                        })
                        .unwrap_or_else(|| "No file".to_owned());
                    ui.label(RichText::new(title).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                        if ui.button("Refresh").clicked()
                            && let Some(path) = self.source_control_hunk_path.clone()
                        {
                            match self.source_control_hunk_stage {
                                GitChangeStage::Staged => {
                                    self.begin_source_control_staged_hunks(path)
                                }
                                GitChangeStage::Unstaged => self.begin_source_control_hunks(path),
                            }
                        }
                    });
                });

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
                let viewport_height = ui.available_height();
                let selection_changed = ui.input(|input| {
                    handle_list_navigation_keys(
                        input,
                        &mut self.source_control_hunk_selected,
                        self.source_control_hunks.len(),
                        selection_page_step(SOURCE_CONTROL_GIT_ROW_HEIGHT, viewport_height),
                    )
                });
                let stage = self.source_control_hunk_stage;
                let action_items = source_control_hunk_panel_action_items(stage);
                let selected_hunk_available = selected_source_control_hunk_available(
                    self.source_control_hunk_path.as_ref(),
                    &self.source_control_hunks,
                    self.source_control_hunk_selected,
                );
                if let Some(action) =
                    ui.input(|input| source_control_hunk_keyboard_action(input, stage))
                    && let Some(selection) = selected_source_control_hunk(
                        self.source_control_hunk_path.as_ref(),
                        &self.source_control_hunks,
                        self.source_control_hunk_selected,
                    )
                {
                    self.queue_source_control_hunk_panel_action(
                        selection,
                        stage,
                        action,
                        &mut pending_hunk_actions,
                    );
                }

                ui.separator();
                if self.source_control_hunks.is_empty() {
                    ui.label(
                        RichText::new(source_control_hunk_empty_label(
                            self.source_control_hunk_stage,
                        ))
                        .small(),
                    );
                } else {
                    let hunk_path_available = self.source_control_hunk_path.is_some();
                    let mut row_action: Option<(
                        SourceControlHunkRowActionSelection,
                        SourceControlHunkActionKind,
                    )> = None;
                    let mut scroll_area = ScrollArea::vertical().auto_shrink([false, false]);
                    if selection_changed {
                        scroll_area =
                            scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                                self.source_control_hunk_selected,
                                self.source_control_hunks.len(),
                                SOURCE_CONTROL_GIT_ROW_HEIGHT,
                                viewport_height,
                            ));
                    }
                    scroll_area.show_rows(
                        ui,
                        SOURCE_CONTROL_GIT_ROW_HEIGHT,
                        self.source_control_hunks.len(),
                        |ui, rows| {
                            for row in rows {
                                let Some(row_display) = self
                                    .source_control_hunks
                                    .get(row)
                                    .map(SourceControlHunkRowDisplay::new)
                                else {
                                    continue;
                                };
                                let selected = row == self.source_control_hunk_selected;
                                let response = render_git_panel_row(
                                    ui,
                                    selected,
                                    row_display.badge.as_str(),
                                    row_display.header.as_str(),
                                    row_display.detail.as_str(),
                                )
                                .on_hover_text(row_display.label.as_str());
                                if response.clicked() {
                                    self.source_control_hunk_selected = row;
                                }
                                if response.double_clicked() && hunk_path_available {
                                    row_action = Some((
                                        row_display.action_selection(),
                                        SourceControlHunkActionKind::OpenDiff,
                                    ));
                                }
                                response.context_menu(|ui| {
                                    if hunk_path_available {
                                        for item in action_items {
                                            if ui.button(item.label).clicked() {
                                                row_action = Some((
                                                    row_display.action_selection(),
                                                    item.kind,
                                                ));
                                                ui.close();
                                            }
                                        }
                                    }
                                });
                            }
                        },
                    );
                    if let Some((row_selection, action)) = row_action
                        && let Some(path) = self.source_control_hunk_path.as_ref()
                    {
                        self.queue_source_control_hunk_panel_action(
                            row_selection.selection(path.clone()),
                            stage,
                            action,
                            &mut pending_hunk_actions,
                        );
                    }
                }

                ui.horizontal(|ui| {
                    for item in action_items {
                        if ui
                            .add_enabled(selected_hunk_available, egui::Button::new(item.label))
                            .on_hover_text(item.tooltip)
                            .clicked()
                            && let Some(selection) = selected_source_control_hunk(
                                self.source_control_hunk_path.as_ref(),
                                &self.source_control_hunks,
                                self.source_control_hunk_selected,
                            )
                        {
                            self.queue_source_control_hunk_panel_action(
                                selection,
                                stage,
                                item.kind,
                                &mut pending_hunk_actions,
                            );
                        }
                    }
                    ui.label(
                        RichText::new(format!("{} hunks", self.source_control_hunks.len()))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });

        if close {
            self.source_control_hunks_open = false;
            self.status = "Closed git hunks".to_owned();
        }
        if let Some((selection, stage)) = pending_hunk_actions.open_source {
            self.open_source_control_hunk_source(selection, stage);
        }
        if let Some((path, stage, hunk_index)) = pending_hunk_actions.open_diff {
            self.open_source_control_hunk_diff(path, stage, hunk_index);
        }
        if let Some((path, stage, hunk_index)) = pending_hunk_actions.copy_patch {
            self.copy_source_control_hunk_patch(ctx, path, stage, hunk_index);
        }
    }

    fn queue_source_control_hunk_panel_action(
        &mut self,
        selection: SourceControlHunkSelection,
        stage: GitChangeStage,
        action: SourceControlHunkActionKind,
        pending_hunk_actions: &mut SourceControlHunkPendingActions,
    ) {
        if !source_control_hunk_action_is_available(stage, action) {
            return;
        }
        if !source_control_hunk_selection_matches_current(
            &selection,
            self.source_control_hunk_path.as_deref(),
            self.source_control_hunk_stage,
            stage,
            &self.source_control_hunks,
        ) {
            self.status = source_control_stale_hunk_action_status(
                stage,
                &selection.path,
                selection.hunk_index,
            );
            return;
        }

        match action {
            SourceControlHunkActionKind::Primary | SourceControlHunkActionKind::Discard => {
                if !source_control_hunk_action_is_mutating(stage, action) {
                    return;
                }
                if let Some(command) = source_control_hunk_action_command(
                    selection.path,
                    selection.hunk_index,
                    selection.hunk_fingerprint,
                    stage,
                    action,
                ) {
                    self.command_bus.push(command);
                }
            }
            SourceControlHunkActionKind::OpenSource => {
                pending_hunk_actions.open_source = Some((selection, stage));
            }
            SourceControlHunkActionKind::OpenDiff => {
                pending_hunk_actions.open_diff =
                    Some((selection.path, stage, selection.hunk_index));
            }
            SourceControlHunkActionKind::CopyPatch => {
                pending_hunk_actions.copy_patch =
                    Some((selection.path, stage, selection.hunk_index));
            }
        }
    }

    fn open_source_control_hunk_source(
        &mut self,
        selection: SourceControlHunkSelection,
        stage: GitChangeStage,
    ) {
        if !file_path_open_buffer_or_known_openable(
            &self.buffers,
            self.index.files(),
            &selection.path,
            Path::exists,
        ) {
            self.status = source_control_hunk_source_open_missing_status(
                stage,
                &selection.path,
                selection.hunk_index,
            );
            return;
        }

        let status = source_control_hunk_source_open_success_status(
            stage,
            &selection.path,
            selection.hunk_index,
            selection.source_line,
        );
        self.open_file_at_known_openable(selection.path, selection.source_line, 1);
        self.status = status;
    }
}

#[derive(Debug, Default)]
struct SourceControlHunkPendingActions {
    open_source: Option<(SourceControlHunkSelection, GitChangeStage)>,
    open_diff: Option<(std::path::PathBuf, GitChangeStage, usize)>,
    copy_patch: Option<(std::path::PathBuf, GitChangeStage, usize)>,
}

#[cfg(test)]
pub(crate) fn source_control_hunk_label(hunk: &GitDiffHunk) -> String {
    let header = source_control_hunk_header_label_cow(&hunk.header);
    source_control_hunk_label_with_header(hunk, header.as_ref())
}

fn source_control_hunk_label_with_header(hunk: &GitDiffHunk, header: &str) -> String {
    let mut label = String::with_capacity(header.len() + 32);
    write!(
        &mut label,
        "#{}  {}  +{} -{}",
        hunk.index, header, hunk.additions, hunk.deletions
    )
    .expect("writing hunk label to string should not fail");
    label
}

#[cfg(test)]
fn source_control_hunk_header_label(header: &str) -> String {
    source_control_hunk_header_label_cow(header).into_owned()
}

fn source_control_hunk_header_label_cow(header: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(header, SOURCE_CONTROL_HUNK_HEADER_LABEL_MAX_CHARS, "")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceControlHunkRowDisplay {
    badge: String,
    header: String,
    detail: String,
    label: String,
    hunk_index: usize,
    hunk_fingerprint: u64,
    source_line: usize,
}

impl SourceControlHunkRowDisplay {
    fn new(hunk: &GitDiffHunk) -> Self {
        let header = source_control_hunk_header_label_cow(&hunk.header).into_owned();
        let detail = source_control_hunk_detail(hunk);
        let label = source_control_hunk_label_with_header(hunk, &header);
        Self {
            badge: source_control_hunk_badge(hunk.index),
            header,
            detail,
            label,
            hunk_index: hunk.index,
            hunk_fingerprint: hunk.fingerprint,
            source_line: source_control_hunk_source_line(hunk),
        }
    }

    #[cfg(test)]
    fn selection(&self, path: std::path::PathBuf) -> SourceControlHunkSelection {
        self.action_selection().selection(path)
    }

    fn action_selection(&self) -> SourceControlHunkRowActionSelection {
        SourceControlHunkRowActionSelection {
            hunk_index: self.hunk_index,
            hunk_fingerprint: self.hunk_fingerprint,
            source_line: self.source_line,
        }
    }
}

fn source_control_hunk_badge(index: usize) -> String {
    format!("#{index}")
}

fn source_control_hunk_detail(hunk: &GitDiffHunk) -> String {
    format!("+{} -{}", hunk.additions, hunk.deletions)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceControlHunkRowActionSelection {
    hunk_index: usize,
    hunk_fingerprint: u64,
    source_line: usize,
}

impl SourceControlHunkRowActionSelection {
    fn selection(self, path: std::path::PathBuf) -> SourceControlHunkSelection {
        SourceControlHunkSelection {
            path,
            hunk_index: self.hunk_index,
            hunk_fingerprint: self.hunk_fingerprint,
            source_line: self.source_line,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceControlHunkSelection {
    path: std::path::PathBuf,
    hunk_index: usize,
    hunk_fingerprint: u64,
    source_line: usize,
}

impl SourceControlHunkSelection {
    fn new(path: std::path::PathBuf, hunk: &GitDiffHunk) -> Self {
        Self {
            path,
            hunk_index: hunk.index,
            hunk_fingerprint: hunk.fingerprint,
            source_line: source_control_hunk_source_line(hunk),
        }
    }
}

fn selected_source_control_hunk(
    path: Option<&std::path::PathBuf>,
    hunks: &[GitDiffHunk],
    selected: usize,
) -> Option<SourceControlHunkSelection> {
    hunks
        .get(selected)
        .and_then(|hunk| path.map(|path| SourceControlHunkSelection::new(path.clone(), hunk)))
}

fn selected_source_control_hunk_available(
    path: Option<&std::path::PathBuf>,
    hunks: &[GitDiffHunk],
    selected: usize,
) -> bool {
    path.is_some() && hunks.get(selected).is_some()
}

fn source_control_hunk_selection_matches_current(
    selection: &SourceControlHunkSelection,
    current_path: Option<&Path>,
    current_stage: GitChangeStage,
    action_stage: GitChangeStage,
    hunks: &[GitDiffHunk],
) -> bool {
    current_path == Some(selection.path.as_path())
        && current_stage == action_stage
        && hunks.iter().any(|hunk| {
            hunk.index == selection.hunk_index && hunk.fingerprint == selection.hunk_fingerprint
        })
}

pub(crate) fn source_control_hunk_source_line(hunk: &GitDiffHunk) -> usize {
    hunk.new_start.max(1)
}

pub(crate) fn source_control_hunk_source_open_success_status(
    stage: GitChangeStage,
    path: &std::path::Path,
    hunk_index: usize,
    line: usize,
) -> String {
    format!(
        "Opened {} hunk {hunk_index} source at {}:{line}",
        crate::source_control_hunk_runtime::hunk_stage_label(stage),
        display_path_label_cow(path)
    )
}

pub(crate) fn source_control_hunk_source_open_missing_status(
    stage: GitChangeStage,
    path: &std::path::Path,
    hunk_index: usize,
) -> String {
    format!(
        "No source file for {} hunk {hunk_index} in {}",
        crate::source_control_hunk_runtime::hunk_stage_label(stage),
        display_path_label_cow(path)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceControlHunkActionKind {
    Primary,
    OpenSource,
    OpenDiff,
    CopyPatch,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceControlHunkActionItem {
    kind: SourceControlHunkActionKind,
    label: &'static str,
    keyboard_label: &'static str,
    tooltip: &'static str,
}

const STAGED_SOURCE_CONTROL_HUNK_ACTIONS: &[SourceControlHunkActionItem] = &[
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::Primary,
        label: "Unstage Hunk",
        keyboard_label: "Enter Unstage Hunk",
        tooltip: "Unstage Hunk (Enter)",
    },
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::OpenSource,
        label: "Open File at Hunk",
        keyboard_label: "O Open File at Hunk",
        tooltip: "Open File at Hunk (O)",
    },
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::OpenDiff,
        label: "Open Hunk Diff",
        keyboard_label: "D Open Hunk Diff",
        tooltip: "Open Hunk Diff (D)",
    },
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::CopyPatch,
        label: "Copy Hunk Patch",
        keyboard_label: "P Copy Hunk Patch",
        tooltip: "Copy Hunk Patch (P)",
    },
];

const UNSTAGED_SOURCE_CONTROL_HUNK_ACTIONS: &[SourceControlHunkActionItem] = &[
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::Primary,
        label: "Stage Hunk",
        keyboard_label: "Enter Stage Hunk",
        tooltip: "Stage Hunk (Enter)",
    },
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::OpenSource,
        label: "Open File at Hunk",
        keyboard_label: "O Open File at Hunk",
        tooltip: "Open File at Hunk (O)",
    },
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::OpenDiff,
        label: "Open Hunk Diff",
        keyboard_label: "D Open Hunk Diff",
        tooltip: "Open Hunk Diff (D)",
    },
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::CopyPatch,
        label: "Copy Hunk Patch",
        keyboard_label: "P Copy Hunk Patch",
        tooltip: "Copy Hunk Patch (P)",
    },
    SourceControlHunkActionItem {
        kind: SourceControlHunkActionKind::Discard,
        label: "Discard Hunk",
        keyboard_label: "Delete Discard Hunk",
        tooltip: "Discard Hunk (Delete)",
    },
];

fn source_control_hunk_keyboard_action(
    input: &InputState,
    stage: GitChangeStage,
) -> Option<SourceControlHunkActionKind> {
    source_control_hunk_panel_action_items(stage)
        .iter()
        .map(|item| item.kind)
        .find(|action| source_control_hunk_keyboard_action_pressed(input, *action))
}

fn source_control_hunk_keyboard_action_pressed(
    input: &InputState,
    action: SourceControlHunkActionKind,
) -> bool {
    match action {
        SourceControlHunkActionKind::Primary => plain_key_pressed(input, Key::Enter),
        SourceControlHunkActionKind::OpenSource => plain_key_pressed(input, Key::O),
        SourceControlHunkActionKind::OpenDiff => plain_key_pressed(input, Key::D),
        SourceControlHunkActionKind::CopyPatch => plain_key_pressed(input, Key::P),
        SourceControlHunkActionKind::Discard => plain_key_pressed(input, Key::Delete),
    }
}

#[cfg(test)]
pub(crate) fn source_control_hunk_keyboard_action_labels(
    stage: GitChangeStage,
) -> Vec<&'static str> {
    source_control_hunk_panel_action_items(stage)
        .iter()
        .map(|item| item.keyboard_label)
        .collect()
}

#[cfg(test)]
fn source_control_hunk_keyboard_action_label(
    stage: GitChangeStage,
    action: SourceControlHunkActionKind,
) -> Option<&'static str> {
    source_control_hunk_panel_action_item(stage, action).map(|item| item.keyboard_label)
}

#[cfg(test)]
pub(crate) fn source_control_hunk_panel_action_labels(stage: GitChangeStage) -> Vec<&'static str> {
    source_control_hunk_panel_action_items(stage)
        .iter()
        .map(|item| item.label)
        .collect()
}

#[cfg(test)]
pub(crate) fn source_control_hunk_panel_action_tooltips(
    stage: GitChangeStage,
) -> Vec<&'static str> {
    source_control_hunk_panel_action_items(stage)
        .iter()
        .map(|item| item.tooltip)
        .collect()
}

fn source_control_hunk_panel_action_items(
    stage: GitChangeStage,
) -> &'static [SourceControlHunkActionItem] {
    match stage {
        GitChangeStage::Staged => STAGED_SOURCE_CONTROL_HUNK_ACTIONS,
        GitChangeStage::Unstaged => UNSTAGED_SOURCE_CONTROL_HUNK_ACTIONS,
    }
}

fn source_control_hunk_panel_action_item(
    stage: GitChangeStage,
    action: SourceControlHunkActionKind,
) -> Option<SourceControlHunkActionItem> {
    source_control_hunk_panel_action_items(stage)
        .iter()
        .copied()
        .find(|item| item.kind == action)
}

fn source_control_hunk_panel_action_label(
    stage: GitChangeStage,
    action: SourceControlHunkActionKind,
) -> Option<&'static str> {
    source_control_hunk_panel_action_item(stage, action).map(|item| item.label)
}

#[cfg(test)]
fn source_control_hunk_panel_action_tooltip(
    stage: GitChangeStage,
    action: SourceControlHunkActionKind,
) -> Option<&'static str> {
    source_control_hunk_panel_action_item(stage, action).map(|item| item.tooltip)
}

fn source_control_hunk_action_is_mutating(
    stage: GitChangeStage,
    action: SourceControlHunkActionKind,
) -> bool {
    matches!(
        (stage, action),
        (GitChangeStage::Staged, SourceControlHunkActionKind::Primary)
            | (
                GitChangeStage::Unstaged,
                SourceControlHunkActionKind::Primary
            )
            | (
                GitChangeStage::Unstaged,
                SourceControlHunkActionKind::Discard
            )
    )
}

fn source_control_hunk_action_is_available(
    stage: GitChangeStage,
    action: SourceControlHunkActionKind,
) -> bool {
    source_control_hunk_panel_action_label(stage, action).is_some()
}

fn source_control_hunk_action_command(
    path: std::path::PathBuf,
    hunk_index: usize,
    hunk_fingerprint: u64,
    stage: GitChangeStage,
    action: SourceControlHunkActionKind,
) -> Option<Command> {
    match (stage, action) {
        (GitChangeStage::Staged, SourceControlHunkActionKind::Primary) => {
            Some(Command::UnstageFileHunk {
                path,
                hunk_index,
                hunk_fingerprint: Some(hunk_fingerprint),
            })
        }
        (GitChangeStage::Unstaged, SourceControlHunkActionKind::Primary) => {
            Some(Command::StageFileHunk {
                path,
                hunk_index,
                hunk_fingerprint: Some(hunk_fingerprint),
            })
        }
        (GitChangeStage::Unstaged, SourceControlHunkActionKind::Discard) => {
            Some(Command::DiscardFileHunk {
                path,
                hunk_index,
                hunk_fingerprint: Some(hunk_fingerprint),
            })
        }
        (_, SourceControlHunkActionKind::OpenSource)
        | (_, SourceControlHunkActionKind::OpenDiff)
        | (_, SourceControlHunkActionKind::CopyPatch)
        | (GitChangeStage::Staged, SourceControlHunkActionKind::Discard) => None,
    }
}

fn source_control_stale_hunk_action_status(
    stage: GitChangeStage,
    path: &std::path::Path,
    hunk_index: usize,
) -> String {
    format!(
        "Refresh {} hunks in {} before changing hunk {hunk_index}",
        crate::source_control_hunk_runtime::hunk_stage_label(stage),
        display_path_label_cow(path)
    )
}

fn source_control_hunk_stage_title(stage: GitChangeStage) -> &'static str {
    match stage {
        GitChangeStage::Staged => "Staged Changes",
        GitChangeStage::Unstaged => "Changes",
    }
}

fn source_control_hunk_empty_label(stage: GitChangeStage) -> &'static str {
    match stage {
        GitChangeStage::Staged => "No staged hunks",
        GitChangeStage::Unstaged => "No unstaged hunks",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SOURCE_CONTROL_HUNK_HEADER_LABEL_MAX_CHARS, SourceControlHunkActionKind,
        SourceControlHunkPendingActions, SourceControlHunkRowActionSelection,
        SourceControlHunkRowDisplay, SourceControlHunkSelection, selected_source_control_hunk,
        source_control_hunk_action_command, source_control_hunk_action_is_available,
        source_control_hunk_action_is_mutating, source_control_hunk_header_label,
        source_control_hunk_header_label_cow, source_control_hunk_keyboard_action_label,
        source_control_hunk_label, source_control_hunk_panel_action_label,
        source_control_hunk_panel_action_tooltip, source_control_hunk_selection_matches_current,
        source_control_hunk_source_open_missing_status,
        source_control_hunk_source_open_success_status, source_control_stale_hunk_action_status,
    };
    use crate::source_control_runtime::source_control_app_for_test;
    use kuroya_core::{Command, GitChangeStage, GitDiffHunk};
    use std::{borrow::Cow, path::PathBuf};

    fn hunk(index: usize, new_start: usize) -> GitDiffHunk {
        GitDiffHunk {
            index,
            fingerprint: 1000 + index as u64,
            old_start: 1,
            old_lines: 1,
            new_start,
            new_lines: 1,
            additions: 1,
            deletions: 0,
            header: format!("@@ -1 +{new_start} @@"),
        }
    }

    #[test]
    fn selected_source_control_hunk_maps_virtual_rows_to_hunk_indices() {
        let path = PathBuf::from("src/main.rs");
        let hunks = vec![hunk(7, 12), hunk(9, 30)];

        let selected = selected_source_control_hunk(Some(&path), &hunks, 1).unwrap();

        assert_eq!(selected.path, path);
        assert_eq!(selected.hunk_index, 9);
        assert_eq!(selected.hunk_fingerprint, 1009);
        assert_eq!(selected.source_line, 30);
        assert!(selected_source_control_hunk(None, &hunks, 0).is_none());
        assert!(
            selected_source_control_hunk(Some(&PathBuf::from("src/lib.rs")), &hunks, 2).is_none()
        );
    }

    #[test]
    fn source_control_hunk_action_command_maps_mutating_actions() {
        let path = PathBuf::from("src/main.rs");

        assert_eq!(
            source_control_hunk_action_command(
                path.clone(),
                2,
                1002,
                GitChangeStage::Unstaged,
                SourceControlHunkActionKind::Primary,
            ),
            Some(Command::StageFileHunk {
                path: path.clone(),
                hunk_index: 2,
                hunk_fingerprint: Some(1002),
            })
        );
        assert_eq!(
            source_control_hunk_action_command(
                path.clone(),
                3,
                1003,
                GitChangeStage::Staged,
                SourceControlHunkActionKind::Primary,
            ),
            Some(Command::UnstageFileHunk {
                path: path.clone(),
                hunk_index: 3,
                hunk_fingerprint: Some(1003),
            })
        );
        assert_eq!(
            source_control_hunk_action_command(
                path.clone(),
                4,
                1004,
                GitChangeStage::Unstaged,
                SourceControlHunkActionKind::Discard,
            ),
            Some(Command::DiscardFileHunk {
                path,
                hunk_index: 4,
                hunk_fingerprint: Some(1004),
            })
        );
    }

    #[test]
    fn source_control_hunk_action_command_skips_invalid_actions() {
        let path = PathBuf::from("src/main.rs");

        assert_eq!(
            source_control_hunk_action_command(
                path.clone(),
                1,
                1001,
                GitChangeStage::Staged,
                SourceControlHunkActionKind::Discard,
            ),
            None
        );
        assert_eq!(
            source_control_hunk_action_command(
                path,
                1,
                1001,
                GitChangeStage::Unstaged,
                SourceControlHunkActionKind::OpenDiff,
            ),
            None
        );
    }

    #[test]
    fn source_control_hunk_selection_match_requires_current_path_stage_index_and_fingerprint() {
        let path = PathBuf::from("src/main.rs");
        let other_path = PathBuf::from("src/lib.rs");
        let current_hunk = hunk(3, 12);
        let stale_fingerprint = SourceControlHunkSelection {
            path: path.clone(),
            hunk_index: 3,
            hunk_fingerprint: 4040,
            source_line: 12,
        };
        let current_selection = SourceControlHunkSelection::new(path.clone(), &current_hunk);

        assert!(source_control_hunk_selection_matches_current(
            &current_selection,
            Some(path.as_path()),
            GitChangeStage::Unstaged,
            GitChangeStage::Unstaged,
            std::slice::from_ref(&current_hunk),
        ));
        assert!(!source_control_hunk_selection_matches_current(
            &current_selection,
            Some(other_path.as_path()),
            GitChangeStage::Unstaged,
            GitChangeStage::Unstaged,
            std::slice::from_ref(&current_hunk),
        ));
        assert!(!source_control_hunk_selection_matches_current(
            &current_selection,
            Some(path.as_path()),
            GitChangeStage::Staged,
            GitChangeStage::Unstaged,
            std::slice::from_ref(&current_hunk),
        ));
        assert!(!source_control_hunk_selection_matches_current(
            &stale_fingerprint,
            Some(path.as_path()),
            GitChangeStage::Unstaged,
            GitChangeStage::Unstaged,
            &[current_hunk],
        ));
    }

    #[test]
    fn source_control_hunk_panel_mutating_actions_guard_stale_selection_identity() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut current_hunk = hunk(4, 22);
        current_hunk.fingerprint = 4444;
        let mut app = source_control_app_for_test(root, true);
        app.source_control_hunks_open = true;
        app.source_control_hunk_path = Some(path.clone());
        app.source_control_hunk_stage = GitChangeStage::Unstaged;
        app.source_control_hunks = vec![current_hunk.clone()];
        let mut pending = SourceControlHunkPendingActions::default();

        app.queue_source_control_hunk_panel_action(
            SourceControlHunkSelection {
                path: path.clone(),
                hunk_index: 4,
                hunk_fingerprint: 9999,
                source_line: 22,
            },
            GitChangeStage::Unstaged,
            SourceControlHunkActionKind::Primary,
            &mut pending,
        );

        assert!(app.command_bus.is_empty());
        assert_eq!(
            app.status,
            source_control_stale_hunk_action_status(GitChangeStage::Unstaged, &path, 4)
        );

        app.status.clear();
        app.queue_source_control_hunk_panel_action(
            SourceControlHunkSelection::new(path.clone(), &current_hunk),
            GitChangeStage::Unstaged,
            SourceControlHunkActionKind::Primary,
            &mut pending,
        );

        assert_eq!(
            app.command_bus.pop(),
            Some(Command::StageFileHunk {
                path,
                hunk_index: 4,
                hunk_fingerprint: Some(4444),
            })
        );
        assert!(app.status.is_empty());
    }

    #[test]
    fn source_control_hunk_panel_view_actions_guard_stale_selection_identity() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut current_hunk = hunk(4, 22);
        current_hunk.fingerprint = 4444;
        let stale_selection = SourceControlHunkSelection {
            path: path.clone(),
            hunk_index: 4,
            hunk_fingerprint: 9999,
            source_line: 22,
        };
        let mut app = source_control_app_for_test(root, true);
        app.source_control_hunks_open = true;
        app.source_control_hunk_path = Some(path.clone());
        app.source_control_hunk_stage = GitChangeStage::Unstaged;
        app.source_control_hunks = vec![current_hunk.clone()];

        for action in [
            SourceControlHunkActionKind::OpenSource,
            SourceControlHunkActionKind::OpenDiff,
            SourceControlHunkActionKind::CopyPatch,
        ] {
            app.status.clear();
            let mut pending = SourceControlHunkPendingActions::default();

            app.queue_source_control_hunk_panel_action(
                stale_selection.clone(),
                GitChangeStage::Unstaged,
                action,
                &mut pending,
            );

            assert!(pending.open_source.is_none());
            assert!(pending.open_diff.is_none());
            assert!(pending.copy_patch.is_none());
            assert_eq!(
                app.status,
                source_control_stale_hunk_action_status(GitChangeStage::Unstaged, &path, 4)
            );
        }

        let current_selection = SourceControlHunkSelection::new(path.clone(), &current_hunk);
        let mut pending = SourceControlHunkPendingActions::default();
        app.status.clear();

        app.queue_source_control_hunk_panel_action(
            current_selection.clone(),
            GitChangeStage::Unstaged,
            SourceControlHunkActionKind::OpenDiff,
            &mut pending,
        );

        assert_eq!(pending.open_diff, Some((path, GitChangeStage::Unstaged, 4)));
        assert!(app.status.is_empty());
    }

    #[test]
    fn source_control_hunk_panel_rejects_unavailable_actions_before_stale_checks() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, true);
        app.source_control_hunks_open = true;
        app.source_control_hunk_path = Some(path.clone());
        app.source_control_hunk_stage = GitChangeStage::Staged;
        app.source_control_hunks = vec![hunk(4, 22)];
        app.status.clear();
        let mut pending = SourceControlHunkPendingActions::default();

        assert!(!source_control_hunk_action_is_available(
            GitChangeStage::Staged,
            SourceControlHunkActionKind::Discard
        ));
        app.queue_source_control_hunk_panel_action(
            SourceControlHunkSelection {
                path,
                hunk_index: 4,
                hunk_fingerprint: 9999,
                source_line: 22,
            },
            GitChangeStage::Staged,
            SourceControlHunkActionKind::Discard,
            &mut pending,
        );

        assert!(pending.open_source.is_none());
        assert!(pending.open_diff.is_none());
        assert!(pending.copy_patch.is_none());
        assert!(app.command_bus.is_empty());
        assert!(app.status.is_empty());
    }

    #[test]
    fn source_control_hunk_mutating_commands_preserve_raw_path_and_identity() {
        let path = PathBuf::from("workspace").join("line\nname\u{202e}hidden.rs");

        let command = source_control_hunk_action_command(
            path.clone(),
            7,
            7777,
            GitChangeStage::Unstaged,
            SourceControlHunkActionKind::Discard,
        );

        assert_eq!(
            command,
            Some(Command::DiscardFileHunk {
                path,
                hunk_index: 7,
                hunk_fingerprint: Some(7777),
            })
        );
        assert!(source_control_hunk_action_is_mutating(
            GitChangeStage::Unstaged,
            SourceControlHunkActionKind::Discard
        ));
        assert!(!source_control_hunk_action_is_mutating(
            GitChangeStage::Staged,
            SourceControlHunkActionKind::Discard
        ));
    }

    #[test]
    fn source_control_hunk_labels_skip_staged_discard_action() {
        assert_eq!(
            source_control_hunk_keyboard_action_label(
                GitChangeStage::Staged,
                SourceControlHunkActionKind::Discard,
            ),
            None
        );
        assert_eq!(
            source_control_hunk_panel_action_label(
                GitChangeStage::Staged,
                SourceControlHunkActionKind::Discard,
            ),
            None
        );
        assert_eq!(
            source_control_hunk_panel_action_tooltip(
                GitChangeStage::Staged,
                SourceControlHunkActionKind::Discard,
            ),
            None
        );
    }

    #[test]
    fn source_control_hunk_header_label_cow_borrows_clean_ascii_and_unicode_headers() {
        for header in [
            "@@ -1,2 +1,3 @@ fn render_hunks",
            "@@ -8 +8 @@ fn render_\u{03bb}_hunks",
        ] {
            match source_control_hunk_header_label_cow(header) {
                Cow::Borrowed(label) => assert_eq!(label, header),
                Cow::Owned(label) => panic!("expected borrowed hunk header, got {label:?}"),
            }
            assert_eq!(source_control_hunk_header_label(header), header);
        }
    }

    #[test]
    fn source_control_hunk_header_label_cow_owns_dirty_truncated_and_fallback_headers() {
        let long_header = format!(
            "@@ -1 +1 @@ {}",
            "very-long-hunk-header-".repeat(SOURCE_CONTROL_HUNK_HEADER_LABEL_MAX_CHARS)
        );
        let cases = [
            "@@ -1 +1 @@ before\nfn after\u{202e}".to_owned(),
            long_header,
            "\r\n\t\u{202e}".to_owned(),
        ];

        for header in cases {
            let label = source_control_hunk_header_label_cow(&header);

            assert_eq!(label.as_ref(), source_control_hunk_header_label(&header));
            assert!(
                matches!(&label, Cow::Owned(_)),
                "expected owned hunk header label for {header:?}"
            );
            assert!(!label.contains('\n'));
            assert!(!label.contains('\r'));
            assert!(!label.contains('\t'));
            assert!(!label.contains('\u{202e}'));
            assert!(
                label.chars().count() <= SOURCE_CONTROL_HUNK_HEADER_LABEL_MAX_CHARS,
                "label should be bounded: {label}"
            );
        }

        let overlong_header = format!(
            "@@ -1 +1 @@ {}",
            "very-long-hunk-header-".repeat(SOURCE_CONTROL_HUNK_HEADER_LABEL_MAX_CHARS)
        );
        let truncated = source_control_hunk_header_label_cow(&overlong_header);
        assert!(truncated.contains("..."));
        assert_eq!(
            source_control_hunk_header_label_cow("\u{202e}").as_ref(),
            ""
        );
    }

    #[test]
    fn source_control_hunk_label_sanitizes_and_bounds_header_text() {
        let mut hunk = hunk(4, 18);
        hunk.header = format!(
            "@@ -1 +18 @@ fn before\nfn after \u{202e}{}",
            "very-long-".repeat(32)
        );

        let label = source_control_hunk_label(&hunk);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(
            label.chars().count() <= SOURCE_CONTROL_HUNK_HEADER_LABEL_MAX_CHARS + 32,
            "label should be bounded: {label}"
        );
    }

    #[test]
    fn source_control_hunk_row_display_sanitizes_huge_unsafe_header() {
        let mut hunk = hunk(42, 0);
        hunk.additions = 123;
        hunk.deletions = 45;
        hunk.header = format!(
            "@@ -0,0 +0,0 @@ start\n{}\u{202e}\u{0000}end",
            "very-long-unsafe-header-".repeat(1024)
        );

        let display = SourceControlHunkRowDisplay::new(&hunk);

        assert_eq!(display.hunk_index, 42);
        assert_eq!(display.hunk_fingerprint, 1042);
        assert_eq!(display.source_line, 1);
        assert!(display.label.starts_with("#42  @@ -0,0 +0,0 @@ start "));
        assert!(display.label.ends_with("  +123 -45"));
        assert!(!display.label.contains('\n'));
        assert!(!display.label.contains('\u{202e}'));
        assert!(!display.label.contains('\u{0000}'));
        assert!(display.label.contains("..."));

        let label_frame_chars =
            format!("#{}    +{} -{}", hunk.index, hunk.additions, hunk.deletions)
                .chars()
                .count();
        assert!(
            display.label.chars().count()
                <= SOURCE_CONTROL_HUNK_HEADER_LABEL_MAX_CHARS + label_frame_chars,
            "label should be bounded: {}",
            display.label
        );
    }

    #[test]
    fn source_control_hunk_row_display_sanitizes_label_without_rewriting_raw_hunk_or_path_data() {
        let mut hunk = hunk(8, 44);
        hunk.fingerprint = 8888;
        hunk.additions = 9;
        hunk.deletions = 2;
        hunk.header = "@@ -40 +44 @@ before\nraw\u{202e}header".to_owned();
        let raw_header = hunk.header.clone();

        let display = SourceControlHunkRowDisplay::new(&hunk);

        assert_eq!(hunk.header, raw_header);
        assert_eq!(display.hunk_index, 8);
        assert_eq!(display.hunk_fingerprint, 8888);
        assert_eq!(display.source_line, 44);
        assert!(display.label.starts_with("#8  @@ -40 +44 @@ before "));
        assert!(display.label.ends_with("rawheader  +9 -2"));
        assert!(!display.label.contains('\n'));
        assert!(!display.label.contains('\u{202e}'));

        let path = PathBuf::from("workspace").join("raw\npath\u{202e}.rs");
        assert_eq!(
            display.selection(path.clone()),
            SourceControlHunkSelection {
                path,
                hunk_index: 8,
                hunk_fingerprint: 8888,
                source_line: 44,
            }
        );
    }

    #[test]
    fn source_control_hunk_row_display_carries_render_and_action_values() {
        let mut hunk = hunk(12, 33);
        hunk.fingerprint = 9876;
        hunk.additions = 5;
        hunk.deletions = 7;
        hunk.header = "@@ -2,3 +33,5 @@ fn demo".to_owned();

        let display = SourceControlHunkRowDisplay::new(&hunk);

        assert_eq!(display.label, "#12  @@ -2,3 +33,5 @@ fn demo  +5 -7");
        assert_eq!(display.hunk_index, 12);
        assert_eq!(display.hunk_fingerprint, 9876);
        assert_eq!(display.source_line, 33);
        assert_eq!(
            display.action_selection(),
            SourceControlHunkRowActionSelection {
                hunk_index: 12,
                hunk_fingerprint: 9876,
                source_line: 33,
            }
        );

        let path = PathBuf::from("src/main.rs");
        assert_eq!(
            display.selection(path.clone()),
            SourceControlHunkSelection {
                path,
                hunk_index: 12,
                hunk_fingerprint: 9876,
                source_line: 33,
            }
        );
    }

    #[test]
    fn source_control_hunk_statuses_sanitize_and_bound_path_labels() {
        let path = PathBuf::from("workspace")
            .join(format!("bad\nname\u{202e}-{}.rs", "very-long-".repeat(32)));

        let opened =
            source_control_hunk_source_open_success_status(GitChangeStage::Unstaged, &path, 2, 17);
        let missing =
            source_control_hunk_source_open_missing_status(GitChangeStage::Staged, &path, 3);

        for status in [opened, missing] {
            assert!(!status.contains('\n'));
            assert!(!status.contains('\u{202e}'));
            assert!(status.contains("..."));
        }
    }
}

use crate::{
    KuroyaApp,
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
    ui_text::count_label,
    workspace_tasks_runtime::{
        RunningWorkspaceTask, workspace_task_command_label, workspace_task_fingerprint,
        workspace_task_name_label, workspace_task_snapshot_is_running,
    },
};
use eframe::egui::{self, Color32, Context, Key, RichText, ScrollArea};
use kuroya_core::{Command, WorkspaceTask, workspace_task_kind_label};
use std::collections::HashSet;

const WORKSPACE_TASK_ROW_HEIGHT: f32 = 24.0;
const WORKSPACE_TASK_RUNNING_LOOKUP_MIN_SCAN_WORK: usize = 16;

impl KuroyaApp {
    pub(crate) fn render_workspace_tasks_panel(&mut self, ctx: &Context) {
        let mut close = false;
        self.prune_finished_workspace_tasks();
        clamp_selection(
            &mut self.workspace_tasks_selected,
            self.workspace_tasks.len(),
        );

        egui::Window::new("Workspace Tasks")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 120.0,
            ))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 96.0])
            .default_size([620.0, 380.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Tasks").strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                        if ui
                            .add_enabled(
                                self.workspace_trusted && !self.workspace_tasks_loading,
                                egui::Button::new("Refresh"),
                            )
                            .clicked()
                        {
                            self.spawn_workspace_task_load();
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
                        &mut self.workspace_tasks_selected,
                        self.workspace_tasks.len(),
                        selection_page_step(WORKSPACE_TASK_ROW_HEIGHT, viewport_height),
                    )
                });
                let run_selected = ui.input(|input| input.key_pressed(Key::Enter));

                ui.separator();
                if !self.workspace_trusted {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Tasks are disabled in restricted workspaces.")
                                .small()
                                .color(Color32::from_rgb(221, 146, 72)),
                        );
                        if ui.button("Trust Workspace").clicked() {
                            self.command_bus.push(Command::TrustWorkspace);
                        }
                    });
                    ui.add_space(6.0);
                }
                let mut selected_visible_snapshot = None;
                if self.workspace_tasks.is_empty() {
                    ui.label(RichText::new("No workspace tasks configured").small());
                } else {
                    let mut workspace_tasks_selected = self.workspace_tasks_selected;
                    let workspace_trusted = self.workspace_trusted;
                    let workspace_tasks_loading = self.workspace_tasks_loading;
                    let tasks = &self.workspace_tasks;
                    let running_tasks = &self.running_workspace_tasks;
                    let mut pending_command = None;
                    let mut scroll_area = ScrollArea::vertical().auto_shrink([false, false]);
                    if selection_changed {
                        scroll_area =
                            scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                                workspace_tasks_selected,
                                tasks.len(),
                                WORKSPACE_TASK_ROW_HEIGHT,
                                viewport_height,
                            ));
                    }
                    scroll_area.show_rows(
                        ui,
                        WORKSPACE_TASK_ROW_HEIGHT,
                        tasks.len(),
                        |ui, rows| {
                            workspace_task_for_each_visible_display_row(
                                tasks,
                                rows,
                                running_tasks,
                                |display_row| {
                                    let WorkspaceTaskDisplayRow {
                                        index,
                                        label,
                                        snapshot,
                                    } = display_row;
                                    let selected = index == workspace_tasks_selected;
                                    let response = ui.selectable_label(selected, label);
                                    if response.clicked() {
                                        workspace_tasks_selected = index;
                                    }
                                    if index == workspace_tasks_selected {
                                        selected_visible_snapshot = Some((index, snapshot));
                                    }
                                    if response.double_clicked()
                                        && workspace_task_actions_enabled(
                                            workspace_trusted,
                                            workspace_tasks_loading,
                                        )
                                        && let Some(task) = tasks.get(index)
                                    {
                                        pending_command = Some(workspace_task_run_target_command(
                                            index,
                                            snapshot.fingerprint_or_compute(task),
                                        ));
                                    }
                                },
                            );
                        },
                    );
                    self.workspace_tasks_selected = workspace_tasks_selected;
                    if let Some(command) = pending_command {
                        self.command_bus.push(command);
                    }
                }

                ui.horizontal(|ui| {
                    let actions_enabled = workspace_task_actions_enabled(
                        self.workspace_trusted,
                        self.workspace_tasks_loading,
                    );
                    let selected = workspace_task_selected_row(
                        &self.workspace_tasks,
                        self.workspace_tasks_selected,
                        selected_visible_snapshot,
                        &self.running_workspace_tasks,
                    );
                    if run_selected
                        && let Some(selected) = selected
                        && let Some(command) = selected
                            .run_command(self.workspace_trusted, self.workspace_tasks_loading)
                    {
                        self.command_bus.push(command);
                    }
                    let selected_running = selected.is_some_and(|selected| selected.running());
                    if ui
                        .add_enabled(
                            selected.is_some() && actions_enabled,
                            egui::Button::new("Run"),
                        )
                        .clicked()
                        && let Some(selected) = selected
                        && let Some(command) = selected
                            .run_command(self.workspace_trusted, self.workspace_tasks_loading)
                    {
                        self.command_bus.push(command);
                    }
                    if ui
                        .add_enabled(
                            selected_running && actions_enabled,
                            egui::Button::new("Cancel"),
                        )
                        .clicked()
                        && let Some(selected) = selected
                        && let Some(command) = selected
                            .cancel_command(self.workspace_trusted, self.workspace_tasks_loading)
                    {
                        self.command_bus.push(command);
                    }
                    ui.label(
                        RichText::new(count_label(self.workspace_tasks.len(), "task", "tasks"))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                });
            });

        if close {
            self.workspace_tasks_open = false;
            self.status = "Closed workspace tasks".to_owned();
        }
    }
}

#[cfg(test)]
pub(crate) fn workspace_task_label(task: &WorkspaceTask, running: bool) -> String {
    workspace_task_display_label_for_state(task, running)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceTaskDisplayRow {
    index: usize,
    label: String,
    snapshot: WorkspaceTaskRowSnapshot,
}

impl WorkspaceTaskDisplayRow {
    #[cfg(test)]
    fn label(&self) -> &str {
        &self.label
    }

    #[cfg(test)]
    fn snapshot(&self) -> WorkspaceTaskRowSnapshot {
        self.snapshot
    }

    #[cfg(test)]
    fn fingerprint_or_compute(&self, task: &WorkspaceTask) -> u64 {
        self.snapshot.fingerprint_or_compute(task)
    }
}

#[cfg(test)]
fn workspace_task_display_row(
    index: usize,
    task: &WorkspaceTask,
    running_tasks: &[RunningWorkspaceTask],
) -> WorkspaceTaskDisplayRow {
    let snapshot = workspace_task_row_snapshot(index, task, running_tasks);
    workspace_task_display_row_with_snapshot(index, task, snapshot)
}

fn workspace_task_display_row_with_running_lookup(
    index: usize,
    task: &WorkspaceTask,
    running_lookup: &WorkspaceTaskRunningLookup<'_>,
) -> WorkspaceTaskDisplayRow {
    let snapshot = workspace_task_row_snapshot_with_running_lookup(index, task, running_lookup);
    workspace_task_display_row_with_snapshot(index, task, snapshot)
}

fn workspace_task_for_each_visible_display_row(
    tasks: &[WorkspaceTask],
    rows: std::ops::Range<usize>,
    running_tasks: &[RunningWorkspaceTask],
    mut visit: impl FnMut(WorkspaceTaskDisplayRow),
) {
    let rows = workspace_task_visible_row_bounds(tasks.len(), rows);
    let running_lookup = WorkspaceTaskRunningLookup::new(running_tasks, rows.len());
    for index in rows {
        visit(workspace_task_display_row_with_running_lookup(
            index,
            &tasks[index],
            &running_lookup,
        ));
    }
}

#[cfg(test)]
fn workspace_task_visible_display_rows(
    tasks: &[WorkspaceTask],
    rows: std::ops::Range<usize>,
    running_tasks: &[RunningWorkspaceTask],
) -> Vec<WorkspaceTaskDisplayRow> {
    let rows = workspace_task_visible_row_bounds(tasks.len(), rows);
    let mut display_rows = Vec::with_capacity(rows.len());
    workspace_task_for_each_visible_display_row(tasks, rows, running_tasks, |row| {
        display_rows.push(row)
    });
    display_rows
}

fn workspace_task_visible_row_bounds(
    task_count: usize,
    rows: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    let start = rows.start.min(task_count);
    let end = rows.end.min(task_count);
    if start >= end {
        start..start
    } else {
        start..end
    }
}

fn workspace_task_display_row_with_snapshot(
    index: usize,
    task: &WorkspaceTask,
    snapshot: WorkspaceTaskRowSnapshot,
) -> WorkspaceTaskDisplayRow {
    WorkspaceTaskDisplayRow {
        index,
        label: workspace_task_display_label_for_state(task, snapshot.running),
        snapshot,
    }
}

fn workspace_task_display_label_for_state(task: &WorkspaceTask, running: bool) -> String {
    let kind = workspace_task_kind_label(task.kind);
    let default = workspace_task_default_fragment(task.default);
    let running = workspace_task_running_fragment(running);
    let name = workspace_task_name_label(&task.name);
    let command = workspace_task_command_label(task);
    workspace_task_display_label(kind, default, running, &name, &command)
}

fn workspace_task_default_fragment(default: bool) -> &'static str {
    if default { " default" } else { "" }
}

fn workspace_task_running_fragment(running: bool) -> &'static str {
    if running { " running" } else { "" }
}

fn workspace_task_display_label(
    kind: &str,
    default: &str,
    running: &str,
    name: &str,
    command: &str,
) -> String {
    let mut label = String::with_capacity(
        kind.len() + default.len() + running.len() + "    ".len() + name.len() + command.len(),
    );
    label.push_str(kind);
    label.push_str(default);
    label.push_str(running);
    label.push_str("  ");
    label.push_str(name);
    label.push_str("  ");
    label.push_str(command);
    label
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceTaskRowSnapshot {
    fingerprint: Option<u64>,
    running: bool,
}

impl WorkspaceTaskRowSnapshot {
    fn fingerprint_or_compute(self, task: &WorkspaceTask) -> u64 {
        self.fingerprint
            .unwrap_or_else(|| workspace_task_fingerprint(task))
    }
}

fn workspace_task_row_snapshot(
    index: usize,
    task: &WorkspaceTask,
    running_tasks: &[RunningWorkspaceTask],
) -> WorkspaceTaskRowSnapshot {
    workspace_task_row_snapshot_with_fingerprint(index, running_tasks, || {
        workspace_task_fingerprint(task)
    })
}

fn workspace_task_row_snapshot_with_fingerprint(
    index: usize,
    running_tasks: &[RunningWorkspaceTask],
    fingerprint: impl FnOnce() -> u64,
) -> WorkspaceTaskRowSnapshot {
    if running_tasks.is_empty() {
        return WorkspaceTaskRowSnapshot {
            fingerprint: None,
            running: false,
        };
    }

    let fingerprint = fingerprint();
    WorkspaceTaskRowSnapshot {
        fingerprint: Some(fingerprint),
        running: workspace_task_fingerprint_is_running(index, fingerprint, running_tasks),
    }
}

fn workspace_task_row_snapshot_with_running_lookup(
    index: usize,
    task: &WorkspaceTask,
    running_lookup: &WorkspaceTaskRunningLookup<'_>,
) -> WorkspaceTaskRowSnapshot {
    if !running_lookup.has_running_tasks() {
        return WorkspaceTaskRowSnapshot {
            fingerprint: None,
            running: false,
        };
    }

    let fingerprint = workspace_task_fingerprint(task);
    WorkspaceTaskRowSnapshot {
        fingerprint: Some(fingerprint),
        running: running_lookup.is_running(index, fingerprint),
    }
}

struct WorkspaceTaskRunningLookup<'a> {
    running_tasks: &'a [RunningWorkspaceTask],
    indexed: Option<HashSet<(usize, u64)>>,
}

impl<'a> WorkspaceTaskRunningLookup<'a> {
    fn new(running_tasks: &'a [RunningWorkspaceTask], expected_checks: usize) -> Self {
        let scan_work = running_tasks.len().saturating_mul(expected_checks);
        let indexed = (scan_work >= WORKSPACE_TASK_RUNNING_LOOKUP_MIN_SCAN_WORK
            && running_tasks.len() > 1)
            .then(|| {
                let mut indexed = HashSet::with_capacity(running_tasks.len());
                indexed.extend(
                    running_tasks
                        .iter()
                        .map(|running| (running.task_index, running.fingerprint)),
                );
                indexed
            });

        Self {
            running_tasks,
            indexed,
        }
    }

    fn has_running_tasks(&self) -> bool {
        !self.running_tasks.is_empty()
    }

    fn is_running(&self, index: usize, fingerprint: u64) -> bool {
        self.indexed.as_ref().map_or_else(
            || workspace_task_fingerprint_is_running(index, fingerprint, self.running_tasks),
            |indexed| indexed.contains(&(index, fingerprint)),
        )
    }
}

fn workspace_task_selected_row_snapshot(
    selected: usize,
    task: &WorkspaceTask,
    visible_snapshot: Option<(usize, WorkspaceTaskRowSnapshot)>,
    running_tasks: &[RunningWorkspaceTask],
) -> WorkspaceTaskRowSnapshot {
    if let Some((_, snapshot)) = visible_snapshot.filter(|(index, _)| *index == selected)
        && workspace_task_row_snapshot_matches_current(selected, task, snapshot, running_tasks)
    {
        return snapshot;
    }

    workspace_task_row_snapshot(selected, task, running_tasks)
}

fn workspace_task_row_snapshot_matches_current(
    selected: usize,
    task: &WorkspaceTask,
    snapshot: WorkspaceTaskRowSnapshot,
    running_tasks: &[RunningWorkspaceTask],
) -> bool {
    match snapshot.fingerprint {
        Some(fingerprint) => {
            fingerprint == workspace_task_fingerprint(task)
                && snapshot.running
                    == workspace_task_fingerprint_is_running(selected, fingerprint, running_tasks)
        }
        None => running_tasks.is_empty() && !snapshot.running,
    }
}

#[derive(Clone, Copy)]
struct WorkspaceTaskSelectedRow<'a> {
    index: usize,
    task: &'a WorkspaceTask,
    snapshot: WorkspaceTaskRowSnapshot,
}

impl WorkspaceTaskSelectedRow<'_> {
    fn running(self) -> bool {
        self.snapshot.running
    }

    fn fingerprint(self) -> u64 {
        self.snapshot.fingerprint_or_compute(self.task)
    }

    fn run_command(self, trusted: bool, loading: bool) -> Option<Command> {
        if !workspace_task_actions_enabled(trusted, loading) {
            return None;
        }

        Some(workspace_task_run_target_command(
            self.index,
            self.fingerprint(),
        ))
    }

    fn cancel_command(self, trusted: bool, loading: bool) -> Option<Command> {
        if !workspace_task_actions_enabled(trusted, loading) || !self.running() {
            return None;
        }

        Some(workspace_task_cancel_target_command(
            self.index,
            self.fingerprint(),
        ))
    }
}

fn workspace_task_selected_row<'a>(
    tasks: &'a [WorkspaceTask],
    selected: usize,
    visible_snapshot: Option<(usize, WorkspaceTaskRowSnapshot)>,
    running_tasks: &[RunningWorkspaceTask],
) -> Option<WorkspaceTaskSelectedRow<'a>> {
    let task = tasks.get(selected)?;
    Some(WorkspaceTaskSelectedRow {
        index: selected,
        task,
        snapshot: workspace_task_selected_row_snapshot(
            selected,
            task,
            visible_snapshot,
            running_tasks,
        ),
    })
}

#[cfg(test)]
pub(crate) fn workspace_task_is_running(
    index: usize,
    task: &WorkspaceTask,
    running_tasks: &[RunningWorkspaceTask],
) -> bool {
    workspace_task_fingerprint_is_running(index, workspace_task_fingerprint(task), running_tasks)
}

pub(crate) fn workspace_task_fingerprint_is_running(
    index: usize,
    fingerprint: u64,
    running_tasks: &[RunningWorkspaceTask],
) -> bool {
    if running_tasks.is_empty() {
        return false;
    }
    workspace_task_snapshot_is_running(index, fingerprint, running_tasks)
}

#[cfg(test)]
pub(crate) fn workspace_task_run_command(
    tasks: &[WorkspaceTask],
    selected: usize,
    trusted: bool,
    loading: bool,
) -> Option<Command> {
    if !workspace_task_actions_enabled(trusted, loading) {
        return None;
    }

    workspace_task_selected_row(tasks, selected, None, &[])?.run_command(trusted, loading)
}

#[cfg(test)]
pub(crate) fn workspace_task_run_snapshot_command(
    selected: usize,
    fingerprint: u64,
    trusted: bool,
    loading: bool,
) -> Option<Command> {
    workspace_task_actions_enabled(trusted, loading)
        .then(|| workspace_task_run_target_command(selected, fingerprint))
}

fn workspace_task_run_target_command(selected: usize, fingerprint: u64) -> Command {
    Command::RunWorkspaceTaskSnapshot {
        index: selected,
        fingerprint,
    }
}

fn workspace_task_cancel_target_command(selected: usize, fingerprint: u64) -> Command {
    Command::CancelWorkspaceTaskSnapshot {
        index: selected,
        fingerprint,
    }
}

fn workspace_task_actions_enabled(trusted: bool, loading: bool) -> bool {
    trusted && !loading
}

#[cfg(test)]
pub(crate) fn workspace_task_cancel_command(
    tasks: &[WorkspaceTask],
    selected: usize,
    trusted: bool,
    loading: bool,
    running_tasks: &[RunningWorkspaceTask],
) -> Option<Command> {
    if !workspace_task_actions_enabled(trusted, loading) {
        return None;
    }

    workspace_task_selected_row(tasks, selected, None, running_tasks)?
        .cancel_command(trusted, loading)
}

#[cfg(test)]
pub(crate) fn workspace_task_cancel_snapshot_command(
    selected: usize,
    fingerprint: u64,
    trusted: bool,
    loading: bool,
    running_tasks: &[RunningWorkspaceTask],
) -> Option<Command> {
    if !workspace_task_actions_enabled(trusted, loading) {
        return None;
    }

    workspace_task_fingerprint_is_running(selected, fingerprint, running_tasks)
        .then(|| workspace_task_cancel_target_command(selected, fingerprint))
}

#[cfg(test)]
mod tests {
    use super::{
        WorkspaceTaskRowSnapshot, WorkspaceTaskRunningLookup,
        workspace_task_cancel_snapshot_command, workspace_task_display_row,
        workspace_task_fingerprint_is_running, workspace_task_label, workspace_task_row_snapshot,
        workspace_task_row_snapshot_with_fingerprint,
        workspace_task_row_snapshot_with_running_lookup, workspace_task_run_snapshot_command,
        workspace_task_selected_row, workspace_task_selected_row_snapshot,
        workspace_task_visible_display_rows, workspace_task_visible_row_bounds,
    };
    use crate::workspace_tasks_runtime::{RunningWorkspaceTask, workspace_task_fingerprint};
    use kuroya_core::{Command, WorkspaceTask, WorkspaceTaskKind};
    use std::{cell::Cell, collections::BTreeMap};

    #[test]
    fn workspace_task_snapshot_commands_reuse_precomputed_fingerprint() {
        let fingerprint = 42;
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 3,
            fingerprint,
            session_id: 9,
        }];

        assert_eq!(
            workspace_task_run_snapshot_command(3, fingerprint, true, false),
            Some(Command::RunWorkspaceTaskSnapshot {
                index: 3,
                fingerprint,
            })
        );
        assert_eq!(
            workspace_task_cancel_snapshot_command(3, fingerprint, true, false, &running_tasks),
            Some(Command::CancelWorkspaceTaskSnapshot {
                index: 3,
                fingerprint,
            })
        );
    }

    #[test]
    fn workspace_task_snapshot_commands_stay_disabled_when_untrusted_or_loading() {
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 0,
            fingerprint: 7,
            session_id: 9,
        }];

        assert_eq!(
            workspace_task_run_snapshot_command(0, 7, false, false),
            None
        );
        assert_eq!(workspace_task_run_snapshot_command(0, 7, true, true), None);
        assert_eq!(
            workspace_task_cancel_snapshot_command(0, 7, false, false, &running_tasks),
            None
        );
        assert_eq!(
            workspace_task_cancel_snapshot_command(0, 7, true, true, &running_tasks),
            None
        );
    }

    #[test]
    fn workspace_task_fingerprint_running_check_does_not_need_task_value() {
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 1,
            fingerprint: 11,
            session_id: 9,
        }];

        assert!(workspace_task_fingerprint_is_running(1, 11, &running_tasks));
        assert!(!workspace_task_fingerprint_is_running(
            1,
            12,
            &running_tasks
        ));
        assert!(!workspace_task_fingerprint_is_running(
            2,
            11,
            &running_tasks
        ));
        assert!(!workspace_task_fingerprint_is_running(1, 11, &[]));
    }

    #[test]
    fn workspace_task_display_row_reuses_sanitized_label_and_snapshot() {
        let mut task = workspace_task_for_panel_test("Build\n\u{202e}All");
        task.kind = WorkspaceTaskKind::Test;
        task.default = true;
        task.command = "cargo\n\u{202e}test".to_owned();
        task.args = vec!["--message".to_owned(), "hello\tworld".to_owned()];
        let fingerprint = workspace_task_fingerprint(&task);
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 5,
            fingerprint,
            session_id: 9,
        }];

        let row = workspace_task_display_row(5, &task, &running_tasks);

        assert_eq!(row.index, 5);
        assert_eq!(
            row.snapshot(),
            WorkspaceTaskRowSnapshot {
                fingerprint: Some(fingerprint),
                running: true
            }
        );
        assert_eq!(row.label(), workspace_task_label(&task, true));
        assert!(row.label().starts_with("test default running  Build All  "));
        assert!(!row.label().chars().any(char::is_control));
        assert!(!row.label().contains('\u{202e}'));
    }

    #[test]
    fn workspace_task_display_row_preserves_raw_task_for_snapshot_commands() {
        let mut task = workspace_task_for_panel_test("Raw Command");
        task.command = "cargo\n\u{202e}raw".to_owned();
        task.args = vec![
            "run\tmode".to_owned(),
            "\u{2066}actual-arg\u{2069}".to_owned(),
        ];
        let raw_command = task.command.clone();
        let raw_args = task.args.clone();
        let fingerprint = workspace_task_fingerprint(&task);
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 0,
            fingerprint,
            session_id: 9,
        }];

        let row = workspace_task_display_row(0, &task, &running_tasks);

        assert_eq!(task.command, raw_command);
        assert_eq!(task.args, raw_args);
        assert!(!row.label().contains(&raw_command));
        assert!(!row.label().chars().any(char::is_control));
        assert_eq!(row.fingerprint_or_compute(&task), fingerprint);
        assert_eq!(
            workspace_task_run_snapshot_command(
                row.index,
                row.fingerprint_or_compute(&task),
                true,
                false,
            ),
            Some(Command::RunWorkspaceTaskSnapshot {
                index: 0,
                fingerprint,
            })
        );
        assert_eq!(
            workspace_task_cancel_snapshot_command(
                row.index,
                row.fingerprint_or_compute(&task),
                true,
                false,
                &running_tasks,
            ),
            Some(Command::CancelWorkspaceTaskSnapshot {
                index: 0,
                fingerprint,
            })
        );
    }

    #[test]
    fn workspace_task_visible_display_rows_prepare_labels_for_requested_range() {
        let hidden_task = workspace_task_for_panel_test("Hidden");
        let mut running_task = workspace_task_for_panel_test("Build");
        running_task.command = "cargo\ncheck".to_owned();
        let default_task = WorkspaceTask {
            name: "Default".to_owned(),
            default: true,
            ..workspace_task_for_panel_test("Default")
        };
        let tasks = vec![hidden_task, running_task, default_task];
        let raw_running_command = tasks[1].command.clone();
        let fingerprint = workspace_task_fingerprint(&tasks[1]);
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 1,
            fingerprint,
            session_id: 9,
        }];

        let rows = workspace_task_visible_display_rows(&tasks, 1..3, &running_tasks);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].index, 1);
        assert_eq!(rows[1].index, 2);
        assert_eq!(rows[0].label(), workspace_task_label(&tasks[1], true));
        assert_eq!(rows[1].label(), workspace_task_label(&tasks[2], false));
        assert_eq!(
            rows[0].snapshot(),
            WorkspaceTaskRowSnapshot {
                fingerprint: Some(fingerprint),
                running: true
            }
        );
        assert_eq!(tasks[1].command, raw_running_command);
        assert!(!rows[0].label().contains('\n'));
    }

    #[test]
    fn workspace_task_visible_display_rows_bound_large_requested_range_to_task_slice() {
        let tasks = vec![
            workspace_task_for_panel_test("Build"),
            workspace_task_for_panel_test("Test"),
        ];
        let fingerprint = workspace_task_fingerprint(&tasks[1]);
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 1,
            fingerprint,
            session_id: 9,
        }];

        assert_eq!(
            workspace_task_visible_row_bounds(tasks.len(), 1..usize::MAX),
            1..2
        );

        let rows = workspace_task_visible_display_rows(&tasks, 1..usize::MAX, &running_tasks);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].index, 1);
        assert_eq!(rows[0].label(), workspace_task_label(&tasks[1], true));
        assert_eq!(
            rows[0].snapshot(),
            WorkspaceTaskRowSnapshot {
                fingerprint: Some(fingerprint),
                running: true
            }
        );
    }

    #[test]
    fn workspace_task_row_snapshot_skips_fingerprint_when_nothing_is_running() {
        let fingerprint_calls = Cell::new(0);

        let snapshot = workspace_task_row_snapshot_with_fingerprint(0, &[], || {
            fingerprint_calls.set(fingerprint_calls.get() + 1);
            7
        });

        assert_eq!(
            snapshot,
            WorkspaceTaskRowSnapshot {
                fingerprint: None,
                running: false
            }
        );
        assert_eq!(fingerprint_calls.get(), 0);
    }

    #[test]
    fn workspace_task_row_snapshot_caches_fingerprint_for_running_checks() {
        let task = workspace_task_for_panel_test("Build");
        let fingerprint = workspace_task_fingerprint(&task);
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 2,
            fingerprint,
            session_id: 9,
        }];

        assert_eq!(
            workspace_task_row_snapshot(2, &task, &running_tasks),
            WorkspaceTaskRowSnapshot {
                fingerprint: Some(fingerprint),
                running: true
            }
        );
        assert_eq!(
            workspace_task_row_snapshot(3, &task, &running_tasks),
            WorkspaceTaskRowSnapshot {
                fingerprint: Some(fingerprint),
                running: false
            }
        );
    }

    #[test]
    fn workspace_task_running_lookup_matches_running_snapshot_checks() {
        let tasks = (0..10)
            .map(|index| workspace_task_for_panel_test(&format!("Task {index}")))
            .collect::<Vec<_>>();
        let running_tasks = vec![
            RunningWorkspaceTask {
                task_index: 1,
                fingerprint: workspace_task_fingerprint(&tasks[1]),
                session_id: 9,
            },
            RunningWorkspaceTask {
                task_index: 4,
                fingerprint: workspace_task_fingerprint(&tasks[4]),
                session_id: 10,
            },
            RunningWorkspaceTask {
                task_index: 8,
                fingerprint: workspace_task_fingerprint(&tasks[8]),
                session_id: 11,
            },
        ];
        let lookup = WorkspaceTaskRunningLookup::new(&running_tasks, tasks.len());

        for (index, task) in tasks.iter().enumerate() {
            assert_eq!(
                workspace_task_row_snapshot_with_running_lookup(index, task, &lookup),
                workspace_task_row_snapshot(index, task, &running_tasks),
            );
        }
    }

    #[test]
    fn workspace_task_selected_row_snapshot_reuses_visible_snapshot_for_same_index() {
        let task = workspace_task_for_panel_test("Build");
        let fingerprint = workspace_task_fingerprint(&task);
        let visible_snapshot = WorkspaceTaskRowSnapshot {
            fingerprint: Some(fingerprint),
            running: true,
        };
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 4,
            fingerprint,
            session_id: 9,
        }];

        assert_eq!(
            workspace_task_selected_row_snapshot(
                4,
                &task,
                Some((4, visible_snapshot)),
                &running_tasks,
            ),
            visible_snapshot
        );
        assert_eq!(
            workspace_task_selected_row_snapshot(4, &task, Some((3, visible_snapshot)), &[]),
            WorkspaceTaskRowSnapshot {
                fingerprint: None,
                running: false
            }
        );
    }

    #[test]
    fn workspace_task_selected_row_snapshot_recomputes_stale_same_index_snapshot() {
        let task = workspace_task_for_panel_test("Build");
        let stale_visible_snapshot = WorkspaceTaskRowSnapshot {
            fingerprint: Some(99),
            running: true,
        };

        assert_eq!(
            workspace_task_selected_row_snapshot(0, &task, Some((0, stale_visible_snapshot)), &[]),
            WorkspaceTaskRowSnapshot {
                fingerprint: None,
                running: false
            }
        );

        let tasks = vec![task.clone()];
        let selected =
            workspace_task_selected_row(&tasks, 0, Some((0, stale_visible_snapshot)), &[])
                .expect("valid selected task");

        assert!(!selected.running());
        assert_eq!(selected.fingerprint(), workspace_task_fingerprint(&task));
        assert_eq!(selected.cancel_command(true, false), None);
    }

    #[test]
    fn workspace_task_selected_row_uses_current_running_state_for_hidden_row() {
        let tasks = vec![
            workspace_task_for_panel_test("Visible"),
            workspace_task_for_panel_test("Hidden Selected"),
        ];
        let fingerprint = workspace_task_fingerprint(&tasks[1]);
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 1,
            fingerprint,
            session_id: 9,
        }];

        let selected = workspace_task_selected_row(&tasks, 1, None, &running_tasks)
            .expect("hidden selected task still exists");

        assert!(selected.running());
        assert_eq!(
            selected.cancel_command(true, false),
            Some(Command::CancelWorkspaceTaskSnapshot {
                index: 1,
                fingerprint,
            })
        );
    }

    #[test]
    fn workspace_task_selected_row_rejects_invalid_selection() {
        let tasks = vec![workspace_task_for_panel_test("Build")];

        assert!(
            workspace_task_selected_row(
                &tasks,
                1,
                Some((
                    1,
                    WorkspaceTaskRowSnapshot {
                        fingerprint: Some(99),
                        running: true
                    }
                )),
                &[]
            )
            .is_none()
        );
    }

    #[test]
    fn workspace_task_selected_row_ignores_snapshot_from_other_row() {
        let task = workspace_task_for_panel_test("Build");
        let tasks = vec![task.clone()];
        let stale_visible_snapshot = WorkspaceTaskRowSnapshot {
            fingerprint: Some(99),
            running: true,
        };

        let selected =
            workspace_task_selected_row(&tasks, 0, Some((1, stale_visible_snapshot)), &[])
                .expect("valid selected task");

        assert_eq!(selected.index, 0);
        assert!(!selected.running());
        assert_eq!(selected.fingerprint(), workspace_task_fingerprint(&task));
        assert_eq!(
            selected.cancel_command(true, false),
            None,
            "a stale visible running snapshot must not enable cancel"
        );
    }

    #[test]
    fn workspace_task_selected_row_commands_use_raw_task_fingerprint() {
        let mut task = workspace_task_for_panel_test("Raw\nName");
        task.command = "cargo\n\u{202e}raw".to_owned();
        task.args = vec!["run\tmode".to_owned()];
        let fingerprint = workspace_task_fingerprint(&task);
        let tasks = vec![task];
        let visible_snapshot = WorkspaceTaskRowSnapshot {
            fingerprint: Some(fingerprint),
            running: true,
        };
        let running_tasks = vec![RunningWorkspaceTask {
            task_index: 0,
            fingerprint,
            session_id: 9,
        }];

        let selected =
            workspace_task_selected_row(&tasks, 0, Some((0, visible_snapshot)), &running_tasks)
                .expect("valid selected task");

        assert_eq!(
            selected.run_command(true, false),
            Some(Command::RunWorkspaceTaskSnapshot {
                index: 0,
                fingerprint,
            })
        );
        assert_eq!(
            selected.cancel_command(true, false),
            Some(Command::CancelWorkspaceTaskSnapshot {
                index: 0,
                fingerprint,
            })
        );
    }

    #[test]
    fn workspace_task_row_snapshot_fingerprint_or_compute_lazily_fills_empty_snapshot() {
        let task = workspace_task_for_panel_test("Build");
        let snapshot = WorkspaceTaskRowSnapshot {
            fingerprint: None,
            running: false,
        };

        assert_eq!(
            snapshot.fingerprint_or_compute(&task),
            workspace_task_fingerprint(&task)
        );
    }

    fn workspace_task_for_panel_test(name: &str) -> WorkspaceTask {
        WorkspaceTask {
            name: name.to_owned(),
            command: "cargo".to_owned(),
            args: vec!["check".to_owned()],
            cwd: None,
            env: BTreeMap::new(),
            kind: WorkspaceTaskKind::Build,
            default: false,
        }
    }
}

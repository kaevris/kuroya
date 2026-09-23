use crate::{
    KuroyaApp,
    native_paths::normalize_native_path,
    path_display::sanitized_display_label_cow,
    quick_open::normalize_quick_open_workspace_path,
    ui_icons::{IconKind, icon_text_button},
    workspace_tasks_runtime::workspace_task_fingerprint,
};
use eframe::egui::{
    self, Align2, Color32, ColorImage, RichText, Sense, Stroke, TextStyle, TextureHandle,
    TextureOptions, pos2, vec2,
};
use image::ImageFormat;
use kuroya_core::{
    Command, WorkspaceTask, WorkspaceTaskKind, workspace_task_command_preview,
    workspace_task_kind_title,
};
use std::{
    borrow::Cow,
    collections::{HashSet, VecDeque},
    path::{Component, Path, PathBuf},
};

const DASHBOARD_MAX_WIDTH: f32 = 640.0;
const DASHBOARD_ACTION_GAP: f32 = 10.0;
const DASHBOARD_RECENT_FILE_LIMIT: usize = 6;
const DASHBOARD_RECENT_PROJECT_LIMIT: usize = 6;
const DASHBOARD_WORKSPACE_TASK_LIMIT: usize = 4;
const DASHBOARD_WORKSPACE_TASK_SCAN_MAX: usize = 256;
const DASHBOARD_LABEL_MAX_CHARS: usize = 48;
const DASHBOARD_DETAIL_MAX_CHARS: usize = 62;
const DASHBOARD_RECENT_CANDIDATE_SCAN_MIN: usize = 32;
const DASHBOARD_RECENT_CANDIDATE_SCAN_MAX: usize = 128;
const DASHBOARD_DISPLAY_TEXT_CACHE_ID: &str = "kuroya_dashboard_display_text_cache";
const DASHBOARD_DISPLAY_TEXT_CACHE_LIMIT: usize = 48;
const DASHBOARD_LOGO_SIZE: f32 = 68.0;
const DASHBOARD_LOGO_BYTES: &[u8] = include_bytes!("../../../assets/logos/kuroya-mark.png");
const DASHBOARD_WORKSPACE_TASKS_UNTRUSTED_LABEL: &str =
    "Workspace tasks hidden until this workspace is trusted";
const DASHBOARD_WORKSPACE_TASKS_EMPTY_LABEL: &str = "No workspace tasks found";

#[cfg(test)]
fn dashboard_logo_size_for_test() -> f32 {
    DASHBOARD_LOGO_SIZE
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardRecentFile {
    pub(crate) path: PathBuf,
    pub(crate) label: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardWorkspaceTask {
    pub(crate) index: usize,
    pub(crate) fingerprint: u64,
    pub(crate) label: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DashboardRecentProject {
    pub(crate) path: PathBuf,
    pub(crate) label: String,
    pub(crate) detail: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct DashboardRecentProjects {
    pub(crate) projects: Vec<DashboardRecentProject>,
    pub(crate) skipped_current_count: usize,
    pub(crate) stale_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DashboardDisplayText {
    label: String,
    detail: String,
}

struct DashboardDisplaySections {
    recent_files: Vec<DashboardRecentFile>,
    workspace_tasks: Option<Vec<DashboardWorkspaceTask>>,
    recent_projects: DashboardRecentProjects,
}

#[derive(Clone, Default)]
struct DashboardDisplayTextCache {
    recent_files: VecDeque<DashboardRecentFileDisplayTextCacheEntry>,
    recent_projects: VecDeque<DashboardRecentProjectDisplayTextCacheEntry>,
    workspace_tasks: VecDeque<DashboardWorkspaceTaskDisplayTextCacheEntry>,
}

#[derive(Clone)]
struct DashboardRecentFileDisplayTextCacheEntry {
    workspace_root: PathBuf,
    path: PathBuf,
    text: DashboardDisplayText,
}

#[derive(Clone)]
struct DashboardRecentProjectDisplayTextCacheEntry {
    path: PathBuf,
    display_path: String,
    text: DashboardDisplayText,
}

#[derive(Clone)]
struct DashboardWorkspaceTaskDisplayTextCacheEntry {
    task: WorkspaceTask,
    text: DashboardDisplayText,
}

impl DashboardDisplayTextCache {
    fn recent_file_display_text(
        &mut self,
        workspace_root: &Path,
        path: &Path,
    ) -> DashboardDisplayText {
        if let Some(entry) = self.recent_files.iter().find(|entry| {
            entry.workspace_root.as_path() == workspace_root && entry.path.as_path() == path
        }) {
            return entry.text.clone();
        }

        let text = dashboard_recent_file_display_text(workspace_root, path);
        push_dashboard_display_text_cache_entry(
            &mut self.recent_files,
            DashboardRecentFileDisplayTextCacheEntry {
                workspace_root: workspace_root.to_path_buf(),
                path: path.to_path_buf(),
                text: text.clone(),
            },
        );
        text
    }

    #[cfg(test)]
    fn recent_project_display_text(
        &mut self,
        path: &Path,
        path_display: &str,
    ) -> DashboardDisplayText {
        self.recent_project_display_text_from_display_path(path, path_display.to_owned())
    }

    fn recent_project_display_text_from_display_path(
        &mut self,
        path: &Path,
        path_display: String,
    ) -> DashboardDisplayText {
        if let Some(entry) = self.recent_projects.iter().find(|entry| {
            entry.path.as_path() == path && entry.display_path == path_display.as_str()
        }) {
            return entry.text.clone();
        }

        let text = dashboard_recent_project_display_text(path, &path_display);
        push_dashboard_display_text_cache_entry(
            &mut self.recent_projects,
            DashboardRecentProjectDisplayTextCacheEntry {
                path: path.to_path_buf(),
                display_path: path_display,
                text: text.clone(),
            },
        );
        text
    }

    fn workspace_task_display_text(&mut self, task: &WorkspaceTask) -> DashboardDisplayText {
        if let Some(entry) = self
            .workspace_tasks
            .iter()
            .find(|entry| &entry.task == task)
        {
            return entry.text.clone();
        }

        let text = dashboard_workspace_task_display_text(task);
        push_dashboard_display_text_cache_entry(
            &mut self.workspace_tasks,
            DashboardWorkspaceTaskDisplayTextCacheEntry {
                task: task.clone(),
                text: text.clone(),
            },
        );
        text
    }
}

fn push_dashboard_display_text_cache_entry<T>(entries: &mut VecDeque<T>, entry: T) {
    if entries.len() >= DASHBOARD_DISPLAY_TEXT_CACHE_LIMIT {
        entries.pop_front();
    }
    entries.push_back(entry);
}

impl KuroyaApp {
    pub(crate) fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        let full_width = ui.available_width();
        let content_width = full_width.clamp(320.0, DASHBOARD_MAX_WIDTH);
        let left_pad = ((full_width - content_width) * 0.5).max(0.0);
        let top_pad = (ui.available_height() * 0.14).clamp(32.0, 84.0);
        let dashboard = cached_dashboard_display_sections(
            ui.ctx(),
            &self.workspace.root,
            &self.quick_open_recent_files,
            self.workspace_trusted,
            &self.workspace_tasks,
            &self.recent_projects,
        );

        ui.add_space(top_pad);
        ui.horizontal(|ui| {
            ui.add_space(left_pad);
            ui.vertical(|ui| {
                ui.set_width(content_width);
                render_dashboard_title(ui, &mut self.dashboard_logo_texture);
                ui.add_space(22.0);
                self.render_dashboard_actions(ui, content_width);
                ui.add_space(22.0);
                render_section_rule(ui, "Recent Files");
                ui.add_space(8.0);
                if let Some(label) = dashboard_recent_files_empty_label(&dashboard.recent_files) {
                    render_empty_state_label(ui, label);
                } else {
                    self.render_recent_files(ui, content_width, &dashboard.recent_files);
                }
                ui.add_space(22.0);
                render_section_rule(ui, "Workspace Tasks");
                ui.add_space(8.0);
                match &dashboard.workspace_tasks {
                    Some(workspace_tasks) => {
                        if let Some(label) =
                            dashboard_workspace_tasks_empty_label(true, workspace_tasks)
                        {
                            render_empty_state_label(ui, label);
                        } else {
                            self.render_workspace_tasks(ui, content_width, workspace_tasks);
                        }
                    }
                    None => {
                        render_empty_state_label(ui, DASHBOARD_WORKSPACE_TASKS_UNTRUSTED_LABEL);
                    }
                }
                ui.add_space(22.0);
                render_section_rule(ui, "Recent Projects");
                ui.add_space(8.0);
                if let Some(label) =
                    dashboard_recent_projects_empty_label(&dashboard.recent_projects)
                {
                    render_empty_state_label(ui, label);
                } else {
                    self.render_recent_projects(
                        ui,
                        content_width,
                        &dashboard.recent_projects.projects,
                    );
                }
            });
        });
    }

    fn render_dashboard_actions(&mut self, ui: &mut egui::Ui, content_width: f32) {
        let two_columns = content_width >= 560.0;
        let action_width = if two_columns {
            ((content_width - DASHBOARD_ACTION_GAP) * 0.5).floor()
        } else {
            content_width
        };

        if two_columns {
            ui.horizontal(|ui| {
                if dashboard_action(
                    ui,
                    IconKind::NewFile,
                    "New File",
                    "Create an untitled buffer",
                    action_width,
                ) {
                    self.command_bus.push(Command::NewFile);
                }
                ui.add_space(DASHBOARD_ACTION_GAP);
                if dashboard_action(
                    ui,
                    IconKind::Search,
                    "Quick Open",
                    "Find a workspace file",
                    action_width,
                ) {
                    self.command_bus.push(Command::ToggleQuickOpen);
                }
            });
            ui.add_space(DASHBOARD_ACTION_GAP);
            ui.horizontal(|ui| {
                if dashboard_action(
                    ui,
                    IconKind::FolderOpen,
                    "Open Folder",
                    "Choose a project folder",
                    action_width,
                ) {
                    self.command_bus.push(Command::OpenWorkspacePrompt);
                }
                ui.add_space(DASHBOARD_ACTION_GAP);
                if dashboard_action(
                    ui,
                    IconKind::Refresh,
                    "Reindex Workspace",
                    "Refresh files and git state",
                    action_width,
                ) {
                    self.spawn_index();
                    self.spawn_git_scan();
                }
            });
        } else {
            if dashboard_action(
                ui,
                IconKind::NewFile,
                "New File",
                "Create an untitled buffer",
                action_width,
            ) {
                self.command_bus.push(Command::NewFile);
            }
            ui.add_space(DASHBOARD_ACTION_GAP);
            if dashboard_action(
                ui,
                IconKind::Search,
                "Quick Open",
                "Find a workspace file",
                action_width,
            ) {
                self.command_bus.push(Command::ToggleQuickOpen);
            }
            ui.add_space(DASHBOARD_ACTION_GAP);
            if dashboard_action(
                ui,
                IconKind::FolderOpen,
                "Open Folder",
                "Choose a project folder",
                action_width,
            ) {
                self.command_bus.push(Command::OpenWorkspacePrompt);
            }
            ui.add_space(DASHBOARD_ACTION_GAP);
            if dashboard_action(
                ui,
                IconKind::Refresh,
                "Reindex Workspace",
                "Refresh files and git state",
                action_width,
            ) {
                self.spawn_index();
                self.spawn_git_scan();
            }
        }
    }

    fn render_workspace_tasks(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        tasks: &[DashboardWorkspaceTask],
    ) {
        let task_width = content_width.min(520.0);
        for task in tasks {
            if icon_text_button(
                ui,
                IconKind::Terminal,
                &task.label,
                Some(task.detail.as_str()),
                task_width,
            )
            .clicked()
            {
                self.command_bus.push(Command::RunWorkspaceTaskSnapshot {
                    index: task.index,
                    fingerprint: task.fingerprint,
                });
            }
            ui.add_space(6.0);
        }
    }

    fn render_recent_files(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        files: &[DashboardRecentFile],
    ) {
        let file_width = content_width.min(520.0);
        for file in files {
            if icon_text_button(
                ui,
                IconKind::File,
                &file.label,
                Some(file.detail.as_str()),
                file_width,
            )
            .clicked()
            {
                self.command_bus.push(Command::OpenFile(file.path.clone()));
            }
            ui.add_space(6.0);
        }
    }

    fn render_recent_projects(
        &mut self,
        ui: &mut egui::Ui,
        content_width: f32,
        projects: &[DashboardRecentProject],
    ) {
        let project_width = content_width.min(520.0);
        for project in projects {
            if icon_text_button(
                ui,
                IconKind::Folder,
                &project.label,
                Some(project.detail.as_str()),
                project_width,
            )
            .clicked()
            {
                self.command_bus
                    .push(Command::OpenWorkspace(project.path.clone()));
            }
            ui.add_space(6.0);
        }
    }
}

fn cached_dashboard_display_sections(
    ctx: &egui::Context,
    workspace_root: &Path,
    recent_files: &VecDeque<PathBuf>,
    workspace_trusted: bool,
    workspace_tasks: &[WorkspaceTask],
    recent_projects: &[PathBuf],
) -> DashboardDisplaySections {
    ctx.data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<DashboardDisplayTextCache>(egui::Id::new(
            DASHBOARD_DISPLAY_TEXT_CACHE_ID,
        ));
        let recent_files = dashboard_recent_files_with_display_cache(
            workspace_root,
            recent_files,
            DASHBOARD_RECENT_FILE_LIMIT,
            cache,
        );
        let workspace_tasks = workspace_trusted.then(|| {
            dashboard_workspace_tasks_with_display_cache(
                workspace_tasks,
                DASHBOARD_WORKSPACE_TASK_LIMIT,
                cache,
            )
        });
        let recent_projects = dashboard_recent_projects_with_display_cache(
            workspace_root,
            recent_projects,
            DASHBOARD_RECENT_PROJECT_LIMIT,
            cache,
        );

        DashboardDisplaySections {
            recent_files,
            workspace_tasks,
            recent_projects,
        }
    })
}

#[cfg(test)]
pub(crate) fn dashboard_recent_files(
    workspace_root: &Path,
    recent_files: &VecDeque<PathBuf>,
    limit: usize,
) -> Vec<DashboardRecentFile> {
    let mut display_text = |workspace_root: &Path, path: &Path| {
        dashboard_recent_file_display_text(workspace_root, path)
    };
    dashboard_recent_files_with_display_text(workspace_root, recent_files, limit, &mut display_text)
}

fn dashboard_recent_files_with_display_cache(
    workspace_root: &Path,
    recent_files: &VecDeque<PathBuf>,
    limit: usize,
    cache: &mut DashboardDisplayTextCache,
) -> Vec<DashboardRecentFile> {
    let mut display_text =
        |workspace_root: &Path, path: &Path| cache.recent_file_display_text(workspace_root, path);
    dashboard_recent_files_with_display_text(workspace_root, recent_files, limit, &mut display_text)
}

fn dashboard_recent_files_with_display_text(
    workspace_root: &Path,
    recent_files: &VecDeque<PathBuf>,
    limit: usize,
    display_text: &mut impl FnMut(&Path, &Path) -> DashboardDisplayText,
) -> Vec<DashboardRecentFile> {
    let mut file_probe = |path: &Path| path.is_file();
    dashboard_recent_files_with_display_text_and_file_probe(
        workspace_root,
        recent_files,
        limit,
        display_text,
        &mut file_probe,
    )
}

#[cfg(test)]
fn dashboard_recent_files_with_file_probe(
    workspace_root: &Path,
    recent_files: &VecDeque<PathBuf>,
    limit: usize,
    file_probe: &mut impl FnMut(&Path) -> bool,
) -> Vec<DashboardRecentFile> {
    let mut display_text = |workspace_root: &Path, path: &Path| {
        dashboard_recent_file_display_text(workspace_root, path)
    };
    dashboard_recent_files_with_display_text_and_file_probe(
        workspace_root,
        recent_files,
        limit,
        &mut display_text,
        file_probe,
    )
}

fn dashboard_recent_files_with_display_text_and_file_probe(
    workspace_root: &Path,
    recent_files: &VecDeque<PathBuf>,
    limit: usize,
    display_text: &mut impl FnMut(&Path, &Path) -> DashboardDisplayText,
    file_probe: &mut impl FnMut(&Path) -> bool,
) -> Vec<DashboardRecentFile> {
    if limit == 0 {
        return Vec::new();
    }

    let capacity = limit.min(recent_files.len());
    let mut files: Vec<DashboardRecentFile> = Vec::with_capacity(capacity);
    let scan_limit = dashboard_recent_candidate_scan_limit(limit, recent_files.len());
    let mut seen = HashSet::with_capacity(scan_limit);
    for path in recent_files.iter().take(scan_limit) {
        if files.len() >= limit {
            break;
        }
        let Some(path) = normalize_quick_open_workspace_path(workspace_root, path) else {
            continue;
        };
        let path_key = dashboard_path_key_from_display_string(path.display().to_string());
        if !seen.insert(path_key) {
            continue;
        }
        if !file_probe(&path) {
            continue;
        }

        let display = display_text(workspace_root, &path);
        files.push(DashboardRecentFile {
            path,
            label: display.label,
            detail: display.detail,
        });
    }
    files
}

#[cfg(test)]
pub(crate) fn dashboard_workspace_tasks(
    tasks: &[WorkspaceTask],
    limit: usize,
) -> Vec<DashboardWorkspaceTask> {
    let mut display_text = |task: &WorkspaceTask| dashboard_workspace_task_display_text(task);
    dashboard_workspace_tasks_with_display_text(tasks, limit, &mut display_text)
}

fn dashboard_workspace_tasks_with_display_cache(
    tasks: &[WorkspaceTask],
    limit: usize,
    cache: &mut DashboardDisplayTextCache,
) -> Vec<DashboardWorkspaceTask> {
    let mut display_text = |task: &WorkspaceTask| cache.workspace_task_display_text(task);
    dashboard_workspace_tasks_with_display_text(tasks, limit, &mut display_text)
}

fn dashboard_workspace_tasks_with_display_text(
    tasks: &[WorkspaceTask],
    limit: usize,
    display_text: &mut impl FnMut(&WorkspaceTask) -> DashboardDisplayText,
) -> Vec<DashboardWorkspaceTask> {
    if limit == 0 {
        return Vec::new();
    }

    let scan_len = dashboard_workspace_task_scan_len(tasks.len());
    if scan_len == 0 {
        return Vec::new();
    }

    let tasks = &tasks[..scan_len];
    let capacity = limit.min(scan_len);
    let mut selected: Vec<DashboardWorkspaceTask> = Vec::with_capacity(capacity);
    let mut selected_indices = [false; DASHBOARD_WORKSPACE_TASK_SCAN_MAX];
    for index in dashboard_workspace_task_default_indices(tasks)
        .into_iter()
        .flatten()
    {
        if selected.len() >= limit {
            break;
        }
        push_dashboard_task(
            tasks,
            index,
            &mut selected,
            &mut selected_indices,
            display_text,
        );
    }

    for index in 0..scan_len {
        if selected.len() >= limit {
            break;
        }
        push_dashboard_task(
            tasks,
            index,
            &mut selected,
            &mut selected_indices,
            display_text,
        );
    }

    selected
}

fn dashboard_workspace_task_scan_len(task_count: usize) -> usize {
    task_count.min(DASHBOARD_WORKSPACE_TASK_SCAN_MAX)
}

#[cfg(test)]
fn dashboard_workspace_tasks_if_trusted(
    workspace_trusted: bool,
    tasks: &[WorkspaceTask],
    limit: usize,
) -> Option<Vec<DashboardWorkspaceTask>> {
    if !workspace_trusted {
        return None;
    }

    Some(dashboard_workspace_tasks(tasks, limit))
}

fn dashboard_workspace_task_default_indices(tasks: &[WorkspaceTask]) -> [Option<usize>; 3] {
    let mut defaults = [None; 3];
    let mut first_by_kind = [None; 3];
    for (index, task) in tasks.iter().enumerate() {
        let Some(slot) = dashboard_workspace_task_kind_slot(task.kind) else {
            continue;
        };
        first_by_kind[slot].get_or_insert(index);
        if task.default {
            defaults[slot].get_or_insert(index);
        }
    }

    [
        defaults[0].or(first_by_kind[0]),
        defaults[1].or(first_by_kind[1]),
        defaults[2].or(first_by_kind[2]),
    ]
}

fn dashboard_workspace_task_kind_slot(kind: WorkspaceTaskKind) -> Option<usize> {
    match kind {
        WorkspaceTaskKind::Build => Some(0),
        WorkspaceTaskKind::Test => Some(1),
        WorkspaceTaskKind::Run => Some(2),
        WorkspaceTaskKind::Custom => None,
    }
}

#[cfg(test)]
pub(crate) fn dashboard_recent_projects(
    current_workspace_root: &Path,
    recent_projects: &[PathBuf],
    limit: usize,
) -> DashboardRecentProjects {
    let mut display_text = |path: &Path, path_display: String| {
        dashboard_recent_project_display_text_from_display_string(path, path_display)
    };
    dashboard_recent_projects_with_display_text(
        current_workspace_root,
        recent_projects,
        limit,
        &mut display_text,
    )
}

fn dashboard_recent_projects_with_display_cache(
    current_workspace_root: &Path,
    recent_projects: &[PathBuf],
    limit: usize,
    cache: &mut DashboardDisplayTextCache,
) -> DashboardRecentProjects {
    let mut display_text = |path: &Path, path_display: String| {
        cache.recent_project_display_text_from_display_path(path, path_display)
    };
    dashboard_recent_projects_with_display_text(
        current_workspace_root,
        recent_projects,
        limit,
        &mut display_text,
    )
}

fn dashboard_recent_projects_with_display_text(
    current_workspace_root: &Path,
    recent_projects: &[PathBuf],
    limit: usize,
    display_text: &mut impl FnMut(&Path, String) -> DashboardDisplayText,
) -> DashboardRecentProjects {
    let mut dir_probe = |path: &Path| path.is_dir();
    dashboard_recent_projects_with_display_text_and_dir_probe(
        current_workspace_root,
        recent_projects,
        limit,
        display_text,
        &mut dir_probe,
    )
}

#[cfg(test)]
fn dashboard_recent_projects_with_dir_probe(
    current_workspace_root: &Path,
    recent_projects: &[PathBuf],
    limit: usize,
    dir_probe: &mut impl FnMut(&Path) -> bool,
) -> DashboardRecentProjects {
    let mut display_text = |path: &Path, path_display: String| {
        dashboard_recent_project_display_text_from_display_string(path, path_display)
    };
    dashboard_recent_projects_with_display_text_and_dir_probe(
        current_workspace_root,
        recent_projects,
        limit,
        &mut display_text,
        dir_probe,
    )
}

fn dashboard_recent_projects_with_display_text_and_dir_probe(
    current_workspace_root: &Path,
    recent_projects: &[PathBuf],
    limit: usize,
    display_text: &mut impl FnMut(&Path, String) -> DashboardDisplayText,
    dir_probe: &mut impl FnMut(&Path) -> bool,
) -> DashboardRecentProjects {
    if limit == 0 {
        return DashboardRecentProjects::default();
    }

    let current_key = dashboard_path_key(current_workspace_root);
    let mut summary = DashboardRecentProjects::default();
    let capacity = limit.min(recent_projects.len());
    let mut projects: Vec<DashboardRecentProject> = Vec::with_capacity(capacity);
    let mut seen = HashSet::with_capacity(capacity);
    let scan_limit = dashboard_recent_candidate_scan_limit(limit, recent_projects.len());
    let mut stale_probe_cache = HashSet::with_capacity(scan_limit);
    for path in recent_projects.iter().take(scan_limit) {
        if projects.len() >= limit {
            break;
        }
        let path = dashboard_lexical_normalize_path(&normalize_native_path(path.clone()));
        let path_display = path.display().to_string();
        let path_key = dashboard_path_key_from_display(&path_display);
        if path_key == current_key {
            summary.skipped_current_count += 1;
            continue;
        }
        if seen.contains(&path_key) {
            continue;
        }
        if stale_probe_cache.contains(&path_key) {
            summary.stale_count += 1;
            continue;
        }

        if !dir_probe(&path) {
            stale_probe_cache.insert(path_key);
            summary.stale_count += 1;
            continue;
        }

        seen.insert(path_key);

        let display = display_text(&path, path_display);
        projects.push(DashboardRecentProject {
            path,
            label: display.label,
            detail: display.detail,
        });
    }
    summary.projects = projects;
    summary
}

fn dashboard_recent_files_empty_label(files: &[DashboardRecentFile]) -> Option<&'static str> {
    files
        .is_empty()
        .then_some("No recent files in this workspace")
}

fn dashboard_workspace_tasks_empty_label(
    workspace_trusted: bool,
    tasks: &[DashboardWorkspaceTask],
) -> Option<&'static str> {
    if !workspace_trusted {
        return Some(DASHBOARD_WORKSPACE_TASKS_UNTRUSTED_LABEL);
    }

    tasks
        .is_empty()
        .then_some(DASHBOARD_WORKSPACE_TASKS_EMPTY_LABEL)
}

fn dashboard_recent_projects_empty_label(
    recent_projects: &DashboardRecentProjects,
) -> Option<&'static str> {
    if !recent_projects.projects.is_empty() {
        return None;
    }
    if recent_projects.stale_count > 0 {
        return Some("Recent workspaces no longer exist");
    }
    if recent_projects.skipped_current_count > 0 {
        return Some("No other recent workspaces");
    }

    Some("No recent workspaces yet")
}

fn push_dashboard_task(
    tasks: &[WorkspaceTask],
    index: usize,
    selected: &mut Vec<DashboardWorkspaceTask>,
    selected_indices: &mut [bool; DASHBOARD_WORKSPACE_TASK_SCAN_MAX],
    display_text: &mut impl FnMut(&WorkspaceTask) -> DashboardDisplayText,
) {
    let Some(task) = tasks.get(index) else {
        return;
    };
    let Some(was_selected) = selected_indices.get_mut(index) else {
        return;
    };
    if *was_selected {
        return;
    }
    *was_selected = true;

    let display = display_text(task);
    selected.push(DashboardWorkspaceTask {
        index,
        fingerprint: workspace_task_fingerprint(task),
        label: display.label,
        detail: display.detail,
    });
}

fn dashboard_recent_candidate_scan_limit(limit: usize, candidate_count: usize) -> usize {
    if limit == 0 || candidate_count == 0 {
        return 0;
    }

    limit
        .saturating_mul(16)
        .clamp(
            DASHBOARD_RECENT_CANDIDATE_SCAN_MIN,
            DASHBOARD_RECENT_CANDIDATE_SCAN_MAX,
        )
        .min(candidate_count)
}

fn render_dashboard_title(ui: &mut egui::Ui, logo_texture: &mut Option<TextureHandle>) {
    ui.horizontal(|ui| {
        render_brand_mark(ui, logo_texture);
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.add_space(1.0);
            ui.label(RichText::new("Kuroya").size(30.0).strong());
            ui.label(
                RichText::new("A better code editor, built in Rust")
                    .size(13.0)
                    .color(muted_text(ui)),
            );
        });
    });
}

fn render_brand_mark(ui: &mut egui::Ui, logo_texture: &mut Option<TextureHandle>) {
    if logo_texture.is_none() {
        *logo_texture = load_dashboard_logo_texture(ui.ctx());
    }

    if let Some(texture) = logo_texture.as_ref() {
        ui.add(
            egui::Image::from_texture(texture)
                .fit_to_exact_size(vec2(DASHBOARD_LOGO_SIZE, DASHBOARD_LOGO_SIZE))
                .alt_text("Kuroya logo"),
        )
        .on_hover_text("Kuroya");
    } else {
        ui.allocate_exact_size(
            vec2(DASHBOARD_LOGO_SIZE, DASHBOARD_LOGO_SIZE),
            Sense::hover(),
        );
    }
}

fn load_dashboard_logo_texture(ctx: &egui::Context) -> Option<TextureHandle> {
    let image = dashboard_logo_color_image()?;

    Some(ctx.load_texture("kuroya-dashboard-logo-mark", image, TextureOptions::LINEAR))
}

fn dashboard_logo_color_image() -> Option<ColorImage> {
    let decoded =
        image::load_from_memory_with_format(DASHBOARD_LOGO_BYTES, ImageFormat::Png).ok()?;
    let rgba = decoded.into_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();
    Some(ColorImage::from_rgba_unmultiplied(
        [usize::try_from(width).ok()?, usize::try_from(height).ok()?],
        &pixels,
    ))
}

fn dashboard_action(
    ui: &mut egui::Ui,
    icon: IconKind,
    label: &str,
    detail: &str,
    width: f32,
) -> bool {
    icon_text_button(ui, icon, label, Some(detail), width).clicked()
}

fn render_section_rule(ui: &mut egui::Ui, label: &str) {
    let available = ui.available_width();
    let height = 22.0;
    let (rect, _) = ui.allocate_exact_size(vec2(available, height), Sense::hover());
    let text_color = muted_text(ui);
    let font = TextStyle::Small.resolve(ui.style());
    let text_width = ui
        .fonts_mut(|fonts| fonts.layout_no_wrap(label.to_owned(), font.clone(), text_color))
        .rect
        .width();
    let line_start = (rect.left() + text_width + 16.0).min(rect.right());
    if line_start < rect.right() {
        ui.painter().line_segment(
            [
                pos2(line_start, rect.center().y),
                pos2(rect.right(), rect.center().y),
            ],
            Stroke::new(1.0_f32, ui.visuals().widgets.inactive.bg_stroke.color),
        );
    }
    ui.painter().text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        label,
        font,
        text_color,
    );
}

fn render_empty_state_label(ui: &mut egui::Ui, label: &str) {
    ui.label(RichText::new(label).small().color(muted_text(ui)));
}

#[cfg(test)]
fn dashboard_path_detail(path: &Path) -> String {
    sanitized_dashboard_detail_string(path.display().to_string(), ".")
}

fn dashboard_file_label(path: &Path) -> String {
    dashboard_leaf_label(path, "File")
}

fn dashboard_file_detail(workspace_root: &Path, path: &Path) -> String {
    let display_path = path.strip_prefix(workspace_root).unwrap_or(path);
    sanitized_dashboard_detail_string(display_path.display().to_string(), ".")
}

fn dashboard_recent_file_display_text(workspace_root: &Path, path: &Path) -> DashboardDisplayText {
    DashboardDisplayText {
        label: dashboard_file_label(path),
        detail: dashboard_file_detail(workspace_root, path),
    }
}

fn dashboard_project_label(path: &Path) -> String {
    dashboard_leaf_label(path, "Workspace")
}

fn dashboard_recent_project_display_text(path: &Path, path_display: &str) -> DashboardDisplayText {
    DashboardDisplayText {
        label: dashboard_project_label(path),
        detail: sanitized_dashboard_detail(path_display, "."),
    }
}

#[cfg(test)]
fn dashboard_recent_project_display_text_from_display_string(
    path: &Path,
    path_display: String,
) -> DashboardDisplayText {
    DashboardDisplayText {
        label: dashboard_project_label(path),
        detail: sanitized_dashboard_detail_string(path_display, "."),
    }
}

fn dashboard_task_label(task: &WorkspaceTask) -> String {
    let kind = workspace_task_kind_title(task.kind);
    let default_suffix = if task.default { " default" } else { "" };
    let mut label = String::with_capacity(kind.len() + default_suffix.len() + 1 + task.name.len());
    label.push_str(kind);
    label.push_str(default_suffix);
    label.push(' ');
    label.push_str(&task.name);
    match sanitized_dashboard_label_cow(&label, "Workspace task") {
        Cow::Borrowed(_) => label,
        Cow::Owned(label) => label,
    }
}

fn dashboard_task_detail(task: &WorkspaceTask) -> String {
    let detail = workspace_task_command_preview(task);
    match sanitized_dashboard_detail_cow(&detail, "task command") {
        Cow::Borrowed(_) => detail,
        Cow::Owned(detail) => detail,
    }
}

fn dashboard_workspace_task_display_text(task: &WorkspaceTask) -> DashboardDisplayText {
    DashboardDisplayText {
        label: dashboard_task_label(task),
        detail: dashboard_task_detail(task),
    }
}

fn dashboard_leaf_label(path: &Path, fallback: &str) -> String {
    let label = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback);
    sanitized_dashboard_label(label, fallback)
}

fn sanitized_dashboard_label(value: &str, fallback: &str) -> String {
    sanitized_dashboard_label_cow(value, fallback).into_owned()
}

fn sanitized_dashboard_label_cow<'a>(value: &'a str, fallback: &str) -> Cow<'a, str> {
    sanitized_display_label_cow(value, DASHBOARD_LABEL_MAX_CHARS, fallback)
}

fn sanitized_dashboard_detail(value: &str, fallback: &str) -> String {
    sanitized_dashboard_detail_cow(value, fallback).into_owned()
}

fn sanitized_dashboard_detail_string(value: String, fallback: &str) -> String {
    match sanitized_dashboard_detail_cow(&value, fallback) {
        Cow::Borrowed(_) => value,
        Cow::Owned(value) => value,
    }
}

fn sanitized_dashboard_detail_cow<'a>(value: &'a str, fallback: &str) -> Cow<'a, str> {
    sanitized_display_label_cow(value, DASHBOARD_DETAIL_MAX_CHARS, fallback)
}

fn dashboard_path_key(path: &Path) -> String {
    dashboard_path_key_from_display_string(
        dashboard_lexical_normalize_path(&normalize_native_path(path.to_path_buf()))
            .display()
            .to_string(),
    )
}

fn dashboard_path_key_from_display(display: &str) -> String {
    if cfg!(windows) {
        if display.is_ascii() {
            let mut key = display.to_owned();
            key.make_ascii_lowercase();
            key
        } else {
            display.to_lowercase()
        }
    } else {
        display.to_owned()
    }
}

fn dashboard_path_key_from_display_string(mut display: String) -> String {
    if cfg!(windows) {
        if display.is_ascii() {
            display.make_ascii_lowercase();
            display
        } else {
            display.to_lowercase()
        }
    } else {
        display
    }
}

fn dashboard_lexical_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                );
                if can_pop {
                    normalized.pop();
                } else if !normalized.has_root() {
                    normalized.push("..");
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn muted_text(ui: &egui::Ui) -> Color32 {
    ui.visuals().weak_text_color.unwrap_or_else(|| {
        blend_color(
            ui.visuals().text_color(),
            ui.visuals().extreme_bg_color,
            0.45,
        )
    })
}

fn blend_color(base: Color32, overlay: Color32, amount: f32) -> Color32 {
    let mix = |a: u8, b: u8| a as f32 + ((b as f32 - a as f32) * amount.clamp(0.0, 1.0));
    Color32::from_rgb(
        mix(base.r(), overlay.r()).round() as u8,
        mix(base.g(), overlay.g()).round() as u8,
        mix(base.b(), overlay.b()).round() as u8,
    )
}

#[cfg(test)]
mod tests;

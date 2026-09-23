use crate::{
    KuroyaApp,
    editor_tabs::{
        buffer_tab_close_tooltip, buffer_tab_display_name, buffer_tab_label_from_display_name,
    },
    file_runtime::file_path_open_buffer_or_known_openable,
    git_diff_state::DiffBufferSource,
    path_clipboard::{PathCopyKind, copy_path_to_clipboard},
    status_bar::items::git_status_count_badge_label,
    ui_icons::{IconKind, draw_icon, icon_button},
    ui_text::count_label,
};
use eframe::egui::{
    self, Align, Color32, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextStyle, Ui, pos2,
    vec2,
};
use kuroya_core::{BufferId, Command, GitChangeStage, GitFileStatus, GitStatusEntry, TextBuffer};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

const TAB_MIN_WIDTH: f32 = 72.0;
const TAB_MAX_WIDTH: f32 = 220.0;
const TAB_ACTIONS_RESERVED_WIDTH: f32 = 228.0;
const TAB_ROW_HEIGHT: f32 = 34.0;

impl KuroyaApp {
    pub(crate) fn render_tabs(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            let available_width = ui.available_width();
            let actions_width = responsive_tab_actions_width(available_width);
            let tab_area_width = (available_width - actions_width).max(0.0);
            let tab_max_width = responsive_tab_max_width(tab_area_width, self.buffers.len());

            let tabs = self.prepare_tab_rows();

            ui.allocate_ui_with_layout(
                vec2(tab_area_width, TAB_ROW_HEIGHT),
                egui::Layout::left_to_right(Align::Center),
                |ui| {
                    egui::ScrollArea::horizontal()
                        .id_salt("main_open_file_tabs_scroll")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .auto_shrink([false, false])
                        .max_height(TAB_ROW_HEIGHT)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for tab_state in tabs {
                                    let id = tab_state.id;
                                    debug_assert_eq!(
                                        tab_state.path_capabilities,
                                        TabPathCapabilities::default()
                                    );
                                    let tab = file_tab(
                                        ui,
                                        &tab_state.label,
                                        &tab_state.name,
                                        tab_state.selected,
                                        tab_state.changed_on_disk,
                                        tab_max_width,
                                    );
                                    if tab.close_clicked {
                                        if self.tab_action_is_current(&tab_state) {
                                            self.request_close_buffer(id);
                                        }
                                    } else if tab.tab_clicked {
                                        if self.tab_action_is_current(&tab_state) {
                                            self.set_active_buffer(id);
                                        }
                                    }
                                    tab.response.context_menu(|ui| {
                                        let mut path_exists_cache = HashMap::with_capacity(
                                            tab_path_openability_cache_capacity(
                                                &tab_state,
                                                self.explorer_compare_path.as_deref(),
                                            ),
                                        );
                                        let tab_action_current =
                                            self.tab_action_is_current(&tab_state);
                                        if !tab_action_current {
                                            ui.label(RichText::new("Tab changed").small());
                                        }
                                        if tab_action_current {
                                            if ui.button("Save").clicked() {
                                                self.spawn_save(id);
                                                ui.close();
                                            }
                                            if ui.button("Reload from Disk").clicked() {
                                                self.begin_reload_buffer_from_disk(id);
                                                ui.close();
                                            }
                                            if ui
                                                .button(if tab_state.read_only {
                                                    "Disable Read Only"
                                                } else {
                                                    "Enable Read Only"
                                                })
                                                .clicked()
                                            {
                                                self.toggle_buffer_read_only(id);
                                                ui.close();
                                            }
                                            let can_copy_patch =
                                                self.can_copy_diff_buffer_patch(id);
                                            if tab_state.diff_source.is_some() || can_copy_patch {
                                                ui.separator();
                                                let diff_capabilities = {
                                                    let indexed_files = self.index.files();
                                                    let buffers = &self.buffers;
                                                    let mut path_exists = TabPathOpenabilityProbe {
                                                        cache: &mut path_exists_cache,
                                                        buffers,
                                                        indexed_files,
                                                    };
                                                    prepare_tab_path_capabilities(
                                                        &tab_state,
                                                        TabCapabilityRequests::DIFF_SOURCE_ACTIONS,
                                                        None,
                                                        &mut path_exists,
                                                    )
                                                };
                                                let stage = tab_state
                                                    .diff_source
                                                    .as_ref()
                                                    .and_then(|source| source.hunk_stage);
                                                let swap_enabled =
                                                    tab_state.diff_source.as_ref().is_some_and(
                                                        |source| source.base_path.is_some(),
                                                    );
                                                visit_diff_tab_context_actions(
                                                    stage,
                                                    diff_capabilities.diff_source_exists,
                                                    can_copy_patch,
                                                    tab_state.diff_source.is_some(),
                                                    tab_state.diff_source.is_some(),
                                                    swap_enabled,
                                                    |action| {
                                                        if ui
                                                            .button(diff_tab_context_action_label(
                                                                action,
                                                            ))
                                                            .clicked()
                                                        {
                                                            match action {
                                            DiffTabContextActionKind::RefreshDiff => {
                                                self.refresh_diff_buffer(id);
                                            }
                                            DiffTabContextActionKind::SwapCompareSides => {
                                                self.swap_diff_sides(id);
                                            }
                                            DiffTabContextActionKind::CopyPatch => {
                                                self.copy_diff_buffer_patch(ui.ctx(), id);
                                            }
                                            DiffTabContextActionKind::CopyCurrentHunkPatch => {
                                                self.set_active_buffer(id);
                                                self.copy_diff_buffer_hunk_patch(ui.ctx(), id);
                                            }
                                            DiffTabContextActionKind::OpenSourceAtCurrentHunk => {
                                                self.set_active_buffer(id);
                                                self.open_active_diff_hunk_source();
                                            }
                                            DiffTabContextActionKind::OpenBaseFile => {
                                                self.open_diff_base_file(id);
                                            }
                                            DiffTabContextActionKind::OpenBaseAtCurrentHunk => {
                                                self.set_active_buffer(id);
                                                self.open_active_diff_hunk_base();
                                            }
                                            DiffTabContextActionKind::OpenSourceFile => {
                                                self.open_diff_source_file(id);
                                            }
                                            DiffTabContextActionKind::PreviousDiffHunk
                                            | DiffTabContextActionKind::NextDiffHunk => {
                                                self.set_active_buffer(id);
                                                if let Some(command) =
                                                    diff_tab_context_action_command(
                                                        action,
                                                        tab_state.diff_source.as_ref(),
                                                    )
                                                {
                                                    self.command_bus.push(command);
                                                }
                                            }
                                            _ => {
                                                if diff_tab_context_action_requires_active_buffer(
                                                    action,
                                                ) {
                                                    self.set_active_buffer(id);
                                                }
                                                if let Some(source) = tab_state.diff_source.as_ref()
                                                {
                                                    if let Some(command) =
                                                        diff_tab_context_action_command(
                                                            action,
                                                            Some(source),
                                                        )
                                                    {
                                                        self.command_bus.push(command);
                                                    }
                                                }
                                            }
                                        }
                                                            ui.close();
                                                        }
                                                    },
                                                );
                                                ui.separator();
                                            }
                                            if let Some(path) = tab_state.context_path.as_ref() {
                                                if tab_state.has_unstaged_changes
                                                    && ui.button("Open Changes").clicked()
                                                {
                                                    if tab_state.file_path.as_ref() == Some(path) {
                                                        self.open_buffer_changes(id);
                                                    } else {
                                                        self.command_bus.push(
                                                            Command::OpenFileChanges(path.clone()),
                                                        );
                                                    }
                                                    ui.close();
                                                }
                                                if tab_state.has_staged_changes
                                                    && ui.button("Open Staged Changes").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::OpenStagedFileChanges(
                                                            path.clone(),
                                                        ),
                                                    );
                                                    ui.close();
                                                }
                                                if tab_state.has_unstaged_changes
                                                    && ui.button("Open Hunks").clicked()
                                                {
                                                    self.command_bus
                                                        .push(Command::OpenFileHunks(path.clone()));
                                                    ui.close();
                                                }
                                                if tab_state.has_staged_changes
                                                    && ui.button("Open Staged Hunks").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::OpenStagedFileHunks(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                                if tab_state.has_unstaged_changes
                                                    && ui.button("Copy Patch").clicked()
                                                {
                                                    self.command_bus
                                                        .push(Command::CopyFilePatch(path.clone()));
                                                    ui.close();
                                                }
                                                if tab_state.has_staged_changes
                                                    && ui.button("Copy Staged Patch").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::CopyStagedFilePatch(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                            }
                                            if let Some(path) = tab_state.file_path.as_ref()
                                                && ui.button("Open Blame").clicked()
                                            {
                                                self.command_bus
                                                    .push(Command::OpenFileBlame(path.clone()));
                                                ui.close();
                                            }
                                            if let Some(path) = tab_state.context_path.as_ref() {
                                                let compare_capabilities = {
                                                    let indexed_files = self.index.files();
                                                    let buffers = &self.buffers;
                                                    let mut path_exists = TabPathOpenabilityProbe {
                                                        cache: &mut path_exists_cache,
                                                        buffers,
                                                        indexed_files,
                                                    };
                                                    prepare_tab_path_capabilities(
                                                        &tab_state,
                                                        TabCapabilityRequests::FILE_COMPARE_ACTIONS,
                                                        self.explorer_compare_path.as_deref(),
                                                        &mut path_exists,
                                                    )
                                                };
                                                if compare_capabilities.can_compare_with_saved
                                                    && ui.button("Compare with Saved").clicked()
                                                {
                                                    self.set_active_buffer(id);
                                                    self.command_bus
                                                        .push(Command::CompareActiveFileWithSaved);
                                                    ui.close();
                                                }
                                                if compare_capabilities.can_select_for_compare
                                                    && ui.button("Select for Compare").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::SelectFileForCompare(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                                if compare_capabilities.can_compare_with_selected
                                                    && ui.button("Compare with Selected").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::CompareFileWithSelected(
                                                            path.clone(),
                                                        ),
                                                    );
                                                    ui.close();
                                                }
                                                if tab_state.has_unstaged_changes
                                                    && ui.button("Stage Changes").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::StageFileChange(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                                if tab_state.has_staged_changes
                                                    && ui.button("Unstage Changes").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::UnstageFileChange(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                                if tab_state.has_source_control_changes
                                                    && ui.button("Discard Changes").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::DiscardFileChanges(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                                if tab_state.has_source_control_changes
                                                    && ui
                                                        .button("Reveal in Source Control")
                                                        .clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::RevealFileInSourceControl(
                                                            path.clone(),
                                                        ),
                                                    );
                                                    ui.close();
                                                }
                                                if ui.button("Reveal in Explorer").clicked() {
                                                    self.command_bus.push(
                                                        Command::RevealFileInExplorer(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                                if ui.button("Compare with HEAD").clicked() {
                                                    self.command_bus.push(
                                                        Command::OpenFileHeadChanges(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                                if tab_state.has_head_revision
                                                    && ui.button("Open File at HEAD").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::OpenFileHeadRevision(path.clone()),
                                                    );
                                                    ui.close();
                                                }
                                                if tab_state.has_index_revision
                                                    && ui.button("Open File at Index").clicked()
                                                {
                                                    self.command_bus.push(
                                                        Command::OpenFileIndexRevision(
                                                            path.clone(),
                                                        ),
                                                    );
                                                    ui.close();
                                                }
                                                visit_file_tab_path_context_actions(
                                                    tab_state.file_path.is_some(),
                                                    |action| {
                                                        if ui
                                                            .button(
                                                                file_tab_path_context_action_label(
                                                                    action,
                                                                ),
                                                            )
                                                            .clicked()
                                                        {
                                                            match action {
                                            FileTabPathContextActionKind::CopyPath => {
                                                self.status = copy_path_to_clipboard(
                                                    ui.ctx(),
                                                    &self.workspace.root,
                                                    path,
                                                    PathCopyKind::Absolute,
                                                );
                                            }
                                            FileTabPathContextActionKind::CopyRelativePath => {
                                                self.status = copy_path_to_clipboard(
                                                    ui.ctx(),
                                                    &self.workspace.root,
                                                    path,
                                                    PathCopyKind::Relative,
                                                );
                                            }
                                            FileTabPathContextActionKind::Delete => {
                                                if let Some(file_path) =
                                                    tab_state.file_path.as_ref()
                                                {
                                                    self.command_bus.push(Command::DeletePath(
                                                        file_path.clone(),
                                                    ));
                                                }
                                            }
                                        }
                                                            ui.close();
                                                        }
                                                    },
                                                );
                                            }
                                            if ui.button("Save As").clicked() {
                                                self.begin_save_as(id);
                                                ui.close();
                                            }
                                            if ui.button("Split Right").clicked() {
                                                self.split_buffer_right_from_tab_menu(id);
                                                ui.close();
                                            }
                                            if ui.button("Reset Split Widths").clicked() {
                                                self.reset_pane_weights();
                                                ui.close();
                                            }
                                            if ui.button("Reopen Closed File").clicked() {
                                                self.command_bus.push(Command::ReopenClosedFile);
                                                ui.close();
                                            }
                                            if ui.button("Close").clicked() {
                                                self.request_close_buffer(id);
                                                ui.close();
                                            }
                                            if ui.button("Close Others").clicked() {
                                                self.close_other_buffers(id);
                                                ui.close();
                                            }
                                            if ui.button("Close All").clicked() {
                                                self.close_all_buffers();
                                                ui.close();
                                            }
                                        }
                                    });
                                }
                            });
                        });
                },
            );

            ui.allocate_ui_with_layout(
                vec2(actions_width, TAB_ROW_HEIGHT),
                egui::Layout::right_to_left(Align::Center),
                |ui| {
                    ui.add_space(6.0);
                    if icon_button(ui, IconKind::Command, "Command palette").clicked() {
                        self.command_bus.push(Command::ToggleCommandPalette);
                    }
                    if icon_button(ui, IconKind::Search, "Quick open").clicked() {
                        self.command_bus.push(Command::ToggleQuickOpen);
                    }
                    if icon_button(ui, IconKind::GitBranch, "Source control").clicked() {
                        self.command_bus.push(Command::ToggleSourceControl);
                    }
                    if let Some(badge) = git_status_count_badge_label(
                        self.git.counts(),
                        self.settings.scm_count_badge,
                        self.settings.git_count_badge,
                    ) {
                        ui.label(RichText::new(badge).small());
                    }
                    if icon_button(ui, IconKind::Terminal, "Toggle terminal").clicked() {
                        self.command_bus.push(Command::ToggleTerminal);
                    }
                    if icon_button(ui, IconKind::Settings, "Settings").clicked() {
                        self.command_bus.push(Command::ToggleSettingsPanel);
                    }
                },
            );
        });
    }

    fn prepare_tab_rows(&self) -> Vec<PreparedTabRow> {
        let tab_count = self.buffers.len();
        let mut cache = TabRowPreparationCache::with_capacity(tab_count);
        let git_entries = self.git.entries_slice();
        let mut rows = Vec::with_capacity(tab_count);
        for buffer in &self.buffers {
            rows.push(self.prepare_buffer_tab_row_with_cache(buffer, &mut cache, git_entries));
        }
        rows
    }

    pub(crate) fn close_all_buffers(&mut self) {
        let mut closed = 0;
        let mut pending = Vec::new();
        let has_pending_close_buffers = !self.pending_close_buffers.is_empty();
        let mut index = 0;
        while index < self.buffers.len() {
            let buffer = &self.buffers[index];
            let id = buffer.id();
            if buffer.is_dirty() {
                if self.dirty_close_buffer != Some(id)
                    && (!has_pending_close_buffers || !self.pending_close_buffers.contains(&id))
                {
                    pending.push(id);
                }
                index += 1;
            } else {
                self.force_close_buffer(id);
                closed += 1;
            }
        }
        self.pending_close_buffers.extend(pending);
        if !self.pending_close_buffers.is_empty() {
            self.begin_next_pending_close();
        } else if closed > 0 {
            self.status = format!("Closed {}", count_label(closed, "buffer", "buffers"));
        }
    }

    #[cfg(test)]
    fn prepare_buffer_tab_row(&self, buffer: &TextBuffer) -> PreparedTabRow {
        let mut cache = TabRowPreparationCache::with_capacity(1);
        self.prepare_buffer_tab_row_with_cache(buffer, &mut cache, self.git.entries_slice())
    }

    fn prepare_buffer_tab_row_with_cache<'a>(
        &self,
        buffer: &TextBuffer,
        cache: &mut TabRowPreparationCache<'a>,
        git_entries: &'a [GitStatusEntry],
    ) -> PreparedTabRow {
        let id = buffer.id();
        let name = self.buffer_tab_name_for_row(buffer);
        let dirty = buffer.is_dirty();
        let read_only = buffer.is_read_only();
        let changed_on_disk = self.buffer_has_observed_external_change(id);
        let display = cache.tab_row_display(&name, dirty, changed_on_disk, read_only);
        let file_path_ref = buffer.path().map(PathBuf::as_path);
        let diff_source_ref = self.diff_buffer_sources.get(&id);
        let context_path_ref = tab_context_path_ref(file_path_ref, diff_source_ref);
        let source_control = context_path_ref
            .map(|path| cache.source_control_state(git_entries, path))
            .unwrap_or_default();
        let file_path = file_path_ref.map(Path::to_path_buf);
        let diff_source = diff_source_ref.cloned();
        let context_path = tab_context_path(file_path.as_deref(), diff_source.as_ref());
        prepare_tab_row_with_display(
            TabRowPreparationInput {
                id,
                selected: self.active == Some(id),
                name,
                dirty,
                read_only,
                changed_on_disk,
                file_path,
                context_path,
                diff_source,
                has_unstaged_changes: source_control.has_unstaged_changes,
                has_staged_changes: source_control.has_staged_changes,
                source_control_status: source_control.status,
            },
            display,
            TabCapabilityRequests::ROW,
            Path::exists,
        )
    }

    fn buffer_tab_name_for_row(&self, buffer: &TextBuffer) -> String {
        let id = buffer.id();
        if self.virtual_buffer_labels.contains_key(&id) {
            return self.buffer_tab_name(id);
        }
        buffer
            .path()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "Untitled".to_owned())
    }
}

#[derive(Debug, Default)]
struct TabRowPreparationCache<'a> {
    displays: HashMap<String, TabRowDisplayStates>,
    source_control: Option<HashMap<&'a Path, TabSourceControlState>>,
}

impl<'a> TabRowPreparationCache<'a> {
    fn with_capacity(tab_count: usize) -> Self {
        Self {
            displays: HashMap::with_capacity(tab_count),
            source_control: None,
        }
    }

    fn tab_row_display(
        &mut self,
        name: &str,
        dirty: bool,
        changed_on_disk: bool,
        read_only: bool,
    ) -> TabRowDisplay {
        let state = tab_row_display_state_index(dirty, changed_on_disk, read_only);
        if let Some(display) = self.displays.get(name).and_then(|states| states.get(state)) {
            return display.clone();
        }

        let display = prepare_tab_row_display(name, dirty, changed_on_disk, read_only);
        if let Some(states) = self.displays.get_mut(name) {
            states.set(state, display.clone());
            return display;
        }

        let mut states = TabRowDisplayStates::default();
        states.set(state, display.clone());
        self.displays.insert(name.to_owned(), states);
        display
    }

    fn source_control_state(
        &mut self,
        git_entries: &'a [GitStatusEntry],
        path: &Path,
    ) -> TabSourceControlState {
        if let Some(states) = self.source_control.as_ref() {
            return states.get(path).copied().unwrap_or_default();
        }

        if git_entries.is_empty() {
            return TabSourceControlState::default();
        }

        self.source_control
            .get_or_insert_with(|| tab_source_control_states(git_entries))
            .get(path)
            .copied()
            .unwrap_or_default()
    }

    #[cfg(test)]
    fn display_state_count(&self) -> usize {
        self.displays
            .values()
            .map(TabRowDisplayStates::display_count)
            .sum()
    }
}

const TAB_ROW_DISPLAY_STATE_COUNT: usize = 8;

#[derive(Debug)]
struct TabRowDisplayStates {
    states: [Option<TabRowDisplay>; TAB_ROW_DISPLAY_STATE_COUNT],
}

impl Default for TabRowDisplayStates {
    fn default() -> Self {
        Self {
            states: std::array::from_fn(|_| None),
        }
    }
}

impl TabRowDisplayStates {
    fn get(&self, state: usize) -> Option<&TabRowDisplay> {
        self.states.get(state).and_then(Option::as_ref)
    }

    fn set(&mut self, state: usize, display: TabRowDisplay) {
        if let Some(slot) = self.states.get_mut(state) {
            *slot = Some(display);
        }
    }

    #[cfg(test)]
    fn display_count(&self) -> usize {
        self.states.iter().filter(|state| state.is_some()).count()
    }
}

fn tab_row_display_state_index(dirty: bool, changed_on_disk: bool, read_only: bool) -> usize {
    usize::from(dirty) | (usize::from(changed_on_disk) << 1) | (usize::from(read_only) << 2)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TabSourceControlState {
    has_unstaged_changes: bool,
    has_staged_changes: bool,
    status: Option<GitFileStatus>,
}

fn tab_source_control_states(entries: &[GitStatusEntry]) -> HashMap<&Path, TabSourceControlState> {
    let mut states: HashMap<&Path, TabSourceControlState> = HashMap::with_capacity(entries.len());
    for entry in entries {
        let state = states.entry(entry.path.as_path()).or_default();
        match entry.stage {
            GitChangeStage::Unstaged => state.has_unstaged_changes = true,
            GitChangeStage::Staged => state.has_staged_changes = true,
        }
        if entry.stage == GitChangeStage::Unstaged || state.status.is_none() {
            state.status = Some(entry.status);
        }
    }
    states
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TabRowDisplay {
    name: String,
    label: String,
}

fn prepare_tab_row_display(
    name: &str,
    dirty: bool,
    changed_on_disk: bool,
    read_only: bool,
) -> TabRowDisplay {
    let name = buffer_tab_display_name(name);
    let label = buffer_tab_label_from_display_name(&name, dirty, changed_on_disk, read_only);
    TabRowDisplay { name, label }
}

#[derive(Debug, Clone)]
struct TabRowPreparationInput {
    id: BufferId,
    selected: bool,
    name: String,
    dirty: bool,
    read_only: bool,
    changed_on_disk: bool,
    file_path: Option<PathBuf>,
    context_path: Option<PathBuf>,
    diff_source: Option<DiffBufferSource>,
    has_unstaged_changes: bool,
    has_staged_changes: bool,
    source_control_status: Option<GitFileStatus>,
}

#[derive(Debug, Clone)]
struct PreparedTabRow {
    id: BufferId,
    selected: bool,
    name: String,
    label: String,
    dirty: bool,
    read_only: bool,
    changed_on_disk: bool,
    file_path: Option<PathBuf>,
    context_path: Option<PathBuf>,
    diff_source: Option<DiffBufferSource>,
    has_unstaged_changes: bool,
    has_staged_changes: bool,
    has_source_control_changes: bool,
    has_head_revision: bool,
    has_index_revision: bool,
    path_capabilities: TabPathCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TabCapabilityRequests {
    diff_source_actions: bool,
    file_compare_actions: bool,
}

impl TabCapabilityRequests {
    const ROW: Self = Self {
        diff_source_actions: false,
        file_compare_actions: false,
    };
    const DIFF_SOURCE_ACTIONS: Self = Self {
        diff_source_actions: true,
        file_compare_actions: false,
    };
    const FILE_COMPARE_ACTIONS: Self = Self {
        diff_source_actions: false,
        file_compare_actions: true,
    };
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TabPathCapabilities {
    diff_source_exists: bool,
    can_compare_with_saved: bool,
    can_select_for_compare: bool,
    can_compare_with_selected: bool,
}

fn tab_context_path(
    file_path: Option<&Path>,
    diff_source: Option<&DiffBufferSource>,
) -> Option<PathBuf> {
    tab_context_path_ref(file_path, diff_source).map(Path::to_path_buf)
}

fn tab_context_path_ref<'a>(
    file_path: Option<&'a Path>,
    diff_source: Option<&'a DiffBufferSource>,
) -> Option<&'a Path> {
    file_path.or_else(|| diff_source.map(|source| source.path.as_path()))
}

fn tab_path_exists_cached<'a>(
    cache: &mut HashMap<&'a Path, bool>,
    path: &'a Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    if let Some(exists) = cache.get(path) {
        return *exists;
    }

    let exists = path_exists(path);
    cache.insert(path, exists);
    exists
}

fn tab_path_known_openable_cached<'a>(
    cache: &mut HashMap<&'a Path, bool>,
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &'a Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    tab_path_exists_cached(cache, path, |path| {
        file_path_open_buffer_or_known_openable(buffers, indexed_files, path, path_exists)
    })
}

trait TabPathProbe<'a> {
    fn path_exists(&mut self, path: &'a Path) -> bool;
}

impl<'a, F> TabPathProbe<'a> for F
where
    F: FnMut(&'a Path) -> bool,
{
    fn path_exists(&mut self, path: &'a Path) -> bool {
        self(path)
    }
}

struct TabPathOpenabilityProbe<'a, 'cache> {
    cache: &'cache mut HashMap<&'a Path, bool>,
    buffers: &'cache [TextBuffer],
    indexed_files: &'cache [PathBuf],
}

impl<'a, 'cache> TabPathProbe<'a> for TabPathOpenabilityProbe<'a, 'cache> {
    fn path_exists(&mut self, path: &'a Path) -> bool {
        tab_path_known_openable_cached(
            self.cache,
            self.buffers,
            self.indexed_files,
            path,
            Path::exists,
        )
    }
}

fn tab_path_openability_cache_capacity(
    row: &PreparedTabRow,
    selected_compare_path: Option<&Path>,
) -> usize {
    usize::from(row.diff_source.is_some())
        + usize::from(row.context_path.is_some())
        + usize::from(row.dirty && row.file_path.is_some())
        + usize::from(selected_compare_path.is_some())
}

#[cfg(test)]
fn prepare_tab_row(
    input: TabRowPreparationInput,
    capability_requests: TabCapabilityRequests,
    path_exists: impl FnMut(&Path) -> bool,
) -> PreparedTabRow {
    let display = prepare_tab_row_display(
        &input.name,
        input.dirty,
        input.changed_on_disk,
        input.read_only,
    );
    prepare_tab_row_with_display(input, display, capability_requests, path_exists)
}

fn prepare_tab_row_with_display(
    input: TabRowPreparationInput,
    display: TabRowDisplay,
    capability_requests: TabCapabilityRequests,
    mut path_exists: impl FnMut(&Path) -> bool,
) -> PreparedTabRow {
    debug_assert_eq!(display.name, buffer_tab_display_name(&input.name));
    let has_source_control_changes = input.source_control_status.is_some();
    let has_head_revision = input
        .source_control_status
        .is_some_and(tab_source_control_has_head_revision);
    let has_index_revision = input.source_control_status.is_some_and(|status| {
        tab_source_control_has_index_revision(
            status,
            input.has_unstaged_changes,
            input.has_staged_changes,
        )
    });
    let mut row = PreparedTabRow {
        id: input.id,
        selected: input.selected,
        label: display.label,
        name: display.name,
        dirty: input.dirty,
        read_only: input.read_only,
        changed_on_disk: input.changed_on_disk,
        file_path: input.file_path,
        context_path: input.context_path,
        diff_source: input.diff_source,
        has_unstaged_changes: input.has_unstaged_changes,
        has_staged_changes: input.has_staged_changes,
        has_source_control_changes,
        has_head_revision,
        has_index_revision,
        path_capabilities: TabPathCapabilities::default(),
    };
    row.path_capabilities =
        prepare_tab_path_capabilities(&row, capability_requests, None, &mut path_exists);
    row
}

fn prepare_tab_path_capabilities<'a>(
    row: &'a PreparedTabRow,
    requests: TabCapabilityRequests,
    selected_compare_path: Option<&'a Path>,
    path_exists: &mut impl TabPathProbe<'a>,
) -> TabPathCapabilities {
    let mut capabilities = TabPathCapabilities::default();

    if requests.diff_source_actions {
        capabilities.diff_source_exists = row
            .diff_source
            .as_ref()
            .is_some_and(|source| path_exists.path_exists(&source.path));
    }

    if requests.file_compare_actions {
        let context_exists = row
            .context_path
            .as_ref()
            .map(|path| path_exists.path_exists(path));
        capabilities.can_select_for_compare = context_exists.unwrap_or(false);
        let mut saved_file_path_exists = None;

        if row.dirty {
            capabilities.can_compare_with_saved = match (
                row.file_path.as_ref(),
                row.context_path.as_ref(),
                context_exists,
            ) {
                (Some(file_path), Some(context_path), Some(exists))
                    if file_path == context_path =>
                {
                    exists
                }
                (Some(file_path), _, _) => {
                    let exists = path_exists.path_exists(file_path);
                    saved_file_path_exists = Some((file_path.as_path(), exists));
                    exists
                }
                _ => false,
            };
        }

        capabilities.can_compare_with_selected =
            match (row.context_path.as_ref(), selected_compare_path) {
                (Some(path), Some(selected)) if capabilities.can_select_for_compare => {
                    selected != path
                        && saved_file_path_exists
                            .filter(|(file_path, _)| *file_path == selected)
                            .map(|(_, exists)| exists)
                            .unwrap_or_else(|| path_exists.path_exists(selected))
                }
                _ => false,
            };
    }

    capabilities
}

impl KuroyaApp {
    fn tab_action_is_current(&self, row: &PreparedTabRow) -> bool {
        let Some(buffer) = self.buffer(row.id) else {
            return false;
        };
        tab_action_still_targets_row(row, buffer, self.diff_buffer_sources.get(&row.id))
    }
}

fn tab_action_still_targets_row(
    row: &PreparedTabRow,
    buffer: &TextBuffer,
    diff_source: Option<&DiffBufferSource>,
) -> bool {
    buffer.id() == row.id
        && buffer.path() == row.file_path.as_ref()
        && diff_source == row.diff_source.as_ref()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffTabContextActionKind {
    RefreshDiff,
    CopyPatch,
    CopyCurrentHunkPatch,
    PreviousDiffHunk,
    NextDiffHunk,
    SwapCompareSides,
    OpenBaseFile,
    OpenBaseAtCurrentHunk,
    OpenSourceFile,
    OpenSourceAtCurrentHunk,
    OpenBlame,
    StageCurrentDiffHunk,
    UnstageCurrentDiffHunk,
    DiscardCurrentDiffHunk,
}

fn visit_diff_tab_context_actions(
    stage: Option<GitChangeStage>,
    source_exists: bool,
    patch_enabled: bool,
    refresh_enabled: bool,
    base_enabled: bool,
    swap_enabled: bool,
    mut visit: impl FnMut(DiffTabContextActionKind),
) {
    if refresh_enabled {
        visit(DiffTabContextActionKind::RefreshDiff);
    }
    if swap_enabled {
        visit(DiffTabContextActionKind::SwapCompareSides);
    }
    if patch_enabled {
        visit(DiffTabContextActionKind::CopyPatch);
        visit(DiffTabContextActionKind::CopyCurrentHunkPatch);
        visit(DiffTabContextActionKind::PreviousDiffHunk);
        visit(DiffTabContextActionKind::NextDiffHunk);
    }
    if base_enabled {
        visit(DiffTabContextActionKind::OpenBaseFile);
        if patch_enabled {
            visit(DiffTabContextActionKind::OpenBaseAtCurrentHunk);
        }
    }
    if source_exists {
        visit(DiffTabContextActionKind::OpenSourceFile);
        visit(DiffTabContextActionKind::OpenSourceAtCurrentHunk);
        visit(DiffTabContextActionKind::OpenBlame);
    }
    match stage {
        Some(GitChangeStage::Unstaged) => {
            visit(DiffTabContextActionKind::StageCurrentDiffHunk);
            visit(DiffTabContextActionKind::DiscardCurrentDiffHunk);
        }
        Some(GitChangeStage::Staged) => {
            visit(DiffTabContextActionKind::UnstageCurrentDiffHunk);
        }
        None => {}
    }
}

#[cfg(test)]
pub(crate) fn diff_tab_context_action_labels(
    stage: Option<GitChangeStage>,
    source_exists: bool,
    patch_enabled: bool,
    refresh_enabled: bool,
    base_enabled: bool,
    swap_enabled: bool,
) -> Vec<&'static str> {
    let mut labels = Vec::with_capacity(diff_tab_context_action_count(
        stage,
        source_exists,
        patch_enabled,
        refresh_enabled,
        base_enabled,
        swap_enabled,
    ));
    visit_diff_tab_context_actions(
        stage,
        source_exists,
        patch_enabled,
        refresh_enabled,
        base_enabled,
        swap_enabled,
        |action| labels.push(diff_tab_context_action_label(action)),
    );
    labels
}

#[cfg(test)]
fn diff_tab_context_action_count(
    stage: Option<GitChangeStage>,
    source_exists: bool,
    patch_enabled: bool,
    refresh_enabled: bool,
    base_enabled: bool,
    swap_enabled: bool,
) -> usize {
    usize::from(refresh_enabled)
        + usize::from(swap_enabled)
        + if patch_enabled { 4 } else { 0 }
        + if base_enabled {
            1 + usize::from(patch_enabled)
        } else {
            0
        }
        + if source_exists { 3 } else { 0 }
        + match stage {
            Some(GitChangeStage::Unstaged) => 2,
            Some(GitChangeStage::Staged) => 1,
            None => 0,
        }
}

fn diff_tab_context_action_label(action: DiffTabContextActionKind) -> &'static str {
    match action {
        DiffTabContextActionKind::RefreshDiff => "Refresh Diff",
        DiffTabContextActionKind::SwapCompareSides => "Swap Compare Sides",
        DiffTabContextActionKind::CopyPatch => "Copy Patch",
        DiffTabContextActionKind::CopyCurrentHunkPatch => "Copy Current Hunk Patch",
        DiffTabContextActionKind::PreviousDiffHunk => "Previous Diff Hunk",
        DiffTabContextActionKind::NextDiffHunk => "Next Diff Hunk",
        DiffTabContextActionKind::OpenBaseFile => "Open Diff Base File",
        DiffTabContextActionKind::OpenBaseAtCurrentHunk => "Open Base at Current Hunk",
        DiffTabContextActionKind::OpenSourceFile => "Open Diff Source File",
        DiffTabContextActionKind::OpenSourceAtCurrentHunk => "Open Source at Current Hunk",
        DiffTabContextActionKind::OpenBlame => "Open Blame",
        DiffTabContextActionKind::StageCurrentDiffHunk => "Stage Current Diff Hunk",
        DiffTabContextActionKind::UnstageCurrentDiffHunk => "Unstage Current Diff Hunk",
        DiffTabContextActionKind::DiscardCurrentDiffHunk => "Discard Current Diff Hunk",
    }
}

fn diff_tab_context_action_requires_active_buffer(action: DiffTabContextActionKind) -> bool {
    matches!(
        action,
        DiffTabContextActionKind::StageCurrentDiffHunk
            | DiffTabContextActionKind::OpenBaseAtCurrentHunk
            | DiffTabContextActionKind::OpenSourceAtCurrentHunk
            | DiffTabContextActionKind::CopyCurrentHunkPatch
            | DiffTabContextActionKind::PreviousDiffHunk
            | DiffTabContextActionKind::NextDiffHunk
            | DiffTabContextActionKind::UnstageCurrentDiffHunk
            | DiffTabContextActionKind::DiscardCurrentDiffHunk
    )
}

fn diff_tab_context_action_command(
    action: DiffTabContextActionKind,
    source: Option<&DiffBufferSource>,
) -> Option<Command> {
    match action {
        DiffTabContextActionKind::PreviousDiffHunk => Some(Command::PreviousDiffHunk),
        DiffTabContextActionKind::NextDiffHunk => Some(Command::NextDiffHunk),
        DiffTabContextActionKind::OpenBlame => {
            source.map(|source| Command::OpenFileBlame(source.path.clone()))
        }
        DiffTabContextActionKind::StageCurrentDiffHunk => Some(Command::StageActiveDiffHunk),
        DiffTabContextActionKind::UnstageCurrentDiffHunk => Some(Command::UnstageActiveDiffHunk),
        DiffTabContextActionKind::DiscardCurrentDiffHunk => Some(Command::DiscardActiveDiffHunk),
        DiffTabContextActionKind::RefreshDiff
        | DiffTabContextActionKind::SwapCompareSides
        | DiffTabContextActionKind::CopyPatch
        | DiffTabContextActionKind::CopyCurrentHunkPatch
        | DiffTabContextActionKind::OpenSourceAtCurrentHunk
        | DiffTabContextActionKind::OpenBaseFile
        | DiffTabContextActionKind::OpenBaseAtCurrentHunk
        | DiffTabContextActionKind::OpenSourceFile => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileTabPathContextActionKind {
    CopyPath,
    CopyRelativePath,
    Delete,
}

fn visit_file_tab_path_context_actions(
    can_delete_file: bool,
    mut visit: impl FnMut(FileTabPathContextActionKind),
) {
    visit(FileTabPathContextActionKind::CopyPath);
    visit(FileTabPathContextActionKind::CopyRelativePath);
    if can_delete_file {
        visit(FileTabPathContextActionKind::Delete);
    }
}

fn file_tab_path_context_action_label(action: FileTabPathContextActionKind) -> &'static str {
    match action {
        FileTabPathContextActionKind::CopyPath => "Copy Path",
        FileTabPathContextActionKind::CopyRelativePath => "Copy Relative Path",
        FileTabPathContextActionKind::Delete => "Delete",
    }
}

#[cfg(test)]
pub(crate) fn file_tab_path_context_action_labels(can_delete_file: bool) -> Vec<&'static str> {
    let mut labels = Vec::with_capacity(2 + usize::from(can_delete_file));
    visit_file_tab_path_context_actions(can_delete_file, |action| {
        labels.push(file_tab_path_context_action_label(action));
    });
    labels
}

fn tab_source_control_has_head_revision(status: kuroya_core::GitFileStatus) -> bool {
    !matches!(
        status,
        kuroya_core::GitFileStatus::Added | kuroya_core::GitFileStatus::Untracked
    )
}

fn tab_source_control_has_index_revision(
    status: kuroya_core::GitFileStatus,
    has_unstaged_changes: bool,
    has_staged_changes: bool,
) -> bool {
    let staged_index = has_staged_changes
        && !matches!(
            status,
            kuroya_core::GitFileStatus::Deleted
                | kuroya_core::GitFileStatus::Untracked
                | kuroya_core::GitFileStatus::Conflicted
        );
    let unstaged_index = has_unstaged_changes
        && !matches!(
            status,
            kuroya_core::GitFileStatus::Added
                | kuroya_core::GitFileStatus::Untracked
                | kuroya_core::GitFileStatus::Conflicted
        );
    staged_index || unstaged_index
}

#[cfg(test)]
pub(crate) fn file_tab_compare_context_action_labels(
    has_context_path: bool,
    can_compare_with_saved: bool,
    can_compare_with_selected: bool,
) -> Vec<&'static str> {
    let label_count = if has_context_path {
        1 + usize::from(can_compare_with_saved) + usize::from(can_compare_with_selected)
    } else {
        0
    };
    let mut labels = Vec::with_capacity(label_count);
    if has_context_path && can_compare_with_saved {
        labels.push("Compare with Saved");
    }
    if has_context_path {
        labels.push("Select for Compare");
    }
    if has_context_path && can_compare_with_selected {
        labels.push("Compare with Selected");
    }
    labels
}

struct FileTabResponse {
    response: Response,
    tab_clicked: bool,
    close_clicked: bool,
}

fn file_tab(
    ui: &mut Ui,
    label: &str,
    name: &str,
    selected: bool,
    changed_on_disk: bool,
    max_width: f32,
) -> FileTabResponse {
    let font_id = TextStyle::Button.resolve(ui.style());
    let text_color = ui.visuals().widgets.inactive.fg_stroke.color;
    let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(label.to_owned(), font_id, text_color));
    let width = (galley.rect.width() + 42.0).clamp(TAB_MIN_WIDTH, max_width);
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    let visuals = ui.visuals();
    let close_rect =
        Rect::from_center_size(pos2(rect.right() - 14.0, rect.center().y), vec2(18.0, 18.0));
    let pointer_pos = ui.input(|input| input.pointer.hover_pos());
    let close_hovered =
        response.hovered() && pointer_pos.is_some_and(|pos| close_rect.contains(pos));
    let close_clicked = response.clicked()
        && response
            .interact_pointer_pos()
            .is_some_and(|pos| close_rect.contains(pos));
    let tab_clicked = response.clicked() && !close_clicked;

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
        ui.painter().rect_filled(rect.shrink(1.0), 4.0, fill);
    }
    if selected || response.hovered() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
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

    let text_pos = pos2(
        rect.left() + 10.0,
        rect.center().y - galley.rect.height() / 2.0,
    );
    let text_clip = Rect::from_min_max(
        pos2(rect.left() + 8.0, rect.top()),
        pos2(close_rect.left() - 4.0, rect.bottom()),
    );
    ui.painter()
        .with_clip_rect(text_clip)
        .galley(text_pos, galley, text_color);

    if close_hovered {
        ui.painter()
            .rect_filled(close_rect, 4.0, visuals.widgets.hovered.bg_fill);
        ui.painter().rect_stroke(
            close_rect,
            4.0,
            Stroke::new(1.0, visuals.widgets.hovered.bg_stroke.color),
            StrokeKind::Inside,
        );
    }

    let tint = if close_hovered {
        visuals.widgets.hovered.fg_stroke.color
    } else {
        visuals
            .widgets
            .inactive
            .fg_stroke
            .color
            .gamma_multiply(0.72)
    };
    let icon_rect = Rect::from_center_size(close_rect.center(), vec2(12.0, 12.0));
    draw_icon(ui, icon_rect, IconKind::Close, tint);
    response = if close_hovered {
        response.on_hover_text(buffer_tab_close_tooltip(name))
    } else if changed_on_disk {
        response.on_hover_text("Changed on disk; save or reload to resolve")
    } else {
        response
    };

    FileTabResponse {
        response,
        tab_clicked,
        close_clicked,
    }
}

fn responsive_tab_max_width(available_width: f32, tab_count: usize) -> f32 {
    if tab_count == 0 {
        return TAB_MAX_WIDTH;
    }
    if available_width.is_infinite() && available_width.is_sign_positive() {
        return TAB_MAX_WIDTH;
    }
    if !available_width.is_finite() {
        return TAB_MIN_WIDTH;
    }

    let tab_area_width = available_width.max(TAB_MIN_WIDTH);
    (tab_area_width / tab_count as f32).clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH)
}

fn responsive_tab_actions_width(available_width: f32) -> f32 {
    if !available_width.is_finite() {
        return TAB_ACTIONS_RESERVED_WIDTH;
    }
    available_width.clamp(0.0, TAB_ACTIONS_RESERVED_WIDTH)
}

#[cfg(test)]
mod tests;

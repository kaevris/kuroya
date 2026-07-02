use super::{CommandPaletteCatalogEntry, CommandPaletteResult};
use crate::{command_palette_items::CommandPaletteQueryMemoryEntry, history::NavigationLocation};
use kuroya_core::{Command, PluginCommandRegistry, WorkspaceTask, keymap::KeyBinding};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

pub(super) const COMMAND_PALETTE_RESULT_SNAPSHOT_LIMIT: usize = 8;
pub(super) const COMMAND_PALETTE_RESULT_SNAPSHOT_RESULT_LIMIT: usize = 1024;

#[derive(Debug, Clone, Default)]
pub(crate) struct CommandPaletteResultsCache {
    pub(super) query: String,
    pub(super) git_enabled: bool,
    pub(super) workspace_root: PathBuf,
    pub(super) recent_projects: Vec<PathBuf>,
    pub(super) navigation_back: VecDeque<NavigationLocation>,
    pub(super) navigation_forward: VecDeque<NavigationLocation>,
    pub(super) current_navigation_location: Option<NavigationLocation>,
    pub(super) workspace_tasks: Vec<WorkspaceTask>,
    pub(super) workspace_tasks_runnable: bool,
    pub(super) plugin_commands: PluginCommandRegistry,
    pub(super) keybindings: Vec<KeyBinding>,
    pub(super) commands_catalog: Vec<CommandPaletteCatalogEntry>,
    pub(super) command_recent: VecDeque<Command>,
    pub(super) command_query_memory: VecDeque<CommandPaletteQueryMemoryEntry>,
    pub(super) summary_label: String,
    pub(super) empty_label: String,
    pub(super) results: Vec<CommandPaletteResult>,
    pub(super) result_snapshots: VecDeque<CommandPaletteResultsSnapshot>,
}

#[derive(Debug, Clone)]
pub(super) struct CommandPaletteResultsSnapshot {
    pub(super) query: String,
    pub(super) git_enabled: bool,
    pub(super) workspace_tasks_runnable: bool,
    pub(super) command_recent: VecDeque<Command>,
    pub(super) command_query_memory: VecDeque<CommandPaletteQueryMemoryEntry>,
    pub(super) summary_label: String,
    pub(super) empty_label: String,
    pub(super) results: Vec<CommandPaletteResult>,
}

impl CommandPaletteResultsCache {
    pub(super) fn catalog_matches(
        &self,
        workspace_root: &Path,
        recent_projects: &[PathBuf],
        navigation_back: &VecDeque<NavigationLocation>,
        navigation_forward: &VecDeque<NavigationLocation>,
        current_navigation_location: Option<&NavigationLocation>,
        workspace_tasks: &[WorkspaceTask],
        plugin_commands: &PluginCommandRegistry,
        keybindings: &[KeyBinding],
    ) -> bool {
        self.workspace_root == workspace_root
            && self.recent_projects == recent_projects
            && self.navigation_back.iter().eq(navigation_back.iter())
            && self.navigation_forward.iter().eq(navigation_forward.iter())
            && self.current_navigation_location.as_ref() == current_navigation_location
            && self.workspace_tasks == workspace_tasks
            && &self.plugin_commands == plugin_commands
            && self.keybindings == keybindings
    }

    pub(super) fn matches(
        &self,
        query: &str,
        git_enabled: bool,
        workspace_root: &Path,
        recent_projects: &[PathBuf],
        navigation_back: &VecDeque<NavigationLocation>,
        navigation_forward: &VecDeque<NavigationLocation>,
        current_navigation_location: Option<&NavigationLocation>,
        workspace_tasks: &[WorkspaceTask],
        plugin_commands: &PluginCommandRegistry,
        keybindings: &[KeyBinding],
        workspace_tasks_runnable: bool,
        command_recent: &VecDeque<Command>,
        command_query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
    ) -> bool {
        self.result_state_matches(
            query,
            git_enabled,
            workspace_tasks_runnable,
            command_recent,
            command_query_memory,
        ) && self.catalog_matches(
            workspace_root,
            recent_projects,
            navigation_back,
            navigation_forward,
            current_navigation_location,
            workspace_tasks,
            plugin_commands,
            keybindings,
        )
    }

    fn result_state_matches(
        &self,
        query: &str,
        git_enabled: bool,
        workspace_tasks_runnable: bool,
        command_recent: &VecDeque<Command>,
        command_query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
    ) -> bool {
        self.query == query
            && self.git_enabled == git_enabled
            && self.workspace_tasks_runnable == workspace_tasks_runnable
            && self.command_recent.iter().eq(command_recent.iter())
            && self
                .command_query_memory
                .iter()
                .eq(command_query_memory.iter())
    }

    pub(super) fn current_result_snapshot(&self) -> Option<CommandPaletteResultsSnapshot> {
        if self.results.len() > COMMAND_PALETTE_RESULT_SNAPSHOT_RESULT_LIMIT {
            return None;
        }

        Some(CommandPaletteResultsSnapshot {
            query: self.query.clone(),
            git_enabled: self.git_enabled,
            workspace_tasks_runnable: self.workspace_tasks_runnable,
            command_recent: self.command_recent.clone(),
            command_query_memory: self.command_query_memory.clone(),
            summary_label: self.summary_label.clone(),
            empty_label: self.empty_label.clone(),
            results: self.results.clone(),
        })
    }

    pub(super) fn remember_result_snapshot(
        &mut self,
        snapshot: Option<CommandPaletteResultsSnapshot>,
    ) {
        if COMMAND_PALETTE_RESULT_SNAPSHOT_LIMIT == 0 {
            self.result_snapshots.clear();
            return;
        }
        let Some(snapshot) = snapshot else {
            return;
        };

        if let Some(index) = self
            .result_snapshots
            .iter()
            .position(|cached| cached.same_key_as(&snapshot))
        {
            self.result_snapshots.remove(index);
        }

        self.result_snapshots.push_front(snapshot);
        while self.result_snapshots.len() > COMMAND_PALETTE_RESULT_SNAPSHOT_LIMIT {
            self.result_snapshots.pop_back();
        }
    }

    pub(super) fn restore_result_snapshot(
        &mut self,
        query: &str,
        git_enabled: bool,
        workspace_tasks_runnable: bool,
        command_recent: &VecDeque<Command>,
        command_query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
    ) -> bool {
        let Some(index) = self.result_snapshots.iter().position(|snapshot| {
            snapshot.matches(
                query,
                git_enabled,
                workspace_tasks_runnable,
                command_recent,
                command_query_memory,
            )
        }) else {
            return false;
        };
        if !self.result_snapshot_is_restorable(&self.result_snapshots[index]) {
            self.result_snapshots.remove(index);
            return false;
        }

        let snapshot = self
            .result_snapshots
            .remove(index)
            .expect("snapshot index came from current result_snapshots");
        self.query = snapshot.query;
        self.git_enabled = snapshot.git_enabled;
        self.workspace_tasks_runnable = snapshot.workspace_tasks_runnable;
        self.command_recent = snapshot.command_recent;
        self.command_query_memory = snapshot.command_query_memory;
        self.summary_label = snapshot.summary_label;
        self.empty_label = snapshot.empty_label;
        self.results = snapshot.results;
        true
    }

    fn result_snapshot_is_restorable(&self, snapshot: &CommandPaletteResultsSnapshot) -> bool {
        snapshot.results.len() <= COMMAND_PALETTE_RESULT_SNAPSHOT_RESULT_LIMIT
            && snapshot
                .results
                .iter()
                .all(|result| result.catalog_index < self.commands_catalog.len())
    }
}

impl CommandPaletteResultsSnapshot {
    fn matches(
        &self,
        query: &str,
        git_enabled: bool,
        workspace_tasks_runnable: bool,
        command_recent: &VecDeque<Command>,
        command_query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
    ) -> bool {
        self.query == query
            && self.git_enabled == git_enabled
            && self.workspace_tasks_runnable == workspace_tasks_runnable
            && self.command_recent.iter().eq(command_recent.iter())
            && self
                .command_query_memory
                .iter()
                .eq(command_query_memory.iter())
    }

    fn same_key_as(&self, other: &Self) -> bool {
        self.matches(
            &other.query,
            other.git_enabled,
            other.workspace_tasks_runnable,
            &other.command_recent,
            &other.command_query_memory,
        )
    }
}

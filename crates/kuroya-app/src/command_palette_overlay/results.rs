mod cache;
mod rows;
mod text;

#[cfg(test)]
use crate::command_palette_items::MAX_COMMAND_PALETTE_QUERY_PATTERN_CHARS;
use crate::{
    KuroyaApp,
    command_palette_items::{
        COMMAND_PALETTE_DEFAULT_SECTION, CommandPaletteItem, CommandPaletteQueryMemoryEntry,
        CommandPaletteRanker, command_palette_command_match_score_non_empty,
        command_palette_item_section, command_palette_items,
    },
    command_runtime::command_requires_git,
    history::collect_navigation_locations,
    ui_state::{clamp_selection, handle_list_navigation_keys, selection_page_step},
};
use eframe::egui::{Key, Ui};
use kuroya_core::Command;
use rows::{COMMAND_PALETTE_ROW_HEIGHT, render_command_palette_result_list};
use std::collections::VecDeque;
use text::{
    COMMAND_PALETTE_RESULT_CHORD_LIMIT, COMMAND_PALETTE_RESULT_LABEL_LIMIT,
    command_palette_empty_state_label, command_palette_empty_state_label_into,
    command_palette_match_query, command_palette_result_summary,
    command_palette_result_summary_into, normalize_command_palette_result_text_owned,
};
#[cfg(test)]
use text::{COMMAND_PALETTE_RESULT_TEXT_SCAN_CHARS, normalize_command_palette_result_text};

const COMMAND_PALETTE_RESULT_QUERY_RESERVE_LIMIT: usize = 512;
const COMMAND_PALETTE_RESULT_SECTION_LIMIT: usize = 80;

pub(super) type CommandPaletteCatalogEntry = (String, Command, String, String);

pub(crate) use cache::CommandPaletteResultsCache;
#[cfg(test)]
use cache::CommandPaletteResultsSnapshot;
#[cfg(test)]
use cache::{COMMAND_PALETTE_RESULT_SNAPSHOT_LIMIT, COMMAND_PALETTE_RESULT_SNAPSHOT_RESULT_LIMIT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommandPaletteResult {
    catalog_index: usize,
    score: i64,
    match_score: i64,
}

impl CommandPaletteResult {
    fn catalog_entry<'a>(
        &self,
        commands_catalog: &'a [CommandPaletteCatalogEntry],
    ) -> Option<(&'a str, &'a Command, &'a str, &'a str)> {
        commands_catalog
            .get(self.catalog_index)
            .map(|(label, command, chord, section)| {
                (label.as_str(), command, chord.as_str(), section.as_str())
            })
    }
}

impl KuroyaApp {
    pub(super) fn render_command_palette_results(
        &mut self,
        ui: &mut Ui,
        query_changed: bool,
    ) -> Option<Command> {
        self.refresh_command_palette_results_cache();
        let mut selected = self.command_selected;
        let command_to_run = if let Some(cache) = self.command_palette_results_cache.as_ref() {
            let commands = &cache.results;
            clamp_selection(&mut selected, commands.len());
            let (keyboard_command, selection_changed) = handle_command_palette_result_keys(
                ui,
                commands,
                &cache.commands_catalog,
                &mut selected,
                ui.available_height(),
            );
            keyboard_command.or_else(|| {
                render_command_palette_result_list(
                    ui,
                    commands,
                    &cache.commands_catalog,
                    selected,
                    self.settings.ui_font_size,
                    &cache.summary_label,
                    &cache.empty_label,
                    query_changed || selection_changed,
                )
            })
        } else {
            selected = 0;
            render_command_palette_result_list(
                ui,
                &[],
                &[],
                selected,
                self.settings.ui_font_size,
                "",
                "No commands available",
                query_changed,
            )
        };
        self.command_selected = selected;
        command_to_run
    }

    fn refresh_command_palette_results_cache(&mut self) {
        let current_navigation_location = self.current_navigation_location();
        let workspace_tasks_runnable = self.workspace_trusted && !self.workspace_tasks_loading;
        if self
            .command_palette_results_cache
            .as_ref()
            .is_some_and(|cache| {
                cache.matches(
                    &self.command_query,
                    self.settings.git_enabled,
                    &self.workspace.root,
                    &self.recent_projects,
                    &self.navigation_back,
                    &self.navigation_forward,
                    current_navigation_location.as_ref(),
                    &self.workspace_tasks,
                    &self.plugin_commands,
                    &self.settings.keymap.bindings,
                    workspace_tasks_runnable,
                    &self.command_recent,
                    &self.command_query_memory,
                )
            })
        {
            return;
        }

        if let Some(cache) = self.command_palette_results_cache.as_mut()
            && cache.catalog_matches(
                &self.workspace.root,
                &self.recent_projects,
                &self.navigation_back,
                &self.navigation_forward,
                current_navigation_location.as_ref(),
                &self.workspace_tasks,
                &self.plugin_commands,
                &self.settings.keymap.bindings,
            )
        {
            refresh_cached_command_palette_results(
                &self.matcher,
                self.settings.git_enabled,
                workspace_tasks_runnable,
                &self.command_query,
                &self.command_recent,
                &self.command_query_memory,
                cache,
            );
            return;
        }

        let commands_catalog = self
            .command_palette_results_cache
            .take()
            .filter(|cache| {
                cache.catalog_matches(
                    &self.workspace.root,
                    &self.recent_projects,
                    &self.navigation_back,
                    &self.navigation_forward,
                    current_navigation_location.as_ref(),
                    &self.workspace_tasks,
                    &self.plugin_commands,
                    &self.settings.keymap.bindings,
                )
            })
            .map(|cache| cache.commands_catalog)
            .unwrap_or_else(|| {
                let navigation_locations = collect_navigation_locations(
                    &self.navigation_back,
                    &self.navigation_forward,
                    current_navigation_location.clone(),
                );
                normalized_command_palette_catalog(command_palette_items(
                    &self.workspace.root,
                    &self.recent_projects,
                    &navigation_locations,
                    &self.workspace_tasks,
                    &self.plugin_commands,
                    &self.settings.keymap.bindings,
                ))
            });
        let mut results = Vec::with_capacity(commands_catalog.len());
        filtered_command_palette_results_into(
            &self.matcher,
            self.settings.git_enabled,
            workspace_tasks_runnable,
            &self.command_query,
            &self.command_recent,
            &self.command_query_memory,
            &commands_catalog,
            &mut results,
        );
        let summary_label = command_palette_result_summary(results.len(), &self.command_query);
        let empty_label = command_palette_empty_state_label(&self.command_query);

        self.command_palette_results_cache = Some(CommandPaletteResultsCache {
            query: self.command_query.clone(),
            git_enabled: self.settings.git_enabled,
            workspace_root: self.workspace.root.clone(),
            recent_projects: self.recent_projects.clone(),
            navigation_back: self.navigation_back.clone(),
            navigation_forward: self.navigation_forward.clone(),
            current_navigation_location,
            workspace_tasks: self.workspace_tasks.clone(),
            workspace_tasks_runnable,
            plugin_commands: self.plugin_commands.clone(),
            keybindings: self.settings.keymap.bindings.clone(),
            commands_catalog,
            command_recent: self.command_recent.clone(),
            command_query_memory: self.command_query_memory.clone(),
            summary_label,
            empty_label,
            results,
            result_snapshots: VecDeque::new(),
        });
    }
}

fn refresh_cached_command_palette_results(
    matcher: &fuzzy_matcher::skim::SkimMatcherV2,
    git_enabled: bool,
    workspace_tasks_runnable: bool,
    query: &str,
    command_recent: &VecDeque<Command>,
    command_query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
    cache: &mut CommandPaletteResultsCache,
) {
    let current_snapshot = cache.current_result_snapshot();
    if cache.restore_result_snapshot(
        query,
        git_enabled,
        workspace_tasks_runnable,
        command_recent,
        command_query_memory,
    ) {
        cache.remember_result_snapshot(current_snapshot);
        return;
    }
    cache.remember_result_snapshot(current_snapshot);

    cache.query.clear();
    cache.query.push_str(query);
    cache.git_enabled = git_enabled;
    cache.workspace_tasks_runnable = workspace_tasks_runnable;
    refresh_cached_recent_commands(&mut cache.command_recent, command_recent);
    refresh_cached_query_memory(&mut cache.command_query_memory, command_query_memory);
    filtered_command_palette_results_into(
        matcher,
        git_enabled,
        workspace_tasks_runnable,
        query,
        command_recent,
        command_query_memory,
        &cache.commands_catalog,
        &mut cache.results,
    );
    command_palette_result_summary_into(&mut cache.summary_label, cache.results.len(), query);
    command_palette_empty_state_label_into(&mut cache.empty_label, query);
}

fn refresh_cached_recent_commands(cached: &mut VecDeque<Command>, current: &VecDeque<Command>) {
    if cached.iter().eq(current.iter()) {
        return;
    }

    cached.clear();
    cached.extend(current.iter().cloned());
}

fn refresh_cached_query_memory(
    cached: &mut VecDeque<CommandPaletteQueryMemoryEntry>,
    current: &VecDeque<CommandPaletteQueryMemoryEntry>,
) {
    if cached.iter().eq(current.iter()) {
        return;
    }

    cached.clear();
    cached.extend(current.iter().cloned());
}

fn normalized_command_palette_catalog(
    catalog: Vec<CommandPaletteItem>,
) -> Vec<CommandPaletteCatalogEntry> {
    let mut normalized = Vec::with_capacity(catalog.len());
    for (label, command, chord) in catalog {
        let section = command_palette_item_section(&label, &command);
        let label =
            normalize_command_palette_result_text_owned(label, COMMAND_PALETTE_RESULT_LABEL_LIMIT);
        if label.is_empty() {
            continue;
        }

        if normalized
            .iter()
            .any(|(_, existing_command, _, _)| existing_command == &command)
        {
            continue;
        }

        let chord =
            normalize_command_palette_result_text_owned(chord, COMMAND_PALETTE_RESULT_CHORD_LIMIT);
        let section = normalize_command_palette_result_text_owned(
            section.to_owned(),
            COMMAND_PALETTE_RESULT_SECTION_LIMIT,
        );
        let section = if section.is_empty() {
            COMMAND_PALETTE_DEFAULT_SECTION.to_owned()
        } else {
            section
        };
        normalized.push((label, command, chord, section));
    }
    normalized
}

fn filtered_command_palette_results_into(
    matcher: &fuzzy_matcher::skim::SkimMatcherV2,
    git_enabled: bool,
    workspace_tasks_runnable: bool,
    query: &str,
    command_recent: &VecDeque<Command>,
    command_query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
    commands_catalog: &[CommandPaletteCatalogEntry],
    commands: &mut Vec<CommandPaletteResult>,
) {
    let query = command_palette_match_query(query);
    let query = query.as_ref();
    let query_is_empty = query.is_empty();
    let mut ranker = None;
    let mut ranker_initialized = false;
    commands.clear();
    commands.reserve(if query_is_empty {
        commands_catalog.len()
    } else {
        commands_catalog
            .len()
            .min(COMMAND_PALETTE_RESULT_QUERY_RESERVE_LIMIT)
    });
    for (catalog_index, (label, command, chord, _section)) in commands_catalog.iter().enumerate() {
        if !command_palette_command_visible_with_workspace(
            git_enabled,
            workspace_tasks_runnable,
            command,
        ) {
            continue;
        }

        if label.is_empty() {
            continue;
        }

        let match_score = if query_is_empty {
            0
        } else {
            let Some(match_score) = command_palette_command_match_score_non_empty(
                matcher, label, chord, command, query,
            ) else {
                continue;
            };
            match_score
        };
        if !ranker_initialized {
            ranker =
                CommandPaletteRanker::new_with_bonuses(command_recent, command_query_memory, query);
            ranker_initialized = true;
        }
        let score = ranker.as_ref().map_or(match_score, |ranker| {
            ranker.rank_score(match_score, command)
        });
        commands.push(CommandPaletteResult {
            catalog_index,
            score,
            match_score,
        });
    }
    sort_command_palette_results(commands, commands_catalog);
}

fn handle_command_palette_result_keys(
    ui: &mut Ui,
    commands: &[CommandPaletteResult],
    commands_catalog: &[CommandPaletteCatalogEntry],
    selected: &mut usize,
    viewport_height: f32,
) -> (Option<Command>, bool) {
    let selection_changed = ui.input(|input| {
        handle_list_navigation_keys(
            input,
            selected,
            commands.len(),
            selection_page_step(COMMAND_PALETTE_ROW_HEIGHT, viewport_height),
        )
    });
    if ui.input(|input| input.key_pressed(Key::Enter)) {
        return (
            commands
                .get(*selected)
                .and_then(|result| result.catalog_entry(commands_catalog))
                .map(|(_, command, _, _)| command.clone()),
            selection_changed,
        );
    }
    (None, selection_changed)
}

fn sort_command_palette_results(
    commands: &mut [CommandPaletteResult],
    commands_catalog: &[CommandPaletteCatalogEntry],
) {
    commands.sort_unstable_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.match_score.cmp(&a.match_score))
            .then(
                command_palette_result_section_rank(a, commands_catalog)
                    .cmp(&command_palette_result_section_rank(b, commands_catalog)),
            )
            .then(
                command_palette_result_section(a, commands_catalog)
                    .cmp(command_palette_result_section(b, commands_catalog)),
            )
            .then(
                command_palette_result_label(a, commands_catalog)
                    .cmp(command_palette_result_label(b, commands_catalog)),
            )
            .then(a.catalog_index.cmp(&b.catalog_index))
    });
}

fn command_palette_result_label<'a>(
    result: &CommandPaletteResult,
    commands_catalog: &'a [CommandPaletteCatalogEntry],
) -> &'a str {
    commands_catalog
        .get(result.catalog_index)
        .map_or("", |(label, _, _, _)| label.as_str())
}

fn command_palette_result_section_rank(
    result: &CommandPaletteResult,
    commands_catalog: &[CommandPaletteCatalogEntry],
) -> usize {
    command_palette_section_rank(command_palette_result_section(result, commands_catalog))
}

fn command_palette_result_section<'a>(
    result: &CommandPaletteResult,
    commands_catalog: &'a [CommandPaletteCatalogEntry],
) -> &'a str {
    commands_catalog
        .get(result.catalog_index)
        .map_or("", |(_, _, _, section)| section.as_str())
}

fn command_palette_section_rank(section: &str) -> usize {
    match section {
        "File" => 0,
        "Workspace" => 1,
        "Recent Workspaces" => 2,
        "Edit" => 3,
        "Selection" => 4,
        "View" => 5,
        "Navigation" => 6,
        "Recent Locations" => 7,
        "Search" => 8,
        "Code" => 9,
        "Source Control" => 10,
        "Terminal" => 11,
        "Tasks and Extensions" => 12,
        "Extensions" => 13,
        "Settings" => 14,
        "Developer" => 15,
        "Help" => 16,
        COMMAND_PALETTE_DEFAULT_SECTION => 17,
        _ => 18,
    }
}

#[cfg(test)]
fn command_palette_command_visible(git_enabled: bool, command: &Command) -> bool {
    command_palette_command_visible_with_workspace(git_enabled, true, command)
}

fn command_palette_command_visible_with_workspace(
    git_enabled: bool,
    workspace_tasks_runnable: bool,
    command: &Command,
) -> bool {
    (git_enabled || !command_requires_git(command))
        && (workspace_tasks_runnable || !command_requires_workspace_tasks_runnable(command))
}

fn command_requires_workspace_tasks_runnable(command: &Command) -> bool {
    matches!(
        command,
        Command::RunWorkspaceTask(_)
            | Command::RunWorkspaceTaskSnapshot { .. }
            | Command::CancelWorkspaceTaskSnapshot { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::{
        COMMAND_PALETTE_RESULT_SNAPSHOT_LIMIT, COMMAND_PALETTE_RESULT_SNAPSHOT_RESULT_LIMIT,
        COMMAND_PALETTE_RESULT_TEXT_SCAN_CHARS, CommandPaletteCatalogEntry, CommandPaletteResult,
        CommandPaletteResultsCache, CommandPaletteResultsSnapshot,
        MAX_COMMAND_PALETTE_QUERY_PATTERN_CHARS, command_palette_command_visible,
        command_palette_command_visible_with_workspace, command_palette_empty_state_label,
        command_palette_empty_state_label_into, command_palette_match_query,
        command_palette_result_summary, command_palette_result_summary_into,
        filtered_command_palette_results_into, normalize_command_palette_result_text,
        normalized_command_palette_catalog, refresh_cached_command_palette_results,
        sort_command_palette_results,
    };
    use crate::{
        command_palette_items::{COMMAND_PALETTE_DEFAULT_SECTION, CommandPaletteQueryMemoryEntry},
        history::NavigationLocation,
    };
    use kuroya_core::{Command, PluginCommandRegistry, WorkspaceTaskKind, keymap::KeyBinding};
    use std::{collections::VecDeque, path::PathBuf};

    fn result(catalog_index: usize, score: i64, match_score: i64) -> CommandPaletteResult {
        CommandPaletteResult {
            catalog_index,
            score,
            match_score,
        }
    }

    fn catalog(entries: &[(&str, Command)]) -> Vec<CommandPaletteCatalogEntry> {
        entries
            .iter()
            .map(|(label, command)| {
                (
                    label.to_string(),
                    command.clone(),
                    String::new(),
                    COMMAND_PALETTE_DEFAULT_SECTION.to_owned(),
                )
            })
            .collect()
    }

    fn result_commands(
        results: Vec<CommandPaletteResult>,
        catalog: &[CommandPaletteCatalogEntry],
    ) -> Vec<Command> {
        results
            .into_iter()
            .map(|result| catalog[result.catalog_index].1.clone())
            .collect()
    }

    #[test]
    fn command_palette_results_keep_catalog_order_for_exact_ties() {
        let catalog = catalog(&[
            ("Duplicate", Command::ToggleQuickOpen),
            ("Duplicate", Command::ToggleDevtools),
            ("Duplicate", Command::ToggleTerminal),
        ]);
        let mut results = vec![result(2, 100, 10), result(0, 100, 10), result(1, 100, 10)];

        sort_command_palette_results(&mut results, &catalog);

        assert_eq!(
            result_commands(results, &catalog),
            vec![
                Command::ToggleQuickOpen,
                Command::ToggleDevtools,
                Command::ToggleTerminal
            ]
        );
    }

    #[test]
    fn command_palette_results_rank_scores_before_catalog_order() {
        let catalog = catalog(&[
            ("Alpha", Command::ToggleQuickOpen),
            ("Beta", Command::ToggleTerminal),
            ("Gamma", Command::ToggleDevtools),
        ]);
        let mut results = vec![result(0, 100, 10), result(1, 101, 9), result(2, 100, 11)];

        sort_command_palette_results(&mut results, &catalog);

        assert_eq!(
            result_commands(results, &catalog),
            vec![
                Command::ToggleTerminal,
                Command::ToggleDevtools,
                Command::ToggleQuickOpen
            ]
        );
    }

    #[test]
    fn command_palette_results_cache_matches_current_inputs() {
        let workspace_root = PathBuf::from("workspace");
        let recent_projects = vec![PathBuf::from("other-workspace")];
        let navigation_back = VecDeque::from([NavigationLocation::new(
            workspace_root.join("src/main.rs"),
            3,
            5,
        )]);
        let navigation_forward = VecDeque::new();
        let current_navigation_location =
            NavigationLocation::new(workspace_root.join("src/lib.rs"), 8, 2);
        let keybindings = vec![KeyBinding {
            chord: "Ctrl+P".to_owned(),
            command: Command::ToggleQuickOpen,
        }];
        let command_recent = VecDeque::from([Command::ToggleQuickOpen]);
        let command_query_memory = VecDeque::from([CommandPaletteQueryMemoryEntry {
            query: "open".to_owned(),
            command: Command::ToggleQuickOpen,
            uses: 2,
        }]);
        let cache = CommandPaletteResultsCache {
            query: "open".to_owned(),
            git_enabled: true,
            workspace_root: workspace_root.clone(),
            recent_projects: recent_projects.clone(),
            navigation_back: navigation_back.clone(),
            navigation_forward: navigation_forward.clone(),
            current_navigation_location: Some(current_navigation_location.clone()),
            workspace_tasks: Vec::new(),
            workspace_tasks_runnable: true,
            plugin_commands: PluginCommandRegistry::default(),
            keybindings: keybindings.clone(),
            commands_catalog: vec![(
                "Quick Open".to_owned(),
                Command::ToggleQuickOpen,
                "Ctrl+P".to_owned(),
                COMMAND_PALETTE_DEFAULT_SECTION.to_owned(),
            )],
            command_recent: command_recent.clone(),
            command_query_memory: command_query_memory.clone(),
            summary_label: "1 command matched".to_owned(),
            empty_label: "No commands match \"open\"".to_owned(),
            results: vec![result(0, 1, 1)],
            result_snapshots: VecDeque::new(),
        };

        assert!(cache.matches(
            "open",
            true,
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
            true,
            &command_recent,
            &command_query_memory,
        ));
        assert!(cache.catalog_matches(
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
        ));

        let changed_current_navigation_location =
            NavigationLocation::new(workspace_root.join("src/lib.rs"), 8, 3);
        assert!(!cache.matches(
            "open",
            true,
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&changed_current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
            true,
            &command_recent,
            &command_query_memory,
        ));
        assert!(!cache.matches(
            "open folder",
            true,
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
            true,
            &command_recent,
            &command_query_memory,
        ));
        assert!(cache.catalog_matches(
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
        ));

        assert!(!cache.matches(
            "open",
            false,
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
            true,
            &command_recent,
            &command_query_memory,
        ));
        assert!(!cache.matches(
            "open",
            true,
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
            false,
            &command_recent,
            &command_query_memory,
        ));
        assert!(cache.catalog_matches(
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
        ));

        let changed_command_recent = VecDeque::from([Command::ToggleTerminal]);
        assert!(!cache.matches(
            "open",
            true,
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
            true,
            &changed_command_recent,
            &command_query_memory,
        ));
        assert!(cache.catalog_matches(
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &keybindings,
        ));

        let changed_keybindings = vec![KeyBinding {
            chord: "Ctrl+Shift+P".to_owned(),
            command: Command::ToggleQuickOpen,
        }];
        assert!(!cache.catalog_matches(
            &workspace_root,
            &recent_projects,
            &navigation_back,
            &navigation_forward,
            Some(&current_navigation_location),
            &[],
            &PluginCommandRegistry::default(),
            &changed_keybindings,
        ));
    }

    #[test]
    fn command_palette_results_cache_reuses_catalog_for_query_only_refresh() {
        let workspace_root = PathBuf::from("workspace");
        let mut cache = CommandPaletteResultsCache {
            query: String::with_capacity(16),
            git_enabled: true,
            workspace_root,
            recent_projects: Vec::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            workspace_tasks: Vec::new(),
            workspace_tasks_runnable: true,
            plugin_commands: PluginCommandRegistry::default(),
            keybindings: Vec::new(),
            commands_catalog: catalog(&[
                ("Quick Open", Command::ToggleQuickOpen),
                ("Toggle Terminal", Command::ToggleTerminal),
            ]),
            command_recent: VecDeque::new(),
            command_query_memory: VecDeque::new(),
            summary_label: command_palette_result_summary(2, ""),
            empty_label: command_palette_empty_state_label(""),
            results: Vec::new(),
            result_snapshots: VecDeque::new(),
        };
        let catalog_ptr = cache.commands_catalog.as_ptr();
        let query_ptr = cache.query.as_ptr();
        let recent = VecDeque::from([Command::ToggleTerminal]);
        let memory = VecDeque::new();

        refresh_cached_command_palette_results(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            true,
            "term",
            &recent,
            &memory,
            &mut cache,
        );

        assert_eq!(cache.commands_catalog.as_ptr(), catalog_ptr);
        assert_eq!(cache.query.as_ptr(), query_ptr);
        assert_eq!(cache.query, "term");
        assert_eq!(cache.summary_label, "1 command matched");
        assert_eq!(
            cache.command_recent.iter().cloned().collect::<Vec<_>>(),
            vec![Command::ToggleTerminal]
        );
        assert_eq!(cache.results.len(), 1);
        assert_eq!(cache.results[0].catalog_index, 1);
    }

    #[test]
    fn command_palette_results_cache_reuses_unchanged_refresh_storage() {
        let workspace_root = PathBuf::from("workspace");
        let recent = VecDeque::from([Command::ToggleQuickOpen]);
        let memory = VecDeque::from([CommandPaletteQueryMemoryEntry {
            query: "open".to_owned(),
            command: Command::ToggleQuickOpen,
            uses: 2,
        }]);
        let mut summary_label = String::with_capacity(64);
        command_palette_result_summary_into(&mut summary_label, 1, "open");
        let mut empty_label = String::with_capacity(64);
        command_palette_empty_state_label_into(&mut empty_label, "open");
        let mut cache = CommandPaletteResultsCache {
            query: String::with_capacity(16),
            git_enabled: true,
            workspace_root,
            recent_projects: Vec::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            workspace_tasks: Vec::new(),
            workspace_tasks_runnable: true,
            plugin_commands: PluginCommandRegistry::default(),
            keybindings: Vec::new(),
            commands_catalog: catalog(&[
                ("Quick Open", Command::ToggleQuickOpen),
                ("Toggle Terminal", Command::ToggleTerminal),
            ]),
            command_recent: recent.clone(),
            command_query_memory: memory.clone(),
            summary_label,
            empty_label,
            results: Vec::new(),
            result_snapshots: VecDeque::new(),
        };
        let memory_query_ptr = cache.command_query_memory[0].query.as_ptr();
        let summary_ptr = cache.summary_label.as_ptr();
        let empty_ptr = cache.empty_label.as_ptr();

        refresh_cached_command_palette_results(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            true,
            "term",
            &recent,
            &memory,
            &mut cache,
        );

        assert_eq!(
            cache.command_query_memory[0].query.as_ptr(),
            memory_query_ptr
        );
        assert_eq!(cache.summary_label.as_ptr(), summary_ptr);
        assert_eq!(cache.empty_label.as_ptr(), empty_ptr);
        assert_eq!(cache.summary_label, "1 command matched");
        assert_eq!(cache.empty_label, "No commands match \"term\"");
        assert_eq!(cache.results.len(), 1);
        assert_eq!(cache.results[0].catalog_index, 1);
    }

    #[test]
    fn command_palette_results_cache_restores_previous_query_snapshot() {
        let recent = VecDeque::new();
        let memory = VecDeque::new();
        let mut cache = CommandPaletteResultsCache {
            query: "open".to_owned(),
            git_enabled: true,
            workspace_root: PathBuf::from("workspace"),
            recent_projects: Vec::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            workspace_tasks: Vec::new(),
            workspace_tasks_runnable: true,
            plugin_commands: PluginCommandRegistry::default(),
            keybindings: Vec::new(),
            commands_catalog: catalog(&[
                ("Quick Open", Command::ToggleQuickOpen),
                ("Toggle Terminal", Command::ToggleTerminal),
            ]),
            command_recent: recent.clone(),
            command_query_memory: memory.clone(),
            summary_label: "cached open summary".to_owned(),
            empty_label: "cached open empty".to_owned(),
            results: vec![result(0, 42_424, 31_313)],
            result_snapshots: VecDeque::new(),
        };

        refresh_cached_command_palette_results(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            true,
            "term",
            &recent,
            &memory,
            &mut cache,
        );

        assert_eq!(cache.query, "term");
        assert_eq!(cache.results.len(), 1);
        assert_eq!(cache.results[0].catalog_index, 1);
        assert_eq!(cache.result_snapshots.len(), 1);
        assert_eq!(cache.result_snapshots[0].query, "open");

        refresh_cached_command_palette_results(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            true,
            "open",
            &recent,
            &memory,
            &mut cache,
        );

        assert_eq!(cache.query, "open");
        assert_eq!(cache.summary_label, "cached open summary");
        assert_eq!(cache.empty_label, "cached open empty");
        assert_eq!(cache.results, vec![result(0, 42_424, 31_313)]);
        assert_eq!(
            result_commands(cache.results.clone(), &cache.commands_catalog),
            vec![Command::ToggleQuickOpen]
        );
        assert_eq!(cache.result_snapshots.len(), 1);
        assert_eq!(cache.result_snapshots[0].query, "term");
    }

    #[test]
    fn command_palette_results_cache_keeps_result_snapshots_bounded() {
        let recent = VecDeque::new();
        let memory = VecDeque::new();
        let mut cache = CommandPaletteResultsCache {
            query: String::new(),
            git_enabled: true,
            workspace_root: PathBuf::from("workspace"),
            recent_projects: Vec::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            workspace_tasks: Vec::new(),
            workspace_tasks_runnable: true,
            plugin_commands: PluginCommandRegistry::default(),
            keybindings: Vec::new(),
            commands_catalog: catalog(&[
                ("Quick Open", Command::ToggleQuickOpen),
                ("Toggle Terminal", Command::ToggleTerminal),
            ]),
            command_recent: recent.clone(),
            command_query_memory: memory.clone(),
            summary_label: command_palette_result_summary(2, ""),
            empty_label: command_palette_empty_state_label(""),
            results: vec![result(0, 0, 0), result(1, 0, 0)],
            result_snapshots: VecDeque::new(),
        };

        for index in 0..(COMMAND_PALETTE_RESULT_SNAPSHOT_LIMIT + 3) {
            let query = format!("missing-{index}");
            refresh_cached_command_palette_results(
                &fuzzy_matcher::skim::SkimMatcherV2::default(),
                true,
                true,
                &query,
                &recent,
                &memory,
                &mut cache,
            );
        }

        assert_eq!(
            cache.query,
            format!("missing-{}", COMMAND_PALETTE_RESULT_SNAPSHOT_LIMIT + 2)
        );
        assert_eq!(
            cache.result_snapshots.len(),
            COMMAND_PALETTE_RESULT_SNAPSHOT_LIMIT
        );
        assert!(
            cache
                .result_snapshots
                .iter()
                .all(|snapshot| !snapshot.query.is_empty())
        );
    }

    #[test]
    fn command_palette_results_cache_skips_oversized_result_snapshots() {
        let recent = VecDeque::new();
        let memory = VecDeque::new();
        let mut cache = CommandPaletteResultsCache {
            query: String::new(),
            git_enabled: true,
            workspace_root: PathBuf::from("workspace"),
            recent_projects: Vec::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            workspace_tasks: Vec::new(),
            workspace_tasks_runnable: true,
            plugin_commands: PluginCommandRegistry::default(),
            keybindings: Vec::new(),
            commands_catalog: catalog(&[
                ("Quick Open", Command::ToggleQuickOpen),
                ("Toggle Terminal", Command::ToggleTerminal),
            ]),
            command_recent: recent.clone(),
            command_query_memory: memory.clone(),
            summary_label: command_palette_result_summary(2, ""),
            empty_label: command_palette_empty_state_label(""),
            results: (0..=COMMAND_PALETTE_RESULT_SNAPSHOT_RESULT_LIMIT)
                .map(|_| result(0, 0, 0))
                .collect(),
            result_snapshots: VecDeque::new(),
        };

        refresh_cached_command_palette_results(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            true,
            "term",
            &recent,
            &memory,
            &mut cache,
        );

        assert_eq!(cache.query, "term");
        assert_eq!(cache.results.len(), 1);
        assert_eq!(cache.results[0].catalog_index, 1);
        assert!(cache.result_snapshots.is_empty());
    }

    #[test]
    fn command_palette_results_cache_drops_stale_snapshot_catalog_indexes() {
        let recent = VecDeque::new();
        let memory = VecDeque::new();
        let mut cache = CommandPaletteResultsCache {
            query: "term".to_owned(),
            git_enabled: true,
            workspace_root: PathBuf::from("workspace"),
            recent_projects: Vec::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            workspace_tasks: Vec::new(),
            workspace_tasks_runnable: true,
            plugin_commands: PluginCommandRegistry::default(),
            keybindings: Vec::new(),
            commands_catalog: catalog(&[("Quick Open", Command::ToggleQuickOpen)]),
            command_recent: recent.clone(),
            command_query_memory: memory.clone(),
            summary_label: "cached term summary".to_owned(),
            empty_label: "cached term empty".to_owned(),
            results: vec![result(0, 12, 12)],
            result_snapshots: VecDeque::from([CommandPaletteResultsSnapshot {
                query: "open".to_owned(),
                git_enabled: true,
                workspace_tasks_runnable: true,
                command_recent: recent.clone(),
                command_query_memory: memory.clone(),
                summary_label: "stale open summary".to_owned(),
                empty_label: "stale open empty".to_owned(),
                results: vec![result(3, 99, 99)],
            }]),
        };

        refresh_cached_command_palette_results(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            true,
            "open",
            &recent,
            &memory,
            &mut cache,
        );

        assert_eq!(cache.query, "open");
        assert_eq!(cache.summary_label, "1 command matched");
        assert_eq!(cache.results.len(), 1);
        assert_eq!(cache.results[0].catalog_index, 0);
        assert_eq!(cache.result_snapshots.len(), 1);
        assert_eq!(cache.result_snapshots[0].query, "term");
    }

    #[test]
    fn command_palette_hides_git_only_commands_when_git_is_disabled() {
        assert!(!command_palette_command_visible(
            false,
            &Command::CommitStagedChanges
        ));
        assert!(!command_palette_command_visible(
            false,
            &Command::OpenFileHeadRevision(PathBuf::from("src/lib.rs"))
        ));
        assert!(!command_palette_command_visible(
            false,
            &Command::ToggleGitBranchSwitcher
        ));
        assert!(!command_palette_command_visible(
            false,
            &Command::ToggleGitHistory
        ));
        assert!(!command_palette_command_visible(
            false,
            &Command::ToggleGitStashes
        ));
        assert!(!command_palette_command_visible(
            false,
            &Command::OpenSourceControlInIntegratedTerminal
        ));
        assert!(!command_palette_command_visible(
            false,
            &Command::OpenActiveFileHunkDiff
        ));
        assert!(!command_palette_command_visible(
            false,
            &Command::OpenActiveFileStagedHunkDiff
        ));
        assert!(command_palette_command_visible(
            false,
            &Command::ToggleSourceControl
        ));
        assert!(command_palette_command_visible(
            false,
            &Command::ToggleQuickOpen
        ));
        assert!(command_palette_command_visible(
            true,
            &Command::CommitStagedChanges
        ));
    }

    #[test]
    fn command_palette_hides_workspace_task_snapshot_commands_when_tasks_are_not_runnable() {
        let catalog = catalog(&[
            ("Quick Open", Command::ToggleQuickOpen),
            (
                "Run Build Task Build",
                Command::RunWorkspaceTaskSnapshot {
                    index: 0,
                    fingerprint: 7,
                },
            ),
            (
                "Cancel Build Task Build",
                Command::CancelWorkspaceTaskSnapshot {
                    index: 0,
                    fingerprint: 7,
                },
            ),
            ("Legacy Task", Command::RunWorkspaceTask(0)),
            (
                "Run Build Task",
                Command::RunWorkspaceTaskKind(WorkspaceTaskKind::Build),
            ),
        ]);
        let mut results = Vec::new();
        let recent = VecDeque::new();
        let memory = VecDeque::new();

        filtered_command_palette_results_into(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            false,
            "",
            &recent,
            &memory,
            &catalog,
            &mut results,
        );

        let visible = result_commands(results.clone(), &catalog);
        assert!(visible.contains(&Command::ToggleQuickOpen));
        assert!(visible.contains(&Command::RunWorkspaceTaskKind(WorkspaceTaskKind::Build)));
        assert!(!visible.contains(&Command::RunWorkspaceTask(0)));
        assert!(!visible.contains(&Command::RunWorkspaceTaskSnapshot {
            index: 0,
            fingerprint: 7,
        }));
        assert!(!visible.contains(&Command::CancelWorkspaceTaskSnapshot {
            index: 0,
            fingerprint: 7,
        }));
        assert!(!command_palette_command_visible_with_workspace(
            true,
            false,
            &Command::RunWorkspaceTaskSnapshot {
                index: 0,
                fingerprint: 7,
            },
        ));

        filtered_command_palette_results_into(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            true,
            "",
            &recent,
            &memory,
            &catalog,
            &mut results,
        );

        let visible = result_commands(results, &catalog);
        assert!(visible.contains(&Command::RunWorkspaceTaskSnapshot {
            index: 0,
            fingerprint: 7,
        }));
        assert!(visible.contains(&Command::CancelWorkspaceTaskSnapshot {
            index: 0,
            fingerprint: 7,
        }));
    }

    #[test]
    fn command_palette_result_text_is_single_line_and_bounded() {
        assert_eq!(
            normalize_command_palette_result_text("  Run\nPlugin\tCommand\u{0}Now  ", 64),
            "Run Plugin Command Now"
        );
        assert_eq!(
            normalize_command_palette_result_text("Run\u{202e} Plugin\u{200f} Command", 64),
            "Run Plugin Command"
        );
        assert_eq!(normalize_command_palette_result_text("abcdef", 4), "abcd");
        assert_eq!(normalize_command_palette_result_text("\n\t\u{0}", 64), "");
    }

    #[test]
    fn command_palette_result_text_bounds_hostile_prefix_scans() {
        let mut label = "\u{202e}".repeat(COMMAND_PALETTE_RESULT_TEXT_SCAN_CHARS);
        label.push_str("Run Command");

        assert_eq!(normalize_command_palette_result_text(&label, 64), "");
    }

    #[test]
    fn command_palette_match_query_sanitizes_and_bounds_direct_inputs() {
        assert_eq!(
            command_palette_match_query(" Git\u{202e}\nHistory\u{0000} ").as_ref(),
            "Git History"
        );
        assert_eq!(
            command_palette_result_summary(2, "\u{202e}\u{0000}\n"),
            "Showing 2 commands"
        );

        let mut hostile = "\u{202e}".repeat(COMMAND_PALETTE_RESULT_TEXT_SCAN_CHARS);
        hostile.push_str("Git");
        assert_eq!(command_palette_match_query(&hostile).as_ref(), "");

        let long = "a".repeat(MAX_COMMAND_PALETTE_QUERY_PATTERN_CHARS + 64);
        assert_eq!(
            command_palette_match_query(&long).chars().count(),
            MAX_COMMAND_PALETTE_QUERY_PATTERN_CHARS
        );
    }

    #[test]
    fn command_palette_match_query_borrows_normalized_direct_inputs() {
        let query = "Git History";

        assert!(matches!(
            command_palette_match_query(query),
            std::borrow::Cow::Borrowed(borrowed) if borrowed == query
        ));
    }

    #[test]
    fn command_palette_filter_sanitizes_query_before_matching() {
        let catalog = catalog(&[
            ("Git History", Command::ToggleGitHistory),
            ("Quick Open", Command::ToggleQuickOpen),
        ]);
        let mut results = Vec::new();

        filtered_command_palette_results_into(
            &fuzzy_matcher::skim::SkimMatcherV2::default(),
            true,
            true,
            " Git\u{202e}\nHistory\u{0000} ",
            &VecDeque::new(),
            &VecDeque::new(),
            &catalog,
            &mut results,
        );

        assert_eq!(
            result_commands(results, &catalog),
            vec![Command::ToggleGitHistory]
        );
    }

    #[test]
    fn command_palette_catalog_caches_normalized_result_text() {
        let catalog = normalized_command_palette_catalog(vec![
            (
                "  Run\nPlugin\tCommand\u{0}Now  ".to_owned(),
                Command::ToggleQuickOpen,
                " Ctrl\nP\u{202e} ".to_owned(),
            ),
            (
                "\n\t\u{0}".to_owned(),
                Command::ToggleTerminal,
                String::new(),
            ),
        ]);

        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].0, "Run Plugin Command Now");
        assert_eq!(catalog[0].1, Command::ToggleQuickOpen);
        assert_eq!(catalog[0].2, "Ctrl P");
        assert_eq!(catalog[0].3, "Navigation");
    }

    #[test]
    fn command_palette_catalog_deduplicates_exact_commands_after_normalization() {
        let catalog = normalized_command_palette_catalog(vec![
            (
                " Quick\nOpen ".to_owned(),
                Command::ToggleQuickOpen,
                " Ctrl\nP ".to_owned(),
            ),
            (
                "Quick Open Duplicate".to_owned(),
                Command::ToggleQuickOpen,
                "Ctrl+Shift+P".to_owned(),
            ),
            (
                "Toggle Terminal".to_owned(),
                Command::ToggleTerminal,
                String::new(),
            ),
        ]);

        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].0, "Quick Open");
        assert_eq!(catalog[0].1, Command::ToggleQuickOpen);
        assert_eq!(catalog[0].2, "Ctrl P");
        assert_eq!(catalog[0].3, "Navigation");
        assert_eq!(catalog[1].1, Command::ToggleTerminal);
        assert_eq!(catalog[1].3, "Terminal");
    }

    #[test]
    fn command_palette_empty_state_sanitizes_query_label() {
        assert_eq!(
            command_palette_empty_state_label(" \u{202e}git\nhistory\u{0000} "),
            "No commands match \"git history\""
        );
        assert_eq!(
            command_palette_empty_state_label("\u{202e}\u{0000}\n"),
            "No commands available"
        );
    }
}

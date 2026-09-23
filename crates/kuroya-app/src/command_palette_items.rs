use crate::{
    command_aliases::command_label_aliases,
    command_catalog::command_catalog_slice,
    commands::command_label,
    history::NavigationLocation,
    keybindings::catalog_keybinding_chords,
    navigation_targets::navigation_location_label,
    path_display::{display_path_label_cow, sanitized_display_label_cow},
    workspace_tasks_runtime::{workspace_task_fingerprint, workspace_task_name_label},
    workspace_trust::trusted_workspace_paths_match,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use kuroya_core::{Command, PluginCommandRegistry, WorkspaceTask, keymap::KeyBinding};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{HashSet, VecDeque},
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

const COMMAND_PALETTE_RECENT_PROJECT_LIMIT: usize = 8;
const COMMAND_PALETTE_RECENT_PROJECT_SCAN_LIMIT: usize = 256;
const COMMAND_PALETTE_RECENT_NAVIGATION_LIMIT: usize = 8;
pub(crate) const MAX_COMMAND_PALETTE_RECENT_COMMANDS: usize = 64;
pub(crate) const MAX_COMMAND_PALETTE_QUERY_MEMORY: usize = 128;
pub(crate) const MAX_COMMAND_PALETTE_QUERY_PATTERN_CHARS: usize = 256;
const COMMAND_PALETTE_RECENT_COMMAND_BONUS: i64 = 25;
const COMMAND_PALETTE_QUERY_MEMORY_BONUS: i64 = 96;
const COMMAND_PALETTE_QUERY_MEMORY_PREFIX_BONUS: i64 = 48;
const COMMAND_PALETTE_QUERY_MEMORY_PREFIX_MIN_CHARS: usize = 3;
const COMMAND_PALETTE_QUERY_MEMORY_USE_BONUS: i64 = 8;
const COMMAND_PALETTE_QUERY_MEMORY_MIN_EXACT_TOTAL: i64 =
    COMMAND_PALETTE_QUERY_MEMORY_BONUS + COMMAND_PALETTE_QUERY_MEMORY_USE_BONUS;
const COMMAND_PALETTE_ALIAS_MATCH_PENALTY: i64 = 3;
const COMMAND_PALETTE_LABEL_PREFIX_BONUS: i64 = 25;
const COMMAND_PALETTE_LABEL_WORD_START_BONUS: i64 = 16;
const COMMAND_PALETTE_MEMORY_QUERY_MAX_CHARS: usize = 128;
const COMMAND_PALETTE_QUERY_SCAN_CHARS: usize = 4096;
const COMMAND_PALETTE_PLUGIN_LABEL_MAX_CHARS: usize = 120;

pub(crate) const COMMAND_PALETTE_DEFAULT_SECTION: &str = "Commands";
pub(crate) type CommandPaletteItem = (String, Command, String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandPaletteQueryMemoryEntry {
    pub query: String,
    pub command: Command,
    #[serde(default = "default_command_palette_query_memory_uses")]
    pub uses: u32,
}

fn default_command_palette_query_memory_uses() -> u32 {
    1
}

pub(crate) fn command_palette_items(
    workspace_root: &Path,
    recent_projects: &[PathBuf],
    navigation_locations: &[NavigationLocation],
    workspace_tasks: &[WorkspaceTask],
    plugin_commands: &PluginCommandRegistry,
    bindings: &[KeyBinding],
) -> Vec<CommandPaletteItem> {
    let command_catalog = command_catalog_slice();
    let catalog_chords = catalog_keybinding_chords(command_catalog, bindings);
    let mut items = Vec::with_capacity(
        command_catalog
            .len()
            .saturating_add(2)
            .saturating_add(
                recent_projects
                    .len()
                    .min(COMMAND_PALETTE_RECENT_PROJECT_LIMIT),
            )
            .saturating_add(
                navigation_locations
                    .len()
                    .min(COMMAND_PALETTE_RECENT_NAVIGATION_LIMIT),
            )
            .saturating_add(workspace_tasks.len())
            .saturating_add(plugin_commands.commands().len()),
    );
    for (index, command) in command_catalog.iter().enumerate() {
        items.push(command_palette_item(
            command,
            catalog_chords.chord_for_catalog_index(index),
        ));
        if matches!(command, Command::NewFile) {
            items.push((
                "New File in Workspace".to_owned(),
                Command::CreateFileIn(workspace_root.to_path_buf()),
                String::new(),
            ));
            items.push((
                "New Folder in Workspace".to_owned(),
                Command::CreateFolderIn(workspace_root.to_path_buf()),
                String::new(),
            ));
        }
    }

    push_recent_workspace_palette_items(&mut items, workspace_root, recent_projects);
    push_recent_navigation_palette_items(&mut items, navigation_locations);
    push_workspace_task_palette_items(&mut items, workspace_tasks);
    push_plugin_command_palette_items(&mut items, plugin_commands);
    items
}

fn push_recent_workspace_palette_items(
    items: &mut Vec<CommandPaletteItem>,
    workspace_root: &Path,
    recent_projects: &[PathBuf],
) {
    push_recent_workspace_palette_items_with_dir_probe(
        items,
        workspace_root,
        recent_projects,
        |path| path.is_dir(),
    );
}

fn push_recent_workspace_palette_items_with_dir_probe(
    items: &mut Vec<CommandPaletteItem>,
    workspace_root: &Path,
    recent_projects: &[PathBuf],
    mut project_is_dir: impl FnMut(&Path) -> bool,
) {
    let recent_project_scan_capacity = recent_projects
        .len()
        .min(COMMAND_PALETTE_RECENT_PROJECT_SCAN_LIMIT);
    let mut seen: Vec<&Path> = Vec::with_capacity(COMMAND_PALETTE_RECENT_PROJECT_LIMIT);
    let mut seen_keys: HashSet<CommandPalettePathKey> =
        HashSet::with_capacity(COMMAND_PALETTE_RECENT_PROJECT_LIMIT);
    let mut rejected: Vec<&Path> = Vec::new();
    let mut rejected_keys: HashSet<CommandPalettePathKey> =
        HashSet::with_capacity(recent_project_scan_capacity);
    for project in recent_projects
        .iter()
        .take(COMMAND_PALETTE_RECENT_PROJECT_SCAN_LIMIT)
    {
        if seen.len() >= COMMAND_PALETTE_RECENT_PROJECT_LIMIT {
            break;
        }

        if trusted_workspace_paths_match(project, workspace_root) {
            continue;
        }

        let project_key = command_palette_path_key(project);
        if recent_workspace_path_matches_tracked(
            &rejected,
            &rejected_keys,
            project,
            project_key.as_ref(),
        ) {
            continue;
        }
        if !project_is_dir(project) {
            rejected.push(project.as_path());
            if let Some(project_key) = project_key {
                rejected_keys.insert(project_key);
            }
            continue;
        }
        if recent_workspace_path_matches_tracked(&seen, &seen_keys, project, project_key.as_ref()) {
            continue;
        }
        seen.push(project.as_path());
        if let Some(project_key) = project_key {
            seen_keys.insert(project_key);
        }

        items.push((
            format!("Open Recent {}", display_path_label_cow(project)),
            Command::OpenWorkspace(project.clone()),
            String::new(),
        ));
    }
}

fn recent_workspace_path_matches_tracked(
    paths: &[&Path],
    keys: &HashSet<CommandPalettePathKey>,
    project: &Path,
    project_key: Option<&CommandPalettePathKey>,
) -> bool {
    if let Some(project_key) = project_key {
        return keys.contains(project_key);
    }
    recent_workspace_path_matches_any(paths, project)
}

fn recent_workspace_path_matches_any(paths: &[&Path], project: &Path) -> bool {
    paths
        .iter()
        .any(|seen_project| trusted_workspace_paths_match(seen_project, project))
}

#[cfg(test)]
fn recent_workspace_palette_items(
    workspace_root: &Path,
    recent_projects: &[PathBuf],
) -> Vec<CommandPaletteItem> {
    let mut items = Vec::with_capacity(COMMAND_PALETTE_RECENT_PROJECT_LIMIT);
    push_recent_workspace_palette_items(&mut items, workspace_root, recent_projects);
    items
}

#[cfg(test)]
fn recent_workspace_palette_items_with_dir_probe(
    workspace_root: &Path,
    recent_projects: &[PathBuf],
    project_is_dir: impl FnMut(&Path) -> bool,
) -> Vec<CommandPaletteItem> {
    let mut items = Vec::with_capacity(COMMAND_PALETTE_RECENT_PROJECT_LIMIT);
    push_recent_workspace_palette_items_with_dir_probe(
        &mut items,
        workspace_root,
        recent_projects,
        project_is_dir,
    );
    items
}

#[cfg(test)]
fn recent_navigation_palette_items(
    navigation_locations: &[NavigationLocation],
) -> Vec<CommandPaletteItem> {
    let mut items = Vec::with_capacity(COMMAND_PALETTE_RECENT_NAVIGATION_LIMIT);
    push_recent_navigation_palette_items(&mut items, navigation_locations);
    items
}

fn push_recent_navigation_palette_items(
    items: &mut Vec<CommandPaletteItem>,
    navigation_locations: &[NavigationLocation],
) {
    let mut seen: Vec<(&Path, usize, usize)> =
        Vec::with_capacity(COMMAND_PALETTE_RECENT_NAVIGATION_LIMIT);
    let mut seen_keys: HashSet<(CommandPalettePathKey, usize, usize)> =
        HashSet::with_capacity(COMMAND_PALETTE_RECENT_NAVIGATION_LIMIT);
    for location in navigation_locations.iter().rev() {
        if seen.len() >= COMMAND_PALETTE_RECENT_NAVIGATION_LIMIT {
            break;
        }

        let location_key = command_palette_path_key(&location.path);
        if recent_navigation_location_matches_tracked(
            &seen,
            &seen_keys,
            location.path.as_path(),
            location_key.as_ref(),
            location.line,
            location.column,
        ) {
            continue;
        }
        seen.push((location.path.as_path(), location.line, location.column));
        if let Some(location_key) = location_key {
            seen_keys.insert((location_key, location.line, location.column));
        }

        items.push((
            format!(
                "Go to Recent Location {}",
                navigation_location_label(location)
            ),
            Command::OpenFileAt {
                path: location.path.clone(),
                line: location.line,
                column: location.column,
            },
            String::new(),
        ));
    }
}

fn recent_navigation_location_matches_tracked(
    seen: &[(&Path, usize, usize)],
    seen_keys: &HashSet<(CommandPalettePathKey, usize, usize)>,
    path: &Path,
    path_key: Option<&CommandPalettePathKey>,
    line: usize,
    column: usize,
) -> bool {
    if let Some(path_key) = path_key {
        return seen_keys.contains(&(path_key.clone(), line, column));
    }

    seen.iter().any(|seen_location| {
        recent_navigation_location_key_matches(*seen_location, path, line, column)
    })
}

fn recent_navigation_location_key_matches(
    seen: (&Path, usize, usize),
    path: &Path,
    line: usize,
    column: usize,
) -> bool {
    let (seen_path, seen_line, seen_column) = seen;
    seen_line == line
        && seen_column == column
        && (seen_path == path || trusted_workspace_paths_match(seen_path, path))
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CommandPalettePathKey {
    prefix: Option<String>,
    rooted: bool,
    components: Vec<String>,
}

fn command_palette_path_key(path: &Path) -> Option<CommandPalettePathKey> {
    if path.as_os_str().is_empty() {
        return None;
    }

    let mut key = CommandPalettePathKey {
        prefix: None,
        rooted: false,
        components: Vec::new(),
    };
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.prefix = Some(normalize_command_palette_path_key_component(
                    prefix.as_os_str(),
                ));
            }
            Component::RootDir => key.rooted = true,
            Component::CurDir => {}
            Component::ParentDir => {
                if key
                    .components
                    .last()
                    .is_some_and(|component| component != "..")
                {
                    key.components.pop();
                } else {
                    key.components.push("..".to_owned());
                }
            }
            Component::Normal(component) => {
                key.components
                    .push(normalize_command_palette_path_key_component(component));
            }
        }
    }

    Some(key)
}

fn normalize_command_palette_path_key_component(component: &OsStr) -> String {
    let component = component.to_string_lossy();
    #[cfg(windows)]
    {
        if component.is_ascii() {
            let mut component = component.into_owned();
            component.make_ascii_lowercase();
            component
        } else {
            component.to_lowercase()
        }
    }
    #[cfg(not(windows))]
    {
        component.into_owned()
    }
}

fn command_palette_item(command: &Command, chord: Option<&str>) -> CommandPaletteItem {
    let label = command_label(command);
    let chord = chord.unwrap_or_default().to_owned();
    (label, command.clone(), chord)
}

pub(crate) fn command_palette_item_section(label: &str, command: &Command) -> &'static str {
    match command {
        Command::NewFile
        | Command::SaveActive
        | Command::SaveAs
        | Command::SaveAll
        | Command::ReloadActiveFromDisk
        | Command::OpenActiveFileLatestLocalHistory
        | Command::OpenLocalHistoryBrowser
        | Command::ToggleReadOnly
        | Command::CloseActive
        | Command::ReopenClosedFile
        | Command::SelectActiveFileForCompare
        | Command::CompareActiveFileWithSelected
        | Command::CompareActiveFileWithSaved
        | Command::CopyActiveFilePath
        | Command::CopyActiveFileRelativePath => "File",
        Command::OpenWorkspacePrompt
        | Command::TrustWorkspace
        | Command::RevokeWorkspaceTrust
        | Command::RefreshWorkspace
        | Command::SaveWorkspaceSnapshot
        | Command::RestoreLatestWorkspaceSnapshot
        | Command::RevealActiveFileInExplorer
        | Command::CreateFileIn(_)
        | Command::CreateFolderIn(_) => "Workspace",
        Command::OpenWorkspace(_) if label.starts_with("Open Recent ") => "Recent Workspaces",
        Command::IndentLines
        | Command::OutdentLines
        | Command::DeleteLines
        | Command::JoinLines
        | Command::DuplicateLines
        | Command::MoveLineUp
        | Command::MoveLineDown
        | Command::Undo
        | Command::Redo => "Edit",
        Command::SelectLines
        | Command::SelectRectangularBlock
        | Command::ExpandSelection
        | Command::SelectNextOccurrence
        | Command::SelectAllOccurrences
        | Command::AddCursorAbove
        | Command::AddCursorBelow
        | Command::AddCursorsToLineEnds => "Selection",
        Command::ToggleMinimap
        | Command::ToggleStickyScroll
        | Command::ToggleCommandPalette
        | Command::ToggleDiagnosticsPanel
        | Command::CycleDiagnosticsPanelPlacement
        | Command::SplitEditorRight
        | Command::CloseEditorPane
        | Command::ResetEditorPaneWeights => "View",
        Command::NextTab
        | Command::PreviousTab
        | Command::NavigateBack
        | Command::NavigateForward
        | Command::ToggleQuickOpen
        | Command::ToggleGoToLine
        | Command::GoToMatchingBracket
        | Command::NextDiagnostic
        | Command::PreviousDiagnostic
        | Command::GoToDefinition
        | Command::FindReferences
        | Command::ShowCallHierarchy
        | Command::ShowTypeHierarchy
        | Command::ToggleSymbolsPanel
        | Command::CycleSymbolsPanelPlacement
        | Command::ToggleWorkspaceSymbols => "Navigation",
        Command::OpenFileAt { .. } if label.starts_with("Go to Recent Location ") => {
            "Recent Locations"
        }
        Command::ToggleBufferFind
        | Command::FindNext
        | Command::FindPrevious
        | Command::ToggleProjectSearch
        | Command::NextProjectSearchResult
        | Command::PreviousProjectSearchResult => "Search",
        Command::ToggleLineComment
        | Command::RequestDocumentHighlights
        | Command::RequestHover
        | Command::RenameSymbol
        | Command::RequestCompletions
        | Command::RequestSignatureHelp
        | Command::RequestFoldingRanges
        | Command::ToggleFold
        | Command::ExpandAllFolds
        | Command::FormatDocument
        | Command::RequestCodeActions => "Code",
        Command::NextGitChange
        | Command::PreviousGitChange
        | Command::NextDiffHunk
        | Command::PreviousDiffHunk
        | Command::RefreshActiveDiff
        | Command::SwapActiveDiffSides
        | Command::ToggleSourceControl
        | Command::CycleSourceControlPlacement
        | Command::RevealActiveFileInSourceControl
        | Command::OpenActiveFileChanges
        | Command::OpenActiveFileStagedChanges
        | Command::OpenActiveFileHeadChanges
        | Command::OpenActiveFileHeadRevision
        | Command::OpenActiveFileIndexRevision
        | Command::OpenAllChanges
        | Command::OpenAllUnstagedChanges
        | Command::OpenAllStagedChanges
        | Command::CopyAllChangesPatch
        | Command::CopyUnstagedChangesPatch
        | Command::CopyStagedChangesPatch
        | Command::CopyActiveFilePatch
        | Command::CopyActiveFileStagedPatch
        | Command::OpenActiveFileHunks
        | Command::OpenActiveFileStagedHunks
        | Command::OpenActiveFileBlame
        | Command::StageActiveFileChanges
        | Command::StageAllChanges
        | Command::StageActiveFileHunk
        | Command::StageActiveDiffHunk
        | Command::OpenActiveDiffBaseFile
        | Command::OpenActiveDiffHunkBase
        | Command::OpenActiveDiffSourceFile
        | Command::OpenActiveDiffHunkSource
        | Command::OpenActiveFileHunkDiff
        | Command::OpenActiveFileStagedHunkDiff
        | Command::OpenActiveAccessibleDiffViewer
        | Command::CopyActiveFileHunkPatch
        | Command::CopyActiveFileStagedHunkPatch
        | Command::CopyActiveDiffPatch
        | Command::CopyActiveDiffHunkPatch
        | Command::UnstageActiveFileChanges
        | Command::UnstageAllChanges
        | Command::UnstageActiveFileHunk
        | Command::UnstageActiveDiffHunk
        | Command::DiscardActiveFileChanges
        | Command::DiscardAllChanges
        | Command::DiscardActiveFileHunk
        | Command::DiscardActiveDiffHunk
        | Command::CommitStagedChanges
        | Command::AcceptCurrentConflict
        | Command::AcceptIncomingConflict
        | Command::AcceptBothConflicts
        | Command::ToggleGitBranchSwitcher
        | Command::ToggleGitHistory
        | Command::ToggleGitStashes
        | Command::OpenSourceControlInIntegratedTerminal
        | Command::SaveGitStash => "Source Control",
        Command::ToggleTerminal
        | Command::ToggleTerminalSearch
        | Command::NextTerminalSearchResult
        | Command::PreviousTerminalSearchResult
        | Command::NextTerminalSession
        | Command::PreviousTerminalSession => "Terminal",
        Command::ToggleWorkspaceTasks
        | Command::RunWorkspaceTaskKind(_)
        | Command::RunWorkspaceTaskSnapshot { .. }
        | Command::CancelWorkspaceTaskSnapshot { .. } => "Tasks and Extensions",
        Command::RunPluginCommand { .. } => "Extensions",
        Command::ToggleVimMode
        | Command::ReloadSettings
        | Command::ToggleSettingsPanel
        | Command::OpenSettingsFile
        | Command::ToggleKeybindingsPanel
        | Command::ToggleThemePicker
        | Command::CycleTheme => "Settings",
        Command::ToggleDevtools => "Developer",
        Command::CheckForUpdates => "Help",
        _ => COMMAND_PALETTE_DEFAULT_SECTION,
    }
}

fn push_workspace_task_palette_items(items: &mut Vec<CommandPaletteItem>, tasks: &[WorkspaceTask]) {
    items.extend(tasks.iter().enumerate().map(|(index, task)| {
        (
            workspace_task_palette_label(task),
            Command::RunWorkspaceTaskSnapshot {
                index,
                fingerprint: workspace_task_fingerprint(task),
            },
            String::new(),
        )
    }));
}

fn workspace_task_palette_label(task: &WorkspaceTask) -> String {
    let name = workspace_task_name_label(&task.name);
    match task.kind {
        kuroya_core::WorkspaceTaskKind::Build => format!("Run Build Task {name}"),
        kuroya_core::WorkspaceTaskKind::Test => format!("Run Test Task {name}"),
        kuroya_core::WorkspaceTaskKind::Run => format!("Run Configuration {name}"),
        kuroya_core::WorkspaceTaskKind::Custom => format!("Run Workspace Task {name}"),
    }
}

fn push_plugin_command_palette_items(
    items: &mut Vec<CommandPaletteItem>,
    plugin_commands: &PluginCommandRegistry,
) {
    let plugin_start = items.len();
    items.extend(plugin_commands.commands().iter().map(|command| {
        (
            command_palette_plugin_label(&command.label),
            Command::RunPluginCommand {
                plugin_id: command.plugin_id.clone(),
                command_id: command.command_id.clone(),
            },
            String::new(),
        )
    }));
    items[plugin_start..].sort_by(compare_plugin_command_palette_items);
}

fn command_palette_plugin_label(label: &str) -> String {
    sanitized_display_label_cow(
        label,
        COMMAND_PALETTE_PLUGIN_LABEL_MAX_CHARS,
        "Unnamed plugin command",
    )
    .into_owned()
}

fn compare_plugin_command_palette_items(
    left: &CommandPaletteItem,
    right: &CommandPaletteItem,
) -> Ordering {
    let (left_label, left_command, _) = left;
    let (right_label, right_command, _) = right;
    let (left_plugin_id, left_command_id) = plugin_command_palette_ids(left_command);
    let (right_plugin_id, right_command_id) = plugin_command_palette_ids(right_command);

    cmp_ascii_case_insensitive(left_label, right_label)
        .then_with(|| left_label.cmp(right_label))
        .then_with(|| left_plugin_id.cmp(right_plugin_id))
        .then_with(|| left_command_id.cmp(right_command_id))
}

fn plugin_command_palette_ids(command: &Command) -> (&str, &str) {
    match command {
        Command::RunPluginCommand {
            plugin_id,
            command_id,
        } => (plugin_id.as_str(), command_id.as_str()),
        _ => ("", ""),
    }
}

fn cmp_ascii_case_insensitive(left: &str, right: &str) -> Ordering {
    for (left, right) in left.bytes().zip(right.bytes()) {
        let ordering = left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase());
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

#[cfg(test)]
pub(crate) fn command_palette_match_score(
    matcher: &SkimMatcherV2,
    label: &str,
    chord: &str,
    query: &str,
) -> Option<i64> {
    command_palette_match_score_with_aliases(
        matcher,
        label,
        chord,
        command_label_aliases(label),
        query,
    )
}

#[cfg(test)]
pub(crate) fn command_palette_command_match_score(
    matcher: &SkimMatcherV2,
    label: &str,
    chord: &str,
    command: &Command,
    query: &str,
) -> Option<i64> {
    if let Some(aliases) = command_palette_generated_aliases_for_command(label, command) {
        return command_palette_match_score_with_aliases(matcher, label, chord, aliases, query);
    }

    command_palette_match_score(matcher, label, chord, query)
}

pub(crate) fn command_palette_command_match_score_non_empty(
    matcher: &SkimMatcherV2,
    label: &str,
    chord: &str,
    command: &Command,
    query: &str,
) -> Option<i64> {
    if let Some(aliases) = command_palette_generated_aliases_for_command(label, command) {
        return command_palette_match_score_with_aliases_non_empty(
            matcher, label, chord, aliases, query,
        );
    }

    command_palette_match_score_with_aliases_non_empty(
        matcher,
        label,
        chord,
        command_label_aliases(label),
        query,
    )
}

#[cfg(test)]
fn command_palette_match_score_with_aliases(
    matcher: &SkimMatcherV2,
    label: &str,
    chord: &str,
    aliases: &[&str],
    query: &str,
) -> Option<i64> {
    if !query
        .chars()
        .take(COMMAND_PALETTE_QUERY_SCAN_CHARS)
        .any(|ch| !ch.is_whitespace())
    {
        return Some(0);
    }

    command_palette_match_score_with_aliases_non_empty(matcher, label, chord, aliases, query)
}

fn command_palette_match_score_with_aliases_non_empty(
    matcher: &SkimMatcherV2,
    label: &str,
    chord: &str,
    aliases: &[&str],
    query: &str,
) -> Option<i64> {
    let label_score = crate::fuzzy::fuzzy_match_with_case_fallback(matcher, label, query)
        .map(|score| score + command_palette_label_bonus(label, query));
    let chord_score = (!chord.is_empty())
        .then(|| {
            crate::fuzzy::fuzzy_match_with_case_fallback(matcher, chord, query)
                .map(|score| score - 5)
        })
        .flatten();
    let chord_words_score = command_palette_shortcut_words_match_score(matcher, chord, query);
    let alias_score = aliases
        .iter()
        .filter_map(|alias| {
            crate::fuzzy::fuzzy_match_with_case_fallback(matcher, alias, query)
                .map(|score| score - COMMAND_PALETTE_ALIAS_MATCH_PENALTY)
        })
        .max();
    label_score
        .max(chord_score)
        .max(chord_words_score)
        .max(alias_score)
}

/// Mirrors quick open's filename bonuses: starting a label with the query or
/// matching at a word boundary is a stronger signal than skim's raw score
/// alone ("save" should rank "Save All Files" above "Autosave Interval").
fn command_palette_label_bonus(label: &str, query: &str) -> i64 {
    if label.len() >= query.len() {
        let prefix = &label[..query.len()];
        if prefix.eq_ignore_ascii_case(query) {
            return COMMAND_PALETTE_LABEL_PREFIX_BONUS;
        }
    }
    let query_lowercase = query.to_ascii_lowercase();
    if crate::quick_open::ranking::quick_open_lowercase_word_start_match(
        label,
        query_lowercase.as_str(),
    ) {
        COMMAND_PALETTE_LABEL_WORD_START_BONUS
    } else {
        0
    }
}

fn command_palette_shortcut_words_match_score(
    matcher: &SkimMatcherV2,
    chord: &str,
    query: &str,
) -> Option<i64> {
    if chord.is_empty()
        || !chord.contains('+')
        || !query
            .chars()
            .any(is_command_palette_shortcut_query_separator)
    {
        return None;
    }

    let mut words = String::with_capacity(chord.len());
    for character in chord.chars() {
        if character == '+' {
            words.push(' ');
        } else {
            words.push(character);
        }
    }

    let normalized_query =
        normalize_command_palette_shortcut_query(query).unwrap_or(Cow::Borrowed(query));
    crate::fuzzy::fuzzy_match_with_case_fallback(matcher, &words, normalized_query.as_ref())
        .map(|score| score - 5)
}

fn is_command_palette_shortcut_query_separator(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '+' | '-' | '/' | '\\')
}

fn normalize_command_palette_shortcut_query(query: &str) -> Option<Cow<'_, str>> {
    if !query
        .chars()
        .any(|ch| !ch.is_whitespace() && is_command_palette_shortcut_query_separator(ch))
    {
        return None;
    }

    let mut normalized = String::with_capacity(query.len());
    let mut pending_space = false;
    for ch in query.chars() {
        if is_command_palette_shortcut_query_separator(ch) {
            pending_space = !normalized.is_empty();
            continue;
        }

        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        normalized.push(ch);
    }

    if normalized.is_empty() {
        None
    } else {
        Some(Cow::Owned(normalized))
    }
}

fn command_palette_generated_aliases_for_command(
    label: &str,
    command: &Command,
) -> Option<&'static [&'static str]> {
    match command {
        Command::CreateFileIn(_) if label == "New File in Workspace" => {
            Some(command_label_aliases("New File"))
        }
        Command::CreateFolderIn(_) if label == "New Folder in Workspace" => {
            Some(command_label_aliases("New Folder"))
        }
        Command::OpenWorkspace(_)
            if generated_label_suffix(label, "Open Recent ").is_some()
                || generated_label_suffix(label, "Open Folder ").is_some() =>
        {
            Some(command_label_aliases("Open Folder"))
        }
        Command::OpenFileAt { .. }
            if generated_label_suffix(label, "Go to Recent Location ").is_some() =>
        {
            Some(command_label_aliases("Go to Recent Location"))
        }
        Command::RunWorkspaceTaskSnapshot { .. }
            if generated_label_suffix(label, "Run Build Task ").is_some() =>
        {
            Some(command_label_aliases("Run Build Task"))
        }
        Command::RunWorkspaceTaskSnapshot { .. }
            if generated_label_suffix(label, "Run Test Task ").is_some() =>
        {
            Some(command_label_aliases("Run Test Task"))
        }
        Command::RunWorkspaceTaskSnapshot { .. }
            if generated_label_suffix(label, "Run Configuration ").is_some() =>
        {
            Some(command_label_aliases("Run Configuration"))
        }
        Command::RunWorkspaceTaskSnapshot { .. }
            if generated_label_suffix(label, "Run Workspace Task ").is_some() =>
        {
            Some(command_label_aliases("Run Workspace Task"))
        }
        _ => None,
    }
}

fn generated_label_suffix<'a>(label: &'a str, prefix: &str) -> Option<&'a str> {
    label
        .strip_prefix(prefix)
        .filter(|suffix| !suffix.is_empty())
}

pub(crate) fn record_recent_palette_command(
    recent: &mut VecDeque<Command>,
    command: Command,
    max_entries: usize,
) {
    if max_entries == 0 {
        recent.clear();
        return;
    }
    if let Some(index) = recent.iter().position(|entry| entry == &command) {
        recent.remove(index);
    }

    recent.push_front(command);
    while recent.len() > max_entries {
        recent.pop_back();
    }
}

pub(crate) fn normalize_recent_palette_commands(
    entries: impl IntoIterator<Item = Command>,
    max_entries: usize,
) -> VecDeque<Command> {
    if max_entries == 0 {
        return VecDeque::new();
    }

    let mut recent = VecDeque::new();
    for command in entries {
        if recent.iter().any(|entry| entry == &command) {
            continue;
        }

        recent.push_back(command);
        if recent.len() >= max_entries {
            break;
        }
    }
    recent
}

pub(crate) fn record_command_palette_query_memory(
    memory: &mut VecDeque<CommandPaletteQueryMemoryEntry>,
    query: &str,
    command: &Command,
    max_entries: usize,
) {
    if max_entries == 0 {
        memory.clear();
        return;
    }
    let Some(query) = normalize_command_palette_memory_query(query) else {
        return;
    };
    let entries = std::mem::take(memory);
    *memory = normalize_command_palette_query_memory(entries, max_entries);
    let mut uses = 1;
    if let Some(index) = memory
        .iter()
        .position(|entry| entry.query == query && entry.command == *command)
    {
        if let Some(entry) = memory.remove(index) {
            uses = entry.uses.saturating_add(1).max(1);
        }
    }

    memory.push_front(CommandPaletteQueryMemoryEntry {
        query,
        command: command.clone(),
        uses,
    });
    while memory.len() > max_entries {
        memory.pop_back();
    }
}

pub(crate) fn normalize_command_palette_query_memory(
    entries: impl IntoIterator<Item = CommandPaletteQueryMemoryEntry>,
    max_entries: usize,
) -> VecDeque<CommandPaletteQueryMemoryEntry> {
    if max_entries == 0 {
        return VecDeque::new();
    }

    let mut memory: VecDeque<CommandPaletteQueryMemoryEntry> = VecDeque::new();
    for entry in entries {
        let Some(query) = normalize_command_palette_memory_query(&entry.query) else {
            continue;
        };
        let uses = entry.uses.max(1);
        if let Some(existing) = memory
            .iter_mut()
            .find(|existing| existing.query == query && existing.command == entry.command)
        {
            existing.uses = existing.uses.max(uses);
            continue;
        }

        if memory.len() >= max_entries {
            continue;
        }
        memory.push_back(CommandPaletteQueryMemoryEntry {
            query,
            command: entry.command,
            uses,
        });
    }
    memory
}

#[cfg(test)]
pub(crate) fn command_palette_rank_score(
    match_score: i64,
    recent: &VecDeque<Command>,
    query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
    query: &str,
    command: &Command,
) -> i64 {
    CommandPaletteRanker::new(recent, query_memory, query).rank_score(match_score, command)
}

#[derive(Debug, Clone)]
pub(crate) struct CommandPaletteRanker {
    bonuses: Vec<CommandPaletteRankBonus>,
}

#[derive(Debug, Clone)]
struct CommandPaletteRankBonus {
    command: Command,
    recent_bonus: i64,
    query_memory_bonus: i64,
}

impl CommandPaletteRanker {
    pub(crate) fn new(
        recent: &VecDeque<Command>,
        query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
        query: &str,
    ) -> Self {
        let query = normalize_command_palette_memory_query(query);
        let recent_count = recent.len().min(MAX_COMMAND_PALETTE_RECENT_COMMANDS);
        let query_memory_count = query.as_ref().map_or(0, |_| {
            query_memory.len().min(MAX_COMMAND_PALETTE_QUERY_MEMORY)
        });
        let mut bonuses = Vec::with_capacity(recent_count.saturating_add(query_memory_count));

        for (index, command) in recent
            .iter()
            .take(MAX_COMMAND_PALETTE_RECENT_COMMANDS)
            .enumerate()
        {
            let recent_bonus = (COMMAND_PALETTE_RECENT_COMMAND_BONUS - index as i64).max(0);
            record_command_palette_rank_bonus(&mut bonuses, command, recent_bonus, 0);
        }

        if let Some(query) = query {
            for (index, entry) in query_memory
                .iter()
                .take(MAX_COMMAND_PALETTE_QUERY_MEMORY)
                .enumerate()
            {
                let query_affinity = command_palette_memory_query_affinity(&entry.query, &query);
                let query_memory_bonus =
                    command_palette_query_memory_score(query_affinity, entry.uses, index);
                record_command_palette_rank_bonus(
                    &mut bonuses,
                    &entry.command,
                    0,
                    query_memory_bonus,
                );
            }
        }

        Self { bonuses }
    }

    pub(crate) fn new_with_bonuses(
        recent: &VecDeque<Command>,
        query_memory: &VecDeque<CommandPaletteQueryMemoryEntry>,
        query: &str,
    ) -> Option<Self> {
        let ranker = Self::new(recent, query_memory, query);
        (!ranker.bonuses.is_empty()).then_some(ranker)
    }

    pub(crate) fn rank_score(&self, match_score: i64, command: &Command) -> i64 {
        match_score + self.command_bonus(command)
    }

    fn command_bonus(&self, command: &Command) -> i64 {
        self.bonuses
            .iter()
            .find(|bonus| bonus.command == *command)
            .map(|bonus| bonus.recent_bonus + bonus.query_memory_bonus)
            .unwrap_or_default()
    }
}

fn record_command_palette_rank_bonus(
    bonuses: &mut Vec<CommandPaletteRankBonus>,
    command: &Command,
    recent_bonus: i64,
    query_memory_bonus: i64,
) {
    if recent_bonus == 0 && query_memory_bonus == 0 {
        return;
    }

    if let Some(bonus) = bonuses.iter_mut().find(|bonus| bonus.command == *command) {
        bonus.recent_bonus = bonus.recent_bonus.max(recent_bonus);
        bonus.query_memory_bonus = bonus.query_memory_bonus.max(query_memory_bonus);
        return;
    }

    bonuses.push(CommandPaletteRankBonus {
        command: command.clone(),
        recent_bonus,
        query_memory_bonus,
    });
}

fn command_palette_query_memory_score(query_affinity: i64, uses: u32, index: usize) -> i64 {
    if query_affinity == 0 {
        return 0;
    }

    let uses = i64::from(uses.min(8));
    let recency = (MAX_COMMAND_PALETTE_QUERY_MEMORY.saturating_sub(index) as i64).min(24);
    let score = query_affinity + (uses * COMMAND_PALETTE_QUERY_MEMORY_USE_BONUS) + recency;
    if query_affinity == COMMAND_PALETTE_QUERY_MEMORY_PREFIX_BONUS {
        score.min(COMMAND_PALETTE_QUERY_MEMORY_MIN_EXACT_TOTAL - 1)
    } else {
        score
    }
}

fn command_palette_memory_query_affinity(memory_query: &str, current_query: &str) -> i64 {
    if memory_query == current_query {
        COMMAND_PALETTE_QUERY_MEMORY_BONUS
    } else if command_palette_memory_queries_share_prefix(memory_query, current_query) {
        COMMAND_PALETTE_QUERY_MEMORY_PREFIX_BONUS
    } else {
        0
    }
}

fn command_palette_memory_queries_share_prefix(memory_query: &str, current_query: &str) -> bool {
    (memory_query.starts_with(current_query) || current_query.starts_with(memory_query))
        && memory_query
            .chars()
            .zip(current_query.chars())
            .take_while(|(left, right)| left == right)
            .count()
            >= COMMAND_PALETTE_QUERY_MEMORY_PREFIX_MIN_CHARS
}

fn normalize_command_palette_memory_query(query: &str) -> Option<String> {
    let mut normalized = String::new();
    let mut chars = 0usize;
    let mut pending_space = false;

    for (scanned_chars, ch) in query.chars().enumerate() {
        if scanned_chars >= COMMAND_PALETTE_QUERY_SCAN_CHARS {
            break;
        }

        if chars >= COMMAND_PALETTE_MEMORY_QUERY_MAX_CHARS {
            break;
        }

        if ch.is_whitespace() || ch.is_control() {
            pending_space = !normalized.is_empty();
            continue;
        }
        if is_command_palette_query_format_control(ch) {
            continue;
        }

        if pending_space {
            normalized.push(' ');
            chars += 1;
            pending_space = false;
            if chars >= COMMAND_PALETTE_MEMORY_QUERY_MAX_CHARS {
                break;
            }
        }

        for lower in ch.to_lowercase() {
            if chars >= COMMAND_PALETTE_MEMORY_QUERY_MAX_CHARS {
                break;
            }
            normalized.push(lower);
            chars += 1;
        }
    }

    while normalized.ends_with(' ') {
        normalized.pop();
    }

    if normalized.is_empty() {
        return None;
    }
    Some(normalized)
}

#[cfg(test)]
pub(crate) fn sanitize_command_palette_query_input(input: &str) -> String {
    sanitize_command_palette_query(input, MAX_COMMAND_PALETTE_QUERY_PATTERN_CHARS)
}

pub(crate) fn sanitize_command_palette_query_input_in_place(input: &mut String) -> bool {
    if command_palette_query_is_sanitized(input, MAX_COMMAND_PALETTE_QUERY_PATTERN_CHARS) {
        return false;
    }

    let sanitized = sanitize_command_palette_query(input, MAX_COMMAND_PALETTE_QUERY_PATTERN_CHARS);
    input.clear();
    input.push_str(&sanitized);
    true
}

fn command_palette_query_is_sanitized(input: &str, max_chars: usize) -> bool {
    if max_chars == 0 {
        return input.is_empty();
    }

    let mut previous_space = false;
    for (chars, ch) in input.chars().enumerate() {
        if chars >= max_chars || ch.is_control() || is_command_palette_query_format_control(ch) {
            return false;
        }

        if ch.is_whitespace() {
            if ch != ' ' || chars == 0 || previous_space {
                return false;
            }
            previous_space = true;
        } else {
            previous_space = false;
        }
    }

    !previous_space
}

fn sanitize_command_palette_query(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let mut output = String::with_capacity(input.len().min(max_chars));
    let mut chars = 0usize;
    let mut pending_space = false;

    for (scanned_chars, ch) in input.chars().enumerate() {
        if scanned_chars >= COMMAND_PALETTE_QUERY_SCAN_CHARS {
            break;
        }

        if is_command_palette_query_format_control(ch) {
            continue;
        }

        if ch.is_whitespace() {
            pending_space = chars > 0;
            continue;
        }

        if ch.is_control() {
            continue;
        }

        if pending_space {
            if chars + 1 >= max_chars {
                break;
            }
            output.push(' ');
            chars += 1;
            pending_space = false;
        }

        if chars >= max_chars {
            break;
        }
        output.push(ch);
        chars += 1;
    }

    output
}

fn is_command_palette_query_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{00ad}'
            | '\u{061c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
    )
}

#[cfg(test)]
mod tests {

    #[test]
    fn label_prefix_bonus_prefers_command_names_starting_with_the_query() {
        let matcher = SkimMatcherV2::default();
        let save_all = command_palette_match_score_with_aliases_non_empty(
            &matcher,
            "Save All Files",
            "",
            &[],
            "save",
        )
        .expect("save should match Save All Files");
        let autosave = command_palette_match_score_with_aliases_non_empty(
            &matcher,
            "Autosave Interval",
            "",
            &[],
            "save",
        )
        .expect("save should match Autosave Interval");

        assert!(
            save_all > autosave,
            "prefix match {save_all} must outrank inner match {autosave}"
        );
    }

    #[test]
    fn uppercase_queries_still_match_via_case_fallback() {
        let matcher = SkimMatcherV2::default();
        // "SCM" is uppercase, so skim's smart case makes the plain pass
        // case-sensitive; the alias "scm" only matches through the fallback.
        let score = command_palette_match_score_with_aliases_non_empty(
            &matcher,
            "Source Control",
            "",
            &["scm"],
            "SCM",
        );
        assert!(score.is_some(), "SCM must match through the case fallback");
    }

    use super::{
        COMMAND_PALETTE_DEFAULT_SECTION, COMMAND_PALETTE_QUERY_SCAN_CHARS,
        COMMAND_PALETTE_RECENT_PROJECT_SCAN_LIMIT, CommandPaletteQueryMemoryEntry,
        CommandPaletteRanker, command_palette_command_match_score, command_palette_item_section,
        command_palette_match_score_with_aliases_non_empty, recent_navigation_palette_items,
        recent_workspace_palette_items, recent_workspace_palette_items_with_dir_probe,
        record_command_palette_query_memory, sanitize_command_palette_query_input,
    };
    use crate::{
        command_catalog::command_catalog_slice, commands::command_label,
        history::NavigationLocation,
    };
    use fuzzy_matcher::skim::SkimMatcherV2;
    use kuroya_core::Command;
    use std::{
        cell::RefCell,
        collections::VecDeque,
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn generated_custom_workspace_task_matches_task_runner_alias() {
        let matcher = SkimMatcherV2::default();

        assert!(
            command_palette_command_match_score(
                &matcher,
                "Run Workspace Task Deploy",
                "",
                &Command::RunWorkspaceTaskSnapshot {
                    index: 0,
                    fingerprint: 42,
                },
                "task runner",
            )
            .is_some()
        );
    }

    #[test]
    fn command_palette_item_section_classifies_static_and_generated_commands() {
        assert_eq!(
            command_palette_item_section("Quick Open", &Command::ToggleQuickOpen),
            "Navigation"
        );
        assert_eq!(
            command_palette_item_section(
                "Open Recent workspace",
                &Command::OpenWorkspace(PathBuf::from("workspace")),
            ),
            "Recent Workspaces"
        );
        assert_eq!(
            command_palette_item_section(
                "Run Build Task build",
                &Command::RunWorkspaceTaskSnapshot {
                    index: 0,
                    fingerprint: 7,
                },
            ),
            "Tasks and Extensions"
        );
    }

    #[test]
    fn static_command_catalog_entries_have_specific_palette_sections() {
        for command in command_catalog_slice() {
            let label = command_label(command);
            assert_ne!(
                command_palette_item_section(&label, command),
                COMMAND_PALETTE_DEFAULT_SECTION,
                "{command:?} should have an explicit command palette section"
            );
        }
    }

    #[test]
    fn recent_navigation_items_keep_newest_distinct_locations() {
        let older_duplicate = NavigationLocation::new(PathBuf::from("src/main.rs"), 10, 4);
        let middle = NavigationLocation::new(PathBuf::from("src/lib.rs"), 3, 1);
        let newer_duplicate = NavigationLocation::new(PathBuf::from("src/main.rs"), 10, 4);

        let items = recent_navigation_palette_items(&[older_duplicate, middle, newer_duplicate]);

        assert_eq!(items.len(), 2);
        assert!(matches!(
            &items[0].1,
            Command::OpenFileAt { path, line: 10, column: 4 }
                if path == &PathBuf::from("src/main.rs")
        ));
        assert!(matches!(
            &items[1].1,
            Command::OpenFileAt { path, line: 3, column: 1 }
                if path == &PathBuf::from("src/lib.rs")
        ));
    }

    #[test]
    fn recent_navigation_items_dedupe_lexically_equivalent_paths() {
        let older_duplicate =
            NavigationLocation::new(PathBuf::from("workspace/src/main.rs"), 10, 4);
        let distinct_line = NavigationLocation::new(PathBuf::from("workspace/src/main.rs"), 11, 4);
        let newer_equivalent =
            NavigationLocation::new(PathBuf::from("workspace/src/../src/main.rs"), 10, 4);

        let items =
            recent_navigation_palette_items(&[older_duplicate, distinct_line, newer_equivalent]);

        assert_eq!(items.len(), 2);
        assert!(matches!(
            &items[0].1,
            Command::OpenFileAt { path, line: 10, column: 4 }
                if path == &PathBuf::from("workspace/src/../src/main.rs")
        ));
        assert!(matches!(
            &items[1].1,
            Command::OpenFileAt { path, line: 11, column: 4 }
                if path == &PathBuf::from("workspace/src/main.rs")
        ));
    }

    #[test]
    fn recent_workspace_items_bound_stale_project_scans() {
        let workspace_root = unique_command_palette_test_dir("workspace");
        let valid_project = unique_command_palette_test_dir("valid-project");
        fs::create_dir_all(&workspace_root).expect("create workspace dir");
        fs::create_dir_all(&valid_project).expect("create valid recent project dir");

        let stale_projects = (0..COMMAND_PALETTE_RECENT_PROJECT_SCAN_LIMIT)
            .map(|index| workspace_root.join(format!("missing-{index}")));
        let mut recent_projects = stale_projects.collect::<Vec<_>>();
        recent_projects.push(valid_project.clone());

        let items = recent_workspace_palette_items(&workspace_root, &recent_projects);

        assert!(items.is_empty());

        recent_projects.clear();
        recent_projects.push(valid_project.clone());
        recent_projects.extend(
            (0..COMMAND_PALETTE_RECENT_PROJECT_SCAN_LIMIT)
                .map(|index| workspace_root.join(format!("later-missing-{index}"))),
        );

        let items = recent_workspace_palette_items(&workspace_root, &recent_projects);

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0].1,
            Command::OpenWorkspace(path) if path == &valid_project
        ));

        let _ = fs::remove_dir_all(&workspace_root);
        let _ = fs::remove_dir_all(&valid_project);
    }

    #[test]
    fn recent_workspace_items_cache_stale_equivalent_dir_probes() {
        let workspace_root = PathBuf::from("workspace");
        let stale_project = PathBuf::from("missing-project");
        let valid_project = PathBuf::from("valid-project");
        let recent_projects = vec![
            workspace_root.join("src").join(".."),
            stale_project.clone(),
            stale_project.join("."),
            stale_project.join("child").join(".."),
            valid_project.clone(),
        ];
        let probed = RefCell::new(Vec::new());

        let items = recent_workspace_palette_items_with_dir_probe(
            &workspace_root,
            &recent_projects,
            |project| {
                probed.borrow_mut().push(project.to_path_buf());
                project == valid_project.as_path()
            },
        );

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0].1,
            Command::OpenWorkspace(path) if path == &valid_project
        ));
        assert_eq!(
            probed.into_inner(),
            vec![stale_project, valid_project],
            "equivalent current and stale recent projects should not repeat directory probes"
        );
    }

    #[test]
    fn command_palette_ranker_is_skipped_without_active_bonuses() {
        let recent = VecDeque::new();
        let unrelated_memory = VecDeque::from([CommandPaletteQueryMemoryEntry {
            query: "terminal".to_owned(),
            command: Command::ToggleTerminal,
            uses: 1,
        }]);

        assert!(
            CommandPaletteRanker::new_with_bonuses(&recent, &VecDeque::new(), "open").is_none()
        );
        assert!(
            CommandPaletteRanker::new_with_bonuses(&recent, &unrelated_memory, "workspace")
                .is_none()
        );

        let recent = VecDeque::from([Command::ToggleTerminal]);
        let ranker = CommandPaletteRanker::new_with_bonuses(&recent, &VecDeque::new(), "")
            .expect("recent commands should produce a ranker");

        assert_eq!(ranker.rank_score(10, &Command::ToggleTerminal), 35);
        assert_eq!(ranker.rank_score(10, &Command::ToggleQuickOpen), 10);
    }

    #[test]
    fn command_palette_ranker_collapses_duplicate_command_bonuses() {
        let duplicate_recent = VecDeque::from([
            Command::ToggleTerminal,
            Command::ToggleQuickOpen,
            Command::ToggleTerminal,
        ]);
        let compact_recent = VecDeque::from([Command::ToggleTerminal, Command::ToggleQuickOpen]);
        let duplicate_memory = VecDeque::from([
            CommandPaletteQueryMemoryEntry {
                query: "terminal".to_owned(),
                command: Command::ToggleTerminal,
                uses: 1,
            },
            CommandPaletteQueryMemoryEntry {
                query: "terminal".to_owned(),
                command: Command::ToggleTerminal,
                uses: 8,
            },
        ]);
        let compact_memory = VecDeque::from([CommandPaletteQueryMemoryEntry {
            query: "terminal".to_owned(),
            command: Command::ToggleTerminal,
            uses: 8,
        }]);

        let duplicate_ranker =
            CommandPaletteRanker::new(&duplicate_recent, &duplicate_memory, "terminal");
        let compact_ranker =
            CommandPaletteRanker::new(&compact_recent, &compact_memory, "terminal");

        assert_eq!(
            duplicate_ranker.rank_score(100, &Command::ToggleTerminal),
            compact_ranker.rank_score(100, &Command::ToggleTerminal)
        );
        assert_eq!(
            duplicate_ranker.rank_score(100, &Command::ToggleQuickOpen),
            compact_ranker.rank_score(100, &Command::ToggleQuickOpen)
        );
    }

    #[test]
    fn command_palette_query_normalization_bounds_hostile_prefix_scans() {
        let mut query = "\u{202e}".repeat(COMMAND_PALETTE_QUERY_SCAN_CHARS);
        query.push_str("git");

        assert_eq!(sanitize_command_palette_query_input(&query), "");

        let mut memory = VecDeque::new();
        record_command_palette_query_memory(&mut memory, &query, &Command::ToggleGitHistory, 8);

        assert!(memory.is_empty());
    }

    fn unique_command_palette_test_dir(label: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-command-palette-{label}-{}-{now}",
            std::process::id()
        ))
    }
}

use crate::{
    file_runtime::file_path_open_buffer_or_known_openable,
    ui_icons::{IconKind, draw_icon},
    ui_state::plain_key_pressed,
};
use eframe::egui::{self, InputState, Key, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use kuroya_core::{Command, GitChangeStage, GitFileStatus, GitStatusEntry, TextBuffer};
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    ffi::OsStr,
    hash::{Hash, Hasher},
    path::{Component, Path, PathBuf},
};

use super::{
    copy_patch_command_for_entry, open_changes_command_for_entry, open_changes_label_for_stage,
    source_control_path_component_key,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceControlRowOpenability {
    pub(super) source_exists: bool,
    pub(super) can_compare_with_selected: bool,
}

pub(super) fn source_control_cached_row_openability(
    cache: &mut Option<SourceControlRowOpenability>,
    load: impl FnOnce() -> SourceControlRowOpenability,
) -> SourceControlRowOpenability {
    *cache.get_or_insert_with(load)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceControlRowActionKind {
    OpenChanges,
    CompareWithHead,
    OpenHeadFile,
    OpenIndexFile,
    SelectForCompare,
    CompareWithSelected,
    CopyPatch,
    RevealInExplorer,
    OpenFile,
    OpenFileToResolve,
    OpenBlame,
    OpenHunks,
    Stage,
    Unstage,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SourceControlRowActionTarget {
    kind: SourceControlRowActionKind,
    entry_fingerprint: u64,
}

impl SourceControlRowActionTarget {
    pub(super) fn new(kind: SourceControlRowActionKind, entry: &GitStatusEntry) -> Self {
        Self {
            kind,
            entry_fingerprint: source_control_entry_action_fingerprint(entry),
        }
    }
}

fn source_control_entry_action_fingerprint(entry: &GitStatusEntry) -> u64 {
    let mut hasher = DefaultHasher::new();
    entry.path.hash(&mut hasher);
    source_control_stage_fingerprint_tag(entry.stage).hash(&mut hasher);
    source_control_status_fingerprint_tag(entry.status).hash(&mut hasher);
    hasher.finish()
}

fn source_control_stage_fingerprint_tag(stage: GitChangeStage) -> u8 {
    match stage {
        GitChangeStage::Unstaged => 1,
        GitChangeStage::Staged => 2,
    }
}

fn source_control_status_fingerprint_tag(status: GitFileStatus) -> u8 {
    match status {
        GitFileStatus::Modified => 1,
        GitFileStatus::Added => 2,
        GitFileStatus::Deleted => 3,
        GitFileStatus::Renamed => 4,
        GitFileStatus::Untracked => 5,
        GitFileStatus::Conflicted => 6,
    }
}

const SOURCE_CONTROL_ROW_ACTION_ORDER: [SourceControlRowActionKind; 15] = [
    SourceControlRowActionKind::OpenChanges,
    SourceControlRowActionKind::CompareWithHead,
    SourceControlRowActionKind::OpenHeadFile,
    SourceControlRowActionKind::OpenIndexFile,
    SourceControlRowActionKind::SelectForCompare,
    SourceControlRowActionKind::CompareWithSelected,
    SourceControlRowActionKind::CopyPatch,
    SourceControlRowActionKind::RevealInExplorer,
    SourceControlRowActionKind::OpenFile,
    SourceControlRowActionKind::OpenFileToResolve,
    SourceControlRowActionKind::OpenBlame,
    SourceControlRowActionKind::OpenHunks,
    SourceControlRowActionKind::Stage,
    SourceControlRowActionKind::Unstage,
    SourceControlRowActionKind::Discard,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceControlKeyboardActionKind {
    OpenChanges,
    CompareWithHead,
    OpenHeadFile,
    OpenIndexFile,
    CopyPatch,
    CopyPath,
    CopyRelativePath,
    ShowPath,
    RevealInExplorer,
    OpenFile,
    OpenFileToResolve,
    OpenHunks,
    Stage,
    Unstage,
    Discard,
    OpenBlame,
}

fn source_control_row_action_kinds_for_state(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: bool,
    can_compare_with_selected: bool,
    show_inline_open_file_action: bool,
) -> impl Iterator<Item = SourceControlRowActionKind> {
    SOURCE_CONTROL_ROW_ACTION_ORDER
        .iter()
        .copied()
        .filter(move |kind| {
            source_control_row_action_allowed(
                stage,
                status,
                source_exists,
                can_compare_with_selected,
                show_inline_open_file_action,
                *kind,
            )
        })
}

pub(super) fn source_control_row_action_count(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: bool,
    can_compare_with_selected: bool,
    show_inline_open_file_action: bool,
) -> usize {
    source_control_row_action_kinds_for_state(
        stage,
        status,
        source_exists,
        can_compare_with_selected,
        show_inline_open_file_action,
    )
    .count()
}

fn source_control_row_action_allowed(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: bool,
    can_compare_with_selected: bool,
    show_inline_open_file_action: bool,
    kind: SourceControlRowActionKind,
) -> bool {
    match kind {
        SourceControlRowActionKind::OpenChanges
        | SourceControlRowActionKind::CompareWithHead
        | SourceControlRowActionKind::CopyPatch
        | SourceControlRowActionKind::RevealInExplorer
        | SourceControlRowActionKind::Discard => true,
        SourceControlRowActionKind::OpenHeadFile => source_control_has_head_revision(status),
        SourceControlRowActionKind::OpenIndexFile => {
            source_control_has_index_revision(stage, status)
        }
        SourceControlRowActionKind::SelectForCompare => source_exists,
        SourceControlRowActionKind::CompareWithSelected => {
            source_exists && can_compare_with_selected
        }
        SourceControlRowActionKind::OpenFile => {
            source_exists && status != GitFileStatus::Conflicted && show_inline_open_file_action
        }
        SourceControlRowActionKind::OpenFileToResolve => {
            source_exists && status == GitFileStatus::Conflicted
        }
        SourceControlRowActionKind::OpenBlame => source_exists,
        SourceControlRowActionKind::OpenHunks => {
            source_control_hunks_available(stage, status, source_exists)
        }
        SourceControlRowActionKind::Stage => stage == GitChangeStage::Unstaged,
        SourceControlRowActionKind::Unstage => stage == GitChangeStage::Staged,
    }
}

#[cfg(test)]
fn source_control_row_action_kinds(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: bool,
    can_compare_with_selected: bool,
    show_inline_open_file_action: bool,
) -> Vec<SourceControlRowActionKind> {
    source_control_row_action_kinds_for_state(
        stage,
        status,
        source_exists,
        can_compare_with_selected,
        show_inline_open_file_action,
    )
    .collect()
}

pub(super) fn source_control_path_exists_cached(
    cache: &mut HashMap<PathBuf, bool>,
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    if let Some(exists) = cache.get(path) {
        return *exists;
    }
    let cache_key = source_control_path_exists_cache_key(path);
    let cache_key_matches_path = cache_key.as_path() == path;
    if source_control_path_known_openable(buffers, indexed_files, path) {
        cache.insert(path.to_path_buf(), true);
        if !cache_key_matches_path {
            cache.insert(cache_key, true);
        }
        return true;
    }

    if !cache_key_matches_path {
        if let Some(exists) = cache.get(&cache_key) {
            let exists = *exists;
            cache.insert(path.to_path_buf(), exists);
            return exists;
        }
    }

    let exists = path_exists(path);
    cache.insert(path.to_path_buf(), exists);
    if !cache_key_matches_path {
        cache.insert(cache_key, exists);
    }
    exists
}

fn source_control_path_known_openable(
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
) -> bool {
    let mut fallback_needed = false;
    source_control_path_exists(buffers, indexed_files, path, |_| {
        fallback_needed = true;
        false
    }) && !fallback_needed
}

fn source_control_path_exists_cache_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    let mut has_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                source_control_push_path_exists_cache_key_component(&mut key, prefix.as_os_str());
                has_root = false;
            }
            Component::RootDir => {
                key.push(component.as_os_str());
                has_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(key.components().next_back(), Some(Component::Normal(_)));
                if can_pop {
                    key.pop();
                } else if !has_root {
                    key.push("..");
                }
            }
            Component::Normal(component) => {
                source_control_push_path_exists_cache_key_component(&mut key, component);
            }
        }
    }

    if key.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        key
    }
}

#[cfg(windows)]
fn source_control_push_path_exists_cache_key_component(key: &mut PathBuf, component: &OsStr) {
    let component = source_control_path_component_key(component);
    key.push(Path::new(&component));
}

#[cfg(not(windows))]
fn source_control_push_path_exists_cache_key_component(key: &mut PathBuf, component: &OsStr) {
    key.push(Path::new(component));
}

fn source_control_path_exists(
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    file_path_open_buffer_or_known_openable(buffers, indexed_files, path, path_exists)
}

pub(super) fn source_control_row_openability(
    cache: &mut HashMap<PathBuf, bool>,
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    compare_path: Option<&Path>,
    mut path_exists: impl FnMut(&Path) -> bool,
) -> SourceControlRowOpenability {
    let source_exists =
        source_control_path_exists_cached(cache, buffers, indexed_files, path, |path| {
            path_exists(path)
        });
    let can_compare_with_selected = source_exists
        && compare_path.is_some_and(|compare_path| {
            compare_path != path
                && source_control_path_exists_cached(
                    cache,
                    buffers,
                    indexed_files,
                    compare_path,
                    |path| path_exists(path),
                )
        });

    SourceControlRowOpenability {
        source_exists,
        can_compare_with_selected,
    }
}

pub(super) fn source_control_keyboard_action(
    input: &InputState,
    entry: &GitStatusEntry,
    mut source_exists: impl FnMut() -> bool,
) -> Option<SourceControlKeyboardActionKind> {
    if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::OpenChanges) {
        return Some(SourceControlKeyboardActionKind::OpenChanges);
    }
    if source_control_keyboard_action_pressed(
        input,
        SourceControlKeyboardActionKind::CompareWithHead,
    ) {
        return Some(SourceControlKeyboardActionKind::CompareWithHead);
    }
    if source_control_has_head_revision(entry.status)
        && source_control_keyboard_action_pressed(
            input,
            SourceControlKeyboardActionKind::OpenHeadFile,
        )
    {
        return Some(SourceControlKeyboardActionKind::OpenHeadFile);
    }
    if source_control_has_index_revision(entry.stage, entry.status)
        && source_control_keyboard_action_pressed(
            input,
            SourceControlKeyboardActionKind::OpenIndexFile,
        )
    {
        return Some(SourceControlKeyboardActionKind::OpenIndexFile);
    }
    if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::CopyPatch) {
        return Some(SourceControlKeyboardActionKind::CopyPatch);
    }
    if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::CopyPath) {
        return Some(SourceControlKeyboardActionKind::CopyPath);
    }
    if source_control_keyboard_action_pressed(
        input,
        SourceControlKeyboardActionKind::CopyRelativePath,
    ) {
        return Some(SourceControlKeyboardActionKind::CopyRelativePath);
    }
    if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::ShowPath) {
        return Some(SourceControlKeyboardActionKind::ShowPath);
    }
    if source_control_keyboard_action_pressed(
        input,
        SourceControlKeyboardActionKind::RevealInExplorer,
    ) {
        return Some(SourceControlKeyboardActionKind::RevealInExplorer);
    }
    if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::OpenFile)
        && source_exists()
    {
        return Some(if entry.status == GitFileStatus::Conflicted {
            SourceControlKeyboardActionKind::OpenFileToResolve
        } else {
            SourceControlKeyboardActionKind::OpenFile
        });
    }
    if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::OpenHunks)
        && source_control_keyboard_hunks_available(entry.stage, entry.status, &mut source_exists)
    {
        return Some(SourceControlKeyboardActionKind::OpenHunks);
    }
    match entry.stage {
        GitChangeStage::Staged => {
            if source_control_keyboard_action_pressed(
                input,
                SourceControlKeyboardActionKind::Unstage,
            ) {
                return Some(SourceControlKeyboardActionKind::Unstage);
            }
        }
        GitChangeStage::Unstaged => {
            if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::Stage)
            {
                return Some(SourceControlKeyboardActionKind::Stage);
            }
        }
    }
    if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::Discard) {
        return Some(SourceControlKeyboardActionKind::Discard);
    }
    if source_control_keyboard_action_pressed(input, SourceControlKeyboardActionKind::OpenBlame)
        && source_exists()
    {
        return Some(SourceControlKeyboardActionKind::OpenBlame);
    }
    None
}

fn source_control_keyboard_hunks_available(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: &mut impl FnMut() -> bool,
) -> bool {
    if status == GitFileStatus::Conflicted {
        return false;
    }
    match stage {
        GitChangeStage::Staged => true,
        GitChangeStage::Unstaged => status == GitFileStatus::Deleted || source_exists(),
    }
}

fn source_control_keyboard_action_pressed(
    input: &InputState,
    kind: SourceControlKeyboardActionKind,
) -> bool {
    match kind {
        SourceControlKeyboardActionKind::OpenChanges => plain_key_pressed(input, Key::Enter),
        SourceControlKeyboardActionKind::CompareWithHead => plain_key_pressed(input, Key::C),
        SourceControlKeyboardActionKind::OpenHeadFile => alt_key_pressed(input, Key::H),
        SourceControlKeyboardActionKind::OpenIndexFile => alt_key_pressed(input, Key::I),
        SourceControlKeyboardActionKind::CopyPatch => plain_key_pressed(input, Key::P),
        SourceControlKeyboardActionKind::CopyPath => alt_key_pressed(input, Key::C),
        SourceControlKeyboardActionKind::CopyRelativePath => alt_shift_key_pressed(input, Key::C),
        SourceControlKeyboardActionKind::ShowPath => alt_key_pressed(input, Key::S),
        SourceControlKeyboardActionKind::RevealInExplorer => plain_key_pressed(input, Key::R),
        SourceControlKeyboardActionKind::OpenFile
        | SourceControlKeyboardActionKind::OpenFileToResolve => plain_key_pressed(input, Key::O),
        SourceControlKeyboardActionKind::OpenHunks => plain_key_pressed(input, Key::H),
        SourceControlKeyboardActionKind::Stage => plain_key_pressed(input, Key::S),
        SourceControlKeyboardActionKind::Unstage => plain_key_pressed(input, Key::U),
        SourceControlKeyboardActionKind::Discard => plain_key_pressed(input, Key::Delete),
        SourceControlKeyboardActionKind::OpenBlame => plain_key_pressed(input, Key::B),
    }
}

fn alt_key_pressed(input: &InputState, key: Key) -> bool {
    input.key_pressed(key)
        && input.modifiers.alt
        && !input.modifiers.ctrl
        && !input.modifiers.shift
        && !input.modifiers.mac_cmd
        && !input.modifiers.command
}

fn alt_shift_key_pressed(input: &InputState, key: Key) -> bool {
    input.key_pressed(key)
        && input.modifiers.alt
        && input.modifiers.shift
        && !input.modifiers.ctrl
        && !input.modifiers.mac_cmd
        && !input.modifiers.command
}

#[cfg(test)]
fn source_control_keyboard_action_kinds(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: bool,
) -> Vec<SourceControlKeyboardActionKind> {
    let mut actions = vec![
        SourceControlKeyboardActionKind::OpenChanges,
        SourceControlKeyboardActionKind::CompareWithHead,
    ];
    if source_control_has_head_revision(status) {
        actions.push(SourceControlKeyboardActionKind::OpenHeadFile);
    }
    if source_control_has_index_revision(stage, status) {
        actions.push(SourceControlKeyboardActionKind::OpenIndexFile);
    }
    actions.extend([
        SourceControlKeyboardActionKind::CopyPatch,
        SourceControlKeyboardActionKind::CopyPath,
        SourceControlKeyboardActionKind::CopyRelativePath,
        SourceControlKeyboardActionKind::ShowPath,
        SourceControlKeyboardActionKind::RevealInExplorer,
    ]);
    if source_exists {
        if status == GitFileStatus::Conflicted {
            actions.push(SourceControlKeyboardActionKind::OpenFileToResolve);
        } else {
            actions.push(SourceControlKeyboardActionKind::OpenFile);
        }
    }
    if source_control_hunks_available(stage, status, source_exists) {
        actions.push(SourceControlKeyboardActionKind::OpenHunks);
    }
    match stage {
        GitChangeStage::Staged => actions.push(SourceControlKeyboardActionKind::Unstage),
        GitChangeStage::Unstaged => actions.push(SourceControlKeyboardActionKind::Stage),
    }
    actions.push(SourceControlKeyboardActionKind::Discard);
    if source_exists {
        actions.push(SourceControlKeyboardActionKind::OpenBlame);
    }
    actions
}

#[cfg(test)]
pub(crate) fn source_control_keyboard_action_labels(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: bool,
) -> Vec<&'static str> {
    source_control_keyboard_action_kinds(stage, status, source_exists)
        .into_iter()
        .map(|kind| source_control_keyboard_action_label(stage, kind))
        .collect()
}

#[cfg(test)]
fn source_control_keyboard_action_label(
    stage: GitChangeStage,
    kind: SourceControlKeyboardActionKind,
) -> &'static str {
    match kind {
        SourceControlKeyboardActionKind::OpenChanges => match stage {
            GitChangeStage::Staged => "Enter Open Staged Changes",
            GitChangeStage::Unstaged => "Enter Open Changes",
        },
        SourceControlKeyboardActionKind::CompareWithHead => "C Compare with HEAD",
        SourceControlKeyboardActionKind::OpenHeadFile => "Alt+H Open File at HEAD",
        SourceControlKeyboardActionKind::OpenIndexFile => "Alt+I Open File at Index",
        SourceControlKeyboardActionKind::CopyPatch => "P Copy Patch",
        SourceControlKeyboardActionKind::CopyPath => "Alt+C Copy Path",
        SourceControlKeyboardActionKind::CopyRelativePath => "Alt+Shift+C Copy Relative Path",
        SourceControlKeyboardActionKind::ShowPath => "Alt+S Show Path",
        SourceControlKeyboardActionKind::RevealInExplorer => "R Reveal in Explorer",
        SourceControlKeyboardActionKind::OpenFile => "O Open File",
        SourceControlKeyboardActionKind::OpenFileToResolve => "O Open File to Resolve",
        SourceControlKeyboardActionKind::OpenHunks => match stage {
            GitChangeStage::Staged => "H Open Staged Hunks",
            GitChangeStage::Unstaged => "H Open Hunks",
        },
        SourceControlKeyboardActionKind::Stage => "S Stage Changes",
        SourceControlKeyboardActionKind::Unstage => "U Unstage Changes",
        SourceControlKeyboardActionKind::Discard => "Delete Discard Changes",
        SourceControlKeyboardActionKind::OpenBlame => "B Open Blame",
    }
}

pub(super) fn source_control_keyboard_action_command(
    entry: &GitStatusEntry,
    kind: SourceControlKeyboardActionKind,
) -> Option<Command> {
    let command = match kind {
        SourceControlKeyboardActionKind::OpenChanges => open_changes_command_for_entry(entry),
        SourceControlKeyboardActionKind::CompareWithHead => {
            Command::OpenFileHeadChanges(entry.path.clone())
        }
        SourceControlKeyboardActionKind::OpenHeadFile => {
            Command::OpenFileHeadRevision(entry.path.clone())
        }
        SourceControlKeyboardActionKind::OpenIndexFile => {
            Command::OpenFileIndexRevision(entry.path.clone())
        }
        SourceControlKeyboardActionKind::CopyPatch => copy_patch_command_for_entry(entry),
        SourceControlKeyboardActionKind::CopyPath => Command::CopyFilePath(entry.path.clone()),
        SourceControlKeyboardActionKind::CopyRelativePath => {
            Command::CopyFileRelativePath(entry.path.clone())
        }
        SourceControlKeyboardActionKind::ShowPath => return None,
        SourceControlKeyboardActionKind::RevealInExplorer => {
            Command::RevealFileInExplorer(entry.path.clone())
        }
        SourceControlKeyboardActionKind::OpenFile
        | SourceControlKeyboardActionKind::OpenFileToResolve => {
            Command::OpenFile(entry.path.clone())
        }
        SourceControlKeyboardActionKind::OpenHunks => match entry.stage {
            GitChangeStage::Staged => Command::OpenStagedFileHunks(entry.path.clone()),
            GitChangeStage::Unstaged => Command::OpenFileHunks(entry.path.clone()),
        },
        SourceControlKeyboardActionKind::Stage => Command::StageFileChange(entry.path.clone()),
        SourceControlKeyboardActionKind::Unstage => Command::UnstageFileChange(entry.path.clone()),
        SourceControlKeyboardActionKind::Discard => Command::DiscardFileChanges(entry.path.clone()),
        SourceControlKeyboardActionKind::OpenBlame => Command::OpenFileBlame(entry.path.clone()),
    };
    Some(command)
}

#[cfg(test)]
pub(crate) fn source_control_row_action_labels(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: bool,
    can_compare_with_selected: bool,
    show_inline_open_file_action: bool,
) -> Vec<&'static str> {
    source_control_row_action_kinds(
        stage,
        status,
        source_exists,
        can_compare_with_selected,
        show_inline_open_file_action,
    )
    .into_iter()
    .map(|kind| source_control_row_action_label(stage, kind))
    .collect()
}

#[cfg(test)]
pub(crate) fn source_control_row_action_label_commands(
    entry: &GitStatusEntry,
    source_exists: bool,
    can_compare_with_selected: bool,
    show_inline_open_file_action: bool,
) -> Vec<(&'static str, Command)> {
    source_control_row_action_kinds(
        entry.stage,
        entry.status,
        source_exists,
        can_compare_with_selected,
        show_inline_open_file_action,
    )
    .into_iter()
    .map(|kind| {
        (
            source_control_row_action_label(entry.stage, kind),
            source_control_row_action_command(entry, kind),
        )
    })
    .collect()
}

fn source_control_row_action_label(
    stage: GitChangeStage,
    kind: SourceControlRowActionKind,
) -> &'static str {
    match kind {
        SourceControlRowActionKind::OpenChanges => open_changes_label_for_stage(stage),
        SourceControlRowActionKind::CompareWithHead => "Compare with HEAD",
        SourceControlRowActionKind::OpenHeadFile => "Open File at HEAD",
        SourceControlRowActionKind::OpenIndexFile => "Open File at Index",
        SourceControlRowActionKind::SelectForCompare => "Select for Compare",
        SourceControlRowActionKind::CompareWithSelected => "Compare with Selected",
        SourceControlRowActionKind::OpenHunks => match stage {
            GitChangeStage::Staged => "Open Staged Hunks",
            GitChangeStage::Unstaged => "Open Hunks",
        },
        SourceControlRowActionKind::CopyPatch => "Copy Patch",
        SourceControlRowActionKind::RevealInExplorer => "Reveal in Explorer",
        SourceControlRowActionKind::OpenFile => "Open File",
        SourceControlRowActionKind::OpenFileToResolve => "Open File to Resolve",
        SourceControlRowActionKind::OpenBlame => "Open Blame",
        SourceControlRowActionKind::Stage => "Stage Changes",
        SourceControlRowActionKind::Unstage => "Unstage Changes",
        SourceControlRowActionKind::Discard => "Discard Changes",
    }
}

fn source_control_row_action_icon(kind: SourceControlRowActionKind) -> IconKind {
    match kind {
        SourceControlRowActionKind::OpenChanges => IconKind::Code,
        SourceControlRowActionKind::CompareWithHead => IconKind::Panes,
        SourceControlRowActionKind::OpenHeadFile => IconKind::File,
        SourceControlRowActionKind::OpenIndexFile => IconKind::File,
        SourceControlRowActionKind::SelectForCompare => IconKind::Panes,
        SourceControlRowActionKind::CompareWithSelected => IconKind::Panes,
        SourceControlRowActionKind::OpenHunks => IconKind::Search,
        SourceControlRowActionKind::CopyPatch => IconKind::Copy,
        SourceControlRowActionKind::RevealInExplorer => IconKind::FolderOpen,
        SourceControlRowActionKind::OpenFile => IconKind::File,
        SourceControlRowActionKind::OpenFileToResolve => IconKind::File,
        SourceControlRowActionKind::OpenBlame => IconKind::GitBranch,
        SourceControlRowActionKind::Stage => IconKind::Plus,
        SourceControlRowActionKind::Unstage => IconKind::Minus,
        SourceControlRowActionKind::Discard => IconKind::Trash,
    }
}

fn source_control_row_action_command(
    entry: &GitStatusEntry,
    kind: SourceControlRowActionKind,
) -> Command {
    match kind {
        SourceControlRowActionKind::OpenChanges => open_changes_command_for_entry(entry),
        SourceControlRowActionKind::CompareWithHead => {
            Command::OpenFileHeadChanges(entry.path.clone())
        }
        SourceControlRowActionKind::OpenHeadFile => {
            Command::OpenFileHeadRevision(entry.path.clone())
        }
        SourceControlRowActionKind::OpenIndexFile => {
            Command::OpenFileIndexRevision(entry.path.clone())
        }
        SourceControlRowActionKind::SelectForCompare => {
            Command::SelectFileForCompare(entry.path.clone())
        }
        SourceControlRowActionKind::CompareWithSelected => {
            Command::CompareFileWithSelected(entry.path.clone())
        }
        SourceControlRowActionKind::OpenHunks => match entry.stage {
            GitChangeStage::Staged => Command::OpenStagedFileHunks(entry.path.clone()),
            GitChangeStage::Unstaged => Command::OpenFileHunks(entry.path.clone()),
        },
        SourceControlRowActionKind::CopyPatch => copy_patch_command_for_entry(entry),
        SourceControlRowActionKind::RevealInExplorer => {
            Command::RevealFileInExplorer(entry.path.clone())
        }
        SourceControlRowActionKind::OpenFile => Command::OpenFile(entry.path.clone()),
        SourceControlRowActionKind::OpenFileToResolve => Command::OpenFile(entry.path.clone()),
        SourceControlRowActionKind::OpenBlame => Command::OpenFileBlame(entry.path.clone()),
        SourceControlRowActionKind::Stage => Command::StageFileChange(entry.path.clone()),
        SourceControlRowActionKind::Unstage => Command::UnstageFileChange(entry.path.clone()),
        SourceControlRowActionKind::Discard => Command::DiscardFileChanges(entry.path.clone()),
    }
}

pub(super) fn source_control_validated_row_action_command(
    entry: &GitStatusEntry,
    target: SourceControlRowActionTarget,
    openability: SourceControlRowOpenability,
    show_inline_open_file_action: bool,
) -> Option<Command> {
    if source_control_entry_action_fingerprint(entry) != target.entry_fingerprint {
        return None;
    }
    let kind = target.kind;
    source_control_row_action_allowed(
        entry.stage,
        entry.status,
        openability.source_exists,
        openability.can_compare_with_selected,
        show_inline_open_file_action,
        kind,
    )
    .then(|| source_control_row_action_command(entry, kind))
}

fn source_control_row_action_kind_tag(kind: SourceControlRowActionKind) -> u8 {
    match kind {
        SourceControlRowActionKind::OpenChanges => 1,
        SourceControlRowActionKind::CompareWithHead => 2,
        SourceControlRowActionKind::OpenHeadFile => 3,
        SourceControlRowActionKind::OpenIndexFile => 4,
        SourceControlRowActionKind::SelectForCompare => 5,
        SourceControlRowActionKind::CompareWithSelected => 6,
        SourceControlRowActionKind::CopyPatch => 7,
        SourceControlRowActionKind::RevealInExplorer => 8,
        SourceControlRowActionKind::OpenFile => 9,
        SourceControlRowActionKind::OpenFileToResolve => 10,
        SourceControlRowActionKind::OpenBlame => 11,
        SourceControlRowActionKind::OpenHunks => 12,
        SourceControlRowActionKind::Stage => 13,
        SourceControlRowActionKind::Unstage => 14,
        SourceControlRowActionKind::Discard => 15,
    }
}

pub(super) fn source_control_row_action_strip_width(count: usize) -> f32 {
    if count == 0 {
        0.0
    } else {
        6.0 + count as f32 * 24.0 + count.saturating_sub(1) as f32 * 2.0
    }
}

pub(super) fn render_source_control_row_actions(
    ui: &mut egui::Ui,
    row_rect: Rect,
    row_index: usize,
    entry: &GitStatusEntry,
    source_exists: bool,
    can_compare_with_selected: bool,
    show_inline_open_file_action: bool,
    action_count: usize,
) -> Option<SourceControlRowActionTarget> {
    let total_width = source_control_row_action_strip_width(action_count);
    let mut x = row_rect.right() - total_width;
    let y = row_rect.center().y - 12.0;
    let mut clicked = None;

    for (action_index, kind) in source_control_row_action_kinds_for_state(
        entry.stage,
        entry.status,
        source_exists,
        can_compare_with_selected,
        show_inline_open_file_action,
    )
    .enumerate()
    {
        let rect = Rect::from_min_size(pos2(x, y), vec2(24.0, 24.0));
        let target = SourceControlRowActionTarget::new(kind, entry);
        if source_control_row_action_button(
            ui,
            rect,
            row_index,
            action_index,
            target,
            source_control_row_action_icon(kind),
            source_control_row_action_label(entry.stage, kind),
        ) {
            clicked = Some(target);
        }
        x += 26.0;
    }

    clicked
}

fn source_control_row_action_button(
    ui: &mut egui::Ui,
    rect: Rect,
    row_index: usize,
    action_index: usize,
    target: SourceControlRowActionTarget,
    icon: IconKind,
    tooltip: &'static str,
) -> bool {
    let id = ui.make_persistent_id((
        "source-control-row-action",
        row_index,
        target.entry_fingerprint,
        source_control_row_action_kind_tag(target.kind),
        action_index,
    ));
    let response = ui.interact(rect, id, Sense::click());
    let visuals = ui.visuals();
    let fill = if response.is_pointer_button_down_on() {
        visuals.widgets.active.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.widgets.inactive.weak_bg_fill
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, visuals.widgets.hovered.bg_stroke.color),
            StrokeKind::Inside,
        );
    }
    draw_icon(
        ui,
        Rect::from_center_size(rect.center(), vec2(17.0, 17.0)),
        icon,
        visuals.widgets.inactive.fg_stroke.color,
    );

    let clicked = response.clicked();
    response.on_hover_text(tooltip);
    clicked
}

pub(super) fn source_control_has_head_revision(status: GitFileStatus) -> bool {
    !matches!(status, GitFileStatus::Added | GitFileStatus::Untracked)
}

pub(super) fn source_control_has_index_revision(
    stage: GitChangeStage,
    status: GitFileStatus,
) -> bool {
    match stage {
        GitChangeStage::Staged => !matches!(
            status,
            GitFileStatus::Deleted | GitFileStatus::Untracked | GitFileStatus::Conflicted
        ),
        GitChangeStage::Unstaged => !matches!(
            status,
            GitFileStatus::Added | GitFileStatus::Untracked | GitFileStatus::Conflicted
        ),
    }
}

pub(crate) fn source_control_hunks_available(
    stage: GitChangeStage,
    status: GitFileStatus,
    source_exists: bool,
) -> bool {
    if status == GitFileStatus::Conflicted {
        return false;
    }
    match stage {
        GitChangeStage::Staged => true,
        GitChangeStage::Unstaged => status == GitFileStatus::Deleted || source_exists,
    }
}

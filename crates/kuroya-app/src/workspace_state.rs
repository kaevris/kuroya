use kuroya_core::BufferId;
#[cfg(test)]
use kuroya_core::TextBuffer;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub(crate) use crate::workspace_event_guards::{
    active_buffer_lsp_position_matches, active_buffer_path_matches,
    active_buffer_path_version_matches, background_workspace_event_matches,
    buffer_id_path_version_matches, lsp_event_path_is_current, paths_match_lexically,
    workspace_event_matches,
};
#[cfg(test)]
pub(crate) use crate::workspace_state::watched_paths::WatchedPathChanges;
#[cfg(test)]
pub(crate) use crate::workspace_state::watched_paths::classify_watched_paths;
pub(crate) use crate::workspace_state::watched_paths::{
    app_owned_state_dirs_in_workspace, classify_watched_paths_with_filter,
    dirty_open_buffers_for_changes, reloadable_open_buffers_for_changes, settings_path,
};

mod watched_paths;

pub(crate) type PaneId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenFileRequest {
    AlreadyOpen(BufferId),
    AlreadyPending,
    Spawn,
}

pub(crate) fn paths_match_exact_or_lexically(left: &Path, right: &Path) -> bool {
    left == right || paths_match_lexically(left, right)
}

pub(crate) fn path_set_contains_exact_or_lexically(paths: &HashSet<PathBuf>, path: &Path) -> bool {
    paths.contains(path)
        || paths
            .iter()
            .any(|candidate| paths_match_exact_or_lexically(candidate, path))
}

pub(crate) fn remove_path_set_entry_exact_or_lexically(
    paths: &mut HashSet<PathBuf>,
    path: &Path,
) -> bool {
    if paths.remove(path) {
        return true;
    }

    let mut removed = false;
    paths.retain(|candidate| {
        if !removed && paths_match_lexically(candidate, path) {
            removed = true;
            false
        } else {
            true
        }
    });
    removed
}

pub(crate) fn remove_path_map_entry_exact_or_lexically<T>(
    paths: &mut HashMap<PathBuf, T>,
    path: &Path,
) -> Option<T> {
    if let Some(value) = paths.remove(path) {
        return Some(value);
    }

    paths
        .keys()
        .find(|candidate| paths_match_lexically(candidate, path))
        .cloned()
        .and_then(|candidate| paths.remove(&candidate))
}

#[cfg(test)]
pub(crate) fn classify_open_file_request(
    path: &Path,
    buffers: &[TextBuffer],
    pending_open_paths: &HashSet<PathBuf>,
) -> OpenFileRequest {
    let mut lexical_buffer_id = None;
    for buffer in buffers {
        let Some(candidate) = buffer.path() else {
            continue;
        };
        if candidate == path {
            return OpenFileRequest::AlreadyOpen(buffer.id());
        }
        if lexical_buffer_id.is_none() && paths_match_lexically(candidate, path) {
            lexical_buffer_id = Some(buffer.id());
        }
    }

    if let Some(id) = lexical_buffer_id {
        OpenFileRequest::AlreadyOpen(id)
    } else if path_set_contains_exact_or_lexically(pending_open_paths, path) {
        OpenFileRequest::AlreadyPending
    } else {
        OpenFileRequest::Spawn
    }
}

pub(crate) fn should_activate_loaded_file(
    explicit_activate: bool,
    has_pending_jump: bool,
    has_active_buffer: bool,
    waiting_for_pending_active: bool,
) -> bool {
    explicit_activate || has_pending_jump || (!has_active_buffer && !waiting_for_pending_active)
}

pub(crate) fn take_pending_panes_for_path(
    pending_pane_paths: &mut HashMap<PaneId, PathBuf>,
    path: &Path,
) -> Vec<PaneId> {
    let mut pane_ids = Vec::new();
    pending_pane_paths.retain(|pane_id, pane_path| {
        if paths_match_exact_or_lexically(pane_path, path) {
            pane_ids.push(*pane_id);
            false
        } else {
            true
        }
    });
    if pane_ids.len() > 1 {
        pane_ids.sort_unstable();
    }
    pane_ids
}

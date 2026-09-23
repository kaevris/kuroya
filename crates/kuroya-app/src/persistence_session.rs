use crate::{
    app_session::session_path_dedupe_key,
    buffer_find_history::{
        MAX_BUFFER_FIND_HISTORY, normalize_buffer_find_query_history,
        normalize_buffer_find_replacement_history,
    },
    command_palette_items::{
        MAX_COMMAND_PALETTE_QUERY_MEMORY, MAX_COMMAND_PALETTE_RECENT_COMMANDS,
        normalize_command_palette_query_memory, normalize_recent_palette_commands,
    },
    history::{
        CLOSED_FILE_HISTORY_LIMIT, NAVIGATION_HISTORY_LIMIT, normalize_closed_file_history,
        normalize_navigation_history,
    },
    layout::{
        clamp_diagnostics_panel_width, clamp_explorer_width, clamp_source_control_width,
        clamp_symbols_panel_width, clamp_terminal_height, normalize_weights,
    },
    lsp_workspace_symbol_ranking::MAX_WORKSPACE_SYMBOL_QUERY_MEMORY,
    persistence::{PersistedSession, SkippedRecoveredBuffer, normalize_recent_projects},
    persistence_models::{
        PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS, PERSISTED_SESSION_FOLD_RANGES_MAX,
        PERSISTED_SESSION_HISTORY_STATES_MAX, PERSISTED_SESSION_PANES_MAX,
        PERSISTED_SESSION_PATHS_MAX, PERSISTED_SESSION_RECOVERY_BUFFERS_MAX,
        PERSISTED_SESSION_RECOVERY_SKIPPED_MAX, PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS,
        PERSISTED_SESSION_SELECTIONS_MAX, PERSISTED_SESSION_TERMINAL_SCROLLBACK_MAX_CHARS,
        PERSISTED_SESSION_TERMINAL_SESSIONS_MAX, PERSISTED_SESSION_VIEW_STATES_MAX,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    },
    persistence_storage::{
        atomic_write, atomic_write_async, legacy_session_path, legacy_session_snapshots_dir,
        read_file_bytes_with_limit, read_file_bytes_with_limit_async, session_path,
        session_snapshots_dir, state_dir,
    },
    project_search_state::{MAX_PROJECT_SEARCH_RECENT_QUERIES, normalize_recent_project_searches},
    quick_open::{
        MAX_QUICK_OPEN_QUERY_MEMORY, MAX_QUICK_OPEN_RECENT_FILES,
        normalize_quick_open_query_memory, normalize_quick_open_recent_files,
    },
    source_control_panel::{
        SOURCE_CONTROL_COMMIT_HISTORY_LIMIT, normalize_source_control_commit_history,
    },
    workspace_trust::{trusted_workspace_paths_match, workspace_path_stays_within_root_lexically},
};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs,
    io::ErrorKind,
    mem,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_SESSION_SNAPSHOTS: usize = 8;
pub(crate) const PERSISTED_SESSION_MAX_BYTES: u64 = 8 * 1024 * 1024;
const PERSISTED_SESSION_MAX_BYTES_USIZE: usize = PERSISTED_SESSION_MAX_BYTES as usize;
const SESSION_SNAPSHOT_SCAN_LIMIT: usize = MAX_SESSION_SNAPSHOTS * 128;
const SESSION_SNAPSHOT_SCAN_TRIM_AT: usize = SESSION_SNAPSHOT_SCAN_LIMIT * 2;
static SESSION_SNAPSHOT_COUNTER: AtomicU64 = AtomicU64::new(0);

impl PersistedSession {
    pub fn load(workspace_root: &Path) -> anyhow::Result<Option<Self>> {
        let path = session_path(workspace_root);
        let snapshots = session_snapshots_dir(workspace_root);
        if let Some(session) = load_session_with_snapshots(workspace_root, &path, &snapshots)? {
            return Ok(Some(session));
        }

        let legacy_path = legacy_session_path(workspace_root);
        let legacy_snapshots = legacy_session_snapshots_dir(workspace_root);
        load_session_with_snapshots(workspace_root, &legacy_path, &legacy_snapshots)
    }
}

fn load_session_with_snapshots(
    workspace_root: &Path,
    path: &Path,
    snapshots_dir: &Path,
) -> anyhow::Result<Option<PersistedSession>> {
    let bytes = match read_file_bytes_with_limit(path, PERSISTED_SESSION_MAX_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return load_latest_session_snapshot_in_dir(workspace_root, snapshots_dir);
        }
        Err(error) if error.kind() == ErrorKind::InvalidData => {
            let _ = quarantine_corrupt_session_file(path);
            return load_latest_session_snapshot_in_dir(workspace_root, snapshots_dir);
        }
        Err(error) => return Err(error.into()),
    };

    match serde_json::from_slice(&bytes) {
        Ok(mut session) if persisted_session_workspace_matches(workspace_root, &session) => {
            normalize_persisted_session_paths_for_restore(workspace_root, &mut session);
            Ok(Some(session))
        }
        Ok(_) => {
            let _ = quarantine_mismatched_session_file(path);
            load_latest_session_snapshot_in_dir(workspace_root, snapshots_dir)
        }
        Err(_) => {
            let _ = quarantine_corrupt_session_file(path);
            load_latest_session_snapshot_in_dir(workspace_root, snapshots_dir)
        }
    }
}

pub(crate) fn persisted_session_workspace_matches(
    workspace_root: &Path,
    session: &PersistedSession,
) -> bool {
    trusted_workspace_paths_match(&session.workspace_root, workspace_root)
}

fn quarantine_corrupt_session(workspace_root: &Path) -> anyhow::Result<PathBuf> {
    quarantine_corrupt_session_file(&session_path(workspace_root))
}

fn quarantine_corrupt_session_file(path: &Path) -> anyhow::Result<PathBuf> {
    let quarantine = corrupt_session_path(path);
    std::fs::rename(path, &quarantine)?;
    Ok(quarantine)
}

fn corrupt_session_path(path: &Path) -> PathBuf {
    quarantined_session_path(path, "corrupt")
}

fn quarantine_mismatched_session(workspace_root: &Path) -> anyhow::Result<PathBuf> {
    quarantine_mismatched_session_file(&session_path(workspace_root))
}

fn quarantine_mismatched_session_file(path: &Path) -> anyhow::Result<PathBuf> {
    let quarantine = mismatched_session_path(path);
    std::fs::rename(path, &quarantine)?;
    Ok(quarantine)
}

fn mismatched_session_path(path: &Path) -> PathBuf {
    quarantined_session_path(path, "mismatched")
}

#[cfg(test)]
fn load_latest_session_snapshot_after_quarantine(
    workspace_root: &Path,
    quarantine: impl FnOnce(&Path) -> anyhow::Result<PathBuf>,
) -> anyhow::Result<Option<PersistedSession>> {
    let _ = quarantine(workspace_root);
    let dir = session_snapshots_dir(workspace_root);
    load_latest_session_snapshot_in_dir(workspace_root, &dir)
}

fn quarantined_session_path(path: &Path, reason: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session.json");
    path.with_file_name(format!(
        "{file_name}.{reason}.{}.{}",
        std::process::id(),
        unique
    ))
}

fn load_latest_session_snapshot_in_dir(
    workspace_root: &Path,
    dir: &Path,
) -> anyhow::Result<Option<PersistedSession>> {
    for path in session_snapshot_files(dir)?.into_iter().rev() {
        match read_file_bytes_with_limit(&path, PERSISTED_SESSION_MAX_BYTES) {
            Ok(bytes) => match serde_json::from_slice::<PersistedSession>(&bytes) {
                Ok(mut session)
                    if persisted_session_workspace_matches(workspace_root, &session) =>
                {
                    normalize_persisted_session_paths_for_restore(workspace_root, &mut session);
                    return Ok(Some(session));
                }
                Ok(_) => {
                    let _ = quarantine_mismatched_session_file(&path);
                }
                Err(_) => {
                    let _ = quarantine_corrupt_session_file(&path);
                }
            },
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) if error.kind() == ErrorKind::InvalidData => {
                let _ = quarantine_corrupt_session_file(&path);
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(None)
}

pub fn save_session(workspace_root: &Path, session: &PersistedSession) -> anyhow::Result<()> {
    let dir = state_dir(workspace_root);
    fs::create_dir_all(&dir)?;
    let bytes = session_bytes_for_write(session)?;
    let path = session_path(workspace_root);
    let previous = read_existing_session_bytes(&path)?;
    if previous.as_deref() == Some(bytes.as_slice()) {
        return Ok(());
    }
    if let Some(previous) = previous {
        match serde_json::from_slice::<PersistedSession>(&previous) {
            Ok(previous_session) => {
                if persisted_session_workspace_matches(workspace_root, &previous_session) {
                    write_session_snapshot_best_effort(workspace_root, &previous);
                } else {
                    let _ = quarantine_mismatched_session(workspace_root);
                }
            }
            Err(_) => {
                let _ = quarantine_corrupt_session(workspace_root);
            }
        }
    }
    atomic_write(&path, &bytes)?;
    Ok(())
}

pub async fn save_session_async(
    workspace_root: PathBuf,
    session: PersistedSession,
) -> anyhow::Result<()> {
    let dir = state_dir(&workspace_root);
    tokio::fs::create_dir_all(&dir).await?;
    let bytes = session_bytes_for_write(&session)?;
    let path = session_path(&workspace_root);
    let previous = read_existing_session_bytes_async(&path).await?;
    if previous.as_deref() == Some(bytes.as_slice()) {
        return Ok(());
    }
    if let Some(previous) = previous {
        match serde_json::from_slice::<PersistedSession>(&previous) {
            Ok(previous_session) => {
                if persisted_session_workspace_matches(&workspace_root, &previous_session) {
                    write_session_snapshot_best_effort_async(&workspace_root, &previous).await;
                } else {
                    let _ = quarantine_mismatched_session(&workspace_root);
                }
            }
            Err(_) => {
                let _ = quarantine_corrupt_session(&workspace_root);
            }
        }
    }
    atomic_write_async(&path, &bytes).await?;
    Ok(())
}

pub(crate) fn session_bytes_for_write(session: &PersistedSession) -> anyhow::Result<Vec<u8>> {
    let mut candidate = session.clone();
    let workspace_root = candidate.workspace_root.clone();
    normalize_persisted_session_paths_for_write(&workspace_root, &mut candidate);
    sanitize_session_for_write(&mut candidate);
    let mut bytes = serde_json::to_vec_pretty(&candidate)?;
    if session_bytes_fit(&bytes) {
        return finalized_session_bytes_for_write(candidate);
    }

    if clear_terminal_scrollback(&mut candidate) {
        bytes = serde_json::to_vec_pretty(&candidate)?;
        if session_bytes_fit(&bytes) {
            return finalized_session_bytes_for_write(candidate);
        }
    }

    let mut volatile_candidate = candidate.clone();
    if trim_volatile_session_text(&mut volatile_candidate) {
        let volatile_bytes = serde_json::to_vec_pretty(&volatile_candidate)?;
        if session_bytes_fit(&volatile_bytes) {
            return finalized_session_bytes_for_write(volatile_candidate);
        }
    }

    while !session_bytes_fit(&bytes) && skip_largest_recovery_entry(&mut candidate) {
        bytes = serde_json::to_vec_pretty(&candidate)?;
    }
    if session_bytes_fit(&bytes) {
        return finalized_session_bytes_for_write(candidate);
    }

    if !candidate.history_states.is_empty() || !candidate.recovery_history_states.is_empty() {
        candidate.history_states.clear();
        candidate.recovery_history_states.clear();
        bytes = serde_json::to_vec_pretty(&candidate)?;
        if session_bytes_fit(&bytes) {
            return finalized_session_bytes_for_write(candidate);
        }
    }

    if trim_volatile_session_text(&mut candidate) {
        bytes = serde_json::to_vec_pretty(&candidate)?;
        if session_bytes_fit(&bytes) {
            return finalized_session_bytes_for_write(candidate);
        }
    }

    if clear_volatile_session_state(&mut candidate) {
        bytes = serde_json::to_vec_pretty(&candidate)?;
        if session_bytes_fit(&bytes) {
            return finalized_session_bytes_for_write(candidate);
        }
    }

    anyhow::bail!(
        "persisted session is too large to save after trimming volatile state ({} bytes > {} bytes)",
        bytes.len(),
        PERSISTED_SESSION_MAX_BYTES_USIZE
    );
}

fn finalized_session_bytes_for_write(mut session: PersistedSession) -> anyhow::Result<Vec<u8>> {
    normalize_restored_session_scalars(&mut session);
    truncate_restored_session_text(&mut session);
    normalize_restored_session_lists(&mut session);
    let bytes = serde_json::to_vec_pretty(&session)?;
    if session_bytes_fit(&bytes) {
        return Ok(bytes);
    }

    anyhow::bail!(
        "persisted session is too large to save after enforcing restored state bounds ({} bytes > {} bytes)",
        bytes.len(),
        PERSISTED_SESSION_MAX_BYTES_USIZE
    );
}

pub(crate) fn normalize_persisted_session_paths_for_restore(
    workspace_root: &Path,
    session: &mut PersistedSession,
) {
    truncate_restored_session_lists(session);
    normalize_persisted_session_paths(workspace_root, session);
    sanitize_persisted_session_for_restore(session);
}

fn normalize_persisted_session_paths_for_write(
    workspace_root: &Path,
    session: &mut PersistedSession,
) {
    truncate_restored_session_lists(session);
    normalize_persisted_session_paths(workspace_root, session);
    normalize_restored_session_lists(session);
    prune_invalid_restored_session_state(session);
}

fn normalize_persisted_session_paths(workspace_root: &Path, session: &mut PersistedSession) {
    let normalizer = SessionPathNormalizer::new(workspace_root);

    session.open_files = normalizer.normalize_paths(mem::take(&mut session.open_files));
    session.active_path = normalizer.normalize_option(mem::take(&mut session.active_path));
    for pane_path in &mut session.pane_paths {
        *pane_path = normalizer.normalize_option(pane_path.take());
    }
    normalize_buffer_view_state_paths(&normalizer, &mut session.view_states);
    normalize_pane_view_state_paths(&normalizer, &mut session.pane_view_states);
    normalize_buffer_history_state_paths(&normalizer, &mut session.history_states);
    normalize_buffer_fold_state_paths(&normalizer, &mut session.fold_states);
    session.explorer_expanded =
        normalizer.normalize_paths(mem::take(&mut session.explorer_expanded));
    session.explorer_revealed_path =
        normalizer.normalize_option(mem::take(&mut session.explorer_revealed_path));
    session.quick_open_recent_files =
        normalizer.normalize_paths(mem::take(&mut session.quick_open_recent_files));
    normalize_quick_open_query_memory_paths(&normalizer, &mut session.quick_open_query_memory);
    normalize_workspace_symbol_query_memory_paths(
        &normalizer,
        &mut session.workspace_symbol_query_memory,
    );
    normalize_navigation_location_paths(&normalizer, &mut session.navigation_back);
    normalize_navigation_location_paths(&normalizer, &mut session.navigation_forward);
    normalize_closed_file_entry_paths(&normalizer, &mut session.closed_files);
    normalize_terminal_session_cwd_paths(&normalizer, &mut session.terminal_sessions);
    for recovered in &mut session.recovery {
        recovered.path = normalizer.normalize_option(recovered.path.take());
    }
    for skipped in &mut session.recovery_skipped {
        skipped.path = normalizer.normalize_option(skipped.path.take());
    }
}

fn sanitize_persisted_session_for_restore(session: &mut PersistedSession) {
    truncate_restored_session_text(session);
    truncate_restored_session_lists(session);
    normalize_restored_session_scalars(session);
    normalize_restored_session_lists(session);
    prune_invalid_restored_session_state(session);
}

fn truncate_restored_session_lists(session: &mut PersistedSession) {
    session.open_files.truncate(PERSISTED_SESSION_PATHS_MAX);
    session.pane_paths.truncate(PERSISTED_SESSION_PANES_MAX);
    session.pane_weights.truncate(PERSISTED_SESSION_PANES_MAX);
    session
        .view_states
        .truncate(PERSISTED_SESSION_VIEW_STATES_MAX);
    session
        .pane_view_states
        .truncate(PERSISTED_SESSION_VIEW_STATES_MAX);
    session
        .history_states
        .truncate(PERSISTED_SESSION_HISTORY_STATES_MAX);
    session
        .recovery_view_states
        .truncate(PERSISTED_SESSION_VIEW_STATES_MAX);
    session
        .recovery_history_states
        .truncate(PERSISTED_SESSION_HISTORY_STATES_MAX);
    session
        .fold_states
        .truncate(PERSISTED_SESSION_VIEW_STATES_MAX);
    session
        .explorer_expanded
        .truncate(PERSISTED_SESSION_PATHS_MAX);
    session
        .project_search_recent
        .truncate(MAX_PROJECT_SEARCH_RECENT_QUERIES);
    session
        .buffer_find_query_history
        .truncate(MAX_BUFFER_FIND_HISTORY);
    session
        .buffer_find_replacement_history
        .truncate(MAX_BUFFER_FIND_HISTORY);
    session
        .source_control_commit_history
        .truncate(SOURCE_CONTROL_COMMIT_HISTORY_LIMIT);
    session
        .terminal_sessions
        .truncate(PERSISTED_SESSION_TERMINAL_SESSIONS_MAX);
    session
        .terminal_split_weights
        .truncate(PERSISTED_SESSION_TERMINAL_SESSIONS_MAX);
    session
        .recent_projects
        .truncate(PERSISTED_SESSION_PATHS_MAX);
    session
        .quick_open_recent_files
        .truncate(PERSISTED_SESSION_PATHS_MAX);
    session
        .quick_open_query_memory
        .truncate(MAX_QUICK_OPEN_QUERY_MEMORY);
    session
        .workspace_symbol_query_memory
        .truncate(MAX_WORKSPACE_SYMBOL_QUERY_MEMORY);
    session
        .command_recent
        .truncate(MAX_COMMAND_PALETTE_RECENT_COMMANDS);
    session
        .command_query_memory
        .truncate(MAX_COMMAND_PALETTE_QUERY_MEMORY);
    session.navigation_back.truncate(NAVIGATION_HISTORY_LIMIT);
    session
        .navigation_forward
        .truncate(NAVIGATION_HISTORY_LIMIT);
    session.closed_files.truncate(CLOSED_FILE_HISTORY_LIMIT);
    session
        .recovery
        .truncate(PERSISTED_SESSION_RECOVERY_BUFFERS_MAX);
    session
        .recovery_skipped
        .truncate(PERSISTED_SESSION_RECOVERY_SKIPPED_MAX);

    for state in &mut session.view_states {
        state.selections.truncate(PERSISTED_SESSION_SELECTIONS_MAX);
    }
    for state in &mut session.recovery_view_states {
        state.selections.truncate(PERSISTED_SESSION_SELECTIONS_MAX);
    }
    for state in &mut session.fold_states {
        state.ranges.truncate(PERSISTED_SESSION_FOLD_RANGES_MAX);
    }
}

fn sanitize_session_for_write(session: &mut PersistedSession) {
    normalize_restored_session_scalars(session);
    normalize_weights(&mut session.pane_weights);
    normalize_weights(&mut session.terminal_split_weights);
    session.workspace_symbol_query_memory =
        crate::lsp_workspace_symbol_ranking::normalize_workspace_symbol_query_memory(
            mem::take(&mut session.workspace_symbol_query_memory),
            &session.workspace_root,
            crate::lsp_workspace_symbol_ranking::MAX_WORKSPACE_SYMBOL_QUERY_MEMORY,
        )
        .into_iter()
        .collect();
    session.command_query_memory =
        crate::command_palette_items::normalize_command_palette_query_memory(
            mem::take(&mut session.command_query_memory),
            crate::command_palette_items::MAX_COMMAND_PALETTE_QUERY_MEMORY,
        )
        .into_iter()
        .collect();
}

fn normalize_restored_session_scalars(session: &mut PersistedSession) {
    session.explorer_width = clamp_explorer_width(session.explorer_width);
    session.symbols_panel_width = clamp_symbols_panel_width(session.symbols_panel_width);
    session.diagnostics_panel_width =
        clamp_diagnostics_panel_width(session.diagnostics_panel_width);
    session.source_control_width = clamp_source_control_width(session.source_control_width);
    session.terminal_height = clamp_terminal_height(session.terminal_height);
}

fn normalize_restored_session_lists(session: &mut PersistedSession) {
    normalize_weights(&mut session.pane_weights);
    normalize_weights(&mut session.terminal_split_weights);
    session.recent_projects = normalize_recent_projects(mem::take(&mut session.recent_projects));
    session.project_search_recent = normalize_recent_project_searches(
        mem::take(&mut session.project_search_recent),
        MAX_PROJECT_SEARCH_RECENT_QUERIES,
    )
    .into_iter()
    .collect();
    session.buffer_find_query_history = normalize_buffer_find_query_history(
        mem::take(&mut session.buffer_find_query_history),
        MAX_BUFFER_FIND_HISTORY,
    )
    .into_iter()
    .collect();
    session.buffer_find_replacement_history = normalize_buffer_find_replacement_history(
        mem::take(&mut session.buffer_find_replacement_history),
        MAX_BUFFER_FIND_HISTORY,
    )
    .into_iter()
    .collect();
    session.source_control_commit_history = normalize_source_control_commit_history(
        mem::take(&mut session.source_control_commit_history),
        SOURCE_CONTROL_COMMIT_HISTORY_LIMIT,
    );
    session.quick_open_recent_files = normalize_quick_open_recent_files(
        mem::take(&mut session.quick_open_recent_files),
        &session.workspace_root,
        MAX_QUICK_OPEN_RECENT_FILES,
    )
    .into_iter()
    .collect();
    session.quick_open_query_memory = normalize_quick_open_query_memory(
        mem::take(&mut session.quick_open_query_memory),
        &session.workspace_root,
        MAX_QUICK_OPEN_QUERY_MEMORY,
    )
    .into_iter()
    .collect();
    session.workspace_symbol_query_memory =
        crate::lsp_workspace_symbol_ranking::normalize_workspace_symbol_query_memory(
            mem::take(&mut session.workspace_symbol_query_memory),
            &session.workspace_root,
            crate::lsp_workspace_symbol_ranking::MAX_WORKSPACE_SYMBOL_QUERY_MEMORY,
        )
        .into_iter()
        .collect();
    session.command_recent = normalize_recent_palette_commands(
        mem::take(&mut session.command_recent),
        MAX_COMMAND_PALETTE_RECENT_COMMANDS,
    )
    .into_iter()
    .collect();
    session.command_query_memory = normalize_command_palette_query_memory(
        mem::take(&mut session.command_query_memory),
        MAX_COMMAND_PALETTE_QUERY_MEMORY,
    )
    .into_iter()
    .collect();
    session.navigation_back = normalize_navigation_history(
        mem::take(&mut session.navigation_back)
            .into_iter()
            .map(|location| location.into_navigation_location()),
        NAVIGATION_HISTORY_LIMIT,
    )
    .into_iter()
    .map(|location| {
        crate::persistence::PersistedNavigationLocation::from_navigation_location(&location)
    })
    .collect();
    session.navigation_forward = normalize_navigation_history(
        mem::take(&mut session.navigation_forward)
            .into_iter()
            .map(|location| location.into_navigation_location()),
        NAVIGATION_HISTORY_LIMIT,
    )
    .into_iter()
    .map(|location| {
        crate::persistence::PersistedNavigationLocation::from_navigation_location(&location)
    })
    .collect();
    session.closed_files = normalize_closed_file_history(
        mem::take(&mut session.closed_files)
            .into_iter()
            .map(|entry| entry.into_closed_file_entry()),
        CLOSED_FILE_HISTORY_LIMIT,
    )
    .into_iter()
    .map(|entry| crate::persistence::PersistedClosedFileEntry::from_closed_file_entry(&entry))
    .collect();
}

fn truncate_restored_session_text(session: &mut PersistedSession) {
    truncate_string_chars(
        &mut session.project_search_query,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    truncate_string_chars(
        &mut session.project_search_include,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    truncate_string_chars(
        &mut session.project_search_exclude,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    truncate_string_chars(
        &mut session.buffer_find_query,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    truncate_string_chars(
        &mut session.buffer_find_replacement,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    truncate_string_chars(
        &mut session.source_control_query,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    truncate_string_chars(
        &mut session.source_control_commit_message,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    for message in &mut session.source_control_commit_history {
        truncate_string_chars(message, PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS);
    }
    truncate_string_chars(
        &mut session.source_control_stash_message,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    truncate_string_chars(
        &mut session.source_control_history_query,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    for terminal in &mut session.terminal_sessions {
        truncate_string_chars(
            &mut terminal.scrollback,
            PERSISTED_SESSION_TERMINAL_SCROLLBACK_MAX_CHARS,
        );
        truncate_option_string_chars(
            &mut terminal.process_label,
            PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS,
        );
        truncate_option_string_chars(
            &mut terminal.window_title,
            PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS,
        );
    }
    for recovered in &mut session.recovery {
        truncate_string_chars(
            &mut recovered.display_name,
            PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS,
        );
        truncate_string_chars(
            &mut recovered.text,
            PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS,
        );
    }
    for skipped in &mut session.recovery_skipped {
        truncate_string_chars(
            &mut skipped.display_name,
            PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS,
        );
        truncate_string_chars(
            &mut skipped.reason,
            PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS,
        );
    }
}

fn prune_invalid_restored_session_state(session: &mut PersistedSession) {
    if session
        .active_pane_index
        .is_some_and(|index| index >= session.pane_paths.len())
    {
        session.active_pane_index = None;
    }
    session.pane_weights.truncate(session.pane_paths.len());
    session
        .terminal_split_weights
        .truncate(session.terminal_sessions.len());
    if session.terminal_sessions.is_empty() {
        session.terminal_active_session = 0;
    } else {
        session.terminal_active_session = session
            .terminal_active_session
            .min(session.terminal_sessions.len().saturating_sub(1));
    }

    let target_path_keys = restorable_session_buffer_path_keys(session);
    for pane_path in &mut session.pane_paths {
        if pane_path
            .as_ref()
            .is_some_and(|path| !target_path_keys.contains(&session_path_dedupe_key(path)))
        {
            *pane_path = None;
        }
    }
    normalize_active_pane_index(session);
    if session
        .active_path
        .as_ref()
        .is_some_and(|path| !target_path_keys.contains(&session_path_dedupe_key(path)))
    {
        session.active_path = None;
    }
    session
        .view_states
        .retain(|state| target_path_keys.contains(&session_path_dedupe_key(&state.path)));
    let pane_path_keys = session
        .pane_paths
        .iter()
        .map(|path| path.as_ref().map(|path| session_path_dedupe_key(path)))
        .collect::<Vec<_>>();
    session.pane_view_states.retain(|state| {
        let state_key = session_path_dedupe_key(&state.path);
        target_path_keys.contains(&state_key)
            && pane_path_keys
                .get(state.pane_index)
                .and_then(Option::as_ref)
                .is_some_and(|pane_path_key| pane_path_key == &state_key)
    });
    session
        .history_states
        .retain(|state| target_path_keys.contains(&session_path_dedupe_key(&state.path)));
    for state in &mut session.fold_states {
        state
            .ranges
            .retain(|range| range.start_line < range.end_line);
    }
    session.fold_states.retain(|state| {
        !state.ranges.is_empty() && target_path_keys.contains(&session_path_dedupe_key(&state.path))
    });

    dedupe_recovered_buffers(session);
    let recovery_count = session.recovery.len();
    session
        .recovery_view_states
        .retain(|state| state.recovery_index < recovery_count);
    session
        .recovery_history_states
        .retain(|state| state.recovery_index < recovery_count);
}

fn dedupe_recovered_buffers(session: &mut PersistedSession) {
    let mut last_recovery_index_by_path = HashMap::new();
    let mut pathful_recovery_count = 0usize;
    for (index, recovered) in session.recovery.iter().enumerate() {
        let Some(path) = &recovered.path else {
            continue;
        };
        pathful_recovery_count = pathful_recovery_count.saturating_add(1);
        last_recovery_index_by_path.insert(session_path_dedupe_key(path), index);
    }
    if pathful_recovery_count == last_recovery_index_by_path.len() {
        return;
    }

    let mut recovery_index_map = HashMap::with_capacity(session.recovery.len());
    let mut recovered_buffers = Vec::with_capacity(session.recovery.len());
    for (old_index, recovered) in mem::take(&mut session.recovery).into_iter().enumerate() {
        let keep = match &recovered.path {
            Some(path) => last_recovery_index_by_path
                .get(&session_path_dedupe_key(path))
                .is_some_and(|winner_index| *winner_index == old_index),
            None => true,
        };
        if keep {
            recovery_index_map.insert(old_index, recovered_buffers.len());
            recovered_buffers.push(recovered);
        }
    }
    session.recovery = recovered_buffers;

    remap_recovery_view_state_indices(&mut session.recovery_view_states, &recovery_index_map);
    remap_recovery_history_state_indices(&mut session.recovery_history_states, &recovery_index_map);
}

fn remap_recovery_view_state_indices(
    states: &mut Vec<crate::persistence::RecoveredBufferViewState>,
    recovery_index_map: &HashMap<usize, usize>,
) {
    states.retain_mut(|state| {
        let Some(index) = recovery_index_map.get(&state.recovery_index).copied() else {
            return false;
        };
        state.recovery_index = index;
        true
    });
}

fn remap_recovery_history_state_indices(
    states: &mut Vec<crate::persistence::RecoveredBufferHistoryState>,
    recovery_index_map: &HashMap<usize, usize>,
) {
    states.retain_mut(|state| {
        let Some(index) = recovery_index_map.get(&state.recovery_index).copied() else {
            return false;
        };
        state.recovery_index = index;
        true
    });
}

fn normalize_active_pane_index(session: &mut PersistedSession) {
    if session.active_pane_index.is_some_and(|index| {
        session
            .pane_paths
            .get(index)
            .and_then(Option::as_ref)
            .is_some()
    }) {
        return;
    }

    session.active_pane_index = session
        .active_path
        .as_ref()
        .and_then(|active_path| {
            let active_path_key = session_path_dedupe_key(active_path);
            session.pane_paths.iter().position(|pane_path| {
                pane_path
                    .as_ref()
                    .is_some_and(|path| session_path_dedupe_key(path) == active_path_key)
            })
        })
        .or_else(|| session.pane_paths.iter().position(Option::is_some));
}

fn restorable_session_buffer_path_keys(session: &PersistedSession) -> HashSet<PathBuf> {
    let mut keys = HashSet::new();
    for path in &session.open_files {
        keys.insert(session_path_dedupe_key(path));
    }
    for recovered in &session.recovery {
        if let Some(path) = &recovered.path {
            keys.insert(session_path_dedupe_key(path));
        }
    }
    keys
}

fn session_bytes_fit(bytes: &[u8]) -> bool {
    bytes.len() <= PERSISTED_SESSION_MAX_BYTES_USIZE
}

fn clear_terminal_scrollback(session: &mut PersistedSession) -> bool {
    let mut changed = false;
    for terminal in &mut session.terminal_sessions {
        if !terminal.scrollback.is_empty() {
            terminal.scrollback.clear();
            changed = true;
        }
    }
    changed
}

/// Evicts the largest recovery buffer first when a session exceeds the byte
/// budget. Recovery buffers are stored in buffer-open order, so dropping
/// the LAST entry would always discard the most recently opened/edited
/// work; size-first eviction instead sacrifices the entries that free the
/// most bytes per removal, so small entries carrying real user text
/// survive. `last_edited` timestamps are not part of the persisted
/// recovery schema, so buffer size is used as the eviction signal.
fn skip_largest_recovery_entry(session: &mut PersistedSession) -> bool {
    let Some((index, recovered)) = session
        .recovery
        .iter()
        .enumerate()
        .max_by_key(|(_, recovered)| recovered.text.len())
        .map(|(index, recovered)| (index, recovered.clone()))
    else {
        return false;
    };
    session.recovery.remove(index);
    retain_remapped_recovery_view_states(&mut session.recovery_view_states, index);
    retain_remapped_recovery_history_states(&mut session.recovery_history_states, index);
    let bytes = recovered.text.len();
    push_recovery_skipped_entry(
        session,
        SkippedRecoveredBuffer {
            path: recovered.path,
            display_name: recovered.display_name,
            bytes,
            reason: format!(
                "omitted to keep session file under {PERSISTED_SESSION_MAX_BYTES_USIZE} bytes"
            ),
        },
    );
    true
}

/// Drops the view state of the removed recovery entry and shifts the indices
/// of later entries so they keep pointing at their buffers.
fn retain_remapped_recovery_view_states(
    states: &mut Vec<crate::persistence::RecoveredBufferViewState>,
    removed_index: usize,
) {
    states.retain_mut(|state| match state.recovery_index.cmp(&removed_index) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Greater => {
            state.recovery_index -= 1;
            true
        }
        std::cmp::Ordering::Less => true,
    });
}

/// Drops the history state of the removed recovery entry and shifts the
/// indices of later entries so they keep pointing at their buffers.
fn retain_remapped_recovery_history_states(
    states: &mut Vec<crate::persistence::RecoveredBufferHistoryState>,
    removed_index: usize,
) {
    states.retain_mut(|state| match state.recovery_index.cmp(&removed_index) {
        std::cmp::Ordering::Equal => false,
        std::cmp::Ordering::Greater => {
            state.recovery_index -= 1;
            true
        }
        std::cmp::Ordering::Less => true,
    });
}

fn push_recovery_skipped_entry(session: &mut PersistedSession, skipped: SkippedRecoveredBuffer) {
    if PERSISTED_SESSION_RECOVERY_SKIPPED_MAX == 0 {
        session.recovery_skipped.clear();
        return;
    }
    if session.recovery_skipped.len() >= PERSISTED_SESSION_RECOVERY_SKIPPED_MAX {
        let overflow = session.recovery_skipped.len() + 1 - PERSISTED_SESSION_RECOVERY_SKIPPED_MAX;
        session.recovery_skipped.drain(0..overflow);
    }
    session.recovery_skipped.push(skipped);
}

fn trim_volatile_session_text(session: &mut PersistedSession) -> bool {
    let mut changed = false;
    changed |= truncate_string_chars(
        &mut session.project_search_query,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= truncate_string_chars(
        &mut session.project_search_include,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= truncate_string_chars(
        &mut session.project_search_exclude,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= clear_vec(&mut session.project_search_recent);
    changed |= truncate_string_chars(
        &mut session.buffer_find_query,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= truncate_string_chars(
        &mut session.buffer_find_replacement,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= clear_vec(&mut session.buffer_find_query_history);
    changed |= clear_vec(&mut session.buffer_find_replacement_history);
    changed |= truncate_string_chars(
        &mut session.source_control_query,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= truncate_string_chars(
        &mut session.source_control_commit_message,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= clear_vec(&mut session.source_control_commit_history);
    changed |= truncate_string_chars(
        &mut session.source_control_stash_message,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= truncate_string_chars(
        &mut session.source_control_history_query,
        PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
    );
    changed |= clear_vec(&mut session.quick_open_query_memory);
    changed |= clear_vec(&mut session.workspace_symbol_query_memory);
    changed |= clear_vec(&mut session.command_query_memory);
    for terminal in &mut session.terminal_sessions {
        changed |= truncate_option_string_chars(
            &mut terminal.process_label,
            PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
        );
        changed |= truncate_option_string_chars(
            &mut terminal.window_title,
            PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS,
        );
    }
    changed
}

fn clear_volatile_session_state(session: &mut PersistedSession) -> bool {
    let mut changed = false;
    changed |= clear_vec(&mut session.view_states);
    changed |= clear_vec(&mut session.pane_view_states);
    changed |= clear_vec(&mut session.history_states);
    changed |= clear_vec(&mut session.recovery_view_states);
    changed |= clear_vec(&mut session.recovery_history_states);
    changed |= clear_vec(&mut session.fold_states);
    changed |= clear_vec(&mut session.explorer_expanded);
    changed |= clear_option(&mut session.explorer_revealed_path);
    changed |= clear_vec(&mut session.quick_open_recent_files);
    changed |= clear_vec(&mut session.command_recent);
    changed |= clear_vec(&mut session.navigation_back);
    changed |= clear_vec(&mut session.navigation_forward);
    changed |= clear_vec(&mut session.closed_files);
    changed |= clear_vec(&mut session.recovery_skipped);
    changed
}

fn truncate_option_string_chars(value: &mut Option<String>, max_chars: usize) -> bool {
    value
        .as_mut()
        .is_some_and(|value| truncate_string_chars(value, max_chars))
}

fn truncate_string_chars(value: &mut String, max_chars: usize) -> bool {
    if max_chars == 0 {
        return clear_string(value);
    }
    let Some((byte_index, _)) = value.char_indices().nth(max_chars) else {
        return false;
    };
    value.truncate(byte_index);
    true
}

fn clear_string(value: &mut String) -> bool {
    if value.is_empty() {
        return false;
    }
    value.clear();
    true
}

fn clear_vec<T>(values: &mut Vec<T>) -> bool {
    if values.is_empty() {
        return false;
    }
    values.clear();
    true
}

fn clear_option<T>(value: &mut Option<T>) -> bool {
    value.take().is_some()
}

#[derive(Debug)]
struct SessionPathNormalizer {
    workspace_root: PathBuf,
    root_components: Vec<String>,
}

impl SessionPathNormalizer {
    fn new(workspace_root: &Path) -> Self {
        let workspace_root = lexical_normalize_path(workspace_root);
        let root_components = comparable_path_components(&workspace_root);
        Self {
            workspace_root,
            root_components,
        }
    }

    fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    fn normalize_paths(&self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        let mut normalized: Vec<PathBuf> = Vec::with_capacity(paths.len());
        let mut seen = HashSet::with_capacity(paths.len());
        for path in paths {
            let Some(path) = self.normalize_path(path) else {
                continue;
            };
            if seen.insert(comparable_path_components(&path)) {
                normalized.push(path);
            }
        }
        normalized
    }

    fn normalize_option(&self, path: Option<PathBuf>) -> Option<PathBuf> {
        path.and_then(|path| self.normalize_path(path))
    }

    fn normalize_terminal_cwd_option(&self, path: Option<PathBuf>) -> Option<PathBuf> {
        path.and_then(|path| self.normalize_terminal_cwd(path))
    }

    fn normalize_path(&self, path: PathBuf) -> Option<PathBuf> {
        if path.as_os_str().is_empty() {
            return None;
        }

        let raw_path_has_workspace_prefix = self.path_has_workspace_prefix(&path);
        let candidate =
            self.workspace_descendant_candidate_for_raw_path(&path, raw_path_has_workspace_prefix);
        if !self.restore_path_stays_within_workspace_root_lexically(&candidate) {
            return None;
        }

        if self.workspace_root.as_os_str().is_empty() {
            let path = lexical_normalize_path(&path);
            return (!path.as_os_str().is_empty()).then_some(path);
        }

        let path = self.lexical_normalize_workspace_candidate(&path, raw_path_has_workspace_prefix);
        if path.as_os_str().is_empty() {
            return None;
        }
        (self.path_has_workspace_prefix(&path)
            && !trusted_workspace_paths_match(self.workspace_root(), &path))
        .then_some(path)
    }

    fn normalize_terminal_cwd(&self, path: PathBuf) -> Option<PathBuf> {
        if path.as_os_str().is_empty() {
            return None;
        }

        match self.workspace_prefixed_relative_terminal_cwd_candidate(&path) {
            TerminalCwdWorkspacePrefixCandidate::Candidate(candidate) => {
                return self.normalize_terminal_cwd_candidate(candidate);
            }
            TerminalCwdWorkspacePrefixCandidate::Unsafe => return None,
            TerminalCwdWorkspacePrefixCandidate::NotPrefixed => {}
        }

        let raw_path_has_workspace_prefix = self.path_has_workspace_prefix(&path);
        let candidate =
            self.workspace_descendant_candidate_for_raw_path(&path, raw_path_has_workspace_prefix);
        if !self.restore_path_stays_within_workspace_root_lexically(&candidate) {
            return None;
        }

        if self.workspace_root.as_os_str().is_empty() {
            return Some(current_dir_path_if_empty(lexical_normalize_path(&path)));
        }

        self.normalize_terminal_cwd_candidate(
            self.lexical_normalize_workspace_candidate(&path, raw_path_has_workspace_prefix),
        )
    }

    fn normalize_terminal_cwd_candidate(&self, path: PathBuf) -> Option<PathBuf> {
        if !self.restore_path_stays_within_workspace_root_lexically(&path) {
            return None;
        }

        let path = lexical_normalize_path(&path);
        if self.workspace_root.as_os_str().is_empty() {
            return Some(current_dir_path_if_empty(path));
        }

        (!path.as_os_str().is_empty() && self.path_has_workspace_prefix(&path)).then_some(path)
    }

    fn workspace_prefixed_relative_terminal_cwd_candidate(
        &self,
        path: &Path,
    ) -> TerminalCwdWorkspacePrefixCandidate {
        if path.is_absolute() || path.has_root() || path_has_platform_prefix(path) {
            return TerminalCwdWorkspacePrefixCandidate::NotPrefixed;
        }
        let Some(workspace_name) = self.workspace_root.file_name() else {
            return TerminalCwdWorkspacePrefixCandidate::NotPrefixed;
        };
        let mut components = path.components();
        let Some(Component::Normal(first)) = components.next() else {
            return TerminalCwdWorkspacePrefixCandidate::NotPrefixed;
        };
        if normalize_path_component(first) != normalize_path_component(workspace_name) {
            return TerminalCwdWorkspacePrefixCandidate::NotPrefixed;
        }

        let mut stripped = PathBuf::new();
        for component in components {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if matches!(
                        stripped.components().next_back(),
                        Some(Component::Normal(_))
                    ) {
                        stripped.pop();
                    } else {
                        return TerminalCwdWorkspacePrefixCandidate::Unsafe;
                    }
                }
                Component::Normal(part) => stripped.push(part),
                Component::Prefix(_) | Component::RootDir => {
                    return TerminalCwdWorkspacePrefixCandidate::Unsafe;
                }
            }
        }

        TerminalCwdWorkspacePrefixCandidate::Candidate(self.workspace_root.join(stripped))
    }

    fn workspace_descendant_candidate_for_raw_path(
        &self,
        path: &Path,
        path_has_workspace_prefix: bool,
    ) -> PathBuf {
        if path.is_absolute()
            || path.has_root()
            || path_has_platform_prefix(path)
            || path_has_workspace_prefix
        {
            path.to_path_buf()
        } else {
            self.workspace_root.join(path)
        }
    }

    fn restore_path_stays_within_workspace_root_lexically(&self, path: &Path) -> bool {
        if self.workspace_root.as_os_str().is_empty() {
            return relative_session_restore_path_stays_inside_current_dir(path);
        }
        workspace_path_stays_within_root_lexically(&self.workspace_root, path)
    }

    fn lexical_normalize_workspace_candidate(
        &self,
        path: &Path,
        path_has_workspace_prefix: bool,
    ) -> PathBuf {
        let normalized_path = lexical_normalize_path(path);
        if normalized_path.is_absolute()
            || normalized_path.has_root()
            || path_has_platform_prefix(&normalized_path)
            || path_has_workspace_prefix
        {
            normalized_path
        } else {
            lexical_normalize_path(&self.workspace_root.join(normalized_path))
        }
    }

    fn path_has_workspace_prefix(&self, path: &Path) -> bool {
        path_has_workspace_prefix_with_components(&self.root_components, path)
    }
}

enum TerminalCwdWorkspacePrefixCandidate {
    NotPrefixed,
    Candidate(PathBuf),
    Unsafe,
}

fn relative_session_restore_path_stays_inside_current_dir(path: &Path) -> bool {
    let mut depth = 0usize;
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(_) => depth += 1,
            Component::ParentDir if depth == 0 => return false,
            Component::ParentDir => depth -= 1,
            Component::Prefix(_) | Component::RootDir => return false,
        }
    }
    true
}

fn path_has_platform_prefix(path: &Path) -> bool {
    matches!(path.components().next(), Some(Component::Prefix(_)))
}

fn normalize_buffer_view_state_paths(
    normalizer: &SessionPathNormalizer,
    states: &mut Vec<crate::persistence::BufferViewState>,
) {
    states.retain_mut(|state| {
        let Some(path) = normalizer.normalize_path(mem::take(&mut state.path)) else {
            return false;
        };
        state.path = path;
        true
    });
}

fn normalize_pane_view_state_paths(
    normalizer: &SessionPathNormalizer,
    states: &mut Vec<crate::persistence::PaneBufferViewState>,
) {
    states.retain_mut(|state| {
        let Some(path) = normalizer.normalize_path(mem::take(&mut state.path)) else {
            return false;
        };
        state.path = path;
        true
    });
}

fn normalize_buffer_history_state_paths(
    normalizer: &SessionPathNormalizer,
    states: &mut Vec<crate::persistence::BufferHistoryState>,
) {
    states.retain_mut(|state| {
        let Some(path) = normalizer.normalize_path(mem::take(&mut state.path)) else {
            return false;
        };
        state.path = path;
        true
    });
}

fn normalize_buffer_fold_state_paths(
    normalizer: &SessionPathNormalizer,
    states: &mut Vec<crate::persistence::BufferFoldState>,
) {
    states.retain_mut(|state| {
        let Some(path) = normalizer.normalize_path(mem::take(&mut state.path)) else {
            return false;
        };
        state.path = path;
        true
    });
}

fn normalize_quick_open_query_memory_paths(
    normalizer: &SessionPathNormalizer,
    entries: &mut Vec<crate::quick_open::QuickOpenQueryMemoryEntry>,
) {
    entries.retain_mut(|entry| {
        let Some(path) = normalizer.normalize_path(mem::take(&mut entry.path)) else {
            return false;
        };
        entry.path = path;
        true
    });
}

fn normalize_workspace_symbol_query_memory_paths(
    normalizer: &SessionPathNormalizer,
    entries: &mut Vec<crate::lsp_workspace_symbol_ranking::WorkspaceSymbolQueryMemoryEntry>,
) {
    entries.retain_mut(|entry| {
        let Some(path) = normalizer.normalize_path(mem::take(&mut entry.path)) else {
            return false;
        };
        entry.path = path;
        true
    });
    *entries = crate::lsp_workspace_symbol_ranking::normalize_workspace_symbol_query_memory(
        mem::take(entries),
        normalizer.workspace_root(),
        crate::lsp_workspace_symbol_ranking::MAX_WORKSPACE_SYMBOL_QUERY_MEMORY,
    )
    .into_iter()
    .collect();
}

fn normalize_navigation_location_paths(
    normalizer: &SessionPathNormalizer,
    entries: &mut Vec<crate::persistence::PersistedNavigationLocation>,
) {
    entries.retain_mut(|entry| {
        let Some(path) = normalizer.normalize_path(mem::take(&mut entry.path)) else {
            return false;
        };
        entry.path = path;
        true
    });
}

fn normalize_terminal_session_cwd_paths(
    normalizer: &SessionPathNormalizer,
    sessions: &mut [crate::persistence::PersistedTerminalSession],
) {
    for session in sessions {
        session.cwd = normalizer.normalize_terminal_cwd_option(session.cwd.take());
    }
}

fn normalize_closed_file_entry_paths(
    normalizer: &SessionPathNormalizer,
    entries: &mut Vec<crate::persistence::PersistedClosedFileEntry>,
) {
    entries.retain_mut(|entry| {
        let Some(path) = normalizer.normalize_path(mem::take(&mut entry.path)) else {
            return false;
        };
        entry.path = path;
        true
    });
}

fn lexical_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut has_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => {
                has_root = true;
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                );
                if can_pop {
                    normalized.pop();
                } else if !has_root {
                    normalized.push("..");
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn current_dir_path_if_empty(path: PathBuf) -> PathBuf {
    if path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        path
    }
}

fn path_has_workspace_prefix_with_components(root_components: &[String], path: &Path) -> bool {
    if root_components.is_empty() {
        return path
            .components()
            .next()
            .map(comparable_path_component)
            .is_some_and(|component| component.starts_with("normal:"));
    }
    let mut path_components = path.components().map(comparable_path_component);
    root_components
        .iter()
        .all(|root_component| path_components.next().as_ref() == Some(root_component))
}

fn comparable_path_components(path: &Path) -> Vec<String> {
    path.components().map(comparable_path_component).collect()
}

fn comparable_path_component(component: Component<'_>) -> String {
    match component {
        Component::Prefix(prefix) => {
            format!("prefix:{}", normalize_path_component(prefix.as_os_str()))
        }
        Component::RootDir => "root:".to_owned(),
        Component::CurDir => "cur:".to_owned(),
        Component::ParentDir => "parent:".to_owned(),
        Component::Normal(component) => {
            format!("normal:{}", normalize_path_component(component))
        }
    }
}

fn normalize_path_component(component: &OsStr) -> String {
    let component = component.to_string_lossy();
    #[cfg(windows)]
    {
        component.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        component.into_owned()
    }
}

fn read_existing_session_bytes(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match read_file_bytes_with_limit(path, PERSISTED_SESSION_MAX_BYTES) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) if error.kind() == ErrorKind::InvalidData => {
            quarantine_corrupt_session_file(path)?;
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

async fn read_existing_session_bytes_async(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match read_file_bytes_with_limit_async(path, PERSISTED_SESSION_MAX_BYTES).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) if error.kind() == ErrorKind::InvalidData => {
            quarantine_corrupt_session_file(path)?;
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

fn write_session_snapshot(workspace_root: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let dir = session_snapshots_dir(workspace_root);
    fs::create_dir_all(&dir)?;
    atomic_write(&unique_session_snapshot_path(&dir), bytes)?;
    prune_session_snapshots(&dir)
}

fn write_session_snapshot_best_effort(workspace_root: &Path, bytes: &[u8]) {
    let _ = write_session_snapshot(workspace_root, bytes);
}

async fn write_session_snapshot_async(workspace_root: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let dir = session_snapshots_dir(workspace_root);
    tokio::fs::create_dir_all(&dir).await?;
    atomic_write_async(&unique_session_snapshot_path(&dir), bytes).await?;
    prune_session_snapshots_async(&dir).await
}

async fn write_session_snapshot_best_effort_async(workspace_root: &Path, bytes: &[u8]) {
    let _ = write_session_snapshot_async(workspace_root, bytes).await;
}

fn unique_session_snapshot_path(dir: &Path) -> PathBuf {
    dir.join(format!(
        "session.{}.{}.{:016}.json",
        session_snapshot_unique_id(),
        std::process::id(),
        SESSION_SNAPSHOT_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn session_snapshot_unique_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn prune_session_snapshots(dir: &Path) -> anyhow::Result<()> {
    let mut snapshots = session_snapshot_files(dir)?;
    sort_session_snapshots_oldest_modified_first(&mut snapshots);
    let overflow = snapshots.len().saturating_sub(MAX_SESSION_SNAPSHOTS);
    for path in snapshots.into_iter().take(overflow) {
        fs::remove_file(path)?;
    }
    Ok(())
}

async fn prune_session_snapshots_async(dir: &Path) -> anyhow::Result<()> {
    let mut snapshots = session_snapshot_files_async(dir).await?;
    sort_session_snapshots_oldest_modified_first(&mut snapshots);
    let overflow = snapshots.len().saturating_sub(MAX_SESSION_SNAPSHOTS);
    for path in snapshots.into_iter().take(overflow) {
        tokio::fs::remove_file(path).await?;
    }
    Ok(())
}

/// Orders snapshots oldest-modified first so pruning from the front evicts
/// the oldest writes and keeps the newest. Snapshot names embed wall-clock
/// nanos, so after a system-clock rollback freshly written snapshots sort
/// OLDEST by name and name-order pruning would freeze the safety net
/// exactly when it is needed; the on-disk modified time tracks real write
/// order instead. Entries whose modified time cannot be read are treated
/// as oldest (pruned first, keeping their relative name order); entries
/// with readable times keep name order on ties (the stable-sort fallback).
fn sort_session_snapshots_oldest_modified_first(snapshots: &mut [PathBuf]) {
    snapshots.sort_by_key(|path| snapshot_modified_time(path).unwrap_or(UNIX_EPOCH));
}

fn snapshot_modified_time(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
}

fn session_snapshot_files(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if snapshot_dir_unavailable(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut snapshots = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        if !entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
            continue;
        }
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("session.") && name.ends_with(".json"))
        {
            push_session_snapshot_candidate(&mut snapshots, path);
        }
    }
    trim_session_snapshot_candidates(&mut snapshots);
    sort_session_snapshot_paths(&mut snapshots);
    Ok(snapshots)
}

async fn session_snapshot_files_async(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if snapshot_dir_unavailable(&error) => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut snapshots = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("session.") && name.ends_with(".json"))
        {
            push_session_snapshot_candidate(&mut snapshots, path);
        }
    }
    trim_session_snapshot_candidates(&mut snapshots);
    sort_session_snapshot_paths(&mut snapshots);
    Ok(snapshots)
}

fn snapshot_dir_unavailable(error: &std::io::Error) -> bool {
    matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory)
}

fn push_session_snapshot_candidate(snapshots: &mut Vec<PathBuf>, path: PathBuf) {
    snapshots.push(path);
    if snapshots.len() >= SESSION_SNAPSHOT_SCAN_TRIM_AT {
        trim_session_snapshot_candidates(snapshots);
    }
}

fn trim_session_snapshot_candidates(snapshots: &mut Vec<PathBuf>) {
    let overflow = snapshots.len().saturating_sub(SESSION_SNAPSHOT_SCAN_LIMIT);
    if overflow == 0 {
        return;
    }
    sort_session_snapshot_paths(snapshots);
    snapshots.drain(0..overflow);
}

fn sort_session_snapshot_paths(snapshots: &mut [PathBuf]) {
    snapshots.sort();
    snapshots.sort_by_cached_key(|path| session_snapshot_sort_key(path));
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionSnapshotSortKey {
    Parsed {
        unique: u128,
        process_id: u32,
        counter: u64,
    },
    Unparsed(String),
}

impl Ord for SessionSnapshotSortKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (
                Self::Parsed {
                    unique,
                    process_id,
                    counter,
                },
                Self::Parsed {
                    unique: other_unique,
                    process_id: other_process_id,
                    counter: other_counter,
                },
            ) => {
                (unique, process_id, counter).cmp(&(other_unique, other_process_id, other_counter))
            }
            (Self::Parsed { .. }, Self::Unparsed(_)) => std::cmp::Ordering::Greater,
            (Self::Unparsed(_), Self::Parsed { .. }) => std::cmp::Ordering::Less,
            (Self::Unparsed(left), Self::Unparsed(right)) => left.cmp(right),
        }
    }
}

impl PartialOrd for SessionSnapshotSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn session_snapshot_sort_key(path: &Path) -> SessionSnapshotSortKey {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let Some(stem) = file_name
        .strip_prefix("session.")
        .and_then(|name| name.strip_suffix(".json"))
    else {
        return SessionSnapshotSortKey::Unparsed(file_name.to_owned());
    };
    let mut parts = stem.split('.');
    let (Some(unique), Some(process_id), Some(counter), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return SessionSnapshotSortKey::Unparsed(file_name.to_owned());
    };
    let (Ok(unique), Ok(process_id), Ok(counter)) =
        (unique.parse(), process_id.parse(), counter.parse())
    else {
        return SessionSnapshotSortKey::Unparsed(file_name.to_owned());
    };
    SessionSnapshotSortKey::Parsed {
        unique,
        process_id,
        counter,
    }
}

#[cfg(test)]
mod tests;

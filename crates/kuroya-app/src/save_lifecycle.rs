use kuroya_core::BufferId;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use crate::{path_display::display_path_label_cow, ui_text::truncate_middle};

pub(crate) use crate::save_guard_reasons::protected_preview_save_block_reason_for_buffer;
#[cfg(test)]
pub(crate) use crate::save_guards::SaveAllPlan;
#[cfg(test)]
pub(crate) use crate::save_guards::protected_preview_save_block_reason;
#[cfg(test)]
pub(crate) use crate::save_guards::save_needs_external_change_confirmation;
pub(crate) use crate::save_guards::{
    SaveAllBlocker, autosave_buffer_ids, buffer_display_name, dirty_buffer_ids,
    dirty_buffer_save_block_reason, plan_save_all_dirty_buffers,
    workspace_switch_save_block_reason,
};
#[cfg(test)]
pub(crate) use crate::save_lifecycle::lsp_sync::LspSaveSyncPlan;
pub(crate) use crate::save_lifecycle::lsp_sync::{apply_save_completion, plan_lsp_save_sync};

mod lsp_sync;

const SAVE_COMPLETION_STATUS_MAX_CHARS: usize = 180;
const SESSION_SAVE_MAX_RETRIES: u8 = 3;

static SESSION_SAVE_ROTATION_ORDER: LazyLock<Mutex<Vec<PathBuf>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));
static PENDING_SESSION_SAVE_FINGERPRINTS: LazyLock<Mutex<HashMap<PathBuf, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SESSION_SAVE_FAILURE_COUNTS: LazyLock<Mutex<HashMap<PathBuf, u8>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveRequest {
    Spawn,
    Queued,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FinishedSaveRequest {
    Current { queued_path: Option<PathBuf> },
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionSaveRequest {
    Spawn,
    Queued,
}

pub(crate) fn reserve_save_request(
    id: BufferId,
    path: &Path,
    in_flight_saves: &mut HashSet<BufferId>,
    queued_save_paths: &mut HashMap<BufferId, PathBuf>,
) -> SaveRequest {
    if in_flight_saves.insert(id) {
        queued_save_paths.remove(&id);
        SaveRequest::Spawn
    } else {
        if let Some(queued_path) = queued_save_paths.get_mut(&id) {
            if queued_path.as_path() != path {
                *queued_path = path.to_path_buf();
            }
        } else {
            queued_save_paths.insert(id, path.to_path_buf());
        }
        SaveRequest::Queued
    }
}

pub(crate) fn has_active_save_work<T>(
    id: BufferId,
    in_flight_saves: &HashSet<BufferId>,
    queued_save_paths: &HashMap<BufferId, PathBuf>,
    pending_format_on_save: &HashMap<BufferId, T>,
) -> bool {
    in_flight_saves.contains(&id)
        || queued_save_paths.contains_key(&id)
        || pending_format_on_save.contains_key(&id)
}

pub(crate) fn finish_save_request(
    id: BufferId,
    in_flight_saves: &mut HashSet<BufferId>,
    queued_save_paths: &mut HashMap<BufferId, PathBuf>,
) -> Option<PathBuf> {
    match finish_current_save_request(id, in_flight_saves, queued_save_paths) {
        FinishedSaveRequest::Current { queued_path } => queued_path,
        FinishedSaveRequest::Stale => None,
    }
}

pub(crate) fn finish_current_save_request(
    id: BufferId,
    in_flight_saves: &mut HashSet<BufferId>,
    queued_save_paths: &mut HashMap<BufferId, PathBuf>,
) -> FinishedSaveRequest {
    if !in_flight_saves.remove(&id) {
        return FinishedSaveRequest::Stale;
    }

    FinishedSaveRequest::Current {
        queued_path: queued_save_paths.remove(&id),
    }
}

pub(crate) fn reserve_session_save<T>(
    root: &Path,
    session: T,
    in_flight: &mut Option<PathBuf>,
    queued: &mut HashMap<PathBuf, T>,
    order: &mut Vec<PathBuf>,
) -> SessionSaveRequest {
    if in_flight.is_some() {
        if let Some(queued_session) = queued.get_mut(root) {
            *queued_session = session;
        } else {
            queued.insert(root.to_path_buf(), session);
        }
        enqueue_session_save_order(order, root);
        SessionSaveRequest::Queued
    } else {
        queued.remove(root);
        order.retain(|entry| entry.as_path() != root);
        *in_flight = Some(root.to_path_buf());
        SessionSaveRequest::Spawn
    }
}

pub(crate) fn finish_session_save<T>(
    root: &Path,
    in_flight: &mut Option<PathBuf>,
    queued: &mut HashMap<PathBuf, T>,
    order: &mut Vec<PathBuf>,
) -> Option<(PathBuf, T)> {
    if in_flight.as_deref() != Some(root) {
        return None;
    }

    *in_flight = None;
    let next_root = next_session_save_dispatch_root(root, order, queued)?;
    let next_session = queued.remove(&next_root)?;
    *in_flight = Some(next_root.clone());
    Some((next_root, next_session))
}

pub(crate) fn should_skip_session_persistence(
    current_fingerprint: Option<u64>,
    attempted_fingerprint: Option<u64>,
    saved_fingerprint: Option<u64>,
    save_in_flight: bool,
    saves_queued: bool,
) -> bool {
    let (Some(current), Some(attempted), Some(saved)) = (
        current_fingerprint,
        attempted_fingerprint,
        saved_fingerprint,
    ) else {
        return false;
    };
    !save_in_flight && !saves_queued && current == attempted && attempted == saved
}

pub(crate) fn enqueue_session_save_order(order: &mut Vec<PathBuf>, root: &Path) {
    if !order.iter().any(|entry| entry.as_path() == root) {
        order.push(root.to_path_buf());
    }
}

fn next_session_save_dispatch_root<T>(
    finished_root: &Path,
    order: &mut Vec<PathBuf>,
    queued: &HashMap<PathBuf, T>,
) -> Option<PathBuf> {
    order.retain(|entry| queued.contains_key(entry.as_path()));
    let next_root = order
        .iter()
        .find(|entry| entry.as_path() != finished_root)
        .cloned()
        .or_else(|| {
            order
                .iter()
                .find(|entry| entry.as_path() == finished_root)
                .cloned()
                .or_else(|| {
                    queued
                        .contains_key(finished_root)
                        .then(|| finished_root.to_path_buf())
                })
        })?;
    order.retain(|entry| entry.as_path() != next_root.as_path());
    Some(next_root)
}

pub(crate) fn with_session_save_rotation_order<R>(
    dispatch: impl FnOnce(&mut Vec<PathBuf>) -> R,
) -> R {
    let mut order = SESSION_SAVE_ROTATION_ORDER
        .lock()
        .expect("session save rotation order");
    dispatch(&mut order)
}

pub(crate) fn stage_session_save_fingerprint(root: &Path, fingerprint: u64) {
    PENDING_SESSION_SAVE_FINGERPRINTS
        .lock()
        .expect("pending session save fingerprints")
        .insert(root.to_path_buf(), fingerprint);
}

pub(crate) fn note_session_save_succeeded(root: &Path) -> Option<u64> {
    SESSION_SAVE_FAILURE_COUNTS
        .lock()
        .expect("session save failure counts")
        .remove(root);
    PENDING_SESSION_SAVE_FINGERPRINTS
        .lock()
        .expect("pending session save fingerprints")
        .remove(root)
}

pub(crate) fn note_session_save_failed(root: &Path) -> bool {
    register_session_save_failure(
        &mut SESSION_SAVE_FAILURE_COUNTS
            .lock()
            .expect("session save failure counts"),
        root,
        SESSION_SAVE_MAX_RETRIES,
    )
}

fn register_session_save_failure(
    failures: &mut HashMap<PathBuf, u8>,
    root: &Path,
    max_retries: u8,
) -> bool {
    let attempts = failures.get(root).copied().unwrap_or_default();
    if attempts >= max_retries {
        failures.remove(root);
        return false;
    }
    failures.insert(root.to_path_buf(), attempts + 1);
    true
}

pub(crate) fn save_completion_status(path: &Path, still_dirty: bool) -> String {
    let path_label = display_path_label_cow(path);
    let status = if still_dirty {
        format!("Saved {}; newer edits remain unsaved", path_label.as_ref())
    } else {
        format!("Saved {}", path_label.as_ref())
    };
    truncate_middle(&status, SAVE_COMPLETION_STATUS_MAX_CHARS)
}

#[cfg(test)]
mod tests {
    use super::{
        FinishedSaveRequest, SAVE_COMPLETION_STATUS_MAX_CHARS, SaveRequest, SessionSaveRequest,
        finish_current_save_request, finish_session_save, has_active_save_work,
        register_session_save_failure, reserve_save_request, reserve_session_save,
        save_completion_status,
    };
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };

    #[test]
    fn reserve_save_request_drops_orphaned_queue_before_spawning() {
        let stale_path = PathBuf::from("workspace/src/stale.rs");
        let fresh_path = PathBuf::from("workspace/src/fresh.rs");
        let mut in_flight = HashSet::new();
        let mut queued = HashMap::from([(7, stale_path)]);

        assert_eq!(
            reserve_save_request(7, &fresh_path, &mut in_flight, &mut queued),
            SaveRequest::Spawn
        );

        assert!(in_flight.contains(&7));
        assert!(!queued.contains_key(&7));
    }

    #[test]
    fn reserve_session_save_drops_orphaned_session_for_spawned_root() {
        let root = PathBuf::from("workspace");
        let mut in_flight = None;
        let mut order = Vec::new();
        let mut queued = HashMap::from([(root.clone(), "stale")]);

        assert_eq!(
            reserve_session_save(&root, "fresh", &mut in_flight, &mut queued, &mut order),
            SessionSaveRequest::Spawn
        );

        assert_eq!(in_flight, Some(root.clone()));
        assert!(!queued.contains_key(&root));
    }

    #[test]
    fn stale_save_completion_keeps_queued_path_for_current_owner() {
        let queued_path = PathBuf::from("workspace/src/queued.rs");
        let mut in_flight = HashSet::new();
        let mut queued = HashMap::from([(7, queued_path.clone())]);

        assert_eq!(
            finish_current_save_request(7, &mut in_flight, &mut queued),
            FinishedSaveRequest::Stale
        );

        assert_eq!(queued.get(&7), Some(&queued_path));
    }

    #[test]
    fn repeated_queued_save_request_keeps_single_latest_path() {
        let first = PathBuf::from("workspace/src/main.rs");
        let queued_path = PathBuf::from("workspace/src/queued.rs");
        let mut in_flight = HashSet::new();
        let mut queued = HashMap::new();

        assert_eq!(
            reserve_save_request(7, &first, &mut in_flight, &mut queued),
            SaveRequest::Spawn
        );
        assert_eq!(
            reserve_save_request(7, &queued_path, &mut in_flight, &mut queued),
            SaveRequest::Queued
        );
        assert_eq!(
            reserve_save_request(7, &queued_path, &mut in_flight, &mut queued),
            SaveRequest::Queued
        );

        assert_eq!(queued.len(), 1);
        assert_eq!(queued.get(&7), Some(&queued_path));
    }

    #[test]
    fn stale_session_save_completion_preserves_in_flight_and_queue() {
        let in_flight_root = PathBuf::from("workspace-a");
        let stale_root = PathBuf::from("workspace-stale");
        let queued_root = PathBuf::from("workspace-b");
        let mut in_flight = Some(in_flight_root.clone());
        let mut order = vec![queued_root.clone()];
        let mut queued = HashMap::from([(queued_root.clone(), "queued")]);

        assert_eq!(
            finish_session_save(&stale_root, &mut in_flight, &mut queued, &mut order),
            None
        );

        assert_eq!(in_flight, Some(in_flight_root));
        assert_eq!(queued.get(&queued_root), Some(&"queued"));
    }

    #[test]
    fn session_save_failures_retry_until_cap_then_stop_until_success_resets() {
        let root = PathBuf::from("workspace");
        let mut failures = HashMap::new();

        assert!(register_session_save_failure(&mut failures, &root, 3));
        assert!(register_session_save_failure(&mut failures, &root, 3));
        assert!(register_session_save_failure(&mut failures, &root, 3));
        assert!(!register_session_save_failure(&mut failures, &root, 3));

        assert!(failures.is_empty());
        assert!(register_session_save_failure(&mut failures, &root, 3));
    }

    #[test]
    fn active_save_work_includes_pending_format_without_disk_request() {
        let in_flight = HashSet::new();
        let queued = HashMap::new();
        let pending_format = HashMap::from([(7, ())]);

        assert!(has_active_save_work(
            7,
            &in_flight,
            &queued,
            &pending_format
        ));
    }

    #[test]
    fn save_completion_status_is_single_line_and_bounded() {
        let path = PathBuf::from("workspace/src").join(format!(
            "bad\n{}\u{202e}tail.rs",
            "very-long-name-".repeat(64)
        ));

        let status = save_completion_status(&path, true);

        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.chars().count() <= SAVE_COMPLETION_STATUS_MAX_CHARS);
    }
}

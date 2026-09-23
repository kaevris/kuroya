use crate::{
    KuroyaApp,
    path_display::{compact_path, sanitized_display_label_cow},
    save_lifecycle::{dirty_buffer_ids, has_active_save_work, workspace_switch_save_block_reason},
    transient_state::PendingWorkspaceSwitch,
    workspace_lifecycle::{
        already_in_workspace_status, workspace_path_not_folder_status,
        workspace_switch_blocked_by_exit_status,
    },
    workspace_state::paths_match_lexically,
};
use kuroya_core::{BufferId, TextBuffer};
use std::{
    borrow::Cow,
    collections::HashSet,
    path::{Path, PathBuf},
};

mod exit;

const WORKSPACE_GUARD_TEXT_MAX_CHARS: usize = 120;
const PENDING_GUARD_DIRTY_SET_THRESHOLD: usize = 8;

pub(crate) fn workspace_guard_display_path(path: &Path) -> String {
    workspace_guard_display_owned_text(compact_path(path), WORKSPACE_GUARD_TEXT_MAX_CHARS, ".")
}

pub(crate) fn workspace_guard_status_message(message: &str) -> String {
    workspace_guard_display_text(message, WORKSPACE_GUARD_TEXT_MAX_CHARS, "Save blocked")
}

fn workspace_guard_file_noun_and_verb(dirty_count: usize) -> (&'static str, &'static str) {
    let noun = if dirty_count == 1 { "file" } else { "files" };
    let verb = if dirty_count == 1 { "has" } else { "have" };
    (noun, verb)
}

pub(crate) fn workspace_guard_unsaved_changes_phrase(dirty_count: usize) -> String {
    let (noun, verb) = workspace_guard_file_noun_and_verb(dirty_count);
    format!("{dirty_count} {noun} {verb} unsaved changes")
}

fn workspace_guard_display_text(value: &str, max_chars: usize, fallback: &str) -> String {
    workspace_guard_display_text_cow(value, max_chars, fallback).into_owned()
}

fn workspace_guard_display_owned_text(value: String, max_chars: usize, fallback: &str) -> String {
    let owned_label = {
        let raw = value.as_str();
        match workspace_guard_display_text_cow(raw, max_chars, fallback) {
            Cow::Borrowed(label) => {
                let borrowed_original =
                    !raw.is_empty() && label.as_ptr() == raw.as_ptr() && label.len() == raw.len();
                if borrowed_original {
                    None
                } else {
                    Some(label.to_owned())
                }
            }
            Cow::Owned(label) => Some(label),
        }
    };

    match owned_label {
        Some(label) => label,
        None => value,
    }
}

fn workspace_guard_display_text_cow<'a>(
    value: &'a str,
    max_chars: usize,
    fallback: &str,
) -> Cow<'a, str> {
    sanitized_display_label_cow(value, max_chars, fallback)
}

impl KuroyaApp {
    pub(crate) fn clear_pending_workspace_switch_for_exit(&mut self) {
        self.pending_workspace_switch = None;
    }

    pub(crate) fn cancel_invalid_pending_workspace_switch(&mut self) -> bool {
        let invalid_target = match self.pending_workspace_switch.as_ref() {
            Some(PendingWorkspaceSwitch::Confirm { target })
            | Some(PendingWorkspaceSwitch::ConfirmHomeDirectory { target })
            | Some(PendingWorkspaceSwitch::Saving { target, .. })
                if !target.is_dir() =>
            {
                Some(target.clone())
            }
            _ => None,
        };
        let Some(target) = invalid_target else {
            return false;
        };

        self.pending_workspace_switch = None;
        self.status = workspace_path_not_folder_status(&target);
        true
    }

    pub(crate) fn start_pending_workspace_switch_save(&mut self, target: PathBuf) {
        if self.cancel_pending_workspace_switch_if_exit_is_routing() {
            return;
        }
        if self.cancel_workspace_switch_if_target_is_current_or_invalid(&target) {
            return;
        }

        let dirty = dirty_buffer_ids(&self.buffers);
        if dirty.is_empty() {
            self.open_workspace_now(target);
            return;
        }

        let changed_on_disk = self.observed_external_change_buffer_ids();
        if let Some(reason) = workspace_switch_save_block_reason(
            &dirty,
            &self.buffers,
            &changed_on_disk,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        ) {
            self.status = workspace_guard_status_message(&reason);
            self.pending_workspace_switch = Some(PendingWorkspaceSwitch::Confirm { target });
            return;
        }

        for &id in &dirty {
            self.spawn_save(id);
        }
        self.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving { target, ids: dirty });
        self.advance_pending_workspace_switch_after_save();
    }

    pub(crate) fn advance_pending_workspace_switch_after_save(&mut self) {
        if self.cancel_pending_workspace_switch_if_exit_is_routing()
            || self.cancel_invalid_pending_workspace_switch()
        {
            return;
        }

        let Some(mut pending) = self.pending_workspace_switch.take() else {
            return;
        };
        pending.prune_invalid_buffer_ids(|id| self.buffer(id).is_some());
        let (target, ids) = match pending {
            PendingWorkspaceSwitch::Saving { target, ids } => (target, ids),
            pending => {
                self.pending_workspace_switch = Some(pending);
                return;
            }
        };
        if ids.iter().any(|id| {
            has_active_save_work(
                *id,
                &self.in_flight_saves,
                &self.queued_save_paths,
                &self.pending_format_on_save,
            )
        }) {
            self.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving { target, ids });
            return;
        }

        let changed_on_disk =
            pending_guard_external_change_count(&ids, &self.observed_external_change_buffer_ids());
        if changed_on_disk > 0 {
            self.pending_workspace_switch = Some(PendingWorkspaceSwitch::Confirm { target });
            self.status = workspace_switch_paused_external_change_status(changed_on_disk);
            return;
        }

        let still_dirty = pending_guard_dirty_count(&ids, &self.buffers);
        if still_dirty == 0 {
            self.open_workspace_now(target);
        } else {
            self.pending_workspace_switch = Some(PendingWorkspaceSwitch::Confirm { target });
            self.status = workspace_switch_paused_status(still_dirty);
        }
    }

    pub(crate) fn pause_pending_workspace_switch_after_save_failure(&mut self, id: BufferId) {
        let should_pause = matches!(
            self.pending_workspace_switch.as_ref(),
            Some(PendingWorkspaceSwitch::Saving { ids, .. }) if ids.contains(&id)
        );
        if !should_pause {
            return;
        }

        if let Some(PendingWorkspaceSwitch::Saving { target, .. }) =
            self.pending_workspace_switch.take()
        {
            if self.exit_confirmed || self.pending_exit.is_some() {
                self.status = workspace_switch_blocked_by_exit_status();
                return;
            }
            if self.cancel_workspace_switch_if_target_is_current_or_invalid(&target) {
                return;
            }
            self.pending_workspace_switch = Some(PendingWorkspaceSwitch::Confirm { target });
        }
    }

    fn cancel_pending_workspace_switch_if_exit_is_routing(&mut self) -> bool {
        if !(self.exit_confirmed || self.pending_exit.is_some()) {
            return false;
        }

        self.pending_workspace_switch = None;
        self.status = workspace_switch_blocked_by_exit_status();
        true
    }

    fn cancel_workspace_switch_if_target_is_current_or_invalid(&mut self, target: &Path) -> bool {
        if paths_match_lexically(target, &self.workspace.root) {
            self.pending_workspace_switch = None;
            self.status = already_in_workspace_status(target);
            return true;
        }
        if !target.is_dir() {
            self.pending_workspace_switch = None;
            self.status = workspace_path_not_folder_status(target);
            return true;
        }

        false
    }
}

fn workspace_switch_paused_external_change_status(changed_on_disk: usize) -> String {
    let noun = if changed_on_disk == 1 {
        "file"
    } else {
        "files"
    };
    format!("Workspace switch paused; {changed_on_disk} {noun} changed on disk")
}

fn workspace_switch_paused_status(still_dirty: usize) -> String {
    let (noun, verb) = workspace_guard_file_noun_and_verb(still_dirty);
    format!("Workspace switch paused; {still_dirty} {noun} still {verb} unsaved changes")
}

fn pending_guard_external_change_count(
    ids: &[BufferId],
    changed_on_disk: &HashSet<BufferId>,
) -> usize {
    if ids.is_empty() || changed_on_disk.is_empty() {
        return 0;
    }

    let mut seen = HashSet::with_capacity(ids.len().min(PENDING_GUARD_DIRTY_SET_THRESHOLD));
    ids.iter()
        .copied()
        .filter(|id| changed_on_disk.contains(id) && seen.insert(*id))
        .count()
}

fn pending_guard_dirty_count(ids: &[BufferId], buffers: &[TextBuffer]) -> usize {
    if ids.is_empty() {
        return 0;
    }

    if ids.len() <= PENDING_GUARD_DIRTY_SET_THRESHOLD {
        return buffers
            .iter()
            .filter(|buffer| buffer.is_dirty() && ids.contains(&buffer.id()))
            .count();
    }

    let pending_ids = ids.iter().copied().collect::<HashSet<_>>();
    buffers
        .iter()
        .filter(|buffer| buffer.is_dirty() && pending_ids.contains(&buffer.id()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_state::{PendingFileReload, PendingFormatOnSave, QueuedFileReload},
        path_display::DISPLAY_PATH_LABEL_MAX_CHARS,
        source_control_runtime::source_control_app_for_test,
        transient_state::PendingExit,
    };
    use kuroya_core::{BufferId, TextBuffer};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn workspace_switch_waits_for_pending_format_on_save() {
        let root = temp_workspace("format-root");
        let target = temp_workspace("format-target");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&target).unwrap();
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, true);
        insert_dirty_pending_format_on_save(&mut app, 7, path);
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving {
            target: target.clone(),
            ids: vec![7],
        });

        app.advance_pending_workspace_switch_after_save();

        assert!(matches!(
            app.pending_workspace_switch,
            Some(PendingWorkspaceSwitch::Saving { target: ref actual, .. }) if actual == &target
        ));

        fs::remove_dir_all(app.workspace.root.clone()).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn workspace_switch_pauses_on_clean_external_change_after_save() {
        let root = temp_workspace("external-change-root");
        let target = temp_workspace("external-change-target");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&target).unwrap();
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path),
            "fn main() {}\n".to_owned(),
        ));
        app.external_change_buffers.insert(7);
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving {
            target: target.clone(),
            ids: vec![7],
        });

        app.advance_pending_workspace_switch_after_save();

        assert_eq!(app.workspace.root, root);
        assert!(matches!(
            app.pending_workspace_switch,
            Some(PendingWorkspaceSwitch::Confirm { target: ref actual }) if actual == &target
        ));
        assert_eq!(
            app.status,
            "Workspace switch paused; 1 file changed on disk"
        );

        fs::remove_dir_all(app.workspace.root.clone()).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn workspace_switch_pauses_on_pending_clean_reload_after_save() {
        let root = temp_workspace("pending-reload-after-save-root");
        let target = temp_workspace("pending-reload-after-save-target");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&target).unwrap();
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root.clone(), true);
        let buffer = TextBuffer::from_text(7, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        app.buffers.push(buffer);
        app.in_flight_reloads.insert(
            7,
            PendingFileReload {
                request_id: 1,
                path,
                version,
                force_dirty: false,
            },
        );
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving {
            target: target.clone(),
            ids: vec![7],
        });

        app.advance_pending_workspace_switch_after_save();

        assert_eq!(app.workspace.root, root);
        assert!(app.external_change_buffers.is_empty());
        assert!(matches!(
            app.pending_workspace_switch,
            Some(PendingWorkspaceSwitch::Confirm { target: ref actual }) if actual == &target
        ));
        assert_eq!(
            app.status,
            "Workspace switch paused; 1 file changed on disk"
        );

        fs::remove_dir_all(app.workspace.root.clone()).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn workspace_switch_save_blocks_pending_clean_reload() {
        let root = temp_workspace("pending-reload-root");
        let target = temp_workspace("pending-reload-target");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&target).unwrap();
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, true);
        insert_dirty_pending_clean_reload(&mut app, 7, path);

        app.start_pending_workspace_switch_save(target.clone());

        assert!(matches!(
            app.pending_workspace_switch,
            Some(PendingWorkspaceSwitch::Confirm { target: ref actual }) if actual == &target
        ));
        assert!(app.in_flight_saves.is_empty());
        assert!(app.status.contains("changed on disk"));
        assert!(app.status.contains("before switching"));

        fs::remove_dir_all(app.workspace.root.clone()).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn workspace_switch_save_blocks_queued_clean_reload() {
        let root = temp_workspace("queued-reload-root");
        let target = temp_workspace("queued-reload-target");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&target).unwrap();
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, true);
        insert_dirty_queued_clean_reload(&mut app, 7, path);

        app.start_pending_workspace_switch_save(target.clone());

        assert!(matches!(
            app.pending_workspace_switch,
            Some(PendingWorkspaceSwitch::Confirm { target: ref actual }) if actual == &target
        ));
        assert!(app.in_flight_saves.is_empty());
        assert!(app.status.contains("changed on disk"));
        assert!(app.status.contains("before switching"));

        fs::remove_dir_all(app.workspace.root.clone()).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn pending_workspace_switch_cleans_stale_target_before_routing() {
        let root = temp_workspace("stale-route-root");
        fs::create_dir_all(&root).unwrap();
        let stale_target = root.join(format!(
            "missing\n{}\u{202e}tail",
            "very-long-component-".repeat(16)
        ));
        let mut app = source_control_app_for_test(root.clone(), true);
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving {
            target: stale_target,
            ids: Vec::new(),
        });

        app.advance_pending_workspace_switch_after_save();

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert!(app.status.starts_with("Workspace path is not a folder: "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(
            app.status.chars().count()
                <= "Workspace path is not a folder: ".chars().count()
                    + DISPLAY_PATH_LABEL_MAX_CHARS
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_workspace_switch_save_failure_cleans_stale_target() {
        let root = temp_workspace("stale-save-failure-root");
        fs::create_dir_all(&root).unwrap();
        let stale_target = root.join(format!(
            "missing\n{}\u{202e}tail",
            "very-long-component-".repeat(16)
        ));
        let mut app = source_control_app_for_test(root.clone(), true);
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving {
            target: stale_target,
            ids: vec![7],
        });

        app.pause_pending_workspace_switch_after_save_failure(7);

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert!(app.status.starts_with("Workspace path is not a folder: "));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(
            app.status.chars().count()
                <= "Workspace path is not a folder: ".chars().count()
                    + DISPLAY_PATH_LABEL_MAX_CHARS
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pending_workspace_switch_does_not_route_while_exit_is_pending() {
        let root = temp_workspace("exit-route-root");
        let target = temp_workspace("exit-route-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        let mut app = source_control_app_for_test(root.clone(), true);
        app.pending_exit = Some(PendingExit::Confirm);
        app.pending_workspace_switch = Some(PendingWorkspaceSwitch::Saving {
            target: target.clone(),
            ids: Vec::new(),
        });

        app.advance_pending_workspace_switch_after_save();

        assert_eq!(app.workspace.root, root);
        assert!(app.pending_workspace_switch.is_none());
        assert_eq!(app.status, "Workspace switch canceled; exit is in progress");

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn workspace_guard_display_text_cow_borrows_clean_labels() {
        assert!(matches!(
            workspace_guard_display_text_cow("workspace.rs", WORKSPACE_GUARD_TEXT_MAX_CHARS, "."),
            Cow::Borrowed("workspace.rs")
        ));

        let unicode = "workspace-\u{03bb}.rs";
        match workspace_guard_display_text_cow(unicode, WORKSPACE_GUARD_TEXT_MAX_CHARS, ".") {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn workspace_guard_display_text_cow_owns_unsafe_bounded_and_fallback_labels() {
        let cases = [
            ("  workspace.rs  ", 32, "."),
            ("alpha\nbeta", 64, "."),
            ("alpha\u{202e}beta", 64, "."),
            ("abcdefghijklmnopqrstuvwxyz", 12, "."),
            ("\n\u{202e}\u{0007}", 32, "Save blocked"),
        ];

        for (value, max_chars, fallback) in cases {
            let label = workspace_guard_display_text_cow(value, max_chars, fallback);

            assert_eq!(
                label.as_ref(),
                workspace_guard_display_text(value, max_chars, fallback)
            );
            assert!(
                matches!(label, Cow::Owned(_)),
                "expected owned label for {value:?}"
            );
            assert!(label.chars().count() <= max_chars);
            assert!(!label.chars().any(char::is_control));
            assert!(!label.contains('\u{202e}'));
        }
    }

    #[test]
    fn workspace_guard_display_owned_text_matches_string_wrapper() {
        let cases = [
            ("workspace.rs", 32, "."),
            ("workspace-\u{03bb}.rs", 32, "."),
            ("  workspace.rs  ", 32, "."),
            ("alpha\nbeta", 64, "."),
            ("abcdefghijklmnopqrstuvwxyz", 12, "."),
            ("\n\u{202e}\u{0007}", 32, "Save blocked"),
        ];

        for (value, max_chars, fallback) in cases {
            assert_eq!(
                workspace_guard_display_owned_text(value.to_owned(), max_chars, fallback),
                workspace_guard_display_text(value, max_chars, fallback)
            );
        }
    }

    #[test]
    fn workspace_guard_display_labels_sanitize_and_bound_unsafe_text() {
        let path = Path::new("workspace").join(format!(
            "target\n{}\u{202e}tail",
            "very-long-component-".repeat(16)
        ));
        let path_label = workspace_guard_display_path(&path);
        let status = workspace_guard_status_message(&format!(
            "blocked\n{}\u{2066}tail",
            "reason-".repeat(32)
        ));

        assert!(!path_label.contains('\n'));
        assert!(!path_label.contains('\u{202e}'));
        assert!(path_label.contains("..."));
        assert!(path_label.chars().count() <= WORKSPACE_GUARD_TEXT_MAX_CHARS);
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{2066}'));
        assert!(status.contains("..."));
        assert!(status.chars().count() <= WORKSPACE_GUARD_TEXT_MAX_CHARS);
        assert_eq!(
            workspace_guard_status_message("\n\u{202e}\u{0007}"),
            "Save blocked"
        );
    }

    #[test]
    fn workspace_switch_paused_status_uses_file_count_labels() {
        assert_eq!(
            workspace_switch_paused_external_change_status(1),
            "Workspace switch paused; 1 file changed on disk"
        );
        assert_eq!(
            workspace_switch_paused_external_change_status(2),
            "Workspace switch paused; 2 files changed on disk"
        );
        assert_eq!(
            workspace_switch_paused_status(1),
            "Workspace switch paused; 1 file still has unsaved changes"
        );
        assert_eq!(
            workspace_switch_paused_status(2),
            "Workspace switch paused; 2 files still have unsaved changes"
        );
    }

    #[test]
    fn pending_guard_dirty_count_counts_matching_dirty_buffers_once() {
        let mut first = TextBuffer::from_text(1, Some(PathBuf::from("one.rs")), "one\n".to_owned());
        first.mark_dirty();
        let mut second =
            TextBuffer::from_text(2, Some(PathBuf::from("two.rs")), "two\n".to_owned());
        second.mark_dirty();
        let clean = TextBuffer::from_text(3, Some(PathBuf::from("clean.rs")), "clean\n".to_owned());
        let buffers = vec![first, second, clean];
        let pending_ids = vec![1, 1, 2, 3, 10, 11, 12, 13, 14];

        assert_eq!(pending_guard_dirty_count(&pending_ids, &buffers), 2);
    }

    #[test]
    fn pending_guard_external_change_count_counts_matching_ids_once() {
        let pending_ids = vec![1, 1, 2, 3, 10, 11, 12, 13, 14];
        let changed_on_disk = HashSet::from([1, 2, 8]);

        assert_eq!(
            pending_guard_external_change_count(&pending_ids, &changed_on_disk),
            2
        );
    }

    fn insert_dirty_pending_format_on_save(app: &mut KuroyaApp, id: BufferId, path: PathBuf) {
        let mut buffer = TextBuffer::from_text(id, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.pending_format_on_save.insert(
            id,
            PendingFormatOnSave {
                save_path: path.clone(),
                format_path: path,
                version,
                request_id: 1,
            },
        );
    }

    fn insert_dirty_pending_clean_reload(app: &mut KuroyaApp, id: BufferId, path: PathBuf) {
        let mut buffer = TextBuffer::from_text(id, Some(path.clone()), "fn main() {}\n".to_owned());
        let version = buffer.version();
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.in_flight_reloads.insert(
            id,
            PendingFileReload {
                request_id: 1,
                path,
                version,
                force_dirty: false,
            },
        );
    }

    fn insert_dirty_queued_clean_reload(app: &mut KuroyaApp, id: BufferId, path: PathBuf) {
        let mut buffer = TextBuffer::from_text(id, Some(path.clone()), "fn main() {}\n".to_owned());
        buffer.mark_dirty();
        app.buffers.push(buffer);
        app.queued_file_reloads.insert(
            id,
            QueuedFileReload {
                path,
                force_dirty: false,
            },
        );
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-workspace-guard-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}

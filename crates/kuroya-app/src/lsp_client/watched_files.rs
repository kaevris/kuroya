use super::{commands::LspClientCommand, handle::LspClientHandle};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use kuroya_core::WatchedFileChange;
use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, Mutex},
};

/// Upper bound on glob patterns tracked per server so a misbehaving server
/// cannot grow the registration table without limit. Extra watchers beyond
/// the cap are skipped (the server still hears the success response).
pub(in crate::lsp_client) const MAX_REGISTERED_WATCHER_GLOBS_PER_SERVER: usize = 1024;

/// Per-server `workspace/didChangeWatchedFiles` watcher registrations.
///
/// The runtime task writes it when the server sends
/// `client/registerCapability` / `client/unregisterCapability`; the app frame
/// loop reads it to decide which live clients should receive forwarded
/// filesystem events. Sharing mirrors the stderr ring: one allocation on the
/// handle, cheap clones on both sides.
///
/// Only plain string `globPattern` watchers are tracked today; object-shaped
/// patterns (`{ baseUri, pattern }`) are ignored.
#[derive(Clone, Debug, Default)]
pub(crate) struct LspWatchedFilesState {
    inner: Arc<Mutex<RegisteredWatchers>>,
}

impl LspWatchedFilesState {
    /// Records (or replaces) the watcher registration `id` with `globs`.
    /// Invalid patterns are skipped; valid bare filename patterns are
    /// expanded with `**/` so they match at any depth, mirroring the
    /// project-index glob expansion.
    pub(crate) fn register(&self, id: &str, globs: Vec<String>) {
        if let Ok(mut watchers) = self.inner.lock() {
            watchers.register(id, globs);
        }
    }

    /// Drops the registrations whose `client/registerCapability` id appears
    /// in `ids` (unknown ids are ignored).
    pub(crate) fn unregister(&self, ids: &[String]) {
        if let Ok(mut watchers) = self.inner.lock() {
            watchers.unregister(ids);
        }
    }

    /// Whether any registered watcher glob matches `path`.
    pub(crate) fn matches_any(&self, path: &Path) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .matches_any(path)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
    }
}

impl LspClientHandle {
    pub(crate) fn did_change_watched_files(&self, changes: Vec<WatchedFileChange>) -> bool {
        self.queue_command(LspClientCommand::DidChangeWatchedFiles { changes })
    }

    pub(crate) fn watched_files(&self) -> &LspWatchedFilesState {
        &self.watched_files
    }
}

#[derive(Debug, Default)]
struct RegisteredWatchers {
    registrations: Vec<(String, Vec<String>)>,
    glob_set: Option<GlobSet>,
}

impl RegisteredWatchers {
    fn register(&mut self, id: &str, globs: Vec<String>) {
        let globs = bounded_watcher_globs(globs);
        if let Some(entry) = self
            .registrations
            .iter_mut()
            .find(|(registered_id, _)| registered_id == id)
        {
            entry.1 = globs;
        } else {
            self.registrations.push((id.to_owned(), globs));
        }
        self.rebuild_glob_set();
    }

    fn unregister(&mut self, ids: &[String]) {
        if ids.is_empty() || self.registrations.is_empty() {
            return;
        }
        self.registrations
            .retain(|(registered_id, _)| !ids.contains(registered_id));
        self.rebuild_glob_set();
    }

    fn matches_any(&self, path: &Path) -> bool {
        self.glob_set
            .as_ref()
            .is_some_and(|glob_set| glob_set.is_match(path))
    }

    fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }

    fn rebuild_glob_set(&mut self) {
        let mut builder = GlobSetBuilder::new();
        let mut added = HashSet::new();
        for (_, globs) in &self.registrations {
            for glob in globs {
                add_watcher_glob(&mut builder, &mut added, glob);
            }
        }
        self.glob_set = (!added.is_empty()).then(|| builder.build().ok()).flatten();
    }
}

/// Caps and trims raw watcher glob strings before they are stored.
fn bounded_watcher_globs(globs: Vec<String>) -> Vec<String> {
    globs
        .into_iter()
        .filter(|glob| !glob.trim().is_empty())
        .take(MAX_REGISTERED_WATCHER_GLOBS_PER_SERVER)
        .collect()
}

fn add_watcher_glob(builder: &mut GlobSetBuilder, added: &mut HashSet<String>, pattern: &str) {
    let pattern = pattern.trim();
    if pattern.len() > MAX_REGISTERED_WATCHER_GLOB_PATTERN_BYTES {
        return;
    }
    // Bare filename patterns (`name.rs`) only match at the workspace root in
    // globset, while LSP servers expect them anywhere in the tree; expand to
    // the anchored descendant form like the project-index globs do.
    let bare = !pattern.contains(['/', '\\']) && !pattern.starts_with("**");
    if add_single_watcher_glob(builder, added, pattern).is_err() {
        return;
    }
    if bare {
        let descendant = format!("**/{pattern}");
        let _ = add_single_watcher_glob(builder, added, &descendant);
    }
}

const MAX_REGISTERED_WATCHER_GLOB_PATTERN_BYTES: usize = 512;

fn add_single_watcher_glob(
    builder: &mut GlobSetBuilder,
    added: &mut HashSet<String>,
    pattern: &str,
) -> Result<(), globset::Error> {
    if !added.insert(pattern.to_owned()) {
        return Ok(());
    }
    let mut glob = GlobBuilder::new(pattern);
    glob.case_insensitive(cfg!(windows));
    let compiled = glob.build()?;
    builder.add(compiled);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::commands::LspClientCommand;
    use super::super::handle::LspClientHandle;
    use super::{LspWatchedFilesState, MAX_REGISTERED_WATCHER_GLOBS_PER_SERVER};
    use kuroya_core::WatchedFileChange;
    use std::path::PathBuf;

    fn watcher_state_with_glob(pattern: &str) -> LspWatchedFilesState {
        let state = LspWatchedFilesState::default();
        state.register("watch-1", vec![pattern.to_owned()]);
        state
    }

    #[test]
    fn registered_glob_set_matches_absolute_paths_at_any_depth() {
        let state = watcher_state_with_glob("**/*.rs");

        assert!(state.matches_any(&PathBuf::from("C:/repo/src/main.rs")));
        assert!(state.matches_any(&PathBuf::from("C:/repo/main.rs")));
        assert!(!state.matches_any(&PathBuf::from("C:/repo/src/main.ts")));
    }

    #[test]
    fn bare_filename_globs_match_descendants() {
        let state = watcher_state_with_glob("Cargo.toml");

        assert!(state.matches_any(&PathBuf::from("C:/repo/Cargo.toml")));
        assert!(state.matches_any(&PathBuf::from("C:/repo/crates/app/Cargo.toml")));
        assert!(!state.matches_any(&PathBuf::from("C:/repo/src/main.rs")));
    }

    #[test]
    fn invalid_globs_are_skipped_without_poisoning_the_set() {
        let state = watcher_state_with_glob("**/[invalid");

        assert!(!state.matches_any(&PathBuf::from("C:/repo/src/main.rs")));

        state.register("watch-2", vec!["**/*.rs".to_owned()]);
        assert!(state.matches_any(&PathBuf::from("C:/repo/src/main.rs")));
    }

    #[test]
    fn unregister_removes_only_the_matching_registration_ids() {
        let state = LspWatchedFilesState::default();
        state.register("watch-rs", vec!["**/*.rs".to_owned()]);
        state.register("watch-ts", vec!["**/*.ts".to_owned()]);
        state.unregister(&["watch-rs".to_owned()]);

        assert!(!state.matches_any(&PathBuf::from("C:/repo/src/main.rs")));
        assert!(state.matches_any(&PathBuf::from("C:/repo/src/main.ts")));

        state.unregister(&["watch-ts".to_owned()]);
        assert!(state.is_empty());
        assert!(!state.matches_any(&PathBuf::from("C:/repo/src/main.ts")));
    }

    #[test]
    fn re_registering_an_id_replaces_its_previous_globs() {
        let state = LspWatchedFilesState::default();
        state.register("watch-1", vec!["**/*.rs".to_owned()]);
        state.register("watch-1", vec!["**/*.ts".to_owned()]);

        assert!(!state.matches_any(&PathBuf::from("C:/repo/src/main.rs")));
        assert!(state.matches_any(&PathBuf::from("C:/repo/src/main.ts")));
    }

    #[test]
    fn glob_storage_is_bounded_per_registration() {
        let globs: Vec<String> = (0..(MAX_REGISTERED_WATCHER_GLOBS_PER_SERVER + 64))
            .map(|index| format!("**/file-{index}.rs"))
            .collect();
        let state = LspWatchedFilesState::default();
        state.register("watch-1", globs);

        let dropped = format!("**/file-{MAX_REGISTERED_WATCHER_GLOBS_PER_SERVER}.rs");
        let kept_last = format!("**/file-{}.rs", MAX_REGISTERED_WATCHER_GLOBS_PER_SERVER - 1);
        assert!(!state.matches_any(&PathBuf::from(dropped)));
        assert!(state.matches_any(&PathBuf::from(kept_last)));
    }

    #[test]
    fn did_change_watched_files_queues_notification_command() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let handle = LspClientHandle::from_sender_for_test(tx, 1);

        let queued = handle.did_change_watched_files(vec![WatchedFileChange {
            uri: "file:///workspace/src/main.rs".to_owned(),
            kind: WatchedFileChange::CHANGED,
        }]);

        assert!(queued);
        assert!(matches!(
            rx.try_recv(),
            Ok(LspClientCommand::DidChangeWatchedFiles { .. })
        ));
    }
}

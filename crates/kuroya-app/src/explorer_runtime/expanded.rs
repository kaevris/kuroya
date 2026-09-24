use super::explorer_operation_path_label;
#[cfg(windows)]
use crate::workspace_trust::workspace_path_contains_lexically;
use crate::{
    KuroyaApp,
    explorer::{ExplorerEntryKind, path_matches_kind, retarget_path_prefix},
    workspace_trust::workspace_path_stays_within_root_lexically,
};
#[cfg(test)]
use std::ffi::{OsStr, OsString};
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

impl KuroyaApp {
    pub(super) fn expand_parent_of(&mut self, path: &Path) {
        if let Some(parent) = path.parent()
            && let Some((_, parent)) = normalized_workspace_path(&self.workspace.root, parent)
        {
            self.explorer_expanded.insert(parent);
        }
    }

    pub(crate) fn reveal_file_in_explorer(&mut self, path: std::path::PathBuf) {
        let Some((root, revealed_path)) = normalized_workspace_path(&self.workspace.root, &path)
        else {
            self.status = format!(
                "Cannot reveal {} outside the workspace",
                explorer_operation_path_label(&path)
            );
            return;
        };

        self.explorer_expanded
            .extend(explorer_ancestor_paths_with_normalized_root(
                root,
                &revealed_path,
            ));
        self.explorer_revealed_path = Some(revealed_path.clone());
        self.status = format!(
            "Revealed {} in Explorer",
            explorer_operation_path_label(&revealed_path)
        );
    }

    pub(super) fn retarget_expanded_paths(&mut self, old_path: &Path, new_path: &Path) {
        let mut retargeted = HashSet::with_capacity(self.explorer_expanded.len());
        for path in self.explorer_expanded.drain() {
            retargeted.insert(retarget_path_prefix(&path, old_path, new_path).unwrap_or(path));
        }
        self.explorer_expanded = retargeted;
    }

    pub(super) fn retarget_revealed_path(
        &mut self,
        old_path: &Path,
        new_path: &Path,
        kind: ExplorerEntryKind,
    ) {
        retarget_revealed_path(&mut self.explorer_revealed_path, old_path, new_path, kind);
    }

    pub(super) fn clear_deleted_revealed_path(&mut self, path: &Path, kind: ExplorerEntryKind) {
        clear_deleted_revealed_path(&mut self.explorer_revealed_path, path, kind);
    }
}

pub(crate) fn retarget_revealed_path(
    revealed_path: &mut Option<PathBuf>,
    old_path: &Path,
    new_path: &Path,
    kind: ExplorerEntryKind,
) {
    let Some(path) = revealed_path.as_ref() else {
        return;
    };
    if !path_matches_kind(path, old_path, kind) {
        return;
    }

    *revealed_path = retarget_path_prefix(path, old_path, new_path);
}

pub(crate) fn clear_deleted_revealed_path(
    revealed_path: &mut Option<PathBuf>,
    path: &Path,
    kind: ExplorerEntryKind,
) {
    if revealed_path
        .as_ref()
        .is_some_and(|revealed| path_matches_kind(revealed, path, kind))
    {
        *revealed_path = None;
    }
}

#[cfg(test)]
pub(crate) fn explorer_entry_visible_for(
    root: &Path,
    expanded_paths: &HashSet<PathBuf>,
    path: &Path,
) -> bool {
    ExplorerVisibility::new(root, expanded_paths)
        .is_some_and(|visibility| visibility.is_visible(path))
}

#[cfg(test)]
struct ExplorerVisibility {
    root: PathBuf,
    root_key: PathBuf,
    expanded_path_keys: HashSet<PathBuf>,
}

#[cfg(test)]
impl ExplorerVisibility {
    fn new(root: &Path, expanded_paths: &HashSet<PathBuf>) -> Option<Self> {
        let root = lexically_normalize_path(root)?;
        let root_key = explorer_visibility_path_key(&root)?;
        let expanded_path_keys = expanded_paths
            .iter()
            .filter_map(|path| explorer_visibility_path_key(path))
            .collect();

        Some(Self {
            root,
            root_key,
            expanded_path_keys,
        })
    }

    fn is_visible(&self, path: &Path) -> bool {
        explorer_entry_visible_with_normalized_root(
            &self.root,
            &self.root_key,
            &self.expanded_path_keys,
            path,
        )
    }
}

#[cfg(test)]
fn explorer_entry_visible_with_normalized_root(
    root: &Path,
    root_key: &Path,
    expanded_path_keys: &HashSet<PathBuf>,
    path: &Path,
) -> bool {
    let Some(path) = normalized_workspace_path_with_normalized_root(root, path) else {
        return false;
    };
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let Some(parent) = relative.parent() else {
        return true;
    };
    if parent.as_os_str().is_empty() {
        return true;
    }

    let mut current_key = root_key.to_path_buf();
    for component in parent.components() {
        current_key.push(explorer_visibility_component_key(component.as_os_str()));
        if !expanded_path_keys.contains(&current_key) {
            return false;
        }
    }

    true
}

pub(crate) fn explorer_ancestor_paths(root: &Path, path: &Path) -> Vec<std::path::PathBuf> {
    let Some((root, path)) = normalized_workspace_path(root, path) else {
        return Vec::new();
    };
    explorer_ancestor_paths_with_normalized_root(root, &path)
}

fn explorer_ancestor_paths_with_normalized_root(root: PathBuf, path: &Path) -> Vec<PathBuf> {
    let Ok(relative) = path.strip_prefix(&root) else {
        return Vec::new();
    };
    let ancestor_count = relative.components().count().saturating_sub(1);
    if ancestor_count == 0 {
        return Vec::new();
    }

    let mut current = root;
    let mut ancestors = Vec::with_capacity(ancestor_count);
    for component in relative.components().take(ancestor_count) {
        current.push(component.as_os_str());
        ancestors.push(current.clone());
    }
    ancestors
}

fn normalized_workspace_path(root: &Path, path: &Path) -> Option<(PathBuf, PathBuf)> {
    let root = lexically_normalize_path(root)?;
    let path = normalized_workspace_path_with_normalized_root(&root, path)?;
    Some((root, path))
}

fn normalized_workspace_path_with_normalized_root(root: &Path, path: &Path) -> Option<PathBuf> {
    if !workspace_path_stays_within_root_lexically(root, path) {
        return None;
    }
    let path = lexically_normalize_path(path)?;
    if path.starts_with(root) {
        return Some(path);
    }
    normalized_workspace_path_case_fallback(root, path)
}

#[cfg(windows)]
fn normalized_workspace_path_case_fallback(root: &Path, path: PathBuf) -> Option<PathBuf> {
    if !workspace_path_contains_lexically(root, &path) {
        return None;
    }

    let mut normalized_path = root.to_path_buf();
    for component in path.components().skip(root.components().count()) {
        normalized_path.push(component.as_os_str());
    }
    Some(normalized_path)
}

#[cfg(not(windows))]
fn normalized_workspace_path_case_fallback(_root: &Path, _path: PathBuf) -> Option<PathBuf> {
    None
}

#[cfg(test)]
fn explorer_visibility_path_key(path: &Path) -> Option<PathBuf> {
    let path = lexically_normalize_path(path)?;
    Some(explorer_visibility_normalized_path_key(&path))
}

#[cfg(all(test, not(windows)))]
fn explorer_visibility_normalized_path_key(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(all(test, windows))]
fn explorer_visibility_normalized_path_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                key.push(explorer_visibility_component_key(prefix.as_os_str()))
            }
            Component::RootDir => key.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !key.pop() {
                    key.push("..");
                }
            }
            Component::Normal(component) => key.push(explorer_visibility_component_key(component)),
        }
    }
    key
}

#[cfg(all(test, windows))]
fn explorer_visibility_component_key(component: &OsStr) -> OsString {
    component.to_string_lossy().to_lowercase().into()
}

#[cfg(all(test, not(windows)))]
fn explorer_visibility_component_key(component: &OsStr) -> OsString {
    component.to_os_string()
}

fn lexically_normalize_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(component) => normalized.push(component),
        }
    }
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn normalized_workspace_path_rejects_parent_reentry_paths() {
        let root = PathBuf::from("workspace").join("current");

        let contained = root.join("src").join("..").join("main.rs");
        assert_eq!(
            normalized_workspace_path(&root, &contained),
            Some((root.clone(), root.join("main.rs")))
        );

        let escaped_then_reentered = root.join("..").join("current").join("src").join("main.rs");
        assert_eq!(
            normalized_workspace_path(&root, &escaped_then_reentered),
            None
        );
        assert!(explorer_ancestor_paths(&root, &escaped_then_reentered).is_empty());

        let sibling_then_reentered = PathBuf::from("workspace")
            .join("other")
            .join("..")
            .join("current")
            .join("src")
            .join("main.rs");
        assert_eq!(
            normalized_workspace_path(&root, &sibling_then_reentered),
            None
        );
    }

    #[test]
    fn explorer_entry_visibility_rejects_outside_and_reentered_paths() {
        let root = PathBuf::from("workspace");
        let expanded = HashSet::from([root.join("src")]);

        assert!(explorer_entry_visible_for(
            &root,
            &expanded,
            &root.join("src").join("main.rs")
        ));
        assert!(!explorer_entry_visible_for(
            &root,
            &expanded,
            &root
                .join("..")
                .join("workspace")
                .join("src")
                .join("main.rs")
        ));
        assert!(!explorer_entry_visible_for(
            &root,
            &expanded,
            &PathBuf::from("outside").join("main.rs")
        ));
    }

    #[test]
    fn explorer_entry_visibility_normalizes_expanded_path_lookup() {
        let root = PathBuf::from("workspace");
        let expanded = HashSet::from([
            root.join("src").join("..").join("src"),
            root.join("src").join("nested").join("."),
        ]);

        assert!(explorer_entry_visible_for(
            &root,
            &expanded,
            &root.join("src").join("main.rs")
        ));
        assert!(explorer_entry_visible_for(
            &root,
            &expanded,
            &root.join("src").join("nested").join("lib.rs")
        ));
    }
}

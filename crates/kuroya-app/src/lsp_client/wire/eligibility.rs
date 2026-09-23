use kuroya_core::LspServerConfig;
use std::path::{Path, PathBuf};

pub fn can_use_server_for_path(config: &LspServerConfig, root: &Path, path: &Path) -> bool {
    can_use_server_for_path_with_probe(config, root, path, Path::exists)
}

fn can_use_server_for_path_with_probe(
    config: &LspServerConfig,
    root: &Path,
    path: &Path,
    mut path_exists: impl FnMut(&Path) -> bool,
) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    if config.root_markers.is_empty() {
        return path.starts_with(root);
    }

    let mut probe_cache = PathProbeCache::default();
    if path.parent() == Some(root) {
        let exists = probe_cache.get_or_probe(root_child_path(root, file_name), &mut path_exists);
        if exists {
            return true;
        }
    }

    if nearest_marker_root_exists(config, root, path, &mut path_exists, &mut probe_cache) {
        return true;
    }

    probe_cache.get_or_probe(root_child_path(root, file_name), path_exists)
        || path.starts_with(root)
}

#[derive(Default)]
struct PathProbeCache {
    entries: Vec<(PathBuf, bool)>,
}

impl PathProbeCache {
    fn get_or_probe(
        &mut self,
        candidate: PathBuf,
        mut path_exists: impl FnMut(&Path) -> bool,
    ) -> bool {
        if let Some((_, exists)) = self
            .entries
            .iter()
            .find(|(cached_candidate, _)| cached_candidate == &candidate)
        {
            return *exists;
        }

        let exists = path_exists(&candidate);
        self.entries.push((candidate, exists));
        exists
    }
}

fn nearest_marker_root_exists(
    config: &LspServerConfig,
    root: &Path,
    path: &Path,
    mut path_exists: impl FnMut(&Path) -> bool,
    probe_cache: &mut PathProbeCache,
) -> bool {
    if config.root_markers.is_empty() {
        return false;
    }

    let Some(mut dir) = path.parent() else {
        return false;
    };

    loop {
        if dir.starts_with(root) {
            for marker in &config.root_markers {
                if probe_cache.get_or_probe(root_child_path(dir, marker), &mut path_exists) {
                    return true;
                }
            }
        }

        if dir == root {
            return false;
        }

        let Some(parent) = dir.parent() else {
            return false;
        };
        dir = parent;
    }
}

fn root_child_path(root: &Path, child: &str) -> PathBuf {
    let mut candidate = root.to_path_buf();
    candidate.push(child);
    candidate
}

#[cfg(test)]
mod tests {
    use super::can_use_server_for_path_with_probe;
    use kuroya_core::LspServerConfig;
    use std::{cell::RefCell, path::PathBuf};

    #[test]
    fn nested_project_file_checks_root_markers_before_source_name() {
        let config = config_with_markers(&["Cargo.toml", "rust-project.json"]);
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let probes = RefCell::new(Vec::new());

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |candidate| {
                probes.borrow_mut().push(candidate.to_path_buf());
                candidate == root.join("Cargo.toml")
            },
        ));

        assert_eq!(
            probes.into_inner(),
            vec![
                root.join("src/Cargo.toml"),
                root.join("src/rust-project.json"),
                root.join("Cargo.toml"),
            ]
        );
    }

    #[test]
    fn nested_workspace_file_without_markers_can_use_workspace_root() {
        let config = config_with_markers(&["Cargo.toml", "rust-project.json"]);
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |_| false
        ));
    }

    #[test]
    fn nested_project_file_prefers_nearest_marker() {
        let config = config_with_markers(&["Cargo.toml"]);
        let root = PathBuf::from("workspace");
        let nested_root = root.join("crates/app");
        let path = nested_root.join("src/main.rs");
        let probes = RefCell::new(Vec::new());

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |candidate| {
                probes.borrow_mut().push(candidate.to_path_buf());
                candidate == nested_root.join("Cargo.toml") || candidate == root.join("Cargo.toml")
            },
        ));

        assert_eq!(
            probes.into_inner(),
            vec![
                nested_root.join("src/Cargo.toml"),
                nested_root.join("Cargo.toml"),
            ]
        );
    }

    #[test]
    fn root_project_file_keeps_fast_source_file_probe() {
        let config = config_with_markers(&["Cargo.toml"]);
        let root = PathBuf::from("workspace");
        let path = root.join("main.rs");
        let probes = RefCell::new(Vec::new());

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |candidate| {
                probes.borrow_mut().push(candidate.to_path_buf());
                candidate == root.join("main.rs")
            },
        ));

        assert_eq!(probes.into_inner(), vec![root.join("main.rs")]);
    }

    #[test]
    fn markerless_nested_file_uses_workspace_root_without_probing() {
        let config = config_with_markers(&[]);
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let probes = RefCell::new(Vec::new());

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |candidate| {
                probes.borrow_mut().push(candidate.to_path_buf());
                false
            },
        ));

        assert!(probes.into_inner().is_empty());
    }

    #[test]
    fn markerless_path_outside_workspace_is_not_eligible() {
        let config = config_with_markers(&[]);
        let root = PathBuf::from("workspace");
        let path = PathBuf::from("other/src/main.rs");

        assert!(!can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |_| true,
        ));
    }

    #[test]
    fn nested_file_name_marker_is_not_probed_again_before_workspace_fallback() {
        let config = config_with_markers(&["main.rs"]);
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let probes = RefCell::new(Vec::new());

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |candidate| {
                probes.borrow_mut().push(candidate.to_path_buf());
                false
            },
        ));

        assert_eq!(
            probes.into_inner(),
            vec![root.join("src/main.rs"), root.join("main.rs")]
        );
    }

    #[test]
    fn duplicate_root_markers_reuse_first_probe_before_fallback() {
        let config = config_with_markers(&["Cargo.toml", "Cargo.toml"]);
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let probes = RefCell::new(Vec::new());

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |candidate| {
                probes.borrow_mut().push(candidate.to_path_buf());
                candidate == root.join("main.rs")
            },
        ));

        assert_eq!(
            probes.into_inner(),
            vec![
                root.join("src/Cargo.toml"),
                root.join("Cargo.toml"),
                root.join("main.rs"),
            ]
        );
    }

    #[test]
    fn duplicate_root_file_name_markers_reuse_probe_before_workspace_fallback() {
        let config = config_with_markers(&["main.rs", "main.rs"]);
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let probes = RefCell::new(Vec::new());

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |candidate| {
                probes.borrow_mut().push(candidate.to_path_buf());
                false
            },
        ));

        assert_eq!(
            probes.into_inner(),
            vec![root.join("src/main.rs"), root.join("main.rs")]
        );
    }

    #[test]
    fn root_file_marker_reuses_fast_probe_before_other_markers() {
        let config = config_with_markers(&["main.rs", "Cargo.toml"]);
        let root = PathBuf::from("workspace");
        let path = root.join("main.rs");
        let probes = RefCell::new(Vec::new());

        assert!(can_use_server_for_path_with_probe(
            &config,
            &root,
            &path,
            |candidate| {
                probes.borrow_mut().push(candidate.to_path_buf());
                candidate == root.join("Cargo.toml")
            },
        ));

        assert_eq!(
            probes.into_inner(),
            vec![root.join("main.rs"), root.join("Cargo.toml")]
        );
    }

    fn config_with_markers(markers: &[&str]) -> LspServerConfig {
        LspServerConfig {
            language: "rust".to_owned(),
            command: "rust-analyzer".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: markers.iter().map(|marker| (*marker).to_owned()).collect(),
            enabled: true,
        }
    }
}

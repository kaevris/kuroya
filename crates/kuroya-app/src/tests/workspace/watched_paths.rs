use crate::workspace_state::{WatchedPathChanges, classify_watched_paths};
use crate::{persistence_storage::state_dir, workspace_state::settings_path};
use std::path::PathBuf;

#[test]
fn watched_paths_ignore_internal_state_but_keep_settings_and_project_changes() {
    let root = PathBuf::from("workspace");
    let src = root.join("src/main.rs");
    let duplicate_src = src.clone();
    let legacy_settings = root.join(".kuroya/settings.toml");
    let tasks = root.join(".kuroya/tasks.toml");
    let plugin_manifest = root.join(".kuroya/plugins/example/plugin.toml");
    let session = root.join(".kuroya/session.json");
    let session_temp = root.join(".kuroya/.session.json.tmp.1");

    assert_eq!(
        classify_watched_paths(
            &root,
            &[
                session,
                session_temp,
                legacy_settings,
                tasks.clone(),
                plugin_manifest,
                src.clone(),
                duplicate_src
            ]
        ),
        WatchedPathChanges {
            settings_changed: false,
            tasks_changed: true,
            plugins_changed: true,
            workspace_refresh_needed: true,
            project_paths: vec![src]
        }
    );
}

#[test]
fn watched_paths_normalize_duplicates_and_ignore_workspace_escapes() {
    let root = PathBuf::from("workspace");
    let src = root.join("src/main.rs");
    let cargo = root.join("Cargo.toml");

    assert_eq!(
        classify_watched_paths(
            &root,
            &[
                root.join("src/./main.rs"),
                root.join("src/generated/../main.rs"),
                root.join(".kuroya/plugins/../session.json"),
                root.join(".kuroya/../Cargo.toml"),
                root.join("src/../../outside.rs"),
            ],
        ),
        WatchedPathChanges {
            settings_changed: false,
            tasks_changed: true,
            plugins_changed: false,
            workspace_refresh_needed: true,
            project_paths: vec![src, cargo],
        }
    );
}

#[test]
fn watched_paths_ignore_stacked_parent_reentry_and_current_dir_escapes() {
    assert_eq!(
        classify_watched_paths(
            &PathBuf::from("workspace"),
            &[
                PathBuf::from("../../../workspace/.kuroya/plugins/example/plugin.toml"),
                PathBuf::from("../../workspace/Cargo.toml"),
                PathBuf::from("workspace-sibling/.kuroya/plugins/example/plugin.toml"),
            ],
        ),
        WatchedPathChanges::default()
    );

    assert_eq!(
        classify_watched_paths(
            &PathBuf::from("."),
            &[
                PathBuf::from("../outside.rs"),
                PathBuf::from("../../.kuroya/plugins/example/plugin.toml"),
                PathBuf::from("src/main.rs"),
            ],
        ),
        WatchedPathChanges {
            settings_changed: false,
            tasks_changed: false,
            plugins_changed: false,
            workspace_refresh_needed: true,
            project_paths: vec![PathBuf::from("src/main.rs")],
        }
    );
}

#[cfg(windows)]
#[test]
fn watched_paths_match_workspace_paths_case_insensitively() {
    let root = PathBuf::from(r"C:\Repo\Project");
    let legacy_settings = PathBuf::from(r"c:\repo\project\.kuroya\settings.toml");
    let tasks = PathBuf::from(r"c:\repo\project\.kuroya\tasks.toml");
    let plugin_manifest = PathBuf::from(r"c:\repo\project\.kuroya\plugins\example\plugin.toml");
    let outside = PathBuf::from(r"c:\repo\other\src\main.rs");

    assert_eq!(
        classify_watched_paths(&root, &[legacy_settings, tasks, plugin_manifest, outside]),
        WatchedPathChanges {
            settings_changed: false,
            tasks_changed: true,
            plugins_changed: true,
            workspace_refresh_needed: false,
            project_paths: Vec::new(),
        }
    );
}

#[test]
fn watched_paths_do_not_treat_crash_recovery_writes_as_project_changes() {
    let root = PathBuf::from("workspace");

    assert_eq!(
        classify_watched_paths(
            &root,
            &[
                root.join(".kuroya/session.json"),
                root.join(".kuroya/.session.json.tmp.2"),
                state_dir(&root).join("session.json"),
                state_dir(&root).join(".session.json.tmp.2"),
            ]
        ),
        WatchedPathChanges::default()
    );
}

#[test]
fn watched_paths_reload_global_settings_path() {
    let root = PathBuf::from("workspace");
    let settings = settings_path(&root);

    assert_eq!(
        classify_watched_paths(&root, &[settings]),
        WatchedPathChanges {
            settings_changed: true,
            tasks_changed: false,
            plugins_changed: false,
            workspace_refresh_needed: false,
            project_paths: Vec::new(),
        }
    );
}

#[test]
fn watched_paths_keep_project_tasks_and_plugins_under_workspace_kuroya() {
    let root = PathBuf::from("workspace");
    let tasks = root.join(".kuroya/tasks.toml");
    let plugin_manifest = root.join(".kuroya/plugins/example/plugin.toml");

    assert_eq!(
        classify_watched_paths(&root, &[tasks, plugin_manifest]),
        WatchedPathChanges {
            settings_changed: false,
            tasks_changed: true,
            plugins_changed: true,
            workspace_refresh_needed: false,
            project_paths: Vec::new(),
        }
    );
}

#[test]
fn watched_paths_reload_tasks_for_inferred_task_sources() {
    let root = PathBuf::from("workspace");
    let cargo = root.join("Cargo.toml");
    let package = root.join("package.json");
    let child_package = root.join("packages/app/package.json");
    let pnpm_lock = root.join("pnpm-lock.yaml");
    let yarn_lock = root.join("yarn.lock");
    let bun_lockb = root.join("bun.lockb");
    let bun_lock = root.join("bun.lock");
    let makefile = root.join("Makefile");
    let justfile = root.join("justfile");
    let dot_justfile = root.join(".justfile");

    assert_eq!(
        classify_watched_paths(
            &root,
            &[
                cargo.clone(),
                package.clone(),
                child_package.clone(),
                pnpm_lock.clone(),
                yarn_lock.clone(),
                bun_lockb.clone(),
                bun_lock.clone(),
                makefile.clone(),
                justfile.clone(),
                dot_justfile.clone(),
            ],
        ),
        WatchedPathChanges {
            settings_changed: false,
            tasks_changed: true,
            plugins_changed: false,
            workspace_refresh_needed: true,
            project_paths: vec![
                cargo,
                package,
                child_package,
                pnpm_lock,
                yarn_lock,
                bun_lockb,
                bun_lock,
                makefile,
                justfile,
                dot_justfile
            ],
        }
    );
}

#[test]
fn watched_paths_do_not_reload_tasks_for_generated_dependency_manifests() {
    let root = PathBuf::from("workspace");
    let package = root.join("node_modules/dep/package.json");
    let pnpm_lock = root.join("node_modules/dep/pnpm-lock.yaml");
    let yarn_lock = root.join("node_modules/dep/yarn.lock");
    let bun_lockb = root.join("node_modules/dep/bun.lockb");
    let bun_lock = root.join("node_modules/dep/bun.lock");
    let justfile = root.join("node_modules/dep/justfile");

    assert_eq!(
        classify_watched_paths(
            &root,
            &[
                package.clone(),
                pnpm_lock.clone(),
                yarn_lock.clone(),
                bun_lockb.clone(),
                bun_lock.clone(),
                justfile.clone(),
            ]
        ),
        WatchedPathChanges::default()
    );
}

#[test]
fn watched_paths_ignore_generated_changes() {
    let root = PathBuf::from("workspace");
    let generated = root.join("target/debug/generated.rs");
    let dependency = root.join("node_modules/dep/index.js");

    assert_eq!(
        classify_watched_paths(&root, &[generated.clone(), dependency.clone()]),
        WatchedPathChanges::default()
    );
}

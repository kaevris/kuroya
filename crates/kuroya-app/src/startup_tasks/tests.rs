use super::{
    GIT_SCOPED_REFRESH_MAX_PATHS, GitScanRootCacheEntry, PendingWorkspacePluginReload,
    PendingWorkspaceRefresh, WORKSPACE_PLUGIN_RELOAD_DEBOUNCE, WORKSPACE_PLUGIN_RELOAD_MAX_WAIT,
    WORKSPACE_REFRESH_DEBOUNCE, WORKSPACE_REFRESH_MAX_WAIT, begin_git_scan_request_state,
    begin_workspace_index_request_state, begin_workspace_plugin_discovery_request_state,
    cached_git_scan_root_for_auto_repository_detection, filter_disabled_workspace_plugins,
    finish_git_scan_request_state, finish_workspace_index_request_state,
    finish_workspace_plugin_discovery_request_state, git_auto_refresh_enabled,
    git_open_parent_repositories, git_repository_ignored, git_repository_in_subfolders_with_limits,
    git_repository_marker_exists, git_repository_scan_children, git_repository_scan_folder_ignored,
    git_scan_root_for_auto_repository_detection, git_scoped_refresh_is_supported,
    invalidate_git_scan_request_state, invalidate_startup_task_request_state,
    invalidate_workspace_index_request_state, invalidate_workspace_plugin_discovery_request_state,
    next_startup_task_request_id, pending_refresh_is_due, pending_refresh_wakeup_at,
    plugins_disabled_status, resolved_cached_git_scan_root_for_auto_repository_detection,
    workspace_plugin_reload_due, workspace_plugin_reload_wakeup_at, workspace_plugins_enabled,
    workspace_plugins_restricted_status,
};
use crate::{
    persistence_storage::{app_state_dir, state_dir},
    ui_events::UiEvent,
};
use kuroya_core::{
    GitAutoRepositoryDetection, GitOpenRepositoryInParentFolders, PluginCapabilities,
    PluginContributions, PluginDescriptor, PluginDiscoveryError, PluginManifest,
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[test]
fn workspace_plugin_reload_waits_for_debounce_window() {
    let scheduled = std::time::Instant::now();
    let pending = PendingWorkspacePluginReload::new(scheduled);

    assert!(!workspace_plugin_reload_due(
        None,
        scheduled + WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
        WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
        WORKSPACE_PLUGIN_RELOAD_MAX_WAIT
    ));
    assert!(!workspace_plugin_reload_due(
        Some(pending),
        scheduled + WORKSPACE_PLUGIN_RELOAD_DEBOUNCE - Duration::from_millis(1),
        WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
        WORKSPACE_PLUGIN_RELOAD_MAX_WAIT
    ));
    assert!(workspace_plugin_reload_due(
        Some(pending),
        scheduled + WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
        WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
        WORKSPACE_PLUGIN_RELOAD_MAX_WAIT
    ));
}

#[test]
fn workspace_plugin_reload_max_wait_prevents_debounce_starvation() {
    let scheduled = std::time::Instant::now();
    let mut pending = PendingWorkspacePluginReload::new(scheduled);
    for elapsed in 1..WORKSPACE_PLUGIN_RELOAD_MAX_WAIT.as_millis() {
        pending.record_change(scheduled + Duration::from_millis(elapsed as u64));
        if elapsed % 250 == 0 {
            assert!(
                !workspace_plugin_reload_due(
                    Some(pending),
                    scheduled + Duration::from_millis(elapsed as u64),
                    WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
                    WORKSPACE_PLUGIN_RELOAD_MAX_WAIT
                ),
                "reload fired early at {elapsed}ms"
            );
        }
    }

    assert!(workspace_plugin_reload_due(
        Some(pending),
        scheduled + WORKSPACE_PLUGIN_RELOAD_MAX_WAIT,
        WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
        WORKSPACE_PLUGIN_RELOAD_MAX_WAIT
    ));
}

#[test]
fn workspace_refresh_waits_for_debounce_window() {
    let scheduled = std::time::Instant::now();
    let pending = PendingWorkspaceRefresh::new(scheduled);

    assert!(!pending_refresh_is_due(
        &pending,
        scheduled + WORKSPACE_REFRESH_DEBOUNCE - Duration::from_millis(1)
    ));
    assert!(pending_refresh_is_due(
        &pending,
        scheduled + WORKSPACE_REFRESH_DEBOUNCE
    ));
}

#[test]
fn workspace_refresh_max_wait_prevents_debounce_starvation() {
    let scheduled = std::time::Instant::now();
    let mut pending = PendingWorkspaceRefresh::new(scheduled);
    pending.record_change(scheduled + WORKSPACE_REFRESH_MAX_WAIT - Duration::from_millis(1));

    assert!(pending_refresh_is_due(
        &pending,
        scheduled + WORKSPACE_REFRESH_MAX_WAIT
    ));
}

#[test]
fn workspace_refresh_wakeup_bounds_debounce_by_max_wait() {
    let scheduled = std::time::Instant::now();
    let mut pending = PendingWorkspaceRefresh::new(scheduled);

    assert_eq!(
        pending_refresh_wakeup_at(&pending),
        scheduled + WORKSPACE_REFRESH_DEBOUNCE
    );

    pending.record_change(scheduled + WORKSPACE_REFRESH_MAX_WAIT);
    assert_eq!(
        pending_refresh_wakeup_at(&pending),
        scheduled + WORKSPACE_REFRESH_MAX_WAIT
    );
}

#[test]
fn workspace_plugin_reload_wakeup_bounds_debounce_by_max_wait() {
    let scheduled = std::time::Instant::now();
    let mut pending = PendingWorkspacePluginReload::new(scheduled);

    assert_eq!(
        workspace_plugin_reload_wakeup_at(
            Some(pending),
            WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
            WORKSPACE_PLUGIN_RELOAD_MAX_WAIT
        ),
        Some(scheduled + WORKSPACE_PLUGIN_RELOAD_DEBOUNCE)
    );

    pending.record_change(scheduled + WORKSPACE_PLUGIN_RELOAD_MAX_WAIT);
    assert_eq!(
        workspace_plugin_reload_wakeup_at(
            Some(pending),
            WORKSPACE_PLUGIN_RELOAD_DEBOUNCE,
            WORKSPACE_PLUGIN_RELOAD_MAX_WAIT
        ),
        Some(scheduled + WORKSPACE_PLUGIN_RELOAD_MAX_WAIT)
    );
}

#[test]
fn workspace_index_request_starts_when_idle() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );

    assert_eq!(next_request_id, 1);
    assert_eq!(active_request_id, 1);
    assert_eq!(in_flight, Some(1));
    assert!(!queued);
}

#[test]
fn startup_task_request_ids_wrap_without_zero() {
    assert_eq!(next_startup_task_request_id(0), 1);
    assert_eq!(next_startup_task_request_id(41), 42);
    assert_eq!(next_startup_task_request_id(u64::MAX - 1), u64::MAX);
    assert_eq!(next_startup_task_request_id(u64::MAX), 1);
}

#[test]
fn workspace_index_request_queues_once_while_in_flight() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );
    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );
    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );

    assert_eq!(next_request_id, 1);
    assert_eq!(active_request_id, 1);
    assert_eq!(in_flight, Some(1));
    assert!(queued);
}

#[test]
fn startup_task_queued_request_keeps_saturated_in_flight_id_active() {
    let mut next_request_id = u64::MAX - 1;
    let mut active_request_id = u64::MAX - 1;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(u64::MAX)
    );
    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );

    assert_eq!(next_request_id, u64::MAX);
    assert_eq!(active_request_id, u64::MAX);
    assert_eq!(in_flight, Some(u64::MAX));
    assert!(queued);

    assert!(finish_workspace_index_request_state(
        &mut in_flight,
        &mut queued,
        u64::MAX,
    ));
    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );
    assert_eq!(active_request_id, 1);
    assert_eq!(in_flight, Some(1));
}

#[test]
fn workspace_index_finish_drains_queued_refresh_once() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );
    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );

    assert!(finish_workspace_index_request_state(
        &mut in_flight,
        &mut queued,
        1
    ));
    assert_eq!(in_flight, None);
    assert!(!queued);
    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(2)
    );
}

#[test]
fn startup_task_invalidation_does_not_reuse_saturated_in_flight_id() {
    let mut next_request_id = u64::MAX;
    let mut active_request_id = u64::MAX;
    let mut in_flight = Some(u64::MAX);
    let mut queued = true;

    invalidate_startup_task_request_state(
        &mut next_request_id,
        &mut active_request_id,
        &mut in_flight,
        &mut queued,
    );

    assert_eq!(next_request_id, 1);
    assert_eq!(active_request_id, 1);
    assert_eq!(in_flight, None);
    assert!(!queued);
    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(2)
    );
}

#[test]
fn workspace_index_finish_ignores_unrelated_request_id() {
    let mut in_flight = Some(4);
    let mut queued = true;

    assert!(!finish_workspace_index_request_state(
        &mut in_flight,
        &mut queued,
        3
    ));

    assert_eq!(in_flight, Some(4));
    assert!(queued);
}

#[test]
fn workspace_index_invalidation_keeps_request_ids_monotonic() {
    let mut next_request_id = 4;
    let mut active_request_id = 4;
    let mut in_flight = Some(4);
    let mut queued = true;

    invalidate_workspace_index_request_state(
        &mut next_request_id,
        &mut active_request_id,
        &mut in_flight,
        &mut queued,
    );

    assert_eq!(next_request_id, 5);
    assert_eq!(active_request_id, 5);
    assert_eq!(in_flight, None);
    assert!(!queued);
    assert_eq!(
        begin_workspace_index_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(6)
    );
}

#[test]
fn git_scan_request_starts_when_idle() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_git_scan_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );

    assert_eq!(next_request_id, 1);
    assert_eq!(active_request_id, 1);
    assert_eq!(in_flight, Some(1));
    assert!(!queued);
}

#[test]
fn git_scan_request_queues_once_while_in_flight() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_git_scan_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );
    assert_eq!(
        begin_git_scan_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );
    assert_eq!(
        begin_git_scan_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );

    assert_eq!(next_request_id, 1);
    assert_eq!(active_request_id, 1);
    assert_eq!(in_flight, Some(1));
    assert!(queued);
}

#[test]
fn git_scan_finish_drains_queued_refresh_once() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_git_scan_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );
    assert_eq!(
        begin_git_scan_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );

    assert!(finish_git_scan_request_state(
        &mut in_flight,
        &mut queued,
        1
    ));
    assert_eq!(in_flight, None);
    assert!(!queued);
    assert_eq!(
        begin_git_scan_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(2)
    );
}

#[test]
fn git_scan_finish_ignores_unrelated_request_id() {
    let mut in_flight = Some(4);
    let mut queued = true;

    assert!(!finish_git_scan_request_state(
        &mut in_flight,
        &mut queued,
        3
    ));

    assert_eq!(in_flight, Some(4));
    assert!(queued);
}

#[test]
fn git_scan_invalidation_keeps_request_ids_monotonic() {
    let mut next_request_id = 4;
    let mut active_request_id = 4;
    let mut in_flight = Some(4);
    let mut queued = true;

    invalidate_git_scan_request_state(
        &mut next_request_id,
        &mut active_request_id,
        &mut in_flight,
        &mut queued,
    );

    assert_eq!(next_request_id, 5);
    assert_eq!(active_request_id, 5);
    assert_eq!(in_flight, None);
    assert!(!queued);
    assert_eq!(
        begin_git_scan_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(6)
    );
}

#[test]
fn workspace_plugin_discovery_request_starts_when_idle() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );

    assert_eq!(next_request_id, 1);
    assert_eq!(active_request_id, 1);
    assert_eq!(in_flight, Some(1));
    assert!(!queued);
}

#[test]
fn workspace_plugin_discovery_request_queues_once_while_in_flight() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );
    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );
    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );

    assert_eq!(next_request_id, 1);
    assert_eq!(active_request_id, 1);
    assert_eq!(in_flight, Some(1));
    assert!(queued);
}

#[test]
fn workspace_plugin_discovery_finish_drains_queued_reload_once() {
    let mut next_request_id = 0;
    let mut active_request_id = 0;
    let mut in_flight = None;
    let mut queued = false;

    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(1)
    );
    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        None
    );

    assert!(finish_workspace_plugin_discovery_request_state(
        &mut in_flight,
        &mut queued,
        1
    ));
    assert_eq!(in_flight, None);
    assert!(!queued);
    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(2)
    );
}

#[test]
fn workspace_plugin_discovery_finish_ignores_unrelated_request_id() {
    let mut in_flight = Some(4);
    let mut queued = true;

    assert!(!finish_workspace_plugin_discovery_request_state(
        &mut in_flight,
        &mut queued,
        3
    ));

    assert_eq!(in_flight, Some(4));
    assert!(queued);
}

#[test]
fn workspace_plugin_discovery_invalidation_keeps_request_ids_monotonic() {
    let mut next_request_id = 4;
    let mut active_request_id = 4;
    let mut in_flight = Some(4);
    let mut queued = true;

    invalidate_workspace_plugin_discovery_request_state(
        &mut next_request_id,
        &mut active_request_id,
        &mut in_flight,
        &mut queued,
    );

    assert_eq!(next_request_id, 5);
    assert_eq!(active_request_id, 5);
    assert_eq!(in_flight, None);
    assert!(!queued);
    assert_eq!(
        begin_workspace_plugin_discovery_request_state(
            &mut next_request_id,
            &mut active_request_id,
            &mut in_flight,
            &mut queued,
        ),
        Some(6)
    );
}

#[test]
fn git_auto_refresh_requires_git_and_autorefresh() {
    assert!(git_auto_refresh_enabled(true, true));
    assert!(!git_auto_refresh_enabled(false, true));
    assert!(!git_auto_refresh_enabled(true, false));
    assert!(!git_auto_refresh_enabled(false, false));
}

#[test]
fn git_parent_repository_policy_matches_vs_code_setting_values() {
    assert!(git_open_parent_repositories(
        GitOpenRepositoryInParentFolders::Always
    ));
    assert!(git_open_parent_repositories(
        GitOpenRepositoryInParentFolders::Prompt
    ));
    assert!(!git_open_parent_repositories(
        GitOpenRepositoryInParentFolders::Never
    ));
}

#[test]
fn git_auto_repository_detection_selects_workspace_subfolder_or_open_editor_repo() {
    let root = std::env::temp_dir()
        .join(format!("kuroya-auto-repo-{}", std::process::id()))
        .join("workspace");
    let repo = root.join("packages").join("app");
    let source = repo.join("src").join("main.rs");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "fn main() {}\n").unwrap();

    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::False,
            1,
            &[],
            &[]
        ),
        None
    );
    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::True,
            1,
            &[],
            &[]
        ),
        Some(root.clone())
    );
    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::SubFolders,
            1,
            &[],
            &[]
        ),
        None
    );
    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &[],
            &[]
        ),
        Some(repo.clone())
    );
    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &["packages".to_owned()],
            &[]
        ),
        None
    );
    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::OpenEditors,
            1,
            &[],
            std::slice::from_ref(&source),
        ),
        Some(repo)
    );
    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::OpenEditors,
            1,
            &["packages".to_owned()],
            std::slice::from_ref(&source),
        ),
        None
    );

    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_open_editor_detection_ignores_paths_outside_workspace() {
    let base =
        std::env::temp_dir().join(format!("kuroya-open-editor-outside-{}", std::process::id()));
    let root = base.join("workspace");
    let outside_repo = base.join("outside_repo");
    let source = outside_repo.join("src").join("main.rs");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(outside_repo.join(".git")).unwrap();
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, "fn main() {}\n").unwrap();

    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::OpenEditors,
            1,
            &[],
            std::slice::from_ref(&source),
        ),
        None
    );

    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn git_repository_marker_accepts_directory_and_file_markers() {
    let root = temp_workspace("kuroya-git-marker-kind");
    fs::create_dir_all(&root).unwrap();

    assert!(!git_repository_marker_exists(&root));

    fs::create_dir_all(root.join(".git")).unwrap();
    assert!(git_repository_marker_exists(&root));

    fs::remove_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git"), "gitdir: ../actual.git\n").unwrap();
    assert!(git_repository_marker_exists(&root));

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_subfolder_scan_detects_worktree_file_marker() {
    let root = temp_workspace("kuroya-auto-repo-worktree-file");
    let repo = root.join("packages").join("app");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join(".git"), "gitdir: ../../.git/worktrees/app\n").unwrap();

    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &[],
            &[]
        ),
        Some(repo)
    );

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_subfolder_scan_children_are_bounded_and_sorted() {
    let root = temp_workspace("kuroya-auto-repo-children");
    fs::create_dir_all(root.join("b")).unwrap();
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(root.join("file.txt"), "").unwrap();

    let children = git_repository_scan_children(&root, 2);

    assert_eq!(children.len(), 2);
    assert!(children.iter().all(|path| path.is_dir()));
    assert!(children.windows(2).all(|pair| pair[0] <= pair[1]));

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_subfolder_scan_children_follow_symlinked_directories() {
    let root = temp_workspace("kuroya-auto-repo-symlink-child");
    let target = root.join("target");
    let link = root.join("linked");
    fs::create_dir_all(&target).unwrap();
    if create_directory_symlink(&target, &link).is_err() {
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
        return;
    }

    let children = git_repository_scan_children(&root, 8);

    assert!(children.contains(&link));

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_subfolder_scan_respects_visited_folder_limit() {
    let root = temp_workspace("kuroya-auto-repo-visited");
    let nested_repo = root.join("a").join("nested");
    fs::create_dir_all(nested_repo.join(".git")).unwrap();

    assert_eq!(
        git_repository_in_subfolders_with_limits(&root, 3, &[], 16, 1),
        None
    );
    assert_eq!(
        git_repository_in_subfolders_with_limits(&root, 3, &[], 16, 4),
        Some(nested_repo)
    );

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_scan_root_cache_reuses_detected_subfolder_repo_until_key_changes() {
    let root = temp_workspace("kuroya-auto-repo-cache");
    let first_repo = root.join("b_repo");
    let earlier_repo = root.join("a_repo");
    fs::create_dir_all(first_repo.join(".git")).unwrap();

    let mut cache = None;
    assert_eq!(
        cached_git_scan_root_for_auto_repository_detection(
            &mut cache,
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &[],
            &[]
        ),
        Some(first_repo.clone())
    );

    fs::create_dir_all(earlier_repo.join(".git")).unwrap();
    assert_eq!(
        git_scan_root_for_auto_repository_detection(
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &[],
            &[]
        ),
        Some(earlier_repo.clone())
    );
    assert_eq!(
        cached_git_scan_root_for_auto_repository_detection(
            &mut cache,
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &[],
            &[]
        ),
        Some(first_repo)
    );

    assert_eq!(
        cached_git_scan_root_for_auto_repository_detection(
            &mut cache,
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &["b_repo".to_owned()],
            &[]
        ),
        Some(earlier_repo)
    );

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_scan_root_cache_recomputes_when_cached_marker_disappears() {
    let root = temp_workspace("kuroya-auto-repo-cache-stale");
    let first_repo = root.join("a_repo");
    let second_repo = root.join("b_repo");
    fs::create_dir_all(first_repo.join(".git")).unwrap();
    fs::create_dir_all(second_repo.join(".git")).unwrap();

    let mut cache = None;
    assert_eq!(
        cached_git_scan_root_for_auto_repository_detection(
            &mut cache,
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &[],
            &[]
        ),
        Some(first_repo.clone())
    );

    fs::remove_dir_all(first_repo.join(".git")).unwrap();
    assert_eq!(
        cached_git_scan_root_for_auto_repository_detection(
            &mut cache,
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &[],
            &[]
        ),
        Some(second_repo)
    );

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_scan_root_cache_tracks_open_editor_inputs() {
    let root = temp_workspace("kuroya-auto-repo-open-editor-cache");
    let app_repo = root.join("packages").join("app");
    let lib_repo = root.join("packages").join("lib");
    let app_source = app_repo.join("src").join("main.rs");
    let lib_source = lib_repo.join("src").join("lib.rs");
    fs::create_dir_all(app_repo.join(".git")).unwrap();
    fs::create_dir_all(lib_repo.join(".git")).unwrap();
    fs::create_dir_all(app_source.parent().unwrap()).unwrap();
    fs::create_dir_all(lib_source.parent().unwrap()).unwrap();
    fs::write(&app_source, "").unwrap();
    fs::write(&lib_source, "").unwrap();

    let mut cache = None;
    assert_eq!(
        cached_git_scan_root_for_auto_repository_detection(
            &mut cache,
            &root,
            GitAutoRepositoryDetection::OpenEditors,
            2,
            &[],
            std::slice::from_ref(&app_source),
        ),
        Some(app_repo)
    );
    assert_eq!(
        cached_git_scan_root_for_auto_repository_detection(
            &mut cache,
            &root,
            GitAutoRepositoryDetection::OpenEditors,
            2,
            &[],
            std::slice::from_ref(&lib_source),
        ),
        Some(lib_repo)
    );

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn spawn_git_scan_defers_auto_repository_discovery_and_cache_update() {
    let root = temp_workspace("kuroya-auto-repo-deferred-scan");
    let repo = root.join("packages").join("app");
    fs::create_dir_all(repo.join(".git")).unwrap();

    {
        let mut app =
            crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
        app.settings.git_auto_repository_detection = GitAutoRepositoryDetection::SubFolders;
        app.settings.git_repository_scan_max_depth = 2;

        assert!(app.spawn_git_scan());
        assert_eq!(app.git_scan_active_request_id, 1);
        assert_eq!(app.git_scan_in_flight_request_id, Some(1));
        assert!(app.git_scan_root_cache.is_none());
    }

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn startup_tasks_skip_placeholder_workspace() {
    let root = temp_workspace("kuroya-placeholder-startup-tasks");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.workspace_placeholder = true;

    app.spawn_index();

    assert_eq!(app.workspace_index_in_flight_request_id, None);
    assert!(!app.spawn_git_scan());
    assert_eq!(app.git_scan_in_flight_request_id, None);
    assert!(!app.spawn_plugin_discovery());
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert_eq!(app.status, "No folder open");

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn ensure_workspace_index_started_is_lazy_and_idempotent() {
    let root = temp_workspace("kuroya-lazy-index-start");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);

    assert!(app.ensure_workspace_index_started());
    assert_eq!(app.workspace_index_in_flight_request_id, Some(1));
    assert!(!app.ensure_workspace_index_started());

    app.workspace_index_in_flight_request_id = None;
    app.project_index_generation = 1;
    assert!(!app.ensure_workspace_index_started());

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn spawn_index_preserves_explorer_directory_cache() {
    let root = temp_workspace("kuroya-spawn-index-explorer-cache");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.explorer_directory_cache.insert(
        root.clone(),
        crate::explorer_tree_panel::ExplorerDirectorySnapshot::Ready(
            crate::explorer_tree_panel::ExplorerDirectoryEntries::default(),
        ),
    );

    app.spawn_index();

    assert!(matches!(
        app.explorer_directory_cache.get(&root),
        Some(crate::explorer_tree_panel::ExplorerDirectorySnapshot::Ready(_))
    ));

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn project_index_options_exclude_only_nested_app_state() {
    let app_state = app_state_dir();
    let workspace_root = app_state
        .parent()
        .expect("test app state should have a parent")
        .to_path_buf();
    let app =
        crate::source_control_runtime::source_control_app_for_test(workspace_root.clone(), true);
    let filter = app.project_index_options().path_filter(&workspace_root);

    assert!(filter.is_excluded(&app_state.join("workspaces/current/session.json")));
    assert!(!filter.is_excluded(&workspace_root.join("src/main.rs")));

    let app = crate::source_control_runtime::source_control_app_for_test(app_state.clone(), true);
    let filter = app.project_index_options().path_filter(&app_state);
    assert!(!filter.is_excluded(&app_state.join("src/main.rs")));
    assert!(filter.is_excluded(&state_dir(&app_state).join("session.json")));
}

#[test]
fn current_git_scan_completion_applies_worker_root_cache_entry() {
    let root = temp_workspace("kuroya-auto-repo-worker-cache");
    let repo = root.join("packages").join("app");
    fs::create_dir_all(repo.join(".git")).unwrap();

    let (scan_root, cache_entry) = resolved_cached_git_scan_root_for_auto_repository_detection(
        None,
        &root,
        GitAutoRepositoryDetection::SubFolders,
        2,
        &[],
        &[],
    );
    assert_eq!(scan_root, Some(repo.clone()));
    let cache_entry = cache_entry.expect("subfolder scan should create a cache entry");

    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.settings.git_auto_repository_detection = GitAutoRepositoryDetection::SubFolders;
    app.git_scan_next_request_id = 1;
    app.git_scan_active_request_id = 1;
    app.git_scan_in_flight_request_id = Some(1);

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root: root.clone(),
            scan_root: Some(repo),
            root_cache_entry: Some(cache_entry.clone()),
            git: kuroya_core::GitSnapshot::default(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.git_scan_root_cache, Some(cache_entry));

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn active_git_scan_without_resolved_root_clears_git_cache_and_selection() {
    let root = temp_workspace("kuroya-auto-repo-no-root");
    fs::create_dir_all(&root).unwrap();

    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.git_scan_next_request_id = 1;
    app.git_scan_active_request_id = 1;
    app.git_scan_in_flight_request_id = Some(1);
    app.git_scan_root_cache = Some(GitScanRootCacheEntry {
        key: super::git_scan_root_cache_key(
            &root,
            GitAutoRepositoryDetection::SubFolders,
            2,
            &[],
            &[],
        ),
        scan_root: root.join("old-repo"),
    });
    app.source_control_selected = 3;

    assert!(crate::ui_event_channel::send_ui_event(
        &app.tx,
        UiEvent::GitScanned {
            request_id: 1,
            root: root.clone(),
            scan_root: None,
            root_cache_entry: None,
            git: kuroya_core::GitSnapshot::default(),
        }
    ));

    assert_eq!(app.handle_events(), 1);
    assert_eq!(app.git_scan_root_cache, None);
    assert_eq!(app.source_control_selected, 0);

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_repository_ignored_matches_absolute_relative_and_name_entries() {
    let root = std::env::temp_dir()
        .join(format!("kuroya-ignore-repo-{}", std::process::id()))
        .join("workspace");
    std::fs::create_dir_all(&root).unwrap();

    assert!(git_repository_ignored(&root, &[root.display().to_string()]));
    assert!(git_repository_ignored(&root, &[".".to_owned()]));
    assert!(git_repository_ignored(&root, &["workspace".to_owned()]));
    assert!(!git_repository_ignored(&root, &["other".to_owned()]));

    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_repository_scan_ignored_folders_match_named_relative_and_absolute_folders() {
    let root = std::env::temp_dir()
        .join(format!("kuroya-scan-ignore-{}", std::process::id()))
        .join("workspace")
        .join("node_modules")
        .join("pkg");
    std::fs::create_dir_all(&root).unwrap();

    assert!(git_repository_scan_folder_ignored(
        &root,
        &["node_modules".to_owned()]
    ));
    assert!(git_repository_scan_folder_ignored(
        &root,
        &["workspace/node_modules".to_owned()]
    ));
    assert!(git_repository_scan_folder_ignored(
        &root,
        &["packages/../node_modules".to_owned()]
    ));
    assert!(git_repository_scan_folder_ignored(
        &root,
        &[root.parent().unwrap().display().to_string()]
    ));
    assert!(!git_repository_scan_folder_ignored(
        &root,
        &["modules".to_owned()]
    ));
    assert!(!git_repository_scan_folder_ignored(
        &root,
        &["../somewhere".to_owned()]
    ));

    std::fs::remove_dir_all(root.ancestors().nth(3).unwrap()).unwrap();
}

#[test]
fn workspace_plugins_follow_workspace_trust() {
    assert!(workspace_plugins_enabled(true));
    assert!(!workspace_plugins_enabled(false));
    assert_eq!(
        workspace_plugins_restricted_status(),
        "Trust this workspace to enable workspace plugins"
    );
}

#[test]
fn spawn_plugin_discovery_skips_and_clears_when_plugins_setting_disabled() {
    let root = temp_workspace("kuroya-plugins-setting-disabled");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.settings.plugins.enabled = false;
    app.plugins.push(test_plugin_descriptor("stale.plugin"));
    app.plugin_errors.push(PluginDiscoveryError {
        root: root.clone(),
        error: "stale".to_owned(),
    });

    assert!(!app.spawn_plugin_discovery());
    assert!(app.plugins.is_empty());
    assert!(app.plugin_errors.is_empty());
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert_eq!(app.status, plugins_disabled_status());

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn disabled_plugin_ids_filter_discovered_descriptors() {
    let mut plugins = vec![
        test_plugin_descriptor("alpha.plugin"),
        test_plugin_descriptor("beta.plugin"),
        test_plugin_descriptor("gamma.plugin"),
    ];

    let removed = filter_disabled_workspace_plugins(
        &mut plugins,
        &["beta.plugin".to_owned(), "missing.plugin".to_owned()],
    );

    assert_eq!(removed, 1);
    assert_eq!(
        plugins
            .iter()
            .map(|plugin| plugin.manifest.id.as_str())
            .collect::<Vec<_>>(),
        ["alpha.plugin", "gamma.plugin"]
    );
    assert_eq!(filter_disabled_workspace_plugins(&mut plugins, &[]), 0);
    assert_eq!(
        filter_disabled_workspace_plugins(&mut plugins, &["alpha.plugin".to_owned()]),
        1
    );
}

#[test]
fn sync_plugin_settings_state_flips_both_directions_without_duplicate_spawns() {
    let root = temp_workspace("kuroya-plugins-sync-toggle");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);

    app.sync_plugin_settings_state();
    assert_eq!(app.workspace_plugins_next_request_id, 1);
    assert_eq!(app.workspace_plugins_active_request_id, 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, Some(1));

    app.sync_plugin_settings_state();
    assert_eq!(app.workspace_plugins_next_request_id, 1);
    assert_eq!(app.workspace_plugins_in_flight_request_id, Some(1));

    app.plugins.push(test_plugin_descriptor("loaded.plugin"));
    app.settings.plugins.enabled = false;
    app.sync_plugin_settings_state();
    assert!(app.plugins.is_empty());
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert_eq!(app.status, plugins_disabled_status());

    let next_after_disable = app.workspace_plugins_next_request_id;
    app.sync_plugin_settings_state();
    assert_eq!(app.workspace_plugins_next_request_id, next_after_disable);
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert_eq!(app.status, plugins_disabled_status());

    app.settings.plugins.enabled = true;
    app.sync_plugin_settings_state();
    assert_eq!(
        app.workspace_plugins_next_request_id,
        next_after_disable + 1
    );
    assert_eq!(
        app.workspace_plugins_in_flight_request_id,
        Some(next_after_disable + 1)
    );

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn sync_plugin_settings_state_defers_to_workspace_trust() {
    let root = temp_workspace("kuroya-plugins-sync-untrusted");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.plugins.push(test_plugin_descriptor("trusted.plugin"));

    app.workspace_trusted = false;
    app.sync_plugin_settings_state();

    assert!(app.plugins.is_empty());
    assert_eq!(app.workspace_plugins_in_flight_request_id, None);
    assert_eq!(app.status, workspace_plugins_restricted_status());

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

fn test_plugin_descriptor(id: &str) -> PluginDescriptor {
    PluginDescriptor {
        root: PathBuf::from(format!(".kuroya/plugins/{id}")),
        manifest: PluginManifest {
            api_version: "1".to_owned(),
            id: id.to_owned(),
            name: id.to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities::default(),
            contributes: PluginContributions::default(),
        },
    }
}

fn temp_workspace(name: &str) -> PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("{name}-{}-{suffix}", std::process::id()))
        .join("workspace")
}

#[cfg(unix)]
fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[test]
fn git_scoped_refresh_is_supported_only_for_scopable_snapshots_and_paths() {
    let workspace = Path::new("kuroya-scoped-guard/workspace");
    let inside = PathBuf::from("kuroya-scoped-guard/workspace/src/main.rs");
    let outside = PathBuf::from("kuroya-scoped-guard/other/src/lib.rs");
    let max = GIT_SCOPED_REFRESH_MAX_PATHS;

    // Git disabled falls back to the full scan (which invalidates).
    assert!(!git_scoped_refresh_is_supported(
        false,
        Some(workspace),
        Some(workspace),
        std::slice::from_ref(&inside),
        max
    ));
    // A snapshot without a repository falls back.
    assert!(!git_scoped_refresh_is_supported(
        true,
        None,
        Some(workspace),
        std::slice::from_ref(&inside),
        max
    ));
    // A resolved scan root that drifted from the snapshot root falls back.
    assert!(!git_scoped_refresh_is_supported(
        true,
        Some(workspace),
        Some(Path::new("kuroya-scoped-guard/workspace/nested")),
        std::slice::from_ref(&inside),
        max
    ));
    // No resolved scan root (auto detection disabled or no repository
    // found) falls back.
    assert!(!git_scoped_refresh_is_supported(
        true,
        Some(workspace),
        None,
        std::slice::from_ref(&inside),
        max
    ));
    // Empty and oversized batches fall back.
    assert!(!git_scoped_refresh_is_supported(
        true,
        Some(workspace),
        Some(workspace),
        &[],
        max
    ));
    let oversized = vec![inside.clone(); max + 1];
    assert!(!git_scoped_refresh_is_supported(
        true,
        Some(workspace),
        Some(workspace),
        &oversized,
        max
    ));
    // Paths outside the snapshot root fall back.
    assert!(!git_scoped_refresh_is_supported(
        true,
        Some(workspace),
        Some(workspace),
        std::slice::from_ref(&outside),
        max
    ));

    // The supported case, including a trailing separator on the snapshot
    // root (libgit2 workdirs keep one).
    assert!(git_scoped_refresh_is_supported(
        true,
        Some(workspace),
        Some(workspace),
        std::slice::from_ref(&inside),
        max
    ));
    assert!(git_scoped_refresh_is_supported(
        true,
        Some(Path::new("kuroya-scoped-guard/workspace/")),
        Some(workspace),
        std::slice::from_ref(&inside),
        max
    ));
}

#[test]
fn spawn_git_scoped_refresh_falls_back_to_full_scan_without_a_repository() {
    let root = temp_workspace("kuroya-scoped-refresh-fallback");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);

    // The snapshot has no repository yet, so the scoped refresh falls back
    // to the full scan request machinery.
    assert!(app.spawn_git_scoped_refresh(vec![root.join("src/main.rs")]));
    assert_eq!(app.git_scan_active_request_id, 1);
    assert_eq!(app.git_scan_in_flight_request_id, Some(1));

    // A refresh while a scan is in flight coalesces like full scans do.
    assert!(!app.spawn_git_scoped_refresh(vec![root.join("src/main.rs")]));
    assert!(app.git_scan_refresh_queued);

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn spawn_git_scoped_refresh_falls_back_when_git_is_disabled() {
    let root = temp_workspace("kuroya-scoped-refresh-disabled");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.settings.git_enabled = false;

    assert!(!app.spawn_git_scoped_refresh(vec![root.join("src/main.rs")]));
    assert_eq!(app.git_scan_in_flight_request_id, None);

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_refresh_for_changed_paths_gates_on_autorefresh_and_batch_size() {
    let root = temp_workspace("kuroya-changed-paths-refresh");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);

    // Autorefresh off: no refresh at all.
    app.settings.git_autorefresh = false;
    assert!(!app.spawn_git_refresh_for_changed_paths(vec![root.join("src/main.rs")]));
    assert_eq!(app.git_scan_in_flight_request_id, None);

    // Autorefresh on with a small batch: the scoped refresh falls back to
    // the full scan while the snapshot has no repository.
    app.settings.git_autorefresh = true;
    assert!(app.spawn_git_refresh_for_changed_paths(vec![root.join("src/main.rs")]));
    assert_eq!(app.git_scan_in_flight_request_id, Some(1));

    // Oversized batches skip the scoped attempt entirely and still refresh.
    app.invalidate_git_scan_requests();
    let oversized = vec![root.join("src/main.rs"); GIT_SCOPED_REFRESH_MAX_PATHS + 1];
    assert!(app.spawn_git_refresh_for_changed_paths(oversized));
    assert!(app.git_scan_in_flight_request_id.is_some());

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn git_refresh_for_saved_path_scopes_paths_inside_the_workspace() {
    let root = temp_workspace("kuroya-saved-path-refresh");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.settings.git_autorefresh = true;

    // Inside the workspace: the scoped refresh starts a git request (it
    // falls back to the full scan while the snapshot has no repository).
    assert!(app.spawn_git_refresh_for_saved_path(&root.join("src/main.rs")));
    assert_eq!(app.git_scan_in_flight_request_id, Some(1));

    // Outside the workspace: fall back to the full auto refresh.
    app.invalidate_git_scan_requests();
    assert!(app.spawn_git_refresh_for_saved_path(&PathBuf::from("kuroya-elsewhere/other.rs")));
    assert!(app.git_scan_in_flight_request_id.is_some());

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn pending_workspace_refresh_feeds_project_paths_to_the_git_refresh() {
    let root = temp_workspace("kuroya-flush-git-refresh");
    fs::create_dir_all(&root).unwrap();
    let mut app = crate::source_control_runtime::source_control_app_for_test(root.clone(), true);
    app.settings.git_autorefresh = true;
    app.schedule_workspace_refresh_with_paths(vec![root.join("src/main.rs")]);

    // Not due yet: nothing runs.
    assert_eq!(app.flush_pending_workspace_refresh(), 0);

    // Force the debounce window to elapse; the flush applies the index
    // update and the git refresh (1) from the same batch.
    if let Some(pending) = app.pending_workspace_refresh.as_mut() {
        pending.first_seen -= WORKSPACE_REFRESH_MAX_WAIT + Duration::from_secs(1);
        pending.last_seen = pending.first_seen;
    }
    assert_eq!(app.flush_pending_workspace_refresh(), 2);

    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn workspace_refresh_batch_worth_patching_compares_batch_to_index_size() {
    use super::workspace_refresh_batch_worth_patching;

    // Cold index: patching has nothing to patch against.
    assert!(!workspace_refresh_batch_worth_patching(10, 0));
    // Small batches stay incremental even on small indexes.
    assert!(workspace_refresh_batch_worth_patching(64, 10_000));
    // A batch approaching an eighth of the index is cheaper to re-walk.
    assert!(workspace_refresh_batch_worth_patching(120, 1_000));
    assert!(!workspace_refresh_batch_worth_patching(130, 1_000));
    // The patching cap alone bounds batches on huge indexes.
    assert!(workspace_refresh_batch_worth_patching(512, 1_000_000));
}

#[test]
fn workspace_refresh_incremental_gate_accepts_batches_within_the_raised_cap() {
    use super::{
        WORKSPACE_INCREMENTAL_CHUNK_PATHS, WORKSPACE_INCREMENTAL_INDEX_MAX_PATHS,
        workspace_refresh_paths_are_incremental,
    };

    let root = std::path::Path::new("C:/ws");
    let batch: Vec<std::path::PathBuf> = (0..WORKSPACE_INCREMENTAL_INDEX_MAX_PATHS)
        .map(|index| root.join(format!("src/file_{index}.rs")))
        .collect();

    assert!(workspace_refresh_paths_are_incremental(true, root, &batch));
    // One path past the cap re-walks.
    let mut over = batch.clone();
    over.push(root.join("src/overflow.rs"));
    assert!(!workspace_refresh_paths_are_incremental(true, root, &over));
    // Chunking must cover the whole batch.
    assert_eq!(
        WORKSPACE_INCREMENTAL_INDEX_MAX_PATHS,
        WORKSPACE_INCREMENTAL_CHUNK_PATHS * 8
    );
}

use crate::{
    explorer::{ExplorerEntryKind, path_matches_kind, retarget_path_prefix, workspace_child_path},
    explorer_delete_dialog::explorer_delete_requires_confirmation,
    explorer_fs_runtime::{explorer_fs_metadata_matches_kind, explorer_fs_path_is_workspace_child},
    explorer_rows::{
        explorer_entry_accessibility_label, explorer_entry_display_name,
        explorer_git_decoration_for_path, explorer_git_status_for_path,
    },
    explorer_runtime::{
        clear_deleted_revealed_path, explorer_ancestor_paths, explorer_entry_visible_for,
        explorer_operation_error_detail, explorer_operation_path_label, retarget_revealed_path,
    },
    explorer_tree_panel::{
        ExplorerDirectoryEntries, ExplorerDirectorySnapshot, explorer_context_path_known_openable,
        explorer_file_compare_context_action_labels,
        explorer_file_source_control_context_action_labels, explorer_parent_entry_index,
        explorer_selected_entry_index,
    },
};

use kuroya_core::{GitChangeStage, GitFileStatus, GitStatusEntry, ProjectEntry, TextBuffer};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

fn explorer_app_for_test(root: PathBuf) -> crate::KuroyaApp {
    let (tx, rx) = crate::ui_event_channel::ui_event_channel();
    let settings = kuroya_core::EditorSettings::default();
    crate::KuroyaApp::from_startup_context(crate::app_startup_context::AppStartupContext {
        runtime: tokio::runtime::Runtime::new().expect("test runtime"),
        tx,
        rx,
        workspace: kuroya_core::Workspace::new(root.clone()),
        settings: settings.clone(),
        settings_panel_draft: settings,
        settings_editor_font_path: String::new(),
        settings_ui_font_path: String::new(),
        theme_picker_selected: 0,
        saved_session: None,
        terminal: crate::terminal::TerminalPane::new(root.clone(), 100, 12.0, 1.2),
        watcher: None,
        recent_projects: Vec::new(),
        trusted_workspaces: vec![root],
        now: std::time::Instant::now(),
        startup_timings: Vec::new(),
    })
}

#[test]
fn explorer_directory_loaded_from_stale_workspace_is_dropped() {
    let root = PathBuf::from("workspace");
    let mut app = explorer_app_for_test(root.clone());
    let stale_generation = app.workspace_event_generation;
    app.workspace_event_generation = stale_generation.wrapping_add(1);
    let directory = root.join("src");
    app.explorer_directory_cache.insert(
        directory.clone(),
        ExplorerDirectorySnapshot::Loading {
            request_token: 7,
            previous: None,
        },
    );

    assert!(!app.apply_explorer_directory_loaded_event(
        &root,
        stale_generation,
        7,
        &directory,
        ExplorerDirectoryEntries::default(),
    ));
    assert!(matches!(
        app.explorer_directory_cache.get(&directory),
        Some(ExplorerDirectorySnapshot::Loading {
            request_token: 7,
            ..
        })
    ));

    let mut loaded = ExplorerDirectoryEntries::default();
    loaded
        .entries
        .push(explorer_entry(&directory, "main.rs", false));
    assert!(app.apply_explorer_directory_loaded_event(
        &root,
        app.workspace_event_generation,
        7,
        &directory,
        loaded,
    ));
    match app.explorer_directory_cache.get(&directory) {
        Some(ExplorerDirectorySnapshot::Ready(entries)) => {
            assert_eq!(entries.entries.len(), 1);
            assert_eq!(entries.error, None);
        }
        _ => panic!("loaded directory should be ready"),
    }
}

#[test]
fn workspace_reset_clears_explorer_directory_cache() {
    let root = PathBuf::from("workspace");
    let mut app = explorer_app_for_test(root.clone());
    let directory = root.join("src");
    app.explorer_directory_cache.insert(
        directory.clone(),
        ExplorerDirectorySnapshot::Ready(ExplorerDirectoryEntries::default()),
    );

    app.reset_open_workspace_state();

    assert!(app.explorer_directory_cache.is_empty());
}

#[test]
fn explorer_delete_confirmation_respects_setting_and_falls_back_safely() {
    let root = Path::new("workspace");
    let path = root.join("src/main.rs");

    assert!(explorer_delete_requires_confirmation(root, &path, true));
    assert!(explorer_delete_requires_confirmation(root, &path, false));
}

#[test]
fn explorer_child_paths_stay_inside_workspace() {
    let root = PathBuf::from("workspace");
    let parent = root.join("src");

    assert_eq!(
        workspace_child_path(&root, &parent, "main.rs").unwrap(),
        root.join("src").join("main.rs")
    );
    assert_eq!(
        workspace_child_path(&root, &parent, "./nested/mod.rs").unwrap(),
        root.join("src").join("nested").join("mod.rs")
    );

    assert!(workspace_child_path(&root, &parent, "../Cargo.toml").is_err());
    assert!(workspace_child_path(&root, &parent, ".").is_err());
    assert!(workspace_child_path(&root, &parent, "./").is_err());
    assert!(workspace_child_path(&root, &parent, "nested/..").is_err());
    assert!(workspace_child_path(&root, &parent, "nested/../main.rs").is_err());
    assert!(workspace_child_path(&root, &root.join("src").join(".."), "main.rs").is_ok());
    assert!(workspace_child_path(&root, &root.join("..").join("outside"), "main.rs").is_err());
    assert!(workspace_child_path(&root, Path::new("other"), "main.rs").is_err());
    assert!(workspace_child_path(&root, &parent, "").is_err());

    assert_eq!(
        workspace_child_path(Path::new("."), Path::new("."), "src/main.rs").unwrap(),
        PathBuf::from("src").join("main.rs")
    );
    assert!(workspace_child_path(Path::new("."), Path::new("."), "../main.rs").is_err());
}

#[test]
fn explorer_fs_runtime_preflight_rejects_workspace_escape_paths() {
    let root = PathBuf::from("workspace");

    assert!(explorer_fs_path_is_workspace_child(
        &root,
        &root.join("src/main.rs")
    ));
    assert!(explorer_fs_path_is_workspace_child(
        &root,
        &root.join("src/../src/main.rs")
    ));
    assert!(!explorer_fs_path_is_workspace_child(
        &root,
        &root.join("../outside/main.rs")
    ));
    assert!(!explorer_fs_path_is_workspace_child(&root, &root));
    assert!(!explorer_fs_path_is_workspace_child(
        &root,
        Path::new("other/main.rs")
    ));
}

#[test]
fn explorer_fs_runtime_preflight_detects_stale_file_kind() {
    let root = std::env::temp_dir().join(format!("kuroya-explorer-kind-{}", std::process::id()));
    let file = root.join("file.txt");
    let folder = root.join("folder");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(&file, b"hello").unwrap();

    let file_metadata = std::fs::metadata(&file).unwrap();
    let folder_metadata = std::fs::metadata(&folder).unwrap();
    assert!(explorer_fs_metadata_matches_kind(
        &file_metadata,
        ExplorerEntryKind::File
    ));
    assert!(!explorer_fs_metadata_matches_kind(
        &file_metadata,
        ExplorerEntryKind::Folder
    ));
    assert!(explorer_fs_metadata_matches_kind(
        &folder_metadata,
        ExplorerEntryKind::Folder
    ));
    assert!(!explorer_fs_metadata_matches_kind(
        &folder_metadata,
        ExplorerEntryKind::File
    ));

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn explorer_reveal_expands_all_ancestor_folders() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("nested").join("main.rs");

    assert_eq!(
        explorer_ancestor_paths(&root, &path),
        vec![root.join("src"), root.join("src").join("nested")]
    );
    assert_eq!(
        explorer_ancestor_paths(
            &root,
            &root
                .join("src")
                .join("..")
                .join("src")
                .join("nested")
                .join("main.rs")
        ),
        vec![root.join("src"), root.join("src").join("nested")]
    );
    assert!(
        explorer_ancestor_paths(&root, &root.join("src").join("..").join("README.md")).is_empty()
    );
    assert!(explorer_ancestor_paths(&root, &root.join("README.md")).is_empty());
    assert!(explorer_ancestor_paths(&root, Path::new("other/main.rs")).is_empty());
}

#[cfg(windows)]
#[test]
fn explorer_reveal_helpers_match_windows_paths_case_insensitively() {
    let root = PathBuf::from(r"C:\Repo\Project");
    let path = PathBuf::from(r"c:\repo\project\src\nested\main.rs");

    assert_eq!(
        explorer_ancestor_paths(&root, &path),
        vec![root.join("src"), root.join("src").join("nested")]
    );

    let expanded = HashSet::from([root.join("src")]);
    assert!(!explorer_entry_visible_for(&root, &expanded, &path));

    let expanded = HashSet::from([root.join("src"), root.join("src").join("nested")]);
    assert!(explorer_entry_visible_for(&root, &expanded, &path));
}

#[test]
fn explorer_entry_visibility_follows_expanded_ancestors() {
    let root = PathBuf::from("workspace");
    let src = root.join("src");
    let nested = src.join("nested");
    let nested_file = nested.join("main.rs");
    let root_file = root.join("README.md");
    let mut expanded = HashSet::new();

    assert!(explorer_entry_visible_for(&root, &expanded, &root_file));
    assert!(explorer_entry_visible_for(&root, &expanded, &src));
    assert!(!explorer_entry_visible_for(&root, &expanded, &nested_file));
    assert!(explorer_entry_visible_for(
        &root,
        &expanded,
        &root.join("src").join("..").join("README.md")
    ));

    expanded.insert(src);
    assert!(!explorer_entry_visible_for(&root, &expanded, &nested_file));

    expanded.insert(nested);
    assert!(explorer_entry_visible_for(&root, &expanded, &nested_file));
}

#[test]
fn explorer_selection_helpers_prefer_revealed_then_active_paths() {
    let root = PathBuf::from("workspace");
    let entries = vec![
        explorer_entry(&root, "README.md", false),
        explorer_entry(&root, "src", true),
        explorer_entry(&root, "src/main.rs", false),
    ];

    assert_eq!(
        explorer_selected_entry_index(
            &entries,
            Some(&root.join("src/main.rs")),
            Some(&root.join("README.md")),
        ),
        Some(2)
    );
    assert_eq!(
        explorer_selected_entry_index(&entries, None, Some(&root.join("README.md"))),
        Some(0)
    );
    assert_eq!(
        explorer_selected_entry_index(
            &entries,
            Some(&root.join("src").join("..").join("src").join("main.rs")),
            None
        ),
        Some(2)
    );
    assert_eq!(
        explorer_selected_entry_index(&entries, Some(Path::new("missing.rs")), None),
        None
    );
    assert_eq!(
        explorer_parent_entry_index(&entries, &root.join("src/main.rs")),
        Some(1)
    );
    assert_eq!(
        explorer_parent_entry_index(
            &entries,
            &root.join("src").join("..").join("src").join("main.rs")
        ),
        Some(1)
    );
}

#[test]
fn explorer_path_prefix_helpers_retarget_children() {
    let old = PathBuf::from("workspace/src");
    let new = PathBuf::from("workspace/crates/app/src");

    assert_eq!(
        retarget_path_prefix(Path::new("workspace/src/main.rs"), &old, &new),
        Some(PathBuf::from("workspace/crates/app/src/main.rs"))
    );
    assert_eq!(
        retarget_path_prefix(Path::new("workspace/src"), &old, &new),
        Some(new.clone())
    );
    assert_eq!(
        retarget_path_prefix(Path::new("workspace/src/../src/nested/main.rs"), &old, &new),
        Some(PathBuf::from("workspace/crates/app/src/nested/main.rs"))
    );
    assert_eq!(
        retarget_path_prefix(Path::new("workspace/tests/main.rs"), &old, &new),
        None
    );
    assert_eq!(
        retarget_path_prefix(Path::new("workspace/src/../tests/main.rs"), &old, &new),
        None
    );
    assert_eq!(
        retarget_path_prefix(Path::new("workspace/src-lib/main.rs"), &old, &new),
        None
    );

    assert!(path_matches_kind(
        Path::new("workspace/src/../src/main.rs"),
        Path::new("workspace/src/main.rs"),
        ExplorerEntryKind::File
    ));
    assert!(path_matches_kind(
        Path::new("workspace/src/main.rs"),
        Path::new("workspace/src"),
        ExplorerEntryKind::Folder
    ));
    assert!(path_matches_kind(
        Path::new("workspace/src/../src/nested/main.rs"),
        Path::new("workspace/src"),
        ExplorerEntryKind::Folder
    ));
    assert!(!path_matches_kind(
        Path::new("workspace/src/main.rs"),
        Path::new("workspace/src"),
        ExplorerEntryKind::File
    ));
    assert!(!path_matches_kind(
        Path::new("workspace/src-lib/main.rs"),
        Path::new("workspace/src"),
        ExplorerEntryKind::Folder
    ));
}

#[cfg(windows)]
#[test]
fn explorer_path_prefix_helpers_match_windows_case_insensitively() {
    assert_eq!(
        retarget_path_prefix(
            Path::new(r"c:\repo\project\src\nested\main.rs"),
            Path::new(r"C:\Repo\Project\src"),
            Path::new(r"C:\Repo\Project\renamed"),
        ),
        Some(PathBuf::from(r"C:\Repo\Project\renamed\nested\main.rs"))
    );

    assert!(path_matches_kind(
        Path::new(r"c:\repo\project\src\main.rs"),
        Path::new(r"C:\Repo\Project\src\main.rs"),
        ExplorerEntryKind::File
    ));
    assert!(path_matches_kind(
        Path::new(r"c:\repo\project\src\nested\main.rs"),
        Path::new(r"C:\Repo\Project\src"),
        ExplorerEntryKind::Folder
    ));
}

#[test]
fn explorer_revealed_path_tracks_renamed_files_and_folders() {
    let root = PathBuf::from("workspace");
    let old_file = root.join("src/main.rs");
    let new_file = root.join("src/lib.rs");
    let mut revealed = Some(old_file.clone());

    retarget_revealed_path(&mut revealed, &old_file, &new_file, ExplorerEntryKind::File);
    assert_eq!(revealed, Some(new_file));

    let old_folder = root.join("src");
    let new_folder = root.join("crates/app/src");
    let mut revealed = Some(old_folder.join("nested/mod.rs"));

    retarget_revealed_path(
        &mut revealed,
        &old_folder,
        &new_folder,
        ExplorerEntryKind::Folder,
    );
    assert_eq!(revealed, Some(new_folder.join("nested/mod.rs")));
}

#[test]
fn explorer_revealed_path_ignores_unrelated_renames_and_clears_deleted_targets() {
    let root = PathBuf::from("workspace");
    let revealed_file = root.join("src/main.rs");
    let mut revealed = Some(revealed_file.clone());

    retarget_revealed_path(
        &mut revealed,
        &root.join("tests/main.rs"),
        &root.join("tests/lib.rs"),
        ExplorerEntryKind::File,
    );
    assert_eq!(revealed, Some(revealed_file.clone()));

    clear_deleted_revealed_path(
        &mut revealed,
        &root.join("src/generated"),
        ExplorerEntryKind::Folder,
    );
    assert_eq!(revealed, Some(revealed_file.clone()));

    clear_deleted_revealed_path(&mut revealed, &root.join("src"), ExplorerEntryKind::Folder);
    assert_eq!(revealed, None);
}

fn explorer_entry(root: &Path, relative: &str, is_dir: bool) -> ProjectEntry {
    let relative_path = PathBuf::from(relative);
    ProjectEntry {
        path: root.join(&relative_path),
        depth: relative_path.components().count().saturating_sub(1),
        relative_path,
        is_dir,
        ..Default::default()
    }
}

#[test]
fn explorer_git_decorations_cover_changed_files_and_folders() {
    let root = PathBuf::from("workspace");
    let entries = vec![
        GitStatusEntry {
            path: root.join("src").join("..").join("src").join("main.rs"),
            status: GitFileStatus::Modified,
            stage: GitChangeStage::Unstaged,
        },
        GitStatusEntry {
            path: root
                .join("src")
                .join("..")
                .join("src")
                .join("nested")
                .join("lib.rs"),
            status: GitFileStatus::Conflicted,
            stage: GitChangeStage::Unstaged,
        },
        GitStatusEntry {
            path: root.join("README.md"),
            status: GitFileStatus::Untracked,
            stage: GitChangeStage::Unstaged,
        },
    ];

    assert_eq!(
        explorer_git_status_for_path(&root.join("src/main.rs"), false, &entries),
        Some(GitFileStatus::Modified)
    );
    assert_eq!(
        explorer_git_status_for_path(&root.join("src"), true, &entries),
        Some(GitFileStatus::Conflicted)
    );
    assert_eq!(
        explorer_git_status_for_path(&root.join("target"), true, &entries),
        None
    );

    let decoration =
        explorer_git_decoration_for_path(&root.join("src"), true, &entries, true).unwrap();
    assert_eq!(decoration.marker, "!");
    assert_eq!(decoration.label, "Conflicted");
    assert_eq!(
        explorer_git_decoration_for_path(&root.join("src"), true, &entries, false),
        None
    );
}

#[test]
fn explorer_git_decorations_do_not_inherit_when_path_is_a_file() {
    let root = PathBuf::from("workspace");
    let entries = vec![GitStatusEntry {
        path: root.join("src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    }];

    assert_eq!(
        explorer_git_status_for_path(&root.join("src"), false, &entries),
        None
    );
}

#[test]
fn explorer_git_decorations_do_not_match_sibling_prefixes() {
    let root = PathBuf::from("workspace");
    let entries = vec![GitStatusEntry {
        path: root.join("src-lib").join("main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    }];

    assert_eq!(
        explorer_git_status_for_path(&root.join("src"), true, &entries),
        None
    );
}

#[test]
fn explorer_entry_accessibility_label_describes_file_and_git_state() {
    let root = PathBuf::from("workspace");
    let entries = vec![GitStatusEntry {
        path: root.join("src/main.rs"),
        status: GitFileStatus::Added,
        stage: GitChangeStage::Unstaged,
    }];
    let decoration =
        explorer_git_decoration_for_path(&root.join("src/main.rs"), false, &entries, true);

    assert_eq!(
        explorer_entry_accessibility_label("main.rs", false, false, decoration),
        "File main.rs, Added"
    );
    assert_eq!(
        explorer_entry_accessibility_label("README.md", false, false, None),
        "File README.md"
    );
}

#[test]
fn explorer_entry_accessibility_label_describes_folder_expansion() {
    let root = PathBuf::from("workspace");
    let entries = vec![GitStatusEntry {
        path: root.join("src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    }];
    let decoration = explorer_git_decoration_for_path(&root.join("src"), true, &entries, true);

    assert_eq!(
        explorer_entry_accessibility_label("src", true, true, decoration),
        "Folder src, expanded, Modified"
    );
    assert_eq!(
        explorer_entry_accessibility_label("tests", true, false, None),
        "Folder tests, collapsed"
    );
}

#[test]
fn explorer_display_names_are_single_line_and_bounded() {
    let display =
        explorer_entry_display_name(&format!("bad\n\u{202e}name\u{2028}.rs{}", "x".repeat(220)));

    assert!(display.starts_with("bad name .rs"));
    assert!(display.contains("..."));
    assert!(!display.contains('\n'));
    assert!(!display.contains('\u{202e}'));
    assert!(!display.contains('\u{2028}'));
    assert!(display.chars().count() <= 163);
}

#[test]
fn explorer_operation_path_labels_are_single_line_and_bounded() {
    let path = PathBuf::from(format!(
        "workspace/bad\n\u{202e}name\u{2028}.rs{}",
        "x".repeat(220)
    ));

    let display = explorer_operation_path_label(&path);

    assert!(display.starts_with("bad name .rs"));
    assert!(display.contains("..."));
    assert!(!display.contains('\n'));
    assert!(!display.contains('\u{202e}'));
    assert!(!display.contains('\u{2028}'));
    assert!(display.chars().count() <= 160);
}

#[test]
fn explorer_operation_error_details_are_single_line_and_bounded() {
    let detail = explorer_operation_error_detail(&format!(
        "failed\nbecause \u{202e}target\u{2029} disappeared {}",
        "x".repeat(260)
    ));

    assert!(detail.starts_with("failed because"));
    assert!(detail.contains("target"));
    assert!(detail.contains("disappeared"));
    assert!(detail.contains("..."));
    assert!(!detail.contains('\n'));
    assert!(!detail.contains('\u{202e}'));
    assert!(!detail.contains('\u{2029}'));
    assert!(detail.chars().count() <= 240);
}

#[test]
fn explorer_context_actions_match_changed_file_stage() {
    assert_eq!(
        explorer_file_source_control_context_action_labels(
            Some(GitFileStatus::Modified),
            true,
            false,
        ),
        vec![
            "Open Changes",
            "Copy Patch",
            "Reveal in Source Control",
            "Compare with HEAD",
            "Open File at HEAD",
            "Open File at Index",
            "Open Hunks",
            "Stage Changes",
            "Discard Changes"
        ]
    );
    assert_eq!(
        explorer_file_source_control_context_action_labels(
            Some(GitFileStatus::Modified),
            false,
            true,
        ),
        vec![
            "Open Staged Changes",
            "Copy Staged Patch",
            "Reveal in Source Control",
            "Compare with HEAD",
            "Open File at HEAD",
            "Open File at Index",
            "Open Staged Hunks",
            "Unstage Changes",
            "Discard Changes"
        ]
    );
    assert_eq!(
        explorer_file_source_control_context_action_labels(
            Some(GitFileStatus::Modified),
            true,
            true,
        ),
        vec![
            "Open Changes",
            "Copy Patch",
            "Open Staged Changes",
            "Copy Staged Patch",
            "Reveal in Source Control",
            "Compare with HEAD",
            "Open File at HEAD",
            "Open File at Index",
            "Open Hunks",
            "Open Staged Hunks",
            "Stage Changes",
            "Unstage Changes",
            "Discard Changes"
        ]
    );
}

#[test]
fn explorer_context_actions_support_file_compare_flow() {
    assert_eq!(
        explorer_file_compare_context_action_labels(false, false, false),
        vec!["Select for Compare"]
    );
    assert_eq!(
        explorer_file_compare_context_action_labels(false, true, false),
        vec!["Select for Compare", "Compare with Selected"]
    );
    assert_eq!(
        explorer_file_compare_context_action_labels(false, true, true),
        vec!["Select for Compare"]
    );
    assert!(explorer_file_compare_context_action_labels(true, true, false).is_empty());
}

#[test]
fn explorer_context_path_openability_uses_index_before_filesystem_probe() {
    let indexed = vec![PathBuf::from("workspace/src/main.rs")];
    let indexed_path = PathBuf::from("workspace/src/main.rs");

    assert!(explorer_context_path_known_openable(
        &[],
        &indexed,
        &indexed_path,
        |_| panic!("indexed file should not probe the filesystem")
    ));

    let unknown_path = PathBuf::from("workspace/src/lib.rs");
    let mut probed = false;
    assert!(explorer_context_path_known_openable(
        &[],
        &indexed,
        &unknown_path,
        |_| {
            probed = true;
            true
        }
    ));
    assert!(probed);
}

#[test]
fn explorer_context_path_openability_uses_open_buffer_before_filesystem_probe() {
    let path = PathBuf::from("workspace/src/main.rs");
    let buffers = vec![TextBuffer::from_text(
        7,
        Some(path.clone()),
        "open\n".to_owned(),
    )];
    let mut probed = false;

    assert!(explorer_context_path_known_openable(
        &buffers,
        &[],
        &path,
        |_| {
            probed = true;
            false
        }
    ));
    assert!(!probed);
}

#[test]
fn explorer_context_actions_skip_hunks_for_conflicts() {
    assert_eq!(
        explorer_file_source_control_context_action_labels(
            Some(GitFileStatus::Conflicted),
            true,
            false,
        ),
        vec![
            "Open Changes",
            "Copy Patch",
            "Reveal in Source Control",
            "Compare with HEAD",
            "Open File at HEAD",
            "Stage Changes",
            "Discard Changes"
        ]
    );
    assert!(explorer_file_source_control_context_action_labels(None, false, false).is_empty());
    assert!(
        !explorer_file_source_control_context_action_labels(
            Some(GitFileStatus::Untracked),
            true,
            false,
        )
        .contains(&"Open File at HEAD")
    );
    assert!(
        !explorer_file_source_control_context_action_labels(
            Some(GitFileStatus::Untracked),
            true,
            false,
        )
        .contains(&"Open File at Index")
    );
}

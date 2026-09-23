use super::{
    DiffTabContextActionKind, TAB_ACTIONS_RESERVED_WIDTH, TAB_MAX_WIDTH, TAB_MIN_WIDTH,
    TabCapabilityRequests, TabPathCapabilities, TabRowPreparationCache, TabRowPreparationInput,
    TabSourceControlState, diff_tab_context_action_command, prepare_tab_path_capabilities,
    prepare_tab_row, responsive_tab_actions_width, responsive_tab_max_width,
    tab_action_still_targets_row, tab_path_exists_cached, tab_path_known_openable_cached,
    tab_source_control_states,
};
use crate::{
    KuroyaApp,
    app_startup_context::AppStartupContext,
    app_state::{PendingFileReload, QueuedFileReload},
    git_diff_state::DiffBufferSource,
    path_display::DISPLAY_PATH_LABEL_MAX_CHARS,
    terminal::TerminalPane,
};
use kuroya_core::{
    Command, EditorSettings, GitChangeStage, GitFileStatus, GitStatusEntry, TextBuffer, Workspace,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    time::Instant,
};
use tokio::runtime::Runtime;

#[test]
fn preparing_many_tab_rows_skips_filesystem_until_existence_capabilities_are_requested() {
    let selected_path = PathBuf::from("src/selected.rs");
    let row_probes = Cell::new(0usize);
    let rows: Vec<_> = (0..1024)
        .map(|index| {
            let path = PathBuf::from(format!("src/file_{index}.rs"));
            prepare_tab_row(
                TabRowPreparationInput {
                    id: index,
                    selected: index == 512,
                    name: format!("file_{index}.rs"),
                    dirty: index == 512,
                    read_only: false,
                    changed_on_disk: false,
                    file_path: Some(path.clone()),
                    context_path: Some(path),
                    diff_source: None,
                    has_unstaged_changes: false,
                    has_staged_changes: false,
                    source_control_status: None,
                },
                TabCapabilityRequests::ROW,
                |_| {
                    row_probes.set(row_probes.get() + 1);
                    true
                },
            )
        })
        .collect();

    assert_eq!(rows.len(), 1024);
    assert_eq!(row_probes.get(), 0);
    assert_eq!(rows[512].path_capabilities, TabPathCapabilities::default());

    let requested_probes = Cell::new(0usize);
    let mut path_exists = |path: &Path| {
        requested_probes.set(requested_probes.get() + 1);
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "file_512.rs" || name == "selected.rs")
    };
    let capabilities = prepare_tab_path_capabilities(
        &rows[512],
        TabCapabilityRequests::FILE_COMPARE_ACTIONS,
        Some(selected_path.as_path()),
        &mut path_exists,
    );

    assert!(capabilities.can_compare_with_saved);
    assert!(capabilities.can_select_for_compare);
    assert!(capabilities.can_compare_with_selected);
    assert_eq!(requested_probes.get(), 2);
}

#[test]
fn tab_path_capabilities_reuse_dirty_file_probe_for_selected_compare_path() {
    let file_path = PathBuf::from("src/main.rs");
    let context_path = PathBuf::from("src/renamed.rs");
    let row = prepare_tab_row(
        TabRowPreparationInput {
            id: 1,
            selected: true,
            name: "main.rs".to_owned(),
            dirty: true,
            read_only: false,
            changed_on_disk: false,
            file_path: Some(file_path.clone()),
            context_path: Some(context_path.clone()),
            diff_source: None,
            has_unstaged_changes: false,
            has_staged_changes: false,
            source_control_status: None,
        },
        TabCapabilityRequests::ROW,
        |_| panic!("row preparation should not probe paths"),
    );
    let probed = RefCell::new(Vec::new());
    let mut path_exists = |path: &Path| {
        probed.borrow_mut().push(path.to_path_buf());
        path == context_path || path == file_path
    };

    let capabilities = prepare_tab_path_capabilities(
        &row,
        TabCapabilityRequests::FILE_COMPARE_ACTIONS,
        Some(file_path.as_path()),
        &mut path_exists,
    );

    assert!(capabilities.can_compare_with_saved);
    assert!(capabilities.can_select_for_compare);
    assert!(capabilities.can_compare_with_selected);
    assert_eq!(
        probed.into_inner(),
        vec![context_path.clone(), file_path.clone()]
    );
}

#[test]
fn tab_path_capabilities_reuse_missing_dirty_file_probe_for_selected_compare_path() {
    let file_path = PathBuf::from("src/missing.rs");
    let context_path = PathBuf::from("src/renamed.rs");
    let row = prepare_tab_row(
        TabRowPreparationInput {
            id: 1,
            selected: true,
            name: "missing.rs".to_owned(),
            dirty: true,
            read_only: false,
            changed_on_disk: false,
            file_path: Some(file_path.clone()),
            context_path: Some(context_path.clone()),
            diff_source: None,
            has_unstaged_changes: false,
            has_staged_changes: false,
            source_control_status: None,
        },
        TabCapabilityRequests::ROW,
        |_| panic!("row preparation should not probe paths"),
    );
    let probed = RefCell::new(Vec::new());
    let mut path_exists = |path: &Path| {
        probed.borrow_mut().push(path.to_path_buf());
        path == context_path
    };

    let capabilities = prepare_tab_path_capabilities(
        &row,
        TabCapabilityRequests::FILE_COMPARE_ACTIONS,
        Some(file_path.as_path()),
        &mut path_exists,
    );

    assert!(!capabilities.can_compare_with_saved);
    assert!(capabilities.can_select_for_compare);
    assert!(!capabilities.can_compare_with_selected);
    assert_eq!(
        probed.into_inner(),
        vec![context_path.clone(), file_path.clone()]
    );
}

#[test]
fn tab_path_exists_cached_reuses_same_context_menu_probe_without_cloning_path() {
    let path = PathBuf::from("src/file.rs");
    let equivalent_path = PathBuf::from("src/file.rs");
    let selected = PathBuf::from("src/selected.rs");
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    assert!(tab_path_exists_cached(&mut cache, &path, |_| {
        probes.set(probes.get() + 1);
        true
    }));
    let cached_key = *cache.keys().next().expect("cache key");
    assert!(std::ptr::eq(cached_key, path.as_path()));

    assert!(tab_path_exists_cached(
        &mut cache,
        equivalent_path.as_path(),
        |_| {
            probes.set(probes.get() + 1);
            false
        }
    ));
    let cached_key = *cache.keys().next().expect("cache key");
    assert!(std::ptr::eq(cached_key, path.as_path()));
    assert_eq!(probes.get(), 1);

    assert!(tab_path_exists_cached(&mut cache, path.as_path(), |_| {
        probes.set(probes.get() + 1);
        false
    }));
    assert_eq!(probes.get(), 1);

    assert!(!tab_path_exists_cached(&mut cache, &selected, |_| {
        probes.set(probes.get() + 1);
        false
    }));
    assert_eq!(probes.get(), 2);
}

#[test]
fn tab_path_openability_cache_uses_index_before_filesystem_probe() {
    let indexed = vec![PathBuf::from("src/file.rs")];
    let path = PathBuf::from("src/file.rs");
    let selected = PathBuf::from("src/selected.rs");
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    assert!(tab_path_known_openable_cached(
        &mut cache,
        &[],
        &indexed,
        &path,
        |_| panic!("indexed file should not probe the filesystem")
    ));
    assert!(tab_path_known_openable_cached(
        &mut cache,
        &[],
        &indexed,
        path.as_path(),
        |_| {
            probes.set(probes.get() + 1);
            false
        }
    ));
    assert_eq!(probes.get(), 0);

    assert!(!tab_path_known_openable_cached(
        &mut cache,
        &[],
        &indexed,
        &selected,
        |_| {
            probes.set(probes.get() + 1);
            false
        }
    ));
    assert_eq!(probes.get(), 1);
}

#[test]
fn tab_path_openability_cache_uses_open_buffer_before_filesystem_probe() {
    let path = PathBuf::from("src/file.rs");
    let buffers = vec![TextBuffer::from_text(
        7,
        Some(path.clone()),
        "open\n".to_owned(),
    )];
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    assert!(tab_path_known_openable_cached(
        &mut cache,
        &buffers,
        &[],
        &path,
        |_| {
            probes.set(probes.get() + 1);
            false
        },
    ));

    assert_eq!(probes.get(), 0);
}

#[test]
fn tab_row_source_control_states_preserve_stage_flags_and_status_precedence() {
    let both_staged_first = PathBuf::from("src/main.rs");
    let both_unstaged_first = PathBuf::from("src/lib.rs");
    let entries = vec![
        GitStatusEntry {
            path: both_staged_first.clone(),
            status: GitFileStatus::Modified,
            stage: GitChangeStage::Staged,
        },
        GitStatusEntry {
            path: both_staged_first.clone(),
            status: GitFileStatus::Deleted,
            stage: GitChangeStage::Unstaged,
        },
        GitStatusEntry {
            path: both_unstaged_first.clone(),
            status: GitFileStatus::Untracked,
            stage: GitChangeStage::Unstaged,
        },
        GitStatusEntry {
            path: both_unstaged_first.clone(),
            status: GitFileStatus::Added,
            stage: GitChangeStage::Staged,
        },
    ];

    let states = tab_source_control_states(&entries);

    assert_eq!(states.len(), 2);
    let staged_first = states
        .get(both_staged_first.as_path())
        .expect("state for duplicate path");
    assert!(staged_first.has_staged_changes);
    assert!(staged_first.has_unstaged_changes);
    assert_eq!(staged_first.status, Some(GitFileStatus::Deleted));

    let unstaged_first = states
        .get(both_unstaged_first.as_path())
        .expect("state for duplicate path");
    assert!(unstaged_first.has_staged_changes);
    assert!(unstaged_first.has_unstaged_changes);
    assert_eq!(unstaged_first.status, Some(GitFileStatus::Untracked));
}

#[test]
fn tab_row_preparation_cache_reuses_source_control_state_map() {
    let path = PathBuf::from("src/main.rs");
    let entries = vec![GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    }];
    let mut cache = TabRowPreparationCache::default();

    let first = cache.source_control_state(&entries, &path);
    let second = cache.source_control_state(&[], &path);

    assert_eq!(second, first);
    assert_eq!(
        cache.source_control_state(&[], Path::new("src/missing.rs")),
        TabSourceControlState::default()
    );
}

#[test]
fn tab_row_preparation_cache_skips_source_control_map_for_empty_entries() {
    let path = PathBuf::from("src/main.rs");
    let mut cache = TabRowPreparationCache::default();

    assert_eq!(
        cache.source_control_state(&[], &path),
        TabSourceControlState::default()
    );
    assert!(cache.source_control.is_none());

    let entries = vec![GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    }];
    let state = cache.source_control_state(&entries, &path);

    assert!(state.has_unstaged_changes);
    assert_eq!(state.status, Some(GitFileStatus::Modified));
    assert!(cache.source_control.is_some());
}

#[test]
fn tab_row_preparation_cache_reuses_display_labels_for_matching_state() {
    let mut cache = TabRowPreparationCache::default();
    let raw_name = format!(
        "bad\nname\u{202e}{}",
        "x".repeat(DISPLAY_PATH_LABEL_MAX_CHARS * 2)
    );

    let first = cache.tab_row_display(&raw_name, true, true, false);
    let second = cache.tab_row_display(&raw_name, true, true, false);
    let clean = cache.tab_row_display(&raw_name, false, false, false);

    assert_eq!(first, second);
    assert_ne!(first.label, clean.label);
    assert_eq!(cache.displays.len(), 1);
    assert_eq!(cache.display_state_count(), 2);
    assert!(!first.name.contains('\n'));
    assert!(!first.name.contains('\u{202e}'));
    assert!(first.label.starts_with("! * "));
    assert!(first.label.contains("..."));
    assert!(first.label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
}

#[test]
fn tab_rows_reuse_display_labels_without_collapsing_raw_paths() {
    let root = PathBuf::from("workspace");
    let first_path = root.join("src/main.rs");
    let second_path = root.join("tests/main.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        1,
        Some(first_path.clone()),
        "first".to_owned(),
    ));
    app.buffers.push(TextBuffer::from_text(
        2,
        Some(second_path.clone()),
        "second".to_owned(),
    ));

    let mut cache = TabRowPreparationCache::default();
    let first_row = app.prepare_buffer_tab_row_with_cache(
        app.buffer(1).expect("buffer should exist"),
        &mut cache,
        &[],
    );
    let second_row = app.prepare_buffer_tab_row_with_cache(
        app.buffer(2).expect("buffer should exist"),
        &mut cache,
        &[],
    );

    assert_eq!(cache.displays.len(), 1);
    assert_eq!(first_row.name, "main.rs");
    assert_eq!(second_row.name, "main.rs");
    assert_eq!(first_row.label, second_row.label);
    assert_eq!(first_row.file_path.as_ref(), Some(&first_path));
    assert_eq!(second_row.file_path.as_ref(), Some(&second_path));
}

#[test]
fn prepared_tab_row_sanitizes_close_hover_name_and_preserves_label_markers() {
    let row = prepare_tab_row(
        TabRowPreparationInput {
            id: 1,
            selected: false,
            name: format!(
                "bad\nname\u{202e}{}",
                "x".repeat(DISPLAY_PATH_LABEL_MAX_CHARS * 2)
            ),
            dirty: true,
            read_only: true,
            changed_on_disk: true,
            file_path: None,
            context_path: None,
            diff_source: None,
            has_unstaged_changes: false,
            has_staged_changes: false,
            source_control_status: None,
        },
        TabCapabilityRequests::ROW,
        |_| panic!("row preparation should not probe paths"),
    );

    assert!(!row.name.contains('\n'));
    assert!(!row.name.contains('\u{202e}'));
    assert!(row.name.contains("..."));
    assert!(row.name.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    assert!(row.label.starts_with("! * RO "));
    assert!(row.label.contains("..."));
    assert!(row.label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
}

#[test]
fn prepared_tab_row_preserves_raw_paths_while_sanitizing_display_text() {
    let file_path = PathBuf::from("workspace").join(format!(
        "bad\nname\u{202e}{}.rs",
        "x".repeat(DISPLAY_PATH_LABEL_MAX_CHARS * 2)
    ));
    let context_path = PathBuf::from("workspace/src/raw.rs");
    let diff_source = DiffBufferSource {
        path: context_path.clone(),
        base_path: None,
        hunk_stage: Some(GitChangeStage::Unstaged),
        saved_buffer_id: None,
    };

    let row = prepare_tab_row(
        TabRowPreparationInput {
            id: 1,
            selected: true,
            name: "bad\nname\u{202e}".to_owned(),
            dirty: false,
            read_only: false,
            changed_on_disk: false,
            file_path: Some(file_path.clone()),
            context_path: Some(context_path.clone()),
            diff_source: Some(diff_source),
            has_unstaged_changes: true,
            has_staged_changes: false,
            source_control_status: Some(GitFileStatus::Modified),
        },
        TabCapabilityRequests::ROW,
        |_| panic!("row preparation should not probe paths"),
    );

    assert!(!row.name.contains('\n'));
    assert!(!row.name.contains('\u{202e}'));
    assert_eq!(row.file_path.as_ref(), Some(&file_path));
    assert_eq!(row.context_path.as_ref(), Some(&context_path));
    assert_eq!(
        row.diff_source.as_ref().map(|source| source.path.as_path()),
        Some(context_path.as_path())
    );
}

#[test]
fn tab_action_guard_requires_live_buffer_to_match_raw_row_target() {
    let raw_path = PathBuf::from("workspace/src/../src/main.rs");
    let normalized_path = PathBuf::from("workspace/src/main.rs");
    let row = prepare_tab_row(
        TabRowPreparationInput {
            id: 1,
            selected: true,
            name: "main.rs".to_owned(),
            dirty: false,
            read_only: false,
            changed_on_disk: false,
            file_path: Some(raw_path.clone()),
            context_path: Some(raw_path.clone()),
            diff_source: None,
            has_unstaged_changes: false,
            has_staged_changes: false,
            source_control_status: None,
        },
        TabCapabilityRequests::ROW,
        |_| panic!("row preparation should not probe paths"),
    );
    let exact = TextBuffer::from_text(1, Some(raw_path), "main".to_owned());
    let equivalent = TextBuffer::from_text(1, Some(normalized_path), "main".to_owned());
    let different_id = TextBuffer::from_text(2, row.file_path.clone(), "main".to_owned());

    assert!(tab_action_still_targets_row(&row, &exact, None));
    assert!(!tab_action_still_targets_row(&row, &equivalent, None));
    assert!(!tab_action_still_targets_row(&row, &different_id, None));
}

#[test]
fn tab_action_guard_requires_live_diff_source_to_match_row_target() {
    let path = PathBuf::from("workspace/src/main.rs");
    let diff_source = DiffBufferSource {
        path: path.clone(),
        base_path: None,
        hunk_stage: Some(GitChangeStage::Unstaged),
        saved_buffer_id: None,
    };
    let changed_diff_source = DiffBufferSource {
        path: path.clone(),
        base_path: Some(PathBuf::from("workspace/src/base.rs")),
        hunk_stage: Some(GitChangeStage::Unstaged),
        saved_buffer_id: None,
    };
    let row = prepare_tab_row(
        TabRowPreparationInput {
            id: 1,
            selected: true,
            name: "main.rs".to_owned(),
            dirty: false,
            read_only: false,
            changed_on_disk: false,
            file_path: Some(path.clone()),
            context_path: Some(path.clone()),
            diff_source: Some(diff_source.clone()),
            has_unstaged_changes: true,
            has_staged_changes: false,
            source_control_status: Some(GitFileStatus::Modified),
        },
        TabCapabilityRequests::ROW,
        |_| panic!("row preparation should not probe paths"),
    );
    let buffer = TextBuffer::from_text(1, Some(path), "diff".to_owned());

    assert!(tab_action_still_targets_row(
        &row,
        &buffer,
        Some(&diff_source)
    ));
    assert!(!tab_action_still_targets_row(&row, &buffer, None));
    assert!(!tab_action_still_targets_row(
        &row,
        &buffer,
        Some(&changed_diff_source)
    ));
}

#[test]
fn responsive_tab_max_width_bounds_tight_and_non_finite_widths() {
    assert_eq!(responsive_tab_max_width(10.0, 3), TAB_MIN_WIDTH);
    assert_eq!(responsive_tab_max_width(f32::NAN, 3), TAB_MIN_WIDTH);
    assert_eq!(responsive_tab_max_width(f32::INFINITY, 3), TAB_MAX_WIDTH);
    assert_eq!(responsive_tab_max_width(1000.0, 0), TAB_MAX_WIDTH);
    assert_eq!(responsive_tab_max_width(300.0, 3), 100.0);
}

#[test]
fn responsive_tab_actions_width_reserves_fixed_action_area_when_possible() {
    assert_eq!(
        responsive_tab_actions_width(TAB_ACTIONS_RESERVED_WIDTH + 300.0),
        TAB_ACTIONS_RESERVED_WIDTH
    );
    assert_eq!(responsive_tab_actions_width(90.0), 90.0);
    assert_eq!(
        responsive_tab_actions_width(f32::INFINITY),
        TAB_ACTIONS_RESERVED_WIDTH
    );
    assert_eq!(
        responsive_tab_actions_width(f32::NAN),
        TAB_ACTIONS_RESERVED_WIDTH
    );
}

#[test]
fn tab_row_marks_pending_clean_reloads_as_changed_on_disk() {
    let root = PathBuf::from("workspace");
    let first = root.join("src/main.rs");
    let second = root.join("src/lib.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        1,
        Some(first.clone()),
        "main".to_owned(),
    ));
    app.buffers.push(TextBuffer::from_text(
        2,
        Some(second.clone()),
        "lib".to_owned(),
    ));
    app.in_flight_reloads.insert(
        1,
        PendingFileReload {
            request_id: 1,
            path: first,
            version: app.buffer(1).expect("buffer should exist").version(),
            force_dirty: false,
        },
    );
    app.queued_file_reloads.insert(
        2,
        QueuedFileReload {
            path: second,
            force_dirty: false,
        },
    );

    let first_row = app.prepare_buffer_tab_row(app.buffer(1).expect("buffer should exist"));
    let second_row = app.prepare_buffer_tab_row(app.buffer(2).expect("buffer should exist"));

    assert!(!app.buffer_changed_on_disk(1));
    assert!(!app.buffer_changed_on_disk(2));
    assert!(first_row.changed_on_disk);
    assert!(second_row.changed_on_disk);
    assert!(first_row.label.starts_with("! "));
    assert!(second_row.label.starts_with("! "));
}

#[test]
fn tab_row_ignores_forced_and_mismatched_pending_reloads() {
    let root = PathBuf::from("workspace");
    let forced = root.join("src/forced.rs");
    let mismatch = root.join("src/mismatch.rs");
    let other = root.join("src/other.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        1,
        Some(forced.clone()),
        "forced".to_owned(),
    ));
    app.buffers.push(TextBuffer::from_text(
        2,
        Some(mismatch),
        "mismatch".to_owned(),
    ));
    app.in_flight_reloads.insert(
        1,
        PendingFileReload {
            request_id: 1,
            path: forced,
            version: app.buffer(1).expect("buffer should exist").version(),
            force_dirty: true,
        },
    );
    app.queued_file_reloads.insert(
        2,
        QueuedFileReload {
            path: other,
            force_dirty: false,
        },
    );

    let forced_row = app.prepare_buffer_tab_row(app.buffer(1).expect("buffer should exist"));
    let mismatch_row = app.prepare_buffer_tab_row(app.buffer(2).expect("buffer should exist"));

    assert!(!forced_row.changed_on_disk);
    assert!(!mismatch_row.changed_on_disk);
    assert!(!forced_row.label.starts_with("! "));
    assert!(!mismatch_row.label.starts_with("! "));
}

#[test]
fn tab_row_preserves_explicit_changed_on_disk_marker() {
    let root = PathBuf::from("workspace");
    let path = root.join("src/main.rs");
    let mut app = app_for_test(root);
    app.buffers
        .push(TextBuffer::from_text(1, Some(path), "main".to_owned()));
    app.mark_buffer_changed_on_disk(1);

    let row = app.prepare_buffer_tab_row(app.buffer(1).expect("buffer should exist"));

    assert!(row.changed_on_disk);
    assert!(row.label.starts_with("! "));
}

#[test]
fn diff_tab_context_action_command_maps_command_bus_actions() {
    let source = DiffBufferSource {
        path: PathBuf::from("src/main.rs"),
        base_path: None,
        hunk_stage: Some(GitChangeStage::Unstaged),
        saved_buffer_id: None,
    };

    assert_eq!(
        diff_tab_context_action_command(DiffTabContextActionKind::OpenBlame, Some(&source)),
        Some(Command::OpenFileBlame(PathBuf::from("src/main.rs")))
    );
    assert_eq!(
        diff_tab_context_action_command(DiffTabContextActionKind::PreviousDiffHunk, None),
        Some(Command::PreviousDiffHunk)
    );
    assert_eq!(
        diff_tab_context_action_command(DiffTabContextActionKind::StageCurrentDiffHunk, None),
        Some(Command::StageActiveDiffHunk)
    );
}

#[test]
fn diff_tab_context_action_command_preserves_raw_source_path() {
    let raw_path = PathBuf::from("workspace/src/../src/main.rs");
    let source = DiffBufferSource {
        path: raw_path.clone(),
        base_path: None,
        hunk_stage: Some(GitChangeStage::Unstaged),
        saved_buffer_id: None,
    };

    assert_eq!(
        diff_tab_context_action_command(DiffTabContextActionKind::OpenBlame, Some(&source)),
        Some(Command::OpenFileBlame(raw_path))
    );
}

#[test]
fn diff_tab_context_action_command_gracefully_skips_non_command_actions() {
    assert_eq!(
        diff_tab_context_action_command(DiffTabContextActionKind::OpenBlame, None),
        None
    );
    assert_eq!(
        diff_tab_context_action_command(DiffTabContextActionKind::RefreshDiff, None),
        None
    );
    assert_eq!(
        diff_tab_context_action_command(DiffTabContextActionKind::OpenSourceFile, None),
        None
    );
}

#[test]
fn close_all_buffers_queues_guarded_closes_for_dirty_buffers() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.buffers.push(TextBuffer::from_text(
        1,
        Some(root.join("clean.rs")),
        "clean".to_owned(),
    ));
    let mut first_dirty = TextBuffer::from_text(2, Some(root.join("first.rs")), "first".to_owned());
    first_dirty.mark_dirty();
    app.buffers.push(first_dirty);
    let mut second_dirty =
        TextBuffer::from_text(3, Some(root.join("second.rs")), "second".to_owned());
    second_dirty.mark_dirty();
    app.buffers.push(second_dirty);

    app.close_all_buffers();

    assert!(app.buffer(1).is_none());
    assert_eq!(app.dirty_close_buffer, Some(2));
    assert_eq!(app.pending_close_buffers, vec![3]);
    assert!(app.buffer(2).is_some());
    assert!(app.buffer(3).is_some());
    assert!(app.status.starts_with("Unsaved changes in "));
}

#[test]
fn close_all_buffers_force_closes_clean_buffers_without_guard() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.buffers.push(TextBuffer::from_text(
        1,
        Some(root.join("one.rs")),
        "one".to_owned(),
    ));
    app.buffers.push(TextBuffer::from_text(
        2,
        Some(root.join("two.rs")),
        "two".to_owned(),
    ));

    app.close_all_buffers();

    assert!(app.buffers.is_empty());
    assert_eq!(app.dirty_close_buffer, None);
    assert!(app.pending_close_buffers.is_empty());
}

#[test]
fn close_all_buffers_does_not_duplicate_buffers_already_under_close_guard() {
    let root = PathBuf::from("workspace");
    let mut app = app_for_test(root.clone());
    app.buffers.push(TextBuffer::from_text(
        1,
        Some(root.join("clean.rs")),
        "clean".to_owned(),
    ));
    let mut guarded = TextBuffer::from_text(2, Some(root.join("guarded.rs")), "guarded".to_owned());
    guarded.mark_dirty();
    app.buffers.push(guarded);
    app.dirty_close_buffer = Some(2);

    app.close_all_buffers();

    assert!(app.buffer(1).is_none());
    assert_eq!(app.dirty_close_buffer, Some(2));
    assert!(app.pending_close_buffers.is_empty());
    assert!(app.buffer(2).is_some());
}

fn app_for_test(root: PathBuf) -> KuroyaApp {
    let (tx, rx) = crate::ui_event_channel::ui_event_channel();
    let settings = EditorSettings::default();
    KuroyaApp::from_startup_context(AppStartupContext {
        runtime: Runtime::new().expect("test runtime"),
        tx,
        rx,
        workspace: Workspace::new(root.clone()),
        settings: settings.clone(),
        settings_panel_draft: settings,
        settings_editor_font_path: String::new(),
        settings_ui_font_path: String::new(),
        theme_picker_selected: 0,
        saved_session: None,
        terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
        watcher: None,
        recent_projects: Vec::new(),
        trusted_workspaces: vec![root],
        now: Instant::now(),
        startup_timings: Vec::new(),
    })
}

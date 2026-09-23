use super::{
    SOURCE_CONTROL_REF_LABEL_MAX_CHARS, SourceControlFilterScope, SourceControlFilterTerm,
    SourceControlRenderRow, SourceControlRowActionKind, SourceControlRowActionTarget,
    SourceControlRowOpenability, SourceControlRowsCacheInputs, SourceControlSortMode,
    SourceControlStageSection, SourceControlStageSectionKind, SourceControlViewMode,
    SourceControlVisibleRow, handle_source_control_keyboard, render_source_control_row,
    source_control_branch_display_label, source_control_branch_display_label_cow,
    source_control_cached_row_openability, source_control_change_list_keyboard_active,
    source_control_display_path_label, source_control_entries_for_untracked_changes_from_slice,
    source_control_filter_terms, source_control_filter_visible_entries,
    source_control_filtered_entries, source_control_git_error_label,
    source_control_git_root_matches_workspace, source_control_hunks_available,
    source_control_path_exists_cached, source_control_prepare_render_rows,
    source_control_ref_display_label, source_control_ref_display_label_cow,
    source_control_render_row_index_for_selection, source_control_repository_label,
    source_control_row_action_count, source_control_row_action_labels,
    source_control_row_click_command, source_control_row_display, source_control_row_openability,
    source_control_rows_cache_entry, source_control_sanitized_path_label,
    source_control_sanitized_path_label_cow, source_control_sanitized_path_label_owned,
    source_control_stage_header_action_enabled, source_control_status_path_label,
    source_control_tree_path_label, source_control_validated_row_action_command,
    source_control_verbose_commit_preview, source_control_visible_entry_count,
    source_control_visible_entry_index_for_selection, source_control_visible_rows,
};
use crate::path_display::DISPLAY_PATH_LABEL_MAX_CHARS;
use eframe::egui::{self, Event, Key, Modifiers, RawInput};
use kuroya_core::{
    Command, CommandBus, GitChangeStage, GitFileStatus, GitStatusEntry, GitUntrackedChanges,
    TextBuffer,
};
use std::{
    borrow::Cow,
    cell::Cell,
    collections::HashMap,
    path::{Path, PathBuf},
};

#[test]
fn git_root_match_accepts_parent_or_child_repository_roots() {
    assert!(source_control_git_root_matches_workspace(
        Path::new("workspace/repo"),
        Path::new("workspace/repo/src")
    ));
    assert!(source_control_git_root_matches_workspace(
        Path::new("workspace/repo"),
        Path::new("workspace")
    ));
    assert!(source_control_git_root_matches_workspace(
        Path::new("workspace/repo/src/.."),
        Path::new("workspace/repo")
    ));
    assert!(!source_control_git_root_matches_workspace(
        Path::new("workspace/old"),
        Path::new("workspace/new")
    ));
    assert!(!source_control_git_root_matches_workspace(
        Path::new("../../workspace/repo"),
        Path::new("workspace/repo")
    ));
    assert!(!source_control_git_root_matches_workspace(
        Path::new("../workspace/repo"),
        Path::new("workspace/repo/src")
    ));
    assert!(!source_control_git_root_matches_workspace(
        Path::new("workspace/repo/src/../../old"),
        Path::new("workspace/repo")
    ));
}

#[test]
fn git_root_match_handles_current_dir_workspace_root() {
    assert!(source_control_git_root_matches_workspace(
        Path::new("."),
        Path::new(".")
    ));
    assert!(source_control_git_root_matches_workspace(
        Path::new("repo"),
        Path::new(".")
    ));
    assert!(source_control_git_root_matches_workspace(
        Path::new("."),
        Path::new("repo/src")
    ));
    assert!(!source_control_git_root_matches_workspace(
        Path::new("../repo"),
        Path::new(".")
    ));
}

#[cfg(windows)]
#[test]
fn git_root_match_is_windows_case_insensitive_for_unicode_components() {
    assert!(source_control_git_root_matches_workspace(
        Path::new(r"C:\Work\Ångström"),
        Path::new(r"c:\work\ångström\src"),
    ));
}

#[test]
fn source_control_filter_terms_parse_scopes_once() {
    assert_eq!(
        source_control_filter_terms("stage:unstaged @modified src/main"),
        Some(vec![
            SourceControlFilterTerm::Scoped {
                scope: SourceControlFilterScope::Stage,
                value: "unstaged",
            },
            SourceControlFilterTerm::Scoped {
                scope: SourceControlFilterScope::StageOrStatus,
                value: "modified",
            },
            SourceControlFilterTerm::Plain("src/main"),
        ])
    );
    assert_eq!(source_control_filter_terms(" \t\n "), None);
}

#[test]
fn source_control_filtered_entries_match_scoped_and_plain_terms() {
    let root = PathBuf::from("workspace");
    let main = GitStatusEntry {
        path: root.join("src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let lib = GitStatusEntry {
        path: root.join("src/lib.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Staged,
    };
    let entries = vec![main.clone(), lib];

    assert_eq!(
        source_control_filtered_entries(&root, &entries, "stage:unstaged status:mod main"),
        vec![main]
    );
}

#[test]
fn source_control_filter_visible_entries_matches_borrowed_and_owned_paths() {
    let root = PathBuf::from("workspace");
    let main = GitStatusEntry {
        path: root.join("src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let new_file = GitStatusEntry {
        path: root.join("src/new.rs"),
        status: GitFileStatus::Untracked,
        stage: GitChangeStage::Unstaged,
    };
    let lib = GitStatusEntry {
        path: root.join("src/lib.rs"),
        status: GitFileStatus::Added,
        stage: GitChangeStage::Staged,
    };
    let entries = vec![main, new_file.clone(), lib.clone()];

    assert_eq!(
        source_control_filter_visible_entries(
            &root,
            Cow::Borrowed(entries.as_slice()),
            "status:untracked"
        ),
        vec![new_file]
    );
    assert_eq!(
        source_control_filter_visible_entries(&root, Cow::Owned(entries), "stage:staged"),
        vec![lib]
    );
}

#[test]
fn source_control_untracked_changes_slice_borrows_when_untracked_are_visible() {
    let modified = GitStatusEntry {
        path: PathBuf::from("workspace/src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let untracked = GitStatusEntry {
        path: PathBuf::from("workspace/src/new.rs"),
        status: GitFileStatus::Untracked,
        stage: GitChangeStage::Unstaged,
    };
    let staged = GitStatusEntry {
        path: PathBuf::from("workspace/src/lib.rs"),
        status: GitFileStatus::Added,
        stage: GitChangeStage::Staged,
    };
    let entries = vec![modified, untracked, staged];

    for mode in [GitUntrackedChanges::Mixed, GitUntrackedChanges::Separate] {
        match source_control_entries_for_untracked_changes_from_slice(&entries, mode) {
            Cow::Borrowed(visible) => assert_eq!(visible, entries.as_slice()),
            Cow::Owned(_) => panic!("visible untracked mode should borrow entries"),
        }
    }
}

#[test]
fn source_control_untracked_changes_slice_filters_when_untracked_are_hidden() {
    let modified = GitStatusEntry {
        path: PathBuf::from("workspace/src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let untracked = GitStatusEntry {
        path: PathBuf::from("workspace/src/new.rs"),
        status: GitFileStatus::Untracked,
        stage: GitChangeStage::Unstaged,
    };
    let staged = GitStatusEntry {
        path: PathBuf::from("workspace/src/lib.rs"),
        status: GitFileStatus::Added,
        stage: GitChangeStage::Staged,
    };
    let entries = vec![modified.clone(), untracked, staged.clone()];

    match source_control_entries_for_untracked_changes_from_slice(
        &entries,
        GitUntrackedChanges::Hidden,
    ) {
        Cow::Owned(visible) => assert_eq!(visible, vec![modified, staged]),
        Cow::Borrowed(_) => panic!("hidden untracked mode should filter entries"),
    }
}

#[test]
fn source_control_untracked_changes_slice_borrows_when_hidden_has_no_untracked() {
    let modified = GitStatusEntry {
        path: PathBuf::from("workspace/src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let staged = GitStatusEntry {
        path: PathBuf::from("workspace/src/lib.rs"),
        status: GitFileStatus::Added,
        stage: GitChangeStage::Staged,
    };
    let entries = vec![modified, staged];

    match source_control_entries_for_untracked_changes_from_slice(
        &entries,
        GitUntrackedChanges::Hidden,
    ) {
        Cow::Borrowed(visible) => assert_eq!(visible, entries.as_slice()),
        Cow::Owned(_) => panic!("hidden mode should borrow when there is nothing to hide"),
    }
}

#[test]
fn stage_header_actions_disable_empty_groups() {
    let empty = SourceControlStageSection {
        stage: GitChangeStage::Staged,
        kind: SourceControlStageSectionKind::StagedChanges,
        count: 0,
    };
    let populated = SourceControlStageSection { count: 1, ..empty };

    assert!(!source_control_stage_header_action_enabled(empty));
    assert!(source_control_stage_header_action_enabled(populated));
}

#[test]
fn hunk_actions_require_a_valid_diff_source() {
    assert!(!source_control_hunks_available(
        GitChangeStage::Unstaged,
        GitFileStatus::Conflicted,
        true
    ));
    assert!(!source_control_hunks_available(
        GitChangeStage::Unstaged,
        GitFileStatus::Modified,
        false
    ));
    assert!(source_control_hunks_available(
        GitChangeStage::Unstaged,
        GitFileStatus::Deleted,
        false
    ));
    assert!(source_control_hunks_available(
        GitChangeStage::Staged,
        GitFileStatus::Added,
        false
    ));
}

#[test]
fn source_control_path_exists_cache_uses_open_buffer_before_filesystem_probe() {
    let path = PathBuf::from("workspace/src/main.rs");
    let buffers = vec![TextBuffer::from_text(
        7,
        Some(path.clone()),
        "open\n".to_owned(),
    )];
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    assert!(source_control_path_exists_cached(
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
fn source_control_path_exists_cache_uses_equivalent_open_buffer_before_filesystem_probe() {
    let path = PathBuf::from("workspace/src/main.rs");
    let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
    let buffers = vec![TextBuffer::from_text(7, Some(path), "open\n".to_owned())];
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    assert!(source_control_path_exists_cached(
        &mut cache,
        &buffers,
        &[],
        &equivalent_path,
        |_| {
            probes.set(probes.get() + 1);
            false
        },
    ));

    assert_eq!(probes.get(), 0);
}

#[test]
fn source_control_path_exists_cache_reuses_raw_path_probe_result() {
    let path = PathBuf::from("workspace/src/bad\nname\u{202e}.rs");
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    assert!(source_control_path_exists_cached(
        &mut cache,
        &[],
        &[],
        &path,
        |probe_path| {
            assert_eq!(probe_path, path.as_path());
            probes.set(probes.get() + 1);
            true
        },
    ));
    assert!(source_control_path_exists_cached(
        &mut cache,
        &[],
        &[],
        &path,
        |_| {
            probes.set(probes.get() + 1);
            false
        },
    ));

    assert_eq!(cache.get(path.as_path()), Some(&true));
    assert_eq!(probes.get(), 1);
}

#[test]
fn source_control_path_exists_cache_reuses_equivalent_fallback_probe_result() {
    let path = PathBuf::from("workspace/src/main.rs");
    let equivalent_path = PathBuf::from("workspace/src/./main.rs");
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    assert!(source_control_path_exists_cached(
        &mut cache,
        &[],
        &[],
        &path,
        |probe_path| {
            assert_eq!(probe_path, path.as_path());
            probes.set(probes.get() + 1);
            true
        },
    ));
    assert!(source_control_path_exists_cached(
        &mut cache,
        &[],
        &[],
        &equivalent_path,
        |_| {
            probes.set(probes.get() + 1);
            false
        },
    ));

    assert_eq!(cache.get(equivalent_path.as_path()), Some(&true));
    assert_eq!(probes.get(), 1);
}

#[test]
fn source_control_row_openability_reuses_equivalent_fallback_probe_for_compare_path() {
    let path = PathBuf::from("workspace/src/main.rs");
    let compare_path = PathBuf::from("workspace/src/../src/main.rs");
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    let openability = source_control_row_openability(
        &mut cache,
        &[],
        &[],
        &path,
        Some(&compare_path),
        |probe_path| {
            assert_eq!(probe_path, path.as_path());
            probes.set(probes.get() + 1);
            true
        },
    );

    assert_eq!(
        openability,
        SourceControlRowOpenability {
            source_exists: true,
            can_compare_with_selected: true,
        }
    );
    assert_eq!(probes.get(), 1);
}

#[test]
fn source_control_row_openability_uses_equivalent_indexed_paths_before_fallback_probe() {
    let path = PathBuf::from("workspace/src/main.rs");
    let compare_path = PathBuf::from("workspace/src/../src/main.rs");
    let indexed_files = vec![path.clone()];
    let probes = Cell::new(0usize);
    let mut cache = HashMap::new();

    let openability = source_control_row_openability(
        &mut cache,
        &[],
        &indexed_files,
        &path,
        Some(&compare_path),
        |_| {
            probes.set(probes.get() + 1);
            false
        },
    );

    assert_eq!(
        openability,
        SourceControlRowOpenability {
            source_exists: true,
            can_compare_with_selected: true,
        }
    );
    assert_eq!(probes.get(), 0);
}

#[test]
fn source_control_visible_row_helpers_count_and_select_entries() {
    let entries = vec![
        GitStatusEntry {
            path: PathBuf::from("workspace/src/main.rs"),
            status: GitFileStatus::Modified,
            stage: GitChangeStage::Unstaged,
        },
        GitStatusEntry {
            path: PathBuf::from("workspace/src/new.rs"),
            status: GitFileStatus::Untracked,
            stage: GitChangeStage::Unstaged,
        },
        GitStatusEntry {
            path: PathBuf::from("workspace/src/lib.rs"),
            status: GitFileStatus::Modified,
            stage: GitChangeStage::Staged,
        },
    ];

    let rows = source_control_visible_rows(
        &entries,
        true,
        GitUntrackedChanges::Separate,
        false,
        true,
        false,
    );

    assert_eq!(source_control_visible_entry_count(&rows), 2);
    assert_eq!(
        source_control_visible_entry_index_for_selection(&rows, 0),
        Some(0)
    );
    assert_eq!(
        source_control_visible_entry_index_for_selection(&rows, 1),
        Some(2)
    );
    assert_eq!(
        source_control_visible_entry_index_for_selection(&rows, 2),
        None
    );
}

#[test]
fn source_control_row_display_prepares_display_only_text() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("bad\nname\u{202e}.rs");
    let entry = GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };

    let display = source_control_row_display(&root, &entry, SourceControlViewMode::Tree, true);

    assert_eq!(display.text, "src/bad name.rs  Modified");
    assert_eq!(display.hover_path_label, "src/bad name.rs");
    assert_eq!(entry.path, path);
}

#[test]
fn source_control_prepare_render_rows_skips_stale_entry_indices() {
    let root = PathBuf::from("workspace");
    let path = root.join("src/main.rs");
    let entries = vec![GitStatusEntry {
        path,
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    }];
    let section = SourceControlStageSection {
        stage: GitChangeStage::Unstaged,
        kind: SourceControlStageSectionKind::Changes,
        count: 2,
    };
    let rows = vec![
        SourceControlVisibleRow::Header(section),
        SourceControlVisibleRow::Entry {
            entry_index: 9,
            visible_index: 0,
        },
        SourceControlVisibleRow::Entry {
            entry_index: 0,
            visible_index: 1,
        },
    ];

    let render_rows = source_control_prepare_render_rows(&entries, &rows);

    assert_eq!(render_rows.len(), 2);
    assert_eq!(render_rows[0], SourceControlRenderRow::Header(section));
    assert!(matches!(
        render_rows[1],
        SourceControlRenderRow::Entry {
            entry_index: 0,
            visible_index: 1,
            ..
        }
    ));
    assert_eq!(
        source_control_render_row_index_for_selection(&render_rows, 0),
        None
    );
    assert_eq!(
        source_control_render_row_index_for_selection(&render_rows, 1),
        Some(1)
    );
}

#[test]
fn source_control_prepare_render_rows_keeps_actions_on_raw_entries() {
    let root = PathBuf::from("workspace");
    let path = root.join("src").join("bad\nname\u{202e}.rs");
    let entries = vec![GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    }];
    let rows = vec![SourceControlVisibleRow::Entry {
        entry_index: 0,
        visible_index: 0,
    }];

    let render_rows = source_control_prepare_render_rows(&entries, &rows);

    let SourceControlRenderRow::Entry { entry_index, .. } = &render_rows[0] else {
        panic!("expected prepared entry row");
    };

    let display = source_control_row_display(
        &root,
        &entries[*entry_index],
        SourceControlViewMode::Tree,
        true,
    );
    assert_eq!(display.text, "src/bad name.rs  Modified");
    assert_eq!(
        source_control_row_click_command(true, &entries[*entry_index]),
        Some(Command::OpenFileChanges(path.clone()))
    );
    assert_eq!(entries[*entry_index].path, path);
}

#[test]
fn source_control_cached_row_openability_reuses_prepared_value() {
    let probes = Cell::new(0usize);
    let mut cache = None;
    let available = SourceControlRowOpenability {
        source_exists: true,
        can_compare_with_selected: false,
    };

    let first = source_control_cached_row_openability(&mut cache, || {
        probes.set(probes.get() + 1);
        available
    });
    let second = source_control_cached_row_openability(&mut cache, || {
        probes.set(probes.get() + 1);
        SourceControlRowOpenability {
            source_exists: false,
            can_compare_with_selected: true,
        }
    });

    assert_eq!(first, available);
    assert_eq!(second, available);
    assert_eq!(probes.get(), 1);
}

#[test]
fn source_control_row_action_validation_rejects_stale_openability_and_stage() {
    let path = PathBuf::from("workspace/src/main.rs");
    let entry = GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let missing_source = SourceControlRowOpenability {
        source_exists: false,
        can_compare_with_selected: false,
    };
    let available_source = SourceControlRowOpenability {
        source_exists: true,
        can_compare_with_selected: true,
    };

    assert_eq!(
        source_control_validated_row_action_command(
            &entry,
            SourceControlRowActionTarget::new(SourceControlRowActionKind::OpenFile, &entry),
            missing_source,
            true,
        ),
        None
    );
    assert_eq!(
        source_control_validated_row_action_command(
            &entry,
            SourceControlRowActionTarget::new(
                SourceControlRowActionKind::CompareWithSelected,
                &entry,
            ),
            SourceControlRowOpenability {
                source_exists: true,
                can_compare_with_selected: false,
            },
            true,
        ),
        None
    );
    assert_eq!(
        source_control_validated_row_action_command(
            &entry,
            SourceControlRowActionTarget::new(SourceControlRowActionKind::Unstage, &entry),
            available_source,
            true,
        ),
        None
    );
    assert_eq!(
        source_control_validated_row_action_command(
            &entry,
            SourceControlRowActionTarget::new(SourceControlRowActionKind::Stage, &entry),
            missing_source,
            true,
        ),
        Some(Command::StageFileChange(path.clone()))
    );
    assert_eq!(
        source_control_validated_row_action_command(
            &entry,
            SourceControlRowActionTarget::new(SourceControlRowActionKind::OpenFile, &entry),
            available_source,
            true,
        ),
        Some(Command::OpenFile(path))
    );
}

#[test]
fn source_control_row_action_validation_rejects_stale_entry_fingerprint() {
    let old_entry = GitStatusEntry {
        path: PathBuf::from("workspace/src/old.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let new_path = PathBuf::from("workspace/src/new.rs");
    let new_entry = GitStatusEntry {
        path: new_path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let available_source = SourceControlRowOpenability {
        source_exists: true,
        can_compare_with_selected: true,
    };
    let stale_target =
        SourceControlRowActionTarget::new(SourceControlRowActionKind::Stage, &old_entry);

    assert_eq!(
        source_control_validated_row_action_command(
            &new_entry,
            stale_target,
            available_source,
            true,
        ),
        None
    );
    assert_eq!(
        source_control_validated_row_action_command(
            &new_entry,
            SourceControlRowActionTarget::new(SourceControlRowActionKind::Stage, &new_entry),
            available_source,
            true,
        ),
        Some(Command::StageFileChange(new_path))
    );
}

#[test]
fn source_control_row_action_validation_preserves_raw_path_payload() {
    let raw_path = PathBuf::from("workspace/src/bad\nname\u{202e}.rs");
    let entry = GitStatusEntry {
        path: raw_path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let available_source = SourceControlRowOpenability {
        source_exists: true,
        can_compare_with_selected: true,
    };

    assert_eq!(
        source_control_validated_row_action_command(
            &entry,
            SourceControlRowActionTarget::new(SourceControlRowActionKind::OpenFile, &entry),
            available_source,
            true,
        ),
        Some(Command::OpenFile(raw_path.clone()))
    );
    assert_eq!(entry.path, raw_path);
}

#[test]
fn source_control_row_action_count_matches_visible_action_filter() {
    for (stage, status, source_exists, can_compare_with_selected, show_inline_open_file_action) in [
        (
            GitChangeStage::Unstaged,
            GitFileStatus::Modified,
            true,
            true,
            true,
        ),
        (
            GitChangeStage::Staged,
            GitFileStatus::Deleted,
            false,
            false,
            true,
        ),
        (
            GitChangeStage::Unstaged,
            GitFileStatus::Conflicted,
            true,
            false,
            false,
        ),
    ] {
        assert_eq!(
            source_control_row_action_count(
                stage,
                status,
                source_exists,
                can_compare_with_selected,
                show_inline_open_file_action,
            ),
            source_control_row_action_labels(
                stage,
                status,
                source_exists,
                can_compare_with_selected,
                show_inline_open_file_action,
            )
            .len()
        );
    }
}

#[test]
fn source_control_row_hidden_actions_do_not_probe_openability() {
    let entry = GitStatusEntry {
        path: PathBuf::from("workspace/src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let probes = Cell::new(0usize);
    let ctx = egui::Context::default();

    let _ = ctx.run(RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let display = source_control_row_display(
                Path::new("workspace"),
                &entry,
                SourceControlViewMode::List,
                false,
            );
            let row = render_source_control_row(
                ui,
                &entry,
                display,
                false,
                false,
                0,
                || {
                    probes.set(probes.get() + 1);
                    SourceControlRowOpenability {
                        source_exists: true,
                        can_compare_with_selected: true,
                    }
                },
                true,
            );

            assert!(row.action.is_none());
        });
    });

    assert_eq!(probes.get(), 0);
}

#[test]
fn source_control_keyboard_stage_shortcut_does_not_probe_openability() {
    let path = PathBuf::from("workspace/src/main.rs");
    let entry = GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let probes = Cell::new(0usize);

    let result = run_source_control_keyboard_frame_with_probe(
        std::slice::from_ref(&entry),
        Key::S,
        true,
        false,
        |_| {
            probes.set(probes.get() + 1);
            true
        },
    );

    assert_eq!(result.command, Some(Command::StageFileChange(path)));
    assert_eq!(probes.get(), 0);
}

#[test]
fn source_control_keyboard_openability_probe_reuses_row_cache() {
    let path = PathBuf::from("workspace/src/main.rs");
    let entry = GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let ctx = egui::Context::default();
    let input = RawInput {
        events: vec![Event::Key {
            key: Key::O,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..RawInput::default()
    };
    let mut selected = 0;
    let mut query = String::new();
    let mut source_control_open = true;
    let mut status = "unchanged".to_owned();
    let mut command_bus = CommandBus::default();
    let rows = source_control_visible_rows(
        std::slice::from_ref(&entry),
        false,
        GitUntrackedChanges::Mixed,
        false,
        false,
        false,
    );
    let mut cache = HashMap::new();
    let probes = Cell::new(0usize);

    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            handle_source_control_keyboard(
                ui,
                std::slice::from_ref(&entry),
                &rows,
                source_control_visible_entry_count(&rows),
                &[],
                &[],
                &mut selected,
                &mut query,
                &mut source_control_open,
                &mut status,
                &mut command_bus,
                &mut cache,
                |_| {
                    probes.set(probes.get() + 1);
                    true
                },
                false,
                false,
                true,
                240.0,
            );
        });
    });

    assert_eq!(command_bus.pop(), Some(Command::OpenFile(path.clone())));
    assert!(source_control_path_exists_cached(
        &mut cache,
        &[],
        &[],
        &path,
        |_| {
            probes.set(probes.get() + 1);
            true
        },
    ));
    assert_eq!(probes.get(), 1);
}

#[test]
fn source_control_status_path_label_sanitizes_display_only_text() {
    let label = source_control_status_path_label(Path::new("src/bad\nname\u{202e}.rs"));

    assert_eq!(label, "bad name.rs");
}

#[test]
fn source_control_sanitized_path_label_cow_borrows_clean_ascii_and_unicode() {
    let ascii = "src/main.rs";
    match source_control_sanitized_path_label_cow(ascii) {
        Cow::Borrowed(label) => assert_eq!(label, ascii),
        Cow::Owned(label) => panic!("expected borrowed ASCII path label, got {label:?}"),
    }

    let unicode = "src/\u{e9}clair.rs";
    match source_control_sanitized_path_label_cow(unicode) {
        Cow::Borrowed(label) => assert_eq!(label, unicode),
        Cow::Owned(label) => panic!("expected borrowed Unicode path label, got {label:?}"),
    }
}

#[test]
fn source_control_sanitized_path_label_cow_owns_dirty_truncated_and_fallback_labels() {
    match source_control_sanitized_path_label_cow("src/bad\nname\u{202e}.rs") {
        Cow::Owned(label) => assert_eq!(label, "src/bad name.rs"),
        Cow::Borrowed(label) => panic!("expected owned dirty path label, got {label:?}"),
    }

    let long = "very-long-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS);
    match source_control_sanitized_path_label_cow(&long) {
        Cow::Owned(label) => {
            assert!(label.contains("..."));
            assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        }
        Cow::Borrowed(label) => panic!("expected owned truncated path label, got {label:?}"),
    }

    match source_control_sanitized_path_label_cow("") {
        Cow::Owned(label) => assert_eq!(label, "."),
        Cow::Borrowed(label) => panic!("expected owned fallback path label, got {label:?}"),
    }
}

#[test]
fn source_control_sanitized_path_label_string_wrapper_matches_cow_helper() {
    let dirty = "src/bad\rname\u{202e}.rs";
    let long = "very-long-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS);
    let cases = [
        "src/main.rs",
        "src/\u{e9}clair.rs",
        dirty,
        "",
        long.as_str(),
    ];

    for value in cases {
        assert_eq!(
            source_control_sanitized_path_label(value),
            source_control_sanitized_path_label_cow(value).into_owned()
        );
    }
}

#[test]
fn source_control_sanitized_path_label_owned_reuses_clean_owned_labels() {
    fn assert_reuses_owned_allocation(value: String) {
        let ptr = value.as_ptr();
        let capacity = value.capacity();
        let expected = value.clone();

        let label = source_control_sanitized_path_label_owned(value);

        assert_eq!(label, expected);
        assert_eq!(label.as_ptr(), ptr);
        assert_eq!(label.capacity(), capacity);
    }

    assert_reuses_owned_allocation(String::from("src/main.rs"));
    assert_reuses_owned_allocation(String::from("src/\u{e9}clair.rs"));

    let label = source_control_sanitized_path_label_owned(String::from("src/bad\nname.rs"));
    assert_eq!(label, "src/bad name.rs");
}

#[test]
fn source_control_branch_display_label_sanitizes_display_only_text() {
    let branch = format!("feature/bad\nname\u{202e}/{}", "very-long-".repeat(24));

    let label = source_control_branch_display_label(Some(&branch));

    assert!(!label.contains('\n'));
    assert!(!label.contains('\u{202e}'));
    assert!(label.contains("..."));
    assert!(label.chars().count() <= SOURCE_CONTROL_REF_LABEL_MAX_CHARS);
}

#[test]
fn source_control_ref_display_label_cow_borrows_clean_ascii_and_unicode() {
    let ascii = "feature/main";
    match source_control_ref_display_label_cow(ascii, "detached") {
        Cow::Borrowed(label) => assert_eq!(label, ascii),
        Cow::Owned(label) => panic!("expected borrowed ASCII ref label, got {label:?}"),
    }

    let unicode = "feature/\u{e9}clair";
    match source_control_branch_display_label_cow(Some(unicode)) {
        Cow::Borrowed(label) => assert_eq!(label, unicode),
        Cow::Owned(label) => panic!("expected borrowed Unicode branch label, got {label:?}"),
    }
}

#[test]
fn source_control_ref_display_label_cow_owns_dirty_truncated_and_fallback_labels() {
    match source_control_ref_display_label_cow("feature/bad\nname\u{202e}", "detached") {
        Cow::Owned(label) => assert_eq!(label, "feature/bad name"),
        Cow::Borrowed(label) => panic!("expected owned dirty ref label, got {label:?}"),
    }

    let long = "very-long-".repeat(24);
    match source_control_ref_display_label_cow(&long, "detached") {
        Cow::Owned(label) => {
            assert!(label.contains("..."));
            assert!(label.chars().count() <= SOURCE_CONTROL_REF_LABEL_MAX_CHARS);
        }
        Cow::Borrowed(label) => panic!("expected owned truncated ref label, got {label:?}"),
    }

    match source_control_ref_display_label_cow("", "detached") {
        Cow::Owned(label) => assert_eq!(label, "detached"),
        Cow::Borrowed(label) => panic!("expected owned fallback ref label, got {label:?}"),
    }
}

#[test]
fn source_control_ref_string_wrappers_match_cow_helpers() {
    let dirty = "feature/bad\rbranch\u{202e}";
    let cases = [("main", "detached"), (dirty, "detached"), ("", "detached")];

    for (value, fallback) in cases {
        assert_eq!(
            source_control_ref_display_label(value, fallback),
            source_control_ref_display_label_cow(value, fallback).into_owned()
        );
    }

    assert_eq!(
        source_control_branch_display_label(Some(dirty)),
        source_control_branch_display_label_cow(Some(dirty)).into_owned()
    );
    assert_eq!(
        source_control_branch_display_label(None),
        source_control_branch_display_label_cow(None).into_owned()
    );
}

#[test]
fn source_control_repository_label_respects_reference_details_toggle() {
    let root = PathBuf::from("workspace/project");
    let branch = "feature/bad\nbranch";

    assert_eq!(
        source_control_repository_label(&root, Some(branch), true),
        "project (feature/bad branch)"
    );
    assert_eq!(
        source_control_repository_label(&root, Some(branch), false),
        "project"
    );
    assert_eq!(
        source_control_repository_label(&root, None, true),
        "project (detached)"
    );
}

#[test]
fn source_control_repository_label_sanitizes_name_and_branch() {
    let root = PathBuf::from(format!("repo\n{}\u{202e}", "very-long-".repeat(24)));
    let branch = format!("feature/bad\rbranch/{}", "very-long-".repeat(24));

    let label = source_control_repository_label(&root, Some(&branch), true);

    assert!(!label.contains('\n'));
    assert!(!label.contains('\r'));
    assert!(!label.contains('\u{202e}'));
    assert!(label.contains("..."));
    assert!(label.chars().count() <= SOURCE_CONTROL_REF_LABEL_MAX_CHARS);
}

#[test]
fn source_control_verbose_commit_preview_sanitizes_display_path_labels() {
    let entries = vec![GitStatusEntry {
        path: PathBuf::from("src/bad\nname\u{202e}.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Staged,
    }];

    let preview = source_control_verbose_commit_preview(&entries, true, true).unwrap();

    assert_eq!(preview.lines().count(), 3);
    assert!(preview.contains("#\tmodified: bad name.rs"));
    assert!(!preview.contains('\u{202e}'));
}

#[test]
fn source_control_change_list_keyboard_requires_list_focus() {
    assert!(source_control_change_list_keyboard_active(false, true));
    assert!(!source_control_change_list_keyboard_active(false, false));
    assert!(!source_control_change_list_keyboard_active(true, true));
}

#[test]
fn source_control_filter_escape_clear_reports_selection_changed() {
    let ctx = egui::Context::default();
    let input = RawInput {
        events: vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..RawInput::default()
    };
    let mut selected = 7;
    let mut query = "src".to_owned();
    let mut source_control_open = true;
    let mut status = "unchanged".to_owned();
    let mut command_bus = CommandBus::default();
    let mut path_exists_cache = HashMap::new();
    let mut selection_changed = false;

    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            selection_changed = handle_source_control_keyboard(
                ui,
                &[],
                &[],
                0,
                &[],
                &[],
                &mut selected,
                &mut query,
                &mut source_control_open,
                &mut status,
                &mut command_bus,
                &mut path_exists_cache,
                |_| true,
                true,
                false,
                false,
                240.0,
            );
        });
    });

    assert!(selection_changed);
    assert_eq!(selected, 0);
    assert!(query.is_empty());
    assert!(source_control_open);
    assert_eq!(status, "unchanged");
    assert_eq!(command_bus.pop(), None);
}

#[test]
fn source_control_keyboard_actions_ignore_row_shortcuts_without_list_focus() {
    let path = PathBuf::from("src/main.rs");
    let entry = GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };

    let unfocused =
        run_source_control_keyboard_frame(std::slice::from_ref(&entry), Key::S, false, false);
    assert_eq!(unfocused.command, None);
    assert_eq!(unfocused.status, "unchanged");

    let focused = run_source_control_keyboard_frame(&[entry], Key::S, true, false);
    assert_eq!(focused.command, Some(Command::StageFileChange(path)));
}

#[test]
fn source_control_keyboard_actions_ignore_row_shortcuts_while_commit_input_has_focus() {
    let entry = GitStatusEntry {
        path: PathBuf::from("src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };

    let result = run_source_control_keyboard_frame(&[entry], Key::S, true, true);

    assert_eq!(result.command, None);
    assert_eq!(result.status, "unchanged");
}

#[test]
fn source_control_display_path_label_sanitizes_list_and_tree_labels() {
    let root = PathBuf::from("repo");
    let path = root.join("src").join("bad\nname\u{202e}.rs");

    let list_label =
        source_control_display_path_label(&root, &path, SourceControlViewMode::List, true);
    let tree_label =
        source_control_display_path_label(&root, &path, SourceControlViewMode::Tree, true);

    assert_eq!(list_label, "bad name.rs");
    assert_eq!(tree_label, "src/bad name.rs");
}

#[test]
fn source_control_display_path_label_preserves_list_tree_and_raw_path_semantics() {
    let root = PathBuf::from("repo");
    let path = root.join("src").join("main.rs");

    assert_eq!(
        source_control_display_path_label(&root, &path, SourceControlViewMode::List, true),
        "main.rs"
    );
    assert_eq!(
        source_control_display_path_label(&root, &path, SourceControlViewMode::Tree, true),
        "src/main.rs"
    );
    assert_eq!(
        source_control_display_path_label(&root, &path, SourceControlViewMode::Tree, false),
        "main.rs"
    );

    let raw_path = root.join("src").join("bad\nname\u{202e}.rs");
    assert_eq!(
        source_control_tree_path_label(&root, &raw_path, true),
        "src/bad\nname\u{202e}.rs"
    );
    assert_eq!(
        source_control_tree_path_label(&root, &raw_path, false),
        "bad\nname\u{202e}.rs"
    );
}

#[test]
fn source_control_display_label_sanitizes_but_click_command_preserves_raw_path() {
    let root = PathBuf::from("repo");
    let path = root.join("src").join("bad\nname\u{202e}.rs");
    let entry = GitStatusEntry {
        path: path.clone(),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };

    let label = source_control_display_path_label(&root, &path, SourceControlViewMode::Tree, true);

    assert_eq!(label, "src/bad name.rs");
    assert_eq!(
        source_control_row_click_command(true, &entry),
        Some(Command::OpenFileChanges(path.clone()))
    );
    assert_eq!(entry.path, path);
}

struct KeyboardFrameResult {
    command: Option<Command>,
    status: String,
}

fn run_source_control_keyboard_frame(
    entries: &[GitStatusEntry],
    key: Key,
    change_list_focused: bool,
    commit_focused: bool,
) -> KeyboardFrameResult {
    run_source_control_keyboard_frame_with_probe(
        entries,
        key,
        change_list_focused,
        commit_focused,
        |_| true,
    )
}

fn run_source_control_keyboard_frame_with_probe(
    entries: &[GitStatusEntry],
    key: Key,
    change_list_focused: bool,
    commit_focused: bool,
    path_exists: impl FnMut(&Path) -> bool,
) -> KeyboardFrameResult {
    let ctx = egui::Context::default();
    let input = RawInput {
        events: vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..RawInput::default()
    };
    let mut selected = 0;
    let mut query = String::new();
    let mut source_control_open = true;
    let mut status = "unchanged".to_owned();
    let mut command_bus = CommandBus::default();
    let rows = source_control_visible_rows(
        entries,
        false,
        GitUntrackedChanges::Mixed,
        false,
        false,
        false,
    );
    let mut path_exists_cache = HashMap::new();
    let mut path_exists = path_exists;

    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            handle_source_control_keyboard(
                ui,
                entries,
                &rows,
                source_control_visible_entry_count(&rows),
                &[],
                &[],
                &mut selected,
                &mut query,
                &mut source_control_open,
                &mut status,
                &mut command_bus,
                &mut path_exists_cache,
                &mut path_exists,
                false,
                commit_focused,
                change_list_focused,
                240.0,
            );
        });
    });

    KeyboardFrameResult {
        command: command_bus.pop(),
        status,
    }
}

#[test]
fn source_control_git_error_label_prefixes_error_text() {
    assert_eq!(
        source_control_git_error_label("repository is bare"),
        "Git error: repository is bare"
    );
    // Whitespace-only errors collapse to the bare label.
    assert_eq!(source_control_git_error_label("   "), "Git error");
}

#[test]
fn source_control_git_error_label_sanitizes_control_characters() {
    let label = source_control_git_error_label("bad\u{7}input");
    assert!(label.starts_with("Git error: "), "{label}");
    assert!(!label.contains('\u{7}'), "{label}");
}

fn cache_test_entries(root: &Path) -> Vec<GitStatusEntry> {
    vec![
        GitStatusEntry {
            path: root.join("src/main.rs"),
            status: GitFileStatus::Modified,
            stage: GitChangeStage::Unstaged,
        },
        GitStatusEntry {
            path: root.join("src/lib.rs"),
            status: GitFileStatus::Added,
            stage: GitChangeStage::Staged,
        },
    ]
}

fn cache_test_inputs<'a>(
    root: &'a Path,
    query: &'a str,
    git_revision: u64,
) -> SourceControlRowsCacheInputs<'a> {
    SourceControlRowsCacheInputs {
        git_revision,
        root,
        query,
        sort: SourceControlSortMode::Path,
        untracked_changes: GitUntrackedChanges::Mixed,
        always_show_staged: false,
        unstaged_collapsed: false,
        untracked_collapsed: false,
        staged_collapsed: false,
    }
}

#[test]
fn source_control_rows_cache_reuses_rows_when_inputs_are_unchanged() {
    let root = PathBuf::from("workspace");
    let entries = cache_test_entries(&root);
    let mut cache = None;
    let inputs = cache_test_inputs(&root, "", 1);

    let first = source_control_rows_cache_entry(inputs, Cow::Borrowed(&entries), &mut cache);
    assert_eq!(first.entries, entries);
    assert!(!first.rows.is_empty());

    // Mutate the cached rows to prove the next call reuses them instead of
    // rebuilding from `entries`.
    cache
        .as_mut()
        .expect("rows cache populated")
        .entries
        .push(GitStatusEntry {
            path: root.join("mutated.rs"),
            status: GitFileStatus::Added,
            stage: GitChangeStage::Unstaged,
        });

    let second = source_control_rows_cache_entry(inputs, Cow::Borrowed(&entries), &mut cache);

    assert_eq!(second.entries.len(), entries.len() + 1);
    assert!(
        second
            .entries
            .iter()
            .any(|entry| entry.path == root.join("mutated.rs"))
    );
}

#[test]
fn source_control_rows_cache_invalidates_on_git_revision_change() {
    let root = PathBuf::from("workspace");
    let entries = cache_test_entries(&root);
    let mut cache = None;

    let _ = source_control_rows_cache_entry(
        cache_test_inputs(&root, "", 1),
        Cow::Borrowed(&entries),
        &mut cache,
    );
    cache
        .as_mut()
        .expect("rows cache populated")
        .entries
        .clear();

    let rebuilt = source_control_rows_cache_entry(
        cache_test_inputs(&root, "", 2),
        Cow::Borrowed(&entries),
        &mut cache,
    );

    assert_eq!(rebuilt.entries, entries);
}

#[test]
fn source_control_rows_cache_invalidates_on_query_change() {
    let root = PathBuf::from("workspace");
    let entries = cache_test_entries(&root);
    let main = GitStatusEntry {
        path: root.join("src/main.rs"),
        status: GitFileStatus::Modified,
        stage: GitChangeStage::Unstaged,
    };
    let mut cache = None;

    let _ = source_control_rows_cache_entry(
        cache_test_inputs(&root, "", 1),
        Cow::Borrowed(&entries),
        &mut cache,
    );
    cache
        .as_mut()
        .expect("rows cache populated")
        .entries
        .clear();

    let rebuilt = source_control_rows_cache_entry(
        cache_test_inputs(&root, "main", 1),
        Cow::Borrowed(&entries),
        &mut cache,
    );

    assert_eq!(rebuilt.entries, vec![main]);
}

#[test]
fn source_control_rows_cache_invalidates_on_sort_mode_and_collapse_inputs() {
    let root = PathBuf::from("workspace");
    let entries = cache_test_entries(&root);
    let mut cache = None;

    let _ = source_control_rows_cache_entry(
        cache_test_inputs(&root, "", 1),
        Cow::Borrowed(&entries),
        &mut cache,
    );
    cache
        .as_mut()
        .expect("rows cache populated")
        .entries
        .clear();

    let mut sort_inputs = cache_test_inputs(&root, "", 1);
    sort_inputs.sort = SourceControlSortMode::Status;
    let resorted =
        source_control_rows_cache_entry(sort_inputs, Cow::Borrowed(&entries), &mut cache);
    assert_eq!(resorted.entries, entries);

    cache
        .as_mut()
        .expect("rows cache populated")
        .entries
        .clear();

    let mut collapsed_inputs = cache_test_inputs(&root, "", 1);
    collapsed_inputs.unstaged_collapsed = true;
    let collapsed =
        source_control_rows_cache_entry(collapsed_inputs, Cow::Borrowed(&entries), &mut cache);
    assert_eq!(collapsed.entries, entries);
    // Only the staged entry survives collapsing the unstaged section.
    assert_eq!(source_control_visible_entry_count(&collapsed.rows), 1);
}

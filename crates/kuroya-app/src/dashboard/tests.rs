use super::{
    DASHBOARD_DETAIL_MAX_CHARS, DASHBOARD_LABEL_MAX_CHARS, DASHBOARD_LOGO_BYTES,
    DASHBOARD_WORKSPACE_TASK_LIMIT, DASHBOARD_WORKSPACE_TASK_SCAN_MAX, DashboardDisplayText,
    DashboardDisplayTextCache, dashboard_file_detail, dashboard_file_label,
    dashboard_logo_color_image, dashboard_logo_size_for_test, dashboard_path_detail,
    dashboard_project_label, dashboard_recent_candidate_scan_limit, dashboard_recent_files,
    dashboard_recent_files_empty_label, dashboard_recent_files_with_display_cache,
    dashboard_recent_files_with_file_probe, dashboard_recent_projects,
    dashboard_recent_projects_empty_label, dashboard_recent_projects_with_dir_probe,
    dashboard_recent_projects_with_display_cache, dashboard_task_detail, dashboard_task_label,
    dashboard_workspace_task_scan_len, dashboard_workspace_tasks,
    dashboard_workspace_tasks_empty_label, dashboard_workspace_tasks_if_trusted,
    dashboard_workspace_tasks_with_display_cache, dashboard_workspace_tasks_with_display_text,
    sanitized_dashboard_detail, sanitized_dashboard_detail_cow, sanitized_dashboard_label,
    sanitized_dashboard_label_cow,
};
use crate::ui_text::truncate_middle;
use crate::workspace_tasks_runtime::workspace_task_fingerprint;
use kuroya_core::{WorkspaceTask, WorkspaceTaskKind};
use std::{
    borrow::Cow,
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    process,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn dashboard_logo_asset_is_transparent_square_mark() {
    let image = image::load_from_memory_with_format(DASHBOARD_LOGO_BYTES, image::ImageFormat::Png)
        .expect("dashboard logo should decode as png")
        .into_rgba8();
    let (width, height) = image.dimensions();
    let (pixels, _) = image.as_raw().as_chunks::<4>();
    let transparent_pixels = pixels.iter().filter(|pixel| pixel[3] == 0).count();

    assert_eq!((width, height), (320, 320));
    assert!(dashboard_logo_size_for_test() >= 64.0);
    for (x, y) in [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
    ] {
        assert_eq!(image.get_pixel(x, y)[3], 0);
    }
    assert!(transparent_pixels > (width as usize * height as usize) / 2);
    assert!(pixels.iter().any(|pixel| pixel[3] == 255));
}

#[test]
fn dashboard_logo_color_image_matches_render_texture_input() {
    let image = dashboard_logo_color_image().expect("dashboard logo color image should decode");
    let visible_pixels = image.pixels.iter().filter(|pixel| pixel.a() > 0).count();

    assert_eq!(image.size, [320, 320]);
    assert!(visible_pixels > 20_000);
    assert!(
        image
            .pixels
            .iter()
            .any(|pixel| pixel.r() > 180 && pixel.g() > 180 && pixel.b() > 180)
    );
    assert!(
        image
            .pixels
            .iter()
            .any(|pixel| pixel.b() > pixel.r().saturating_add(40))
    );
}

#[test]
fn dashboard_recent_files_are_workspace_scoped_deduped_and_bounded() {
    let root = unique_temp_dir("dashboard-recent-files");
    let main = root.join("src/main.rs");
    let lib = root.join("src/lib.rs");
    let readme = root.join("README.md");
    let outside = root.join("../other/src/main.rs");
    fs::create_dir_all(main.parent().unwrap()).unwrap();
    fs::write(&main, "fn main() {}\n").unwrap();
    fs::write(&lib, "pub fn lib() {}\n").unwrap();
    fs::write(&readme, "# workspace\n").unwrap();
    let recent = VecDeque::from([
        root.join("src/../src/main.rs"),
        outside,
        main.clone(),
        lib.clone(),
        readme.clone(),
    ]);

    let files = dashboard_recent_files(&root, &recent, 2);

    assert_eq!(
        files
            .into_iter()
            .map(|file| (file.path, file.label, file.detail.replace('\\', "/")))
            .collect::<Vec<_>>(),
        vec![
            (main, "main.rs".to_owned(), "src/main.rs".to_owned()),
            (lib, "lib.rs".to_owned(), "src/lib.rs".to_owned())
        ]
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dashboard_recent_files_honor_zero_limit() {
    let root = PathBuf::from("workspace");
    let recent = VecDeque::from([root.join("src/main.rs")]);

    assert!(dashboard_recent_files(&root, &recent, 0).is_empty());
}

#[test]
fn dashboard_recent_files_dedupe_display_equivalent_paths() {
    let root = unique_temp_dir("dashboard-recent-files-display-key");
    let main = root.join("src/main.rs");
    fs::create_dir_all(main.parent().unwrap()).unwrap();
    fs::write(&main, "fn main() {}\n").unwrap();

    let display_equivalent = if cfg!(windows) {
        root.join("SRC/MAIN.RS")
    } else {
        root.join("src/./main.rs")
    };
    let recent = VecDeque::from([display_equivalent, main.clone()]);

    let files = dashboard_recent_files(&root, &recent, 4);

    assert_eq!(files.len(), 1);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dashboard_recent_files_skip_duplicate_candidates_before_file_probe() {
    let root = PathBuf::from("workspace");
    let main = root.join("src/main.rs");
    let lib = root.join("src/lib.rs");
    let recent = VecDeque::from([root.join("src/../src/main.rs"), main.clone(), lib.clone()]);
    let mut probed = Vec::new();

    let files = {
        let mut file_probe = |path: &Path| {
            probed.push(path.to_path_buf());
            true
        };
        dashboard_recent_files_with_file_probe(&root, &recent, 4, &mut file_probe)
    };

    assert_eq!(probed, vec![main.clone(), lib.clone()]);
    assert_eq!(
        files.into_iter().map(|file| file.path).collect::<Vec<_>>(),
        vec![main, lib]
    );
}

#[test]
fn dashboard_recent_files_reuse_duplicate_stale_candidate_file_probe_results() {
    let root = PathBuf::from("workspace");
    let missing = root.join("missing.rs");
    let main = root.join("src/main.rs");
    let recent = VecDeque::from([
        root.join("src/../missing.rs"),
        missing.clone(),
        main.clone(),
    ]);
    let mut probed = Vec::new();

    let files = {
        let existing = main.clone();
        let mut file_probe = |path: &Path| {
            probed.push(path.to_path_buf());
            path == existing.as_path()
        };
        dashboard_recent_files_with_file_probe(&root, &recent, 4, &mut file_probe)
    };

    assert_eq!(probed, vec![missing, main.clone()]);
    assert_eq!(
        files.into_iter().map(|file| file.path).collect::<Vec<_>>(),
        vec![main]
    );
}

#[test]
fn dashboard_recent_files_empty_label_tracks_current_workspace_files() {
    let root = unique_temp_dir("dashboard-recent-files-empty");
    let recent = VecDeque::from([root.join("../other/src/main.rs")]);

    let files = dashboard_recent_files(&root, &recent, 2);

    assert_eq!(
        dashboard_recent_files_empty_label(&files),
        Some("No recent files in this workspace")
    );

    let main = root.join("src/main.rs");
    fs::create_dir_all(main.parent().unwrap()).unwrap();
    fs::write(&main, "fn main() {}\n").unwrap();
    let recent = VecDeque::from([main]);
    let files = dashboard_recent_files(&root, &recent, 2);

    assert_eq!(dashboard_recent_files_empty_label(&files), None);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dashboard_recent_candidate_scan_limit_bounds_ui_filesystem_probes() {
    assert_eq!(dashboard_recent_candidate_scan_limit(0, 500), 0);
    assert_eq!(dashboard_recent_candidate_scan_limit(4, 12), 12);
    assert_eq!(dashboard_recent_candidate_scan_limit(1, 500), 32);
    assert_eq!(dashboard_recent_candidate_scan_limit(16, 500), 128);
}

#[test]
fn dashboard_recent_files_stop_after_bounded_stale_candidate_scan() {
    let root = unique_temp_dir("dashboard-recent-files-scan-cap");
    let main = root.join("src/main.rs");
    fs::create_dir_all(main.parent().unwrap()).unwrap();
    fs::write(&main, "fn main() {}\n").unwrap();
    let scan_limit = dashboard_recent_candidate_scan_limit(1, 128);
    let mut recent = VecDeque::new();
    for index in 0..scan_limit {
        recent.push_back(root.join(format!("missing-{index}.rs")));
    }
    recent.push_back(main);

    let files = dashboard_recent_files(&root, &recent, 1);

    assert!(files.is_empty());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dashboard_path_truncation_keeps_middle_ellipsis() {
    assert_eq!(truncate_middle("short.rs", 16), "short.rs");
    assert_eq!(
        truncate_middle("abcdefghijklmnopqrstuvwxyz", 12),
        "abcd...vwxyz"
    );
}

#[test]
fn dashboard_recent_projects_skip_current_stale_paths_deduped_and_bounded() {
    let temp_root = unique_temp_dir("dashboard-recent-projects");
    let current = temp_root.join("current");
    let alpha = temp_root.join("alpha");
    let beta = temp_root.join("beta");
    let gamma = temp_root.join("gamma");
    let stale = temp_root.join("deleted");
    fs::create_dir_all(&current).unwrap();
    fs::create_dir_all(&alpha).unwrap();
    fs::create_dir_all(&beta).unwrap();
    fs::create_dir_all(&gamma).unwrap();
    let recent = vec![
        current.join("."),
        stale,
        alpha.join("nested/.."),
        alpha.clone(),
        beta.clone(),
        gamma,
    ];

    let recent_projects = dashboard_recent_projects(&current, &recent, 2);

    assert_eq!(recent_projects.skipped_current_count, 1);
    assert_eq!(recent_projects.stale_count, 1);
    assert_eq!(
        recent_projects
            .projects
            .into_iter()
            .map(|project| (project.path, project.label))
            .collect::<Vec<_>>(),
        vec![(alpha, "alpha".to_owned()), (beta, "beta".to_owned())]
    );

    fs::remove_dir_all(temp_root).unwrap();
}

#[test]
fn dashboard_recent_projects_reuse_duplicate_candidate_dir_probe_results() {
    let current = PathBuf::from("workspace/current");
    let alpha = PathBuf::from("workspace/alpha");
    let stale = PathBuf::from("workspace/deleted");
    let recent = vec![
        current.join("."),
        alpha.join("nested/.."),
        alpha.clone(),
        stale.join("."),
        stale.clone(),
    ];
    let mut probed = Vec::new();

    let recent_projects = {
        let existing = alpha.clone();
        let mut dir_probe = |path: &Path| {
            probed.push(path.to_path_buf());
            path == existing.as_path()
        };
        dashboard_recent_projects_with_dir_probe(&current, &recent, 4, &mut dir_probe)
    };

    assert_eq!(probed, vec![alpha.clone(), stale.clone()]);
    assert_eq!(recent_projects.skipped_current_count, 1);
    assert_eq!(recent_projects.stale_count, 2);
    assert_eq!(
        recent_projects
            .projects
            .into_iter()
            .map(|project| project.path)
            .collect::<Vec<_>>(),
        vec![alpha]
    );
}

#[test]
fn dashboard_recent_projects_stop_after_bounded_stale_candidate_scan() {
    let temp_root = unique_temp_dir("dashboard-recent-projects-scan-cap");
    let current = temp_root.join("current");
    let alpha = temp_root.join("alpha");
    fs::create_dir_all(&current).unwrap();
    fs::create_dir_all(&alpha).unwrap();
    let scan_limit = dashboard_recent_candidate_scan_limit(1, 128);
    let mut recent = Vec::new();
    for index in 0..scan_limit {
        recent.push(temp_root.join(format!("missing-{index}")));
    }
    recent.push(alpha);

    let recent_projects = dashboard_recent_projects(&current, &recent, 1);

    assert!(recent_projects.projects.is_empty());
    assert_eq!(recent_projects.stale_count, scan_limit);

    fs::remove_dir_all(temp_root).unwrap();
}

#[test]
fn dashboard_recent_projects_honor_zero_limit() {
    let temp_root = unique_temp_dir("dashboard-recent-project-limit");
    fs::create_dir_all(&temp_root).unwrap();

    assert!(
        dashboard_recent_projects(&temp_root, std::slice::from_ref(&temp_root), 0)
            .projects
            .is_empty()
    );

    fs::remove_dir_all(temp_root).unwrap();
}

#[test]
fn dashboard_recent_projects_empty_label_reports_current_stale_and_empty_states() {
    let temp_root = unique_temp_dir("dashboard-recent-project-empty");
    let current = temp_root.join("current");
    fs::create_dir_all(&current).unwrap();

    let current_only = dashboard_recent_projects(&current, std::slice::from_ref(&current), 6);
    assert_eq!(
        dashboard_recent_projects_empty_label(&current_only),
        Some("No other recent workspaces")
    );

    let stale = temp_root.join("missing");
    let stale_only = dashboard_recent_projects(&current, &[stale], 6);
    assert_eq!(
        dashboard_recent_projects_empty_label(&stale_only),
        Some("Recent workspaces no longer exist")
    );

    let empty = dashboard_recent_projects(&current, &[], 6);
    assert_eq!(
        dashboard_recent_projects_empty_label(&empty),
        Some("No recent workspaces yet")
    );

    fs::remove_dir_all(temp_root).unwrap();
}

#[test]
fn dashboard_file_and_project_labels_are_bounded() {
    let long_file = PathBuf::from(format!("{}.rs", "a".repeat(80)));
    let long_project = PathBuf::from("workspace").join("b".repeat(80));

    let file_label = dashboard_file_label(&long_file);
    let project_label = dashboard_project_label(&long_project);

    assert_eq!(file_label.chars().count(), DASHBOARD_LABEL_MAX_CHARS);
    assert!(file_label.contains("..."));
    assert_eq!(project_label.chars().count(), DASHBOARD_LABEL_MAX_CHARS);
    assert!(project_label.contains("..."));
}

#[test]
fn dashboard_label_and_detail_cow_borrow_clean_values() {
    let ascii_label = "src/main.rs";
    let unicode_label = "workspace-\u{03bb}";
    let ascii_detail = "src/main.rs";
    let unicode_detail = "workspace/\u{03bb}/main.rs";

    assert!(matches!(
        sanitized_dashboard_label_cow(ascii_label, "File"),
        Cow::Borrowed(label) if label == ascii_label
    ));
    assert!(matches!(
        sanitized_dashboard_label_cow(unicode_label, "File"),
        Cow::Borrowed(label) if label == unicode_label
    ));
    assert!(matches!(
        sanitized_dashboard_detail_cow(ascii_detail, "."),
        Cow::Borrowed(detail) if detail == ascii_detail
    ));
    assert!(matches!(
        sanitized_dashboard_detail_cow(unicode_detail, "."),
        Cow::Borrowed(detail) if detail == unicode_detail
    ));
}

#[test]
fn dashboard_label_and_detail_cow_own_dirty_truncated_and_fallback_values() {
    let long_label = "a".repeat(DASHBOARD_LABEL_MAX_CHARS + 16);
    let long_detail = "workspace/".to_owned() + &"b".repeat(DASHBOARD_DETAIL_MAX_CHARS + 16);
    let label_cases = [
        ("  clean.rs  ", "File"),
        ("bad\nlabel", "File"),
        ("\u{202e}", "File"),
        (long_label.as_str(), "File"),
    ];
    let detail_cases = [
        ("  src/main.rs  ", "."),
        ("src\r\nmain.rs", "."),
        ("\u{2066}", "."),
        (long_detail.as_str(), "."),
    ];

    for (value, fallback) in label_cases {
        let label = sanitized_dashboard_label_cow(value, fallback);

        assert_eq!(label.as_ref(), sanitized_dashboard_label(value, fallback));
        assert!(
            matches!(label, Cow::Owned(_)),
            "expected owned dashboard label for {value:?}"
        );
    }

    for (value, fallback) in detail_cases {
        let detail = sanitized_dashboard_detail_cow(value, fallback);

        assert_eq!(detail.as_ref(), sanitized_dashboard_detail(value, fallback));
        assert!(
            matches!(detail, Cow::Owned(_)),
            "expected owned dashboard detail for {value:?}"
        );
    }
}

#[test]
fn dashboard_labels_and_details_are_sanitized_for_display() {
    let root = PathBuf::from("workspace");
    let noisy_file = root.join(format!(
        "bad\n{}\u{202e}.rs",
        "file".repeat(DASHBOARD_LABEL_MAX_CHARS)
    ));
    let noisy_project = PathBuf::from(format!(
        "repo\r\n{}\u{2066}",
        "project".repeat(DASHBOARD_DETAIL_MAX_CHARS)
    ));
    let noisy_task = WorkspaceTask {
        name: format!(
            "Generate\n{}\u{202e}",
            "bundle".repeat(DASHBOARD_LABEL_MAX_CHARS)
        ),
        command: "cargo\r\n".to_owned(),
        args: vec![format!(
            "test\t{}\u{2066}",
            "arg".repeat(DASHBOARD_DETAIL_MAX_CHARS)
        )],
        cwd: None,
        env: BTreeMap::new(),
        kind: WorkspaceTaskKind::Run,
        default: true,
    };

    let file_label = dashboard_file_label(&noisy_file);
    let file_detail = dashboard_file_detail(&root, &noisy_file);
    let blank_project_name = PathBuf::from("\n\u{202e}");
    let project_label = dashboard_project_label(&blank_project_name);
    let project_detail = dashboard_path_detail(&noisy_project);
    let task_label = dashboard_task_label(&noisy_task);
    let task_detail = dashboard_task_detail(&noisy_task);

    assert_dashboard_text_is_safe(&file_label, DASHBOARD_LABEL_MAX_CHARS);
    assert_dashboard_text_is_safe(&file_detail, DASHBOARD_DETAIL_MAX_CHARS);
    assert_dashboard_text_is_safe(&project_label, DASHBOARD_LABEL_MAX_CHARS);
    assert_dashboard_text_is_safe(&project_detail, DASHBOARD_DETAIL_MAX_CHARS);
    assert_dashboard_text_is_safe(&task_label, DASHBOARD_LABEL_MAX_CHARS);
    assert_dashboard_text_is_safe(&task_detail, DASHBOARD_DETAIL_MAX_CHARS);
    assert_eq!(project_label, "Workspace");
    assert!(file_label.contains("..."));
    assert!(file_detail.contains("..."));
    assert!(project_detail.contains("..."));
    assert!(task_label.contains("..."));
    assert!(task_detail.contains("..."));
}

#[test]
fn dashboard_rows_preserve_raw_targets_when_display_text_is_prepared() {
    let temp_root = unique_temp_dir("dashboard-raw-targets");
    let current = temp_root.join("current");
    let file_name = format!("{}target.rs", "file-".repeat(12));
    let file_path = current.join("src").join(&file_name);
    let project_name = format!("{}target", "workspace-".repeat(7));
    let project_path = temp_root.join(&project_name);
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::create_dir_all(&project_path).unwrap();
    fs::write(&file_path, "fn main() {}\n").unwrap();

    let mut display_cache = DashboardDisplayTextCache::default();
    let recent_files = VecDeque::from([file_path.clone()]);
    let files =
        dashboard_recent_files_with_display_cache(&current, &recent_files, 1, &mut display_cache);
    let files_again =
        dashboard_recent_files_with_display_cache(&current, &recent_files, 1, &mut display_cache);
    assert_eq!(files_again, files);
    assert_eq!(display_cache.recent_files.len(), 1);
    assert_eq!(files[0].path, file_path);
    assert_ne!(files[0].label, file_name);
    assert!(files[0].label.contains("..."));
    assert!(files[0].detail.contains("..."));

    let projects = dashboard_recent_projects_with_display_cache(
        &current,
        std::slice::from_ref(&project_path),
        1,
        &mut display_cache,
    );
    assert_eq!(display_cache.recent_projects.len(), 1);
    assert_eq!(projects.projects[0].path, project_path);
    assert_ne!(projects.projects[0].label, project_name);
    assert!(projects.projects[0].label.contains("..."));
    assert!(projects.projects[0].detail.contains("..."));

    let task_name = format!("{}target", "task-".repeat(12));
    let tasks = vec![task(&task_name, WorkspaceTaskKind::Build, true, &["build"])];
    let dashboard_tasks =
        dashboard_workspace_tasks_with_display_cache(&tasks, 1, &mut display_cache);
    assert_eq!(display_cache.workspace_tasks.len(), 1);
    assert_eq!(dashboard_tasks[0].index, 0);
    assert_eq!(
        dashboard_tasks[0].fingerprint,
        workspace_task_fingerprint(&tasks[0])
    );
    assert!(dashboard_tasks[0].label.contains("..."));
    assert_eq!(dashboard_tasks[0].detail, "cargo build");

    fs::remove_dir_all(temp_root).unwrap();
}

#[test]
fn dashboard_recent_project_display_cache_keys_changed_display_paths() {
    let path = PathBuf::from("workspace");
    let mut display_cache = DashboardDisplayTextCache::default();

    let first = display_cache.recent_project_display_text(&path, "workspace");
    let second = display_cache.recent_project_display_text(&path, "workspace\nrenamed");

    assert_eq!(first.detail, "workspace");
    assert_eq!(second.detail, "workspace renamed");
    assert_eq!(display_cache.recent_projects.len(), 2);
}

#[test]
fn dashboard_workspace_tasks_prioritize_default_build_test_and_run() {
    let tasks = vec![
        task("Lint", WorkspaceTaskKind::Custom, false, &["lint"]),
        task("Build Debug", WorkspaceTaskKind::Build, false, &["build"]),
        task(
            "Test All",
            WorkspaceTaskKind::Test,
            true,
            &["test", "--all"],
        ),
        task("Run App", WorkspaceTaskKind::Run, true, &["run"]),
        task(
            "Build Release",
            WorkspaceTaskKind::Build,
            true,
            &["build", "--release"],
        ),
    ];

    let dashboard_tasks = dashboard_workspace_tasks(&tasks, 3);

    assert_eq!(
        dashboard_tasks
            .into_iter()
            .map(|task| (task.index, task.label, task.detail))
            .collect::<Vec<_>>(),
        vec![
            (
                4,
                "Build default Build Release".to_owned(),
                "cargo build --release".to_owned()
            ),
            (
                2,
                "Test default Test All".to_owned(),
                "cargo test --all".to_owned()
            ),
            (3, "Run default Run App".to_owned(), "cargo run".to_owned())
        ]
    );
}

#[test]
fn dashboard_workspace_tasks_fill_remaining_slots_in_source_order() {
    let tasks = vec![
        task("Lint", WorkspaceTaskKind::Custom, false, &["lint"]),
        task("Build", WorkspaceTaskKind::Build, true, &["build"]),
        task("Format", WorkspaceTaskKind::Custom, false, &["fmt"]),
    ];

    let dashboard_tasks = dashboard_workspace_tasks(&tasks, 3);

    assert_eq!(
        dashboard_tasks
            .into_iter()
            .map(|task| task.index)
            .collect::<Vec<_>>(),
        vec![1, 0, 2]
    );
}

#[test]
fn dashboard_workspace_tasks_prepare_display_text_once_per_selected_task() {
    let tasks = vec![
        task("Lint", WorkspaceTaskKind::Custom, false, &["lint"]),
        task("Build", WorkspaceTaskKind::Build, true, &["build"]),
        task("Format", WorkspaceTaskKind::Custom, false, &["fmt"]),
        task("Test", WorkspaceTaskKind::Test, true, &["test"]),
    ];
    let mut display_calls = Vec::new();
    let mut display_text = |task: &WorkspaceTask| {
        display_calls.push(task.name.clone());
        DashboardDisplayText {
            label: task.name.clone(),
            detail: task.command.clone(),
        }
    };

    let dashboard_tasks = dashboard_workspace_tasks_with_display_text(&tasks, 4, &mut display_text);

    assert_eq!(
        dashboard_tasks
            .into_iter()
            .map(|task| task.index)
            .collect::<Vec<_>>(),
        vec![1, 3, 0, 2]
    );
    assert_eq!(
        display_calls,
        vec![
            "Build".to_owned(),
            "Test".to_owned(),
            "Lint".to_owned(),
            "Format".to_owned()
        ]
    );
}

#[test]
fn dashboard_workspace_tasks_honor_zero_limit() {
    let tasks = vec![task("Build", WorkspaceTaskKind::Build, true, &["build"])];

    assert!(dashboard_workspace_tasks(&tasks, 0).is_empty());
}

#[test]
fn dashboard_workspace_tasks_ignore_entries_beyond_scan_cap() {
    let mut tasks = (0..DASHBOARD_WORKSPACE_TASK_SCAN_MAX)
        .map(|index| {
            task(
                &format!("Task {index}"),
                WorkspaceTaskKind::Custom,
                false,
                &["custom"],
            )
        })
        .collect::<Vec<_>>();
    tasks.push(task(
        "Late default build",
        WorkspaceTaskKind::Build,
        true,
        &["build"],
    ));

    let dashboard_tasks = dashboard_workspace_tasks(&tasks, 1);

    assert_eq!(
        dashboard_workspace_task_scan_len(usize::MAX),
        DASHBOARD_WORKSPACE_TASK_SCAN_MAX
    );
    assert_eq!(dashboard_tasks.len(), 1);
    assert_eq!(dashboard_tasks[0].index, 0);
}

#[test]
fn dashboard_workspace_tasks_skip_selection_until_workspace_trusted() {
    let tasks = vec![task("Build", WorkspaceTaskKind::Build, true, &["build"])];

    assert_eq!(
        dashboard_workspace_tasks_if_trusted(false, &tasks, DASHBOARD_WORKSPACE_TASK_LIMIT),
        None
    );

    let trusted_tasks =
        dashboard_workspace_tasks_if_trusted(true, &tasks, DASHBOARD_WORKSPACE_TASK_LIMIT).unwrap();
    assert_eq!(
        trusted_tasks
            .into_iter()
            .map(|task| task.index)
            .collect::<Vec<_>>(),
        vec![0]
    );
}

#[test]
fn dashboard_workspace_tasks_empty_label_tracks_empty_and_untrusted_states() {
    let empty_tasks = dashboard_workspace_tasks(&[], DASHBOARD_WORKSPACE_TASK_LIMIT);

    assert_eq!(
        dashboard_workspace_tasks_empty_label(true, &empty_tasks),
        Some("No workspace tasks found")
    );
    assert_eq!(
        dashboard_workspace_tasks_empty_label(false, &empty_tasks),
        Some("Workspace tasks hidden until this workspace is trusted")
    );

    let tasks = vec![task("Build", WorkspaceTaskKind::Build, true, &["build"])];
    let dashboard_tasks = dashboard_workspace_tasks(&tasks, DASHBOARD_WORKSPACE_TASK_LIMIT);

    assert_eq!(
        dashboard_workspace_tasks_empty_label(false, &dashboard_tasks),
        Some("Workspace tasks hidden until this workspace is trusted")
    );
    assert_eq!(
        dashboard_workspace_tasks_empty_label(true, &dashboard_tasks),
        None
    );
}

fn task(name: &str, kind: WorkspaceTaskKind, default: bool, args: &[&str]) -> WorkspaceTask {
    WorkspaceTask {
        name: name.to_owned(),
        command: "cargo".to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: None,
        env: BTreeMap::new(),
        kind,
        default,
    }
}

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", process::id()))
}

fn assert_dashboard_text_is_safe(text: &str, max_chars: usize) {
    assert!(
        text.chars().count() <= max_chars,
        "dashboard text should be bounded: {text:?}"
    );
    assert!(
        !text.chars().any(char::is_control),
        "dashboard text should not contain controls: {text:?}"
    );
    assert!(
        !text.chars().any(is_bidi_format_control),
        "dashboard text should not contain bidi controls: {text:?}"
    );
}

fn is_bidi_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

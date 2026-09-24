use super::super::{
    JUSTFILE_MAX_BYTES, MAKEFILE_MAX_BYTES, PACKAGE_MANIFEST_MAX_BYTES,
    TASK_DISPLAY_LABEL_MAX_CHARS, TASK_DISPLAY_TRUNCATION_MARKER, WorkspaceTaskKind,
    load_workspace_tasks, makefile_targets, read_first_regular_utf8_file_with_limit,
};
use super::unique_test_dir;
use std::{collections::BTreeSet, fs, io, path::Path};

#[test]
fn load_workspace_tasks_infers_makefile_targets_when_config_is_missing() {
    let root = unique_test_dir("kuroya-make-tasks");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Makefile"),
        "build:\n\tcargo build\n\ntest run:\n\tcargo test\n",
    )
    .unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert_eq!(
        tasks
            .iter()
            .map(|task| (
                task.name.as_str(),
                task.kind,
                task.command.as_str(),
                task.args.iter().map(String::as_str).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "Make Build",
                WorkspaceTaskKind::Build,
                "make",
                vec!["build"]
            ),
            ("Make Test", WorkspaceTaskKind::Test, "make", vec!["test"]),
            ("Make Run", WorkspaceTaskKind::Run, "make", vec!["run"]),
        ]
    );

    let _ = fs::remove_dir_all(root);
}

#[cfg(target_os = "linux")]
#[test]
fn load_workspace_tasks_falls_back_from_non_file_makefile_candidate() {
    let root = unique_test_dir("kuroya-makefile-directory-fallback");
    fs::create_dir_all(root.join("Makefile")).unwrap();
    fs::write(root.join("makefile"), "build:\n\tcargo build\n").unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert_eq!(
        tasks
            .iter()
            .map(|task| task.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Make Build"]
    );

    let _ = fs::remove_dir_all(root);
}

#[cfg(target_os = "linux")]
#[test]
fn load_workspace_tasks_stops_at_oversized_makefile_candidate() {
    let root = unique_test_dir("kuroya-oversized-makefile-order");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Makefile"),
        vec![b'a'; usize::try_from(MAKEFILE_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();
    fs::write(root.join("makefile"), "build:\n\tcargo build\n").unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert!(tasks.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_workspace_tasks_ignores_makefile_variable_assignments_when_inferring_targets() {
    let root = unique_test_dir("kuroya-make-assignment-tasks");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Makefile"),
        "build := cargo build\nbuild : = cargo build\ntest ::= cargo test\nrun :::= cargo run\n",
    )
    .unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert!(tasks.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_workspace_tasks_ignores_makefile_recipe_lines_when_inferring_targets() {
    let root = unique_test_dir("kuroya-make-recipe-tasks");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("Makefile"),
        "other:\n\t@echo build: not a target\n\t@echo test run: still not targets\n",
    )
    .unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert!(tasks.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn makefile_targets_collects_rules_without_assignments_or_recipes() {
    let targets = makefile_targets(concat!(
        "# ignored comment\n",
        ".PHONY: build\n",
        "build test: src/lib.rs\n",
        "run::\n",
        "\t@echo deploy: not a target\n",
        "deploy := ./deploy.sh\n",
        "lint :::= eslint .\n",
    ));

    assert_eq!(targets, BTreeSet::from(["build", "run", "test"]));
}

#[test]
fn discovery_reader_falls_back_from_non_file_candidate() {
    let root = unique_test_dir("kuroya-discovery-reader-fallback");
    fs::create_dir_all(root.join("first")).unwrap();
    fs::write(root.join("second"), "build:\n").unwrap();

    let text =
        read_first_regular_utf8_file_with_limit(&root, &["first", "second"], MAKEFILE_MAX_BYTES)
            .unwrap()
            .unwrap();

    assert_eq!(text, "build:\n");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_reader_accepts_symlink_to_regular_candidate() {
    #[cfg(unix)]
    fn symlink_file(target: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn symlink_file(target: &Path, link: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_file(target, link)
    }

    let root = unique_test_dir("kuroya-discovery-reader-symlink");
    fs::create_dir_all(&root).unwrap();
    let target = root.join("actual");
    let link = root.join("linked");
    fs::write(&target, "build:\n").unwrap();
    if symlink_file(&target, &link).is_err() {
        let _ = fs::remove_dir_all(root);
        return;
    }

    let text = read_first_regular_utf8_file_with_limit(&root, &["linked"], MAKEFILE_MAX_BYTES)
        .unwrap()
        .unwrap();

    assert_eq!(text, "build:\n");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_reader_stops_at_oversized_regular_candidate() {
    let root = unique_test_dir("kuroya-discovery-reader-oversized");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("first"),
        vec![b'a'; usize::try_from(MAKEFILE_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();
    fs::write(root.join("second"), "build:\n").unwrap();

    let error =
        read_first_regular_utf8_file_with_limit(&root, &["first", "second"], MAKEFILE_MAX_BYTES)
            .unwrap()
            .unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::InvalidData);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_workspace_tasks_infers_justfile_targets_when_config_is_missing() {
    let root = unique_test_dir("kuroya-just-tasks");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("justfile"),
        concat!(
            "# ignored comment\n",
            "export profile := \"debug\"\n",
            "\n",
            "build:\n",
            "    cargo build\n",
            "\n",
            "test profile=\"debug\":\n",
            "    cargo test\n",
            "\n",
            "dev:\n",
            "    cargo run\n",
            "\n",
            "type-check:\n",
            "    cargo check\n",
            "\n",
            "deploy: build test\n",
            "    ./deploy.sh\n",
            "\n",
            "_private:\n",
            "    echo hidden\n",
            "\n",
            "build:\n",
            "    echo duplicate\n",
        ),
    )
    .unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert_eq!(
        tasks
            .iter()
            .map(|task| (
                task.name.as_str(),
                task.kind,
                task.command.as_str(),
                task.args.iter().map(String::as_str).collect::<Vec<_>>(),
                task.default
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                "Just Build",
                WorkspaceTaskKind::Build,
                "just",
                vec!["build"],
                true
            ),
            (
                "Just Test",
                WorkspaceTaskKind::Test,
                "just",
                vec!["test"],
                true
            ),
            (
                "Just Dev",
                WorkspaceTaskKind::Run,
                "just",
                vec!["dev"],
                true
            ),
            (
                "Just Type Check",
                WorkspaceTaskKind::Custom,
                "just",
                vec!["type-check"],
                false
            ),
            (
                "Just Deploy",
                WorkspaceTaskKind::Custom,
                "just",
                vec!["deploy"],
                false
            ),
        ]
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_workspace_tasks_falls_back_from_non_file_justfile_candidate() {
    let root = unique_test_dir("kuroya-justfile-directory-fallback");
    fs::create_dir_all(root.join("justfile")).unwrap();
    fs::write(root.join(".justfile"), "build:\n    cargo build\n").unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert_eq!(
        tasks
            .iter()
            .map(|task| task.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Just Build"]
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn load_workspace_tasks_stops_at_oversized_justfile_candidate() {
    let root = unique_test_dir("kuroya-oversized-justfile-order");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("justfile"),
        vec![b'a'; usize::try_from(JUSTFILE_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();
    fs::write(root.join(".justfile"), "build:\n    cargo build\n").unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert!(tasks.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn inferred_justfile_tasks_bound_huge_target_display_without_rewriting_arg() {
    let root = unique_test_dir("kuroya-huge-just-target-tasks");
    let target = format!("deploy-{}", "x".repeat(TASK_DISPLAY_LABEL_MAX_CHARS * 2));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("justfile"),
        format!("{target}:\n    ./deploy.sh\n"),
    )
    .unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].name.chars().count() <= TASK_DISPLAY_LABEL_MAX_CHARS);
    assert!(tasks[0].name.ends_with(TASK_DISPLAY_TRUNCATION_MARKER));
    assert_eq!(tasks[0].args, vec![target]);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn inferred_tasks_skip_oversized_package_json_makefile_and_justfile() {
    let root = unique_test_dir("kuroya-oversized-inferred-tasks");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("package.json"),
        vec![b'a'; usize::try_from(PACKAGE_MANIFEST_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();
    fs::write(
        root.join("Makefile"),
        vec![b'a'; usize::try_from(MAKEFILE_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();
    fs::write(
        root.join("justfile"),
        vec![b'a'; usize::try_from(JUSTFILE_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();

    let tasks = load_workspace_tasks(&root).unwrap();

    assert!(tasks.is_empty());

    let _ = fs::remove_dir_all(root);
}

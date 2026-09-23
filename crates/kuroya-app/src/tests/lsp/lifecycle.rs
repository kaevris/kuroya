use crate::{
    KuroyaApp,
    app_startup_context::AppStartupContext,
    large_file_mode::{
        LARGE_FILE_MODE_MAX_LINES, LARGE_FILE_PERFORMANCE_MODE_MAX_LINES,
        buffer_uses_large_file_mode, buffer_uses_large_file_performance_mode,
    },
    lsp_lifecycle::{
        BackgroundLanguageBlockReason, background_language_block_reason,
        buffer_allows_background_language, due_language_sync_ids, lsp_lifecycle_target_for_buffer,
        lsp_lifecycle_targets_for_buffers, lsp_server_config_for_buffer,
        lsp_server_configs_for_buffer, open_lsp_workspace_edit_block_reason,
    },
    lsp_runtime::{
        LSP_RESTART_BASE_DELAY, LspRestartDecision, clear_pending_lsp_restart_for_started_client,
        due_lsp_restart_languages, due_lsp_symbol_refresh_ids, lsp_restart_buffer_ids,
        lsp_restart_decision, lsp_restart_delay, pending_lsp_restart_should_run,
        schedule_lsp_restart_at,
    },
    terminal::TerminalPane,
};
use kuroya_core::{EditorSettings, LspServerConfig, PluginLanguageRegistry, TextBuffer, Workspace};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::runtime::Runtime;

#[test]
fn lsp_lifecycle_target_uses_existing_server_key_for_safe_buffers() {
    let buffer = TextBuffer::from_text(
        7,
        Some(PathBuf::from("workspace/src/lib.rs")),
        "fn main() {}".to_owned(),
    );

    let configs = lsp_configs();
    let target = lsp_lifecycle_target_for_buffer(
        &buffer,
        &configs,
        &PluginLanguageRegistry::default(),
        &HashSet::new(),
        &HashSet::new(),
    )
    .unwrap();
    assert_eq!(target.0, "rust");
    assert!(
        target
            .1
            .ends_with(Path::new("workspace").join("src").join("lib.rs"))
    );
}

#[test]
fn lsp_lifecycle_target_uses_custom_extension_server_for_plain_text_buffers() {
    let buffer = TextBuffer::from_text(
        17,
        Some(PathBuf::from("workspace/src/main.gleam")),
        "pub fn main() { Nil }".to_owned(),
    );
    let mut configs = lsp_configs();
    configs.push(LspServerConfig {
        language: "gleam".to_owned(),
        command: "gleam".to_owned(),
        args: vec!["lsp".to_owned()],
        extensions: vec!["gleam".to_owned()],
        root_markers: vec!["gleam.toml".to_owned()],
        enabled: true,
    });

    let target = lsp_lifecycle_target_for_buffer(
        &buffer,
        &configs,
        &PluginLanguageRegistry::default(),
        &HashSet::new(),
        &HashSet::new(),
    )
    .unwrap();

    assert_eq!(target.0, "gleam");
    assert!(target.1.ends_with(Path::new("workspace/src/main.gleam")));
}

#[test]
fn lsp_lifecycle_uses_precise_builtin_language_id_for_react_buffers() {
    let buffer = TextBuffer::from_text(
        18,
        Some(PathBuf::from("workspace/src/App.tsx")),
        "export function App() { return <main />; }".to_owned(),
    );
    let configs = lsp_configs();
    let plugin_languages = PluginLanguageRegistry::default();

    let (config, language) = lsp_server_config_for_buffer(&configs, &plugin_languages, &buffer)
        .expect("tsx should use the TypeScript server");

    assert_eq!(config.language, "typescript");
    assert_eq!(language.as_ref(), "typescriptreact");
}

#[test]
fn lsp_lifecycle_custom_extension_server_can_override_builtin_extension() {
    let buffer = TextBuffer::from_text(
        19,
        Some(PathBuf::from("workspace/src/App.tsx")),
        "export function App() { return <main />; }".to_owned(),
    );
    let mut configs = lsp_configs();
    configs.push(LspServerConfig {
        language: "custom-tsx".to_owned(),
        command: "custom-tsx-lsp".to_owned(),
        args: Vec::new(),
        extensions: vec!["tsx".to_owned()],
        root_markers: Vec::new(),
        enabled: true,
    });
    let plugin_languages = PluginLanguageRegistry::default();

    let (config, language) = lsp_server_config_for_buffer(&configs, &plugin_languages, &buffer)
        .expect("custom tsx server should be selected");

    assert_eq!(config.language, "custom-tsx");
    assert_eq!(language.as_ref(), "custom-tsx");
}

#[test]
fn lsp_server_configs_for_buffer_returns_every_matching_config_in_order() {
    let buffer = TextBuffer::from_text(
        21,
        Some(PathBuf::from("workspace/src/main.rs")),
        "fn main() {}".to_owned(),
    );
    let rust_a = LspServerConfig {
        language: "rust".to_owned(),
        command: "rust-analyzer".to_owned(),
        args: Vec::new(),
        extensions: Vec::new(),
        root_markers: vec!["Cargo.toml".to_owned()],
        enabled: true,
    };
    let rust_a_duplicate = rust_a.clone();
    let rust_b = LspServerConfig {
        language: "rust".to_owned(),
        command: "rust-analyzer-obsidian".to_owned(),
        args: vec!["--stdio".to_owned()],
        extensions: Vec::new(),
        root_markers: Vec::new(),
        enabled: true,
    };
    let python = LspServerConfig {
        language: "python".to_owned(),
        command: "pyright-langserver".to_owned(),
        args: vec!["--stdio".to_owned()],
        extensions: Vec::new(),
        root_markers: Vec::new(),
        enabled: true,
    };
    let configs = vec![rust_a, python, rust_b, rust_a_duplicate];
    let plugin_languages = PluginLanguageRegistry::default();

    let matches = lsp_server_configs_for_buffer(&configs, &plugin_languages, &buffer);

    assert_eq!(
        matches
            .iter()
            .map(|(config, _)| (config.language.as_str(), config.command.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("rust", "rust-analyzer"),
            ("rust", "rust-analyzer-obsidian")
        ]
    );
    for (config, language) in &matches {
        assert_eq!(config.language, "rust");
        assert_eq!(language.as_ref(), "rust");
    }

    let (primary, _) = lsp_server_config_for_buffer(&configs, &plugin_languages, &buffer)
        .expect("rust config should be present");
    assert_eq!(primary.command, "rust-analyzer");
}

#[test]
fn lsp_lifecycle_target_skips_protected_or_unsupported_buffers() {
    let rust = TextBuffer::from_text(
        8,
        Some(PathBuf::from("workspace/src/main.rs")),
        "fn main() {}".to_owned(),
    );
    let text = TextBuffer::from_text(
        9,
        Some(PathBuf::from("workspace/notes.txt")),
        "notes".to_owned(),
    );
    let configs = lsp_configs();

    assert_eq!(
        lsp_lifecycle_target_for_buffer(
            &rust,
            &configs,
            &PluginLanguageRegistry::default(),
            &HashSet::from([8]),
            &HashSet::new(),
        ),
        None
    );
    assert_eq!(
        lsp_lifecycle_target_for_buffer(
            &rust,
            &configs,
            &PluginLanguageRegistry::default(),
            &HashSet::new(),
            &HashSet::from([8]),
        ),
        None
    );
    assert_eq!(
        lsp_lifecycle_target_for_buffer(
            &text,
            &configs,
            &PluginLanguageRegistry::default(),
            &HashSet::new(),
            &HashSet::new(),
        ),
        None
    );
}

#[test]
fn background_language_block_reason_covers_protected_buffers() {
    let rust = TextBuffer::from_text(
        18,
        Some(PathBuf::from("workspace/src/main.rs")),
        "fn main() {}".to_owned(),
    );
    let large_text = std::iter::repeat_n("x", LARGE_FILE_MODE_MAX_LINES + 1)
        .collect::<Vec<_>>()
        .join("\n");
    let large = TextBuffer::from_text(
        19,
        Some(PathBuf::from("workspace/src/large.rs")),
        large_text,
    );
    let performance_text = std::iter::repeat_n("x", LARGE_FILE_PERFORMANCE_MODE_MAX_LINES + 1)
        .collect::<Vec<_>>()
        .join("\n");
    let performance = TextBuffer::from_text(
        20,
        Some(PathBuf::from("workspace/src/performance.rs")),
        performance_text,
    );

    assert_eq!(
        background_language_block_reason(18, &rust, &HashSet::new(), &HashSet::from([18])),
        Some(BackgroundLanguageBlockReason::BinaryPreview)
    );
    assert_eq!(
        background_language_block_reason(18, &rust, &HashSet::from([18]), &HashSet::new()),
        Some(BackgroundLanguageBlockReason::LossyDecoded)
    );
    assert_eq!(
        background_language_block_reason(19, &large, &HashSet::new(), &HashSet::new()),
        Some(BackgroundLanguageBlockReason::LargeFileMode)
    );
    assert!(!buffer_uses_large_file_mode(&performance));
    assert!(buffer_uses_large_file_performance_mode(&performance));
    assert!(!buffer_allows_background_language(&performance));
    assert_eq!(
        background_language_block_reason(20, &performance, &HashSet::new(), &HashSet::new()),
        Some(BackgroundLanguageBlockReason::LargeBuffer)
    );
    assert_eq!(
        lsp_lifecycle_target_for_buffer(
            &large,
            &lsp_configs(),
            &PluginLanguageRegistry::default(),
            &HashSet::new(),
            &HashSet::new(),
        ),
        None
    );
    assert_eq!(
        lsp_lifecycle_target_for_buffer(
            &performance,
            &lsp_configs(),
            &PluginLanguageRegistry::default(),
            &HashSet::new(),
            &HashSet::new()
        ),
        None
    );
}

#[test]
fn lsp_lifecycle_targets_collect_safe_open_buffers_for_teardown() {
    let rust = TextBuffer::from_text(
        10,
        Some(PathBuf::from("workspace/src/main.rs")),
        "fn main() {}".to_owned(),
    );
    let python = TextBuffer::from_text(
        11,
        Some(PathBuf::from("workspace/app.py")),
        "print('hello')".to_owned(),
    );
    let notes = TextBuffer::from_text(
        12,
        Some(PathBuf::from("workspace/notes.txt")),
        "notes".to_owned(),
    );

    let configs = lsp_configs();
    let targets = lsp_lifecycle_targets_for_buffers(
        &[rust, python, notes],
        &configs,
        &PluginLanguageRegistry::default(),
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].0, "rust");
    assert_eq!(targets[1].0, "python");
    assert!(targets[0].1.ends_with(Path::new("workspace/src/main.rs")));
    assert!(targets[1].1.ends_with(Path::new("workspace/app.py")));
}

#[test]
fn lsp_restart_buffer_ids_collect_safe_buffers_for_language() {
    let root = unique_test_root("lsp-restart-buffer-ids");
    std::fs::create_dir_all(root.join("src")).expect("create rust test workspace");
    std::fs::write(root.join("Cargo.toml"), b"[package]\nname = \"fixture\"\n")
        .expect("write rust root marker");
    std::fs::write(root.join("src/main.rs"), b"fn main() {}").expect("write rust source fixture");
    let rust = TextBuffer::from_text(
        10,
        Some(root.join("src/main.rs")),
        "fn main() {}".to_owned(),
    );
    let python = TextBuffer::from_text(11, Some(root.join("app.py")), "print('hello')".to_owned());
    let notes = TextBuffer::from_text(12, Some(root.join("notes.txt")), "notes".to_owned());

    let buffers = vec![rust, python, notes];
    let configs = lsp_configs();
    let ids = lsp_restart_buffer_ids(
        "rust",
        &buffers,
        &configs,
        &PluginLanguageRegistry::default(),
        &root,
        &HashSet::new(),
        &HashSet::new(),
    );

    assert_eq!(ids, vec![10]);
}

#[test]
fn lsp_restart_buffer_ids_skip_protected_buffers() {
    let rust = TextBuffer::from_text(
        13,
        Some(PathBuf::from("src/main.rs")),
        "fn main() {}".to_owned(),
    );
    let buffers = vec![rust];
    let configs = lsp_configs();

    assert!(
        lsp_restart_buffer_ids(
            "rust",
            &buffers,
            &configs,
            &PluginLanguageRegistry::default(),
            Path::new("."),
            &HashSet::from([13]),
            &HashSet::new(),
        )
        .is_empty()
    );
    assert!(
        lsp_restart_buffer_ids(
            "rust",
            &buffers,
            &configs,
            &PluginLanguageRegistry::default(),
            Path::new("."),
            &HashSet::new(),
            &HashSet::from([13]),
        )
        .is_empty()
    );
}

#[test]
fn lsp_restart_decision_does_not_spend_attempts_without_buffers() {
    assert_eq!(
        lsp_restart_decision(Some(3), 0, 3),
        LspRestartDecision::NoEligibleBuffers
    );
}

#[test]
fn lsp_restart_decision_counts_only_real_restarts() {
    assert_eq!(
        lsp_restart_decision(None, 2, 3),
        LspRestartDecision::Restart { attempt: 1 }
    );
    assert_eq!(
        lsp_restart_decision(Some(2), 1, 3),
        LspRestartDecision::Restart { attempt: 3 }
    );
    assert_eq!(
        lsp_restart_decision(Some(3), 1, 3),
        LspRestartDecision::Disable
    );
}

#[test]
fn lsp_restart_backoff_scales_by_attempt() {
    assert_eq!(
        lsp_restart_delay(1, LSP_RESTART_BASE_DELAY),
        Duration::from_millis(250)
    );
    assert_eq!(
        lsp_restart_delay(2, LSP_RESTART_BASE_DELAY),
        Duration::from_millis(500)
    );
    assert_eq!(
        lsp_restart_delay(3, LSP_RESTART_BASE_DELAY),
        Duration::from_millis(1000)
    );
}

#[test]
fn lsp_restart_schedule_and_due_languages_are_stable() {
    let now = Instant::now();
    let mut pending = HashMap::new();
    pending.insert("python".to_owned(), now + Duration::from_millis(10));
    pending.insert("rust".to_owned(), schedule_lsp_restart_at(now, 1));

    assert_eq!(
        due_lsp_restart_languages(&pending, now + Duration::from_millis(20)),
        vec!["python".to_owned()]
    );
    assert_eq!(
        due_lsp_restart_languages(&pending, now + LSP_RESTART_BASE_DELAY),
        vec!["python".to_owned(), "rust".to_owned()]
    );
}

#[test]
fn pending_lsp_restart_skips_stale_or_unavailable_servers() {
    assert!(pending_lsp_restart_should_run(true, false, false));
    assert!(!pending_lsp_restart_should_run(false, false, false));
    assert!(!pending_lsp_restart_should_run(true, true, false));
    assert!(!pending_lsp_restart_should_run(true, false, true));
}

#[test]
fn started_lsp_client_clears_only_matching_pending_restart() {
    let now = Instant::now();
    let mut pending = HashMap::from([
        ("python".to_owned(), now + Duration::from_secs(1)),
        ("rust".to_owned(), now + Duration::from_secs(2)),
    ]);

    assert!(clear_pending_lsp_restart_for_started_client(
        &mut pending,
        "rust"
    ));
    assert!(!clear_pending_lsp_restart_for_started_client(
        &mut pending,
        "rust"
    ));
    assert_eq!(
        pending,
        HashMap::from([("python".to_owned(), now + Duration::from_secs(1))])
    );
}

#[test]
fn unavailable_pending_lsp_restart_clears_stale_attempt_state() {
    let root = std::env::temp_dir().join("kuroya-lsp-unavailable-pending-restart");
    let mut app = app_for_test(root);
    app.lsp_unavailable.insert("rust".to_owned());
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now() - Duration::from_secs(1));

    assert_eq!(app.flush_pending_lsp_restarts(), 0);
    assert!(!app.pending_lsp_restarts.contains_key("rust"));
    assert!(!app.lsp_restart_attempts.contains_key("rust"));
    assert!(app.lsp_unavailable.contains("rust"));
}

#[test]
fn restricted_workspace_pending_lsp_restart_clears_stale_attempt_state() {
    let root = std::env::temp_dir().join("kuroya-lsp-restricted-pending-restart");
    let mut app = app_for_test(root);
    app.workspace_trusted = false;
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now() - Duration::from_secs(1));

    assert_eq!(app.flush_pending_lsp_restarts(), 0);
    assert!(!app.pending_lsp_restarts.contains_key("rust"));
    assert!(!app.lsp_restart_attempts.contains_key("rust"));
    assert!(!app.lsp_clients.contains_key("rust"));
    assert_eq!(
        app.status,
        "rust LSP restart skipped; workspace is restricted"
    );
}

#[test]
fn lsp_symbol_refresh_ids_become_due_after_debounce() {
    let now = Instant::now();
    let pending = HashMap::from([
        (2, now - Duration::from_millis(260)),
        (1, now - Duration::from_millis(260)),
        (3, now - Duration::from_millis(80)),
        (4, now + Duration::from_millis(40)),
    ]);

    assert_eq!(
        due_lsp_symbol_refresh_ids(&pending, now, Duration::from_millis(240)),
        vec![1, 2]
    );
    assert!(due_lsp_symbol_refresh_ids(&pending, now, Duration::from_millis(300)).is_empty());
}

#[test]
fn open_lsp_workspace_edits_block_unsafe_buffers() {
    let clean = TextBuffer::from_text(
        1,
        Some(PathBuf::from("workspace/clean.rs")),
        "clean".to_owned(),
    );
    let mut changed = TextBuffer::from_text(
        2,
        Some(PathBuf::from("workspace/changed.rs")),
        "changed".to_owned(),
    );
    changed.insert_at_cursor("!");
    let lossy = TextBuffer::from_text(
        3,
        Some(PathBuf::from("workspace/lossy.dat")),
        "ok\u{FFFD}".to_owned(),
    );
    let binary = TextBuffer::from_text(
        4,
        Some(PathBuf::from("workspace/binary.dat")),
        "ok\0".to_owned(),
    );
    let large_text = std::iter::repeat_n("x", LARGE_FILE_MODE_MAX_LINES + 1)
        .collect::<Vec<_>>()
        .join("\n");
    let large = TextBuffer::from_text(5, Some(PathBuf::from("workspace/large.rs")), large_text);
    let performance_text = std::iter::repeat_n("x", LARGE_FILE_PERFORMANCE_MODE_MAX_LINES + 1)
        .collect::<Vec<_>>()
        .join("\n");
    let performance = TextBuffer::from_text(
        6,
        Some(PathBuf::from("workspace/performance.rs")),
        performance_text,
    );
    let buffers = vec![clean, changed, lossy, binary, large, performance];
    let changed_on_disk = HashSet::from([2]);
    let lossy_buffers = HashSet::from([3]);
    let binary_buffers = HashSet::from([4]);

    assert_eq!(
        open_lsp_workspace_edit_block_reason(
            1,
            &changed_on_disk,
            &lossy_buffers,
            &binary_buffers,
            &buffers
        ),
        None
    );
    assert_eq!(
        open_lsp_workspace_edit_block_reason(
            2,
            &changed_on_disk,
            &lossy_buffers,
            &binary_buffers,
            &buffers
        ),
        Some("changed on disk")
    );
    assert_eq!(
        open_lsp_workspace_edit_block_reason(
            3,
            &changed_on_disk,
            &lossy_buffers,
            &binary_buffers,
            &buffers
        ),
        Some("UTF-8 replacement preview")
    );
    assert_eq!(
        open_lsp_workspace_edit_block_reason(
            4,
            &changed_on_disk,
            &lossy_buffers,
            &binary_buffers,
            &buffers
        ),
        Some("binary preview")
    );
    assert_eq!(
        open_lsp_workspace_edit_block_reason(
            5,
            &changed_on_disk,
            &lossy_buffers,
            &binary_buffers,
            &buffers
        ),
        Some("large file mode")
    );
    assert_eq!(
        open_lsp_workspace_edit_block_reason(
            6,
            &changed_on_disk,
            &lossy_buffers,
            &binary_buffers,
            &buffers
        ),
        Some("large buffer")
    );
}

#[test]
fn language_sync_ids_become_due_after_debounce() {
    let now = Instant::now();
    let mut pending = HashMap::new();
    pending.insert(3, now - Duration::from_millis(250));
    pending.insert(1, now - Duration::from_millis(250));
    pending.insert(2, now - Duration::from_millis(40));
    pending.insert(4, now + Duration::from_millis(40));

    assert_eq!(
        due_language_sync_ids(&pending, now, Duration::from_millis(180)),
        vec![1, 3]
    );
    assert!(due_language_sync_ids(&pending, now, Duration::from_millis(300)).is_empty());
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

fn unique_test_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
}

fn lsp_configs() -> Vec<kuroya_core::LspServerConfig> {
    EditorSettings::default().lsp_server_configs()
}

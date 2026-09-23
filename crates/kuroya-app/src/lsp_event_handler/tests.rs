use super::*;
use crate::{
    app_startup_context::AppStartupContext,
    app_state::PendingFormatOnSave,
    lsp_client::LspClientHandle,
    lsp_diagnostics_batch::{LSP_DIAGNOSTIC_BATCH_DELAY, PendingLspDiagnosticsSource},
    lsp_progress::LspProgressKey,
    lsp_runtime::{LSP_LANGUAGE_LABEL_MAX_CHARS, LSP_STATUS_MESSAGE_MAX_CHARS, lsp_client_key},
    lsp_ui_events::LspServerResultTarget,
    path_display::DISPLAY_PATH_LABEL_MAX_CHARS,
    terminal::TerminalPane,
};
use kuroya_core::{
    Diagnostic, DiagnosticSeverity, EditorSettings, LspCodeLens, LspDocumentHighlight,
    LspInlayHint, LspSemanticToken, LspServerConfig, LspTextEdit, LspWorkDoneProgress,
    LspWorkDoneProgressKind, TextBuffer, TextEdit, Workspace,
};
use std::path::PathBuf;
use tokio::runtime::Runtime;

#[test]
fn stopped_server_restarts_by_client_key_without_touching_sibling_servers() {
    let root = std::env::temp_dir().join("kuroya-lsp-multi-server-restart");
    let source_dir = root.join("src");
    std::fs::create_dir_all(&source_dir).expect("create source dir");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"lsp-test\"\n")
        .expect("write cargo manifest");
    let source = source_dir.join("main.rs");
    std::fs::write(&source, "fn main() {}\n").expect("write source");
    let settings = EditorSettings {
        lsp_servers: vec![
            LspServerConfig {
                language: "rust".to_owned(),
                command: "rust-analyzer".to_owned(),
                args: Vec::new(),
                extensions: Vec::new(),
                root_markers: vec!["Cargo.toml".to_owned()],
                enabled: true,
            },
            LspServerConfig {
                language: "rust".to_owned(),
                command: "rust-analyzer-obsidian".to_owned(),
                args: vec!["--stdio".to_owned()],
                extensions: Vec::new(),
                root_markers: vec!["Cargo.toml".to_owned()],
                enabled: true,
            },
        ],
        ..EditorSettings::default()
    };
    let mut app = app_for_test_with_settings(root.clone(), settings.clone());
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(source),
        "fn main() {}\n".to_owned(),
    ));

    let configs = settings.lsp_server_configs();
    let primary_config = configs
        .iter()
        .find(|config| config.command == "rust-analyzer")
        .expect("primary rust config")
        .clone();
    let secondary_config = configs
        .iter()
        .find(|config| config.command == "rust-analyzer-obsidian")
        .expect("secondary rust config")
        .clone();
    let primary_key = lsp_client_key(&primary_config, &configs);
    let secondary_key = lsp_client_key(&secondary_config, &configs);
    app.lsp_clients.insert(
        primary_key.clone(),
        LspClientHandle::disconnected_with_generation_for_test(10),
    );
    app.lsp_clients.insert(
        secondary_key.clone(),
        LspClientHandle::disconnected_with_generation_for_test(11),
    );

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 10,
        }))
        .is_none()
    );

    assert!(!app.lsp_clients.contains_key(&primary_key));
    assert!(app.lsp_clients.contains_key(&secondary_key));
    assert_eq!(app.lsp_restart_attempts.get(&primary_key), Some(&1));
    assert!(app.pending_lsp_restarts.contains_key(&primary_key));
    assert!(!app.lsp_restart_attempts.contains_key(&secondary_key));
    assert!(!app.pending_lsp_restarts.contains_key(&secondary_key));
    assert!(
        app.status.starts_with("rust (rust-analyzer) LSP stopped"),
        "{}",
        app.status
    );

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root,
            generation: 11,
        }))
        .is_none()
    );

    assert!(!app.lsp_clients.contains_key(&secondary_key));
    assert_eq!(app.lsp_restart_attempts.get(&primary_key), Some(&1));
    assert_eq!(app.lsp_restart_attempts.get(&secondary_key), Some(&1));
    assert!(app.pending_lsp_restarts.contains_key(&primary_key));
    assert!(app.pending_lsp_restarts.contains_key(&secondary_key));
    assert!(
        app.status
            .starts_with("rust (rust-analyzer-obsidian) LSP stopped"),
        "{}",
        app.status
    );
}

#[test]
fn server_stopped_purges_only_stopped_server_diagnostics() {
    let root = std::env::temp_dir().join("kuroya-lsp-stop-purges-its-bucket");
    let source_dir = root.join("src");
    std::fs::create_dir_all(&source_dir).expect("create source dir");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"lsp-test\"\n")
        .expect("write cargo manifest");
    let source = source_dir.join("main.rs");
    std::fs::write(&source, "fn main() {}\n").expect("write source");
    let settings = EditorSettings {
        lsp_servers: vec![
            LspServerConfig {
                language: "rust".to_owned(),
                command: "rust-analyzer".to_owned(),
                args: Vec::new(),
                extensions: Vec::new(),
                root_markers: vec!["Cargo.toml".to_owned()],
                enabled: true,
            },
            LspServerConfig {
                language: "rust".to_owned(),
                command: "rust-analyzer-obsidian".to_owned(),
                args: vec!["--stdio".to_owned()],
                extensions: Vec::new(),
                root_markers: vec!["Cargo.toml".to_owned()],
                enabled: true,
            },
        ],
        ..EditorSettings::default()
    };
    let mut app = app_for_test_with_settings(root.clone(), settings.clone());
    let configs = settings.lsp_server_configs();
    let primary_key = lsp_client_key(
        configs
            .iter()
            .find(|config| config.command == "rust-analyzer")
            .expect("primary rust config"),
        &configs,
    );
    let secondary_key = lsp_client_key(
        configs
            .iter()
            .find(|config| config.command == "rust-analyzer-obsidian")
            .expect("secondary rust config"),
        &configs,
    );
    app.lsp_clients.insert(
        primary_key,
        LspClientHandle::disconnected_with_generation_for_test(10),
    );
    app.lsp_clients.insert(
        secondary_key,
        LspClientHandle::disconnected_with_generation_for_test(11),
    );

    let mut static_diagnostic = test_diagnostic(&source, "static marker");
    static_diagnostic.source = "kuroya-static".to_owned();
    app.diagnostics
        .replace_static(source.clone(), vec![static_diagnostic]);
    let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;
    app.pending_lsp_diagnostics.queue_for_server(
        PendingLspDiagnosticsSource {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 10,
        },
        source.clone(),
        None,
        vec![test_diagnostic(&source, "from primary")],
        queued_at,
    );
    app.pending_lsp_diagnostics.queue_for_server(
        PendingLspDiagnosticsSource {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 11,
        },
        source.clone(),
        None,
        vec![test_diagnostic(&source, "from secondary")],
        queued_at,
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 2);
    assert_eq!(app.diagnostics.for_path(&source).len(), 3);

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 10,
        }))
        .is_none()
    );

    let remaining = app
        .diagnostics
        .for_path(&source)
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    assert_eq!(remaining.len(), 2, "{remaining:?}");
    assert!(remaining.contains(&"from secondary"), "{remaining:?}");
    assert!(remaining.contains(&"static marker"), "{remaining:?}");

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerUnavailable {
            language: "rust".to_owned(),
            root,
            generation: 11,
        }))
        .is_none()
    );

    let remaining = app.diagnostics.for_path(&source);
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].message, "static marker");
    assert_eq!(remaining[0].source, "kuroya-static");
}

#[test]
fn unavailable_lsp_status_clears_dead_client_and_restart_state() {
    let root = std::env::temp_dir().join("kuroya-lsp-unavailable-clears-client");
    let mut app = app_for_test(root.clone());

    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::Status {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 1,
            message: "rust LSP unavailable: program not found".to_owned(),
        }))
        .is_none()
    );
    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerUnavailable {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.lsp_clients.contains_key("rust"));
    assert!(!app.lsp_restart_attempts.contains_key("rust"));
    assert!(!app.pending_lsp_restarts.contains_key("rust"));
    assert!(app.lsp_unavailable.contains("rust"));
    assert_eq!(app.status, "rust LSP unavailable: program not found");
}

#[test]
fn unavailable_lsp_status_continues_pending_format_on_save() {
    let root = std::env::temp_dir().join("kuroya-lsp-unavailable-format-save");
    let source = root.join("main.rs");
    std::fs::create_dir_all(&root).expect("create root");
    std::fs::write(&source, "fn main() {}\n").expect("write source");
    let mut app = app_for_test(root.clone());
    let mut buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    buffer.mark_dirty();
    let version = buffer.version();
    app.buffers.push(buffer);
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    app.pending_format_on_save.insert(
        7,
        PendingFormatOnSave {
            save_path: source.clone(),
            format_path: source,
            version,
            request_id: 21,
        },
    );

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerUnavailable {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.pending_format_on_save.contains_key(&7));
    assert!(!app.format_on_save_bypass.contains(&7));
    assert!(app.in_flight_saves.contains(&7));
    assert!(app.status.starts_with("Saving "));
}

#[test]
fn startup_failure_stopped_event_clears_client_and_schedules_restart() {
    let root = std::env::temp_dir().join("kuroya-lsp-startup-stopped-restart");
    let source_dir = root.join("src");
    std::fs::create_dir_all(&source_dir).expect("create source dir");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"lsp-test\"\n")
        .expect("write cargo manifest");
    let source = source_dir.join("main.rs");
    std::fs::write(&source, "fn main() {}\n").expect("write source");
    let mut app = app_for_test(root.clone());
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(source),
        "fn main() {}\n".to_owned(),
    ));

    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::Status {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 1,
            message: "rust LSP initialize failed: timed out".to_owned(),
        }))
        .is_none()
    );
    assert!(!app.lsp_unavailable.contains("rust"));
    assert!(app.lsp_clients.contains_key("rust"));

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.lsp_clients.contains_key("rust"));
    assert_eq!(app.lsp_restart_attempts.get("rust"), Some(&1));
    assert!(app.pending_lsp_restarts.contains_key("rust"));
    assert!(!app.lsp_unavailable.contains("rust"));
}

#[test]
fn server_stopped_continues_pending_format_on_save_without_formatting_result() {
    let root = std::env::temp_dir().join("kuroya-lsp-stopped-format-save");
    let source_dir = root.join("src");
    std::fs::create_dir_all(&source_dir).expect("create source dir");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"lsp-test\"\n")
        .expect("write cargo manifest");
    let source = source_dir.join("main.rs");
    std::fs::write(&source, "fn main() {}\n").expect("write source");
    let mut app = app_for_test(root.clone());
    let mut buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    buffer.mark_dirty();
    let version = buffer.version();
    app.buffers.push(buffer);
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    app.pending_format_on_save.insert(
        7,
        PendingFormatOnSave {
            save_path: source.clone(),
            format_path: source,
            version,
            request_id: 21,
        },
    );

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.pending_format_on_save.contains_key(&7));
    assert!(!app.format_on_save_bypass.contains(&7));
    assert!(app.in_flight_saves.contains(&7));
    assert_eq!(app.lsp_restart_attempts.get("rust"), Some(&1));
    assert!(app.pending_lsp_restarts.contains_key("rust"));
    assert!(app.status.starts_with("Saving "));
}

#[test]
fn server_stopped_falls_back_pending_workspace_symbols_to_index() {
    let root = std::env::temp_dir().join("kuroya-lsp-stopped-workspace-symbol-fallback");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(source.clone()),
        "fn main() {}\n".to_owned(),
    ));
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    app.workspace_symbols_open = true;
    app.workspace_symbol_query = "main".to_owned();
    app.workspace_symbol_submitted_query = "main".to_owned();
    app.workspace_symbol_submitted_path = Some(source);
    app.status = "Searching workspace symbols for `main`".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert_eq!(app.workspace_symbol_submitted_path, None);
    assert_eq!(
        app.status,
        "rust LSP stopped; no indexed workspace symbols for `main`"
    );
}

#[test]
fn buffer_synced_does_not_clear_lsp_restart_backoff() {
    let root = std::env::temp_dir().join("kuroya-lsp-buffer-synced-keeps-backoff");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(source.clone()),
        "fn main() {}\n".to_owned(),
    ));
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::BufferSynced {
            id: 7,
            path: source,
            version: 0,
        }))
        .is_none()
    );

    assert_eq!(app.lsp_restart_attempts.get("rust"), Some(&2));
    assert!(app.pending_lsp_restarts.contains_key("rust"));
    assert_eq!(app.status, "main.rs synced with LSP at v0");
}

#[test]
fn buffer_synced_status_sanitizes_and_bounds_path_label() {
    let root = std::env::temp_dir().join("kuroya-lsp-buffer-synced-path-label");
    let file_name = format!(
        "main\n{}\u{202e}.rs",
        "path-fragment-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
    );
    let source = root.join(file_name);
    let mut app = app_for_test(root);
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.buffers.push(buffer);

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::BufferSynced {
            id: 7,
            path: source,
            version,
        }))
        .is_none()
    );

    assert_display_safe(&app.status);
    assert!(app.status.contains("..."));
    assert!(
        app.status.chars().count()
            <= " synced with LSP at v0".chars().count() + DISPLAY_PATH_LABEL_MAX_CHARS
    );
}

#[test]
fn server_ready_clears_lsp_restart_backoff_for_matching_root() {
    let root = std::env::temp_dir().join("kuroya-lsp-server-ready-clears-backoff");
    let mut app = app_for_test(root.clone());
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerReady {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.lsp_restart_attempts.contains_key("rust"));
    assert!(!app.pending_lsp_restarts.contains_key("rust"));
    assert_eq!(app.status, "rust LSP ready");
}

#[test]
fn lsp_lifecycle_event_matches_accepts_only_current_server() {
    let root = std::env::temp_dir().join("kuroya-lsp-lifecycle-equivalent-root");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );

    assert!(app.lsp_lifecycle_event_matches("rust", &event_root, 2));
    assert!(!app.lsp_lifecycle_event_matches("rust", Path::new("other-workspace"), 2));
    assert!(!app.lsp_lifecycle_event_matches("rust", &event_root, 1));
    assert!(!app.lsp_lifecycle_event_matches("python", &event_root, 2));
}

#[test]
fn equivalent_root_server_ready_clears_lsp_restart_backoff() {
    let root = std::env::temp_dir().join("kuroya-lsp-server-ready-equivalent-root");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root);
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerReady {
            language: "rust".to_owned(),
            root: event_root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.lsp_restart_attempts.contains_key("rust"));
    assert!(!app.pending_lsp_restarts.contains_key("rust"));
    assert_eq!(app.status, "rust LSP ready");
}

#[test]
fn server_ready_status_sanitizes_language_without_changing_raw_restart_keys() {
    let root = std::env::temp_dir().join("kuroya-lsp-server-ready-sanitizes-language");
    let language = format!(
        "rust\n{}\u{202e}",
        "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS)
    );
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        language.clone(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );
    app.lsp_restart_attempts.insert(language.clone(), 2);
    app.pending_lsp_restarts
        .insert(language.clone(), Instant::now());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerReady {
            language: language.clone(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.lsp_restart_attempts.contains_key(&language));
    assert!(!app.pending_lsp_restarts.contains_key(&language));
    assert_display_safe(&app.status);
    assert!(app.status.contains("..."));
    assert!(
        app.status.chars().count() <= LSP_LANGUAGE_LABEL_MAX_CHARS + " LSP ready".chars().count()
    );
}

#[test]
fn server_ready_from_other_workspace_is_ignored() {
    let root = std::env::temp_dir().join("kuroya-lsp-server-ready-current-root");
    let mut app = app_for_test(root);
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerReady {
            language: "rust".to_owned(),
            root: PathBuf::from("other-workspace"),
            generation: 1,
        }))
        .is_none()
    );

    assert_eq!(app.lsp_restart_attempts.get("rust"), Some(&2));
    assert!(app.pending_lsp_restarts.contains_key("rust"));
}

#[test]
fn stale_server_ready_does_not_clear_current_restart_backoff() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-ready-keeps-backoff");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now());
    app.status = "current status".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerReady {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert_eq!(app.lsp_restart_attempts.get("rust"), Some(&2));
    assert!(app.pending_lsp_restarts.contains_key("rust"));
    assert_eq!(app.status, "current status");
}

#[test]
fn stale_server_stopped_does_not_remove_current_client_or_schedule_restart() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-stopped-keeps-client");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );
    app.status = "current status".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert_eq!(
        app.lsp_clients.get("rust").map(LspClientHandle::generation),
        Some(2)
    );
    assert!(!app.pending_lsp_restarts.contains_key("rust"));
    assert!(!app.lsp_restart_attempts.contains_key("rust"));
    assert_eq!(app.status, "current status");
}

#[test]
fn stale_server_stopped_does_not_clear_current_active_progress() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-stopped-keeps-progress");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );
    let current_key = LspProgressKey::new("rust", root.clone(), 2, "token");
    app.lsp_progress_titles
        .insert(current_key.clone(), "Indexing".to_owned());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert_eq!(
        app.lsp_progress_titles
            .get(&current_key)
            .map(String::as_str),
        Some("Indexing")
    );
}

#[test]
fn server_stopped_restart_status_sanitizes_language_and_uses_raw_client_key() {
    let root = std::env::temp_dir().join("kuroya-lsp-server-stopped-sanitizes-language");
    let language = format!(
        "rust\n{}\u{202e}",
        "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS)
    );
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        language.clone(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );
    app.lsp_restart_attempts.insert(language.clone(), 2);

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: language.clone(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.lsp_clients.contains_key(&language));
    assert!(!app.lsp_restart_attempts.contains_key(&language));
    assert_display_safe(&app.status);
    assert!(app.status.contains("..."));
    assert!(
        app.status.chars().count()
            <= LSP_LANGUAGE_LABEL_MAX_CHARS
                + " LSP stopped; no open buffers to restart".chars().count()
    );
}

#[test]
fn stale_work_done_progress_does_not_update_status_or_active_progress() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-progress-keeps-current");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );
    app.status = "current status".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::WorkDoneProgress {
            language: "rust".to_owned(),
            root,
            generation: 1,
            progress: progress("token", LspWorkDoneProgressKind::Begin, Some("Indexing")),
        }))
        .is_none()
    );

    assert!(app.lsp_progress_titles.is_empty());
    assert_eq!(app.status, "current status");
}

#[test]
fn server_stopped_clears_active_progress_for_matching_server() {
    let root = std::env::temp_dir().join("kuroya-lsp-stopped-clears-progress");
    let mut app = app_for_test(root.clone());
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::WorkDoneProgress {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 1,
            progress: progress("token", LspWorkDoneProgressKind::Begin, Some("Indexing")),
        }))
        .is_none()
    );
    assert!(app.lsp_progress_titles.contains_key(&LspProgressKey::new(
        "rust",
        root.clone(),
        1,
        "token"
    )));

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(app.lsp_progress_titles.is_empty());
}

#[test]
fn equivalent_root_work_done_progress_reuses_current_workspace_progress_key() {
    let root = std::env::temp_dir().join("kuroya-lsp-progress-equivalent-root");
    let equivalent_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::WorkDoneProgress {
            language: "rust".to_owned(),
            root: equivalent_root,
            generation: 1,
            progress: progress("token", LspWorkDoneProgressKind::Begin, Some("Indexing")),
        }))
        .is_none()
    );
    assert_eq!(app.lsp_progress_titles.len(), 1);
    assert!(app.lsp_progress_titles.contains_key(&LspProgressKey::new(
        "rust",
        root.clone(),
        1,
        "token"
    )));

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::WorkDoneProgress {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 1,
            progress: progress("token", LspWorkDoneProgressKind::Report, None),
        }))
        .is_none()
    );

    assert_eq!(app.lsp_progress_titles.len(), 1);
    assert_eq!(app.status, "LSP: Indexing");
}

#[test]
fn equivalent_root_server_stopped_clears_active_progress() {
    let root = std::env::temp_dir().join("kuroya-lsp-stopped-equivalent-root-clears-progress");
    let event_root = root.join("src").join("..");
    let mut app = app_for_test(root.clone());
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    app.lsp_progress_titles.insert(
        LspProgressKey::new("rust", root.clone(), 1, "token"),
        "Indexing".to_owned(),
    );

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerStopped {
            language: "rust".to_owned(),
            root: event_root,
            generation: 1,
        }))
        .is_none()
    );

    assert!(!app.lsp_clients.contains_key("rust"));
    assert!(app.lsp_progress_titles.is_empty());
    assert_eq!(app.status, "rust LSP stopped; no open buffers to restart");
}

#[test]
fn stale_unavailable_status_does_not_mark_current_server_unavailable() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-unavailable-keeps-current");
    let mut app = app_for_test(root.clone());
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );
    app.lsp_restart_attempts.insert("rust".to_owned(), 2);
    app.pending_lsp_restarts
        .insert("rust".to_owned(), Instant::now());
    app.status = "current status".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerUnavailable {
            language: "rust".to_owned(),
            root,
            generation: 1,
        }))
        .is_none()
    );

    assert_eq!(
        app.lsp_clients.get("rust").map(LspClientHandle::generation),
        Some(2)
    );
    assert_eq!(app.lsp_restart_attempts.get("rust"), Some(&2));
    assert!(app.pending_lsp_restarts.contains_key("rust"));
    assert!(!app.lsp_unavailable.contains("rust"));
    assert_eq!(app.status, "current status");
}

#[test]
fn unavailable_status_text_alone_no_longer_marks_server_unavailable() {
    let root = std::env::temp_dir().join("kuroya-lsp-status-text-no-unavailable");
    let mut app = app_for_test(root.clone());
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::Status {
            language: "rust".to_owned(),
            root,
            generation: 1,
            message: "rust LSP unavailable: program not found".to_owned(),
        }))
        .is_none()
    );

    assert!(app.lsp_clients.contains_key("rust"));
    assert!(!app.lsp_unavailable.contains("rust"));
    assert_eq!(app.status, "rust LSP unavailable: program not found");
}

#[test]
fn status_event_sanitizes_dirty_messages() {
    let root = std::env::temp_dir().join("kuroya-lsp-status-message-display-safe");
    let mut app = app_for_test(root.clone());
    app.lsp_clients
        .insert("rust".to_owned(), LspClientHandle::disconnected_for_test());
    let message = format!(
        "rust LSP unavailable: first line\nsecond line \u{202e}{}",
        "message-fragment-".repeat(LSP_STATUS_MESSAGE_MAX_CHARS)
    );

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::Status {
            language: "rust".to_owned(),
            root,
            generation: 1,
            message,
        }))
        .is_none()
    );

    assert_display_safe(&app.status);
    assert!(app.status.contains("..."));
    assert!(app.status.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
}

#[test]
fn stale_publish_diagnostics_are_rejected_for_open_buffer_versions() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-diagnostics");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(source.clone()),
        "fn main() {}\n".to_owned(),
    ));

    let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;
    app.pending_lsp_diagnostics.queue(
        source.clone(),
        Some(1),
        vec![test_diagnostic(&source, "stale")],
        queued_at,
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 0);
    assert!(app.diagnostics.for_path(&source).is_empty());

    app.pending_lsp_diagnostics.queue(
        source.clone(),
        Some(0),
        vec![test_diagnostic(&source, "current")],
        queued_at,
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 1);
    assert_eq!(app.diagnostics.for_path(&source)[0].message, "current");
}

#[test]
fn stale_publish_diagnostics_are_rejected_for_old_lsp_generation() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-diagnostic-generation");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.buffers.push(buffer);
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::Diagnostics {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 1,
            path: source.clone(),
            version: Some(version),
            diagnostics: vec![test_diagnostic(&source, "stale")],
        }))
        .is_none()
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 0);
    assert!(app.diagnostics.for_path(&source).is_empty());

    let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;
    app.pending_lsp_diagnostics.queue_for_server(
        PendingLspDiagnosticsSource {
            language: "rust".to_owned(),
            root: root.clone(),
            generation: 1,
        },
        source.clone(),
        Some(version),
        vec![test_diagnostic(&source, "stale queued")],
        queued_at,
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 0);
    assert!(app.diagnostics.for_path(&source).is_empty());

    app.pending_lsp_diagnostics.queue_for_server(
        PendingLspDiagnosticsSource {
            language: "rust".to_owned(),
            root,
            generation: 2,
        },
        source.clone(),
        Some(version),
        vec![test_diagnostic(&source, "current")],
        queued_at,
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 1);
    assert_eq!(app.diagnostics.for_path(&source)[0].message, "current");
}

#[test]
fn queued_lsp_diagnostics_from_equivalent_root_are_flushed() {
    let root = std::env::temp_dir().join("kuroya-lsp-equivalent-root-diagnostics");
    let event_root = root.join("src").join("..");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root);
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.buffers.push(buffer);
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;
    app.pending_lsp_diagnostics.queue_for_server(
        PendingLspDiagnosticsSource {
            language: "rust".to_owned(),
            root: event_root,
            generation: 1,
        },
        source.clone(),
        Some(version),
        vec![test_diagnostic(&source, "current")],
        queued_at,
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 1);
    assert_eq!(app.diagnostics.for_path(&source)[0].message, "current");
}

#[test]
fn queued_lsp_diagnostics_from_equivalent_path_reject_stale_buffer_version() {
    let root = std::env::temp_dir().join("kuroya-lsp-equivalent-path-stale-diagnostics");
    let source = root.join("src").join("main.rs");
    let equivalent_source = root.join("src").join("..").join("src").join("main.rs");
    let mut app = app_for_test(root);
    let mut buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let stale_version = buffer.version();
    buffer.apply_edit(TextEdit {
        range: 0..0,
        inserted: "// current\n".to_owned(),
    });
    app.buffers.push(buffer);
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;
    app.pending_lsp_diagnostics.queue_for_server(
        PendingLspDiagnosticsSource {
            language: "rust".to_owned(),
            root: app.workspace.root.clone(),
            generation: 1,
        },
        equivalent_source.clone(),
        Some(stale_version),
        vec![test_diagnostic(&equivalent_source, "stale")],
        queued_at,
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 0);
    assert!(app.diagnostics.for_path(&source).is_empty());
}

#[test]
fn queued_lsp_diagnostics_from_equivalent_path_validate_open_buffer_ranges() {
    let root = std::env::temp_dir().join("kuroya-lsp-equivalent-path-range-diagnostics");
    let source = root.join("src").join("main.rs");
    let equivalent_source = root.join("src").join("..").join("src").join("main.rs");
    let mut app = app_for_test(root);
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "alpha\n".to_owned());
    let version = buffer.version();
    app.buffers.push(buffer);
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(1),
    );

    let mut diagnostic = test_diagnostic(&equivalent_source, "wide range");
    diagnostic.column = 3;
    diagnostic.char_range = 2..99;
    let queued_at = Instant::now() - LSP_DIAGNOSTIC_BATCH_DELAY;
    app.pending_lsp_diagnostics.queue_for_server(
        PendingLspDiagnosticsSource {
            language: "rust".to_owned(),
            root: app.workspace.root.clone(),
            generation: 1,
        },
        equivalent_source,
        Some(version),
        vec![diagnostic],
        queued_at,
    );

    assert_eq!(app.flush_pending_lsp_diagnostics(), 1);
    let diagnostics = app.diagnostics.for_path(&source);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].column, 3);
    assert_eq!(diagnostics[0].char_range, 2..5);
}

#[test]
fn stale_lsp_overlay_results_do_not_replace_current_caches() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-overlays");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let mut buffer = TextBuffer::from_text(
        7,
        Some(source.clone()),
        "fn main() {\n    value\n}\n".to_owned(),
    );
    let stale_version = buffer.version();
    buffer.apply_edit(TextEdit {
        range: 0..0,
        inserted: "// current\n".to_owned(),
    });
    let current_version = buffer.version();
    assert!(current_version > stale_version);
    app.buffers.push(buffer);

    let current_hint = LspInlayHint {
        line: 2,
        column: 5,
        label: "current hint".to_owned(),
        kind: None,
    };
    let current_lens = LspCodeLens {
        line: 2,
        column: 5,
        title: "Current Lens".to_owned(),
        command: None,
        command_arguments: None,
        resolve_payload: None,
    };
    let current_token = LspSemanticToken {
        line: 2,
        column: 5,
        length: 5,
        token_type: "current".to_owned(),
        modifiers: Vec::new(),
    };
    app.inlay_hints
        .insert(source.clone(), vec![current_hint.clone()]);
    app.code_lenses
        .insert(source.clone(), vec![current_lens.clone()]);
    app.semantic_tokens
        .insert(source.clone(), vec![current_token.clone()]);
    app.status = "current status".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::InlayHintsResult {
            id: 7,
            path: source.clone(),
            version: stale_version,
            hints: Some(vec![LspInlayHint {
                line: 1,
                column: 1,
                label: "stale hint".to_owned(),
                kind: None,
            }]),
            error: None,
        }))
        .is_none()
    );
    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::CodeLensesResult {
            id: 7,
            path: source.clone(),
            version: stale_version,
            lenses: Some(vec![LspCodeLens {
                line: 1,
                column: 1,
                title: "Stale Lens".to_owned(),
                command: None,
                command_arguments: None,
                resolve_payload: None,
            }]),
            error: None,
        }))
        .is_none()
    );
    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::SemanticTokensResult {
            id: 7,
            path: source.clone(),
            version: stale_version,
            tokens: Some(vec![LspSemanticToken {
                line: 1,
                column: 1,
                length: 4,
                token_type: "stale".to_owned(),
                modifiers: Vec::new(),
            }]),
            error: Some("stale failure".to_owned()),
        }))
        .is_none()
    );

    assert_eq!(app.inlay_hints.get(&source), Some(&vec![current_hint]));
    assert_eq!(app.code_lenses.get(&source), Some(&vec![current_lens]));
    assert_eq!(app.semantic_tokens.get(&source), Some(&vec![current_token]));
    assert_eq!(app.status, "current status");
}

#[test]
fn lsp_server_result_from_current_generation_dispatches_inner_result() {
    let root = std::env::temp_dir().join("kuroya-lsp-current-server-result");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.buffers.push(buffer);
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerResult {
            target: LspServerResultTarget {
                language: "rust".to_owned(),
                root,
                generation: 2,
            },
            event: Box::new(LspUiEvent::InlayHintsResult {
                id: 7,
                path: source.clone(),
                version,
                hints: Some(vec![LspInlayHint {
                    line: 1,
                    column: 1,
                    label: "current hint".to_owned(),
                    kind: None,
                }]),
                error: None,
            }),
        }))
        .is_none()
    );

    assert_eq!(
        app.inlay_hints
            .get(&source)
            .and_then(|hints| hints.first())
            .map(|hint| hint.label.as_str()),
        Some("current hint")
    );
}

#[test]
fn stale_lsp_server_result_does_not_replace_current_caches() {
    let root = std::env::temp_dir().join("kuroya-lsp-stale-server-result");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.buffers.push(buffer);
    app.lsp_clients.insert(
        "rust".to_owned(),
        LspClientHandle::disconnected_with_generation_for_test(2),
    );
    let current_hint = LspInlayHint {
        line: 2,
        column: 5,
        label: "current hint".to_owned(),
        kind: None,
    };
    app.inlay_hints
        .insert(source.clone(), vec![current_hint.clone()]);
    app.status = "current status".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::ServerResult {
            target: LspServerResultTarget {
                language: "rust".to_owned(),
                root,
                generation: 1,
            },
            event: Box::new(LspUiEvent::InlayHintsResult {
                id: 7,
                path: source.clone(),
                version,
                hints: Some(vec![LspInlayHint {
                    line: 1,
                    column: 1,
                    label: "stale hint".to_owned(),
                    kind: None,
                }]),
                error: None,
            }),
        }))
        .is_none()
    );

    assert_eq!(app.inlay_hints.get(&source), Some(&vec![current_hint]));
    assert_eq!(app.status, "current status");
}

#[test]
fn disabled_inlay_hints_result_does_not_repopulate_cache() {
    let root = std::env::temp_dir().join("kuroya-lsp-disabled-inlay-hints");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.buffers.push(buffer);
    app.settings.inlay_hints = false;
    app.status = "unchanged".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::InlayHintsResult {
            id: 7,
            path: source.clone(),
            version,
            hints: Some(vec![LspInlayHint {
                line: 1,
                column: 1,
                label: "hint".to_owned(),
                kind: None,
            }]),
            error: None,
        }))
        .is_none()
    );

    assert!(!app.inlay_hints.contains_key(&source));
    assert_eq!(app.status, "unchanged");
}

#[test]
fn disabled_code_lenses_result_does_not_repopulate_cache() {
    let root = std::env::temp_dir().join("kuroya-lsp-disabled-code-lenses");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.buffers.push(buffer);
    app.settings.code_lens = false;
    app.status = "unchanged".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::CodeLensesResult {
            id: 7,
            path: source.clone(),
            version,
            lenses: Some(vec![LspCodeLens {
                line: 1,
                column: 1,
                title: "Run".to_owned(),
                command: None,
                command_arguments: None,
                resolve_payload: None,
            }]),
            error: None,
        }))
        .is_none()
    );

    assert!(!app.code_lenses.contains_key(&source));
    assert_eq!(app.status, "unchanged");
}

#[test]
fn disabled_hover_result_does_not_open_popup_or_cache() {
    let root = std::env::temp_dir().join("kuroya-lsp-disabled-hover");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.active = Some(7);
    app.buffers.push(buffer);
    app.settings.hover_enabled = false;
    app.status = "unchanged".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::HoverResult {
            id: 7,
            path: source,
            version,
            line: 0,
            column: 1,
            contents: Some("hover docs".to_owned()),
        }))
        .is_none()
    );

    assert!(app.lsp_hover.is_none());
    assert!(app.lsp_hover_cache.is_empty());
    assert_eq!(app.status, "unchanged");
}

#[test]
fn untrusted_workspace_ignores_late_lsp_results() {
    let root = std::env::temp_dir().join("kuroya-lsp-untrusted-late-results");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.active = Some(7);
    app.buffers.push(buffer);
    app.workspace_trusted = false;
    app.status = "unchanged".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::InlayHintsResult {
            id: 7,
            path: source.clone(),
            version,
            hints: Some(vec![LspInlayHint {
                line: 1,
                column: 1,
                label: "hint".to_owned(),
                kind: None,
            }]),
            error: None,
        }))
        .is_none()
    );
    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::FormattingResult {
            request_id: 1,
            id: 7,
            path: source.clone(),
            version,
            edits: Some(vec![LspTextEdit {
                path: source.clone(),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 1,
                new_text: "// formatted\n".to_owned(),
            }]),
            error: None,
        }))
        .is_none()
    );
    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::HoverResult {
            id: 7,
            path: source.clone(),
            version,
            line: 0,
            column: 1,
            contents: Some("hover docs".to_owned()),
        }))
        .is_none()
    );

    assert!(!app.inlay_hints.contains_key(&source));
    assert!(app.lsp_hover.is_none());
    assert!(app.lsp_hover_cache.is_empty());
    assert_eq!(
        app.buffer(7).expect("buffer").text(),
        "fn main() {}\n".to_owned()
    );
    assert_eq!(app.status, "unchanged");
}

#[test]
fn disabled_document_highlights_result_does_not_repopulate_cache() {
    let root = std::env::temp_dir().join("kuroya-lsp-disabled-document-highlights");
    let source = root.join("src").join("main.rs");
    let mut app = app_for_test(root.clone());
    let buffer = TextBuffer::from_text(7, Some(source.clone()), "fn main() {}\n".to_owned());
    let version = buffer.version();
    app.active = Some(7);
    app.buffers.push(buffer);
    app.settings.document_highlights_enabled = false;
    app.document_highlights_path = Some(source.clone());
    app.document_highlights = vec![LspDocumentHighlight {
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 4,
        kind: None,
    }];
    app.status = "unchanged".to_owned();

    assert!(
        app.handle_lsp_event(UiEvent::Lsp(LspUiEvent::DocumentHighlightsResult {
            id: 7,
            path: source,
            version,
            line: 0,
            column: 1,
            highlights: Some(vec![LspDocumentHighlight {
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 4,
                kind: None,
            }]),
            error: None,
        }))
        .is_none()
    );

    assert!(app.document_highlights_path.is_none());
    assert!(app.document_highlights.is_empty());
    assert_eq!(app.status, "unchanged");
}

fn test_diagnostic(path: &std::path::Path, message: &str) -> Diagnostic {
    Diagnostic {
        path: path.to_path_buf(),
        line: 1,
        column: 1,
        char_range: 0..1,
        severity: DiagnosticSeverity::Warning,
        source: "rust-analyzer".to_owned(),
        message: message.to_owned(),
        unused: false,
        deprecated: false,
    }
}

fn progress(
    token: &str,
    kind: LspWorkDoneProgressKind,
    title: Option<&str>,
) -> LspWorkDoneProgress {
    LspWorkDoneProgress {
        token: token.to_owned(),
        kind,
        title: title.map(str::to_owned),
        message: None,
        percentage: None,
    }
}

fn app_for_test(root: PathBuf) -> KuroyaApp {
    let settings = EditorSettings::default();
    app_for_test_with_settings(root, settings)
}

fn app_for_test_with_settings(root: PathBuf, settings: EditorSettings) -> KuroyaApp {
    let (tx, rx) = crate::ui_event_channel::ui_event_channel();
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

fn assert_display_safe(value: &str) {
    assert!(!value.chars().any(char::is_control), "{value:?}");
    assert!(!value.chars().any(is_bidi_format_control), "{value:?}");
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

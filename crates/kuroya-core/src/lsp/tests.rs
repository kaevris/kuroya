use super::*;

mod code_action_signature_formatting;
mod completion_snippet;
mod hierarchy_symbol;
mod uri;
mod watched_files;
mod workspace_edit;
mod workspace_symbol;

#[test]
fn bounded_lsp_value_accepts_exact_byte_limit_and_preserves_value() {
    let value = json!("0123456789");

    let payload = bounded_lsp_value(&value, 12).unwrap();

    assert_eq!(payload.as_ref(), &value);
}

#[test]
fn bounded_lsp_value_rejects_payloads_above_byte_limit() {
    let value = json!("0123456789");

    assert!(bounded_lsp_value(&value, 11).is_none());
}

#[test]
fn default_lsp_servers_cover_common_language_ids() {
    let servers = default_server_configs();
    let languages = servers
        .iter()
        .map(|server| server.language.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    for language in [
        "rust",
        "python",
        "typescript",
        "javascript",
        "go",
        "java",
        "c",
        "cpp",
        "csharp",
        "php",
        "ruby",
        "lua",
        "dart",
        "kotlin",
        "swift",
        "toml",
        "json",
        "html",
        "css",
        "yaml",
        "xml",
        "markdown",
        "vue",
        "svelte",
        "dockerfile",
        "terraform",
        "powershell",
        "shellscript",
        "sql",
    ] {
        assert!(languages.contains(language), "missing {language} default");
    }
    assert_eq!(languages.len(), servers.len());
}

#[test]
fn server_for_language_uses_popular_language_defaults() {
    assert_eq!(
        server_for_language(LanguageId::Php)
            .expect("php default")
            .command,
        "intelephense"
    );
    assert_eq!(
        server_for_language(LanguageId::Ruby)
            .expect("ruby default")
            .command,
        "ruby-lsp"
    );
    assert_eq!(
        server_for_language(LanguageId::Dart)
            .expect("dart default")
            .args,
        vec!["language-server".to_owned(), "--protocol=lsp".to_owned()]
    );
    assert!(server_for_language(LanguageId::PlainText).is_none());
}

#[test]
fn lsp_language_id_for_path_preserves_common_variant_extensions() {
    assert_eq!(
        lsp_language_id_for_path(LanguageId::TypeScript, Some(Path::new("src/App.tsx"))),
        "typescriptreact"
    );
    assert_eq!(
        lsp_language_id_for_path(LanguageId::JavaScript, Some(Path::new("src/App.jsx"))),
        "javascriptreact"
    );
    assert_eq!(
        lsp_language_id_for_path(LanguageId::Css, Some(Path::new("src/app.scss"))),
        "scss"
    );
    assert_eq!(
        lsp_language_id_for_path(LanguageId::Css, Some(Path::new("src/app.less"))),
        "less"
    );
    assert_eq!(
        lsp_language_id_for_path(LanguageId::Json, Some(Path::new("tsconfig.jsonc"))),
        "jsonc"
    );
    assert_eq!(
        lsp_language_id_for_path(LanguageId::TypeScript, Some(Path::new("src/app.mts"))),
        "typescript"
    );
}

#[test]
fn parses_publish_diagnostics_notification() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let value = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "version": 42,
            "diagnostics": [{
                "range": {
                    "start": { "line": 1, "character": 2 },
                    "end": { "line": 1, "character": 6 }
                },
                "severity": 1,
                "source": "rust-analyzer",
                "tags": [1, 2],
                "message": "example"
            }]
        }
    });

    let (_, version, diagnostics) = parse_publish_diagnostics(&value).unwrap();
    assert_eq!(version, Some(42));
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
    assert_eq!(diagnostics[0].column, 3);
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostics[0].source, "rust-analyzer");
    assert_eq!(diagnostics[0].message, "example");
    assert!(diagnostics[0].unused);
    assert!(diagnostics[0].deprecated);
}

#[test]
fn publish_diagnostics_without_severity_default_to_error() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let value = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 1 }
                },
                "message": "missing severity"
            }]
        }
    });

    let (_, _, diagnostics) = parse_publish_diagnostics(&value).unwrap();

    assert_eq!(diagnostics.len(), 1);
    // A missing severity is an error per the LSP convention editors follow.
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);

    let params = PublishDiagnosticsParams {
        uri,
        version: None,
        diagnostics: vec![LspDiagnostic {
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 0,
                },
                end: LspPosition {
                    line: 0,
                    character: 1,
                },
            },
            severity: None,
            source: None,
            tags: Vec::new(),
            message: "missing severity".to_owned(),
        }],
    };

    let (_, _, diagnostics) = diagnostics_from_lsp(params).unwrap();

    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
}

#[test]
fn lsp_ranges_reject_overflow_and_reversed_coordinates() {
    let overflow = json!({
        "start": { "line": u64::MAX, "character": 0 },
        "end": { "line": u64::MAX, "character": 1 }
    });
    let reversed_line = json!({
        "start": { "line": 3, "character": 0 },
        "end": { "line": 2, "character": 99 }
    });
    let reversed_character = json!({
        "start": { "line": 3, "character": 8 },
        "end": { "line": 3, "character": 7 }
    });
    let zero_width = json!({
        "start": { "line": 0, "character": 0 },
        "end": { "line": 0, "character": 0 }
    });

    assert!(parse_lsp_range(&overflow).is_none());
    assert!(parse_lsp_range(&reversed_line).is_none());
    assert!(parse_lsp_range(&reversed_character).is_none());
    assert_eq!(parse_lsp_range(&zero_width), Some((1, 1, 1, 1)));
}

#[test]
fn parse_prepare_rename_response_reads_object_range_and_placeholder() {
    let response = json!({
        "id": 9,
        "result": {
            "range": {
                "start": { "line": 2, "character": 4 },
                "end": { "line": 2, "character": 12 }
            },
            "placeholder": "old_symbol"
        }
    });

    assert_eq!(
        parse_prepare_rename_response(&response),
        Some(LspPrepareRename {
            start_line: 3,
            start_column: 5,
            end_line: 3,
            end_column: 13,
            placeholder: Some("old_symbol".to_owned()),
        })
    );
}

#[test]
fn parse_prepare_rename_response_reads_bare_range_and_deprecated_start_end_shapes() {
    let bare_range = json!({
        "id": 9,
        "result": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 4 }
        }
    });
    let wrapped_range = json!({
        "id": 9,
        "result": {
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 4 }
            }
        }
    });
    let expected = Some(LspPrepareRename {
        start_line: 1,
        start_column: 1,
        end_line: 1,
        end_column: 5,
        placeholder: None,
    });

    assert_eq!(parse_prepare_rename_response(&bare_range), expected);
    assert_eq!(parse_prepare_rename_response(&wrapped_range), expected);
}

#[test]
fn parse_prepare_rename_response_rejects_null_malformed_and_reversed_results() {
    // Null result: the server cannot rename at this position.
    let null_result = json!({ "id": 9, "result": null });
    assert_eq!(parse_prepare_rename_response(&null_result), None);

    // Missing range entirely.
    let missing_range = json!({ "id": 9, "result": { "placeholder": "orphan" } });
    assert_eq!(parse_prepare_rename_response(&missing_range), None);

    // Reversed range is rejected like every other parsed LSP range.
    let reversed = json!({
        "id": 9,
        "result": {
            "range": {
                "start": { "line": 5, "character": 4 },
                "end": { "line": 5, "character": 2 }
            }
        }
    });
    assert_eq!(parse_prepare_rename_response(&reversed), None);

    // Malformed coordinates.
    let malformed = json!({
        "id": 9,
        "result": {
            "range": {
                "start": { "line": -1, "character": 0 },
                "end": { "line": 0, "character": 0 }
            }
        }
    });
    assert_eq!(parse_prepare_rename_response(&malformed), None);

    // Missing result object altogether.
    assert_eq!(parse_prepare_rename_response(&json!({ "id": 9 })), None);
}

#[test]
fn parse_prepare_rename_response_bounds_and_drops_placeholder_text() {
    let oversized = "x".repeat(MAX_LSP_PREPARE_RENAME_PLACEHOLDER_CHARS + 1);
    let response = json!({
        "id": 9,
        "result": {
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "placeholder": oversized
        }
    });

    let parsed = parse_prepare_rename_response(&response).expect("oversized placeholder parses");
    assert_eq!(
        parsed.placeholder,
        Some("x".repeat(MAX_LSP_PREPARE_RENAME_PLACEHOLDER_CHARS))
    );

    let non_string_placeholder = json!({
        "id": 9,
        "result": {
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 }
            },
            "placeholder": 42
        }
    });
    let parsed = parse_prepare_rename_response(&non_string_placeholder).expect("parses");
    assert_eq!(parsed.placeholder, None);
}

#[test]
fn prepare_rename_request_encodes_text_document_position_params() {
    let message = LspWireMessage::prepare_rename(11, Path::new("src/main.rs"), 3, 7);
    let LspWireMessage::Request { id, method, params } = message else {
        panic!("expected a request message");
    };

    assert_eq!(id, 11);
    assert_eq!(method, "textDocument/prepareRename");
    assert_eq!(
        params
            .get("textDocument")
            .and_then(|document| document.get("uri"))
            .and_then(Value::as_str),
        Some(path_to_file_uri(Path::new("src/main.rs")).as_str())
    );
    assert_eq!(
        params.get("position"),
        Some(&json!({ "line": 3, "character": 7 }))
    );
}

#[test]
fn initialize_request_advertises_rename_prepare_support() {
    let LspWireMessage::Request { method, params, .. } =
        LspWireMessage::initialize(1, Path::new("workspace"))
    else {
        panic!("expected the initialize request");
    };

    assert_eq!(method, "initialize");
    assert_eq!(
        params
            .get("capabilities")
            .and_then(|capabilities| capabilities.get("textDocument"))
            .and_then(|text_document| text_document.get("rename"))
            .and_then(|rename| rename.get("prepareSupport"))
            .and_then(Value::as_bool),
        Some(true)
    );
}

#[test]
fn deserialized_diagnostics_skip_malformed_entries_instead_of_dropping_publish() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        version: Some(7),
        diagnostics: vec![
            LspDiagnostic {
                range: LspRange {
                    start: LspPosition {
                        line: 0,
                        character: 4,
                    },
                    end: LspPosition {
                        line: 0,
                        character: 2,
                    },
                },
                severity: None,
                source: None,
                tags: Vec::new(),
                message: "bad range".to_owned(),
            },
            LspDiagnostic {
                range: LspRange {
                    start: LspPosition {
                        line: 2,
                        character: 1,
                    },
                    end: LspPosition {
                        line: 2,
                        character: 5,
                    },
                },
                severity: Some(1),
                source: None,
                tags: Vec::new(),
                message: "good range".to_owned(),
            },
        ],
    };

    // One malformed entry is skipped; the parseable rest still publishes so
    // stale diagnostics are replaced.
    let (_, version, diagnostics) = diagnostics_from_lsp(params).unwrap();
    assert_eq!(version, Some(7));
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
    assert_eq!(diagnostics[0].message, "good range");

    // A publish whose entries are all malformed still replaces state (with an
    // empty set); only an unusable URI fails the whole publish.
    let all_malformed = PublishDiagnosticsParams {
        uri,
        version: None,
        diagnostics: vec![LspDiagnostic {
            range: LspRange {
                start: LspPosition {
                    line: 0,
                    character: 4,
                },
                end: LspPosition {
                    line: 0,
                    character: 2,
                },
            },
            severity: None,
            source: None,
            tags: Vec::new(),
            message: "bad range".to_owned(),
        }],
    };
    let (_, _, diagnostics) = diagnostics_from_lsp(all_malformed).unwrap();
    assert!(diagnostics.is_empty());
}

#[test]
fn deserialized_multi_line_ranges_store_line_end_sentinel() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let params = PublishDiagnosticsParams {
        uri,
        version: None,
        diagnostics: vec![
            LspDiagnostic {
                range: LspRange {
                    start: LspPosition {
                        line: 2,
                        character: 4,
                    },
                    end: LspPosition {
                        line: 6,
                        character: 9,
                    },
                },
                severity: None,
                source: None,
                tags: Vec::new(),
                message: "multi line".to_owned(),
            },
            LspDiagnostic {
                range: LspRange {
                    start: LspPosition {
                        line: 0,
                        character: 3,
                    },
                    end: LspPosition {
                        line: 0,
                        character: 3,
                    },
                },
                severity: None,
                source: None,
                tags: Vec::new(),
                message: "zero width single line".to_owned(),
            },
        ],
    };

    let (_, _, diagnostics) = diagnostics_from_lsp(params).unwrap();

    assert_eq!(diagnostics.len(), 2);
    // The end character sits on a later line, so the stored range keeps the
    // start offset and a sentinel end for consumers to clamp to line end.
    assert_eq!(diagnostics[0].line, 3);
    assert_eq!(diagnostics[0].column, 5);
    assert_eq!(diagnostics[0].char_range, 4..usize::MAX);
    // Single-line ranges keep the minimum positive width.
    assert_eq!(diagnostics[1].char_range, 3..4);
}

#[test]
fn published_diagnostics_skip_malformed_entries_and_store_multi_line_sentinel() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let value = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": [
                {
                    "range": "garbage",
                    "message": "malformed"
                },
                {
                    "range": {
                        "start": { "line": 0, "character": 1 },
                        "end": { "line": 3, "character": 7 }
                    },
                    "message": "multi line"
                },
                {
                    "range": {
                        "start": { "line": 1, "character": 2 },
                        "end": { "line": 1, "character": 6 }
                    },
                    "message": "single line"
                }
            ]
        }
    });

    let (_, _, diagnostics) = parse_publish_diagnostics(&value).unwrap();

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].line, 1);
    assert_eq!(diagnostics[0].column, 2);
    assert_eq!(diagnostics[0].char_range, 1..usize::MAX);
    assert_eq!(diagnostics[0].message, "multi line");
    assert_eq!(diagnostics[1].char_range, 2..6);
    assert_eq!(diagnostics[1].message, "single line");
}

#[test]
fn publish_diagnostic_text_fields_are_sanitized_and_bounded() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let value = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": [
                {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 1 }
                    },
                    "source": format!(
                        "  {}\ntail",
                        "\u{03c3}".repeat(MAX_LSP_DIAGNOSTIC_SOURCE_CHARS + 4)
                    ),
                    "message": format!(
                        "{}tail",
                        "\u{03bc}".repeat(MAX_LSP_DIAGNOSTIC_MESSAGE_CHARS + 4)
                    )
                },
                {
                    "range": {
                        "start": { "line": 1, "character": 0 },
                        "end": { "line": 1, "character": 1 }
                    },
                    "source": " \n\t ",
                    "message": " \n\t "
                }
            ]
        }
    });

    let (_, _, diagnostics) = parse_publish_diagnostics(&value).unwrap();

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(
        diagnostics[0].source.matches('\u{03c3}').count(),
        MAX_LSP_DIAGNOSTIC_SOURCE_CHARS
    );
    assert!(!diagnostics[0].source.contains("tail"));
    assert!(!diagnostics[0].source.contains('\n'));
    assert_eq!(
        diagnostics[0].message.matches('\u{03bc}').count(),
        MAX_LSP_DIAGNOSTIC_MESSAGE_CHARS
    );
    assert!(!diagnostics[0].message.contains("tail"));
    assert_eq!(diagnostics[1].source, "lsp");
    assert_eq!(diagnostics[1].message, "LSP diagnostic");
}

#[test]
fn publish_diagnostics_are_bounded_per_file() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let diagnostics = (0..MAX_LSP_DIAGNOSTICS_PER_FILE + 25)
        .map(|idx| {
            json!({
                "range": {
                    "start": { "line": idx, "character": 0 },
                    "end": { "line": idx, "character": 4 }
                },
                "severity": 2,
                "message": format!("diagnostic {idx}")
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics
        }
    });

    let (_, _, diagnostics) = parse_publish_diagnostics(&value).unwrap();

    assert_eq!(diagnostics.len(), MAX_LSP_DIAGNOSTICS_PER_FILE);
    assert_eq!(
        diagnostics.last().map(|diagnostic| diagnostic.line),
        Some(MAX_LSP_DIAGNOSTICS_PER_FILE)
    );
}

#[test]
fn publish_diagnostics_do_not_parse_past_per_file_bound() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let mut diagnostics = (0..MAX_LSP_DIAGNOSTICS_PER_FILE)
        .map(|idx| {
            json!({
                "range": {
                    "start": { "line": idx, "character": 0 },
                    "end": { "line": idx, "character": 4 }
                },
                "message": format!("diagnostic {idx}")
            })
        })
        .collect::<Vec<_>>();
    diagnostics.push(json!({
        "range": "not a diagnostic range",
        "message": "past the bound"
    }));
    let value = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics
        }
    });

    let (_, _, diagnostics) = parse_publish_diagnostics(&value).unwrap();

    assert_eq!(diagnostics.len(), MAX_LSP_DIAGNOSTICS_PER_FILE);
    assert_eq!(
        diagnostics
            .last()
            .map(|diagnostic| diagnostic.message.as_str()),
        Some("diagnostic 4999")
    );
}

#[test]
fn initialize_advertises_work_done_progress() {
    let value = LspWireMessage::initialize(1, Path::new("workspace")).to_json();

    assert_eq!(
        value["params"]["capabilities"]["window"]["workDoneProgress"],
        true
    );
}

#[test]
fn response_message_serializes_json_rpc_result() {
    let value = LspWireMessage::response(7, json!(null)).to_json();

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 7);
    assert!(value["result"].is_null());
    assert!(value.get("method").is_none());
}

#[test]
fn parses_work_done_progress_notifications() {
    let begin = json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": "rust-analyzer:index",
            "value": {
                "kind": "begin",
                "title": "Indexing",
                "message": "Scanning workspace",
                "percentage": 12
            }
        }
    });

    let progress = parse_work_done_progress(&begin).unwrap();
    assert_eq!(progress.token, "rust-analyzer:index");
    assert_eq!(progress.kind, LspWorkDoneProgressKind::Begin);
    assert_eq!(progress.title.as_deref(), Some("Indexing"));
    assert_eq!(progress.message.as_deref(), Some("Scanning workspace"));
    assert_eq!(progress.percentage, Some(12));

    let report = json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": 42,
            "value": {
                "kind": "report",
                "message": "Finishing",
                "percentage": 150
            }
        }
    });

    let progress = parse_work_done_progress(&report).unwrap();
    assert_eq!(progress.token, "42");
    assert_eq!(progress.kind, LspWorkDoneProgressKind::Report);
    assert_eq!(progress.percentage, Some(100));
}

#[test]
fn work_done_progress_strings_are_sanitized_and_bounded() {
    let value = json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": format!(
                "{}tail",
                "\u{03c4}".repeat(MAX_LSP_PROGRESS_TOKEN_CHARS + 4)
            ),
            "value": {
                "kind": "begin",
                "title": "Build\nWorkspace",
                "message": format!(
                    "{}tail",
                    "\u{03bc}".repeat(MAX_LSP_PROGRESS_MESSAGE_CHARS + 4)
                ),
                "percentage": 7
            }
        }
    });

    let progress = parse_work_done_progress(&value).unwrap();

    assert_eq!(
        progress.token.matches('\u{03c4}').count(),
        MAX_LSP_PROGRESS_TOKEN_CHARS
    );
    assert!(!progress.token.contains("tail"));
    assert_eq!(progress.title.as_deref(), Some("Build Workspace"));
    let message = progress.message.as_deref().unwrap();
    assert_eq!(
        message.matches('\u{03bc}').count(),
        MAX_LSP_PROGRESS_MESSAGE_CHARS
    );
    assert!(!message.contains("tail"));
}

#[test]
fn parses_work_done_progress_create_requests() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "window/workDoneProgress/create",
        "params": {
            "token": "cargo-check"
        }
    });

    let request = parse_work_done_progress_create(&value).unwrap();
    assert_eq!(request.id, LspRequestId::Number(9));
    assert_eq!(request.token, "cargo-check");
}

#[test]
fn parses_work_done_progress_create_requests_with_string_ids() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": "progress-create-9",
        "method": "window/workDoneProgress/create",
        "params": {
            "token": "cargo-check"
        }
    });

    let request = parse_work_done_progress_create(&value).unwrap();
    assert_eq!(
        request.id,
        LspRequestId::String("progress-create-9".to_owned())
    );
    assert_eq!(request.token, "cargo-check");
}

#[test]
fn work_done_progress_create_tokens_are_bounded() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "window/workDoneProgress/create",
        "params": {
            "token": format!(
                "{}tail",
                "\u{03c1}".repeat(MAX_LSP_PROGRESS_TOKEN_CHARS + 4)
            )
        }
    });

    let request = parse_work_done_progress_create(&value).unwrap();

    assert_eq!(
        request.token.matches('\u{03c1}').count(),
        MAX_LSP_PROGRESS_TOKEN_CHARS
    );
    assert!(!request.token.contains("tail"));
}

#[test]
fn did_save_notification_uses_text_document_uri() {
    let value = LspWireMessage::did_save(Path::new("src/main.rs")).to_json();

    assert_eq!(value["method"], "textDocument/didSave");
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
    assert!(value.get("id").is_none());
}

#[test]
fn did_close_notification_uses_text_document_uri() {
    let value = LspWireMessage::did_close(Path::new("src/main.rs")).to_json();

    assert_eq!(value["method"], "textDocument/didClose");
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
    assert!(value.get("id").is_none());
}

#[test]
fn cancel_request_notification_uses_lsp_cancel_method() {
    let value = LspWireMessage::cancel_request(42).to_json();

    assert_eq!(value["method"], "$/cancelRequest");
    assert_eq!(value["params"]["id"], 42);
    assert!(value.get("id").is_none());
}

#[test]
fn hover_request_uses_text_document_position_params() {
    let value = LspWireMessage::hover(9, Path::new("src/main.rs"), 4, 2).to_json();

    assert_eq!(value["id"], 9);
    assert_eq!(value["method"], "textDocument/hover");
    assert_eq!(value["params"]["position"]["line"], 4);
    assert_eq!(value["params"]["position"]["character"], 2);
}

#[test]
fn request_builders_clamp_oversized_lsp_coordinates() {
    let value =
        LspWireMessage::hover(9, Path::new("src/main.rs"), usize::MAX, usize::MAX).to_json();

    assert_eq!(
        value["params"]["position"]["line"],
        MAX_LSP_POSITION_COMPONENT
    );
    assert_eq!(
        value["params"]["position"]["character"],
        MAX_LSP_POSITION_COMPONENT
    );

    let range =
        LspWireMessage::inlay_hints(10, Path::new("src/main.rs"), usize::MAX, usize::MAX, 1, 0)
            .to_json();

    assert_eq!(
        range["params"]["range"]["start"]["line"],
        MAX_LSP_POSITION_COMPONENT
    );
    assert_eq!(
        range["params"]["range"]["end"]["line"],
        MAX_LSP_POSITION_COMPONENT
    );
    assert_eq!(
        range["params"]["range"]["end"]["character"],
        MAX_LSP_POSITION_COMPONENT
    );
}

#[test]
fn rename_request_includes_new_name() {
    let value = LspWireMessage::rename(14, Path::new("src/main.rs"), 3, 9, "new_name").to_json();

    assert_eq!(value["id"], 14);
    assert_eq!(value["method"], "textDocument/rename");
    assert_eq!(value["params"]["position"]["line"], 3);
    assert_eq!(value["params"]["position"]["character"], 9);
    assert_eq!(value["params"]["newName"], "new_name");
}

#[test]
fn workspace_execute_command_request_uses_command_and_arguments() {
    let arguments = std::sync::Arc::new(json!([{ "kind": "test", "id": 9 }]));
    let value =
        LspWireMessage::workspace_execute_command(36, "rust-analyzer.runSingle", Some(&arguments))
            .expect("valid command")
            .to_json();

    assert_eq!(value["id"], 36);
    assert_eq!(value["method"], "workspace/executeCommand");
    assert_eq!(value["params"]["command"], "rust-analyzer.runSingle");
    assert_eq!(value["params"]["arguments"][0]["id"], 9);
}

#[test]
fn parses_hover_responses_from_markup_and_marked_strings() {
    let markup = json!({
        "jsonrpc": "2.0",
        "id": 10,
        "result": {
            "contents": {
                "kind": "markdown",
                "value": "```rust\nfn main()\n```"
            }
        }
    });
    assert_eq!(
        parse_hover_response(&markup).unwrap().text,
        "```rust\nfn main()\n```"
    );

    let marked = json!({
        "jsonrpc": "2.0",
        "id": 11,
        "result": {
            "contents": [
                "String",
                { "language": "rust", "value": "struct String" }
            ]
        }
    });
    assert_eq!(
        parse_hover_response(&marked).unwrap().text,
        "String\n\n```rust\nstruct String\n```"
    );
}

#[test]
fn hover_response_text_is_sanitized_and_bounded() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 10,
        "result": {
            "contents": format!(
                "prefix\u{0007}{}tail",
                "x".repeat(MAX_LSP_HOVER_CHARS)
            )
        }
    });

    let hover = parse_hover_response(&value).unwrap();

    assert_eq!(hover.text.chars().count(), MAX_LSP_HOVER_CHARS);
    assert!(hover.text.contains("prefix "));
    assert!(!hover.text.contains('\u{0007}'));
    assert!(!hover.text.contains("tail"));
}

#[test]
fn lsp_text_fields_strip_hidden_format_controls() {
    let hover = json!({
        "jsonrpc": "2.0",
        "id": 10,
        "result": {
            "contents": {
                "kind": "markdown",
                "value": "safe\u{202e} hover\u{2066}"
            }
        }
    });
    let progress = json!({
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": "rust-analyzer\u{202d}:index",
            "value": {
                "kind": "begin",
                "title": "Index\u{200f}ing",
                "message": "scan\u{2069}ning"
            }
        }
    });

    let hover = parse_hover_response(&hover).unwrap();
    let progress = parse_work_done_progress(&progress).unwrap();

    assert_eq!(hover.text, "safe hover");
    assert_eq!(progress.token, "rust-analyzer:index");
    assert_eq!(progress.title.as_deref(), Some("Indexing"));
    assert_eq!(progress.message.as_deref(), Some("scanning"));
}

#[test]
fn lsp_position_parsing_rejects_out_of_range_components() {
    let too_large = MAX_LSP_POSITION_COMPONENT + 1;

    assert!(
        parse_lsp_position(&json!({
            "line": too_large,
            "character": 0
        }))
        .is_none()
    );
    assert!(
        parse_lsp_range(&json!({
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": too_large }
        }))
        .is_none()
    );
}

use super::*;

#[test]
fn initialize_advertises_inlay_hint_support() {
    let value = LspWireMessage::initialize(1, Path::new("workspace")).to_json();

    assert_eq!(
        value["params"]["capabilities"]["textDocument"]["inlayHint"]["dynamicRegistration"],
        false
    );
}

#[test]
fn initialize_advertises_call_hierarchy_support() {
    let value = LspWireMessage::initialize(1, Path::new("workspace")).to_json();

    assert_eq!(
        value["params"]["capabilities"]["textDocument"]["callHierarchy"]["dynamicRegistration"],
        false
    );
}

#[test]
fn initialize_advertises_type_hierarchy_support() {
    let value = LspWireMessage::initialize(1, Path::new("workspace")).to_json();

    assert_eq!(
        value["params"]["capabilities"]["textDocument"]["typeHierarchy"]["dynamicRegistration"],
        false
    );
}

#[test]
fn initialize_advertises_code_lens_support() {
    let value = LspWireMessage::initialize(1, Path::new("workspace")).to_json();

    assert_eq!(
        value["params"]["capabilities"]["textDocument"]["codeLens"]["dynamicRegistration"],
        false
    );
    assert_eq!(
        value["params"]["capabilities"]["textDocument"]["codeLens"]["resolveSupport"]["properties"],
        json!(["command"])
    );
}

#[test]
fn initialize_advertises_semantic_token_support() {
    let value = LspWireMessage::initialize(1, Path::new("workspace")).to_json();
    let semantic_tokens = &value["params"]["capabilities"]["textDocument"]["semanticTokens"];

    assert_eq!(semantic_tokens["dynamicRegistration"], false);
    assert_eq!(semantic_tokens["requests"]["full"]["delta"], false);
    assert_eq!(semantic_tokens["requests"]["range"], false);
    assert_eq!(semantic_tokens["tokenTypes"][0], "namespace");
    assert_eq!(semantic_tokens["tokenTypes"][12], "function");
    assert_eq!(semantic_tokens["tokenModifiers"][0], "declaration");
    assert_eq!(semantic_tokens["formats"], json!(["relative"]));
}

#[test]
fn initialize_semantic_tokens_capability_is_complete_for_strict_server_decoders() {
    let value = LspWireMessage::initialize(1, Path::new("workspace")).to_json();
    let semantic_tokens = &value["params"]["capabilities"]["textDocument"]["semanticTokens"];

    for required_key in ["requests", "tokenTypes", "tokenModifiers", "formats"] {
        assert!(
            semantic_tokens.get(required_key).is_some(),
            "semanticTokens.{required_key} must be advertised; strict server decoders reject the capability without it"
        );
    }
    assert_eq!(semantic_tokens["formats"], json!(["relative"]));
    assert!(
        !semantic_tokens["tokenTypes"]
            .as_array()
            .expect("tokenTypes array")
            .is_empty()
    );
    assert!(
        !semantic_tokens["tokenModifiers"]
            .as_array()
            .expect("tokenModifiers array")
            .is_empty()
    );
    assert!(semantic_tokens["requests"].get("range").is_some());
    assert!(semantic_tokens["requests"].get("full").is_some());
}

#[test]
fn initialize_capability_sub_objects_carry_required_fields_of_strict_decoders() {
    let value = LspWireMessage::initialize(1, Path::new("workspace")).to_json();
    let text_document = &value["params"]["capabilities"]["textDocument"];

    assert!(
        !text_document["codeAction"]["codeActionLiteralSupport"]["codeActionKind"]["valueSet"]
            .as_array()
            .expect("codeActionKind.valueSet array")
            .is_empty()
    );
    assert!(
        !text_document["codeAction"]["resolveSupport"]["properties"]
            .as_array()
            .expect("codeAction resolveSupport.properties array")
            .is_empty()
    );
    assert!(
        !text_document["completion"]["completionItem"]["resolveSupport"]["properties"]
            .as_array()
            .expect("completion resolveSupport.properties array")
            .is_empty()
    );
    assert!(
        !text_document["hover"]["contentFormat"]
            .as_array()
            .expect("hover.contentFormat array")
            .is_empty()
    );
    assert_eq!(
        text_document["signatureHelp"]["signatureInformation"]["parameterInformation"]["labelOffsetSupport"],
        true
    );
}

#[test]
fn document_highlight_request_uses_text_document_position_params() {
    let value = LspWireMessage::document_highlight(29, Path::new("src/main.rs"), 5, 11).to_json();

    assert_eq!(value["id"], 29);
    assert_eq!(value["method"], "textDocument/documentHighlight");
    assert_eq!(value["params"]["position"]["line"], 5);
    assert_eq!(value["params"]["position"]["character"], 11);
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
}

#[test]
fn call_hierarchy_requests_use_lsp_methods_and_items() {
    let prepare =
        LspWireMessage::prepare_call_hierarchy(41, Path::new("src/main.rs"), 5, 11).to_json();

    assert_eq!(prepare["id"], 41);
    assert_eq!(prepare["method"], "textDocument/prepareCallHierarchy");
    assert_eq!(prepare["params"]["position"]["line"], 5);
    assert_eq!(prepare["params"]["position"]["character"], 11);
    assert!(
        prepare["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );

    let item = LspCallHierarchyItem {
        name: "main".to_owned(),
        detail: None,
        kind: 12,
        path: PathBuf::from("src/main.rs"),
        line: 6,
        column: 12,
        end_line: 6,
        end_column: 16,
        raw: json!({
            "name": "main",
            "kind": 12,
            "uri": "file:///src/main.rs",
            "range": {
                "start": { "line": 5, "character": 11 },
                "end": { "line": 5, "character": 15 }
            },
            "selectionRange": {
                "start": { "line": 5, "character": 11 },
                "end": { "line": 5, "character": 15 }
            },
            "data": { "id": 7 }
        }),
    };

    let incoming = LspWireMessage::call_hierarchy_incoming(42, &item).to_json();
    let outgoing = LspWireMessage::call_hierarchy_outgoing(43, &item).to_json();

    assert_eq!(incoming["method"], "callHierarchy/incomingCalls");
    assert_eq!(outgoing["method"], "callHierarchy/outgoingCalls");
    assert_eq!(incoming["params"]["item"]["data"]["id"], 7);
    assert_eq!(outgoing["params"]["item"]["name"], "main");
}

#[test]
fn type_hierarchy_requests_use_lsp_methods_and_items() {
    let prepare =
        LspWireMessage::prepare_type_hierarchy(44, Path::new("src/main.rs"), 5, 11).to_json();

    assert_eq!(prepare["id"], 44);
    assert_eq!(prepare["method"], "textDocument/prepareTypeHierarchy");
    assert_eq!(prepare["params"]["position"]["line"], 5);
    assert_eq!(prepare["params"]["position"]["character"], 11);

    let item = LspTypeHierarchyItem {
        name: "Widget".to_owned(),
        detail: None,
        kind: 5,
        path: PathBuf::from("src/main.rs"),
        line: 6,
        column: 12,
        end_line: 6,
        end_column: 18,
        raw: json!({
            "name": "Widget",
            "kind": 5,
            "uri": "file:///src/main.rs",
            "range": {
                "start": { "line": 5, "character": 11 },
                "end": { "line": 5, "character": 17 }
            },
            "selectionRange": {
                "start": { "line": 5, "character": 11 },
                "end": { "line": 5, "character": 17 }
            },
            "data": { "id": 9 }
        }),
    };

    let supertypes = LspWireMessage::type_hierarchy_supertypes(45, &item).to_json();
    let subtypes = LspWireMessage::type_hierarchy_subtypes(46, &item).to_json();

    assert_eq!(supertypes["method"], "typeHierarchy/supertypes");
    assert_eq!(subtypes["method"], "typeHierarchy/subtypes");
    assert_eq!(supertypes["params"]["item"]["data"]["id"], 9);
    assert_eq!(subtypes["params"]["item"]["name"], "Widget");
}

#[test]
fn document_symbols_request_uses_text_document_uri() {
    let value = LspWireMessage::document_symbols(16, Path::new("src/lib.rs")).to_json();

    assert_eq!(value["id"], 16);
    assert_eq!(value["method"], "textDocument/documentSymbol");
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
}

#[test]
fn folding_ranges_request_uses_text_document_uri() {
    let value = LspWireMessage::folding_ranges(31, Path::new("src/lib.rs")).to_json();

    assert_eq!(value["id"], 31);
    assert_eq!(value["method"], "textDocument/foldingRange");
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
}

#[test]
fn inlay_hints_request_uses_document_range() {
    let value = LspWireMessage::inlay_hints(33, Path::new("src/lib.rs"), 2, 0, 12, 24).to_json();

    assert_eq!(value["id"], 33);
    assert_eq!(value["method"], "textDocument/inlayHint");
    assert_eq!(value["params"]["range"]["start"]["line"], 2);
    assert_eq!(value["params"]["range"]["start"]["character"], 0);
    assert_eq!(value["params"]["range"]["end"]["line"], 12);
    assert_eq!(value["params"]["range"]["end"]["character"], 24);
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
}

#[test]
fn code_lenses_request_uses_text_document_uri() {
    let value = LspWireMessage::code_lenses(34, Path::new("src/lib.rs")).to_json();

    assert_eq!(value["id"], 34);
    assert_eq!(value["method"], "textDocument/codeLens");
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
}

#[test]
fn code_lens_resolve_request_uses_original_lens_as_params() {
    let lens = LspCodeLens {
        line: 6,
        column: 5,
        title: String::new(),
        command: None,
        command_arguments: None,
        resolve_payload: Some(std::sync::Arc::new(json!({
            "range": {
                "start": { "line": 5, "character": 4 },
                "end": { "line": 5, "character": 12 }
            },
            "data": { "id": 9 }
        }))),
    };

    let value = LspWireMessage::code_lens_resolve(35, &lens)
        .expect("resolve payload")
        .to_json();

    assert_eq!(value["id"], 35);
    assert_eq!(value["method"], "codeLens/resolve");
    assert_eq!(value["params"]["data"]["id"], 9);
    assert_eq!(value["params"]["range"]["start"]["line"], 5);
}

#[test]
fn semantic_tokens_request_uses_text_document_uri() {
    let value = LspWireMessage::semantic_tokens(35, Path::new("src/lib.rs")).to_json();

    assert_eq!(value["id"], 35);
    assert_eq!(value["method"], "textDocument/semanticTokens/full");
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
}

#[test]
fn workspace_symbols_request_uses_query() {
    let value = LspWireMessage::workspace_symbols(18, "handler").to_json();

    assert_eq!(value["id"], 18);
    assert_eq!(value["method"], "workspace/symbol");
    assert_eq!(value["params"]["query"], "handler");
}

#[test]
fn references_request_uses_position_and_context() {
    let value = LspWireMessage::references(25, Path::new("src/lib.rs"), 7, 3, true).to_json();

    assert_eq!(value["id"], 25);
    assert_eq!(value["method"], "textDocument/references");
    assert_eq!(value["params"]["position"]["line"], 7);
    assert_eq!(value["params"]["position"]["character"], 3);
    assert_eq!(value["params"]["context"]["includeDeclaration"], true);
    assert!(
        value["params"]["textDocument"]["uri"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("file://"))
    );
}

#[test]
fn parses_document_highlights_and_deduplicates_ranges() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 30,
        "result": [
            {
                "range": {
                    "start": { "line": 2, "character": 4 },
                    "end": { "line": 2, "character": 10 }
                },
                "kind": 2
            },
            {
                "range": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 1, "character": 4 }
                },
                "kind": 3
            },
            {
                "range": {
                    "start": { "line": 2, "character": 4 },
                    "end": { "line": 2, "character": 10 }
                },
                "kind": 2
            }
        ]
    });

    let highlights = parse_document_highlight_response(&value).unwrap();
    assert_eq!(highlights.len(), 2);
    assert_eq!(highlights[0].line, 2);
    assert_eq!(highlights[0].column, 1);
    assert_eq!(highlights[0].end_column, 5);
    assert_eq!(highlights[0].kind, Some(3));
    assert_eq!(highlights[1].line, 3);
    assert_eq!(highlights[1].column, 5);
    assert_eq!(highlights[1].kind, Some(2));
}

#[test]
fn parses_definition_location_and_location_link() {
    let uri = path_to_file_uri(Path::new("src/lib.rs"));
    let location = json!({
        "jsonrpc": "2.0",
        "id": 12,
        "result": {
            "uri": uri,
            "range": {
                "start": { "line": 2, "character": 5 },
                "end": { "line": 2, "character": 8 }
            }
        }
    });
    let definition = parse_definition_response(&location).unwrap();
    assert!(definition.path.ends_with(Path::new("src").join("lib.rs")));
    assert_eq!(definition.line, 3);
    assert_eq!(definition.column, 6);

    let target_uri = path_to_file_uri(Path::new("src/main.rs"));
    let link = json!({
        "jsonrpc": "2.0",
        "id": 13,
        "result": [{
            "targetUri": target_uri,
            "targetSelectionRange": {
                "start": { "line": 0, "character": 1 },
                "end": { "line": 0, "character": 4 }
            }
        }]
    });
    let definition = parse_definition_response(&link).unwrap();
    assert!(definition.path.ends_with(Path::new("src").join("main.rs")));
    assert_eq!(definition.line, 1);
    assert_eq!(definition.column, 2);
}

#[test]
fn parses_reference_locations_and_location_links() {
    let first_uri = path_to_file_uri(Path::new("src/main.rs"));
    let second_uri = path_to_file_uri(Path::new("src/lib.rs"));
    let value = json!({
        "jsonrpc": "2.0",
        "id": 26,
        "result": [
            {
                "uri": first_uri,
                "range": {
                    "start": { "line": 4, "character": 8 },
                    "end": { "line": 4, "character": 12 }
                }
            },
            {
                "targetUri": second_uri,
                "targetSelectionRange": {
                    "start": { "line": 1, "character": 2 },
                    "end": { "line": 1, "character": 6 }
                }
            },
            {
                "uri": first_uri,
                "range": {
                    "start": { "line": 4, "character": 8 },
                    "end": { "line": 4, "character": 12 }
                }
            }
        ]
    });

    let references = parse_references_response(&value).unwrap();
    assert_eq!(references.len(), 2);
    assert!(
        references[0]
            .path
            .ends_with(Path::new("src").join("lib.rs"))
    );
    assert_eq!(references[0].line, 2);
    assert_eq!(references[0].column, 3);
    assert_eq!(references[0].end_column, 7);
    assert!(
        references[1]
            .path
            .ends_with(Path::new("src").join("main.rs"))
    );
    assert_eq!(references[1].line, 5);
    assert_eq!(references[1].column, 9);
    assert_eq!(references[1].end_line, 5);
}

#[test]
fn reference_results_are_bounded() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let result = (0..MAX_LSP_REFERENCES + 20)
        .map(|idx| {
            json!({
                "uri": uri,
                "range": {
                    "start": { "line": idx, "character": 0 },
                    "end": { "line": idx, "character": 4 }
                }
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "jsonrpc": "2.0",
        "id": 26,
        "result": result
    });

    let references = parse_references_response(&value).unwrap();

    assert_eq!(references.len(), MAX_LSP_REFERENCES);
    assert_eq!(
        references.last().map(|reference| reference.line),
        Some(MAX_LSP_REFERENCES)
    );
}

#[test]
fn parses_call_hierarchy_prepare_and_call_responses() {
    let root_uri = path_to_file_uri(Path::new("src/main.rs"));
    let caller_uri = path_to_file_uri(Path::new("src/caller.rs"));
    let prepare = json!({
        "jsonrpc": "2.0",
        "id": 41,
        "result": [{
            "name": "main",
            "detail": "fn main()",
            "kind": 12,
            "uri": root_uri,
            "range": {
                "start": { "line": 4, "character": 0 },
                "end": { "line": 6, "character": 1 }
            },
            "selectionRange": {
                "start": { "line": 4, "character": 3 },
                "end": { "line": 4, "character": 7 }
            },
            "data": { "token": "main" }
        }]
    });

    let items = parse_call_hierarchy_prepare_response(&prepare).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "main");
    assert_eq!(items[0].detail.as_deref(), Some("fn main()"));
    assert_eq!(items[0].line, 5);
    assert_eq!(items[0].column, 4);
    assert_eq!(items[0].raw["data"]["token"], "main");

    let calls = json!({
        "jsonrpc": "2.0",
        "id": 42,
        "result": [{
            "from": {
                "name": "caller",
                "kind": 12,
                "uri": caller_uri,
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 2, "character": 1 }
                },
                "selectionRange": {
                    "start": { "line": 0, "character": 3 },
                    "end": { "line": 0, "character": 9 }
                }
            },
            "fromRanges": [{
                "start": { "line": 1, "character": 4 },
                "end": { "line": 1, "character": 10 }
            }]
        }]
    });

    let incoming = parse_call_hierarchy_incoming_response(&calls).unwrap();
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].item.name, "caller");
    assert_eq!(incoming[0].item.line, 1);
    assert_eq!(incoming[0].ranges[0].line, 2);
    assert_eq!(incoming[0].ranges[0].column, 5);
}

#[test]
fn call_hierarchy_items_bound_fields_and_raw_payloads() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let value = json!({
        "jsonrpc": "2.0",
        "id": 41,
        "result": [
            {
                "name": "oversized-payload",
                "kind": 12,
                "uri": uri,
                "selectionRange": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 4 }
                },
                "data": { "blob": "x".repeat(MAX_LSP_CALL_HIERARCHY_ITEM_PAYLOAD_BYTES + 1) }
            },
            {
                "name": format!(
                    "{}tail",
                    "\u{03bd}".repeat(MAX_LSP_CALL_HIERARCHY_NAME_CHARS + 4)
                ),
                "detail": format!(
                    "{}tail",
                    "\u{03b4}".repeat(MAX_LSP_CALL_HIERARCHY_DETAIL_CHARS + 4)
                ),
                "kind": 12,
                "uri": path_to_file_uri(Path::new("src/lib.rs")),
                "selectionRange": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 1, "character": 4 }
                }
            },
            {
                "name": "bad-kind",
                "kind": 300,
                "uri": path_to_file_uri(Path::new("src/bad.rs")),
                "selectionRange": {
                    "start": { "line": 2, "character": 0 },
                    "end": { "line": 2, "character": 4 }
                }
            }
        ]
    });

    let items = parse_call_hierarchy_prepare_response(&value).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].name.matches('\u{03bd}').count(),
        MAX_LSP_CALL_HIERARCHY_NAME_CHARS
    );
    assert!(!items[0].name.contains("tail"));
    let detail = items[0].detail.as_deref().unwrap();
    assert_eq!(
        detail.matches('\u{03b4}').count(),
        MAX_LSP_CALL_HIERARCHY_DETAIL_CHARS
    );
    assert!(!detail.contains("tail"));
}

#[test]
fn parses_type_hierarchy_prepare_and_relation_responses() {
    let root_uri = path_to_file_uri(Path::new("src/widget.rs"));
    let parent_uri = path_to_file_uri(Path::new("src/parent.rs"));
    let prepare = json!({
        "jsonrpc": "2.0",
        "id": 44,
        "result": [{
            "name": "Widget",
            "detail": "struct Widget",
            "kind": 23,
            "uri": root_uri,
            "range": {
                "start": { "line": 3, "character": 0 },
                "end": { "line": 8, "character": 1 }
            },
            "selectionRange": {
                "start": { "line": 3, "character": 7 },
                "end": { "line": 3, "character": 13 }
            },
            "data": { "token": "widget" }
        }]
    });

    let items = parse_type_hierarchy_prepare_response(&prepare).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "Widget");
    assert_eq!(items[0].detail.as_deref(), Some("struct Widget"));
    assert_eq!(items[0].line, 4);
    assert_eq!(items[0].column, 8);
    assert_eq!(items[0].raw["data"]["token"], "widget");

    let relations = json!({
        "jsonrpc": "2.0",
        "id": 45,
        "result": [{
            "name": "Parent",
            "kind": 5,
            "uri": parent_uri,
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 2, "character": 1 }
            },
            "selectionRange": {
                "start": { "line": 0, "character": 6 },
                "end": { "line": 0, "character": 12 }
            }
        }]
    });

    let supertypes = parse_type_hierarchy_supertypes_response(&relations).unwrap();
    assert_eq!(supertypes.len(), 1);
    assert_eq!(supertypes[0].name, "Parent");
    assert_eq!(supertypes[0].line, 1);
    assert_eq!(supertypes[0].column, 7);
}

#[test]
fn parses_hierarchical_and_flat_document_symbols() {
    let document_path = PathBuf::from("src").join("lib.rs");
    let hierarchical = json!({
        "jsonrpc": "2.0",
        "id": 17,
        "result": [{
            "name": "module",
            "detail": "mod",
            "kind": 2,
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 10, "character": 1 }
            },
            "selectionRange": {
                "start": { "line": 0, "character": 4 },
                "end": { "line": 0, "character": 10 }
            },
            "children": [{
                "name": "run",
                "kind": 12,
                "range": {
                    "start": { "line": 2, "character": 0 },
                    "end": { "line": 4, "character": 1 }
                },
                "selectionRange": {
                    "start": { "line": 2, "character": 3 },
                    "end": { "line": 2, "character": 6 }
                }
            }]
        }]
    });

    let symbols = parse_document_symbols_response(&hierarchical, &document_path).unwrap();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "module");
    assert_eq!(symbols[0].line, 1);
    assert_eq!(symbols[0].column, 5);
    assert_eq!(symbols[1].name, "run");
    assert_eq!(symbols[1].depth, 1);

    let flat_uri = path_to_file_uri(Path::new("src/main.rs"));
    let flat = json!({
        "jsonrpc": "2.0",
        "id": 18,
        "result": [{
            "name": "main",
            "kind": 12,
            "containerName": "crate",
            "location": {
                "uri": flat_uri,
                "range": {
                    "start": { "line": 0, "character": 3 },
                    "end": { "line": 1, "character": 1 }
                }
            }
        }]
    });
    let symbols = parse_document_symbols_response(&flat, &document_path).unwrap();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "main");
    assert_eq!(symbols[0].detail.as_deref(), Some("crate"));
    assert!(symbols[0].path.ends_with(Path::new("src").join("main.rs")));
}

#[test]
fn document_symbol_results_are_bounded() {
    let document_path = PathBuf::from("src").join("lib.rs");
    let symbols = (0..MAX_LSP_DOCUMENT_SYMBOLS + 20)
        .map(|idx| {
            json!({
                "name": format!("symbol_{idx}"),
                "kind": 12,
                "range": {
                    "start": { "line": idx, "character": 0 },
                    "end": { "line": idx, "character": 6 }
                },
                "selectionRange": {
                    "start": { "line": idx, "character": 0 },
                    "end": { "line": idx, "character": 6 }
                }
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "jsonrpc": "2.0",
        "id": 17,
        "result": symbols
    });

    let symbols = parse_document_symbols_response(&value, &document_path).unwrap();

    assert_eq!(symbols.len(), MAX_LSP_DOCUMENT_SYMBOLS);
    assert_eq!(
        symbols.last().map(|symbol| symbol.name.as_str()),
        Some("symbol_4999")
    );
}

#[test]
fn parses_folding_ranges_and_skips_single_line_ranges() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 32,
        "result": [
            {
                "startLine": 1,
                "startCharacter": 4,
                "endLine": 8,
                "endCharacter": 1,
                "kind": "region"
            },
            {
                "startLine": 1,
                "startCharacter": 4,
                "endLine": 8,
                "endCharacter": 1,
                "kind": "region"
            },
            {
                "startLine": 4,
                "endLine": 4
            }
        ]
    });

    let ranges = parse_folding_ranges_response(&value).unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start_line, 2);
    assert_eq!(ranges[0].start_column, Some(5));
    assert_eq!(ranges[0].end_line, 9);
    assert_eq!(ranges[0].end_column, Some(2));
    assert_eq!(ranges[0].kind.as_deref(), Some("region"));
}

#[test]
fn parses_inlay_hints_from_string_and_label_parts() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 33,
        "result": [
            {
                "position": { "line": 2, "character": 16 },
                "label": ": usize",
                "kind": 1
            },
            {
                "position": { "line": 1, "character": 8 },
                "label": [
                    { "value": "name" },
                    { "value": ": " },
                    { "value": "&str" }
                ],
                "kind": 2
            },
            {
                "position": { "line": 2, "character": 16 },
                "label": ": usize",
                "kind": 1
            }
        ]
    });

    let hints = parse_inlay_hints_response(&value).unwrap();

    assert_eq!(hints.len(), 2);
    assert_eq!(hints[0].line, 2);
    assert_eq!(hints[0].column, 9);
    assert_eq!(hints[0].label, "name: &str");
    assert_eq!(hints[0].kind, Some(2));
    assert_eq!(hints[1].line, 3);
    assert_eq!(hints[1].column, 17);
    assert_eq!(hints[1].label, ": usize");
    assert_eq!(hints[1].kind, Some(1));
}

#[test]
fn inlay_hint_labels_are_sanitized_and_bounded() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 33,
        "result": [
            {
                "position": { "line": 1, "character": 0 },
                "label": "  alpha\nbeta\t",
                "kind": 1
            },
            {
                "position": { "line": 2, "character": 0 },
                "label": [
                    { "value": "\u{03b1}".repeat(MAX_LSP_INLAY_HINT_LABEL_CHARS + 4) },
                    { "value": "tail" }
                ],
                "kind": 1
            },
            {
                "position": { "line": 3, "character": 0 },
                "label": [
                    { "value": "\t  " },
                    { "value": "gamma" },
                    { "value": "\n" },
                    { "value": "delta  " }
                ],
                "kind": 1
            }
        ]
    });

    let hints = parse_inlay_hints_response(&value).unwrap();

    assert_eq!(hints.len(), 3);
    assert_eq!(hints[0].label, "alpha beta");
    assert_eq!(
        hints[1].label.matches('\u{03b1}').count(),
        MAX_LSP_INLAY_HINT_LABEL_CHARS
    );
    assert!(!hints[1].label.contains("tail"));
    assert_eq!(hints[2].label, "gamma delta");
}

#[test]
fn inlay_hint_label_parts_are_bounded() {
    let mut parts = (0..MAX_LSP_INLAY_HINT_LABEL_PARTS)
        .map(|_| json!({ "value": "x" }))
        .collect::<Vec<_>>();
    parts.push(json!({ "value": "tail" }));
    let value = json!({
        "jsonrpc": "2.0",
        "id": 33,
        "result": [{
            "position": { "line": 1, "character": 0 },
            "label": parts,
            "kind": 1
        }]
    });

    let hints = parse_inlay_hints_response(&value).unwrap();

    assert_eq!(hints.len(), 1);
    assert_eq!(
        hints[0].label.chars().count(),
        MAX_LSP_INLAY_HINT_LABEL_PARTS
    );
    assert!(!hints[0].label.contains("tail"));
}

#[test]
fn parses_code_lenses_from_command_titles() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 34,
        "result": [
            {
                "range": {
                    "start": { "line": 5, "character": 4 },
                    "end": { "line": 5, "character": 12 }
                },
                "command": {
                    "title": "Run Test",
                    "command": "rust-analyzer.runSingle",
                    "arguments": [{ "kind": "test", "id": 7 }]
                }
            },
            {
                "range": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 1, "character": 8 }
                },
                "command": {
                    "title": "  References\n",
                    "command": "editor.showReferences"
                }
            },
            {
                "range": {
                    "start": { "line": 5, "character": 4 },
                    "end": { "line": 5, "character": 12 }
                },
                "command": {
                    "title": "Run Test",
                    "command": "rust-analyzer.runSingle",
                    "arguments": [{ "kind": "test", "id": 7 }]
                }
            },
            {
                "range": {
                    "start": { "line": 8, "character": 0 },
                    "end": { "line": 8, "character": 0 }
                }
            },
            {
                "range": {
                    "start": { "line": 9, "character": 2 },
                    "end": { "line": 9, "character": 2 }
                },
                "data": { "id": 42 }
            }
        ]
    });

    let lenses = parse_code_lenses_response(&value).unwrap();

    assert_eq!(lenses.len(), 3);
    assert_eq!(lenses[0].line, 2);
    assert_eq!(lenses[0].column, 1);
    assert_eq!(lenses[0].title, "References");
    assert_eq!(lenses[0].command.as_deref(), Some("editor.showReferences"));
    assert_eq!(lenses[1].line, 6);
    assert_eq!(lenses[1].column, 5);
    assert_eq!(lenses[1].title, "Run Test");
    assert_eq!(
        lenses[1].command.as_deref(),
        Some("rust-analyzer.runSingle")
    );
    assert_eq!(
        lenses[1]
            .command_arguments
            .as_deref()
            .and_then(Value::as_array)
            .and_then(|arguments| arguments.first())
            .and_then(|argument| argument.get("id")),
        Some(&json!(7))
    );
    assert_eq!(lenses[2].line, 10);
    assert_eq!(lenses[2].column, 3);
    assert_eq!(lenses[2].title, "");
    assert!(lenses[2].needs_resolve());
    assert_eq!(
        lenses[2]
            .resolve_payload
            .as_deref()
            .and_then(|payload| payload.get("data"))
            .and_then(|data| data.get("id")),
        Some(&json!(42))
    );
}

#[test]
fn code_lens_resolve_response_parses_single_resolved_lens() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 35,
        "result": {
            "range": {
                "start": { "line": 5, "character": 4 },
                "end": { "line": 5, "character": 12 }
            },
            "data": { "id": 9 },
            "command": {
                "title": "Run Test",
                "command": "rust-analyzer.runSingle",
                "arguments": [{ "kind": "test", "id": 9 }]
            }
        }
    });

    let lens = parse_code_lens_resolve_response(&value).unwrap();

    assert_eq!(lens.line, 6);
    assert_eq!(lens.column, 5);
    assert_eq!(lens.title, "Run Test");
    assert_eq!(lens.command.as_deref(), Some("rust-analyzer.runSingle"));
    assert_eq!(
        lens.command_arguments
            .as_deref()
            .and_then(Value::as_array)
            .and_then(|arguments| arguments.first())
            .and_then(|argument| argument.get("id")),
        Some(&json!(9))
    );
    assert!(!lens.needs_resolve());
}

#[test]
fn code_lens_titles_are_sanitized_and_bounded() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 34,
        "result": [{
            "range": {
                "start": { "line": 5, "character": 4 },
                "end": { "line": 5, "character": 12 }
            },
            "command": {
                "title": format!(
                    "  {}tail\n",
                    "\u{03b2}".repeat(MAX_LSP_CODE_LENS_TITLE_CHARS + 4)
                ),
                "command": "rust-analyzer.runSingle"
            }
        }]
    });

    let lenses = parse_code_lenses_response(&value).unwrap();

    assert_eq!(lenses.len(), 1);
    assert_eq!(
        lenses[0].title.matches('\u{03b2}').count(),
        MAX_LSP_CODE_LENS_TITLE_CHARS
    );
    assert!(!lenses[0].title.contains("tail"));
    assert!(!lenses[0].title.contains('\n'));
}

#[test]
fn parses_semantic_tokens_from_delta_encoded_full_response() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 35,
        "result": {
            "data": [
                1, 2, 5, 12, 3,
                0, 8, 3, 8, 0,
                2, 0, 4, 2, 4,
                0, 0, 0, 12, 0
            ]
        }
    });

    let tokens = parse_semantic_tokens_response(&value).unwrap();

    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].line, 2);
    assert_eq!(tokens[0].column, 3);
    assert_eq!(tokens[0].length, 5);
    assert_eq!(tokens[0].token_type, "function");
    assert_eq!(tokens[0].modifiers, vec!["declaration", "definition"]);
    assert_eq!(tokens[1].line, 2);
    assert_eq!(tokens[1].column, 11);
    assert_eq!(tokens[1].token_type, "variable");
    assert!(tokens[1].modifiers.is_empty());
    assert_eq!(tokens[2].line, 4);
    assert_eq!(tokens[2].column, 1);
    assert_eq!(tokens[2].token_type, "class");
    assert_eq!(tokens[2].modifiers, vec!["readonly"]);
}

#[test]
fn semantic_tokens_reject_overflowing_delta_positions() {
    let value = json!({
        "jsonrpc": "2.0",
        "id": 35,
        "result": {
            "data": [
                u64::MAX, 0, 1, 12, 0
            ]
        }
    });

    assert!(parse_semantic_tokens_response(&value).is_none());
}

#[test]
fn lsp_symbol_feature_results_are_bounded() {
    let hints = (0..MAX_LSP_INLAY_HINTS + 10)
        .map(|idx| {
            json!({
                "position": { "line": idx, "character": 0 },
                "label": format!("hint{idx}")
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        parse_inlay_hints_response(&json!({ "result": hints }))
            .unwrap()
            .len(),
        MAX_LSP_INLAY_HINTS
    );

    let lenses = (0..MAX_LSP_CODE_LENSES + 10)
        .map(|idx| {
            json!({
                "range": {
                    "start": { "line": idx, "character": 0 },
                    "end": { "line": idx, "character": 1 }
                },
                "command": {
                    "title": format!("Run {idx}"),
                    "command": "example.run"
                }
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        parse_code_lenses_response(&json!({ "result": lenses }))
            .unwrap()
            .len(),
        MAX_LSP_CODE_LENSES
    );

    let semantic_data = (0..MAX_LSP_SEMANTIC_TOKENS + 10)
        .flat_map(|_| [1_u64, 0, 1, 12, 0])
        .collect::<Vec<_>>();
    assert_eq!(
        parse_semantic_tokens_response(&json!({ "result": { "data": semantic_data } }))
            .unwrap()
            .len(),
        MAX_LSP_SEMANTIC_TOKENS
    );

    let symbols = (0..MAX_LSP_WORKSPACE_SYMBOLS + 10)
        .map(|idx| {
            json!({
                "name": format!("Symbol{idx}"),
                "kind": 12,
                "location": {
                    "uri": path_to_file_uri(Path::new(&format!("src/{idx}.rs")))
                }
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        parse_workspace_symbols_response(&json!({ "result": symbols }))
            .unwrap()
            .len(),
        MAX_LSP_WORKSPACE_SYMBOLS
    );
}

#[test]
fn parses_workspace_symbols_from_locations_and_uri_only_results() {
    let first_uri = path_to_file_uri(Path::new("src/main.rs"));
    let second_uri = path_to_file_uri(Path::new("src/lib.rs"));
    let value = json!({
        "jsonrpc": "2.0",
        "id": 27,
        "result": [
            {
                "name": "main",
                "kind": 12,
                "containerName": "crate",
                "location": {
                    "uri": first_uri,
                    "range": {
                        "start": { "line": 2, "character": 3 },
                        "end": { "line": 4, "character": 1 }
                    }
                }
            },
            {
                "name": "Handler",
                "kind": 5,
                "detail": "struct",
                "location": {
                    "uri": second_uri
                }
            },
            {
                "name": "main",
                "kind": 12,
                "containerName": "crate",
                "location": {
                    "uri": first_uri,
                    "range": {
                        "start": { "line": 2, "character": 3 },
                        "end": { "line": 4, "character": 1 }
                    }
                }
            }
        ]
    });

    let symbols = parse_workspace_symbols_response(&value).unwrap();
    assert_eq!(symbols.len(), 2);
    assert!(symbols[0].path.ends_with(Path::new("src").join("lib.rs")));
    assert_eq!(symbols[0].name, "Handler");
    assert_eq!(symbols[0].detail.as_deref(), Some("struct"));
    assert_eq!(symbols[0].line, 1);
    assert_eq!(symbols[0].column, 1);
    assert!(symbols[1].path.ends_with(Path::new("src").join("main.rs")));
    assert_eq!(symbols[1].name, "main");
    assert_eq!(symbols[1].detail.as_deref(), Some("crate"));
    assert_eq!(symbols[1].line, 3);
    assert_eq!(symbols[1].column, 4);
    assert_eq!(symbols[1].end_line, 5);
}

#[test]
fn workspace_symbol_labels_are_sanitized_and_bounded() {
    let uri = path_to_file_uri(Path::new("src/main.rs"));
    let value = json!({
        "jsonrpc": "2.0",
        "id": 27,
        "result": [{
            "name": format!(
                "{}tail",
                "\u{03b3}".repeat(MAX_LSP_WORKSPACE_SYMBOL_NAME_CHARS + 4)
            ),
            "kind": 12,
            "containerName": format!(
                "  {}\ntail",
                "\u{03b4}".repeat(MAX_LSP_WORKSPACE_SYMBOL_DETAIL_CHARS + 4)
            ),
            "location": {
                "uri": uri
            }
        }]
    });

    let symbols = parse_workspace_symbols_response(&value).unwrap();

    assert_eq!(symbols.len(), 1);
    assert_eq!(
        symbols[0].name.matches('\u{03b3}').count(),
        MAX_LSP_WORKSPACE_SYMBOL_NAME_CHARS
    );
    assert!(!symbols[0].name.contains("tail"));
    let detail = symbols[0].detail.as_deref().unwrap();
    assert_eq!(
        detail.matches('\u{03b4}').count(),
        MAX_LSP_WORKSPACE_SYMBOL_DETAIL_CHARS
    );
    assert!(!detail.contains("tail"));
    assert!(!detail.contains('\n'));
}

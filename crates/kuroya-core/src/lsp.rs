#[cfg(test)]
use crate::DiagnosticSeverity;
use crate::{Diagnostic, LanguageId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

mod completion_signature;
mod diagnostics;
mod document_sync;
mod snippet;
mod symbols;
mod text;
mod uri;
mod workspace_edit;

use diagnostics::lsp_code_action_diagnostic;
use text::{
    bounded_lsp_text, bounded_lsp_text_capacity, push_bounded_lsp_markdown_text,
    push_bounded_lsp_text, trim_lsp_text_in_place,
};
use workspace_edit::{
    WorkspaceEditResourceMode, parse_workspace_apply_edit, parse_workspace_edit_with_resource_mode,
};

pub use completion_signature::{
    LspCompletionItem, LspParameterInformation, LspSignatureHelp, LspSignatureInformation,
    parse_completion_item_resolve_response, parse_completion_response,
    parse_signature_help_response,
};
pub use diagnostics::{
    LspDiagnostic, PublishDiagnosticsParams, diagnostics_from_lsp, parse_publish_diagnostics,
};
pub use document_sync::{
    ContentChange, TextDocumentSyncKindSetting, parse_text_document_sync_kind,
    text_document_content_change_event,
};

pub use symbols::{
    LspCallHierarchyCall, LspCallHierarchyItem, LspCallHierarchyRange, LspDocumentSymbol,
    LspFoldingRange, LspReference, LspTypeHierarchyItem, LspWorkspaceSymbol,
    parse_call_hierarchy_incoming_response, parse_call_hierarchy_outgoing_response,
    parse_call_hierarchy_prepare_response, parse_document_symbols_response,
    parse_folding_ranges_response, parse_references_response,
    parse_type_hierarchy_prepare_response, parse_type_hierarchy_subtypes_response,
    parse_type_hierarchy_supertypes_response, parse_workspace_symbols_response,
};
pub use uri::{file_uri_to_path, path_to_file_uri};
pub use workspace_edit::parse_workspace_edit_response;

#[cfg(test)]
use completion_signature::{
    MAX_LSP_COMPLETION_COMMIT_CHARACTER_CHARS, MAX_LSP_COMPLETION_COMMIT_CHARACTERS,
    MAX_LSP_COMPLETION_DETAIL_CHARS, MAX_LSP_COMPLETION_DOCUMENTATION_CHARS,
    MAX_LSP_COMPLETION_FILTER_TEXT_CHARS, MAX_LSP_COMPLETION_ITEMS, MAX_LSP_COMPLETION_LABEL_CHARS,
    MAX_LSP_COMPLETION_RESOLVE_PAYLOAD_BYTES, MAX_LSP_COMPLETION_SORT_TEXT_CHARS,
    MAX_LSP_SIGNATURE_DOCUMENTATION_CHARS, MAX_LSP_SIGNATURE_LABEL_CHARS,
    MAX_LSP_SIGNATURE_PARAMETER_DOCUMENTATION_CHARS, MAX_LSP_SIGNATURE_PARAMETER_LABEL_CHARS,
};
#[cfg(test)]
use snippet::expand_lsp_completion_snippet;
#[cfg(test)]
use std::borrow::Cow;
#[cfg(test)]
use symbols::{
    MAX_LSP_CALL_HIERARCHY_DETAIL_CHARS, MAX_LSP_CALL_HIERARCHY_ITEM_PAYLOAD_BYTES,
    MAX_LSP_CALL_HIERARCHY_NAME_CHARS, MAX_LSP_DOCUMENT_SYMBOLS, MAX_LSP_REFERENCES,
    MAX_LSP_WORKSPACE_SYMBOL_DETAIL_CHARS, MAX_LSP_WORKSPACE_SYMBOL_NAME_CHARS,
    MAX_LSP_WORKSPACE_SYMBOLS,
};
#[cfg(test)]
use uri::{MAX_LSP_URI_BYTES, percent_decode_uri_path, percent_encode_uri_path};

const LSP_DIAGNOSTIC_TAG_UNNECESSARY: u8 = 1;
const LSP_DIAGNOSTIC_TAG_DEPRECATED: u8 = 2;
const MAX_LSP_DIAGNOSTICS_PER_FILE: usize = 5_000;
const MAX_LSP_DIAGNOSTIC_SOURCE_CHARS: usize = 128;
const MAX_LSP_DIAGNOSTIC_MESSAGE_CHARS: usize = 2_000;
const MAX_LSP_DOCUMENT_HIGHLIGHTS: usize = 500;
const MAX_LSP_PREPARE_RENAME_PLACEHOLDER_CHARS: usize = 256;
const MAX_LSP_INLAY_HINTS: usize = 500;
const MAX_LSP_INLAY_HINT_LABEL_PARTS: usize = 512;
const MAX_LSP_INLAY_HINT_LABEL_CHARS: usize = 10_000;
const MAX_LSP_CODE_LENSES: usize = 500;
const MAX_LSP_CODE_LENS_TITLE_CHARS: usize = 512;
const MAX_LSP_CODE_LENS_RESOLVE_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_LSP_COMMAND_ID_CHARS: usize = 512;
const MAX_LSP_COMMAND_ARGUMENTS_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_LSP_CODE_ACTIONS: usize = 100;
const MAX_LSP_CODE_ACTION_TITLE_CHARS: usize = 512;
const MAX_LSP_CODE_ACTION_KIND_CHARS: usize = 128;
const MAX_LSP_CODE_ACTION_RESOLVE_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_LSP_SEMANTIC_TOKENS: usize = 5_000;
const MAX_LSP_SEMANTIC_TOKEN_LENGTH: usize = 1_000_000;
const MAX_LSP_HOVER_CHARS: usize = 64 * 1024;
const MAX_LSP_HOVER_PARTS: usize = 64;
const MAX_LSP_MARKED_STRING_LANGUAGE_CHARS: usize = 64;
const MAX_LSP_APPLY_EDIT_LABEL_CHARS: usize = 512;
const MAX_LSP_APPLY_EDIT_FAILURE_REASON_CHARS: usize = 512;
const MAX_LSP_REQUEST_ID_STRING_CHARS: usize = 256;
const MAX_LSP_PROGRESS_TOKEN_CHARS: usize = 256;
const MAX_LSP_PROGRESS_TITLE_CHARS: usize = 256;
const MAX_LSP_PROGRESS_MESSAGE_CHARS: usize = 512;
const MAX_LSP_TEXT_EDITS: usize = 2_000;
const MAX_LSP_TEXT_EDIT_NEW_TEXT_BYTES: usize = 2 * 1024 * 1024;
const MAX_LSP_POSITION_COMPONENT: usize = i32::MAX as usize;
const MAX_SNIPPET_NESTING: usize = 48;
const MAX_SNIPPET_SOURCE_BYTES: usize = 256 * 1024;
const MAX_SNIPPET_EXPANSION_BYTES: usize = 64 * 1024;
const MAX_SNIPPET_TABSTOPS: usize = 512;
const SEMANTIC_TOKEN_TYPES: [&str; 23] = [
    "namespace",
    "type",
    "class",
    "enum",
    "interface",
    "struct",
    "typeParameter",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "event",
    "function",
    "method",
    "macro",
    "keyword",
    "modifier",
    "comment",
    "string",
    "number",
    "regexp",
    "operator",
    "decorator",
];
const SEMANTIC_TOKEN_MODIFIERS: [&str; 10] = [
    "declaration",
    "definition",
    "readonly",
    "static",
    "deprecated",
    "abstract",
    "async",
    "modification",
    "documentation",
    "defaultLibrary",
];
const CODE_ACTION_KINDS: [&str; 9] = [
    "quickfix",
    "refactor",
    "refactor.extract",
    "refactor.inline",
    "refactor.rewrite",
    "source",
    "source.addMissingImports",
    "source.organizeImports",
    "source.fixAll",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LspServerConfig {
    pub language: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub root_markers: Vec<String>,
    /// Entries missing from the settings file (older schemas) stay enabled.
    #[serde(default = "lsp_server_enabled_default")]
    pub enabled: bool,
}

fn lsp_server_enabled_default() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspEvent {
    ServerStarting(String),
    ServerReady(String),
    DiagnosticsChanged(PathBuf),
    ServerStopped(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspWireMessage {
    Request {
        id: u64,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: LspRequestId,
        result: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspRequestId {
    Number(u64),
    String(String),
}

impl LspRequestId {
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Number(id) => Some(*id),
            Self::String(_) => None,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Self::Number(id) => json!(id),
            Self::String(id) => json!(id),
        }
    }
}

impl From<u64> for LspRequestId {
    fn from(id: u64) -> Self {
        Self::Number(id)
    }
}

/// One entry of a `workspace/didChangeWatchedFiles` notification.
///
/// `kind` uses the LSP `FileChangeType` numbering: 1 = Created,
/// 2 = Changed, 3 = Deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedFileChange {
    pub uri: String,
    pub kind: u8,
}

impl WatchedFileChange {
    pub const CREATED: u8 = 1;
    pub const CHANGED: u8 = 2;
    pub const DELETED: u8 = 3;
}

impl LspWireMessage {
    pub fn initialize(id: u64, root: &Path) -> Self {
        Self::Request {
            id,
            method: "initialize".to_owned(),
            params: json!({
                "processId": std::process::id(),
                "rootUri": path_to_file_uri(root),
                "capabilities": {
                    "window": {
                        "workDoneProgress": true
                    },
                    "workspace": {
                        "applyEdit": true,
                        "symbol": {
                            "dynamicRegistration": false
                        }
                    },
                    "textDocument": {
                        "publishDiagnostics": {
                            "relatedInformation": false
                        },
                        "synchronization": {
                            "didSave": true,
                            "dynamicRegistration": false
                        },
                        "hover": {
                            "dynamicRegistration": false,
                            "contentFormat": ["markdown", "plaintext"]
                        },
                        "documentSymbol": {
                            "dynamicRegistration": false,
                            "hierarchicalDocumentSymbolSupport": true
                        },
                        "documentHighlight": {
                            "dynamicRegistration": false
                        },
                        "callHierarchy": {
                            "dynamicRegistration": false
                        },
                        "typeHierarchy": {
                            "dynamicRegistration": false
                        },
                        "foldingRange": {
                            "dynamicRegistration": false,
                            "lineFoldingOnly": false
                        },
                        "inlayHint": {
                            "dynamicRegistration": false
                        },
                        "codeLens": {
                            "dynamicRegistration": false,
                            "resolveSupport": {
                                "properties": ["command"]
                            }
                        },
                        "semanticTokens": {
                            "dynamicRegistration": false,
                            "requests": {
                                "range": false,
                                "full": {
                                    "delta": false
                                }
                            },
                            "tokenTypes": SEMANTIC_TOKEN_TYPES,
                            "tokenModifiers": SEMANTIC_TOKEN_MODIFIERS
                        },
                        "completion": {
                            "dynamicRegistration": false,
                            "completionItem": {
                                "snippetSupport": true,
                                "documentationFormat": ["markdown", "plaintext"],
                                "commitCharactersSupport": true,
                                "insertReplaceSupport": true,
                                "dataSupport": true,
                                "resolveSupport": {
                                    "properties": [
                                        "detail",
                                        "documentation",
                                        "additionalTextEdits"
                                    ]
                                }
                            },
                            "completionList": {
                                "itemDefaults": [
                                    "commitCharacters",
                                    "editRange",
                                    "insertTextFormat"
                                ]
                            }
                        },
                        "signatureHelp": {
                            "dynamicRegistration": false,
                            "signatureInformation": {
                                "documentationFormat": ["markdown", "plaintext"],
                                "parameterInformation": {
                                    "labelOffsetSupport": true
                                },
                                "activeParameterSupport": true
                            }
                        },
                        "formatting": {
                            "dynamicRegistration": false
                        },
                        "references": {
                            "dynamicRegistration": false
                        },
                        "rename": {
                            "dynamicRegistration": false,
                            "prepareSupport": true
                        },
                        "codeAction": {
                            "dynamicRegistration": false,
                            "dataSupport": true,
                            "codeActionLiteralSupport": {
                                "codeActionKind": {
                                    "valueSet": CODE_ACTION_KINDS
                                }
                            },
                            "isPreferredSupport": true,
                            "resolveSupport": {
                                "properties": ["edit"]
                            }
                        }
                    }
                }
            }),
        }
    }

    pub fn initialized() -> Self {
        Self::Notification {
            method: "initialized".to_owned(),
            params: json!({}),
        }
    }

    pub fn did_open(path: &Path, language: LanguageId, version: i32, text: &str) -> Self {
        Self::Notification {
            method: "textDocument/didOpen".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path),
                    "languageId": lsp_language_id(language),
                    "version": version,
                    "text": text
                }
            }),
        }
    }

    pub fn did_change(path: &Path, version: i32, text: &str) -> Self {
        Self::Notification {
            method: "textDocument/didChange".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path),
                    "version": version
                },
                "contentChanges": [
                    { "text": text }
                ]
            }),
        }
    }

    pub fn did_save(path: &Path) -> Self {
        Self::Notification {
            method: "textDocument/didSave".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                }
            }),
        }
    }

    pub fn did_close(path: &Path) -> Self {
        Self::Notification {
            method: "textDocument/didClose".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                }
            }),
        }
    }

    pub fn did_change_watched_files(changes: &[WatchedFileChange]) -> Self {
        Self::Notification {
            method: "workspace/didChangeWatchedFiles".to_owned(),
            params: json!({
                "changes": changes.iter().map(|change| json!({
                    "uri": change.uri,
                    "type": change.kind
                })).collect::<Vec<_>>()
            }),
        }
    }

    pub fn hover(id: u64, path: &Path, line: usize, character: usize) -> Self {
        Self::Request {
            id,
            method: "textDocument/hover".to_owned(),
            params: text_document_position_params(path, line, character),
        }
    }

    pub fn document_highlight(id: u64, path: &Path, line: usize, character: usize) -> Self {
        Self::Request {
            id,
            method: "textDocument/documentHighlight".to_owned(),
            params: text_document_position_params(path, line, character),
        }
    }

    pub fn definition(id: u64, path: &Path, line: usize, character: usize) -> Self {
        Self::Request {
            id,
            method: "textDocument/definition".to_owned(),
            params: text_document_position_params(path, line, character),
        }
    }

    pub fn prepare_call_hierarchy(id: u64, path: &Path, line: usize, character: usize) -> Self {
        Self::Request {
            id,
            method: "textDocument/prepareCallHierarchy".to_owned(),
            params: text_document_position_params(path, line, character),
        }
    }

    pub fn call_hierarchy_incoming(id: u64, item: &LspCallHierarchyItem) -> Self {
        Self::Request {
            id,
            method: "callHierarchy/incomingCalls".to_owned(),
            params: json!({
                "item": item.raw.clone()
            }),
        }
    }

    pub fn call_hierarchy_outgoing(id: u64, item: &LspCallHierarchyItem) -> Self {
        Self::Request {
            id,
            method: "callHierarchy/outgoingCalls".to_owned(),
            params: json!({
                "item": item.raw.clone()
            }),
        }
    }

    pub fn prepare_type_hierarchy(id: u64, path: &Path, line: usize, character: usize) -> Self {
        Self::Request {
            id,
            method: "textDocument/prepareTypeHierarchy".to_owned(),
            params: text_document_position_params(path, line, character),
        }
    }

    pub fn type_hierarchy_supertypes(id: u64, item: &LspTypeHierarchyItem) -> Self {
        Self::Request {
            id,
            method: "typeHierarchy/supertypes".to_owned(),
            params: json!({
                "item": item.raw.clone()
            }),
        }
    }

    pub fn type_hierarchy_subtypes(id: u64, item: &LspTypeHierarchyItem) -> Self {
        Self::Request {
            id,
            method: "typeHierarchy/subtypes".to_owned(),
            params: json!({
                "item": item.raw.clone()
            }),
        }
    }

    pub fn references(
        id: u64,
        path: &Path,
        line: usize,
        character: usize,
        include_declaration: bool,
    ) -> Self {
        let mut params = text_document_position_params(path, line, character);
        params["context"] = json!({
            "includeDeclaration": include_declaration
        });
        Self::Request {
            id,
            method: "textDocument/references".to_owned(),
            params,
        }
    }

    pub fn prepare_rename(id: u64, path: &Path, line: usize, character: usize) -> Self {
        Self::Request {
            id,
            method: "textDocument/prepareRename".to_owned(),
            params: text_document_position_params(path, line, character),
        }
    }

    pub fn rename(id: u64, path: &Path, line: usize, character: usize, new_name: &str) -> Self {
        let mut params = text_document_position_params(path, line, character);
        params["newName"] = json!(new_name);
        Self::Request {
            id,
            method: "textDocument/rename".to_owned(),
            params,
        }
    }

    pub fn document_symbols(id: u64, path: &Path) -> Self {
        Self::Request {
            id,
            method: "textDocument/documentSymbol".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                }
            }),
        }
    }

    pub fn folding_ranges(id: u64, path: &Path) -> Self {
        Self::Request {
            id,
            method: "textDocument/foldingRange".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                }
            }),
        }
    }

    pub fn inlay_hints(
        id: u64,
        path: &Path,
        start_line: usize,
        start_character: usize,
        end_line: usize,
        end_character: usize,
    ) -> Self {
        Self::Request {
            id,
            method: "textDocument/inlayHint".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                },
                "range": lsp_range_value(start_line, start_character, end_line, end_character)
            }),
        }
    }

    pub fn code_lenses(id: u64, path: &Path) -> Self {
        Self::Request {
            id,
            method: "textDocument/codeLens".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                }
            }),
        }
    }

    pub fn code_lens_resolve(id: u64, lens: &LspCodeLens) -> Option<Self> {
        Some(Self::Request {
            id,
            method: "codeLens/resolve".to_owned(),
            params: lens.resolve_payload.as_ref()?.as_ref().clone(),
        })
    }

    pub fn workspace_execute_command(
        id: u64,
        command: &str,
        arguments: Option<&Arc<Value>>,
    ) -> Option<Self> {
        let command = bounded_lsp_text(command, MAX_LSP_COMMAND_ID_CHARS)?;
        let mut params = json!({
            "command": command
        });
        if let Some(arguments) = arguments {
            params["arguments"] = arguments.as_ref().clone();
        }
        Some(Self::Request {
            id,
            method: "workspace/executeCommand".to_owned(),
            params,
        })
    }

    pub fn semantic_tokens(id: u64, path: &Path) -> Self {
        Self::Request {
            id,
            method: "textDocument/semanticTokens/full".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                }
            }),
        }
    }

    pub fn workspace_symbols(id: u64, query: &str) -> Self {
        Self::Request {
            id,
            method: "workspace/symbol".to_owned(),
            params: json!({
                "query": query
            }),
        }
    }

    pub fn completion(id: u64, path: &Path, line: usize, character: usize) -> Self {
        let mut params = text_document_position_params(path, line, character);
        params["context"] = json!({
            "triggerKind": 1
        });
        Self::Request {
            id,
            method: "textDocument/completion".to_owned(),
            params,
        }
    }

    pub fn completion_item_resolve(id: u64, item: &LspCompletionItem) -> Option<Self> {
        Some(Self::Request {
            id,
            method: "completionItem/resolve".to_owned(),
            params: item.resolve_payload.as_ref()?.as_ref().clone(),
        })
    }

    pub fn signature_help(id: u64, path: &Path, line: usize, character: usize) -> Self {
        let mut params = text_document_position_params(path, line, character);
        params["context"] = json!({
            "triggerKind": 1
        });
        Self::Request {
            id,
            method: "textDocument/signatureHelp".to_owned(),
            params,
        }
    }

    pub fn formatting(id: u64, path: &Path, tab_size: usize, insert_spaces: bool) -> Self {
        Self::Request {
            id,
            method: "textDocument/formatting".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                },
                "options": {
                    "tabSize": tab_size.max(1),
                    "insertSpaces": insert_spaces,
                    "trimTrailingWhitespace": true,
                    "insertFinalNewline": true,
                    "trimFinalNewlines": true
                }
            }),
        }
    }

    pub fn code_action(
        id: u64,
        path: &Path,
        start_line: usize,
        start_character: usize,
        end_line: usize,
        end_character: usize,
    ) -> Self {
        Self::code_action_with_diagnostics(
            id,
            path,
            start_line,
            start_character,
            end_line,
            end_character,
            &[],
        )
    }

    pub fn code_action_with_diagnostics(
        id: u64,
        path: &Path,
        start_line: usize,
        start_character: usize,
        end_line: usize,
        end_character: usize,
        diagnostics: &[Diagnostic],
    ) -> Self {
        Self::Request {
            id,
            method: "textDocument/codeAction".to_owned(),
            params: json!({
                "textDocument": {
                    "uri": path_to_file_uri(path)
                },
                "range": lsp_range_value(start_line, start_character, end_line, end_character),
                "context": {
                    "diagnostics": diagnostics
                        .iter()
                        .map(lsp_code_action_diagnostic)
                        .collect::<Vec<_>>()
                }
            }),
        }
    }

    pub fn code_action_resolve(id: u64, action: &LspCodeAction) -> Option<Self> {
        Some(Self::Request {
            id,
            method: "codeAction/resolve".to_owned(),
            params: action.resolve_payload.as_ref()?.as_ref().clone(),
        })
    }

    pub fn shutdown(id: u64) -> Self {
        Self::Request {
            id,
            method: "shutdown".to_owned(),
            params: json!(null),
        }
    }

    pub fn exit() -> Self {
        Self::Notification {
            method: "exit".to_owned(),
            params: json!(null),
        }
    }

    pub fn cancel_request(id: u64) -> Self {
        Self::Notification {
            method: "$/cancelRequest".to_owned(),
            params: json!({ "id": id }),
        }
    }

    pub fn response(id: impl Into<LspRequestId>, result: Value) -> Self {
        Self::Response {
            id: id.into(),
            result,
        }
    }

    /// Success reply for `client/registerCapability`. The empty
    /// `capabilities` result is spec-valid; it only means the client accepted
    /// the registration without extra server-side processing data.
    pub fn register_capability_response(id: impl Into<LspRequestId>) -> Self {
        Self::Response {
            id: id.into(),
            result: json!({ "capabilities": {} }),
        }
    }

    /// Success reply for `client/unregisterCapability`; same shape as the
    /// registration reply.
    pub fn unregister_capability_response(id: impl Into<LspRequestId>) -> Self {
        Self::Response {
            id: id.into(),
            result: json!({ "capabilities": {} }),
        }
    }

    pub fn apply_workspace_edit_response(
        id: impl Into<LspRequestId>,
        applied: bool,
        failure_reason: Option<&str>,
        failed_change: Option<usize>,
    ) -> Self {
        let mut result = json!({
            "applied": applied
        });
        if !applied {
            if let Some(failure_reason) = failure_reason.and_then(|reason| {
                bounded_lsp_text(reason, MAX_LSP_APPLY_EDIT_FAILURE_REASON_CHARS)
            }) {
                result["failureReason"] = json!(failure_reason);
            }
            if let Some(failed_change) = failed_change {
                result["failedChange"] = json!(failed_change);
            }
        }
        Self::Response {
            id: id.into(),
            result,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Request { id, method, params } => json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            }),
            Self::Notification { method, params } => json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params
            }),
            Self::Response { id, result } => json!({
                "jsonrpc": "2.0",
                "id": id.to_json(),
                "result": result
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspHover {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspDefinition {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

/// Valid `textDocument/prepareRename` result: the one-based range the server
/// is willing to rename plus the optional placeholder the editor should
/// prefill. Coordinates follow the parsed-range convention (one-based).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspPrepareRename {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspDocumentHighlight {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub kind: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspTextEdit {
    pub path: PathBuf,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspInlayHint {
    pub line: usize,
    pub column: usize,
    pub label: String,
    pub kind: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspCodeLens {
    pub line: usize,
    pub column: usize,
    pub title: String,
    pub command: Option<String>,
    pub command_arguments: Option<Arc<Value>>,
    pub resolve_payload: Option<Arc<Value>>,
}

impl LspCodeLens {
    pub fn needs_resolve(&self) -> bool {
        self.title.is_empty() && self.resolve_payload.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspSemanticToken {
    pub line: usize,
    pub column: usize,
    pub length: usize,
    pub token_type: String,
    pub modifiers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspCodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edits: Vec<LspTextEdit>,
    pub document_changes: Vec<LspWorkspaceDocumentChange>,
    pub resolve_payload: Option<Arc<Value>>,
}

impl LspCodeAction {
    pub fn needs_resolve(&self) -> bool {
        self.edits.is_empty() && self.document_changes.is_empty() && self.resolve_payload.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspWorkDoneProgressKind {
    Begin,
    Report,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspWorkDoneProgress {
    pub token: String,
    pub kind: LspWorkDoneProgressKind,
    pub title: Option<String>,
    pub message: Option<String>,
    pub percentage: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspWorkDoneProgressCreate {
    pub id: LspRequestId,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LspApplyWorkspaceEdit {
    pub id: LspRequestId,
    pub label: Option<String>,
    pub edits: Vec<LspTextEdit>,
    pub document_changes: Vec<LspWorkspaceDocumentChange>,
    pub document_versions: BTreeMap<PathBuf, i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspWorkspaceDocumentChange {
    TextEdit {
        path: PathBuf,
        version: Option<i32>,
        edits: Vec<LspTextEdit>,
    },
    Resource(LspWorkspaceResourceOperation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspWorkspaceResourceOperation {
    CreateFile {
        path: PathBuf,
        overwrite: bool,
        ignore_if_exists: bool,
    },
    RenameFile {
        old_path: PathBuf,
        new_path: PathBuf,
        overwrite: bool,
        ignore_if_exists: bool,
    },
    DeleteFile {
        path: PathBuf,
        recursive: bool,
        ignore_if_not_exists: bool,
    },
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct LspPosition {
    pub line: usize,
    pub character: usize,
}

/// Languages whose built-in servers start enabled. Every other built-in
/// ships disabled; users can switch servers on in the LSP settings.
const DEFAULT_ENABLED_LANGUAGES: [&str; 7] = [
    "rust",
    "python",
    "typescript",
    "javascript",
    "go",
    "c",
    "cpp",
];

pub fn default_server_configs() -> Vec<LspServerConfig> {
    let mut servers = vec![
        lsp_server_config(
            "rust",
            "rust-analyzer",
            &[],
            &["Cargo.toml", "rust-project.json", ".git"],
        ),
        lsp_server_config(
            "python",
            "pyright-langserver",
            &["--stdio"],
            &[
                "pyproject.toml",
                "setup.py",
                "requirements.txt",
                "Pipfile",
                "poetry.lock",
                "uv.lock",
                ".git",
            ],
        ),
        lsp_server_config(
            "typescript",
            "typescript-language-server",
            &["--stdio"],
            &[
                "tsconfig.json",
                "jsconfig.json",
                "package.json",
                "deno.json",
                "deno.jsonc",
                ".git",
            ],
        ),
        lsp_server_config(
            "javascript",
            "typescript-language-server",
            &["--stdio"],
            &[
                "jsconfig.json",
                "tsconfig.json",
                "package.json",
                "deno.json",
                "deno.jsonc",
                ".git",
            ],
        ),
        lsp_server_config("go", "gopls", &[], &["go.work", "go.mod", ".git"]),
        lsp_server_config(
            "java",
            "jdtls",
            &[],
            &[
                "pom.xml",
                "build.gradle",
                "build.gradle.kts",
                ".project",
                ".git",
            ],
        ),
        lsp_server_config(
            "c",
            "clangd",
            &[],
            &[
                "compile_commands.json",
                "compile_flags.txt",
                ".clangd",
                "CMakeLists.txt",
                "Makefile",
                ".git",
            ],
        ),
        lsp_server_config(
            "cpp",
            "clangd",
            &[],
            &[
                "compile_commands.json",
                "compile_flags.txt",
                ".clangd",
                "CMakeLists.txt",
                "Makefile",
                ".git",
            ],
        ),
        lsp_server_config(
            "csharp",
            "csharp-ls",
            &[],
            &[
                "global.json",
                "Directory.Build.props",
                "Directory.Build.targets",
                ".git",
            ],
        ),
        lsp_server_config(
            "php",
            "intelephense",
            &["--stdio"],
            &["composer.json", "composer.lock", ".git"],
        ),
        lsp_server_config(
            "ruby",
            "ruby-lsp",
            &[],
            &["Gemfile", ".ruby-version", ".git"],
        ),
        lsp_server_config(
            "lua",
            "lua-language-server",
            &[],
            &[".luarc.json", ".luarc.jsonc", ".git"],
        ),
        lsp_server_config(
            "dart",
            "dart",
            &["language-server", "--protocol=lsp"],
            &["pubspec.yaml", ".git"],
        ),
        lsp_server_config(
            "kotlin",
            "kotlin-language-server",
            &[],
            &[
                "settings.gradle",
                "settings.gradle.kts",
                "build.gradle.kts",
                ".git",
            ],
        ),
        lsp_server_config(
            "swift",
            "sourcekit-lsp",
            &[],
            &["Package.swift", "compile_commands.json", ".git"],
        ),
        lsp_server_config(
            "toml",
            "taplo",
            &["lsp", "stdio"],
            &["taplo.toml", "Cargo.toml", ".git"],
        ),
        lsp_server_config(
            "json",
            "vscode-json-language-server",
            &["--stdio"],
            &["package.json", "tsconfig.json", ".git"],
        ),
        lsp_server_config(
            "html",
            "vscode-html-language-server",
            &["--stdio"],
            &["package.json", ".git"],
        ),
        lsp_server_config(
            "css",
            "vscode-css-language-server",
            &["--stdio"],
            &["package.json", ".git"],
        ),
        lsp_server_config(
            "yaml",
            "yaml-language-server",
            &["--stdio"],
            &[".yamllint", "package.json", ".git"],
        ),
        lsp_server_config("xml", "lemminx", &[], &["pom.xml", "build.gradle", ".git"]),
        lsp_server_config(
            "markdown",
            "marksman",
            &["server"],
            &[".marksman.toml", ".git"],
        ),
        lsp_server_config(
            "vue",
            "vue-language-server",
            &["--stdio"],
            &["package.json", "tsconfig.json", "jsconfig.json", ".git"],
        ),
        lsp_server_config(
            "svelte",
            "svelteserver",
            &["--stdio"],
            &[
                "package.json",
                "svelte.config.js",
                "svelte.config.ts",
                ".git",
            ],
        ),
        lsp_server_config(
            "dockerfile",
            "docker-langserver",
            &["--stdio"],
            &["Dockerfile", "Containerfile", ".git"],
        ),
        lsp_server_config(
            "terraform",
            "terraform-ls",
            &["serve"],
            &[".terraform", "terraform.tf", ".git"],
        ),
        lsp_server_config("powershell", "powershell-es", &["-Stdio"], &[".git"]),
        lsp_server_config("shellscript", "bash-language-server", &["start"], &[".git"]),
        lsp_server_config("sql", "sqls", &[], &["sqls.yml", ".git"]),
    ];
    for server in &mut servers {
        server.enabled = DEFAULT_ENABLED_LANGUAGES.contains(&server.language.as_str());
    }
    servers
}

pub fn server_config_for_language(
    configs: &[LspServerConfig],
    language: LanguageId,
) -> Option<&LspServerConfig> {
    let wanted = lsp_language_id(language);
    configs.iter().find(|config| config.language == wanted)
}

fn lsp_server_config(
    language: &str,
    command: &str,
    args: &[&str],
    root_markers: &[&str],
) -> LspServerConfig {
    LspServerConfig {
        language: language.to_owned(),
        command: command.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        extensions: Vec::new(),
        enabled: true,
        root_markers: root_markers
            .iter()
            .map(|marker| (*marker).to_owned())
            .collect(),
    }
}

pub fn server_for_language(language: LanguageId) -> Option<LspServerConfig> {
    server_config_for_language(&default_server_configs(), language).cloned()
}

#[derive(Clone, Copy)]
struct ParsedLspPosition {
    line: usize,
    character: usize,
}

pub(super) fn value_as_usize(value: &Value) -> Option<usize> {
    usize::try_from(value.as_u64()?).ok()
}

pub(super) fn value_as_u8(value: &Value) -> Option<u8> {
    u8::try_from(value.as_u64()?).ok()
}

pub(super) fn one_based_lsp_position_component(value: usize) -> Option<usize> {
    if value > MAX_LSP_POSITION_COMPONENT {
        return None;
    }
    value.checked_add(1)
}

fn parse_lsp_position_bounds(value: &Value) -> Option<ParsedLspPosition> {
    let line = value_as_usize(value.get("line")?)?;
    let character = value_as_usize(value.get("character")?)?;
    one_based_lsp_position_component(line)?;
    one_based_lsp_position_component(character)?;
    Some(ParsedLspPosition { line, character })
}

fn parse_lsp_struct_position_bounds(value: LspPosition) -> Option<ParsedLspPosition> {
    one_based_lsp_position_component(value.line)?;
    one_based_lsp_position_component(value.character)?;
    Some(ParsedLspPosition {
        line: value.line,
        character: value.character,
    })
}

fn lsp_range_bounds_are_valid(start: ParsedLspPosition, end: ParsedLspPosition) -> bool {
    end.line > start.line || (end.line == start.line && end.character >= start.character)
}

fn parse_lsp_range_bounds(value: &Value) -> Option<(ParsedLspPosition, ParsedLspPosition)> {
    let start = parse_lsp_position_bounds(value.get("start")?)?;
    let end = parse_lsp_position_bounds(value.get("end")?)?;
    lsp_range_bounds_are_valid(start, end).then_some((start, end))
}

fn parse_lsp_struct_range_bounds(
    range: &LspRange,
) -> Option<(ParsedLspPosition, ParsedLspPosition)> {
    let start = parse_lsp_struct_position_bounds(range.start)?;
    let end = parse_lsp_struct_position_bounds(range.end)?;
    lsp_range_bounds_are_valid(start, end).then_some((start, end))
}

fn parse_lsp_position(value: &Value) -> Option<(usize, usize)> {
    let position = parse_lsp_position_bounds(value)?;
    Some((
        one_based_lsp_position_component(position.line)?,
        one_based_lsp_position_component(position.character)?,
    ))
}

pub fn parse_work_done_progress(value: &Value) -> Option<LspWorkDoneProgress> {
    if value.get("method")?.as_str()? != "$/progress" {
        return None;
    }
    let params = value.get("params")?;
    let token = progress_token_to_string(params.get("token")?)?;
    let value = params.get("value")?;
    let kind = match value.get("kind")?.as_str()? {
        "begin" => LspWorkDoneProgressKind::Begin,
        "report" => LspWorkDoneProgressKind::Report,
        "end" => LspWorkDoneProgressKind::End,
        _ => return None,
    };
    Some(LspWorkDoneProgress {
        token,
        kind,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .and_then(|title| bounded_lsp_text(title, MAX_LSP_PROGRESS_TITLE_CHARS)),
        message: value
            .get("message")
            .and_then(Value::as_str)
            .and_then(|message| bounded_lsp_text(message, MAX_LSP_PROGRESS_MESSAGE_CHARS)),
        percentage: value
            .get("percentage")
            .and_then(Value::as_u64)
            .map(|percentage| percentage.min(100) as u8),
    })
}

pub fn parse_work_done_progress_create(value: &Value) -> Option<LspWorkDoneProgressCreate> {
    if value.get("method")?.as_str()? != "window/workDoneProgress/create" {
        return None;
    }
    let id = parse_lsp_request_id(value.get("id")?)?;
    let token = progress_token_to_string(value.get("params")?.get("token")?)?;
    Some(LspWorkDoneProgressCreate { id, token })
}

pub fn parse_apply_workspace_edit_request(value: &Value) -> Option<LspApplyWorkspaceEdit> {
    if value.get("method")?.as_str()? != "workspace/applyEdit" {
        return None;
    }
    let id = parse_lsp_request_id(value.get("id")?)?;
    let params = value.get("params")?;
    let edit = params.get("edit")?;
    let label = params
        .get("label")
        .and_then(Value::as_str)
        .and_then(|label| bounded_lsp_text(label, MAX_LSP_APPLY_EDIT_LABEL_CHARS));
    let parsed = parse_workspace_apply_edit(edit)?;
    Some(LspApplyWorkspaceEdit {
        id,
        label,
        edits: parsed.edits,
        document_changes: parsed.document_changes,
        document_versions: parsed.document_versions,
    })
}

pub fn parse_hover_response(value: &Value) -> Option<LspHover> {
    let result = value.get("result")?;
    if result.is_null() {
        return None;
    }
    let contents = result.get("contents")?;
    let text = hover_contents_to_text(contents)?;
    let text = text.trim();
    (!text.is_empty()).then(|| LspHover {
        text: text.to_owned(),
    })
}

pub fn parse_document_highlight_response(value: &Value) -> Option<Vec<LspDocumentHighlight>> {
    let result = value.get("result")?;
    if result.is_null() {
        return Some(Vec::new());
    }

    let result = result.as_array()?;
    let mut highlights = Vec::with_capacity(result.len().min(MAX_LSP_DOCUMENT_HIGHLIGHTS));
    for item in result.iter().take(MAX_LSP_DOCUMENT_HIGHLIGHTS) {
        if let Some(highlight) = parse_document_highlight_item(item) {
            highlights.push(highlight);
        }
    }
    highlights.sort_unstable_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.column.cmp(&b.column))
            .then(a.end_line.cmp(&b.end_line))
            .then(a.end_column.cmp(&b.end_column))
            .then(a.kind.cmp(&b.kind))
    });
    highlights.dedup();
    Some(highlights)
}

pub fn parse_definition_response(value: &Value) -> Option<LspDefinition> {
    let result = value.get("result")?;
    let location = if let Some(items) = result.as_array() {
        items.first()?
    } else {
        result
    };

    if location.is_null() {
        return None;
    }

    let (path, start_line, start_column, _, _) = parse_lsp_location_range(location)?;
    Some(LspDefinition {
        path,
        line: start_line,
        column: start_column,
    })
}

/// Parses a `textDocument/prepareRename` response. A null result means the
/// server cannot rename the symbol at the requested position. The spec allows
/// three success shapes: a bare `Range`, a `{ range, placeholder? }` object,
/// and the deprecated `{ start, end }` object; all parse to the same result.
/// Ranges must be well-formed and ordered (start <= end) like every other
/// parsed LSP range.
pub fn parse_prepare_rename_response(value: &Value) -> Option<LspPrepareRename> {
    let result = value.get("result")?;
    if result.is_null() {
        return None;
    }

    let range = if result.get("start").is_some() || result.get("end").is_some() {
        result
    } else {
        result.get("range")?
    };
    let (start_line, start_column, end_line, end_column) = parse_lsp_range(range)?;
    let placeholder = result
        .get("placeholder")
        .and_then(Value::as_str)
        .and_then(|placeholder| {
            bounded_lsp_text(placeholder, MAX_LSP_PREPARE_RENAME_PLACEHOLDER_CHARS)
        });
    Some(LspPrepareRename {
        start_line,
        start_column,
        end_line,
        end_column,
        placeholder,
    })
}

pub fn parse_inlay_hints_response(value: &Value) -> Option<Vec<LspInlayHint>> {
    let result = value.get("result")?;
    if result.is_null() {
        return Some(Vec::new());
    }

    let result = result.as_array()?;
    let mut hints = Vec::with_capacity(result.len().min(MAX_LSP_INLAY_HINTS));
    for item in result.iter().take(MAX_LSP_INLAY_HINTS) {
        if let Some(hint) = parse_inlay_hint_item(item) {
            hints.push(hint);
        }
    }
    hints.sort_unstable_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.column.cmp(&b.column))
            .then(a.label.cmp(&b.label))
            .then(a.kind.cmp(&b.kind))
    });
    hints.dedup();
    Some(hints)
}

pub fn parse_code_lenses_response(value: &Value) -> Option<Vec<LspCodeLens>> {
    let result = value.get("result")?;
    if result.is_null() {
        return Some(Vec::new());
    }

    let result = result.as_array()?;
    let mut lenses = Vec::with_capacity(result.len().min(MAX_LSP_CODE_LENSES));
    for item in result.iter().take(MAX_LSP_CODE_LENSES) {
        if let Some(lens) = parse_code_lens_item(item) {
            lenses.push(lens);
        }
    }
    lenses.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.column.cmp(&b.column))
            .then(a.title.cmp(&b.title))
            .then(a.command.cmp(&b.command))
    });
    lenses.dedup();
    Some(lenses)
}

pub fn parse_code_lens_resolve_response(value: &Value) -> Option<LspCodeLens> {
    let result = value.get("result")?;
    if result.is_null() {
        return None;
    }

    parse_code_lens_item(result).filter(|lens| !lens.title.is_empty())
}

pub fn parse_semantic_tokens_response(value: &Value) -> Option<Vec<LspSemanticToken>> {
    let result = value.get("result")?;
    if result.is_null() {
        return Some(Vec::new());
    }

    let data = result.get("data")?.as_array()?;
    let mut tokens = Vec::with_capacity((data.len() / 5).min(MAX_LSP_SEMANTIC_TOKENS));
    let mut line = 0usize;
    let mut column = 0usize;
    for chunk in data.chunks_exact(5).take(MAX_LSP_SEMANTIC_TOKENS) {
        let delta_line = value_as_usize(&chunk[0])?;
        let delta_start = value_as_usize(&chunk[1])?;
        let length = value_as_usize(&chunk[2])?;
        let token_type_idx = value_as_usize(&chunk[3])?;
        let modifier_bits = chunk[4].as_u64()?;

        line = line.checked_add(delta_line)?;
        if delta_line == 0 {
            column = column.checked_add(delta_start)?;
        } else {
            column = delta_start;
        }
        if length == 0 || length > MAX_LSP_SEMANTIC_TOKEN_LENGTH {
            continue;
        }
        let Some(token_type) = SEMANTIC_TOKEN_TYPES.get(token_type_idx) else {
            continue;
        };
        tokens.push(LspSemanticToken {
            line: one_based_lsp_position_component(line)?,
            column: one_based_lsp_position_component(column)?,
            length,
            token_type: (*token_type).to_owned(),
            modifiers: semantic_token_modifiers(modifier_bits),
        });
    }

    tokens.sort_unstable_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.column.cmp(&b.column))
            .then(a.length.cmp(&b.length))
            .then(a.token_type.cmp(&b.token_type))
            .then(a.modifiers.cmp(&b.modifiers))
    });
    tokens.dedup();
    Some(tokens)
}

pub fn parse_formatting_response(value: &Value, document_path: &Path) -> Option<Vec<LspTextEdit>> {
    let result = value.get("result")?;
    if result.is_null() {
        return Some(Vec::new());
    }

    let edits = result.as_array()?;
    let mut output = Vec::with_capacity(edits.len().min(MAX_LSP_TEXT_EDITS));
    collect_lsp_text_edits(&mut output, document_path, edits)?;
    Some(output)
}

pub fn parse_code_action_response(value: &Value) -> Option<Vec<LspCodeAction>> {
    let result = value.get("result")?;
    if result.is_null() {
        return Some(Vec::new());
    }

    let result = result.as_array()?;
    let mut actions = Vec::with_capacity(result.len().min(MAX_LSP_CODE_ACTIONS));
    for item in result.iter().take(MAX_LSP_CODE_ACTIONS) {
        if let Some(action) = parse_code_action_item(item) {
            actions.push(action);
        }
    }
    Some(actions)
}

pub fn parse_code_action_resolve_response(value: &Value) -> Option<LspCodeAction> {
    let result = value.get("result")?;
    if result.is_null() {
        return None;
    }

    parse_code_action_item(result)
        .filter(|action| !action.edits.is_empty() || !action.document_changes.is_empty())
}

pub fn lsp_language_id(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Rust => "rust",
        LanguageId::Toml => "toml",
        LanguageId::Json => "json",
        LanguageId::Sql => "sql",
        LanguageId::Markdown => "markdown",
        LanguageId::PowerShell => "powershell",
        LanguageId::Python => "python",
        LanguageId::TypeScript => "typescript",
        LanguageId::JavaScript => "javascript",
        LanguageId::Css => "css",
        LanguageId::Html => "html",
        LanguageId::Yaml => "yaml",
        LanguageId::Go => "go",
        LanguageId::Java => "java",
        LanguageId::C => "c",
        LanguageId::Cpp => "cpp",
        LanguageId::CSharp => "csharp",
        LanguageId::Php => "php",
        LanguageId::Ruby => "ruby",
        LanguageId::Lua => "lua",
        LanguageId::Dart => "dart",
        LanguageId::Kotlin => "kotlin",
        LanguageId::Swift => "swift",
        LanguageId::Vue => "vue",
        LanguageId::Svelte => "svelte",
        LanguageId::Xml => "xml",
        LanguageId::Dockerfile => "dockerfile",
        LanguageId::Terraform => "terraform",
        LanguageId::Shell => "shellscript",
        LanguageId::Diff => "diff",
        LanguageId::PlainText => "plaintext",
    }
}

pub fn lsp_language_id_for_path(language: LanguageId, path: Option<&Path>) -> &'static str {
    let Some(extension) = path
        .and_then(Path::extension)
        .and_then(|extension| extension.to_str())
    else {
        return lsp_language_id(language);
    };

    match language {
        LanguageId::TypeScript if extension.eq_ignore_ascii_case("tsx") => "typescriptreact",
        LanguageId::JavaScript if extension.eq_ignore_ascii_case("jsx") => "javascriptreact",
        LanguageId::Css if extension.eq_ignore_ascii_case("scss") => "scss",
        LanguageId::Css if extension.eq_ignore_ascii_case("sass") => "sass",
        LanguageId::Css if extension.eq_ignore_ascii_case("less") => "less",
        LanguageId::Json if extension.eq_ignore_ascii_case("jsonc") => "jsonc",
        LanguageId::Markdown if extension.eq_ignore_ascii_case("mdx") => "mdx",
        _ => lsp_language_id(language),
    }
}

fn text_document_position_params(path: &Path, line: usize, character: usize) -> Value {
    json!({
        "textDocument": {
            "uri": path_to_file_uri(path)
        },
        "position": lsp_position_value(line, character)
    })
}

fn lsp_position_component(value: usize) -> usize {
    value.min(MAX_LSP_POSITION_COMPONENT)
}

fn lsp_position_value(line: usize, character: usize) -> Value {
    json!({
        "line": lsp_position_component(line),
        "character": lsp_position_component(character)
    })
}

fn lsp_range_value(
    start_line: usize,
    start_character: usize,
    end_line: usize,
    end_character: usize,
) -> Value {
    let start_line = lsp_position_component(start_line);
    let start_character = lsp_position_component(start_character);
    let mut end_line = lsp_position_component(end_line);
    let mut end_character = lsp_position_component(end_character);
    if end_line < start_line || (end_line == start_line && end_character < start_character) {
        end_line = start_line;
        end_character = start_character;
    }
    json!({
        "start": {
            "line": start_line,
            "character": start_character
        },
        "end": {
            "line": end_line,
            "character": end_character
        }
    })
}

pub(super) fn parse_lsp_range(value: &Value) -> Option<(usize, usize, usize, usize)> {
    let (start, end) = parse_lsp_range_bounds(value)?;
    Some((
        one_based_lsp_position_component(start.line)?,
        one_based_lsp_position_component(start.character)?,
        one_based_lsp_position_component(end.line)?,
        one_based_lsp_position_component(end.character)?,
    ))
}

pub(super) fn parse_lsp_location_range(
    value: &Value,
) -> Option<(PathBuf, usize, usize, usize, usize)> {
    let (uri, range) = if let Some(uri) = value.get("uri").and_then(Value::as_str) {
        (uri, value.get("range")?)
    } else {
        (
            value.get("targetUri")?.as_str()?,
            value
                .get("targetSelectionRange")
                .or_else(|| value.get("targetRange"))?,
        )
    };
    let path = file_uri_to_path(uri)?;
    let (line, column, end_line, end_column) = parse_lsp_range(range)?;
    Some((path, line, column, end_line, end_column))
}

fn parse_document_highlight_item(value: &Value) -> Option<LspDocumentHighlight> {
    let (line, column, end_line, end_column) = parse_lsp_range(value.get("range")?)?;
    Some(LspDocumentHighlight {
        line,
        column,
        end_line,
        end_column,
        kind: value.get("kind").and_then(value_as_u8),
    })
}

fn parse_inlay_hint_item(value: &Value) -> Option<LspInlayHint> {
    let (line, column) = parse_lsp_position(value.get("position")?)?;
    let label = inlay_hint_label_to_text(value.get("label")?)?;
    Some(LspInlayHint {
        line,
        column,
        label,
        kind: value.get("kind").and_then(value_as_u8),
    })
}

fn inlay_hint_label_to_text(value: &Value) -> Option<String> {
    if let Some(label) = value.as_str() {
        return bounded_lsp_text(label, MAX_LSP_INLAY_HINT_LABEL_CHARS);
    }

    let mut text = String::new();
    let mut chars = 0;
    for part in value
        .as_array()?
        .iter()
        .take(MAX_LSP_INLAY_HINT_LABEL_PARTS)
        .filter_map(|part| part.get("value").and_then(Value::as_str))
    {
        push_bounded_lsp_text(&mut text, &mut chars, part, MAX_LSP_INLAY_HINT_LABEL_CHARS);
        if chars >= MAX_LSP_INLAY_HINT_LABEL_CHARS {
            break;
        }
    }
    trim_lsp_text_in_place(text)
}

fn parse_code_lens_item(value: &Value) -> Option<LspCodeLens> {
    let (line, column, _, _) = parse_lsp_range(value.get("range")?)?;
    let command = value.get("command");
    let title = command
        .and_then(|command| command.get("title"))
        .and_then(Value::as_str)
        .and_then(|title| bounded_lsp_text(title, MAX_LSP_CODE_LENS_TITLE_CHARS))
        .unwrap_or_default();
    let resolve_payload = if title.is_empty() && value.get("data").is_some() {
        bounded_lsp_value(value, MAX_LSP_CODE_LENS_RESOLVE_PAYLOAD_BYTES)
    } else {
        None
    };
    if title.is_empty() && resolve_payload.is_none() {
        return None;
    }
    Some(LspCodeLens {
        line,
        column,
        title,
        command: command
            .and_then(|command| command.get("command"))
            .and_then(Value::as_str)
            .and_then(|command| bounded_lsp_text(command, MAX_LSP_COMMAND_ID_CHARS))
            .filter(|command| !command.is_empty()),
        command_arguments: command.and_then(code_lens_command_arguments),
        resolve_payload,
    })
}

fn code_lens_command_arguments(command: &Value) -> Option<Arc<Value>> {
    let arguments = command.get("arguments")?;
    arguments.as_array()?;
    bounded_lsp_value(arguments, MAX_LSP_COMMAND_ARGUMENTS_PAYLOAD_BYTES)
}

fn semantic_token_modifiers(bits: u64) -> Vec<String> {
    SEMANTIC_TOKEN_MODIFIERS
        .iter()
        .enumerate()
        .filter(|(idx, _)| bits & (1_u64 << idx) != 0)
        .map(|(_, modifier)| (*modifier).to_owned())
        .collect()
}

fn hover_contents_to_text(value: &Value) -> Option<String> {
    hover_contents_to_bounded_text(value, MAX_LSP_HOVER_CHARS)
}

fn hover_contents_to_bounded_text(value: &Value, max_chars: usize) -> Option<String> {
    let mut text = String::with_capacity(bounded_lsp_text_capacity("", max_chars));
    let mut chars = 0;
    append_hover_contents_to_text(value, &mut text, &mut chars, max_chars)?;
    trim_lsp_text_in_place(text)
}

fn append_hover_contents_to_text(
    value: &Value,
    output: &mut String,
    chars: &mut usize,
    max_chars: usize,
) -> Option<()> {
    if let Some(text) = value.as_str() {
        push_bounded_lsp_markdown_text(output, chars, text, max_chars);
        return Some(());
    }

    if let (Some(language), Some(value)) = (
        value.get("language").and_then(Value::as_str),
        value.get("value").and_then(Value::as_str),
    ) {
        if let Some(language) = bounded_lsp_text(language, MAX_LSP_MARKED_STRING_LANGUAGE_CHARS) {
            push_bounded_lsp_markdown_text(output, chars, "```", max_chars);
            push_bounded_lsp_markdown_text(output, chars, &language, max_chars);
            push_bounded_lsp_markdown_text(output, chars, "\n", max_chars);
            push_bounded_lsp_markdown_text(output, chars, value, max_chars);
            push_bounded_lsp_markdown_text(output, chars, "\n```", max_chars);
        } else {
            push_bounded_lsp_markdown_text(output, chars, value, max_chars);
        }
        return Some(());
    }

    if let Some(value) = value.get("value").and_then(Value::as_str) {
        push_bounded_lsp_markdown_text(output, chars, value, max_chars);
        return Some(());
    }

    if let Some(items) = value.as_array() {
        let mut appended = false;
        for item in items.iter().take(MAX_LSP_HOVER_PARTS) {
            let Some(item_text) =
                hover_contents_to_bounded_text(item, max_chars.saturating_sub(*chars))
            else {
                continue;
            };
            if item_text.trim().is_empty() {
                continue;
            }
            if appended {
                push_bounded_lsp_markdown_text(output, chars, "\n\n", max_chars);
            }
            push_bounded_lsp_markdown_text(output, chars, &item_text, max_chars);
            appended = true;
            if *chars >= max_chars {
                break;
            }
        }
        return appended.then_some(());
    }

    None
}

fn collect_lsp_text_edits(
    output: &mut Vec<LspTextEdit>,
    path: &Path,
    edits: &[Value],
) -> Option<()> {
    output.reserve(
        edits
            .len()
            .min(MAX_LSP_TEXT_EDITS.saturating_sub(output.len())),
    );
    for edit in edits {
        if output.len() >= MAX_LSP_TEXT_EDITS {
            return None;
        }
        let (start_line, start_column, end_line, end_column) = parse_lsp_range(edit.get("range")?)?;
        let new_text = edit.get("newText")?.as_str()?;
        if new_text.len() > MAX_LSP_TEXT_EDIT_NEW_TEXT_BYTES {
            return None;
        }
        output.push(LspTextEdit {
            path: path.to_path_buf(),
            start_line,
            start_column,
            end_line,
            end_column,
            new_text: new_text.to_owned(),
        });
    }

    Some(())
}

fn parse_code_action_item(value: &Value) -> Option<LspCodeAction> {
    if value.get("disabled").is_some() {
        return None;
    }
    let title = bounded_lsp_text(
        value.get("title")?.as_str()?,
        MAX_LSP_CODE_ACTION_TITLE_CHARS,
    )?;
    let (edits, document_changes) = match value.get("edit") {
        Some(edit) => {
            let parsed =
                parse_workspace_edit_with_resource_mode(edit, WorkspaceEditResourceMode::Preserve)?;
            (parsed.edits, parsed.document_changes)
        }
        None => (Vec::new(), Vec::new()),
    };
    let resolve_payload =
        if edits.is_empty() && document_changes.is_empty() && value.get("data").is_some() {
            bounded_lsp_value(value, MAX_LSP_CODE_ACTION_RESOLVE_PAYLOAD_BYTES)
        } else {
            None
        };
    if edits.is_empty() && document_changes.is_empty() && resolve_payload.is_none() {
        return None;
    }
    Some(LspCodeAction {
        title,
        kind: value
            .get("kind")
            .and_then(Value::as_str)
            .and_then(|kind| bounded_lsp_text(kind, MAX_LSP_CODE_ACTION_KIND_CHARS)),
        edits,
        document_changes,
        resolve_payload,
    })
}

pub(super) fn bounded_lsp_value(value: &Value, max_bytes: usize) -> Option<Arc<Value>> {
    let mut writer = LspValueSizeWriter::new(max_bytes);
    serde_json::to_writer(&mut writer, value).ok()?;
    Some(Arc::new(value.clone()))
}

struct LspValueSizeWriter {
    bytes: usize,
    max_bytes: usize,
}

impl LspValueSizeWriter {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: 0,
            max_bytes,
        }
    }
}

impl Write for LspValueSizeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buf.len())
            .ok_or_else(|| io::Error::other("serialized LSP value size overflowed"))?;
        if self.bytes > self.max_bytes {
            return Err(io::Error::other("serialized LSP value exceeds byte cap"));
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn progress_token_to_string(value: &Value) -> Option<String> {
    if let Some(token) = value.as_str() {
        return bounded_lsp_text(token, MAX_LSP_PROGRESS_TOKEN_CHARS);
    }
    if let Some(token) = value.as_i64() {
        return Some(token.to_string());
    }
    value.as_u64().map(|token| token.to_string())
}

pub fn parse_lsp_request_id(value: &Value) -> Option<LspRequestId> {
    if let Some(id) = value.as_u64() {
        return Some(LspRequestId::Number(id));
    }
    let id = value.as_str()?;
    (id.chars().take(MAX_LSP_REQUEST_ID_STRING_CHARS + 1).count()
        <= MAX_LSP_REQUEST_ID_STRING_CHARS)
        .then(|| LspRequestId::String(id.to_owned()))
}

#[cfg(test)]
mod tests;

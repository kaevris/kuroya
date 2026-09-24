use super::super::super::response_error;
use crate::ui_event_channel::Sender;
use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
use kuroya_core::{BufferId, parse_semantic_tokens_response};
use serde_json::Value;
use std::path::PathBuf;

const INVALID_SEMANTIC_TOKENS_RESPONSE: &str = "invalid textDocument/semanticTokens/full response";
const MAX_VALIDATED_SEMANTIC_TOKEN_CHUNKS: usize = 5_000;
const MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT: usize = i32::MAX as usize;
const MAX_SEMANTIC_TOKEN_LENGTH: usize = 1_000_000;
const SEMANTIC_TOKEN_TYPE_COUNT: usize = 23;

pub(super) fn send_semantic_tokens_result(
    id: BufferId,
    path: PathBuf,
    version: u64,
    value: &Value,
    ui_tx: &Sender<UiEvent>,
) {
    let mut error = response_error(value);
    let tokens = if error.is_none() {
        match parse_semantic_tokens_success_response(value) {
            Some(tokens) => Some(tokens),
            None => {
                error = Some(INVALID_SEMANTIC_TOKENS_RESPONSE.to_owned());
                None
            }
        }
    } else {
        None
    };
    let _ = crate::lsp_client::response::emit_lsp_response_event(
        ui_tx,
        LspUiEvent::SemanticTokensResult {
            id,
            path,
            version,
            tokens,
            error,
        },
    );
}

fn parse_semantic_tokens_success_response(
    value: &Value,
) -> Option<Vec<kuroya_core::LspSemanticToken>> {
    let result = value.get("result")?;
    if result.is_null() {
        return Some(Vec::new());
    }

    let data = result.get("data")?.as_array()?;
    if data.is_empty() {
        return Some(Vec::new());
    }
    if data.len() % 5 != 0 {
        return None;
    }
    if !is_valid_semantic_token_data(data) {
        return None;
    }

    parse_semantic_tokens_response(value)
}

fn is_valid_semantic_token_data(data: &[Value]) -> bool {
    let mut line = 0usize;
    let mut column = 0usize;

    let (chunks, _) = data.as_chunks::<5>();
    for chunk in chunks.iter().take(MAX_VALIDATED_SEMANTIC_TOKEN_CHUNKS) {
        let Some(delta_line) = lsp_u64_to_usize(&chunk[0]) else {
            return false;
        };
        let Some(delta_start) = lsp_u64_to_usize(&chunk[1]) else {
            return false;
        };
        let Some(length) = lsp_u64_to_usize(&chunk[2]) else {
            return false;
        };
        if length == 0 || length > MAX_SEMANTIC_TOKEN_LENGTH {
            return false;
        }
        let Some(token_type_idx) = lsp_u64_to_usize(&chunk[3]) else {
            return false;
        };
        if token_type_idx >= SEMANTIC_TOKEN_TYPE_COUNT || chunk[4].as_u64().is_none() {
            return false;
        }

        let Some(next_line) = line.checked_add(delta_line) else {
            return false;
        };
        let next_column = if delta_line == 0 {
            column.checked_add(delta_start)
        } else {
            Some(delta_start)
        };
        let Some(next_column) = next_column else {
            return false;
        };
        if !zero_based_coordinate_fits_valid_position_component(next_line)
            || !zero_based_coordinate_fits_valid_position_component(next_column)
        {
            return false;
        }
        line = next_line;
        column = next_column;
    }

    true
}

fn lsp_u64_to_usize(value: &Value) -> Option<usize> {
    usize::try_from(value.as_u64()?).ok()
}

fn zero_based_coordinate_fits_valid_position_component(value: usize) -> bool {
    value <= MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT
}

#[cfg(test)]
mod tests {
    use super::{
        INVALID_SEMANTIC_TOKENS_RESPONSE, MAX_SEMANTIC_TOKEN_LENGTH,
        MAX_VALIDATED_SEMANTIC_TOKEN_CHUNKS, MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT,
        SEMANTIC_TOKEN_TYPE_COUNT, send_semantic_tokens_result,
    };
    use crate::{lsp_ui_events::LspUiEvent, ui_events::UiEvent};
    use serde_json::{Value, json};
    use std::path::PathBuf;

    #[test]
    fn semantic_tokens_result_reports_invalid_success_payload() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({ "result": { "data": [0, 0, 1, 12] } }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult {
                id,
                path,
                version,
                tokens,
                error,
            }) => {
                assert_eq!(id, 7);
                assert_eq!(path, PathBuf::from("src/main.rs"));
                assert_eq!(version, 11);
                assert_eq!(tokens, None);
                assert_eq!(error.as_deref(), Some(INVALID_SEMANTIC_TOKENS_RESPONSE));
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_reports_non_numeric_data_item() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({ "result": { "data": [0, 0, "wide", 12, 0] } }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(tokens, None);
                assert_eq!(error.as_deref(), Some(INVALID_SEMANTIC_TOKENS_RESPONSE));
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_reports_coordinate_that_cannot_become_one_based_position() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let overflowing_component = (MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT as u64) + 1;
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({ "result": { "data": [overflowing_component, 0, 1, 12, 0] } }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(tokens, None);
                assert_eq!(error.as_deref(), Some(INVALID_SEMANTIC_TOKENS_RESPONSE));
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_reports_column_above_position_component_bound() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let overflowing_component = (MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT as u64) + 1;
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({ "result": { "data": [1, overflowing_component, 1, 12, 0] } }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(tokens, None);
                assert_eq!(error.as_deref(), Some(INVALID_SEMANTIC_TOKENS_RESPONSE));
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_accepts_largest_valid_position_component() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({
                "result": {
                    "data": [
                        MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT,
                        MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT,
                        1,
                        12,
                        0
                    ]
                }
            }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(error, None);
                let tokens = tokens.expect("parsed semantic tokens");
                assert_eq!(tokens.len(), 1);
                assert_eq!(
                    tokens[0].line,
                    MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT + 1
                );
                assert_eq!(
                    tokens[0].column,
                    MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT + 1
                );
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_reports_zero_length_token() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({ "result": { "data": [0, 0, 0, 12, 0] } }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(tokens, None);
                assert_eq!(error.as_deref(), Some(INVALID_SEMANTIC_TOKENS_RESPONSE));
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_reports_oversized_token_length() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({ "result": { "data": [0, 0, MAX_SEMANTIC_TOKEN_LENGTH + 1, 12, 0] } }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(tokens, None);
                assert_eq!(error.as_deref(), Some(INVALID_SEMANTIC_TOKENS_RESPONSE));
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_reports_unknown_token_type_index() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({ "result": { "data": [0, 0, 1, SEMANTIC_TOKEN_TYPE_COUNT, 0] } }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(tokens, None);
                assert_eq!(error.as_deref(), Some(INVALID_SEMANTIC_TOKENS_RESPONSE));
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_reports_same_line_delta_start_overflow() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({
                "result": {
                    "data": [
                        0, MAX_VALIDATED_SEMANTIC_TOKEN_POSITION_COMPONENT, 1, 12, 0,
                        0, 1, 1, 12, 0
                    ]
                }
            }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(tokens, None);
                assert_eq!(error.as_deref(), Some(INVALID_SEMANTIC_TOKENS_RESPONSE));
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_only_validates_the_bounded_result_prefix() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let mut data = Vec::<Value>::with_capacity(MAX_VALIDATED_SEMANTIC_TOKEN_CHUNKS * 5 + 5);
        for _ in 0..MAX_VALIDATED_SEMANTIC_TOKEN_CHUNKS {
            data.extend([1.into(), 0.into(), 1.into(), 12.into(), 0.into()]);
        }
        data.extend([
            (usize::MAX as u64).into(),
            0.into(),
            1.into(),
            12.into(),
            0.into(),
        ]);

        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({ "result": { "data": data } }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult { tokens, error, .. }) => {
                assert_eq!(error, None);
                assert_eq!(
                    tokens.expect("parsed semantic tokens").len(),
                    MAX_VALIDATED_SEMANTIC_TOKEN_CHUNKS
                );
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_result_sends_parsed_tokens_event() {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        send_semantic_tokens_result(
            7,
            PathBuf::from("src/main.rs"),
            11,
            &json!({
                "result": {
                    "data": [
                        1, 2, 5, 12, 3,
                        0, 8, 3, 8, 0
                    ]
                }
            }),
            &tx,
        );

        match rx.recv().expect("semantic tokens result event") {
            UiEvent::Lsp(LspUiEvent::SemanticTokensResult {
                id,
                path,
                version,
                tokens,
                error,
            }) => {
                assert_eq!(id, 7);
                assert_eq!(path, PathBuf::from("src/main.rs"));
                assert_eq!(version, 11);
                let tokens = tokens.expect("parsed semantic tokens");
                assert_eq!(tokens.len(), 2);
                assert_eq!(tokens[0].line, 2);
                assert_eq!(tokens[0].column, 3);
                assert_eq!(tokens[0].token_type, "function");
                assert_eq!(tokens[1].line, 2);
                assert_eq!(tokens[1].column, 11);
                assert_eq!(tokens[1].token_type, "variable");
                assert_eq!(error, None);
            }
            other => panic!("expected semantic tokens result event, got {other:?}"),
        }
    }
}

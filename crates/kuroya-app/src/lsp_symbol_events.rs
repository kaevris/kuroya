use crate::KuroyaApp;
use crate::lsp_symbol_events::{
    code_lenses::{
        handle_code_lens_command_result, handle_code_lens_resolve_result, handle_code_lenses_result,
    },
    document::handle_document_symbols_result,
    folding::handle_folding_ranges_result,
    inlay_hints::handle_inlay_hints_result,
    semantic_tokens::handle_semantic_tokens_result,
    workspace::handle_workspace_symbols_result,
};
use crate::lsp_ui_events::LspUiEvent;
use crate::workspace_state::paths_match_lexically;
use kuroya_core::{BufferId, TextBuffer};
use std::path::Path;

mod code_lenses;
mod document;
mod folding;
mod inlay_hints;
mod position;
mod semantic_tokens;
mod workspace;

impl KuroyaApp {
    pub(crate) fn handle_lsp_symbol_event(&mut self, event: LspUiEvent) {
        match event {
            LspUiEvent::DocumentSymbolsResult {
                id,
                path,
                version,
                symbols,
                error,
            } => {
                handle_document_symbols_result(self, id, path, version, symbols, error);
            }
            LspUiEvent::FoldingRangesResult {
                id,
                path,
                version,
                ranges,
                error,
            } => {
                handle_folding_ranges_result(self, id, path, version, ranges, error);
            }
            LspUiEvent::InlayHintsResult {
                id,
                path,
                version,
                hints,
                error,
            } => {
                handle_inlay_hints_result(self, id, path, version, hints, error);
            }
            LspUiEvent::CodeLensesResult {
                id,
                path,
                version,
                lenses,
                error,
            } => {
                handle_code_lenses_result(self, id, path, version, lenses, error);
            }
            LspUiEvent::CodeLensResolveResult {
                id,
                path,
                version,
                lens,
                error,
            } => {
                handle_code_lens_resolve_result(self, id, path, version, lens, error);
            }
            LspUiEvent::CodeLensCommandResult {
                id,
                path,
                version,
                title,
                command,
                error,
            } => {
                handle_code_lens_command_result(self, id, path, version, title, command, error);
            }
            LspUiEvent::SemanticTokensResult {
                id,
                path,
                version,
                tokens,
                error,
            } => {
                handle_semantic_tokens_result(self, id, path, version, tokens, error);
            }
            LspUiEvent::WorkspaceSymbolsResult {
                id,
                path,
                query,
                symbols,
                error,
            } if workspace_symbol_request_source_matches(&self.buffers, id, &path) => {
                handle_workspace_symbols_result(self, path, query, symbols, error);
            }
            _ => {}
        }
    }
}

fn workspace_symbol_request_source_matches(
    buffers: &[TextBuffer],
    id: BufferId,
    path: &Path,
) -> bool {
    buffers.iter().any(|buffer| {
        buffer.id() == id
            && buffer
                .path()
                .is_some_and(|buffer_path| paths_match_lexically(buffer_path, path))
    })
}

#[cfg(test)]
mod tests {
    use super::workspace_symbol_request_source_matches;
    use kuroya_core::TextBuffer;
    use std::path::{Path, PathBuf};

    #[test]
    fn workspace_symbol_request_source_matches_live_buffer_id_and_path() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/./main.rs");
        let other_path = Path::new("workspace/src/lib.rs");
        let buffers = [TextBuffer::from_text(
            7,
            Some(path.clone()),
            "fn main() {}\n".to_owned(),
        )];

        assert!(workspace_symbol_request_source_matches(
            &buffers,
            7,
            &equivalent_path
        ));
        assert!(!workspace_symbol_request_source_matches(
            &buffers,
            8,
            &equivalent_path
        ));
        assert!(!workspace_symbol_request_source_matches(
            &buffers, 7, other_path
        ));
        assert!(!workspace_symbol_request_source_matches(&[], 7, &path));
    }
}

use crate::lsp_client::pending::PendingLspRequests;
mod code_lenses;
mod document;
mod folding;
mod inlay_hints;
mod semantic_tokens;
mod workspace;

use crate::lsp_client::commands::LspClientCommand;
use tokio::process::ChildStdin;

pub(super) async fn handle_symbol_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    match command {
        LspClientCommand::DocumentSymbols { id, path, version } => {
            document::dispatch_document_symbols_request(
                id,
                path,
                version,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::FoldingRanges { id, path, version } => {
            folding::dispatch_folding_ranges_request(
                id,
                path,
                version,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::InlayHints {
            id,
            path,
            version,
            end_line,
            end_character,
        } => {
            inlay_hints::dispatch_inlay_hints_request(
                id,
                path,
                version,
                end_line,
                end_character,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::CodeLenses { id, path, version } => {
            code_lenses::dispatch_code_lenses_request(
                id,
                path,
                version,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::ResolveCodeLens {
            id,
            path,
            version,
            lens,
        } => {
            code_lenses::dispatch_code_lens_resolve_request(
                id,
                path,
                version,
                lens,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::ExecuteCommand {
            id,
            path,
            version,
            title,
            command,
            arguments,
        } => {
            code_lenses::dispatch_execute_command_request(
                id,
                path,
                version,
                title,
                command,
                arguments,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::SemanticTokens { id, path, version } => {
            semantic_tokens::dispatch_semantic_tokens_request(
                id,
                path,
                version,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        LspClientCommand::WorkspaceSymbols { id, path, query } => {
            workspace::dispatch_workspace_symbols_request(
                id,
                path,
                query,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        _ => true,
    }
}

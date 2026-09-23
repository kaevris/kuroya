use crate::{
    KuroyaApp,
    lsp_ui_events::LspUiEvent,
    workspace_state::{active_buffer_lsp_position_matches, lsp_event_path_is_current},
};
use kuroya_core::BufferId;
use std::path::Path;

mod call_hierarchy;
mod definition;
mod info_results;
mod type_hierarchy;

impl KuroyaApp {
    pub(crate) fn handle_lsp_navigation_event(&mut self, event: LspUiEvent) {
        match event {
            LspUiEvent::HoverResult {
                id,
                path,
                version,
                line,
                column,
                contents,
            } => {
                self.handle_lsp_hover_result(id, path, version, line, column, contents);
            }
            LspUiEvent::DocumentHighlightsResult {
                id,
                path,
                version,
                line,
                column,
                highlights,
                error,
            } => {
                self.handle_lsp_document_highlights_result(
                    id, path, version, line, column, highlights, error,
                );
            }
            LspUiEvent::DefinitionResult {
                id,
                origin_path,
                version,
                origin_line,
                origin_column,
                definition,
                error,
            } => {
                self.handle_lsp_definition_result(
                    id,
                    origin_path,
                    version,
                    origin_line,
                    origin_column,
                    definition,
                    error,
                );
            }
            LspUiEvent::CallHierarchyPrepared {
                id,
                path,
                version,
                line,
                column,
                items,
                error,
            } => {
                self.handle_lsp_call_hierarchy_prepared(
                    id, path, version, line, column, items, error,
                );
            }
            LspUiEvent::CallHierarchyIncomingResult {
                id,
                path,
                version,
                item,
                calls,
                error,
            } => {
                self.handle_lsp_call_hierarchy_incoming(id, path, version, item, calls, error);
            }
            LspUiEvent::CallHierarchyOutgoingResult {
                id,
                path,
                version,
                item,
                calls,
                error,
            } => {
                self.handle_lsp_call_hierarchy_outgoing(id, path, version, item, calls, error);
            }
            LspUiEvent::TypeHierarchyPrepared {
                id,
                path,
                version,
                line,
                column,
                items,
                error,
            } => {
                self.handle_lsp_type_hierarchy_prepared(
                    id, path, version, line, column, items, error,
                );
            }
            LspUiEvent::TypeHierarchySupertypesResult {
                id,
                path,
                version,
                item,
                supertypes,
                error,
            } => {
                self.handle_lsp_type_hierarchy_supertypes(
                    id, path, version, item, supertypes, error,
                );
            }
            LspUiEvent::TypeHierarchySubtypesResult {
                id,
                path,
                version,
                item,
                subtypes,
                error,
            } => {
                self.handle_lsp_type_hierarchy_subtypes(id, path, version, item, subtypes, error);
            }
            LspUiEvent::ReferencesResult {
                id,
                path,
                version,
                line,
                column,
                references,
                error,
            } => {
                self.handle_lsp_references_result(
                    id, path, version, line, column, references, error,
                );
            }
            LspUiEvent::PrepareRenameResult {
                id,
                path,
                version,
                line,
                column,
                range,
                error,
            } => {
                self.handle_lsp_prepare_rename_result(
                    id, path, version, line, column, range, error,
                );
            }
            LspUiEvent::RenameResult {
                id,
                origin_path,
                version,
                origin_line,
                origin_column,
                new_name,
                edits,
                error,
            } => {
                self.handle_lsp_rename_result(
                    id,
                    origin_path,
                    version,
                    origin_line,
                    origin_column,
                    new_name,
                    edits,
                    error,
                );
            }
            _ => {}
        }
    }
}

fn active_lsp_navigation_response_matches(
    app: &KuroyaApp,
    id: BufferId,
    path: &Path,
    version: u64,
    line: usize,
    one_based_column: usize,
) -> bool {
    app.active == Some(id)
        && lsp_event_path_is_current(&app.workspace.root, path)
        && active_buffer_lsp_position_matches(
            app.active_buffer(),
            path,
            version,
            line,
            one_based_column,
        )
}

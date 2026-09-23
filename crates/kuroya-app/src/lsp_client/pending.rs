#[cfg(test)]
use super::commands::LspClientCommand;
use crate::lsp_completion_resolve::CompletionResolveIntent;
use kuroya_core::{BufferId, LspCallHierarchyItem, LspCompletionItem, LspTypeHierarchyItem};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) const MAX_PENDING_LSP_REQUESTS: usize = 512;
const MAX_PENDING_LSP_FORMATTING_REQUESTS: usize = 128;
pub(super) const MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS: usize = 512;
pub(super) const MAX_LSP_OUTBOUND_JSON_PAYLOAD_BYTES: usize = 64 * 1024;

pub(super) const LSP_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Default)]
pub(super) struct PendingLspRequests {
    requests: HashMap<u64, PendingLspRequest>,
    deadlines: HashMap<u64, tokio::time::Instant>,
}

impl PendingLspRequests {
    pub(super) fn insert(&mut self, request_id: u64, pending: PendingLspRequest) {
        self.deadlines.insert(
            request_id,
            tokio::time::Instant::now() + LSP_REQUEST_TIMEOUT,
        );
        self.requests.insert(request_id, pending);
    }

    pub(super) fn remove(&mut self, request_id: &u64) -> Option<PendingLspRequest> {
        self.deadlines.remove(request_id);
        self.requests.remove(request_id)
    }

    #[cfg(test)]
    pub(super) fn get(&self, request_id: &u64) -> Option<&PendingLspRequest> {
        self.requests.get(request_id)
    }

    pub(super) fn contains_key(&self, request_id: &u64) -> bool {
        self.requests.contains_key(request_id)
    }

    pub(super) fn len(&self) -> usize {
        self.requests.len()
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (u64, &PendingLspRequest)> {
        self.requests
            .iter()
            .map(|(request_id, pending)| (*request_id, pending))
    }

    pub(super) fn drain(
        &mut self,
    ) -> std::collections::hash_map::Drain<'_, u64, PendingLspRequest> {
        self.deadlines.clear();
        self.requests.drain()
    }

    pub(super) fn earliest_deadline(&self) -> Option<tokio::time::Instant> {
        self.deadlines.values().copied().min()
    }

    pub(super) fn take_expired(
        &mut self,
        now: tokio::time::Instant,
    ) -> Vec<(u64, PendingLspRequest)> {
        let mut expired: Vec<u64> = self
            .deadlines
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(request_id, _)| *request_id)
            .collect();
        expired.sort_unstable();
        expired
            .into_iter()
            .filter_map(|request_id| {
                self.remove(&request_id)
                    .map(|pending| (request_id, pending))
            })
            .collect()
    }
}

pub(super) fn lsp_request_target_is_valid(id: BufferId, path: &Path) -> bool {
    id != 0 && !path.as_os_str().is_empty()
}

pub(super) fn bounded_lsp_outbound_text(value: String, max_chars: usize) -> Option<String> {
    (value.chars().take(max_chars.saturating_add(1)).count() <= max_chars).then_some(value)
}

pub(super) fn lsp_json_payload_is_bounded(value: &Value, max_bytes: usize) -> bool {
    let mut counter = CountingWriter::new(max_bytes);
    serde_json::to_writer(&mut counter, value).is_ok()
}

struct CountingWriter {
    bytes: usize,
    max_bytes: usize,
}

impl CountingWriter {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: 0,
            max_bytes,
        }
    }
}

impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let next = self.bytes.saturating_add(buf.len());
        if next > self.max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "lsp json payload too large",
            ));
        }
        self.bytes = next;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(super) enum PendingLspRequest {
    Hover {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    DocumentHighlights {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    Definition {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    PrepareCallHierarchy {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    CallHierarchyIncoming {
        id: BufferId,
        path: PathBuf,
        version: u64,
        item: LspCallHierarchyItem,
    },
    CallHierarchyOutgoing {
        id: BufferId,
        path: PathBuf,
        version: u64,
        item: LspCallHierarchyItem,
    },
    PrepareTypeHierarchy {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    TypeHierarchySupertypes {
        id: BufferId,
        path: PathBuf,
        version: u64,
        item: LspTypeHierarchyItem,
    },
    TypeHierarchySubtypes {
        id: BufferId,
        path: PathBuf,
        version: u64,
        item: LspTypeHierarchyItem,
    },
    References {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    PrepareRename {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    Rename {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
        new_name: String,
    },
    DocumentSymbols {
        id: BufferId,
        path: PathBuf,
        version: u64,
    },
    FoldingRanges {
        id: BufferId,
        path: PathBuf,
        version: u64,
    },
    InlayHints {
        id: BufferId,
        path: PathBuf,
        version: u64,
        end_line: usize,
        end_character: usize,
    },
    CodeLenses {
        id: BufferId,
        path: PathBuf,
        version: u64,
    },
    ResolveCodeLens {
        id: BufferId,
        path: PathBuf,
        version: u64,
    },
    ExecuteCommand {
        id: BufferId,
        path: PathBuf,
        version: u64,
        title: String,
        command: String,
    },
    SemanticTokens {
        id: BufferId,
        path: PathBuf,
        version: u64,
    },
    WorkspaceSymbols {
        id: BufferId,
        path: PathBuf,
        query: String,
    },
    Completion {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    ResolveCompletionItem {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
        item: Box<LspCompletionItem>,
        intent: CompletionResolveIntent,
    },
    SignatureHelp {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    Formatting {
        request_id: u64,
        id: BufferId,
        path: PathBuf,
        version: u64,
    },
    CodeActions {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
    ResolveCodeAction {
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    },
}

pub(super) fn register_pending_request(
    pending_requests: &mut PendingLspRequests,
    request_id: u64,
    pending: PendingLspRequest,
) {
    if pending_requests.contains_key(&request_id) || !pending.has_valid_target() {
        return;
    }

    pending_requests.insert(request_id, pending);
    prune_oldest_pending_requests(pending_requests, MAX_PENDING_LSP_REQUESTS);
}

impl PendingLspRequest {
    fn has_valid_target(&self) -> bool {
        let (id, path) = match self {
            Self::Hover { id, path, .. }
            | Self::DocumentHighlights { id, path, .. }
            | Self::Definition { id, path, .. }
            | Self::PrepareCallHierarchy { id, path, .. }
            | Self::CallHierarchyIncoming { id, path, .. }
            | Self::CallHierarchyOutgoing { id, path, .. }
            | Self::PrepareTypeHierarchy { id, path, .. }
            | Self::TypeHierarchySupertypes { id, path, .. }
            | Self::TypeHierarchySubtypes { id, path, .. }
            | Self::References { id, path, .. }
            | Self::PrepareRename { id, path, .. }
            | Self::Rename { id, path, .. }
            | Self::DocumentSymbols { id, path, .. }
            | Self::FoldingRanges { id, path, .. }
            | Self::InlayHints { id, path, .. }
            | Self::CodeLenses { id, path, .. }
            | Self::ResolveCodeLens { id, path, .. }
            | Self::ExecuteCommand { id, path, .. }
            | Self::SemanticTokens { id, path, .. }
            | Self::WorkspaceSymbols { id, path, .. }
            | Self::Completion { id, path, .. }
            | Self::ResolveCompletionItem { id, path, .. }
            | Self::SignatureHelp { id, path, .. }
            | Self::Formatting { id, path, .. }
            | Self::CodeActions { id, path, .. }
            | Self::ResolveCodeAction { id, path, .. } => (id, path),
        };

        lsp_request_target_is_valid(*id, path)
    }
}

#[cfg(test)]
pub(super) fn superseded_pending_request_ids(
    pending_requests: &PendingLspRequests,
    command: &LspClientCommand,
) -> Vec<u64> {
    pending_request_dispatch_plan(pending_requests, command).0
}

#[cfg(test)]
pub(super) fn pending_request_dispatch_plan(
    pending_requests: &PendingLspRequests,
    command: &LspClientCommand,
) -> (Vec<u64>, bool) {
    let key = PendingLspRequestCoalescingKey::from_command(command);
    let mut has_exact_match = false;
    let mut request_ids = Vec::new();
    for (request_id, pending) in pending_requests.iter() {
        if pending_request_matches_command(pending, command) {
            has_exact_match = true;
        } else if key.is_some_and(|key| key.matches_pending(pending)) {
            request_ids.push(request_id);
        }
    }
    request_ids.sort_unstable();
    (request_ids, has_exact_match)
}

#[cfg(test)]
pub(super) fn has_exact_pending_request(
    pending_requests: &PendingLspRequests,
    command: &LspClientCommand,
) -> bool {
    pending_requests
        .iter()
        .any(|(_, pending)| pending_request_matches_command(pending, command))
}

#[cfg(test)]
fn pending_request_matches_command(
    pending: &PendingLspRequest,
    command: &LspClientCommand,
) -> bool {
    match (pending, command) {
        (
            PendingLspRequest::Hover {
                id,
                path,
                version,
                line,
                character,
            },
            LspClientCommand::Hover {
                id: command_id,
                path: command_path,
                version: command_version,
                line: command_line,
                character: command_character,
            },
        )
        | (
            PendingLspRequest::DocumentHighlights {
                id,
                path,
                version,
                line,
                character,
            },
            LspClientCommand::DocumentHighlights {
                id: command_id,
                path: command_path,
                version: command_version,
                line: command_line,
                character: command_character,
            },
        )
        | (
            PendingLspRequest::Completion {
                id,
                path,
                version,
                line,
                character,
            },
            LspClientCommand::Completion {
                id: command_id,
                path: command_path,
                version: command_version,
                line: command_line,
                character: command_character,
            },
        )
        | (
            PendingLspRequest::SignatureHelp {
                id,
                path,
                version,
                line,
                character,
            },
            LspClientCommand::SignatureHelp {
                id: command_id,
                path: command_path,
                version: command_version,
                line: command_line,
                character: command_character,
            },
        ) => {
            id == command_id
                && path == command_path
                && version == command_version
                && line == command_line
                && character == command_character
        }
        (
            PendingLspRequest::DocumentSymbols { id, path, version },
            LspClientCommand::DocumentSymbols {
                id: command_id,
                path: command_path,
                version: command_version,
            },
        )
        | (
            PendingLspRequest::FoldingRanges { id, path, version },
            LspClientCommand::FoldingRanges {
                id: command_id,
                path: command_path,
                version: command_version,
            },
        )
        | (
            PendingLspRequest::SemanticTokens { id, path, version },
            LspClientCommand::SemanticTokens {
                id: command_id,
                path: command_path,
                version: command_version,
            },
        )
        | (
            PendingLspRequest::CodeLenses { id, path, version },
            LspClientCommand::CodeLenses {
                id: command_id,
                path: command_path,
                version: command_version,
            },
        ) => id == command_id && path == command_path && version == command_version,
        (
            PendingLspRequest::InlayHints {
                id,
                path,
                version,
                end_line,
                end_character,
            },
            LspClientCommand::InlayHints {
                id: command_id,
                path: command_path,
                version: command_version,
                end_line: command_end_line,
                end_character: command_end_character,
            },
        ) => {
            id == command_id
                && path == command_path
                && version == command_version
                && end_line == command_end_line
                && end_character == command_end_character
        }
        (
            PendingLspRequest::WorkspaceSymbols { id, path, query },
            LspClientCommand::WorkspaceSymbols {
                id: command_id,
                path: command_path,
                query: command_query,
            },
        ) => id == command_id && path == command_path && query == command_query,
        _ => false,
    }
}

fn prune_oldest_pending_requests(pending_requests: &mut PendingLspRequests, max_len: usize) {
    let max_formatting_len = MAX_PENDING_LSP_FORMATTING_REQUESTS.min(max_len);
    while pending_formatting_request_count(pending_requests) > max_formatting_len {
        let Some(oldest) = oldest_matching_pending_request_id(pending_requests, |pending| {
            matches!(pending, PendingLspRequest::Formatting { .. })
        }) else {
            break;
        };
        pending_requests.remove(&oldest);
    }

    while pending_requests.len() > max_len {
        let Some(oldest) = oldest_matching_pending_request_id(pending_requests, |pending| {
            !matches!(pending, PendingLspRequest::Formatting { .. })
        })
        .or_else(|| oldest_matching_pending_request_id(pending_requests, |_| true)) else {
            break;
        };
        pending_requests.remove(&oldest);
    }
}

fn pending_formatting_request_count(pending_requests: &PendingLspRequests) -> usize {
    pending_requests
        .iter()
        .filter(|(_, pending)| matches!(pending, PendingLspRequest::Formatting { .. }))
        .count()
}

fn oldest_matching_pending_request_id(
    pending_requests: &PendingLspRequests,
    matches_pending: impl Fn(&PendingLspRequest) -> bool,
) -> Option<u64> {
    pending_requests
        .iter()
        .filter_map(|(request_id, pending)| matches_pending(pending).then_some(request_id))
        .min()
}

#[cfg(test)]
#[derive(Debug, Copy, Clone)]
enum PendingLspRequestCoalescingKey<'a> {
    Hover(BufferId, &'a Path),
    DocumentHighlights(BufferId, &'a Path),
    DocumentSymbols(BufferId, &'a Path),
    FoldingRanges(BufferId, &'a Path),
    InlayHints(BufferId, &'a Path),
    CodeLenses(BufferId, &'a Path),
    SemanticTokens(BufferId, &'a Path),
    WorkspaceSymbols(BufferId, &'a Path),
    Completion(BufferId, &'a Path),
    SignatureHelp(BufferId, &'a Path),
    CodeActions(BufferId, &'a Path),
}

#[cfg(test)]
impl<'a> PendingLspRequestCoalescingKey<'a> {
    fn from_command(command: &'a LspClientCommand) -> Option<Self> {
        match command {
            LspClientCommand::Hover { id, path, .. } => Some(Self::Hover(*id, path.as_path())),
            LspClientCommand::DocumentHighlights { id, path, .. } => {
                Some(Self::DocumentHighlights(*id, path.as_path()))
            }
            LspClientCommand::DocumentSymbols { id, path, .. } => {
                Some(Self::DocumentSymbols(*id, path.as_path()))
            }
            LspClientCommand::FoldingRanges { id, path, .. } => {
                Some(Self::FoldingRanges(*id, path.as_path()))
            }
            LspClientCommand::InlayHints { id, path, .. } => {
                Some(Self::InlayHints(*id, path.as_path()))
            }
            LspClientCommand::CodeLenses { id, path, .. } => {
                Some(Self::CodeLenses(*id, path.as_path()))
            }
            LspClientCommand::SemanticTokens { id, path, .. } => {
                Some(Self::SemanticTokens(*id, path.as_path()))
            }
            LspClientCommand::WorkspaceSymbols { id, path, .. } => {
                Some(Self::WorkspaceSymbols(*id, path.as_path()))
            }
            LspClientCommand::Completion { id, path, .. } => {
                Some(Self::Completion(*id, path.as_path()))
            }
            LspClientCommand::SignatureHelp { id, path, .. } => {
                Some(Self::SignatureHelp(*id, path.as_path()))
            }
            LspClientCommand::CodeActions { id, path, .. } => {
                Some(Self::CodeActions(*id, path.as_path()))
            }
            _ => None,
        }
    }

    fn matches_pending(self, pending: &PendingLspRequest) -> bool {
        match (self, pending) {
            (Self::Hover(command_id, command_path), PendingLspRequest::Hover { id, path, .. })
            | (
                Self::DocumentHighlights(command_id, command_path),
                PendingLspRequest::DocumentHighlights { id, path, .. },
            )
            | (
                Self::DocumentSymbols(command_id, command_path),
                PendingLspRequest::DocumentSymbols { id, path, .. },
            )
            | (
                Self::FoldingRanges(command_id, command_path),
                PendingLspRequest::FoldingRanges { id, path, .. },
            )
            | (
                Self::InlayHints(command_id, command_path),
                PendingLspRequest::InlayHints { id, path, .. },
            )
            | (
                Self::CodeLenses(command_id, command_path),
                PendingLspRequest::CodeLenses { id, path, .. },
            )
            | (
                Self::SemanticTokens(command_id, command_path),
                PendingLspRequest::SemanticTokens { id, path, .. },
            )
            | (
                Self::WorkspaceSymbols(command_id, command_path),
                PendingLspRequest::WorkspaceSymbols { id, path, .. },
            )
            | (
                Self::Completion(command_id, command_path),
                PendingLspRequest::Completion { id, path, .. },
            )
            | (
                Self::SignatureHelp(command_id, command_path),
                PendingLspRequest::SignatureHelp { id, path, .. },
            )
            | (
                Self::CodeActions(command_id, command_path),
                PendingLspRequest::CodeActions { id, path, .. },
            ) => command_id == *id && command_path == path.as_path(),
            _ => false,
        }
    }
}

#[cfg(test)]
impl<const N: usize> From<[(u64, PendingLspRequest); N]> for PendingLspRequests {
    fn from(entries: [(u64, PendingLspRequest); N]) -> Self {
        let mut pending_requests = Self::default();
        for (request_id, pending) in entries {
            pending_requests.insert(request_id, pending);
        }
        pending_requests
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_REQUEST_TIMEOUT, MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS,
        MAX_PENDING_LSP_FORMATTING_REQUESTS, MAX_PENDING_LSP_REQUESTS, PendingLspRequest,
        PendingLspRequests, bounded_lsp_outbound_text, has_exact_pending_request,
        lsp_json_payload_is_bounded, pending_formatting_request_count,
        prune_oldest_pending_requests, register_pending_request, superseded_pending_request_ids,
    };
    use crate::lsp_client::commands::LspClientCommand;
    use std::{path::PathBuf, time::Duration};

    fn pending(version: u64) -> PendingLspRequest {
        PendingLspRequest::Formatting {
            request_id: version,
            id: 1,
            path: PathBuf::from("src/main.rs"),
            version,
        }
    }

    fn hover(request_id: u64) -> PendingLspRequest {
        PendingLspRequest::Hover {
            id: 1,
            path: PathBuf::from(format!("src/{request_id}.rs")),
            version: request_id,
            line: 0,
            character: 0,
        }
    }

    #[test]
    fn pending_lsp_requests_are_bounded_by_registration() {
        let mut pending_requests = PendingLspRequests::default();

        for request_id in 1..=(MAX_PENDING_LSP_REQUESTS as u64 + 2) {
            register_pending_request(&mut pending_requests, request_id, hover(request_id));
        }

        assert_eq!(pending_requests.len(), MAX_PENDING_LSP_REQUESTS);
        assert!(!pending_requests.contains_key(&1));
        assert!(!pending_requests.contains_key(&2));
        assert!(pending_requests.contains_key(&(MAX_PENDING_LSP_REQUESTS as u64 + 2)));
    }

    #[test]
    fn pending_lsp_request_registration_keeps_existing_duplicate_id() {
        let mut pending_requests = PendingLspRequests::from([(7, hover(1))]);

        register_pending_request(&mut pending_requests, 7, hover(2));

        assert!(matches!(
            pending_requests.get(&7),
            Some(PendingLspRequest::Hover { version: 1, .. })
        ));
    }

    #[test]
    fn pending_lsp_request_registration_rejects_invalid_target_state() {
        let mut pending_requests = PendingLspRequests::default();

        register_pending_request(
            &mut pending_requests,
            7,
            PendingLspRequest::Hover {
                id: 0,
                path: PathBuf::from("src/main.rs"),
                version: 1,
                line: 0,
                character: 0,
            },
        );
        register_pending_request(
            &mut pending_requests,
            8,
            PendingLspRequest::Hover {
                id: 1,
                path: PathBuf::new(),
                version: 1,
                line: 0,
                character: 0,
            },
        );

        assert!(pending_requests.is_empty());
    }

    #[test]
    fn outbound_lsp_text_payloads_are_bounded_by_character_count() {
        let valid = "a".repeat(MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS);
        let oversized = "b".repeat(MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS + 1);

        assert_eq!(
            bounded_lsp_outbound_text(valid.clone(), MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS),
            Some(valid)
        );
        assert!(
            bounded_lsp_outbound_text(oversized, MAX_LSP_OUTBOUND_TEXT_PAYLOAD_CHARS).is_none()
        );
    }

    #[test]
    fn outbound_lsp_json_payloads_are_bounded_by_serialized_size() {
        let exact = serde_json::json!({ "data": "x".repeat(8) });
        let max_bytes = serde_json::to_vec(&exact).unwrap().len();

        assert!(lsp_json_payload_is_bounded(&exact, max_bytes));
        assert!(!lsp_json_payload_is_bounded(
            &serde_json::json!({ "data": "x".repeat(9) }),
            max_bytes
        ));
    }

    #[test]
    fn pending_lsp_request_pruning_removes_lowest_request_ids_first() {
        let mut pending_requests =
            PendingLspRequests::from([(10, hover(10)), (5, hover(5)), (7, hover(7))]);

        prune_oldest_pending_requests(&mut pending_requests, 1);

        assert_eq!(pending_requests.len(), 1);
        assert!(pending_requests.contains_key(&10));
    }

    #[test]
    fn pending_lsp_request_pruning_preserves_formatting_requests() {
        let mut pending_requests = PendingLspRequests::from([(1, pending(1)), (2, hover(2))]);

        prune_oldest_pending_requests(&mut pending_requests, 1);

        assert!(matches!(
            pending_requests.get(&1),
            Some(PendingLspRequest::Formatting { .. })
        ));
        assert!(!pending_requests.contains_key(&2));
    }

    #[test]
    fn pending_lsp_request_pruning_bounds_all_formatting_requests() {
        let mut pending_requests = PendingLspRequests::default();

        for request_id in 1..=(MAX_PENDING_LSP_REQUESTS as u64 + 2) {
            register_pending_request(&mut pending_requests, request_id, pending(request_id));
        }

        assert_eq!(pending_requests.len(), MAX_PENDING_LSP_FORMATTING_REQUESTS);
        assert_eq!(
            pending_formatting_request_count(&pending_requests),
            MAX_PENDING_LSP_FORMATTING_REQUESTS
        );
        assert!(!pending_requests.contains_key(&1));
        assert!(!pending_requests.contains_key(&2));
        assert!(pending_requests.contains_key(&(MAX_PENDING_LSP_REQUESTS as u64 + 2)));
    }

    #[test]
    fn pending_lsp_request_pruning_enforces_formatting_cap_before_total_cap() {
        let newest_formatting = MAX_PENDING_LSP_FORMATTING_REQUESTS as u64 + 2;
        let mut pending_requests = PendingLspRequests::from([(1_000, hover(1_000))]);
        for request_id in 1..=newest_formatting {
            pending_requests.insert(request_id, pending(request_id));
        }

        prune_oldest_pending_requests(&mut pending_requests, MAX_PENDING_LSP_REQUESTS);

        assert!(pending_requests.contains_key(&1_000));
        assert!(!pending_requests.contains_key(&1));
        assert!(!pending_requests.contains_key(&2));
        assert!(pending_requests.contains_key(&newest_formatting));
        assert_eq!(
            pending_formatting_request_count(&pending_requests),
            MAX_PENDING_LSP_FORMATTING_REQUESTS
        );
    }

    #[test]
    fn pending_lsp_request_pruning_removes_oldest_formatting_when_only_formatting_remains() {
        let mut pending_requests = PendingLspRequests::from([(1, pending(1)), (2, pending(2))]);

        prune_oldest_pending_requests(&mut pending_requests, 1);

        assert_eq!(pending_requests.len(), 1);
        assert!(!pending_requests.contains_key(&1));
        assert!(matches!(
            pending_requests.get(&2),
            Some(PendingLspRequest::Formatting { .. })
        ));
    }

    #[test]
    fn superseded_pending_lsp_requests_are_taken_for_ui_driven_request_family() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([
            (
                10,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 1,
                    character: 2,
                },
            ),
            (
                11,
                PendingLspRequest::Hover {
                    id: 2,
                    path: path.clone(),
                    version: 3,
                    line: 1,
                    character: 2,
                },
            ),
            (
                12,
                PendingLspRequest::Completion {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 1,
                    character: 2,
                },
            ),
            (
                13,
                PendingLspRequest::Hover {
                    id: 1,
                    path: PathBuf::from("src/lib.rs"),
                    version: 3,
                    line: 1,
                    character: 2,
                },
            ),
        ]);

        let superseded = superseded_pending_request_ids(
            &pending_requests,
            &LspClientCommand::Hover {
                id: 1,
                path,
                version: 4,
                line: 3,
                character: 4,
            },
        );

        assert_eq!(superseded, vec![10]);
        assert!(pending_requests.contains_key(&10));
        assert!(pending_requests.contains_key(&11));
        assert!(pending_requests.contains_key(&12));
        assert!(pending_requests.contains_key(&13));
    }

    #[test]
    fn superseded_pending_lsp_requests_keep_exact_match_for_reuse() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([
            (
                10,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 2,
                    line: 1,
                    character: 2,
                },
            ),
            (
                11,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 4,
                    character: 5,
                },
            ),
        ]);

        let command = LspClientCommand::Hover {
            id: 1,
            path,
            version: 3,
            line: 4,
            character: 5,
        };
        let superseded = superseded_pending_request_ids(&pending_requests, &command);

        assert_eq!(superseded, vec![10]);
        assert!(pending_requests.contains_key(&10));
        assert!(pending_requests.contains_key(&11));
        assert!(has_exact_pending_request(&pending_requests, &command));
    }

    #[test]
    fn explicit_lsp_actions_do_not_supersede_pending_requests() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([(
            10,
            PendingLspRequest::References {
                id: 1,
                path: path.clone(),
                version: 3,
                line: 1,
                character: 2,
            },
        )]);

        let superseded = superseded_pending_request_ids(
            &pending_requests,
            &LspClientCommand::References {
                id: 1,
                path,
                version: 4,
                line: 3,
                character: 4,
                include_declaration: true,
            },
        );

        assert!(superseded.is_empty());
        assert!(pending_requests.contains_key(&10));
    }

    #[test]
    fn exact_pending_lsp_request_matches_same_high_frequency_command() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([
            (
                10,
                PendingLspRequest::Hover {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 4,
                    character: 5,
                },
            ),
            (
                11,
                PendingLspRequest::Completion {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 4,
                    character: 5,
                },
            ),
        ]);

        assert!(has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::Hover {
                id: 1,
                path,
                version: 3,
                line: 4,
                character: 5,
            },
        ));
    }

    #[test]
    fn exact_pending_lsp_request_rejects_stale_same_document_command() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([(
            10,
            PendingLspRequest::Hover {
                id: 1,
                path: path.clone(),
                version: 3,
                line: 4,
                character: 5,
            },
        )]);

        assert!(!has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::Hover {
                id: 1,
                path: path.clone(),
                version: 4,
                line: 4,
                character: 5,
            },
        ));
        assert!(!has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::Hover {
                id: 1,
                path,
                version: 3,
                line: 4,
                character: 6,
            },
        ));
    }

    #[test]
    fn exact_pending_lsp_request_matches_inlay_hints_with_complete_inputs() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([(
            10,
            PendingLspRequest::InlayHints {
                id: 1,
                path: path.clone(),
                version: 3,
                end_line: 100,
                end_character: 0,
            },
        )]);

        assert!(has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::InlayHints {
                id: 1,
                path: path.clone(),
                version: 3,
                end_line: 100,
                end_character: 0,
            },
        ));
    }

    #[test]
    fn exact_pending_lsp_request_rejects_stale_inlay_hint_inputs() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([(
            10,
            PendingLspRequest::InlayHints {
                id: 1,
                path: path.clone(),
                version: 3,
                end_line: 100,
                end_character: 0,
            },
        )]);

        assert!(!has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::InlayHints {
                id: 1,
                path: path.clone(),
                version: 4,
                end_line: 100,
                end_character: 0,
            },
        ));
        assert!(!has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::InlayHints {
                id: 1,
                path,
                version: 3,
                end_line: 101,
                end_character: 0,
            },
        ));
    }

    #[test]
    fn exact_pending_lsp_request_matches_symbol_requests_with_complete_inputs() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([
            (
                10,
                PendingLspRequest::CodeLenses {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                },
            ),
            (
                11,
                PendingLspRequest::WorkspaceSymbols {
                    id: 1,
                    path: path.clone(),
                    query: "read".to_owned(),
                },
            ),
        ]);

        assert!(has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::CodeLenses {
                id: 1,
                path: path.clone(),
                version: 3,
            },
        ));
        assert!(has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::WorkspaceSymbols {
                id: 1,
                path,
                query: "read".to_owned(),
            },
        ));
    }

    #[test]
    fn exact_pending_lsp_request_rejects_stale_symbol_requests() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([
            (
                10,
                PendingLspRequest::CodeLenses {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                },
            ),
            (
                11,
                PendingLspRequest::WorkspaceSymbols {
                    id: 1,
                    path: path.clone(),
                    query: "read".to_owned(),
                },
            ),
        ]);

        assert!(!has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::CodeLenses {
                id: 1,
                path: path.clone(),
                version: 4,
            },
        ));
        assert!(!has_exact_pending_request(
            &pending_requests,
            &LspClientCommand::WorkspaceSymbols {
                id: 1,
                path,
                query: "write".to_owned(),
            },
        ));
    }

    #[test]
    fn code_action_list_requests_supersede_older_code_action_lists_for_same_document() {
        let path = PathBuf::from("src/main.rs");
        let pending_requests = PendingLspRequests::from([
            (
                10,
                PendingLspRequest::CodeActions {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 1,
                    character: 2,
                },
            ),
            (
                11,
                PendingLspRequest::ResolveCodeAction {
                    id: 1,
                    path: path.clone(),
                    version: 3,
                    line: 1,
                    character: 2,
                },
            ),
            (
                12,
                PendingLspRequest::CodeActions {
                    id: 2,
                    path: path.clone(),
                    version: 3,
                    line: 1,
                    character: 2,
                },
            ),
            (
                13,
                PendingLspRequest::CodeActions {
                    id: 1,
                    path: PathBuf::from("src/lib.rs"),
                    version: 3,
                    line: 1,
                    character: 2,
                },
            ),
        ]);

        let superseded = superseded_pending_request_ids(
            &pending_requests,
            &LspClientCommand::CodeActions {
                id: 1,
                path,
                version: 4,
                origin_line: 9,
                origin_character: 5,
                start_line: 9,
                start_character: 5,
                end_line: 9,
                end_character: 5,
                diagnostics: Vec::new(),
            },
        );

        assert_eq!(superseded, vec![10]);
        assert!(pending_requests.contains_key(&10));
        assert!(pending_requests.contains_key(&11));
        assert!(pending_requests.contains_key(&12));
        assert!(pending_requests.contains_key(&13));
    }

    #[test]
    fn pending_request_timeout_is_twenty_seconds() {
        assert_eq!(LSP_REQUEST_TIMEOUT, Duration::from_secs(20));
    }

    #[test]
    fn inserted_pending_requests_get_deadlines_at_registration() {
        let before = tokio::time::Instant::now();
        let mut pending_requests = PendingLspRequests::default();

        pending_requests.insert(7, hover(7));

        let deadline = pending_requests
            .earliest_deadline()
            .expect("registered request should carry a deadline");
        assert!(deadline >= before + LSP_REQUEST_TIMEOUT);
        assert!(deadline <= tokio::time::Instant::now() + LSP_REQUEST_TIMEOUT);
    }

    #[test]
    fn earliest_deadline_is_none_without_pending_requests_and_uses_the_minimum() {
        let mut pending_requests = PendingLspRequests::default();
        assert_eq!(pending_requests.earliest_deadline(), None);

        pending_requests =
            PendingLspRequests::from([(10, hover(10)), (5, hover(5)), (7, hover(7))]);
        let base = tokio::time::Instant::now();
        pending_requests.deadlines.insert(5, base);
        pending_requests
            .deadlines
            .insert(7, base + LSP_REQUEST_TIMEOUT);
        pending_requests
            .deadlines
            .insert(10, base + LSP_REQUEST_TIMEOUT * 2);

        assert_eq!(pending_requests.earliest_deadline(), Some(base));
    }

    #[test]
    fn take_expired_removes_only_due_entries_in_request_id_order() {
        let now = tokio::time::Instant::now();
        let mut pending_requests =
            PendingLspRequests::from([(10, hover(10)), (5, hover(5)), (7, hover(7))]);
        pending_requests
            .deadlines
            .insert(5, now - Duration::from_secs(1));
        pending_requests
            .deadlines
            .insert(7, now - Duration::from_secs(2));
        pending_requests
            .deadlines
            .insert(10, now + Duration::from_secs(30));

        let expired = pending_requests.take_expired(now);
        let expired_ids: Vec<u64> = expired
            .into_iter()
            .map(|(request_id, _)| request_id)
            .collect();

        assert_eq!(expired_ids, vec![5, 7]);
        assert!(pending_requests.contains_key(&10));
        assert_eq!(pending_requests.len(), 1);
    }

    #[test]
    fn removing_a_pending_request_also_drops_its_deadline() {
        let mut pending_requests = PendingLspRequests::from([(7, hover(7))]);
        assert!(pending_requests.earliest_deadline().is_some());

        assert!(pending_requests.remove(&7).is_some());

        assert_eq!(pending_requests.earliest_deadline(), None);
        assert!(pending_requests.is_empty());
    }
}

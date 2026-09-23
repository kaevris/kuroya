use kuroya_core::{Diagnostic, TextBuffer};
use std::{
    collections::{HashMap, VecDeque},
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) const LSP_DIAGNOSTIC_BATCH_DELAY: Duration = Duration::from_millis(50);
pub(crate) const PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY: usize = 1024;
const PENDING_LSP_DIAGNOSTIC_PAYLOAD_CAPACITY: usize = 5_000;

#[derive(Debug, Default)]
pub(crate) struct PendingLspDiagnosticsBatch {
    first_queued_at: Option<Instant>,

    diagnostics_by_path: HashMap<PathBuf, HashMap<PendingServerSlot, PendingLspDiagnostics>>,
    path_keys: HashMap<PendingLspDiagnosticsPathKey, PathBuf>,
    path_order: VecDeque<PathBuf>,
}

type PendingServerSlot = Option<PendingLspDiagnosticsSource>;

#[derive(Debug)]
enum PendingSlotUpdate {
    Insert,
    Replace { previous_queued_at: Instant },
}

#[derive(Debug)]
struct PendingLspDiagnostics {
    source: Option<PendingLspDiagnosticsSource>,
    queued_at: Instant,
    version: Option<u64>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PendingLspDiagnosticsPathKey {
    prefix: Option<String>,
    rooted: bool,
    components: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PendingLspDiagnosticsSource {
    pub(crate) language: String,
    pub(crate) root: PathBuf,
    pub(crate) generation: u64,
}

#[derive(Debug)]
pub(crate) struct PendingLspDiagnosticsEntry {
    pub(crate) source: Option<PendingLspDiagnosticsSource>,
    pub(crate) path: PathBuf,
    pub(crate) version: Option<u64>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl PendingLspDiagnosticsBatch {
    #[cfg(test)]
    pub(crate) fn queue(
        &mut self,
        path: PathBuf,
        version: Option<u64>,
        diagnostics: Vec<Diagnostic>,
        now: Instant,
    ) {
        self.queue_with_source(None, path, version, diagnostics, now);
    }

    pub(crate) fn queue_for_server(
        &mut self,
        source: PendingLspDiagnosticsSource,
        path: PathBuf,
        version: Option<u64>,
        diagnostics: Vec<Diagnostic>,
        now: Instant,
    ) {
        self.queue_with_source(Some(source), path, version, diagnostics, now);
    }

    fn queue_with_source(
        &mut self,
        source: Option<PendingLspDiagnosticsSource>,
        path: PathBuf,
        version: Option<u64>,
        mut diagnostics: Vec<Diagnostic>,
        now: Instant,
    ) {
        limit_pending_lsp_diagnostics_payload(&mut diagnostics);
        if let Some(existing_path) = self.pending_path_for(&path) {
            self.update_pending_lsp_diagnostics(&existing_path, source, version, diagnostics, now);
            return;
        }

        let path_key = PendingLspDiagnosticsPathKey::new(&path);
        let mut evicted = false;
        while self.diagnostics_by_path.len() >= PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY {
            if self.evict_oldest_pending_path() {
                evicted = true;
            } else {
                break;
            }
        }
        if evicted {
            self.refresh_first_queued_at();
        }
        self.path_order.push_back(path.clone());
        if let Some(path_key) = path_key {
            self.path_keys.insert(path_key, path.clone());
        }
        self.diagnostics_by_path
            .insert(path.clone(), HashMap::new());
        self.update_pending_lsp_diagnostics(&path, source, version, diagnostics, now);
    }

    fn pending_path_for(&mut self, path: &Path) -> Option<PathBuf> {
        if self.diagnostics_by_path.contains_key(path) {
            return Some(path.to_path_buf());
        }
        let key = PendingLspDiagnosticsPathKey::new(path)?;
        let existing_path = self.path_keys.get(&key).cloned()?;
        if self.diagnostics_by_path.contains_key(&existing_path) {
            Some(existing_path)
        } else {
            self.path_keys.remove(&key);
            None
        }
    }

    fn update_pending_lsp_diagnostics(
        &mut self,
        path: &Path,
        source: Option<PendingLspDiagnosticsSource>,
        version: Option<u64>,
        diagnostics: Vec<Diagnostic>,
        now: Instant,
    ) {
        let slot = match self
            .diagnostics_by_path
            .get(path)
            .and_then(|slots| slots.get(&source))
        {
            Some(existing) => {
                if !lsp_diagnostics_payload_should_replace(existing.version, version) {
                    return;
                }
                PendingSlotUpdate::Replace {
                    previous_queued_at: existing.queued_at,
                }
            }
            None => PendingSlotUpdate::Insert,
        };
        match slot {
            PendingSlotUpdate::Insert => {
                let pending = PendingLspDiagnostics {
                    source: source.clone(),
                    queued_at: now,
                    version,
                    diagnostics,
                };
                if let Some(slots) = self.diagnostics_by_path.get_mut(path) {
                    slots.insert(source, pending);
                }
                self.first_queued_at = Some(
                    self.first_queued_at
                        .map_or(now, |queued_at| queued_at.min(now)),
                );
            }
            PendingSlotUpdate::Replace { previous_queued_at } => {
                if let Some(existing) = self
                    .diagnostics_by_path
                    .get_mut(path)
                    .and_then(|slots| slots.get_mut(&source))
                {
                    replace_pending_lsp_diagnostics(existing, source, version, diagnostics, now);
                }
                self.move_path_order_to_back(path);
                self.refresh_first_queued_at_after_requeue(previous_queued_at, now);
            }
        }
    }

    pub(crate) fn take_due_entries(
        &mut self,
        now: Instant,
        delay: Duration,
    ) -> Vec<PendingLspDiagnosticsEntry> {
        self.take_due_pending(now, delay)
    }

    #[cfg(test)]
    pub(crate) fn take_due(
        &mut self,
        now: Instant,
        delay: Duration,
    ) -> Vec<(PathBuf, Option<u64>, Vec<Diagnostic>)> {
        self.take_due_pending(now, delay)
            .into_iter()
            .map(|entry| (entry.path, entry.version, entry.diagnostics))
            .collect()
    }

    fn take_due_pending(
        &mut self,
        now: Instant,
        delay: Duration,
    ) -> Vec<PendingLspDiagnosticsEntry> {
        if self.diagnostics_by_path.is_empty()
            || self
                .first_queued_at
                .is_some_and(|queued_at| now.saturating_duration_since(queued_at) < delay)
        {
            return Vec::new();
        }

        let mut entries =
            Vec::with_capacity(self.diagnostics_by_path.len().min(self.path_order.len()));
        let mut path_order = std::mem::take(&mut self.path_order);
        let mut retained_order = None;
        let mut next_first_queued_at = None;
        while let Some(path) = path_order.pop_front() {
            let Some(slots) = self.diagnostics_by_path.get(&path) else {
                continue;
            };
            let due_slots: Vec<PendingServerSlot> = slots
                .iter()
                .filter(|(_, pending)| now.saturating_duration_since(pending.queued_at) >= delay)
                .map(|(slot, _)| slot.clone())
                .collect();
            if due_slots.is_empty() {
                if let Some(queued_at) = slots.values().map(|pending| pending.queued_at).min() {
                    next_first_queued_at = Some(
                        next_first_queued_at
                            .map_or(queued_at, |first: Instant| first.min(queued_at)),
                    );
                }
                retained_order
                    .get_or_insert_with(|| VecDeque::with_capacity(path_order.len() + 1))
                    .push_back(path);
                continue;
            }

            for slot in due_slots {
                if let Some(pending) = self.remove_pending_entry(&path, slot) {
                    entries.push(PendingLspDiagnosticsEntry {
                        source: pending.source,
                        path: path.clone(),
                        version: pending.version,
                        diagnostics: pending.diagnostics,
                    });
                }
            }

            if self
                .diagnostics_by_path
                .get(&path)
                .is_some_and(|slots| !slots.is_empty())
            {
                retained_order
                    .get_or_insert_with(|| VecDeque::with_capacity(path_order.len() + 1))
                    .push_back(path);
            }
        }
        self.path_order = retained_order.unwrap_or_default();
        self.first_queued_at = next_first_queued_at;
        if entries.is_empty() {
            return Vec::new();
        }
        if entries.len() > 1 {
            entries.sort_by(|left, right| left.path.cmp(&right.path));
        }
        entries
    }

    pub(crate) fn next_due_after(&self, now: Instant, delay: Duration) -> Option<Duration> {
        if self.diagnostics_by_path.is_empty() {
            return None;
        }
        Some(
            self.first_queued_at
                .map(|queued_at| queued_at + delay)
                .map_or(Duration::ZERO, |due| due.saturating_duration_since(now)),
        )
    }

    pub(crate) fn clear(&mut self) {
        self.first_queued_at = None;
        self.diagnostics_by_path.clear();
        self.path_keys.clear();
        self.path_order.clear();
    }

    fn remove_pending_entry(
        &mut self,
        path: &Path,
        slot: PendingServerSlot,
    ) -> Option<PendingLspDiagnostics> {
        let removed = self.diagnostics_by_path.get_mut(path)?.remove(&slot)?;
        if self
            .diagnostics_by_path
            .get(path)
            .is_none_or(|slots| slots.is_empty())
        {
            self.diagnostics_by_path.remove(path);
            if let Some(key) = PendingLspDiagnosticsPathKey::new(path) {
                self.path_keys.remove(&key);
            }
        }
        Some(removed)
    }

    fn remove_path_entry(&mut self, path: &Path) -> bool {
        let removed = self.diagnostics_by_path.remove(path);
        if let Some(key) = PendingLspDiagnosticsPathKey::new(path) {
            self.path_keys.remove(&key);
        }
        removed.is_some_and(|slots| !slots.is_empty())
    }

    fn evict_oldest_pending_path(&mut self) -> bool {
        while let Some(oldest) = self.path_order.pop_front() {
            if self.remove_path_entry(&oldest) {
                return true;
            }
        }

        let Some(oldest) = self
            .diagnostics_by_path
            .iter()
            .filter(|(_, slots)| !slots.is_empty())
            .min_by(|(left_path, left), (right_path, right)| {
                let left_queued_at = left.values().map(|pending| pending.queued_at).min();
                let right_queued_at = right.values().map(|pending| pending.queued_at).min();
                left_queued_at
                    .cmp(&right_queued_at)
                    .then_with(|| left_path.cmp(right_path))
            })
            .map(|(path, _)| path.clone())
        else {
            return false;
        };
        self.remove_path_entry(&oldest)
    }

    fn move_path_order_to_back(&mut self, path: &Path) {
        self.path_order.retain(|queued_path| queued_path != path);
        self.path_order.push_back(path.to_path_buf());
    }

    fn refresh_first_queued_at(&mut self) {
        self.first_queued_at = self
            .diagnostics_by_path
            .values()
            .filter_map(|slots| slots.values().map(|pending| pending.queued_at).min())
            .min();
    }

    fn refresh_first_queued_at_after_requeue(&mut self, previous_queued_at: Instant, now: Instant) {
        if self.first_queued_at == Some(previous_queued_at) {
            self.refresh_first_queued_at();
        } else {
            self.first_queued_at = Some(
                self.first_queued_at
                    .map_or(now, |queued_at| queued_at.min(now)),
            );
        }
    }
}

impl PendingLspDiagnosticsPathKey {
    fn new(path: &Path) -> Option<Self> {
        if path.as_os_str().is_empty() {
            return None;
        }

        let mut key = Self {
            prefix: None,
            rooted: false,
            components: Vec::new(),
        };
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => {
                    key.prefix = Some(pending_lsp_diagnostics_component_key(prefix.as_os_str()));
                }
                Component::RootDir => key.rooted = true,
                Component::CurDir => {}
                Component::ParentDir => {
                    if key
                        .components
                        .last()
                        .is_some_and(|component| component != "..")
                    {
                        key.components.pop();
                    } else {
                        key.components.push("..".to_owned());
                    }
                }
                Component::Normal(component) => {
                    key.components
                        .push(pending_lsp_diagnostics_component_key(component));
                }
            }
        }

        Some(key)
    }
}

fn pending_lsp_diagnostics_component_key(component: &OsStr) -> String {
    let component = component.to_string_lossy();
    #[cfg(windows)]
    {
        if component.is_ascii() {
            let mut component = component.into_owned();
            component.make_ascii_lowercase();
            component
        } else {
            component.to_lowercase()
        }
    }
    #[cfg(not(windows))]
    {
        component.into_owned()
    }
}

fn replace_pending_lsp_diagnostics(
    existing: &mut PendingLspDiagnostics,
    source: Option<PendingLspDiagnosticsSource>,
    version: Option<u64>,
    diagnostics: Vec<Diagnostic>,
    now: Instant,
) -> Instant {
    let previous_queued_at = existing.queued_at;
    existing.source = source;
    existing.queued_at = now;
    existing.version = version;
    existing.diagnostics = diagnostics;
    previous_queued_at
}

fn limit_pending_lsp_diagnostics_payload(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.truncate(PENDING_LSP_DIAGNOSTIC_PAYLOAD_CAPACITY);
}

fn lsp_diagnostics_payload_should_replace(
    existing_version: Option<u64>,
    incoming_version: Option<u64>,
) -> bool {
    match (existing_version, incoming_version) {
        (Some(existing), Some(incoming)) => incoming > existing,
        (Some(_), None) => false,
        (None, Some(_)) | (None, None) => true,
    }
}

pub(crate) fn valid_lsp_diagnostics_for_buffer(
    buffer: &TextBuffer,
    diagnostics: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    if diagnostics.is_empty() {
        return diagnostics;
    }

    let mut valid_diagnostics = Vec::with_capacity(diagnostics.len());
    let mut line_cache = LspDiagnosticLineCache::default();
    for diagnostic in diagnostics {
        if let Some(diagnostic) =
            valid_lsp_diagnostic_for_buffer(buffer, diagnostic, &mut line_cache)
        {
            valid_diagnostics.push(diagnostic);
        }
    }
    valid_diagnostics
}

#[derive(Default)]
struct LspDiagnosticLineCache {
    line_idx: Option<usize>,
    text: String,
}

impl LspDiagnosticLineCache {
    fn line<'a>(&'a mut self, buffer: &TextBuffer, line_idx: usize) -> Option<&'a str> {
        if self.line_idx == Some(line_idx) {
            return Some(&self.text);
        }
        if line_idx >= buffer.len_lines() {
            return None;
        }

        let line_start = buffer.line_column_to_char(line_idx, 0);
        let line_end = buffer.line_content_end_char(line_idx);
        self.text = buffer.text_range(line_start..line_end)?;
        self.line_idx = Some(line_idx);
        Some(&self.text)
    }
}

#[derive(Clone, Copy)]
struct LspDiagnosticLineOffsets {
    start: usize,
    end: usize,
    char_len: usize,
}

fn valid_lsp_diagnostic_for_buffer(
    buffer: &TextBuffer,
    mut diagnostic: Diagnostic,
    line_cache: &mut LspDiagnosticLineCache,
) -> Option<Diagnostic> {
    let line_idx = diagnostic.line.checked_sub(1)?;
    let line = line_cache.line(buffer, line_idx)?;
    let start_utf16 = diagnostic.column.checked_sub(1)?;
    let offsets = lsp_diagnostic_line_offsets(line, start_utf16, diagnostic.char_range.end)?;
    let start = offsets.start;
    let mut end = offsets.end;
    if end < start {
        end = start;
    }
    if end == start && start < offsets.char_len {
        end = start + 1;
    }
    diagnostic.column = start + 1;
    diagnostic.char_range = start..end;
    Some(diagnostic)
}

fn lsp_diagnostic_line_offsets(
    text: &str,
    start_utf16: usize,
    requested_end_utf16: usize,
) -> Option<LspDiagnosticLineOffsets> {
    let mut utf16_offset = 0usize;
    let mut char_offset = 0usize;
    let mut start = (start_utf16 == 0).then_some(0);
    let mut end = (requested_end_utf16 == 0).then_some(0);

    for ch in text.chars() {
        if start.is_none() && start_utf16 == utf16_offset {
            start = Some(char_offset);
        }
        if end.is_none() && requested_end_utf16 == utf16_offset {
            end = Some(char_offset);
        }

        let width = ch.len_utf16();
        let next_utf16_offset = utf16_offset + width;
        let next_char_offset = char_offset + 1;

        if start.is_none() {
            if start_utf16 == next_utf16_offset {
                start = Some(next_char_offset);
            } else if start_utf16 > utf16_offset && start_utf16 < next_utf16_offset {
                start = Some(char_offset);
            }
        }
        if end.is_none() && requested_end_utf16 <= next_utf16_offset {
            if requested_end_utf16 == next_utf16_offset
                || (requested_end_utf16 > utf16_offset && requested_end_utf16 < next_utf16_offset)
            {
                end = Some(next_char_offset);
            }
        }

        utf16_offset = next_utf16_offset;
        char_offset = next_char_offset;
    }

    let start = match start {
        Some(start) => start,
        None if start_utf16 == utf16_offset => char_offset,
        None => return None,
    };
    let end = if requested_end_utf16 >= utf16_offset {
        char_offset
    } else {
        end?
    };

    Some(LspDiagnosticLineOffsets {
        start,
        end,
        char_len: char_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_DIAGNOSTIC_BATCH_DELAY, PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY,
        PENDING_LSP_DIAGNOSTIC_PAYLOAD_CAPACITY, PendingLspDiagnosticsBatch,
        PendingLspDiagnosticsPathKey, PendingLspDiagnosticsSource,
        valid_lsp_diagnostics_for_buffer,
    };
    use kuroya_core::{Diagnostic, DiagnosticSeverity, TextBuffer};
    use std::{
        collections::HashSet,
        ops::Range,
        path::{Path, PathBuf},
        time::{Duration, Instant},
    };

    fn diagnostic(path: &Path, message: &str) -> Diagnostic {
        Diagnostic {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            char_range: Range { start: 0, end: 1 },
            severity: DiagnosticSeverity::Warning,
            source: "lsp".to_owned(),
            message: message.to_owned(),
            unused: false,
            deprecated: false,
        }
    }

    fn positioned_diagnostic(
        path: &Path,
        line: usize,
        column: usize,
        range: Range<usize>,
        message: &str,
    ) -> Diagnostic {
        Diagnostic {
            line,
            column,
            char_range: range,
            ..diagnostic(path, message)
        }
    }

    #[test]
    fn pending_lsp_diagnostics_wait_for_batch_delay() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        batch.queue(path.clone(), None, vec![diagnostic(&path, "first")], now);

        assert!(batch.take_due(now, LSP_DIAGNOSTIC_BATCH_DELAY).is_empty());
        assert!(
            batch
                .take_due(
                    now + LSP_DIAGNOSTIC_BATCH_DELAY - Duration::from_millis(1),
                    LSP_DIAGNOSTIC_BATCH_DELAY,
                )
                .is_empty()
        );

        let entries = batch.take_due(
            now + Duration::from_millis(10) + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, path);
        assert_eq!(entries[0].1, None);
        assert_eq!(entries[0].2[0].message, "first");
    }

    #[test]
    fn pending_lsp_diagnostics_keep_latest_payload_per_path() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        batch.queue(path.clone(), Some(1), vec![diagnostic(&path, "first")], now);
        batch.queue(
            path.clone(),
            Some(2),
            vec![diagnostic(&path, "latest")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due(
            now + Duration::from_millis(10) + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, Some(2));
        assert_eq!(entries[0].2[0].message, "latest");
    }

    #[test]
    fn pending_lsp_diagnostics_keep_latest_payload_for_equivalent_paths() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        batch.queue(path.clone(), Some(1), vec![diagnostic(&path, "first")], now);
        batch.queue(
            equivalent_path.clone(),
            Some(2),
            vec![diagnostic(&equivalent_path, "latest")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due(
            now + Duration::from_millis(10) + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, path);
        assert_eq!(entries[0].1, Some(2));
        assert_eq!(entries[0].2[0].message, "latest");
    }

    #[test]
    fn pending_lsp_diagnostics_restarts_due_time_when_equivalent_path_replaces_payload() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let replaced_at = now + LSP_DIAGNOSTIC_BATCH_DELAY - Duration::from_millis(10);
        batch.queue(path.clone(), Some(1), vec![diagnostic(&path, "first")], now);
        batch.queue(
            equivalent_path.clone(),
            Some(2),
            vec![diagnostic(&equivalent_path, "latest")],
            replaced_at,
        );

        assert!(
            batch
                .take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY)
                .is_empty()
        );
        assert_eq!(
            batch.next_due_after(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY),
            Some(Duration::from_millis(40))
        );

        let entries = batch.take_due(
            replaced_at + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, path);
        assert_eq!(entries[0].1, Some(2));
        assert_eq!(entries[0].2[0].message, "latest");
    }

    #[test]
    fn pending_lsp_diagnostics_path_key_matches_lexical_equivalent_paths() {
        let key = PendingLspDiagnosticsPathKey::new(Path::new("workspace/src/main.rs"));
        let equivalent_key =
            PendingLspDiagnosticsPathKey::new(Path::new("workspace/src/../src/./main.rs"));
        let other_key = PendingLspDiagnosticsPathKey::new(Path::new("workspace/old/main.rs"));

        assert_eq!(key, equivalent_key);
        assert_ne!(key, other_key);
    }

    #[cfg(windows)]
    #[test]
    fn pending_lsp_diagnostics_path_key_matches_windows_paths_case_insensitively() {
        let key = PendingLspDiagnosticsPathKey::new(Path::new(r"C:\Repo\Project\src\main.rs"));
        let equivalent_key =
            PendingLspDiagnosticsPathKey::new(Path::new(r"c:/repo/project/src/./MAIN.rs"));

        assert_eq!(key, equivalent_key);
    }

    #[test]
    fn pending_lsp_diagnostics_clear_due_entries_removes_path_index() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("workspace/src/main.rs");
        batch.queue(path.clone(), None, vec![diagnostic(&path, "first")], now);

        assert_eq!(batch.path_keys.len(), 1);

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);

        assert_eq!(entries.len(), 1);
        assert!(batch.path_keys.is_empty());
    }

    #[test]
    fn pending_lsp_diagnostics_replaces_stale_equivalent_path_index() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("workspace/src/main.rs");
        let stale_equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let key = PendingLspDiagnosticsPathKey::new(&stale_equivalent_path).unwrap();
        batch.path_keys.insert(key, stale_equivalent_path);

        batch.queue(path.clone(), Some(1), vec![diagnostic(&path, "fresh")], now);

        let key = PendingLspDiagnosticsPathKey::new(&path).unwrap();
        assert_eq!(batch.path_keys.len(), 1);
        assert_eq!(batch.path_keys.get(&key), Some(&path));

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, path);
        assert_eq!(entries[0].2[0].message, "fresh");
    }

    #[test]
    fn pending_lsp_diagnostics_keep_payloads_from_coattached_servers_for_same_path() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        let root = PathBuf::from("workspace");

        batch.queue_for_server(
            PendingLspDiagnosticsSource {
                language: "rust".to_owned(),
                root: root.clone(),
                generation: 10,
            },
            path.clone(),
            Some(5),
            vec![diagnostic(&path, "primary server")],
            now,
        );

        batch.queue_for_server(
            PendingLspDiagnosticsSource {
                language: "rust".to_owned(),
                root,
                generation: 11,
            },
            path.clone(),
            Some(5),
            vec![diagnostic(&path, "sibling server")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due_entries(
            now + Duration::from_millis(10) + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 2);
        let mut payloads = entries
            .iter()
            .map(|entry| {
                (
                    entry.source.as_ref().unwrap().generation,
                    entry.diagnostics[0].message.as_str(),
                )
            })
            .collect::<Vec<_>>();
        payloads.sort_unstable();
        assert_eq!(
            payloads,
            vec![(10, "primary server"), (11, "sibling server")]
        );
    }

    #[test]
    fn pending_lsp_diagnostics_keep_distinct_server_roots_independent_for_same_path() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");

        batch.queue_for_server(
            PendingLspDiagnosticsSource {
                language: "rust".to_owned(),
                root: PathBuf::from("workspace/root"),
                generation: 3,
            },
            path.clone(),
            Some(1),
            vec![diagnostic(&path, "first root")],
            now,
        );
        batch.queue_for_server(
            PendingLspDiagnosticsSource {
                language: "rust".to_owned(),
                root: PathBuf::from("workspace/other"),
                generation: 4,
            },
            path.clone(),
            Some(9),
            vec![diagnostic(&path, "second root")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due_entries(
            now + Duration::from_millis(10) + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 2);
        let mut messages = entries
            .iter()
            .map(|entry| entry.diagnostics[0].message.as_str())
            .collect::<Vec<_>>();
        messages.sort_unstable();
        assert_eq!(messages, vec!["first root", "second root"]);
    }

    #[test]
    fn pending_lsp_diagnostics_replace_payload_for_same_server_slot() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        let root = PathBuf::from("workspace");
        let source = PendingLspDiagnosticsSource {
            language: "rust".to_owned(),
            root,
            generation: 2,
        };

        batch.queue_for_server(
            source.clone(),
            path.clone(),
            Some(1),
            vec![diagnostic(&path, "old payload")],
            now,
        );
        batch.queue_for_server(
            source,
            path.clone(),
            Some(2),
            vec![diagnostic(&path, "new payload")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due_entries(
            now + Duration::from_millis(10) + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].version, Some(2));
        assert_eq!(entries[0].diagnostics[0].message, "new payload");
    }

    #[test]
    fn pending_lsp_diagnostics_restarts_due_time_when_payload_is_replaced() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        let replaced_at = now + LSP_DIAGNOSTIC_BATCH_DELAY - Duration::from_millis(10);
        batch.queue(path.clone(), Some(1), vec![diagnostic(&path, "first")], now);
        batch.queue(
            path.clone(),
            Some(2),
            vec![diagnostic(&path, "latest")],
            replaced_at,
        );

        assert!(
            batch
                .take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY)
                .is_empty()
        );
        assert_eq!(
            batch.next_due_after(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY),
            Some(Duration::from_millis(40))
        );

        let entries = batch.take_due(
            replaced_at + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, Some(2));
        assert_eq!(entries[0].2[0].message, "latest");
    }

    #[test]
    fn pending_lsp_diagnostics_keep_newer_path_pending_when_older_path_is_due() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let old_path = PathBuf::from("src/old.rs");
        let new_path = PathBuf::from("src/new.rs");
        let newer_at = now + LSP_DIAGNOSTIC_BATCH_DELAY - Duration::from_millis(1);
        batch.queue(
            old_path.clone(),
            Some(1),
            vec![diagnostic(&old_path, "old")],
            now,
        );
        batch.queue(
            new_path.clone(),
            Some(2),
            vec![diagnostic(&new_path, "new")],
            newer_at,
        );

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, old_path);
        assert_eq!(entries[0].2[0].message, "old");
        assert_eq!(
            batch.next_due_after(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY),
            Some(Duration::from_millis(49))
        );

        let entries = batch.take_due(
            newer_at + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, new_path);
        assert_eq!(entries[0].2[0].message, "new");
    }

    #[test]
    fn pending_lsp_diagnostics_ignore_older_versioned_payload_per_path() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        batch.queue(
            path.clone(),
            Some(3),
            vec![diagnostic(&path, "latest")],
            now,
        );
        batch.queue(
            path.clone(),
            Some(2),
            vec![diagnostic(&path, "stale")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due(
            now + Duration::from_millis(10) + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, Some(3));
        assert_eq!(entries[0].2[0].message, "latest");
    }

    #[test]
    fn pending_lsp_diagnostics_ignore_older_payload_for_equivalent_paths() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        batch.queue(
            path.clone(),
            Some(3),
            vec![diagnostic(&path, "latest")],
            now,
        );
        batch.queue(
            equivalent_path,
            Some(2),
            vec![diagnostic(&path, "stale")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, path);
        assert_eq!(entries[0].1, Some(3));
        assert_eq!(entries[0].2[0].message, "latest");
    }

    #[test]
    fn pending_lsp_diagnostics_ignore_duplicate_versioned_payload_per_path() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        batch.queue(path.clone(), Some(2), vec![diagnostic(&path, "first")], now);
        batch.queue(
            path.clone(),
            Some(2),
            vec![diagnostic(&path, "latest")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, Some(2));
        assert_eq!(entries[0].2[0].message, "first");
    }

    #[test]
    fn pending_lsp_diagnostics_keep_versioned_payload_over_unversioned_duplicate() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        batch.queue(
            path.clone(),
            Some(2),
            vec![diagnostic(&path, "versioned")],
            now,
        );
        batch.queue(
            path.clone(),
            None,
            vec![diagnostic(&path, "unversioned")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, Some(2));
        assert_eq!(entries[0].2[0].message, "versioned");
    }

    #[test]
    fn pending_lsp_diagnostics_allow_versioned_payload_to_replace_unversioned() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        batch.queue(
            path.clone(),
            None,
            vec![diagnostic(&path, "unversioned")],
            now,
        );
        batch.queue(
            path.clone(),
            Some(2),
            vec![diagnostic(&path, "versioned")],
            now + Duration::from_millis(10),
        );

        let entries = batch.take_due(
            now + Duration::from_millis(10) + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, Some(2));
        assert_eq!(entries[0].2[0].message, "versioned");
    }

    #[test]
    fn pending_lsp_diagnostics_drain_due_entries_in_path_order() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        for path in [
            PathBuf::from("src/z.rs"),
            PathBuf::from("src/a.rs"),
            PathBuf::from("src/m.rs"),
        ] {
            batch.queue(
                path.clone(),
                None,
                vec![diagnostic(&path, "diagnostic")],
                now,
            );
        }

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);

        assert_eq!(
            entries
                .into_iter()
                .map(|(path, _, _)| path)
                .collect::<Vec<_>>(),
            vec![
                PathBuf::from("src/a.rs"),
                PathBuf::from("src/m.rs"),
                PathBuf::from("src/z.rs")
            ]
        );
    }

    #[test]
    fn pending_lsp_diagnostics_caps_payload_without_rewriting_retained_diagnostics() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        let raw_diagnostic_path = PathBuf::from("workspace/src/../src/main.rs");
        let diagnostics = (0..PENDING_LSP_DIAGNOSTIC_PAYLOAD_CAPACITY + 2)
            .map(|idx| diagnostic(&raw_diagnostic_path, &format!("raw message {idx}")))
            .collect::<Vec<_>>();

        batch.queue(path, None, diagnostics, now);

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].2.len(), PENDING_LSP_DIAGNOSTIC_PAYLOAD_CAPACITY);
        assert_eq!(entries[0].2[0].path, raw_diagnostic_path);
        assert_eq!(entries[0].2[0].message, "raw message 0");
        assert_eq!(
            entries[0].2[PENDING_LSP_DIAGNOSTIC_PAYLOAD_CAPACITY - 1].message,
            format!(
                "raw message {}",
                PENDING_LSP_DIAGNOSTIC_PAYLOAD_CAPACITY - 1
            )
        );
    }

    #[test]
    fn pending_lsp_diagnostics_caps_queued_paths() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        for idx in 0..PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY + 2 {
            let path = PathBuf::from(format!("src/file-{idx:04}.rs"));
            batch.queue(
                path.clone(),
                None,
                vec![diagnostic(&path, "diagnostic")],
                now,
            );
        }

        let entries = batch.take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY);
        let paths = entries
            .iter()
            .map(|(path, _, _)| path.clone())
            .collect::<HashSet<_>>();

        assert_eq!(entries.len(), PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY);
        assert!(!paths.contains(&PathBuf::from("src/file-0000.rs")));
        assert!(!paths.contains(&PathBuf::from("src/file-0001.rs")));
        assert!(paths.contains(&PathBuf::from("src/file-0002.rs")));
        assert!(paths.contains(&PathBuf::from(format!(
            "src/file-{:04}.rs",
            PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY + 1
        ))));
    }

    #[test]
    fn pending_lsp_diagnostics_capacity_eviction_uses_refreshed_order() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let protected_path = PathBuf::from("src/protected.rs");
        let old_path = PathBuf::from("src/old.rs");
        let refreshed_at = now + Duration::from_millis(10);
        batch.queue(
            protected_path.clone(),
            Some(1),
            vec![diagnostic(&protected_path, "first")],
            now,
        );
        batch.queue(
            old_path.clone(),
            None,
            vec![diagnostic(&old_path, "old")],
            now + Duration::from_millis(1),
        );
        batch.queue(
            protected_path.clone(),
            Some(2),
            vec![diagnostic(&protected_path, "latest")],
            refreshed_at,
        );

        assert_eq!(
            batch
                .path_order
                .iter()
                .filter(|path| *path == &protected_path)
                .count(),
            1
        );

        for idx in 0..PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY - 1 {
            let path = PathBuf::from(format!("src/new-{idx:04}.rs"));
            batch.queue(
                path.clone(),
                None,
                vec![diagnostic(&path, "new")],
                refreshed_at,
            );
        }

        let entries = batch.take_due(
            refreshed_at + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );
        let paths = entries
            .iter()
            .map(|(path, _, _)| path.clone())
            .collect::<HashSet<_>>();

        assert_eq!(entries.len(), PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY);
        assert!(paths.contains(&protected_path));
        assert!(!paths.contains(&old_path));
        assert_eq!(
            entries
                .iter()
                .find(|(path, _, _)| path == &protected_path)
                .unwrap()
                .2[0]
                .message,
            "latest"
        );
    }

    #[test]
    fn pending_lsp_diagnostics_recomputes_due_time_after_capacity_eviction() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let old_path = PathBuf::from("src/old.rs");
        let newer_at = now + LSP_DIAGNOSTIC_BATCH_DELAY - Duration::from_millis(1);
        batch.queue(
            old_path.clone(),
            None,
            vec![diagnostic(&old_path, "old")],
            now,
        );

        for idx in 0..PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY {
            let path = PathBuf::from(format!("src/new-{idx:04}.rs"));
            batch.queue(path.clone(), None, vec![diagnostic(&path, "new")], newer_at);
        }

        assert!(
            batch
                .take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY)
                .is_empty()
        );
        assert_eq!(
            batch.next_due_after(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY),
            Some(Duration::from_millis(49))
        );

        let entries = batch.take_due(
            newer_at + LSP_DIAGNOSTIC_BATCH_DELAY,
            LSP_DIAGNOSTIC_BATCH_DELAY,
        );

        assert_eq!(entries.len(), PENDING_LSP_DIAGNOSTIC_PATH_CAPACITY);
        assert!(!entries.iter().any(|(path, _, _)| path == &old_path));
    }

    #[test]
    fn pending_lsp_diagnostics_clear_drops_queued_updates() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");
        batch.queue(path.clone(), None, vec![diagnostic(&path, "first")], now);

        batch.clear();

        assert!(
            batch
                .take_due(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY,)
                .is_empty()
        );
    }

    #[test]
    fn pending_lsp_diagnostics_reports_next_due_delay() {
        let mut batch = PendingLspDiagnosticsBatch::default();
        let now = Instant::now();
        let path = PathBuf::from("src/main.rs");

        assert_eq!(batch.next_due_after(now, LSP_DIAGNOSTIC_BATCH_DELAY), None);

        batch.queue(path.clone(), None, vec![diagnostic(&path, "first")], now);
        assert_eq!(
            batch.next_due_after(now, LSP_DIAGNOSTIC_BATCH_DELAY),
            Some(LSP_DIAGNOSTIC_BATCH_DELAY)
        );
        assert_eq!(
            batch.next_due_after(now + LSP_DIAGNOSTIC_BATCH_DELAY, LSP_DIAGNOSTIC_BATCH_DELAY),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn lsp_diagnostics_for_open_buffer_filter_invalid_lines_and_columns() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());
        let path = PathBuf::from("src/main.rs");

        let diagnostics = valid_lsp_diagnostics_for_buffer(
            &buffer,
            vec![
                positioned_diagnostic(&path, 1, 1, 0..5, "valid"),
                positioned_diagnostic(&path, 2, 5, 4..5, "line-end"),
                positioned_diagnostic(&path, 2, 6, 5..6, "past-line"),
                positioned_diagnostic(&path, 3, 1, 0..1, "missing-line"),
            ],
        );

        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>(),
            vec!["valid", "line-end"]
        );
    }

    #[test]
    fn lsp_diagnostics_for_open_buffer_clamp_ranges_to_line_content() {
        let buffer = TextBuffer::from_text(1, None, "alpha".to_owned());
        let path = PathBuf::from("src/main.rs");

        let diagnostics = valid_lsp_diagnostics_for_buffer(
            &buffer,
            vec![positioned_diagnostic(&path, 1, 3, 2..99, "wide range")],
        );

        assert_eq!(diagnostics[0].column, 3);
        assert_eq!(diagnostics[0].char_range, 2..5);
    }

    #[test]
    fn lsp_diagnostics_for_open_buffer_keep_stale_reversed_ranges_monotonic() {
        let buffer = TextBuffer::from_text(1, None, "alpha".to_owned());
        let path = PathBuf::from("src/main.rs");

        let diagnostics = valid_lsp_diagnostics_for_buffer(
            &buffer,
            vec![
                positioned_diagnostic(&path, 1, 3, 0..1, "reversed-mid-line"),
                positioned_diagnostic(&path, 1, 6, 0..1, "reversed-at-eol"),
            ],
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].column, 3);
        assert_eq!(diagnostics[0].char_range, 2..3);
        assert_eq!(diagnostics[1].column, 6);
        assert_eq!(diagnostics[1].char_range, 5..5);
    }

    #[test]
    fn lsp_diagnostics_for_open_buffer_convert_utf16_ranges_to_char_ranges() {
        let buffer = TextBuffer::from_text(1, None, "😀alpha".to_owned());
        let path = PathBuf::from("src/main.rs");

        let diagnostics = valid_lsp_diagnostics_for_buffer(
            &buffer,
            vec![
                positioned_diagnostic(&path, 1, 3, 2..7, "identifier"),
                positioned_diagnostic(&path, 1, 2, 1..2, "inside-surrogate"),
            ],
        );

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].column, 2);
        assert_eq!(diagnostics[0].char_range, 1..6);

        assert_eq!(diagnostics[1].column, 1);
        assert_eq!(diagnostics[1].char_range, 0..1);
    }

    #[test]
    fn lsp_diagnostics_for_open_buffer_clamp_ends_inside_surrogates_upward() {
        let buffer = TextBuffer::from_text(1, None, "a😀b".to_owned());
        let path = PathBuf::from("src/main.rs");

        let diagnostics = valid_lsp_diagnostics_for_buffer(
            &buffer,
            vec![positioned_diagnostic(
                &path,
                1,
                1,
                0..2,
                "end-inside-surrogate",
            )],
        );

        assert_eq!(diagnostics.len(), 1);

        assert_eq!(diagnostics[0].column, 1);
        assert_eq!(diagnostics[0].char_range, 0..2);
    }

    #[test]
    fn lsp_diagnostics_for_open_buffer_clamp_multi_line_sentinel_ranges_to_line_end() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta".to_owned());
        let path = PathBuf::from("src/main.rs");

        let diagnostics = valid_lsp_diagnostics_for_buffer(
            &buffer,
            vec![positioned_diagnostic(
                &path,
                1,
                3,
                2..usize::MAX,
                "multi line",
            )],
        );

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].column, 3);
        assert_eq!(diagnostics[0].char_range, 2..5);
    }
}

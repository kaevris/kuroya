use crate::{
    lsp_labels::symbol_kind_label,
    path_display::{DISPLAY_PATH_LABEL_MAX_CHARS, sanitized_display_label_cow},
    ui_state::{clamp_selection, selected_row_scroll_offset},
};
use eframe::egui::{Context, Id, ScrollArea, Ui};
use kuroya_core::LspWorkspaceSymbol;
use std::{
    borrow::Cow,
    collections::{VecDeque, hash_map::DefaultHasher},
    fmt::Write as _,
    hash::{Hash, Hasher},
    ops::Range,
    path::Path,
    sync::{Arc, OnceLock},
};

pub(super) const WORKSPACE_SYMBOL_ROW_HEIGHT: f32 = 24.0;
/// Results visible before the list starts scrolling.
const WORKSPACE_SYMBOL_MAX_VISIBLE_ROWS: usize = 12;
const WORKSPACE_SYMBOL_DISPLAY_CACHE_ID: &str = "kuroya.workspace_symbol_results.display_cache";
const WORKSPACE_SYMBOL_DISPLAY_CACHE_MAX_ROWS: usize = 128;
const WORKSPACE_SYMBOL_NAME_MAX_CHARS: usize = 160;
const WORKSPACE_SYMBOL_DETAIL_MAX_CHARS: usize = 240;
const WORKSPACE_SYMBOL_TOOLTIP_NAME_MAX_CHARS: usize = 320;
const WORKSPACE_SYMBOL_TOOLTIP_DETAIL_MAX_CHARS: usize = 480;
const WORKSPACE_SYMBOL_TOOLTIP_PATH_MAX_CHARS: usize = 320;
const WORKSPACE_SYMBOL_INLINE_SANITIZE_BYTES: usize = 4096;

pub(super) fn render_workspace_symbol_results(
    ui: &mut Ui,
    symbols: &[LspWorkspaceSymbol],
    selected: &mut usize,
    scroll_to_selection: bool,
) -> Option<LspWorkspaceSymbol> {
    if !clamp_workspace_symbol_selection(selected, symbols.len()) {
        ui.add_space(24.0);
        ui.centered_and_justified(|ui| {
            ui.label("No workspace symbols");
        });
        return None;
    }

    let mut open = None;
    let symbol_count = symbols.len();
    let viewport_height = ui.available_height();
    // Cap the viewport so the popup hugs its results instead of stretching
    // to fill the window; more results simply scroll.
    let max_height =
        (symbol_count.min(WORKSPACE_SYMBOL_MAX_VISIBLE_ROWS) as f32) * WORKSPACE_SYMBOL_ROW_HEIGHT;
    let mut scroll_area = ScrollArea::vertical().max_height(max_height);
    if scroll_to_selection {
        scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
            *selected,
            symbol_count,
            WORKSPACE_SYMBOL_ROW_HEIGHT,
            viewport_height,
        ));
    }
    scroll_area.show_rows(ui, WORKSPACE_SYMBOL_ROW_HEIGHT, symbol_count, |ui, rows| {
        for index in workspace_symbol_display_row_targets(symbol_count, rows) {
            let Some(row) = PreparedWorkspaceSymbolRow::new(ui.ctx(), symbols, index) else {
                continue;
            };
            let mut response = ui.selectable_label(
                row.is_selected(*selected, symbol_count),
                row.display.label(),
            );
            if response.hovered() {
                response = response.on_hover_text(row.tooltip());
            }
            if response.clicked() {
                if let Some(target) = row.select_open_target(selected, symbols) {
                    open = Some(target);
                }
            }
        }
    });

    open
}

fn clamp_workspace_symbol_selection(selected: &mut usize, symbol_count: usize) -> bool {
    if symbol_count == 0 {
        *selected = 0;
        return false;
    }

    clamp_selection(selected, symbol_count);
    true
}

#[cfg(test)]
fn prepare_workspace_symbol_display_rows<'a>(
    ctx: &Context,
    symbols: &'a [LspWorkspaceSymbol],
    rows: Range<usize>,
) -> Vec<PreparedWorkspaceSymbolRow<'a>> {
    let rows = workspace_symbol_display_row_targets(symbols.len(), rows);
    let mut prepared = Vec::with_capacity(rows.len());
    rows.filter_map(|index| PreparedWorkspaceSymbolRow::new(ctx, symbols, index))
        .for_each(|row| prepared.push(row));
    prepared
}

fn workspace_symbol_display_row_targets(symbol_count: usize, rows: Range<usize>) -> Range<usize> {
    let start = rows.start.min(symbol_count);
    let end = rows.end.min(symbol_count);
    if end <= start {
        start..start
    } else {
        start..end
    }
}

struct PreparedWorkspaceSymbolRow<'a> {
    index: usize,
    identity: WorkspaceSymbolRowDisplayCacheKey,
    display: Arc<WorkspaceSymbolRowDisplay>,
    target: &'a LspWorkspaceSymbol,
}

impl<'a> PreparedWorkspaceSymbolRow<'a> {
    fn new(ctx: &Context, symbols: &'a [LspWorkspaceSymbol], index: usize) -> Option<Self> {
        let target = symbols.get(index)?;
        let identity = WorkspaceSymbolRowDisplayCacheKey::new(index, target);
        Some(Self {
            index,
            identity,
            display: cached_workspace_symbol_row_display(ctx, identity, target),
            target,
        })
    }

    fn is_selected(&self, selected: usize, symbol_count: usize) -> bool {
        selected < symbol_count && self.index == selected
    }

    fn select_open_target(
        &self,
        selected: &mut usize,
        symbols: &[LspWorkspaceSymbol],
    ) -> Option<LspWorkspaceSymbol> {
        let current = symbols.get(self.index)?;
        let current_identity = WorkspaceSymbolRowDisplayCacheKey::new(self.index, current);
        if current_identity != self.identity || current != self.target {
            return None;
        }

        *selected = self.index;
        Some(current.clone())
    }

    fn tooltip(&self) -> &str {
        self.display.tooltip(self.target)
    }
}

fn cached_workspace_symbol_row_display(
    ctx: &Context,
    key: WorkspaceSymbolRowDisplayCacheKey,
    symbol: &LspWorkspaceSymbol,
) -> Arc<WorkspaceSymbolRowDisplay> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<WorkspaceSymbolResultsDisplayCache>(Id::new(
            WORKSPACE_SYMBOL_DISPLAY_CACHE_ID,
        ))
        .display_for_symbol(key, symbol)
    })
}

#[cfg(test)]
fn workspace_symbol_label(symbol: &LspWorkspaceSymbol) -> String {
    WorkspaceSymbolRowDisplay::new(symbol).label().to_owned()
}

#[derive(Clone, Default)]
struct WorkspaceSymbolResultsDisplayCache {
    rows: VecDeque<WorkspaceSymbolRowDisplayCacheEntry>,
}

impl WorkspaceSymbolResultsDisplayCache {
    fn display_for_symbol(
        &mut self,
        key: WorkspaceSymbolRowDisplayCacheKey,
        symbol: &LspWorkspaceSymbol,
    ) -> Arc<WorkspaceSymbolRowDisplay> {
        if let Some(position) = self.rows.iter().position(|entry| entry.key == key) {
            let entry = self
                .rows
                .remove(position)
                .expect("workspace symbol display cache position should be valid");
            let display = Arc::clone(&entry.display);
            self.rows.push_back(entry);
            return display;
        }

        let display = Arc::new(WorkspaceSymbolRowDisplay::new(symbol));
        self.rows.push_back(WorkspaceSymbolRowDisplayCacheEntry {
            key,
            display: Arc::clone(&display),
        });
        while self.rows.len() > WORKSPACE_SYMBOL_DISPLAY_CACHE_MAX_ROWS {
            self.rows.pop_front();
        }
        display
    }
}

#[derive(Clone)]
struct WorkspaceSymbolRowDisplayCacheEntry {
    key: WorkspaceSymbolRowDisplayCacheKey,
    display: Arc<WorkspaceSymbolRowDisplay>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorkspaceSymbolRowDisplayCacheKey {
    index: usize,
    fingerprint: u64,
}

impl WorkspaceSymbolRowDisplayCacheKey {
    fn new(index: usize, symbol: &LspWorkspaceSymbol) -> Self {
        let mut hasher = DefaultHasher::new();
        symbol.name.hash(&mut hasher);
        symbol.detail.hash(&mut hasher);
        symbol.kind.hash(&mut hasher);
        symbol.path.hash(&mut hasher);
        symbol.line.hash(&mut hasher);
        symbol.column.hash(&mut hasher);
        symbol.end_line.hash(&mut hasher);
        symbol.end_column.hash(&mut hasher);

        Self {
            index,
            fingerprint: hasher.finish(),
        }
    }
}

struct WorkspaceSymbolRowDisplay {
    label: String,
    tooltip: OnceLock<String>,
}

impl WorkspaceSymbolRowDisplay {
    fn new(symbol: &LspWorkspaceSymbol) -> Self {
        let kind = symbol_kind_label(symbol.kind);
        let name = workspace_symbol_display_label_cow(
            &symbol.name,
            WORKSPACE_SYMBOL_NAME_MAX_CHARS,
            "<unnamed>",
        );
        let detail = symbol
            .detail
            .as_deref()
            .map(|detail| {
                workspace_symbol_display_label_cow(detail, WORKSPACE_SYMBOL_DETAIL_MAX_CHARS, "")
            })
            .filter(|detail| !detail.is_empty());
        let path = workspace_symbol_path_label(&symbol.path);
        Self {
            label: workspace_symbol_row_label(
                kind,
                name.as_ref(),
                detail.as_deref(),
                path.as_ref(),
                symbol.line,
                symbol.column,
            ),
            tooltip: OnceLock::new(),
        }
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn tooltip(&self, symbol: &LspWorkspaceSymbol) -> &str {
        self.tooltip
            .get_or_init(|| workspace_symbol_tooltip(symbol))
            .as_str()
    }
}

fn workspace_symbol_tooltip(symbol: &LspWorkspaceSymbol) -> String {
    let kind = symbol_kind_label(symbol.kind);
    let name = workspace_symbol_display_label_cow(
        &symbol.name,
        WORKSPACE_SYMBOL_TOOLTIP_NAME_MAX_CHARS,
        "<unnamed>",
    );
    let detail = symbol
        .detail
        .as_deref()
        .map(|detail| {
            workspace_symbol_display_label_cow(
                detail,
                WORKSPACE_SYMBOL_TOOLTIP_DETAIL_MAX_CHARS,
                "",
            )
        })
        .filter(|detail| !detail.is_empty());
    let path = workspace_symbol_tooltip_path_label(&symbol.path);

    workspace_symbol_row_tooltip(
        kind,
        name.as_ref(),
        detail.as_deref(),
        path.as_ref(),
        symbol.line,
        symbol.column,
        symbol.end_line,
        symbol.end_column,
    )
}

fn workspace_symbol_row_label(
    kind: &str,
    name: &str,
    detail: Option<&str>,
    path: &str,
    line: usize,
    column: usize,
) -> String {
    const LOCATION_ESTIMATE_CHARS: usize = 24;

    let detail_len = detail.map_or(0, |detail| detail.len() + 2);

    let mut label = String::with_capacity(
        kind.len() + 2 + name.len() + detail_len + 2 + path.len() + LOCATION_ESTIMATE_CHARS,
    );
    label.push_str(kind);
    label.push_str("  ");
    label.push_str(name);
    if let Some(detail) = detail {
        label.push_str("  ");
        label.push_str(detail);
    }
    let _ = write!(label, "  {path}:{line}:{column}");
    label
}

fn workspace_symbol_row_tooltip(
    kind: &str,
    name: &str,
    detail: Option<&str>,
    path: &str,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
) -> String {
    const LOCATION_ESTIMATE_CHARS: usize = 48;

    let detail_len = detail.map_or(0, |detail| detail.len() + 1);
    let mut tooltip = String::with_capacity(
        kind.len() + 2 + name.len() + detail_len + path.len() + LOCATION_ESTIMATE_CHARS,
    );
    tooltip.push_str(kind);
    tooltip.push_str("  ");
    tooltip.push_str(name);
    if let Some(detail) = detail {
        tooltip.push('\n');
        tooltip.push_str(detail);
    }
    let _ = write!(tooltip, "\n{path}:{line}:{column}");
    if end_line != line || end_column != column {
        let _ = write!(tooltip, "-{end_line}:{end_column}");
    }
    tooltip
}

fn workspace_symbol_path_label(path: &Path) -> Cow<'_, str> {
    if path.as_os_str().is_empty() {
        return Cow::Borrowed(".");
    }

    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
        return workspace_symbol_display_label_cow(name, DISPLAY_PATH_LABEL_MAX_CHARS, ".");
    }

    Cow::Owned(workspace_symbol_display_label_owned(
        path.display().to_string(),
        DISPLAY_PATH_LABEL_MAX_CHARS,
        ".",
    ))
}

fn workspace_symbol_tooltip_path_label(path: &Path) -> Cow<'_, str> {
    if path.as_os_str().is_empty() {
        return Cow::Borrowed(".");
    }

    if let Some(path) = path.to_str() {
        return workspace_symbol_display_label_cow(
            path,
            WORKSPACE_SYMBOL_TOOLTIP_PATH_MAX_CHARS,
            ".",
        );
    }

    Cow::Owned(workspace_symbol_display_label_owned(
        path.display().to_string(),
        WORKSPACE_SYMBOL_TOOLTIP_PATH_MAX_CHARS,
        ".",
    ))
}

fn workspace_symbol_display_label_cow<'a>(
    value: &'a str,
    max_chars: usize,
    fallback: &str,
) -> Cow<'a, str> {
    if value.len() <= WORKSPACE_SYMBOL_INLINE_SANITIZE_BYTES {
        return sanitized_display_label_cow(value, max_chars, fallback);
    }

    Cow::Owned(bounded_workspace_symbol_display_label(
        value, max_chars, fallback,
    ))
}

fn workspace_symbol_display_label_owned(value: String, max_chars: usize, fallback: &str) -> String {
    match workspace_symbol_display_label_cow(&value, max_chars, fallback) {
        Cow::Borrowed(label) if label.as_ptr() == value.as_ptr() && label.len() == value.len() => {
            value
        }
        Cow::Borrowed(label) => label.to_owned(),
        Cow::Owned(label) => label,
    }
}

fn bounded_workspace_symbol_display_label(value: &str, max_chars: usize, fallback: &str) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let mut label = BoundedDisplayLabel::new(max_chars);
    let mut pending_space = false;
    let mut emitted_any = false;
    let mut last_output_space = false;
    for ch in value.chars() {
        if is_workspace_symbol_hidden_format_control(ch) {
            continue;
        }

        if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') {
            pending_space = emitted_any;
            continue;
        }

        if pending_space && !last_output_space {
            label.push(' ');
        }
        pending_space = false;

        label.push(ch);
        emitted_any = true;
        last_output_space = ch == ' ';
    }

    label.finish(fallback)
}

struct BoundedDisplayLabel {
    max_chars: usize,
    head: String,
    head_chars: usize,
    tail: VecDeque<char>,
    tail_capacity: usize,
    chars: usize,
    pending_trailing_spaces: usize,
}

impl BoundedDisplayLabel {
    fn new(max_chars: usize) -> Self {
        let tail_capacity = max_chars.saturating_sub(3).div_ceil(2);
        Self {
            max_chars,
            head: String::with_capacity(max_chars),
            head_chars: 0,
            tail: VecDeque::with_capacity(tail_capacity),
            tail_capacity,
            chars: 0,
            pending_trailing_spaces: 0,
        }
    }

    fn push(&mut self, ch: char) {
        if self.chars == 0 && ch.is_whitespace() {
            return;
        }

        if ch.is_whitespace() {
            self.pending_trailing_spaces = self.pending_trailing_spaces.saturating_add(1);
            return;
        }

        self.flush_pending_spaces();
        self.push_trimmed(ch);
    }

    fn flush_pending_spaces(&mut self) {
        if self.pending_trailing_spaces == 0 {
            return;
        }

        self.push_trimmed_repeated(' ', self.pending_trailing_spaces);
        self.pending_trailing_spaces = 0;
    }

    fn push_trimmed(&mut self, ch: char) {
        if self.head_chars < self.max_chars {
            self.head.push(ch);
            self.head_chars += 1;
        }

        if self.tail_capacity > 0 {
            if self.tail.len() == self.tail_capacity {
                self.tail.pop_front();
            }
            self.tail.push_back(ch);
        }

        self.chars += 1;
    }

    fn push_trimmed_repeated(&mut self, ch: char, count: usize) {
        let head_remaining = self.max_chars.saturating_sub(self.head_chars);
        let head_chars = head_remaining.min(count);
        for _ in 0..head_chars {
            self.head.push(ch);
        }
        self.head_chars += head_chars;

        if self.tail_capacity > 0 {
            if count >= self.tail_capacity {
                self.tail.clear();
                for _ in 0..self.tail_capacity {
                    self.tail.push_back(ch);
                }
            } else {
                for _ in 0..count {
                    if self.tail.len() == self.tail_capacity {
                        self.tail.pop_front();
                    }
                    self.tail.push_back(ch);
                }
            }
        }

        self.chars += count;
    }

    fn finish(self, fallback: &str) -> String {
        if self.chars == 0 {
            return sanitized_display_label_cow(fallback, self.max_chars, "").into_owned();
        }

        if self.chars <= self.max_chars {
            return self.head;
        }

        if self.max_chars <= 3 {
            return ".".repeat(self.max_chars);
        }

        let keep = self.max_chars - 3;
        let head_chars = keep / 2;
        let tail_chars = keep - head_chars;
        let mut label = String::with_capacity(self.max_chars);
        label.extend(self.head.chars().take(head_chars));
        label.push_str("...");
        label.extend(
            self.tail
                .iter()
                .skip(self.tail.len().saturating_sub(tail_chars))
                .copied(),
        );
        label
    }
}

fn is_workspace_symbol_hidden_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
    )
}

#[cfg(test)]
mod tests {
    use super::{
        WORKSPACE_SYMBOL_DETAIL_MAX_CHARS, WORKSPACE_SYMBOL_DISPLAY_CACHE_MAX_ROWS,
        WORKSPACE_SYMBOL_INLINE_SANITIZE_BYTES, WORKSPACE_SYMBOL_NAME_MAX_CHARS,
        WORKSPACE_SYMBOL_TOOLTIP_DETAIL_MAX_CHARS, WORKSPACE_SYMBOL_TOOLTIP_NAME_MAX_CHARS,
        WORKSPACE_SYMBOL_TOOLTIP_PATH_MAX_CHARS, WorkspaceSymbolResultsDisplayCache,
        WorkspaceSymbolRowDisplay, WorkspaceSymbolRowDisplayCacheKey,
        clamp_workspace_symbol_selection, prepare_workspace_symbol_display_rows,
        workspace_symbol_label, workspace_symbol_tooltip,
    };
    use crate::path_display::{DISPLAY_PATH_LABEL_MAX_CHARS, display_path_label_cow};
    use eframe::egui::Context;
    use kuroya_core::LspWorkspaceSymbol;
    use std::{path::PathBuf, sync::Arc};

    #[test]
    fn workspace_symbol_selection_resets_when_results_are_empty() {
        let mut selected = 42;

        assert!(!clamp_workspace_symbol_selection(&mut selected, 0));
        assert_eq!(selected, 0);
    }

    #[test]
    fn workspace_symbol_selection_clamps_to_last_result() {
        let mut selected = 42;

        assert!(clamp_workspace_symbol_selection(&mut selected, 3));
        assert_eq!(selected, 2);
    }

    #[test]
    fn workspace_symbol_label_includes_kind_detail_path_and_location() {
        let symbol = LspWorkspaceSymbol {
            name: "AppState".to_owned(),
            detail: Some("struct".to_owned()),
            kind: 23,
            path: PathBuf::from("src/app_state.rs"),
            line: 12,
            column: 1,
            end_line: 12,
            end_column: 9,
        };

        assert_eq!(
            workspace_symbol_label(&symbol),
            "struct  AppState  struct  app_state.rs:12:1"
        );
    }

    #[test]
    fn workspace_symbol_label_sanitizes_display_fields() {
        let symbol = LspWorkspaceSymbol {
            name: "  App\n\t\u{202e}State  ".to_owned(),
            detail: Some("  struct\r\n\u{2066}value\t ".to_owned()),
            kind: 23,
            path: PathBuf::from("src").join("bad\n\u{202e}tail.rs"),
            line: 12,
            column: 1,
            end_line: 12,
            end_column: 9,
        };

        let label = workspace_symbol_label(&symbol);

        assert_eq!(label, "struct  App State  struct value  bad tail.rs:12:1");
        assert!(!label.chars().any(|ch| ch.is_control()));
        assert!(!label.contains('\u{202e}'));
        assert!(!label.contains('\u{2066}'));
    }

    #[test]
    fn workspace_symbol_row_display_reuses_built_label() {
        let symbol = LspWorkspaceSymbol {
            name: "  App\n\t\u{202e}State  ".to_owned(),
            detail: Some("  struct\r\n\u{2066}value\t ".to_owned()),
            kind: 23,
            path: PathBuf::from("src").join("bad\n\u{202e}tail.rs"),
            line: 12,
            column: 1,
            end_line: 12,
            end_column: 9,
        };

        let display = WorkspaceSymbolRowDisplay::new(&symbol);
        let label = display.label();

        assert_eq!(label, "struct  App State  struct value  bad tail.rs:12:1");
        assert_eq!(display.label().as_ptr(), label.as_ptr());
    }

    #[test]
    fn workspace_symbol_row_display_reuses_built_tooltip() {
        let symbol = LspWorkspaceSymbol {
            name: "AppState".to_owned(),
            detail: Some("struct".to_owned()),
            kind: 23,
            path: PathBuf::from("src/app_state.rs"),
            line: 12,
            column: 1,
            end_line: 12,
            end_column: 9,
        };
        let display = WorkspaceSymbolRowDisplay::new(&symbol);

        let tooltip = display.tooltip(&symbol);

        assert_eq!(
            tooltip,
            "struct  AppState\nstruct\nsrc/app_state.rs:12:1-12:9"
        );
        assert_eq!(display.tooltip(&symbol).as_ptr(), tooltip.as_ptr());
    }

    #[test]
    fn workspace_symbol_prepared_rows_skip_stale_indexes() {
        let ctx = Context::default();
        let symbols = vec![LspWorkspaceSymbol {
            name: "AppState".to_owned(),
            detail: None,
            kind: 12,
            path: PathBuf::from("src/main.rs"),
            line: 3,
            column: 5,
            end_line: 3,
            end_column: 8,
        }];

        let rows = prepare_workspace_symbol_display_rows(&ctx, &symbols, 0..3);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].index, 0);
        assert!(rows[0].is_selected(0, symbols.len()));
        assert!(!rows[0].is_selected(1, symbols.len()));

        let mut selected = 99;
        assert!(rows[0].select_open_target(&mut selected, &[]).is_none());
        assert_eq!(selected, 99);
    }

    #[test]
    fn workspace_symbol_prepared_rows_bound_oversized_or_stale_ranges() {
        let ctx = Context::default();
        let symbols = vec![LspWorkspaceSymbol {
            name: "AppState".to_owned(),
            detail: None,
            kind: 12,
            path: PathBuf::from("src/main.rs"),
            line: 3,
            column: 5,
            end_line: 3,
            end_column: 8,
        }];

        let rows = prepare_workspace_symbol_display_rows(&ctx, &symbols, 0..usize::MAX);
        let stale_rows =
            prepare_workspace_symbol_display_rows(&ctx, &symbols, usize::MAX - 4..usize::MAX);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].index, 0);
        assert!(stale_rows.is_empty());
    }

    #[test]
    fn workspace_symbol_prepared_row_opens_raw_target_not_display_text() {
        let ctx = Context::default();
        let symbol = LspWorkspaceSymbol {
            name: "  App\n\t\u{202e}State  ".to_owned(),
            detail: Some("  detail\r\n\u{2066}value\t ".to_owned()),
            kind: 12,
            path: PathBuf::from("src").join("bad\n\u{202e}tail.rs"),
            line: 7,
            column: 11,
            end_line: 7,
            end_column: 19,
        };
        let symbols = vec![symbol.clone()];

        let rows = prepare_workspace_symbol_display_rows(&ctx, &symbols, 0..1);
        let mut selected = 42;
        let opened = rows[0]
            .select_open_target(&mut selected, &symbols)
            .expect("prepared row should open a live target");

        assert_eq!(
            rows[0].display.label(),
            "fn  App State  detail value  bad tail.rs:7:11"
        );
        assert_eq!(selected, 0);
        assert_eq!(&opened.name, &symbol.name);
        assert_eq!(&opened.detail, &symbol.detail);
        assert_eq!(opened.kind, symbol.kind);
        assert_eq!(&opened.path, &symbol.path);
        assert_eq!(opened.line, symbol.line);
        assert_eq!(opened.column, symbol.column);
        assert_eq!(opened.end_line, symbol.end_line);
        assert_eq!(opened.end_column, symbol.end_column);
    }

    #[test]
    fn workspace_symbol_prepared_row_rejects_changed_live_target_before_open() {
        let ctx = Context::default();
        let symbol = LspWorkspaceSymbol {
            name: "AppState".to_owned(),
            detail: None,
            kind: 12,
            path: PathBuf::from("src/main.rs"),
            line: 7,
            column: 11,
            end_line: 7,
            end_column: 19,
        };
        let symbols = vec![symbol];
        let rows = prepare_workspace_symbol_display_rows(&ctx, &symbols, 0..1);
        let changed_symbols = vec![LspWorkspaceSymbol {
            name: "OtherState".to_owned(),
            detail: None,
            kind: 12,
            path: PathBuf::from("src/other.rs"),
            line: 8,
            column: 1,
            end_line: 8,
            end_column: 10,
        }];

        let mut selected = 42;

        assert!(
            rows[0]
                .select_open_target(&mut selected, &changed_symbols)
                .is_none()
        );
        assert_eq!(selected, 42);
    }

    #[test]
    fn workspace_symbol_display_cache_reuses_unchanged_row_display() {
        let symbol = LspWorkspaceSymbol {
            name: "AppState".to_owned(),
            detail: None,
            kind: 12,
            path: PathBuf::from("src/main.rs"),
            line: 3,
            column: 5,
            end_line: 3,
            end_column: 8,
        };
        let mut cache = WorkspaceSymbolResultsDisplayCache::default();
        let key = WorkspaceSymbolRowDisplayCacheKey::new(4, &symbol);

        let first = cache.display_for_symbol(key, &symbol);
        let second = cache.display_for_symbol(key, &symbol);

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.label(), "fn  AppState  main.rs:3:5");
    }

    #[test]
    fn workspace_symbol_display_cache_refreshes_changed_raw_row() {
        let symbol = LspWorkspaceSymbol {
            name: "AppState".to_owned(),
            detail: None,
            kind: 12,
            path: PathBuf::from("src/main.rs"),
            line: 3,
            column: 5,
            end_line: 3,
            end_column: 8,
        };
        let mut cache = WorkspaceSymbolResultsDisplayCache::default();

        let first =
            cache.display_for_symbol(WorkspaceSymbolRowDisplayCacheKey::new(4, &symbol), &symbol);

        let mut changed = symbol.clone();
        changed.name = "OtherState".to_owned();
        changed.path = PathBuf::from("src/other.rs");
        changed.line = 9;
        let second = cache.display_for_symbol(
            WorkspaceSymbolRowDisplayCacheKey::new(4, &changed),
            &changed,
        );

        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(first.label(), "fn  AppState  main.rs:3:5");
        assert_eq!(second.label(), "fn  OtherState  other.rs:9:5");
        assert_eq!(symbol.name, "AppState");
    }

    #[test]
    fn workspace_symbol_display_cache_is_bounded_to_recent_rows() {
        let mut cache = WorkspaceSymbolResultsDisplayCache::default();

        for idx in 0..WORKSPACE_SYMBOL_DISPLAY_CACHE_MAX_ROWS + 2 {
            let symbol = LspWorkspaceSymbol {
                name: format!("Symbol{idx}"),
                detail: None,
                kind: 12,
                path: PathBuf::from(format!("src/{idx}.rs")),
                line: idx,
                column: 1,
                end_line: idx,
                end_column: 8,
            };
            cache.display_for_symbol(
                WorkspaceSymbolRowDisplayCacheKey::new(idx, &symbol),
                &symbol,
            );
        }

        assert_eq!(cache.rows.len(), WORKSPACE_SYMBOL_DISPLAY_CACHE_MAX_ROWS);
        assert_eq!(cache.rows.front().unwrap().key.index, 2);
        assert_eq!(
            cache.rows.back().unwrap().key.index,
            WORKSPACE_SYMBOL_DISPLAY_CACHE_MAX_ROWS + 1
        );
    }

    #[test]
    fn workspace_symbol_label_bounds_display_fields() {
        let symbol = LspWorkspaceSymbol {
            name: "n".repeat(WORKSPACE_SYMBOL_NAME_MAX_CHARS * 2),
            detail: Some("d".repeat(WORKSPACE_SYMBOL_DETAIL_MAX_CHARS * 2)),
            kind: 12,
            path: PathBuf::from(format!(
                "{}.rs",
                "p".repeat(DISPLAY_PATH_LABEL_MAX_CHARS * 2)
            )),
            line: 3,
            column: 5,
            end_line: 3,
            end_column: 8,
        };

        let label = workspace_symbol_label(&symbol);
        let expected = format!(
            "fn  {}  {}  {}:3:5",
            crate::path_display::sanitized_display_label_cow(
                &symbol.name,
                WORKSPACE_SYMBOL_NAME_MAX_CHARS,
                "<unnamed>"
            )
            .as_ref(),
            crate::path_display::sanitized_display_label_cow(
                symbol.detail.as_deref().unwrap(),
                WORKSPACE_SYMBOL_DETAIL_MAX_CHARS,
                ""
            )
            .as_ref(),
            display_path_label_cow(&symbol.path).as_ref()
        );

        assert_eq!(label, expected);
        assert_eq!(
            label
                .strip_prefix("fn  ")
                .and_then(|body| body.split_once("  "))
                .map(|(name, _)| name.chars().count()),
            Some(WORKSPACE_SYMBOL_NAME_MAX_CHARS)
        );
        assert!(label.contains("..."));
    }

    #[test]
    fn workspace_symbol_label_names_empty_fields() {
        let symbol = LspWorkspaceSymbol {
            name: "\n\t\u{202e}".to_owned(),
            detail: Some("\r\n\u{2066}".to_owned()),
            kind: 12,
            path: PathBuf::from("src/main.rs"),
            line: 3,
            column: 5,
            end_line: 3,
            end_column: 8,
        };

        assert_eq!(
            workspace_symbol_label(&symbol),
            "fn  <unnamed>  main.rs:3:5"
        );
    }

    #[test]
    fn workspace_symbol_tooltip_sanitizes_and_bounds_display_fields() {
        let unsafe_chunk = format!(
            "row\n\t\u{202e}{}\u{2066}",
            "unsafe ".repeat(WORKSPACE_SYMBOL_INLINE_SANITIZE_BYTES / 4)
        );
        let symbol = LspWorkspaceSymbol {
            name: format!("Name {unsafe_chunk} tail"),
            detail: Some(format!("Detail {unsafe_chunk} tail")),
            kind: 12,
            path: PathBuf::from("workspace")
                .join("src")
                .join(format!("path {unsafe_chunk} tail.rs")),
            line: 123,
            column: 456,
            end_line: 124,
            end_column: 9,
        };

        let tooltip = workspace_symbol_tooltip(&symbol);
        let mut lines = tooltip.lines();
        let header = lines
            .next()
            .expect("workspace symbol tooltip should include a header");
        let detail = lines
            .next()
            .expect("workspace symbol tooltip should include detail");
        let location = lines
            .next()
            .expect("workspace symbol tooltip should include location");
        let name = header
            .strip_prefix("fn  ")
            .expect("workspace symbol tooltip should include kind and name");
        let path_label = super::workspace_symbol_tooltip_path_label(&symbol.path);

        assert!(name.chars().count() <= WORKSPACE_SYMBOL_TOOLTIP_NAME_MAX_CHARS);
        assert!(detail.chars().count() <= WORKSPACE_SYMBOL_TOOLTIP_DETAIL_MAX_CHARS);
        assert!(path_label.chars().count() <= WORKSPACE_SYMBOL_TOOLTIP_PATH_MAX_CHARS);
        assert_eq!(location, format!("{path_label}:123:456-124:9"));
        assert!(tooltip.contains("..."));
        assert!(!tooltip.chars().any(|ch| {
            ch.is_control() && ch != '\n'
                || matches!(ch, '\u{2028}' | '\u{2029}' | '\u{202e}' | '\u{2066}')
        }));
        assert!(
            (tooltip.chars().count())
                <= "fn".chars().count()
                    + 2
                    + WORKSPACE_SYMBOL_TOOLTIP_NAME_MAX_CHARS
                    + 1
                    + WORKSPACE_SYMBOL_TOOLTIP_DETAIL_MAX_CHARS
                    + 1
                    + WORKSPACE_SYMBOL_TOOLTIP_PATH_MAX_CHARS
                    + ":123:456-124:9".chars().count()
        );
    }

    #[test]
    fn workspace_symbol_label_hardens_huge_unsafe_display_fields() {
        let unsafe_chunk = format!(
            "row\n\t\u{200b}\u{200c}\u{200d}\u{202e}{}\u{2060}\u{2066}\u{feff}",
            "unsafe ".repeat(WORKSPACE_SYMBOL_INLINE_SANITIZE_BYTES / 4)
        );
        let symbol = LspWorkspaceSymbol {
            name: format!("Name {unsafe_chunk} tail"),
            detail: Some(format!("Detail {unsafe_chunk} tail")),
            kind: 12,
            path: PathBuf::from("src").join(format!("path {unsafe_chunk} tail.rs")),
            line: 123,
            column: 456,
            end_line: 123,
            end_column: 460,
        };

        let display = WorkspaceSymbolRowDisplay::new(&symbol);
        let label = display.label();
        let (name, rest) = label
            .strip_prefix("fn  ")
            .and_then(|body| body.split_once("  "))
            .expect("workspace symbol label should include name");
        let (detail, location) = rest
            .split_once("  ")
            .expect("workspace symbol label should include detail");
        let path_label = super::workspace_symbol_path_label(&symbol.path);
        let location_suffix = format!("{path_label}:123:456");

        assert!(name.chars().count() <= WORKSPACE_SYMBOL_NAME_MAX_CHARS);
        assert!(detail.chars().count() <= WORKSPACE_SYMBOL_DETAIL_MAX_CHARS);
        assert!(path_label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
        assert!(location.ends_with(&location_suffix));
        assert!(label.contains("..."));
        assert!(!label.chars().any(|ch| {
            ch.is_control()
                || matches!(
                    ch,
                    '\u{200b}'
                        | '\u{200c}'
                        | '\u{200d}'
                        | '\u{2028}'
                        | '\u{2029}'
                        | '\u{202e}'
                        | '\u{2060}'
                        | '\u{2066}'
                        | '\u{feff}'
                )
        }));
        assert!(
            label.chars().count()
                <= "fn".chars().count()
                    + 2
                    + WORKSPACE_SYMBOL_NAME_MAX_CHARS
                    + 2
                    + WORKSPACE_SYMBOL_DETAIL_MAX_CHARS
                    + 2
                    + DISPLAY_PATH_LABEL_MAX_CHARS
                    + ":123:456".chars().count()
        );
    }

    #[test]
    fn workspace_symbol_tooltip_hardens_huge_zero_width_display_fields() {
        let hidden = "\u{200b}\u{200c}\u{200d}\u{2060}\u{feff}";
        let unsafe_chunk = format!(
            "row{hidden}\n{}tail{hidden}",
            "unsafe ".repeat(WORKSPACE_SYMBOL_INLINE_SANITIZE_BYTES / 4)
        );
        let symbol = LspWorkspaceSymbol {
            name: format!("Name {unsafe_chunk}"),
            detail: Some(format!("Detail {unsafe_chunk}")),
            kind: 12,
            path: PathBuf::from("workspace")
                .join("src")
                .join(format!("path {unsafe_chunk}.rs")),
            line: 7,
            column: 11,
            end_line: 7,
            end_column: 19,
        };

        let display = WorkspaceSymbolRowDisplay::new(&symbol);
        let tooltip = workspace_symbol_tooltip(&symbol);

        assert!(!display.label().chars().any(is_hidden_test_control));
        assert!(!tooltip.chars().any(is_hidden_test_control));
        assert!(display.label().contains("..."));
        assert!(tooltip.contains("..."));
    }

    fn is_hidden_test_control(ch: char) -> bool {
        matches!(
            ch,
            '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
        )
    }
}

use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button},
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
};
use eframe::egui::{self, Align, Context, Key, RichText, ScrollArea};
use kuroya_core::LspReference;
use std::{
    borrow::Cow,
    fmt::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

const REFERENCES_ROW_HEIGHT: f32 = 24.0;
const REFERENCES_PREPARED_CACHE_ID: &str = "lsp_reference_popup.prepared";
pub(crate) const LSP_POPUP_LABEL_MAX_CHARS: usize = 240;
const LSP_POPUP_COMPONENT_MAX_CHARS: usize = 120;
const LSP_POPUP_PATH_MAX_CHARS: usize = 160;
const LSP_POPUP_INPUT_SCAN_MAX_CHARS: usize = 4096;
const LSP_POPUP_INPUT_SCAN_MAX_BYTES: usize = 16 * 1024;
const LSP_POPUP_TRUNCATION_MARKER: &str = "...";
const LSP_POPUP_TRUNCATION_MARKER_CHARS: usize = LSP_POPUP_TRUNCATION_MARKER.len();

impl KuroyaApp {
    pub(crate) fn render_references_popup(&mut self, ctx: &Context) {
        let mut close = false;
        let mut open_index = None;
        let prepared = reference_popup_prepared(
            ctx,
            self.references_path.as_deref(),
            self.references_line,
            self.references_column,
            &self.references,
        );
        let row_count = prepared.row_count();
        clamp_selection(&mut self.references_selected, row_count);

        egui::Window::new("References")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 156.0,
            ))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 132.0])
            .default_size([620.0, 320.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(prepared.target_label())
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if popup_button(ui, "Close", PopupButtonKind::Secondary).clicked() {
                            close = true;
                        }
                    });
                });

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
                let viewport_height = ui.available_height();
                let selection_changed = ui.input(|input| {
                    handle_list_navigation_keys(
                        input,
                        &mut self.references_selected,
                        row_count,
                        selection_page_step(REFERENCES_ROW_HEIGHT, viewport_height),
                    )
                });
                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    open_index = reference_popup_open_index(self.references_selected, row_count);
                }

                ui.separator();
                if row_count == 0 {
                    ui.add_space(24.0);
                    ui.centered_and_justified(|ui| {
                        ui.label("No references");
                    });
                } else {
                    let mut scroll_area = ScrollArea::vertical();
                    if selection_changed {
                        scroll_area =
                            scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                                self.references_selected,
                                row_count,
                                REFERENCES_ROW_HEIGHT,
                                viewport_height,
                            ));
                    }
                    scroll_area.show_rows(ui, REFERENCES_ROW_HEIGHT, row_count, |ui, rows| {
                        for idx in rows {
                            let Some(row) = prepared.row(idx) else {
                                continue;
                            };
                            if ui
                                .selectable_label(idx == self.references_selected, row.label())
                                .clicked()
                            {
                                self.references_selected = idx;
                                open_index = reference_popup_open_index(idx, row_count);
                            }
                        }
                    });
                }
            });

        if close {
            clear_reference_popup_prepared(ctx);
            self.references_open = false;
            self.references.clear();
            self.status = "Closed references".to_owned();
        } else if let Some(reference) = open_index
            .and_then(|idx| reference_popup_take_open_target(&mut self.references, &prepared, idx))
        {
            clear_reference_popup_prepared(ctx);
            self.open_reference(reference);
        }
    }
}

#[derive(Clone)]
struct PreparedReferencePopup {
    target: Arc<PreparedReferencePopupTarget>,
    rows: Arc<[PreparedReferencePopupRow]>,
}

impl PreparedReferencePopup {
    fn reuse_or_build(
        previous: Option<Self>,
        target_path: Option<&Path>,
        target_line: usize,
        target_column: usize,
        references: &[LspReference],
    ) -> (PreparedReferencePopup, bool) {
        if let Some(cache) = previous {
            if cache.matches(target_path, target_line, target_column, references) {
                return (cache, false);
            }
        }

        (
            Self::build(target_path, target_line, target_column, references),
            true,
        )
    }

    fn build(
        target_path: Option<&Path>,
        target_line: usize,
        target_column: usize,
        references: &[LspReference],
    ) -> Self {
        let mut rows = Vec::with_capacity(references.len());
        let mut label = String::new();
        for (source_index, reference) in references.iter().enumerate() {
            if reference_popup_reference_range_is_valid(reference) {
                rows.push(PreparedReferencePopupRow::from_reference(
                    source_index,
                    reference,
                    &mut label,
                ));
            }
        }
        Self {
            target: Arc::new(PreparedReferencePopupTarget::new(
                target_path,
                target_line,
                target_column,
            )),
            rows: rows.into(),
        }
    }

    fn matches(
        &self,
        target_path: Option<&Path>,
        target_line: usize,
        target_column: usize,
        references: &[LspReference],
    ) -> bool {
        self.target.matches(target_path, target_line, target_column)
            && prepared_reference_rows_match(&self.rows, references)
    }

    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn target_label(&self) -> &str {
        self.target.label.as_ref()
    }

    fn row(&self, idx: usize) -> Option<&PreparedReferencePopupRow> {
        self.rows.get(idx)
    }
}

impl Default for PreparedReferencePopup {
    fn default() -> Self {
        Self {
            target: Arc::new(PreparedReferencePopupTarget::new(None, 0, 0)),
            rows: Arc::from([]),
        }
    }
}

struct PreparedReferencePopupTarget {
    path: Option<PathBuf>,
    line: usize,
    column: usize,
    label: Arc<str>,
}

impl PreparedReferencePopupTarget {
    fn new(target_path: Option<&Path>, target_line: usize, target_column: usize) -> Self {
        Self {
            path: target_path.map(Path::to_path_buf),
            line: target_line,
            column: target_column,
            label: reference_popup_target_label(target_path, target_line, target_column),
        }
    }

    fn matches(
        &self,
        target_path: Option<&Path>,
        target_line: usize,
        target_column: usize,
    ) -> bool {
        self.path.as_deref() == target_path
            && self.line == target_line
            && self.column == target_column
    }
}

#[derive(Clone)]
struct PreparedReferencePopupRow {
    source_index: usize,
    target: LspReference,
    label: Arc<str>,
}

impl PreparedReferencePopupRow {
    fn from_reference(source_index: usize, reference: &LspReference, label: &mut String) -> Self {
        lsp_popup_location_label_into(label, &reference.path, reference.line, reference.column);
        Self {
            source_index,
            target: reference.clone(),
            label: Arc::<str>::from(label.as_str()),
        }
    }

    fn matches(&self, source_index: usize, reference: &LspReference) -> bool {
        self.source_index == source_index && &self.target == reference
    }

    fn label(&self) -> &str {
        self.label.as_ref()
    }
}

fn reference_popup_prepared(
    ctx: &Context,
    target_path: Option<&Path>,
    target_line: usize,
    target_column: usize,
    references: &[LspReference],
) -> PreparedReferencePopup {
    let id = egui::Id::new(REFERENCES_PREPARED_CACHE_ID);
    ctx.data_mut(|data| {
        let (cache, rebuilt) = PreparedReferencePopup::reuse_or_build(
            data.get_temp::<PreparedReferencePopup>(id),
            target_path,
            target_line,
            target_column,
            references,
        );
        if rebuilt {
            data.insert_temp(id, cache.clone());
        }
        cache
    })
}

fn clear_reference_popup_prepared(ctx: &Context) {
    let id = egui::Id::new(REFERENCES_PREPARED_CACHE_ID);
    ctx.data_mut(|data| {
        data.remove_temp::<PreparedReferencePopup>(id);
    });
}

fn reference_popup_target_label(
    target_path: Option<&Path>,
    target_line: usize,
    target_column: usize,
) -> Arc<str> {
    target_path
        .filter(|_| reference_popup_position_is_valid(target_line, target_column))
        .map(|path| Arc::<str>::from(lsp_popup_location_label(path, target_line, target_column)))
        .unwrap_or_else(|| Arc::<str>::from("No target"))
}

fn reference_popup_open_index(selected: usize, row_count: usize) -> Option<usize> {
    (selected < row_count).then_some(selected)
}

fn reference_popup_take_open_target(
    references: &mut Vec<LspReference>,
    prepared: &PreparedReferencePopup,
    selected: usize,
) -> Option<LspReference> {
    let source_index = prepared.row(selected)?.source_index;
    if !references
        .get(source_index)
        .is_some_and(reference_popup_reference_range_is_valid)
    {
        return None;
    }
    Some(references.swap_remove(source_index))
}

fn prepared_reference_rows_match(
    rows: &[PreparedReferencePopupRow],
    references: &[LspReference],
) -> bool {
    let mut references = references
        .iter()
        .enumerate()
        .filter(|(_, reference)| reference_popup_reference_range_is_valid(reference));

    rows.iter().all(|row| {
        references
            .next()
            .is_some_and(|(source_index, reference)| row.matches(source_index, reference))
    }) && references.next().is_none()
}

fn reference_popup_reference_range_is_valid(reference: &LspReference) -> bool {
    reference_popup_position_is_valid(reference.line, reference.column)
        && reference_popup_position_is_valid(reference.end_line, reference.end_column)
        && (reference.end_line > reference.line
            || (reference.end_line == reference.line && reference.end_column >= reference.column))
}

fn reference_popup_position_is_valid(line: usize, column: usize) -> bool {
    line > 0 && column > 0
}

pub(crate) fn lsp_popup_location_label(path: &Path, line: usize, column: usize) -> String {
    let mut label = String::new();
    lsp_popup_location_label_into(&mut label, path, line, column);
    label
}

pub(crate) fn lsp_popup_location_label_into(
    label: &mut String,
    path: &Path,
    line: usize,
    column: usize,
) {
    label.clear();
    let mut label_chars = append_lsp_popup_path_label(label, path);
    let location_chars = decimal_digits(line) + decimal_digits(column) + 2;
    label.reserve(location_chars);
    let _ = write!(label, ":{line}:{column}");
    label_chars += location_chars;
    if label_chars > LSP_POPUP_LABEL_MAX_CHARS {
        truncate_lsp_popup_label_from(label, 0, LSP_POPUP_LABEL_MAX_CHARS);
    }
}

#[cfg(test)]
pub(crate) fn lsp_popup_item_location_label(
    name: &str,
    path: &Path,
    line: usize,
    column: usize,
) -> String {
    let mut label = String::new();
    lsp_popup_item_location_label_into(&mut label, name, path, line, column);
    label
}

pub(crate) fn lsp_popup_item_location_label_into(
    label: &mut String,
    name: &str,
    path: &Path,
    line: usize,
    column: usize,
) {
    label.clear();
    let mut label_chars = append_lsp_popup_item_name_label(label, name);
    label.push_str("  ");
    label_chars += 2;
    label_chars += append_lsp_popup_path_label(label, path);
    let location_chars = decimal_digits(line) + decimal_digits(column) + 2;
    label.reserve(location_chars);
    let _ = write!(label, ":{line}:{column}");
    label_chars += location_chars;
    if label_chars > LSP_POPUP_LABEL_MAX_CHARS {
        truncate_lsp_popup_label_from(label, 0, LSP_POPUP_LABEL_MAX_CHARS);
    }
}

pub(crate) fn lsp_popup_bound_label(value: &mut String, max_chars: usize) {
    if lsp_popup_char_count_exceeds(value, max_chars) {
        truncate_lsp_popup_label_from(value, 0, max_chars);
    }
}

#[cfg(test)]
pub(crate) fn lsp_popup_display_text(value: &str, max_chars: usize) -> String {
    let mut output = String::with_capacity(value.len().min(max_chars));
    append_lsp_popup_display_text(&mut output, value, max_chars);
    output
}

fn append_lsp_popup_display_text(output: &mut String, value: &str, max_chars: usize) -> usize {
    if max_chars == 0 {
        return 0;
    }

    let start_len = output.len();
    let mut output_chars = 0usize;
    let mut pending_space = false;
    let mut truncated = false;
    for (scanned_chars, (byte_idx, ch)) in value.char_indices().enumerate() {
        if scanned_chars >= LSP_POPUP_INPUT_SCAN_MAX_CHARS
            || byte_idx >= LSP_POPUP_INPUT_SCAN_MAX_BYTES
        {
            truncated = output_chars > 0;
            break;
        }

        if is_lsp_popup_bidi_control(ch) {
            continue;
        }

        if ch.is_control() || ch.is_whitespace() {
            if output_chars > 0 {
                pending_space = true;
            }
            continue;
        }

        if pending_space {
            if output_chars >= max_chars {
                truncated = true;
                break;
            }
            output.push(' ');
            output_chars += 1;
            pending_space = false;
        }

        if output_chars >= max_chars {
            truncated = true;
            break;
        }
        output.push(ch);
        output_chars += 1;
    }

    if truncated {
        truncate_lsp_popup_label_from(output, start_len, max_chars)
    } else {
        output_chars
    }
}

fn append_lsp_popup_item_name_label(output: &mut String, name: &str) -> usize {
    let start_len = output.len();
    let appended_chars = append_lsp_popup_display_text(output, name, LSP_POPUP_COMPONENT_MAX_CHARS);
    if output.len() == start_len {
        output.push_str("Unnamed");
        return "Unnamed".len();
    }
    appended_chars
}

fn append_lsp_popup_path_label(output: &mut String, path: &Path) -> usize {
    let start_len = output.len();
    let appended_chars = append_lsp_popup_display_text(
        output,
        lsp_popup_compact_path_text(path).as_ref(),
        LSP_POPUP_PATH_MAX_CHARS,
    );
    if output.len() == start_len {
        output.push('.');
        return 1;
    }
    appended_chars
}

fn truncate_lsp_popup_label_from(output: &mut String, start_len: usize, max_chars: usize) -> usize {
    if max_chars <= LSP_POPUP_TRUNCATION_MARKER_CHARS {
        output.truncate(start_len);
        output.extend(LSP_POPUP_TRUNCATION_MARKER.chars().take(max_chars));
        return max_chars;
    }

    let keep_chars = max_chars - LSP_POPUP_TRUNCATION_MARKER_CHARS;
    let keep_byte_len = output[start_len..]
        .char_indices()
        .nth(keep_chars)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| output.len().saturating_sub(start_len));
    output.truncate(start_len + keep_byte_len);
    let trimmed_len = output[start_len..].trim_end().len();
    output.truncate(start_len + trimmed_len);
    let kept_chars = output[start_len..].chars().count();
    output.push_str(LSP_POPUP_TRUNCATION_MARKER);
    kept_chars + LSP_POPUP_TRUNCATION_MARKER_CHARS
}

fn lsp_popup_compact_path_text(path: &Path) -> Cow<'_, str> {
    if path.as_os_str().is_empty() {
        return Cow::Borrowed(".");
    }

    path.file_name()
        .and_then(|name| name.to_str())
        .map(Cow::Borrowed)
        .unwrap_or_else(|| Cow::Owned(path.display().to_string()))
}

fn is_lsp_popup_bidi_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn lsp_popup_char_count_exceeds(value: &str, max_chars: usize) -> bool {
    value.chars().nth(max_chars).is_some()
}

fn decimal_digits(value: usize) -> usize {
    let mut digits = 1;
    let mut remaining = value;
    while remaining >= 10 {
        remaining /= 10;
        digits += 1;
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_POPUP_COMPONENT_MAX_CHARS, LSP_POPUP_INPUT_SCAN_MAX_CHARS, LSP_POPUP_LABEL_MAX_CHARS,
        LSP_POPUP_TRUNCATION_MARKER, LSP_POPUP_TRUNCATION_MARKER_CHARS, PreparedReferencePopup,
        is_lsp_popup_bidi_control, lsp_popup_display_text, lsp_popup_item_location_label,
        lsp_popup_location_label, reference_popup_open_index,
        reference_popup_reference_range_is_valid, reference_popup_take_open_target,
    };
    use kuroya_core::LspReference;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn references_location_label_preserves_ordinary_display_and_one_based_location() {
        let path = PathBuf::from("workspace/src/main.rs");

        assert_eq!(lsp_popup_location_label(&path, 12, 8), "main.rs:12:8");
    }

    #[test]
    fn references_location_label_strips_bidi_and_collapses_controls() {
        let path = PathBuf::from("workspace/src/mod\u{202e}\nname.rs");
        let label = lsp_popup_location_label(&path, 1, 2);

        assert_eq!(label, "mod name.rs:1:2");
        assert!(!label.chars().any(char::is_control));
        assert!(!label.chars().any(is_lsp_popup_bidi_control));
    }

    #[test]
    fn references_location_label_uses_blank_path_fallback() {
        let path = PathBuf::from("\u{202e}\n\t");

        assert_eq!(lsp_popup_location_label(&path, 3, 4), ".:3:4");
    }

    #[test]
    fn references_display_text_caps_huge_labels() {
        let label = lsp_popup_display_text(
            &"x".repeat(LSP_POPUP_LABEL_MAX_CHARS + 32),
            LSP_POPUP_LABEL_MAX_CHARS,
        );

        assert_eq!(label.chars().count(), LSP_POPUP_LABEL_MAX_CHARS);
        assert!(label.ends_with("..."));
    }

    #[test]
    fn references_display_text_preserves_exact_truncation_boundary() {
        let label = lsp_popup_display_text(
            &"x".repeat(LSP_POPUP_LABEL_MAX_CHARS),
            LSP_POPUP_LABEL_MAX_CHARS,
        );

        assert_eq!(label, "x".repeat(LSP_POPUP_LABEL_MAX_CHARS));
    }

    #[test]
    fn references_display_text_bounds_huge_removed_controls() {
        let label = lsp_popup_display_text(
            &"\u{202e}".repeat(LSP_POPUP_INPUT_SCAN_MAX_CHARS + 32),
            LSP_POPUP_LABEL_MAX_CHARS,
        );

        assert_eq!(label, "");
    }

    #[test]
    fn references_item_location_label_bounds_huge_unsafe_name_and_path() {
        let unsafe_text = "\u{202e}\n\t".repeat(LSP_POPUP_INPUT_SCAN_MAX_CHARS + 64);
        let path = PathBuf::from(unsafe_text.clone());
        let label = lsp_popup_item_location_label(&unsafe_text, &path, 7, 9);

        assert_eq!(label, "Unnamed  .:7:9");
        assert!(label.chars().count() <= LSP_POPUP_LABEL_MAX_CHARS);
        assert!(!label.chars().any(char::is_control));
        assert!(!label.chars().any(is_lsp_popup_bidi_control));
    }

    #[test]
    fn references_item_location_label_truncates_when_location_crosses_boundary() {
        let name = "n".repeat(LSP_POPUP_COMPONENT_MAX_CHARS);
        let path_chars = LSP_POPUP_LABEL_MAX_CHARS - LSP_POPUP_COMPONENT_MAX_CHARS - 2;
        let path = PathBuf::from("p".repeat(path_chars));
        let label = lsp_popup_item_location_label(&name, &path, 1, 1);
        let expected_path_chars = path_chars - LSP_POPUP_TRUNCATION_MARKER_CHARS;
        let expected = format!(
            "{name}  {}{LSP_POPUP_TRUNCATION_MARKER}",
            "p".repeat(expected_path_chars)
        );

        assert_eq!(label, expected);
        assert_eq!(label.chars().count(), LSP_POPUP_LABEL_MAX_CHARS);
    }

    #[test]
    fn references_prepared_popup_reuses_sanitized_rows_for_same_raw_reference() {
        let target = PathBuf::from("workspace/src/root.rs");
        let references = vec![reference(
            PathBuf::from("workspace/src/mod\nname.rs"),
            1,
            2,
            3,
            4,
        )];
        let prepared = PreparedReferencePopup::build(Some(target.as_path()), 10, 20, &references);
        let cached_label = Arc::clone(&prepared.rows[0].label);
        let cached_target_label = Arc::clone(&prepared.target.label);
        let (prepared, rebuilt) = PreparedReferencePopup::reuse_or_build(
            Some(prepared),
            Some(target.as_path()),
            10,
            20,
            &references,
        );

        assert!(!rebuilt);
        assert!(prepared.matches(Some(target.as_path()), 10, 20, &references));
        assert_eq!(prepared.target_label(), "root.rs:10:20");
        assert_eq!(
            prepared.row(0).map(|row| row.label()),
            Some("mod name.rs:1:2")
        );
        assert!(Arc::ptr_eq(&cached_label, &prepared.rows[0].label));
        assert!(Arc::ptr_eq(&cached_target_label, &prepared.target.label));
    }

    #[test]
    fn references_prepared_popup_invalidates_on_raw_range_change() {
        let target = PathBuf::from("workspace/src/root.rs");
        let mut references = vec![reference(
            PathBuf::from("workspace/src/main.rs"),
            1,
            2,
            3,
            4,
        )];
        let prepared = PreparedReferencePopup::build(Some(target.as_path()), 10, 20, &references);
        let cached_label = Arc::clone(&prepared.rows[0].label);

        references[0].end_column = 9;
        let (prepared, rebuilt) = PreparedReferencePopup::reuse_or_build(
            Some(prepared),
            Some(target.as_path()),
            10,
            20,
            &references,
        );

        assert!(rebuilt);
        assert!(prepared.matches(Some(target.as_path()), 10, 20, &references));
        assert_eq!(prepared.row(0).map(|row| row.label()), Some("main.rs:1:2"));
        assert_eq!(prepared.row(0).map(|row| &row.target), Some(&references[0]));
        assert!(!Arc::ptr_eq(&cached_label, &prepared.rows[0].label));
        assert_eq!(references[0].end_column, 9);
    }

    #[test]
    fn references_prepared_popup_filters_invalid_ranges_and_keeps_raw_valid_target() {
        let raw_path = PathBuf::from("workspace/src/mod\u{202e}\nname.rs");
        let mut references = vec![
            reference(PathBuf::from("workspace/src/zero.rs"), 0, 2, 1, 4),
            reference(raw_path.clone(), 1, 2, 1, 4),
            reference(PathBuf::from("workspace/src/reversed.rs"), 3, 8, 3, 2),
        ];
        let prepared = PreparedReferencePopup::build(None, 0, 0, &references);
        let (prepared, rebuilt) =
            PreparedReferencePopup::reuse_or_build(Some(prepared), None, 0, 0, &references);

        assert!(!rebuilt);
        assert!(prepared.matches(None, 0, 0, &references));
        assert_eq!(prepared.row_count(), 1);
        assert_eq!(
            prepared.row(0).map(|row| row.label()),
            Some("mod name.rs:1:2")
        );
        let opened = reference_popup_take_open_target(&mut references, &prepared, 0)
            .expect("selected reference opens");
        assert_eq!(opened, reference(raw_path.clone(), 1, 2, 1, 4));
        assert_eq!(opened.path, raw_path);
        assert_eq!(references.len(), 2);
        assert_eq!(
            reference_popup_take_open_target(&mut references, &prepared, 1),
            None
        );
    }

    #[test]
    fn references_prepared_popup_invalidates_when_filtered_source_index_moves() {
        let mut references = vec![reference(
            PathBuf::from("workspace/src/main.rs"),
            1,
            2,
            1,
            4,
        )];
        let prepared = PreparedReferencePopup::build(None, 0, 0, &references);
        let cached_label = Arc::clone(&prepared.rows[0].label);

        references.insert(
            0,
            reference(PathBuf::from("workspace/src/zero.rs"), 0, 2, 1, 4),
        );
        let (prepared, rebuilt) =
            PreparedReferencePopup::reuse_or_build(Some(prepared), None, 0, 0, &references);

        assert!(rebuilt);
        assert!(prepared.matches(None, 0, 0, &references));
        assert_eq!(prepared.row(0).map(|row| row.source_index), Some(1));
        assert!(!Arc::ptr_eq(&cached_label, &prepared.rows[0].label));
        assert_eq!(
            reference_popup_take_open_target(&mut references, &prepared, 0),
            Some(reference(
                PathBuf::from("workspace/src/main.rs"),
                1,
                2,
                1,
                4
            ))
        );
    }

    #[test]
    fn references_prepared_popup_reuses_label_scratch_without_aliasing_rows() {
        let references = vec![
            reference(PathBuf::from("workspace/src/main.rs"), 1, 2, 1, 4),
            reference(PathBuf::from("workspace/src/lib.rs"), 5, 6, 5, 8),
        ];
        let prepared = PreparedReferencePopup::build(None, 0, 0, &references);

        assert_eq!(prepared.row_count(), 2);
        assert_eq!(prepared.row(0).map(|row| row.label()), Some("main.rs:1:2"));
        assert_eq!(prepared.row(1).map(|row| row.label()), Some("lib.rs:5:6"));
        let mut open_references = references.clone();
        assert_eq!(
            reference_popup_take_open_target(&mut open_references, &prepared, 0),
            Some(references[0].clone())
        );
        let mut open_references = references.clone();
        assert_eq!(
            reference_popup_take_open_target(&mut open_references, &prepared, 1),
            Some(references[1].clone())
        );
    }

    #[test]
    fn references_prepared_popup_target_label_falls_back_for_invalid_target_position() {
        let target = PathBuf::from("workspace/src/root.rs");
        let prepared = PreparedReferencePopup::build(Some(target.as_path()), 0, 20, &[]);

        assert_eq!(prepared.target_label(), "No target");
    }

    #[test]
    fn references_prepared_popup_opens_raw_target_not_sanitized_label() {
        let raw_path = PathBuf::from("workspace/src/mod\u{202e}\nname.rs");
        let references = vec![reference(raw_path.clone(), 1, 2, 3, 4)];
        let prepared = PreparedReferencePopup::build(None, 0, 0, &references);

        assert_eq!(
            prepared.row(0).map(|row| row.label()),
            Some("mod name.rs:1:2")
        );
        let mut open_references = references.clone();
        assert_eq!(
            reference_popup_take_open_target(&mut open_references, &prepared, 0),
            Some(references[0].clone())
        );
        assert_eq!(
            reference_popup_take_open_target(&mut open_references, &prepared, 1),
            None
        );
        assert_eq!(references[0].path, raw_path);
    }

    #[test]
    fn references_open_index_rejects_stale_selection() {
        assert_eq!(reference_popup_open_index(0, 0), None);
        assert_eq!(reference_popup_open_index(2, 2), None);
        assert_eq!(reference_popup_open_index(1, 2), Some(1));
    }

    #[test]
    fn references_range_guard_rejects_zero_or_reversed_ranges() {
        assert!(reference_popup_reference_range_is_valid(&reference(
            PathBuf::from("workspace/src/main.rs"),
            1,
            1,
            1,
            4
        )));
        assert!(!reference_popup_reference_range_is_valid(&reference(
            PathBuf::from("workspace/src/main.rs"),
            0,
            1,
            1,
            4
        )));
        assert!(!reference_popup_reference_range_is_valid(&reference(
            PathBuf::from("workspace/src/main.rs"),
            4,
            8,
            4,
            2
        )));
    }

    fn reference(
        path: PathBuf,
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
    ) -> LspReference {
        LspReference {
            path,
            line,
            column,
            end_line,
            end_column,
        }
    }
}

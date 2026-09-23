use crate::{
    editor_pane_rows::EditorRowContext,
    editor_text_geometry::{visual_width, visual_width_for_char, visual_x_for_char_idx},
    folding::FoldedRange,
    theme::diagnostic_color,
};
use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, pos2};
use kuroya_core::{LspCodeLens, LspInlayHint};

const MAX_CODE_LENS_TITLE_CHARS: usize = 80;
const MAX_CODE_LENSES_PER_ROW: usize = 128;
const MAX_INLAY_HINTS_PER_ROW: usize = 128;
const MAX_IME_PREEDIT_CHARS: usize = 80;
const CODE_LENS_SEPARATOR: &str = "  |  ";
const DIAGNOSTIC_MESSAGE_GAP_COLUMNS: f32 = 3.0;
const DIAGNOSTIC_MESSAGE_MIN_VISIBLE_COLUMNS: f32 = 6.0;
const DIAGNOSTIC_MESSAGE_MIN_VISIBLE_WIDTH: f32 = 48.0;

pub(crate) fn paint_folded_label(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: Pos2,
    visual_text_width: usize,
    row: &EditorRowContext<'_>,
    range: FoldedRange,
) {
    if !inline_overlay_geometry_is_valid(
        rect,
        text_pos.x,
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return;
    }

    let hidden = range.end_line.saturating_sub(range.start_line);
    let label = format!(" ... {hidden} folded");
    let x = text_pos.x
        + (((visual_text_width + 2) as f32) * row.char_width)
            .min((rect.width() - row.gutter_width - 140.0).max(0.0));
    painter.text(
        pos2(x, rect.top() + 3.0),
        egui::Align2::LEFT_TOP,
        label,
        FontId::new(row.font_size, FontFamily::Monospace),
        row.weak_text_color,
    );
}

pub(crate) fn paint_diagnostic_message(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: Pos2,
    visual_text_width: usize,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) {
    if !inline_overlay_geometry_is_valid(
        rect,
        text_pos.x,
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return;
    }

    let line_number = line_idx + 1;
    let Some(display) = diagnostic_message_display(row, line_number) else {
        return;
    };
    let Some(x) = diagnostic_message_origin_x(
        rect,
        row.gutter_width,
        text_pos.x,
        visual_text_width,
        row.char_width,
    ) else {
        return;
    };
    painter.text(
        pos2(x, rect.top() + 3.0),
        egui::Align2::LEFT_TOP,
        display.message,
        FontId::new(row.font_size, FontFamily::Monospace),
        display.color,
    );
}

struct DiagnosticMessageDisplay<'a> {
    message: &'a str,
    color: Color32,
}

fn diagnostic_message_display<'a>(
    row: &'a EditorRowContext<'_>,
    line_number: usize,
) -> Option<DiagnosticMessageDisplay<'a>> {
    row.diagnostic_messages
        .get(&line_number)
        .map(|message| DiagnosticMessageDisplay {
            message: message.as_str(),
            color: row
                .diagnostics_by_line
                .get(&line_number)
                .copied()
                .map(diagnostic_color)
                .unwrap_or(row.weak_text_color),
        })
}

fn diagnostic_message_origin_x(
    rect: egui::Rect,
    gutter_width: f32,
    text_pos_x: f32,
    visual_text_width: usize,
    char_width: f32,
) -> Option<f32> {
    if !rect.right().is_finite()
        || !rect.width().is_finite()
        || !gutter_width.is_finite()
        || !text_pos_x.is_finite()
        || !char_width.is_finite()
        || char_width <= 0.0
    {
        return None;
    }

    let min_visible_width = (DIAGNOSTIC_MESSAGE_MIN_VISIBLE_COLUMNS * char_width)
        .max(DIAGNOSTIC_MESSAGE_MIN_VISIBLE_WIDTH);
    let text_area_width = rect.width() - gutter_width.max(0.0);
    if text_area_width < min_visible_width {
        return None;
    }

    let text_end_x = text_pos_x + visual_text_width as f32 * char_width;
    let origin_x = text_end_x + DIAGNOSTIC_MESSAGE_GAP_COLUMNS * char_width;
    if !text_end_x.is_finite()
        || !origin_x.is_finite()
        || origin_x + min_visible_width > rect.right()
    {
        return None;
    }

    Some(origin_x)
}

pub(crate) fn paint_inlay_hints(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: Pos2,
    text: &str,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) {
    if !inline_overlay_geometry_is_valid(
        rect,
        text_pos.x,
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return;
    }

    let line_number = line_idx + 1;
    let hints = inlay_hint_displays_for_line(
        row.inlay_hints,
        line_number,
        row.inlay_hints_maximum_length,
        row.inlay_hints_padding,
    );
    if hints.is_empty() {
        return;
    }

    let mut columns = VisualColumnWalker::new(text, row.tab_width);
    let font_id = inlay_hint_font_id(
        row.inlay_hints_font_family,
        row.inlay_hints_font_size,
        row.font_size,
    );
    for hint in hints {
        let visual_col = columns.visual_column_for_char_offset(hint.column.saturating_sub(1));
        let x = text_pos.x + visual_col as f32 * row.char_width;
        if x >= rect.right() - 24.0 {
            continue;
        }
        paint_owned_inline_label(
            painter,
            pos2(x + row.char_width * 0.35, rect.top() + 3.0),
            hint.label,
            font_id.clone(),
            row.weak_text_color,
        );
    }
}

pub(crate) fn inlay_hints_for_line(hints: &[LspInlayHint], line_number: usize) -> &[LspInlayHint] {
    let start = hints.partition_point(|hint| hint.line < line_number);
    let end = start + hints[start..].partition_point(|hint| hint.line == line_number);
    &hints[start..end]
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InlayHintDisplay {
    column: usize,
    label: String,
}

fn inlay_hint_displays_for_line(
    hints: &[LspInlayHint],
    line_number: usize,
    maximum_length: usize,
    padding: bool,
) -> Vec<InlayHintDisplay> {
    let line_hints = inlay_hints_for_line(hints, line_number);
    let mut displays = Vec::with_capacity(line_hints.len().min(MAX_INLAY_HINTS_PER_ROW));
    for hint in line_hints.iter().take(MAX_INLAY_HINTS_PER_ROW) {
        let label = inlay_hint_label(&hint.label, maximum_length, padding);
        if !label.trim().is_empty() {
            displays.push(InlayHintDisplay {
                column: hint.column,
                label,
            });
        }
    }
    displays
}

struct VisualColumnWalker<'a> {
    text: &'a str,
    chars: std::str::Chars<'a>,
    char_offset: usize,
    visual_column: usize,
    tab_width: usize,
}

impl<'a> VisualColumnWalker<'a> {
    fn new(text: &'a str, tab_width: usize) -> Self {
        Self {
            text,
            chars: text.chars(),
            char_offset: 0,
            visual_column: 0,
            tab_width: tab_width.max(1),
        }
    }

    fn visual_column_for_char_offset(&mut self, target_offset: usize) -> usize {
        if target_offset < self.char_offset {
            self.reset();
        }

        while self.char_offset < target_offset {
            let Some(ch) = self.chars.next() else {
                break;
            };
            self.visual_column = self.visual_column.saturating_add(visual_width_for_char(
                ch,
                self.visual_column,
                self.tab_width,
            ));
            self.char_offset += 1;
        }

        self.visual_column
    }

    fn reset(&mut self) {
        self.chars = self.text.chars();
        self.char_offset = 0;
        self.visual_column = 0;
    }
}

pub(crate) fn paint_ime_preedit(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: Pos2,
    snapshot_range: &std::ops::Range<usize>,
    line_text: &str,
    cursor_char_idx: Option<usize>,
    row: &EditorRowContext<'_>,
) {
    if !inline_overlay_geometry_is_valid(
        rect,
        text_pos.x,
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return;
    }

    let Some(preedit) = row.ime_preedit else {
        return;
    };
    let Some(cursor_char_idx) = cursor_char_idx else {
        return;
    };
    let Some(label) = ime_preedit_label(preedit) else {
        return;
    };
    let Some(x) = ime_preedit_origin_x(
        text_pos.x,
        line_text,
        cursor_char_idx,
        snapshot_range.start,
        row.tab_width,
        row.char_width,
    ) else {
        return;
    };
    if x >= rect.right() - row.char_width {
        return;
    }

    painter.text(
        pos2(x, rect.top() + 3.0),
        egui::Align2::LEFT_TOP,
        label.as_str(),
        FontId::new(row.font_size, FontFamily::Monospace),
        row.text_color,
    );

    let label_width = visual_width(&label, row.tab_width).max(1) as f32 * row.char_width;
    let underline_y = (rect.bottom() - 3.0).max(rect.top() + 3.0);
    painter.line_segment(
        [
            pos2(x, underline_y),
            pos2((x + label_width).min(rect.right()), underline_y),
        ],
        egui::Stroke::new(1.0_f32, row.selection_bg_fill),
    );
}

pub(crate) fn paint_completion_preview(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: Pos2,
    snapshot_range: &std::ops::Range<usize>,
    line_idx: usize,
    cursor_char_idx: Option<usize>,
    line_char_count: usize,
    visual_text_width: usize,
    row: &EditorRowContext<'_>,
) {
    if !inline_overlay_geometry_is_valid(
        rect,
        text_pos.x,
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return;
    }

    let Some(preview) = row
        .completion_preview
        .filter(|preview| preview.line_idx == line_idx && !preview.text.is_empty())
    else {
        return;
    };
    let Some(cursor_char_idx) = cursor_char_idx else {
        return;
    };
    let Some(x) = completion_preview_origin_x(
        text_pos.x,
        cursor_char_idx,
        snapshot_range.start,
        line_char_count,
        visual_text_width,
        row.char_width,
    ) else {
        return;
    };
    if x >= rect.right() - row.char_width {
        return;
    }

    painter.text(
        pos2(x, rect.top() + 3.0),
        egui::Align2::LEFT_TOP,
        &preview.text,
        FontId::new(row.font_size, FontFamily::Monospace),
        row.weak_text_color,
    );
}

pub(crate) fn paint_code_lenses(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: Pos2,
    visual_text_width: usize,
    line_idx: usize,
    row: &EditorRowContext<'_>,
) {
    if !inline_overlay_geometry_is_valid(
        rect,
        text_pos.x,
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return;
    }

    let line_number = line_idx + 1;
    let Some(display) = code_lens_line_display(row.code_lenses, line_number) else {
        return;
    };
    let Some(x) = code_lens_origin_x(rect, text_pos.x, visual_text_width, row) else {
        return;
    };
    let Some(y) = code_lens_origin_y(rect, row) else {
        return;
    };

    paint_owned_inline_label(
        painter,
        pos2(x, y),
        display.label,
        code_lens_font_id(
            row.code_lens_font_family,
            row.code_lens_font_size,
            row.font_size,
        ),
        row.weak_text_color,
    );
}

pub(crate) fn code_lens_command_at_pointer(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    text_pos: Pos2,
    visual_text_width: usize,
    line_idx: usize,
    pointer: Pos2,
    row: &EditorRowContext<'_>,
) -> Option<LspCodeLens> {
    if !pointer.x.is_finite() || !pointer.y.is_finite() {
        return None;
    }
    let origin_y = code_lens_origin_y(rect, row)?;
    if pointer.y < origin_y || pointer.y > rect.bottom() {
        return None;
    }

    let font_id = code_lens_font_id(
        row.code_lens_font_family,
        row.code_lens_font_size,
        row.font_size,
    );
    let x = code_lens_origin_x(rect, text_pos.x, visual_text_width, row)?;
    code_lens_command_at_x(row.code_lenses, line_idx + 1, x, pointer.x, |text| {
        code_lens_text_width(ui, text, &font_id)
    })
}

#[cfg(test)]
pub(crate) fn code_lens_line_label(lenses: &[LspCodeLens], line_number: usize) -> Option<String> {
    code_lens_line_display(lenses, line_number).map(|display| display.label)
}

#[derive(Debug)]
struct CodeLensLineDisplay<'a> {
    label: String,
    #[cfg(test)]
    items: Vec<CodeLensDisplayItem<'a>>,
    #[cfg(not(test))]
    _marker: std::marker::PhantomData<&'a LspCodeLens>,
}

#[cfg(test)]
#[derive(Debug)]
struct CodeLensDisplayItem<'a> {
    title_range: std::ops::Range<usize>,
    lens: &'a LspCodeLens,
}

fn code_lens_line_display(
    lenses: &[LspCodeLens],
    line_number: usize,
) -> Option<CodeLensLineDisplay<'_>> {
    let line_lenses = code_lenses_for_line(lenses, line_number);
    let mut label = String::new();
    let mut has_items = false;
    #[cfg(test)]
    let mut items = Vec::with_capacity(line_lenses.len().min(MAX_CODE_LENSES_PER_ROW));
    for lens in line_lenses.iter().take(MAX_CODE_LENSES_PER_ROW) {
        let label_len = label.len();
        if !label.is_empty() {
            label.push_str(CODE_LENS_SEPARATOR);
        }

        #[cfg(test)]
        let title_start = label.len();
        if push_code_lens_title(&lens.title, &mut label) {
            has_items = true;
            #[cfg(test)]
            items.push(CodeLensDisplayItem {
                title_range: title_start..label.len(),
                lens,
            });
        } else {
            label.truncate(label_len);
        }
    }

    has_items.then_some(CodeLensLineDisplay {
        label,
        #[cfg(test)]
        items,
        #[cfg(not(test))]
        _marker: std::marker::PhantomData,
    })
}

fn code_lens_command_at_x(
    lenses: &[LspCodeLens],
    line_number: usize,
    mut x: f32,
    pointer_x: f32,
    mut text_width: impl FnMut(&str) -> f32,
) -> Option<LspCodeLens> {
    let mut title = String::with_capacity(MAX_CODE_LENS_TITLE_CHARS);
    let mut title_seen = false;
    let mut separator_width = None;
    for lens in code_lenses_for_line(lenses, line_number)
        .iter()
        .take(MAX_CODE_LENSES_PER_ROW)
    {
        title.clear();
        if !push_code_lens_title(&lens.title, &mut title) {
            continue;
        }

        if title_seen {
            let separator_width =
                *separator_width.get_or_insert_with(|| text_width(CODE_LENS_SEPARATOR));
            x += separator_width;
        } else {
            title_seen = true;
        }

        let title_width = text_width(title.as_str());
        if pointer_x >= x && pointer_x <= x + title_width {
            return lens.command.as_ref().map(|_| lens.clone());
        }
        x += title_width;
    }
    None
}

pub(crate) fn code_lens_origin_x(
    rect: egui::Rect,
    text_pos_x: f32,
    visual_text_width: usize,
    row: &EditorRowContext<'_>,
) -> Option<f32> {
    if !inline_overlay_geometry_is_valid(
        rect,
        text_pos_x,
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return None;
    }

    let x = text_pos_x
        + (((visual_text_width + 4) as f32) * row.char_width)
            .min((rect.width() - row.gutter_width - 220.0).max(0.0));
    x.is_finite().then_some(x)
}

fn code_lens_origin_y(rect: egui::Rect, row: &EditorRowContext<'_>) -> Option<f32> {
    if !rect.top().is_finite() || !rect.bottom().is_finite() || rect.bottom() < rect.top() {
        return None;
    }
    if !row.row_height.is_finite() || row.row_height <= 0.0 {
        return None;
    }

    let y = rect.top() + row.row_height * 0.48;
    y.is_finite().then_some(y)
}

fn code_lens_text_width(ui: &mut egui::Ui, text: &str, font_id: &FontId) -> f32 {
    ui.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font_id.clone(), Color32::WHITE)
            .rect
            .width()
    })
}

fn paint_owned_inline_label(
    painter: &egui::Painter,
    pos: Pos2,
    label: String,
    font_id: FontId,
    color: Color32,
) {
    let galley = painter.layout_no_wrap(label, font_id, color);
    painter.galley(pos, galley, color);
}

pub(crate) fn code_lenses_for_line(lenses: &[LspCodeLens], line_number: usize) -> &[LspCodeLens] {
    let start = lenses.partition_point(|lens| lens.line < line_number);
    let end = start + lenses[start..].partition_point(|lens| lens.line == line_number);
    &lenses[start..end]
}

pub(crate) fn ime_preedit_origin_x(
    text_pos_x: f32,
    line_text: &str,
    cursor_char_idx: usize,
    snapshot_start: usize,
    tab_width: usize,
    char_width: f32,
) -> Option<f32> {
    if !text_pos_x.is_finite() || !char_width.is_finite() || char_width <= 0.0 {
        return None;
    }

    let x = visual_x_for_char_idx(
        text_pos_x,
        line_text,
        cursor_char_idx,
        snapshot_start,
        tab_width,
        char_width,
    );
    x.is_finite().then_some(x)
}

pub(crate) fn completion_preview_origin_x(
    text_pos_x: f32,
    cursor_char_idx: usize,
    snapshot_start: usize,
    line_char_count: usize,
    visual_text_width: usize,
    char_width: f32,
) -> Option<f32> {
    if cursor_char_idx != snapshot_start.saturating_add(line_char_count) {
        return None;
    }
    if !text_pos_x.is_finite() || !char_width.is_finite() || char_width <= 0.0 {
        return None;
    }

    let x = text_pos_x + visual_text_width as f32 * char_width;
    x.is_finite().then_some(x)
}

fn inline_overlay_geometry_is_valid(
    rect: egui::Rect,
    text_pos_x: f32,
    char_width: f32,
    row_height: f32,
    gutter_width: f32,
) -> bool {
    rect.left().is_finite()
        && rect.top().is_finite()
        && rect.right().is_finite()
        && rect.bottom().is_finite()
        && rect.right() >= rect.left()
        && rect.bottom() >= rect.top()
        && text_pos_x.is_finite()
        && char_width.is_finite()
        && char_width > 0.0
        && row_height.is_finite()
        && row_height > 0.0
        && gutter_width.is_finite()
        && gutter_width >= 0.0
}

pub(crate) fn ime_preedit_label(text: &str) -> Option<String> {
    let mut output = String::new();
    for ch in text
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_IME_PREEDIT_CHARS)
    {
        output.push(ch);
    }

    (!output.is_empty()).then_some(output)
}

pub(crate) fn inlay_hint_label(label: &str, maximum_length: usize, padding: bool) -> String {
    let capacity = if maximum_length == 0 {
        label.len()
    } else {
        label.len().min(maximum_length)
    };
    let mut output = String::with_capacity(capacity + 2);
    let mut copied = false;
    for (count, ch) in label.chars().enumerate() {
        if maximum_length != 0 && count >= maximum_length {
            break;
        }

        let ch = if ch.is_control() { ' ' } else { ch };
        if !copied {
            if padding || !matches!(ch, ':' | ',' | ')' | ']' | '}' | '>') {
                output.push(' ');
            }
            copied = true;
        }
        output.push(ch);
    }
    if padding && copied {
        output.push(' ');
    }
    output
}

pub(crate) fn inlay_hint_font_id(family: &str, font_size: usize, editor_font_size: f32) -> FontId {
    let size = if font_size == 0 {
        editor_font_size
    } else {
        font_size as f32
    };
    FontId::new(size.max(8.0), inline_annotation_font_family(family))
}

pub(crate) fn code_lens_font_id(family: &str, font_size: usize, editor_font_size: f32) -> FontId {
    let size = if font_size == 0 {
        editor_font_size * 0.9
    } else {
        font_size as f32
    };
    FontId::new(size.max(8.0), inline_annotation_font_family(family))
}

fn inline_annotation_font_family(family: &str) -> FontFamily {
    let family = family.trim();
    if family.is_empty()
        || family.eq_ignore_ascii_case("editor")
        || family.eq_ignore_ascii_case("default")
    {
        FontFamily::Monospace
    } else {
        FontFamily::Name(family.to_owned().into())
    }
}

#[cfg(test)]
pub(crate) fn code_lens_title(title: &str) -> String {
    let mut output = String::with_capacity(title.len().min(MAX_CODE_LENS_TITLE_CHARS));
    push_code_lens_title(title, &mut output);
    output
}

fn push_code_lens_title(title: &str, output: &mut String) -> bool {
    let start_len = output.len();
    let mut output_chars = 0;
    let mut pending_space = false;

    for ch in title
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
    {
        if ch.is_whitespace() {
            if output_chars > 0 {
                pending_space = true;
            }
            continue;
        }

        if pending_space {
            if output_chars + 1 >= MAX_CODE_LENS_TITLE_CHARS {
                break;
            }
            output.push(' ');
            output_chars += 1;
            pending_space = false;
        }

        if output_chars >= MAX_CODE_LENS_TITLE_CHARS {
            break;
        }
        output.push(ch);
        output_chars += 1;
    }

    if output_chars == 0 {
        output.truncate(start_len);
        false
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InlayHintDisplay, MAX_CODE_LENSES_PER_ROW, MAX_INLAY_HINTS_PER_ROW, VisualColumnWalker,
        code_lens_command_at_x, code_lens_font_id, code_lens_line_display, code_lens_line_label,
        code_lens_title, code_lenses_for_line, completion_preview_origin_x,
        diagnostic_message_origin_x, ime_preedit_label, ime_preedit_origin_x,
        inlay_hint_displays_for_line, inlay_hint_label, inlay_hints_for_line,
        inline_overlay_geometry_is_valid,
    };
    use eframe::egui::{FontFamily, Rect, pos2};
    use kuroya_core::{LspCodeLens, LspInlayHint};
    use std::sync::Arc;

    #[test]
    fn inlay_hint_label_sanitizes_controls_spacing_and_length() {
        assert_eq!(inlay_hint_label(": usize", 43, false), ": usize");
        assert_eq!(inlay_hint_label("param", 43, false), " param");
        assert_eq!(inlay_hint_label("a\nb", 43, false), " a b");
        assert_eq!(inlay_hint_label("abcdef", 3, false), " abc");
        assert_eq!(inlay_hint_label("abcdef", 0, false), " abcdef");
        assert_eq!(inlay_hint_label(": T", 43, true), " : T ");
    }

    #[test]
    fn inlay_hint_displays_prepare_sanitized_labels_for_matching_line() {
        let hints = [
            inlay_hint(1, 1, "before"),
            inlay_hint(3, 2, "name\nvalue"),
            inlay_hint(3, 6, "\n\t"),
            inlay_hint(3, 9, ": usize"),
            inlay_hint(4, 1, "after"),
        ];

        assert_eq!(
            inlay_hint_displays_for_line(&hints, 3, 7, false),
            vec![
                InlayHintDisplay {
                    column: 2,
                    label: " name va".to_owned()
                },
                InlayHintDisplay {
                    column: 9,
                    label: ": usize".to_owned()
                },
            ]
        );
    }

    #[test]
    fn ime_preedit_label_sanitizes_controls_and_length() {
        assert_eq!(ime_preedit_label("wen\nzi").as_deref(), Some("wenzi"));
        assert_eq!(ime_preedit_label("\n\t"), None);
        assert_eq!(ime_preedit_label(&"a".repeat(100)).unwrap().len(), 80);
    }

    #[test]
    fn ime_preedit_origin_uses_visual_cursor_column() {
        assert_eq!(ime_preedit_origin_x(10.0, "\tab", 2, 0, 4, 8.0), Some(50.0));
        assert_eq!(
            ime_preedit_origin_x(10.0, "e\u{0301}x", 2, 0, 4, 8.0),
            Some(18.0)
        );
        assert_eq!(
            ime_preedit_origin_x(10.0, "abcdef", 6, 3, 4, 8.0),
            Some(34.0)
        );
    }

    #[test]
    fn completion_preview_origin_requires_cursor_at_rendered_line_end() {
        assert_eq!(
            completion_preview_origin_x(10.0, 3, 0, 3, 6, 8.0),
            Some(58.0)
        );
        assert_eq!(completion_preview_origin_x(10.0, 2, 0, 3, 6, 8.0), None);
        assert_eq!(
            completion_preview_origin_x(10.0, 6, 3, 3, 3, 8.0),
            Some(34.0)
        );
    }

    #[test]
    fn visual_column_walker_advances_inlay_columns_without_restarting() {
        let mut columns = VisualColumnWalker::new("\tab", 4);

        assert_eq!(columns.visual_column_for_char_offset(0), 0);
        assert_eq!(columns.visual_column_for_char_offset(1), 4);
        assert_eq!(columns.visual_column_for_char_offset(3), 6);
        assert_eq!(columns.visual_column_for_char_offset(1), 4);
        assert_eq!(columns.visual_column_for_char_offset(99), 6);
    }

    #[test]
    fn diagnostic_message_origin_uses_line_end_gap() {
        let rect = test_rect(400.0);

        assert_eq!(
            diagnostic_message_origin_x(rect, 40.0, 40.0, 10, 8.0),
            Some(144.0)
        );
    }

    #[test]
    fn diagnostic_message_origin_skips_narrow_pane_instead_of_overlapping_text() {
        let rect = test_rect(120.0);

        assert_eq!(diagnostic_message_origin_x(rect, 40.0, 40.0, 8, 8.0), None);
    }

    #[test]
    fn diagnostic_message_origin_skips_when_line_end_is_offscreen() {
        let rect = test_rect(400.0);

        assert_eq!(diagnostic_message_origin_x(rect, 40.0, 40.0, 60, 8.0), None);
    }

    #[test]
    fn diagnostic_message_origin_rejects_invalid_char_width() {
        let rect = test_rect(400.0);

        assert_eq!(diagnostic_message_origin_x(rect, 40.0, 40.0, 10, 0.0), None);
        assert_eq!(
            diagnostic_message_origin_x(rect, 40.0, 40.0, 10, -8.0),
            None
        );
        assert_eq!(
            diagnostic_message_origin_x(rect, 40.0, 40.0, 10, f32::NAN),
            None
        );
    }

    #[test]
    fn inline_overlay_geometry_rejects_non_finite_inputs() {
        let rect = test_rect(400.0);

        assert!(inline_overlay_geometry_is_valid(
            rect, 40.0, 8.0, 18.0, 40.0
        ));
        assert!(!inline_overlay_geometry_is_valid(
            rect,
            f32::NAN,
            8.0,
            18.0,
            40.0
        ));
        assert!(!inline_overlay_geometry_is_valid(
            rect,
            40.0,
            f32::INFINITY,
            18.0,
            40.0
        ));
        assert!(!inline_overlay_geometry_is_valid(
            rect, 40.0, 8.0, 0.0, 40.0
        ));
        assert!(!inline_overlay_geometry_is_valid(
            Rect::from_min_max(pos2(400.0, 0.0), pos2(0.0, 20.0)),
            40.0,
            8.0,
            18.0,
            40.0
        ));
    }

    #[test]
    fn inline_origin_helpers_reject_non_finite_geometry() {
        assert_eq!(ime_preedit_origin_x(f32::NAN, "abc", 1, 0, 4, 8.0), None);
        assert_eq!(ime_preedit_origin_x(10.0, "abc", 1, 0, 4, f32::NAN), None);
        assert_eq!(
            completion_preview_origin_x(10.0, 3, 0, 3, 6, f32::INFINITY),
            None
        );
        assert_eq!(completion_preview_origin_x(f32::NAN, 3, 0, 3, 6, 8.0), None);
    }

    #[test]
    fn code_lens_title_sanitizes_controls_spacing_and_length() {
        assert_eq!(code_lens_title(" Run\nTest\tNow "), "Run Test Now");
        assert_eq!(code_lens_title(""), "");
        assert_eq!(
            code_lens_title(
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyz"
            ),
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqr"
        );
    }

    #[test]
    fn code_lens_line_label_keeps_matching_non_empty_titles_in_order() {
        let lenses = [
            LspCodeLens {
                line: 3,
                column: 1,
                title: "References".to_owned(),
                command: Some("editor.showReferences".to_owned()),
                command_arguments: None,
                resolve_payload: None,
            },
            LspCodeLens {
                line: 4,
                column: 1,
                title: "Run Test".to_owned(),
                command: Some("rust-analyzer.runSingle".to_owned()),
                command_arguments: None,
                resolve_payload: None,
            },
            LspCodeLens {
                line: 4,
                column: 8,
                title: "  Debug\nTest ".to_owned(),
                command: Some("rust-analyzer.debugSingle".to_owned()),
                command_arguments: None,
                resolve_payload: None,
            },
            LspCodeLens {
                line: 4,
                column: 12,
                title: "\n\t".to_owned(),
                command: None,
                command_arguments: None,
                resolve_payload: None,
            },
        ];

        assert_eq!(
            code_lens_line_label(&lenses, 4),
            Some("Run Test  |  Debug Test".to_owned())
        );
        assert_eq!(
            code_lens_line_label(&lenses, 3),
            Some("References".to_owned())
        );
        assert_eq!(code_lens_line_label(&lenses, 2), None);
    }

    #[test]
    fn code_lens_line_display_keeps_raw_lsp_payload_with_sanitized_title() {
        let command_arguments = Arc::new(serde_json::Value::String("raw-args".to_owned()));
        let resolve_payload = Arc::new(serde_json::Value::String("raw-resolve".to_owned()));
        let lenses = [LspCodeLens {
            line: 4,
            column: 1,
            title: "  Run\nTest  ".to_owned(),
            command: Some("rust-analyzer.runSingle".to_owned()),
            command_arguments: Some(Arc::clone(&command_arguments)),
            resolve_payload: Some(Arc::clone(&resolve_payload)),
        }];

        let display = code_lens_line_display(&lenses, 4).unwrap();

        assert_eq!(display.label, "Run Test");
        assert_eq!(display.items.len(), 1);
        assert_eq!(
            &display.label[display.items[0].title_range.clone()],
            "Run Test"
        );
        assert_eq!(display.items[0].lens.title, "  Run\nTest  ");
        assert_eq!(
            display.items[0].lens.command.as_deref(),
            Some("rust-analyzer.runSingle")
        );
        assert!(Arc::ptr_eq(
            display.items[0].lens.command_arguments.as_ref().unwrap(),
            &command_arguments
        ));
        assert!(Arc::ptr_eq(
            display.items[0].lens.resolve_payload.as_ref().unwrap(),
            &resolve_payload
        ));
    }

    #[test]
    fn code_lens_command_hit_test_walks_sanitized_titles() {
        let command_arguments = Arc::new(serde_json::Value::String("raw-args".to_owned()));
        let resolve_payload = Arc::new(serde_json::Value::String("raw-resolve".to_owned()));
        let lenses = [
            LspCodeLens {
                line: 3,
                column: 1,
                title: "Before".to_owned(),
                command: Some("before".to_owned()),
                command_arguments: None,
                resolve_payload: None,
            },
            LspCodeLens {
                line: 4,
                column: 1,
                title: "  Run\nTest  ".to_owned(),
                command: Some("run".to_owned()),
                command_arguments: Some(Arc::clone(&command_arguments)),
                resolve_payload: Some(Arc::clone(&resolve_payload)),
            },
            LspCodeLens {
                line: 4,
                column: 2,
                title: "\n\t".to_owned(),
                command: Some("empty".to_owned()),
                command_arguments: None,
                resolve_payload: None,
            },
            LspCodeLens {
                line: 4,
                column: 3,
                title: "No Command".to_owned(),
                command: None,
                command_arguments: None,
                resolve_payload: None,
            },
            LspCodeLens {
                line: 4,
                column: 4,
                title: "Debug".to_owned(),
                command: Some("debug".to_owned()),
                command_arguments: None,
                resolve_payload: None,
            },
        ];
        let width = |text: &str| text.len() as f32;

        let hit = code_lens_command_at_x(&lenses, 4, 100.0, 104.0, width).unwrap();
        assert_eq!(hit.title, "  Run\nTest  ");
        assert_eq!(hit.command.as_deref(), Some("run"));
        assert!(Arc::ptr_eq(
            hit.command_arguments.as_ref().unwrap(),
            &command_arguments
        ));
        assert!(Arc::ptr_eq(
            hit.resolve_payload.as_ref().unwrap(),
            &resolve_payload
        ));
        assert!(code_lens_command_at_x(&lenses, 4, 100.0, 110.0, width).is_none());
        assert!(code_lens_command_at_x(&lenses, 4, 100.0, 116.0, width).is_none());

        let debug_hit = code_lens_command_at_x(&lenses, 4, 100.0, 130.0, width).unwrap();
        assert_eq!(debug_hit.command.as_deref(), Some("debug"));
    }

    #[test]
    fn code_lens_command_hit_test_keeps_row_cap() {
        let lenses = (0..MAX_CODE_LENSES_PER_ROW + 1)
            .map(|idx| LspCodeLens {
                line: 2,
                column: idx + 1,
                title: format!("Lens {idx}"),
                command: Some(format!("command.{idx}")),
                command_arguments: None,
                resolve_payload: None,
            })
            .collect::<Vec<_>>();
        let capped_out_pointer_x = (MAX_CODE_LENSES_PER_ROW * 2) as f32;

        assert!(code_lens_command_at_x(&lenses, 2, 0.0, capped_out_pointer_x, |_| 1.0).is_none());
    }

    #[test]
    fn inline_row_preparation_caps_hints_and_code_lenses_per_line() {
        let hints = (0..MAX_INLAY_HINTS_PER_ROW + 8)
            .map(|idx| LspInlayHint {
                line: 2,
                column: idx + 1,
                label: format!("hint{idx}"),
                kind: None,
            })
            .collect::<Vec<_>>();
        let hint_displays = inlay_hint_displays_for_line(&hints, 2, 0, false);

        assert_eq!(hint_displays.len(), MAX_INLAY_HINTS_PER_ROW);
        assert_eq!(hint_displays.first().unwrap().column, 1);
        assert_eq!(
            hint_displays.last().unwrap().column,
            MAX_INLAY_HINTS_PER_ROW
        );

        let lenses = (0..MAX_CODE_LENSES_PER_ROW + 8)
            .map(|idx| LspCodeLens {
                line: 2,
                column: idx + 1,
                title: format!("Lens {idx}"),
                command: Some(format!("command.{idx}")),
                command_arguments: None,
                resolve_payload: None,
            })
            .collect::<Vec<_>>();
        let display = code_lens_line_display(&lenses, 2).unwrap();

        assert_eq!(display.items.len(), MAX_CODE_LENSES_PER_ROW);
        assert_eq!(display.items.first().unwrap().lens.title, "Lens 0");
        assert_eq!(
            display.items.last().unwrap().lens.title,
            format!("Lens {}", MAX_CODE_LENSES_PER_ROW - 1)
        );
        assert_eq!(
            &display.label[display.items.last().unwrap().title_range.clone()],
            format!("Lens {}", MAX_CODE_LENSES_PER_ROW - 1)
        );
    }

    #[test]
    fn inlay_hints_for_line_slices_sorted_matching_range() {
        let hints = [
            inlay_hint(1, 1, "a"),
            inlay_hint(3, 2, "b"),
            inlay_hint(3, 6, "c"),
            inlay_hint(5, 1, "d"),
        ];

        assert_eq!(
            inlay_hints_for_line(&hints, 3)
                .iter()
                .map(|hint| hint.label.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c"]
        );
        assert!(inlay_hints_for_line(&hints, 2).is_empty());
        assert!(inlay_hints_for_line(&hints, 6).is_empty());
    }

    #[test]
    fn code_lenses_for_line_slices_sorted_matching_range() {
        let lenses = [
            code_lens(1, 1, "A"),
            code_lens(4, 1, "Run"),
            code_lens(4, 8, "Debug"),
            code_lens(6, 1, "Refs"),
        ];

        assert_eq!(
            code_lenses_for_line(&lenses, 4)
                .iter()
                .map(|lens| lens.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Run", "Debug"]
        );
        assert!(code_lenses_for_line(&lenses, 3).is_empty());
        assert!(code_lenses_for_line(&lenses, 7).is_empty());
    }

    #[test]
    fn code_lens_font_id_follows_editor_size_or_setting() {
        let default_font = code_lens_font_id("", 0, 20.0);
        assert_eq!(default_font.size, 18.0);
        assert_eq!(default_font.family, FontFamily::Monospace);

        let custom_font = code_lens_font_id("CodeLens", 11, 20.0);
        assert_eq!(custom_font.size, 11.0);
        assert_eq!(custom_font.family, FontFamily::Name("CodeLens".into()));
    }

    fn inlay_hint(line: usize, column: usize, label: &str) -> LspInlayHint {
        LspInlayHint {
            line,
            column,
            label: label.to_owned(),
            kind: None,
        }
    }

    fn code_lens(line: usize, column: usize, title: &str) -> LspCodeLens {
        LspCodeLens {
            line,
            column,
            title: title.to_owned(),
            command: None,
            command_arguments: None,
            resolve_payload: None,
        }
    }

    fn test_rect(width: f32) -> Rect {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(width, 20.0))
    }
}

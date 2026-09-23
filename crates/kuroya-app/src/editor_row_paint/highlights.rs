use crate::{
    editor_pane_rows::EditorRowContext,
    editor_pane_support::{DiagnosticTagKind, paint_char_range_highlight},
    editor_text_geometry::visual_column_for_char_offset,
    theme::{document_highlight_color, semantic_token_color},
};
use eframe::egui::{self, Color32, Stroke, pos2};
use std::{collections::BTreeSet, ops::Range};

const DOCUMENT_HIGHLIGHT_TINT_ALPHA: u8 = 60;

pub(super) fn paint_row_highlights(
    painter: &egui::Painter,
    rect: egui::Rect,
    snapshot_range: &Range<usize>,
    line_end_visible: bool,
    line_text: &str,
    row: &EditorRowContext<'_>,
) {
    for (range, _) in sorted_range_spans_before_snapshot_end(
        row.diagnostic_tag_spans,
        snapshot_range,
        |(range, _)| range,
    )
    .iter()
    .filter(|(range, kind)| {
        *kind == DiagnosticTagKind::Unused && range_overlaps_snapshot(range, snapshot_range)
    }) {
        paint_char_range_highlight(
            painter,
            rect,
            row.gutter_width,
            row.char_width,
            row.row_height,
            snapshot_range,
            line_text,
            row.tab_width,
            range,
            Color32::from_rgba_premultiplied(110, 116, 130, 36),
        );
    }

    let (_, semantic_token_ranges) = sorted_non_overlapping_range_spans_for_snapshot(
        row.semantic_token_ranges,
        snapshot_range,
        |(range, _, _)| range,
    );
    for (range, token_type, modifiers) in semantic_token_ranges {
        paint_char_range_highlight(
            painter,
            rect,
            row.gutter_width,
            row.char_width,
            row.row_height,
            snapshot_range,
            line_text,
            row.tab_width,
            range,
            semantic_token_color(token_type, modifiers),
        );
    }

    visit_unicode_highlight_ranges(
        snapshot_range,
        line_text,
        row.unicode_highlight_ambiguous_characters,
        row.unicode_highlight_invisible_characters,
        row.unicode_highlight_non_basic_ascii,
        row.unicode_highlight_allowed_characters,
        row.unicode_highlight_allowed_locales,
        |range, kind| {
            paint_char_range_highlight(
                painter,
                rect,
                row.gutter_width,
                row.char_width,
                row.row_height,
                snapshot_range,
                line_text,
                row.tab_width,
                &range,
                unicode_highlight_color(kind),
            );
            true
        },
    );

    for (range, kind) in sorted_range_spans_before_snapshot_end(
        row.document_highlight_ranges,
        snapshot_range,
        |(range, _)| range,
    )
    .iter()
    .filter(|(range, _)| range_overlaps_snapshot(range, snapshot_range))
    {
        paint_char_range_highlight(
            painter,
            rect,
            row.gutter_width,
            row.char_width,
            row.row_height,
            snapshot_range,
            line_text,
            row.tab_width,
            range,
            document_highlight_fill(*kind),
        );
    }

    let selection_corner_radius = selection_corner_radius(row.rounded_selection);
    for selection in row.selections {
        let Some(range) = selection_range_for_snapshot(*selection, snapshot_range) else {
            continue;
        };
        paint_selection_range_highlight(
            painter,
            rect,
            row.gutter_width,
            row.char_width,
            row.row_height,
            snapshot_range,
            line_end_visible,
            line_text,
            row.tab_width,
            &range,
            row.selection_bg_fill,
            selection_corner_radius,
        );
    }

    let (visible_find_match_start, visible_find_matches) =
        sorted_non_overlapping_range_spans_for_snapshot(
            row.find_matches,
            snapshot_range,
            |range| range,
        );
    for (visible_offset, range) in visible_find_matches.iter().enumerate() {
        let match_idx = visible_find_match_start + visible_offset;
        let color = if match_idx == row.active_find_match {
            translucent_highlight(row.warn_fg_color, 96)
        } else {
            translucent_highlight(row.warn_fg_color, 54)
        };
        paint_char_range_highlight(
            painter,
            rect,
            row.gutter_width,
            row.char_width,
            row.row_height,
            snapshot_range,
            line_text,
            row.tab_width,
            range,
            color,
        );
    }
}

fn sorted_range_spans_before_snapshot_end<'a, T>(
    spans: &'a [T],
    snapshot_range: &Range<usize>,
    range_for: impl Fn(&T) -> &Range<usize>,
) -> &'a [T] {
    let end = spans.partition_point(|span| range_for(span).start < snapshot_range.end);
    &spans[..end]
}

fn sorted_non_overlapping_range_spans_for_snapshot<'a, T>(
    spans: &'a [T],
    snapshot_range: &Range<usize>,
    range_for: impl Fn(&T) -> &Range<usize>,
) -> (usize, &'a [T]) {
    let start = spans.partition_point(|span| range_for(span).end <= snapshot_range.start);
    let end =
        start + spans[start..].partition_point(|span| range_for(span).start < snapshot_range.end);
    (start, &spans[start..end])
}

fn range_overlaps_snapshot(range: &Range<usize>, snapshot_range: &Range<usize>) -> bool {
    range.start < snapshot_range.end && range.end > snapshot_range.start
}

fn selection_range_for_snapshot(
    selection: kuroya_core::Selection,
    snapshot_range: &Range<usize>,
) -> Option<Range<usize>> {
    if selection.is_caret() {
        return None;
    }
    let range = selection.range();
    range_overlaps_snapshot(&range, snapshot_range).then_some(range)
}

fn selection_corner_radius(rounded_selection: bool) -> f32 {
    if rounded_selection { 2.0 } else { 0.0 }
}

fn paint_selection_range_highlight(
    painter: &egui::Painter,
    rect: egui::Rect,
    gutter_width: f32,
    char_width: f32,
    row_height: f32,
    snapshot_range: &Range<usize>,
    line_end_visible: bool,
    line_text: &str,
    tab_width: usize,
    range: &Range<usize>,
    color: Color32,
    corner_radius: f32,
) {
    let Some((col_start, col_end)) = selection_visual_columns_for_snapshot(
        range,
        snapshot_range,
        line_end_visible,
        line_text,
        tab_width,
    ) else {
        return;
    };
    painter.rect_filled(
        egui::Rect::from_min_size(
            pos2(
                rect.left() + gutter_width + col_start as f32 * char_width,
                rect.top() + 2.0,
            ),
            egui::vec2((col_end - col_start) as f32 * char_width, row_height - 4.0),
        ),
        corner_radius,
        color,
    );
}

fn selection_visual_columns_for_snapshot(
    range: &Range<usize>,
    snapshot_range: &Range<usize>,
    line_end_visible: bool,
    line_text: &str,
    tab_width: usize,
) -> Option<(usize, usize)> {
    let start = range.start.max(snapshot_range.start);
    let end = range.end.min(snapshot_range.end);
    if start >= end {
        return None;
    }

    let text_char_count = line_text.chars().count();
    let char_start = start
        .saturating_sub(snapshot_range.start)
        .min(text_char_count);
    let char_end = end
        .saturating_sub(snapshot_range.start)
        .min(text_char_count);
    let col_start = visual_column_for_char_offset(line_text, char_start, tab_width);
    let mut col_end = visual_column_for_char_offset(line_text, char_end, tab_width);
    let selection_reaches_line_ending = line_end_visible
        && end > snapshot_range.start.saturating_add(text_char_count)
        && char_end == text_char_count;
    if selection_reaches_line_ending {
        col_end = col_end.saturating_add(1);
    } else if col_end <= col_start {
        col_end = col_start.saturating_add(1);
    }

    (col_end > col_start).then_some((col_start, col_end))
}

pub(super) fn paint_row_deprecated_diagnostic_tags(
    painter: &egui::Painter,
    rect: egui::Rect,
    snapshot_range: &Range<usize>,
    line_text: &str,
    row: &EditorRowContext<'_>,
) {
    for range in
        deprecated_diagnostic_tag_ranges_for_snapshot(row.diagnostic_tag_spans, snapshot_range)
    {
        paint_diagnostic_tag_strikethrough(
            painter,
            rect,
            row.gutter_width,
            row.char_width,
            row.row_height,
            snapshot_range,
            line_text,
            row.tab_width,
            range,
            row.weak_text_color,
        );
    }
}

fn deprecated_diagnostic_tag_ranges_for_snapshot<'a>(
    spans: &'a [(Range<usize>, DiagnosticTagKind)],
    snapshot_range: &'a Range<usize>,
) -> impl Iterator<Item = &'a Range<usize>> + 'a {
    sorted_range_spans_before_snapshot_end(spans, snapshot_range, |(range, _)| range)
        .iter()
        .filter(move |(range, kind)| {
            *kind == DiagnosticTagKind::Deprecated && range_overlaps_snapshot(range, snapshot_range)
        })
        .map(|(range, _)| range)
}

fn paint_diagnostic_tag_strikethrough(
    painter: &egui::Painter,
    rect: egui::Rect,
    gutter_width: f32,
    char_width: f32,
    row_height: f32,
    snapshot_range: &Range<usize>,
    line_text: &str,
    tab_width: usize,
    range: &Range<usize>,
    color: Color32,
) {
    let start = range.start.max(snapshot_range.start);
    let end = range.end.min(snapshot_range.end);
    if start >= end {
        return;
    }

    let text_char_count = line_text.chars().count();
    let char_start = start
        .saturating_sub(snapshot_range.start)
        .min(text_char_count);
    let char_end = end
        .saturating_sub(snapshot_range.start)
        .min(text_char_count);
    let col_start = visual_column_for_char_offset(line_text, char_start, tab_width);
    let mut col_end = visual_column_for_char_offset(line_text, char_end, tab_width);
    if col_end <= col_start {
        col_end = col_start + 1;
    }

    let left = rect.left() + gutter_width + col_start as f32 * char_width;
    let right = (rect.left() + gutter_width + col_end as f32 * char_width).min(rect.right());
    if right <= left {
        return;
    }

    let y = rect.top() + row_height * 0.56;
    painter.line_segment([pos2(left, y), pos2(right, y)], Stroke::new(1.0_f32, color));
}

fn translucent_highlight(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn document_highlight_fill(kind: Option<u8>) -> Color32 {
    translucent_highlight(
        document_highlight_color(kind),
        DOCUMENT_HIGHLIGHT_TINT_ALPHA,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnicodeHighlightKind {
    Invisible,
    Ambiguous,
    NonBasicAscii,
}

#[cfg(test)]
pub(crate) fn unicode_highlight_ranges(
    snapshot_range: &Range<usize>,
    line_text: &str,
    ambiguous_characters: bool,
    invisible_characters: bool,
    non_basic_ascii: bool,
    allowed_characters: &BTreeSet<char>,
    allowed_locales: &BTreeSet<String>,
) -> Vec<(Range<usize>, UnicodeHighlightKind)> {
    let mut ranges = Vec::new();
    visit_unicode_highlight_ranges(
        snapshot_range,
        line_text,
        ambiguous_characters,
        invisible_characters,
        non_basic_ascii,
        allowed_characters,
        allowed_locales,
        |range, kind| {
            ranges.push((range, kind));
            true
        },
    );
    ranges
}

fn visit_unicode_highlight_ranges(
    snapshot_range: &Range<usize>,
    line_text: &str,
    ambiguous_characters: bool,
    invisible_characters: bool,
    non_basic_ascii: bool,
    allowed_characters: &BTreeSet<char>,
    allowed_locales: &BTreeSet<String>,
    mut visit: impl FnMut(Range<usize>, UnicodeHighlightKind) -> bool,
) {
    if !unicode_highlight_scan_needed(
        line_text,
        ambiguous_characters,
        invisible_characters,
        non_basic_ascii,
    ) {
        return;
    }

    for (column, ch) in line_text.chars().enumerate() {
        let Some(kind) = unicode_highlight_kind(
            ch,
            ambiguous_characters,
            invisible_characters,
            non_basic_ascii,
            allowed_characters,
            allowed_locales,
        ) else {
            continue;
        };
        if !visit(
            snapshot_range.start + column..snapshot_range.start + column + 1,
            kind,
        ) {
            return;
        }
    }
}

fn unicode_highlight_scan_needed(
    line_text: &str,
    ambiguous_characters: bool,
    invisible_characters: bool,
    non_basic_ascii: bool,
) -> bool {
    (ambiguous_characters || invisible_characters || non_basic_ascii) && !line_text.is_ascii()
}

pub(crate) fn unicode_highlight_kind(
    ch: char,
    ambiguous_characters: bool,
    invisible_characters: bool,
    non_basic_ascii: bool,
    allowed_characters: &BTreeSet<char>,
    allowed_locales: &BTreeSet<String>,
) -> Option<UnicodeHighlightKind> {
    if allowed_characters.contains(&ch) {
        return None;
    }

    if invisible_characters && is_invisible_unicode_character(ch) {
        Some(UnicodeHighlightKind::Invisible)
    } else if unicode_locale_allows_character(ch, allowed_locales) {
        None
    } else if ambiguous_characters && is_ambiguous_unicode_character(ch) {
        Some(UnicodeHighlightKind::Ambiguous)
    } else if non_basic_ascii && !ch.is_ascii() {
        Some(UnicodeHighlightKind::NonBasicAscii)
    } else {
        None
    }
}

fn unicode_highlight_color(kind: UnicodeHighlightKind) -> Color32 {
    match kind {
        UnicodeHighlightKind::Invisible => Color32::from_rgba_unmultiplied(107, 76, 42, 60),
        UnicodeHighlightKind::Ambiguous => Color32::from_rgba_unmultiplied(100, 79, 35, 60),
        UnicodeHighlightKind::NonBasicAscii => Color32::from_rgba_unmultiplied(65, 70, 94, 60),
    }
}

fn is_invisible_unicode_character(ch: char) -> bool {
    matches!(
        ch,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
    )
}

fn is_ambiguous_unicode_character(ch: char) -> bool {
    matches!(
        ch,
        '\u{00A0}'
            | '\u{0391}'
            | '\u{0392}'
            | '\u{0395}'
            | '\u{0396}'
            | '\u{0397}'
            | '\u{0399}'
            | '\u{039A}'
            | '\u{039C}'
            | '\u{039D}'
            | '\u{039F}'
            | '\u{03A1}'
            | '\u{03A4}'
            | '\u{03A5}'
            | '\u{03A7}'
            | '\u{0406}'
            | '\u{0408}'
            | '\u{0410}'
            | '\u{0412}'
            | '\u{0415}'
            | '\u{041A}'
            | '\u{041C}'
            | '\u{041D}'
            | '\u{041E}'
            | '\u{0420}'
            | '\u{0421}'
            | '\u{0422}'
            | '\u{0423}'
            | '\u{0425}'
            | '\u{0430}'
            | '\u{0435}'
            | '\u{043E}'
            | '\u{0440}'
            | '\u{0441}'
            | '\u{0443}'
            | '\u{0445}'
            | '\u{0456}'
            | '\u{0458}'
            | '\u{04CF}'
    )
}

fn unicode_locale_allows_character(ch: char, allowed_languages: &BTreeSet<String>) -> bool {
    allowed_languages
        .iter()
        .any(|language| unicode_language_allows_character(language, ch))
}

fn unicode_language_allows_character(language: &str, ch: char) -> bool {
    match language {
        "ja" => is_japanese_character(ch) || is_cjk_character(ch),
        "zh" | "yue" => is_cjk_character(ch),
        "ko" => is_hangul_character(ch) || is_cjk_character(ch),
        "el" => is_greek_character(ch),
        "ru" | "uk" | "bg" | "be" | "mk" | "sr" => is_cyrillic_character(ch),
        "ar" | "fa" | "ur" | "ps" => is_arabic_character(ch),
        "he" | "iw" => is_hebrew_character(ch),
        "hi" | "mr" | "ne" | "sa" => is_devanagari_character(ch),
        "th" => is_thai_character(ch),
        language if is_latin_locale(language) => is_latin_extended_character(ch),
        _ => false,
    }
}

fn is_latin_locale(language: &str) -> bool {
    matches!(
        language,
        "ca" | "cs"
            | "cy"
            | "da"
            | "de"
            | "en"
            | "es"
            | "et"
            | "eu"
            | "fi"
            | "fr"
            | "ga"
            | "gl"
            | "hr"
            | "hu"
            | "is"
            | "it"
            | "lt"
            | "lv"
            | "mt"
            | "nl"
            | "no"
            | "pl"
            | "pt"
            | "ro"
            | "sk"
            | "sl"
            | "sq"
            | "sv"
            | "tr"
            | "vi"
    )
}

fn is_latin_extended_character(ch: char) -> bool {
    matches!(ch, '\u{00C0}'..='\u{024F}' | '\u{1E00}'..='\u{1EFF}')
}

fn is_japanese_character(ch: char) -> bool {
    matches!(
        ch,
        '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF00}'..='\u{FFEF}'
    )
}

fn is_cjk_character(ch: char) -> bool {
    matches!(
        ch,
        '\u{3000}'..='\u{303F}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
    )
}

fn is_hangul_character(ch: char) -> bool {
    matches!(
        ch,
        '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{AC00}'..='\u{D7AF}'
    )
}

fn is_greek_character(ch: char) -> bool {
    matches!(ch, '\u{0370}'..='\u{03FF}' | '\u{1F00}'..='\u{1FFF}')
}

fn is_cyrillic_character(ch: char) -> bool {
    matches!(
        ch,
        '\u{0400}'..='\u{052F}' | '\u{2DE0}'..='\u{2DFF}' | '\u{A640}'..='\u{A69F}'
    )
}

fn is_arabic_character(ch: char) -> bool {
    matches!(
        ch,
        '\u{0600}'..='\u{06FF}' | '\u{0750}'..='\u{077F}' | '\u{08A0}'..='\u{08FF}'
    )
}

fn is_hebrew_character(ch: char) -> bool {
    matches!(ch, '\u{0590}'..='\u{05FF}')
}

fn is_devanagari_character(ch: char) -> bool {
    matches!(ch, '\u{0900}'..='\u{097F}')
}

fn is_thai_character(ch: char) -> bool {
    matches!(ch, '\u{0E00}'..='\u{0E7F}')
}

#[cfg(test)]
mod tests {
    use super::{
        Color32, DOCUMENT_HIGHLIGHT_TINT_ALPHA, UnicodeHighlightKind,
        deprecated_diagnostic_tag_ranges_for_snapshot, document_highlight_fill,
        range_overlaps_snapshot, selection_corner_radius, selection_range_for_snapshot,
        selection_visual_columns_for_snapshot, sorted_non_overlapping_range_spans_for_snapshot,
        sorted_range_spans_before_snapshot_end, translucent_highlight, unicode_highlight_color,
        unicode_highlight_kind, unicode_highlight_ranges,
    };
    use crate::editor_pane_support::DiagnosticTagKind;
    use kuroya_core::Selection;
    use std::collections::BTreeSet;

    #[test]
    fn unicode_highlight_kind_follows_visibility_settings() {
        assert_eq!(
            unicode_highlight_kind(
                '\u{200B}',
                true,
                true,
                false,
                &BTreeSet::new(),
                &BTreeSet::new()
            ),
            Some(UnicodeHighlightKind::Invisible)
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{0391}',
                true,
                true,
                false,
                &BTreeSet::new(),
                &BTreeSet::new()
            ),
            Some(UnicodeHighlightKind::Ambiguous)
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{00E9}',
                true,
                true,
                true,
                &BTreeSet::new(),
                &BTreeSet::new()
            ),
            Some(UnicodeHighlightKind::NonBasicAscii)
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{00E9}',
                true,
                true,
                false,
                &BTreeSet::new(),
                &BTreeSet::new()
            ),
            None
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{0391}',
                false,
                true,
                false,
                &BTreeSet::new(),
                &BTreeSet::new()
            ),
            None
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{200B}',
                true,
                false,
                false,
                &BTreeSet::new(),
                &BTreeSet::new()
            ),
            None
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{0391}',
                true,
                true,
                true,
                &BTreeSet::from(['\u{0391}']),
                &BTreeSet::new()
            ),
            None
        );
    }

    #[test]
    fn unicode_highlight_kind_respects_allowed_locale_scripts() {
        assert_eq!(
            unicode_highlight_kind(
                '\u{0391}',
                true,
                true,
                true,
                &BTreeSet::new(),
                &BTreeSet::from(["el".to_owned()])
            ),
            None
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{00E9}',
                true,
                true,
                true,
                &BTreeSet::new(),
                &BTreeSet::from(["fr".to_owned()])
            ),
            None
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{3042}',
                true,
                true,
                true,
                &BTreeSet::new(),
                &BTreeSet::from(["ja".to_owned()])
            ),
            None
        );
        assert_eq!(
            unicode_highlight_kind(
                '\u{200B}',
                true,
                true,
                true,
                &BTreeSet::new(),
                &BTreeSet::from(["ja".to_owned()])
            ),
            Some(UnicodeHighlightKind::Invisible)
        );
    }

    #[test]
    fn unicode_highlight_ranges_return_absolute_character_ranges() {
        assert_eq!(
            unicode_highlight_ranges(
                &(10..14),
                "a\u{200B}\u{0391}\u{00E9}",
                true,
                true,
                true,
                &BTreeSet::from(['\u{0391}']),
                &BTreeSet::new()
            ),
            vec![
                (11..12, UnicodeHighlightKind::Invisible),
                (13..14, UnicodeHighlightKind::NonBasicAscii),
            ]
        );
    }

    #[test]
    fn selection_corner_radius_follows_setting() {
        assert_eq!(selection_corner_radius(true), 2.0);
        assert_eq!(selection_corner_radius(false), 0.0);
    }

    #[test]
    fn selection_visual_columns_include_visible_line_ending_cell() {
        assert_eq!(
            selection_visual_columns_for_snapshot(&(2..5), &(0..6), true, "abcd", 4),
            Some((2, 5))
        );
        assert_eq!(
            selection_visual_columns_for_snapshot(&(2..5), &(0..6), false, "abcd", 4),
            Some((2, 4))
        );
        assert_eq!(
            selection_visual_columns_for_snapshot(&(4..5), &(0..5), true, "abcd", 4),
            Some((4, 5))
        );
        assert_eq!(
            selection_visual_columns_for_snapshot(&(0..1), &(0..1), true, "", 4),
            Some((0, 1))
        );
    }

    #[test]
    fn selection_range_for_snapshot_skips_off_row_selections() {
        assert_eq!(
            selection_range_for_snapshot(
                Selection {
                    anchor: 7,
                    cursor: 7
                },
                &(5..10)
            ),
            None
        );
        assert_eq!(
            selection_range_for_snapshot(
                Selection {
                    anchor: 3,
                    cursor: 8
                },
                &(5..10)
            ),
            Some(3..8)
        );
        assert_eq!(
            selection_range_for_snapshot(
                Selection {
                    anchor: 12,
                    cursor: 6
                },
                &(5..10)
            ),
            Some(6..12)
        );
        assert_eq!(
            selection_range_for_snapshot(
                Selection {
                    anchor: 0,
                    cursor: 5
                },
                &(5..10)
            ),
            None
        );
        assert_eq!(
            selection_range_for_snapshot(
                Selection {
                    anchor: 10,
                    cursor: 12
                },
                &(5..10)
            ),
            None
        );
    }

    #[test]
    fn sorted_range_spans_before_snapshot_end_bounds_future_ranges() {
        let ranges = [0..3, 5..9, 10..12, 20..22];

        assert_eq!(
            sorted_range_spans_before_snapshot_end(&ranges, &(7..11), |range| range),
            &[0..3, 5..9, 10..12]
        );
    }

    #[test]
    fn sorted_non_overlapping_range_spans_for_snapshot_bounds_both_edges() {
        let ranges = [0..3, 5..8, 8..12, 12..14, 20..22];

        let (start, visible) =
            sorted_non_overlapping_range_spans_for_snapshot(&ranges, &(7..12), |range| range);

        assert_eq!(start, 1);
        assert_eq!(visible, &[5..8, 8..12]);
    }

    #[test]
    fn deprecated_diagnostic_tags_use_snapshot_bounded_span_slice() {
        let spans = [
            (0..2, DiagnosticTagKind::Deprecated),
            (4..8, DiagnosticTagKind::Unused),
            (8..12, DiagnosticTagKind::Deprecated),
            (20..24, DiagnosticTagKind::Deprecated),
        ];

        let visible = deprecated_diagnostic_tag_ranges_for_snapshot(&spans, &(7..10))
            .cloned()
            .collect::<Vec<_>>();

        assert_eq!(visible, vec![8..12]);
    }

    #[test]
    fn range_overlaps_snapshot_excludes_touching_edges() {
        assert!(!range_overlaps_snapshot(&(0..5), &(5..10)));
        assert!(range_overlaps_snapshot(&(4..6), &(5..10)));
        assert!(range_overlaps_snapshot(&(8..12), &(5..10)));
        assert!(!range_overlaps_snapshot(&(10..12), &(5..10)));
    }

    #[test]
    fn translucent_highlight_uses_straight_alpha_channels() {
        let source = Color32::from_rgb(110, 116, 130);

        assert_eq!(
            translucent_highlight(source, 96),
            Color32::from_rgba_unmultiplied(110, 116, 130, 96)
        );

        assert_ne!(
            translucent_highlight(source, 96),
            Color32::from_rgba_premultiplied(110, 116, 130, 96)
        );
    }

    #[test]
    fn unicode_highlight_colors_are_translucent_straight_alpha_tints() {
        let opaque_sources = [
            (
                UnicodeHighlightKind::Invisible,
                Color32::from_rgb(107, 76, 42),
            ),
            (
                UnicodeHighlightKind::Ambiguous,
                Color32::from_rgb(100, 79, 35),
            ),
            (
                UnicodeHighlightKind::NonBasicAscii,
                Color32::from_rgb(65, 70, 94),
            ),
        ];

        for (kind, opaque) in opaque_sources {
            let tinted = unicode_highlight_color(kind);

            assert_eq!(tinted.a(), 60);
            assert_eq!(
                tinted,
                Color32::from_rgba_unmultiplied(opaque.r(), opaque.g(), opaque.b(), 60)
            );
            assert_ne!(
                tinted,
                Color32::from_rgba_premultiplied(opaque.r(), opaque.g(), opaque.b(), 60)
            );
        }
    }

    #[test]
    fn document_highlight_fill_tints_the_opaque_theme_box() {
        let fill = document_highlight_fill(None);

        assert_eq!(fill.a(), DOCUMENT_HIGHLIGHT_TINT_ALPHA);
        assert_ne!(fill, Color32::from_rgb(48, 56, 72));
    }
}

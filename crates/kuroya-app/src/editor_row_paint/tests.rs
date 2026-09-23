use super::{
    BACKGROUND_IMAGE_LINE_HIGHLIGHT_MAX_ALPHA, BACKGROUND_IMAGE_SELECTION_MAX_ALPHA,
    BACKGROUND_IMAGE_STICKY_SCROLL_MAX_ALPHA, MAX_WHITESPACE_SELECTION_RANGES_PER_ROW,
    RowTextMetrics, SelectionRangeCursor, VisualColumnScanner, WhitespaceMarker,
    WhitespaceMarkerKind, WhitespaceMarkerPaintStrategy, active_indent_guide_column,
    active_indent_guide_column_for_buffer, color_decorators_visible, column_ruler_color,
    control_character_label, diff_empty_decoration_fill, diff_empty_decoration_visible,
    diff_move_decoration_fill, diff_move_decoration_stripe_fill, diff_move_decoration_visible,
    editor_row_line_snapshot, editor_row_wrap_width, editor_selection_fill,
    editor_sticky_scroll_fill, folded_region_highlight_fill, folded_region_highlight_visible,
    hex_color_decorations, indent_guide_color, injected_language_render_rect,
    injected_language_render_rect_with_scanner, leading_indent_guide_columns,
    limit_layout_job_line_rendering, line_highlight_fill, line_highlight_visible,
    merge_conflict_line_fill, parse_hex_color, rendered_trailing_whitespace_start,
    row_text_metrics, selection_ranges_for_snapshot, trailing_whitespace_start,
    visible_whitespace_marker, whitespace_marker_kind_for_selection_ranges,
    whitespace_marker_label, whitespace_marker_paint_strategy, whitespace_marker_scan_needed,
    whitespace_marker_trailing_start, whitespace_selection_marker_scan_needed,
    whitespace_selection_ranges_for_marker_scan, whitespace_selection_ranges_for_snapshot,
};
use crate::syntax_tree_cache::TreeSitterInjection;
use eframe::egui::{Color32, Rect, TextFormat, pos2, text::LayoutJob};
use kuroya_core::{
    EditorColorDecoratorsActivatedOn, EditorDefaultColorDecorators,
    EditorExperimentalWhitespaceRendering, EditorRenderWhitespace, EditorWordWrap, LanguageId,
    MergeConflictLineKind, Selection, TextBuffer,
};
use std::ops::Range;

#[test]
fn indent_guides_track_space_and_tab_indentation() {
    assert_eq!(
        leading_indent_guide_columns("        let x = 1;", 4),
        [4, 8]
    );
    assert_eq!(leading_indent_guide_columns("\t  let x = 1;", 4), [4]);
    assert!(leading_indent_guide_columns("let x = 1;", 4).is_empty());
}

#[test]
fn active_indent_guide_follows_deepest_guide_before_cursor() {
    assert_eq!(active_indent_guide_column("        let x = 1;", 0, 4), None);
    assert_eq!(
        active_indent_guide_column("        let x = 1;", 4, 4),
        Some(4)
    );
    assert_eq!(
        active_indent_guide_column("        let x = 1;", 8, 4),
        Some(8)
    );
    assert_eq!(
        active_indent_guide_column("        let x = 1;", 14, 4),
        Some(8)
    );
    assert_eq!(active_indent_guide_column("\t  let x = 1;", 1, 4), Some(4));
    assert_eq!(active_indent_guide_column("let x = 1;", 4, 4), None);
}

#[test]
fn active_indent_guide_for_buffer_uses_current_cursor_line() {
    let mut buffer = TextBuffer::from_text(1, None, "root\n        child".to_owned());
    buffer.set_single_cursor(buffer.line_column_to_char(1, 8));

    assert_eq!(active_indent_guide_column_for_buffer(&buffer, 4), Some(8));
}

#[test]
fn editor_row_line_snapshot_respects_rendering_limit() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\n".to_owned());

    let capped = editor_row_line_snapshot(&buffer, 0, 3).unwrap();
    assert_eq!(capped.snapshot.text, "abc");
    assert_eq!(capped.snapshot.char_range, 0..3);
    assert!(!capped.line_end_visible);

    let uncapped = editor_row_line_snapshot(&buffer, 0, -1).unwrap();
    assert_eq!(uncapped.snapshot.text, "abcdef\n");
    assert_eq!(uncapped.snapshot.char_range, 0..7);
    assert!(uncapped.line_end_visible);
}

#[test]
fn editor_row_line_snapshot_detects_capped_line_end_with_bounded_probe() {
    let exact = TextBuffer::from_text(1, None, "abc\n".to_owned());
    let longer = TextBuffer::from_text(2, None, "abcd\n".to_owned());

    let exact_cap = editor_row_line_snapshot(&exact, 0, 3).unwrap();
    assert_eq!(exact_cap.snapshot.text, "abc");
    assert!(exact_cap.line_end_visible);

    let longer_cap = editor_row_line_snapshot(&longer, 0, 3).unwrap();
    assert_eq!(longer_cap.snapshot.text, "abc");
    assert!(!longer_cap.line_end_visible);
}

#[test]
fn injected_language_render_rect_skips_non_overlapping_ranges() {
    let rect = row_rect();
    let text = "abcdef";

    assert_eq!(
        injected_language_render_rect(rect, &(10..16), text, 20.0, 4, 8.0, &injection(4..9)),
        None
    );
    assert_eq!(
        injected_language_render_rect(rect, &(10..16), text, 20.0, 4, 8.0, &injection(16..20)),
        None
    );
}

#[test]
fn injected_language_render_rect_clips_to_snapshot_and_row_rect() {
    let rect = row_rect();
    let text = "abcdef";
    let render_rect =
        injected_language_render_rect(rect, &(10..16), text, 20.0, 4, 8.0, &injection(8..14))
            .unwrap();

    assert_rect(render_rect, 20.0, 3.0, 52.0, 17.0);

    let clipped_right = injected_language_render_rect(
        Rect::from_min_max(pos2(0.0, 0.0), pos2(44.0, 20.0)),
        &(10..16),
        text,
        20.0,
        4,
        8.0,
        &injection(12..16),
    )
    .unwrap();

    assert_rect(clipped_right, 36.0, 3.0, 44.0, 17.0);
}

#[test]
fn injected_language_render_rect_uses_visual_columns_for_tabs_and_zero_width_marks() {
    let rect = row_rect();
    let text = "a\tb\u{0301}c";
    let render_rect = injected_language_render_rect(
        rect,
        &(0..text.chars().count()),
        text,
        20.0,
        4,
        8.0,
        &injection(1..4),
    )
    .unwrap();

    assert_rect(render_rect, 28.0, 3.0, 60.0, 17.0);
}

#[test]
fn injected_language_render_rect_handles_absolute_tabs_and_zero_width_only_ranges() {
    let tab_text = "\tab";
    let tab_rect = injected_language_render_rect(
        row_rect(),
        &(10..13),
        tab_text,
        20.0,
        4,
        8.0,
        &injection(11..13),
    )
    .unwrap();
    assert_rect(tab_rect, 52.0, 3.0, 68.0, 17.0);

    let zero_width_text = "e\u{0301}x";
    let zero_width_rect = injected_language_render_rect(
        row_rect(),
        &(0..3),
        zero_width_text,
        20.0,
        4,
        8.0,
        &injection(1..2),
    )
    .unwrap();
    assert_rect(zero_width_rect, 28.0, 3.0, 36.0, 17.0);
}

#[test]
fn visual_column_scanner_handles_forward_and_out_of_order_offsets() {
    let mut scanner = VisualColumnScanner::new("a\tb\u{0301}c", 4);

    assert_eq!(scanner.visual_columns_for_offsets(1, 4), (1, 5));
    assert_eq!(scanner.visual_columns_for_offsets(0, 2), (0, 4));
    assert_eq!(scanner.visual_columns_for_offsets(4, 5), (5, 6));
}

#[test]
fn injected_language_render_rects_share_visual_column_scanner() {
    let rect = row_rect();
    let text = "a\tb\u{0301}c";
    let text_len = text.chars().count();
    let mut scanner = VisualColumnScanner::new(text, 4);

    let first = injected_language_render_rect_with_scanner(
        rect,
        &(0..text_len),
        text_len,
        20.0,
        8.0,
        &injection(0..2),
        &mut scanner,
    )
    .unwrap();
    let second = injected_language_render_rect_with_scanner(
        rect,
        &(0..text_len),
        text_len,
        20.0,
        8.0,
        &injection(2..5),
        &mut scanner,
    )
    .unwrap();
    let out_of_order = injected_language_render_rect_with_scanner(
        rect,
        &(0..text_len),
        text_len,
        20.0,
        8.0,
        &injection(1..3),
        &mut scanner,
    )
    .unwrap();

    assert_rect(first, 20.0, 3.0, 52.0, 17.0);
    assert_rect(second, 52.0, 3.0, 68.0, 17.0);
    assert_rect(out_of_order, 28.0, 3.0, 60.0, 17.0);
}

#[test]
fn injected_language_render_rect_ignores_trimmed_line_endings() {
    let buffer = TextBuffer::from_text(1, None, "abc\r\n".to_owned());
    let snapshot = editor_row_line_snapshot(&buffer, 0, -1).unwrap();
    let text = snapshot
        .snapshot
        .text
        .trim_end_matches(['\r', '\n'])
        .to_owned();

    assert_eq!(snapshot.snapshot.char_range, 0..5);
    assert_eq!(text, "abc");
    assert_eq!(
        injected_language_render_rect(
            row_rect(),
            &snapshot.snapshot.char_range,
            &text,
            20.0,
            4,
            8.0,
            &injection(3..5)
        ),
        None
    );
}

#[test]
fn injected_language_render_rect_respects_truncated_line_snapshots() {
    let rect = row_rect();
    let full_text = "abcdef";
    let snapshot_text = &full_text[..3];

    let clipped =
        injected_language_render_rect(rect, &(0..3), snapshot_text, 20.0, 4, 8.0, &injection(1..6))
            .unwrap();

    assert_rect(clipped, 28.0, 3.0, 44.0, 17.0);
    assert_eq!(
        injected_language_render_rect(rect, &(0..3), snapshot_text, 20.0, 4, 8.0, &injection(3..6)),
        None
    );
}

#[test]
fn control_character_labels_cover_common_control_codes() {
    assert_eq!(control_character_label('\u{0000}'), Some("NUL"));
    assert_eq!(control_character_label('\u{001B}'), Some("ESC"));
    assert_eq!(control_character_label('\t'), None);
    assert_eq!(control_character_label('a'), None);
}

#[test]
fn line_highlight_can_require_focus() {
    assert!(line_highlight_visible(false, false));
    assert!(line_highlight_visible(true, true));
    assert!(!line_highlight_visible(true, false));
}

#[test]
fn editor_row_chrome_colors_follow_theme_visuals() {
    let mut visuals = eframe::egui::Visuals::light();
    visuals.widgets.active.weak_bg_fill = Color32::from_rgb(208, 215, 224);
    visuals.widgets.noninteractive.bg_stroke.color = Color32::from_rgb(178, 188, 200);
    visuals.widgets.inactive.bg_stroke.color = Color32::from_rgb(188, 198, 210);
    visuals.selection.stroke.color = Color32::from_rgb(47, 111, 237);

    assert_eq!(
        line_highlight_fill(&visuals, false),
        visuals.widgets.active.weak_bg_fill
    );
    assert_eq!(
        line_highlight_fill(&visuals, true).a(),
        BACKGROUND_IMAGE_LINE_HIGHLIGHT_MAX_ALPHA
    );
    assert_eq!(
        editor_selection_fill(Color32::from_rgb(20, 90, 180), true).a(),
        BACKGROUND_IMAGE_SELECTION_MAX_ALPHA
    );
    assert_eq!(
        editor_sticky_scroll_fill(Color32::from_rgb(20, 30, 40), true).a(),
        BACKGROUND_IMAGE_STICKY_SCROLL_MAX_ALPHA
    );
    assert_eq!(
        column_ruler_color(&visuals),
        visuals.widgets.noninteractive.bg_stroke.color
    );
    assert_eq!(
        indent_guide_color(&visuals, false),
        visuals.widgets.inactive.bg_stroke.color
    );
    assert_eq!(
        indent_guide_color(&visuals, true),
        visuals.selection.stroke.color
    );
}

#[test]
fn editor_ime_output_requires_editable_text_input() {
    assert!(super::editor_ime_output_enabled(true, false));
    assert!(!super::editor_ime_output_enabled(false, false));
    assert!(!super::editor_ime_output_enabled(true, true));
}

#[test]
fn folded_region_highlight_follows_setting_and_fold_state() {
    assert!(folded_region_highlight_visible(true, true));
    assert!(!folded_region_highlight_visible(false, true));
    assert!(!folded_region_highlight_visible(true, false));
    assert!(folded_region_highlight_fill(Color32::from_rgb(126, 136, 150)).a() > 0);
}

#[test]
fn folded_region_highlight_fill_uses_straight_alpha_channels() {
    let base = Color32::from_rgb(126, 136, 150);

    assert_eq!(
        folded_region_highlight_fill(base),
        Color32::from_rgba_unmultiplied(126, 136, 150, 44)
    );

    assert_ne!(
        folded_region_highlight_fill(base),
        Color32::from_rgba_premultiplied(126, 136, 150, 44)
    );
}

#[test]
fn diff_empty_decoration_visible_matches_diff_empty_lines() {
    assert!(diff_empty_decoration_visible(true, true, "+"));
    assert!(diff_empty_decoration_visible(true, true, "-"));
    assert!(diff_empty_decoration_fill("+").a() > 0);
    assert!(diff_empty_decoration_fill("-").a() > 0);
    assert!(!diff_empty_decoration_visible(false, true, "+"));
    assert!(!diff_empty_decoration_visible(true, false, "+"));
    assert!(!diff_empty_decoration_visible(true, true, "+text"));
    assert!(!diff_empty_decoration_visible(true, true, "-text"));
    assert!(!diff_empty_decoration_visible(true, true, " context"));
}

#[test]
fn color_decorators_visibility_follows_editor_settings() {
    assert!(!color_decorators_visible(
        false,
        EditorDefaultColorDecorators::Always,
        EditorColorDecoratorsActivatedOn::Hover,
        true
    ));
    assert!(!color_decorators_visible(
        true,
        EditorDefaultColorDecorators::Never,
        EditorColorDecoratorsActivatedOn::Hover,
        true
    ));
    assert!(color_decorators_visible(
        true,
        EditorDefaultColorDecorators::Always,
        EditorColorDecoratorsActivatedOn::Click,
        false
    ));
    assert!(color_decorators_visible(
        true,
        EditorDefaultColorDecorators::Auto,
        EditorColorDecoratorsActivatedOn::Hover,
        true
    ));
    assert!(!color_decorators_visible(
        true,
        EditorDefaultColorDecorators::Auto,
        EditorColorDecoratorsActivatedOn::Hover,
        false
    ));
}

#[test]
fn hex_color_decorations_find_bounded_css_hex_literals() {
    let decorations = hex_color_decorations("\u{03bb} fg #ff00aa bg #1234567 #bad", 1);

    assert_eq!(decorations.len(), 1);
    assert_eq!(decorations[0].column, 5);
    assert_eq!(
        decorations[0].color,
        eframe::egui::Color32::from_rgb(0xff, 0x00, 0xaa)
    );
    assert_eq!(
        parse_hex_color(&['1', '2', '3', 'a', 'B', 'c']),
        Some(eframe::egui::Color32::from_rgb(0x12, 0x3a, 0xbc))
    );
    assert!(parse_hex_color(&['z', '2', '3', 'a', 'B', 'c']).is_none());
}

#[test]
fn diff_move_decoration_visible_requires_diff_patch_moved_line() {
    assert!(diff_move_decoration_visible(true, true));
    assert!(!diff_move_decoration_visible(false, true));
    assert!(!diff_move_decoration_visible(true, false));
    assert!(diff_move_decoration_fill().a() > 0);
    assert_eq!(diff_move_decoration_stripe_fill().a(), u8::MAX);
}

#[test]
fn editor_row_wrap_width_follows_word_wrap_setting() {
    assert_eq!(
        editor_row_wrap_width(600.0, 80.0, EditorWordWrap::On, 80, 8.0),
        520.0
    );
    assert_eq!(
        editor_row_wrap_width(160.0, 80.0, EditorWordWrap::On, 80, 8.0),
        120.0
    );
    assert_eq!(
        editor_row_wrap_width(600.0, 80.0, EditorWordWrap::WordWrapColumn, 72, 8.0),
        576.0
    );
    assert_eq!(
        editor_row_wrap_width(600.0, 80.0, EditorWordWrap::Bounded, 40, 8.0),
        320.0
    );
    assert_eq!(
        editor_row_wrap_width(240.0, 80.0, EditorWordWrap::Bounded, 80, 8.0),
        160.0
    );
    assert!(editor_row_wrap_width(600.0, 80.0, EditorWordWrap::Off, 80, 8.0).is_infinite());
}

#[test]
fn editor_row_wrap_width_clamps_invalid_geometry() {
    assert_eq!(
        editor_row_wrap_width(f32::NAN, 80.0, EditorWordWrap::On, 80, 8.0),
        120.0
    );
    assert_eq!(
        editor_row_wrap_width(f32::INFINITY, 80.0, EditorWordWrap::On, 80, 8.0),
        120.0
    );
    assert_eq!(
        editor_row_wrap_width(80.0, f32::INFINITY, EditorWordWrap::Bounded, 80, 8.0),
        120.0
    );

    let huge_column = editor_row_wrap_width(
        600.0,
        80.0,
        EditorWordWrap::WordWrapColumn,
        usize::MAX,
        f32::MAX,
    );
    assert_eq!(huge_column, f32::MAX);
    assert!(huge_column.is_finite());
}

#[test]
fn layout_job_line_rendering_limit_truncates_text_and_sections() {
    let mut job = LayoutJob::default();
    job.append("ab", 0.0, TextFormat::default());
    job.append("cdef", 0.0, TextFormat::default());

    limit_layout_job_line_rendering(&mut job, 4);

    assert_eq!(job.text, "abcd");
    assert_eq!(job.sections.len(), 2);
    assert_eq!(job.sections[0].byte_range, 0..2);
    assert_eq!(job.sections[1].byte_range, 2..4);
}

#[test]
fn layout_job_line_rendering_limit_handles_unicode_and_disabled_limit() {
    let mut job = LayoutJob::default();
    job.append("a\u{00e9}z", 0.0, TextFormat::default());

    limit_layout_job_line_rendering(&mut job, 2);
    assert_eq!(job.text, "a\u{00e9}");
    assert_eq!(job.sections[0].byte_range, 0..3);

    limit_layout_job_line_rendering(&mut job, -1);
    assert_eq!(job.text, "a\u{00e9}");
}

#[test]
fn merge_conflict_line_fills_distinguish_marker_and_sides() {
    assert_ne!(
        merge_conflict_line_fill(MergeConflictLineKind::Start),
        merge_conflict_line_fill(MergeConflictLineKind::Current)
    );
    assert_ne!(
        merge_conflict_line_fill(MergeConflictLineKind::Current),
        merge_conflict_line_fill(MergeConflictLineKind::Incoming)
    );
    assert_eq!(
        merge_conflict_line_fill(MergeConflictLineKind::Start),
        merge_conflict_line_fill(MergeConflictLineKind::Separator)
    );
}

#[test]
fn whitespace_marker_labels_follow_render_modes() {
    let selections = [Selection {
        anchor: 11,
        cursor: 13,
    }];

    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::None,
            ' ',
            0,
            10,
            3,
            false,
            false,
            &selections,
        ),
        None
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::All,
            '\t',
            1,
            11,
            3,
            false,
            false,
            &selections,
        ),
        Some(">")
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Selection,
            ' ',
            1,
            11,
            3,
            false,
            false,
            &selections,
        ),
        Some(".")
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Selection,
            ' ',
            3,
            13,
            3,
            false,
            false,
            &selections,
        ),
        None
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Trailing,
            ' ',
            3,
            13,
            3,
            false,
            false,
            &selections,
        ),
        Some(".")
    );
}

#[test]
fn experimental_whitespace_rendering_selects_marker_strategy() {
    assert_eq!(
        whitespace_marker_paint_strategy(
            EditorExperimentalWhitespaceRendering::Svg,
            EditorRenderWhitespace::All,
        ),
        Some(WhitespaceMarkerPaintStrategy::Svg)
    );
    assert_eq!(
        whitespace_marker_paint_strategy(
            EditorExperimentalWhitespaceRendering::Font,
            EditorRenderWhitespace::All,
        ),
        Some(WhitespaceMarkerPaintStrategy::Font)
    );
    assert_eq!(
        whitespace_marker_paint_strategy(
            EditorExperimentalWhitespaceRendering::Off,
            EditorRenderWhitespace::All,
        ),
        None
    );
    assert_eq!(
        whitespace_marker_paint_strategy(
            EditorExperimentalWhitespaceRendering::Svg,
            EditorRenderWhitespace::None,
        ),
        None
    );
}

#[test]
fn experimental_whitespace_rendering_off_suppresses_visible_markers() {
    let selections: [Selection; 0] = [];

    assert_eq!(
        visible_whitespace_marker(
            EditorExperimentalWhitespaceRendering::Off,
            EditorRenderWhitespace::All,
            ' ',
            0,
            0,
            0,
            false,
            false,
            &selections,
        ),
        None
    );
    assert_eq!(
        visible_whitespace_marker(
            EditorExperimentalWhitespaceRendering::Font,
            EditorRenderWhitespace::All,
            ' ',
            0,
            0,
            0,
            false,
            false,
            &selections,
        ),
        Some(WhitespaceMarker {
            strategy: WhitespaceMarkerPaintStrategy::Font,
            kind: WhitespaceMarkerKind::Space,
        })
    );
    assert_eq!(
        visible_whitespace_marker(
            EditorExperimentalWhitespaceRendering::Svg,
            EditorRenderWhitespace::All,
            '\t',
            0,
            0,
            0,
            false,
            false,
            &selections,
        ),
        Some(WhitespaceMarker {
            strategy: WhitespaceMarkerPaintStrategy::Svg,
            kind: WhitespaceMarkerKind::Tab,
        })
    );
}

#[test]
fn whitespace_boundary_markers_cover_edges_runs_tabs_and_trailing() {
    let selections = [];

    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Boundary,
            ' ',
            0,
            0,
            4,
            false,
            false,
            &selections,
        ),
        Some(".")
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Boundary,
            ' ',
            1,
            1,
            4,
            false,
            false,
            &selections,
        ),
        None
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Boundary,
            ' ',
            1,
            1,
            4,
            false,
            true,
            &selections,
        ),
        Some(".")
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Boundary,
            '\t',
            2,
            2,
            4,
            false,
            false,
            &selections,
        ),
        Some(">")
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Boundary,
            ' ',
            4,
            4,
            4,
            false,
            false,
            &selections,
        ),
        Some(".")
    );
}

#[test]
fn trailing_whitespace_start_uses_character_offsets_without_allocating_rows() {
    assert_eq!(trailing_whitespace_start("abc"), 3);
    assert_eq!(trailing_whitespace_start("abc  "), 3);
    assert_eq!(trailing_whitespace_start("  "), 0);
    assert_eq!(trailing_whitespace_start("\u{03bb}  "), 1);
}

#[test]
fn rendered_trailing_whitespace_requires_visible_line_end() {
    let selections = [];
    let text = "let x ";

    assert_eq!(rendered_trailing_whitespace_start(text, true), 5);
    assert_eq!(rendered_trailing_whitespace_start(text, false), 6);
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Trailing,
            ' ',
            5,
            5,
            rendered_trailing_whitespace_start(text, true),
            false,
            false,
            &selections,
        ),
        Some(".")
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Trailing,
            ' ',
            5,
            5,
            rendered_trailing_whitespace_start(text, false),
            false,
            false,
            &selections,
        ),
        None
    );
    assert_eq!(
        whitespace_marker_label(
            EditorRenderWhitespace::Boundary,
            ' ',
            5,
            5,
            rendered_trailing_whitespace_start(text, false),
            false,
            false,
            &selections,
        ),
        None
    );
}

#[test]
fn whitespace_marker_trailing_start_skips_non_trailing_modes_and_capped_rows() {
    let metrics = row_text_metrics("abc  ", 4);

    assert_eq!(
        whitespace_marker_trailing_start(EditorRenderWhitespace::Trailing, &metrics, true),
        3
    );
    assert_eq!(
        whitespace_marker_trailing_start(EditorRenderWhitespace::Trailing, &metrics, false),
        usize::MAX
    );
    assert_eq!(
        whitespace_marker_trailing_start(EditorRenderWhitespace::All, &metrics, true),
        usize::MAX
    );
}

#[test]
fn selection_whitespace_marker_scan_requires_row_overlap() {
    let selections = [Selection {
        anchor: 10,
        cursor: 14,
    }];

    assert!(whitespace_selection_marker_scan_needed(
        EditorRenderWhitespace::Selection,
        &selections,
        &(12..20)
    ));
    assert!(whitespace_selection_marker_scan_needed(
        EditorRenderWhitespace::Selection,
        &selections,
        &(5..11)
    ));
    assert!(!whitespace_selection_marker_scan_needed(
        EditorRenderWhitespace::Selection,
        &selections,
        &(14..20)
    ));
    assert!(!whitespace_selection_marker_scan_needed(
        EditorRenderWhitespace::Selection,
        &[Selection {
            anchor: 12,
            cursor: 12,
        }],
        &(10..14)
    ));
    assert!(whitespace_selection_marker_scan_needed(
        EditorRenderWhitespace::All,
        &[],
        &(40..44)
    ));
}

#[test]
fn selection_ranges_for_snapshot_clip_skip_carets_and_cap_results() {
    let selections = [
        Selection {
            anchor: 0,
            cursor: 5,
        },
        Selection {
            anchor: 8,
            cursor: 12,
        },
        Selection {
            anchor: 14,
            cursor: 14,
        },
        Selection {
            anchor: 18,
            cursor: 12,
        },
        Selection {
            anchor: 19,
            cursor: 25,
        },
    ];

    assert_eq!(
        selection_ranges_for_snapshot(&selections, &(10..20), 2),
        vec![10..12, 12..18]
    );
    assert_eq!(
        selection_ranges_for_snapshot(&selections, &(10..20), 0),
        Vec::<Range<usize>>::new()
    );
}

#[test]
fn selection_ranges_for_snapshot_normalize_out_of_order_ranges() {
    let selections = [
        Selection {
            anchor: 18,
            cursor: 12,
        },
        Selection {
            anchor: 0,
            cursor: 5,
        },
        Selection {
            anchor: 8,
            cursor: 12,
        },
        Selection {
            anchor: 19,
            cursor: 25,
        },
    ];

    assert_eq!(
        selection_ranges_for_snapshot(&selections, &(10..20), 4),
        vec![10..12, 12..18, 19..20]
    );
}

#[test]
fn whitespace_selection_ranges_are_bounded_for_marker_lookup() {
    let selections = (0..MAX_WHITESPACE_SELECTION_RANGES_PER_ROW + 2)
        .map(|idx| Selection {
            anchor: 10 + idx * 2,
            cursor: 11 + idx * 2,
        })
        .collect::<Vec<_>>();
    let snapshot_range = 10..10 + selections.len() * 2;

    let ranges = whitespace_selection_ranges_for_snapshot(
        EditorRenderWhitespace::Selection,
        &selections,
        &snapshot_range,
    )
    .unwrap();

    assert_eq!(ranges.len(), MAX_WHITESPACE_SELECTION_RANGES_PER_ROW);
    assert_eq!(ranges.first().unwrap(), &(10..11));
    assert_eq!(
        ranges.last().unwrap(),
        &(10 + (MAX_WHITESPACE_SELECTION_RANGES_PER_ROW - 1) * 2
            ..11 + (MAX_WHITESPACE_SELECTION_RANGES_PER_ROW - 1) * 2)
    );
    assert_eq!(
        whitespace_marker_kind_for_selection_ranges(
            EditorRenderWhitespace::Selection,
            ' ',
            0,
            10,
            usize::MAX,
            false,
            false,
            &ranges,
        ),
        Some(WhitespaceMarkerKind::Space)
    );
    assert_eq!(
        whitespace_marker_kind_for_selection_ranges(
            EditorRenderWhitespace::Selection,
            ' ',
            0,
            10 + MAX_WHITESPACE_SELECTION_RANGES_PER_ROW * 2,
            usize::MAX,
            false,
            false,
            &ranges,
        ),
        None
    );
    assert!(
        whitespace_selection_ranges_for_snapshot(
            EditorRenderWhitespace::All,
            &selections,
            &snapshot_range
        )
        .is_none()
    );
}

#[test]
fn whitespace_selection_ranges_skip_rows_without_renderable_whitespace() {
    let selections = [Selection {
        anchor: 10,
        cursor: 20,
    }];

    assert!(
        whitespace_selection_ranges_for_marker_scan(
            EditorRenderWhitespace::Selection,
            &selections,
            &(10..20),
            &row_text_metrics("identifier", 4),
        )
        .is_none()
    );
    assert!(
        whitespace_selection_ranges_for_marker_scan(
            EditorRenderWhitespace::Selection,
            &selections,
            &(10..20),
            &row_text_metrics("has space", 4),
        )
        .is_some()
    );
}

#[test]
fn selection_range_cursor_advances_through_sorted_ranges() {
    let ranges = [10..12, 14..16, 20..21];
    let mut cursor = SelectionRangeCursor::new(&ranges);

    assert!(!cursor.contains(9));
    assert!(cursor.contains(10));
    assert!(cursor.contains(11));
    assert!(!cursor.contains(12));
    assert!(cursor.contains(14));
    assert!(!cursor.contains(19));
    assert!(cursor.contains(20));
    assert!(!cursor.contains(21));
}

#[test]
fn whitespace_marker_scan_skips_rows_without_spaces_or_tabs() {
    let selections = [Selection {
        anchor: 10,
        cursor: 20,
    }];

    assert!(!whitespace_marker_scan_needed(
        EditorRenderWhitespace::All,
        &[],
        &(10..20),
        &row_text_metrics("identifier", 4)
    ));
    assert!(!whitespace_marker_scan_needed(
        EditorRenderWhitespace::Selection,
        &selections,
        &(10..20),
        &row_text_metrics("\u{03bb}\u{00e9}x", 4)
    ));
    assert!(whitespace_marker_scan_needed(
        EditorRenderWhitespace::All,
        &[],
        &(10..20),
        &row_text_metrics("has space", 4)
    ));
    assert!(whitespace_marker_scan_needed(
        EditorRenderWhitespace::Boundary,
        &[],
        &(10..20),
        &row_text_metrics("a\tb", 4)
    ));
}

#[test]
fn whitespace_marker_scan_still_filters_selection_rows() {
    let selections = [Selection {
        anchor: 10,
        cursor: 14,
    }];

    assert!(whitespace_marker_scan_needed(
        EditorRenderWhitespace::Selection,
        &selections,
        &(12..20),
        &row_text_metrics("has space", 4)
    ));
    assert!(!whitespace_marker_scan_needed(
        EditorRenderWhitespace::Selection,
        &selections,
        &(14..20),
        &row_text_metrics("has space", 4)
    ));
}

#[test]
fn row_text_metrics_reuses_single_scan_for_display_flags() {
    assert_eq!(
        row_text_metrics("plain ascii", 4),
        RowTextMetrics {
            char_count: 11,
            visual_width: 11,
            has_renderable_whitespace: true,
            has_control_characters: false,
            trailing_whitespace_start: 11,
        }
    );
    assert_eq!(
        row_text_metrics("a\tb", 4),
        RowTextMetrics {
            char_count: 3,
            visual_width: 5,
            has_renderable_whitespace: true,
            has_control_characters: false,
            trailing_whitespace_start: 3,
        }
    );
    assert_eq!(
        row_text_metrics("a\u{0001}  ", 4),
        RowTextMetrics {
            char_count: 4,
            visual_width: 4,
            has_renderable_whitespace: true,
            has_control_characters: true,
            trailing_whitespace_start: 2,
        }
    );
    assert_eq!(
        row_text_metrics("e\u{0301}x", 4),
        RowTextMetrics {
            char_count: 3,
            visual_width: 2,
            has_renderable_whitespace: false,
            has_control_characters: false,
            trailing_whitespace_start: 3,
        }
    );
}

fn row_rect() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 20.0))
}

fn injection(range: Range<usize>) -> TreeSitterInjection {
    TreeSitterInjection {
        language: LanguageId::Sql,
        range,
    }
}

fn assert_rect(rect: Rect, left: f32, top: f32, right: f32, bottom: f32) {
    const EPSILON: f32 = 0.001;
    assert!((rect.left() - left).abs() < EPSILON, "{rect:?}");
    assert!((rect.top() - top).abs() < EPSILON, "{rect:?}");
    assert!((rect.right() - right).abs() < EPSILON, "{rect:?}");
    assert!((rect.bottom() - bottom).abs() < EPSILON, "{rect:?}");
}

use super::layout::{
    TERMINAL_MAX_LAYOUT_POINTS, TERMINAL_PATH_LINK_SCAN_MAX_COLUMNS,
    TERMINAL_SPLIT_SEPARATOR_WIDTH, terminal_cell_rect,
};
use super::*;
use std::path::Path;

#[test]
fn terminal_display_labels_borrow_clean_ascii() {
    let label = terminal_display_label("Cargo Build").expect("display label");

    assert_eq!(label.as_ref(), "Cargo Build");
    assert!(matches!(&label, Cow::Borrowed(_)));
}

#[test]
fn terminal_display_labels_borrow_clean_unicode() {
    let raw = "pwsh \u{65e5}\u{672c}\u{8a9e} cafe\u{301} \u{1f680}";
    let label = terminal_display_label(raw).expect("display label");

    assert_eq!(label.as_ref(), raw);
    assert_eq!(
        terminal_display_label_normalized(raw, None).as_deref(),
        Some(raw)
    );
    assert!(matches!(&label, Cow::Borrowed(_)));
}

#[test]
fn terminal_display_labels_normalize_dirty_unicode() {
    let raw = "\u{2003}pwsh\u{2028}\u{202e}\u{65e5}\u{672c}\u{7}  ";
    let label = terminal_display_label(raw).expect("display label");

    assert_eq!(label.as_ref(), "pwsh \u{65e5}\u{672c}");
    assert!(matches!(&label, Cow::Owned(_)));
}

#[test]
fn terminal_display_labels_reject_empty_dirty_unicode() {
    assert!(terminal_display_label("\u{2003}\u{202e}\u{2066}\r\n").is_none());
}

#[test]
fn terminal_display_labels_truncate_overlong_unicode() {
    let raw = "\u{754c}".repeat(TERMINAL_DISPLAY_LABEL_MAX_CHARS + 1);
    let expected = "\u{754c}".repeat(TERMINAL_DISPLAY_LABEL_MAX_CHARS);
    let label = terminal_display_label(&raw).expect("display label");

    assert_eq!(label.as_ref(), expected);
    assert_eq!(label.chars().count(), TERMINAL_DISPLAY_LABEL_MAX_CHARS);
    assert!(matches!(&label, Cow::Owned(_)));
}

#[test]
fn terminal_search_preview_display_labels_strip_bidi_and_controls() {
    let preview = terminal_search_preview_display_label("build\u{202e}\tfailed\u{2066} now")
        .expect("display preview");

    assert_eq!(preview.as_ref(), "build failed now");
    assert!(!preview.contains('\u{202e}'));
    assert!(!preview.contains('\u{2066}'));
}

#[test]
fn terminal_search_preview_display_labels_borrow_clean_preview() {
    let preview =
        terminal_search_preview_display_label("cargo test passed").expect("display preview");

    assert_eq!(preview.as_ref(), "cargo test passed");
    assert!(matches!(&preview, Cow::Borrowed(_)));
}

#[test]
fn terminal_session_label_context_reuses_display_shell_label() {
    let context = TerminalSessionLabelContext::new(
        "  pwsh\r\n\u{202e}core  ",
        &format!(" {DEFAULT_TERMINAL_TABS_TITLE} "),
    );

    assert!(context.uses_default_title_template);
    assert_eq!(
        context
            .shell_display_label()
            .expect("display shell")
            .as_ref(),
        "Terminal"
    );
    assert!(matches!(
        context.display_shell_label.as_ref(),
        Some(Cow::Borrowed(_))
    ));
}

#[test]
fn terminal_session_label_numbers_every_session_in_creation_order() {
    let label = terminal_session_label_for_shell(1, "Cargo Build");

    assert_eq!(label.as_ref(), "Cargo Build 1");
    assert!(matches!(&label, Cow::Owned(_)));

    let label = terminal_session_label_for_shell(2, "Cargo Build");

    assert_eq!(label.as_ref(), "Cargo Build 2");
    assert!(matches!(&label, Cow::Owned(_)));
}

#[test]
fn terminal_tab_hit_testing_prefers_the_topmost_tab_rect() {
    let tab_rects = terminal_tab_rects(
        pos2(0.0, 0.0),
        vec2(190.0, 32.0),
        8.0,
        TerminalTabOrientation::Horizontal,
        &[1, 2, 3],
    );

    assert_eq!(
        terminal_tab_session_at(&tab_rects, pos2(10.0, 16.0)),
        Some(1)
    );
    assert_eq!(
        terminal_tab_session_at(&tab_rects, pos2(200.0, 16.0)),
        Some(2)
    );
    assert_eq!(
        terminal_tab_session_at(&tab_rects, pos2(400.0, 16.0)),
        Some(3)
    );
    assert_eq!(terminal_tab_session_at(&tab_rects, pos2(400.0, 40.0)), None);
    assert_eq!(
        terminal_tab_session_at(&tab_rects, pos2(f32::NAN, 16.0)),
        None
    );

    let overlapping = vec![
        (1, Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 32.0))),
        (2, Rect::from_min_size(pos2(50.0, 0.0), vec2(100.0, 32.0))),
    ];
    assert_eq!(
        terminal_tab_session_at(&overlapping, pos2(60.0, 16.0)),
        Some(2)
    );
}

#[test]
fn terminal_display_labels_bound_huge_control_prefixes() {
    let raw = format!(
        "{}visible",
        "\x1b".repeat(TERMINAL_DISPLAY_LABEL_MAX_EXACT_UTF8_BYTES + 1)
    );

    assert!(terminal_display_label(&raw).is_none());
}

#[test]
fn terminal_path_labels_borrow_clean_utf8_path_text() {
    let path = Path::new("workspace/tools");
    let compact = compact_terminal_path(path);
    let tooltip = terminal_path_tooltip(path);

    assert_eq!(compact.as_ref(), "tools");
    assert!(matches!(&compact, Cow::Borrowed(_)));
    assert_eq!(tooltip.as_ref(), "workspace/tools");
    assert!(matches!(&tooltip, Cow::Borrowed(_)));
}

#[test]
fn terminal_command_status_tooltips_borrow_static_text() {
    let tooltip = terminal_command_status_tooltip(TerminalCommandStatus::Running);

    assert_eq!(tooltip.as_ref(), "Shell command running");
    assert!(matches!(&tooltip, Cow::Borrowed(_)));

    let profile = terminal_profile_tab_tooltip(TerminalCommandStatus::Running);

    assert_eq!(profile.as_ref(), "Terminal session\nShell command running");
    assert!(matches!(&profile, Cow::Borrowed(_)));

    let profile = terminal_profile_tab_tooltip(TerminalCommandStatus::Unknown);

    assert_eq!(profile.as_ref(), "Terminal session");
    assert!(matches!(&profile, Cow::Borrowed(_)));
}

#[test]
fn terminal_command_status_tooltips_format_failed_exit_codes() {
    let tooltip = terminal_command_status_tooltip(TerminalCommandStatus::Failed(127));

    assert_eq!(tooltip.as_ref(), "Command exited with code 127");
    assert!(matches!(&tooltip, Cow::Owned(_)));

    let profile = terminal_profile_tab_tooltip(TerminalCommandStatus::Failed(127));

    assert_eq!(
        profile.as_ref(),
        "Terminal session\nCommand exited with code 127"
    );
    assert!(matches!(&profile, Cow::Owned(_)));
}

#[test]
fn terminal_tab_ansi_color_names_map_to_palette_indexes() {
    assert_eq!(terminal_tab_ansi_color_index("terminal.ansiRed"), Some(1));
    assert_eq!(
        terminal_tab_ansi_color_index("terminal.ansiBrightWhite"),
        Some(15)
    );
    assert_eq!(terminal_tab_ansi_color_index("foreground"), None);
}

#[test]
fn terminal_tab_hex_colors_reject_non_ascii_without_panicking() {
    assert_eq!(parse_terminal_tab_hex_color("#a\u{e9}abc"), None);
    assert_eq!(
        parse_terminal_tab_hex_color("#123456"),
        Some(Color32::from_rgb(0x12, 0x34, 0x56))
    );
}

#[test]
fn numbered_terminal_session_labels_preserve_suffix_when_base_is_capped() {
    let raw = format!(
        "{}\u{202e}\n{}",
        "x".repeat(48),
        "y".repeat(TERMINAL_DISPLAY_LABEL_MAX_CHARS)
    );

    let label = terminal_session_label_for_shell(42, &raw);

    assert!(label.ends_with(" 42"));
    assert_eq!(label.chars().count(), TERMINAL_DISPLAY_LABEL_MAX_CHARS);
    assert!(!label.contains('\n'));
    assert!(!label.contains('\u{202e}'));
}

#[test]
fn numbered_terminal_session_labels_fall_back_to_id_when_label_sanitizes_empty() {
    assert_eq!(
        terminal_session_label_for_shell(7, "\u{202e}\u{2066}\n\r"),
        "7"
    );
}

#[test]
fn terminal_session_label_context_reuses_shell_tooltip_label() {
    let context =
        TerminalSessionLabelContext::new(" pwsh\r\n\u{202e}core ", DEFAULT_TERMINAL_TABS_TITLE);
    let tooltip = context.shell_tooltip_label();

    assert_eq!(tooltip.as_ref(), "Terminal");
    assert!(!tooltip.contains('\n'));
    assert!(!tooltip.contains('\u{202e}'));
}

#[test]
fn terminal_path_link_scan_rejects_unbounded_screen_dimensions() {
    assert!(terminal_path_link_scan_allowed((24, 120)));
    assert!(!terminal_path_link_scan_allowed((0, 120)));
    assert!(!terminal_path_link_scan_allowed((24, 0)));
    assert!(!terminal_path_link_scan_allowed((
        24,
        TERMINAL_PATH_LINK_SCAN_MAX_COLUMNS + 1
    )));
}

#[test]
fn terminal_render_color_cache_matches_uncached_resolution() {
    let background = Color32::from_rgb(18, 20, 24);
    let text = Color32::from_rgb(222, 226, 233);
    let palette = terminal_ansi_palette_from_colors(
        background,
        text,
        Color32::from_rgb(126, 136, 150),
        Color32::from_rgb(91, 141, 239),
        Color32::from_rgb(231, 185, 87),
        Color32::from_rgb(197, 15, 31),
    );
    let mut cache = TerminalRenderColorCache::new(text, background, true, 4.5, &palette);
    let foreground_color = vt100::Color::Idx(1);
    let background_color = vt100::Color::Idx(4);

    let cached_base = cache.base_colors(foreground_color, background_color, true);
    let mut expected_foreground = terminal_foreground_color(foreground_color, text, &palette);
    let mut expected_background = terminal_background_color(background_color, background, &palette);
    std::mem::swap(&mut expected_foreground, &mut expected_background);

    assert_eq!(
        cached_base,
        TerminalRenderBaseColors {
            foreground: expected_foreground,
            background: expected_background,
        }
    );
    assert_eq!(
        cache.base_colors(foreground_color, background_color, true),
        cached_base
    );

    let cached_text = cache.text_color(
        foreground_color,
        cached_base.foreground,
        cached_base.background,
        true,
        true,
    );

    assert_eq!(
        cached_text,
        terminal_rendered_text_color(
            foreground_color,
            cached_base.foreground,
            cached_base.background,
            true,
            true,
            true,
            4.5,
            &palette,
        )
    );
    assert_eq!(
        cache.text_color(
            foreground_color,
            cached_base.foreground,
            cached_base.background,
            true,
            true,
        ),
        cached_text
    );
}

#[test]
fn terminal_render_text_runs_borrow_unmerged_text() {
    let mut runs = Vec::new();

    push_terminal_text_run(&mut runs, 0, 0, 1, "a", Color32::WHITE, false, false);

    assert_eq!(runs[0].text, "a");
    assert!(matches!(&runs[0].text, Cow::Borrowed(_)));
}

#[test]
fn terminal_render_text_runs_allocate_only_when_merging() {
    let mut runs = Vec::new();

    push_terminal_text_run(&mut runs, 0, 0, 1, "a", Color32::WHITE, false, true);
    assert!(matches!(&runs[0].text, Cow::Borrowed(_)));

    push_terminal_text_run(&mut runs, 0, 1, 1, "b", Color32::WHITE, false, true);

    assert_eq!(runs[0].text, "ab");
    assert!(matches!(&runs[0].text, Cow::Owned(_)));
}

#[test]
fn terminal_layout_values_reject_non_finite_dimensions() {
    assert_eq!(bounded_terminal_layout_value(f32::NAN), 0.0);
    assert_eq!(bounded_terminal_layout_value(f32::NEG_INFINITY), 0.0);
    assert_eq!(
        bounded_terminal_layout_value(f32::INFINITY),
        TERMINAL_MAX_LAYOUT_POINTS
    );
    assert_eq!(bounded_terminal_layout_value(-12.0), 0.0);
    assert_eq!(
        bounded_terminal_layout_value(TERMINAL_MAX_LAYOUT_POINTS * 2.0),
        TERMINAL_MAX_LAYOUT_POINTS
    );
}

#[test]
fn terminal_split_separator_width_is_bounded_by_available_space() {
    assert_eq!(
        terminal_split_separator_width(600.0, 3),
        TERMINAL_SPLIT_SEPARATOR_WIDTH
    );
    assert_eq!(terminal_split_separator_width(10.0, 3), 5.0);
    assert_eq!(terminal_split_separator_width(f32::NAN, 3), 0.0);
    assert_eq!(terminal_split_separator_width(600.0, 1), 0.0);
}

#[test]
fn terminal_split_separator_line_rect_is_centered_and_thin() {
    let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(7.0, 120.0));
    let line = terminal_split_separator_line_rect(rect).expect("separator line");

    assert_eq!(line.center(), rect.center());
    assert_eq!(line.width(), 1.0);
    assert_eq!(line.height(), 120.0);
}

#[test]
fn terminal_split_separator_line_rect_rejects_empty_geometry() {
    assert_eq!(
        terminal_split_separator_line_rect(Rect::from_min_size(pos2(0.0, 0.0), Vec2::ZERO)),
        None
    );
}

#[test]
fn terminal_input_hover_text_mentions_context_menu_override() {
    assert_eq!(
        terminal_input_hover_text(),
        "Terminal input\nRight-click for terminal actions"
    );
}

#[test]
fn terminal_cell_position_rejects_non_finite_geometry() {
    let inner = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 80.0));

    assert_eq!(
        terminal_cell_position_at_pointer(Some(pos2(f32::NAN, 20.0)), inner, 10.0, 20.0, 4, 10,),
        None
    );
    assert_eq!(
        terminal_cell_position_at_pointer(Some(pos2(20.0, 20.0)), inner, f32::NAN, 20.0, 4, 10),
        None
    );
    assert_eq!(
        terminal_cell_position_at_pointer(
            Some(pos2(20.0, 20.0)),
            Rect::from_min_size(pos2(0.0, 0.0), Vec2::ZERO),
            10.0,
            20.0,
            4,
            10,
        ),
        None
    );
}

#[test]
fn terminal_cell_position_clamps_pointer_to_visible_cells() {
    let inner = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 80.0));

    assert_eq!(
        terminal_cell_position_at_pointer(Some(pos2(-100.0, -100.0)), inner, 10.0, 20.0, 4, 10),
        Some(super::super::TerminalCellPosition { row: 0, col: 0 })
    );
    assert_eq!(
        terminal_cell_position_at_pointer(Some(pos2(999.0, 999.0)), inner, 10.0, 20.0, 4, 10),
        Some(super::super::TerminalCellPosition { row: 3, col: 9 })
    );
}

#[test]
fn terminal_cell_rect_clips_wide_cells_to_content_bounds() {
    let inner = Rect::from_min_size(pos2(0.0, 0.0), vec2(15.0, 20.0));
    let rect = terminal_cell_rect(inner, 0, 1, 2, 10.0, 20.0).expect("cell rect");

    assert_eq!(rect.left(), 10.0);
    assert_eq!(rect.right(), 15.0);
    assert_eq!(terminal_cell_rect(inner, 0, 2, 1, 10.0, 20.0), None);
}

#[test]
fn terminal_render_grid_matches_cell_rect_clipping() {
    let inner = Rect::from_min_size(pos2(0.0, 0.0), vec2(15.0, 20.0));
    let grid = terminal_render_grid(inner, 4, 10, 10.0, 20.0).expect("render grid");
    let rect = grid.cell_rect(0, 1, 2).expect("cell rect");

    assert_eq!(grid.visible_rows, 1);
    assert_eq!(grid.visible_cols, 2);
    assert_eq!(
        rect,
        terminal_cell_rect(inner, 0, 1, 2, 10.0, 20.0).unwrap()
    );
    assert_eq!(grid.cell_rect(0, 2, 1), None);
}

#[test]
fn terminal_render_grid_rejects_invalid_geometry() {
    let inner = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0));

    assert!(terminal_render_grid(inner, 0, 10, 8.0, 16.0).is_none());
    assert!(terminal_render_grid(inner, 4, 10, f32::NAN, 16.0).is_none());
    assert!(
        terminal_render_grid(
            Rect::from_min_size(pos2(f32::NAN, 0.0), vec2(100.0, 80.0)),
            4,
            10,
            8.0,
            16.0,
        )
        .is_none()
    );
}

#[test]
fn terminal_prepared_text_runs_cache_display_positions() {
    let mut runs = Vec::new();
    push_terminal_text_run(&mut runs, 1, 2, 3, "abc", Color32::WHITE, true, false);

    let render_grid = terminal_render_grid(
        Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 80.0)),
        5,
        10,
        8.0,
        16.0,
    );
    let prepared: Vec<_> = prepare_terminal_text_runs(&runs, render_grid).collect();

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].position, pos2(26.0, 36.0));
    assert_eq!(prepared[0].text, "abc");
    assert_eq!(
        prepared[0].underline,
        Some((pos2(26.0, 50.0), pos2(50.0, 50.0)))
    );
}

#[test]
fn terminal_prepared_text_runs_skip_invalid_geometry() {
    let mut runs = Vec::new();
    push_terminal_text_run(&mut runs, 0, 0, 1, "a", Color32::WHITE, false, false);

    assert!(
        prepare_terminal_text_runs(
            &runs,
            terminal_render_grid(
                Rect::from_min_size(pos2(f32::NAN, 0.0), vec2(100.0, 80.0)),
                4,
                10,
                8.0,
                16.0,
            ),
        )
        .next()
        .is_none()
    );
    assert!(
        prepare_terminal_text_runs(
            &runs,
            terminal_render_grid(
                Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0)),
                4,
                10,
                f32::NAN,
                16.0,
            ),
        )
        .next()
        .is_none()
    );
}

use super::{
    DIFF_PATCH_OVERLAY_SCAN_MAX_LINES, active_bracket_pair_matches_required, capped_folding_ranges,
    diff_code_lenses_for_patch_buffer, diff_moved_patch_lines, editor_accessibility_enabled,
    editor_bracket_pair_colorization_enabled, editor_bracket_pair_guides_for_mode,
    editor_char_width, editor_code_lens_enabled, editor_diff_code_lenses_enabled,
    editor_diff_overview_ruler_enabled, editor_find_matches_enabled, editor_folding_enabled,
    editor_folding_ranges_for_buffer, editor_font_size, editor_git_blame_decoration_enabled,
    editor_gutter_width, editor_highlight_active_indentation_for_mode, editor_inlay_hints_enabled,
    editor_match_brackets_for_mode, editor_minimap_enabled, editor_path_openable_cached,
    editor_scm_diff_gutter_enabled, editor_scm_diff_minimap_enabled,
    editor_scm_diff_overview_enabled, editor_sticky_scroll_enabled,
    editor_sticky_scroll_max_line_count, editor_stop_rendering_line_after_for_mode,
    editor_validation_decorations_enabled, editor_vim_search_matches_enabled,
    editor_word_wrap_for_buffer, folded_ranges_allowed_by_folding_ranges,
    line_number_width_for_min_chars, path_exists_cached, renderable_code_lenses,
    renderable_git_blame_lines, renderable_inlay_hints, sort_code_lenses_by_position,
    unicode_highlight_allowed_characters, unicode_highlight_allowed_locales,
};
use crate::{
    KuroyaApp,
    app_startup_context::AppStartupContext,
    editor_vim_key_events::{vim_clear_searches, vim_set_last_search_for_test},
    folding::FoldedRange,
    large_file_mode::{
        LARGE_FILE_MODE_LINE_RENDER_CHAR_LIMIT, LARGE_FILE_MODE_MAX_BYTES,
        LARGE_FILE_PERFORMANCE_MODE_MAX_LINES, buffer_uses_large_file_mode,
        buffer_uses_large_file_performance_mode,
    },
    terminal::TerminalPane,
    transient_state::EditorImePreedit,
};
use eframe::egui::Color32;
use kuroya_core::{
    DiffWordWrap, EditorAccessibilitySupport, EditorBracketPairGuideMode, EditorFoldingStrategy,
    EditorHighlightActiveIndentation, EditorLineDecorationsWidth, EditorLineNumbers,
    EditorMatchBrackets, EditorRenderValidationDecorations, EditorSettings, EditorWordWrap,
    EditorWordWrapOverride, GitBlameLine, GitChangeStage, LanguageId, LspCodeLens, LspFoldingRange,
    LspInlayHint, MAX_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT, ScmDiffDecorations, TextBuffer,
    ThemeSettings, Workspace,
};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
    time::Instant,
};
use tokio::runtime::Runtime;

#[test]
fn editor_gutter_width_tracks_visible_gutter_features() {
    assert_eq!(
        editor_gutter_width(
            EditorLineNumbers::On,
            true,
            true,
            5,
            EditorLineDecorationsWidth::default(),
            8.0
        ),
        84.0
    );
    assert_eq!(
        editor_gutter_width(
            EditorLineNumbers::On,
            true,
            true,
            8,
            EditorLineDecorationsWidth::default(),
            8.0
        ),
        108.0
    );
    assert_eq!(
        editor_gutter_width(
            EditorLineNumbers::Off,
            false,
            false,
            8,
            EditorLineDecorationsWidth::Pixels(20.0),
            8.0
        ),
        24.0
    );
    assert_eq!(
        editor_gutter_width(
            EditorLineNumbers::On,
            false,
            false,
            5,
            EditorLineDecorationsWidth::Pixels(20.0),
            8.0
        ),
        66.0
    );
    assert_eq!(
        editor_gutter_width(
            EditorLineNumbers::On,
            false,
            false,
            5,
            EditorLineDecorationsWidth::Ch(2.0),
            8.0
        ),
        62.0
    );
    assert_eq!(
        editor_gutter_width(
            EditorLineNumbers::Off,
            false,
            false,
            8,
            EditorLineDecorationsWidth::Pixels(0.0),
            8.0
        ),
        24.0
    );
    assert_eq!(line_number_width_for_min_chars(0, f32::NAN), 12.0);
}

#[test]
fn editor_char_width_uses_safe_default_for_invalid_measurements() {
    assert_eq!(editor_char_width(9.5), 9.5);
    assert_eq!(editor_char_width(0.0), 8.0);
    assert_eq!(editor_char_width(f32::NAN), 8.0);
    assert_eq!(editor_char_width(f32::INFINITY), 8.0);
    assert_eq!(editor_font_size(f32::NAN), 13.0);
    assert_eq!(editor_font_size(0.0), 13.0);
    assert_eq!(
        editor_font_size(f32::MAX),
        kuroya_core::settings::MAX_EDITOR_FONT_SIZE
    );
}

#[test]
fn path_exists_cached_reuses_same_render_pass_probe() {
    let path = PathBuf::from("workspace/file.rs");
    let other_path = PathBuf::from("workspace/other.rs");
    let calls = Cell::new(0);
    let mut cache = HashMap::new();

    assert!(path_exists_cached(&mut cache, &path, |_| {
        calls.set(calls.get() + 1);
        true
    }));
    assert!(path_exists_cached(&mut cache, path.as_path(), |_| {
        calls.set(calls.get() + 1);
        false
    }));
    assert_eq!(calls.get(), 1);

    assert!(!path_exists_cached(&mut cache, &other_path, |_| {
        calls.set(calls.get() + 1);
        false
    }));
    assert_eq!(calls.get(), 2);
}

#[test]
fn editor_path_openable_cache_uses_open_buffer_before_filesystem_probe() {
    let path = PathBuf::from("workspace/file.rs");
    let buffers = vec![TextBuffer::from_text(
        7,
        Some(path.clone()),
        "open\n".to_owned(),
    )];
    let calls = Cell::new(0);
    let mut cache = HashMap::new();

    assert!(editor_path_openable_cached(
        &mut cache,
        &buffers,
        &[],
        &path,
        |_| {
            calls.set(calls.get() + 1);
            false
        },
    ));

    assert_eq!(calls.get(), 0);
}

#[test]
fn pane_data_compare_actions_use_open_buffers_for_missing_paths() {
    let root = PathBuf::from("workspace");
    let active_path = root.join("active.rs");
    let selected_path = root.join("selected.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(active_path.clone()),
        "active\n".to_owned(),
    ));
    app.buffers.push(TextBuffer::from_text(
        8,
        Some(selected_path.clone()),
        "selected\n".to_owned(),
    ));
    app.explorer_compare_path = Some(selected_path);
    app.settings.scm_diff_decorations = ScmDiffDecorations::None;

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert_eq!(data.active_path.as_deref(), Some(active_path.as_path()));
    assert!(data.compare_file_actions);
    assert!(data.compare_with_selected_actions);
}

#[test]
fn pane_data_selection_color_follows_custom_theme() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    app.settings.theme = ThemeSettings {
        selection: Some([12, 34, 56]),
        ..ThemeSettings::default()
    };
    app.buffers
        .push(TextBuffer::from_text(7, None, "selected text\n".to_owned()));

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert_eq!(data.selection_bg_fill, Color32::from_rgb(12, 34, 56));
}

#[test]
fn editor_accessibility_enabled_follows_support_mode() {
    assert!(editor_accessibility_enabled(
        EditorAccessibilitySupport::Auto
    ));
    assert!(editor_accessibility_enabled(EditorAccessibilitySupport::On));
    assert!(!editor_accessibility_enabled(
        EditorAccessibilitySupport::Off
    ));
}

#[test]
fn large_file_mode_disables_editor_minimap() {
    assert!(editor_minimap_enabled(true, false));
    assert!(!editor_minimap_enabled(false, false));
    assert!(!editor_minimap_enabled(true, true));
    assert!(!editor_minimap_enabled(false, true));
    assert!(editor_folding_enabled(true, false));
    assert!(!editor_folding_enabled(false, false));
    assert!(!editor_folding_enabled(true, true));
    assert_eq!(
        editor_highlight_active_indentation_for_mode(
            EditorHighlightActiveIndentation::Always,
            true,
            false
        ),
        EditorHighlightActiveIndentation::Always
    );
    assert_eq!(
        editor_highlight_active_indentation_for_mode(
            EditorHighlightActiveIndentation::Always,
            true,
            true
        ),
        EditorHighlightActiveIndentation::Off
    );
    assert_eq!(
        editor_highlight_active_indentation_for_mode(
            EditorHighlightActiveIndentation::Always,
            false,
            false
        ),
        EditorHighlightActiveIndentation::Off
    );
}

#[test]
fn performance_mode_keeps_normal_editor_surface_before_hard_large_file_mode() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    let mut text = "x\n".repeat(LARGE_FILE_PERFORMANCE_MODE_MAX_LINES);
    text.push('x');
    let buffer = TextBuffer::from_text(7, None, text);
    assert!(!buffer_uses_large_file_mode(&buffer));
    assert!(buffer_uses_large_file_performance_mode(&buffer));

    app.buffers.push(buffer);
    app.buffer_find_open = true;
    app.buffer_find_query = "x".to_owned();
    app.settings.folding = true;
    app.settings.minimap = true;
    app.settings.match_brackets = EditorMatchBrackets::Always;
    app.settings.bracket_pair_guides = EditorBracketPairGuideMode::Active;
    app.settings.bracket_pair_colorization = true;
    app.settings.highlight_active_indentation = EditorHighlightActiveIndentation::Always;
    app.settings.scm_diff_decorations = ScmDiffDecorations::All;

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert!(data.syntax_highlighting);
    assert!(data.folding);
    assert!(data.show_minimap);
    assert!(data.bracket_pair_colorization);
    assert_eq!(data.match_brackets, EditorMatchBrackets::Always);
    assert_eq!(data.bracket_pair_guides, EditorBracketPairGuideMode::Active);
    assert_eq!(
        data.highlight_active_indentation,
        EditorHighlightActiveIndentation::Always
    );
    assert!(!data.find_matches.is_empty());
    assert!(data.show_scm_diff_gutter);
    assert!(data.show_scm_diff_overview);
    assert!(data.show_scm_diff_minimap);
}

#[test]
fn pane_data_disables_active_indentation_when_indent_guides_are_disabled() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    app.buffers.push(TextBuffer::from_text(
        7,
        None,
        "fn main() {\n    println!(\"hi\");\n}\n".to_owned(),
    ));
    app.settings.indent_guides = false;
    app.settings.highlight_active_indentation = EditorHighlightActiveIndentation::Always;

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert!(!data.indent_guides);
    assert_eq!(
        data.highlight_active_indentation,
        EditorHighlightActiveIndentation::Off
    );
}

#[test]
fn pane_data_uses_resolved_large_file_folding_state_for_gutter_width() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    app.buffers.push(TextBuffer::from_text(
        7,
        None,
        "x".repeat(LARGE_FILE_MODE_MAX_BYTES + 1),
    ));
    app.settings.folding = true;
    app.settings.scm_diff_decorations = ScmDiffDecorations::None;

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert!(!data.folding);
    assert_eq!(
        data.gutter_width,
        editor_gutter_width(
            app.settings.line_numbers,
            app.settings.glyph_margin,
            false,
            app.settings.line_numbers_min_chars,
            app.settings.line_decorations_width,
            8.0,
        )
    );
}

#[test]
fn pane_data_resolves_active_id_when_buffer_index_is_stale() {
    let root = PathBuf::from("workspace");
    let first_path = root.join("first.rs");
    let second_path = root.join("second.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(first_path),
        "first\n".to_owned(),
    ));
    let mut second = TextBuffer::from_text(8, Some(second_path.clone()), "second\n".to_owned());
    second.set_single_cursor(3);
    app.buffers.push(second);
    app.settings.scm_diff_decorations = ScmDiffDecorations::None;

    let data = app.prepare_editor_pane_data(8, 0, 8.0, true, true);

    assert_eq!(data.active_path.as_deref(), Some(second_path.as_path()));
    assert_eq!(data.cursor_positions[0].char_idx, 3);
    assert_eq!(data.visible_line_count, app.buffers[1].len_lines().max(1));
}

#[test]
fn pane_data_clamps_non_finite_render_geometry_settings() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    app.buffers
        .push(TextBuffer::from_text(7, None, "main\n".to_owned()));
    app.settings.font_size = f32::NAN;
    app.settings.cursor_width = f32::NAN;
    app.settings.cursor_height = usize::MAX;
    app.settings.minimap_scale = usize::MAX;
    app.settings.minimap_section_header_font_size = f32::NAN;
    app.settings.minimap_section_header_letter_spacing = f32::NAN;
    app.settings.scm_diff_decorations = ScmDiffDecorations::None;

    let data = app.prepare_editor_pane_data(7, 0, f32::NAN, true, true);

    assert_eq!(data.font_size, 13.0);
    assert!(data.row_height.is_finite());
    assert!(data.row_height > 0.0);
    assert_eq!(data.char_width, 8.0);
    assert!(data.cursor_width.is_finite());
    assert!(data.cursor_width > 0.0);
    assert!(data.cursor_height <= kuroya_core::MAX_EDITOR_CURSOR_HEIGHT);
    assert!(data.minimap_scale <= kuroya_core::MAX_EDITOR_MINIMAP_SCALE);
    assert!(data.minimap_section_header_font_size.is_finite());
    assert!(data.minimap_section_header_letter_spacing.is_finite());
}

#[test]
fn sticky_scroll_can_be_disabled_by_zero_max_line_count() {
    assert!(editor_sticky_scroll_enabled(true, 1));
    assert!(!editor_sticky_scroll_enabled(true, 0));
    assert!(!editor_sticky_scroll_enabled(false, 10));
    assert_eq!(editor_sticky_scroll_max_line_count(true, 0), 0);
    assert_eq!(editor_sticky_scroll_max_line_count(false, 10), 0);
    assert_eq!(editor_sticky_scroll_max_line_count(true, 8), 8);
    assert_eq!(
        editor_sticky_scroll_max_line_count(true, usize::MAX),
        MAX_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT
    );
}

#[test]
fn pane_data_carries_sticky_scroll_max_line_count() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    app.buffers.push(TextBuffer::from_text(
        7,
        None,
        "fn main() {\n    call();\n}\n".to_owned(),
    ));
    app.settings.sticky_scroll = true;
    app.settings.sticky_scroll_max_line_count = 8;

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert!(data.sticky_scroll);
    assert_eq!(data.sticky_scroll_max_line_count, 8);

    app.settings.sticky_scroll_max_line_count = 0;
    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert!(!data.sticky_scroll);
    assert_eq!(data.sticky_scroll_max_line_count, 0);
}

#[test]
fn active_bracket_pair_matches_are_needed_only_for_active_guide_rendering() {
    assert!(active_bracket_pair_matches_required(
        EditorBracketPairGuideMode::Active,
        EditorBracketPairGuideMode::Off,
        false,
    ));
    assert!(active_bracket_pair_matches_required(
        EditorBracketPairGuideMode::Off,
        EditorBracketPairGuideMode::Active,
        false,
    ));
    assert!(active_bracket_pair_matches_required(
        EditorBracketPairGuideMode::On,
        EditorBracketPairGuideMode::Off,
        true,
    ));
    assert!(!active_bracket_pair_matches_required(
        EditorBracketPairGuideMode::On,
        EditorBracketPairGuideMode::Off,
        false,
    ));
    assert!(!active_bracket_pair_matches_required(
        EditorBracketPairGuideMode::Off,
        EditorBracketPairGuideMode::Off,
        true,
    ));
}

#[test]
fn pane_data_keeps_active_bracket_guides_independent_from_match_boxes() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    let mut buffer = TextBuffer::from_text(7, None, "fn main() { call(); }".to_owned());
    buffer.set_single_cursor(14);
    app.buffers.push(buffer);
    app.settings.match_brackets = EditorMatchBrackets::Never;
    app.settings.bracket_pair_guides = EditorBracketPairGuideMode::Active;
    app.settings.bracket_pair_guides_horizontal = EditorBracketPairGuideMode::Off;
    app.settings.highlight_active_bracket_pair = false;

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert_eq!(data.match_brackets, EditorMatchBrackets::Never);
    assert!(data.bracket_matches.is_empty());
    assert_eq!(data.active_bracket_pair_matches, vec![(10, 20)]);
    assert!(
        data.bracket_pair_guide_ranges
            .iter()
            .any(|guide| guide.open_idx == 10 && guide.close_idx == 20)
    );
}

#[test]
fn pane_data_uses_match_bracket_mode_for_match_boxes() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    let mut buffer = TextBuffer::from_text(8, None, "fn main() { call(); }".to_owned());
    buffer.set_single_cursor(14);
    app.buffers.push(buffer);
    app.settings.match_brackets = EditorMatchBrackets::Near;
    app.settings.bracket_pair_guides = EditorBracketPairGuideMode::Off;
    app.settings.bracket_pair_guides_horizontal = EditorBracketPairGuideMode::Off;

    let data = app.prepare_editor_pane_data(8, 0, 8.0, true, true);

    assert_eq!(data.match_brackets, EditorMatchBrackets::Near);
    assert!(data.bracket_matches.is_empty());
    assert!(data.active_bracket_pair_matches.is_empty());
    assert!(data.bracket_pair_guide_ranges.is_empty());
}

#[test]
fn pane_data_clears_disabled_active_buffer_caches_in_large_file_mode() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    let mut buffer = TextBuffer::from_text(7, None, "alpha { alpha }\n".to_owned());
    buffer.set_single_cursor(1);
    app.buffers.push(buffer);
    app.buffer_find_open = true;
    app.buffer_find_query = "alpha".to_owned();
    app.settings.scm_diff_decorations = ScmDiffDecorations::None;

    let _ = app.find_matches_for_buffer_index(0);
    let _ = app
        .editor_bracket_overlay_cache
        .bracket_matches(&app.buffers[0], EditorMatchBrackets::Always);
    assert_eq!(app.buffer_find_cache.cached_buffer_ids_for_test(), vec![7]);
    assert!(app.editor_bracket_overlay_cache.contains_buffer_for_test(7));

    let len_chars = app.buffers[0].len_chars();
    assert!(
        app.buffers[0].replace_range(0..len_chars, &"x".repeat(LARGE_FILE_MODE_MAX_BYTES + 1),)
    );

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert!(!data.syntax_highlighting);
    assert!(data.find_matches.is_empty());
    assert!(
        app.buffer_find_cache
            .cached_buffer_ids_for_test()
            .is_empty()
    );
    assert!(!app.editor_bracket_overlay_cache.contains_buffer_for_test(7));
}

#[test]
fn pane_data_uses_vim_search_matches() {
    vim_clear_searches();
    let mut app = app_for_test(PathBuf::from("workspace"));
    app.settings.vim_keybindings = true;
    app.buffers.push(TextBuffer::from_text(
        7,
        None,
        "alpha beta alpha".to_owned(),
    ));
    {
        let buffer = app.buffer(7).expect("buffer should exist");
        vim_set_last_search_for_test(buffer, "alpha", true, false);
    }

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert_eq!(data.find_matches, vec![0..5, 11..16]);

    app.settings.vim_keybindings = false;
    let disabled = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert!(disabled.find_matches.is_empty());
    vim_clear_searches();
}

#[test]
fn pane_data_keeps_normal_typing_unhighlighted() {
    let mut app = app_for_test(PathBuf::from("workspace"));
    let mut buffer = TextBuffer::from_text(7, None, "alpha beta alpha".to_owned());
    buffer.set_selection(0, 5);
    assert!(buffer.insert_text_with_auto_pairs("x"));
    app.settings.vim_keybindings = false;
    app.buffer_find_open = false;
    app.document_highlights_path = None;
    app.document_highlights.clear();
    app.buffers.push(buffer);

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    assert_eq!(data.selections, [kuroya_core::Selection::caret(1)]);
    assert!(data.find_matches.is_empty());
    assert!(data.document_highlight_ranges.is_empty());
}

#[test]
fn large_file_mode_forces_line_rendering_cap() {
    let cap = i64::try_from(LARGE_FILE_MODE_LINE_RENDER_CHAR_LIMIT).unwrap();

    assert_eq!(editor_stop_rendering_line_after_for_mode(-1, false), -1);
    assert_eq!(editor_stop_rendering_line_after_for_mode(-1, true), cap);
    assert_eq!(
        editor_stop_rendering_line_after_for_mode(i64::MAX, true),
        cap
    );
    assert_eq!(editor_stop_rendering_line_after_for_mode(240, true), 240);
}

#[test]
fn editor_word_wrap_resolves_overrides_before_diff_override() {
    let mut settings = EditorSettings {
        word_wrap: EditorWordWrap::Bounded,
        word_wrap_override1: EditorWordWrapOverride::Off,
        word_wrap_override2: EditorWordWrapOverride::Inherit,
        diff_word_wrap: DiffWordWrap::Inherit,
        ..Default::default()
    };

    assert_eq!(
        editor_word_wrap_for_buffer(&settings, LanguageId::Rust),
        EditorWordWrap::Off
    );
    assert_eq!(
        editor_word_wrap_for_buffer(&settings, LanguageId::Diff),
        EditorWordWrap::Off
    );

    settings.word_wrap_override2 = EditorWordWrapOverride::On;
    settings.diff_word_wrap = DiffWordWrap::Off;

    assert_eq!(
        editor_word_wrap_for_buffer(&settings, LanguageId::Rust),
        EditorWordWrap::On
    );
    assert_eq!(
        editor_word_wrap_for_buffer(&settings, LanguageId::Diff),
        EditorWordWrap::Off
    );
}

#[test]
fn unicode_highlight_allowed_characters_keeps_single_enabled_character_keys() {
    let values = BTreeMap::from([
        ("Α".to_owned(), true),
        ("ß".to_owned(), false),
        ("ab".to_owned(), true),
    ]);

    assert_eq!(
        unicode_highlight_allowed_characters(&values),
        BTreeSet::from(['Α'])
    );
}

#[test]
fn unicode_highlight_allowed_locales_keep_enabled_language_tags() {
    let values = BTreeMap::from([
        (" fr-FR ".to_owned(), true),
        ("ja_JP".to_owned(), true),
        ("_os".to_owned(), true),
        ("ru".to_owned(), false),
    ]);

    assert_eq!(
        unicode_highlight_allowed_locales(&values),
        BTreeSet::from(["fr".to_owned(), "ja".to_owned()])
    );
}

#[test]
fn editor_ime_preedit_is_scoped_to_active_focused_buffer() {
    let preedit = EditorImePreedit {
        buffer_id: 7,
        text: "wen".to_owned(),
    };

    assert_eq!(
        super::editor_ime_preedit_for_buffer(Some(&preedit), 7, true).as_deref(),
        Some("wen")
    );
    assert_eq!(
        super::editor_ime_preedit_for_buffer(Some(&preedit), 8, true),
        None
    );
    assert_eq!(
        super::editor_ime_preedit_for_buffer(Some(&preedit), 7, false),
        None
    );
}

#[test]
fn line_render_protection_disables_bracket_scans() {
    assert_eq!(
        editor_match_brackets_for_mode(EditorMatchBrackets::Always, false, true),
        EditorMatchBrackets::Never
    );
    assert_eq!(
        editor_match_brackets_for_mode(EditorMatchBrackets::Near, false, true),
        EditorMatchBrackets::Never
    );
    assert_eq!(
        editor_match_brackets_for_mode(EditorMatchBrackets::Always, false, false),
        EditorMatchBrackets::Always
    );
    assert_eq!(
        editor_bracket_pair_guides_for_mode(EditorBracketPairGuideMode::On, false, true),
        EditorBracketPairGuideMode::Off
    );
    assert_eq!(
        editor_bracket_pair_guides_for_mode(EditorBracketPairGuideMode::Active, false, false),
        EditorBracketPairGuideMode::Active
    );
    assert_eq!(
        editor_bracket_pair_guides_for_mode(EditorBracketPairGuideMode::On, true, false),
        EditorBracketPairGuideMode::Off
    );
}

#[test]
fn bracket_scan_protection_disables_bracket_pair_colorization() {
    assert!(editor_bracket_pair_colorization_enabled(true, false, false));
    assert!(!editor_bracket_pair_colorization_enabled(
        false, false, false
    ));
    assert!(!editor_bracket_pair_colorization_enabled(true, true, false));
    assert!(!editor_bracket_pair_colorization_enabled(true, false, true));
}

#[test]
fn inline_annotation_settings_gate_lsp_overlays() {
    assert!(editor_code_lens_enabled(true, false));
    assert!(!editor_code_lens_enabled(false, false));
    assert!(!editor_code_lens_enabled(true, true));
    assert!(editor_diff_code_lenses_enabled(true, false));
    assert!(!editor_diff_code_lenses_enabled(false, false));
    assert!(!editor_diff_code_lenses_enabled(true, true));
    assert!(editor_inlay_hints_enabled(true, false));
    assert!(!editor_inlay_hints_enabled(false, false));
    assert!(!editor_inlay_hints_enabled(true, true));
    assert!(editor_git_blame_decoration_enabled(true, false));
    assert!(!editor_git_blame_decoration_enabled(false, false));
    assert!(!editor_git_blame_decoration_enabled(true, true));
}

#[test]
fn renderable_cached_line_annotations_drop_out_of_bounds_rows() {
    let hints = vec![
        inlay_hint(1, 1, "ok"),
        inlay_hint(3, 1, "past"),
        inlay_hint(2, 0, "bad-column"),
    ];
    let lenses = vec![
        code_lens(1, 1, "Run"),
        code_lens(0, 1, "zero"),
        code_lens(2, 0, "bad-column"),
        code_lens(3, 1, "past"),
    ];
    let blame = vec![
        git_blame_line(1, "first"),
        git_blame_line(2, "second"),
        git_blame_line(3, "past"),
        git_blame_line(0, "zero"),
    ];

    assert_eq!(
        renderable_inlay_hints(&hints, 2),
        vec![inlay_hint(1, 1, "ok")]
    );
    assert_eq!(
        renderable_code_lenses(&lenses, 2),
        vec![code_lens(1, 1, "Run")]
    );
    assert_eq!(
        renderable_git_blame_lines(&blame, 2),
        vec![git_blame_line(1, "first"), git_blame_line(2, "second")]
    );
}

#[test]
fn large_file_mode_disables_live_find_and_diff_overview_work() {
    assert!(editor_find_matches_enabled(true, false));
    assert!(!editor_find_matches_enabled(false, false));
    assert!(!editor_find_matches_enabled(true, true));

    assert!(editor_vim_search_matches_enabled(true, false));
    assert!(!editor_vim_search_matches_enabled(false, false));
    assert!(!editor_vim_search_matches_enabled(true, true));

    assert!(editor_diff_overview_ruler_enabled(true, false));
    assert!(!editor_diff_overview_ruler_enabled(false, false));
    assert!(!editor_diff_overview_ruler_enabled(true, true));
}

#[test]
fn large_file_mode_disables_scm_diff_decorations() {
    assert!(editor_scm_diff_gutter_enabled(
        ScmDiffDecorations::All,
        false
    ));
    assert!(editor_scm_diff_overview_enabled(
        ScmDiffDecorations::All,
        false
    ));
    assert!(editor_scm_diff_minimap_enabled(
        ScmDiffDecorations::All,
        false
    ));

    assert!(!editor_scm_diff_gutter_enabled(
        ScmDiffDecorations::All,
        true
    ));
    assert!(!editor_scm_diff_overview_enabled(
        ScmDiffDecorations::All,
        true
    ));
    assert!(!editor_scm_diff_minimap_enabled(
        ScmDiffDecorations::All,
        true
    ));
    assert!(!editor_scm_diff_gutter_enabled(
        ScmDiffDecorations::Overview,
        false
    ));
    assert!(!editor_scm_diff_overview_enabled(
        ScmDiffDecorations::Gutter,
        false
    ));
}

#[test]
fn validation_decorations_follow_setting_readonly_and_large_file_mode() {
    assert!(editor_validation_decorations_enabled(
        EditorRenderValidationDecorations::Editable,
        false,
        false
    ));
    assert!(!editor_validation_decorations_enabled(
        EditorRenderValidationDecorations::Editable,
        true,
        false
    ));
    assert!(editor_validation_decorations_enabled(
        EditorRenderValidationDecorations::On,
        true,
        false
    ));
    assert!(!editor_validation_decorations_enabled(
        EditorRenderValidationDecorations::Off,
        false,
        false
    ));
    assert!(!editor_validation_decorations_enabled(
        EditorRenderValidationDecorations::On,
        false,
        true
    ));
}

#[test]
fn diagnostic_tag_spans_stay_governed_by_tag_settings_not_validation_gate() {
    let root = PathBuf::from("workspace");
    let path = root.join("src/main.rs");
    let mut app = app_for_test(root);
    app.buffers.push(TextBuffer::from_text(
        7,
        Some(path.clone()),
        "alpha beta\n".to_owned(),
    ));
    app.diagnostics.replace_lsp(
        path.clone(),
        vec![kuroya_core::Diagnostic {
            path: path.clone(),
            line: 1,
            column: 1,
            char_range: 0..5,
            severity: kuroya_core::DiagnosticSeverity::Hint,
            source: "rust-analyzer".to_owned(),
            message: "unused".to_owned(),
            unused: true,
            deprecated: false,
        }],
    );
    // Severity decorations off, tag styling on.
    app.settings.render_validation_decorations = EditorRenderValidationDecorations::Off;
    app.settings.show_unused = true;
    app.settings.show_deprecated = true;

    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);

    // The validation gate holds for the severity maps ...
    assert!(data.diagnostics_by_line.is_empty());
    assert!(data.diagnostic_messages.is_empty());
    // ... while tag spans follow their own show_unused/show_deprecated
    // toggles, independent of the validation setting.
    assert_eq!(
        data.diagnostic_tag_spans,
        vec![(0..5, crate::editor_pane_support::DiagnosticTagKind::Unused)]
    );

    // Turning the tag's own toggle off clears the tag spans.
    app.settings.show_unused = false;
    let data = app.prepare_editor_pane_data(7, 0, 8.0, true, true);
    assert!(data.diagnostic_tag_spans.is_empty());
}

#[test]
fn diff_code_lenses_mark_hunk_headers_with_stage_actions() {
    let buffer = TextBuffer::from_text(
        1,
        None,
        "--- a/main.rs\n+++ b/main.rs\n@@ -1 +1 @@\n-old\n+new\n@@ -7 +7 @@\n-left\n+right\n"
            .to_owned(),
    );
    let lenses = diff_code_lenses_for_patch_buffer(&buffer, Some(GitChangeStage::Unstaged));

    assert_eq!(lenses.len(), 2);
    assert_eq!(lenses[0].line, 3);
    assert_eq!(lenses[1].line, 6);
    assert_eq!(
        lenses[0].title,
        "Prev | Next | Copy Hunk | A11y Diff | Stage | Discard"
    );

    let staged_buffer = TextBuffer::from_text(2, None, "@@ -1 +1 @@\n".to_owned());
    let staged = diff_code_lenses_for_patch_buffer(&staged_buffer, Some(GitChangeStage::Staged));
    assert_eq!(
        staged[0].title,
        "Prev | Next | Copy Hunk | A11y Diff | Unstage"
    );
}

#[test]
fn code_lenses_sort_after_diff_lenses_are_appended() {
    let buffer = TextBuffer::from_text(
        1,
        None,
        "--- a/main.rs\n@@ -1 +1 @@\n-old\n+new\n@@ -7 +7 @@\n-left\n+right\n".to_owned(),
    );
    let mut lenses = vec![
        LspCodeLens {
            line: 3,
            column: 2,
            title: "LSP Line 3".to_owned(),
            command: Some("lsp.line3".to_owned()),
            command_arguments: None,
            resolve_payload: None,
        },
        LspCodeLens {
            line: 8,
            column: 1,
            title: "LSP Line 8".to_owned(),
            command: Some("lsp.line8".to_owned()),
            command_arguments: None,
            resolve_payload: None,
        },
    ];

    lenses.extend(diff_code_lenses_for_patch_buffer(&buffer, None));
    sort_code_lenses_by_position(&mut lenses);

    assert_eq!(
        lenses
            .iter()
            .map(|lens| (lens.line, lens.column, lens.title.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (2, 1, "Prev | Next | Copy Hunk | A11y Diff"),
            (3, 2, "LSP Line 3"),
            (5, 1, "Prev | Next | Copy Hunk | A11y Diff"),
            (8, 1, "LSP Line 8"),
        ]
    );
}

#[test]
fn diff_code_lenses_skip_patch_buffers_above_overlay_scan_budget() {
    let text = std::iter::once("@@ -1 +1 @@")
        .chain(std::iter::repeat_n(
            " context",
            DIFF_PATCH_OVERLAY_SCAN_MAX_LINES,
        ))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, None, text);

    assert!(buffer.len_lines() > DIFF_PATCH_OVERLAY_SCAN_MAX_LINES);
    assert!(diff_code_lenses_for_patch_buffer(&buffer, None).is_empty());
}

#[test]
fn diff_code_lenses_skip_patch_buffers_above_byte_budget() {
    let text = format!("@@ -1 +1 @@\n+{}", "x".repeat(LARGE_FILE_MODE_MAX_BYTES));
    let buffer = TextBuffer::from_text(1, None, text);

    assert!(buffer.len_bytes() > LARGE_FILE_MODE_MAX_BYTES);
    assert!(diff_code_lenses_for_patch_buffer(&buffer, None).is_empty());
}

#[test]
fn diff_moved_patch_lines_marks_matching_added_and_deleted_content() {
    let buffer = TextBuffer::from_text(
            1,
            None,
            "--- a/lib.rs\n+++ b/lib.rs\n@@ -1,4 +1,4 @@\n-old();\n keep();\n+old();\n-empty\n+\n+other\n"
                .to_owned(),
        );

    let moved = diff_moved_patch_lines(&buffer, true, true);

    assert_eq!(moved, BTreeSet::from([4, 6]));
    assert!(diff_moved_patch_lines(&buffer, false, true).is_empty());
    assert!(diff_moved_patch_lines(&buffer, true, false).is_empty());
}

#[test]
fn diff_moved_patch_lines_skip_patch_buffers_above_overlay_scan_budget() {
    let text = std::iter::once("-moved();")
        .chain(std::iter::once("+moved();"))
        .chain(std::iter::repeat_n(
            " context",
            DIFF_PATCH_OVERLAY_SCAN_MAX_LINES,
        ))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, None, text);

    assert!(buffer.len_lines() > DIFF_PATCH_OVERLAY_SCAN_MAX_LINES);
    assert!(diff_moved_patch_lines(&buffer, true, true).is_empty());
}

#[test]
fn diff_moved_patch_lines_skip_patch_buffers_above_byte_budget() {
    let text = format!(
        "-moved();\n+moved();\n {}",
        "x".repeat(LARGE_FILE_MODE_MAX_BYTES)
    );
    let buffer = TextBuffer::from_text(1, None, text);

    assert!(buffer.len_bytes() > LARGE_FILE_MODE_MAX_BYTES);
    assert!(diff_moved_patch_lines(&buffer, true, true).is_empty());
}

#[test]
fn folding_ranges_follow_maximum_regions_setting() {
    let ranges = vec![
        folding_range(1, 4),
        folding_range(5, 8),
        folding_range(9, 12),
    ];

    assert_eq!(capped_folding_ranges(&ranges, 2), ranges[..2].to_vec());
    assert!(capped_folding_ranges(&ranges, 0).is_empty());

    let folded = vec![
        FoldedRange {
            start_line: 1,
            end_line: 4,
        },
        FoldedRange {
            start_line: 9,
            end_line: 12,
        },
    ];
    assert_eq!(
        folded_ranges_allowed_by_folding_ranges(&folded, &ranges[..2]),
        vec![FoldedRange {
            start_line: 1,
            end_line: 4,
        }]
    );
}

#[test]
fn folding_strategy_selects_cached_or_indentation_ranges() {
    let buffer = TextBuffer::from_text(
        1,
        None,
        "root\n    child\n    block\n        nested\n    done\n".to_owned(),
    );
    let cached = vec![folding_range(1, 5), folding_range(20, 30)];

    assert_eq!(
        editor_folding_ranges_for_buffer(&buffer, Some(&cached), EditorFoldingStrategy::Auto, 5),
        vec![folding_range(1, 5)]
    );

    let indentation = editor_folding_ranges_for_buffer(
        &buffer,
        Some(&cached),
        EditorFoldingStrategy::Indentation,
        5,
    );
    assert_eq!(indentation, vec![folding_range(1, 5), folding_range(3, 4)]);
    assert_eq!(
        editor_folding_ranges_for_buffer(
            &buffer,
            Some(&cached),
            EditorFoldingStrategy::Indentation,
            1
        ),
        vec![folding_range(1, 5)]
    );
}

#[test]
fn folding_ranges_drop_stale_invalid_cached_ranges_before_capping() {
    let buffer = TextBuffer::from_text(1, None, "root\n    child\n    done\n".to_owned());
    let cached = vec![
        folding_range(0, 2),
        folding_range(1, 3),
        folding_range(2, 2),
        folding_range(2, 99),
        folding_range(1, 2),
    ];

    assert_eq!(
        editor_folding_ranges_for_buffer(&buffer, Some(&cached), EditorFoldingStrategy::Auto, 2,),
        vec![folding_range(1, 3), folding_range(1, 2)]
    );
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

fn git_blame_line(line_number: usize, summary: &str) -> GitBlameLine {
    GitBlameLine {
        line_number,
        short_oid: "abcdef0".to_owned(),
        author: "Kuroya".to_owned(),
        author_time_seconds: 0,
        summary: summary.to_owned(),
    }
}

fn app_for_test(root: PathBuf) -> KuroyaApp {
    let (tx, rx) = crate::ui_event_channel::ui_event_channel();
    let settings = EditorSettings::default();
    KuroyaApp::from_startup_context(AppStartupContext {
        runtime: Runtime::new().expect("test runtime"),
        tx,
        rx,
        workspace: Workspace::new(root.clone()),
        settings: settings.clone(),
        settings_panel_draft: settings,
        settings_editor_font_path: String::new(),
        settings_ui_font_path: String::new(),
        theme_picker_selected: 0,
        saved_session: None,
        terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
        watcher: None,
        recent_projects: Vec::new(),
        trusted_workspaces: vec![root],
        now: Instant::now(),
        startup_timings: Vec::new(),
    })
}

fn folding_range(start_line: usize, end_line: usize) -> LspFoldingRange {
    LspFoldingRange {
        start_line,
        start_column: None,
        end_line,
        end_column: None,
        kind: None,
    }
}

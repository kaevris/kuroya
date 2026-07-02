use super::*;

mod visual;

#[test]
fn editor_match_brackets_accepts_legacy_boolean_values() {
    let disabled: EditorSettings = toml::from_str("match_brackets = false\n")
        .expect("legacy disabled match_brackets should load");
    assert_eq!(disabled.match_brackets, EditorMatchBrackets::Never);

    let enabled: EditorSettings = toml::from_str("match_brackets = true\n")
        .expect("legacy enabled match_brackets should load");
    assert_eq!(enabled.match_brackets, EditorMatchBrackets::Always);
}

#[test]
fn editor_find_seed_accepts_named_and_legacy_boolean_values() {
    let selection: EditorSettings =
        toml::from_str("find_seed_search_string_from_selection = \"selection\"\n")
            .expect("selection-only find seed setting should load");
    assert_eq!(
        selection.find_seed_search_string_from_selection,
        EditorFindSeedSearchStringFromSelection::Selection
    );

    let disabled: EditorSettings =
        toml::from_str("find_seed_search_string_from_selection = false\n")
            .expect("legacy disabled find seed setting should load");
    assert_eq!(
        disabled.find_seed_search_string_from_selection,
        EditorFindSeedSearchStringFromSelection::Never
    );

    let enabled: EditorSettings = toml::from_str("find_seed_search_string_from_selection = true\n")
        .expect("legacy enabled find seed setting should load");
    assert_eq!(
        enabled.find_seed_search_string_from_selection,
        EditorFindSeedSearchStringFromSelection::Always
    );
}

#[test]
fn editor_auto_find_in_selection_accepts_named_and_legacy_boolean_values() {
    let multiline: EditorSettings = toml::from_str("find_auto_find_in_selection = \"multiline\"\n")
        .expect("multiline auto-find-in-selection setting should load");
    assert_eq!(
        multiline.find_auto_find_in_selection,
        EditorFindAutoFindInSelection::Multiline
    );

    let disabled: EditorSettings = toml::from_str("find_auto_find_in_selection = false\n")
        .expect("legacy disabled auto-find-in-selection setting should load");
    assert_eq!(
        disabled.find_auto_find_in_selection,
        EditorFindAutoFindInSelection::Never
    );

    let enabled: EditorSettings = toml::from_str("find_auto_find_in_selection = true\n")
        .expect("legacy enabled auto-find-in-selection setting should load");
    assert_eq!(
        enabled.find_auto_find_in_selection,
        EditorFindAutoFindInSelection::Always
    );
}

#[test]
fn validation_decorations_accept_kuroya_union_values() {
    let editable: EditorSettings = toml::from_str("render_validation_decorations = \"editable\"\n")
        .expect("editable validation decorations setting should load");
    assert_eq!(
        editable.render_validation_decorations,
        EditorRenderValidationDecorations::Editable
    );
    assert!(editable.render_validation_decorations.visible(false));
    assert!(!editable.render_validation_decorations.visible(true));

    let off: EditorSettings = toml::from_str("render_validation_decorations = false\n")
        .expect("false validation decorations setting should load");
    assert_eq!(
        off.render_validation_decorations,
        EditorRenderValidationDecorations::Off
    );
    assert!(!off.render_validation_decorations.visible(false));

    let on: EditorSettings = toml::from_str("render_validation_decorations = true\n")
        .expect("true validation decorations setting should load");
    assert_eq!(
        on.render_validation_decorations,
        EditorRenderValidationDecorations::On
    );
    assert!(on.render_validation_decorations.visible(true));
}

#[test]
fn line_decorations_width_accepts_kuroya_union_values() {
    let pixels: EditorSettings =
        toml::from_str("line_decorations_width = 16.0\n").expect("pixel width should load");
    assert_eq!(
        pixels.line_decorations_width,
        EditorLineDecorationsWidth::Pixels(16.0)
    );

    let chars: EditorSettings =
        toml::from_str("line_decorations_width = \"1.5ch\"\n").expect("ch width should load");
    assert_eq!(
        chars.line_decorations_width,
        EditorLineDecorationsWidth::Ch(1.5)
    );
    assert_eq!(chars.line_decorations_width.pixels(10.0), 15.0);
}

#[test]
fn word_wrap_accepts_kuroya_modes_and_diff_inherits_editor_mode() {
    let column: EditorSettings = toml::from_str("word_wrap = \"wordWrapColumn\"\n")
        .expect("wordWrapColumn setting should load");
    assert_eq!(column.word_wrap, EditorWordWrap::WordWrapColumn);

    let bounded: EditorSettings =
        toml::from_str("word_wrap = \"bounded\"\n").expect("bounded word wrap should load");
    assert_eq!(bounded.word_wrap, EditorWordWrap::Bounded);
    assert_eq!(
        DiffWordWrap::Inherit.resolve(EditorWordWrap::Bounded),
        EditorWordWrap::Bounded
    );
    assert_eq!(
        DiffWordWrap::Off.resolve(EditorWordWrap::Bounded),
        EditorWordWrap::Off
    );
    assert_eq!(
        DiffWordWrap::On.resolve(EditorWordWrap::Off),
        EditorWordWrap::On
    );
    assert_eq!(
        EditorWordWrapOverride::Inherit.resolve(EditorWordWrap::Bounded),
        EditorWordWrap::Bounded
    );
    assert_eq!(
        EditorWordWrapOverride::Off.resolve(EditorWordWrap::Bounded),
        EditorWordWrap::Off
    );
    assert_eq!(
        EditorWordWrapOverride::On.resolve(EditorWordWrap::Off),
        EditorWordWrap::On
    );
}

#[test]
fn word_segmenter_locales_accept_vs_code_string_or_list() {
    let single: EditorSettings = toml::from_str("word_segmenter_locales = \"ja\"\n")
        .expect("single word segmenter locale should load");
    assert_eq!(single.word_segmenter_locales, ["ja"]);

    let multiple: EditorSettings = toml::from_str("word_segmenter_locales = [\"ja\", \"zh-CN\"]\n")
        .expect("word segmenter locale list should load");
    assert_eq!(multiple.word_segmenter_locales, ["ja", "zh-CN"]);
}

#[test]
fn editor_vim_settings_accept_nested_overrides() {
    let settings: EditorSettings = toml::from_str(
        r#"
        vim_keybindings = true

        [vim]
        disabled_bindings = ["Q", "<C-n>"]

        [[vim.key_overrides]]
        before = "H"
        after = "0"

        [[vim.key_overrides]]
        before = "<Home>"
        after = "<C-r>"
        "#,
    )
    .expect("nested vim settings should load");

    assert!(settings.vim_keybindings);
    assert_eq!(settings.vim.disabled_bindings, ["Q", "<C-n>"]);
    assert_eq!(
        settings.vim.key_overrides,
        [
            EditorVimKeyOverride {
                before: "H".to_owned(),
                after: "0".to_owned(),
                command: None,
            },
            EditorVimKeyOverride {
                before: "<Home>".to_owned(),
                after: "<C-r>".to_owned(),
                command: None,
            },
        ]
    );
}

#[test]
fn editor_vim_settings_accept_command_overrides() {
    let settings: EditorSettings = toml::from_str(
        r#"
        [[vim.key_overrides]]
        before = "K"
        command = "RequestHover"
        "#,
    )
    .expect("nested vim command override should load");

    assert_eq!(
        settings.vim.key_overrides,
        [EditorVimKeyOverride {
            before: "K".to_owned(),
            after: String::new(),
            command: Some(crate::Command::RequestHover),
        }]
    );
}

#[test]
fn editor_vim_settings_sanitize_empty_duplicate_and_long_entries() {
    let mut settings = EditorSettings::default();
    settings.vim.disabled_bindings = vec![" x ".to_owned(), "x".to_owned(), String::new()];
    settings.vim.key_overrides = vec![
        EditorVimKeyOverride {
            before: " H ".to_owned(),
            after: " 0 ".to_owned(),
            command: None,
        },
        EditorVimKeyOverride {
            before: "H".to_owned(),
            after: "gg".to_owned(),
            command: None,
        },
        EditorVimKeyOverride {
            before: String::new(),
            after: "0".to_owned(),
            command: None,
        },
        EditorVimKeyOverride {
            before: " K ".to_owned(),
            after: String::new(),
            command: Some(crate::Command::RequestHover),
        },
    ];

    assert!(settings.sanitize());
    assert_eq!(settings.vim.disabled_bindings, ["x"]);
    assert_eq!(
        settings.vim.key_overrides,
        [
            EditorVimKeyOverride {
                before: "H".to_owned(),
                after: "0".to_owned(),
                command: None,
            },
            EditorVimKeyOverride {
                before: "K".to_owned(),
                after: String::new(),
                command: Some(crate::Command::RequestHover),
            },
        ]
    );
}

#[test]
fn lsp_servers_default_to_builtin_configs() {
    let settings: EditorSettings = toml::from_str("").expect("empty settings should load");

    assert!(settings.lsp_servers.is_empty());
    assert_eq!(
        settings.lsp_server_configs(),
        crate::lsp::default_server_configs()
    );
}

#[test]
fn lsp_servers_override_builtins_and_append_custom_servers() {
    let settings: EditorSettings = toml::from_str(
        r#"
        [[lsp_servers]]
        language = "Rust"
        command = "rust-analyzer-custom"
        args = ["--stdio"]
        root_markers = ["Cargo.toml", ".git"]

        [[lsp_servers]]
        language = "go"
        command = "gopls-custom"
        root_markers = ["go.mod", "go.work", ".git"]

        [[lsp_servers]]
        language = "kuroya-test"
        command = "kuroya-test-lsp"
        args = ["--stdio"]
        root_markers = [".kuroya-test"]
        "#,
    )
    .expect("lsp server settings should load");

    let servers = settings.lsp_server_configs();
    let default_count = crate::lsp::default_server_configs().len();
    let rust = servers
        .iter()
        .find(|server| server.language == "rust")
        .expect("rust config should be present");
    let python = servers
        .iter()
        .find(|server| server.language == "python")
        .expect("python config should be present");
    let go = servers
        .iter()
        .find(|server| server.language == "go")
        .expect("custom go config should be present");
    let custom = servers
        .iter()
        .find(|server| server.language == "kuroya-test")
        .expect("custom config should be present");

    assert_eq!(servers.len(), default_count + 1);
    assert_eq!(rust.command, "rust-analyzer-custom");
    assert_eq!(rust.args, ["--stdio"]);
    assert_eq!(rust.root_markers, ["Cargo.toml", ".git"]);
    assert_eq!(python.command, "pyright-langserver");
    assert_eq!(go.command, "gopls-custom");
    assert!(go.args.is_empty());
    assert_eq!(go.root_markers, ["go.mod", "go.work", ".git"]);
    assert_eq!(custom.command, "kuroya-test-lsp");
    assert_eq!(custom.args, ["--stdio"]);
    assert_eq!(custom.root_markers, [".kuroya-test"]);
}

#[test]
fn lsp_server_configs_normalize_in_memory_custom_servers() {
    let mut settings = EditorSettings::default();
    settings.lsp_servers = vec![
        crate::lsp::LspServerConfig {
            language: " Rust ".to_owned(),
            command: " rust-analyzer-custom ".to_owned(),
            args: vec![" --stdio ".to_owned()],
            extensions: vec![".rs".to_owned(), "rs".to_owned(), ".".to_owned()],
            root_markers: vec![" Cargo.toml ".to_owned()],
        },
        crate::lsp::LspServerConfig {
            language: "kuroya-empty".to_owned(),
            command: String::new(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
        },
        crate::lsp::LspServerConfig {
            language: " kuroya-test ".to_owned(),
            command: " kuroya-lsp ".to_owned(),
            args: vec![" --stdio ".to_owned()],
            extensions: vec![".kuroya".to_owned(), "kuroya".to_owned(), ".".to_owned()],
            root_markers: vec![" .kuroya-root ".to_owned()],
        },
        crate::lsp::LspServerConfig {
            language: "RUST".to_owned(),
            command: "rust-analyzer-final".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
        },
    ];

    let servers = settings.lsp_server_configs();
    let rust = servers
        .iter()
        .find(|server| server.language == "rust")
        .expect("rust config should be present");
    let custom = servers
        .iter()
        .find(|server| server.language == "kuroya-test")
        .expect("custom config should be present");

    assert_eq!(rust.command, "rust-analyzer-final");
    assert_eq!(custom.command, "kuroya-lsp");
    assert_eq!(custom.args, ["--stdio"]);
    assert_eq!(custom.extensions, ["kuroya"]);
    assert_eq!(custom.root_markers, [".kuroya-root"]);
    assert!(!servers.iter().any(|server| server.language == " Rust "));
    assert!(
        !servers
            .iter()
            .any(|server| server.language == "kuroya-empty")
    );
}

#[test]
fn lsp_server_configs_sanitize_custom_servers() {
    let mut settings = EditorSettings::default();
    settings.lsp_servers = vec![
        crate::lsp::LspServerConfig {
            language: " Rust ".to_owned(),
            command: " rust-analyzer-custom ".to_owned(),
            args: vec![" --stdio ".to_owned()],
            extensions: vec![".rs".to_owned(), "rs".to_owned(), ".".to_owned()],
            root_markers: vec![" Cargo.toml ".to_owned()],
        },
        crate::lsp::LspServerConfig {
            language: "kuroya-empty".to_owned(),
            command: String::new(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
        },
        crate::lsp::LspServerConfig {
            language: " kuroya-test ".to_owned(),
            command: " kuroya-lsp ".to_owned(),
            args: vec![" --stdio ".to_owned()],
            extensions: vec![".kuroya".to_owned(), "kuroya".to_owned(), ".".to_owned()],
            root_markers: vec![" .kuroya-root ".to_owned()],
        },
        crate::lsp::LspServerConfig {
            language: "RUST".to_owned(),
            command: "rust-analyzer-final".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
        },
    ];

    assert!(settings.sanitize());
    assert_eq!(
        settings.lsp_servers,
        vec![
            crate::lsp::LspServerConfig {
                language: "rust".to_owned(),
                command: "rust-analyzer-final".to_owned(),
                args: Vec::new(),
                extensions: Vec::new(),
                root_markers: Vec::new(),
            },
            crate::lsp::LspServerConfig {
                language: "kuroya-test".to_owned(),
                command: "kuroya-lsp".to_owned(),
                args: vec!["--stdio".to_owned()],
                extensions: vec!["kuroya".to_owned()],
                root_markers: vec![".kuroya-root".to_owned()],
            },
        ]
    );
    assert!(!settings.sanitize());
}

#[test]
fn active_indentation_highlight_accepts_kuroya_union_values() {
    let focused: EditorSettings = toml::from_str("highlight_active_indentation = true\n")
        .expect("boolean active indent setting should load");
    assert_eq!(
        focused.highlight_active_indentation,
        EditorHighlightActiveIndentation::Focused
    );
    assert!(focused.highlight_active_indentation.visible(true));
    assert!(!focused.highlight_active_indentation.visible(false));

    let off: EditorSettings = toml::from_str("highlight_active_indentation = false\n")
        .expect("false active indent setting should load");
    assert_eq!(
        off.highlight_active_indentation,
        EditorHighlightActiveIndentation::Off
    );
    assert!(!off.highlight_active_indentation.visible(true));

    let always: EditorSettings = toml::from_str("highlight_active_indentation = \"always\"\n")
        .expect("always active indent setting should load");
    assert_eq!(
        always.highlight_active_indentation,
        EditorHighlightActiveIndentation::Always
    );
    assert!(always.highlight_active_indentation.visible(false));
}

#[test]
fn autosave_mode_supports_legacy_bool_and_named_modes() {
    let legacy_off: EditorSettings =
        toml::from_str("autosave = false\n").expect("legacy autosave bool should load");
    assert_eq!(
        legacy_off.effective_autosave_mode(),
        EditorAutoSaveMode::Off
    );

    let focus_change: EditorSettings = toml::from_str("autosave_mode = \"onFocusChange\"\n")
        .expect("focus-change autosave mode should load");
    assert_eq!(
        focus_change.effective_autosave_mode(),
        EditorAutoSaveMode::OnFocusChange
    );

    let window_change: EditorSettings = toml::from_str("autosave_mode = \"onWindowChange\"\n")
        .expect("window-change autosave mode should load");
    assert_eq!(
        window_change.effective_autosave_mode(),
        EditorAutoSaveMode::OnWindowChange
    );

    let explicit_off: EditorSettings =
        toml::from_str("autosave_mode = \"off\"\n").expect("off autosave mode should load");
    assert_eq!(
        explicit_off.effective_autosave_mode(),
        EditorAutoSaveMode::Off
    );
}

#[test]
fn editor_cursor_width_is_clamped_to_reasonable_range() {
    assert_eq!(clamp_editor_cursor_width(0.0), MIN_EDITOR_CURSOR_WIDTH);
    assert_eq!(clamp_editor_cursor_width(3.0), 3.0);
    assert_eq!(
        clamp_editor_cursor_width(f32::INFINITY),
        DEFAULT_EDITOR_CURSOR_WIDTH
    );
    assert_eq!(clamp_editor_cursor_height(0), MIN_EDITOR_CURSOR_HEIGHT);
    assert_eq!(clamp_editor_cursor_height(18), 18);
    assert_eq!(
        clamp_editor_cursor_height(usize::MAX),
        MAX_EDITOR_CURSOR_HEIGHT
    );
}

#[test]
fn editor_numeric_settings_are_clamped_to_reasonable_ranges() {
    assert_eq!(clamp_autosave_delay_ms(1), MIN_AUTOSAVE_DELAY_MS);
    assert_eq!(clamp_autosave_delay_ms(1_500), 1_500);
    assert_eq!(clamp_autosave_delay_ms(u64::MAX), MAX_AUTOSAVE_DELAY_MS);
    assert_eq!(clamp_editor_line_height(-1.0), MIN_EDITOR_LINE_HEIGHT);
    assert_eq!(clamp_editor_line_height(22.0), 22.0);
    assert_eq!(
        clamp_editor_line_height(f32::NAN),
        DEFAULT_EDITOR_LINE_HEIGHT
    );
    assert_eq!(
        clamp_editor_letter_spacing(-10.0),
        MIN_EDITOR_LETTER_SPACING
    );
    assert_eq!(clamp_editor_letter_spacing(1.25), 1.25);
    assert_eq!(
        clamp_editor_letter_spacing(f32::NAN),
        DEFAULT_EDITOR_LETTER_SPACING
    );
    assert_eq!(sanitize_editor_font_weight(" 600 "), "600".to_owned());
    assert_eq!(
        sanitize_editor_font_weight("1001"),
        DEFAULT_EDITOR_FONT_WEIGHT.to_owned()
    );
    assert_eq!(clamp_editor_ruler_column(80), 80);
    assert_eq!(
        clamp_editor_ruler_column(usize::MAX),
        MAX_EDITOR_RULER_COLUMN
    );
    assert_eq!(
        clamp_editor_line_decorations_width(-1.0),
        MIN_EDITOR_LINE_DECORATIONS_WIDTH
    );
    assert_eq!(clamp_editor_line_decorations_width(16.0), 16.0);
    assert_eq!(
        clamp_editor_line_decorations_width(f32::NAN),
        DEFAULT_EDITOR_LINE_DECORATIONS_WIDTH
    );
    assert_eq!(
        clamp_editor_line_decorations_width(f32::MAX),
        MAX_EDITOR_LINE_DECORATIONS_WIDTH
    );
    assert_eq!(
        clamp_editor_line_numbers_min_chars(0),
        MIN_EDITOR_LINE_NUMBERS_MIN_CHARS
    );
    assert_eq!(clamp_editor_line_numbers_min_chars(8), 8);
    assert_eq!(
        clamp_editor_line_numbers_min_chars(usize::MAX),
        MAX_EDITOR_LINE_NUMBERS_MIN_CHARS
    );
    assert_eq!(
        clamp_editor_accessibility_page_size(0),
        MIN_EDITOR_ACCESSIBILITY_PAGE_SIZE
    );
    assert_eq!(clamp_editor_accessibility_page_size(250), 250);
    assert_eq!(
        clamp_editor_accessibility_page_size(usize::MAX),
        MAX_EDITOR_ACCESSIBILITY_PAGE_SIZE
    );
    assert_eq!(clamp_editor_tab_index(i64::MIN), MIN_EDITOR_TAB_INDEX);
    assert_eq!(clamp_editor_tab_index(-1), -1);
    assert_eq!(clamp_editor_tab_index(i64::MAX), MAX_EDITOR_TAB_INDEX);
    assert_eq!(
        clamp_editor_word_wrap_column(0),
        MIN_EDITOR_WORD_WRAP_COLUMN
    );
    assert_eq!(clamp_editor_word_wrap_column(96), 96);
    assert_eq!(
        clamp_editor_word_wrap_column(usize::MAX),
        MAX_EDITOR_WORD_WRAP_COLUMN
    );
    assert_eq!(
        clamp_editor_stop_rendering_line_after(i64::MIN),
        MIN_EDITOR_STOP_RENDERING_LINE_AFTER
    );
    assert_eq!(clamp_editor_stop_rendering_line_after(240), 240);
    assert_eq!(
        clamp_editor_stop_rendering_line_after(i64::MAX),
        MAX_EDITOR_STOP_RENDERING_LINE_AFTER
    );
    assert_eq!(
        clamp_editor_reveal_horizontal_right_padding(usize::MAX),
        MAX_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING
    );
    assert_eq!(clamp_editor_reveal_horizontal_right_padding(30), 30);
    assert_eq!(
        clamp_editor_overview_ruler_lanes(usize::MAX),
        MAX_EDITOR_OVERVIEW_RULER_LANES
    );
    assert_eq!(clamp_editor_overview_ruler_lanes(2), 2);
    assert_eq!(editor_stop_rendering_line_after_limit(-1), None);
    assert_eq!(editor_stop_rendering_line_after_limit(240), Some(240));
    assert_eq!(
        clamp_editor_minimap_max_column(1),
        MIN_EDITOR_MINIMAP_MAX_COLUMN
    );
    assert_eq!(clamp_editor_minimap_max_column(80), 80);
    assert_eq!(
        clamp_editor_minimap_max_column(usize::MAX),
        MAX_EDITOR_MINIMAP_MAX_COLUMN
    );
    assert_eq!(clamp_editor_minimap_scale(0), MIN_EDITOR_MINIMAP_SCALE);
    assert_eq!(clamp_editor_minimap_scale(2), 2);
    assert_eq!(
        clamp_editor_minimap_scale(usize::MAX),
        MAX_EDITOR_MINIMAP_SCALE
    );
    assert_eq!(
        clamp_editor_minimap_section_header_font_size(1.0),
        MIN_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE
    );
    assert_eq!(clamp_editor_minimap_section_header_font_size(12.0), 12.0);
    assert_eq!(
        clamp_editor_minimap_section_header_font_size(f32::NAN),
        DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE
    );
    assert_eq!(
        clamp_editor_minimap_section_header_letter_spacing(-1.0),
        MIN_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING
    );
    assert_eq!(clamp_editor_minimap_section_header_letter_spacing(2.0), 2.0);
    assert_eq!(
        clamp_editor_minimap_section_header_letter_spacing(f32::NAN),
        DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING
    );
    assert_eq!(
        clamp_editor_sticky_scroll_max_line_count(0),
        MIN_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT
    );
    assert_eq!(clamp_editor_sticky_scroll_max_line_count(8), 8);
    assert_eq!(
        clamp_editor_sticky_scroll_max_line_count(usize::MAX),
        MAX_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT
    );
    assert_eq!(clamp_editor_padding(12), 12);
    assert_eq!(clamp_editor_padding(usize::MAX), MAX_EDITOR_PADDING);
    assert_eq!(clamp_editor_scrollbar_size(18), 18);
    assert_eq!(
        clamp_editor_scrollbar_size(usize::MAX),
        MAX_EDITOR_SCROLLBAR_SIZE
    );
    assert_eq!(clamp_editor_cursor_surrounding_lines(3), 3);
    assert_eq!(
        clamp_editor_cursor_surrounding_lines(usize::MAX),
        MAX_EDITOR_CURSOR_SURROUNDING_LINES
    );
    assert_eq!(clamp_editor_folding_maximum_regions(0), 0);
    assert_eq!(clamp_editor_folding_maximum_regions(5_000), 5_000);
    assert_eq!(
        clamp_editor_folding_maximum_regions(usize::MAX),
        MAX_EDITOR_FOLDING_MAXIMUM_REGIONS
    );
    assert_eq!(
        clamp_diff_render_side_by_side_inline_breakpoint(0),
        MIN_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT
    );
    assert_eq!(clamp_diff_render_side_by_side_inline_breakpoint(900), 900);
    assert_eq!(
        clamp_diff_render_side_by_side_inline_breakpoint(usize::MAX),
        MAX_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT
    );
    assert_eq!(clamp_diff_split_view_default_ratio(-1.0), 0.0);
    assert_eq!(clamp_diff_split_view_default_ratio(0.35), 0.35);
    assert_eq!(
        clamp_diff_split_view_default_ratio(f32::NAN),
        DEFAULT_DIFF_SPLIT_VIEW_DEFAULT_RATIO
    );
    assert_eq!(clamp_diff_split_view_default_ratio(f32::MAX), 1.0);
    assert_eq!(
        clamp_quick_suggestions_delay_ms(0),
        MIN_QUICK_SUGGESTIONS_DELAY_MS
    );
    assert_eq!(clamp_quick_suggestions_delay_ms(25), 25);
    assert_eq!(
        clamp_quick_suggestions_delay_ms(usize::MAX),
        MAX_QUICK_SUGGESTIONS_DELAY_MS
    );
    assert_eq!(clamp_hover_delay_ms(0), MIN_HOVER_DELAY_MS);
    assert_eq!(clamp_hover_delay_ms(450), 450);
    assert_eq!(clamp_hover_delay_ms(usize::MAX), MAX_HOVER_DELAY_MS);
    assert_eq!(clamp_hover_hiding_delay_ms(0), MIN_HOVER_HIDING_DELAY_MS);
    assert_eq!(clamp_hover_hiding_delay_ms(900), 900);
    assert_eq!(
        clamp_hover_hiding_delay_ms(usize::MAX),
        MAX_HOVER_HIDING_DELAY_MS
    );
    assert_eq!(clamp_suggest_font_size(0), MIN_SUGGEST_FONT_SIZE);
    assert_eq!(clamp_suggest_font_size(15), 15);
    assert_eq!(clamp_suggest_font_size(usize::MAX), MAX_SUGGEST_FONT_SIZE);
    assert_eq!(clamp_suggest_line_height(0), MIN_SUGGEST_LINE_HEIGHT);
    assert_eq!(clamp_suggest_line_height(24), 24);
    assert_eq!(
        clamp_suggest_line_height(usize::MAX),
        MAX_SUGGEST_LINE_HEIGHT
    );
    assert_eq!(
        clamp_inline_suggest_min_show_delay_ms(0),
        MIN_INLINE_SUGGEST_MIN_SHOW_DELAY_MS
    );
    assert_eq!(clamp_inline_suggest_min_show_delay_ms(125), 125);
    assert_eq!(
        clamp_inline_suggest_min_show_delay_ms(usize::MAX),
        MAX_INLINE_SUGGEST_MIN_SHOW_DELAY_MS
    );
    assert_eq!(clamp_scm_input_line_count(0), MIN_SCM_INPUT_LINE_COUNT);
    assert_eq!(clamp_scm_input_line_count(10), 10);
    assert_eq!(
        clamp_scm_input_line_count(usize::MAX),
        MAX_SCM_INPUT_LINE_COUNT
    );
    assert_eq!(clamp_scm_input_font_size(0.0), MIN_SCM_INPUT_FONT_SIZE);
    assert_eq!(clamp_scm_input_font_size(14.0), 14.0);
    assert_eq!(
        clamp_scm_input_font_size(f32::NAN),
        DEFAULT_SCM_INPUT_FONT_SIZE
    );
    assert_eq!(clamp_scm_input_font_size(f32::MAX), MAX_SCM_INPUT_FONT_SIZE);
    assert_eq!(
        crate::git::clamp_diff_max_file_size_mb(0),
        crate::git::MIN_DIFF_MAX_FILE_SIZE_MB
    );
    assert_eq!(
        crate::git::clamp_diff_hide_unchanged_regions_minimum_line_count(0),
        crate::git::MIN_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT
    );
    assert_eq!(
        crate::git::clamp_diff_hide_unchanged_regions_minimum_line_count(9),
        9
    );
    assert_eq!(
        crate::git::clamp_diff_hide_unchanged_regions_minimum_line_count(usize::MAX),
        crate::git::MAX_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT
    );
    assert_eq!(
        crate::git::clamp_diff_hide_unchanged_regions_reveal_line_count(0),
        crate::git::MIN_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT
    );
    assert_eq!(
        crate::git::clamp_diff_hide_unchanged_regions_reveal_line_count(15),
        15
    );
    assert_eq!(
        crate::git::clamp_diff_hide_unchanged_regions_reveal_line_count(usize::MAX),
        crate::git::MAX_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT
    );
    assert_eq!(crate::git::clamp_diff_max_computation_time_ms(0), 0);
    assert_eq!(crate::git::clamp_diff_max_computation_time_ms(2_500), 2_500);
    assert_eq!(
        crate::git::clamp_diff_max_computation_time_ms(usize::MAX),
        crate::git::MAX_DIFF_MAX_COMPUTATION_TIME_MS
    );
    assert_eq!(crate::git::clamp_diff_max_file_size_mb(12), 12);
    assert_eq!(
        crate::git::clamp_diff_max_file_size_mb(usize::MAX),
        crate::git::MAX_DIFF_MAX_FILE_SIZE_MB
    );
    assert_eq!(clamp_window_zoom_level(-100.0), MIN_WINDOW_ZOOM_LEVEL);
    assert_eq!(clamp_window_zoom_level(1.5), 1.5);
    assert_eq!(clamp_window_zoom_level(f32::NAN), DEFAULT_WINDOW_ZOOM_LEVEL);
    assert_eq!(clamp_window_zoom_level(100.0), MAX_WINDOW_ZOOM_LEVEL);
    assert!((window_zoom_factor(1.0) - 1.2).abs() < f32::EPSILON);
}

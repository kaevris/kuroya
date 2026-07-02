use crate::{
    KuroyaApp,
    completion_preview::{CompletionInlinePreview, completion_inline_preview_for_item},
    editor_pane_support::{
        DiagnosticTagSpan, DocumentHighlightSpan, SemanticTokenSpan, diagnostic_line_maps,
        diagnostic_tag_spans_for_buffer, document_highlight_spans_for_buffer,
        semantic_token_spans_for_buffer,
    },
    editor_vim_key_events::{vim_effective_cursor_style, vim_search_highlight_ranges_for_buffer},
    file_runtime::file_path_open_buffer_or_known_openable,
    folding::indentation_folding_ranges,
    folding::{FoldedRange, visible_line_indices},
    large_file_mode::{
        LARGE_FILE_MODE_LINE_RENDER_CHAR_LIMIT, LARGE_FILE_MODE_MAX_BYTES,
        buffer_needs_bracket_scan_protection, buffer_needs_line_render_protection_cached,
        buffer_uses_large_file_mode,
    },
    session_state::editor_row_height,
    syntax_tree_cache::TreeSitterInjection,
    theme::theme_palette,
    transient_state::EditorImePreedit,
};
use eframe::egui::Color32;
use kuroya_core::settings::clamp_editor_font_size;
use kuroya_core::{
    BufferId, DiagnosticSeverity, EditorAccessibilitySupport, EditorBracketPairGuideMode,
    EditorColorDecoratorsActivatedOn, EditorCursorSmoothCaretAnimation, EditorCursorStyle,
    EditorDefaultColorDecorators, EditorExperimentalWhitespaceRendering, EditorFoldingStrategy,
    EditorHighlightActiveIndentation, EditorLightbulbMode, EditorLineDecorationsWidth,
    EditorLineNumbers, EditorMatchBrackets, EditorMinimapAutohide, EditorMinimapShowSlider,
    EditorMinimapSide, EditorMinimapSize, EditorMouseMiddleClickAction, EditorMouseStyle,
    EditorMultiCursorModifier, EditorRenderFinalNewline, EditorRenderLineHighlight,
    EditorRenderValidationDecorations, EditorRenderWhitespace, EditorShowFoldingControls,
    EditorWordWrap, GitBlameLine, GitChangeStage, GitLineChangeKind, LanguageId, LspCodeLens,
    LspFoldingRange, LspInlayHint, MergeConflict, ScmDiffDecorations,
    ScmDiffDecorationsGutterAction, ScmDiffDecorationsGutterPattern,
    ScmDiffDecorationsGutterVisibility, Selection, TextBuffer,
    buffer::{BracketPairGuide, CursorPosition},
    clamp_editor_cursor_height, clamp_editor_cursor_width, clamp_editor_folding_maximum_regions,
    clamp_editor_line_numbers_min_chars, clamp_editor_minimap_max_column,
    clamp_editor_minimap_scale, clamp_editor_minimap_section_header_font_size,
    clamp_editor_minimap_section_header_letter_spacing, clamp_editor_ruler_column,
    clamp_editor_sticky_scroll_max_line_count, clamp_editor_stop_rendering_line_after,
    clamp_editor_word_wrap_column, clamp_scm_diff_decorations_gutter_width,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::Range,
    path::{Path, PathBuf},
};

const DIFF_PATCH_OVERLAY_SCAN_MAX_LINES: usize = 20_000;

pub(crate) struct EditorPaneData {
    pub(crate) font_size: f32,
    pub(crate) row_height: f32,
    pub(crate) gutter_width: f32,
    pub(crate) char_width: f32,
    pub(crate) line_numbers: EditorLineNumbers,
    pub(crate) select_on_line_numbers: bool,
    pub(crate) render_whitespace: EditorRenderWhitespace,
    pub(crate) experimental_whitespace_rendering: EditorExperimentalWhitespaceRendering,
    pub(crate) render_final_newline: EditorRenderFinalNewline,
    pub(crate) render_control_characters: bool,
    pub(crate) unicode_highlight_ambiguous_characters: bool,
    pub(crate) unicode_highlight_invisible_characters: bool,
    pub(crate) unicode_highlight_non_basic_ascii: bool,
    pub(crate) unicode_highlight_allowed_characters: BTreeSet<char>,
    pub(crate) unicode_highlight_allowed_locales: BTreeSet<String>,
    pub(crate) render_line_highlight: EditorRenderLineHighlight,
    pub(crate) render_line_highlight_only_when_focus: bool,
    pub(crate) word_wrap: EditorWordWrap,
    pub(crate) word_wrap_column: usize,
    pub(crate) stop_rendering_line_after: i64,
    pub(crate) bracket_pair_colorization: bool,
    pub(crate) bracket_pair_colorization_independent_color_pool_per_bracket_type: bool,
    pub(crate) bracket_pair_guides: EditorBracketPairGuideMode,
    pub(crate) bracket_pair_guides_horizontal: EditorBracketPairGuideMode,
    pub(crate) highlight_active_bracket_pair: bool,
    pub(crate) match_brackets: EditorMatchBrackets,
    pub(crate) syntax_highlighting: bool,
    pub(crate) folding: bool,
    pub(crate) folding_highlight: bool,
    pub(crate) sticky_scroll: bool,
    pub(crate) sticky_scroll_max_line_count: usize,
    pub(crate) sticky_scroll_scroll_with_editor: bool,
    pub(crate) unfold_on_click_after_end_of_line: bool,
    pub(crate) show_folding_controls: EditorShowFoldingControls,
    pub(crate) contextmenu: bool,
    pub(crate) focused: bool,
    pub(crate) show_minimap: bool,
    pub(crate) minimap_side: EditorMinimapSide,
    pub(crate) minimap_autohide: EditorMinimapAutohide,
    pub(crate) minimap_size: EditorMinimapSize,
    pub(crate) minimap_show_slider: EditorMinimapShowSlider,
    pub(crate) minimap_scale: usize,
    pub(crate) minimap_render_characters: bool,
    pub(crate) minimap_max_column: usize,
    pub(crate) minimap_section_headers: BTreeMap<usize, String>,
    pub(crate) minimap_section_header_font_size: f32,
    pub(crate) minimap_section_header_letter_spacing: f32,
    pub(crate) multi_cursor_modifier: EditorMultiCursorModifier,
    pub(crate) double_click_selects_block: bool,
    pub(crate) drag_and_drop: bool,
    pub(crate) selection_clipboard: bool,
    pub(crate) mouse_middle_click_action: EditorMouseMiddleClickAction,
    pub(crate) mouse_style: EditorMouseStyle,
    pub(crate) glyph_margin: bool,
    pub(crate) lightbulb: EditorLightbulbMode,
    pub(crate) indent_guides: bool,
    pub(crate) highlight_active_indentation: EditorHighlightActiveIndentation,
    pub(crate) ruler_column: usize,
    pub(crate) overview_ruler_border: bool,
    pub(crate) overview_ruler_lanes: usize,
    pub(crate) hide_cursor_in_overview_ruler: bool,
    pub(crate) rounded_selection: bool,
    pub(crate) color_decorators: bool,
    pub(crate) color_decorators_activated_on: EditorColorDecoratorsActivatedOn,
    pub(crate) color_decorators_limit: usize,
    pub(crate) default_color_decorators: EditorDefaultColorDecorators,
    pub(crate) tab_width: usize,
    pub(crate) cursor_smooth_caret_animation: EditorCursorSmoothCaretAnimation,
    pub(crate) cursor_style: EditorCursorStyle,
    pub(crate) cursor_blinking: bool,
    pub(crate) cursor_width: f32,
    pub(crate) cursor_height: usize,
    pub(crate) ime_output_enabled: bool,
    pub(crate) accessibility_enabled: bool,
    pub(crate) accessibility_page_size: usize,
    pub(crate) aria_label: String,
    pub(crate) aria_required: bool,
    pub(crate) render_rich_screen_reader_content: bool,
    pub(crate) tab_index: i64,
    pub(crate) diff_lines: BTreeMap<usize, GitLineChangeKind>,
    pub(crate) cursor_positions: Vec<CursorPosition>,
    pub(crate) selections: Vec<Selection>,
    pub(crate) find_matches: Vec<Range<usize>>,
    pub(crate) selection_bg_fill: Color32,
    pub(crate) document_highlight_ranges: Vec<DocumentHighlightSpan>,
    pub(crate) semantic_token_ranges: Vec<SemanticTokenSpan>,
    pub(crate) syntax_injections: Vec<TreeSitterInjection>,
    pub(crate) diagnostics_by_line: HashMap<usize, DiagnosticSeverity>,
    pub(crate) diagnostic_messages: HashMap<usize, String>,
    pub(crate) diagnostic_tag_spans: Vec<DiagnosticTagSpan>,
    pub(crate) git_blame_editor_decoration_enabled: bool,
    pub(crate) git_blame_editor_decoration_disable_hover: bool,
    pub(crate) git_blame_editor_decoration_template: String,
    pub(crate) git_blame_lines: Vec<GitBlameLine>,
    pub(crate) active_path: Option<PathBuf>,
    pub(crate) folding_ranges: Vec<LspFoldingRange>,
    pub(crate) inlay_hints: Vec<LspInlayHint>,
    pub(crate) inlay_hints_font_family: String,
    pub(crate) inlay_hints_font_size: usize,
    pub(crate) inlay_hints_padding: bool,
    pub(crate) inlay_hints_maximum_length: usize,
    pub(crate) code_lenses: Vec<LspCodeLens>,
    pub(crate) code_lens_font_family: String,
    pub(crate) code_lens_font_size: usize,
    pub(crate) completion_preview: Option<CompletionInlinePreview>,
    pub(crate) placeholder: String,
    pub(crate) ime_preedit: Option<String>,
    pub(crate) folded_ranges: Vec<FoldedRange>,
    pub(crate) bracket_matches: Vec<(usize, usize)>,
    pub(crate) active_bracket_pair_matches: Vec<(usize, usize)>,
    pub(crate) bracket_pair_guide_ranges: Vec<BracketPairGuide>,
    pub(crate) merge_conflicts: Vec<MergeConflict>,
    pub(crate) visible_line_indices: Vec<usize>,
    pub(crate) visible_line_count: usize,
    pub(crate) diff_stage: Option<GitChangeStage>,
    pub(crate) diff_move_lines: BTreeSet<usize>,
    pub(crate) diff_render_gutter_menu: bool,
    pub(crate) diff_render_indicators: bool,
    pub(crate) diff_render_margin_revert_icon: bool,
    pub(crate) diff_render_overview_ruler: bool,
    pub(crate) diff_accessibility_verbose: bool,
    pub(crate) diff_experimental_show_empty_decorations: bool,
    pub(crate) show_scm_diff_gutter: bool,
    pub(crate) show_scm_diff_overview: bool,
    pub(crate) show_scm_diff_minimap: bool,
    pub(crate) scm_diff_decorations_gutter_action: ScmDiffDecorationsGutterAction,
    pub(crate) scm_diff_decorations_gutter_visibility: ScmDiffDecorationsGutterVisibility,
    pub(crate) scm_diff_decorations_gutter_width: usize,
    pub(crate) scm_diff_decorations_gutter_pattern: ScmDiffDecorationsGutterPattern,
    pub(crate) staged_hunk_actions: bool,
    pub(crate) source_control_unstaged_actions: bool,
    pub(crate) source_control_staged_actions: bool,
    pub(crate) source_control_discard_actions: bool,
    pub(crate) source_control_path_actions: bool,
    pub(crate) compare_saved_actions: bool,
    pub(crate) compare_file_actions: bool,
    pub(crate) compare_with_selected_actions: bool,
    pub(crate) diff_base_file_actions: bool,
    pub(crate) diff_source_file_actions: bool,
    pub(crate) diff_patch_actions: bool,
    pub(crate) diff_refresh_actions: bool,
    pub(crate) diff_swap_actions: bool,
}

impl KuroyaApp {
    pub(crate) fn prepare_editor_pane_data(
        &mut self,
        active_id: BufferId,
        buffer_index: usize,
        char_width: f32,
        focused: bool,
        accepts_text_input: bool,
    ) -> EditorPaneData {
        let char_width = editor_char_width(char_width);
        let (active_id, buffer_index) =
            resolve_editor_pane_buffer(&self.buffers, active_id, buffer_index)
                .unwrap_or((active_id, buffer_index));
        let (buffer_id, line_count, large_file_mode) = {
            let buffer = &self.buffers[buffer_index];
            (
                buffer.id(),
                buffer.len_lines(),
                buffer_uses_large_file_mode(buffer),
            )
        };
        let line_render_protection = buffer_needs_line_render_protection_cached(
            &mut self.line_render_protection_cache,
            &self.buffers[buffer_index],
        );
        let bracket_scan_protection =
            buffer_needs_bracket_scan_protection(&self.buffers[buffer_index]);
        self.clear_editor_pane_protected_caches(
            buffer_id,
            large_file_mode,
            bracket_scan_protection,
        );
        let folding = editor_folding_enabled(self.settings.folding, large_file_mode);
        let git_blame_editor_decoration_enabled = editor_git_blame_decoration_enabled(
            self.settings.git_blame_editor_decoration_enabled,
            large_file_mode,
        );
        let active_path_for_blame = self.buffers[buffer_index].path().cloned();
        if git_blame_editor_decoration_enabled && let Some(path) = active_path_for_blame {
            self.ensure_file_blame_cached(path);
        }

        let show_minimap = editor_minimap_enabled(self.settings.minimap, large_file_mode);
        let scm_diff_decorations = self.settings.scm_diff_decorations;
        let show_scm_diff_gutter =
            editor_scm_diff_gutter_enabled(scm_diff_decorations, large_file_mode);
        let show_scm_diff_overview =
            editor_scm_diff_overview_enabled(scm_diff_decorations, large_file_mode);
        let show_scm_diff_minimap =
            show_minimap && editor_scm_diff_minimap_enabled(scm_diff_decorations, large_file_mode);
        let diff_lines = if show_scm_diff_gutter || show_scm_diff_overview || show_scm_diff_minimap
        {
            self.diff_lines_for(active_id)
        } else {
            BTreeMap::new()
        };
        let merge_conflicts = self.merge_conflicts_for_buffer(active_id, buffer_index);
        let font_size = editor_font_size(self.settings.font_size);
        let row_height = editor_row_height(font_size, self.settings.line_height);
        let stop_rendering_line_after = editor_stop_rendering_line_after_for_mode(
            self.settings.stop_rendering_line_after,
            line_render_protection,
        );
        let gutter_width = editor_gutter_width(
            self.settings.line_numbers,
            self.settings.glyph_margin,
            folding,
            self.settings.line_numbers_min_chars,
            self.settings.line_decorations_width,
            char_width,
        );
        let (cursor_positions, selections) = {
            let buffer = &self.buffers[buffer_index];
            (buffer.cursor_positions(), buffer.selections().to_vec())
        };
        let find_matches = if editor_find_matches_enabled(self.buffer_find_open, large_file_mode) {
            self.find_matches_for_buffer_index(buffer_index)
        } else if editor_vim_search_matches_enabled(self.settings.vim_keybindings, large_file_mode)
        {
            let buffer = &self.buffers[buffer_index];
            vim_search_highlight_ranges_for_buffer(buffer)
        } else {
            Vec::new()
        };
        let buffer = &self.buffers[buffer_index];
        let document_highlight_ranges = if large_file_mode {
            Vec::new()
        } else {
            document_highlight_spans_for_buffer(
                buffer,
                self.document_highlights_path.as_deref(),
                &self.document_highlights,
            )
        };
        let match_brackets = editor_match_brackets_for_mode(
            self.settings.match_brackets,
            large_file_mode,
            bracket_scan_protection,
        );
        let bracket_matches = self
            .editor_bracket_overlay_cache
            .bracket_matches(buffer, match_brackets);
        let bracket_pair_guides = editor_bracket_pair_guides_for_mode(
            self.settings.bracket_pair_guides,
            large_file_mode,
            bracket_scan_protection,
        );
        let bracket_pair_guides_horizontal = editor_bracket_pair_guides_for_mode(
            self.settings.bracket_pair_guides_horizontal,
            large_file_mode,
            bracket_scan_protection,
        );
        let active_bracket_pair_matches = if active_bracket_pair_matches_required(
            bracket_pair_guides,
            bracket_pair_guides_horizontal,
            self.settings.highlight_active_bracket_pair,
        ) {
            if match_brackets == EditorMatchBrackets::Always {
                bracket_matches.clone()
            } else {
                self.editor_bracket_overlay_cache
                    .bracket_matches(buffer, EditorMatchBrackets::Always)
            }
        } else {
            Vec::new()
        };
        let bracket_pair_guide_ranges =
            if bracket_pair_guides.enabled() || bracket_pair_guides_horizontal.enabled() {
                self.editor_bracket_overlay_cache
                    .bracket_pair_guides(buffer)
            } else {
                Vec::new()
            };
        let render_validation_decorations = editor_validation_decorations_enabled(
            self.settings.render_validation_decorations,
            buffer.is_read_only(),
            large_file_mode,
        );
        let diagnostic_path = self.diagnostic_path_for(buffer);
        let diagnostics_for_path = self.diagnostics.for_path(&diagnostic_path);
        let (diagnostics_by_line, diagnostic_messages) = if render_validation_decorations {
            diagnostic_line_maps(diagnostics_for_path)
        } else {
            diagnostic_line_maps(&[])
        };
        let diagnostic_tag_spans = if large_file_mode {
            Vec::new()
        } else {
            diagnostic_tag_spans_for_buffer(
                buffer,
                diagnostics_for_path,
                self.settings.show_unused,
                self.settings.show_deprecated,
            )
        };
        let active_path = buffer.path().cloned();
        let git_blame_lines = if git_blame_editor_decoration_enabled {
            active_path
                .as_ref()
                .and_then(|path| self.source_control_blame_lines_for_path(path))
                .map(|lines| renderable_git_blame_lines(lines, line_count))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let sticky_scroll = self.settings.sticky_scroll && folding;
        let sticky_scroll_max_line_count = editor_sticky_scroll_max_line_count(
            sticky_scroll,
            self.settings.sticky_scroll_max_line_count,
        );
        let cached_folding_ranges = active_path
            .as_ref()
            .and_then(|path| self.folding_ranges.get(path));
        let folding_ranges = if folding {
            editor_folding_ranges_for_buffer(
                buffer,
                cached_folding_ranges,
                self.settings.folding_strategy,
                self.settings.folding_maximum_regions,
            )
        } else {
            Vec::new()
        };
        let inlay_hints = active_path
            .as_ref()
            .filter(|_| editor_inlay_hints_enabled(self.settings.inlay_hints, large_file_mode))
            .and_then(|path| self.inlay_hints.get(path))
            .map(|hints| renderable_inlay_hints(hints, line_count))
            .unwrap_or_default();
        let diff_source = self.diff_buffer_sources.get(&active_id);
        let diff_stage = diff_source.and_then(|source| source.hunk_stage);
        let mut code_lenses = active_path
            .as_ref()
            .filter(|_| editor_code_lens_enabled(self.settings.code_lens, large_file_mode))
            .and_then(|path| self.code_lenses.get(path))
            .map(|lenses| renderable_code_lenses(lenses, line_count))
            .unwrap_or_default();
        if editor_diff_code_lenses_enabled(self.settings.diff_code_lens, large_file_mode)
            && buffer.language() == LanguageId::Diff
        {
            code_lenses.extend(diff_code_lenses_for_patch_buffer(buffer, diff_stage));
            sort_code_lenses_by_position(&mut code_lenses);
        }
        let completion_preview = active_path
            .as_ref()
            .filter(|active_path| {
                focused
                    && self.completion_open
                    && self.settings.suggest_preview
                    && self
                        .completion_path
                        .as_ref()
                        .is_some_and(|completion_path| completion_path == *active_path)
            })
            .and_then(|_| {
                completion_inline_preview_for_item(
                    self.completion_line,
                    self.completion_items.get(self.completion_selected),
                    &self.completion_prefix,
                    self.settings.suggest_preview_mode,
                )
            });
        let semantic_token_ranges = if large_file_mode {
            Vec::new()
        } else {
            active_path
                .as_ref()
                .and_then(|path| self.semantic_tokens.get(path))
                .map(|tokens| semantic_token_spans_for_buffer(buffer, tokens))
                .unwrap_or_default()
        };
        let syntax_injections = if large_file_mode {
            Vec::new()
        } else {
            self.syntax_tree_cache
                .injections_for_buffer(buffer)
                .unwrap_or_default()
        };
        let folded_ranges = if folding {
            active_path
                .as_ref()
                .and_then(|path| self.folded_ranges.get(path))
                .map(|folded| folded_ranges_allowed_by_folding_ranges(folded, &folding_ranges))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let visible_line_count = line_count.max(1);
        let visible_line_indices = if folding && !folded_ranges.is_empty() {
            visible_line_indices(visible_line_count, &folded_ranges)
        } else {
            Vec::new()
        };
        let visible_line_count = if visible_line_indices.is_empty() {
            visible_line_count
        } else {
            visible_line_indices.len().max(1)
        };
        let source_control_path = active_path
            .as_deref()
            .or_else(|| diff_source.map(|source| source.path.as_path()));
        let staged_hunk_actions = diff_stage.is_none()
            && active_path
                .as_ref()
                .is_some_and(|path| self.git.has_stage_for(path, GitChangeStage::Staged));
        let source_control_unstaged_actions = source_control_path
            .as_ref()
            .is_some_and(|path| self.git.has_stage_for(path, GitChangeStage::Unstaged));
        let source_control_staged_actions = source_control_path
            .as_ref()
            .is_some_and(|path| self.git.has_stage_for(path, GitChangeStage::Staged));
        let source_control_discard_actions = source_control_path
            .as_ref()
            .is_some_and(|path| self.git.status_for(path).is_some());
        let source_control_path_actions = source_control_path.is_some();
        let mut path_exists_cache = HashMap::new();
        let mut path_openable_cache = HashMap::new();
        let indexed_files = self.index.files();
        let compare_saved_actions = active_path.as_ref().is_some_and(|path| {
            buffer.is_dirty() && path_exists_cached(&mut path_exists_cache, path, Path::exists)
        });
        let compare_file_actions = source_control_path.as_ref().is_some_and(|path| {
            editor_path_openable_cached(
                &mut path_openable_cache,
                &self.buffers,
                indexed_files,
                path,
                Path::exists,
            )
        });
        let compare_with_selected_actions = source_control_path.as_ref().is_some_and(|path| {
            editor_path_openable_cached(
                &mut path_openable_cache,
                &self.buffers,
                indexed_files,
                path,
                Path::exists,
            ) && self.explorer_compare_path.as_ref().is_some_and(|selected| {
                selected != path
                    && editor_path_openable_cached(
                        &mut path_openable_cache,
                        &self.buffers,
                        indexed_files,
                        selected,
                        Path::exists,
                    )
            })
        });
        let diff_base_file_actions = diff_source.is_some();
        let diff_source_file_actions = diff_source.is_some_and(|source| {
            editor_path_openable_cached(
                &mut path_openable_cache,
                &self.buffers,
                indexed_files,
                &source.path,
                Path::exists,
            )
        });
        let diff_patch_actions = buffer.language() == LanguageId::Diff
            && !large_file_mode
            && self.virtual_buffer_labels.contains_key(&active_id);
        let diff_move_lines = crate::editor_pane_data::diff_moved_patch_lines(
            buffer,
            self.settings.diff_experimental_show_moves,
            diff_patch_actions && !large_file_mode,
        );
        let diff_refresh_actions = diff_source.is_some();
        let diff_swap_actions = diff_source.is_some_and(|source| source.base_path.is_some());
        let word_wrap = editor_word_wrap_for_buffer(&self.settings, buffer.language());
        let palette = theme_palette(&self.settings.theme);
        let selection_bg_fill = palette.selection;
        let minimap_section_headers = if show_minimap {
            self.minimap_section_header_cache.headers_for(
                buffer,
                self.settings.minimap_show_region_section_headers,
                self.settings.minimap_show_mark_section_headers,
                &self.settings.minimap_mark_section_header_regex,
            )
        } else {
            BTreeMap::new()
        };

        EditorPaneData {
            font_size,
            row_height,
            gutter_width,
            char_width,
            line_numbers: self.settings.line_numbers,
            select_on_line_numbers: self.settings.select_on_line_numbers,
            render_whitespace: self.settings.render_whitespace,
            experimental_whitespace_rendering: self.settings.experimental_whitespace_rendering,
            render_final_newline: self.settings.render_final_newline,
            render_control_characters: self.settings.render_control_characters,
            unicode_highlight_ambiguous_characters: self
                .settings
                .unicode_highlight_ambiguous_characters,
            unicode_highlight_invisible_characters: self
                .settings
                .unicode_highlight_invisible_characters,
            unicode_highlight_non_basic_ascii: self
                .settings
                .unicode_highlight_non_basic_ascii
                .enabled(self.workspace_trusted),
            unicode_highlight_allowed_characters: unicode_highlight_allowed_characters(
                &self.settings.unicode_highlight_allowed_characters,
            ),
            unicode_highlight_allowed_locales: unicode_highlight_allowed_locales(
                &self.settings.unicode_highlight_allowed_locales,
            ),
            render_line_highlight: self.settings.render_line_highlight,
            render_line_highlight_only_when_focus: self
                .settings
                .render_line_highlight_only_when_focus,
            word_wrap,
            word_wrap_column: clamp_editor_word_wrap_column(self.settings.word_wrap_column),
            stop_rendering_line_after,
            bracket_pair_colorization: editor_bracket_pair_colorization_enabled(
                self.settings.bracket_pair_colorization,
                large_file_mode,
                bracket_scan_protection,
            ),
            bracket_pair_colorization_independent_color_pool_per_bracket_type: self
                .settings
                .bracket_pair_colorization_independent_color_pool_per_bracket_type,
            bracket_pair_guides,
            bracket_pair_guides_horizontal,
            highlight_active_bracket_pair: self.settings.highlight_active_bracket_pair,
            match_brackets,
            syntax_highlighting: !large_file_mode,
            folding,
            folding_highlight: self.settings.folding_highlight && folding,
            sticky_scroll: sticky_scroll_max_line_count > 0,
            sticky_scroll_max_line_count,
            sticky_scroll_scroll_with_editor: self.settings.sticky_scroll_scroll_with_editor,
            unfold_on_click_after_end_of_line: self.settings.unfold_on_click_after_end_of_line,
            show_folding_controls: self.settings.show_folding_controls,
            contextmenu: self.settings.contextmenu,
            focused,
            show_minimap,
            minimap_side: self.settings.minimap_side,
            minimap_autohide: self.settings.minimap_autohide,
            minimap_size: self.settings.minimap_size,
            minimap_show_slider: self.settings.minimap_show_slider,
            minimap_scale: clamp_editor_minimap_scale(self.settings.minimap_scale),
            minimap_render_characters: self.settings.minimap_render_characters,
            minimap_max_column: clamp_editor_minimap_max_column(self.settings.minimap_max_column),
            minimap_section_headers,
            minimap_section_header_font_size: clamp_editor_minimap_section_header_font_size(
                self.settings.minimap_section_header_font_size,
            ),
            minimap_section_header_letter_spacing:
                clamp_editor_minimap_section_header_letter_spacing(
                    self.settings.minimap_section_header_letter_spacing,
                ),
            multi_cursor_modifier: self.settings.multi_cursor_modifier,
            double_click_selects_block: self.settings.double_click_selects_block,
            drag_and_drop: self.settings.drag_and_drop,
            selection_clipboard: self.settings.selection_clipboard,
            mouse_middle_click_action: self.settings.mouse_middle_click_action,
            mouse_style: self.settings.mouse_style,
            glyph_margin: self.settings.glyph_margin,
            lightbulb: self.settings.lightbulb,
            indent_guides: self.settings.indent_guides,
            highlight_active_indentation: editor_highlight_active_indentation_for_mode(
                self.settings.highlight_active_indentation,
                self.settings.indent_guides,
                large_file_mode,
            ),
            ruler_column: clamp_editor_ruler_column(self.settings.ruler_column),
            overview_ruler_border: self.settings.overview_ruler_border,
            overview_ruler_lanes: self.settings.overview_ruler_lanes,
            hide_cursor_in_overview_ruler: self.settings.hide_cursor_in_overview_ruler,
            rounded_selection: self.settings.rounded_selection,
            color_decorators: self.settings.color_decorators,
            color_decorators_activated_on: self.settings.color_decorators_activated_on,
            color_decorators_limit: self.settings.color_decorators_limit,
            default_color_decorators: self.settings.default_color_decorators,
            tab_width: self.settings.tab_width.max(1),
            cursor_smooth_caret_animation: self.settings.cursor_smooth_caret_animation,
            cursor_style: vim_effective_cursor_style(
                self.settings.cursor_style,
                self.settings.vim_keybindings,
                self.editor_vim_mode,
                self.editor_vim_pending_key,
            ),
            cursor_blinking: self.settings.cursor_blinking,
            cursor_width: clamp_editor_cursor_width(self.settings.cursor_width),
            cursor_height: clamp_editor_cursor_height(self.settings.cursor_height),
            ime_output_enabled: accepts_text_input,
            accessibility_enabled: editor_accessibility_enabled(
                self.settings.accessibility_support,
            ),
            accessibility_page_size: self.settings.accessibility_page_size,
            aria_label: self.settings.aria_label.clone(),
            aria_required: self.settings.aria_required,
            render_rich_screen_reader_content: self.settings.render_rich_screen_reader_content,
            tab_index: self.settings.tab_index,
            diff_lines,
            cursor_positions,
            selections,
            find_matches,
            selection_bg_fill,
            document_highlight_ranges,
            semantic_token_ranges,
            syntax_injections,
            diagnostics_by_line,
            diagnostic_messages,
            diagnostic_tag_spans,
            git_blame_editor_decoration_enabled,
            git_blame_editor_decoration_disable_hover: self
                .settings
                .git_blame_editor_decoration_disable_hover,
            git_blame_editor_decoration_template: self
                .settings
                .git_blame_editor_decoration_template
                .clone(),
            git_blame_lines,
            active_path,
            folding_ranges,
            inlay_hints,
            inlay_hints_font_family: self.settings.inlay_hints_font_family.clone(),
            inlay_hints_font_size: self.settings.inlay_hints_font_size,
            inlay_hints_padding: self.settings.inlay_hints_padding,
            inlay_hints_maximum_length: self.settings.inlay_hints_maximum_length,
            code_lenses,
            code_lens_font_family: self.settings.code_lens_font_family.clone(),
            code_lens_font_size: self.settings.code_lens_font_size,
            completion_preview,
            placeholder: self.settings.placeholder.clone(),
            ime_preedit: editor_ime_preedit_for_buffer(
                self.ime_preedit.as_ref(),
                active_id,
                accepts_text_input,
            ),
            folded_ranges,
            bracket_matches,
            active_bracket_pair_matches,
            bracket_pair_guide_ranges,
            merge_conflicts,
            visible_line_indices,
            visible_line_count,
            diff_stage,
            diff_move_lines,
            diff_render_gutter_menu: self.settings.diff_render_gutter_menu,
            diff_render_indicators: self.settings.diff_render_indicators,
            diff_render_margin_revert_icon: self.settings.diff_render_margin_revert_icon,
            diff_render_overview_ruler: editor_diff_overview_ruler_enabled(
                self.settings.diff_render_overview_ruler,
                large_file_mode,
            ),
            diff_accessibility_verbose: self.settings.diff_accessibility_verbose,
            diff_experimental_show_empty_decorations: self
                .settings
                .diff_experimental_show_empty_decorations,
            show_scm_diff_gutter,
            show_scm_diff_overview,
            show_scm_diff_minimap,
            scm_diff_decorations_gutter_action: self.settings.scm_diff_decorations_gutter_action,
            scm_diff_decorations_gutter_visibility: self
                .settings
                .scm_diff_decorations_gutter_visibility,
            scm_diff_decorations_gutter_width: clamp_scm_diff_decorations_gutter_width(
                self.settings.scm_diff_decorations_gutter_width,
            ),
            scm_diff_decorations_gutter_pattern: self.settings.scm_diff_decorations_gutter_pattern,
            staged_hunk_actions,
            source_control_unstaged_actions,
            source_control_staged_actions,
            source_control_discard_actions,
            source_control_path_actions,
            compare_saved_actions,
            compare_file_actions,
            compare_with_selected_actions,
            diff_base_file_actions,
            diff_source_file_actions,
            diff_patch_actions,
            diff_refresh_actions,
            diff_swap_actions,
        }
    }

    fn clear_editor_pane_protected_caches(
        &mut self,
        buffer_id: BufferId,
        performance_mode: bool,
        bracket_scan_protection: bool,
    ) {
        if performance_mode {
            self.buffer_find_cache.clear_for_buffer(buffer_id);
        }
        if performance_mode || bracket_scan_protection {
            self.editor_bracket_overlay_cache
                .clear_for_buffer(buffer_id);
        }
    }
}

impl EditorPaneData {
    pub(crate) fn visible_row_for_line_idx(&self, line_idx: usize) -> usize {
        if self.folding && !self.visible_line_indices.is_empty() {
            crate::folding::visible_row_for_line(&self.visible_line_indices, line_idx)
        } else {
            line_idx.min(self.visible_line_count.saturating_sub(1))
        }
    }
}

pub(crate) fn editor_ime_preedit_for_buffer(
    preedit: Option<&EditorImePreedit>,
    active_id: BufferId,
    focused: bool,
) -> Option<String> {
    focused
        .then_some(preedit?)
        .filter(|preedit| preedit.buffer_id == active_id)
        .map(|preedit| preedit.text.clone())
}

fn resolve_editor_pane_buffer(
    buffers: &[TextBuffer],
    active_id: BufferId,
    buffer_index: usize,
) -> Option<(BufferId, usize)> {
    if buffers
        .get(buffer_index)
        .is_some_and(|buffer| buffer.id() == active_id)
    {
        return Some((active_id, buffer_index));
    }

    buffers
        .iter()
        .position(|buffer| buffer.id() == active_id)
        .map(|index| (active_id, index))
        .or_else(|| {
            buffers
                .get(buffer_index)
                .map(|buffer| (buffer.id(), buffer_index))
        })
}

fn editor_char_width(char_width: f32) -> f32 {
    if char_width.is_finite() && char_width > 0.0 {
        char_width
    } else {
        8.0
    }
}

fn editor_font_size(font_size: f32) -> f32 {
    clamp_editor_font_size(font_size, 13.0)
}

fn path_exists_cached(
    cache: &mut HashMap<PathBuf, bool>,
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    if let Some(exists) = cache.get(path) {
        return *exists;
    }

    let exists = path_exists(path);
    cache.insert(path.to_path_buf(), exists);
    exists
}

fn editor_path_openable_cached(
    cache: &mut HashMap<PathBuf, bool>,
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> bool {
    path_exists_cached(cache, path, |path| {
        file_path_open_buffer_or_known_openable(buffers, indexed_files, path, path_exists)
    })
}

fn editor_folding_enabled(setting_enabled: bool, large_file_mode: bool) -> bool {
    setting_enabled && !large_file_mode
}

fn editor_minimap_enabled(setting_enabled: bool, large_file_mode: bool) -> bool {
    setting_enabled && !large_file_mode
}

#[cfg(test)]
fn editor_sticky_scroll_enabled(setting_enabled: bool, max_line_count: usize) -> bool {
    editor_sticky_scroll_max_line_count(setting_enabled, max_line_count) > 0
}

fn editor_sticky_scroll_max_line_count(setting_enabled: bool, max_line_count: usize) -> usize {
    if setting_enabled && max_line_count > 0 {
        clamp_editor_sticky_scroll_max_line_count(max_line_count)
    } else {
        0
    }
}

fn editor_find_matches_enabled(find_open: bool, large_file_mode: bool) -> bool {
    find_open && !large_file_mode
}

fn editor_vim_search_matches_enabled(vim_keybindings: bool, large_file_mode: bool) -> bool {
    vim_keybindings && !large_file_mode
}

fn editor_code_lens_enabled(setting_enabled: bool, large_file_mode: bool) -> bool {
    setting_enabled && !large_file_mode
}

fn editor_diff_code_lenses_enabled(setting_enabled: bool, large_file_mode: bool) -> bool {
    setting_enabled && !large_file_mode
}

fn editor_diff_overview_ruler_enabled(setting_enabled: bool, large_file_mode: bool) -> bool {
    setting_enabled && !large_file_mode
}

fn editor_scm_diff_gutter_enabled(setting: ScmDiffDecorations, large_file_mode: bool) -> bool {
    setting.show_gutter() && !large_file_mode
}

fn editor_scm_diff_overview_enabled(setting: ScmDiffDecorations, large_file_mode: bool) -> bool {
    setting.show_overview() && !large_file_mode
}

fn editor_scm_diff_minimap_enabled(setting: ScmDiffDecorations, large_file_mode: bool) -> bool {
    setting.show_minimap() && !large_file_mode
}

fn editor_inlay_hints_enabled(setting_enabled: bool, large_file_mode: bool) -> bool {
    setting_enabled && !large_file_mode
}

fn editor_git_blame_decoration_enabled(setting_enabled: bool, large_file_mode: bool) -> bool {
    setting_enabled && !large_file_mode
}

fn editor_highlight_active_indentation_for_mode(
    setting: EditorHighlightActiveIndentation,
    indent_guides: bool,
    large_file_mode: bool,
) -> EditorHighlightActiveIndentation {
    if !indent_guides || large_file_mode {
        EditorHighlightActiveIndentation::Off
    } else {
        setting
    }
}

fn editor_match_brackets_for_mode(
    setting: EditorMatchBrackets,
    large_file_mode: bool,
    protect_long_lines: bool,
) -> EditorMatchBrackets {
    if large_file_mode || protect_long_lines {
        EditorMatchBrackets::Never
    } else {
        setting
    }
}

fn editor_bracket_pair_guides_for_mode(
    setting: EditorBracketPairGuideMode,
    large_file_mode: bool,
    protect_long_lines: bool,
) -> EditorBracketPairGuideMode {
    if large_file_mode || protect_long_lines {
        EditorBracketPairGuideMode::Off
    } else {
        setting
    }
}

fn active_bracket_pair_matches_required(
    vertical_guides: EditorBracketPairGuideMode,
    horizontal_guides: EditorBracketPairGuideMode,
    highlight_active_bracket_pair: bool,
) -> bool {
    if !vertical_guides.enabled() && !horizontal_guides.enabled() {
        return false;
    }
    highlight_active_bracket_pair
        || vertical_guides.active_only()
        || horizontal_guides.active_only()
}

fn editor_bracket_pair_colorization_enabled(
    setting_enabled: bool,
    large_file_mode: bool,
    protect_long_lines: bool,
) -> bool {
    setting_enabled && !large_file_mode && !protect_long_lines
}

fn editor_stop_rendering_line_after_for_mode(setting: i64, protect_long_lines: bool) -> i64 {
    let setting = clamp_editor_stop_rendering_line_after(setting);
    if !protect_long_lines {
        return setting;
    }

    let large_file_limit =
        i64::try_from(LARGE_FILE_MODE_LINE_RENDER_CHAR_LIMIT).unwrap_or(i64::MAX);
    if setting < 0 {
        large_file_limit
    } else {
        setting.min(large_file_limit)
    }
}

fn editor_word_wrap_for_buffer(
    settings: &kuroya_core::EditorSettings,
    language: LanguageId,
) -> EditorWordWrap {
    let editor_word_wrap = settings
        .word_wrap_override2
        .resolve(settings.word_wrap_override1.resolve(settings.word_wrap));
    if language == LanguageId::Diff {
        settings.diff_word_wrap.resolve(editor_word_wrap)
    } else {
        editor_word_wrap
    }
}

fn unicode_highlight_allowed_characters(values: &BTreeMap<String, bool>) -> BTreeSet<char> {
    values
        .iter()
        .filter(|(_, allowed)| **allowed)
        .filter_map(|(key, _)| {
            let mut chars = key.chars();
            let ch = chars.next()?;
            chars.next().is_none().then_some(ch)
        })
        .collect()
}

fn unicode_highlight_allowed_locales(values: &BTreeMap<String, bool>) -> BTreeSet<String> {
    values
        .iter()
        .filter(|(_, allowed)| **allowed)
        .filter_map(|(key, _)| unicode_highlight_locale_language(key))
        .collect()
}

fn unicode_highlight_locale_language(locale: &str) -> Option<String> {
    let normalized = locale.trim().replace('_', "-").to_ascii_lowercase();
    let language = normalized
        .split(['-', '.'])
        .next()
        .unwrap_or_default()
        .trim();
    if language.is_empty() || language.starts_with('_') {
        None
    } else {
        Some(language.to_owned())
    }
}

fn editor_validation_decorations_enabled(
    setting: EditorRenderValidationDecorations,
    read_only: bool,
    large_file_mode: bool,
) -> bool {
    !large_file_mode && setting.visible(read_only)
}

pub(crate) fn editor_gutter_width(
    line_numbers: EditorLineNumbers,
    glyph_margin: bool,
    folding: bool,
    line_numbers_min_chars: usize,
    line_decorations_width: EditorLineDecorationsWidth,
    char_width: f32,
) -> f32 {
    let line_number_width = if line_numbers == EditorLineNumbers::Off {
        0.0
    } else {
        line_number_width_for_min_chars(line_numbers_min_chars, char_width)
    };
    let decorations_width = line_decorations_width.pixels(char_width);
    let glyph_width = if glyph_margin { 12.0 } else { 0.0 };
    let fold_width = if folding { 16.0 } else { 0.0 };
    (2.0_f32 + line_number_width + decorations_width + glyph_width + fold_width).max(24.0)
}

pub(crate) fn line_number_width_for_min_chars(min_chars: usize, char_width: f32) -> f32 {
    let char_width = if char_width.is_finite() && char_width > 0.0 {
        char_width
    } else {
        8.0
    };
    clamp_editor_line_numbers_min_chars(min_chars) as f32 * char_width + 4.0
}

pub(crate) fn diff_code_lenses_for_patch_buffer(
    buffer: &TextBuffer,
    stage: Option<GitChangeStage>,
) -> Vec<LspCodeLens> {
    if !diff_patch_overlay_scan_allowed(buffer) {
        return Vec::new();
    }

    let title = diff_code_lens_title(stage);
    (0..buffer.len_lines())
        .filter(|line| buffer.line_starts_with(*line, "@@"))
        .map(|line| LspCodeLens {
            line: line + 1,
            column: 1,
            title: title.clone(),
            command: None,
            command_arguments: None,
            resolve_payload: None,
        })
        .collect()
}

fn diff_code_lens_title(stage: Option<GitChangeStage>) -> String {
    let mut title = "Prev | Next | Copy Hunk | A11y Diff".to_owned();
    match stage {
        Some(GitChangeStage::Unstaged) => title.push_str(" | Stage | Discard"),
        Some(GitChangeStage::Staged) => title.push_str(" | Unstage"),
        None => {}
    }
    title
}

fn sort_code_lenses_by_position(lenses: &mut [LspCodeLens]) {
    lenses.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
            .then(left.title.cmp(&right.title))
            .then(left.command.cmp(&right.command))
    });
}

fn renderable_inlay_hints(hints: &[LspInlayHint], line_count: usize) -> Vec<LspInlayHint> {
    let line_count = line_count.max(1);
    let mut renderable = Vec::with_capacity(hints.len().min(line_count));
    for hint in hints {
        if (1..=line_count).contains(&hint.line) && hint.column > 0 {
            renderable.push(hint.clone());
        }
    }
    renderable
}

fn renderable_code_lenses(lenses: &[LspCodeLens], line_count: usize) -> Vec<LspCodeLens> {
    let line_count = line_count.max(1);
    let mut renderable = Vec::with_capacity(lenses.len().min(line_count));
    for lens in lenses {
        if (1..=line_count).contains(&lens.line) && lens.column > 0 {
            renderable.push(lens.clone());
        }
    }
    renderable
}

fn renderable_git_blame_lines(lines: &[GitBlameLine], line_count: usize) -> Vec<GitBlameLine> {
    let line_count = line_count.max(1);
    let mut renderable = Vec::with_capacity(lines.len().min(line_count));
    for line in lines {
        if (1..=line_count).contains(&line.line_number) {
            renderable.push(line.clone());
        }
    }
    renderable
}

#[cfg(test)]
pub(crate) fn capped_folding_ranges(
    ranges: &[LspFoldingRange],
    maximum_regions: usize,
) -> Vec<LspFoldingRange> {
    let mut ranges = ranges.to_vec();
    ranges.truncate(clamp_editor_folding_maximum_regions(maximum_regions));
    ranges
}

pub(crate) fn editor_folding_ranges_for_buffer(
    buffer: &TextBuffer,
    cached_ranges: Option<&Vec<LspFoldingRange>>,
    strategy: EditorFoldingStrategy,
    maximum_regions: usize,
) -> Vec<LspFoldingRange> {
    match strategy {
        EditorFoldingStrategy::Auto => {
            let ranges = cached_ranges.map(Vec::as_slice).unwrap_or(&[]);
            renderable_folding_ranges(ranges, buffer.len_lines(), maximum_regions)
        }
        EditorFoldingStrategy::Indentation => {
            let ranges = indentation_folding_ranges(buffer);
            renderable_folding_ranges(&ranges, buffer.len_lines(), maximum_regions)
        }
    }
}

fn renderable_folding_ranges(
    ranges: &[LspFoldingRange],
    line_count: usize,
    maximum_regions: usize,
) -> Vec<LspFoldingRange> {
    let line_count = line_count.max(1);
    let maximum_regions = clamp_editor_folding_maximum_regions(maximum_regions);
    if maximum_regions == 0 {
        return Vec::new();
    }
    let mut renderable = Vec::with_capacity(maximum_regions.min(ranges.len()));
    for range in ranges {
        if range.start_line > 0 && range.end_line > range.start_line && range.end_line <= line_count
        {
            renderable.push(range.clone());
            if renderable.len() == maximum_regions {
                break;
            }
        }
    }
    renderable
}

pub(crate) fn editor_accessibility_enabled(mode: EditorAccessibilitySupport) -> bool {
    !matches!(mode, EditorAccessibilitySupport::Off)
}

pub(crate) fn folded_ranges_allowed_by_folding_ranges(
    folded_ranges: &[FoldedRange],
    folding_ranges: &[LspFoldingRange],
) -> Vec<FoldedRange> {
    if folded_ranges.is_empty() || folding_ranges.is_empty() {
        return Vec::new();
    }

    let mut allowed = Vec::with_capacity(folded_ranges.len().min(folding_ranges.len()));
    if folding_ranges_sorted_by_span(folding_ranges) && folded_ranges_sorted_by_span(folded_ranges)
    {
        let mut folding_index = 0usize;
        for folded in folded_ranges {
            let target = (folded.start_line, folded.end_line);
            while let Some(range) = folding_ranges.get(folding_index) {
                match (range.start_line, range.end_line).cmp(&target) {
                    std::cmp::Ordering::Less => folding_index += 1,
                    std::cmp::Ordering::Equal => {
                        allowed.push(*folded);
                        break;
                    }
                    std::cmp::Ordering::Greater => break,
                }
            }
        }
        return allowed;
    }

    for range in folded_ranges.iter().copied() {
        if folding_ranges_contains_span(folding_ranges, range) {
            allowed.push(range);
        }
    }
    allowed
}

fn folding_ranges_sorted_by_span(folding_ranges: &[LspFoldingRange]) -> bool {
    folding_ranges.windows(2).all(|pair| {
        let left = &pair[0];
        let right = &pair[1];
        (left.start_line, left.end_line) <= (right.start_line, right.end_line)
    })
}

fn folded_ranges_sorted_by_span(folded_ranges: &[FoldedRange]) -> bool {
    folded_ranges.windows(2).all(|pair| {
        let left = pair[0];
        let right = pair[1];
        (left.start_line, left.end_line) <= (right.start_line, right.end_line)
    })
}

fn folding_ranges_contains_span(folding_ranges: &[LspFoldingRange], folded: FoldedRange) -> bool {
    folding_ranges
        .iter()
        .any(|range| range.start_line == folded.start_line && range.end_line == folded.end_line)
}

pub(crate) fn diff_moved_patch_lines(
    buffer: &TextBuffer,
    show_moves: bool,
    diff_patch_actions: bool,
) -> BTreeSet<usize> {
    if !show_moves || !diff_patch_actions || !diff_patch_overlay_scan_allowed(buffer) {
        return BTreeSet::new();
    }

    let mut deleted: HashMap<String, Vec<usize>> = HashMap::new();
    let mut added: HashMap<String, Vec<usize>> = HashMap::new();
    for line_idx in 0..buffer.len_lines() {
        let Some(line) = buffer.line(line_idx) else {
            continue;
        };
        let line = line.trim_end_matches(['\r', '\n']);
        let Some((prefix, content)) = diff_patch_changed_line(line) else {
            continue;
        };
        let content = content.trim();
        if content.is_empty() {
            continue;
        }

        let lines = if prefix == '-' {
            &mut deleted
        } else {
            &mut added
        };
        lines
            .entry(content.to_owned())
            .or_default()
            .push(line_idx + 1);
    }

    let mut moved = BTreeSet::new();
    for (content, deleted_lines) in deleted {
        let Some(added_lines) = added.get(&content) else {
            continue;
        };
        moved.extend(deleted_lines);
        moved.extend(added_lines.iter().copied());
    }
    moved
}

fn diff_patch_overlay_scan_allowed(buffer: &TextBuffer) -> bool {
    buffer.len_lines() <= DIFF_PATCH_OVERLAY_SCAN_MAX_LINES
        && buffer.len_bytes() <= LARGE_FILE_MODE_MAX_BYTES
}

fn diff_patch_changed_line(line: &str) -> Option<(char, &str)> {
    if line.starts_with("---") || line.starts_with("+++") {
        return None;
    }

    let mut chars = line.chars();
    let prefix = chars.next()?;
    matches!(prefix, '-' | '+').then(|| (prefix, chars.as_str()))
}

#[cfg(test)]
mod tests;

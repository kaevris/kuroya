use super::{
    MAX_MINIMAP_LINE_LENGTH_CACHES, MAX_MINIMAP_LINE_SAMPLES, MinimapLineLengthCache,
    MinimapMarkerLines, MinimapSectionHeaderCache, minimap_background_color,
    minimap_background_fill, minimap_content_line_span, minimap_cursor_line_color,
    minimap_default_line_color, minimap_find_match_line_color, minimap_first_visible_line,
    minimap_jump_line_from_y, minimap_line_change_for_sample, minimap_line_len, minimap_line_width,
    minimap_marker_line_bounds, minimap_render_size, minimap_row_count, minimap_sample_count,
    minimap_section_header_char_advance, minimap_section_header_display_text,
    minimap_section_header_for_sample, minimap_section_header_scan_allowed, minimap_slider_color,
    minimap_slider_visible, minimap_stroke_width, minimap_viewport_rect,
    minimap_visible_line_count,
};
use crate::large_file_mode::{LARGE_FILE_MODE_MAX_BYTES, LARGE_FILE_MODE_MAX_LINES};
use egui::{Color32, Rect, pos2, vec2};
use kuroya_core::{
    EditorMinimapShowSlider, GitLineChangeKind, MAX_EDITOR_MINIMAP_MAX_COLUMN, TextBuffer,
};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn minimap_slider_visibility_follows_setting_and_interaction() {
    assert!(minimap_slider_visible(
        EditorMinimapShowSlider::Always,
        false,
        false
    ));
    assert!(!minimap_slider_visible(
        EditorMinimapShowSlider::Mouseover,
        false,
        false
    ));
    assert!(minimap_slider_visible(
        EditorMinimapShowSlider::Mouseover,
        true,
        false
    ));
    assert!(minimap_slider_visible(
        EditorMinimapShowSlider::Mouseover,
        false,
        true
    ));
}

#[test]
fn minimap_chrome_colors_follow_theme_visuals() {
    let mut visuals = egui::Visuals::light();
    visuals.code_bg_color = Color32::from_rgb(224, 231, 240);
    visuals.widgets.active.bg_fill = Color32::from_rgb(208, 215, 224);
    visuals.widgets.inactive.bg_stroke.color = Color32::from_rgb(188, 198, 210);
    visuals.warn_fg_color = Color32::from_rgb(161, 104, 24);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(218, 225, 233);

    assert_eq!(
        minimap_background_color(&visuals, false),
        visuals.code_bg_color
    );
    assert_eq!(
        minimap_cursor_line_color(&visuals),
        visuals.widgets.active.bg_fill
    );
    assert_eq!(
        minimap_default_line_color(&visuals),
        visuals.widgets.inactive.bg_stroke.color
    );
    assert_eq!(
        minimap_find_match_line_color(&visuals),
        visuals.warn_fg_color
    );
    assert_eq!(
        minimap_slider_color(&visuals, true, false),
        Color32::from_rgba_unmultiplied(218, 225, 233, 180)
    );
}

#[test]
fn minimap_background_fill_clears_base_slab_over_background_images() {
    let bg = Color32::from_rgb(24, 26, 31);

    assert_eq!(minimap_background_fill(bg, false), bg);
    assert_eq!(minimap_background_fill(bg, true), Color32::TRANSPARENT);
    assert_eq!(
        minimap_background_fill(Color32::TRANSPARENT, true),
        Color32::TRANSPARENT
    );

    let mut visuals = egui::Visuals::dark();
    visuals.code_bg_color = bg;
    assert_eq!(minimap_background_color(&visuals, false), bg);
    assert_eq!(
        minimap_background_color(&visuals, true),
        Color32::TRANSPARENT
    );
}

#[test]
fn minimap_render_size_rejects_hidden_or_invalid_geometry() {
    assert_eq!(minimap_render_size(0.0, 120.0), None);
    assert_eq!(minimap_render_size(80.0, 0.0), None);
    assert_eq!(minimap_render_size(f32::NAN, 120.0), None);
    assert_eq!(minimap_render_size(80.0, f32::INFINITY), None);
    assert_eq!(minimap_render_size(80.0, 120.0), Some(vec2(80.0, 120.0)));
}

#[test]
fn minimap_content_line_span_keeps_strokes_inside_cramped_rects() {
    let normal = Rect::from_min_size(pos2(10.0, 0.0), vec2(80.0, 120.0));
    let (x, width) = minimap_content_line_span(normal).unwrap();
    assert_eq!(x, 16.0);
    assert_eq!(width, 68.0);
    assert!(x >= normal.left());
    assert!(x + width <= normal.right());

    let cramped = Rect::from_min_size(pos2(10.0, 0.0), vec2(8.0, 120.0));
    let (x, width) = minimap_content_line_span(cramped).unwrap();
    assert_eq!(x, 12.0);
    assert_eq!(width, 4.0);
    assert!(x >= cramped.left());
    assert!(x + width <= cramped.right());

    let invalid = Rect::from_min_size(pos2(10.0, 0.0), vec2(f32::NAN, 120.0));
    assert_eq!(minimap_content_line_span(invalid), None);
}

#[test]
fn minimap_line_width_uses_character_mode_or_block_mode() {
    assert_eq!(minimap_line_width(5, 10, 80.0, true), 40.0);
    assert_eq!(minimap_line_width(20, 10, 80.0, true), 80.0);
    assert_eq!(minimap_line_width(5, 10, 80.0, false), 80.0);
    assert_eq!(minimap_line_width(0, 10, 80.0, false), 1.0);
    assert_eq!(minimap_line_width(0, 10, 0.5, false), 0.5);
    assert_eq!(minimap_line_width(5, 10, f32::NAN, true), 0.0);
    assert_eq!(minimap_line_width(5, 10, f32::INFINITY, false), 0.0);
    assert_eq!(minimap_line_width(5, 10, -80.0, true), 0.0);
}

#[test]
fn minimap_marker_line_lookup_bounds_sparse_marker_sets() {
    let marker_lines = HashSet::from([4, 9]);
    let bounded_lookup = MinimapMarkerLines::new(&marker_lines, 8);
    let direct_lookup = MinimapMarkerLines::new(&marker_lines, 1);

    assert_eq!(minimap_marker_line_bounds(&marker_lines), Some((4, 9)));
    assert!(!bounded_lookup.contains(3));
    assert!(bounded_lookup.contains(4));
    assert!(!bounded_lookup.contains(5));
    assert!(bounded_lookup.contains(9));
    assert!(!bounded_lookup.contains(10));
    assert_eq!(direct_lookup.contains(4), bounded_lookup.contains(4));
    assert_eq!(direct_lookup.contains(5), bounded_lookup.contains(5));
}

#[test]
fn minimap_line_len_uses_presence_only_for_block_mode() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\n\n\u{03bb}ambda\n".to_owned());

    assert_eq!(minimap_line_len(&buffer, 0, 3, true), 3);
    assert_eq!(minimap_line_len(&buffer, 0, 3, false), 1);
    assert_eq!(minimap_line_len(&buffer, 1, 3, false), 0);
    assert_eq!(minimap_line_len(&buffer, 2, 3, false), 1);
}

#[test]
fn minimap_line_len_caps_direct_character_scans() {
    let buffer = TextBuffer::from_text(1, None, "x".repeat(MAX_EDITOR_MINIMAP_MAX_COLUMN + 10));

    assert_eq!(
        minimap_line_len(&buffer, 0, usize::MAX, true),
        MAX_EDITOR_MINIMAP_MAX_COLUMN
    );
}

#[test]
fn minimap_stroke_width_clamps_scale() {
    assert_eq!(minimap_stroke_width(0), 1.0);
    assert_eq!(minimap_stroke_width(2), 2.0);
    assert_eq!(minimap_stroke_width(99), 3.0);
}

#[test]
fn minimap_sample_count_tracks_scaled_stroke_width() {
    assert_eq!(minimap_sample_count(10_000, 240.0, 1.0), 240);
    assert_eq!(minimap_sample_count(10_000, 240.0, 2.0), 120);
    assert_eq!(minimap_sample_count(80, 240.0, 3.0), 80);
    assert_eq!(minimap_sample_count(0, f32::NAN, f32::NAN), 1);
    assert_eq!(
        minimap_sample_count(usize::MAX, f32::MAX, 1.0),
        MAX_MINIMAP_LINE_SAMPLES
    );
}

#[test]
fn minimap_visible_line_count_rejects_invalid_geometry_and_clamps_to_file() {
    assert_eq!(minimap_visible_line_count(200.0, 20.0, 1_000), 10);
    assert_eq!(minimap_visible_line_count(10_000.0, 1.0, 40), 40);
    assert_eq!(minimap_visible_line_count(f32::NAN, 20.0, 40), 1);
    assert_eq!(minimap_visible_line_count(200.0, f32::INFINITY, 40), 1);
    assert_eq!(minimap_visible_line_count(200.0, 0.0, 40), 1);
}

#[test]
fn minimap_visible_line_count_bounds_extreme_geometry_before_integer_cast() {
    assert_eq!(minimap_visible_line_count(f32::MAX, 0.000_001, 40), 40);
    assert_eq!(
        minimap_visible_line_count(f32::MAX, f32::MIN_POSITIVE, usize::MAX),
        usize::MAX
    );
}

#[test]
fn minimap_first_visible_line_rejects_invalid_geometry_and_stale_offsets() {
    assert_eq!(minimap_first_visible_line(200.0, 20.0, 40), 10);
    assert_eq!(minimap_first_visible_line(-200.0, 20.0, 40), 0);
    assert_eq!(minimap_first_visible_line(20_000.0, 20.0, 40), 39);
    assert_eq!(minimap_first_visible_line(f32::NAN, 20.0, 40), 0);
    assert_eq!(minimap_first_visible_line(200.0, f32::NAN, 40), 0);
}

#[test]
fn minimap_first_visible_line_bounds_extreme_offsets_before_integer_cast() {
    assert_eq!(minimap_first_visible_line(f32::MAX, 0.000_001, 40), 39);
    assert_eq!(
        minimap_first_visible_line(f32::MAX, f32::MIN_POSITIVE, usize::MAX),
        usize::MAX - 1
    );
}

#[test]
fn minimap_row_count_prefers_fold_filtered_rows_and_clamps_to_buffer() {
    let indices = vec![0usize, 1, 5, 6, 9];
    assert_eq!(minimap_row_count(&indices, 100, 10), 5);
    assert_eq!(minimap_row_count(&[], 7, 10), 7);
    assert_eq!(minimap_row_count(&[], 0, 10), 1);
    assert_eq!(minimap_row_count(&[], usize::MAX, 10), 10);
}

#[test]
fn minimap_thumb_math_uses_fold_filtered_row_space() {
    // 1000 buffer lines, but folding collapses them to 40 scrollable rows.
    let row_count = minimap_row_count(&[], 40, 1000);
    assert_eq!(row_count, 40);
    let visible_lines = minimap_visible_line_count(200.0, 20.0, row_count);
    assert_eq!(visible_lines, 10);
    let first_visible_row = minimap_first_visible_line(200.0, 20.0, row_count);
    assert_eq!(first_visible_row, 10);

    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 200.0));
    let viewport = minimap_viewport_rect(rect, first_visible_row, visible_lines, row_count);
    // Height: 10/40 * 200 = 50 clamps to 44; travel = 156; top ratio = 10/30.
    assert!((viewport.height() - 44.0).abs() < 0.0001);
    let expected_top = (10.0f64 / 30.0) * (200.0 - 44.0);
    assert!((f64::from(viewport.top()) - expected_top).abs() < 0.0001);
}

#[test]
fn minimap_click_maps_fold_filtered_row_to_buffer_line() {
    let visible_line_indices = vec![0usize, 1, 5, 6, 9];
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 100.0));

    // Click near the bottom: center row 4 -> target row 3 -> buffer line 6.
    assert_eq!(
        minimap_jump_line_from_y(100.0, rect, 10, 5, 2, &visible_line_indices),
        6
    );
    // Click at the top maps to the first visible buffer line.
    assert_eq!(
        minimap_jump_line_from_y(0.0, rect, 10, 5, 2, &visible_line_indices),
        0
    );
    // Without folds the fold-filtered row is the buffer line.
    assert_eq!(minimap_jump_line_from_y(100.0, rect, 10, 5, 2, &[]), 3);
}

#[test]
fn minimap_line_length_cache_reuses_matching_buffer_version_and_settings() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\nxy\n\nlambda\nz".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let first = cache.sampled_lengths_for(&buffer, 5, 4, true).to_vec();
    let second = cache.sampled_lengths_for(&buffer, 5, 4, true).to_vec();

    assert_eq!(sample_lengths(&first), vec![4, 2, 0, 4, 1]);
    assert_eq!(sample_lines(&first), vec![0, 1, 2, 3, 4]);
    assert_eq!(second, first);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hits(), 1);
}

#[test]
fn minimap_line_length_cache_clamps_requested_samples_to_lines() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\nxy\n".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let samples = cache.sampled_lengths_for(&buffer, 20, 6, true);

    assert_eq!(sample_lines(samples), vec![0, 1, 2]);
    assert_eq!(sample_lengths(samples), vec![6, 2, 0]);
}

#[test]
fn minimap_line_length_cache_caps_requested_samples_to_guardrail() {
    let text = (0..(MAX_MINIMAP_LINE_SAMPLES + 10))
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, None, text);
    let mut cache = MinimapLineLengthCache::default();

    let samples = cache.sampled_lengths_for(&buffer, usize::MAX, 80, true);

    assert_eq!(samples.len(), MAX_MINIMAP_LINE_SAMPLES);
}

#[test]
fn minimap_line_length_cache_normalizes_oversized_max_column() {
    let buffer = TextBuffer::from_text(1, None, "x".repeat(MAX_EDITOR_MINIMAP_MAX_COLUMN + 10));
    let mut cache = MinimapLineLengthCache::default();

    let first = cache
        .sampled_lengths_for(&buffer, 1, MAX_EDITOR_MINIMAP_MAX_COLUMN + 1, true)
        .to_vec();
    let second = cache
        .sampled_lengths_for(&buffer, 1, usize::MAX, true)
        .to_vec();

    assert_eq!(sample_lengths(&first), vec![MAX_EDITOR_MINIMAP_MAX_COLUMN]);
    assert_eq!(second, first);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hits(), 1);
}

#[test]
fn minimap_line_length_cache_invalidates_older_buffer_versions() {
    let mut buffer = TextBuffer::from_text(1, None, "a".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let first = cache.sampled_lengths_for(&buffer, 1, 10, true).to_vec();
    buffer.set_single_cursor(buffer.len_chars());
    buffer.insert_at_cursor("bcdef");
    let second = cache.sampled_lengths_for(&buffer, 1, 10, true).to_vec();

    assert_eq!(sample_lengths(&first), vec![1]);
    assert_eq!(sample_lengths(&second), vec![6]);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hits(), 0);
}

#[test]
fn minimap_line_length_cache_separates_render_settings() {
    let buffer = TextBuffer::from_text(1, None, "abcdef".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let character_mode = cache.sampled_lengths_for(&buffer, 1, 3, true).to_vec();
    let block_mode = cache.sampled_lengths_for(&buffer, 1, 3, false).to_vec();
    let character_mode_again = cache.sampled_lengths_for(&buffer, 1, 3, true).to_vec();

    assert_eq!(sample_lengths(&character_mode), vec![3]);
    assert_eq!(sample_lengths(&block_mode), vec![1]);
    assert_eq!(character_mode_again, character_mode);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.hits(), 1);
}

#[test]
fn minimap_line_length_cache_reuses_block_mode_across_max_column() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\n\nxy".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let first = cache.sampled_lengths_for(&buffer, 3, 80, false).to_vec();
    let second = cache.sampled_lengths_for(&buffer, 3, 120, false).to_vec();

    assert_eq!(sample_lengths(&first), vec![1, 0, 1]);
    assert_eq!(sample_lines(&first), vec![0, 1, 2]);
    assert_eq!(second, first);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hits(), 1);
}

#[test]
fn minimap_line_length_cache_reuses_character_lengths_for_block_presence() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\n\nxy".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let character_mode = cache.sampled_lengths_for(&buffer, 3, 6, true).to_vec();
    let block_mode = cache.sampled_lengths_for(&buffer, 3, 6, false).to_vec();

    assert_eq!(sample_lengths(&character_mode), vec![6, 0, 2]);
    assert_eq!(sample_lengths(&block_mode), vec![1, 0, 1]);
    assert_eq!(sample_lines(&block_mode), vec![0, 1, 2]);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.sample_reuses(), block_mode.len());
}

#[test]
fn minimap_line_length_cache_does_not_reuse_block_presence_for_character_lengths() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\n\nxy".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let block_mode = cache.sampled_lengths_for(&buffer, 3, 6, false).to_vec();
    let character_mode = cache.sampled_lengths_for(&buffer, 3, 6, true).to_vec();

    assert_eq!(sample_lengths(&block_mode), vec![1, 0, 1]);
    assert_eq!(sample_lengths(&character_mode), vec![6, 0, 2]);
    assert_eq!(sample_lines(&character_mode), vec![0, 1, 2]);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.sample_reuses(), 0);
}

#[test]
fn minimap_line_length_cache_reuses_sample_lines_across_render_settings() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\nxy\nlambda".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let character_mode = cache.sampled_lengths_for(&buffer, 3, 6, true).to_vec();
    let block_mode = cache.sampled_lengths_for(&buffer, 3, 6, false).to_vec();

    assert_eq!(sample_lines(&character_mode), vec![0, 1, 2]);
    assert_eq!(sample_lines(&block_mode), vec![0, 1, 2]);
    assert_eq!(sample_lengths(&character_mode), vec![6, 2, 6]);
    assert_eq!(sample_lengths(&block_mode), vec![1, 1, 1]);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.sample_reuses(), block_mode.len());
    assert_eq!(cache.sample_line_cache_len(), 1);
    assert_eq!(cache.sample_line_cache_hits(), 1);
}

#[test]
fn minimap_line_length_cache_reuses_capped_lengths_when_max_column_shrinks() {
    let buffer = TextBuffer::from_text(1, None, "abcdef\nxy\nlambda".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let wide = cache.sampled_lengths_for(&buffer, 3, 6, true).to_vec();
    let narrow = cache.sampled_lengths_for(&buffer, 3, 3, true).to_vec();

    assert_eq!(sample_lengths(&wide), vec![6, 2, 6]);
    assert_eq!(sample_lengths(&narrow), vec![3, 2, 3]);
    assert_eq!(sample_lines(&narrow), vec![0, 1, 2]);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.sample_reuses(), narrow.len());
    assert_eq!(cache.sample_line_cache_hits(), 1);
}

#[test]
fn minimap_line_length_cache_reuses_only_uncapped_lengths_when_max_column_grows() {
    let buffer = TextBuffer::from_text(1, None, "a\nabcd\nxy".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let narrow = cache.sampled_lengths_for(&buffer, 3, 3, true).to_vec();
    let wide = cache.sampled_lengths_for(&buffer, 3, 8, true).to_vec();

    assert_eq!(sample_lengths(&narrow), vec![1, 3, 2]);
    assert_eq!(sample_lengths(&wide), vec![1, 4, 2]);
    assert_eq!(sample_lines(&wide), vec![0, 1, 2]);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.sample_reuses(), 2);
    assert_eq!(cache.sample_line_cache_hits(), 1);
}

#[test]
fn minimap_line_length_cache_reuses_line_lengths_across_sample_counts() {
    let buffer = TextBuffer::from_text(1, None, "a\nbb\nccc\ndddd\neeeee\nffffff".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    let dense = cache.sampled_lengths_for(&buffer, 6, 10, true).to_vec();
    let sparse = cache.sampled_lengths_for(&buffer, 3, 10, true).to_vec();

    assert_eq!(sample_lines(&dense), vec![0, 1, 2, 3, 4, 5]);
    assert_eq!(sample_lengths(&dense), vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(sample_lines(&sparse), vec![0, 3, 5]);
    assert_eq!(sample_lengths(&sparse), vec![1, 4, 6]);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.sample_reuses(), sparse.len());
}

#[test]
fn minimap_line_length_cache_does_not_reuse_line_lengths_after_buffer_edit() {
    let mut buffer = TextBuffer::from_text(1, None, "a\nbb\nccc".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    cache.sampled_lengths_for(&buffer, 3, 10, true);
    buffer.set_single_cursor(buffer.len_chars());
    buffer.insert_at_cursor("dddd");
    let refreshed = cache.sampled_lengths_for(&buffer, 3, 10, true).to_vec();

    assert_eq!(sample_lengths(&refreshed), vec![1, 2, 7]);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.sample_reuses(), 0);
}

#[test]
fn minimap_line_length_cache_reuses_evicted_sample_storage() {
    let text = (0..100)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, None, text.clone());
    let mut cache = MinimapLineLengthCache::default();

    cache.sampled_lengths_for(&buffer, 64, 80, true);
    let reusable_capacity = cache.newest_sample_capacity();
    assert!(reusable_capacity >= 64);

    for buffer_id in 2..=MAX_MINIMAP_LINE_LENGTH_CACHES {
        let buffer = TextBuffer::from_text(buffer_id as u64, None, text.clone());
        cache.sampled_lengths_for(&buffer, 1, 80, true);
    }
    assert_eq!(cache.len(), MAX_MINIMAP_LINE_LENGTH_CACHES);

    let buffer = TextBuffer::from_text(99, None, text);
    let samples = cache.sampled_lengths_for(&buffer, MAX_MINIMAP_LINE_LENGTH_CACHES, 80, true);

    assert_eq!(samples.len(), MAX_MINIMAP_LINE_LENGTH_CACHES);
    assert!(cache.newest_sample_capacity() >= reusable_capacity);
    assert_eq!(cache.len(), MAX_MINIMAP_LINE_LENGTH_CACHES);
}

#[test]
fn minimap_line_length_cache_keeps_reusable_source_when_full() {
    let text = (0..64)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, None, text);
    let mut cache = MinimapLineLengthCache::default();

    cache.sampled_lengths_for(&buffer, 64, 80, true);
    for sample_count in 1..MAX_MINIMAP_LINE_LENGTH_CACHES {
        cache.sampled_lengths_for(&buffer, sample_count, 80, true);
    }
    assert_eq!(cache.len(), MAX_MINIMAP_LINE_LENGTH_CACHES);

    let previous_reuses = cache.sample_reuses();
    let samples = cache.sampled_lengths_for(&buffer, MAX_MINIMAP_LINE_LENGTH_CACHES, 80, true);

    assert_eq!(samples.len(), MAX_MINIMAP_LINE_LENGTH_CACHES);
    assert_eq!(
        cache.sample_reuses() - previous_reuses,
        MAX_MINIMAP_LINE_LENGTH_CACHES
    );
}

#[test]
fn minimap_line_length_cache_clears_entries_for_buffer() {
    let first_buffer = TextBuffer::from_text(1, None, "abcdef".to_owned());
    let second_buffer = TextBuffer::from_text(2, None, "xyz".to_owned());
    let mut cache = MinimapLineLengthCache::default();

    cache.sampled_lengths_for(&first_buffer, 1, 6, true);
    cache.sampled_lengths_for(&second_buffer, 1, 6, true);
    cache.clear_for_buffer(1);

    assert_eq!(cache.len(), 1);
    let remaining = cache.sampled_lengths_for(&second_buffer, 1, 6, true);
    assert_eq!(sample_lengths(remaining), vec![3]);
    assert_eq!(cache.hits(), 1);
}

#[test]
fn minimap_section_header_cache_clears_entries_for_buffer() {
    let first_buffer = TextBuffer::from_text(1, None, "#region Setup\n".to_owned());
    let second_buffer = TextBuffer::from_text(2, None, "#region Later\n".to_owned());
    let mut cache = MinimapSectionHeaderCache::default();

    cache.headers_for(&first_buffer, true, false, "");
    cache.headers_for(&second_buffer, true, false, "");
    cache.clear_for_buffer(1);

    assert_eq!(cache.len(), 1);
    assert!(!cache.contains_buffer_for_test(1));
    assert!(cache.contains_buffer_for_test(2));
    let remaining = cache.headers_for(&second_buffer, true, false, "");
    assert_eq!(remaining.get(&1).map(String::as_str), Some("Later"));
    assert_eq!(cache.hits(), 1);
}

#[test]
fn minimap_section_header_cache_clears_disabled_buffer_headers() {
    let buffer = TextBuffer::from_text(1, None, "#region Setup\n".to_owned());
    let mut cache = MinimapSectionHeaderCache::default();

    cache.headers_for(&buffer, true, false, "");
    let disabled = cache.headers_for(&buffer, false, false, "");

    assert!(disabled.is_empty());
    assert_eq!(cache.len(), 0);
}

#[test]
fn minimap_section_header_scan_guard_matches_large_file_thresholds() {
    assert!(minimap_section_header_scan_allowed(
        LARGE_FILE_MODE_MAX_LINES,
        LARGE_FILE_MODE_MAX_BYTES
    ));
    assert!(!minimap_section_header_scan_allowed(
        LARGE_FILE_MODE_MAX_LINES + 1,
        1
    ));
    assert!(!minimap_section_header_scan_allowed(
        1,
        LARGE_FILE_MODE_MAX_BYTES + 1
    ));
}

#[test]
fn minimap_section_header_text_is_bounded_by_width_and_spacing() {
    assert_eq!(
        minimap_section_header_display_text("  Helpers  ", 48.0, 9.0, 1.0),
        "Helpe"
    );
    assert_eq!(
        minimap_section_header_display_text("Helpers", 8.0, 9.0, 1.0),
        ""
    );
    assert!(minimap_section_header_char_advance(9.0, 2.0) > 7.0);
}

#[test]
fn minimap_section_header_text_strips_bidi_and_control_markers() {
    assert_eq!(
        minimap_section_header_display_text(
            "\u{202e}Set\u{2066}up\u{2069}\nIgnored",
            120.0,
            9.0,
            1.0
        ),
        "SetupIgnored"
    );
    assert_eq!(
        minimap_section_header_display_text("\u{202e}  Setup  \u{2069}", 120.0, 9.0, 1.0),
        "Setup"
    );
}

#[test]
fn minimap_section_header_text_keeps_valid_multibyte_chars() {
    assert_eq!(
        minimap_section_header_display_text("  \u{03bb}ambda tools  ", 48.0, 9.0, 1.0),
        "\u{03bb}ambd"
    );
}

#[test]
fn minimap_diff_line_lookup_streams_forward_without_repeated_tree_searches() {
    let diff_lines = BTreeMap::from([
        (0, GitLineChangeKind::Deleted),
        (4, GitLineChangeKind::Added),
        (9, GitLineChangeKind::Modified),
    ]);
    let mut iter = diff_lines.iter().peekable();

    assert_eq!(minimap_line_change_for_sample(&mut iter, 2), None);
    assert_eq!(
        minimap_line_change_for_sample(&mut iter, 4),
        Some(GitLineChangeKind::Added)
    );
    assert_eq!(
        minimap_line_change_for_sample(&mut iter, 4),
        Some(GitLineChangeKind::Added)
    );
    assert_eq!(minimap_line_change_for_sample(&mut iter, 8), None);
    assert_eq!(
        minimap_line_change_for_sample(&mut iter, 9),
        Some(GitLineChangeKind::Modified)
    );
    assert_eq!(minimap_line_change_for_sample(&mut iter, 10), None);
}

#[test]
fn minimap_section_headers_snap_to_next_sampled_line() {
    let headers = BTreeMap::from([
        (0, "Ignored".to_owned()),
        (2, "Setup".to_owned()),
        (4, "Helpers".to_owned()),
        (9, "Tail".to_owned()),
    ]);
    let mut iter = headers.iter().peekable();

    assert_eq!(minimap_section_header_for_sample(&mut iter, 1), None);
    assert_eq!(
        minimap_section_header_for_sample(&mut iter, 3),
        Some("Setup")
    );
    assert_eq!(
        minimap_section_header_for_sample(&mut iter, 8),
        Some("Helpers")
    );
    assert_eq!(minimap_section_header_for_sample(&mut iter, 8), None);
    assert_eq!(
        minimap_section_header_for_sample(&mut iter, 10),
        Some("Tail")
    );
}

#[test]
fn minimap_section_header_cache_reuses_matching_buffer_version_and_settings() {
    let buffer = TextBuffer::from_text(
        1,
        None,
        "#region Setup\nfn main() {}\n// MARK: Run\n".to_owned(),
    );
    let mut cache = MinimapSectionHeaderCache::default();

    let first = cache.headers_for(&buffer, true, true, "MARK: (?<label>.*)");
    let second = cache.headers_for(&buffer, true, true, "MARK: (?<label>.*)");

    assert_eq!(first, second);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hits(), 1);
    assert_eq!(first.get(&1).map(String::as_str), Some("Setup"));
    assert_eq!(first.get(&3).map(String::as_str), Some("Run"));
}

#[test]
fn minimap_section_header_cache_shares_headers_without_deep_clone() {
    let buffer = TextBuffer::from_text(1, None, "#region Setup\n".to_owned());
    let mut cache = MinimapSectionHeaderCache::default();

    let first = cache.headers_for(&buffer, true, false, "");
    let second = cache.headers_for(&buffer, true, false, "");

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hits(), 1);
}

#[test]
fn minimap_section_header_cache_debounces_version_rescans_within_window() {
    let mut buffer = TextBuffer::from_text(1, None, "#region Setup\n".to_owned());
    let mut cache = MinimapSectionHeaderCache::default();

    let first = cache.headers_for(&buffer, true, false, "");
    buffer.set_single_cursor(buffer.len_chars());
    buffer.insert_at_cursor("#region Later\n");

    // Within the rescan debounce window the previous headers are reused
    // without scanning the new buffer version.
    let debounced = cache.headers_for(&buffer, true, false, "");
    assert!(Arc::ptr_eq(&first, &debounced));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.hits(), 0);

    // Once the window elapses the next frame rescans the new version.
    cache.entries[0].last_rescan = Instant::now() - Duration::from_secs(2);
    let refreshed = cache.headers_for(&buffer, true, false, "");
    assert!(!Arc::ptr_eq(&first, &refreshed));
    assert_eq!(refreshed.get(&2).map(String::as_str), Some("Later"));
    assert_eq!(cache.len(), 1);
}

#[test]
fn minimap_section_header_cache_invalidates_older_buffer_versions() {
    let mut buffer = TextBuffer::from_text(1, None, "#region Setup\n".to_owned());
    let mut cache = MinimapSectionHeaderCache {
        rescan_debounce: Duration::ZERO,
        ..Default::default()
    };

    let first = cache.headers_for(&buffer, true, false, "");
    buffer.set_single_cursor(buffer.len_chars());
    buffer.insert_at_cursor("#region Later\n");
    let second = cache.headers_for(&buffer, true, false, "");

    assert_eq!(cache.len(), 1);
    assert_ne!(first, second);
    assert_eq!(second.get(&1).map(String::as_str), Some("Setup"));
    assert_eq!(second.get(&2).map(String::as_str), Some("Later"));
}

fn sample_lengths(samples: &[super::MinimapLineSample]) -> Vec<usize> {
    samples.iter().map(|sample| sample.line_len).collect()
}

fn sample_lines(samples: &[super::MinimapLineSample]) -> Vec<usize> {
    samples.iter().map(|sample| sample.line_idx).collect()
}

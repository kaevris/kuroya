use crate::{
    editor_pane_rows::EditorRowContext, editor_pane_support::fingerprint_fold_u64,
    editor_text_geometry::visual_column_for_char_offset, theme::bracket_depth_color,
};
use eframe::egui::{self, Color32, Pos2, pos2, vec2};
use kuroya_core::{
    EditorBracketPairGuideMode, TextBuffer,
    buffer::{BracketColor, BracketPairGuide},
};
use std::{
    collections::HashMap,
    ops::Range,
    sync::{LazyLock, Mutex, MutexGuard},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResolvedBracketPairGuide {
    open_idx: usize,
    close_idx: usize,
    depth: usize,
    active: bool,
    open_line: usize,
    open_column: usize,
    close_line: usize,
    close_column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BracketPairGuideBucketKey {
    buffer_id: u64,
    buffer_version: u64,
    buffer_len_chars: usize,
    guides_fingerprint: u64,
    active_matches_fingerprint: u64,
    vertical_guides: EditorBracketPairGuideMode,
    horizontal_guides: EditorBracketPairGuideMode,
}

#[derive(Default)]
struct BracketPairGuideBucketCache {
    key: Option<BracketPairGuideBucketKey>,
    buckets: HashMap<usize, Vec<ResolvedBracketPairGuide>>,
}

fn bracket_pair_guide_bucket_cache() -> MutexGuard<'static, BracketPairGuideBucketCache> {
    static BRACKET_PAIR_GUIDE_BUCKETS: LazyLock<Mutex<BracketPairGuideBucketCache>> =
        LazyLock::new(|| Mutex::new(BracketPairGuideBucketCache::default()));
    BRACKET_PAIR_GUIDE_BUCKETS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn bracket_pair_guides_fingerprint(guides: &[BracketPairGuide]) -> u64 {
    let mut hash = guides.len() as u64;
    for guide in guides {
        fingerprint_fold_u64(&mut hash, guide.open_idx as u64);
        fingerprint_fold_u64(&mut hash, guide.close_idx as u64);
        fingerprint_fold_u64(&mut hash, guide.depth as u64);
    }
    hash
}

fn bracket_active_matches_fingerprint(matches: &[(usize, usize)]) -> u64 {
    let mut hash = matches.len() as u64;
    for (open_idx, close_idx) in matches {
        fingerprint_fold_u64(&mut hash, *open_idx as u64);
        fingerprint_fold_u64(&mut hash, *close_idx as u64);
    }
    hash
}

fn build_bracket_pair_guide_buckets(
    buffer: &TextBuffer,
    guides: &[BracketPairGuide],
    active_matches: &[(usize, usize)],
    vertical_guides: EditorBracketPairGuideMode,
    horizontal_guides: EditorBracketPairGuideMode,
) -> HashMap<usize, Vec<ResolvedBracketPairGuide>> {
    let mut buckets: HashMap<usize, Vec<ResolvedBracketPairGuide>> = HashMap::new();
    for guide in guides {
        let active = guide_is_active(guide.open_idx, guide.close_idx, active_matches);
        if !guide_mode_shows_any(vertical_guides, horizontal_guides, active) {
            continue;
        }
        let vertical_shows = guide_mode_shows(vertical_guides, active);
        let open_pos = buffer.char_position(guide.open_idx);
        let close_pos = buffer.char_position(guide.close_idx);
        let resolved = ResolvedBracketPairGuide {
            open_idx: guide.open_idx,
            close_idx: guide.close_idx,
            depth: guide.depth,
            active,
            open_line: open_pos.line,
            open_column: open_pos.column,
            close_line: close_pos.line,
            close_column: close_pos.column,
        };
        if vertical_shows {
            let start = resolved.open_line.min(resolved.close_line);
            let end = resolved.open_line.max(resolved.close_line);
            for line in start..=end {
                buckets.entry(line).or_default().push(resolved);
            }
        } else {
            buckets
                .entry(resolved.open_line)
                .or_default()
                .push(resolved);
            if resolved.close_line != resolved.open_line {
                buckets
                    .entry(resolved.close_line)
                    .or_default()
                    .push(resolved);
            }
        }
    }
    buckets
}

pub(crate) fn paint_bracket_pair_guides(
    painter: &egui::Painter,
    line_idx: usize,
    line_text: &str,
    text_pos: Pos2,
    rect: egui::Rect,
    row: &EditorRowContext<'_>,
) {
    let vertical_guides = row.bracket_pair_guides;
    let horizontal_guides = row.bracket_pair_guides_horizontal;
    if (!vertical_guides.enabled() && !horizontal_guides.enabled())
        || !bracket_overlay_geometry_is_valid(
            text_pos.x,
            rect.left(),
            rect.top(),
            rect.right(),
            rect.bottom(),
            row.char_width,
            row.row_height,
            row.gutter_width,
        )
    {
        return;
    }

    let key = BracketPairGuideBucketKey {
        buffer_id: row.buffer.id(),
        buffer_version: row.buffer.version(),
        buffer_len_chars: row.buffer.len_chars(),
        guides_fingerprint: bracket_pair_guides_fingerprint(row.bracket_pair_guide_ranges),
        active_matches_fingerprint: bracket_active_matches_fingerprint(
            row.active_bracket_pair_matches,
        ),
        vertical_guides,
        horizontal_guides,
    };
    let mut cache = bracket_pair_guide_bucket_cache();
    if cache.key != Some(key) {
        cache.key = Some(key);
        cache.buckets = build_bracket_pair_guide_buckets(
            row.buffer,
            row.bracket_pair_guide_ranges,
            row.active_bracket_pair_matches,
            vertical_guides,
            horizontal_guides,
        );
    }
    let Some(row_guides) = cache.buckets.get(&line_idx) else {
        return;
    };

    for guide in row_guides {
        let draw_vertical = guide_mode_shows(vertical_guides, guide.active)
            && guide_visible_on_line(guide.open_line, guide.close_line, line_idx);
        let draw_horizontal = guide_mode_shows(horizontal_guides, guide.active)
            && (line_idx == guide.open_line || line_idx == guide.close_line);
        if !draw_vertical && !draw_horizontal {
            continue;
        }

        let open_x = bracket_guide_x(
            row,
            text_pos,
            line_idx,
            line_text,
            guide.open_line,
            guide.open_column,
        );
        let stroke = bracket_pair_guide_stroke(
            guide.depth,
            guide.active,
            row.highlight_active_bracket_pair,
            row.weak_text_color,
        );

        if draw_vertical {
            let top = if line_idx == guide.open_line {
                rect.top() + row.row_height * 0.58
            } else {
                rect.top() + 2.0
            };
            let bottom = if line_idx == guide.close_line {
                rect.top() + row.row_height * 0.42
            } else {
                rect.bottom() - 2.0
            };
            if bottom > top {
                painter.line_segment([pos2(open_x, top), pos2(open_x, bottom)], stroke);
            }
        }

        if draw_horizontal {
            let close_x = bracket_guide_x(
                row,
                text_pos,
                line_idx,
                line_text,
                guide.close_line,
                guide.close_column,
            );
            if guide.open_line == guide.close_line && line_idx == guide.open_line {
                paint_horizontal_bracket_pair_guide(painter, rect, row, open_x, close_x, stroke);
            } else {
                if line_idx == guide.open_line {
                    paint_horizontal_bracket_pair_guide(
                        painter,
                        rect,
                        row,
                        open_x,
                        open_x + row.char_width,
                        stroke,
                    );
                }
                if line_idx == guide.close_line {
                    paint_horizontal_bracket_pair_guide(
                        painter, rect, row, open_x, close_x, stroke,
                    );
                }
            }
        }
    }
}

pub(crate) fn paint_bracket_depth_markers(
    painter: &egui::Painter,
    snapshot_range: &Range<usize>,
    line_text: &str,
    text_pos: Pos2,
    rect: egui::Rect,
    bracket_colors: &[BracketColor],
    row: &EditorRowContext<'_>,
) {
    if !bracket_overlay_geometry_is_valid(
        text_pos.x,
        rect.left(),
        rect.top(),
        rect.right(),
        rect.bottom(),
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return;
    }

    let start = bracket_colors.partition_point(|color| color.char_idx < snapshot_range.start);
    let end = start
        + bracket_colors[start..].partition_point(|color| color.char_idx < snapshot_range.end);
    for color in &bracket_colors[start..end] {
        let char_offset = color.char_idx.saturating_sub(snapshot_range.start);
        let col = visual_column_for_char_offset(line_text, char_offset, row.tab_width);
        let x = text_pos.x + col as f32 * row.char_width;
        let y = rect.top() + row.row_height - 4.0;
        painter.line_segment(
            [pos2(x, y), pos2(x + row.char_width.max(4.0), y)],
            egui::Stroke::new(1.4, bracket_depth_color(color.depth)),
        );
    }
}

pub(crate) fn paint_bracket_match_boxes(
    painter: &egui::Painter,
    snapshot_range: &Range<usize>,
    line_text: &str,
    text_pos: Pos2,
    rect: egui::Rect,
    row: &EditorRowContext<'_>,
) {
    if !bracket_overlay_geometry_is_valid(
        text_pos.x,
        rect.left(),
        rect.top(),
        rect.right(),
        rect.bottom(),
        row.char_width,
        row.row_height,
        row.gutter_width,
    ) {
        return;
    }

    for (a, b) in row.bracket_matches {
        for bracket in [a, b] {
            if snapshot_range.contains(bracket) {
                let char_offset = bracket.saturating_sub(snapshot_range.start);
                let col = visual_column_for_char_offset(line_text, char_offset, row.tab_width);
                painter.rect_stroke(
                    egui::Rect::from_min_size(
                        pos2(text_pos.x + (col as f32 * row.char_width), rect.top() + 2.0),
                        vec2(row.char_width, row.row_height - 3.0),
                    ),
                    2.0,
                    egui::Stroke::new(1.0, Color32::from_rgb(231, 185, 87)),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
}

fn bracket_guide_x(
    row: &EditorRowContext<'_>,
    text_pos: Pos2,
    current_line: usize,
    current_line_text: &str,
    line: usize,
    column: usize,
) -> f32 {
    if line == current_line {
        let visual_col = visual_column_for_char_offset(current_line_text, column, row.tab_width);
        return text_pos.x + visual_col as f32 * row.char_width + row.char_width * 0.5;
    }

    let line_text = row.buffer.line(line).unwrap_or_default();
    let line_text = line_text.trim_end_matches(['\r', '\n']);
    let visual_col = visual_column_for_char_offset(line_text, column, row.tab_width);
    text_pos.x + visual_col as f32 * row.char_width + row.char_width * 0.5
}

fn paint_horizontal_bracket_pair_guide(
    painter: &egui::Painter,
    rect: egui::Rect,
    row: &EditorRowContext<'_>,
    start_x: f32,
    end_x: f32,
    stroke: egui::Stroke,
) {
    let left = start_x.min(end_x).max(rect.left() + row.gutter_width);
    let right = start_x
        .max(end_x)
        .max(left + row.char_width.min(8.0))
        .min(rect.right());
    if right <= left {
        return;
    }
    let y = rect.top() + row.row_height * 0.5;
    painter.line_segment([pos2(left, y), pos2(right, y)], stroke);
}

fn bracket_pair_guide_stroke(
    depth: usize,
    active: bool,
    highlight_active: bool,
    inactive_color: Color32,
) -> egui::Stroke {
    if active && highlight_active {
        egui::Stroke::new(1.5, bracket_depth_color(depth))
    } else {
        egui::Stroke::new(1.0, inactive_color)
    }
}

pub(crate) fn guide_mode_shows(mode: EditorBracketPairGuideMode, active: bool) -> bool {
    mode.enabled() && (!mode.active_only() || active)
}

fn guide_mode_shows_any(
    vertical: EditorBracketPairGuideMode,
    horizontal: EditorBracketPairGuideMode,
    active: bool,
) -> bool {
    guide_mode_shows(vertical, active) || guide_mode_shows(horizontal, active)
}

pub(crate) fn guide_visible_on_line(open_line: usize, close_line: usize, line_idx: usize) -> bool {
    let start = open_line.min(close_line);
    let end = open_line.max(close_line);
    start <= line_idx && line_idx <= end
}

pub(crate) fn guide_is_active(
    open_idx: usize,
    close_idx: usize,
    matches: &[(usize, usize)],
) -> bool {
    matches
        .iter()
        .any(|(left, right)| open_idx == (*left).min(*right) && close_idx == (*left).max(*right))
}

fn bracket_overlay_geometry_is_valid(
    text_x: f32,
    rect_left: f32,
    rect_top: f32,
    rect_right: f32,
    rect_bottom: f32,
    char_width: f32,
    row_height: f32,
    gutter_width: f32,
) -> bool {
    text_x.is_finite()
        && rect_left.is_finite()
        && rect_top.is_finite()
        && rect_right.is_finite()
        && rect_bottom.is_finite()
        && rect_right >= rect_left
        && rect_bottom >= rect_top
        && char_width.is_finite()
        && char_width > 0.0
        && row_height.is_finite()
        && row_height > 0.0
        && gutter_width.is_finite()
        && gutter_width >= 0.0
}

#[cfg(test)]
mod tests {
    use super::{
        bracket_active_matches_fingerprint, bracket_overlay_geometry_is_valid,
        bracket_pair_guides_fingerprint, build_bracket_pair_guide_buckets, guide_is_active,
        guide_mode_shows, guide_mode_shows_any, guide_visible_on_line,
    };
    use kuroya_core::{EditorBracketPairGuideMode, TextBuffer, buffer::BracketPairGuide};

    #[test]
    fn bracket_pair_guide_bucket_resolves_each_spanning_line_once() {
        let buffer = TextBuffer::from_text(1, None, "{\nmid\n}".to_owned());
        let guides = [BracketPairGuide {
            open_idx: 0,
            close_idx: 6,
            depth: 1,
        }];

        let vertical = build_bracket_pair_guide_buckets(
            &buffer,
            &guides,
            &[],
            EditorBracketPairGuideMode::On,
            EditorBracketPairGuideMode::Off,
        );
        for line in 0..=2usize {
            assert_eq!(vertical.get(&line).map(Vec::len), Some(1));
        }
        let resolved = &vertical[&1][0];
        assert_eq!((resolved.open_line, resolved.open_column), (0, 0));
        assert_eq!((resolved.close_line, resolved.close_column), (2, 0));
        assert_eq!(resolved.depth, 1);
        assert!(!resolved.active);

        let horizontal = build_bracket_pair_guide_buckets(
            &buffer,
            &guides,
            &[],
            EditorBracketPairGuideMode::Off,
            EditorBracketPairGuideMode::On,
        );
        assert_eq!(horizontal.get(&0).map(Vec::len), Some(1));
        assert_eq!(horizontal.get(&1), None);
        assert_eq!(horizontal.get(&2).map(Vec::len), Some(1));
    }

    #[test]
    fn bracket_pair_guide_bucket_keeps_source_order_within_a_line() {
        let buffer = TextBuffer::from_text(1, None, "{\n{\n}\n}".to_owned());
        let guides = [
            BracketPairGuide {
                open_idx: 2,
                close_idx: 4,
                depth: 1,
            },
            BracketPairGuide {
                open_idx: 0,
                close_idx: 6,
                depth: 0,
            },
        ];

        let buckets = build_bracket_pair_guide_buckets(
            &buffer,
            &guides,
            &[],
            EditorBracketPairGuideMode::On,
            EditorBracketPairGuideMode::Off,
        );

        let line = &buckets[&1];
        assert_eq!(line.len(), 2);
        assert_eq!(line[0].open_idx, 2);
        assert_eq!(line[1].open_idx, 0);
    }

    #[test]
    fn bracket_pair_guide_bucket_follows_active_only_modes() {
        let buffer = TextBuffer::from_text(1, None, "{\n}".to_owned());
        let guides = [BracketPairGuide {
            open_idx: 0,
            close_idx: 2,
            depth: 0,
        }];

        let active = build_bracket_pair_guide_buckets(
            &buffer,
            &guides,
            &[(0, 2)],
            EditorBracketPairGuideMode::Active,
            EditorBracketPairGuideMode::Off,
        );
        assert_eq!(active.get(&0).map(Vec::len), Some(1));
        assert_eq!(active.get(&1).map(Vec::len), Some(1));
        assert!(active[&0][0].active);

        let inactive = build_bracket_pair_guide_buckets(
            &buffer,
            &guides,
            &[],
            EditorBracketPairGuideMode::Active,
            EditorBracketPairGuideMode::Active,
        );
        assert!(inactive.is_empty());
    }

    #[test]
    fn bracket_pair_guide_fingerprints_track_payload_changes() {
        let base = [BracketPairGuide {
            open_idx: 1,
            close_idx: 4,
            depth: 1,
        }];
        let shifted = [BracketPairGuide {
            open_idx: 2,
            close_idx: 4,
            depth: 1,
        }];

        assert_ne!(
            bracket_pair_guides_fingerprint(&base),
            bracket_pair_guides_fingerprint(&shifted)
        );
        assert_eq!(
            bracket_pair_guides_fingerprint(&base),
            bracket_pair_guides_fingerprint(&base)
        );
        assert_ne!(
            bracket_active_matches_fingerprint(&[(1, 4)]),
            bracket_active_matches_fingerprint(&[(1, 5)])
        );
    }

    #[test]
    fn bracket_pair_guide_modes_follow_active_state() {
        assert!(!guide_mode_shows(EditorBracketPairGuideMode::Off, true));
        assert!(guide_mode_shows(EditorBracketPairGuideMode::On, false));
        assert!(!guide_mode_shows(EditorBracketPairGuideMode::Active, false));
        assert!(guide_mode_shows(EditorBracketPairGuideMode::Active, true));
        assert!(!guide_mode_shows_any(
            EditorBracketPairGuideMode::Active,
            EditorBracketPairGuideMode::Active,
            false,
        ));
        assert!(guide_mode_shows_any(
            EditorBracketPairGuideMode::Active,
            EditorBracketPairGuideMode::Off,
            true,
        ));
    }

    #[test]
    fn bracket_pair_guides_are_visible_between_pair_lines() {
        assert!(guide_visible_on_line(2, 5, 2));
        assert!(guide_visible_on_line(2, 5, 4));
        assert!(guide_visible_on_line(2, 5, 5));
        assert!(!guide_visible_on_line(2, 5, 1));
        assert!(!guide_visible_on_line(2, 5, 6));
    }

    #[test]
    fn bracket_pair_guide_active_match_accepts_reversed_pairs() {
        assert!(guide_is_active(4, 12, &[(12, 4)]));
        assert!(guide_is_active(4, 12, &[(4, 12)]));
        assert!(!guide_is_active(4, 12, &[(5, 12)]));
    }

    #[test]
    fn bracket_overlay_geometry_rejects_non_finite_or_collapsed_metrics() {
        assert!(bracket_overlay_geometry_is_valid(
            10.0, 0.0, 0.0, 120.0, 24.0, 8.0, 18.0, 48.0
        ));
        assert!(!bracket_overlay_geometry_is_valid(
            f32::NAN,
            0.0,
            0.0,
            120.0,
            24.0,
            8.0,
            18.0,
            48.0
        ));
        assert!(!bracket_overlay_geometry_is_valid(
            10.0, 0.0, 0.0, 120.0, 24.0, 0.0, 18.0, 48.0
        ));
        assert!(!bracket_overlay_geometry_is_valid(
            10.0,
            0.0,
            0.0,
            120.0,
            24.0,
            8.0,
            f32::INFINITY,
            48.0
        ));
        assert!(!bracket_overlay_geometry_is_valid(
            10.0, 120.0, 0.0, 0.0, 24.0, 8.0, 18.0, 48.0
        ));
        assert!(!bracket_overlay_geometry_is_valid(
            10.0, 0.0, 24.0, 120.0, 0.0, 8.0, 18.0, 48.0
        ));
        assert!(!bracket_overlay_geometry_is_valid(
            10.0, 0.0, 0.0, 120.0, 24.0, 8.0, 18.0, -1.0
        ));
    }
}

use egui::{Rect, pos2};

const MINIMAP_VIEWPORT_MIN_HEIGHT: f32 = 4.0;
const MINIMAP_VIEWPORT_MAX_HEIGHT: f32 = 44.0;

pub(crate) fn minimap_sample_line(
    sample_idx: usize,
    sample_count: usize,
    line_count: usize,
) -> usize {
    let line_count = line_count.max(1);
    if sample_count <= 1 {
        return 0;
    }

    let sample_idx = sample_idx.min(sample_count - 1);
    let denominator = sample_count - 1;
    let max_line = line_count - 1;
    if let Some(rounded) = sample_idx
        .checked_mul(max_line)
        .and_then(|product| product.checked_add(denominator / 2))
        .map(|numerator| numerator / denominator)
    {
        return rounded.min(max_line);
    }

    let sample_idx = sample_idx as u128;
    let denominator = denominator as u128;
    let max_line = max_line as u128;
    let rounded = (sample_idx * max_line + denominator / 2) / denominator;
    rounded.min(max_line) as usize
}

pub(crate) fn minimap_line_from_y(y: f32, rect: Rect, line_count: usize) -> usize {
    let max_line = line_count.saturating_sub(1);
    if max_line == 0 {
        return 0;
    }

    let top = rect.top();
    let bottom = rect.bottom();
    let height = bottom - top;
    if !y.is_finite()
        || !top.is_finite()
        || !bottom.is_finite()
        || !height.is_finite()
        || height <= 0.0
    {
        return 0;
    }

    let clamped_y = y.clamp(top, bottom);
    let ratio = ((clamped_y - top) as f64) / height as f64;
    ((ratio * max_line as f64).round() as usize).min(max_line)
}

pub(crate) fn minimap_target_line_from_y(
    y: f32,
    rect: Rect,
    line_count: usize,
    visible_lines: usize,
) -> usize {
    let line_count = line_count.max(1);
    let center = minimap_line_from_y(y, rect, line_count);
    center
        .saturating_sub(visible_lines / 2)
        .min(line_count.saturating_sub(visible_lines.max(1)))
}

pub(crate) fn minimap_content_fits_viewport(visible_lines: usize, line_count: usize) -> bool {
    visible_lines.max(1) >= line_count.max(1)
}

pub(crate) fn minimap_viewport_rect(
    rect: Rect,
    first_visible_line: usize,
    visible_lines: usize,
    line_count: usize,
) -> Rect {
    let line_count = line_count.max(1);
    let visible_lines = visible_lines.max(1);
    let rect_width = rect.width();
    let rect_height = rect.height();
    if !minimap_rect_bounds_are_finite(rect)
        || !rect_width.is_finite()
        || rect_width <= 0.0
        || !rect_height.is_finite()
        || rect_height <= 0.0
    {
        return minimap_collapsed_viewport_rect(rect);
    }

    let visible_lines = visible_lines.min(line_count);
    if minimap_content_fits_viewport(visible_lines, line_count) {
        return rect;
    }

    let max_first_visible_line = line_count.saturating_sub(visible_lines);
    let height = ((visible_lines as f64 / line_count as f64) * rect_height as f64)
        .clamp(
            MINIMAP_VIEWPORT_MIN_HEIGHT as f64,
            MINIMAP_VIEWPORT_MAX_HEIGHT as f64,
        )
        .min(rect_height as f64) as f32;
    let travel = (rect_height - height).max(0.0) as f64;
    let top_ratio = if max_first_visible_line == 0 {
        0.0
    } else {
        first_visible_line.min(max_first_visible_line) as f64 / max_first_visible_line as f64
    };
    let top = (rect.top() as f64 + top_ratio * travel) as f32;
    let bottom = (top + height).min(rect.bottom());
    Rect::from_min_max(pos2(rect.left(), top), pos2(rect.right(), bottom))
}

fn minimap_rect_bounds_are_finite(rect: Rect) -> bool {
    rect.left().is_finite()
        && rect.right().is_finite()
        && rect.top().is_finite()
        && rect.bottom().is_finite()
}

fn minimap_collapsed_viewport_rect(rect: Rect) -> Rect {
    let left = minimap_finite_or_zero(rect.left());
    let right = minimap_finite_or_zero(rect.right()).max(left);
    let right = if (right - left).is_finite() {
        right
    } else {
        left
    };
    let top = minimap_finite_or_zero(rect.top());
    Rect::from_min_max(pos2(left, top), pos2(right, top))
}

fn minimap_finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::{
        minimap_content_fits_viewport, minimap_line_from_y, minimap_sample_line,
        minimap_target_line_from_y, minimap_viewport_rect,
    };
    use egui::{Rect, pos2, vec2};

    #[test]
    fn minimap_content_fits_viewport_detects_full_rect_thumb_case() {
        assert!(minimap_content_fits_viewport(10, 10));
        assert!(minimap_content_fits_viewport(20, 10));
        assert!(minimap_content_fits_viewport(0, 0));
        assert!(!minimap_content_fits_viewport(5, 10));
    }

    #[test]
    fn minimap_line_from_y_rejects_non_finite_geometry() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 200.0));

        assert_eq!(minimap_line_from_y(f32::NAN, rect, 100), 0);

        let invalid_height = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, f32::NAN));
        assert_eq!(minimap_line_from_y(100.0, invalid_height, 100), 0);
    }

    #[test]
    fn minimap_line_from_y_preserves_large_file_precision() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 100.0));
        let line_count = 50_000_001;

        assert_eq!(
            minimap_line_from_y(33.333_336, rect, line_count),
            16_666_668
        );
        assert_eq!(
            minimap_line_from_y(66.666_664, rect, line_count),
            33_333_332
        );
    }

    #[test]
    fn minimap_line_from_y_clamps_extreme_coordinates_before_ratio() {
        let rect = Rect::from_min_max(pos2(0.0, -f32::MAX), pos2(80.0, 0.0));

        assert_eq!(minimap_line_from_y(f32::MAX, rect, 10), 9);
        assert_eq!(minimap_line_from_y(-f32::MAX, rect, 10), 0);
    }

    #[test]
    fn minimap_viewport_rect_collapses_invalid_height_without_nan_thumb() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(80.0, f32::NAN));

        let viewport = minimap_viewport_rect(rect, 10, 20, 100);

        assert_eq!(viewport.top(), 20.0);
        assert_eq!(viewport.bottom(), 20.0);
    }

    #[test]
    fn minimap_viewport_rect_collapses_invalid_width_without_inverted_thumb() {
        let inverted = Rect::from_min_max(pos2(90.0, 20.0), pos2(10.0, 200.0));
        let viewport = minimap_viewport_rect(inverted, 10, 20, 100);

        assert_eq!(viewport.left(), 90.0);
        assert_eq!(viewport.right(), 90.0);
        assert_eq!(viewport.top(), 20.0);
        assert_eq!(viewport.bottom(), 20.0);

        let overflowing = Rect::from_min_max(pos2(-f32::MAX, 20.0), pos2(f32::MAX, 200.0));
        let viewport = minimap_viewport_rect(overflowing, 10, 20, 100);

        assert_eq!(viewport.left(), -f32::MAX);
        assert_eq!(viewport.right(), -f32::MAX);
        assert_eq!(viewport.top(), 20.0);
        assert_eq!(viewport.bottom(), 20.0);
    }

    #[test]
    fn minimap_viewport_rect_collapses_non_finite_bounds_to_safe_coordinates() {
        let rect = Rect::from_min_max(pos2(f32::NAN, 30.0), pos2(f32::INFINITY, 200.0));

        let viewport = minimap_viewport_rect(rect, 10, 20, 100);

        assert_eq!(viewport.left(), 0.0);
        assert_eq!(viewport.right(), 0.0);
        assert_eq!(viewport.top(), 30.0);
        assert_eq!(viewport.bottom(), 30.0);
    }

    #[test]
    fn minimap_viewport_rect_uses_precise_large_file_position() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 100.0));
        let viewport = minimap_viewport_rect(rect, 16_666_667, 100, 50_000_001);

        assert!((viewport.top() - 32.0).abs() < 0.0001);
        assert!((viewport.height() - 4.0).abs() < 0.0001);
    }

    #[test]
    fn minimap_viewport_rect_covers_rect_when_every_line_is_visible() {
        let rect = Rect::from_min_size(pos2(0.0, 10.0), vec2(80.0, 200.0));

        assert_eq!(minimap_viewport_rect(rect, 4, 10, 10), rect);
        assert_eq!(minimap_viewport_rect(rect, 4, 20, 10), rect);
    }

    #[test]
    fn minimap_viewport_rect_keeps_bottom_inside_rect_after_rounding() {
        let rect = Rect::from_min_size(pos2(0.25, 0.5), vec2(80.0, 99.7));
        let viewport = minimap_viewport_rect(rect, usize::MAX, 7, 10_000_003);

        assert!(viewport.bottom() <= rect.bottom());
        assert_eq!(viewport.bottom(), rect.bottom());
    }

    #[test]
    fn minimap_sample_line_uses_integer_math_for_huge_files() {
        let line_count = usize::MAX;
        assert_eq!(minimap_sample_line(0, 5, line_count), 0);
        assert_eq!(minimap_sample_line(4, 5, line_count), line_count - 1);

        let samples = (0..5)
            .map(|index| minimap_sample_line(index, 5, line_count))
            .collect::<Vec<_>>();
        assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn minimap_target_line_from_y_clamps_to_last_scrollable_first_line() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 200.0));

        assert_eq!(minimap_target_line_from_y(200.0, rect, 100, 20), 80);
        assert_eq!(minimap_target_line_from_y(200.0, rect, 10, 20), 0);
    }
}

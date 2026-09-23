use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, Ui, pos2};

pub(crate) fn draw_file(ui: &Ui, rect: Rect, color: Color32) {
    if !is_drawable_rect(rect) {
        return;
    }

    let painter = ui.painter();
    let color = opaque_color_or(color, Color32::WHITE);
    let stroke = Stroke::new(1.5_f32, color);
    let p = |x: f32, y: f32| {
        pos2(
            rect.left() + rect.width() * (x / 24.0),
            rect.top() + rect.height() * (y / 24.0),
        )
    };
    painter.rect_stroke(
        Rect::from_min_max(p(6.0, 4.0), p(18.0, 20.0)),
        2.0,
        stroke,
        StrokeKind::Inside,
    );
    painter.line_segment([p(13.0, 4.0), p(18.0, 9.0)], stroke);
    painter.line_segment([p(13.0, 4.0), p(13.0, 9.0)], stroke);
    painter.line_segment([p(13.0, 9.0), p(18.0, 9.0)], stroke);
}

pub(crate) fn draw_folder(ui: &Ui, rect: Rect, color: Color32, open: bool) {
    if !is_drawable_rect(rect) {
        return;
    }

    let painter = ui.painter();
    let color = opaque_color_or(color, Color32::WHITE);
    let stroke = Stroke::new(1.5_f32, color);
    let p = |x: f32, y: f32| {
        pos2(
            rect.left() + rect.width() * (x / 24.0),
            rect.top() + rect.height() * (y / 24.0),
        )
    };
    painter.line_segment([p(3.5, 8.0), p(8.5, 8.0)], stroke);
    painter.line_segment([p(8.5, 8.0), p(10.5, 10.0)], stroke);
    painter.line_segment([p(10.5, 10.0), p(20.5, 10.0)], stroke);
    painter.line_segment([p(20.5, 10.0), p(20.5, 18.0)], stroke);
    painter.line_segment([p(20.5, 18.0), p(3.5, 18.0)], stroke);
    painter.line_segment([p(3.5, 18.0), p(3.5, 8.0)], stroke);
    if open {
        painter.line_segment([p(5.0, 13.0), p(19.0, 13.0)], Stroke::new(1.2_f32, color));
    }
}

pub(crate) fn draw_plus(ui: &Ui, center: Pos2, radius: f32, color: Color32) {
    if !is_finite_pos(center) || !radius.is_finite() || radius <= 0.0 {
        return;
    }

    let painter = ui.painter();
    let color = opaque_color_or(color, Color32::WHITE);
    let stroke = Stroke::new(1.5_f32, color);
    painter.line_segment(
        [
            pos2(center.x - radius, center.y),
            pos2(center.x + radius, center.y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            pos2(center.x, center.y - radius),
            pos2(center.x, center.y + radius),
        ],
        stroke,
    );
}

pub(crate) fn arc_points(center: Pos2, radius: f32, start: f32, end: f32) -> Vec<Pos2> {
    const STEPS: usize = 20;
    if !is_finite_pos(center)
        || !radius.is_finite()
        || radius <= 0.0
        || !start.is_finite()
        || !end.is_finite()
    {
        return Vec::new();
    }

    let step_angle = (end - start) / STEPS as f32;
    if !step_angle.is_finite() {
        return Vec::new();
    }

    let (step_sin, step_cos) = step_angle.sin_cos();
    let (mut sin_t, mut cos_t) = start.sin_cos();
    let mut points = Vec::with_capacity(STEPS + 1);

    for _ in 0..=STEPS {
        points.push(pos2(center.x + cos_t * radius, center.y + sin_t * radius));

        let next_cos = cos_t * step_cos - sin_t * step_sin;
        let next_sin = sin_t * step_cos + cos_t * step_sin;
        cos_t = next_cos;
        sin_t = next_sin;
    }

    points
}

fn is_drawable_rect(rect: Rect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
        && rect.width().is_finite()
        && rect.height().is_finite()
        && rect.width() > 0.0
        && rect.height() > 0.0
}

fn is_finite_pos(pos: Pos2) -> bool {
    pos.x.is_finite() && pos.y.is_finite()
}

fn opaque_color_or(color: Color32, fallback: Color32) -> Color32 {
    let color = if color.a() == 0 { fallback } else { color };
    color.to_opaque()
}

#[cfg(test)]
mod tests {
    use super::{arc_points, is_drawable_rect, opaque_color_or};
    use egui::{Color32, Rect, pos2, vec2};

    #[test]
    fn arc_points_rejects_non_finite_geometry() {
        assert!(arc_points(pos2(f32::NAN, 0.0), 4.0, 0.0, 1.0).is_empty());
        assert!(arc_points(pos2(0.0, 0.0), f32::INFINITY, 0.0, 1.0).is_empty());
        assert!(arc_points(pos2(0.0, 0.0), 4.0, f32::NAN, 1.0).is_empty());
        assert_eq!(arc_points(pos2(0.0, 0.0), 4.0, 0.0, 1.0).len(), 21);
    }

    #[test]
    fn primitive_rect_guard_rejects_non_finite_and_empty_rects() {
        assert!(is_drawable_rect(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(1.0, 1.0)
        )));
        assert!(!is_drawable_rect(Rect::from_min_size(
            pos2(f32::NAN, 0.0),
            vec2(1.0, 1.0)
        )));
        assert!(!is_drawable_rect(Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(0.0, 1.0)
        )));
    }

    #[test]
    fn primitive_colors_fall_back_from_transparent_values() {
        assert_eq!(
            opaque_color_or(Color32::TRANSPARENT, Color32::from_rgb(7, 8, 9)),
            Color32::from_rgb(7, 8, 9)
        );
    }
}

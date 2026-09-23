use eframe::egui;

const ANIMATION_TIME: f32 = 0.12;
const KNOB_RADIUS: f32 = 7.0;
const LABEL_GAP: f32 = 8.0;
const TRACK_CORNER_RADIUS: u8 = 9;
const TRACK_SIZE: egui::Vec2 = egui::vec2(34.0, 18.0);

#[allow(dead_code)]
pub(crate) fn ui_switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    draw_switch(ui, value, None)
}

pub(crate) fn ui_switch_with_label(
    ui: &mut egui::Ui,
    value: &mut bool,
    label: &str,
) -> egui::Response {
    draw_switch(ui, value, Some(label))
}

fn knob_center_x(rect: egui::Rect, on: bool) -> f32 {
    let edge_inset = (TRACK_SIZE.y - 2.0 * KNOB_RADIUS) / 2.0 + KNOB_RADIUS;
    if on {
        rect.right() - edge_inset
    } else {
        rect.left() + edge_inset
    }
}

fn draw_switch(ui: &mut egui::Ui, value: &mut bool, label: Option<&str>) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(TRACK_SIZE, egui::Sense::click());
    let keyboard_activated = response.has_focus()
        && ui.input(|input| {
            input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Space)
        });
    let mut changed = false;
    if ui.is_enabled() && (response.clicked() || keyboard_activated) {
        *value = !*value;
        changed = true;
    }

    if let Some(label) = label {
        ui.allocate_exact_size(egui::vec2(LABEL_GAP, 0.0), egui::Sense::hover());
        let label_response = add_clickable_label(ui, label);
        if ui.is_enabled() && label_response.clicked() {
            *value = !*value;
            changed = true;
        }
        response |= label_response;
    }

    if changed {
        response.mark_changed();
    }

    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            ui.is_enabled(),
            *value,
            label.unwrap_or_default(),
        )
    });

    if ui.is_rect_visible(rect) {
        paint_switch(ui, &response, rect, *value);
    }

    response
}

fn paint_switch(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect, on: bool) {
    let painter = ui.painter();
    let visuals = ui.visuals();
    let corner_radius = egui::CornerRadius::same(TRACK_CORNER_RADIUS);

    let (fill, stroke) = if on {
        (visuals.selection.bg_fill, egui::Stroke::NONE)
    } else {
        (
            visuals.extreme_bg_color,
            visuals.widgets.noninteractive.bg_stroke,
        )
    };
    painter.rect_filled(rect, corner_radius, fill);
    if stroke.width > 0.0 {
        painter.rect_stroke(rect, corner_radius, stroke, egui::StrokeKind::Inside);
    }

    let knob_x =
        ui.ctx()
            .animate_value_with_time(response.id, knob_center_x(rect, on), ANIMATION_TIME);
    let knob_color = if on {
        visuals.strong_text_color()
    } else {
        visuals.weak_text_color()
    };
    painter.circle_filled(egui::pos2(knob_x, rect.center().y), KNOB_RADIUS, knob_color);

    if response.has_focus() {
        painter.rect_stroke(
            rect.expand(2.0),
            egui::CornerRadius::same(TRACK_CORNER_RADIUS + 2),
            visuals.selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
}

fn add_clickable_label(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font_id, ui.visuals().text_color());
    let (rect, response) = ui.allocate_exact_size(galley.size(), egui::Sense::click());
    ui.painter()
        .galley(rect.min, galley, ui.visuals().text_color());
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track_rect() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), TRACK_SIZE)
    }

    #[test]
    fn knob_rests_near_left_edge_when_off() {
        assert!((knob_center_x(track_rect(), false) - 109.0).abs() < f32::EPSILON);
    }

    #[test]
    fn knob_rests_near_right_edge_when_on() {
        assert!((knob_center_x(track_rect(), true) - 125.0).abs() < f32::EPSILON);
    }

    #[test]
    fn knob_travel_equals_track_width_minus_height() {
        let travel = knob_center_x(track_rect(), true) - knob_center_x(track_rect(), false);
        assert!((travel - (TRACK_SIZE.x - TRACK_SIZE.y)).abs() < f32::EPSILON);
    }
}

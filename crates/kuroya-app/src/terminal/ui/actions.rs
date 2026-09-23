use crate::ui_icons::{IconKind, draw_icon};
use egui::{Color32, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, pos2, vec2};

use super::layout::bounded_terminal_layout_value;

pub(super) fn terminal_tab_rail_width(available_width: f32) -> f32 {
    let available_width = bounded_terminal_layout_value(available_width);
    if available_width <= 0.0 {
        0.0
    } else if available_width < 360.0 {
        (available_width * 0.42).clamp(104.0, available_width)
    } else {
        210.0
    }
}

pub(super) fn terminal_action_button(
    ui: &mut egui::Ui,
    icon: IconKind,
    label: &str,
    tooltip: &str,
) -> Response {
    terminal_action_button_enabled(ui, true, icon, label, tooltip)
}

pub(super) fn terminal_action_button_enabled(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: IconKind,
    label: &str,
    tooltip: &str,
) -> Response {
    let font_id = TextStyle::Button.resolve(ui.style());
    let text_color = if enabled {
        ui.visuals().widgets.inactive.fg_stroke.color
    } else {
        ui.visuals().widgets.noninteractive.fg_stroke.color
    };
    let active_fill = ui.visuals().widgets.active.bg_fill;
    let hovered_fill = ui.visuals().widgets.hovered.bg_fill;
    let hovered_stroke = ui.visuals().widgets.hovered.bg_stroke.color;
    let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(label.to_owned(), font_id, text_color));
    let width = bounded_terminal_layout_value((galley.rect.width() + 48.0).ceil()).max(34.0);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(width, 34.0), sense);
    let fill = if enabled && response.is_pointer_button_down_on() {
        active_fill
    } else if enabled && response.hovered() {
        hovered_fill
    } else {
        Color32::TRANSPARENT
    };

    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect.shrink(1.0), 5.0, fill);
    }
    if enabled && response.hovered() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            5.0,
            Stroke::new(1.0_f32, hovered_stroke),
            StrokeKind::Inside,
        );
    }

    let icon_rect =
        Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(18.0, 18.0));
    draw_icon(ui, icon_rect, icon, text_color);
    ui.painter().galley(
        pos2(
            rect.left() + 32.0,
            rect.center().y - galley.rect.height() / 2.0,
        ),
        galley,
        text_color,
    );
    response.on_hover_text(tooltip)
}

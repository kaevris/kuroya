use crate::{ui_icon_shapes::draw_icon, ui_icons::IconKind};
use egui::{Align2, Color32, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, Ui, pos2, vec2};

pub(crate) fn icon_button(ui: &mut Ui, icon: IconKind, tooltip: &str) -> Response {
    let tint = ui.visuals().widgets.inactive.fg_stroke.color;
    icon_button_tinted(ui, icon, tooltip, tint)
}

pub(crate) fn icon_button_tinted(
    ui: &mut Ui,
    icon: IconKind,
    tooltip: &str,
    tint: Color32,
) -> Response {
    let button_side = ui.spacing().interact_size.y.max(34.0);
    let icon_side = (button_side - 12.0).max(22.0);
    let (rect, response) = ui.allocate_exact_size(vec2(button_side, button_side), Sense::click());
    let visuals = ui.visuals();
    let fill = if response.is_pointer_button_down_on() {
        visuals.widgets.active.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    };

    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect.shrink(1.0), 5.0, fill);
    }
    if response.hovered() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            5.0,
            Stroke::new(1.0_f32, visuals.widgets.hovered.bg_stroke.color),
            StrokeKind::Inside,
        );
    }

    let icon_rect = Rect::from_center_size(rect.center(), vec2(icon_side, icon_side));
    draw_icon(ui, icon_rect, icon, tint);
    response.on_hover_text(tooltip)
}

pub(crate) fn icon_label(ui: &mut Ui, icon: IconKind, tint: Color32, tooltip: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
    draw_icon(ui, rect, icon, tint);
    response.on_hover_text(tooltip)
}

pub(crate) fn icon_text_button(
    ui: &mut Ui,
    icon: IconKind,
    label: &str,
    detail: Option<&str>,
    width: f32,
) -> Response {
    let height = if detail.is_some() { 48.0 } else { 36.0 };
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let visuals = ui.visuals();
    let fill = if response.is_pointer_button_down_on() {
        visuals.widgets.active.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        visuals.widgets.inactive.weak_bg_fill
    };

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0_f32, visuals.widgets.inactive.bg_stroke.color),
        StrokeKind::Inside,
    );

    let icon_rect =
        Rect::from_center_size(pos2(rect.left() + 23.0, rect.center().y), vec2(22.0, 22.0));
    draw_icon(
        ui,
        icon_rect,
        icon,
        visuals.widgets.inactive.fg_stroke.color,
    );

    let text_x = rect.left() + 46.0;
    let label_y = if detail.is_some() {
        rect.top() + 15.0
    } else {
        rect.center().y
    };
    ui.painter().text(
        pos2(text_x, label_y),
        Align2::LEFT_CENTER,
        label,
        TextStyle::Button.resolve(ui.style()),
        visuals.text_color(),
    );
    if let Some(detail) = detail {
        ui.painter().text(
            pos2(text_x, rect.top() + 33.0),
            Align2::LEFT_CENTER,
            detail,
            TextStyle::Small.resolve(ui.style()),
            visuals.text_color(),
        );
    }

    response
}

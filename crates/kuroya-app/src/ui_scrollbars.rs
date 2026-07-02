use eframe::egui::{self, Color32, Rect, pos2};
use egui::scroll_area::ScrollBarVisibility;
use kuroya_core::EditorScrollbarVisibility;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScrollbarAxes {
    pub(crate) horizontal_visible: bool,
    pub(crate) vertical_visible: bool,
    pub(crate) visibility: ScrollBarVisibility,
}

pub(crate) fn scrollbar_visibility(setting: EditorScrollbarVisibility) -> ScrollBarVisibility {
    match setting {
        EditorScrollbarVisibility::Auto => ScrollBarVisibility::VisibleWhenNeeded,
        EditorScrollbarVisibility::Visible => ScrollBarVisibility::AlwaysVisible,
        EditorScrollbarVisibility::Hidden => ScrollBarVisibility::AlwaysHidden,
    }
}

#[cfg(test)]
pub(crate) fn scrollbar_axis_enabled(setting: EditorScrollbarVisibility) -> bool {
    !matches!(setting, EditorScrollbarVisibility::Hidden)
}

pub(crate) fn scrollbar_axis_visible(setting: EditorScrollbarVisibility, needed: bool) -> bool {
    match setting {
        EditorScrollbarVisibility::Auto => needed,
        EditorScrollbarVisibility::Visible => true,
        EditorScrollbarVisibility::Hidden => false,
    }
}

pub(crate) fn scrollbar_axes(
    vertical: EditorScrollbarVisibility,
    horizontal: EditorScrollbarVisibility,
    vertical_needed: bool,
    horizontal_needed: bool,
) -> ScrollbarAxes {
    let vertical_visible = scrollbar_axis_visible(vertical, vertical_needed);
    let horizontal_visible = scrollbar_axis_visible(horizontal, horizontal_needed);

    ScrollbarAxes {
        horizontal_visible,
        vertical_visible,
        visibility: scrollbar_visibility_for_visible_axes(vertical_visible, horizontal_visible),
    }
}

#[cfg(test)]
pub(crate) fn scrollbar_visibility_for_axes(
    vertical: EditorScrollbarVisibility,
    horizontal: EditorScrollbarVisibility,
    vertical_needed: bool,
    horizontal_needed: bool,
) -> ScrollBarVisibility {
    scrollbar_axes(vertical, horizontal, vertical_needed, horizontal_needed).visibility
}

pub(crate) fn scrollbar_rect_for_axes(rect: Rect, axes: ScrollbarAxes) -> Rect {
    let x = if axes.horizontal_visible {
        rect.left()..=rect.right()
    } else {
        hidden_scrollbar_axis(rect.right())
    };
    let y = if axes.vertical_visible {
        rect.top()..=rect.bottom()
    } else {
        hidden_scrollbar_axis(rect.bottom())
    };

    Rect::from_min_max(pos2(*x.start(), *y.start()), pos2(*x.end(), *y.end()))
}

fn scrollbar_visibility_for_visible_axes(
    vertical_visible: bool,
    horizontal_visible: bool,
) -> ScrollBarVisibility {
    if vertical_visible || horizontal_visible {
        ScrollBarVisibility::AlwaysVisible
    } else {
        ScrollBarVisibility::AlwaysHidden
    }
}

pub(crate) fn themed_scrollbar_width(vertical_size: usize, horizontal_size: usize) -> f32 {
    vertical_size.max(horizontal_size).max(1) as f32
}

pub(crate) fn themed_scrollbar_style(
    vertical_size: usize,
    horizontal_size: usize,
    floating: bool,
) -> egui::style::ScrollStyle {
    let bar_width = themed_scrollbar_width(vertical_size, horizontal_size);
    egui::style::ScrollStyle {
        floating,
        bar_width,
        handle_min_length: 24.0,
        bar_inner_margin: 1.0,
        bar_outer_margin: 1.0,
        floating_width: themed_scrollbar_floating_width(bar_width),
        floating_allocated_width: 0.0,
        foreground_color: true,
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.08,
        interact_background_opacity: 0.16,
        dormant_handle_opacity: 0.34,
        active_handle_opacity: 0.54,
        interact_handle_opacity: 0.86,
    }
}

pub(crate) fn apply_themed_scrollbar_visuals(ui: &mut egui::Ui) {
    let (inactive, hovered, active) = themed_scrollbar_handle_colors(ui.visuals());
    let code_bg = ui.visuals().code_bg_color;
    let visuals = ui.visuals_mut();
    visuals.extreme_bg_color = code_bg;
    visuals.widgets.inactive.fg_stroke.color = inactive;
    visuals.widgets.hovered.fg_stroke.color = hovered;
    visuals.widgets.active.fg_stroke.color = active;
}

pub(crate) fn themed_scrollbar_handle_colors(
    visuals: &egui::Visuals,
) -> (Color32, Color32, Color32) {
    let background = visuals.code_bg_color;
    let accent = visuals.selection.stroke.color;
    let text = visuals.text_color();
    (
        themed_scrollbar_blend_color(background, accent, 0.40),
        themed_scrollbar_blend_color(background, accent, 0.62),
        themed_scrollbar_blend_color(background, text, 0.72),
    )
}

fn themed_scrollbar_floating_width(bar_width: f32) -> f32 {
    if !bar_width.is_finite() || bar_width <= 2.0 {
        return bar_width.max(1.0);
    }

    (bar_width * 0.38).clamp(2.0, bar_width)
}

fn hidden_scrollbar_axis(edge: f32) -> std::ops::RangeInclusive<f32> {
    let start = if edge.is_finite() {
        edge + 10_000.0
    } else {
        10_000.0
    };
    start..=start + 1.0
}

fn themed_scrollbar_blend_color(base: Color32, overlay: Color32, amount: f32) -> Color32 {
    let amount = if amount.is_finite() {
        amount.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let inverse = 1.0 - amount;
    Color32::from_rgb(
        (base.r() as f32 * inverse + overlay.r() as f32 * amount).round() as u8,
        (base.g() as f32 * inverse + overlay.g() as f32 * amount).round() as u8,
        (base.b() as f32 * inverse + overlay.b() as f32 * amount).round() as u8,
    )
}

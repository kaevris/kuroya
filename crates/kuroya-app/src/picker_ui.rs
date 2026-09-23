use eframe::egui::{self, Button, Context, Response, ScrollArea, Ui};

pub(crate) const PICKER_ROW_HEIGHT: f32 = 24.0;

const PICKER_WINDOW_PREFERRED_SIZE: [f32; 2] = [620.0, 420.0];
const PICKER_WINDOW_MIN_SIZE: [f32; 2] = [280.0, 180.0];
const PICKER_WINDOW_MARGIN: [f32; 2] = [32.0, 96.0];

pub(crate) fn picker_window_size(ctx: &Context) -> [f32; 2] {
    let available = ctx.available_rect().size();
    picker_window_size_for_available(available.x, available.y)
}

pub(crate) fn picker_window_size_for_available(
    available_width: f32,
    available_height: f32,
) -> [f32; 2] {
    [
        picker_window_dimension(
            PICKER_WINDOW_PREFERRED_SIZE[0],
            PICKER_WINDOW_MIN_SIZE[0],
            available_width,
            PICKER_WINDOW_MARGIN[0],
        ),
        picker_window_dimension(
            PICKER_WINDOW_PREFERRED_SIZE[1],
            PICKER_WINDOW_MIN_SIZE[1],
            available_height,
            PICKER_WINDOW_MARGIN[1],
        ),
    ]
}

pub(crate) fn picker_scroll_area() -> ScrollArea {
    ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
}

pub(crate) fn picker_selectable_row(ui: &mut Ui, selected: bool, label: &str) -> Response {
    ui.add(Button::selectable(selected, label).truncate())
}

fn picker_window_dimension(preferred: f32, minimum: f32, available: f32, margin: f32) -> f32 {
    if !available.is_finite() || available <= 0.0 {
        return preferred;
    }

    let usable = (available - margin).max(1.0);
    preferred.min(usable).max(minimum.min(usable))
}

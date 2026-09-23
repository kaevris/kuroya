pub(crate) const MIN_EDITOR_PANE_WIDTH: f32 = 220.0;
pub(crate) const EDITOR_SPLIT_HANDLE_WIDTH: f32 = 6.0;

pub(crate) const EXPLORER_DEFAULT_WIDTH: f32 = 260.0;
pub(crate) const EXPLORER_MIN_WIDTH: f32 = 180.0;
pub(crate) const EXPLORER_MAX_WIDTH: f32 = 420.0;

pub(crate) const SYMBOLS_PANEL_DEFAULT_WIDTH: f32 = 300.0;
pub(crate) const SYMBOLS_PANEL_MIN_WIDTH: f32 = 220.0;
pub(crate) const SYMBOLS_PANEL_MAX_WIDTH: f32 = 460.0;

pub(crate) const DIAGNOSTICS_PANEL_DEFAULT_WIDTH: f32 = 340.0;
pub(crate) const DIAGNOSTICS_PANEL_MIN_WIDTH: f32 = 260.0;
pub(crate) const DIAGNOSTICS_PANEL_MAX_WIDTH: f32 = 540.0;

pub(crate) const SOURCE_CONTROL_DEFAULT_WIDTH: f32 = 320.0;
pub(crate) const SOURCE_CONTROL_MIN_WIDTH: f32 = 240.0;
pub(crate) const SOURCE_CONTROL_MAX_WIDTH: f32 = 520.0;

pub(crate) const TERMINAL_DEFAULT_HEIGHT: f32 = 220.0;
pub(crate) const TERMINAL_MIN_HEIGHT: f32 = 140.0;
pub(crate) const TERMINAL_MAX_HEIGHT: f32 = 1_200.0;
pub(crate) const TERMINAL_OPEN_HEIGHT_RATIO: f32 = 0.5;
pub(crate) const TERMINAL_REMAINING_EDITOR_MIN_HEIGHT: f32 = 180.0;

mod split_weights;

pub(crate) use split_weights::{adjust_split_weights, normalize_weights};

fn clamp_panel_width(width: f32, default: f32, min: f32, max: f32) -> f32 {
    let min = finite_non_negative(min);
    let max = finite_non_negative(max).max(min);
    if width.is_finite() {
        width.clamp(min, max)
    } else {
        default.clamp(min, max)
    }
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

pub(crate) fn clamp_explorer_width(width: f32) -> f32 {
    clamp_panel_width(
        width,
        EXPLORER_DEFAULT_WIDTH,
        EXPLORER_MIN_WIDTH,
        EXPLORER_MAX_WIDTH,
    )
}

pub(crate) fn clamp_symbols_panel_width(width: f32) -> f32 {
    clamp_panel_width(
        width,
        SYMBOLS_PANEL_DEFAULT_WIDTH,
        SYMBOLS_PANEL_MIN_WIDTH,
        SYMBOLS_PANEL_MAX_WIDTH,
    )
}

pub(crate) fn clamp_diagnostics_panel_width(width: f32) -> f32 {
    clamp_panel_width(
        width,
        DIAGNOSTICS_PANEL_DEFAULT_WIDTH,
        DIAGNOSTICS_PANEL_MIN_WIDTH,
        DIAGNOSTICS_PANEL_MAX_WIDTH,
    )
}

pub(crate) fn clamp_source_control_width(width: f32) -> f32 {
    clamp_panel_width(
        width,
        SOURCE_CONTROL_DEFAULT_WIDTH,
        SOURCE_CONTROL_MIN_WIDTH,
        SOURCE_CONTROL_MAX_WIDTH,
    )
}

pub(crate) fn responsive_side_panel_max_width(
    total_width: f32,
    other_panel_min_width: f32,
    panel_min_width: f32,
    panel_max_width: f32,
) -> f32 {
    let panel_min_width = finite_non_negative(panel_min_width);
    let panel_max_width = finite_non_negative(panel_max_width).max(panel_min_width);
    if !total_width.is_finite() || total_width <= 0.0 {
        return panel_max_width;
    }

    let other_panel_min_width = finite_non_negative(other_panel_min_width);
    let available_width = total_width - MIN_EDITOR_PANE_WIDTH - other_panel_min_width;
    if !available_width.is_finite() {
        return panel_max_width;
    }

    available_width.clamp(panel_min_width, panel_max_width)
}

pub(crate) fn clamp_terminal_height(height: f32) -> f32 {
    if height.is_finite() {
        height.clamp(TERMINAL_MIN_HEIGHT, TERMINAL_MAX_HEIGHT)
    } else {
        TERMINAL_DEFAULT_HEIGHT
    }
}

pub(crate) fn responsive_terminal_max_height(available_height: f32) -> f32 {
    if !available_height.is_finite() || available_height <= 0.0 {
        return TERMINAL_MAX_HEIGHT;
    }

    (available_height - TERMINAL_REMAINING_EDITOR_MIN_HEIGHT)
        .clamp(TERMINAL_MIN_HEIGHT, TERMINAL_MAX_HEIGHT)
}

pub(crate) fn clamp_terminal_height_for_available_height(
    height: f32,
    available_height: f32,
) -> f32 {
    let max_height = responsive_terminal_max_height(available_height);
    if height.is_finite() {
        height.clamp(TERMINAL_MIN_HEIGHT, max_height)
    } else {
        terminal_open_height(available_height)
    }
}

pub(crate) fn terminal_open_height(available_height: f32) -> f32 {
    if !available_height.is_finite() || available_height <= 0.0 {
        return TERMINAL_DEFAULT_HEIGHT;
    }

    let target = available_height * TERMINAL_OPEN_HEIGHT_RATIO;
    target.clamp(
        TERMINAL_MIN_HEIGHT,
        responsive_terminal_max_height(available_height),
    )
}

/// Largest sensible size for a floating popup window: the viewport minus
/// margins, floored at a usable minimum so the window can never grow past
/// the screen (egui persists window sizes, and anchored windows that
/// overflow the screen leave no reachable resize handle).
pub(crate) fn popup_window_max_size(ctx: &eframe::egui::Context) -> eframe::egui::Vec2 {
    popup_window_max_size_with_top_margin(ctx, 24.0)
}

/// Same as [`popup_window_max_size`] for windows anchored near the top of
/// the screen: `top_margin` reserves the anchor offset plus a bottom margin.
pub(crate) fn popup_window_max_size_with_top_margin(
    ctx: &eframe::egui::Context,
    top_margin: f32,
) -> eframe::egui::Vec2 {
    let available = ctx.content_rect().size();
    eframe::egui::vec2(
        (available.x - 32.0).max(320.0),
        (available.y - top_margin - 48.0).max(240.0),
    )
}

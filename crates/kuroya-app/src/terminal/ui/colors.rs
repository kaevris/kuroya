use egui::{Color32, Stroke};

pub(crate) const TERMINAL_TAB_ACTIVE_FILL_BLEND: f32 = 0.18;
pub(crate) const TERMINAL_TAB_ACTIVE_STROKE_BLEND: f32 = 0.55;
pub(super) const TERMINAL_TAB_HOVER_FILL_BLEND: f32 = 0.08;

pub(super) fn terminal_tab_highlight(
    selected: bool,
    fill: Color32,
    text_color: Color32,
) -> (Color32, Stroke) {
    if selected {
        (
            blend_color(fill, text_color, TERMINAL_TAB_ACTIVE_FILL_BLEND),
            Stroke::new(
                1.0_f32,
                blend_color(fill, text_color, TERMINAL_TAB_ACTIVE_STROKE_BLEND),
            ),
        )
    } else {
        (fill, Stroke::NONE)
    }
}

pub(super) fn terminal_tab_hover_fill(fill: Color32, text_color: Color32) -> Color32 {
    blend_color(fill, text_color, TERMINAL_TAB_HOVER_FILL_BLEND)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalAnsiPalette {
    basic: [Color32; 16],
}

pub(super) fn terminal_background(ui: &egui::Ui) -> Color32 {
    terminal_opaque_color(ui.visuals().code_bg_color, Color32::BLACK)
}

pub(super) fn terminal_accent(ui: &egui::Ui) -> Color32 {
    let accent = ui.visuals().selection.stroke.color;
    let accent = if accent.a() == 0 {
        ui.visuals().selection.bg_fill
    } else {
        accent
    };
    terminal_opaque_color(accent, ui.visuals().text_color())
}

pub(super) fn terminal_muted_text(ui: &egui::Ui) -> Color32 {
    let muted = ui
        .visuals()
        .weak_text_color
        .unwrap_or_else(|| blend_color(ui.visuals().text_color(), terminal_background(ui), 0.45));
    terminal_opaque_color(muted, ui.visuals().text_color())
}

pub(super) fn terminal_ansi_palette(ui: &egui::Ui) -> TerminalAnsiPalette {
    terminal_ansi_palette_from_colors(
        terminal_background(ui),
        ui.visuals().text_color(),
        terminal_muted_text(ui),
        terminal_accent(ui),
        ui.visuals().warn_fg_color,
        ui.visuals().error_fg_color,
    )
}

pub(crate) fn terminal_ansi_palette_from_colors(
    background: Color32,
    foreground: Color32,
    muted: Color32,
    accent: Color32,
    warning: Color32,
    error: Color32,
) -> TerminalAnsiPalette {
    let background = terminal_opaque_color(background, Color32::BLACK);
    let dark = relative_luminance(background) < 0.5;
    let foreground = terminal_opaque_color(
        foreground,
        terminal_default_foreground_for_background(background),
    );
    let muted = terminal_opaque_color(muted, blend_color(foreground, background, 0.45));
    let accent = terminal_opaque_color(accent, foreground);
    let warning = terminal_opaque_color(
        warning,
        if dark {
            Color32::from_rgb(231, 185, 87)
        } else {
            Color32::from_rgb(150, 100, 0)
        },
    );
    let error = terminal_opaque_color(
        error,
        if dark {
            Color32::from_rgb(218, 76, 76)
        } else {
            Color32::from_rgb(180, 30, 40)
        },
    );
    let green = if dark {
        Color32::from_rgb(94, 201, 128)
    } else {
        Color32::from_rgb(36, 142, 83)
    };
    let black = if dark {
        blend_color(background, foreground, 0.10)
    } else {
        blend_color(foreground, background, 0.08)
    };
    let white = if dark {
        blend_color(foreground, background, 0.08)
    } else {
        blend_color(background, foreground, 0.12)
    };
    let blue = accent;
    let yellow = warning;
    let red = error;
    let magenta = blend_color(error, accent, 0.45);
    let cyan = blend_color(accent, green, 0.45);

    TerminalAnsiPalette {
        basic: [
            black,
            red,
            green,
            yellow,
            blue,
            magenta,
            cyan,
            white,
            muted,
            terminal_bright_color(red, dark),
            terminal_bright_color(green, dark),
            terminal_bright_color(yellow, dark),
            terminal_bright_color(blue, dark),
            terminal_bright_color(magenta, dark),
            terminal_bright_color(cyan, dark),
            foreground,
        ],
    }
}

pub(crate) fn terminal_foreground_color(
    color: vt100::Color,
    default: Color32,
    palette: &TerminalAnsiPalette,
) -> Color32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Rgb(red, green, blue) => Color32::from_rgb(red, green, blue),
        vt100::Color::Idx(index) => ansi_index_color(index, palette).unwrap_or(default),
    }
}

pub(super) fn terminal_background_color(
    color: vt100::Color,
    default: Color32,
    palette: &TerminalAnsiPalette,
) -> Color32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Rgb(red, green, blue) => Color32::from_rgb(red, green, blue),
        vt100::Color::Idx(index) => ansi_index_color(index, palette).unwrap_or(default),
    }
}

pub(crate) fn terminal_bold_foreground_color(
    color: vt100::Color,
    resolved: Color32,
    draw_bright: bool,
    palette: &TerminalAnsiPalette,
) -> Color32 {
    if !draw_bright {
        return resolved;
    }

    match color {
        vt100::Color::Idx(index @ 0..=7) => {
            ansi_index_color(index + 8, palette).unwrap_or(resolved)
        }
        _ => blend_color(resolved, Color32::WHITE, 0.18),
    }
}

pub(crate) fn terminal_dim_foreground_color(foreground: Color32, background: Color32) -> Color32 {
    blend_color(foreground, background, 0.45)
}

pub(crate) fn terminal_contrast_color(
    foreground: Color32,
    background: Color32,
    minimum_ratio: f32,
) -> Color32 {
    let minimum_ratio = if minimum_ratio.is_finite() {
        minimum_ratio.clamp(1.0, 21.0)
    } else {
        1.0
    };
    if minimum_ratio <= 1.0 || contrast_ratio(foreground, background) >= minimum_ratio {
        return foreground;
    }

    let black = Color32::BLACK;
    let white = Color32::WHITE;
    let target = if contrast_ratio(black, background) > contrast_ratio(white, background) {
        black
    } else {
        white
    };

    let mut low = 0.0;
    let mut high = 1.0;
    let mut adjusted = target;
    for _ in 0..10 {
        let mid = (low + high) / 2.0;
        let candidate = blend_color(foreground, target, mid);
        if contrast_ratio(candidate, background) >= minimum_ratio {
            adjusted = candidate;
            high = mid;
        } else {
            low = mid;
        }
    }
    adjusted
}

fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let a_luminance = relative_luminance(a);
    let b_luminance = relative_luminance(b);
    let lighter = a_luminance.max(b_luminance);
    let darker = a_luminance.min(b_luminance);
    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: Color32) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = value as f32 / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }

    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

fn ansi_index_color(index: u8, palette: &TerminalAnsiPalette) -> Option<Color32> {
    match index {
        0..=15 => Some(palette.basic[usize::from(index)]),
        16..=231 => {
            let idx = index - 16;
            let red = idx / 36;
            let green = (idx % 36) / 6;
            let blue = idx % 6;
            Some(Color32::from_rgb(
                color_cube_component(red),
                color_cube_component(green),
                color_cube_component(blue),
            ))
        }
        232..=255 => {
            let level = 8 + (index - 232) * 10;
            Some(Color32::from_rgb(level, level, level))
        }
    }
}

fn terminal_bright_color(color: Color32, dark: bool) -> Color32 {
    if dark {
        blend_color(color, Color32::WHITE, 0.24)
    } else {
        blend_color(color, Color32::BLACK, 0.14)
    }
}

fn color_cube_component(value: u8) -> u8 {
    if value == 0 { 0 } else { 55 + value * 40 }
}

pub(super) fn blend_color(base: Color32, overlay: Color32, amount: f32) -> Color32 {
    let amount = if amount.is_finite() {
        amount.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix = |base: u8, overlay: u8| base as f32 + ((overlay as f32 - base as f32) * amount);
    Color32::from_rgb(
        mix(base.r(), overlay.r()).round() as u8,
        mix(base.g(), overlay.g()).round() as u8,
        mix(base.b(), overlay.b()).round() as u8,
    )
}

fn terminal_opaque_color(color: Color32, fallback: Color32) -> Color32 {
    let color = if color.a() == 0 { fallback } else { color };
    Color32::from_rgb(color.r(), color.g(), color.b())
}

fn terminal_default_foreground_for_background(background: Color32) -> Color32 {
    if relative_luminance(background) < 0.5 {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}

#[cfg(test)]
mod tests {
    use super::{
        blend_color, terminal_ansi_palette_from_colors, terminal_background_color,
        terminal_contrast_color, terminal_foreground_color,
    };
    use egui::Color32;

    fn test_palette() -> super::TerminalAnsiPalette {
        terminal_ansi_palette_from_colors(
            Color32::from_rgb(250, 250, 250),
            Color32::from_rgb(30, 30, 30),
            Color32::from_rgb(120, 120, 120),
            Color32::from_rgb(40, 100, 220),
            Color32::from_rgb(150, 100, 0),
            Color32::from_rgb(180, 30, 40),
        )
    }

    #[test]
    fn ansi_background_black_resolves_palette_black_not_default_background() {
        let palette = test_palette();
        let default_background = Color32::from_rgb(245, 245, 245);

        assert_eq!(
            terminal_background_color(vt100::Color::Default, default_background, &palette),
            default_background
        );
        assert_eq!(
            terminal_background_color(vt100::Color::Idx(0), default_background, &palette),
            terminal_foreground_color(vt100::Color::Idx(0), default_background, &palette)
        );
        assert_ne!(
            terminal_background_color(vt100::Color::Idx(0), default_background, &palette),
            default_background
        );
    }

    #[test]
    fn blend_color_uses_base_for_non_finite_amount() {
        let base = Color32::from_rgb(10, 20, 30);
        let overlay = Color32::from_rgb(200, 210, 220);

        assert_eq!(blend_color(base, overlay, f32::NAN), base);
        assert_eq!(blend_color(base, overlay, f32::INFINITY), base);
    }

    #[test]
    fn contrast_color_treats_non_finite_minimum_as_no_extra_contrast() {
        let foreground = Color32::from_rgb(120, 120, 120);
        let background = Color32::from_rgb(124, 124, 124);

        assert_eq!(
            terminal_contrast_color(foreground, background, f32::NAN),
            foreground
        );
    }

    #[test]
    fn ansi_palette_uses_opaque_fallbacks_for_transparent_theme_colors() {
        let palette = terminal_ansi_palette_from_colors(
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
            Color32::TRANSPARENT,
        );

        assert_ne!(
            terminal_foreground_color(vt100::Color::Idx(15), Color32::TRANSPARENT, &palette),
            Color32::TRANSPARENT
        );
        assert_ne!(
            terminal_background_color(vt100::Color::Idx(0), Color32::TRANSPARENT, &palette),
            Color32::TRANSPARENT
        );
    }
}

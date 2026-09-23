use crate::path_display::sanitized_display_label_cow;
use egui::{self, Context, vec2};
use kuroya_core::{PluginThemeRegistration, PluginThemeRegistry, ThemeSettings};
use std::{borrow::Cow, sync::OnceLock};

use colors::{blend_color, color_distance, higher_contrast_color, readable_color_or};

pub(crate) use colors::{
    bracket_depth_color, diagnostic_color, document_highlight_color, rgb, semantic_token_color,
};

mod colors;

const MIN_TEXT_CONTRAST: f32 = 4.5;
const MIN_MUTED_TEXT_CONTRAST: f32 = 3.0;
const MIN_ACCENT_CONTRAST: f32 = 2.0;
const MIN_SURFACE_DISTANCE: f32 = 8.0;
pub(crate) const THEME_DISPLAY_LABEL_MAX_CHARS: usize = 80;
pub(crate) const THEME_REFERENCE_LABEL_MAX_CHARS: usize = 120;

const DARK_SURFACE_TEXT: [u8; 3] = [222, 226, 233];
const LIGHT_SURFACE_TEXT: [u8; 3] = [36, 41, 49];
const DARK_SURFACE_MUTED: [u8; 3] = [126, 136, 150];
const LIGHT_SURFACE_MUTED: [u8; 3] = [91, 99, 113];
const DARK_SURFACE_ACCENT: [u8; 3] = [91, 141, 239];
const LIGHT_SURFACE_ACCENT: [u8; 3] = [47, 111, 237];
const DARK_SURFACE_WARNING: [u8; 3] = [231, 185, 87];
const LIGHT_SURFACE_WARNING: [u8; 3] = [161, 104, 24];
const DARK_SURFACE_ERROR: [u8; 3] = [232, 98, 98];
const LIGHT_SURFACE_ERROR: [u8; 3] = [190, 50, 50];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThemePalette {
    pub(crate) background: egui::Color32,
    pub(crate) panel: egui::Color32,
    pub(crate) panel_alt: egui::Color32,
    pub(crate) text: egui::Color32,
    pub(crate) muted: egui::Color32,
    pub(crate) accent: egui::Color32,
    pub(crate) selection: egui::Color32,
    pub(crate) warning: egui::Color32,
    pub(crate) error: egui::Color32,
}

pub(crate) fn apply_theme(ctx: &Context, theme: &ThemeSettings) {
    let palette = theme_palette(theme);
    let mut visuals = egui::Visuals::dark();
    let background = palette.background;
    let panel = palette.panel;
    let panel_alt = palette.panel_alt;
    let text = palette.text;
    let muted = palette.muted;
    let border = blend_color(panel, text, 0.12);
    let hover = blend_color(panel_alt, text, 0.06);

    visuals.dark_mode =
        (background.r() as u16 + background.g() as u16 + background.b() as u16) < 384;
    visuals.panel_fill = panel;
    visuals.window_fill = panel;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.window_corner_radius = egui::CornerRadius::same(6);
    visuals.menu_corner_radius = egui::CornerRadius::same(5);
    visuals.extreme_bg_color = background;
    visuals.text_edit_bg_color = Some(background);
    visuals.code_bg_color = panel_alt;
    visuals.faint_bg_color = blend_color(background, panel_alt, 0.45);
    visuals.weak_text_color = Some(muted);
    visuals.hyperlink_color = text;
    visuals.warn_fg_color = palette.warning;
    visuals.error_fg_color = palette.error;
    visuals.override_text_color = Some(text);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, palette.accent);
    visuals.widgets.noninteractive.bg_fill = panel;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, text);
    visuals.widgets.inactive.bg_fill = panel;
    visuals.widgets.inactive.weak_bg_fill = panel_alt;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, text);
    visuals.widgets.hovered.bg_fill = hover;
    visuals.widgets.hovered.weak_bg_fill = hover;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, text);
    visuals.widgets.active.bg_fill = blend_color(panel_alt, text, 0.10);
    visuals.widgets.active.weak_bg_fill = blend_color(panel_alt, text, 0.08);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, text);
    visuals.widgets.open.bg_fill = panel_alt;
    visuals.widgets.open.weak_bg_fill = panel_alt;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, border);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, text);
    visuals.selection.bg_fill = palette.selection;
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(4);
    }
    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = vec2(6.0, 6.0);
        style.spacing.button_padding = vec2(8.0, 4.0);
        style.spacing.interact_size = vec2(28.0, 28.0);
        style.spacing.icon_width = 16.0;
        style.spacing.icon_width_inner = 12.0;
        style.spacing.icon_spacing = 6.0;
    });
}

pub(crate) fn theme_palette(theme: &ThemeSettings) -> ThemePalette {
    let background = rgb(theme.background);
    let panel = separated_surface_or(rgb(theme.panel), background, surface_step(background, 0.06));
    let panel_alt = separated_surface_or(rgb(theme.panel_alt), panel, surface_step(panel, 0.08));
    let text = readable_theme_color(
        rgb(theme.text),
        background,
        &[DARK_SURFACE_TEXT, LIGHT_SURFACE_TEXT],
        MIN_TEXT_CONTRAST,
    );
    let muted = readable_theme_color(
        rgb(theme.muted_text),
        background,
        &[
            DARK_SURFACE_MUTED,
            LIGHT_SURFACE_MUTED,
            DARK_SURFACE_TEXT,
            LIGHT_SURFACE_TEXT,
        ],
        MIN_MUTED_TEXT_CONTRAST,
    );
    let accent = readable_theme_color(
        rgb(theme.accent),
        panel_alt,
        &[
            DARK_SURFACE_ACCENT,
            LIGHT_SURFACE_ACCENT,
            DARK_SURFACE_TEXT,
            LIGHT_SURFACE_TEXT,
        ],
        MIN_ACCENT_CONTRAST,
    );
    let selection = theme
        .selection
        .map(rgb)
        .unwrap_or_else(|| derived_selection_fill(panel_alt, accent));
    let warning = readable_theme_color(
        rgb(theme.warning),
        panel,
        &[
            DARK_SURFACE_WARNING,
            LIGHT_SURFACE_WARNING,
            DARK_SURFACE_TEXT,
            LIGHT_SURFACE_TEXT,
        ],
        MIN_ACCENT_CONTRAST,
    );
    let error = readable_theme_color(
        rgb(theme.error),
        panel,
        &[
            DARK_SURFACE_ERROR,
            LIGHT_SURFACE_ERROR,
            DARK_SURFACE_TEXT,
            LIGHT_SURFACE_TEXT,
        ],
        MIN_ACCENT_CONTRAST,
    );

    ThemePalette {
        background,
        panel,
        panel_alt,
        text,
        muted,
        accent,
        selection,
        warning,
        error,
    }
}

fn derived_selection_fill(panel_alt: egui::Color32, accent: egui::Color32) -> egui::Color32 {
    blend_color(panel_alt, accent, 0.36)
}

fn separated_surface_or(
    preferred: egui::Color32,
    reference: egui::Color32,
    fallback: egui::Color32,
) -> egui::Color32 {
    if color_distance(preferred, reference) >= MIN_SURFACE_DISTANCE {
        preferred
    } else {
        fallback
    }
}

fn surface_step(reference: egui::Color32, amount: f32) -> egui::Color32 {
    let overlay = higher_contrast_color(reference, egui::Color32::BLACK, egui::Color32::WHITE);
    blend_color(reference, overlay, amount)
}

fn readable_theme_color(
    preferred: egui::Color32,
    background: egui::Color32,
    fallback_candidates: &[[u8; 3]],
    min_contrast: f32,
) -> egui::Color32 {
    let fallback = fallback_candidates
        .iter()
        .copied()
        .map(rgb)
        .fold(high_contrast_text_color(background), |best, candidate| {
            higher_contrast_color(background, best, candidate)
        });
    readable_color_or(preferred, background, fallback, min_contrast)
}

fn high_contrast_text_color(background: egui::Color32) -> egui::Color32 {
    higher_contrast_color(background, rgb(DARK_SURFACE_TEXT), rgb(LIGHT_SURFACE_TEXT))
}

static BUILT_IN_THEMES: OnceLock<Vec<ThemeSettings>> = OnceLock::new();

pub(crate) fn built_in_themes() -> &'static [ThemeSettings] {
    BUILT_IN_THEMES
        .get_or_init(ThemeSettings::built_in_presets)
        .as_slice()
}

pub(crate) fn next_built_in_theme_after(name: &str) -> ThemeSettings {
    let themes = built_in_themes();
    if themes.is_empty() {
        return ThemeSettings::default();
    }

    let name = name.trim();
    let next = themes
        .iter()
        .position(|theme| theme_name_matches_trimmed(&theme.name, name))
        .map(|index| (index + 1) % themes.len())
        .unwrap_or_default();
    themes[next].clone()
}

#[cfg(test)]
pub(crate) fn selected_theme_index(theme: &ThemeSettings) -> usize {
    let name = theme.name.trim();
    built_in_themes()
        .iter()
        .position(|candidate| theme_name_matches_trimmed(&candidate.name, name))
        .unwrap_or_default()
}

pub(crate) fn selected_theme_index_with_plugins(
    theme: &ThemeSettings,
    plugin_themes: &PluginThemeRegistry,
) -> usize {
    let name = theme.name.trim();
    let built_ins = built_in_themes();
    if let Some(index) = built_ins
        .iter()
        .position(|candidate| theme_name_matches_trimmed(&candidate.name, name))
    {
        return index;
    }

    plugin_themes
        .themes()
        .iter()
        .position(|candidate| plugin_theme_matches_name(candidate, name))
        .map(|index| built_ins.len() + index)
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn available_theme_labels(plugin_themes: &PluginThemeRegistry) -> Vec<String> {
    built_in_themes()
        .iter()
        .map(|theme| theme_display_label(&theme.name))
        .chain(
            plugin_themes
                .themes()
                .iter()
                .map(plugin_theme_display_label_bounded),
        )
        .collect()
}

pub(crate) fn theme_display_label(label: &str) -> String {
    theme_display_label_cow(label).into_owned()
}

fn theme_display_label_cow(label: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(label, THEME_DISPLAY_LABEL_MAX_CHARS, "Unnamed theme")
}

pub(crate) fn plugin_theme_display_label(theme: &PluginThemeRegistration) -> &str {
    first_non_empty_trimmed(&[&theme.label, &theme.theme_id, &theme.plugin_id])
        .unwrap_or("Plugin theme")
}

pub(crate) fn plugin_theme_display_label_bounded(theme: &PluginThemeRegistration) -> String {
    plugin_theme_display_label_bounded_cow(theme).into_owned()
}

fn plugin_theme_display_label_bounded_cow(theme: &PluginThemeRegistration) -> Cow<'_, str> {
    sanitized_display_label_cow(
        plugin_theme_display_label(theme),
        THEME_DISPLAY_LABEL_MAX_CHARS,
        "Plugin theme",
    )
}

pub(crate) fn plugin_theme_reference_label(theme: &PluginThemeRegistration) -> String {
    let plugin_id = theme.plugin_id.trim();
    let theme_id = theme.theme_id.trim();
    match (plugin_id.is_empty(), theme_id.is_empty()) {
        (false, false) => bounded_plugin_theme_reference_pair(plugin_id, theme_id),
        (true, false) => bounded_plugin_theme_reference_part(theme_id),
        (false, true) => bounded_plugin_theme_reference_part(plugin_id),
        (true, true) => "plugin-theme".to_owned(),
    }
}

fn bounded_plugin_theme_reference_part(label: &str) -> String {
    bounded_plugin_theme_reference_part_cow(label).into_owned()
}

fn bounded_plugin_theme_reference_part_cow(label: &str) -> Cow<'_, str> {
    bounded_plugin_theme_reference_part_with(label, THEME_REFERENCE_LABEL_MAX_CHARS, "plugin-theme")
}

fn bounded_plugin_theme_reference_part_with<'a>(
    label: &'a str,
    max_chars: usize,
    fallback: &str,
) -> Cow<'a, str> {
    sanitized_display_label_cow(label, max_chars, fallback)
}

fn bounded_plugin_theme_reference_pair(plugin_id: &str, theme_id: &str) -> String {
    if plugin_id
        .len()
        .checked_add(theme_id.len())
        .and_then(|len| len.checked_add(1))
        .is_some_and(|len| len <= THEME_REFERENCE_LABEL_MAX_CHARS)
    {
        let mut label = String::with_capacity(plugin_id.len() + 1 + theme_id.len());
        label.push_str(plugin_id);
        label.push(':');
        label.push_str(theme_id);
        return match sanitized_display_label_cow(
            &label,
            THEME_REFERENCE_LABEL_MAX_CHARS,
            "plugin-theme",
        ) {
            Cow::Borrowed(_) => label,
            Cow::Owned(label) => label,
        };
    }

    let plugin_budget = (THEME_REFERENCE_LABEL_MAX_CHARS - 1) / 2;
    let theme_budget = THEME_REFERENCE_LABEL_MAX_CHARS - plugin_budget - 1;
    let plugin = bounded_plugin_theme_reference_part_with(plugin_id, plugin_budget, "plugin");
    let theme = bounded_plugin_theme_reference_part_with(theme_id, theme_budget, "theme");
    let mut label = String::with_capacity(plugin.len() + 1 + theme.len());
    label.push_str(plugin.as_ref());
    label.push(':');
    label.push_str(theme.as_ref());
    label
}

fn plugin_theme_matches_name(theme: &PluginThemeRegistration, name: &str) -> bool {
    theme_name_matches_trimmed(&theme.label, name)
        || theme_name_matches_trimmed(&theme.theme_id, name)
        || theme_name_matches_trimmed(&plugin_theme_display_label_bounded(theme), name)
        || theme_name_matches_trimmed(&plugin_theme_reference_label(theme), name)
        || plugin_theme_reference_matches_name(theme, name)
}

fn plugin_theme_reference_matches_name(theme: &PluginThemeRegistration, name: &str) -> bool {
    let plugin_id = theme.plugin_id.trim();
    let theme_id = theme.theme_id.trim();
    match (plugin_id.is_empty(), theme_id.is_empty()) {
        (false, false) => {
            name.split_once(':')
                .is_some_and(|(candidate_plugin, candidate_theme)| {
                    candidate_plugin.trim().eq_ignore_ascii_case(plugin_id)
                        && candidate_theme.trim().eq_ignore_ascii_case(theme_id)
                })
        }
        (true, false) => theme_id.eq_ignore_ascii_case(name),
        (false, true) => plugin_id.eq_ignore_ascii_case(name),
        (true, true) => name.eq_ignore_ascii_case("plugin-theme"),
    }
}

fn theme_name_matches_trimmed(candidate: &str, name: &str) -> bool {
    let candidate = candidate.trim();
    !candidate.is_empty() && !name.is_empty() && candidate.eq_ignore_ascii_case(name)
}

fn first_non_empty_trimmed<'a>(values: &[&'a str]) -> Option<&'a str> {
    values
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        MIN_ACCENT_CONTRAST, MIN_MUTED_TEXT_CONTRAST, MIN_TEXT_CONTRAST,
        THEME_DISPLAY_LABEL_MAX_CHARS, THEME_REFERENCE_LABEL_MAX_CHARS,
        bounded_plugin_theme_reference_pair, bounded_plugin_theme_reference_part,
        bounded_plugin_theme_reference_part_cow, colors, plugin_theme_display_label_bounded,
        plugin_theme_display_label_bounded_cow, plugin_theme_reference_label, rgb,
        selected_theme_index, selected_theme_index_with_plugins, theme_display_label,
        theme_display_label_cow, theme_palette,
    };
    use egui::Context;
    use kuroya_core::{
        PLUGIN_API_VERSION, PluginCapabilities, PluginContributions, PluginDescriptor,
        PluginManifest, PluginThemeContribution, PluginThemeRegistration, PluginThemeRegistry,
        ThemeSettings,
    };
    use std::{borrow::Cow, path::PathBuf};

    #[test]
    fn theme_palette_falls_back_from_unreadable_colors() {
        let palette = theme_palette(&ThemeSettings {
            name: "Broken".to_owned(),
            background: [20, 20, 20],
            panel: [20, 20, 20],
            panel_alt: [20, 20, 20],
            text: [20, 20, 20],
            muted_text: [20, 20, 20],
            accent: [20, 20, 20],
            selection: None,
            warning: [20, 20, 20],
            error: [20, 20, 20],
        });

        assert_ne!(palette.panel, palette.background);
        assert_ne!(palette.panel_alt, palette.panel);
        assert!(colors::contrast_ratio(palette.text, palette.background) >= MIN_TEXT_CONTRAST);
        assert!(
            colors::contrast_ratio(palette.muted, palette.background) >= MIN_MUTED_TEXT_CONTRAST
        );
        assert!(colors::contrast_ratio(palette.accent, palette.panel_alt) >= MIN_ACCENT_CONTRAST);
        assert!(colors::contrast_ratio(palette.warning, palette.panel) >= MIN_ACCENT_CONTRAST);
        assert!(colors::contrast_ratio(palette.error, palette.panel) >= MIN_ACCENT_CONTRAST);
        assert_eq!(
            palette.selection,
            colors::blend_color(palette.panel_alt, palette.accent, 0.36)
        );
    }

    #[test]
    fn apply_theme_styles_open_window_header_visuals() {
        let theme = ThemeSettings {
            name: "Header".to_owned(),
            background: [10, 12, 16],
            panel: [21, 25, 32],
            panel_alt: [38, 46, 58],
            text: [235, 238, 244],
            muted_text: [145, 153, 166],
            accent: [82, 139, 255],
            selection: None,
            warning: [231, 185, 87],
            error: [232, 98, 98],
        };
        let palette = theme_palette(&theme);
        let ctx = Context::default();

        super::apply_theme(&ctx, &theme);
        let visuals = ctx.style().visuals.clone();

        assert_eq!(visuals.window_fill, palette.panel);
        assert_eq!(visuals.widgets.open.weak_bg_fill, palette.panel_alt);
        assert_eq!(visuals.widgets.open.bg_fill, palette.panel_alt);
        assert_eq!(visuals.widgets.open.fg_stroke.color, palette.text);
    }

    #[test]
    fn selected_theme_index_trims_builtin_names() {
        let theme = ThemeSettings {
            name: " graphite ".to_owned(),
            ..ThemeSettings::default()
        };

        assert_eq!(
            ThemeSettings::built_in_presets()[selected_theme_index(&theme)].name,
            "Graphite"
        );
    }

    #[test]
    fn theme_display_label_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            theme_display_label_cow("Graphite"),
            Cow::Borrowed("Graphite")
        ));

        let unicode = "\u{591c}\u{660e}\u{3051} Theme";
        match theme_display_label_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed theme label, got {label:?}"),
        }
        assert_eq!(theme_display_label(unicode), unicode);
    }

    #[test]
    fn theme_display_label_cow_owns_dirty_truncated_and_fallback_values() {
        for value in ["  Graphite  ", "Graphite\nDark", "\u{202e}", "   "] {
            let label = theme_display_label_cow(value);

            assert_eq!(label.as_ref(), theme_display_label(value));
            assert!(
                matches!(label, Cow::Owned(_)),
                "expected owned theme label for {value:?}"
            );
        }

        let long = format!("Theme {}", "name-".repeat(32));
        let label = theme_display_label_cow(&long);

        assert_eq!(label.as_ref(), theme_display_label(&long));
        assert!(matches!(label, Cow::Owned(_)));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= THEME_DISPLAY_LABEL_MAX_CHARS);
    }

    #[test]
    fn plugin_theme_selection_accepts_label_id_or_reference() {
        let registry = PluginThemeRegistry::from_plugins(&[PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/solar"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "solar.plugin".to_owned(),
                name: "Solar".to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    themes: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    themes: vec![PluginThemeContribution {
                        id: "solar-dark".to_owned(),
                        label: " Solar Dark ".to_owned(),
                        path: PathBuf::from("workspace/.kuroya/plugins/solar/themes/dark.toml"),
                    }],
                    ..PluginContributions::default()
                },
            },
        }]);
        let plugin_index = ThemeSettings::built_in_presets().len();

        for name in [
            "solar dark",
            "solar-dark",
            "SOLAR.PLUGIN:SOLAR-DARK",
            " solar.plugin : solar-dark ",
        ] {
            assert_eq!(
                selected_theme_index_with_plugins(
                    &ThemeSettings {
                        name: name.to_owned(),
                        ..ThemeSettings::default()
                    },
                    &registry,
                ),
                plugin_index
            );
        }
    }

    #[test]
    fn plugin_theme_display_label_cow_borrows_clean_ascii_and_unicode_values() {
        let ascii = PluginThemeRegistration {
            plugin_id: "solar.plugin".to_owned(),
            theme_id: "solar-dark".to_owned(),
            label: "Solar Dark".to_owned(),
            path: PathBuf::from("themes/dark.toml"),
        };
        assert!(matches!(
            plugin_theme_display_label_bounded_cow(&ascii),
            Cow::Borrowed("Solar Dark")
        ));

        let unicode = PluginThemeRegistration {
            plugin_id: "aurora.plugin".to_owned(),
            theme_id: "aurora-dark".to_owned(),
            label: "\u{591c}\u{660e}\u{3051} Dark".to_owned(),
            path: PathBuf::from("themes/aurora.toml"),
        };
        match plugin_theme_display_label_bounded_cow(&unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode.label.as_str()),
            Cow::Owned(label) => panic!("expected borrowed plugin theme label, got {label:?}"),
        }
    }

    #[test]
    fn plugin_theme_display_label_cow_owns_dirty_truncated_and_preserves_fallback() {
        let dirty = PluginThemeRegistration {
            plugin_id: "unsafe.plugin".to_owned(),
            theme_id: "unsafe-dark".to_owned(),
            label: "Unsafe\n\u{202e}Dark".to_owned(),
            path: PathBuf::from("themes/dark.toml"),
        };
        let label = plugin_theme_display_label_bounded_cow(&dirty);
        assert_eq!(label.as_ref(), plugin_theme_display_label_bounded(&dirty));
        assert!(matches!(label, Cow::Owned(_)));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));

        let long = PluginThemeRegistration {
            plugin_id: "long.plugin".to_owned(),
            theme_id: "long-dark".to_owned(),
            label: format!("Long {}", "theme-".repeat(32)),
            path: PathBuf::from("themes/long.toml"),
        };
        let label = plugin_theme_display_label_bounded_cow(&long);
        assert_eq!(label.as_ref(), plugin_theme_display_label_bounded(&long));
        assert!(matches!(label, Cow::Owned(_)));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= THEME_DISPLAY_LABEL_MAX_CHARS);

        let fallback = PluginThemeRegistration {
            plugin_id: " ".to_owned(),
            theme_id: "\t".to_owned(),
            label: "\n".to_owned(),
            path: PathBuf::from("themes/fallback.toml"),
        };
        assert_eq!(
            plugin_theme_display_label_bounded_cow(&fallback).as_ref(),
            "Plugin theme"
        );
        assert_eq!(
            plugin_theme_display_label_bounded(&fallback),
            "Plugin theme"
        );
    }

    #[test]
    fn plugin_theme_labels_fall_back_to_stable_ids() {
        let registry = PluginThemeRegistry::from_plugins(&[PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/plain"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "plain.plugin".to_owned(),
                name: "Plain".to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    themes: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    themes: vec![PluginThemeContribution {
                        id: "plain-dark".to_owned(),
                        label: " ".to_owned(),
                        path: PathBuf::from("workspace/.kuroya/plugins/plain/themes/dark.toml"),
                    }],
                    ..PluginContributions::default()
                },
            },
        }]);

        assert_eq!(
            super::available_theme_labels(&registry)
                .last()
                .map(String::as_str),
            Some("plain-dark")
        );
    }

    #[test]
    fn plugin_theme_display_labels_are_sanitized_and_bounded() {
        let registry = PluginThemeRegistry::from_plugins(&[PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/unsafe"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "unsafe.plugin".to_owned(),
                name: "Unsafe".to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    themes: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    themes: vec![PluginThemeContribution {
                        id: "unsafe-dark".to_owned(),
                        label: format!("Unsafe\n{}\u{202e}tail", "theme-label-".repeat(32)),
                        path: PathBuf::from("workspace/.kuroya/plugins/unsafe/themes/dark.toml"),
                    }],
                    ..PluginContributions::default()
                },
            },
        }]);

        let label = super::available_theme_labels(&registry).pop().unwrap();

        assert!(label.starts_with("Unsafe "));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= THEME_DISPLAY_LABEL_MAX_CHARS);
    }

    #[test]
    fn plugin_theme_reference_part_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            bounded_plugin_theme_reference_part_cow("solar.plugin"),
            Cow::Borrowed("solar.plugin")
        ));

        let unicode = "\u{591c}\u{660e}\u{3051}-plugin";
        match bounded_plugin_theme_reference_part_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed reference label, got {label:?}"),
        }
    }

    #[test]
    fn plugin_theme_reference_part_cow_owns_dirty_truncated_and_fallback_values() {
        for value in ["  solar.plugin  ", "solar\nplugin", "\u{202e}", "   "] {
            let label = bounded_plugin_theme_reference_part_cow(value);

            assert_eq!(label.as_ref(), bounded_plugin_theme_reference_part(value));
            assert!(
                matches!(label, Cow::Owned(_)),
                "expected owned reference label for {value:?}"
            );
        }

        let long = format!("plugin-{}", "id-".repeat(80));
        let label = bounded_plugin_theme_reference_part_cow(&long);

        assert_eq!(label.as_ref(), bounded_plugin_theme_reference_part(&long));
        assert!(matches!(label, Cow::Owned(_)));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= THEME_REFERENCE_LABEL_MAX_CHARS);
    }

    #[test]
    fn plugin_theme_reference_pair_labels_sanitize_and_bound_parts() {
        assert_eq!(
            bounded_plugin_theme_reference_pair("solar.plugin", "solar-dark"),
            "solar.plugin:solar-dark"
        );
        assert_eq!(
            bounded_plugin_theme_reference_pair(
                "\u{591c}\u{660e}\u{3051}.plugin",
                "\u{591c}\u{660e}\u{3051}-dark"
            ),
            "\u{591c}\u{660e}\u{3051}.plugin:\u{591c}\u{660e}\u{3051}-dark"
        );

        let dirty = bounded_plugin_theme_reference_pair("plugin\nid", "theme\u{202e}id");
        assert_eq!(dirty, "plugin id:themeid");

        let over_budget = bounded_plugin_theme_reference_pair(
            "solar.plugin",
            &format!("dark\n{}\u{202e}tail", "theme-id-".repeat(32)),
        );
        assert!(over_budget.starts_with("solar.plugin:"));
        assert!(!over_budget.contains('\n'));
        assert!(!over_budget.contains('\u{202e}'));
        assert!(over_budget.contains("..."));
        assert!(over_budget.chars().count() <= THEME_REFERENCE_LABEL_MAX_CHARS);
    }

    #[test]
    fn plugin_theme_reference_labels_are_sanitized_and_bounded() {
        let registration = PluginThemeRegistration {
            plugin_id: format!("plugin\n{}\u{2066}tail", "very-long-plugin-id-".repeat(16)),
            theme_id: format!("theme\n{}\u{2067}tail", "very-long-theme-id-".repeat(16)),
            label: "Unsafe Theme".to_owned(),
            path: PathBuf::from("themes/dark.toml"),
        };

        let label = plugin_theme_reference_label(&registration);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{2066}'));
        assert!(!label.contains('\u{2067}'));
        assert!(label.contains(':'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= THEME_REFERENCE_LABEL_MAX_CHARS);
    }

    #[test]
    fn plugin_theme_selection_accepts_bounded_display_label() {
        let long_label = format!("Solar\n{}\u{202e}tail", "dark-theme-".repeat(24));
        let registry = PluginThemeRegistry::from_plugins(&[PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/solar"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "solar.plugin".to_owned(),
                name: "Solar".to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    themes: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    themes: vec![PluginThemeContribution {
                        id: "solar-dark".to_owned(),
                        label: long_label,
                        path: PathBuf::from("workspace/.kuroya/plugins/solar/themes/dark.toml"),
                    }],
                    ..PluginContributions::default()
                },
            },
        }]);
        let bounded_label = plugin_theme_display_label_bounded(&registry.themes()[0]);

        assert!(bounded_label.chars().count() <= THEME_DISPLAY_LABEL_MAX_CHARS);
        assert_eq!(
            selected_theme_index_with_plugins(
                &ThemeSettings {
                    name: bounded_label,
                    ..ThemeSettings::default()
                },
                &registry,
            ),
            ThemeSettings::built_in_presets().len()
        );
    }

    #[test]
    fn plugin_theme_selection_accepts_bounded_reference_label() {
        let long_id = format!("solar\n{}\u{202e}tail", "dark-theme-".repeat(24));
        let registry = PluginThemeRegistry::from_plugins(&[PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/solar"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "solar.plugin".to_owned(),
                name: "Solar".to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    themes: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    themes: vec![PluginThemeContribution {
                        id: long_id,
                        label: "Solar Dark".to_owned(),
                        path: PathBuf::from("workspace/.kuroya/plugins/solar/themes/dark.toml"),
                    }],
                    ..PluginContributions::default()
                },
            },
        }]);
        let bounded_reference = plugin_theme_reference_label(&registry.themes()[0]);

        assert!(bounded_reference.contains("..."));
        assert_eq!(
            selected_theme_index_with_plugins(
                &ThemeSettings {
                    name: bounded_reference,
                    ..ThemeSettings::default()
                },
                &registry,
            ),
            ThemeSettings::built_in_presets().len()
        );
    }

    #[test]
    fn theme_palette_preserves_readable_custom_colors() {
        let theme = ThemeSettings {
            background: [250, 250, 250],
            panel: [236, 236, 236],
            panel_alt: [220, 220, 220],
            text: [10, 10, 10],
            muted_text: [80, 80, 80],
            accent: [30, 90, 220],
            warning: [145, 92, 18],
            error: [170, 45, 45],
            ..ThemeSettings::default()
        };
        let palette = theme_palette(&theme);

        assert_eq!(palette.background, rgb(theme.background));
        assert_eq!(palette.text, rgb(theme.text));
        assert_eq!(palette.accent, rgb(theme.accent));
    }

    #[test]
    fn theme_palette_uses_custom_selection_color() {
        let theme = ThemeSettings {
            selection: Some([18, 64, 118]),
            ..ThemeSettings::default()
        };
        let palette = theme_palette(&theme);

        assert_eq!(palette.selection, rgb([18, 64, 118]));
    }
}

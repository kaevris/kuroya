use std::{collections::BTreeMap, hash::Hash, ops::RangeInclusive};

use crate::preference_panels::sections::{
    SETTINGS_TARGET_EDITOR_DISPLAY, SETTINGS_TEXT_INPUT_MAX_CHARS, SettingsHighlightState,
    bounded_settings_text_edit_width, guarded_f32_drag_value, settings_target_heading,
};
use eframe::egui;
use kuroya_core::{
    DEFAULT_EDITOR_FAST_SCROLL_SENSITIVITY, DEFAULT_EDITOR_LINE_DECORATIONS_WIDTH,
    DEFAULT_EDITOR_MOUSE_WHEEL_SCROLL_SENSITIVITY, EditorColorDecoratorsActivatedOn,
    EditorDefaultColorDecorators, EditorExperimentalGpuAcceleration,
    EditorExperimentalWhitespaceRendering, EditorLineDecorationsWidth, EditorLineNumbers,
    EditorRenderFinalNewline, EditorRenderWhitespace, EditorSettings,
    EditorUnicodeHighlightNonBasicAscii, EditorUnicodeHighlightScope,
    MAX_EDITOR_COLOR_DECORATORS_LIMIT, MAX_EDITOR_LINE_DECORATIONS_WIDTH,
    MAX_EDITOR_LINE_NUMBERS_MIN_CHARS, MAX_EDITOR_PADDING, MAX_EDITOR_SCROLL_BEYOND_LAST_COLUMN,
    MAX_EDITOR_SCROLL_SENSITIVITY, MIN_EDITOR_COLOR_DECORATORS_LIMIT,
    MIN_EDITOR_LINE_DECORATIONS_WIDTH, MIN_EDITOR_LINE_NUMBERS_MIN_CHARS, MIN_EDITOR_PADDING,
    MIN_EDITOR_SCROLL_BEYOND_LAST_COLUMN, MIN_EDITOR_SCROLL_SENSITIVITY,
};
pub(super) fn render_display_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(ui, highlight, SETTINGS_TARGET_EDITOR_DISPLAY, "Display");
    egui::Grid::new("settings_editor_display_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Line numbers");
            editor_line_numbers_combo(ui, "editor_line_numbers", &mut draft.line_numbers);
            ui.end_row();

            ui.label("Line number width");
            ui.add(ranged_drag_value(
                &mut draft.line_numbers_min_chars,
                1.0,
                MIN_EDITOR_LINE_NUMBERS_MIN_CHARS..=MAX_EDITOR_LINE_NUMBERS_MIN_CHARS,
            ))
            .on_hover_text("Minimum number of digits reserved for line numbers");
            ui.end_row();

            ui.label("Line number selection");
            ui.checkbox(
                &mut draft.select_on_line_numbers,
                "Select line when clicking line number",
            )
            .on_hover_text("Clicking a visible line number selects the whole line");
            ui.end_row();

            ui.label("Scroll beyond last column");
            ui.add(ranged_drag_value(
                &mut draft.scroll_beyond_last_column,
                1.0,
                MIN_EDITOR_SCROLL_BEYOND_LAST_COLUMN..=MAX_EDITOR_SCROLL_BEYOND_LAST_COLUMN,
            ));
            ui.end_row();

            ui.label("Scroll behavior");
            ui.vertical(|ui| {
                ui.checkbox(
                    &mut draft.scroll_on_middle_click,
                    "Scroll with middle mouse button",
                );
                ui.checkbox(
                    &mut draft.scroll_predominant_axis,
                    "Keep trackpad scroll on predominant axis",
                );
                ui.checkbox(&mut draft.inertial_scroll, "Use inertial scrolling");
                ui.checkbox(&mut draft.mouse_wheel_zoom, "Zoom font with mouse wheel");
            });
            ui.end_row();

            ui.label("Mouse wheel sensitivity");
            guarded_f32_drag_value(
                ui,
                &mut draft.mouse_wheel_scroll_sensitivity,
                0.1,
                MIN_EDITOR_SCROLL_SENSITIVITY..=MAX_EDITOR_SCROLL_SENSITIVITY,
                DEFAULT_EDITOR_MOUSE_WHEEL_SCROLL_SENSITIVITY,
            );
            ui.end_row();

            ui.label("Fast scroll sensitivity");
            guarded_f32_drag_value(
                ui,
                &mut draft.fast_scroll_sensitivity,
                0.5,
                MIN_EDITOR_SCROLL_SENSITIVITY..=MAX_EDITOR_SCROLL_SENSITIVITY,
                DEFAULT_EDITOR_FAST_SCROLL_SENSITIVITY,
            );
            ui.end_row();

            ui.label("Top padding");
            ui.add(ranged_drag_value(
                &mut draft.padding_top,
                1.0,
                MIN_EDITOR_PADDING..=MAX_EDITOR_PADDING,
            ));
            ui.end_row();

            ui.label("Bottom padding");
            ui.add(ranged_drag_value(
                &mut draft.padding_bottom,
                1.0,
                MIN_EDITOR_PADDING..=MAX_EDITOR_PADDING,
            ));
            ui.end_row();

            ui.label("Editor links");
            ui.checkbox(&mut draft.links, "Detect links in editor text");
            ui.end_row();

            ui.label("Context menu");
            ui.checkbox(&mut draft.contextmenu, "Use editor context menu");
            ui.end_row();

            ui.label("Color decorators");
            ui.checkbox(&mut draft.color_decorators, "Show inline color previews");
            ui.end_row();

            ui.label("Color picker trigger");
            editor_color_decorators_activated_on_combo(
                ui,
                "editor_color_decorators_activated_on",
                &mut draft.color_decorators_activated_on,
            );
            ui.end_row();

            ui.label("Color decorator limit");
            ui.add(ranged_drag_value(
                &mut draft.color_decorators_limit,
                50.0,
                MIN_EDITOR_COLOR_DECORATORS_LIMIT..=MAX_EDITOR_COLOR_DECORATORS_LIMIT,
            ));
            ui.end_row();

            ui.label("Default colors");
            editor_default_color_decorators_combo(
                ui,
                "editor_default_color_decorators",
                &mut draft.default_color_decorators,
            );
            ui.end_row();

            ui.label("Decoration width");
            let mut line_decorations_width = draft.line_decorations_width.pixels(8.0);
            if guarded_f32_drag_value(
                ui,
                &mut line_decorations_width,
                1.0,
                MIN_EDITOR_LINE_DECORATIONS_WIDTH..=MAX_EDITOR_LINE_DECORATIONS_WIDTH,
                DEFAULT_EDITOR_LINE_DECORATIONS_WIDTH,
            )
            .on_hover_text("Pixels reserved between line numbers and editor text")
            .changed()
            {
                draft.line_decorations_width =
                    EditorLineDecorationsWidth::Pixels(line_decorations_width).clamped();
            }
            ui.end_row();

            ui.label("Render whitespace");
            editor_render_whitespace_combo(
                ui,
                "editor_render_whitespace",
                &mut draft.render_whitespace,
            );
            ui.end_row();

            ui.label("Whitespace rendering");
            editor_experimental_whitespace_rendering_combo(
                ui,
                "editor_experimental_whitespace_rendering",
                &mut draft.experimental_whitespace_rendering,
            );
            ui.end_row();

            ui.label("GPU acceleration");
            editor_experimental_gpu_acceleration_combo(
                ui,
                "editor_experimental_gpu_acceleration",
                &mut draft.experimental_gpu_acceleration,
            );
            ui.end_row();

            ui.label("Final newline number");
            editor_render_final_newline_combo(
                ui,
                "editor_render_final_newline",
                &mut draft.render_final_newline,
            );
            ui.end_row();

            ui.label("Control characters");
            ui.checkbox(
                &mut draft.render_control_characters,
                "Render control characters",
            );
            ui.end_row();

            ui.label("Unicode invisible");
            ui.checkbox(
                &mut draft.unicode_highlight_invisible_characters,
                "Highlight invisible characters",
            );
            ui.end_row();

            ui.label("Unicode ambiguous");
            ui.checkbox(
                &mut draft.unicode_highlight_ambiguous_characters,
                "Highlight ambiguous characters",
            );
            ui.end_row();

            ui.label("Unicode non-ASCII");
            editor_unicode_highlight_non_basic_ascii_combo(
                ui,
                "editor_unicode_highlight_non_basic_ascii",
                &mut draft.unicode_highlight_non_basic_ascii,
            );
            ui.end_row();

            ui.label("Unicode scopes");
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Comments");
                    editor_unicode_highlight_scope_combo(
                        ui,
                        "editor_unicode_highlight_include_comments",
                        &mut draft.unicode_highlight_include_comments,
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Strings");
                    editor_unicode_highlight_scope_combo(
                        ui,
                        "editor_unicode_highlight_include_strings",
                        &mut draft.unicode_highlight_include_strings,
                    );
                });
            });
            ui.end_row();

            ui.label("Allowed Unicode");
            render_bool_map_input(
                ui,
                &mut draft.unicode_highlight_allowed_characters,
                "Α=true",
                3,
            )
            .on_hover_text("One character per line, formatted as character=true");
            ui.end_row();

            ui.label("Allowed locales");
            render_bool_map_input(
                ui,
                &mut draft.unicode_highlight_allowed_locales,
                "_os=true",
                3,
            )
            .on_hover_text("Locale keys formatted as locale=true");
            ui.end_row();
        });
}

fn ranged_drag_value<'a, Num>(
    value: &'a mut Num,
    speed: f64,
    range: RangeInclusive<Num>,
) -> egui::DragValue<'a>
where
    Num: egui::emath::Numeric,
{
    egui::DragValue::new(value).speed(speed).range(range)
}

macro_rules! editor_enum_combo {
    ($combo_fn:ident, $label_fn:ident, $enum_ty:ty, [$($variant:path => $label:literal),+ $(,)?]) => {
        fn $combo_fn(ui: &mut egui::Ui, id: impl Hash, value: &mut $enum_ty) {
            egui::ComboBox::from_id_salt(id)
                .selected_text($label_fn(*value))
                .show_ui(ui, |ui| {
                    $(ui.selectable_value(value, $variant, $label);)+
                });
        }

        fn $label_fn(mode: $enum_ty) -> &'static str {
            match mode {
                $($variant => $label,)+
            }
        }
    };
}

editor_enum_combo!(
    editor_line_numbers_combo,
    editor_line_numbers_label,
    EditorLineNumbers,
    [
        EditorLineNumbers::On => "On",
        EditorLineNumbers::Off => "Off",
        EditorLineNumbers::Relative => "Relative",
        EditorLineNumbers::Interval => "Interval",
    ]
);

editor_enum_combo!(
    editor_color_decorators_activated_on_combo,
    editor_color_decorators_activated_on_label,
    EditorColorDecoratorsActivatedOn,
    [
        EditorColorDecoratorsActivatedOn::ClickAndHover => "Click and hover",
        EditorColorDecoratorsActivatedOn::Hover => "Hover",
        EditorColorDecoratorsActivatedOn::Click => "Click",
    ]
);

editor_enum_combo!(
    editor_default_color_decorators_combo,
    editor_default_color_decorators_label,
    EditorDefaultColorDecorators,
    [
        EditorDefaultColorDecorators::Auto => "Auto",
        EditorDefaultColorDecorators::Always => "Always",
        EditorDefaultColorDecorators::Never => "Never",
    ]
);

editor_enum_combo!(
    editor_render_whitespace_combo,
    editor_render_whitespace_label,
    EditorRenderWhitespace,
    [
        EditorRenderWhitespace::None => "None",
        EditorRenderWhitespace::Boundary => "Boundary",
        EditorRenderWhitespace::Selection => "Selection",
        EditorRenderWhitespace::Trailing => "Trailing",
        EditorRenderWhitespace::All => "All",
    ]
);

editor_enum_combo!(
    editor_experimental_whitespace_rendering_combo,
    editor_experimental_whitespace_rendering_label,
    EditorExperimentalWhitespaceRendering,
    [
        EditorExperimentalWhitespaceRendering::Svg => "SVG",
        EditorExperimentalWhitespaceRendering::Font => "Font",
        EditorExperimentalWhitespaceRendering::Off => "Off",
    ]
);

editor_enum_combo!(
    editor_experimental_gpu_acceleration_combo,
    editor_experimental_gpu_acceleration_label,
    EditorExperimentalGpuAcceleration,
    [
        EditorExperimentalGpuAcceleration::Off => "Off",
        EditorExperimentalGpuAcceleration::On => "On",
    ]
);

editor_enum_combo!(
    editor_render_final_newline_combo,
    editor_render_final_newline_label,
    EditorRenderFinalNewline,
    [
        EditorRenderFinalNewline::Off => "Off",
        EditorRenderFinalNewline::On => "On",
        EditorRenderFinalNewline::Dimmed => "Dimmed",
    ]
);

editor_enum_combo!(
    editor_unicode_highlight_non_basic_ascii_combo,
    editor_unicode_highlight_non_basic_ascii_label,
    EditorUnicodeHighlightNonBasicAscii,
    [
        EditorUnicodeHighlightNonBasicAscii::Off => "Off",
        EditorUnicodeHighlightNonBasicAscii::On => "On",
        EditorUnicodeHighlightNonBasicAscii::InUntrustedWorkspace => "Untrusted workspace",
    ]
);

editor_enum_combo!(
    editor_unicode_highlight_scope_combo,
    editor_unicode_highlight_scope_label,
    EditorUnicodeHighlightScope,
    [
        EditorUnicodeHighlightScope::Off => "Off",
        EditorUnicodeHighlightScope::On => "On",
        EditorUnicodeHighlightScope::InUntrustedWorkspace => "Untrusted workspace",
    ]
);

fn render_bool_map_input(
    ui: &mut egui::Ui,
    values: &mut BTreeMap<String, bool>,
    hint_text: &'static str,
    rows: usize,
) -> egui::Response {
    let mut value = bool_map_input_value(values);
    let response = ui.add_sized(
        [
            bounded_settings_text_edit_width(ui.available_width(), 320.0),
            (rows as f32 * 24.0).max(48.0),
        ],
        egui::TextEdit::multiline(&mut value)
            .desired_rows(rows)
            .hint_text(hint_text),
    );
    if response.changed() {
        *values = parse_bool_map_input_value(&value);
    }
    response
}

fn bool_map_input_value(values: &BTreeMap<String, bool>) -> String {
    let mut value = String::with_capacity(bool_map_input_capacity(values));
    let mut chars = 0usize;

    for (index, (key, allowed)) in values.iter().enumerate() {
        if index > 0 && !push_bool_map_input_char(&mut value, &mut chars, '\n') {
            return value;
        }

        if !push_bool_map_input_key(&mut value, &mut chars, key)
            || !push_bool_map_input_str(&mut value, &mut chars, "=")
            || !push_bool_map_input_str(
                &mut value,
                &mut chars,
                if *allowed { "true" } else { "false" },
            )
        {
            return value;
        }

        if chars >= SETTINGS_TEXT_INPUT_MAX_CHARS {
            break;
        }
    }

    value
}

fn bool_map_input_capacity(values: &BTreeMap<String, bool>) -> usize {
    let mut capacity = 0usize;

    for (index, (key, allowed)) in values.iter().enumerate() {
        if index > 0 {
            capacity = capacity.saturating_add(1);
        }
        capacity = capacity
            .saturating_add(key.len())
            .saturating_add(if *allowed {
                "=true".len()
            } else {
                "=false".len()
            });

        if capacity >= SETTINGS_TEXT_INPUT_MAX_CHARS {
            return SETTINGS_TEXT_INPUT_MAX_CHARS;
        }
    }

    capacity
}

fn push_bool_map_input_key(value: &mut String, chars: &mut usize, key: &str) -> bool {
    for ch in key.chars() {
        if is_bool_map_input_format_control(ch) {
            continue;
        }

        let replacement = match ch {
            '\t' | '\r' | '\n' => ' ',
            ch if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') => ' ',
            ch => ch,
        };

        if !push_bool_map_input_char(value, chars, replacement) {
            return false;
        }
    }

    true
}

fn push_bool_map_input_str(value: &mut String, chars: &mut usize, text: &str) -> bool {
    for ch in text.chars() {
        if !push_bool_map_input_char(value, chars, ch) {
            return false;
        }
    }

    true
}

fn push_bool_map_input_char(value: &mut String, chars: &mut usize, ch: char) -> bool {
    if *chars >= SETTINGS_TEXT_INPUT_MAX_CHARS {
        mark_bool_map_input_truncated(value);
        return false;
    }

    value.push(ch);
    *chars += 1;
    true
}

fn mark_bool_map_input_truncated(value: &mut String) {
    if SETTINGS_TEXT_INPUT_MAX_CHARS > 3 {
        truncate_bool_map_input_to_chars(value, SETTINGS_TEXT_INPUT_MAX_CHARS - 3);
        value.push_str("...");
    }
}

fn truncate_bool_map_input_to_chars(value: &mut String, max_chars: usize) {
    if let Some((byte_index, _)) = value.char_indices().nth(max_chars) {
        value.truncate(byte_index);
    }
}

fn is_bool_map_input_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

fn parse_bool_map_input_value(value: &str) -> BTreeMap<String, bool> {
    value
        .lines()
        .filter_map(|line| {
            let (key, allowed) = line.split_once('=')?;
            let key = key.trim();
            let allowed = allowed.trim().parse::<bool>().ok()?;
            (!key.is_empty()).then(|| (key.to_owned(), allowed))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{bool_map_input_value, parse_bool_map_input_value, render_bool_map_input};
    use crate::preference_panels::sections::SETTINGS_TEXT_INPUT_MAX_CHARS;
    use eframe::egui;
    use std::collections::BTreeMap;

    #[test]
    fn bool_map_input_value_sanitizes_keys_in_one_bounded_display() {
        let values = BTreeMap::from([
            ("alpha\u{202e}\r\nbeta".to_owned(), true),
            ("locale\tone".to_owned(), false),
        ]);

        let display = bool_map_input_value(&values);

        assert_eq!(display, "alpha  beta=true\nlocale one=false");
    }

    #[test]
    fn bool_map_input_value_bounds_long_keys_with_display_ellipsis() {
        let long_key = "x".repeat(SETTINGS_TEXT_INPUT_MAX_CHARS + 64);
        let values = BTreeMap::from([(long_key, true)]);

        let display = bool_map_input_value(&values);

        assert!(display.ends_with("..."));
        assert!(!display.contains("=true"));
        assert_eq!(display.chars().count(), SETTINGS_TEXT_INPUT_MAX_CHARS);
    }

    #[test]
    fn bool_map_parse_trims_and_ignores_invalid_lines() {
        let values =
            parse_bool_map_input_value(" alpha = true \nmissing\n = false\nbeta=false\nbad=nope");

        assert_eq!(
            values,
            BTreeMap::from([("alpha".to_owned(), true), ("beta".to_owned(), false)])
        );
    }

    #[test]
    fn bool_map_render_keeps_raw_values_when_unchanged() {
        let ctx = egui::Context::default();
        let mut values = BTreeMap::from([
            (" alpha ".to_owned(), true),
            ("beta\u{202e}".to_owned(), false),
        ]);
        let original = values.clone();

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                render_bool_map_input(ui, &mut values, "A=true", 2);
            });
        });

        assert_eq!(values, original);
    }
}

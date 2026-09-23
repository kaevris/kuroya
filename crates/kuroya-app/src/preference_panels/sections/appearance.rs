use crate::{
    path_display::{display_error_label_cow, sanitized_display_label_cow},
    popup_buttons::{PopupButtonKind, popup_compact_button, popup_compact_button_enabled},
    preference_panels::sections::{
        SETTINGS_DISPLAY_TEXT_MAX_CHARS, SETTINGS_TARGET_APPEARANCE, SettingsHighlightState,
        bounded_settings_display_text, bounded_settings_multiline_join,
        bounded_settings_text_edit_width, settings_switch, settings_target_block,
    },
    theme::{
        THEME_DISPLAY_LABEL_MAX_CHARS, THEME_REFERENCE_LABEL_MAX_CHARS, built_in_themes,
        plugin_theme_display_label_bounded, theme_display_label,
    },
    ui_icons::{IconKind, icon_button},
};
use eframe::egui;
use kuroya_core::{
    EditorBackgroundImageFit, EditorBackgroundImagePosition, EditorBackgroundImageScope,
    EditorSettings, PluginThemeRegistration, PluginThemeRegistry, ThemeSettings,
    clamp_editor_background_image_dim, load_plugin_theme_settings, load_theme_settings_from_path,
};
use std::path::{Path, PathBuf};

const MAX_CUSTOM_THEME_PATHS: usize = 256;
const MAX_CUSTOM_THEME_PATH_CHARS: usize = 4096;
const THEME_CONFIG_EXAMPLE: &str = r##"name = "My Theme"

[palette]
background = "#101318"
panel = "#181C22"
panel_alt = "#202733"
text = "#E6EDF3"
muted_text = "#8B949E"
accent = "#58A6FF"
selection = "#1F6FEB"
warning = "#D29922"
error = "#F85149"
"##;

#[derive(Clone, Debug, PartialEq, Eq)]
struct CustomThemeOption {
    source: String,
    path: PathBuf,
    label: String,
    reference: String,
}

pub(super) fn render_appearance_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    workspace_root: &Path,
    plugin_themes: &PluginThemeRegistry,
    editor_font_path: &str,
    ui_font_path: &str,
    choose_editor_font: &mut bool,
    clear_editor_font: &mut bool,
    choose_ui_font: &mut bool,
    clear_ui_font: &mut bool,
    choose_background_image: &mut bool,
    clear_background_image: &mut bool,
    status: &mut Option<String>,
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_block(ui, highlight, SETTINGS_TARGET_APPEARANCE, |ui| {
        render_appearance_settings_content(
            ui,
            draft,
            workspace_root,
            plugin_themes,
            editor_font_path,
            ui_font_path,
            choose_editor_font,
            clear_editor_font,
            choose_ui_font,
            clear_ui_font,
            choose_background_image,
            clear_background_image,
            status,
        );
    });
}

fn render_appearance_settings_content(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    workspace_root: &Path,
    plugin_themes: &PluginThemeRegistry,
    editor_font_path: &str,
    ui_font_path: &str,
    choose_editor_font: &mut bool,
    clear_editor_font: &mut bool,
    choose_ui_font: &mut bool,
    clear_ui_font: &mut bool,
    choose_background_image: &mut bool,
    clear_background_image: &mut bool,
    status: &mut Option<String>,
) {
    ui.label(egui::RichText::new("Theme").strong());
    egui::Grid::new("settings_appearance_theme_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Theme");
            render_theme_combo(ui, draft, workspace_root, plugin_themes, status);
            ui.end_row();

            ui.label("Custom theme files");
            render_custom_theme_paths(ui, draft);
            ui.end_row();
        });
    render_theme_format_help(ui);

    ui.add_space(12.0);
    ui.label(egui::RichText::new("Fonts").strong());
    egui::Grid::new("settings_appearance_fonts_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Editor font file");
            render_font_file_picker(ui, editor_font_path, choose_editor_font, clear_editor_font);
            ui.end_row();

            ui.label("UI font file");
            render_font_file_picker(ui, ui_font_path, choose_ui_font, clear_ui_font);
            ui.end_row();
        });

    ui.add_space(12.0);
    ui.label(egui::RichText::new("Editor background").strong());
    render_editor_background_settings(ui, draft, choose_background_image, clear_background_image);
}

fn render_editor_background_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    choose_background_image: &mut bool,
    clear_background_image: &mut bool,
) {
    let has_image = draft
        .background_image_path
        .as_deref()
        .is_some_and(|path| !path.trim().is_empty());
    let controls_enabled =
        editor_background_controls_enabled(draft.background_image_enabled, has_image);

    egui::Grid::new("settings_appearance_editor_background_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Enabled");
            settings_switch(
                ui,
                &mut draft.background_image_enabled,
                "Editor background image",
            )
            .on_hover_text("Editor background image");
            ui.end_row();

            ui.label("Image");
            render_background_image_file_picker(
                ui,
                draft.background_image_path.as_deref(),
                choose_background_image,
                clear_background_image,
            );
            ui.end_row();

            ui.label("Area");
            ui.add_enabled_ui(controls_enabled, |ui| {
                egui::ComboBox::from_id_salt("settings_appearance_editor_background_scope")
                    .selected_text(editor_background_image_scope_label(
                        draft.background_image_scope,
                    ))
                    .width(bounded_settings_text_edit_width(
                        ui.available_width(),
                        180.0,
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.background_image_scope,
                            EditorBackgroundImageScope::Editor,
                            "Editor only",
                        );
                        ui.selectable_value(
                            &mut draft.background_image_scope,
                            EditorBackgroundImageScope::FullApp,
                            "Full app",
                        );
                    });
            });
            ui.end_row();

            ui.label("Dim");
            ui.add_enabled_ui(controls_enabled, |ui| {
                let mut percent =
                    clamp_editor_background_image_dim(draft.background_image_dim) * 100.0;
                if ui
                    .add(
                        egui::Slider::new(&mut percent, 0.0..=100.0)
                            .suffix("%")
                            .step_by(1.0),
                    )
                    .changed()
                {
                    draft.background_image_dim = percent / 100.0;
                }
            });
            ui.end_row();

            ui.label("Scaling");
            ui.add_enabled_ui(controls_enabled, |ui| {
                egui::ComboBox::from_id_salt("settings_appearance_editor_background_fit")
                    .selected_text(editor_background_image_fit_label(
                        draft.background_image_fit,
                    ))
                    .width(bounded_settings_text_edit_width(
                        ui.available_width(),
                        180.0,
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.background_image_fit,
                            EditorBackgroundImageFit::Cover,
                            "Cover",
                        );
                        ui.selectable_value(
                            &mut draft.background_image_fit,
                            EditorBackgroundImageFit::Contain,
                            "Contain",
                        );
                        ui.selectable_value(
                            &mut draft.background_image_fit,
                            EditorBackgroundImageFit::Stretch,
                            "Stretch",
                        );
                    });
            });
            ui.end_row();

            ui.label("Vertical position");
            ui.add_enabled_ui(controls_enabled, |ui| {
                egui::ComboBox::from_id_salt("settings_appearance_editor_background_position")
                    .selected_text(editor_background_image_position_label(
                        draft.background_image_position,
                    ))
                    .width(bounded_settings_text_edit_width(
                        ui.available_width(),
                        180.0,
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.background_image_position,
                            EditorBackgroundImagePosition::Top,
                            "Top",
                        );
                        ui.selectable_value(
                            &mut draft.background_image_position,
                            EditorBackgroundImagePosition::Center,
                            "Center",
                        );
                        ui.selectable_value(
                            &mut draft.background_image_position,
                            EditorBackgroundImagePosition::Bottom,
                            "Bottom",
                        );
                    });
            });
            ui.end_row();
        });
}

fn render_background_image_file_picker(
    ui: &mut egui::Ui,
    current: Option<&str>,
    choose_file: &mut bool,
    clear_file: &mut bool,
) {
    let selected = current.map(str::trim).filter(|path| !path.is_empty());
    let selected_display = selected.map(|path| {
        bounded_settings_display_text(
            path,
            SETTINGS_DISPLAY_TEXT_MAX_CHARS,
            "Editor background image",
        )
    });
    let text = egui::RichText::new(selected_display.as_deref().unwrap_or("No image selected"))
        .monospace()
        .color(if selected.is_some() {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        });

    ui.horizontal(|ui| {
        let button_side = ui.spacing().interact_size.y.max(34.0);
        let actions_width = button_side * 2.0 + ui.spacing().item_spacing.x * 2.0;
        let label_width = (ui.available_width() - actions_width).clamp(48.0, 260.0);
        let response = ui.add_sized(
            [label_width, ui.spacing().interact_size.y],
            egui::Label::new(text).truncate(),
        );
        if let Some(selected_display) = selected_display {
            response.on_hover_text(selected_display);
        }

        if icon_button(ui, IconKind::FolderOpen, "Choose image").clicked() {
            *choose_file = true;
        }
        if ui
            .add_enabled_ui(selected.is_some(), |ui| {
                icon_button(ui, IconKind::Close, "Clear image")
            })
            .inner
            .clicked()
        {
            *clear_file = true;
        }
    });
}

fn editor_background_controls_enabled(enabled: bool, has_image: bool) -> bool {
    enabled && has_image
}

fn editor_background_image_fit_label(fit: EditorBackgroundImageFit) -> &'static str {
    match fit {
        EditorBackgroundImageFit::Cover => "Cover",
        EditorBackgroundImageFit::Contain => "Contain",
        EditorBackgroundImageFit::Stretch => "Stretch",
    }
}

fn editor_background_image_scope_label(scope: EditorBackgroundImageScope) -> &'static str {
    match scope {
        EditorBackgroundImageScope::Editor => "Editor only",
        EditorBackgroundImageScope::FullApp => "Full app",
    }
}

fn editor_background_image_position_label(position: EditorBackgroundImagePosition) -> &'static str {
    match position {
        EditorBackgroundImagePosition::Top => "Top",
        EditorBackgroundImagePosition::Center => "Center",
        EditorBackgroundImagePosition::Bottom => "Bottom",
    }
}

fn render_theme_combo(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    workspace_root: &Path,
    plugin_themes: &PluginThemeRegistry,
    status: &mut Option<String>,
) {
    let custom_options = custom_theme_options(workspace_root, &draft.custom_theme_paths);
    let selected = selected_theme_combo_label(draft, &custom_options);
    let combo_width = bounded_settings_text_edit_width(ui.available_width(), 320.0);

    egui::ComboBox::from_id_salt("settings_appearance_theme_combo")
        .selected_text(selected)
        .width(combo_width)
        .show_ui(ui, |ui| {
            ui.label(egui::RichText::new("Built in").color(ui.visuals().weak_text_color()));
            for theme in built_in_themes() {
                let label = theme_display_label(&theme.name);
                let selected = draft.active_custom_theme_path.is_none()
                    && theme_name_matches(&draft.theme, theme);
                if ui.selectable_label(selected, label.as_str()).clicked() {
                    draft.theme = theme.clone();
                    draft.active_custom_theme_path = None;
                    *status = Some(format!("Selected theme {label} in settings draft"));
                    ui.close();
                }
            }

            if !plugin_themes.themes().is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Plugins").color(ui.visuals().weak_text_color()));
                for registration in plugin_themes.themes() {
                    render_plugin_theme_combo_item(ui, draft, registration, status);
                }
            }

            if !custom_options.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new("Custom files").color(ui.visuals().weak_text_color()));
                for option in custom_options {
                    render_custom_theme_combo_item(ui, draft, option, status);
                }
            }
        });
}

fn render_plugin_theme_combo_item(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    registration: &PluginThemeRegistration,
    status: &mut Option<String>,
) {
    let label = plugin_theme_display_label_bounded(registration);
    let selected =
        draft.active_custom_theme_path.is_none() && draft.theme.name.trim() == label.trim();

    if ui.selectable_label(selected, label.as_str()).clicked() {
        match load_plugin_theme_settings(registration) {
            Ok(mut theme) => {
                theme.name = label.clone();
                draft.theme = theme;
                draft.active_custom_theme_path = None;
                *status = Some(format!("Selected theme {label} in settings draft"));
                ui.close();
            }
            Err(error) => {
                let error = error.to_string();
                let error = display_error_label_cow(&error);
                *status = Some(format!("Could not load theme {label}: {}", error.as_ref()));
            }
        }
    }
}

fn render_custom_theme_combo_item(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    option: CustomThemeOption,
    status: &mut Option<String>,
) {
    let selected = draft
        .active_custom_theme_path
        .as_deref()
        .is_some_and(|active| active.trim() == option.source);

    let response = ui.selectable_label(selected, option.label.as_str());
    let clicked = response.clicked();
    response.on_hover_text(option.reference.as_str());

    if clicked {
        match load_theme_settings_from_path(&option.path) {
            Ok(mut theme) => {
                if theme.name.trim().is_empty() {
                    theme.name = option.label.clone();
                }
                draft.theme = theme;
                draft.active_custom_theme_path = Some(option.source.clone());
                *status = Some(format!(
                    "Selected custom theme {} in settings draft",
                    option.label
                ));
                ui.close();
            }
            Err(error) => {
                let error = error.to_string();
                let error = display_error_label_cow(&error);
                *status = Some(format!(
                    "Could not load custom theme {}: {}",
                    option.label,
                    error.as_ref()
                ));
            }
        }
    }
}

fn selected_theme_combo_label(
    draft: &EditorSettings,
    custom_options: &[CustomThemeOption],
) -> String {
    if let Some(active) = draft.active_custom_theme_path.as_deref() {
        let active = active.trim();
        if let Some(option) = custom_options
            .iter()
            .find(|option| option.source.as_str() == active)
        {
            return format!("Custom: {}", option.label);
        }
    }

    theme_display_label(&draft.theme.name)
}

fn theme_name_matches(left: &ThemeSettings, right: &ThemeSettings) -> bool {
    left.name.trim().eq_ignore_ascii_case(right.name.trim())
}

fn render_custom_theme_paths(ui: &mut egui::Ui, draft: &mut EditorSettings) {
    let mut value =
        bounded_settings_multiline_join(draft.custom_theme_paths.iter().map(String::as_str));
    let response = ui.add_sized(
        [
            bounded_settings_text_edit_width(ui.available_width(), 360.0),
            72.0,
        ],
        egui::TextEdit::multiline(&mut value)
            .desired_rows(3)
            .hint_text(".kuroya/themes/night.toml"),
    );
    if response.changed() {
        draft.custom_theme_paths = parse_custom_theme_paths_input(&value);
    }
}

fn render_theme_format_help(ui: &mut egui::Ui) {
    ui.collapsing("Theme file format", |ui| {
        ui.label("Add a TOML file path above, then select it from the theme dropdown.");
        ui.label("Colors accept #RRGGBB, #RGB, or [r, g, b].");
        let mut example = THEME_CONFIG_EXAMPLE.to_owned();
        ui.add_sized(
            [
                bounded_settings_text_edit_width(ui.available_width(), 420.0),
                176.0,
            ],
            egui::TextEdit::multiline(&mut example)
                .font(egui::TextStyle::Monospace)
                .desired_rows(11)
                .interactive(false),
        );
    });
}

fn custom_theme_options(workspace_root: &Path, raw_paths: &[String]) -> Vec<CustomThemeOption> {
    raw_paths
        .iter()
        .filter_map(|raw_path| custom_theme_option(workspace_root, raw_path))
        .collect()
}

fn custom_theme_option(workspace_root: &Path, raw_path: &str) -> Option<CustomThemeOption> {
    let source = raw_path.trim();
    if source.is_empty() {
        return None;
    }

    let raw_path = Path::new(source);
    let path = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else {
        workspace_root.join(raw_path)
    };
    let label = custom_theme_label(&path, source);
    let reference = custom_theme_reference(&path);

    Some(CustomThemeOption {
        source: source.to_owned(),
        path,
        label,
        reference,
    })
}

fn custom_theme_label(path: &Path, fallback: &str) -> String {
    let label = path
        .file_stem()
        .or_else(|| path.file_name())
        .and_then(|label| label.to_str())
        .unwrap_or(fallback);
    sanitized_display_label_cow(label, THEME_DISPLAY_LABEL_MAX_CHARS, "Custom theme").into_owned()
}

fn custom_theme_reference(path: &Path) -> String {
    let label = path.display().to_string();
    sanitized_display_label_cow(&label, THEME_REFERENCE_LABEL_MAX_CHARS, "Theme file").into_owned()
}

fn parse_custom_theme_paths_input(value: &str) -> Vec<String> {
    value
        .lines()
        .take(MAX_CUSTOM_THEME_PATHS)
        .map(|line| line.chars().take(MAX_CUSTOM_THEME_PATH_CHARS).collect())
        .collect()
}

fn render_font_file_picker(
    ui: &mut egui::Ui,
    current: &str,
    choose_file: &mut bool,
    clear_file: &mut bool,
) {
    let selected = current.trim();
    let has_selection = !selected.is_empty();
    let selected_display = bounded_settings_display_text(
        selected,
        SETTINGS_DISPLAY_TEXT_MAX_CHARS,
        "Custom font file",
    );
    let label = if has_selection {
        selected_display.as_str()
    } else {
        "Use bundled font"
    };
    let text = egui::RichText::new(label)
        .monospace()
        .color(if has_selection {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        });

    ui.horizontal(|ui| {
        let label_width = (ui.available_width() - 168.0).clamp(96.0, 260.0);
        let response = ui.add_sized(
            [label_width, ui.spacing().interact_size.y],
            egui::Label::new(text).truncate(),
        );
        if has_selection {
            response.on_hover_text(selected_display);
        }

        if popup_compact_button(ui, "Choose", PopupButtonKind::Primary).clicked() {
            *choose_file = true;
        }

        if popup_compact_button_enabled(ui, has_selection, "Clear", PopupButtonKind::Secondary)
            .clicked()
        {
            *clear_file = true;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        custom_theme_option, editor_background_controls_enabled, editor_background_image_fit_label,
        editor_background_image_position_label, editor_background_image_scope_label,
        parse_custom_theme_paths_input,
    };
    use kuroya_core::{
        EditorBackgroundImageFit, EditorBackgroundImagePosition, EditorBackgroundImageScope,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn custom_theme_paths_parse_preserves_raw_lines_for_apply() {
        let paths = parse_custom_theme_paths_input(" .kuroya/themes/a.toml \n\nb.toml ");

        assert_eq!(paths, [" .kuroya/themes/a.toml ", "", "b.toml "]);
    }

    #[test]
    fn custom_theme_paths_parse_caps_live_draft_size() {
        let input = (0..300)
            .map(|index| format!("{}\n", format!("theme-{index}-").repeat(500)))
            .collect::<String>();
        let paths = parse_custom_theme_paths_input(&input);

        assert_eq!(paths.len(), 256);
        assert!(paths.iter().all(|path| path.chars().count() <= 4096));
    }

    #[test]
    fn custom_theme_option_resolves_relative_paths_against_workspace() {
        let workspace = PathBuf::from("workspace");
        let option = custom_theme_option(&workspace, " .kuroya/themes/night.toml ").unwrap();

        assert_eq!(option.source, ".kuroya/themes/night.toml");
        assert_eq!(
            option.path,
            Path::new("workspace").join(".kuroya/themes/night.toml")
        );
        assert_eq!(option.label, "night");
    }

    #[test]
    fn editor_background_controls_require_an_enabled_image() {
        assert!(!editor_background_controls_enabled(false, false));
        assert!(!editor_background_controls_enabled(false, true));
        assert!(!editor_background_controls_enabled(true, false));
        assert!(editor_background_controls_enabled(true, true));
    }

    #[test]
    fn editor_background_combo_labels_cover_all_values() {
        assert_eq!(
            editor_background_image_fit_label(EditorBackgroundImageFit::Cover),
            "Cover"
        );
        assert_eq!(
            editor_background_image_fit_label(EditorBackgroundImageFit::Contain),
            "Contain"
        );
        assert_eq!(
            editor_background_image_fit_label(EditorBackgroundImageFit::Stretch),
            "Stretch"
        );
        assert_eq!(
            editor_background_image_scope_label(EditorBackgroundImageScope::Editor),
            "Editor only"
        );
        assert_eq!(
            editor_background_image_scope_label(EditorBackgroundImageScope::FullApp),
            "Full app"
        );
        assert_eq!(
            editor_background_image_position_label(EditorBackgroundImagePosition::Top),
            "Top"
        );
        assert_eq!(
            editor_background_image_position_label(EditorBackgroundImagePosition::Center),
            "Center"
        );
        assert_eq!(
            editor_background_image_position_label(EditorBackgroundImagePosition::Bottom),
            "Bottom"
        );
    }
}

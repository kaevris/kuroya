use crate::{
    preference_panels::sections::{
        SETTINGS_TARGET_LSP, SettingsHighlightState, bounded_settings_text_edit_width,
        bounded_singleline_text_edit, bounded_singleline_text_edit_with_hint,
        settings_target_heading,
    },
    ui_icons::{IconKind, icon_button},
};
use eframe::egui;
use kuroya_core::{
    EditorGotoLocationMultiple, EditorLightbulbMode, EditorRenderValidationDecorations,
    EditorSettings, LspServerConfig, MAX_EDITOR_CODE_LENS_FONT_SIZE,
    MAX_EDITOR_INLAY_HINTS_FONT_SIZE, MAX_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH, MAX_HOVER_DELAY_MS,
    MAX_HOVER_HIDING_DELAY_MS, MIN_EDITOR_CODE_LENS_FONT_SIZE, MIN_EDITOR_INLAY_HINTS_FONT_SIZE,
    MIN_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH, MIN_HOVER_DELAY_MS, MIN_HOVER_HIDING_DELAY_MS,
};

pub(super) fn render_lsp_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(ui, highlight, SETTINGS_TARGET_LSP, "LSP");
    egui::Grid::new("settings_lsp_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("LSP servers");
            render_lsp_servers(ui, &mut draft.lsp_servers);
            ui.end_row();

            ui.label("Hover");
            ui.checkbox(&mut draft.hover_enabled, "Enable LSP hover requests");
            ui.end_row();

            ui.label("Hover delay");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.hover_delay_ms)
                        .speed(25.0)
                        .range(MIN_HOVER_DELAY_MS..=MAX_HOVER_DELAY_MS),
                );
                ui.label("ms");
            });
            ui.end_row();

            ui.label("Hover hiding delay");
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut draft.hover_hiding_delay_ms)
                        .speed(25.0)
                        .range(MIN_HOVER_HIDING_DELAY_MS..=MAX_HOVER_HIDING_DELAY_MS),
                );
                ui.label("ms");
            });
            ui.end_row();

            ui.label("Hover behavior");
            ui.vertical(|ui| {
                ui.checkbox(&mut draft.hover_sticky, "Keep hover open under mouse");
                ui.checkbox(&mut draft.hover_above, "Prefer hover above the line");
                ui.checkbox(
                    &mut draft.hover_show_long_line_warning,
                    "Show long line warning hovers",
                );
            });
            ui.end_row();

            ui.label("Lightbulb");
            editor_lightbulb_combo(ui, "editor_lightbulb", &mut draft.lightbulb);
            ui.end_row();

            ui.label("Validation decorations");
            editor_render_validation_decorations_combo(
                ui,
                "editor_render_validation_decorations",
                &mut draft.render_validation_decorations,
            );
            ui.end_row();

            ui.label("Document highlights");
            ui.checkbox(
                &mut draft.document_highlights_enabled,
                "Highlight symbol references",
            );
            ui.end_row();

            ui.label("Code lens");
            ui.checkbox(&mut draft.code_lens, "Show inline code lens actions");
            ui.end_row();

            ui.label("Code lens font");
            bounded_singleline_text_edit_with_hint(
                ui,
                &mut draft.code_lens_font_family,
                220.0,
                Some("Editor font"),
            );
            ui.end_row();

            ui.label("Code lens font size");
            ui.add(
                egui::DragValue::new(&mut draft.code_lens_font_size)
                    .speed(1.0)
                    .range(MIN_EDITOR_CODE_LENS_FONT_SIZE..=MAX_EDITOR_CODE_LENS_FONT_SIZE),
            )
            .on_hover_text("Use 0 for 90% of the editor font size");
            ui.end_row();

            ui.label("Go to definitions");
            editor_goto_location_multiple_combo(
                ui,
                "editor_goto_location_multiple_definitions",
                &mut draft.goto_location_multiple_definitions,
            );
            ui.end_row();

            ui.label("Go to type definitions");
            editor_goto_location_multiple_combo(
                ui,
                "editor_goto_location_multiple_type_definitions",
                &mut draft.goto_location_multiple_type_definitions,
            );
            ui.end_row();

            ui.label("Go to declarations");
            editor_goto_location_multiple_combo(
                ui,
                "editor_goto_location_multiple_declarations",
                &mut draft.goto_location_multiple_declarations,
            );
            ui.end_row();

            ui.label("Go to implementations");
            editor_goto_location_multiple_combo(
                ui,
                "editor_goto_location_multiple_implementations",
                &mut draft.goto_location_multiple_implementations,
            );
            ui.end_row();

            ui.label("Go to references");
            editor_goto_location_multiple_combo(
                ui,
                "editor_goto_location_multiple_references",
                &mut draft.goto_location_multiple_references,
            );
            ui.end_row();

            ui.label("Go to tests");
            editor_goto_location_multiple_combo(
                ui,
                "editor_goto_location_multiple_tests",
                &mut draft.goto_location_multiple_tests,
            );
            ui.end_row();

            ui.label("Alt definition command");
            bounded_singleline_text_edit(
                ui,
                &mut draft.goto_location_alternative_definition_command,
                260.0,
            );
            ui.end_row();

            ui.label("Alt type definition command");
            bounded_singleline_text_edit(
                ui,
                &mut draft.goto_location_alternative_type_definition_command,
                260.0,
            );
            ui.end_row();

            ui.label("Alt declaration command");
            bounded_singleline_text_edit(
                ui,
                &mut draft.goto_location_alternative_declaration_command,
                260.0,
            );
            ui.end_row();

            ui.label("Alt implementation command");
            bounded_singleline_text_edit(
                ui,
                &mut draft.goto_location_alternative_implementation_command,
                260.0,
            );
            ui.end_row();

            ui.label("Alt reference command");
            bounded_singleline_text_edit(
                ui,
                &mut draft.goto_location_alternative_reference_command,
                260.0,
            );
            ui.end_row();

            ui.label("Alt tests command");
            bounded_singleline_text_edit(
                ui,
                &mut draft.goto_location_alternative_tests_command,
                260.0,
            );
            ui.end_row();

            ui.label("Inlay hints");
            ui.checkbox(
                &mut draft.inlay_hints,
                "Show inline type and parameter hints",
            );
            ui.end_row();

            ui.label("Inlay hint font");
            bounded_singleline_text_edit_with_hint(
                ui,
                &mut draft.inlay_hints_font_family,
                220.0,
                Some("Editor font"),
            );
            ui.end_row();

            ui.label("Inlay hint font size");
            ui.add(
                egui::DragValue::new(&mut draft.inlay_hints_font_size)
                    .speed(1.0)
                    .range(MIN_EDITOR_INLAY_HINTS_FONT_SIZE..=MAX_EDITOR_INLAY_HINTS_FONT_SIZE),
            )
            .on_hover_text("Use 0 to follow the editor font size");
            ui.end_row();

            ui.label("Inlay hint max length");
            ui.add(
                egui::DragValue::new(&mut draft.inlay_hints_maximum_length)
                    .speed(1.0)
                    .range(
                        MIN_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH
                            ..=MAX_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH,
                    ),
            )
            .on_hover_text("Use 0 to never truncate");
            ui.end_row();

            ui.label("Inlay hint padding");
            ui.checkbox(&mut draft.inlay_hints_padding, "Pad inlay hint labels");
            ui.end_row();

            ui.label("Parameter hints");
            ui.checkbox(
                &mut draft.parameter_hints_enabled,
                "Enable LSP signature help",
            );
            ui.end_row();

            ui.label("Parameter hint triggers");
            ui.checkbox(
                &mut draft.parameter_hints_on_trigger_characters,
                "Show hints after trigger characters",
            );
            ui.end_row();

            ui.label("Parameter hint cycle");
            ui.checkbox(
                &mut draft.parameter_hints_cycle,
                "Cycle parameter hints at the end of the list",
            );
            ui.end_row();
        });
}

fn render_lsp_servers(ui: &mut egui::Ui, servers: &mut Vec<LspServerConfig>) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(lsp_servers_summary(servers.len()))
                    .color(ui.visuals().weak_text_color()),
            )
            .on_hover_text("Add a row to override a built-in server or register a custom one");
            if icon_button(ui, IconKind::Plus, "Add custom LSP server").clicked() {
                servers.push(empty_lsp_server_config());
            }
        });

        let mut remove_server = None;
        for (index, server) in servers.iter_mut().enumerate() {
            if index > 0 {
                ui.add_space(4.0);
            }
            ui.push_id(("lsp_server", index), |ui| {
                let mut remove_current = false;
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Server {}", index + 1)).strong());
                        if icon_button(ui, IconKind::Trash, "Remove LSP server").clicked() {
                            remove_current = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Language");
                        bounded_singleline_text_edit_with_hint(
                            ui,
                            &mut server.language,
                            96.0,
                            Some("rust"),
                        )
                        .on_hover_text(
                            "Use an editor language ID, for example rust, python, typescript, php, ruby, go, csharp, or shellscript",
                        );
                        ui.label("Command");
                        let command_width =
                            bounded_settings_text_edit_width(ui.available_width(), 220.0);
                        bounded_singleline_text_edit_with_hint(
                            ui,
                            &mut server.command,
                            command_width,
                            Some("rust-analyzer"),
                        )
                        .on_hover_text("Server command name or executable path");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Args")
                            .on_hover_text("Arguments passed to the server command");
                        render_lsp_string_items(ui, &mut server.args, "argument");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Extensions")
                            .on_hover_text("File extensions that should use this language ID");
                        render_lsp_string_items(ui, &mut server.extensions, "gleam");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Root markers")
                            .on_hover_text("Files or directories used to identify project roots");
                        render_lsp_string_items(ui, &mut server.root_markers, "Cargo.toml");
                    });
                });
                if remove_current {
                    remove_server = Some(index);
                }
            });
        }

        if let Some(index) = remove_server {
            servers.remove(index);
        }
    });
}

fn render_lsp_string_items(ui: &mut egui::Ui, values: &mut Vec<String>, hint_text: &'static str) {
    ui.vertical(|ui| {
        let mut remove_item = None;
        for (index, value) in values.iter_mut().enumerate() {
            ui.push_id(("lsp_string_item", index), |ui| {
                ui.horizontal(|ui| {
                    let edit_width = bounded_settings_text_edit_width(
                        ui.available_width() - settings_icon_button_width(ui),
                        260.0,
                    );
                    bounded_singleline_text_edit_with_hint(ui, value, edit_width, Some(hint_text));
                    if icon_button(ui, IconKind::Trash, "Remove item").clicked() {
                        remove_item = Some(index);
                    }
                });
            });
        }
        if let Some(index) = remove_item {
            values.remove(index);
        }
        if icon_button(ui, IconKind::Plus, "Add item").clicked() {
            values.push(String::new());
        }
    });
}

fn settings_icon_button_width(ui: &egui::Ui) -> f32 {
    ui.spacing().interact_size.y.max(34.0) + ui.spacing().item_spacing.x
}

fn lsp_servers_summary(count: usize) -> String {
    match count {
        0 => "Built-in defaults".to_owned(),
        1 => "1 custom server".to_owned(),
        count => format!("{count} custom servers"),
    }
}

fn empty_lsp_server_config() -> LspServerConfig {
    LspServerConfig {
        language: String::new(),
        command: String::new(),
        args: Vec::new(),
        extensions: Vec::new(),
        root_markers: Vec::new(),
    }
}

fn editor_lightbulb_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorLightbulbMode,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_lightbulb_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorLightbulbMode::Off, "Off");
            ui.selectable_value(value, EditorLightbulbMode::On, "On");
            ui.selectable_value(value, EditorLightbulbMode::OnCode, "On code");
        });
}

fn editor_lightbulb_label(mode: EditorLightbulbMode) -> &'static str {
    match mode {
        EditorLightbulbMode::Off => "Off",
        EditorLightbulbMode::On => "On",
        EditorLightbulbMode::OnCode => "On code",
    }
}

fn editor_goto_location_multiple_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorGotoLocationMultiple,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_goto_location_multiple_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorGotoLocationMultiple::Peek, "Peek");
            ui.selectable_value(
                value,
                EditorGotoLocationMultiple::GotoAndPeek,
                "Go to and peek",
            );
            ui.selectable_value(value, EditorGotoLocationMultiple::Goto, "Go to");
        });
}

fn editor_goto_location_multiple_label(mode: EditorGotoLocationMultiple) -> &'static str {
    match mode {
        EditorGotoLocationMultiple::Peek => "Peek",
        EditorGotoLocationMultiple::GotoAndPeek => "Go to and peek",
        EditorGotoLocationMultiple::Goto => "Go to",
    }
}

fn editor_render_validation_decorations_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut EditorRenderValidationDecorations,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(editor_render_validation_decorations_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, EditorRenderValidationDecorations::Off, "Off");
            ui.selectable_value(
                value,
                EditorRenderValidationDecorations::Editable,
                "Editable",
            );
            ui.selectable_value(value, EditorRenderValidationDecorations::On, "On");
        });
}

fn editor_render_validation_decorations_label(
    mode: EditorRenderValidationDecorations,
) -> &'static str {
    match mode {
        EditorRenderValidationDecorations::Off => "Off",
        EditorRenderValidationDecorations::Editable => "Editable",
        EditorRenderValidationDecorations::On => "On",
    }
}

use crate::{
    preference_panels::sections::{
        SETTINGS_TARGET_LSP, SettingsHighlightState, bounded_settings_text_edit_width,
        bounded_singleline_text_edit, bounded_singleline_text_edit_with_hint,
        settings_target_heading,
    },
    ui_icons::{IconKind, icon_button},
    ui_switch::ui_switch,
};
use eframe::egui;
use kuroya_core::{
    EditorGotoLocationMultiple, EditorLightbulbMode, EditorRenderValidationDecorations,
    EditorSettings, LspServerConfig, MAX_EDITOR_CODE_LENS_FONT_SIZE,
    MAX_EDITOR_INLAY_HINTS_FONT_SIZE, MAX_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH, MAX_HOVER_DELAY_MS,
    MAX_HOVER_HIDING_DELAY_MS, MIN_EDITOR_CODE_LENS_FONT_SIZE, MIN_EDITOR_INLAY_HINTS_FONT_SIZE,
    MIN_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH, MIN_HOVER_DELAY_MS, MIN_HOVER_HIDING_DELAY_MS,
    default_lsp_server_for_language, lsp_server_matches_builtin, missing_builtin_lsp_servers,
};

pub(super) fn render_lsp_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(ui, highlight, SETTINGS_TARGET_LSP, "LSP");
    render_lsp_servers(ui, &mut draft.lsp_servers);

    egui::Grid::new("settings_lsp_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Suggest missing servers");
            ui_switch(ui, &mut draft.lsp_suggest_missing_servers).on_hover_text(
                "When a file opens whose built-in language server is disabled, offer to enable it",
            );
            ui.end_row();

            ui.label("Hover");
            ui_switch(ui, &mut draft.hover_enabled).on_hover_text("Enable LSP hover requests");
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
                ui.horizontal(|ui| {
                    let r = ui_switch(ui, &mut draft.hover_sticky);
                    ui.label(
                        egui::RichText::new("Keep hover open under mouse")
                            .small()
                            .weak(),
                    );
                    r
                });
                ui.horizontal(|ui| {
                    let r = ui_switch(ui, &mut draft.hover_above);
                    ui.label(
                        egui::RichText::new("Prefer hover above the line")
                            .small()
                            .weak(),
                    );
                    r
                });
                ui.horizontal(|ui| {
                    let r = ui_switch(ui, &mut draft.hover_show_long_line_warning);
                    ui.label(
                        egui::RichText::new("Show long line warning hovers")
                            .small()
                            .weak(),
                    );
                    r
                });
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
            ui_switch(ui, &mut draft.document_highlights_enabled)
                .on_hover_text("Highlight symbol references");
            ui.end_row();

            ui.label("Code lens");
            ui_switch(ui, &mut draft.code_lens).on_hover_text("Show inline code lens actions");
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
            ui_switch(ui, &mut draft.inlay_hints)
                .on_hover_text("Show inline type and parameter hints");
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
            ui_switch(ui, &mut draft.inlay_hints_padding).on_hover_text("Pad inlay hint labels");
            ui.end_row();

            ui.label("Parameter hints");
            ui_switch(ui, &mut draft.parameter_hints_enabled)
                .on_hover_text("Enable LSP signature help");
            ui.end_row();

            ui.label("Parameter hint triggers");
            ui_switch(ui, &mut draft.parameter_hints_on_trigger_characters)
                .on_hover_text("Show hints after trigger characters");
            ui.end_row();

            ui.label("Parameter hint cycle");
            ui_switch(ui, &mut draft.parameter_hints_cycle)
                .on_hover_text("Cycle parameter hints at the end of the list");
            ui.end_row();
        });
}

fn render_lsp_servers(ui: &mut egui::Ui, servers: &mut Vec<LspServerConfig>) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("LSP servers").strong());
        ui.label(
            egui::RichText::new(lsp_servers_summary(servers))
                .small()
                .color(ui.visuals().weak_text_color()),
        )
        .on_hover_text("Built-in servers can be edited, reset, or removed like custom ones");
        let missing_defaults = missing_builtin_lsp_servers(servers);
        if !missing_defaults.is_empty() {
            let restore_tooltip = format!(
                "Restore built-in LSP server ({})",
                missing_defaults
                    .iter()
                    .map(|server| server.language.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if icon_button(ui, IconKind::Restore, &restore_tooltip)
                .on_hover_text("Add back built-in servers that were removed from the list")
                .clicked()
            {
                servers.extend(missing_defaults);
            }
        }
        if icon_button(ui, IconKind::Plus, "Add custom LSP server").clicked() {
            servers.push(empty_lsp_server_config());
        }
    });

    let mut remove_server = None;
    let mut reset_server = None;
    for (index, server) in servers.iter_mut().enumerate() {
        ui.push_id(("lsp_server", index), |ui| {
            let badge = server_badge(server);
            let collapsing_id = ui.make_persistent_id("collapsing");
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                collapsing_id,
                false,
            );
            let header = state.show_header(ui, |ui| {
                ui_switch(ui, &mut server.enabled).on_hover_text(if server.enabled {
                    "Server is enabled; disable it to keep its settings but stop using it"
                } else {
                    "Server is disabled; switch it on to use it again"
                });
                ui.label(if server.enabled {
                    egui::RichText::new(server_label(index, server)).strong()
                } else {
                    egui::RichText::new(server_label(index, server)).weak()
                });
                if let Some(badge) = badge {
                    ui.label(
                        egui::RichText::new(badge)
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                if badge == Some(LSP_BADGE_EDITED)
                    && icon_button(
                        ui,
                        IconKind::Refresh,
                        "Reset this server to the built-in defaults",
                    )
                    .clicked()
                {
                    reset_server = Some(index);
                }
                if icon_button(ui, IconKind::Trash, "Remove LSP server").clicked() {
                    remove_server = Some(index);
                }
            });
            header.body(|ui| {
                ui.add_space(2.0);
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
            ui.add_space(2.0);
        });
    }

    if let Some(index) = remove_server {
        servers.remove(index);
    }
    if let Some(index) = reset_server {
        if let Some(default) = default_lsp_server_for_language(&servers[index].language) {
            servers[index] = default;
        }
    }
}

const LSP_BADGE_BUILTIN: &str = "built-in";
const LSP_BADGE_EDITED: &str = "edited";

fn server_label(index: usize, server: &LspServerConfig) -> String {
    if server.language.is_empty() {
        format!("Server {}", index + 1)
    } else {
        format!("Server {} \u{00b7} {}", index + 1, server.language)
    }
}

fn server_badge(server: &LspServerConfig) -> Option<&'static str> {
    if lsp_server_matches_builtin(server) {
        Some(LSP_BADGE_BUILTIN)
    } else if default_lsp_server_for_language(&server.language).is_some() {
        Some(LSP_BADGE_EDITED)
    } else {
        None
    }
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

fn lsp_servers_summary(servers: &[LspServerConfig]) -> String {
    let builtin = servers
        .iter()
        .filter(|server| lsp_server_matches_builtin(server))
        .count();
    let disabled = servers.iter().filter(|server| !server.enabled).count();
    let base = match servers.len() {
        0 => return "No servers \u{00b7} LSP disabled".to_owned(),
        total if builtin == 0 => format!("{total} custom servers"),
        total if builtin == total => format!("{total} servers, all built-in"),
        total => format!("{total} servers, {builtin} built-in"),
    };
    if disabled > 0 {
        format!("{base}, {disabled} disabled")
    } else {
        base
    }
}

fn empty_lsp_server_config() -> LspServerConfig {
    LspServerConfig {
        language: String::new(),
        command: String::new(),
        args: Vec::new(),
        extensions: Vec::new(),
        root_markers: Vec::new(),
        enabled: true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_server_badges_distinguish_builtin_edited_and_custom() {
        let builtin = default_lsp_server_for_language("rust").expect("rust default should exist");
        assert_eq!(server_badge(&builtin), Some(LSP_BADGE_BUILTIN));

        let mut edited = builtin.clone();
        edited.command = "rust-analyzer-nightly".to_owned();
        assert_eq!(server_badge(&edited), Some(LSP_BADGE_EDITED));

        let custom = empty_lsp_server_config();
        assert_eq!(server_badge(&custom), None);
        let unknown_language = LspServerConfig {
            language: "kuroya-unknown".to_owned(),
            command: "kuroya-lsp".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
            enabled: true,
        };
        assert_eq!(server_badge(&unknown_language), None);
    }

    #[test]
    fn lsp_server_labels_include_language_when_set() {
        let mut server = empty_lsp_server_config();
        assert_eq!(server_label(0, &server), "Server 1");
        server.language = "go".to_owned();
        assert_eq!(server_label(2, &server), "Server 3 \u{00b7} go");
    }

    #[test]
    fn lsp_servers_summary_reports_builtin_and_custom_counts() {
        assert_eq!(lsp_servers_summary(&[]), "No servers \u{00b7} LSP disabled");

        let mut all_enabled = kuroya_core::default_server_configs();
        for server in &mut all_enabled {
            server.enabled = true;
        }
        assert_eq!(
            lsp_servers_summary(&all_enabled),
            format!("{} servers, all built-in", all_enabled.len())
        );

        let mut mixed = kuroya_core::default_server_configs();
        let builtin_before = mixed
            .iter()
            .filter(|server| lsp_server_matches_builtin(server))
            .count();
        let disabled = mixed.iter().filter(|server| !server.enabled).count();
        mixed[0].command = "changed".to_owned();
        mixed.push(empty_lsp_server_config());
        assert_eq!(
            lsp_servers_summary(&mixed),
            format!(
                "{} servers, {} built-in, {disabled} disabled",
                mixed.len(),
                builtin_before - 1
            )
        );

        let customs = vec![empty_lsp_server_config()];
        assert_eq!(lsp_servers_summary(&customs), "1 custom servers");
    }

    #[test]
    fn lsp_servers_summary_reports_disabled_count() {
        let mut servers = kuroya_core::default_server_configs();
        let enabled_index = servers
            .iter()
            .position(|server| server.enabled)
            .expect("defaults should have enabled entries");
        servers[enabled_index].enabled = false;
        servers[enabled_index + 1].enabled = false;
        let total = servers.len();
        let disabled = servers.iter().filter(|server| !server.enabled).count();

        assert_eq!(
            lsp_servers_summary(&servers),
            format!("{total} servers, all built-in, {disabled} disabled")
        );
    }
}

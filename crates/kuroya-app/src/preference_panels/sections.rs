use crate::{
    ui_icon_shapes::draw_icon,
    ui_icons::IconKind,
    ui_state::{clamp_selection, handle_list_navigation_keys, selection_page_step},
    update_checker::DEFAULT_UPDATE_GITHUB_REPOSITORY,
};
use eframe::egui;
use kuroya_core::{EditorSettings, SETTINGS_SCHEMA_VERSION};
use std::ops::RangeInclusive;

mod appearance;
mod discord;
mod editor;
mod files;
mod general;
mod lsp;
mod plugins;
mod scrollbars;
mod terminal;
mod vim;

pub(super) const SETTINGS_SECTION_GENERAL: usize = 0;
pub(super) const SETTINGS_SECTION_EDITOR: usize = 1;
pub(super) const SETTINGS_SECTION_LSP: usize = 2;
pub(super) const SETTINGS_SECTION_VIM: usize = 3;
pub(super) const SETTINGS_SECTION_TERMINAL: usize = 4;
pub(super) const SETTINGS_SECTION_FILES: usize = 5;
pub(super) const SETTINGS_SECTION_APPEARANCE: usize = 6;
pub(super) const SETTINGS_SECTION_SOURCE_CONTROL: usize = 7;
pub(super) const SETTINGS_SECTION_DEVELOPER: usize = 8;
pub(super) const SETTINGS_SECTION_PLUGINS: usize = 9;
pub(super) const SETTINGS_SECTION_DISCORD: usize = 10;
pub(super) const SETTINGS_SECTIONS: [&str; 11] = [
    "General",
    "Editor",
    "LSP",
    "Vim",
    "Terminal",
    "Files",
    "Appearance",
    "Source Control",
    "Developer",
    "Plugins",
    "Discord",
];
const SETTINGS_SECTION_ICONS: [IconKind; SETTINGS_SECTIONS.len()] = [
    IconKind::Settings,
    IconKind::Code,
    IconKind::Lsp,
    IconKind::Keyboard,
    IconKind::Terminal,
    IconKind::File,
    IconKind::Theme,
    IconKind::GitBranch,
    IconKind::Command,
    IconKind::Command,
    IconKind::Command,
];
pub(super) const SETTINGS_DISPLAY_TEXT_MAX_CHARS: usize = 240;
pub(super) const SETTINGS_TEXT_INPUT_MAX_CHARS: usize = 8_192;

pub(super) const SETTINGS_TARGET_GENERAL: &str = "settings.general";
pub(super) const SETTINGS_TARGET_EDITOR_TEXT_LAYOUT: &str = "settings.editor.text_layout";
pub(super) const SETTINGS_TARGET_EDITOR_DISPLAY: &str = "settings.editor.display";
pub(super) const SETTINGS_TARGET_EDITOR_TYPING: &str = "settings.editor.typing";
pub(super) const SETTINGS_TARGET_EDITOR_LANGUAGE: &str = "settings.editor.language";
pub(super) const SETTINGS_TARGET_SCROLLBARS: &str = "settings.scrollbars";
pub(super) const SETTINGS_TARGET_LSP: &str = "settings.lsp";
pub(super) const SETTINGS_TARGET_EDITOR_CURSOR: &str = "settings.editor.cursor";
pub(super) const SETTINGS_TARGET_EDITOR_CODE_VIEW: &str = "settings.editor.code_view";
pub(super) const SETTINGS_TARGET_EDITOR_DIFF: &str = "settings.editor.diff";
pub(super) const SETTINGS_TARGET_SOURCE_CONTROL: &str = "settings.source_control";
pub(super) const SETTINGS_TARGET_EDITOR_SOURCE_CONTROL: &str = SETTINGS_TARGET_SOURCE_CONTROL;
pub(super) const SETTINGS_TARGET_VIM_KEYBINDINGS: &str = "settings.vim.keybindings";
pub(super) const SETTINGS_TARGET_TERMINAL_PROFILE: &str = "settings.terminal.profile";
pub(super) const SETTINGS_TARGET_TERMINAL_BUFFER: &str = "settings.terminal.buffer";
pub(super) const SETTINGS_TARGET_TERMINAL_CURSOR: &str = "settings.terminal.cursor";
pub(super) const SETTINGS_TARGET_TERMINAL_COLOR: &str = "settings.terminal.color";
pub(super) const SETTINGS_TARGET_TERMINAL_INTERACTION: &str = "settings.terminal.interaction";
pub(super) const SETTINGS_TARGET_FILES_SAVE_ACTIONS: &str = "settings.files.save_actions";
pub(super) const SETTINGS_TARGET_FILES_SAVE_CLEANUP: &str = "settings.files.save_cleanup";
pub(super) const SETTINGS_TARGET_APPEARANCE: &str = "settings.appearance";
pub(super) const SETTINGS_TARGET_DEVELOPER: &str = "settings.developer";
pub(super) const SETTINGS_TARGET_PLUGINS: &str = "settings.plugins";
pub(super) const SETTINGS_TARGET_DISCORD: &str = "settings.discord";

pub(super) struct SettingsHighlightState<'a> {
    active_target: Option<&'a str>,
    pending_scroll_target: Option<&'a mut Option<String>>,
}

impl<'a> SettingsHighlightState<'a> {
    pub(super) fn new(
        active_target: Option<&'a str>,
        pending_scroll_target: &'a mut Option<String>,
    ) -> Self {
        Self {
            active_target,
            pending_scroll_target: Some(pending_scroll_target),
        }
    }

    #[cfg(test)]
    pub(super) fn disabled() -> Self {
        Self {
            active_target: None,
            pending_scroll_target: None,
        }
    }

    fn mark_target_rect(&mut self, ui: &mut egui::Ui, target: &str, rect: egui::Rect) {
        let rect = rect.expand2(egui::vec2(6.0, 4.0));
        if self
            .pending_scroll_target
            .as_ref()
            .and_then(|pending| pending.as_deref())
            .is_some_and(|pending| pending == target)
        {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
            if let Some(pending) = self.pending_scroll_target.as_deref_mut() {
                *pending = None;
            }
        }

        if self.active_target.is_some_and(|active| active == target) {
            let warning = ui.visuals().warn_fg_color;
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(4),
                egui::Color32::from_rgba_premultiplied(warning.r(), warning.g(), warning.b(), 28),
            );
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(4),
                egui::Stroke::new(1.0_f32, warning),
                egui::StrokeKind::Inside,
            );
        }
    }
}

pub(super) fn settings_target_heading(
    ui: &mut egui::Ui,
    highlight: &mut SettingsHighlightState<'_>,
    target: &str,
    title: &'static str,
) -> egui::Response {
    let response = ui.label(egui::RichText::new(title).strong());
    highlight.mark_target_rect(ui, target, response.rect);
    response
}

pub(super) fn settings_target_block<R>(
    ui: &mut egui::Ui,
    highlight: &mut SettingsHighlightState<'_>,
    target: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let inner = ui.scope(add_contents);
    highlight.mark_target_rect(ui, target, inner.response.rect);
    inner.inner
}

pub(super) fn render_settings_sidebar(ui: &mut egui::Ui, selected: &mut usize) {
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
    clamp_selection(selected, SETTINGS_SECTIONS.len());
    let width = settings_sidebar_row_width(ui.available_width());
    let row_height = 40.0;
    let focus_id = ui.make_persistent_id("settings-sidebar-keyboard");
    let row_ids: [egui::Id; SETTINGS_SECTIONS.len()] =
        std::array::from_fn(|index| ui.make_persistent_id(("settings-sidebar-section", index)));
    let sidebar_focused = ui.memory(|memory| {
        memory.has_focus(focus_id) || row_ids.iter().any(|id| memory.has_focus(*id))
    });
    let navigation_page_step = selection_page_step(row_height, ui.available_height());
    let request_selected_focus = sidebar_focused
        && ui.input(|input| {
            handle_list_navigation_keys(
                input,
                selected,
                SETTINGS_SECTIONS.len(),
                navigation_page_step,
            )
        });

    for (index, label) in SETTINGS_SECTIONS.iter().enumerate() {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::hover());
        let response = ui.interact(rect, row_ids[index], egui::Sense::click());
        if response.clicked() {
            *selected = index;
            ui.memory_mut(|memory| memory.request_focus(focus_id));
        }
        if request_selected_focus && *selected == index {
            ui.memory_mut(|memory| memory.request_focus(focus_id));
        }
        let is_selected = *selected == index;
        let has_focus = is_selected && (response.has_focus() || sidebar_focused);

        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::SelectableLabel,
                ui.is_enabled(),
                is_selected,
                *label,
            )
        });

        if is_selected || response.hovered() || has_focus {
            let visuals = ui.visuals();
            let fill = if is_selected {
                visuals.widgets.active.weak_bg_fill
            } else if has_focus {
                visuals.widgets.active.bg_fill
            } else {
                visuals.widgets.hovered.bg_fill
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(6), fill);
        }

        let text_color = if is_selected {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        let icon_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 20.0, rect.center().y),
            egui::vec2(20.0, 20.0),
        );
        draw_icon(ui, icon_rect, SETTINGS_SECTION_ICONS[index], text_color);
        ui.painter().text(
            rect.left_center() + egui::vec2(42.0, 0.0),
            egui::Align2::LEFT_CENTER,
            *label,
            egui::TextStyle::Button.resolve(ui.style()),
            text_color,
        );
    }
}

pub(super) fn render_settings_section_picker(ui: &mut egui::Ui, selected: &mut usize) {
    clamp_selection(selected, SETTINGS_SECTIONS.len());
    egui::ComboBox::from_id_salt("settings-section-picker")
        .selected_text(SETTINGS_SECTIONS[*selected])
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (index, label) in SETTINGS_SECTIONS.iter().enumerate() {
                ui.selectable_value(selected, index, *label);
            }
        });
}

pub(super) fn settings_control_row(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    add_control: impl FnOnce(&mut egui::Ui),
) {
    let available_width = ui.available_width().max(0.0);
    if available_width >= 460.0 {
        let row_height = 58.0;
        let control_width = (available_width * 0.28).clamp(120.0, 220.0);
        let gap = ui.spacing().item_spacing.x;
        let (row_rect, _) = ui.allocate_exact_size(
            egui::vec2(available_width, row_height),
            egui::Sense::hover(),
        );
        let control_left = (row_rect.right() - control_width).max(row_rect.left());
        let label_right = (control_left - gap).max(row_rect.left());
        let label_rect = egui::Rect::from_min_max(
            row_rect.left_top(),
            egui::pos2(label_right, row_rect.bottom()),
        );
        let control_rect = egui::Rect::from_min_max(
            egui::pos2(control_left, row_rect.top()),
            row_rect.right_bottom(),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(label_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(title).strong());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(description).color(ui.visuals().weak_text_color()),
                    )
                    .truncate(),
                );
            },
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(control_rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            add_control,
        );
    } else {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(title).strong());
            ui.label(egui::RichText::new(description).color(ui.visuals().weak_text_color()));
            ui.add_space(4.0);
            add_control(ui);
        });
        ui.add_space(8.0);
    }
    ui.add_space(2.0);
}

pub(super) fn settings_toggle_row(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    value: &mut bool,
) {
    settings_control_row(ui, title, description, |ui| {
        settings_switch(ui, value, title).on_hover_text(title);
    });
}

pub(super) fn settings_switch(ui: &mut egui::Ui, value: &mut bool, label: &str) -> egui::Response {
    crate::ui_switch::ui_switch_with_label(ui, value, label)
}

fn settings_sidebar_row_width(available_width: f32) -> f32 {
    available_width.max(0.0)
}

pub(super) fn guarded_f32_drag_value(
    ui: &mut egui::Ui,
    value: &mut f32,
    speed: f64,
    range: RangeInclusive<f32>,
    fallback: f32,
) -> egui::Response {
    let mut display_value = finite_f32_drag_display_value(*value, fallback, range.clone());
    let response = ui.add(
        egui::DragValue::new(&mut display_value)
            .speed(speed)
            .range(range),
    );

    if response.changed() {
        *value = display_value;
    }

    response
}

pub(super) fn finite_f32_drag_display_value(
    value: f32,
    fallback: f32,
    range: RangeInclusive<f32>,
) -> f32 {
    let min = *range.start();
    let max = *range.end();
    let fallback = if fallback.is_finite() { fallback } else { min };
    let display_value = if value.is_finite() { value } else { fallback };
    display_value.clamp(min, max)
}

pub(super) fn bounded_settings_display_text(
    value: &str,
    max_chars: usize,
    fallback: &str,
) -> String {
    let mut display = bounded_settings_text(value, max_chars, false, true);
    if display.is_empty() {
        display = bounded_settings_text(fallback, max_chars, false, true);
    }
    display
}

pub(super) fn bounded_settings_singleline_input(value: &str) -> String {
    bounded_settings_text(value, SETTINGS_TEXT_INPUT_MAX_CHARS, false, false)
}

pub(super) fn bounded_singleline_text_edit(
    ui: &mut egui::Ui,
    value: &mut String,
    desired_width: f32,
) -> egui::Response {
    bounded_singleline_text_edit_with_hint(ui, value, desired_width, None)
}

pub(super) fn bounded_singleline_text_edit_with_hint(
    ui: &mut egui::Ui,
    value: &mut String,
    desired_width: f32,
    hint_text: Option<&'static str>,
) -> egui::Response {
    let mut display_value = bounded_settings_singleline_input(value);
    let normalized_existing_value = display_value != *value;
    let mut text_edit = egui::TextEdit::singleline(&mut display_value)
        .desired_width(desired_width)
        .clip_text(true);
    if let Some(hint_text) = hint_text {
        text_edit = text_edit.hint_text(hint_text);
    }
    let response = ui.add(text_edit);

    if response.changed() || normalized_existing_value {
        *value = display_value;
    }

    response
}

pub(super) fn bounded_settings_multiline_input(value: &str) -> String {
    bounded_settings_text(value, SETTINGS_TEXT_INPUT_MAX_CHARS, true, false)
}

pub(super) fn bounded_settings_multiline_join<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut output = String::new();
    let mut chars = 0usize;
    let mut truncated = false;

    for (index, value) in values.into_iter().enumerate() {
        if index > 0 {
            if chars >= SETTINGS_TEXT_INPUT_MAX_CHARS {
                truncated = true;
                break;
            }
            output.push('\n');
            chars += 1;
        }

        let mut previous_was_cr = false;
        for ch in value.chars() {
            if ch == '\n' && previous_was_cr {
                previous_was_cr = false;
                continue;
            }
            previous_was_cr = ch == '\r';

            if is_settings_format_control(ch) {
                continue;
            }

            let replacement = match ch {
                '\r' | '\n' => '\n',
                '\t' => '\t',
                ch if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') => ' ',
                ch => ch,
            };

            if chars >= SETTINGS_TEXT_INPUT_MAX_CHARS {
                truncated = true;
                break;
            }

            output.push(replacement);
            chars += 1;
        }

        if truncated {
            break;
        }
    }

    if truncated && SETTINGS_TEXT_INPUT_MAX_CHARS > 3 {
        truncate_to_chars(&mut output, SETTINGS_TEXT_INPUT_MAX_CHARS - 3);
        output.push_str("...");
    }

    output
}

pub(super) fn bounded_settings_text_edit_width(available_width: f32, max_width: f32) -> f32 {
    available_width.max(0.0).min(max_width.max(0.0))
}

fn bounded_settings_text(
    value: &str,
    max_chars: usize,
    allow_newlines: bool,
    trim_edges: bool,
) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let mut output = String::with_capacity(value.len().min(max_chars));
    let mut chars = 0usize;
    let mut truncated = false;
    let mut previous_was_cr = false;

    for ch in value.chars() {
        if allow_newlines && ch == '\n' && previous_was_cr {
            previous_was_cr = false;
            continue;
        }
        previous_was_cr = ch == '\r';

        if is_settings_format_control(ch) {
            continue;
        }

        let replacement = match ch {
            '\r' | '\n' if allow_newlines => '\n',
            '\t' if allow_newlines => '\t',
            '\t' => ' ',
            ch if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') => ' ',
            ch => ch,
        };

        if chars >= max_chars {
            truncated = true;
            break;
        }

        output.push(replacement);
        chars += 1;
    }

    if truncated && max_chars > 3 {
        truncate_to_chars(&mut output, max_chars - 3);
        output.push_str("...");
    }

    if trim_edges {
        trim_string_in_place(&mut output);
    }

    output
}

fn truncate_to_chars(value: &mut String, max_chars: usize) {
    if let Some((byte_index, _)) = value.char_indices().nth(max_chars) {
        value.truncate(byte_index);
    }
}

fn trim_string_in_place(value: &mut String) {
    let start = value.len() - value.trim_start().len();
    if start > 0 {
        value.drain(..start);
    }
    let end = value.trim_end().len();
    value.truncate(end);
}

fn is_settings_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}'
    )
}

pub(super) fn render_general_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    general::render_general_settings_with_highlight(ui, draft, highlight);
}

pub(super) fn render_editor_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    editor::render_editor_settings(ui, draft, highlight);
    #[cfg(debug_assertions)]
    if std::env::var("KUROYA_DEBUG_NO_SB").is_ok() {
        return;
    }
    scrollbars::render_scrollbar_settings_with_highlight(ui, draft, highlight);
}

pub(super) fn render_lsp_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    lsp::render_lsp_settings_with_highlight(ui, draft, highlight);
}

pub(super) fn render_source_control_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    editor::render_source_control_settings(ui, draft, highlight);
}

pub(super) fn render_developer_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    settings_target_block(ui, highlight, SETTINGS_TARGET_DEVELOPER, |ui| {
        ui.label(egui::RichText::new("Application").strong());
        egui::Grid::new("settings_developer_application_grid")
            .num_columns(2)
            .spacing([18.0, 10.0])
            .show(ui, |ui| {
                developer_info_row(ui, "Version", env!("CARGO_PKG_VERSION"));
                developer_info_row(ui, "Package", env!("CARGO_PKG_NAME"));
                developer_info_row(ui, "Build", developer_build_profile());
                developer_info_row(ui, "Target", &developer_target_label());
                developer_info_row(ui, "Settings schema", &SETTINGS_SCHEMA_VERSION.to_string());
                developer_info_row(
                    ui,
                    "Update source",
                    &developer_update_source_label(&draft.updates_github_repository),
                );
            });

        ui.add_space(12.0);
        ui.label(egui::RichText::new("Devtools").strong());
        egui::Grid::new("settings_developer_devtools_grid")
            .num_columns(2)
            .spacing([18.0, 10.0])
            .show(ui, |ui| {
                ui.label("Devtools logging");
                settings_switch(
                    ui,
                    &mut draft.devtools_verbose_logging,
                    "Verbose devtools logging",
                );
                ui.end_row();

                ui.label("Devtools profiling");
                ui.add_enabled_ui(cfg!(debug_assertions), |ui| {
                    settings_switch(
                        ui,
                        &mut draft.devtools_profiling_enabled,
                        "Enable profiling",
                    );
                });
                ui.end_row();
            });
    });
}

fn developer_info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.add(
        egui::Label::new(
            egui::RichText::new(value)
                .monospace()
                .small()
                .color(ui.visuals().text_color()),
        )
        .wrap(),
    );
    ui.end_row();
}

fn developer_build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn developer_target_label() -> String {
    format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn developer_update_source_label(repository: &str) -> String {
    let default_label = format!("{DEFAULT_UPDATE_GITHUB_REPOSITORY} (default)");
    bounded_settings_display_text(repository, SETTINGS_DISPLAY_TEXT_MAX_CHARS, &default_label)
}

pub(super) fn render_plugins_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    discovered_plugins: &[kuroya_core::PluginDescriptor],
    highlight: &mut SettingsHighlightState<'_>,
) {
    plugins::render_plugins_settings_with_highlight(ui, draft, discovered_plugins, highlight);
}

pub(super) fn render_vim_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    vim::render_vim_settings(ui, draft, highlight);
}

pub(super) fn vim_key_capture_active(ctx: &egui::Context) -> bool {
    vim::vim_key_capture_active(ctx)
}

pub(super) fn vim_key_capture_clear(ctx: &egui::Context) {
    vim::vim_key_capture_clear(ctx);
}

pub(super) fn render_terminal_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    terminal::render_terminal_settings(ui, draft, highlight);
}

pub(super) fn render_files_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    files::render_files_settings_with_highlight(ui, draft, highlight);
}

pub(super) fn render_discord_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    discord::render_discord_settings(ui, draft, highlight);
}

pub(super) fn render_appearance_settings(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    workspace_root: &std::path::Path,
    plugin_themes: &kuroya_core::PluginThemeRegistry,
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
    appearance::render_appearance_settings_with_highlight(
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
        highlight,
    );
}

#[cfg(test)]
mod tests {
    use super::{
        SETTINGS_DISPLAY_TEXT_MAX_CHARS, SETTINGS_SECTION_DISCORD, SETTINGS_SECTION_EDITOR,
        SETTINGS_SECTION_GENERAL, SETTINGS_SECTION_LSP, SETTINGS_SECTION_PLUGINS,
        SETTINGS_SECTION_VIM, SETTINGS_SECTIONS, SETTINGS_TEXT_INPUT_MAX_CHARS,
        bounded_settings_display_text, bounded_settings_multiline_input,
        bounded_singleline_text_edit, developer_update_source_label, finite_f32_drag_display_value,
        guarded_f32_drag_value, render_settings_sidebar, settings_sidebar_row_width,
    };
    use eframe::egui::{self, Event, Key, Modifiers, RawInput};

    #[test]
    fn settings_sidebar_arrow_keys_move_selection_and_focus() {
        let ctx = egui::Context::default();
        let mut selected = SETTINGS_SECTION_GENERAL;

        assert!(run_settings_sidebar_frame(
            &ctx,
            &mut selected,
            Some(SETTINGS_SECTION_GENERAL),
            Some(Key::ArrowDown),
        ));

        assert_eq!(selected, SETTINGS_SECTION_EDITOR);
        run_settings_sidebar_frame(&ctx, &mut selected, None, Some(Key::ArrowDown));
        assert_eq!(selected, SETTINGS_SECTION_LSP);
        run_settings_sidebar_frame(
            &ctx,
            &mut selected,
            Some(SETTINGS_SECTION_LSP),
            Some(Key::ArrowDown),
        );
        assert_eq!(selected, SETTINGS_SECTION_VIM);
    }

    #[test]
    fn settings_sidebar_ignores_navigation_when_unfocused() {
        let ctx = egui::Context::default();
        let mut selected = SETTINGS_SECTION_GENERAL;

        run_settings_sidebar_frame(&ctx, &mut selected, None, Some(Key::ArrowDown));

        assert_eq!(selected, SETTINGS_SECTION_GENERAL);
    }

    #[test]
    fn settings_sidebar_clamps_restored_section_index() {
        let ctx = egui::Context::default();
        let mut selected = usize::MAX;

        run_settings_sidebar_frame(&ctx, &mut selected, None, None);

        assert_eq!(selected, SETTINGS_SECTION_DISCORD);
    }

    #[test]
    fn settings_sidebar_sections_keep_expected_order() {
        assert_eq!(
            SETTINGS_SECTIONS,
            [
                "General",
                "Editor",
                "LSP",
                "Vim",
                "Terminal",
                "Files",
                "Appearance",
                "Source Control",
                "Developer",
                "Plugins",
                "Discord",
            ]
        );
        assert_eq!(SETTINGS_SECTIONS[SETTINGS_SECTION_LSP], "LSP");
        assert_eq!(SETTINGS_SECTIONS[SETTINGS_SECTION_VIM], "Vim");
        assert_eq!(SETTINGS_SECTIONS[SETTINGS_SECTION_PLUGINS], "Plugins");
        assert_eq!(SETTINGS_SECTIONS[SETTINGS_SECTION_DISCORD], "Discord");
        assert_eq!(SETTINGS_SECTION_DISCORD, SETTINGS_SECTIONS.len() - 1);
    }

    #[test]
    fn settings_sidebar_row_width_does_not_exceed_available_width() {
        assert_eq!(settings_sidebar_row_width(96.0), 96.0);
        assert_eq!(settings_sidebar_row_width(180.0), 180.0);
        assert_eq!(settings_sidebar_row_width(-8.0), 0.0);
    }

    #[test]
    fn settings_display_text_strips_controls_and_bounds() {
        let raw = format!(
            "  src/\u{202e}main.rs\n{}  ",
            "a".repeat(SETTINGS_DISPLAY_TEXT_MAX_CHARS)
        );

        let display = bounded_settings_display_text(&raw, SETTINGS_DISPLAY_TEXT_MAX_CHARS, "empty");

        assert!(!display.contains('\u{202e}'));
        assert!(!display.contains('\n'));
        assert!(display.ends_with("..."));
        assert!(display.chars().count() <= SETTINGS_DISPLAY_TEXT_MAX_CHARS);
    }

    #[test]
    fn settings_display_text_uses_sanitized_fallback_for_blank_values() {
        assert_eq!(
            bounded_settings_display_text("\u{202e}\n", 32, " Bundled\nfont "),
            "Bundled font"
        );
    }

    #[test]
    fn developer_update_source_label_uses_default_for_blank_values() {
        assert_eq!(
            developer_update_source_label("\u{202e}\n"),
            "kaevris/kuroya (default)"
        );
    }

    #[test]
    fn developer_update_source_label_bounds_custom_values() {
        let raw = format!(
            "owner/repo{}\u{202e}\n",
            "a".repeat(SETTINGS_DISPLAY_TEXT_MAX_CHARS)
        );
        let display = developer_update_source_label(&raw);

        assert!(!display.contains('\u{202e}'));
        assert!(!display.contains('\n'));
        assert!(display.ends_with("..."));
        assert!(display.chars().count() <= SETTINGS_DISPLAY_TEXT_MAX_CHARS);
    }

    #[test]
    fn settings_multiline_input_preserves_line_breaks_but_hides_controls() {
        let display = bounded_settings_multiline_input("one\r\ntwo\u{202e}\u{200b}\u{feff}\nthree");

        assert_eq!(display, "one\ntwo\nthree");
    }

    #[test]
    fn settings_multiline_join_bounds_without_losing_visible_raw_lines() {
        let long = "a".repeat(super::SETTINGS_TEXT_INPUT_MAX_CHARS + 80);
        let display = super::bounded_settings_multiline_join([" raw ", "", long.as_str()]);

        assert!(display.starts_with(" raw \n\n"));
        assert!(display.ends_with("..."));
        assert!(display.chars().count() <= super::SETTINGS_TEXT_INPUT_MAX_CHARS);
    }

    #[test]
    fn finite_drag_display_clamps_without_requiring_raw_mutation() {
        assert_eq!(
            finite_f32_drag_display_value(f32::NAN, 13.0, 10.0..=28.0),
            13.0
        );
        assert_eq!(
            finite_f32_drag_display_value(f32::INFINITY, 13.0, 10.0..=28.0),
            13.0
        );
        assert_eq!(
            finite_f32_drag_display_value(-10.0, 13.0, 10.0..=28.0),
            10.0
        );
        assert_eq!(finite_f32_drag_display_value(40.0, 13.0, 10.0..=28.0), 28.0);
        assert_eq!(finite_f32_drag_display_value(15.5, 13.0, 10.0..=28.0), 15.5);
    }

    #[test]
    fn guarded_f32_drag_render_preserves_non_finite_raw_draft() {
        let ctx = egui::Context::default();
        let mut value = f32::NAN;

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                guarded_f32_drag_value(ui, &mut value, 0.25, 10.0..=28.0, 13.0);
            });
        });

        assert!(value.is_nan());
    }

    #[test]
    fn bounded_singleline_text_edit_normalizes_existing_hostile_value() {
        let ctx = egui::Context::default();
        let mut value = format!(
            "font\nfamily\u{202e}{}",
            "x".repeat(SETTINGS_TEXT_INPUT_MAX_CHARS + 64)
        );

        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                bounded_singleline_text_edit(ui, &mut value, 260.0);
            });
        });

        assert!(value.starts_with("font family"));
        assert!(!value.contains('\n'));
        assert!(!value.contains('\u{202e}'));
        assert!(value.ends_with("..."));
        assert!(value.chars().count() <= SETTINGS_TEXT_INPUT_MAX_CHARS);
    }

    fn run_settings_sidebar_frame(
        ctx: &egui::Context,
        selected: &mut usize,
        focus_index: Option<usize>,
        key: Option<Key>,
    ) -> bool {
        let events = key
            .map(|key| Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
            .into_iter()
            .collect();
        let input = RawInput {
            events,
            ..RawInput::default()
        };
        let mut selected_has_focus = false;

        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_min_size(egui::vec2(180.0, 240.0));
                let focus_id = ui.make_persistent_id("settings-sidebar-keyboard");
                if focus_index.is_some() {
                    ui.memory_mut(|memory| memory.request_focus(focus_id));
                }
                render_settings_sidebar(ui, selected);
                selected_has_focus = ui.memory(|memory| memory.has_focus(focus_id));
            });
        });

        selected_has_focus
    }
}

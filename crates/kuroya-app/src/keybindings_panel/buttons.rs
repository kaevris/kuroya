use crate::{
    KuroyaApp,
    keybindings::keybinding_default_chord_for_command,
    keybindings_panel_actions::PendingKeybindingsPanelActions,
    popup_buttons::{PopupButtonKind, popup_button, popup_button_enabled},
    ui_icons::{IconKind, icon_button},
};
use eframe::egui::{RichText, Ui};
use kuroya_core::Command;
use std::fmt::Write;

use super::{KeybindingPanelItem, controls};

pub(super) fn render_keybinding_buttons(
    app: &KuroyaApp,
    ui: &mut Ui,
    items: &[KeybindingPanelItem],
    capturing: bool,
    actions: &mut PendingKeybindingsPanelActions,
) {
    ui.horizontal(|ui| {
        let selected_command =
            controls::command_for_selected_index(app.keybindings_selected, items);
        let selected_bound_command =
            controls::bound_command_for_selected_index(app.keybindings_selected, items);
        let reset_command = selected_command.clone();
        let can_edit = !capturing && selected_command.is_some();
        let can_remove = !capturing && selected_bound_command.is_some();
        let can_reset = !capturing
            && reset_command
                .as_ref()
                .is_some_and(|command| keybinding_reset_is_available(items, command));
        if ui
            .add_enabled_ui(can_edit, |ui| {
                icon_button(
                    ui,
                    IconKind::Keyboard,
                    "Capture shortcut for selected command",
                )
            })
            .inner
            .clicked()
        {
            actions.start_capture = selected_command;
        }
        if popup_button_enabled(ui, can_remove, "Clear", PopupButtonKind::Danger)
            .on_hover_text("Remove the selected command's shortcut")
            .clicked()
        {
            actions.remove_binding = selected_bound_command;
        }
        if popup_button_enabled(ui, can_reset, "Reset", PopupButtonKind::Secondary)
            .on_hover_text("Restore the selected command's default shortcut")
            .clicked()
        {
            actions.reset_binding = reset_command;
        }
        if popup_button(ui, "Open Settings", PopupButtonKind::Secondary).clicked() {
            actions.open_settings = true;
        }
        let count_color = ui.visuals().weak_text_color();
        ui.label(
            RichText::new(keybinding_result_count_label(
                items.len(),
                &app.keybindings_query,
            ))
            .small()
            .color(count_color),
        );
    });
}

fn keybinding_reset_is_available(items: &[KeybindingPanelItem], command: &Command) -> bool {
    let Some(default_chord) = keybinding_default_chord_for_command(command) else {
        return false;
    };
    items
        .iter()
        .find(|item| &item.command == command)
        .is_some_and(|item| item.chord != default_chord)
}

fn keybinding_result_count_label(count: usize, query: &str) -> String {
    let noun = if count == 1 { "command" } else { "commands" };
    let mut label = String::with_capacity(24);
    let _ = write!(label, "{count} {noun}");
    if !query.trim().is_empty() {
        label.push_str(" matched");
    }
    label
}

#[cfg(test)]
mod tests {
    use super::keybinding_result_count_label;

    #[test]
    fn keybinding_result_count_label_reports_filter_context() {
        assert_eq!(keybinding_result_count_label(1, ""), "1 command");
        assert_eq!(keybinding_result_count_label(8, ""), "8 commands");
        assert_eq!(keybinding_result_count_label(1, "git"), "1 command matched");
        assert_eq!(
            keybinding_result_count_label(8, "git"),
            "8 commands matched"
        );
    }
}

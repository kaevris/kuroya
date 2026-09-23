use super::rows::KEYBINDING_ROW_HEIGHT;
use crate::{
    KuroyaApp,
    commands::command_label,
    keybinding_input::CapturedKeybinding,
    keybindings_panel_actions::PendingKeybindingsPanelActions,
    ui_icons::{IconKind, icon_button},
    ui_state::{handle_list_navigation_keys, plain_key_pressed, selection_page_step},
};
use eframe::egui::{self, Key, TextEdit, Ui};
use kuroya_core::Command;

use super::KeybindingPanelItem;

pub(super) fn render_keybinding_controls(
    app: &mut KuroyaApp,
    ui: &mut Ui,
    items: &[KeybindingPanelItem],
    capturing: bool,
    actions: &mut PendingKeybindingsPanelActions,
) -> bool {
    render_capture_or_search(app, ui, actions);
    handle_keyboard_navigation(app, ui, items, capturing, actions)
}

fn render_capture_or_search(
    app: &mut KuroyaApp,
    ui: &mut Ui,
    actions: &mut PendingKeybindingsPanelActions,
) {
    if let Some(label) = app.keybinding_capture_command.as_ref().map(command_label) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label("Capturing shortcut for");
                ui.label(egui::RichText::new(label).strong());
                if icon_button(ui, IconKind::Close, "Cancel shortcut capture").clicked() {
                    actions.captured = Some(CapturedKeybinding::Cancel);
                }
            });
            ui.label(
                egui::RichText::new(
                    "Press a shortcut. Press Esc twice to bind Escape. Text keys require Ctrl, Alt, or Cmd.",
                )
                .small()
                .color(ui.visuals().weak_text_color()),
            );
        });
        return;
    }

    let response = ui.add(
        TextEdit::singleline(&mut app.keybindings_query)
            .hint_text("Search command or shortcut; Enter captures the selected command")
            .desired_width(f32::INFINITY),
    );
    response.request_focus();
    if response.changed() {
        app.keybindings_selected = 0;
    }
}

fn handle_keyboard_navigation(
    app: &mut KuroyaApp,
    ui: &mut Ui,
    items: &[KeybindingPanelItem],
    capturing: bool,
    actions: &mut PendingKeybindingsPanelActions,
) -> bool {
    if capturing {
        return false;
    }

    if ui.input(|input| input.key_pressed(Key::Escape)) {
        actions.close = true;
    }
    let viewport_height = ui.available_height();
    let selection_changed = ui.input(|input| {
        handle_list_navigation_keys(
            input,
            &mut app.keybindings_selected,
            items.len(),
            selection_page_step(KEYBINDING_ROW_HEIGHT, viewport_height),
        )
    });
    if ui.input(|input| input.key_pressed(Key::Enter)) {
        actions.start_capture = selected_command(app, items);
    }
    if ui.input(|input| plain_key_pressed(input, Key::Delete)) {
        actions.remove_binding = selected_bound_command(app, items);
    }
    selection_changed
}

pub(super) fn selected_command(app: &KuroyaApp, items: &[KeybindingPanelItem]) -> Option<Command> {
    command_for_selected_index(app.keybindings_selected, items)
}

pub(super) fn selected_bound_command(
    app: &KuroyaApp,
    items: &[KeybindingPanelItem],
) -> Option<Command> {
    bound_command_for_selected_index(app.keybindings_selected, items)
}

pub(super) fn command_for_selected_index(
    selected_index: usize,
    items: &[KeybindingPanelItem],
) -> Option<Command> {
    items.get(selected_index).map(|item| item.command.clone())
}

pub(super) fn bound_command_for_selected_index(
    selected_index: usize,
    items: &[KeybindingPanelItem],
) -> Option<Command> {
    items
        .get(selected_index)
        .and_then(|item| (!item.chord.is_empty()).then(|| item.command.clone()))
}

#[cfg(test)]
mod tests {
    use super::{bound_command_for_selected_index, command_for_selected_index};
    use crate::keybindings_panel::KeybindingPanelItem;
    use kuroya_core::Command;

    #[test]
    fn selected_index_helpers_ignore_stale_indexes() {
        let items = vec![keybinding_item("Ctrl+P", Command::ToggleQuickOpen)];

        assert_eq!(
            command_for_selected_index(0, &items),
            Some(Command::ToggleQuickOpen)
        );
        assert_eq!(command_for_selected_index(1, &items), None);
        assert_eq!(bound_command_for_selected_index(1, &items), None);
    }

    #[test]
    fn selected_index_helpers_only_remove_bound_commands() {
        let items = vec![
            keybinding_item("", Command::ToggleQuickOpen),
            keybinding_item("Ctrl+`", Command::ToggleTerminal),
        ];

        assert_eq!(
            command_for_selected_index(0, &items),
            Some(Command::ToggleQuickOpen)
        );
        assert_eq!(bound_command_for_selected_index(0, &items), None);
        assert_eq!(
            bound_command_for_selected_index(1, &items),
            Some(Command::ToggleTerminal)
        );
    }

    fn keybinding_item(chord: &str, command: Command) -> KeybindingPanelItem {
        KeybindingPanelItem {
            chord: chord.to_owned(),
            command,
            label: "Command".to_owned(),
            search_text: "Command".to_owned(),
        }
    }
}

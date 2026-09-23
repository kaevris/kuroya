use crate::keybindings_panel_actions::PendingKeybindingsPanelActions;
use kuroya_core::Command;

use super::KeybindingPanelItem;

pub(in crate::keybindings_panel) fn guard_keybindings_panel_actions(
    actions: &mut PendingKeybindingsPanelActions,
    items: &[KeybindingPanelItem],
) {
    if actions
        .start_capture
        .as_ref()
        .is_some_and(|command| !items_contain_command(items, command))
    {
        actions.start_capture = None;
    }

    if actions
        .remove_binding
        .as_ref()
        .is_some_and(|command| !items_contain_bound_command(items, command))
    {
        actions.remove_binding = None;
    }

    if actions
        .reset_binding
        .as_ref()
        .is_some_and(|command| !items_contain_command(items, command))
    {
        actions.reset_binding = None;
    }
}

fn items_contain_command(items: &[KeybindingPanelItem], command: &Command) -> bool {
    items.iter().any(|item| &item.command == command)
}

fn items_contain_bound_command(items: &[KeybindingPanelItem], command: &Command) -> bool {
    items
        .iter()
        .any(|item| &item.command == command && !item.chord.is_empty())
}

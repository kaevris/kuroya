use crate::preference_panels::sections::{
    SETTINGS_TARGET_TERMINAL_INTERACTION, SettingsHighlightState,
    bounded_settings_singleline_input, bounded_settings_text_edit_width, settings_target_heading,
};
use crate::ui_switch::ui_switch;
use eframe::egui;
use kuroya_core::{
    EditorSettings, TerminalConfirmOnExit, TerminalConfirmOnKill, TerminalHideOnStartup,
    TerminalMiddleClickBehavior, TerminalMultiLinePasteWarning, TerminalRightClickBehavior,
    TerminalTabsFocusMode, TerminalTabsHideCondition, TerminalTabsLocation,
    TerminalTabsShowActions, TerminalTabsShowActiveTerminal,
};

pub(super) fn render_interaction_settings_with_highlight(
    ui: &mut egui::Ui,
    draft: &mut EditorSettings,
    highlight: &mut SettingsHighlightState<'_>,
) {
    ui.add_space(12.0);
    settings_target_heading(
        ui,
        highlight,
        SETTINGS_TARGET_TERMINAL_INTERACTION,
        "Interaction",
    );
    egui::Grid::new("settings_terminal_interaction_grid")
        .num_columns(2)
        .spacing([18.0, 10.0])
        .show(ui, |ui| {
            ui.label("Right click");
            terminal_right_click_behavior_combo(
                ui,
                "terminal_right_click_behavior",
                &mut draft.terminal_right_click_behavior,
            );
            ui.end_row();

            ui.label("Middle click");
            terminal_middle_click_behavior_combo(
                ui,
                "terminal_middle_click_behavior",
                &mut draft.terminal_middle_click_behavior,
            );
            ui.end_row();

            ui.label("Exit confirmation");
            terminal_confirm_on_exit_combo(
                ui,
                "terminal_confirm_on_exit",
                &mut draft.terminal_confirm_on_exit,
            );
            ui.end_row();

            ui.label("Kill confirmation");
            terminal_confirm_on_kill_combo(
                ui,
                "terminal_confirm_on_kill",
                &mut draft.terminal_confirm_on_kill,
            );
            ui.end_row();

            ui.label("Startup visibility");
            terminal_hide_on_startup_combo(
                ui,
                "terminal_hide_on_startup",
                &mut draft.terminal_hide_on_startup,
            );
            ui.end_row();

            ui.label("Last terminal closed");
            ui_switch(ui, &mut draft.terminal_hide_on_last_closed).on_hover_text(
                "Hide terminal pane. When off, closing the last terminal leaves an empty terminal pane with the new-terminal button.",
            );
            ui.end_row();

            ui.label("Terminal tabs");
            ui_switch(ui, &mut draft.terminal_tabs_enabled);
            ui.end_row();

            ui.label("Default tab icon");
            let mut icon = bounded_settings_singleline_input(&draft.terminal_tabs_default_icon);
            let icon_response = ui.add(
                egui::TextEdit::singleline(&mut icon)
                    .desired_width(bounded_settings_text_edit_width(ui.available_width(), 220.0)),
            );
            if icon_response.changed() {
                draft.terminal_tabs_default_icon = icon;
            }
            icon_response.on_hover_text(
                "Codicon ID, for example terminal, code, settings, search, or git-branch.",
            );
            ui.end_row();

            ui.label("CLI tab titles");
            ui_switch(ui, &mut draft.terminal_tabs_allow_agent_cli_title).on_hover_text(
                "Allow escape-sequence titles. Allows agent CLIs and other terminal apps to set the terminal tab title.",
            );
            ui.end_row();

            ui.label("Tab title");
            let mut title = bounded_settings_singleline_input(&draft.terminal_tabs_title);
            let title_response = ui.add(
                egui::TextEdit::singleline(&mut title)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(bounded_settings_text_edit_width(ui.available_width(), 280.0)),
            );
            if title_response.changed() {
                draft.terminal_tabs_title = title;
            }
            title_response.on_hover_text(
                "Template supports ${process}, ${sequence}, ${cwd}, ${workspaceFolder}, and ${workspaceFolderName}.",
            );
            ui.end_row();

            ui.label("Tab hide condition");
            terminal_tabs_hide_condition_combo(
                ui,
                "terminal_tabs_hide_condition",
                &mut draft.terminal_tabs_hide_condition,
            );
            ui.end_row();

            ui.label("Terminal action buttons");
            terminal_tabs_show_actions_combo(
                ui,
                "terminal_tabs_show_actions",
                &mut draft.terminal_tabs_show_actions,
            );
            ui.end_row();

            ui.label("Active terminal info");
            terminal_tabs_show_active_terminal_combo(
                ui,
                "terminal_tabs_show_active_terminal",
                &mut draft.terminal_tabs_show_active_terminal,
            );
            ui.end_row();

            ui.label("Tab focus mode");
            terminal_tabs_focus_mode_combo(
                ui,
                "terminal_tabs_focus_mode",
                &mut draft.terminal_tabs_focus_mode,
            );
            ui.end_row();

            ui.label("Tab location");
            terminal_tabs_location_combo(
                ui,
                "terminal_tabs_location",
                &mut draft.terminal_tabs_location,
            );
            ui.end_row();

            ui.label("Alt-click");
            ui_switch(ui, &mut draft.terminal_alt_click_moves_cursor).on_hover_text(
                "Sends left/right cursor movement to the shell so Alt-click can reposition the prompt cursor.",
            );
            ui.end_row();

            ui.label("Copy on selection");
            ui_switch(ui, &mut draft.terminal_copy_on_selection);
            ui.end_row();

            ui.label("Bracketed paste");
            ui_switch(ui, &mut draft.terminal_ignore_bracketed_paste_mode).on_hover_text(
                "Ignore shell bracketed paste mode. When off, pasted text is wrapped if the terminal app requests bracketed paste.",
            );
            ui.end_row();

            ui.label("Multiline paste warning");
            terminal_multi_line_paste_warning_combo(
                ui,
                "terminal_enable_multi_line_paste_warning",
                &mut draft.terminal_enable_multi_line_paste_warning,
            );
            ui.end_row();

            ui.label("Word separators");
            let mut separators = bounded_settings_singleline_input(&draft.terminal_word_separators);
            let separators_response = ui.add(
                egui::TextEdit::singleline(&mut separators)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(bounded_settings_text_edit_width(ui.available_width(), 280.0)),
            );
            if separators_response.changed() {
                draft.terminal_word_separators = separators;
            }
            separators_response.on_hover_text(
                "Characters treated as separators when selecting words in terminal text",
            );
            ui.end_row();
        });
}

fn terminal_multi_line_paste_warning_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalMultiLinePasteWarning,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_multi_line_paste_warning_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalMultiLinePasteWarning::Auto, "Auto");
            ui.selectable_value(value, TerminalMultiLinePasteWarning::Always, "Always");
            ui.selectable_value(value, TerminalMultiLinePasteWarning::Never, "Never");
        });
}

fn terminal_tabs_hide_condition_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalTabsHideCondition,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_tabs_hide_condition_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalTabsHideCondition::Never, "Never hide");
            ui.selectable_value(
                value,
                TerminalTabsHideCondition::SingleTerminal,
                "Hide for single terminal",
            );
            ui.selectable_value(
                value,
                TerminalTabsHideCondition::SingleGroup,
                "Hide for single group",
            );
        });
}

fn terminal_hide_on_startup_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalHideOnStartup,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_hide_on_startup_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalHideOnStartup::Never, "Never hide");
            ui.selectable_value(value, TerminalHideOnStartup::WhenEmpty, "Hide when empty");
            ui.selectable_value(value, TerminalHideOnStartup::Always, "Always hide");
        });
}

fn terminal_tabs_show_actions_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalTabsShowActions,
) {
    const OPTIONS: [TerminalTabsShowActions; 4] = [
        TerminalTabsShowActions::Always,
        TerminalTabsShowActions::SingleTerminal,
        TerminalTabsShowActions::SingleTerminalOrNarrow,
        TerminalTabsShowActions::Never,
    ];

    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_tabs_show_actions_label(*value))
        .show_ui(ui, |ui| {
            for option in OPTIONS {
                ui.selectable_value(value, option, terminal_tabs_show_actions_label(option));
            }
        });
}

fn terminal_tabs_show_active_terminal_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalTabsShowActiveTerminal,
) {
    const OPTIONS: [TerminalTabsShowActiveTerminal; 4] = [
        TerminalTabsShowActiveTerminal::Always,
        TerminalTabsShowActiveTerminal::SingleTerminal,
        TerminalTabsShowActiveTerminal::SingleTerminalOrNarrow,
        TerminalTabsShowActiveTerminal::Never,
    ];

    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_tabs_show_active_terminal_label(*value))
        .show_ui(ui, |ui| {
            for option in OPTIONS {
                ui.selectable_value(
                    value,
                    option,
                    terminal_tabs_show_active_terminal_label(option),
                );
            }
        });
}

fn terminal_tabs_focus_mode_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalTabsFocusMode,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_tabs_focus_mode_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalTabsFocusMode::SingleClick, "Single click");
            ui.selectable_value(value, TerminalTabsFocusMode::DoubleClick, "Double click");
        });
}

fn terminal_tabs_location_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalTabsLocation,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_tabs_location_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalTabsLocation::Top, "Top");
            ui.selectable_value(value, TerminalTabsLocation::Left, "Left");
            ui.selectable_value(value, TerminalTabsLocation::Right, "Right");
        });
}

fn terminal_confirm_on_exit_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalConfirmOnExit,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_confirm_on_exit_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalConfirmOnExit::Never, "Never");
            ui.selectable_value(value, TerminalConfirmOnExit::Always, "Always");
            ui.selectable_value(
                value,
                TerminalConfirmOnExit::HasChildProcesses,
                "Has child processes",
            );
        });
}

fn terminal_confirm_on_kill_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalConfirmOnKill,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_confirm_on_kill_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalConfirmOnKill::Never, "Never");
            ui.selectable_value(value, TerminalConfirmOnKill::Editor, "Editor");
            ui.selectable_value(value, TerminalConfirmOnKill::Panel, "Panel");
            ui.selectable_value(value, TerminalConfirmOnKill::Always, "Always");
        });
}

fn terminal_right_click_behavior_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalRightClickBehavior,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_right_click_behavior_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalRightClickBehavior::Default, "Context menu");
            ui.selectable_value(value, TerminalRightClickBehavior::CopyPaste, "Copy/Paste");
            ui.selectable_value(value, TerminalRightClickBehavior::Paste, "Paste");
            ui.selectable_value(value, TerminalRightClickBehavior::SelectWord, "Select word");
            ui.selectable_value(value, TerminalRightClickBehavior::Nothing, "Nothing");
        });
}

fn terminal_middle_click_behavior_combo(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    value: &mut TerminalMiddleClickBehavior,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(terminal_middle_click_behavior_label(*value))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, TerminalMiddleClickBehavior::Default, "Default");
            ui.selectable_value(value, TerminalMiddleClickBehavior::Paste, "Paste");
        });
}

fn terminal_multi_line_paste_warning_label(
    behavior: TerminalMultiLinePasteWarning,
) -> &'static str {
    match behavior {
        TerminalMultiLinePasteWarning::Auto => "Auto",
        TerminalMultiLinePasteWarning::Always => "Always",
        TerminalMultiLinePasteWarning::Never => "Never",
    }
}

fn terminal_tabs_hide_condition_label(behavior: TerminalTabsHideCondition) -> &'static str {
    match behavior {
        TerminalTabsHideCondition::Never => "Never hide",
        TerminalTabsHideCondition::SingleTerminal => "Hide for single terminal",
        TerminalTabsHideCondition::SingleGroup => "Hide for single group",
    }
}

fn terminal_hide_on_startup_label(behavior: TerminalHideOnStartup) -> &'static str {
    match behavior {
        TerminalHideOnStartup::Never => "Never hide",
        TerminalHideOnStartup::WhenEmpty => "Hide when empty",
        TerminalHideOnStartup::Always => "Always hide",
    }
}

fn terminal_tabs_show_actions_label(behavior: TerminalTabsShowActions) -> &'static str {
    match behavior {
        TerminalTabsShowActions::Always => "Always",
        TerminalTabsShowActions::SingleTerminal => "Single terminal",
        TerminalTabsShowActions::SingleTerminalOrNarrow => "Single terminal or narrow",
        TerminalTabsShowActions::Never => "Never",
    }
}

fn terminal_tabs_show_active_terminal_label(
    behavior: TerminalTabsShowActiveTerminal,
) -> &'static str {
    match behavior {
        TerminalTabsShowActiveTerminal::Always => "Always",
        TerminalTabsShowActiveTerminal::SingleTerminal => "Single terminal",
        TerminalTabsShowActiveTerminal::SingleTerminalOrNarrow => "Single terminal or narrow",
        TerminalTabsShowActiveTerminal::Never => "Never",
    }
}

fn terminal_tabs_focus_mode_label(mode: TerminalTabsFocusMode) -> &'static str {
    match mode {
        TerminalTabsFocusMode::SingleClick => "Single click",
        TerminalTabsFocusMode::DoubleClick => "Double click",
    }
}

fn terminal_tabs_location_label(location: TerminalTabsLocation) -> &'static str {
    match location {
        TerminalTabsLocation::Top => "Top",
        TerminalTabsLocation::Left => "Left",
        TerminalTabsLocation::Right => "Right",
    }
}

fn terminal_confirm_on_exit_label(behavior: TerminalConfirmOnExit) -> &'static str {
    match behavior {
        TerminalConfirmOnExit::Never => "Never",
        TerminalConfirmOnExit::Always => "Always",
        TerminalConfirmOnExit::HasChildProcesses => "Has child processes",
    }
}

fn terminal_confirm_on_kill_label(behavior: TerminalConfirmOnKill) -> &'static str {
    match behavior {
        TerminalConfirmOnKill::Never => "Never",
        TerminalConfirmOnKill::Editor => "Editor",
        TerminalConfirmOnKill::Panel => "Panel",
        TerminalConfirmOnKill::Always => "Always",
    }
}

fn terminal_right_click_behavior_label(behavior: TerminalRightClickBehavior) -> &'static str {
    match behavior {
        TerminalRightClickBehavior::Default => "Context menu",
        TerminalRightClickBehavior::CopyPaste => "Copy/Paste",
        TerminalRightClickBehavior::Paste => "Paste",
        TerminalRightClickBehavior::SelectWord => "Select word",
        TerminalRightClickBehavior::Nothing => "Nothing",
    }
}

fn terminal_middle_click_behavior_label(behavior: TerminalMiddleClickBehavior) -> &'static str {
    match behavior {
        TerminalMiddleClickBehavior::Default => "Default",
        TerminalMiddleClickBehavior::Paste => "Paste",
    }
}

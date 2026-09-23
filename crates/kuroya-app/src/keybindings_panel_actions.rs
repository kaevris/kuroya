use crate::{
    KuroyaApp,
    app_state::KeybindingEscapeCancel,
    commands::{command_label, keybinding_chord_for_command},
    keybinding_input::CapturedKeybinding,
    keybindings::{
        KeybindingChordShadow, keybinding_chord_shadow_conflict,
        keybinding_default_chord_for_command,
    },
    keybindings_runtime::{
        keybinding_cancel_save_failed_status, malformed_keybinding_chord_rejection_reason,
        prune_stale_keybinding_assignments,
    },
    workspace_state::settings_path,
};
use kuroya_core::Command;
use std::time::{Duration, Instant};

const KEYBINDING_ESCAPE_ARM_WINDOW: Duration = Duration::from_secs(1);
const KEYBINDING_ESCAPE_ARM_MESSAGE: &str = "Press Esc again to bind Escape";

#[derive(Default)]
pub(crate) struct PendingKeybindingsPanelActions {
    pub(crate) captured: Option<CapturedKeybinding>,
    pub(crate) start_capture: Option<Command>,
    pub(crate) remove_binding: Option<Command>,
    pub(crate) reset_binding: Option<Command>,
    pub(crate) close: bool,
    pub(crate) open_settings: bool,
}

impl KuroyaApp {
    pub(crate) fn apply_keybindings_panel_actions(
        &mut self,
        actions: PendingKeybindingsPanelActions,
    ) {
        if let Some(captured) = actions.captured {
            match captured {
                CapturedKeybinding::Cancel => {
                    self.cancel_keybinding_capture();
                }
                CapturedKeybinding::Rejected(reason) => {
                    self.status = reason;
                }
                CapturedKeybinding::Escape => self.handle_keybinding_escape_capture(Instant::now()),
                CapturedKeybinding::Chord(chord) => {
                    self.save_captured_keybinding_chord(chord);
                }
            }
        } else if let Some(command) = actions.start_capture {
            if self.has_keybinding_capture_in_progress() && !self.cancel_keybinding_capture() {
                return;
            }
            self.keybinding_capture_command = Some(command);
            self.status =
                "Capturing shortcut; press keys, or press Esc twice to bind Escape".to_owned();
        } else if let Some(command) = actions.remove_binding {
            if self.has_keybinding_capture_in_progress() && !self.cancel_keybinding_capture() {
                return;
            }
            self.remove_keybinding_for_command(command);
        } else if let Some(command) = actions.reset_binding {
            if self.has_keybinding_capture_in_progress() && !self.cancel_keybinding_capture() {
                return;
            }
            self.reset_keybinding_for_command(command);
        } else if actions.close {
            if self.has_keybinding_capture_in_progress() && !self.cancel_keybinding_capture() {
                return;
            }
            self.keybindings_open = false;
            self.status = "Closed keyboard shortcuts".to_owned();
        } else if actions.open_settings {
            if self.has_keybinding_capture_in_progress() && !self.cancel_keybinding_capture() {
                return;
            }
            self.keybindings_open = false;
            self.open_settings_file();
        }
    }

    pub(crate) fn finish_expired_keybinding_escape_capture(&mut self, now: Instant) -> bool {
        let Some(pending) = self.keybinding_escape_cancel.as_ref() else {
            return false;
        };
        if now < pending.deadline {
            return false;
        }
        let command = pending.command.clone();
        self.keybinding_escape_cancel = None;
        if self.keybinding_capture_command == Some(command) {
            self.keybinding_capture_command = None;
        }
        self.status = "Canceled shortcut capture".to_owned();
        true
    }

    pub(crate) fn keybinding_escape_cancel_remaining(&self, now: Instant) -> Option<Duration> {
        self.keybinding_escape_cancel
            .as_ref()
            .map(|pending| pending.deadline.saturating_duration_since(now))
    }

    pub(crate) fn has_keybinding_capture_in_progress(&self) -> bool {
        self.keybinding_capture_command.is_some() || self.keybinding_escape_cancel.is_some()
    }

    pub(crate) fn cancel_keybinding_capture(&mut self) -> bool {
        if self.keybinding_escape_cancel.is_some() {
            self.restore_keybinding_escape_cancel();
            self.keybinding_escape_cancel.is_none()
        } else {
            self.keybinding_capture_command = None;
            self.status = "Canceled shortcut capture".to_owned();
            true
        }
    }

    fn save_captured_keybinding_chord(&mut self, chord: String) {
        if let Some(reason) = malformed_keybinding_chord_rejection_reason(&chord) {
            self.status = reason.to_owned();
            return;
        }

        let Some(command) = self.keybinding_capture_command.clone() else {
            return;
        };
        if self.keybinding_escape_cancel.is_some() {
            self.restore_keybinding_escape_cancel();
            if self.keybinding_escape_cancel.is_some() {
                return;
            }
        }
        self.keybinding_capture_command = None;
        if self.save_keybinding_chord(command.clone(), chord) {
            self.report_keybinding_shadow_warning(&command);
        } else {
            self.keybinding_capture_command = Some(command);
        }
    }

    fn handle_keybinding_escape_capture(&mut self, now: Instant) {
        let Some(command) = self.keybinding_capture_command.clone() else {
            return;
        };
        if self
            .keybinding_escape_cancel
            .as_ref()
            .is_some_and(|pending| pending.command == command && now <= pending.deadline)
        {
            self.bind_escape_for_armed_capture(command);
            return;
        }

        self.keybinding_escape_cancel = Some(KeybindingEscapeCancel {
            command,
            bindings_before_escape: Vec::new(),
            deadline: now + KEYBINDING_ESCAPE_ARM_WINDOW,
            saved_escape_binding: false,
        });
        self.status = KEYBINDING_ESCAPE_ARM_MESSAGE.to_owned();
    }

    fn bind_escape_for_armed_capture(&mut self, command: Command) {
        self.keybinding_escape_cancel = None;
        self.keybinding_capture_command = None;
        if !self.save_keybinding_chord(command.clone(), "Escape".to_owned()) {
            self.keybinding_capture_command = Some(command);
        }
    }

    fn reset_keybinding_for_command(&mut self, command: Command) {
        let Some(default_chord) = keybinding_default_chord_for_command(&command) else {
            return;
        };
        let label = command_label(&command);
        if keybinding_chord_for_command(&self.settings.keymap.bindings, &command).as_deref()
            == Some(default_chord.as_str())
        {
            self.status = format!("{label} already uses the default shortcut {default_chord}");
            return;
        }

        if self.save_keybinding_chord(command, default_chord.clone()) {
            self.status = format!("Restored default shortcut {default_chord} for {label}");
        }
    }

    fn report_keybinding_shadow_warning(&mut self, command: &Command) {
        let Some(chord) = keybinding_chord_for_command(&self.settings.keymap.bindings, command)
        else {
            return;
        };
        if let Some(shadow) =
            keybinding_chord_shadow_conflict(&self.settings.keymap.bindings, command, &chord)
        {
            self.status = keybinding_shadow_warning_status(&chord, shadow);
        }
    }

    fn restore_keybinding_escape_cancel(&mut self) {
        let Some(pending) = self.keybinding_escape_cancel.take() else {
            return;
        };
        if !pending.saved_escape_binding {
            self.keybinding_capture_command = None;
            self.status = "Canceled shortcut capture".to_owned();
            return;
        }
        let mut settings = self.settings.clone();
        settings.keymap.bindings = pending.bindings_before_escape.clone();
        prune_stale_keybinding_assignments(&mut settings.keymap.bindings);
        let label = command_label(&pending.command);
        match settings.save(&settings_path(&self.workspace.root)) {
            Ok(()) => {
                self.settings = settings;
                self.keybinding_capture_command = None;
                self.status = "Canceled shortcut capture".to_owned();
            }
            Err(error) => {
                self.keybinding_capture_command = Some(pending.command.clone());
                self.keybinding_escape_cancel = Some(pending);
                self.status = keybinding_cancel_save_failed_status(&label, error);
            }
        }
    }
}

fn keybinding_shadow_warning_status(chord: &str, shadow: KeybindingChordShadow) -> String {
    match shadow {
        KeybindingChordShadow::ShadowedBy { existing_chord } => {
            format!("Warning: {chord} is shadowed by existing {existing_chord} binding")
        }
        KeybindingChordShadow::Shadows { existing_chord } => {
            format!("Warning: {chord} shadows existing {existing_chord} binding")
        }
    }
}

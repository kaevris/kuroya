use crate::{
    KuroyaApp,
    commands::command_label,
    devtools::{CommandTraceEntry, MAX_COMMAND_TRACE_ENTRIES, record_command_trace_entry},
    devtools_trace_id::next_devtools_trace_id,
    keybinding_parse::parse_key_chord,
    path_clipboard::{PathCopyKind, copy_path_to_clipboard},
    terminal::shortcut_is_terminal_input,
};
use eframe::egui::{Context, KeyboardShortcut};
use kuroya_core::{Command, CommandBus, keymap::KeyBinding};
use std::{path::Path, time::Instant};

const COMMAND_DRAIN_BUDGET: usize = 128;

#[derive(Debug, Default)]
pub(crate) struct ShortcutDispatchCache {
    source_bindings: Vec<KeyBinding>,
    bindings: Vec<ParsedShortcutBinding>,
    terminal_binding_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedShortcutBinding {
    shortcut: KeyboardShortcut,
    command: Command,
}

impl ShortcutDispatchCache {
    fn refresh(&mut self, bindings: &[KeyBinding]) {
        if self.source_bindings.as_slice() == bindings {
            return;
        }

        self.source_bindings.clear();
        self.source_bindings.extend_from_slice(bindings);
        self.bindings.clear();
        self.bindings.reserve(bindings.len());
        self.terminal_binding_indices.clear();
        self.terminal_binding_indices.reserve(bindings.len());
        for binding in bindings {
            if let Some(shortcut) = parse_key_chord(&binding.chord) {
                let binding_index = self.bindings.len();
                if shortcut_available_when_terminal_focused(&shortcut, &binding.command) {
                    self.terminal_binding_indices.push(binding_index);
                }
                self.bindings.push(ParsedShortcutBinding {
                    shortcut,
                    command: binding.command.clone(),
                });
            }
        }
    }

    #[cfg(test)]
    fn bindings(&self) -> &[ParsedShortcutBinding] {
        &self.bindings
    }

    fn push_consumed_shortcuts(
        &self,
        ctx: &Context,
        terminal_input_focused: bool,
        command_bus: &mut CommandBus,
    ) {
        if terminal_input_focused {
            for &binding_index in &self.terminal_binding_indices {
                self.push_consumed_shortcut(ctx, command_bus, binding_index);
            }
        } else {
            for binding_index in 0..self.bindings.len() {
                self.push_consumed_shortcut(ctx, command_bus, binding_index);
            }
        }
    }

    fn push_consumed_shortcut(
        &self,
        ctx: &Context,
        command_bus: &mut CommandBus,
        binding_index: usize,
    ) {
        let binding = &self.bindings[binding_index];
        if ctx.input_mut(|input| input.consume_shortcut(&binding.shortcut)) {
            command_bus.push(binding.command.clone());
        }
    }
}

impl KuroyaApp {
    pub(crate) fn dispatch_shortcuts(&mut self, ctx: &Context) {
        if self.keybinding_capture_command.is_some() {
            return;
        }

        self.shortcut_dispatch_cache
            .refresh(&self.settings.keymap.bindings);
        if self.shortcut_dispatch_cache.bindings.is_empty() {
            return;
        }

        let terminal_input_focused = self.terminal.input_focused(ctx);
        self.shortcut_dispatch_cache.push_consumed_shortcuts(
            ctx,
            terminal_input_focused,
            &mut self.command_bus,
        );
    }

    pub(crate) fn drain_commands(&mut self, ctx: &Context) -> usize {
        let mut count = 0usize;
        while count < COMMAND_DRAIN_BUDGET {
            let Some(command) = self.command_bus.pop() else {
                break;
            };
            count = count.saturating_add(1);
            let profiling = self.profiling_enabled();
            let profile_started = profiling.then(Instant::now);
            let label = command_label(&command);
            let profile_label = profiling.then(|| label.clone());
            self.record_command_trace_label(label);
            if let Some(command) = self.run_context_command(ctx, command) {
                self.run_command(command);
            }
            if let (Some(started), Some(label)) = (profile_started, profile_label) {
                self.record_profile_sample("command", label, started.elapsed());
            }
        }
        count
    }

    fn record_command_trace_label(&mut self, label: String) {
        let id = next_devtools_trace_id(&mut self.next_command_trace_id);
        let verbose_label = self
            .settings
            .devtools_verbose_logging
            .then(|| label.clone());
        record_command_trace_entry(
            &mut self.command_trace,
            CommandTraceEntry { id, label },
            MAX_COMMAND_TRACE_ENTRIES,
        );
        if let Some(label) = verbose_label {
            self.record_verbose_log("command", label);
        }
    }

    fn run_context_command(&mut self, ctx: &Context, command: Command) -> Option<Command> {
        match command {
            kuroya_core::Command::ToggleBufferFind if self.terminal.input_focused(ctx) => {
                self.terminal.open_terminal_search();
            }
            kuroya_core::Command::FindNext
                if self.terminal.advance_terminal_search_result_if_open(1) => {}
            kuroya_core::Command::FindPrevious
                if self.terminal.advance_terminal_search_result_if_open(-1) => {}
            kuroya_core::Command::CopyAllChangesPatch => self.copy_all_changes_patch(ctx),
            kuroya_core::Command::CopyUnstagedChangesPatch => {
                self.copy_stage_patch(ctx, kuroya_core::GitChangeStage::Unstaged);
            }
            kuroya_core::Command::CopyStagedChangesPatch => {
                self.copy_stage_patch(ctx, kuroya_core::GitChangeStage::Staged);
            }
            kuroya_core::Command::CopyActiveFilePatch => {
                self.copy_active_file_patch(ctx, kuroya_core::GitChangeStage::Unstaged);
            }
            kuroya_core::Command::CopyActiveFileStagedPatch => {
                self.copy_active_file_patch(ctx, kuroya_core::GitChangeStage::Staged);
            }
            kuroya_core::Command::CopyFilePatch(path) => {
                self.copy_file_patch(ctx, path, kuroya_core::GitChangeStage::Unstaged);
            }
            kuroya_core::Command::CopyStagedFilePatch(path) => {
                self.copy_file_patch(ctx, path, kuroya_core::GitChangeStage::Staged);
            }
            kuroya_core::Command::CopyActiveFileHunkPatch => {
                self.copy_active_file_hunk_patch(ctx, kuroya_core::GitChangeStage::Unstaged);
            }
            kuroya_core::Command::CopyActiveFileStagedHunkPatch => {
                self.copy_active_file_hunk_patch(ctx, kuroya_core::GitChangeStage::Staged);
            }
            kuroya_core::Command::CopyActiveFilePath => {
                self.copy_active_file_path(ctx, PathCopyKind::Absolute);
            }
            kuroya_core::Command::CopyActiveFileRelativePath => {
                self.copy_active_file_path(ctx, PathCopyKind::Relative);
            }
            kuroya_core::Command::CopyFilePath(path) => {
                self.copy_file_path_to_clipboard(ctx, path, PathCopyKind::Absolute);
            }
            kuroya_core::Command::CopyFileRelativePath(path) => {
                self.copy_file_path_to_clipboard(ctx, path, PathCopyKind::Relative);
            }
            kuroya_core::Command::CopyActiveDiffPatch => self.copy_active_diff_patch(ctx),
            kuroya_core::Command::CopyActiveDiffHunkPatch => self.copy_active_diff_hunk_patch(ctx),
            command => return Some(command),
        }
        None
    }

    fn copy_active_file_path(&mut self, ctx: &Context, kind: PathCopyKind) {
        let action = match kind {
            PathCopyKind::Absolute => "copy path",
            PathCopyKind::Relative => "copy relative path",
        };
        let Some(path) = self.active_file_or_diff_source_path(action) else {
            return;
        };
        self.copy_file_path_to_clipboard(ctx, &path, kind);
    }

    fn copy_file_path_to_clipboard<P>(&mut self, ctx: &Context, path: P, kind: PathCopyKind)
    where
        P: AsRef<Path>,
    {
        self.status = copy_path_to_clipboard(ctx, &self.workspace.root, path.as_ref(), kind);
    }
}

#[cfg(test)]
fn shortcut_available_for_dispatch(
    shortcut: &KeyboardShortcut,
    terminal_input_focused: bool,
    command: &Command,
) -> bool {
    !terminal_input_focused || shortcut_available_when_terminal_focused(shortcut, command)
}

fn shortcut_available_when_terminal_focused(
    shortcut: &KeyboardShortcut,
    command: &Command,
) -> bool {
    matches!(command, Command::ToggleBufferFind) || !shortcut_is_terminal_input(shortcut)
}

#[cfg(test)]
mod tests {
    use super::{ShortcutDispatchCache, shortcut_available_for_dispatch};
    use eframe::egui::{self, Event, Key, KeyboardShortcut, Modifiers, RawInput};
    use kuroya_core::{Command, CommandBus, keymap::KeyBinding};

    #[test]
    fn shortcut_dispatch_cache_reuses_parsed_keybindings_until_keymap_changes() {
        let bindings = vec![
            KeyBinding {
                chord: "Ctrl+P".to_owned(),
                command: Command::ToggleQuickOpen,
            },
            KeyBinding {
                chord: "not a shortcut".to_owned(),
                command: Command::Undo,
            },
        ];
        let mut cache = ShortcutDispatchCache::default();

        cache.refresh(&bindings);

        assert_eq!(cache.bindings().len(), 1);
        assert_eq!(
            cache.bindings()[0].shortcut,
            KeyboardShortcut::new(Modifiers::CTRL, Key::P)
        );
        assert_eq!(cache.bindings()[0].command, Command::ToggleQuickOpen);
        let parsed_ptr = cache.bindings().as_ptr();

        cache.refresh(&bindings);

        assert_eq!(cache.bindings().as_ptr(), parsed_ptr);

        let changed = [KeyBinding {
            chord: "Ctrl+F".to_owned(),
            command: Command::ToggleBufferFind,
        }];
        cache.refresh(&changed);

        assert_eq!(cache.bindings().len(), 1);
        assert_eq!(
            cache.bindings()[0].shortcut,
            KeyboardShortcut::new(Modifiers::CTRL, Key::F)
        );
        assert_eq!(cache.bindings()[0].command, Command::ToggleBufferFind);
    }

    #[test]
    fn shortcut_dispatch_cache_pushes_consumed_shortcuts_directly_to_command_bus() {
        let mut cache = ShortcutDispatchCache::default();
        cache.refresh(&[
            KeyBinding {
                chord: "Ctrl+P".to_owned(),
                command: Command::ToggleQuickOpen,
            },
            KeyBinding {
                chord: "Ctrl+F".to_owned(),
                command: Command::ToggleBufferFind,
            },
        ]);

        assert_eq!(
            dispatched_commands_for_shortcut(&cache, false, Key::P, Modifiers::CTRL),
            vec![Command::ToggleQuickOpen]
        );
    }

    #[test]
    fn shortcut_dispatch_cache_pushes_command_palette_ctrl_shift_p() {
        let mut cache = ShortcutDispatchCache::default();
        cache.refresh(&[KeyBinding {
            chord: "Ctrl+Shift+P".to_owned(),
            command: Command::ToggleCommandPalette,
        }]);

        assert_eq!(
            dispatched_commands_for_shortcut(
                &cache,
                false,
                Key::P,
                Modifiers::CTRL | Modifiers::SHIFT,
            ),
            vec![Command::ToggleCommandPalette]
        );
    }

    #[test]
    fn terminal_focused_shortcut_dispatch_keeps_order_after_filtered_binding() {
        let mut cache = ShortcutDispatchCache::default();
        cache.refresh(&[
            KeyBinding {
                chord: "Ctrl+P".to_owned(),
                command: Command::ToggleQuickOpen,
            },
            KeyBinding {
                chord: "Ctrl+P".to_owned(),
                command: Command::ToggleBufferFind,
            },
        ]);

        assert_eq!(
            dispatched_commands_for_shortcut(&cache, false, Key::P, Modifiers::CTRL),
            vec![Command::ToggleQuickOpen]
        );
        assert_eq!(
            dispatched_commands_for_shortcut(&cache, true, Key::P, Modifiers::CTRL),
            vec![Command::ToggleBufferFind]
        );
    }

    #[test]
    fn terminal_focused_shortcut_dispatch_preserves_pty_control_input() {
        assert!(!shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::P),
            true,
            &Command::ToggleQuickOpen
        ));
        assert!(!shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::Z),
            true,
            &Command::Undo
        ));
        assert!(!shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::W),
            true,
            &Command::CloseActive
        ));
        assert!(!shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::Backslash),
            true,
            &Command::SplitEditorRight
        ));
        assert!(!shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::CloseBracket),
            true,
            &Command::IndentLines
        ));
        assert!(!shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::Space),
            true,
            &Command::RequestCompletions
        ));
        assert!(shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::F),
            true,
            &Command::ToggleBufferFind
        ));
        assert!(shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::Backtick),
            true,
            &Command::ToggleTerminal
        ));
        assert!(shortcut_available_for_dispatch(
            &KeyboardShortcut::new(Modifiers::CTRL, Key::P),
            false,
            &Command::ToggleQuickOpen
        ));
    }

    fn dispatched_commands_for_shortcut(
        cache: &ShortcutDispatchCache,
        terminal_input_focused: bool,
        key: Key,
        modifiers: Modifiers,
    ) -> Vec<Command> {
        let ctx = egui::Context::default();
        let mut command_bus = CommandBus::default();
        let input = RawInput {
            modifiers,
            events: vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..RawInput::default()
        };

        let _ = ctx.run(input, |ctx| {
            cache.push_consumed_shortcuts(ctx, terminal_input_focused, &mut command_bus);
        });

        command_bus.drain().collect()
    }
}

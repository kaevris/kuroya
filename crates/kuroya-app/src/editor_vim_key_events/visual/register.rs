use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::{
    EditorVimLastChange, EditorVimMode, EditorVimNamedRegister, EditorVimPendingKey,
    EditorVimRegister, EditorVimRepeatAction, VimKeyResult, vim_escape_key,
    vim_named_register_for_key, vim_repeatable_change_result,
};
use super::character_action::{vim_put_over_visual_character, vim_visual_character_put_key};
use super::{
    vim_delete_visual_character_into_named_register, vim_exit_visual_selection,
    vim_visual_character_change_key, vim_visual_character_clamped_cursor,
    vim_visual_character_delete_key, vim_visual_character_repeat_count,
    vim_visual_character_yank_key, vim_yank_visual_character_into_named_register,
};

pub(in crate::editor_vim_key_events) fn handle_vim_visual_character_register_prefix_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    pending: &mut Option<EditorVimPendingKey>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
    suppress_text: Option<char>,
) -> VimKeyResult {
    let anchor = vim_visual_character_clamped_cursor(buffer, anchor);
    let cursor = vim_visual_character_clamped_cursor(buffer, cursor);
    if vim_escape_key(key, modifiers) {
        *pending = None;
        vim_exit_visual_selection(buffer, anchor, cursor);
        return VimKeyResult::handled(suppress_text);
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        *pending = Some(EditorVimPendingKey::VisualCharacterRegisterPrefix {
            anchor,
            cursor,
            count,
        });
        return VimKeyResult::ignored();
    }
    if let Some(register) = vim_named_register_for_key(key, modifiers) {
        *pending = Some(EditorVimPendingKey::VisualCharacterRegisterCommand {
            anchor,
            cursor,
            count,
            register,
        });
        return VimKeyResult::handled(suppress_text);
    }

    *pending = Some(EditorVimPendingKey::VisualCharacterRegisterPrefix {
        anchor,
        cursor,
        count,
    });
    if suppress_text.is_some() {
        VimKeyResult::handled(suppress_text)
    } else {
        VimKeyResult::ignored()
    }
}

pub(in crate::editor_vim_key_events) fn handle_vim_visual_character_register_command_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    anchor: usize,
    cursor: usize,
    count: Option<usize>,
    register: EditorVimNamedRegister,
    suppress_text: Option<char>,
) -> VimKeyResult {
    let anchor = vim_visual_character_clamped_cursor(buffer, anchor);
    let cursor = vim_visual_character_clamped_cursor(buffer, cursor);
    if vim_escape_key(key, modifiers) {
        *pending = None;
        vim_exit_visual_selection(buffer, anchor, cursor);
        return VimKeyResult::handled(suppress_text);
    }
    if modifiers.command || modifiers.alt || modifiers.ctrl {
        *pending = Some(EditorVimPendingKey::VisualCharacterRegisterCommand {
            anchor,
            cursor,
            count,
            register,
        });
        return VimKeyResult::ignored();
    }
    if vim_visual_character_yank_key(key, modifiers) {
        vim_yank_visual_character_into_named_register(
            buffer,
            anchor,
            cursor,
            unnamed_register,
            register,
        );
        super::vim_record_visual_bounds_marks(buffer, anchor, cursor);
        *pending = None;
        return VimKeyResult::handled(suppress_text);
    }
    if vim_visual_character_put_key(key, modifiers) {
        let repeat = count.unwrap_or(1).clamp(1, super::super::VIM_MAX_COUNT);
        let source = super::super::state::vim_named_register(register).map(|mut register| {
            if repeat > 1 {
                register.text = register.text.repeat(repeat);
            }
            register
        });
        let changed = vim_put_over_visual_character(
            buffer,
            anchor,
            cursor,
            source.as_ref(),
            unnamed_register,
        );
        *pending = None;
        return if changed {
            VimKeyResult::changed(suppress_text)
        } else {
            VimKeyResult::handled(suppress_text)
        };
    }
    if vim_visual_character_delete_key(key, modifiers) {
        let changed = vim_delete_visual_character_into_named_register(
            buffer,
            anchor,
            cursor,
            unnamed_register,
            register,
        );
        *pending = None;
        return if changed {
            VimKeyResult::changed(suppress_text)
        } else {
            VimKeyResult::handled(suppress_text)
        };
    }
    if vim_visual_character_change_key(key, modifiers) {
        let repeat_count = vim_visual_character_repeat_count(buffer, anchor, cursor);
        let changed = vim_delete_visual_character_into_named_register(
            buffer,
            anchor,
            cursor,
            unnamed_register,
            register,
        );
        *pending = None;
        return if changed {
            *mode = EditorVimMode::Insert;
            vim_repeatable_change_result(
                changed,
                last_change,
                EditorVimRepeatAction::SubstituteForwardCharsIntoRegister(register),
                repeat_count,
                suppress_text,
            )
        } else {
            VimKeyResult::handled(suppress_text)
        };
    }

    *pending = Some(EditorVimPendingKey::VisualCharacterRegisterCommand {
        anchor,
        cursor,
        count,
        register,
    });
    if suppress_text.is_some() {
        VimKeyResult::handled(suppress_text)
    } else {
        VimKeyResult::ignored()
    }
}

mod case;
mod change_delete;
mod operator_motion;
mod register;
mod simple;
mod visual;
mod yank;

use self::case::handle_vim_case_pending_key_event;
use self::change_delete::handle_vim_change_delete_pending_key_event;
use self::register::handle_vim_register_pending_key_event;
use self::simple::handle_vim_simple_pending_key_event;
use self::visual::handle_vim_visual_pending_key_event;
use self::yank::handle_vim_yank_pending_key_event;
use super::direct::handle_vim_direct_normal_key_event;
use super::*;

pub(super) fn handle_vim_pending_or_direct_normal_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    indent_unit: &str,
    count: Option<usize>,
    count_value: usize,
    suppress_text: Option<char>,
) -> VimKeyResult {
    if let Some(pending_key) = pending.take() {
        if let Some(motion) = vim_operator_last_find_motion_for_key(key, modifiers, *last_char_find)
            && let Some((operator_count, motion_count, operator)) =
                vim_pending_key_last_find_operator_go(&pending_key)
        {
            return handle_vim_operator_go_motion_key_event(
                buffer,
                mode,
                unnamed_register,
                last_change,
                operator_count,
                motion_count,
                operator,
                motion,
                suppress_text,
            );
        }
        if let Some(next_pending) =
            vim_pending_key_next_operator_go(Some(pending_key), key, modifiers)
        {
            *pending = Some(next_pending);
            return VimKeyResult::handled(suppress_text);
        }
        if let Some(result) = handle_vim_simple_pending_key_event(
            buffer,
            key,
            modifiers,
            pending,
            last_char_find,
            last_change,
            indent_unit,
            pending_key,
            suppress_text,
        ) {
            return result;
        }
        if let Some(result) = handle_vim_register_pending_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            unnamed_register,
            last_change,
            pending_key,
            suppress_text,
        ) {
            return result;
        }
        if let Some(result) = handle_vim_change_delete_pending_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            last_char_find,
            unnamed_register,
            last_change,
            pending_key,
            suppress_text,
        ) {
            return result;
        }
        if let Some(result) = handle_vim_case_pending_key_event(
            buffer,
            key,
            modifiers,
            pending,
            last_char_find,
            last_change,
            pending_key,
            suppress_text,
        ) {
            return result;
        }
        if let Some(result) = handle_vim_yank_pending_key_event(
            buffer,
            key,
            modifiers,
            pending,
            last_char_find,
            unnamed_register,
            pending_key,
            suppress_text,
        ) {
            return result;
        }
        match (pending_key, key) {
            (EditorVimPendingKey::SearchInput { count, forward }, key) => {
                return handle_vim_search_input_key_event(
                    buffer,
                    key,
                    modifiers,
                    pending,
                    count,
                    forward,
                    suppress_text,
                );
            }
            (EditorVimPendingKey::CommandInput, key) => {
                return handle_vim_command_input_key_event(
                    buffer,
                    key,
                    modifiers,
                    pending,
                    last_change,
                    suppress_text,
                );
            }
            (
                EditorVimPendingKey::OperatorGoMotion {
                    operator_count,
                    motion_count,
                    operator,
                },
                key,
            ) => {
                if let Some(motion) = vim_operator_go_motion_for_key(key, modifiers) {
                    return handle_vim_operator_go_motion_key_event(
                        buffer,
                        mode,
                        unnamed_register,
                        last_change,
                        operator_count,
                        motion_count,
                        operator,
                        motion,
                        suppress_text,
                    );
                }
            }
            _ => {}
        }
        if let Some(result) = handle_vim_visual_pending_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            last_char_find,
            unnamed_register,
            last_change,
            pending_key,
            indent_unit,
            suppress_text,
        ) {
            return result;
        }
    }

    handle_vim_direct_normal_key_event(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_char_find,
        unnamed_register,
        last_change,
        indent_unit,
        count,
        count_value,
        suppress_text,
    )
}

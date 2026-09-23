use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::*;

pub(super) fn handle_vim_visual_pending_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    pending_key: EditorVimPendingKey,
    indent_unit: &str,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    match (pending_key, key) {
        (
            EditorVimPendingKey::VisualLine {
                anchor,
                cursor,
                count,
            },
            key,
        ) => Some(handle_vim_visual_line_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            unnamed_register,
            last_change,
            anchor,
            cursor,
            count,
            indent_unit,
            suppress_text,
        )),
        (
            EditorVimPendingKey::VisualLineGo {
                anchor,
                cursor,
                count,
            },
            key,
        ) => Some(handle_vim_visual_line_go_key_event(
            buffer,
            key,
            modifiers,
            pending,
            anchor,
            cursor,
            count,
            suppress_text,
        )),
        (EditorVimPendingKey::VisualCharacter { anchor, cursor }, key) => {
            Some(handle_vim_visual_character_key_event(
                buffer,
                key,
                modifiers,
                mode,
                pending,
                last_char_find,
                unnamed_register,
                last_change,
                anchor,
                cursor,
                None,
                indent_unit,
                suppress_text,
            ))
        }
        (
            EditorVimPendingKey::VisualCharacterCount {
                anchor,
                cursor,
                count,
            },
            key,
        ) => Some(handle_vim_visual_character_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            last_char_find,
            unnamed_register,
            last_change,
            anchor,
            cursor,
            Some(count),
            indent_unit,
            suppress_text,
        )),
        (
            EditorVimPendingKey::VisualCharacterGo {
                anchor,
                cursor,
                count,
            },
            key,
        ) => Some(handle_vim_visual_character_go_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            last_char_find,
            unnamed_register,
            last_change,
            anchor,
            cursor,
            count,
            indent_unit,
            suppress_text,
        )),
        (EditorVimPendingKey::VisualCharacterReplace { anchor, cursor }, key) => {
            Some(handle_vim_visual_character_replace_key_event(
                buffer,
                key,
                modifiers,
                pending,
                last_change,
                anchor,
                cursor,
                suppress_text,
            ))
        }
        (EditorVimPendingKey::VisualLineReplace { anchor, cursor }, key) => {
            Some(handle_vim_visual_line_replace_key_event(
                buffer,
                key,
                modifiers,
                pending,
                last_change,
                anchor,
                cursor,
                suppress_text,
            ))
        }
        (
            EditorVimPendingKey::VisualCharacterCharFind {
                anchor,
                cursor,
                count,
                motion,
            },
            key,
        ) => Some(handle_vim_visual_character_char_find_key_event(
            buffer,
            key,
            modifiers,
            pending,
            last_char_find,
            anchor,
            cursor,
            count,
            motion,
            suppress_text,
        )),
        (
            EditorVimPendingKey::VisualCharacterRegisterPrefix {
                anchor,
                cursor,
                count,
            },
            key,
        ) => Some(handle_vim_visual_character_register_prefix_key_event(
            buffer,
            key,
            modifiers,
            pending,
            anchor,
            cursor,
            count,
            suppress_text,
        )),
        (
            EditorVimPendingKey::VisualCharacterRegisterCommand {
                anchor,
                cursor,
                count,
                register,
            },
            key,
        ) => Some(handle_vim_visual_character_register_command_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            unnamed_register,
            last_change,
            anchor,
            cursor,
            count,
            register,
            suppress_text,
        )),
        (
            EditorVimPendingKey::VisualCharacterTextObject {
                anchor,
                cursor,
                count,
                scope,
            },
            key,
        ) => Some(handle_vim_visual_character_text_object_key_event(
            buffer,
            key,
            modifiers,
            pending,
            anchor,
            cursor,
            count,
            scope,
            suppress_text,
        )),
        _ => None,
    }
}

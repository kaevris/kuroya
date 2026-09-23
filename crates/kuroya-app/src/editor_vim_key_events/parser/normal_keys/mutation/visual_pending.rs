use eframe::egui::{Key, Modifiers};

use super::super::super::super::{
    EditorVimPendingKey, vim_visual_character_case_conversion, vim_visual_character_change_key,
    vim_visual_character_delete_key, vim_visual_character_indent_key,
    vim_visual_character_join_key, vim_visual_character_outdent_key,
};

pub(super) fn vim_visual_pending_key_can_mutate(
    key: Key,
    modifiers: Modifiers,
    pending: Option<EditorVimPendingKey>,
    printable_key_char: Option<char>,
) -> Option<bool> {
    if matches!(
        pending,
        Some(EditorVimPendingKey::VisualLine { .. } | EditorVimPendingKey::VisualLineGo { .. })
    ) {
        return Some(
            vim_visual_character_delete_key(key, modifiers)
                || vim_visual_character_indent_key(key, modifiers)
                || vim_visual_character_outdent_key(key, modifiers),
        );
    }
    if matches!(
        pending,
        Some(
            EditorVimPendingKey::VisualCharacter { .. }
                | EditorVimPendingKey::VisualCharacterCount { .. }
                | EditorVimPendingKey::VisualCharacterGo { .. }
        )
    ) {
        let joins_visual_selection = vim_visual_character_join_key(key, modifiers)
            && !matches!(pending, Some(EditorVimPendingKey::VisualCharacterGo { .. }));
        let indents_visual_selection = vim_visual_character_indent_key(key, modifiers);
        let outdents_visual_selection = vim_visual_character_outdent_key(key, modifiers);
        return Some(
            vim_visual_character_delete_key(key, modifiers)
                || vim_visual_character_change_key(key, modifiers)
                || joins_visual_selection
                || indents_visual_selection
                || outdents_visual_selection
                || vim_visual_character_case_conversion(key, modifiers).is_some(),
        );
    }
    if matches!(
        pending,
        Some(EditorVimPendingKey::VisualCharacterCharFind { .. })
    ) {
        return Some(false);
    }
    if matches!(
        pending,
        Some(EditorVimPendingKey::VisualCharacterRegisterCommand { .. })
    ) {
        return Some(
            vim_visual_character_delete_key(key, modifiers)
                || vim_visual_character_change_key(key, modifiers),
        );
    }
    if matches!(
        pending,
        Some(EditorVimPendingKey::VisualCharacterReplace { .. })
    ) {
        return Some(printable_key_char.is_some());
    }
    if matches!(
        pending,
        Some(
            EditorVimPendingKey::VisualCharacterTextObject { .. }
                | EditorVimPendingKey::VisualCharacterRegisterPrefix { .. }
        )
    ) {
        return Some(false);
    }
    None
}

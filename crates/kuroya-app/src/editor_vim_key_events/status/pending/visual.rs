use super::super::super::EditorVimPendingKey;
use super::super::labels::{
    push_count_prefix, push_optional_count, push_register_prefix, vim_char_find_motion_label,
    vim_text_object_scope_label,
};

pub(super) fn push_visual_pending_label(label: &mut String, pending: EditorVimPendingKey) -> bool {
    match pending {
        EditorVimPendingKey::VisualCharacter { .. } => label.push('v'),
        EditorVimPendingKey::VisualLine { count, .. } => {
            label.push('V');
            push_optional_count(label, count);
        }
        EditorVimPendingKey::VisualLineGo { count, .. } => {
            label.push('V');
            push_optional_count(label, count);
            label.push('g');
        }
        EditorVimPendingKey::VisualCharacterCount { count, .. } => {
            label.push('v');
            push_count_prefix(label, count);
        }
        EditorVimPendingKey::VisualCharacterGo { count, .. } => {
            label.push('v');
            push_optional_count(label, count);
            label.push('g');
        }
        EditorVimPendingKey::VisualCharacterReplace { .. } => label.push_str("vr"),
        EditorVimPendingKey::VisualCharacterCharFind { count, motion, .. } => {
            label.push('v');
            push_optional_count(label, count);
            label.push(vim_char_find_motion_label(motion));
        }
        EditorVimPendingKey::VisualCharacterTextObject { count, scope, .. } => {
            label.push('v');
            push_optional_count(label, count);
            label.push(vim_text_object_scope_label(scope));
        }
        EditorVimPendingKey::VisualCharacterRegisterPrefix { count, .. } => {
            label.push('v');
            push_optional_count(label, count);
            label.push('"');
        }
        EditorVimPendingKey::VisualCharacterRegisterCommand {
            count, register, ..
        } => {
            label.push('v');
            push_optional_count(label, count);
            push_register_prefix(label, register);
        }
        _ => return false,
    }
    true
}

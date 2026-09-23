use std::fmt::Write as _;

use super::super::state::vim_named_register_label;
use super::super::{
    EditorVimCaseConversion, EditorVimCharFindMotion, EditorVimNamedRegister,
    EditorVimOperatorGoKind, EditorVimTextObjectScope,
};

pub(super) fn push_counted_operator(label: &mut String, count: usize, operator: &str) {
    push_count_prefix(label, count);
    label.push_str(operator);
}

pub(super) fn push_count_prefix(label: &mut String, count: usize) {
    if count > 1 {
        let _ = write!(label, "{count}");
    }
}

pub(super) fn push_optional_count(label: &mut String, count: Option<usize>) {
    if let Some(count) = count {
        push_count_prefix(label, count);
    }
}

pub(super) fn push_vim_bounded_status_text(label: &mut String, text: &str, max_chars: usize) {
    let mut chars = text.chars();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else {
            return;
        };
        label.push(ch);
    }
    if chars.next().is_some() {
        label.push_str("...");
    }
}

pub(super) fn push_register_prefix(label: &mut String, register: EditorVimNamedRegister) {
    label.push('"');
    label.push(vim_named_register_label(register));
}
pub(super) fn push_text_object_prefix(
    label: &mut String,
    operator_count: usize,
    operator: &str,
    motion_count: usize,
    scope: EditorVimTextObjectScope,
) {
    push_count_prefix(label, operator_count);
    label.push_str(operator);
    push_count_prefix(label, motion_count);
    label.push(vim_text_object_scope_label(scope));
}

pub(super) fn push_char_find_operator_prefix(
    label: &mut String,
    operator_count: usize,
    operator: &str,
    motion_count: usize,
) {
    push_count_prefix(label, operator_count);
    label.push_str(operator);
    push_count_prefix(label, motion_count);
}

pub(super) fn vim_text_object_scope_label(scope: EditorVimTextObjectScope) -> char {
    match scope {
        EditorVimTextObjectScope::Inner => 'i',
        EditorVimTextObjectScope::Outer => 'a',
    }
}

pub(super) fn vim_char_find_motion_label(motion: EditorVimCharFindMotion) -> char {
    match motion {
        EditorVimCharFindMotion::FindBackward => 'F',
        EditorVimCharFindMotion::FindForward => 'f',
        EditorVimCharFindMotion::TillBackward => 'T',
        EditorVimCharFindMotion::TillForward => 't',
    }
}

pub(super) fn vim_case_operator_label(conversion: EditorVimCaseConversion) -> &'static str {
    match conversion {
        EditorVimCaseConversion::Lower => "gu",
        EditorVimCaseConversion::Upper => "gU",
        EditorVimCaseConversion::Toggle => "g~",
    }
}

pub(super) fn push_operator_go_label(label: &mut String, operator: EditorVimOperatorGoKind) {
    match operator {
        EditorVimOperatorGoKind::Change => label.push('c'),
        EditorVimOperatorGoKind::ChangeIntoRegister(register) => {
            push_register_prefix(label, register);
            label.push('c');
        }
        EditorVimOperatorGoKind::ConvertCase(conversion) => {
            label.push_str(vim_case_operator_label(conversion));
        }
        EditorVimOperatorGoKind::Delete => label.push('d'),
        EditorVimOperatorGoKind::DeleteIntoRegister(register) => {
            push_register_prefix(label, register);
            label.push('d');
        }
        EditorVimOperatorGoKind::ToggleCase => label.push_str("g~"),
        EditorVimOperatorGoKind::Yank => label.push('y'),
        EditorVimOperatorGoKind::YankIntoRegister(register) => {
            push_register_prefix(label, register);
            label.push('y');
        }
    }
}

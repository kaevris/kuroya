use std::fmt::Write as _;

use kuroya_core::EditorVimSettings;

mod case;
mod operator;
mod visual;

use self::case::push_case_pending_label;
use self::operator::push_operator_pending_label;
use self::visual::push_visual_pending_label;
use super::super::EditorVimPendingKey;
use super::super::command_input::VIM_COMMAND_INPUT;
use super::super::key_tokens::{vim_key_sequence_events, vim_key_token_for_event};
use super::labels::{
    push_count_prefix, push_counted_operator, push_operator_go_label, push_optional_count,
    push_register_prefix, push_vim_bounded_status_text,
};

const VIM_COMMAND_STATUS_QUERY_MAX_CHARS: usize = 96;

pub(crate) fn vim_pending_key_sequence_status_label(
    pending: Option<EditorVimPendingKey>,
    vim_settings: &EditorVimSettings,
) -> Option<String> {
    let EditorVimPendingKey::CustomKeySequence {
        binding_index,
        matched,
    } = pending?
    else {
        return None;
    };
    let binding = vim_settings.key_overrides.get(binding_index)?;
    let keys = vim_key_sequence_events(&binding.before)?;
    let matched = matched.min(keys.len());
    if matched == 0 {
        return None;
    }

    let mut label = String::new();
    let starts_with_space = keys
        .first()
        .copied()
        .is_some_and(|(key, modifiers)| vim_status_key_is_plain_space(key, modifiers));
    if starts_with_space {
        label.push_str("leader");
    } else {
        label.push_str("pending ");
        label.push_str(&vim_status_key_token(keys[0])?);
    }

    for key in keys.iter().copied().take(matched).skip(1) {
        label.push(' ');
        label.push_str(&vim_status_key_token(key)?);
    }
    Some(label)
}

pub(crate) fn vim_pending_command_status_label(
    pending: Option<EditorVimPendingKey>,
) -> Option<String> {
    if pending.is_none() {
        return super::super::command_input::vim_command_status_message();
    }
    let pending = pending?;
    if matches!(pending, EditorVimPendingKey::SearchInput { .. }) {
        return None;
    }
    if matches!(pending, EditorVimPendingKey::CommandInput) {
        return Some(VIM_COMMAND_INPUT.with(|input| {
            let command = input.borrow();
            let mut label = String::with_capacity(
                1 + command
                    .len()
                    .min(VIM_COMMAND_STATUS_QUERY_MAX_CHARS * 4 + "...".len()),
            );
            label.push(':');
            push_vim_bounded_status_text(&mut label, &command, VIM_COMMAND_STATUS_QUERY_MAX_CHARS);
            label
        }));
    }

    let mut label = String::new();
    if push_operator_pending_label(&mut label, pending)
        || push_case_pending_label(&mut label, pending)
        || push_visual_pending_label(&mut label, pending)
    {
        return (!label.is_empty()).then_some(label);
    }

    match pending {
        EditorVimPendingKey::Count(count) => {
            let _ = write!(label, "{count}");
        }
        EditorVimPendingKey::FindCharBackward(count) => {
            push_counted_operator(&mut label, count, "F")
        }
        EditorVimPendingKey::FindCharForward(count) => {
            push_counted_operator(&mut label, count, "f")
        }
        EditorVimPendingKey::TillCharBackward(count) => {
            push_counted_operator(&mut label, count, "T")
        }
        EditorVimPendingKey::TillCharForward(count) => {
            push_counted_operator(&mut label, count, "t")
        }
        EditorVimPendingKey::Go(count) => {
            push_optional_count(&mut label, count);
            label.push('g');
        }
        EditorVimPendingKey::IndentLine(count) => push_counted_operator(&mut label, count, ">"),
        EditorVimPendingKey::OutdentLine(count) => push_counted_operator(&mut label, count, "<"),
        EditorVimPendingKey::ReplaceChar(count) => push_counted_operator(&mut label, count, "r"),
        EditorVimPendingKey::JumpMark { linewise } => label.push(if linewise { '\'' } else { '`' }),
        EditorVimPendingKey::SetMark => label.push('m'),
        EditorVimPendingKey::RegisterPrefix(count) => {
            push_count_prefix(&mut label, count);
            label.push('"');
        }
        EditorVimPendingKey::RegisterCommand {
            prefix_count,
            command_count,
            register,
        } => {
            push_count_prefix(&mut label, prefix_count);
            push_register_prefix(&mut label, register);
            push_optional_count(&mut label, command_count);
        }
        EditorVimPendingKey::OperatorGoMotion {
            operator_count,
            motion_count,
            operator,
        } => {
            push_count_prefix(&mut label, operator_count);
            push_operator_go_label(&mut label, operator);
            push_count_prefix(&mut label, motion_count);
            label.push('g');
        }
        EditorVimPendingKey::CommandInput => {}
        EditorVimPendingKey::SearchInput { .. } => {}
        _ => {}
    }

    (!label.is_empty()).then_some(label)
}

pub(super) fn vim_pending_is_visual(pending: EditorVimPendingKey) -> bool {
    matches!(
        pending,
        EditorVimPendingKey::VisualCharacter { .. }
            | EditorVimPendingKey::VisualCharacterCount { .. }
            | EditorVimPendingKey::VisualCharacterGo { .. }
            | EditorVimPendingKey::VisualCharacterReplace { .. }
            | EditorVimPendingKey::VisualCharacterCharFind { .. }
            | EditorVimPendingKey::VisualCharacterTextObject { .. }
            | EditorVimPendingKey::VisualCharacterRegisterPrefix { .. }
            | EditorVimPendingKey::VisualCharacterRegisterCommand { .. }
            | EditorVimPendingKey::VisualLine { .. }
            | EditorVimPendingKey::VisualLineGo { .. }
            | EditorVimPendingKey::VisualLineReplace { .. }
    )
}

pub(super) fn vim_pending_is_replace(pending: EditorVimPendingKey) -> bool {
    matches!(
        pending,
        EditorVimPendingKey::ReplaceChar(_)
            | EditorVimPendingKey::VisualCharacterReplace { .. }
            | EditorVimPendingKey::VisualLineReplace { .. }
    )
}

fn vim_status_key_is_plain_space(
    key: eframe::egui::Key,
    modifiers: eframe::egui::Modifiers,
) -> bool {
    key == eframe::egui::Key::Space && modifiers == eframe::egui::Modifiers::NONE
}

fn vim_status_key_token(key: (eframe::egui::Key, eframe::egui::Modifiers)) -> Option<String> {
    let token = vim_key_token_for_event(key.0, key.1)?;
    if token == " " {
        Some("<Space>".to_owned())
    } else {
        Some(token)
    }
}

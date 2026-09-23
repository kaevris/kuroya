use eframe::egui::{Key, Modifiers};
use kuroya_core::TextBuffer;

use super::super::super::super::*;
use super::super::super::operator_motion::{
    VimPendingOperator, handle_vim_pending_operator_motion_transition,
};

pub(super) fn handle_vim_change_line_motion_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    operator_count: usize,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    if let Some(result) = handle_vim_pending_operator_motion_transition(
        key,
        modifiers,
        pending,
        VimPendingOperator::Change,
        None,
        operator_count,
        1,
        false,
        suppress_text,
    ) {
        return Some(result);
    }
    let motion = vim_change_motion_for_key(key, modifiers)?;
    Some(apply_change_operator_motion(
        buffer,
        mode,
        unnamed_register,
        last_change,
        operator_count,
        1,
        motion,
        suppress_text,
    ))
}

pub(super) fn handle_vim_change_line_into_register_motion_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    operator_count: usize,
    register: EditorVimNamedRegister,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    if let Some(result) = handle_vim_pending_operator_motion_transition(
        key,
        modifiers,
        pending,
        VimPendingOperator::Change,
        Some(register),
        operator_count,
        1,
        false,
        suppress_text,
    ) {
        return Some(result);
    }
    let motion = vim_change_motion_for_key(key, modifiers)?;
    Some(apply_change_operator_motion_into_register(
        buffer,
        mode,
        unnamed_register,
        last_change,
        operator_count,
        1,
        motion,
        register,
        suppress_text,
    ))
}

pub(super) fn handle_vim_change_motion_count_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    operator_count: usize,
    motion_count: usize,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    if let Some(result) = handle_vim_pending_operator_motion_transition(
        key,
        modifiers,
        pending,
        VimPendingOperator::Change,
        None,
        operator_count,
        motion_count,
        true,
        suppress_text,
    ) {
        return Some(result);
    }
    let motion = vim_change_motion_for_key(key, modifiers)?;
    Some(apply_change_operator_motion(
        buffer,
        mode,
        unnamed_register,
        last_change,
        operator_count,
        motion_count,
        motion,
        suppress_text,
    ))
}

pub(super) fn handle_vim_change_motion_count_into_register_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    operator_count: usize,
    motion_count: usize,
    register: EditorVimNamedRegister,
    suppress_text: Option<char>,
) -> Option<VimKeyResult> {
    if let Some(result) = handle_vim_pending_operator_motion_transition(
        key,
        modifiers,
        pending,
        VimPendingOperator::Change,
        Some(register),
        operator_count,
        motion_count,
        true,
        suppress_text,
    ) {
        return Some(result);
    }
    let motion = vim_change_motion_for_key(key, modifiers)?;
    Some(apply_change_operator_motion_into_register(
        buffer,
        mode,
        unnamed_register,
        last_change,
        operator_count,
        motion_count,
        motion,
        register,
        suppress_text,
    ))
}

fn vim_change_motion_for_key(key: Key, modifiers: Modifiers) -> Option<EditorVimOperatorMotion> {
    let motion = vim_operator_motion_for_key(key, modifiers)?;
    Some(match motion {
        EditorVimOperatorMotion::WordForward => EditorVimOperatorMotion::WordForwardChange,
        other => other,
    })
}

fn apply_change_operator_motion(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    operator_count: usize,
    motion_count: usize,
    motion: EditorVimOperatorMotion,
    suppress_text: Option<char>,
) -> VimKeyResult {
    let count = vim_combined_count(operator_count, motion_count);
    let changed = vim_apply_change_operator_motion(
        buffer,
        operator_count,
        motion_count,
        motion,
        unnamed_register,
    );
    *mode = EditorVimMode::Insert;
    vim_repeatable_change_result(
        changed,
        last_change,
        EditorVimRepeatAction::ChangeOperatorMotion(motion),
        count,
        suppress_text,
    )
}

fn apply_change_operator_motion_into_register(
    buffer: &mut TextBuffer,
    mode: &mut EditorVimMode,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    operator_count: usize,
    motion_count: usize,
    motion: EditorVimOperatorMotion,
    register: EditorVimNamedRegister,
    suppress_text: Option<char>,
) -> VimKeyResult {
    let count = vim_combined_count(operator_count, motion_count);
    let changed = vim_apply_change_operator_motion_into_named_register(
        buffer,
        operator_count,
        motion_count,
        motion,
        unnamed_register,
        register,
    );
    *mode = EditorVimMode::Insert;
    vim_repeatable_change_result(
        changed,
        last_change,
        EditorVimRepeatAction::ChangeOperatorMotionIntoRegister { motion, register },
        count,
        suppress_text,
    )
}

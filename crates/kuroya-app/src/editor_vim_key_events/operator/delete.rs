use kuroya_core::TextBuffer;

use super::super::commands::{
    vim_change_line_span_into_registers, vim_delete_line_span_into_registers,
};
use super::super::{
    EditorVimNamedRegister, EditorVimOperatorMotion, EditorVimRegister, EditorVimRegisterKind,
    EditorVimTextObjectKind, EditorVimTextObjectScope, vim_combined_count,
};
use super::motions::{vim_operator_motion_is_linewise, vim_operator_motion_range};
use super::registers::vim_delete_range_into_register;
use super::text_objects::vim_text_object_range;

pub(in crate::editor_vim_key_events) fn vim_apply_operator_motion(
    buffer: &mut TextBuffer,
    operator_count: usize,
    motion_count: usize,
    motion: EditorVimOperatorMotion,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> bool {
    vim_apply_operator_motion_into_registers(
        buffer,
        operator_count,
        motion_count,
        motion,
        unnamed_register,
        None,
    )
}

pub(in crate::editor_vim_key_events) fn vim_apply_change_operator_motion(
    buffer: &mut TextBuffer,
    operator_count: usize,
    motion_count: usize,
    motion: EditorVimOperatorMotion,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> bool {
    if vim_operator_motion_is_linewise(motion) {
        let count = vim_combined_count(operator_count, motion_count);
        let Some(range) = vim_operator_motion_range(buffer, count, motion) else {
            return false;
        };
        return vim_change_line_span_into_registers(buffer, range, unnamed_register, None);
    }
    vim_apply_operator_motion_into_registers(
        buffer,
        operator_count,
        motion_count,
        motion,
        unnamed_register,
        None,
    )
}

pub(in crate::editor_vim_key_events) fn vim_apply_change_operator_motion_into_named_register(
    buffer: &mut TextBuffer,
    operator_count: usize,
    motion_count: usize,
    motion: EditorVimOperatorMotion,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: EditorVimNamedRegister,
) -> bool {
    if vim_operator_motion_is_linewise(motion) {
        let count = vim_combined_count(operator_count, motion_count);
        let Some(range) = vim_operator_motion_range(buffer, count, motion) else {
            return false;
        };
        return vim_change_line_span_into_registers(
            buffer,
            range,
            unnamed_register,
            Some(named_register),
        );
    }
    vim_apply_operator_motion_into_named_register(
        buffer,
        operator_count,
        motion_count,
        motion,
        unnamed_register,
        named_register,
    )
}

pub(in crate::editor_vim_key_events) fn vim_apply_operator_motion_into_named_register(
    buffer: &mut TextBuffer,
    operator_count: usize,
    motion_count: usize,
    motion: EditorVimOperatorMotion,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: EditorVimNamedRegister,
) -> bool {
    vim_apply_operator_motion_into_registers(
        buffer,
        operator_count,
        motion_count,
        motion,
        unnamed_register,
        Some(named_register),
    )
}

fn vim_apply_operator_motion_into_registers(
    buffer: &mut TextBuffer,
    operator_count: usize,
    motion_count: usize,
    motion: EditorVimOperatorMotion,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
) -> bool {
    let count = vim_combined_count(operator_count, motion_count);
    let Some(range) = vim_operator_motion_range(buffer, count, motion) else {
        return false;
    };
    if vim_operator_motion_is_linewise(motion) {
        return vim_delete_line_span_into_registers(
            buffer,
            range,
            unnamed_register,
            named_register,
        );
    }
    vim_delete_range_into_register(
        buffer,
        range,
        EditorVimRegisterKind::Characterwise,
        unnamed_register,
        named_register,
    )
}

pub(in crate::editor_vim_key_events) fn vim_apply_text_object(
    buffer: &mut TextBuffer,
    operator_count: usize,
    motion_count: usize,
    scope: EditorVimTextObjectScope,
    kind: EditorVimTextObjectKind,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> bool {
    vim_apply_text_object_into_registers(
        buffer,
        operator_count,
        motion_count,
        scope,
        kind,
        unnamed_register,
        None,
    )
}

pub(in crate::editor_vim_key_events) fn vim_apply_text_object_into_named_register(
    buffer: &mut TextBuffer,
    operator_count: usize,
    motion_count: usize,
    scope: EditorVimTextObjectScope,
    kind: EditorVimTextObjectKind,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: EditorVimNamedRegister,
) -> bool {
    vim_apply_text_object_into_registers(
        buffer,
        operator_count,
        motion_count,
        scope,
        kind,
        unnamed_register,
        Some(named_register),
    )
}

fn vim_apply_text_object_into_registers(
    buffer: &mut TextBuffer,
    operator_count: usize,
    motion_count: usize,
    scope: EditorVimTextObjectScope,
    kind: EditorVimTextObjectKind,
    unnamed_register: &mut Option<EditorVimRegister>,
    named_register: Option<EditorVimNamedRegister>,
) -> bool {
    let count = vim_combined_count(operator_count, motion_count);
    let Some(range) = vim_text_object_range(buffer, count, scope, kind) else {
        return false;
    };
    vim_delete_range_into_register(
        buffer,
        range,
        EditorVimRegisterKind::Characterwise,
        unnamed_register,
        named_register,
    )
}

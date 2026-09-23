use eframe::egui::{Key, Modifiers};
use kuroya_core::EditorVimSettings;
use kuroya_core::TextBuffer;

mod command_input;
mod commands;
mod direct;
mod input_edit;
mod insert;
mod key_tokens;
mod keys;
mod motion;
mod normal;
mod operator;
mod parser;
mod pending;
mod repeat;
mod scan;
mod search;
mod settings_overrides;
mod state;
mod status;
mod types;
mod visual;

pub(crate) use self::command_input::vim_clear_command_input;
use self::command_input::{
    handle_vim_command_input_key_event, vim_command_input_accept_key, vim_command_input_cancel_key,
    vim_command_input_control_edit,
};
use self::commands::vim_replay_counted_insert_session;
use self::commands::{
    vim_change_lines_into_named_register, vim_change_lines_into_register,
    vim_delete_lines_into_named_register, vim_delete_lines_into_register, vim_put_register_after,
    vim_put_register_before, vim_yank_lines, vim_yank_lines_into_named_register,
};
use self::insert::handle_vim_insert_key_event;
#[cfg(test)]
pub(crate) use self::insert::{
    vim_record_insert_replay_key_with_auto_indent, vim_record_inserted_text,
};
pub(crate) use self::insert::{
    vim_record_insert_replay_key_with_auto_indent_for_buffer, vim_record_inserted_text_for_buffer,
};
pub(crate) use self::key_tokens::{
    vim_key_sequence_is_single_supported, vim_key_sequence_is_supported,
    vim_key_sequence_starts_with, vim_key_sequences_match, vim_key_token_for_event,
};
use self::keys::{
    insert_mode_key_can_mutate, no_text_modifiers, vim_escape_key, vim_go_to_line,
    vim_insert_delete_char_backward_key, vim_insert_delete_line_backward_key,
    vim_insert_delete_word_backward_key, vim_line_column_motion_key, vim_line_range_for_count,
    vim_normal_key_is_handled, vim_normal_key_next_mode, vim_printable_key_char,
    vim_replacement_key_char, vim_search_direction_for_key,
};
use self::motion::{
    vim_apply_char_find, vim_case_conversion_repeated_operator_key, vim_change_word_motion_range,
    vim_char_at, vim_convert_case_lines, vim_convert_case_range, vim_ctrl_scroll_lines,
    vim_delete_backward_chars_into_named_register, vim_delete_forward_chars_into_named_register,
    vim_delete_line_backward, vim_indent_lines, vim_join_lines_without_whitespace,
    vim_line_column_motion_char, vim_line_first_non_whitespace_char, vim_line_outdent_len,
    vim_line_scroll_lines, vim_matching_bracket_range, vim_move_counted_line_first_non_whitespace,
    vim_move_down_lines, vim_move_next_line_first_non_whitespace, vim_move_next_paragraph,
    vim_move_previous_big_word_end, vim_move_previous_line_first_non_whitespace,
    vim_move_previous_paragraph, vim_move_space_backward, vim_move_space_forward,
    vim_move_to_line_column, vim_move_to_matching_bracket, vim_move_up_lines,
    vim_next_paragraph_line, vim_outdent_lines, vim_page_scroll_lines, vim_previous_paragraph_line,
    vim_replace_forward_chars, vim_toggle_case_range, vim_word_motion_range,
};
#[cfg(test)]
use self::motion::{vim_open_line_above_text, vim_open_line_below_text};
use self::normal::handle_vim_normal_key_event;
use self::operator::{
    vim_apply_change_operator_motion, vim_apply_change_operator_motion_into_named_register,
    vim_apply_operator_motion, vim_apply_operator_motion_into_named_register,
    vim_apply_text_object, vim_apply_text_object_into_named_register,
    vim_convert_case_operator_motion, vim_convert_case_text_object, vim_delete_range_into_register,
    vim_delete_to_line_end_into_named_register, vim_text_object_range,
    vim_toggle_case_operator_motion, vim_toggle_case_text_object, vim_yank_operator_motion,
    vim_yank_operator_motion_into_named_register, vim_yank_range_into_register,
    vim_yank_text_object, vim_yank_text_object_into_named_register,
};
use self::parser::{
    vim_combined_count, vim_count_digit, vim_normal_key_can_mutate, vim_normal_key_next_pending,
    vim_normal_key_next_pending_after_count, vim_operator_char_find_motion_for_key,
    vim_operator_go_motion_for_key, vim_operator_last_find_motion_for_key,
    vim_operator_motion_for_key, vim_pending_key_accepts, vim_pending_key_last_find_operator_go,
    vim_pending_key_next_char_find, vim_pending_key_next_named_register,
    vim_pending_key_next_operator_count, vim_pending_key_next_operator_go,
    vim_pending_key_next_text_object, vim_push_count_digit, vim_register_command_count,
    vim_register_command_next_count, vim_text_object_kind_for_key, vim_text_object_scope_for_key,
};
use self::pending::handle_vim_pending_or_direct_normal_key_event;
use self::repeat::{
    handle_vim_operator_go_motion_key_event, vim_record_insert_change, vim_repeatable_change_result,
};
#[cfg(test)]
use self::scan::vim_events_include_mutation;
pub(crate) use self::scan::{
    vim_events_include_mutation_with_settings, vim_key_sequence_is_normal_mode_supported,
    vim_text_after_suppression,
};
#[cfg(test)]
pub(crate) use self::search::vim_last_search_word_for_test;
pub(crate) use self::search::vim_pending_search_status_label;
pub(crate) use self::search::vim_search_highlight_ranges_for_buffer;
#[cfg(test)]
pub(crate) use self::search::vim_set_last_search_for_test;
#[cfg(test)]
use self::search::{VIM_SEARCH_INPUT, VIM_SEARCHES, vim_search_word_target, vim_set_last_search};
use self::search::{
    handle_vim_search_input_key_event, vim_operator_search_match_range,
    vim_operator_search_repeat_range, vim_operator_search_word_under_cursor_range,
    vim_repeat_last_search, vim_search_input_accept_key, vim_search_input_cancel_key,
    vim_search_input_control_edit, vim_search_match_range, vim_search_word_under_cursor,
};
pub(crate) use self::search::{
    vim_clear_search_input, vim_clear_searches, vim_forget_searches_for_buffer,
};
pub(crate) use self::settings_overrides::sanitize_vim_settings_for_runtime;
use self::settings_overrides::{
    VimSettingsPreflightAction, handle_vim_insert_escape_override, handle_vim_key_override,
    vim_settings_preflight_action,
};
#[cfg(test)]
pub(crate) use self::state::vim_marks_are_empty_for_test;
pub(crate) use self::state::{
    vim_clear_marks, vim_clear_named_registers, vim_forget_marks_for_buffer,
};
use self::state::{
    vim_jump_mark_name_for_key, vim_jump_to_mark, vim_mark_name_for_key, vim_named_register,
    vim_named_register_for_key, vim_set_mark, vim_set_previous_context_mark,
};
pub(crate) use self::status::{
    vim_effective_cursor_style, vim_mode_status_label, vim_pending_command_status_label,
    vim_pending_key_sequence_status_label,
};
pub(crate) use self::types::{
    EditorVimCaseConversion, EditorVimCharFind, EditorVimCharFindMotion, EditorVimInsertReplayStep,
    EditorVimLastChange, EditorVimMode, EditorVimNamedRegister, EditorVimOperatorGoKind,
    EditorVimOperatorMotion, EditorVimPendingKey, EditorVimRegister, EditorVimRegisterKind,
    EditorVimRepeatAction, EditorVimTextObjectKind, EditorVimTextObjectScope, VimKeyResult,
};
use self::types::{VIM_DEFAULT_CTRL_SCROLL_LINES, VIM_DEFAULT_PAGE_SCROLL_LINES, VIM_MAX_COUNT};

use self::visual::{
    handle_vim_visual_character_char_find_key_event, handle_vim_visual_character_go_key_event,
    handle_vim_visual_character_key_event, handle_vim_visual_character_register_command_key_event,
    handle_vim_visual_character_register_prefix_key_event,
    handle_vim_visual_character_replace_key_event,
    handle_vim_visual_character_text_object_key_event, handle_vim_visual_line_go_key_event,
    handle_vim_visual_line_key_event, handle_vim_visual_line_replace_key_event,
    vim_cancel_pending_visual_character, vim_set_visual_character_selection,
    vim_visual_character_case_conversion, vim_visual_character_change_key,
    vim_visual_character_delete_key, vim_visual_character_indent_key,
    vim_visual_character_join_key, vim_visual_character_outdent_key, vim_visual_pending_after_key,
};

pub(crate) fn vim_collapse_selection_for_insert(buffer: &mut TextBuffer) {
    if !buffer.has_selection() {
        return;
    }

    let cursors = buffer
        .selections()
        .iter()
        .map(|selection| selection.cursor)
        .collect::<Vec<_>>();
    buffer.set_cursors(cursors);
}

pub(crate) fn vim_collapse_multi_cursor_selection(buffer: &mut TextBuffer) {
    if buffer.selections().len() > 1 {
        buffer.set_single_cursor(buffer.cursor());
    }
}

#[cfg(test)]
pub(crate) fn vim_search_input_text_for_test() -> String {
    VIM_SEARCH_INPUT.with(|input| input.borrow().clone())
}

#[cfg(test)]
pub(crate) fn vim_set_search_input_text_for_test(text: &str) {
    VIM_SEARCH_INPUT.with(|input| *input.borrow_mut() = text.to_owned());
}

#[cfg(test)]
pub(crate) fn vim_command_input_text_for_test() -> String {
    self::command_input::VIM_COMMAND_INPUT.with(|input| input.borrow().clone())
}

#[cfg(test)]
pub(crate) fn vim_set_command_input_text_for_test(text: &str) {
    self::command_input::VIM_COMMAND_INPUT.with(|input| *input.borrow_mut() = text.to_owned());
}

#[cfg(test)]
fn handle_vim_editor_key_event(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
) -> VimKeyResult {
    let mut last_char_find = None;
    let mut unnamed_register = None;
    handle_vim_editor_key_event_with_state(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        &mut last_char_find,
        &mut unnamed_register,
    )
}

#[cfg(test)]
fn handle_vim_editor_key_event_with_state(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
) -> VimKeyResult {
    let mut last_change = None;
    handle_vim_editor_key_event_with_repeat_state(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_char_find,
        unnamed_register,
        &mut last_change,
    )
}

#[cfg(test)]
fn handle_vim_editor_key_event_with_repeat_state(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
) -> VimKeyResult {
    handle_vim_editor_key_event_with_state_and_indent(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_char_find,
        unnamed_register,
        last_change,
        "    ",
    )
}

#[cfg(test)]
fn handle_vim_editor_key_event_with_state_and_indent(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    indent_unit: &str,
) -> VimKeyResult {
    let default_settings = EditorVimSettings::default();
    handle_vim_editor_key_event_with_settings_and_indent(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_char_find,
        unnamed_register,
        last_change,
        &default_settings,
        indent_unit,
    )
}

pub(crate) fn handle_vim_editor_key_event_with_settings_and_indent(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    vim_settings: &EditorVimSettings,
    indent_unit: &str,
) -> VimKeyResult {
    buffer.begin_undo_group();
    let was_insert = matches!(*mode, EditorVimMode::Insert);
    let result = handle_vim_editor_key_event_grouped(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_char_find,
        unnamed_register,
        last_change,
        vim_settings,
        indent_unit,
    );
    if !matches!(*mode, EditorVimMode::Insert) {
        buffer.end_undo_group();
    } else if was_insert {
        buffer.begin_undo_group();
    }
    result
}

fn handle_vim_editor_key_event_grouped(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    vim_settings: &EditorVimSettings,
    indent_unit: &str,
) -> VimKeyResult {
    if let Some(result) = handle_vim_insert_escape_override(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_char_find,
        unnamed_register,
        last_change,
        vim_settings,
        indent_unit,
    ) {
        return result;
    }

    if let Some(result) = handle_vim_key_override(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_char_find,
        unnamed_register,
        last_change,
        vim_settings,
        indent_unit,
    ) {
        return result;
    }

    handle_vim_editor_key_event_without_overrides(
        buffer,
        key,
        modifiers,
        mode,
        pending,
        last_char_find,
        unnamed_register,
        last_change,
        indent_unit,
    )
}

fn handle_vim_editor_key_event_without_overrides(
    buffer: &mut TextBuffer,
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    last_char_find: &mut Option<EditorVimCharFind>,
    unnamed_register: &mut Option<EditorVimRegister>,
    last_change: &mut Option<EditorVimLastChange>,
    indent_unit: &str,
) -> VimKeyResult {
    let result = match *mode {
        EditorVimMode::Insert => handle_vim_insert_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            last_char_find,
            last_change,
            indent_unit,
        ),
        EditorVimMode::Normal => handle_vim_normal_key_event(
            buffer,
            key,
            modifiers,
            mode,
            pending,
            last_char_find,
            unnamed_register,
            last_change,
            indent_unit,
        ),
    };

    if matches!(*mode, EditorVimMode::Insert) {
        vim_collapse_selection_for_insert(buffer);
    }

    result
}

#[cfg(test)]
mod tests;

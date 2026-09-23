mod char_find;
mod character;
mod character_action;
mod character_navigation;
mod go;
mod keys;
mod line;
mod motion;
mod ops;
mod pending_state;
mod register;
mod replace;
mod selection;
mod text_object;

pub(super) use self::char_find::handle_vim_visual_character_char_find_key_event;
pub(super) use self::character::handle_vim_visual_character_key_event;
pub(super) use self::go::handle_vim_visual_character_go_key_event;
pub(super) use self::keys::{
    vim_visual_character_case_conversion, vim_visual_character_change_key,
    vim_visual_character_char_find_repeat_key, vim_visual_character_delete_key,
    vim_visual_character_indent_key, vim_visual_character_join_key,
    vim_visual_character_outdent_key, vim_visual_character_replace_key,
    vim_visual_character_swap_key, vim_visual_character_toggle_key, vim_visual_character_yank_key,
};
pub(super) use self::line::{
    handle_vim_visual_line_go_key_event, handle_vim_visual_line_key_event,
    vim_set_visual_line_selection,
};
pub(super) use self::motion::{
    vim_visual_character_char_find_target, vim_visual_character_motion_target,
};
pub(super) use self::ops::{
    vim_convert_case_visual_character, vim_delete_visual_character,
    vim_delete_visual_character_into_named_register, vim_indent_visual_character_lines,
    vim_join_visual_character_lines, vim_join_visual_character_lines_without_whitespace,
    vim_outdent_visual_character_lines, vim_replace_visual_character, vim_replace_visual_line_span,
    vim_yank_visual_character, vim_yank_visual_character_into_named_register,
};
pub(super) use self::pending_state::{
    vim_cancel_pending_visual_character, vim_restore_visual_character_pending,
    vim_visual_pending_after_key,
};
pub(super) use self::register::{
    handle_vim_visual_character_register_command_key_event,
    handle_vim_visual_character_register_prefix_key_event,
};
pub(super) use self::replace::{
    handle_vim_visual_character_replace_key_event, handle_vim_visual_line_replace_key_event,
};
pub(super) use self::selection::{
    vim_exit_visual_selection, vim_record_visual_bounds_marks, vim_set_visual_character_selection,
    vim_visual_character_clamped_cursor, vim_visual_character_join_repeat_count,
    vim_visual_character_line_repeat_count, vim_visual_character_line_span,
    vim_visual_character_repeat_count,
};
pub(super) use self::text_object::handle_vim_visual_character_text_object_key_event;

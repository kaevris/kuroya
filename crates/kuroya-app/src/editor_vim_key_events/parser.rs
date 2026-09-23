mod counts;
mod motions;
mod normal_keys;
mod pending;
mod text_objects;

pub(super) use self::counts::{
    vim_combined_count, vim_count_digit, vim_push_count_digit, vim_register_command_count,
    vim_register_command_next_count,
};
pub(super) use self::motions::{
    vim_operator_char_find_motion_for_key, vim_operator_go_motion_for_key,
    vim_operator_last_find_motion_for_key, vim_operator_motion_for_key,
    vim_pending_key_last_find_operator_go,
};
pub(super) use self::normal_keys::{
    vim_normal_key_can_mutate, vim_normal_key_next_pending, vim_normal_key_next_pending_after_count,
};
pub(super) use self::pending::{
    vim_pending_key_accepts, vim_pending_key_next_char_find, vim_pending_key_next_named_register,
    vim_pending_key_next_operator_count, vim_pending_key_next_operator_go,
    vim_pending_key_next_text_object,
};
pub(super) use self::text_objects::{vim_text_object_kind_for_key, vim_text_object_scope_for_key};

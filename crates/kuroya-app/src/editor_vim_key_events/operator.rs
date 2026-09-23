mod case;
mod delete;
mod motions;
mod registers;
mod text_objects;
mod yank;

pub(super) use case::{
    vim_convert_case_operator_motion, vim_convert_case_text_object,
    vim_toggle_case_operator_motion, vim_toggle_case_text_object,
};
pub(super) use delete::{
    vim_apply_change_operator_motion, vim_apply_change_operator_motion_into_named_register,
    vim_apply_operator_motion, vim_apply_operator_motion_into_named_register,
    vim_apply_text_object, vim_apply_text_object_into_named_register,
};
pub(super) use registers::{
    vim_delete_range_into_register, vim_delete_to_line_end,
    vim_delete_to_line_end_into_named_register, vim_yank_range_into_register,
};
pub(super) use text_objects::vim_text_object_range;
pub(super) use yank::{
    vim_yank_operator_motion, vim_yank_operator_motion_into_named_register, vim_yank_text_object,
    vim_yank_text_object_into_named_register,
};

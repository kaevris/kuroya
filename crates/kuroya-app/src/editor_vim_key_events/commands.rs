mod apply_last;
mod insert_replay;
mod linewise_registers;
mod put;

pub(super) use self::apply_last::vim_apply_last_change;
pub(super) use self::insert_replay::vim_replay_counted_insert_session;
pub(super) use self::linewise_registers::{
    vim_change_line_span_into_registers, vim_change_lines_into_named_register,
    vim_change_lines_into_register, vim_delete_line_span_into_registers,
    vim_delete_lines_into_named_register, vim_delete_lines_into_register,
    vim_yank_line_span_into_registers, vim_yank_lines, vim_yank_lines_into_named_register,
};
pub(super) use self::put::{vim_put_register_after, vim_put_register_before};

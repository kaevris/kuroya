mod read;
mod write;

pub(in crate::lsp_client) use read::{LspMessageReadBuffer, read_message};
pub(in crate::lsp_client) use write::{
    write_did_change_full_document, write_did_change_incremental, write_did_open_full_document,
    write_message,
};

pub(in crate::lsp_client) fn lsp_version(version: u64) -> i32 {
    version.min(i32::MAX as u64) as i32
}

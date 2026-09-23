use crate::lsp_client::{commands::LspClientCommand, handle::LspClientHandle};
use kuroya_core::BufferId;
use std::path::PathBuf;

impl LspClientHandle {
    pub fn prepare_rename(
        &self,
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
    ) -> bool {
        self.queue_command(LspClientCommand::PrepareRename {
            id,
            path,
            version,
            line,
            character,
        })
    }

    pub fn rename(
        &self,
        id: BufferId,
        path: PathBuf,
        version: u64,
        line: usize,
        character: usize,
        new_name: String,
    ) -> bool {
        self.queue_command(LspClientCommand::Rename {
            id,
            path,
            version,
            line,
            character,
            new_name,
        })
    }
}

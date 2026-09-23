use crate::lsp_client::pending::PendingLspRequests;
mod call_hierarchy;
mod family;
mod info;
mod references;
mod rename;
mod type_hierarchy;

use crate::lsp_client::commands::LspClientCommand;
use family::{NavigationRequestFamily, navigation_request_family};
use tokio::process::ChildStdin;

pub(super) async fn handle_navigation_request_command(
    command: LspClientCommand,
    writer: &mut ChildStdin,
    next_request_id: &mut u64,
    pending_requests: &mut PendingLspRequests,
) -> bool {
    let Some(family) = navigation_request_family(&command) else {
        return true;
    };

    match family {
        NavigationRequestFamily::Info => {
            info::handle_info_navigation_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        NavigationRequestFamily::CallHierarchy => {
            call_hierarchy::handle_call_hierarchy_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        NavigationRequestFamily::TypeHierarchy => {
            type_hierarchy::handle_type_hierarchy_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        NavigationRequestFamily::References => {
            references::handle_references_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
        NavigationRequestFamily::Rename => {
            rename::handle_rename_request_command(
                command,
                writer,
                next_request_id,
                pending_requests,
            )
            .await
        }
    }
}

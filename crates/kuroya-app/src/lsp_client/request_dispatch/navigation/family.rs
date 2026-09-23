use crate::lsp_client::commands::LspClientCommand;

pub(super) enum NavigationRequestFamily {
    Info,
    CallHierarchy,
    TypeHierarchy,
    References,
    Rename,
}

pub(super) fn navigation_request_family(
    command: &LspClientCommand,
) -> Option<NavigationRequestFamily> {
    match command {
        LspClientCommand::Hover { .. }
        | LspClientCommand::DocumentHighlights { .. }
        | LspClientCommand::Definition { .. } => Some(NavigationRequestFamily::Info),
        LspClientCommand::PrepareCallHierarchy { .. }
        | LspClientCommand::CallHierarchyIncoming { .. }
        | LspClientCommand::CallHierarchyOutgoing { .. } => {
            Some(NavigationRequestFamily::CallHierarchy)
        }
        LspClientCommand::PrepareTypeHierarchy { .. }
        | LspClientCommand::TypeHierarchySupertypes { .. }
        | LspClientCommand::TypeHierarchySubtypes { .. } => {
            Some(NavigationRequestFamily::TypeHierarchy)
        }
        LspClientCommand::References { .. } => Some(NavigationRequestFamily::References),
        LspClientCommand::PrepareRename { .. } | LspClientCommand::Rename { .. } => {
            Some(NavigationRequestFamily::Rename)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::navigation_request_family;
    use crate::lsp_client::commands::LspClientCommand;

    #[test]
    fn navigation_request_routing_ignores_non_navigation_commands() {
        assert!(navigation_request_family(&LspClientCommand::Shutdown).is_none());
    }
}

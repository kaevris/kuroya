use crate::{
    KuroyaApp,
    path_display::display_error_label_cow,
    save_lifecycle::{
        finish_session_save, note_session_save_failed, note_session_save_succeeded,
        with_session_save_rotation_order,
    },
    workspace_state::workspace_event_matches,
};
use std::path::{Path, PathBuf};

pub(super) fn handle_session_saved_event(app: &mut KuroyaApp, root: PathBuf) {
    if let Some(fingerprint) = note_session_save_succeeded(&root) {
        app.last_saved_session_structure_fingerprint = Some(fingerprint);
    }
    finish_session_save_and_start_next(app, &root);
}

pub(super) fn handle_session_save_failed_event(app: &mut KuroyaApp, root: PathBuf, error: String) {
    let is_current_workspace = workspace_event_matches(&app.workspace.root, &root);
    if is_current_workspace {
        app.status = session_save_failure_status(&error);
        app.last_saved_session_structure_fingerprint = None;
    }
    finish_session_save_and_start_next(app, &root);
    if is_current_workspace && note_session_save_failed(&root) {
        app.request_session_save(root, app.build_session_save_snapshot());
    }
}

fn finish_session_save_and_start_next(app: &mut KuroyaApp, root: &Path) {
    let was_current = app.session_save_in_flight.as_deref() == Some(root);
    let next = with_session_save_rotation_order(|order| {
        finish_session_save(
            root,
            &mut app.session_save_in_flight,
            &mut app.queued_session_saves,
            order,
        )
    });
    if let Some((next_root, next_session)) = next {
        app.spawn_session_save(next_root, next_session);
    } else if was_current {
        app.session_save_in_flight_snapshot = None;
        app.session_save_in_flight_task = None;
    }
}

fn session_save_failure_status(error: &str) -> String {
    let error = display_error_label_cow(error);
    format!("Could not save session: {}", error.as_ref())
}

#[cfg(test)]
mod tests {
    use super::session_save_failure_status;
    use crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS;

    #[test]
    fn session_save_failure_status_sanitizes_and_bounds_error_detail() {
        let status = session_save_failure_status(&format!(
            "first line\nsecond line \u{202e}{}",
            "x".repeat(400)
        ));

        assert!(status.starts_with("Could not save session: first line "));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Could not save session: ".chars().count() + DISPLAY_ERROR_LABEL_MAX_CHARS
        );
    }

    #[test]
    fn session_save_failure_status_falls_back_for_blank_error_detail() {
        assert_eq!(
            session_save_failure_status("\n\u{202e}\u{0007}"),
            "Could not save session: unknown error"
        );
    }
}

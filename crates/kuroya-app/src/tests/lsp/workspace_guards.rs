use crate::lsp_event_handler::lsp_status_event_matches;
use crate::workspace_state::{
    active_buffer_path_matches, active_buffer_path_version_matches, buffer_id_path_version_matches,
    lsp_event_path_is_current,
};
use kuroya_core::TextBuffer;
use std::path::{Path, PathBuf};

#[test]
fn lsp_event_paths_must_belong_to_current_workspace() {
    let root = Path::new("workspace/current");

    assert!(lsp_event_path_is_current(
        root,
        Path::new("workspace/current/src/main.rs")
    ));
    assert!(!lsp_event_path_is_current(
        root,
        Path::new("workspace/old/src/main.rs")
    ));
}

#[test]
fn lsp_buffer_guards_match_active_path_and_version() {
    let path = PathBuf::from("workspace/src/main.rs");
    let mut buffer = TextBuffer::from_text(3, Some(path.clone()), "fn main() {}".to_owned());
    let version = buffer.version();
    let other = TextBuffer::from_text(
        4,
        Some(PathBuf::from("workspace/src/lib.rs")),
        "".to_owned(),
    );

    assert!(active_buffer_path_matches(Some(&buffer), &path));
    assert!(!active_buffer_path_matches(Some(&other), &path));
    assert!(active_buffer_path_version_matches(
        Some(&buffer),
        &path,
        version
    ));
    assert!(buffer_id_path_version_matches(
        &[buffer.clone(), other.clone()],
        3,
        &path,
        version
    ));

    buffer.insert_at_cursor("\n");
    assert!(!active_buffer_path_version_matches(
        Some(&buffer),
        &path,
        version
    ));
    assert!(!buffer_id_path_version_matches(
        &[buffer, other],
        3,
        &path,
        version
    ));
}

#[test]
fn lsp_status_events_must_belong_to_current_workspace() {
    let root = Path::new("workspace/current");

    assert!(lsp_status_event_matches(root, root));
    assert!(lsp_status_event_matches(
        root,
        Path::new("workspace/current/src/..")
    ));
    assert!(!lsp_status_event_matches(root, Path::new("workspace/old")));
}

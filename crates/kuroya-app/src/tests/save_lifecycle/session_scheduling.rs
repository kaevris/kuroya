use super::session_for_test;
use crate::save_lifecycle::{
    SessionSaveRequest, finish_session_save, reserve_session_save, should_skip_session_persistence,
};
use std::{collections::HashMap, path::PathBuf};

#[test]
fn unchanged_session_fingerprint_skips_snapshot_build_when_idle() {
    assert!(should_skip_session_persistence(
        Some(7),
        Some(7),
        Some(7),
        false,
        false
    ));
}

#[test]
fn changed_session_fingerprint_rebuilds_snapshot() {
    assert!(!should_skip_session_persistence(
        Some(8),
        Some(7),
        Some(7),
        false,
        false
    ));
}

#[test]
fn missing_attempted_or_saved_fingerprint_never_skips() {
    assert!(!should_skip_session_persistence(
        Some(7),
        None,
        Some(7),
        false,
        false
    ));
    assert!(!should_skip_session_persistence(
        Some(7),
        Some(7),
        None,
        false,
        false
    ));
}

#[test]
fn failed_save_clears_saved_fingerprint_so_next_tick_retries() {
    assert!(should_skip_session_persistence(
        Some(7),
        Some(7),
        Some(7),
        false,
        false
    ));

    assert!(!should_skip_session_persistence(
        Some(7),
        Some(7),
        None,
        false,
        false
    ));
}

#[test]
fn in_flight_or_queued_saves_block_fingerprint_skip() {
    assert!(!should_skip_session_persistence(
        Some(7),
        Some(7),
        Some(7),
        true,
        false
    ));
    assert!(!should_skip_session_persistence(
        Some(7),
        Some(7),
        Some(7),
        false,
        true
    ));
}

#[test]
fn session_save_dispatch_interleaves_queued_roots_round_robin() {
    let root_a = PathBuf::from("workspace-a");
    let root_b = PathBuf::from("workspace-b");
    let root_c = PathBuf::from("workspace-c");
    let mut in_flight = Some(PathBuf::from("workspace-x"));
    let mut queued = HashMap::new();
    let mut order = Vec::new();

    for root in [&root_b, &root_a, &root_c] {
        assert_eq!(
            reserve_session_save(
                root,
                session_for_test(root, "queued.rs"),
                &mut in_flight,
                &mut queued,
                &mut order
            ),
            SessionSaveRequest::Queued
        );
    }
    assert_eq!(order, vec![root_b.clone(), root_a.clone(), root_c.clone()]);

    let mut dispatched = Vec::new();
    let mut finishing = PathBuf::from("workspace-x");
    for _ in 0..6 {
        let (root, _) = finish_session_save(&finishing, &mut in_flight, &mut queued, &mut order)
            .expect("fair dispatch");
        reserve_session_save(
            &root,
            session_for_test(&root, "updated.rs"),
            &mut in_flight,
            &mut queued,
            &mut order,
        );
        dispatched.push(root.clone());
        finishing = root;
    }

    assert_eq!(
        dispatched,
        vec![
            root_b.clone(),
            root_a.clone(),
            root_c.clone(),
            root_b.clone(),
            root_a.clone(),
            root_c,
        ]
    );
}

#[test]
fn session_save_finished_root_with_own_update_waits_behind_other_queued_roots() {
    let current_root = PathBuf::from("workspace-current");
    let other_root = PathBuf::from("workspace-other");
    let mut in_flight = Some(current_root.clone());
    let mut queued = HashMap::new();
    let mut order = Vec::new();

    assert_eq!(
        reserve_session_save(
            &current_root,
            session_for_test(&current_root, "own-update.rs"),
            &mut in_flight,
            &mut queued,
            &mut order
        ),
        SessionSaveRequest::Queued
    );
    assert_eq!(
        reserve_session_save(
            &other_root,
            session_for_test(&other_root, "waiting.rs"),
            &mut in_flight,
            &mut queued,
            &mut order
        ),
        SessionSaveRequest::Queued
    );

    let (first_dispatched, _) =
        finish_session_save(&current_root, &mut in_flight, &mut queued, &mut order)
            .expect("first dispatch");
    assert_eq!(first_dispatched, other_root);

    let (second_dispatched, second_session) =
        finish_session_save(&other_root, &mut in_flight, &mut queued, &mut order)
            .expect("second dispatch");
    assert_eq!(second_dispatched, current_root);
    assert_eq!(second_session.workspace_root, current_root);
    assert_eq!(in_flight, Some(current_root));
}

use super::session_for_test;
use crate::save_lifecycle::{
    FinishedSaveRequest, SaveRequest, SessionSaveRequest, finish_current_save_request,
    finish_save_request, finish_session_save, has_active_save_work, reserve_save_request,
    reserve_session_save,
};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[test]
fn session_save_requests_spawn_one_and_queue_latest_per_root() {
    let root_a = PathBuf::from("workspace-a");
    let root_b = PathBuf::from("workspace-b");
    let first = session_for_test(&root_a, "first.rs");
    let second = session_for_test(&root_a, "second.rs");
    let third = session_for_test(&root_a, "third.rs");
    let other = session_for_test(&root_b, "other.rs");
    let mut in_flight = None;
    let mut queued = HashMap::new();
    let mut order = Vec::new();

    assert_eq!(
        reserve_session_save(&root_a, first, &mut in_flight, &mut queued, &mut order),
        SessionSaveRequest::Spawn
    );
    assert_eq!(in_flight, Some(root_a.clone()));
    assert!(queued.is_empty());

    assert_eq!(
        reserve_session_save(&root_a, second, &mut in_flight, &mut queued, &mut order),
        SessionSaveRequest::Queued
    );
    assert_eq!(
        reserve_session_save(
            &root_a,
            third.clone(),
            &mut in_flight,
            &mut queued,
            &mut order
        ),
        SessionSaveRequest::Queued
    );
    assert_eq!(
        reserve_session_save(
            &root_b,
            other.clone(),
            &mut in_flight,
            &mut queued,
            &mut order
        ),
        SessionSaveRequest::Queued
    );
    assert_eq!(queued.get(&root_a), Some(&third));
    assert_eq!(queued.get(&root_b), Some(&other));
    assert_eq!(order, vec![root_a, root_b]);
}

#[test]
fn session_save_completion_rotates_to_longest_waiting_queued_root() {
    let root_a = PathBuf::from("workspace-a");
    let root_b = PathBuf::from("workspace-b");
    let mut in_flight = Some(root_a.clone());
    let mut queued = HashMap::new();
    queued.insert(root_a.clone(), session_for_test(&root_a, "queued-a.rs"));
    queued.insert(root_b.clone(), session_for_test(&root_b, "queued-b.rs"));
    let mut order = vec![root_a.clone(), root_b.clone()];

    assert_eq!(
        finish_session_save(
            Path::new("stale-workspace"),
            &mut in_flight,
            &mut queued,
            &mut order
        ),
        None
    );
    assert_eq!(in_flight, Some(root_a.clone()));

    let (first_dispatched, first_session) =
        finish_session_save(&root_a, &mut in_flight, &mut queued, &mut order)
            .expect("first dispatch");
    assert_eq!(first_dispatched, root_b);
    assert_eq!(first_session.workspace_root, root_b);
    assert_eq!(in_flight, Some(root_b.clone()));
    assert!(!queued.contains_key(&root_b));

    let (second_dispatched, second_session) =
        finish_session_save(&root_b, &mut in_flight, &mut queued, &mut order)
            .expect("second dispatch");
    assert_eq!(second_dispatched, root_a);
    assert_eq!(second_session.workspace_root, root_a);
    assert_eq!(in_flight, Some(root_a.clone()));
    assert!(queued.is_empty());
}

#[test]
fn session_save_completion_resumes_finished_roots_own_snapshot_when_queue_is_empty() {
    let root_a = PathBuf::from("workspace-a");
    let mut in_flight = Some(root_a.clone());
    let mut queued = HashMap::from([(root_a.clone(), session_for_test(&root_a, "own.rs"))]);
    let mut order = vec![root_a.clone()];

    let (dispatched, resumed) =
        finish_session_save(&root_a, &mut in_flight, &mut queued, &mut order)
            .expect("own snapshot dispatch");

    assert_eq!(dispatched, root_a);
    assert_eq!(resumed.workspace_root, root_a);
    assert_eq!(in_flight, Some(root_a));
}

#[test]
fn save_requests_are_serialized_per_buffer() {
    let first = PathBuf::from("workspace/src/main.rs");
    let second = PathBuf::from("workspace/src/lib.rs");
    let mut in_flight = HashSet::new();
    let mut queued = HashMap::new();

    assert_eq!(
        reserve_save_request(7, &first, &mut in_flight, &mut queued),
        SaveRequest::Spawn
    );
    assert!(in_flight.contains(&7));
    assert!(queued.is_empty());

    assert_eq!(
        reserve_save_request(7, &second, &mut in_flight, &mut queued),
        SaveRequest::Queued
    );
    assert_eq!(queued.get(&7), Some(&second));
    assert_eq!(
        finish_save_request(7, &mut in_flight, &mut queued),
        Some(second)
    );
    assert!(!in_flight.contains(&7));
    assert!(queued.is_empty());
}

#[test]
fn save_request_queue_keeps_latest_requested_path() {
    let first = PathBuf::from("workspace/src/main.rs");
    let second = PathBuf::from("workspace/src/lib.rs");
    let third = PathBuf::from("workspace/src/final.rs");
    let mut in_flight = HashSet::new();
    let mut queued = HashMap::new();

    assert_eq!(
        reserve_save_request(7, &first, &mut in_flight, &mut queued),
        SaveRequest::Spawn
    );
    assert_eq!(
        reserve_save_request(7, &second, &mut in_flight, &mut queued),
        SaveRequest::Queued
    );
    assert_eq!(
        reserve_save_request(7, &third, &mut in_flight, &mut queued),
        SaveRequest::Queued
    );

    assert_eq!(
        finish_save_request(7, &mut in_flight, &mut queued),
        Some(third)
    );
}

#[test]
fn active_save_work_includes_in_flight_queued_and_pending_format() {
    let in_flight = HashSet::from([1]);
    let queued = HashMap::from([(2, PathBuf::from("workspace/src/queued.rs"))]);
    let pending_format = HashMap::from([(3, ())]);

    assert!(has_active_save_work(
        1,
        &in_flight,
        &queued,
        &pending_format
    ));
    assert!(has_active_save_work(
        2,
        &in_flight,
        &queued,
        &pending_format
    ));
    assert!(has_active_save_work(
        3,
        &in_flight,
        &queued,
        &pending_format
    ));
    assert!(!has_active_save_work(
        4,
        &in_flight,
        &queued,
        &pending_format
    ));
}

#[test]
fn stale_save_completion_does_not_drain_queued_save_path() {
    let queued_path = PathBuf::from("workspace/src/queued.rs");
    let mut in_flight = HashSet::new();
    let mut queued = HashMap::from([(7, queued_path.clone())]);

    assert_eq!(
        finish_current_save_request(7, &mut in_flight, &mut queued),
        FinishedSaveRequest::Stale
    );
    assert_eq!(queued.get(&7), Some(&queued_path));
}

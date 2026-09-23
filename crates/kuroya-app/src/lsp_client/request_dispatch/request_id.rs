use crate::lsp_client::pending::PendingLspRequests;

const FIRST_DISPATCHED_REQUEST_ID: u64 = 3;

pub(super) fn reserve_request_id(
    next_request_id: &mut u64,
    pending_requests: &PendingLspRequests,
) -> u64 {
    for _ in 0..=pending_requests.len() {
        let request_id = reserve_next_request_id(next_request_id);
        if !pending_requests.contains_key(&request_id) {
            return request_id;
        }
    }

    reserve_next_request_id(next_request_id)
}

fn reserve_next_request_id(next_request_id: &mut u64) -> u64 {
    if *next_request_id < FIRST_DISPATCHED_REQUEST_ID {
        *next_request_id = FIRST_DISPATCHED_REQUEST_ID;
    }

    let request_id = *next_request_id;
    *next_request_id = request_id
        .checked_add(1)
        .unwrap_or(FIRST_DISPATCHED_REQUEST_ID);
    request_id
}

#[cfg(test)]
mod tests {
    use super::{FIRST_DISPATCHED_REQUEST_ID, reserve_request_id};
    use crate::lsp_client::pending::{PendingLspRequest, PendingLspRequests};
    use std::path::PathBuf;

    fn hover(version: u64) -> PendingLspRequest {
        PendingLspRequest::Hover {
            id: 1,
            path: PathBuf::from("src/main.rs"),
            version,
            line: 0,
            character: 0,
        }
    }

    #[test]
    fn request_ids_advance_monotonically_for_normal_dispatch() {
        let mut next_request_id = 7;
        let pending_requests = PendingLspRequests::default();

        assert_eq!(
            reserve_request_id(&mut next_request_id, &pending_requests),
            7
        );
        assert_eq!(next_request_id, 8);
        assert_eq!(
            reserve_request_id(&mut next_request_id, &pending_requests),
            8
        );
        assert_eq!(next_request_id, 9);
    }

    #[test]
    fn request_id_rollover_skips_reserved_startup_ids() {
        let mut next_request_id = u64::MAX;
        let pending_requests = PendingLspRequests::default();

        assert_eq!(
            reserve_request_id(&mut next_request_id, &pending_requests),
            u64::MAX
        );
        assert_eq!(next_request_id, FIRST_DISPATCHED_REQUEST_ID);
        assert_eq!(
            reserve_request_id(&mut next_request_id, &pending_requests),
            FIRST_DISPATCHED_REQUEST_ID
        );
        assert_eq!(next_request_id, FIRST_DISPATCHED_REQUEST_ID + 1);
    }

    #[test]
    fn reserved_startup_request_id_state_recovers_to_dispatched_range() {
        let mut next_request_id = 0;
        let pending_requests = PendingLspRequests::default();

        assert_eq!(
            reserve_request_id(&mut next_request_id, &pending_requests),
            FIRST_DISPATCHED_REQUEST_ID
        );
        assert_eq!(next_request_id, FIRST_DISPATCHED_REQUEST_ID + 1);

        for reserved_id in 1..FIRST_DISPATCHED_REQUEST_ID {
            let mut next_request_id = reserved_id;

            assert_eq!(
                reserve_request_id(&mut next_request_id, &pending_requests),
                FIRST_DISPATCHED_REQUEST_ID
            );
            assert_eq!(next_request_id, FIRST_DISPATCHED_REQUEST_ID + 1);
        }
    }

    #[test]
    fn request_id_reservation_skips_active_pending_ids() {
        let mut next_request_id = 7;
        let pending_requests = PendingLspRequests::from([(7, hover(1)), (8, hover(2))]);

        assert_eq!(
            reserve_request_id(&mut next_request_id, &pending_requests),
            9
        );
        assert_eq!(next_request_id, 10);
    }

    #[test]
    fn request_id_rollover_skips_active_pending_startup_range_ids() {
        let mut next_request_id = u64::MAX;
        let pending_requests = PendingLspRequests::from([
            (u64::MAX, hover(1)),
            (FIRST_DISPATCHED_REQUEST_ID, hover(2)),
        ]);

        assert_eq!(
            reserve_request_id(&mut next_request_id, &pending_requests),
            FIRST_DISPATCHED_REQUEST_ID + 1
        );
        assert_eq!(next_request_id, FIRST_DISPATCHED_REQUEST_ID + 2);
    }
}

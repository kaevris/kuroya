use super::*;

#[test]
fn terminal_search_plain_text_strips_ansi_sequences() {
    let text = terminal_plain_text(b"\x1b[31merror\x1b[0m\r\n\x1b]0;ignored title\x07done\x07");

    assert_eq!(text, "error\ndone");
}

#[test]
fn terminal_search_plain_text_strips_ansi_control_strings() {
    let text = terminal_plain_text(
        b"start\x1bP1$r q\x1b\\dcs\x1b_Xterm APC payload\x1b\\apc\x1b^pm payload\x1b\\done",
    );

    assert_eq!(text, "startdcsapcdone");
}

#[test]
fn terminal_search_plain_text_applies_backspace_on_current_line() {
    let text = terminal_plain_text("ab\u{8}cd\n\u{8}next\né\u{8}e".as_bytes());

    assert_eq!(text, "acd\nnext\ne");
}

#[test]
fn terminal_search_plain_text_applies_carriage_return_overwrites() {
    let text = terminal_plain_text(b"build 10%\rbuild 20%\nnext\rOK\n");

    assert_eq!(text, "build 20%\nOK\n");
}

#[test]
fn terminal_search_matches_are_case_insensitive_and_track_lines() {
    let matches = terminal_search_matches(7, "Alpha\nbeta alpha\nALPHA", "alpha");

    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0].session_id, 7);
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[0].end, 5);
    assert_eq!(matches[1].line, 1);
    assert_eq!(matches[1].start, 5);
    assert_eq!(matches[2].line, 2);
}

#[test]
fn terminal_search_matches_multiple_hits_on_same_line() {
    let matches = terminal_search_matches(7, "Alpha alpha ALPHA", "alpha");

    let ranges = matches
        .iter()
        .map(|matched| matched.start..matched.end)
        .collect::<Vec<_>>();
    assert_eq!(ranges, vec![0..5, 6..11, 12..17]);
}

#[test]
fn terminal_search_matches_are_bounded_for_dense_output() {
    let limit = terminal_search_match_limit_for_test();
    let text = "a".repeat(limit + 128);
    let matches = terminal_search_matches(7, &text, "a");

    assert_eq!(matches.len(), limit);
    assert_eq!(matches.last().map(|matched| matched.start), Some(limit - 1));
}

#[test]
fn terminal_visible_search_spans_track_screen_cells() {
    let size = test_terminal_size();
    let session = session_with_output(7, size, b"Alpha\r\nbeta alpha\r\n");

    let spans = terminal_visible_search_spans(session.parser.screen(), "alpha");

    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].row, 0);
    assert_eq!(spans[0].start_col, 0);
    assert_eq!(spans[0].end_col, 5);
    assert!(spans[0].contains_cell(0, 4));
    assert!(!spans[0].contains_cell(0, 5));
    assert_eq!(spans[1].row, 1);
    assert_eq!(spans[1].start_col, 5);
    assert_eq!(spans[1].end_col, 10);
}

#[test]
fn terminal_visible_search_spans_keep_combining_marks_on_cell_boundaries() {
    let size = PtySize {
        rows: 2,
        cols: 4,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session = session_with_output(7, size, "e\u{0301}\r\n".as_bytes());

    let spans = terminal_visible_search_spans(session.parser.screen(), "\u{0301}");

    assert_eq!(
        spans,
        vec![TerminalVisibleSearchSpan {
            row: 0,
            start_col: 0,
            end_col: 1,
            active: false
        }]
    );
}

#[test]
fn terminal_visible_search_spans_map_repeated_combining_mark_matches() {
    let size = PtySize {
        rows: 2,
        cols: 6,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session = session_with_output(7, size, "e\u{0301} e\u{0301}\r\n".as_bytes());

    let spans = terminal_visible_search_spans(session.parser.screen(), "\u{0301}");

    assert_eq!(
        spans,
        vec![
            TerminalVisibleSearchSpan {
                row: 0,
                start_col: 0,
                end_col: 1,
                active: false
            },
            TerminalVisibleSearchSpan {
                row: 0,
                start_col: 2,
                end_col: 3,
                active: false
            }
        ]
    );
}

#[test]
fn terminal_visible_search_spans_cover_wide_cells_at_right_edge() {
    let size = PtySize {
        rows: 2,
        cols: 2,
        pixel_width: 0,
        pixel_height: 0,
    };
    let session = session_with_output(7, size, "\u{8868}\r\n".as_bytes());

    let spans = terminal_visible_search_spans(session.parser.screen(), "\u{8868}");

    assert_eq!(
        spans,
        vec![TerminalVisibleSearchSpan {
            row: 0,
            start_col: 0,
            end_col: 2,
            active: false
        }]
    );
}

#[test]
fn terminal_search_preserves_ascii_only_case_folding() {
    let matches = terminal_search_matches(7, "über Über", "über");

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[0].end, "über".len());
}

#[test]
fn terminal_search_query_normalization_preserves_ordinary_spaces() {
    assert_eq!(
        normalized_terminal_search_query_for_test("  alpha  beta  "),
        Some("alpha  beta".to_owned())
    );
}

#[test]
fn terminal_search_query_normalization_strips_controls_and_bidi_marks() {
    let max = terminal_search_query_max_chars_for_test();
    let query = format!(
        "  alpha\t\n\u{202e} beta\r\n{}",
        "z".repeat(max.saturating_sub("alpha beta ".len()))
    );
    let normalized = normalized_terminal_search_query_for_test(&query).unwrap();

    assert!(normalized.starts_with("alpha beta "));
    assert_eq!(normalized.chars().count(), max);
    assert!(!normalized.contains('\t'));
    assert!(!normalized.contains('\n'));
    assert!(!normalized.contains('\u{202e}'));
    assert!(!normalized.contains('\u{200f}'));
}

#[test]
fn terminal_search_matches_use_exactly_bounded_normalized_query() {
    let max = terminal_search_query_max_chars_for_test();
    let capped = "a".repeat(max);
    let text = format!("prefix {capped} suffix");

    let matches = terminal_search_matches(7, &text, &capped);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, "prefix ".len());
    assert_eq!(matches[0].end, "prefix ".len() + capped.len());
}

#[test]
fn terminal_search_rejects_overlong_normalized_query_instead_of_prefix_matching() {
    let max = terminal_search_query_max_chars_for_test();
    let capped = "a".repeat(max);
    let query = format!("{capped}b");
    let text = format!("prefix {capped} suffix");

    assert_eq!(normalized_terminal_search_query_for_test(&query), None);
    assert!(terminal_search_matches(7, &text, &query).is_empty());
}

#[test]
fn terminal_search_visible_spans_use_normalized_query() {
    let size = test_terminal_size();
    let session = session_with_output(7, size, b"alpha beta\r\n");

    let spans = terminal_visible_search_spans(session.parser.screen(), "alpha\t\u{202e} beta");

    assert_eq!(
        spans,
        vec![TerminalVisibleSearchSpan {
            row: 0,
            start_col: 0,
            end_col: 10,
            active: false
        }]
    );
}

#[test]
fn terminal_search_navigation_wraps_matches() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(
        session_with_output(1, size, b"alpha\r\nbeta alpha\r\nalpha"),
        size,
    );
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 3);
    assert_eq!(pane.active_terminal_search_result_label(3), "1/3 results");

    pane.advance_terminal_search(1);
    assert_eq!(pane.search_match, 1);
    pane.advance_terminal_search(2);
    assert_eq!(pane.search_match, 0);
    pane.advance_terminal_search(-1);
    assert_eq!(pane.search_match, 2);
}

#[test]
fn terminal_search_result_commands_open_search_before_navigation() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(
        session_with_output(1, size, b"alpha\r\nbeta alpha\r\nalpha"),
        size,
    );
    pane.search_query = "alpha".to_owned();

    pane.next_terminal_search_result();

    assert!(pane.search_open);
    assert_eq!(pane.search_match, 0);

    pane.next_terminal_search_result();
    assert_eq!(pane.search_match, 1);
    pane.previous_terminal_search_result();
    assert_eq!(pane.search_match, 0);
    pane.previous_terminal_search_result();
    assert_eq!(pane.search_match, 2);
}

#[test]
fn terminal_search_navigation_command_only_handles_open_search() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(
        session_with_output(1, size, b"alpha\r\nbeta alpha\r\nalpha"),
        size,
    );
    pane.search_query = "alpha".to_owned();

    assert!(!pane.advance_terminal_search_result_if_open(1));
    assert!(!pane.search_open);
    assert_eq!(pane.search_match, 0);

    pane.open_terminal_search();

    assert!(pane.advance_terminal_search_result_if_open(1));
    assert_eq!(pane.search_match, 1);
}

#[test]
fn terminal_search_navigation_command_ignores_hidden_search() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(
        session_with_output(1, size, b"alpha\r\nbeta alpha\r\nalpha"),
        size,
    );
    pane.search_query = "alpha".to_owned();
    pane.open_terminal_search();
    pane.set_visible(false);

    assert!(pane.search_open);
    assert!(!pane.advance_terminal_search_result_if_open(1));
    assert_eq!(pane.search_match, 0);
}

#[test]
fn terminal_search_navigation_reveals_selected_scrollback_match() {
    let size = PtySize {
        rows: 3,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let output = (0..8)
        .map(|line| format!("line{line}\r\n"))
        .collect::<String>();
    let mut pane = pane_with_session(session_with_output(1, size, output.as_bytes()), size);

    pane.search_query = "line0".to_owned();
    pane.reset_terminal_search_cursor();

    assert!(pane.sessions[0].scrollback() > 0);

    pane.search_query = "line7".to_owned();
    pane.reset_terminal_search_cursor();

    assert_eq!(pane.sessions[0].scrollback(), 0);
}

#[test]
fn terminal_split_search_matches_all_visible_sessions_in_pane_order() {
    let size = test_terminal_size();
    let first = session_with_output(1, size, b"alpha left\r\nleft only\r\n");
    let second = session_with_output(2, size, b"right alpha\r\nalpha again\r\n");
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_view = true;
    pane.search_query = "alpha".to_owned();

    let session_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();
    let match_count = session_ids.len();

    assert_eq!(session_ids, vec![1, 2, 2]);
    assert_eq!(
        pane.active_terminal_search_result_label(match_count),
        "1/3 results"
    );
    assert_eq!(
        pane.search_cache.scope,
        TerminalSearchCacheScope::Split {
            sessions: vec![
                (1, pane.sessions[0].search_generation),
                (2, pane.sessions[1].search_generation)
            ]
        }
    );
}

#[test]
fn terminal_search_matches_only_active_session_when_not_split() {
    let size = test_terminal_size();
    let first = session_with_output(1, size, b"alpha left\r\n");
    let second = session_with_output(2, size, b"alpha right\r\n");
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.search_query = "alpha".to_owned();

    let first_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();
    pane.active_session = 1;
    let second_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();

    assert_eq!(first_ids, vec![1]);
    assert_eq!(second_ids, vec![2]);
}

#[test]
fn terminal_split_search_navigation_activates_and_reveals_matched_session() {
    let size = PtySize {
        rows: 3,
        cols: 20,
        pixel_width: 0,
        pixel_height: 0,
    };
    let first = session_with_output(1, size, b"alpha left\r\nleft one\r\nleft two\r\n");
    let second = session_with_output(
        2,
        size,
        b"alpha right\r\nright one\r\nright two\r\nright three\r\n",
    );
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_view = true;
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    assert_eq!(pane.active_session, 0);

    pane.advance_terminal_search(1);

    assert_eq!(pane.search_match, 1);
    assert_eq!(pane.active_session, 1);
    assert!(pane.sessions[1].scrollback() > 0);
    assert!(!pane.focus_input_on_show);
}

#[test]
fn terminal_split_search_reset_prefers_active_session_match() {
    let size = test_terminal_size();
    let first = session_with_output(1, size, b"alpha left\r\n");
    let second = session_with_output(2, size, b"alpha right\r\n");
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_view = true;
    pane.active_session = 1;
    pane.search_query = "alpha".to_owned();

    pane.reset_terminal_search_cursor();

    assert_eq!(pane.search_match, 1);
    assert_eq!(pane.active_session, 1);
    assert_eq!(pane.active_terminal_search_result_label(2), "2/2 results");
}

#[test]
fn terminal_split_search_cache_invalidates_when_inactive_session_changes() {
    let size = test_terminal_size();
    let first = session_with_output(1, size, b"alpha left\r\n");
    let second = session_with_output(2, size, b"alpha right\r\n");
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_view = true;
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    let old_second_generation = pane.sessions[1].search_generation;

    pane.sessions[1].replace_search_buffer("bravo right".to_owned());

    let session_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();

    assert_ne!(pane.sessions[1].search_generation, old_second_generation);
    assert_eq!(session_ids, vec![1]);
    assert_eq!(
        pane.search_cache.scope,
        TerminalSearchCacheScope::Split {
            sessions: vec![
                (1, pane.sessions[0].search_generation),
                (2, pane.sessions[1].search_generation)
            ]
        }
    );
}

#[test]
fn terminal_search_scrollback_centers_matches_when_possible() {
    assert_eq!(terminal_search_scrollback_for_line_for_test(8, 3, 0), 5);
    assert_eq!(terminal_search_scrollback_for_line_for_test(8, 3, 3), 3);
    assert_eq!(terminal_search_scrollback_for_line_for_test(8, 3, 7), 0);
    assert_eq!(terminal_search_scrollback_for_line_for_test(2, 5, 0), 0);
}

#[test]
fn terminal_search_cache_tracks_session_generation_and_query() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"alpha"), size);
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 1);
    assert_eq!(
        pane.search_cache.scope,
        TerminalSearchCacheScope::Single {
            session_id: 1,
            generation: pane.sessions[0].search_generation
        }
    );
    assert_eq!(pane.search_cache.query, "alpha");

    pane.sessions[0].replace_search_buffer("bravo".to_owned());

    assert!(pane.active_terminal_search_matches().is_empty());
    assert_eq!(
        pane.search_cache.scope,
        TerminalSearchCacheScope::Single {
            session_id: 1,
            generation: pane.sessions[0].search_generation
        }
    );

    pane.search_query = "bravo".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 1);
    assert_eq!(pane.search_cache.query, "bravo");
}

#[test]
fn terminal_search_cache_reuses_match_allocation_for_full_single_refresh() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(
        session_with_output(1, size, b"alpha one\nbeta only\nalpha two\n"),
        size,
    );
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    pane.search_cache.matches.reserve_exact(64);
    let retained_capacity = pane.search_cache.matches.capacity();
    pane.search_query = "beta".to_owned();

    let lines = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.line)
        .collect::<Vec<_>>();

    assert_eq!(lines, vec![1]);
    assert_eq!(pane.search_cache.matches.capacity(), retained_capacity);
    assert_eq!(pane.search_cache.query, "beta");
    assert!(matches!(
        pane.search_cache.progress,
        TerminalSearchCacheProgress::Single(_)
    ));
}

#[test]
fn terminal_search_cache_reuses_stable_prefix_after_append() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"alpha one\npartial al"), size);
    pane.search_query = "alpha".to_owned();

    let matches = pane.active_terminal_search_matches();
    assert_eq!(matches.len(), 1);
    let retained_preview = matches[0].preview.clone();

    pane.sessions[0].append_search_output(b"pha\nalpha three\n");

    let generation = pane.sessions[0].search_generation;
    let matches = pane.active_terminal_search_matches();
    let lines = matches
        .iter()
        .map(|matched| matched.line)
        .collect::<Vec<_>>();

    assert_eq!(lines, vec![0, 1, 2]);
    assert_eq!(matches[1].start, "partial ".len());
    assert!(std::sync::Arc::ptr_eq(
        &retained_preview,
        &matches[0].preview
    ));
    assert_eq!(
        pane.search_cache.scope,
        TerminalSearchCacheScope::Single {
            session_id: 1,
            generation
        }
    );
}

#[test]
fn terminal_search_append_cache_drops_matches_from_other_sessions() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"alpha one\npartial al"), size);
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 1);
    pane.search_cache.matches.push(TerminalSearchMatch {
        session_id: 99,
        line: 0,
        start: 0,
        end: 5,
        preview: std::sync::Arc::new("stale".to_owned()),
    });

    pane.sessions[0].append_search_output(b"pha\n");

    let session_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();

    assert_eq!(session_ids, vec![1, 1]);
    assert!(
        pane.search_cache
            .matches
            .iter()
            .all(|matched| matched.session_id == 1)
    );
}

#[test]
fn terminal_search_cache_reuses_equivalent_normalized_query() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(
        session_with_output(1, size, b"alpha beta\nalpha beta\n"),
        size,
    );
    pane.search_query = "alpha beta".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    let retained_preview = pane.search_cache.matches[0].preview.clone();
    let retained_scope = pane.search_cache.scope.clone();
    let retained_progress = pane.search_cache.progress;
    assert!(matches!(
        retained_progress,
        TerminalSearchCacheProgress::Single(_)
    ));

    pane.search_match = 99;
    pane.search_query = " \talpha\u{202e} beta\n".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    assert_eq!(pane.search_match, 1);
    assert_eq!(pane.search_cache.query, "alpha beta");
    assert_eq!(pane.search_cache.scope, retained_scope);
    assert_eq!(pane.search_cache.progress, retained_progress);
    assert!(std::sync::Arc::ptr_eq(
        &retained_preview,
        &pane.search_cache.matches[0].preview
    ));
}

#[test]
fn terminal_search_cache_rejects_stale_match_lines_with_current_scope() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"alpha\n"), size);
    pane.search_query = "alpha".to_owned();
    pane.search_cache = TerminalSearchCache {
        scope: TerminalSearchCacheScope::Single {
            session_id: 1,
            generation: pane.sessions[0].search_generation,
        },
        query: "alpha".to_owned(),
        matches: vec![TerminalSearchMatch {
            session_id: 1,
            line: 99,
            start: 0,
            end: 5,
            preview: std::sync::Arc::new("stale".to_owned()),
        }],
        progress: Default::default(),
    };
    pane.search_match = 99;

    let matches = pane.active_terminal_search_matches();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[0].preview.as_ref().as_str(), "alpha");
    assert_eq!(pane.search_match, 0);
}

#[test]
fn terminal_search_cache_rejects_stale_match_ranges_with_current_scope() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"bravo alpha\n"), size);
    pane.search_query = "alpha".to_owned();
    pane.search_cache = TerminalSearchCache {
        scope: TerminalSearchCacheScope::Single {
            session_id: 1,
            generation: pane.sessions[0].search_generation,
        },
        query: "alpha".to_owned(),
        matches: vec![TerminalSearchMatch {
            session_id: 1,
            line: 0,
            start: 0,
            end: 5,
            preview: std::sync::Arc::new("stale".to_owned()),
        }],
        progress: Default::default(),
    };
    pane.search_match = 99;

    let matches = pane.active_terminal_search_matches();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[0].start, "bravo ".len());
    assert_eq!(matches[0].end, "bravo alpha".len());
    assert_eq!(matches[0].preview.as_ref().as_str(), "bravo alpha");
    assert_eq!(pane.search_match, 0);
}

#[test]
fn terminal_search_cache_rejects_out_of_order_matches_with_current_scope() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(
        session_with_output(1, size, b"alpha one\r\nalpha two\r\n"),
        size,
    );
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    pane.search_cache.matches.reverse();

    let lines = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.line)
        .collect::<Vec<_>>();

    assert_eq!(lines, vec![0, 1]);
}

#[test]
fn terminal_search_cache_rejects_overlapping_matches_with_current_scope() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"ababa\n"), size);
    pane.search_query = "aba".to_owned();
    pane.search_cache = TerminalSearchCache {
        scope: TerminalSearchCacheScope::Single {
            session_id: 1,
            generation: pane.sessions[0].search_generation,
        },
        query: "aba".to_owned(),
        matches: vec![
            TerminalSearchMatch {
                session_id: 1,
                line: 0,
                start: 0,
                end: 3,
                preview: std::sync::Arc::new("stale first".to_owned()),
            },
            TerminalSearchMatch {
                session_id: 1,
                line: 0,
                start: 2,
                end: 5,
                preview: std::sync::Arc::new("stale overlap".to_owned()),
            },
        ],
        progress: Default::default(),
    };

    let ranges = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.start..matched.end)
        .collect::<Vec<_>>();

    assert_eq!(ranges, vec![0..3]);
    assert_eq!(
        pane.search_cache.matches[0].preview.as_ref().as_str(),
        "ababa"
    );
}

#[test]
fn terminal_search_cache_rejects_missing_earlier_same_line_match() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"alpha alpha\n"), size);
    pane.search_query = "alpha".to_owned();
    pane.search_cache = TerminalSearchCache {
        scope: TerminalSearchCacheScope::Single {
            session_id: 1,
            generation: pane.sessions[0].search_generation,
        },
        query: "alpha".to_owned(),
        matches: vec![TerminalSearchMatch {
            session_id: 1,
            line: 0,
            start: "alpha ".len(),
            end: "alpha alpha".len(),
            preview: std::sync::Arc::new("alpha alpha".to_owned()),
        }],
        progress: Default::default(),
    };

    let ranges = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.start..matched.end)
        .collect::<Vec<_>>();

    assert_eq!(ranges, vec![0..5, 6..11]);
}

#[test]
fn terminal_search_cache_rejects_missing_trailing_same_line_match() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"alpha alpha\n"), size);
    pane.search_query = "alpha".to_owned();
    pane.search_cache = TerminalSearchCache {
        scope: TerminalSearchCacheScope::Single {
            session_id: 1,
            generation: pane.sessions[0].search_generation,
        },
        query: "alpha".to_owned(),
        matches: vec![TerminalSearchMatch {
            session_id: 1,
            line: 0,
            start: 0,
            end: "alpha".len(),
            preview: std::sync::Arc::new("alpha alpha".to_owned()),
        }],
        progress: Default::default(),
    };

    let ranges = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.start..matched.end)
        .collect::<Vec<_>>();

    assert_eq!(ranges, vec![0..5, 6..11]);
}

#[test]
fn terminal_split_search_cache_rejects_matches_outside_cached_scope() {
    let size = test_terminal_size();
    let first = session_with_output(1, size, b"alpha left\n");
    let second = session_with_output(2, size, b"alpha right\n");
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_view = true;
    pane.search_query = "alpha".to_owned();
    pane.search_cache = TerminalSearchCache {
        scope: TerminalSearchCacheScope::Split {
            sessions: vec![
                (1, pane.sessions[0].search_generation),
                (2, pane.sessions[1].search_generation),
            ],
        },
        query: "alpha".to_owned(),
        matches: vec![TerminalSearchMatch {
            session_id: 99,
            line: 0,
            start: 0,
            end: 5,
            preview: std::sync::Arc::new("stale".to_owned()),
        }],
        progress: Default::default(),
    };

    let session_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();

    assert_eq!(session_ids, vec![1, 2]);
}

#[test]
fn terminal_split_search_cache_rejects_out_of_order_session_matches() {
    let size = test_terminal_size();
    let first = session_with_output(1, size, b"alpha left\n");
    let second = session_with_output(2, size, b"alpha right\n");
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_view = true;
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    pane.search_cache.matches.reverse();

    let session_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();

    assert_eq!(session_ids, vec![1, 2]);
}

#[test]
fn terminal_split_search_cache_reuses_match_allocation_for_full_refresh() {
    let size = test_terminal_size();
    let first = session_with_output(1, size, b"alpha left\nleft only\n");
    let second = session_with_output(2, size, b"alpha right\nright only\n");
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.split_view = true;
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    pane.search_cache.matches.reserve_exact(64);
    let retained_capacity = pane.search_cache.matches.capacity();
    pane.search_query = "right".to_owned();

    let session_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();

    assert_eq!(session_ids, vec![2, 2]);
    assert_eq!(pane.search_cache.matches.capacity(), retained_capacity);
    assert_eq!(
        pane.search_cache.scope,
        TerminalSearchCacheScope::Split {
            sessions: vec![
                (1, pane.sessions[0].search_generation),
                (2, pane.sessions[1].search_generation)
            ]
        }
    );
    assert_eq!(pane.search_cache.query, "right");
}

#[test]
fn terminal_search_resume_point_from_line_count_tracks_append_start() {
    for (buffer, line_count, expected) in [
        ("", 0, (0, 0)),
        ("alpha", 1, (0, 0)),
        ("alpha\n", 1, (6, 1)),
        ("alpha\nbeta", 2, (6, 1)),
        ("alpha\nbeta\n", 2, (11, 2)),
        ("alpha\n\nbeta", 3, (7, 2)),
    ] {
        assert_eq!(
            terminal_search_resume_point_from_line_count_for_test(buffer, line_count),
            expected,
            "buffer={buffer:?}"
        );
    }
}

#[test]
fn terminal_search_cursor_clamps_when_cached_matches_shrink() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(
        session_with_output(1, size, b"alpha one\r\nalpha two\r\n"),
        size,
    );
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    pane.search_match = 1;

    pane.sessions[0].replace_search_buffer("alpha only\n".to_owned());

    assert_eq!(pane.active_terminal_search_matches().len(), 1);
    assert_eq!(pane.search_match, 0);

    pane.search_match = 1;
    pane.sessions[0].replace_search_buffer("bravo only\n".to_owned());

    assert!(pane.active_terminal_search_matches().is_empty());
    assert_eq!(pane.search_match, 0);
}

#[test]
fn terminal_search_cursor_clamps_when_active_session_scope_changes() {
    let size = test_terminal_size();
    let first = session_with_output(1, size, b"alpha one\r\nalpha two\r\n");
    let second = session_with_output(2, size, b"alpha right\r\n");
    let mut pane = pane_with_sessions(vec![first, second], size);
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 2);
    pane.search_match = 1;
    pane.active_session = 1;

    let session_ids = pane
        .active_terminal_search_matches()
        .iter()
        .map(|matched| matched.session_id)
        .collect::<Vec<_>>();

    assert_eq!(session_ids, vec![2]);
    assert_eq!(pane.search_match, 0);
}

#[test]
fn terminal_search_cache_clears_for_empty_queries() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"alpha"), size);
    pane.search_query = "alpha".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 1);
    pane.search_match = 1;
    pane.search_query = "  ".to_owned();

    assert!(pane.active_terminal_search_matches().is_empty());
    assert_eq!(pane.search_match, 0);
    assert_eq!(pane.search_cache.scope, TerminalSearchCacheScope::Empty);
    assert!(pane.search_cache.query.is_empty());
}

#[test]
fn terminal_search_cache_uses_normalized_query_and_clears_empty_normalized_queries() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_with_output(1, size, b"alpha beta"), size);
    pane.search_query = " alpha\t\u{202e} beta ".to_owned();

    assert_eq!(pane.active_terminal_search_matches().len(), 1);
    assert_eq!(pane.search_cache.query, "alpha beta");

    pane.search_match = 1;
    pane.search_query = "\u{202e}\u{2066}\n\t ".to_owned();

    assert!(pane.active_terminal_search_matches().is_empty());
    assert_eq!(pane.search_match, 0);
    assert_eq!(pane.search_cache.scope, TerminalSearchCacheScope::Empty);
    assert!(pane.search_cache.query.is_empty());
}

#[test]
fn terminal_search_full_scan_uses_recent_bytes_for_huge_scrollback() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    let old_line = "old ".to_owned() + &"x".repeat(terminal_search_full_scan_max_bytes_for_test());
    session.replace_search_buffer(format!("{old_line}\nrecent needle\n"));
    let mut pane = pane_with_session(session, size);
    pane.search_query = "needle".to_owned();

    let matches = pane.active_terminal_search_matches();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].line, 1);
    assert_eq!(matches[0].start, "recent ".len());
}

#[test]
fn terminal_search_full_scan_does_not_scan_single_line_over_byte_cap() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);
    session.replace_search_buffer(format!(
        "{}needle",
        "x".repeat(terminal_search_full_scan_max_bytes_for_test())
    ));
    let mut pane = pane_with_session(session, size);
    pane.search_query = "needle".to_owned();

    assert!(pane.active_terminal_search_matches().is_empty());
}

#[test]
fn terminal_search_labels_empty_results() {
    assert_eq!(terminal_search_result_label_for_test(0, 0), "No results");
    assert_eq!(terminal_search_result_label_for_test(10, 2), "2/2 results");
}

#[test]
fn terminal_drain_output_updates_search_buffer_without_ansi() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_without_command(8, size), size);

    pane.sessions[0]
        .tx_output
        .send(TerminalEvent::Output(
            b"\x1b[31mBuild Failed\x1b[0m\r\n".to_vec(),
        ))
        .unwrap();
    pane.drain_output();

    assert!(pane.sessions[0].search_buffer.contains("Build Failed"));
    assert!(!pane.sessions[0].search_buffer.contains('\u{1b}'));
}

#[test]
fn terminal_search_buffer_applies_carriage_return_across_chunks() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);

    session.append_search_output(b"build 10%\r");
    session.append_search_output(b"build 20%\r\nnext");

    assert_eq!(session.search_buffer, "build 20%\nnext");
}

#[test]
fn terminal_search_buffer_strips_split_ansi_sequences_across_chunks() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);

    session.append_search_output(b"\x1b[31");
    session.append_search_output(b"mred\x1b]0;ignored");
    session.append_search_output(b" title\x1b\\ text");

    assert_eq!(session.search_buffer, "red text");
}

#[test]
fn terminal_search_buffer_strips_split_ansi_string_terminators_across_chunks() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);

    session.append_search_output(b"start");
    session.append_search_output(b"\x1b]0;ignored osc");
    session.append_search_output(b"\x1b");
    session.append_search_output(b"\\ osc ");
    session.append_search_output(b"\x1bPignored dcs");
    session.append_search_output(b"\x1b");
    session.append_search_output(b"\\done");

    assert_eq!(session.search_buffer, "start osc done");
}

#[test]
fn terminal_search_buffer_recovers_from_oversized_unterminated_control_sequence() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);
    let oversized_payload = vec![b'x'; terminal_search_control_sequence_max_chars_for_test() + 8];

    session.append_search_output(b"ready\x1b]0;");
    session.append_search_output(&oversized_payload);
    session.append_search_output(b" prompt");

    assert!(session.search_buffer.starts_with("ready"));
    assert!(session.search_buffer.ends_with(" prompt"));
}

#[test]
fn terminal_search_buffer_drops_oversized_control_payload_tail_before_recovery() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);
    let oversized_payload = vec![b'x'; terminal_search_control_sequence_max_chars_for_test() + 8];

    session.append_search_output(b"ready\x1b]0;");
    session.append_search_output(&oversized_payload);
    session.append_search_output(b" prompt");

    assert_eq!(session.search_buffer, "ready prompt");
    assert!(!session.search_buffer.contains('x'));
}

#[test]
fn terminal_search_decoder_resets_after_process_stop() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);

    session.append_search_output(b"\x1b]0;unterminated title");
    session.mark_stopped();
    session.append_search_output(b"prompt ready");

    assert_eq!(session.search_buffer, "prompt ready");
}

#[test]
fn terminal_search_buffer_strips_split_c1_ansi_sequences_across_chunks() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);

    session.append_search_output(&[0xe2, 0x82]);
    session.append_search_output(&[0xac, b' ', 0x9b, b'3', b'1']);
    session.append_search_output(b"mred ");
    session.append_search_output(&[0x9d]);
    session.append_search_output(b"0;ignored title");
    session.append_search_output(&[0x9c, b'o', b's', b'c', b' ']);
    session.append_search_output(&[0x90]);
    session.append_search_output(b"ignored dcs");
    session.append_search_output(&[0x9c, b'd', b'c', b's']);
    session.append_search_output(&[b' ', 0x98]);
    session.append_search_output(b"ignored sos");
    session.append_search_output(&[0x9c, b's', b'o', b's']);
    session.append_search_output(&[b' ', 0x9e]);
    session.append_search_output(b"ignored pm");
    session.append_search_output(&[0x9c, b'p', b'm']);
    session.append_search_output(&[b' ', 0x9f]);
    session.append_search_output(b"ignored apc");
    session.append_search_output(&[0x9c, b'a', b'p', b'c']);

    assert_eq!(session.search_buffer, "\u{20ac} red osc dcs sos pm apc");
}

#[test]
fn terminal_search_buffer_preserves_utf8_split_across_chunks() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);

    session.append_search_output(&[0xe2, 0x82]);

    assert!(session.search_buffer.is_empty());

    session.append_search_output(&[0xac, b' ', b'd', b'o', b'n', b'e']);

    assert_eq!(session.search_buffer, "\u{20ac} done");
}

#[test]
fn terminal_search_buffer_bulk_appends_plain_utf8_output() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);
    let chunk = "alpha\nbeta\tgamma\n\n".repeat(8);

    session.append_search_output(chunk.as_bytes());

    assert_eq!(session.search_buffer, chunk);
    assert_eq!(
        session.search_line_count,
        session.search_buffer.lines().count()
    );
}

#[test]
fn terminal_search_buffer_fast_path_respects_existing_line_state() {
    let size = test_terminal_size();
    let mut session = session_without_command(8, size);

    session.append_search_output(b"alpha");
    session.append_search_output(b"\nbeta\n");
    session.append_search_output(b"\n");

    assert_eq!(session.search_buffer, "alpha\nbeta\n\n");
    assert_eq!(
        session.search_line_count,
        session.search_buffer.lines().count()
    );
}

#[test]
fn terminal_drain_output_is_bounded_per_frame() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_without_command(8, size), size);

    for _ in 0..TERMINAL_DRAIN_EVENT_BUDGET + 3 {
        pane.sessions[0]
            .tx_output
            .send(TerminalEvent::Output(b"x".to_vec()))
            .unwrap();
    }

    assert_eq!(pane.drain_output(), TERMINAL_DRAIN_EVENT_BUDGET);
    assert!(pane.has_pending_output());
    assert_eq!(pane.sessions[0].rx_output.len(), 3);

    assert_eq!(pane.drain_output(), 3);
    assert!(!pane.has_pending_output());
}

#[test]
fn terminal_drain_output_is_bounded_by_bytes_per_frame() {
    let size = test_terminal_size();
    let mut pane = pane_with_session(session_without_command(8, size), size);
    let chunk = vec![b'x'; 4096];
    let expected_events = TERMINAL_DRAIN_BYTE_BUDGET / chunk.len();

    for _ in 0..(expected_events + 3) {
        pane.sessions[0]
            .tx_output
            .send(TerminalEvent::Output(chunk.clone()))
            .unwrap();
    }

    assert_eq!(pane.drain_output(), expected_events);
    assert!(pane.has_pending_output());
    assert_eq!(pane.sessions[0].rx_output.len(), 3);
    assert_eq!(
        pane.sessions[0].search_buffer.len(),
        TERMINAL_DRAIN_BYTE_BUDGET
    );

    assert_eq!(pane.drain_output(), 3);
    assert!(!pane.has_pending_output());
}

#[test]
fn terminal_finished_event_survives_after_byte_budgeted_output() {
    let size = test_terminal_size();
    let (mut pane, _rx_command) = pane_with_command_session(size);

    pane.sessions[0]
        .tx_output
        .send(TerminalEvent::Output(vec![
            b'x';
            TERMINAL_DRAIN_BYTE_BUDGET
        ]))
        .unwrap();
    pane.sessions[0]
        .tx_output
        .send(TerminalEvent::Finished {
            message: None,
            process_exit_code: Some(0),
            reason: TerminalFinishReason::ProcessExit,
        })
        .unwrap();

    assert_eq!(pane.drain_output(), 1);
    assert!(pane.sessions[0].started);
    assert!(pane.has_pending_output());

    assert_eq!(pane.drain_output(), 1);
    assert!(!pane.sessions[0].started);
    assert_eq!(pane.sessions[0].last_process_exit_code, Some(0));
}

#[test]
fn terminal_session_output_channel_is_bounded() {
    let size = test_terminal_size();
    let session = TerminalSession::new(8, size, TERMINAL_SCROLLBACK_ROWS);

    assert_eq!(
        session.rx_output.capacity(),
        Some(TERMINAL_OUTPUT_CHANNEL_BOUND)
    );
}

#[test]
fn terminal_session_command_channel_is_bounded() {
    let (_tx_command, rx_command) = terminal_command_channel();

    assert_eq!(rx_command.capacity(), Some(TERMINAL_COMMAND_CHANNEL_BOUND));
}

#[test]
fn terminal_session_close_uses_out_of_band_signal_when_command_queue_is_full() {
    let size = test_terminal_size();
    let (tx_command, rx_command) = bounded(1);
    tx_command
        .send(TerminalCommand::Input("queued".to_owned()))
        .unwrap();
    let (tx_close, rx_close) = terminal_close_channel();
    let mut session = TerminalSession {
        tx_command: Some(tx_command),
        tx_close: Some(tx_close),
        ..session_without_command(1, size)
    };

    session.close();

    assert!(!session.started);
    assert!(session.tx_command.is_none());
    assert!(session.tx_close.is_none());
    assert!(
        session
            .close_requested
            .load(std::sync::atomic::Ordering::SeqCst)
    );
    assert!(rx_close.try_recv().is_ok());
    match rx_command.try_recv().unwrap() {
        TerminalCommand::Input(input) => assert_eq!(input, "queued"),
        TerminalCommand::Resize(_) | TerminalCommand::Close => panic!("expected queued input"),
    }
    assert!(rx_command.try_recv().is_err());
}

#[test]
fn terminal_search_buffer_is_bounded() {
    let mut buffer = "a".repeat(TERMINAL_SEARCH_BUFFER_MAX_BYTES + 32);
    buffer.push_str("needle");

    trim_terminal_search_buffer_for_test(&mut buffer);

    assert!(buffer.len() <= TERMINAL_SEARCH_BUFFER_TRIM_TARGET_BYTES);
    assert!(buffer.ends_with("needle"));
}

#[test]
fn terminal_search_buffer_trim_keeps_utf8_boundary_and_line_start() {
    let prefix = "α".repeat(TERMINAL_SEARCH_BUFFER_MAX_BYTES / 2 + 8);
    let mut buffer = format!("{prefix}\nneedle\n");

    trim_terminal_search_buffer_for_test(&mut buffer);

    assert!(buffer.len() <= TERMINAL_SEARCH_BUFFER_TRIM_TARGET_BYTES);
    assert_eq!(buffer, "needle\n");
}

#[test]
fn terminal_search_line_count_tracks_decoded_output_changes() {
    let size = test_terminal_size();
    let mut session = session_without_command(1, size);

    session.append_search_output(b"build 10%\r");
    assert_eq!(
        session.search_line_count,
        session.search_buffer.lines().count()
    );

    session.append_search_output(b"build 20%\r\nno");
    assert_eq!(session.search_buffer, "build 20%\nno");
    assert_eq!(
        session.search_line_count,
        session.search_buffer.lines().count()
    );

    session.append_search_output(b"\x08ow\n");
    assert_eq!(session.search_buffer, "build 20%\nnow\n");
    assert_eq!(
        session.search_line_count,
        session.search_buffer.lines().count()
    );

    session.replace_search_buffer("one\ntwo\nthree".to_owned());
    assert_eq!(session.search_line_count, 3);
}

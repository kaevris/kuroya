use super::*;

#[test]
fn source_control_history_filter_matches_commit_metadata_terms() {
    let commits = vec![
        GitCommitSummary {
            oid: "12345678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
            short_oid: "12345678".to_owned(),
            summary: "Add search panel".to_owned(),
            author: "Kuroya Test".to_owned(),
            time_seconds: 10,
        },
        GitCommitSummary {
            oid: "abcdef12aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            short_oid: "abcdef12".to_owned(),
            summary: "Fix terminal scrollback".to_owned(),
            author: "Another Author".to_owned(),
            time_seconds: 2000,
        },
    ];
    let now_seconds = 3700;

    let all = source_control_filtered_history(&commits, "", now_seconds);
    assert_eq!(all.len(), 2);

    let matches = source_control_filtered_history(&commits, "search kuroya", now_seconds);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].short_oid, "12345678");

    let uppercase = source_control_filtered_history(&commits, "SEARCH KUROYA", now_seconds);
    assert_eq!(uppercase.len(), 1);
    assert_eq!(uppercase[0].short_oid, "12345678");

    let oid_match = source_control_filtered_history(&commits, "aaaaaaaa", now_seconds);
    assert_eq!(oid_match.len(), 1);
    assert_eq!(oid_match[0].short_oid, "abcdef12");

    // The second commit is 1700s (28m20s) old: within a 29 minute age range,
    // while the first commit (3690s old) is outside it.
    let age_match = source_control_filtered_history(&commits, "29m", now_seconds);
    assert_eq!(age_match.len(), 1);
    assert_eq!(age_match[0].short_oid, "abcdef12");
}

#[test]
fn source_control_history_with_uncommitted_prepends_worktree_entry_when_enabled_and_dirty() {
    let commit = GitCommitSummary {
        oid: "12345678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        short_oid: "12345678".to_owned(),
        summary: "Add search panel".to_owned(),
        author: "Kuroya Test".to_owned(),
        time_seconds: 10,
    };

    let history =
        source_control_history_with_uncommitted(vec![commit.clone()], true, true, 1_700_000_000);

    assert_eq!(history.len(), 2);
    assert!(source_control_history_commit_is_uncommitted(&history[0]));
    assert_eq!(history[0].oid, GIT_UNCOMMITTED_HISTORY_OID);
    assert_eq!(history[0].short_oid, "uncommitted");
    assert_eq!(history[0].summary, "Uncommitted Changes");
    assert_eq!(history[0].author, "Working Tree");
    assert_eq!(history[0].time_seconds, 1_700_000_000);
    assert_eq!(history[1], commit);
}

#[test]
fn source_control_history_with_uncommitted_respects_setting_and_dirty_state() {
    let commit = GitCommitSummary {
        oid: "abcdef12aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        short_oid: "abcdef12".to_owned(),
        summary: "Fix terminal scrollback".to_owned(),
        author: "Another Author".to_owned(),
        time_seconds: 2000,
    };

    assert_eq!(
        source_control_history_with_uncommitted(vec![commit.clone()], false, true, 3700),
        vec![commit.clone()]
    );
    assert_eq!(
        source_control_history_with_uncommitted(vec![commit.clone()], true, false, 3700),
        vec![commit]
    );
}

#[test]
fn source_control_committed_history_len_ignores_uncommitted_entry() {
    let commit = GitCommitSummary {
        oid: "12345678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        short_oid: "12345678".to_owned(),
        summary: "Add search panel".to_owned(),
        author: "Kuroya Test".to_owned(),
        time_seconds: 10,
    };
    let history = source_control_history_with_uncommitted(vec![commit], true, true, 3700);

    assert_eq!(history.len(), 2);
    assert_eq!(source_control_committed_history_len(&history), 1);
    assert_eq!(
        next_git_history_limit(source_control_committed_history_len(&history), 50, 25),
        75
    );
}

#[test]
fn source_control_history_labels_include_commit_age() {
    let commit = GitCommitSummary {
        oid: "12345678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        short_oid: "12345678".to_owned(),
        summary: "Add search panel".to_owned(),
        author: "Kuroya Test".to_owned(),
        time_seconds: 1000,
    };

    assert_eq!(
        source_control_commit_label(&commit, 1000, true),
        "12345678  Add search panel  Kuroya Test  just now"
    );
    assert_eq!(
        source_control_commit_label(&commit, 1000, false),
        "12345678  Add search panel  just now"
    );
    assert_eq!(source_control_commit_age_label_at(&commit, 1065), "1m ago");
    assert_eq!(source_control_commit_age_label_at(&commit, 4600), "1h ago");
    assert_eq!(
        source_control_commit_age_label_at(&commit, 1000 + 3 * 24 * 60 * 60),
        "3d ago"
    );
    assert_eq!(
        source_control_commit_age_label_at(&commit, 1000 + 75 * 24 * 60 * 60),
        "2mo ago"
    );
    assert_eq!(
        source_control_commit_age_label_at(&commit, 1000 + 2 * 365 * 24 * 60 * 60),
        "2y ago"
    );
    assert_eq!(source_control_commit_age_label_at(&commit, 900), "just now");
}

#[test]
fn source_control_history_copy_actions_match_commit_fields() {
    let commit = GitCommitSummary {
        oid: "12345678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
        short_oid: "12345678".to_owned(),
        summary: "Add search panel".to_owned(),
        author: "Kuroya Test".to_owned(),
        time_seconds: 10,
    };

    assert_eq!(
        source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::Oid),
        "12345678bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert_eq!(
        source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::ShortOid),
        "12345678"
    );
    assert_eq!(
        source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::Summary),
        "Add search panel"
    );
    assert_eq!(
        source_control_commit_copy_text(&commit, SourceControlCommitCopyKind::Author),
        "Kuroya Test"
    );
    assert_eq!(
        source_control_commit_copy_text_at(&commit, SourceControlCommitCopyKind::Age, 3700),
        "1h ago"
    );
    assert_eq!(
        source_control_commit_copy_status(&commit, SourceControlCommitCopyKind::Oid),
        "Copied commit ID 12345678"
    );
    assert_eq!(
        source_control_commit_copy_status(&commit, SourceControlCommitCopyKind::ShortOid),
        "Copied short commit ID 12345678"
    );
    assert_eq!(
        source_control_commit_copy_status(&commit, SourceControlCommitCopyKind::Summary),
        "Copied commit message for 12345678"
    );
    assert_eq!(
        source_control_commit_copy_status(&commit, SourceControlCommitCopyKind::Author),
        "Copied commit author for 12345678"
    );
    assert_eq!(
        source_control_commit_copy_status(&commit, SourceControlCommitCopyKind::Age),
        "Copied commit age for 12345678"
    );
}

#[test]
fn source_control_history_keyboard_actions_match_selected_commit_copy_controls() {
    assert_eq!(
        source_control_history_keyboard_action_labels(),
        vec![
            "Alt+P Copy Patch",
            "Alt+I Copy Commit ID",
            "Alt+S Copy Short Commit ID",
            "Alt+M Copy Commit Message",
            "Alt+A Copy Commit Author",
            "Alt+T Copy Commit Age"
        ]
    );
}

#[test]
fn source_control_stash_label_includes_index_oid_and_message() {
    let stash = GitStashEntry {
        index: 2,
        short_oid: "12345678".to_owned(),
        message: "On main: work in progress".to_owned(),
    };

    assert_eq!(
        source_control_stash_label(&stash),
        "stash@{2}  12345678  On main: work in progress"
    );
}

fn assert_stash_display_text_is_safe(value: &str) {
    assert!(
        !value.chars().any(is_unsafe_stash_display_char),
        "display text contains unsafe characters: {value:?}"
    );
    assert!(
        value.chars().count() <= 360,
        "display text should be bounded: {} chars",
        value.chars().count()
    );
}

fn is_unsafe_stash_display_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

#[test]
fn source_control_stash_label_sanitizes_and_bounds_display_fields() {
    let stash = GitStashEntry {
        index: 2,
        short_oid: format!("12\u{202e}\n34{}", "a".repeat(200)),
        message: format!("On main:\u{2066}\nwork{}", "b".repeat(200)),
    };

    let label = source_control_stash_label(&stash);

    assert!(label.starts_with("stash@{2}  12 34"));
    assert!(label.contains("On main: work"));
    assert!(label.contains("..."));
    assert_stash_display_text_is_safe(&label);
}

#[test]
fn source_control_stash_label_uses_fallbacks_for_blank_display_fields() {
    let stash = GitStashEntry {
        index: 2,
        short_oid: "\u{202e}\n".to_owned(),
        message: "\u{2066}\u{2069}".to_owned(),
    };

    assert_eq!(
        source_control_stash_label(&stash),
        "stash@{2}  unknown  No message"
    );
}

#[test]
fn source_control_stash_copy_actions_match_stash_fields() {
    let stash = GitStashEntry {
        index: 2,
        short_oid: "12345678".to_owned(),
        message: "On main: work in progress".to_owned(),
    };

    assert_eq!(source_control_stash_ref(&stash), "stash@{2}");
    assert_eq!(
        source_control_stash_copy_text(&stash, SourceControlStashCopyKind::Ref),
        "stash@{2}"
    );
    assert_eq!(
        source_control_stash_copy_text(&stash, SourceControlStashCopyKind::Message),
        "On main: work in progress"
    );
    assert_eq!(
        source_control_stash_copy_status(&stash, SourceControlStashCopyKind::Ref),
        "Copied stash ref stash@{2}"
    );
    assert_eq!(
        source_control_stash_copy_status(&stash, SourceControlStashCopyKind::Message),
        "Copied stash message for stash@{2}"
    );
}

#[test]
fn source_control_stash_copy_status_is_safe_while_message_copy_text_is_raw() {
    let message = format!("On main:\u{202e}\n{}", "work".repeat(100));
    let stash = GitStashEntry {
        index: 2,
        short_oid: "12345678".to_owned(),
        message: message.clone(),
    };

    assert_eq!(
        source_control_stash_copy_text(&stash, SourceControlStashCopyKind::Message),
        message
    );

    let status = source_control_stash_copy_status(&stash, SourceControlStashCopyKind::Message);
    assert_eq!(status, "Copied stash message for stash@{2}");
    assert_stash_display_text_is_safe(&status);
}

#[test]
fn source_control_stash_footer_actions_match_selected_stash_controls() {
    assert_eq!(
        source_control_stash_footer_action_labels(),
        vec![
            "Open Changes",
            "Copy Patch",
            "Apply",
            "Pop",
            "Drop",
            "Copy Ref",
            "Copy Message"
        ]
    );
}

#[test]
fn source_control_stash_keyboard_actions_match_selected_stash_copy_controls() {
    assert_eq!(
        source_control_stash_keyboard_action_labels(),
        vec![
            "Alt+P Copy Patch",
            "Alt+R Copy Stash Ref",
            "Alt+M Copy Stash Message",
            "Alt+A Apply Stash",
            "Alt+O Pop Stash",
            "Alt+D Drop Stash"
        ]
    );
}

#[test]
fn source_control_history_statuses_report_lifecycle() {
    assert_eq!(git_history_pending_status(), "Loading git history");
    assert_eq!(git_history_success_status(0), "No git history found");
    assert_eq!(git_history_success_status(1), "Loaded 1 commit");
    assert_eq!(git_history_success_status(2), "Loaded 2 commits");
    assert_eq!(
        git_history_failure_status("no HEAD"),
        "Could not load git history: no HEAD"
    );
}

#[test]
fn source_control_history_paging_matches_graph_settings() {
    assert_eq!(next_git_history_limit(50, 50, 50), 100);
    assert_eq!(next_git_history_limit(40, 50, 25), 75);
    assert_eq!(next_git_history_limit(10, 10, 0), 11);

    assert!(source_control_history_has_more(50, 50));
    assert!(!source_control_history_has_more(49, 50));
    assert!(!source_control_history_has_more(0, 0));
    assert!(source_control_history_can_load_more(false, true));
    assert!(!source_control_history_can_load_more(true, true));
    assert!(!source_control_history_can_load_more(false, false));

    assert!(source_control_history_should_page_on_scroll(
        true, false, true, 176.0, 100.0, 300.0
    ));
    assert!(!source_control_history_should_page_on_scroll(
        false, false, true, 176.0, 100.0, 300.0
    ));
    assert!(!source_control_history_should_page_on_scroll(
        true, true, true, 176.0, 100.0, 300.0
    ));
    assert!(!source_control_history_should_page_on_scroll(
        true, false, true, 120.0, 100.0, 300.0
    ));
}

#[test]
fn source_control_graph_divergence_label_follows_visibility_settings() {
    let divergence = Some(GitRemoteDivergence {
        incoming: 2,
        outgoing: 1,
    });

    assert_eq!(
        source_control_graph_divergence_label(divergence, true, true).as_deref(),
        Some("Incoming 2 Outgoing 1")
    );
    assert_eq!(
        source_control_graph_divergence_label(divergence, true, false).as_deref(),
        Some("Incoming 2")
    );
    assert_eq!(
        source_control_graph_divergence_label(divergence, false, true).as_deref(),
        Some("Outgoing 1")
    );
    assert_eq!(
        source_control_graph_divergence_label(divergence, false, false),
        None
    );
    assert_eq!(
        source_control_graph_divergence_label(
            Some(GitRemoteDivergence {
                incoming: 0,
                outgoing: 0
            }),
            true,
            true,
        ),
        None
    );
    assert_eq!(
        source_control_graph_divergence_label(None, true, true),
        None
    );
}

#[test]
fn source_control_stash_statuses_report_lifecycle() {
    assert_eq!(git_stash_list_pending_status(), "Loading git stashes");
    assert_eq!(git_stash_list_success_status(0), "No git stashes found");
    assert_eq!(git_stash_list_success_status(1), "Loaded 1 git stash");
    assert_eq!(git_stash_list_success_status(2), "Loaded 2 git stashes");
    assert_eq!(
        git_stash_list_failure_status("not a repo"),
        "Could not load git stashes: not a repo"
    );
    assert_eq!(git_stash_save_pending_status(), "Saving git stash");
    assert_eq!(
        git_stash_save_success_status("12345678"),
        "Saved git stash (12345678)"
    );
    assert_eq!(
        git_stash_save_failure_status("no changes"),
        "Could not save git stash: no changes"
    );
    assert_eq!(git_stash_apply_pending_status(1), "Applying git stash 1");
    assert_eq!(git_stash_apply_success_status(1), "Applied git stash 1");
    assert_eq!(
        git_stash_apply_failure_status(1, "conflict"),
        "Could not apply git stash 1: conflict"
    );
    assert_eq!(git_stash_pop_pending_status(1), "Popping git stash 1");
    assert_eq!(git_stash_pop_success_status(1), "Popped git stash 1");
    assert_eq!(
        git_stash_pop_failure_status(1, "conflict"),
        "Could not pop git stash 1: conflict"
    );
    assert_eq!(git_stash_drop_pending_status(1), "Dropping git stash 1");
    assert_eq!(git_stash_drop_success_status(1), "Dropped git stash 1");
    assert_eq!(
        git_stash_drop_failure_status(1, "missing"),
        "Could not drop git stash 1: missing"
    );
}

#[test]
fn source_control_stash_statuses_sanitize_failure_details_and_hashes() {
    let error = format!("first line\u{202e}\nsecond line{}", "x".repeat(400));

    let failure_statuses = [
        git_stash_list_failure_status(&error),
        git_stash_save_failure_status(&error),
        git_stash_apply_failure_status(1, &error),
        git_stash_pop_failure_status(1, &error),
        git_stash_drop_failure_status(1, &error),
    ];

    for status in failure_statuses {
        assert!(status.contains("first line second line"));
        assert!(status.contains("..."));
        assert_stash_display_text_is_safe(&status);
    }

    let save_status = git_stash_save_success_status(&format!("1234\u{2066}\n{}", "a".repeat(120)));
    assert!(save_status.starts_with("Saved git stash (1234 "));
    assert!(save_status.contains("..."));
    assert_stash_display_text_is_safe(&save_status);
}

#[test]
fn source_control_stash_message_can_fall_back_to_commit_input() {
    assert_eq!(
        source_control_stash_message_from_inputs(" stash this ", "commit input", true),
        "stash this"
    );
    assert_eq!(
        source_control_stash_message_from_inputs("", " commit input ", true),
        "commit input"
    );
    assert_eq!(
        source_control_stash_message_from_inputs("", " commit input ", false),
        ""
    );
    let message = source_control_stash_message_from_inputs(
        "",
        &format!(" commit\u{2066}\ninput{} ", "x".repeat(220)),
        true,
    );

    assert!(message.starts_with("commit\u{2066}\ninput"));
    assert!(message.ends_with(&"x".repeat(220)));
}

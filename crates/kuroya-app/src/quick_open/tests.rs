use crate::history::NavigationLocation;

#[cfg(windows)]
use super::QUICK_OPEN_OPEN_FILE_BONUS;
use super::{
    QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT, QUICK_OPEN_OPEN_FILE_SCAN_LIMIT,
    QuickOpenCandidateRankData, QuickOpenLowercaseMatch, QuickOpenMatchQuery, QuickOpenQuery,
    QuickOpenQueryMemoryEntry, QuickOpenRankingBonusContext, QuickOpenResult,
    QuickOpenResultsCache, normalize_quick_open_query_memory, normalize_quick_open_recent_files,
    normalize_quick_open_workspace_path, quick_open_attach_navigation_line_columns,
    quick_open_candidate_beats_result, quick_open_empty_query_index_scan_limit,
    quick_open_empty_query_ranked_results, quick_open_for_each_candidate_path,
    quick_open_lowercase_match_kind, quick_open_lowercase_word_start_match,
    quick_open_open_file_candidates, quick_open_paths_match, quick_open_relative_label,
    quick_open_result_label_with_navigation_line_column,
    quick_open_target_with_navigation_line_column, quick_open_unboosted_empty_query_results,
    record_quick_open_navigation, record_quick_open_query_memory,
    sanitized_quick_open_result_label, sanitized_quick_open_result_label_text,
};
use std::{
    borrow::Cow,
    collections::VecDeque,
    path::{Path, PathBuf},
};

#[test]
fn quick_open_match_query_keeps_clean_single_token_untokenized() {
    let query = QuickOpenMatchQuery::from_sanitized_query("Open".to_owned());

    assert_eq!(query.raw, "Open");
    assert_eq!(query.lowercase, "open");
    assert!(query.tokens.is_empty());
    assert!(query.token_lowercases.is_empty());
    assert_eq!(query.normalized_memory_query.as_deref(), Some("open"));
}

#[test]
fn quick_open_match_query_tokenizes_whitespace_query() {
    let query = QuickOpenMatchQuery::from_sanitized_query("Open File".to_owned());

    assert_eq!(query.raw, "Open File");
    assert_eq!(query.lowercase, "");
    assert_eq!(query.tokens, vec!["Open".to_owned(), "File".to_owned()]);
    assert_eq!(
        query.token_lowercases,
        vec!["open".to_owned(), "file".to_owned()]
    );
    assert_eq!(query.normalized_memory_query.as_deref(), Some("open file"));
}

#[test]
fn quick_open_match_query_preserves_memory_query_rules() {
    let too_short = QuickOpenMatchQuery::from_sanitized_query("A".to_owned());
    let mixed_case = QuickOpenMatchQuery::from_sanitized_query("\u{00c9}x".to_owned());

    assert_eq!(too_short.normalized_memory_query, None);
    assert_eq!(
        mixed_case.normalized_memory_query.as_deref(),
        Some("\u{00e9}x")
    );
}

#[test]
fn lowercase_match_kind_preserves_unicode_lowercase_semantics() {
    let query = "\u{00c9}clair.rs".to_lowercase();

    assert_eq!(
        quick_open_lowercase_match_kind("\u{00e9}clair.rs", &query),
        QuickOpenLowercaseMatch {
            starts_with_query: true,
            exact: true,
        }
    );
    assert_eq!(
        quick_open_lowercase_match_kind("\u{00e9}clair_test.rs", &query),
        QuickOpenLowercaseMatch {
            starts_with_query: false,
            exact: false,
        }
    );
}

#[test]
fn lowercase_word_start_match_respects_file_name_segments() {
    assert!(quick_open_lowercase_word_start_match(
        "copy-buffer.rs",
        "buffer"
    ));
    assert!(quick_open_lowercase_word_start_match(
        "copyBuffer.rs",
        "buffer"
    ));
    assert!(!quick_open_lowercase_word_start_match(
        "rebuffer.rs",
        "buffer"
    ));
}

#[test]
fn results_cache_matches_current_inputs_without_rebuilt_vectors() {
    let recent_files = VecDeque::from([PathBuf::from("workspace/src/main.rs")]);
    let open_files = vec![PathBuf::from("workspace/src/lib.rs")];
    let query_memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "lib".to_owned(),
        path: PathBuf::from("workspace/src/lib.rs"),
        uses: 2,
    }]);
    let navigation_back = VecDeque::from([NavigationLocation::new(
        PathBuf::from("workspace/src/main.rs"),
        4,
        2,
    )]);
    let navigation_forward = VecDeque::from([NavigationLocation::new(
        PathBuf::from("workspace/src/other.rs"),
        8,
        1,
    )]);
    let current_navigation_location =
        NavigationLocation::new(PathBuf::from("workspace/src/lib.rs"), 12, 3);
    let cache = QuickOpenResultsCache {
        query_input: "lib".to_owned(),
        index_generation: 7,
        recent_files: recent_files.clone(),
        open_files: open_files.clone(),
        query_memory: query_memory.clone(),
        navigation_back: navigation_back.clone(),
        navigation_forward: navigation_forward.clone(),
        current_navigation_location: Some(current_navigation_location.clone()),
        parsed_query: QuickOpenQuery {
            pattern: "lib".to_owned(),
            line: None,
            column: 1,
        },
        result_labels: Vec::new(),
        results: Vec::new(),
        background_rank: None,
        last_query_changed_at: None,
        completed_ranking: None,
    };

    assert!(cache.matches(
        "lib",
        7,
        &recent_files,
        open_files.iter().map(PathBuf::as_path),
        &query_memory,
        &navigation_back,
        &navigation_forward,
        Some(&current_navigation_location),
    ));

    let changed_current_navigation_location =
        NavigationLocation::new(PathBuf::from("workspace/src/lib.rs"), 12, 4);
    assert!(!cache.matches(
        "lib",
        7,
        &recent_files,
        open_files.iter().map(PathBuf::as_path),
        &query_memory,
        &navigation_back,
        &navigation_forward,
        Some(&changed_current_navigation_location),
    ));

    let changed_open_files = [PathBuf::from("workspace/src/main.rs")];
    assert!(!cache.matches(
        "lib",
        7,
        &recent_files,
        changed_open_files.iter().map(PathBuf::as_path),
        &query_memory,
        &navigation_back,
        &navigation_forward,
        Some(&current_navigation_location),
    ));

    assert!(!cache.matches(
        "lib",
        8,
        &recent_files,
        open_files.iter().map(PathBuf::as_path),
        &query_memory,
        &navigation_back,
        &navigation_forward,
        Some(&current_navigation_location),
    ));
}

#[test]
fn results_cache_ranking_inputs_match_equivalent_navigation_paths_without_line_columns() {
    let recent_files = VecDeque::new();
    let open_files: Vec<PathBuf> = Vec::new();
    let query_memory = VecDeque::new();
    let main = PathBuf::from("workspace/src/main.rs");
    let cache = QuickOpenResultsCache {
        query_input: "main".to_owned(),
        index_generation: 7,
        recent_files: recent_files.clone(),
        open_files: open_files.clone(),
        query_memory: query_memory.clone(),
        navigation_back: VecDeque::new(),
        navigation_forward: VecDeque::new(),
        current_navigation_location: Some(NavigationLocation::new(main.clone(), 3, 2)),
        parsed_query: QuickOpenQuery {
            pattern: "main".to_owned(),
            line: None,
            column: 1,
        },
        result_labels: Vec::new(),
        results: Vec::new(),
        background_rank: None,
        last_query_changed_at: None,
        completed_ranking: None,
    };

    let same_rank_inputs = vec![NavigationLocation::new(
        PathBuf::from("workspace/src/../src/main.rs"),
        48,
        9,
    )];
    assert!(cache.ranking_inputs_match(
        "main",
        7,
        &recent_files,
        open_files.iter().map(PathBuf::as_path),
        &query_memory,
        &same_rank_inputs,
    ));

    let changed_rank_inputs = vec![NavigationLocation::new(
        PathBuf::from("workspace/src/lib.rs"),
        48,
        9,
    )];
    assert!(!cache.ranking_inputs_match(
        "main",
        7,
        &recent_files,
        open_files.iter().map(PathBuf::as_path),
        &query_memory,
        &changed_rank_inputs,
    ));

    assert!(!cache.ranking_inputs_match(
        "main",
        8,
        &recent_files,
        open_files.iter().map(PathBuf::as_path),
        &query_memory,
        &same_rank_inputs,
    ));
}

#[test]
fn results_cache_refreshes_navigation_metadata_without_rebuilding_results() {
    let path = PathBuf::from("workspace/src/main.rs");
    let mut cache = QuickOpenResultsCache {
        query_input: "main".to_owned(),
        index_generation: 7,
        recent_files: VecDeque::new(),
        open_files: Vec::new(),
        query_memory: VecDeque::new(),
        navigation_back: VecDeque::new(),
        navigation_forward: VecDeque::new(),
        current_navigation_location: Some(NavigationLocation::new(path.clone(), 3, 2)),
        parsed_query: QuickOpenQuery {
            pattern: "main".to_owned(),
            line: None,
            column: 1,
        },
        result_labels: vec!["src/main.rs:3:2".to_owned()],
        results: vec![QuickOpenResult {
            rank_score: 120,
            fuzzy_score: 40,
            path: path.clone(),
            rel: "src/main.rs".to_owned(),
            navigation_line_column: Some((3, 2)),
        }],
        background_rank: None,
        last_query_changed_at: None,
        completed_ranking: None,
    };
    let navigation_back = VecDeque::new();
    let navigation_forward = VecDeque::new();
    let current = NavigationLocation::new(path.clone(), 48, 9);
    let navigation_locations = vec![current.clone()];

    cache.refresh_navigation_metadata(
        &navigation_back,
        &navigation_forward,
        Some(current.clone()),
        &navigation_locations,
    );

    assert_eq!(cache.current_navigation_location, Some(current));
    assert_eq!(cache.results.len(), 1);
    assert_eq!(cache.results[0].path, path);
    assert_eq!(cache.results[0].rank_score, 120);
    assert_eq!(cache.results[0].fuzzy_score, 40);
    assert_eq!(cache.results[0].navigation_line_column, Some((48, 9)));
    assert_eq!(cache.result_labels, vec!["src/main.rs:48:9"]);
}

#[test]
fn quick_open_results_cache_latest_navigation_line_columns() {
    let path = PathBuf::from("workspace/src/main.rs");
    let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
    let navigation = vec![
        NavigationLocation::new(path.clone(), 4, 2),
        NavigationLocation::new(path, 9, 7),
    ];
    let mut results = vec![QuickOpenResult {
        rank_score: 0,
        fuzzy_score: 0,
        path: equivalent_path,
        rel: "src/main.rs".to_owned(),
        navigation_line_column: None,
    }];

    quick_open_attach_navigation_line_columns(&mut results, &navigation);

    assert_eq!(results[0].navigation_line_column, Some((9, 7)));
}

#[test]
fn quick_open_results_cache_clamps_stale_navigation_line_columns() {
    let path = PathBuf::from("workspace/src/main.rs");
    let navigation = vec![NavigationLocation::new(path.clone(), 0, 0)];
    let query = QuickOpenQuery {
        pattern: "main".to_owned(),
        line: None,
        column: 1,
    };
    let mut results = vec![QuickOpenResult {
        rank_score: 0,
        fuzzy_score: 0,
        path: path.clone(),
        rel: "src/main.rs".to_owned(),
        navigation_line_column: None,
    }];

    quick_open_attach_navigation_line_columns(&mut results, &navigation);

    assert_eq!(results[0].navigation_line_column, Some((1, 1)));
    assert_eq!(
        quick_open_result_label_with_navigation_line_column(
            "src/main.rs",
            &query,
            results[0].navigation_line_column,
        ),
        "src/main.rs:1:1"
    );
    assert_eq!(
        quick_open_target_with_navigation_line_column(path.clone(), &query, Some((0, 0))),
        (path, Some((1, 1)))
    );
}

#[test]
fn quick_open_cached_navigation_targets_label_and_open_target() {
    let path = PathBuf::from("workspace/src/main.rs");
    let query = QuickOpenQuery {
        pattern: "main".to_owned(),
        line: None,
        column: 1,
    };

    assert_eq!(
        quick_open_result_label_with_navigation_line_column("src/main.rs", &query, Some((9, 7))),
        "src/main.rs:9:7"
    );
    assert_eq!(
        quick_open_result_label_with_navigation_line_column(
            "src/\u{00e9}clair.rs",
            &query,
            Some((9, 7))
        ),
        "src/\u{00e9}clair.rs:9:7"
    );
    assert_eq!(
        quick_open_target_with_navigation_line_column(path.clone(), &query, Some((9, 7))),
        (path.clone(), Some((9, 7)))
    );

    let explicit_query = QuickOpenQuery {
        pattern: "main".to_owned(),
        line: Some(3),
        column: 5,
    };
    assert_eq!(
        quick_open_target_with_navigation_line_column(path.clone(), &explicit_query, Some((9, 7))),
        (path, Some((3, 5)))
    );
}

#[test]
fn quick_open_result_label_keeps_clean_ascii_fast_path_behavior() {
    assert_eq!(
        sanitized_quick_open_result_label("src/main.rs", 80, "."),
        "src/main.rs"
    );
    assert_eq!(
        sanitized_quick_open_result_label(" src\nmain.rs ", 80, "."),
        "src main.rs"
    );
    assert_eq!(sanitized_quick_open_result_label("", 80, "."), ".");
}

#[test]
fn quick_open_result_label_text_borrows_clean_ascii_and_unicode() {
    for input in ["src/main.rs", "src/\u{00e9}clair.rs"] {
        let label = sanitized_quick_open_result_label_text(input, 80, ".");

        assert_eq!(label.as_ref(), input);
        assert_eq!(sanitized_quick_open_result_label(input, 80, "."), input);
        assert!(matches!(label, Cow::Borrowed(_)));
    }
}

#[test]
fn quick_open_result_label_text_owns_normalized_outputs() {
    for (input, max_chars, fallback, expected) in [
        ("src\nmain.rs", 80, ".", "src main.rs"),
        (" src/main.rs ", 80, ".", "src/main.rs"),
        ("abcdefghijkl", 8, ".", "abcde..."),
        ("", 80, ".", "."),
        ("\u{200b}", 80, ".", "."),
    ] {
        let label = sanitized_quick_open_result_label_text(input, max_chars, fallback);

        assert_eq!(label.as_ref(), expected);
        assert_eq!(
            sanitized_quick_open_result_label(input, max_chars, fallback),
            expected
        );
        assert!(matches!(label, Cow::Owned(_)));
    }

    assert_eq!(
        sanitized_quick_open_result_label_text("src/main.rs", 0, ".").as_ref(),
        ""
    );
    assert_eq!(sanitized_quick_open_result_label("src/main.rs", 0, "."), "");
}

#[test]
fn quick_open_result_label_appends_navigation_suffix_to_clean_rel_label() {
    let query = QuickOpenQuery {
        pattern: "main".to_owned(),
        line: None,
        column: 1,
    };

    assert_eq!(
        quick_open_result_label_with_navigation_line_column("src/main.rs", &query, Some((42, 3))),
        "src/main.rs:42:3"
    );
}

#[test]
fn recent_files_normalize_lexical_workspace_paths() {
    let root = PathBuf::from("workspace");

    let recent = normalize_quick_open_recent_files(
        [
            root.join("src/../src/main.rs"),
            root.join("src/main.rs"),
            root.join("../outside.rs"),
            root.join(".kuroya/../src/lib.rs"),
        ],
        &root,
        10,
    );

    assert_eq!(
        recent.into_iter().collect::<Vec<_>>(),
        vec![root.join("src/main.rs"), root.join("src/lib.rs")]
    );
}

#[test]
fn recent_files_current_dir_root_rejects_parent_escape() {
    let root = PathBuf::from(".");

    let recent = normalize_quick_open_recent_files(
        [
            PathBuf::from("src/main.rs"),
            PathBuf::from("../outside.rs"),
            PathBuf::from("src")
                .join("..")
                .join("..")
                .join("outside.rs"),
            PathBuf::from("../../workspace/src/main.rs"),
        ],
        &root,
        10,
    );

    assert_eq!(
        recent.into_iter().collect::<Vec<_>>(),
        vec![PathBuf::from("src/main.rs")]
    );
    assert_eq!(
        normalize_quick_open_workspace_path(&root, Path::new("../outside.rs")),
        None
    );
}

#[test]
fn relative_label_does_not_strip_lexical_escape() {
    assert_eq!(
        quick_open_relative_label(Path::new("workspace"), Path::new("workspace/../outside.rs")),
        "workspace/../outside.rs"
    );
    assert_eq!(
        quick_open_relative_label(
            Path::new("workspace"),
            Path::new("workspace/../workspace/src/main.rs")
        ),
        "src/main.rs"
    );
    assert_eq!(
        quick_open_relative_label(Path::new("workspace"), Path::new("workspace/src/main.rs")),
        "src/main.rs"
    );
}

#[test]
fn quick_open_path_keys_match_lexically_equivalent_paths() {
    assert!(quick_open_paths_match(
        Path::new("workspace/src/main.rs"),
        Path::new("workspace/../workspace/src/main.rs")
    ));
}

#[test]
fn open_file_candidates_mark_indexed_paths_by_key_without_duplicate_visits() {
    let root = PathBuf::from("workspace");
    let indexed_main = root.join("src/main.rs");
    let indexed_test = root.join("tests/main.rs");
    let equivalent_open = root.join("src").join("..").join("src").join("main.rs");
    let missing_open = root.join("scratch/main.rs");
    let equivalent_test_open = root.join("tests").join(".").join("main.rs");
    let files = [indexed_main.clone(), indexed_test.clone()];
    let open_files = [
        equivalent_open.as_path(),
        missing_open.as_path(),
        equivalent_test_open.as_path(),
    ];
    let mut visited = Vec::new();

    quick_open_for_each_candidate_path(
        &root,
        files.iter().map(PathBuf::as_path),
        open_files.iter().copied(),
        |path| visited.push(path.to_path_buf()),
    );

    assert_eq!(visited, vec![indexed_main, indexed_test, missing_open]);
}

#[test]
fn open_file_candidates_normalize_unindexed_targets_and_reject_escapes() {
    let root = PathBuf::from("workspace");
    let files: [PathBuf; 0] = [];
    let open_files = [
        root.join("src")
            .join("..")
            .join("generated")
            .join("main.rs"),
        root.join("generated").join(".").join("main.rs"),
        root.join("..").join("outside.rs"),
    ];
    let mut visited = Vec::new();

    quick_open_for_each_candidate_path(
        &root,
        files.iter().map(PathBuf::as_path),
        open_files.iter().map(PathBuf::as_path),
        |path| visited.push(path.to_path_buf()),
    );

    assert_eq!(visited, vec![root.join("generated").join("main.rs")]);
}

#[test]
fn open_file_candidate_and_bonus_work_stays_bounded() {
    let root = PathBuf::from("workspace");
    let open_files = (0..QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT + 8)
        .map(|index| root.join(format!("src/open_{index:04}.rs")))
        .collect::<Vec<_>>();

    let candidates =
        quick_open_open_file_candidates(&root, open_files.iter().map(PathBuf::as_path));

    assert_eq!(candidates.len(), QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT);
    assert_eq!(candidates[0].path, open_files[0]);
    assert_eq!(
        candidates.last().map(|candidate| candidate.path.as_path()),
        Some(open_files[QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT - 1].as_path())
    );

    let recent = VecDeque::new();
    let query_memory = VecDeque::new();
    let navigation = [];
    let query = QuickOpenMatchQuery::new("open");
    let context = QuickOpenRankingBonusContext::new(
        &recent,
        open_files.iter().map(PathBuf::as_path),
        &query_memory,
        &navigation,
        &query,
    );

    assert!(context.rank_score(0, &open_files[QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT - 1]) > 0);
    assert_eq!(
        context.rank_score(0, &open_files[QUICK_OPEN_OPEN_FILE_CANDIDATE_LIMIT]),
        0
    );
}

#[test]
fn open_file_candidates_stop_after_stale_scan_guard() {
    let root = PathBuf::from("workspace");
    let open_files = (0..QUICK_OPEN_OPEN_FILE_SCAN_LIMIT + 8)
        .map(|index| PathBuf::from("..").join(format!("outside_{index:04}.rs")))
        .chain(std::iter::once(root.join("src/late.rs")))
        .collect::<Vec<_>>();

    let candidates =
        quick_open_open_file_candidates(&root, open_files.iter().map(PathBuf::as_path));

    assert!(candidates.is_empty());
}

#[test]
fn candidate_result_precheck_matches_quick_open_result_ordering() {
    let worst = QuickOpenResult {
        rank_score: 100,
        fuzzy_score: 10,
        path: PathBuf::from("workspace/src/z.rs"),
        rel: "src/z.rs".to_owned(),
        navigation_line_column: None,
    };

    assert!(quick_open_candidate_beats_result(
        100,
        10,
        "src/a.rs",
        Path::new("workspace/src/a.rs"),
        &worst,
    ));
    assert!(!quick_open_candidate_beats_result(
        100,
        10,
        "src/zz.rs",
        Path::new("workspace/src/zz.rs"),
        &worst,
    ));
    assert!(!quick_open_candidate_beats_result(
        100,
        9,
        "src/a.rs",
        Path::new("workspace/src/a.rs"),
        &worst,
    ));
}

#[test]
fn candidate_rank_data_reuses_key_for_boosted_empty_query_paths() {
    let root = PathBuf::from("workspace");
    let recent = VecDeque::from([root.join("src/../src/main.rs")]);
    let open_files: [&Path; 0] = [];
    let query_memory = VecDeque::new();
    let navigation = Vec::new();
    let query = QuickOpenMatchQuery::new("");
    let context = QuickOpenRankingBonusContext::new(
        &recent,
        open_files.iter().copied(),
        &query_memory,
        &navigation,
        &query,
    );
    let path = root.join("src/main.rs");
    let mut rank_data = QuickOpenCandidateRankData::new(&path);

    let rank_score = context.rank_score_for_candidate(0, &mut rank_data);
    assert_eq!(rank_score, context.rank_score(0, &path));
    assert!(rank_score > 0);

    let mut remaining_bonus_keys = context.bonus_path_keys();
    assert!(remaining_bonus_keys.remove(rank_data.path_key()));
    assert!(remaining_bonus_keys.is_empty());
}

#[test]
fn empty_query_results_stop_scanning_after_boosted_paths_and_fallback_are_ready() {
    let root = PathBuf::from("workspace");
    let files = [
        root.join("src/main.rs"),
        root.join("src/lib.rs"),
        root.join("src/slow_tail.rs"),
    ];
    let recent = VecDeque::from([root.join("src/main.rs")]);
    let open_files: [&Path; 0] = [];
    let query_memory = VecDeque::new();
    let navigation = [];
    let query = QuickOpenMatchQuery::new("");
    let mut visited = 0usize;

    let results = quick_open_empty_query_ranked_results(
        &root,
        files.iter().map(|path| {
            visited += 1;
            path.as_path()
        }),
        &recent,
        open_files,
        &query_memory,
        &navigation,
        &query,
        1,
    );

    assert_eq!(visited, 2);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, root.join("src/main.rs"));
    assert!(results[0].rank_score > 0);
}

#[test]
fn empty_query_results_cap_stale_bonus_scan_after_fallback_is_ready() {
    let root = PathBuf::from("workspace");
    let scan_limit = quick_open_empty_query_index_scan_limit(1);
    let files = (0..scan_limit + 20)
        .map(|index| root.join(format!("src/file_{index:04}.rs")))
        .collect::<Vec<_>>();
    let recent = VecDeque::from([root.join("src/missing.rs")]);
    let open_files: [&Path; 0] = [];
    let query_memory = VecDeque::new();
    let navigation = [];
    let query = QuickOpenMatchQuery::new("");
    let mut visited = 0usize;

    let results = quick_open_empty_query_ranked_results(
        &root,
        files.iter().map(|path| {
            visited += 1;
            path.as_path()
        }),
        &recent,
        open_files,
        &query_memory,
        &navigation,
        &query,
        1,
    );

    assert_eq!(visited, scan_limit);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, root.join("src/file_0000.rs"));
    assert_eq!(results[0].rank_score, 0);
}

#[test]
fn empty_query_results_keep_unindexed_open_file_under_stale_bonus_scan_cap() {
    let root = PathBuf::from("workspace");
    let scan_limit = quick_open_empty_query_index_scan_limit(1);
    let files = (0..scan_limit + 20)
        .map(|index| root.join(format!("src/file_{index:04}.rs")))
        .collect::<Vec<_>>();
    let recent = VecDeque::from([root.join("src/missing.rs")]);
    let open_path = root.join("scratch/open.rs");
    let open_files = [open_path.as_path()];
    let query_memory = VecDeque::new();
    let navigation = [];
    let query = QuickOpenMatchQuery::new("");
    let mut visited = 0usize;

    let results = quick_open_empty_query_ranked_results(
        &root,
        files.iter().map(|path| {
            visited += 1;
            path.as_path()
        }),
        &recent,
        open_files,
        &query_memory,
        &navigation,
        &query,
        1,
    );

    assert_eq!(visited, scan_limit);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, open_path);
    assert!(results[0].rank_score > 0);
}

#[test]
fn unboosted_empty_query_results_deduplicate_stale_index_paths() {
    let root = PathBuf::from("workspace");
    let main = root.join("src/main.rs");
    let lib = root.join("src/lib.rs");
    let files = [
        root.join("src").join("..").join("src").join("main.rs"),
        main.clone(),
        lib.clone(),
    ];

    let results =
        quick_open_unboosted_empty_query_results(&root, files.iter().map(PathBuf::as_path), 2);

    assert_eq!(results.len(), 2);
    assert_eq!(
        results
            .iter()
            .filter(|result| quick_open_paths_match(&result.path, &main))
            .count(),
        1
    );
    assert!(
        results
            .iter()
            .any(|result| quick_open_paths_match(&result.path, &lib))
    );
}

#[test]
fn unboosted_empty_query_results_cap_duplicate_stale_scan() {
    let root = PathBuf::from("workspace");
    let scan_limit = quick_open_empty_query_index_scan_limit(2);
    let duplicate = root.join("src/main.rs");
    let files = (0..scan_limit + 20)
        .map(|_| duplicate.clone())
        .collect::<Vec<_>>();
    let mut visited = 0usize;

    let results = quick_open_unboosted_empty_query_results(
        &root,
        files.iter().map(|path| {
            visited += 1;
            path.as_path()
        }),
        2,
    );

    assert_eq!(visited, scan_limit);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, duplicate);
}

#[cfg(windows)]
#[test]
fn ranking_bonus_context_applies_windows_folded_path_bonus_by_file_name_key() {
    let recent = VecDeque::new();
    let open = [Path::new("workspace/src/Main.rs")];
    let memory = VecDeque::new();
    let navigation = [];
    let query = QuickOpenMatchQuery::new("main");
    let context = QuickOpenRankingBonusContext::new(
        &recent,
        open.iter().copied(),
        &memory,
        &navigation,
        &query,
    );

    assert_eq!(
        context.rank_score(10, Path::new("workspace/src/MAIN.rs")),
        10 + QUICK_OPEN_OPEN_FILE_BONUS
    );
    assert_eq!(
        context.rank_score(10, Path::new("workspace/src/lib.rs")),
        10
    );
}

#[test]
fn record_navigation_cleans_equivalent_stale_recent_entries() {
    let root = PathBuf::from("workspace");
    let mut recent = VecDeque::from([
        root.join("src/../src/main.rs"),
        root.join("src/main.rs"),
        root.join("../outside.rs"),
    ]);

    record_quick_open_navigation(&mut recent, &root, &root.join("src/./main.rs"), 10);

    assert_eq!(
        recent.into_iter().collect::<Vec<_>>(),
        vec![root.join("src/main.rs")]
    );
}

#[test]
fn query_memory_normalizes_lexical_paths_and_merges_uses() {
    let root = PathBuf::from("workspace");
    let mut memory = VecDeque::from([
        QuickOpenQueryMemoryEntry {
            query: " Main ".to_owned(),
            path: root.join("src/../src/main.rs"),
            uses: 2,
        },
        QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: root.join("src/main.rs"),
            uses: 5,
        },
        QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: root.join("../outside.rs"),
            uses: 9,
        },
    ]);

    memory = normalize_quick_open_query_memory(memory, &root, 10);
    record_quick_open_query_memory(&mut memory, &root, "MAIN", &root.join("src/main.rs"), 10);

    assert_eq!(memory.len(), 1);
    assert_eq!(memory[0].query, "main");
    assert_eq!(memory[0].path, root.join("src/main.rs"));
    assert_eq!(memory[0].uses, 6);
}

#[test]
fn query_memory_current_dir_root_rejects_parent_escape() {
    let root = PathBuf::from(".");
    let mut memory = VecDeque::from([
        QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: PathBuf::from("src/main.rs"),
            uses: 2,
        },
        QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: PathBuf::from("../outside.rs"),
            uses: 9,
        },
        QuickOpenQueryMemoryEntry {
            query: "lib".to_owned(),
            path: PathBuf::from("src")
                .join("..")
                .join("..")
                .join("outside.rs"),
            uses: 4,
        },
    ]);

    memory = normalize_quick_open_query_memory(memory, &root, 10);
    record_quick_open_query_memory(&mut memory, &root, "main", Path::new("../outside.rs"), 10);

    assert_eq!(memory.len(), 1);
    assert_eq!(memory[0].query, "main");
    assert_eq!(memory[0].path, PathBuf::from("src/main.rs"));
    assert_eq!(memory[0].uses, 2);
}

use crate::history::NavigationLocation;
#[cfg(windows)]
use crate::quick_open::normalize_quick_open_workspace_path;
use crate::quick_open::{
    MAX_QUICK_OPEN_QUERY_MEMORY_CHARS, MAX_QUICK_OPEN_QUERY_PATTERN_CHARS,
    QUICK_OPEN_RESULT_LABEL_MAX_CHARS, QuickOpenMatchQuery, QuickOpenQuery,
    QuickOpenQueryMemoryEntry, normalize_quick_open_query_memory,
    normalize_quick_open_recent_files, parse_line_column, parse_quick_open_query,
    quick_open_latest_navigation_locations_by_path, quick_open_match_score,
    quick_open_navigation_target, quick_open_rank_score, quick_open_rank_score_with_navigation,
    quick_open_rank_score_with_open_files, quick_open_ranked_results, quick_open_relative_label,
    quick_open_result_label, quick_open_result_label_with_navigation, quick_open_target,
    quick_open_target_with_navigation, record_quick_open_navigation,
    record_quick_open_query_memory, record_quick_open_recent_file, sanitize_quick_open_query_input,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use std::{collections::VecDeque, path::PathBuf};

#[test]
fn parses_line_column_targets() {
    assert_eq!(parse_line_column("42"), Some((42, 1)));
    assert_eq!(parse_line_column("42:7"), Some((42, 7)));
    assert_eq!(parse_line_column("42, 7"), Some((42, 7)));
    assert_eq!(parse_line_column("42 7"), Some((42, 7)));
    assert_eq!(parse_line_column("0"), None);
    assert_eq!(parse_line_column("12:0"), None);
    assert_eq!(parse_line_column("12:3:4"), None);
    assert_eq!(parse_line_column(""), None);
}

#[test]
fn parses_quick_open_line_column_suffixes() {
    assert_eq!(
        parse_quick_open_query("src/main.rs"),
        QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: None,
            column: 1,
        }
    );
    assert_eq!(
        parse_quick_open_query("main.rs:42"),
        QuickOpenQuery {
            pattern: "main.rs".to_owned(),
            line: Some(42),
            column: 1,
        }
    );
    assert_eq!(
        parse_quick_open_query("main.rs:42:7"),
        QuickOpenQuery {
            pattern: "main.rs".to_owned(),
            line: Some(42),
            column: 7,
        }
    );
    assert_eq!(
        parse_quick_open_query("src/main.rs, 42"),
        QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: Some(42),
            column: 1,
        }
    );
}

#[test]
fn quick_open_query_sanitizing_preserves_ordinary_queries() {
    assert_eq!(
        sanitize_quick_open_query_input("src/main.rs"),
        "src/main.rs"
    );
    assert_eq!(
        sanitize_quick_open_query_input("foo.rs:12:3"),
        "foo.rs:12:3"
    );
    assert_eq!(
        parse_quick_open_query("foo.rs:12:3"),
        QuickOpenQuery {
            pattern: "foo.rs".to_owned(),
            line: Some(12),
            column: 3,
        }
    );
}

#[test]
fn quick_open_query_sanitizing_strips_control_and_bidi_controls() {
    assert_eq!(
        sanitize_quick_open_query_input("\u{202e}src/\u{200f}ma\u{0000}in.rs"),
        "src/main.rs"
    );
    assert_eq!(
        parse_quick_open_query("src/\u{202e}main.rs:12:\u{202d}3"),
        QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: Some(12),
            column: 3,
        }
    );

    let matcher = SkimMatcherV2::default();
    let query = QuickOpenMatchQuery::new("ma\u{202e}in");
    assert!(quick_open_match_score(&matcher, "src/main.rs", &query).is_some());
}

#[test]
fn quick_open_query_sanitizing_collapses_unsafe_whitespace() {
    assert_eq!(
        sanitize_quick_open_query_input(" \tsrc\r\n \u{00a0}main.rs  "),
        "src main.rs"
    );

    let matcher = SkimMatcherV2::default();
    let query = QuickOpenMatchQuery::new("src\t\r\nmain");
    assert!(quick_open_match_score(&matcher, "src/main.rs", &query).is_some());
}

#[test]
fn quick_open_query_sanitizing_caps_matched_patterns_and_memory_keys() {
    let long_query = "a".repeat(MAX_QUICK_OPEN_QUERY_PATTERN_CHARS + 64);
    let parsed = parse_quick_open_query(&long_query);
    assert_eq!(
        parsed.pattern.chars().count(),
        MAX_QUICK_OPEN_QUERY_PATTERN_CHARS
    );

    let matcher = SkimMatcherV2::default();
    let query = QuickOpenMatchQuery::new(&long_query);
    let capped_rel = "a".repeat(MAX_QUICK_OPEN_QUERY_PATTERN_CHARS);
    assert!(quick_open_match_score(&matcher, &capped_rel, &query).is_some());

    let mut memory = VecDeque::new();
    let root = PathBuf::from("workspace");
    let inside = root.join("src/main.rs");
    record_quick_open_query_memory(&mut memory, &root, &long_query, &inside, 10);

    assert_eq!(memory.len(), 1);
    assert_eq!(
        memory[0].query.chars().count(),
        MAX_QUICK_OPEN_QUERY_MEMORY_CHARS
    );
    assert_eq!(
        memory[0].query,
        "a".repeat(MAX_QUICK_OPEN_QUERY_MEMORY_CHARS)
    );
}

#[test]
fn quick_open_suffix_parser_ignores_invalid_suffixes() {
    assert_eq!(
        parse_quick_open_query("main.rs:0"),
        QuickOpenQuery {
            pattern: "main.rs:0".to_owned(),
            line: None,
            column: 1,
        }
    );
    assert_eq!(
        parse_quick_open_query("main.rs:42:0"),
        QuickOpenQuery {
            pattern: "main.rs:42:0".to_owned(),
            line: None,
            column: 1,
        }
    );
    assert_eq!(
        parse_quick_open_query("C:\\workspace\\main.rs:9"),
        QuickOpenQuery {
            pattern: "C:\\workspace\\main.rs".to_owned(),
            line: Some(9),
            column: 1,
        }
    );
}

#[test]
fn quick_open_target_and_label_preserve_optional_location() {
    let path = PathBuf::from("workspace/src/main.rs");
    let query = parse_quick_open_query("main:12:3");

    assert_eq!(
        quick_open_target(path.clone(), &query),
        (path, Some((12, 3)))
    );
    assert_eq!(
        quick_open_result_label("src/main.rs", &query),
        "src/main.rs:12:3"
    );

    let query = parse_quick_open_query("main");
    assert_eq!(
        quick_open_result_label("src/main.rs", &query),
        "src/main.rs"
    );
}

#[test]
fn quick_open_targets_recent_navigation_location_without_explicit_line() {
    let path = PathBuf::from("workspace/src/main.rs");
    let query = parse_quick_open_query("main");
    let navigation = vec![
        NavigationLocation::new(path.clone(), 3, 2),
        NavigationLocation::new(path.clone(), 12, 5),
    ];

    assert_eq!(
        quick_open_target_with_navigation(path.clone(), &query, &navigation),
        (path.clone(), Some((12, 5)))
    );
    assert_eq!(
        quick_open_result_label_with_navigation(
            "src/main.rs",
            &query,
            quick_open_navigation_target(&navigation, &path),
        ),
        "src/main.rs:12:5"
    );

    let explicit = parse_quick_open_query("main:7:4");
    assert_eq!(
        quick_open_target_with_navigation(path.clone(), &explicit, &navigation),
        (path, Some((7, 4)))
    );
    assert_eq!(
        quick_open_result_label_with_navigation(
            "src/main.rs",
            &explicit,
            quick_open_navigation_target(&navigation, &PathBuf::from("workspace/src/main.rs")),
        ),
        "src/main.rs:7:4"
    );
}

#[test]
fn quick_open_clamps_stale_navigation_zero_targets() {
    let path = PathBuf::from("workspace/src/main.rs");
    let query = parse_quick_open_query("main");
    let navigation = vec![NavigationLocation::new(path.clone(), 0, 0)];

    assert_eq!(
        quick_open_target_with_navigation(path.clone(), &query, &navigation),
        (path.clone(), Some((1, 1)))
    );
    assert_eq!(
        quick_open_result_label_with_navigation(
            "src/main.rs",
            &query,
            quick_open_navigation_target(&navigation, &path),
        ),
        "src/main.rs:1:1"
    );
}

#[test]
fn quick_open_result_labels_sanitize_and_bound_display_paths() {
    let unsafe_rel = format!(
        "src/bad\n{}\u{202e}/main.rs",
        "very-long-segment-".repeat(24)
    );
    let query = parse_quick_open_query("main:12:3");

    let label = quick_open_result_label(&unsafe_rel, &query);

    assert!(label.ends_with(":12:3"));
    assert!(!label.contains('\n'));
    assert!(!label.contains('\u{202e}'));
    assert!(label.contains("..."));
    assert!(label.chars().count() <= QUICK_OPEN_RESULT_LABEL_MAX_CHARS);
}

#[test]
fn quick_open_navigation_locations_keep_latest_per_path() {
    let main = PathBuf::from("workspace/src/main.rs");
    let lib = PathBuf::from("workspace/src/lib.rs");
    let settings = PathBuf::from("workspace/src/settings.rs");
    let navigation = vec![
        NavigationLocation::new(main.clone(), 3, 2),
        NavigationLocation::new(lib.clone(), 8, 1),
        NavigationLocation::new(main.clone(), 12, 5),
        NavigationLocation::new(settings.clone(), 4, 9),
        NavigationLocation::new(lib.clone(), 20, 7),
    ];

    assert_eq!(
        quick_open_latest_navigation_locations_by_path(&navigation),
        vec![
            NavigationLocation::new(main, 12, 5),
            NavigationLocation::new(settings, 4, 9),
            NavigationLocation::new(lib, 20, 7),
        ]
    );
}

#[test]
fn quick_open_uses_latest_navigation_locations_for_score_target_and_label() {
    let repeated = PathBuf::from("workspace/src/main.rs");
    let other = PathBuf::from("workspace/src/lib.rs");
    let navigation = vec![
        NavigationLocation::new(other.clone(), 4, 1),
        NavigationLocation::new(repeated.clone(), 8, 3),
        NavigationLocation::new(repeated.clone(), 12, 5),
    ];
    let navigation = quick_open_latest_navigation_locations_by_path(&navigation);
    let query = QuickOpenMatchQuery::new("src");

    let repeated_score = quick_open_rank_score_with_navigation(
        100,
        &VecDeque::new(),
        &[],
        &VecDeque::new(),
        &navigation,
        &query,
        &repeated,
    );
    let other_score = quick_open_rank_score_with_navigation(
        100,
        &VecDeque::new(),
        &[],
        &VecDeque::new(),
        &navigation,
        &query,
        &other,
    );

    assert_eq!(repeated_score - other_score, 1);
    assert_eq!(
        quick_open_target_with_navigation(
            repeated.clone(),
            &parse_quick_open_query("main"),
            &navigation
        ),
        (repeated.clone(), Some((12, 5)))
    );
    assert_eq!(
        quick_open_result_label_with_navigation(
            "src/main.rs",
            &parse_quick_open_query("main"),
            quick_open_navigation_target(&navigation, &repeated),
        ),
        "src/main.rs:12:5"
    );
}

#[test]
fn quick_open_relative_labels_use_workspace_relative_paths() {
    let root = PathBuf::from("workspace");
    assert_eq!(
        quick_open_relative_label(&root, &root.join("src/main.rs")),
        "src/main.rs"
    );
    assert_eq!(
        quick_open_relative_label(&root, &PathBuf::from("external/main.rs")),
        "external/main.rs"
    );
}

#[cfg(windows)]
#[test]
fn quick_open_relative_labels_match_windows_paths_case_insensitively() {
    let root = PathBuf::from(r"C:\Users\kuroya\Project");
    let path = PathBuf::from(r"c:\users\kuroya\project\Src\Main.rs");

    assert_eq!(quick_open_relative_label(&root, &path), "Src/Main.rs");
}

#[test]
fn quick_open_recent_files_are_deduplicated_and_bounded() {
    let mut recent = VecDeque::new();
    record_quick_open_recent_file(&mut recent, PathBuf::from("a.rs"), 3);
    record_quick_open_recent_file(&mut recent, PathBuf::from("b.rs"), 3);
    record_quick_open_recent_file(&mut recent, PathBuf::from("a.rs"), 3);
    record_quick_open_recent_file(&mut recent, PathBuf::from("c.rs"), 3);
    record_quick_open_recent_file(&mut recent, PathBuf::from("d.rs"), 3);

    assert_eq!(
        recent.into_iter().collect::<Vec<_>>(),
        vec![
            PathBuf::from("d.rs"),
            PathBuf::from("c.rs"),
            PathBuf::from("a.rs")
        ]
    );
}

#[cfg(windows)]
#[test]
fn quick_open_recent_files_deduplicate_windows_case_variants() {
    let mut recent = VecDeque::from([PathBuf::from(r"C:\Repo\src\Main.rs")]);

    record_quick_open_recent_file(&mut recent, PathBuf::from(r"c:\repo\src\main.rs"), 10);

    assert_eq!(
        recent.into_iter().collect::<Vec<_>>(),
        vec![PathBuf::from(r"c:\repo\src\main.rs")]
    );
}

#[test]
fn quick_open_recent_files_normalize_persisted_order() {
    let root = PathBuf::from("workspace");
    let recent = normalize_quick_open_recent_files(
        vec![
            root.join("a.rs"),
            PathBuf::from("external.rs"),
            root.join("b.rs"),
            root.join("a.rs"),
            root.join("c.rs"),
            root.join("d.rs"),
        ],
        &root,
        3,
    );

    assert_eq!(
        recent.into_iter().collect::<Vec<_>>(),
        vec![root.join("a.rs"), root.join("b.rs"), root.join("c.rs")]
    );
}

#[cfg(windows)]
#[test]
fn quick_open_recent_files_normalize_windows_paths_case_insensitively() {
    let root = PathBuf::from(r"C:\Repo\Project");
    let first = PathBuf::from(r"c:\repo\project\src\main.rs");
    let duplicate = PathBuf::from(r"C:\REPO\PROJECT\src\MAIN.rs");
    let sibling = PathBuf::from(r"c:\repo\project-other\src\main.rs");

    let recent = normalize_quick_open_recent_files([first.clone(), duplicate, sibling], &root, 10);

    assert_eq!(recent.into_iter().collect::<Vec<_>>(), vec![first]);
    assert_eq!(
        normalize_quick_open_workspace_path(
            &root,
            &PathBuf::from(r"c:\repo\project-other\src\main.rs")
        ),
        None
    );
}

#[test]
fn quick_open_navigation_records_only_workspace_files() {
    let mut recent = VecDeque::new();
    let root = PathBuf::from("workspace");
    let inside = root.join("src/main.rs");
    let outside = PathBuf::from("external/main.rs");

    record_quick_open_navigation(&mut recent, &root, &inside, 10);
    record_quick_open_navigation(&mut recent, &root, &outside, 10);

    assert_eq!(recent, VecDeque::from([inside]));
}

#[test]
fn quick_open_query_memory_records_workspace_choices_by_normalized_query() {
    let mut memory = VecDeque::new();
    let root = PathBuf::from("workspace");
    let inside = root.join("src/main.rs");
    let outside = PathBuf::from("external/main.rs");

    record_quick_open_query_memory(&mut memory, &root, " Main ", &inside, 10);
    record_quick_open_query_memory(&mut memory, &root, "main", &inside, 10);
    record_quick_open_query_memory(&mut memory, &root, "main", &outside, 10);
    record_quick_open_query_memory(&mut memory, &root, "", &inside, 10);
    record_quick_open_query_memory(&mut memory, &root, "m", &inside, 10);

    assert_eq!(
        memory,
        VecDeque::from([QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: inside,
            uses: 2,
        }])
    );
}

#[test]
fn quick_open_query_memory_normalizes_persisted_entries() {
    let root = PathBuf::from("workspace");
    let inside = root.join("src/main.rs");
    let outside = PathBuf::from("external/main.rs");
    let memory = normalize_quick_open_query_memory(
        vec![
            QuickOpenQueryMemoryEntry {
                query: " Main ".to_owned(),
                path: inside.clone(),
                uses: 0,
            },
            QuickOpenQueryMemoryEntry {
                query: "main".to_owned(),
                path: inside.clone(),
                uses: 3,
            },
            QuickOpenQueryMemoryEntry {
                query: "main".to_owned(),
                path: outside,
                uses: 1,
            },
            QuickOpenQueryMemoryEntry {
                query: "m".to_owned(),
                path: inside.clone(),
                uses: 1,
            },
        ],
        &root,
        10,
    );

    assert_eq!(
        memory,
        VecDeque::from([QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: inside,
            uses: 3,
        }])
    );
}

#[cfg(windows)]
#[test]
fn quick_open_query_memory_merges_windows_paths_case_insensitively() {
    let root = PathBuf::from(r"C:\Repo\Project");
    let first = PathBuf::from(r"C:\REPO\PROJECT\src\Main.rs");
    let same_file = PathBuf::from(r"c:\repo\project\src\main.rs");
    let sibling = PathBuf::from(r"c:\repo\project-other\src\main.rs");

    let mut memory = normalize_quick_open_query_memory(
        [
            QuickOpenQueryMemoryEntry {
                query: " Main ".to_owned(),
                path: first.clone(),
                uses: 2,
            },
            QuickOpenQueryMemoryEntry {
                query: "main".to_owned(),
                path: same_file.clone(),
                uses: 5,
            },
            QuickOpenQueryMemoryEntry {
                query: "main".to_owned(),
                path: sibling,
                uses: 9,
            },
        ],
        &root,
        10,
    );

    assert_eq!(
        memory,
        VecDeque::from([QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: first,
            uses: 5,
        }])
    );

    record_quick_open_query_memory(&mut memory, &root, "MAIN", &same_file, 10);

    assert_eq!(
        memory,
        VecDeque::from([QuickOpenQueryMemoryEntry {
            query: "main".to_owned(),
            path: same_file,
            uses: 6,
        }])
    );
}

#[test]
fn quick_open_rank_score_boosts_recent_files() {
    let mut recent = VecDeque::new();
    record_quick_open_recent_file(&mut recent, PathBuf::from("src/main.rs"), 10);

    let base_score = 100;
    assert!(
        quick_open_rank_score_with_open_files(
            base_score,
            &recent,
            &[],
            &PathBuf::from("src/main.rs")
        ) > quick_open_rank_score_with_open_files(
            base_score,
            &recent,
            &[],
            &PathBuf::from("src/lib.rs")
        )
    );
}

#[test]
fn quick_open_rank_score_does_not_penalize_older_recent_files() {
    let mut recent = VecDeque::new();
    for index in 0..40 {
        record_quick_open_recent_file(&mut recent, PathBuf::from(format!("src/{index}.rs")), 80);
    }

    let old_recent = PathBuf::from("src/0.rs");
    assert!(quick_open_rank_score_with_open_files(100, &recent, &[], &old_recent) >= 100);
}

#[test]
fn quick_open_rank_score_boosts_open_files() {
    let recent = VecDeque::new();
    let open = PathBuf::from("src/main.rs");
    let closed = PathBuf::from("src/lib.rs");
    let open_files = [open.as_path()];

    assert!(
        quick_open_rank_score_with_open_files(100, &recent, &open_files, &open)
            > quick_open_rank_score_with_open_files(100, &recent, &open_files, &closed)
    );
}

#[test]
fn quick_open_rank_score_boosts_query_memory_matches() {
    let recent = VecDeque::new();
    let remembered = PathBuf::from("workspace/src/main.rs");
    let other = PathBuf::from("workspace/src/lib.rs");
    let memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "main".to_owned(),
        path: remembered.clone(),
        uses: 2,
    }]);
    let query = QuickOpenMatchQuery::new("MAIN");

    assert!(
        quick_open_rank_score(100, &recent, &[], &memory, &query, &remembered)
            > quick_open_rank_score(100, &recent, &[], &memory, &query, &other)
    );
}

#[test]
fn quick_open_rank_score_boosts_query_memory_prefix_refinements() {
    let recent = VecDeque::new();
    let remembered = PathBuf::from("workspace/src/service/main.rs");
    let other = PathBuf::from("workspace/src/settings/main.rs");
    let memory = VecDeque::from([
        QuickOpenQueryMemoryEntry {
            query: "ser".to_owned(),
            path: remembered.clone(),
            uses: 2,
        },
        QuickOpenQueryMemoryEntry {
            query: "settings".to_owned(),
            path: other.clone(),
            uses: 1,
        },
    ]);
    let refined_query = QuickOpenMatchQuery::new("service");

    assert!(
        quick_open_rank_score(100, &recent, &[], &memory, &refined_query, &remembered)
            > quick_open_rank_score(100, &recent, &[], &memory, &refined_query, &other)
    );
}

#[test]
fn quick_open_rank_score_boosts_query_memory_shortened_prefixes_after_three_chars() {
    let recent = VecDeque::new();
    let remembered = PathBuf::from("workspace/src/service/main.rs");
    let other = PathBuf::from("workspace/src/settings/main.rs");
    let memory = VecDeque::from([
        QuickOpenQueryMemoryEntry {
            query: "service".to_owned(),
            path: remembered.clone(),
            uses: 2,
        },
        QuickOpenQueryMemoryEntry {
            query: "settings".to_owned(),
            path: other.clone(),
            uses: 1,
        },
    ]);
    let shortened_query = QuickOpenMatchQuery::new("ser");

    assert!(
        quick_open_rank_score(100, &recent, &[], &memory, &shortened_query, &remembered)
            > quick_open_rank_score(100, &recent, &[], &memory, &shortened_query, &other)
    );
}

#[test]
fn quick_open_rank_score_does_not_boost_one_letter_query_memory_prefixes() {
    let recent = VecDeque::new();
    let remembered = PathBuf::from("workspace/src/service/main.rs");
    let other = PathBuf::from("workspace/src/settings/main.rs");
    let memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "s".to_owned(),
        path: remembered.clone(),
        uses: 8,
    }]);
    let query = QuickOpenMatchQuery::new("service");

    assert_eq!(
        quick_open_rank_score(100, &recent, &[], &memory, &query, &remembered),
        quick_open_rank_score(100, &recent, &[], &memory, &query, &other)
    );
}

#[test]
fn quick_open_rank_score_prefers_exact_query_memory_over_prefix_memory() {
    let recent = VecDeque::new();
    let exact = PathBuf::from("workspace/src/service/main.rs");
    let prefix = PathBuf::from("workspace/src/service.rs");
    let memory = VecDeque::from([
        QuickOpenQueryMemoryEntry {
            query: "ser".to_owned(),
            path: prefix.clone(),
            uses: 8,
        },
        QuickOpenQueryMemoryEntry {
            query: "service".to_owned(),
            path: exact.clone(),
            uses: 1,
        },
    ]);
    let query = QuickOpenMatchQuery::new("service");

    assert!(
        quick_open_rank_score(100, &recent, &[], &memory, &query, &exact)
            > quick_open_rank_score(100, &recent, &[], &memory, &query, &prefix)
    );
}

#[test]
fn quick_open_rank_score_ignores_unrelated_query_memory() {
    let recent = VecDeque::new();
    let remembered = PathBuf::from("workspace/src/main.rs");
    let other = PathBuf::from("workspace/src/lib.rs");
    let memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "main".to_owned(),
        path: remembered.clone(),
        uses: 8,
    }]);
    let query = QuickOpenMatchQuery::new("lib");

    assert_eq!(
        quick_open_rank_score(100, &recent, &[], &memory, &query, &remembered),
        quick_open_rank_score(100, &recent, &[], &memory, &query, &other)
    );
}

#[test]
fn quick_open_rank_score_boosts_recent_navigation_locations() {
    let recent = VecDeque::new();
    let navigated = PathBuf::from("workspace/src/main.rs");
    let other = PathBuf::from("workspace/src/lib.rs");
    let navigation = vec![NavigationLocation::new(navigated.clone(), 24, 7)];
    let query = QuickOpenMatchQuery::new("src");

    assert!(
        quick_open_rank_score_with_navigation(
            100,
            &recent,
            &[],
            &VecDeque::new(),
            &navigation,
            &query,
            &navigated,
        ) > quick_open_rank_score_with_navigation(
            100,
            &recent,
            &[],
            &VecDeque::new(),
            &navigation,
            &query,
            &other,
        )
    );
}

#[cfg(windows)]
#[test]
fn quick_open_navigation_targets_match_windows_paths_case_insensitively() {
    let path = PathBuf::from(r"C:\Repo\Project\src\Main.rs");
    let remembered = PathBuf::from(r"c:\repo\project\src\main.rs");
    let query = parse_quick_open_query("main");
    let navigation = vec![NavigationLocation::new(remembered.clone(), 12, 5)];

    assert_eq!(
        quick_open_target_with_navigation(path.clone(), &query, &navigation),
        (path.clone(), Some((12, 5)))
    );
    assert_eq!(
        quick_open_result_label_with_navigation(
            r"src\Main.rs",
            &query,
            quick_open_navigation_target(&navigation, &path),
        ),
        r"src\Main.rs:12:5"
    );

    let latest = quick_open_latest_navigation_locations_by_path(&[
        NavigationLocation::new(path, 3, 1),
        NavigationLocation::new(remembered.clone(), 12, 5),
    ]);
    assert_eq!(latest, vec![NavigationLocation::new(remembered, 12, 5)]);
}

#[test]
fn quick_open_match_score_prefers_file_name_matches_over_folder_matches() {
    let matcher = SkimMatcherV2::default();
    let query = QuickOpenMatchQuery::new("config");

    let file_name_match = quick_open_match_score(&matcher, "src/commands/config.rs", &query)
        .expect("file name should match");
    let folder_match =
        quick_open_match_score(&matcher, "src/config/mod.rs", &query).expect("folder should match");

    assert!(file_name_match.rank_score > folder_match.rank_score);
}

#[test]
fn quick_open_match_score_boosts_file_name_prefixes() {
    let matcher = SkimMatcherV2::default();
    let prefix_query = QuickOpenMatchQuery::new("main");

    let prefix = quick_open_match_score(&matcher, "src/main.rs", &prefix_query).unwrap();
    let contained = quick_open_match_score(&matcher, "src/domain.rs", &prefix_query).unwrap();

    assert!(prefix.rank_score > contained.rank_score);
}

#[test]
fn quick_open_match_score_boosts_file_name_segment_prefixes() {
    let matcher = SkimMatcherV2::default();
    let query = QuickOpenMatchQuery::new("config");

    let segment = quick_open_match_score(&matcher, "src/user_config.rs", &query).unwrap();
    let buried = quick_open_match_score(&matcher, "src/reconfig.rs", &query).unwrap();

    assert!(segment.rank_score > buried.rank_score);
}

#[test]
fn quick_open_match_score_matches_whitespace_tokens_across_path_segments() {
    let matcher = SkimMatcherV2::default();
    let query = QuickOpenMatchQuery::new("src main");

    assert!(quick_open_match_score(&matcher, "src/main.rs", &query).is_some());
    assert!(quick_open_match_score(&matcher, "src/lib.rs", &query).is_none());
    assert!(quick_open_match_score(&matcher, "docs/main.rs", &query).is_none());
}

#[test]
fn quick_open_ranked_results_filters_by_all_whitespace_tokens() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let files = [
        root.join("src/main.rs"),
        root.join("src/lib.rs"),
        root.join("docs/main.rs"),
    ];
    let query = QuickOpenMatchQuery::new("src main");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &VecDeque::new(),
        &[],
        &VecDeque::new(),
        &[],
        &query,
        10,
    );

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, root.join("src/main.rs"));
}

#[test]
fn quick_open_ranked_results_includes_matching_open_file_missing_from_index() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let indexed = [root.join("src/lib.rs")];
    let open = root.join("src/main.rs");
    let open_files = [open.as_path()];
    let query = QuickOpenMatchQuery::new("main");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        indexed.iter().map(|path| path.as_path()),
        &VecDeque::new(),
        &open_files,
        &VecDeque::new(),
        &[],
        &query,
        10,
    );

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, open);
    assert_eq!(results[0].rel, "src/main.rs");
}

#[test]
fn quick_open_ranked_results_deduplicates_indexed_open_file_candidate() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let indexed = root.join("src/main.rs");
    let equivalent_open = root.join("src").join("..").join("src").join("main.rs");
    let files = [indexed.clone()];
    let open_files = [equivalent_open.as_path()];
    let query = QuickOpenMatchQuery::new("main");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &VecDeque::new(),
        &open_files,
        &VecDeque::new(),
        &[],
        &query,
        10,
    );

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, indexed);
}

#[test]
fn quick_open_ranked_results_applies_lexical_path_bonuses() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let indexed = root.join("src").join("..").join("src").join("main.rs");
    let remembered = root.join("src/main.rs");
    let files = [indexed.clone()];
    let recent = VecDeque::from([remembered.clone()]);
    let open_files = [remembered.as_path()];
    let query_memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "main".to_owned(),
        path: remembered.clone(),
        uses: 3,
    }]);
    let navigation = vec![NavigationLocation::new(remembered.clone(), 12, 5)];
    let query = QuickOpenMatchQuery::new("main");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &recent,
        &open_files,
        &query_memory,
        &navigation,
        &query,
        10,
    );

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, indexed);
    assert_eq!(results[0].navigation_line_column, Some((12, 5)));

    let rel = quick_open_relative_label(&root, &results[0].path);
    let base = quick_open_match_score(&matcher, &rel, &query)
        .expect("indexed file should match")
        .rank_score;
    let expected = quick_open_rank_score_with_navigation(
        base,
        &recent,
        &open_files,
        &query_memory,
        &navigation,
        &query,
        &indexed,
    );
    assert_eq!(results[0].rank_score, expected);
    assert!(results[0].rank_score > base);
}

#[cfg(windows)]
#[test]
fn quick_open_ranked_results_applies_windows_case_insensitive_path_bonuses() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from(r"C:\Repo\Project");
    let indexed = PathBuf::from(r"C:\REPO\PROJECT\src\service\Main.rs");
    let remembered = PathBuf::from(r"c:\repo\project\src\service\main.rs");
    let files = [indexed.clone()];
    let recent = VecDeque::from([remembered.clone()]);
    let open_files = [remembered.as_path()];
    let query_memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "main".to_owned(),
        path: remembered.clone(),
        uses: 8,
    }]);
    let navigation = vec![NavigationLocation::new(remembered.clone(), 42, 9)];
    let query = QuickOpenMatchQuery::new("main");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &recent,
        &open_files,
        &query_memory,
        &navigation,
        &query,
        10,
    );

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, indexed);
    assert_eq!(results[0].rel, "src/service/Main.rs");

    let base = quick_open_match_score(&matcher, &results[0].rel, &query)
        .expect("indexed file should match")
        .rank_score;
    let expected = quick_open_rank_score_with_navigation(
        base,
        &recent,
        &open_files,
        &query_memory,
        &navigation,
        &query,
        &indexed,
    );
    assert_eq!(results[0].rank_score, expected);
    assert!(results[0].rank_score > base);
}

#[test]
fn quick_open_ranked_results_keeps_only_best_matches() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let files = (0..200)
        .map(|index| root.join(format!("src/file_{index:03}.rs")))
        .collect::<Vec<_>>();
    let recent = VecDeque::from([root.join("src/file_199.rs")]);
    let open_files = [files[198].as_path()];
    let query = QuickOpenMatchQuery::new("file");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &recent,
        &open_files,
        &VecDeque::new(),
        &[],
        &query,
        10,
    );

    assert_eq!(results.len(), 10);
    assert_eq!(results[0].path, root.join("src/file_199.rs"));
    assert!(
        results
            .iter()
            .any(|result| result.path == root.join("src/file_198.rs"))
    );
    assert!(
        results
            .windows(2)
            .all(|window| window[0].cmp(&window[1]).is_ge())
    );
}

#[test]
fn quick_open_ranked_results_matches_helper_bonus_scoring() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let remembered = root.join("src/service/main.rs");
    let open = root.join("src/service/open.rs");
    let recent = root.join("src/service/recent.rs");
    let navigated = root.join("src/service/navigation.rs");
    let files = [
        remembered.clone(),
        open.clone(),
        recent.clone(),
        navigated.clone(),
    ];
    let recent_files = VecDeque::from([recent.clone(), remembered.clone()]);
    let open_files = [open.as_path(), remembered.as_path()];
    let query_memory = VecDeque::from([
        QuickOpenQueryMemoryEntry {
            query: "ser".to_owned(),
            path: remembered.clone(),
            uses: 8,
        },
        QuickOpenQueryMemoryEntry {
            query: "service".to_owned(),
            path: remembered.clone(),
            uses: 1,
        },
        QuickOpenQueryMemoryEntry {
            query: "service".to_owned(),
            path: open.clone(),
            uses: 2,
        },
    ]);
    let navigation = vec![
        NavigationLocation::new(open.clone(), 12, 1),
        NavigationLocation::new(navigated.clone(), 42, 9),
        NavigationLocation::new(open.clone(), 90, 4),
    ];
    let query = QuickOpenMatchQuery::new("service");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &recent_files,
        &open_files,
        &query_memory,
        &navigation,
        &query,
        10,
    );

    assert_eq!(results.len(), files.len());
    for result in &results {
        let rel = quick_open_relative_label(&root, &result.path);
        let base = quick_open_match_score(&matcher, &rel, &query).unwrap();
        let expected = quick_open_rank_score_with_navigation(
            base.rank_score,
            &recent_files,
            &open_files,
            &query_memory,
            &navigation,
            &query,
            &result.path,
        );

        assert_eq!(result.rank_score, expected, "{}", result.rel);
    }
}

#[test]
fn quick_open_ranked_results_empty_query_prefers_navigation_recent_open_without_memory() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let untouched = root.join("src/service/untouched.rs");
    let open = root.join("src/service/open.rs");
    let recent = root.join("src/service/recent.rs");
    let navigated = root.join("src/service/navigation.rs");
    let files = [
        untouched.clone(),
        open.clone(),
        recent.clone(),
        navigated.clone(),
    ];
    let recent_files = VecDeque::from([recent.clone()]);
    let open_files = [open.as_path()];
    let query_memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "untouched".to_owned(),
        path: untouched.clone(),
        uses: 8,
    }]);
    let navigation = vec![NavigationLocation::new(navigated.clone(), 42, 9)];
    let query = QuickOpenMatchQuery::new("");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &recent_files,
        &open_files,
        &query_memory,
        &navigation,
        &query,
        10,
    );

    let paths = results
        .iter()
        .map(|result| result.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec![navigated, recent, open, untouched]);
}

#[test]
fn quick_open_ranked_results_empty_query_keeps_late_boosted_files_without_fuzzy_scan() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let files = (0..200)
        .map(|index| root.join(format!("src/file_{index:03}.rs")))
        .collect::<Vec<_>>();
    let recent_files = VecDeque::from([files[199].clone()]);
    let open_files = [files[198].as_path()];
    let navigation = vec![NavigationLocation::new(files[197].clone(), 4, 1)];
    let query = QuickOpenMatchQuery::new("");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &recent_files,
        &open_files,
        &VecDeque::new(),
        &navigation,
        &query,
        5,
    );

    let paths = results
        .iter()
        .map(|result| result.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(paths[0], files[197]);
    assert_eq!(paths[1], files[199]);
    assert_eq!(paths[2], files[198]);
    assert_eq!(paths.len(), 5);
    assert!(paths.contains(&files[0]));
}

#[test]
fn quick_open_ranked_results_empty_query_includes_open_file_missing_from_index() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let indexed = root.join("src/lib.rs");
    let open = root.join("src/main.rs");
    let files = [indexed.clone()];
    let open_files = [open.as_path()];
    let query = QuickOpenMatchQuery::new("");

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &VecDeque::new(),
        &open_files,
        &VecDeque::new(),
        &[],
        &query,
        10,
    );

    let paths = results
        .iter()
        .map(|result| result.path.clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec![open, indexed]);
}

#[test]
fn quick_open_ranked_results_prefers_remembered_query_choice() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let remembered = root.join("src/service/main.rs");
    let stronger_name = root.join("src/service.rs");
    let files = [remembered.clone(), stronger_name.clone()];
    let recent = VecDeque::new();
    let query = QuickOpenMatchQuery::new("main");
    let memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "main".to_owned(),
        path: remembered.clone(),
        uses: 4,
    }]);

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &recent,
        &[],
        &memory,
        &[],
        &query,
        10,
    );

    assert_eq!(results[0].path, remembered);
}

#[test]
fn quick_open_ranked_results_prefers_remembered_choice_for_refined_query() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let remembered = root.join("src/service/main.rs");
    let stronger_name = root.join("src/service.rs");
    let files = [remembered.clone(), stronger_name.clone()];
    let recent = VecDeque::new();
    let query = QuickOpenMatchQuery::new("service");
    let memory = VecDeque::from([QuickOpenQueryMemoryEntry {
        query: "ser".to_owned(),
        path: remembered.clone(),
        uses: 4,
    }]);

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &recent,
        &[],
        &memory,
        &[],
        &query,
        10,
    );

    assert_eq!(results[0].path, remembered);
}

#[test]
fn quick_open_ranked_results_prefers_recent_navigation_location() {
    let matcher = SkimMatcherV2::default();
    let root = PathBuf::from("workspace");
    let navigated = root.join("src/service.rs");
    let other = root.join("src/settings.rs");
    let files = [other.clone(), navigated.clone()];
    let query = QuickOpenMatchQuery::new("s");
    let navigation = vec![NavigationLocation::new(navigated.clone(), 42, 9)];

    let results = quick_open_ranked_results(
        &matcher,
        &root,
        files.iter().map(|path| path.as_path()),
        &VecDeque::new(),
        &[],
        &VecDeque::new(),
        &navigation,
        &query,
        10,
    );

    assert_eq!(results[0].path, navigated);
}

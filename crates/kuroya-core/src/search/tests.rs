use super::*;
use crate::ProjectIndex;
use std::{
    cell::Cell,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

mod line_match;

#[test]
fn project_search_rejects_oversized_query_before_preparing_matcher() {
    let options = SearchOptions {
        query: "n".repeat(MAX_SEARCH_QUERY_BYTES + 1),
        ..SearchOptions::default()
    };

    let error = match PreparedProjectSearch::new(&options) {
        Err(error) => error,
        Ok(_) => panic!("oversized search query should fail"),
    };

    assert!(error.contains("Search query is too long"));
}

#[test]
fn project_search_rejects_oversized_glob_pattern_before_building() {
    let options = SearchOptions {
        query: "needle".to_owned(),
        include_globs: vec!["a".repeat(MAX_SEARCH_GLOB_PATTERN_BYTES + 1)],
        ..SearchOptions::default()
    };

    let error = match PreparedProjectSearch::new(&options) {
        Err(error) => error,
        Ok(_) => panic!("oversized glob pattern should fail"),
    };

    assert!(error.contains("Glob pattern"));
    assert!(error.contains("too long"));
    assert!(error.contains(&MAX_SEARCH_GLOB_PATTERN_BYTES.to_string()));
}

#[test]
fn project_search_rejects_too_many_glob_patterns_before_building() {
    let options = SearchOptions {
        query: "needle".to_owned(),
        exclude_globs: (0..=MAX_SEARCH_GLOB_PATTERNS)
            .map(|index| format!("generated/{index}.rs"))
            .collect(),
        ..SearchOptions::default()
    };

    let error = match PreparedProjectSearch::new(&options) {
        Err(error) => error,
        Ok(_) => panic!("too many glob patterns should fail"),
    };

    assert!(error.contains("Too many glob patterns"));
    assert!(error.contains(&MAX_SEARCH_GLOB_PATTERNS.to_string()));
}

#[test]
fn cancellable_project_search_checks_before_oversized_query_validation() {
    let options = SearchOptions {
        query: "n".repeat(MAX_SEARCH_QUERY_BYTES + 1),
        ..SearchOptions::default()
    };

    let prepared = PreparedProjectSearch::new_with_cancel(&options, &|| true);

    assert!(prepared.is_none());
}

#[test]
fn project_search_index_signature_includes_created_identity() {
    let first = ProjectSearchIndexedFileSignature {
        len: 12,
        modified_nanos: 34,
        created_nanos: 56,
    };
    let second = ProjectSearchIndexedFileSignature {
        created_nanos: 57,
        ..first
    };

    assert_ne!(first, second);
}

#[test]
fn search_file_byte_limit_clamps_extreme_requests() {
    assert_eq!(
        effective_search_file_byte_limit(u64::MAX),
        MAX_SEARCH_FILE_BYTES
    );
    assert_eq!(effective_search_file_byte_limit(128), 128);
}

#[test]
fn indexed_search_skips_files_larger_than_remaining_text_budget_before_reading() {
    let root = search_fixture("indexed-remaining-budget", &[("src/lib.rs", "needle\n")]);
    let path = root.join("src/lib.rs");
    let mut indexed_text_bytes = 5;
    let (_signature, content) =
        read_indexed_search_file_content(&path, 64, 6, &mut indexed_text_bytes);

    assert!(matches!(
        content,
        ProjectSearchIndexedFileContent::IndexBudgetExceeded
    ));
    assert_eq!(indexed_text_bytes, 5);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn indexed_search_reader_preserves_binary_mapping_with_reused_metadata() {
    let root = search_fixture("indexed-reused-metadata-binary", &[]);
    let path = root.join("src/binary.dat");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"needle\0").unwrap();
    let mut indexed_text_bytes = 0;

    let (signature, content) =
        read_indexed_search_file_content(&path, 64, 64, &mut indexed_text_bytes);

    assert!(signature.is_some());
    assert!(matches!(
        content,
        ProjectSearchIndexedFileContent::BinaryOrInvalid
    ));
    assert_eq!(indexed_text_bytes, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn line_column_counter_uses_byte_offsets_for_ascii_lines() {
    let mut counter = SearchLineColumnCounter::new("needle x needle");

    assert_eq!(counter.column_for_byte(0), 1);
    assert_eq!(counter.column_for_byte(9), 10);
}

#[test]
fn line_column_counter_preserves_unicode_prefix_counts() {
    let mut counter = SearchLineColumnCounter::new("\u{00e9} needle needle");

    assert_eq!(counter.column_for_byte(3), 3);
    assert_eq!(counter.column_for_byte(10), 10);
}

#[test]
fn project_search_is_case_insensitive_by_default() {
    let root = search_fixture("case-insensitive", &[("src/lib.rs", "Alpha\nbeta\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "alpha".to_owned(),
            ..SearchOptions::default()
        },
    );

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].line, 1);
    assert_eq!(result.matches[0].column, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_prepared_query_preserves_case_word_and_glob_matching() {
    let root = search_fixture(
        "prepared-query",
        &[
            ("src/one.rs", "Alpha alphabet\n"),
            ("src/two.rs", "prefix ALPHA suffix\n"),
            ("tests/skip.rs", "alpha\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "alpha".to_owned(),
            whole_word: true,
            include_globs: vec!["src".to_owned()],
            ..SearchOptions::default()
        },
    );

    let paths = result
        .matches
        .iter()
        .map(|matched| matched.path.strip_prefix(&root).unwrap().to_path_buf())
        .collect::<Vec<_>>();
    let columns = result
        .matches
        .iter()
        .map(|matched| matched.column)
        .collect::<Vec<_>>();
    assert!(result.error.is_none());
    assert_eq!(
        paths,
        vec![PathBuf::from("src/one.rs"), PathBuf::from("src/two.rs")]
    );
    assert_eq!(columns, vec![1, 8]);
    assert_eq!(result.stats.searched_files, 2);
    assert_eq!(result.stats.matched_files, 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellable_project_search_returns_none_when_cancelled_before_scan() {
    let root = search_fixture("cancel-before-scan", &[("src/lib.rs", "needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&index, SearchOptions::default().max_file_bytes);

    let result = search_project_index_with_cancel(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
        || true,
    );

    assert!(result.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellable_search_index_build_checks_between_files() {
    let root = search_fixture(
        "cancel-index-build-between-files",
        &[
            ("src/one.rs", "needle\n"),
            ("src/two.rs", "needle\n"),
            ("src/three.rs", "needle\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);
    let cancellation_checks = Cell::new(0usize);

    let search_index = ProjectSearchIndex::build_with_text_budget_and_cancel(
        &index,
        SearchOptions::default().max_file_bytes,
        PROJECT_SEARCH_INDEX_MAX_TEXT_BYTES,
        || {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next > 2
        },
    );

    assert!(search_index.is_none());
    assert_eq!(cancellation_checks.get(), 3);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellable_project_search_checks_before_streaming_files() {
    let root = search_fixture(
        "cancel-project-search-before-streaming",
        &[
            ("src/one.rs", "needle\n"),
            ("src/two.rs", "needle\n"),
            ("src/three.rs", "needle\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);
    let cancellation_checks = Cell::new(0usize);

    let result = search_project_with_cancel(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
        || {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next > 4
        },
    );

    assert!(result.is_none());
    assert_eq!(cancellation_checks.get(), 5);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_progress_reports_chunked_deltas() {
    let files = (0..(SEARCH_FILE_CHUNK_SIZE + 1))
        .map(|index| {
            (
                format!("src/file_{index:03}.rs"),
                format!("fn file_{index}() {{ let needle = {index}; }}\n"),
            )
        })
        .collect::<Vec<_>>();
    let borrowed_files = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let root = search_fixture("progress-chunked-deltas", &borrowed_files);
    let index = ProjectIndex::rebuild(&root, 400_000);
    let mut progress_events = Vec::new();

    let result = search_project_with_cancel_and_progress(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: SEARCH_FILE_CHUNK_SIZE + 1,
            ..SearchOptions::default()
        },
        || false,
        |progress| progress_events.push(progress),
    )
    .expect("search should finish");

    assert!(progress_events.len() >= 2);
    assert_eq!(result.matches.len(), SEARCH_FILE_CHUNK_SIZE + 1);
    assert_eq!(
        progress_events
            .iter()
            .map(|progress| progress.matches.len())
            .sum::<usize>(),
        result.matches.len()
    );
    assert_eq!(
        progress_events
            .iter()
            .map(|progress| progress.stats.searched_files)
            .sum::<usize>(),
        result.stats.searched_files
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_progress_emits_first_match_before_remaining_files() {
    let mut files = Vec::new();
    for index in 0..10 {
        files.push((
            format!("src/{index:03}_before.rs"),
            format!("haystack before {index}\n"),
        ));
    }
    files.push(("src/010_match.rs".to_owned(), "needle\n".to_owned()));
    for index in 11..80 {
        files.push((
            format!("src/{index:03}_after.rs"),
            format!("haystack after {index}\n"),
        ));
    }
    let borrowed_files = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let root = search_fixture("progress-first-match-before-tail", &borrowed_files);
    let index = ProjectIndex::rebuild(&root, 40_000);
    let mut progress_events = Vec::new();

    let result = search_project_with_cancel_and_progress(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
        || false,
        |progress| progress_events.push(progress),
    )
    .expect("search should finish");

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.stats.searched_files, 80);
    assert!(progress_events.len() >= 2);
    let first_match_event = progress_events
        .iter()
        .find(|progress| !progress.matches.is_empty())
        .expect("first match should be emitted");
    assert_eq!(first_match_event.matches.len(), 1);
    assert_eq!(first_match_event.stats.searched_files, 11);
    assert_eq!(
        progress_events
            .iter()
            .map(|progress| progress.stats.searched_files)
            .sum::<usize>(),
        result.stats.searched_files
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_progress_reports_binary_skip_stats_without_matches() {
    let root = search_fixture("progress-binary-skips", &[]);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/binary.dat"), b"needle\0").unwrap();
    fs::write(root.join("src/invalid.txt"), vec![b'n', b'e', b'e', 0xff]).unwrap();
    let index = ProjectIndex::rebuild(&root, 40_000);
    let mut progress_events = Vec::new();

    let result = search_project_with_cancel_and_progress(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
        || false,
        |progress| progress_events.push(progress),
    )
    .expect("search should finish");

    assert!(result.matches.is_empty());
    assert_eq!(result.stats.searched_files, 0);
    assert_eq!(result.stats.skipped_binary_files, 2);
    assert_eq!(progress_events.len(), 1);
    assert!(progress_events[0].matches.is_empty());
    assert_eq!(progress_events[0].stats.skipped_binary_files, 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_metadata_cache_records_binary_skip_state() {
    let root = search_fixture("metadata-cache-binary", &[]);
    fs::create_dir_all(root.join("src")).unwrap();
    let path = root.join("src/binary.dat");
    fs::write(&path, b"needle\0").unwrap();
    let index = ProjectIndex::rebuild(&root, 40_000);
    let cache = ProjectSearchMetadataCache::default();

    let result = search_project_with_metadata_cache_and_progress(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
        &cache,
        || false,
        |_| {},
    )
    .expect("search should finish");

    assert!(result.matches.is_empty());
    assert_eq!(result.stats.skipped_binary_files, 1);
    assert_eq!(cache.len(), 1);
    let entries = cache
        .entries
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cached = entries.get(&path).expect("binary file metadata is cached");
    assert_eq!(cached.signature.len, 7);
    assert_eq!(
        cached.state,
        ProjectSearchFileMetadataState::BinaryOrInvalid
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_metadata_cache_does_not_reuse_different_size_limits() {
    let root = search_fixture("metadata-cache-size-limit", &[("src/lib.rs", "needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);
    let cache = ProjectSearchMetadataCache::default();

    let small_limit = search_project_with_metadata_cache_and_progress(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 3,
            ..SearchOptions::default()
        },
        &cache,
        || false,
        |_| {},
    )
    .expect("search should finish");
    let larger_limit = search_project_with_metadata_cache_and_progress(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
        &cache,
        || false,
        |_| {},
    )
    .expect("search should finish");

    assert!(small_limit.matches.is_empty());
    assert_eq!(small_limit.stats.skipped_large_files, 1);
    assert_eq!(larger_limit.matches.len(), 1);
    assert_eq!(larger_limit.stats.searched_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellable_project_search_checks_inside_streamed_files() {
    let text = (0..512)
        .map(|line| format!("haystack {line}\n"))
        .collect::<String>();
    let root = search_fixture("cancel-inside-streamed-file", &[("src/lib.rs", &text)]);
    let index = ProjectIndex::rebuild(&root, 40_000);
    let cancellation_checks = Cell::new(0usize);

    let result = search_project_with_cancel(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 10_000,
            ..SearchOptions::default()
        },
        || {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next > 8
        },
    );

    assert!(result.is_none());
    assert!(cancellation_checks.get() > 8);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellable_project_search_checks_inside_dense_same_line_matches() {
    let text = "needle ".repeat(SEARCH_CANCEL_MATCH_INTERVAL + 1);
    let options = SearchOptions {
        query: "needle".to_owned(),
        max_results: 10_000,
        ..SearchOptions::default()
    };
    let line_needle = LineSearchNeedle::new(&options.query, options.case_sensitive);
    let cancellation_checks = Cell::new(0usize);
    let mut result_budget = SearchResultBudget::new(options.max_results);
    let mut matches = Vec::new();

    let result = search_text_file(
        Path::new("src/lib.rs"),
        &text,
        &line_needle,
        &options,
        &mut result_budget,
        &mut matches,
        &|| {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next >= 2
        },
    );

    assert!(result.is_none());
    assert_eq!(cancellation_checks.get(), 2);
    assert_eq!(matches.len(), SEARCH_CANCEL_MATCH_INTERVAL - 1);
}

#[test]
fn cancellable_project_search_checks_between_indexed_files() {
    let root = search_fixture(
        "cancel-between-files",
        &[
            ("src/one.rs", "needle\n"),
            ("src/two.rs", "needle\n"),
            ("src/three.rs", "needle\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&index, SearchOptions::default().max_file_bytes);
    let cancellation_checks = Cell::new(0usize);

    let result = search_project_index_with_cancel(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
        || {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next > 4
        },
    );

    assert!(result.is_none());
    assert!(cancellation_checks.get() > 4);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellable_project_search_checks_inside_large_indexed_files() {
    let text = (0..512)
        .map(|line| format!("haystack {line}\n"))
        .collect::<String>();
    let root = search_fixture("cancel-inside-large-file", &[("src/lib.rs", &text)]);
    let index = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&index, SearchOptions::default().max_file_bytes);
    let cancellation_checks = Cell::new(0usize);

    let result = search_project_index_with_cancel(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 10_000,
            ..SearchOptions::default()
        },
        || {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next > 4
        },
    );

    assert!(result.is_none());
    assert!(cancellation_checks.get() > 4);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellable_project_search_checks_before_first_indexed_line() {
    let text = "haystack ".repeat(4096);
    let root = search_fixture("cancel-before-first-line", &[("src/lib.rs", &text)]);
    let index = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&index, SearchOptions::default().max_file_bytes);
    let cancellation_checks = Cell::new(0usize);

    let result = search_project_index_with_cancel(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 10_000,
            ..SearchOptions::default()
        },
        || {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next > 4
        },
    );

    assert!(result.is_none());
    assert!(cancellation_checks.get() > 4);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellable_project_search_checks_inside_long_unmatched_lines() {
    let text = "haystack ".repeat((SEARCH_CANCEL_BYTE_INTERVAL / "haystack ".len()) * 4);
    let root = search_fixture("cancel-inside-long-line", &[("src/lib.rs", &text)]);
    let index = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&index, SearchOptions::default().max_file_bytes);
    let cancellation_checks = Cell::new(0usize);

    let result = search_project_index_with_cancel(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 10_000,
            ..SearchOptions::default()
        },
        || {
            let next = cancellation_checks.get().saturating_add(1);
            cancellation_checks.set(next);
            next > 5
        },
    );

    assert!(result.is_none());
    assert!(cancellation_checks.get() > 5);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn case_sensitive_project_search_requires_exact_case() {
    let root = search_fixture("case-sensitive", &[("src/lib.rs", "Alpha\nalpha\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "alpha".to_owned(),
            case_sensitive: true,
            ..SearchOptions::default()
        },
    );

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].line, 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn whole_word_project_search_skips_embedded_identifiers() {
    let root = search_fixture(
        "whole-word",
        &[("src/lib.rs", "alpha alphabet alpha_1 betaalpha\n")],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "alpha".to_owned(),
            whole_word: true,
            ..SearchOptions::default()
        },
    );

    let columns = result
        .matches
        .iter()
        .map(|matched| matched.column)
        .collect::<Vec<_>>();
    assert_eq!(columns, vec![1]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn whole_word_project_search_can_find_later_match_on_same_line() {
    let root = search_fixture("whole-word-later", &[("src/lib.rs", "alphabet alpha\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "alpha".to_owned(),
            whole_word: true,
            ..SearchOptions::default()
        },
    );

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].column, 10);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_reports_multiple_matches_on_same_line() {
    let root = search_fixture(
        "multiple-same-line",
        &[("src/lib.rs", "needle x needle needle\n")],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
    );

    let columns = result
        .matches
        .iter()
        .map(|matched| matched.column)
        .collect::<Vec<_>>();
    assert_eq!(columns, vec![1, 10, 17]);
    assert!(result.matches.iter().all(|matched| matched.line == 1));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_columns_keep_unicode_prefix_counts() {
    let root = search_fixture(
        "unicode-column-counts",
        &[("src/lib.rs", "\u{00e9} needle needle\n")],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
    );

    let columns = result
        .matches
        .iter()
        .map(|matched| matched.column)
        .collect::<Vec<_>>();
    assert_eq!(columns, vec![3, 10]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn search_preview_unicode_slice_bounds_match_char_window() {
    assert_eq!(slice_chars("\u{00e9}a\u{4e2d}b", 1, 3), "a\u{4e2d}");
    assert_eq!(
        slice_chars("\u{00e9}a\u{4e2d}b", 0, 4),
        "\u{00e9}a\u{4e2d}b"
    );
    assert_eq!(slice_chars("\u{00e9}a\u{4e2d}b", 4, 8), "");
}

#[test]
fn search_preview_clamps_non_ascii_match_offsets_to_char_boundaries() {
    let line = format!("  \u{00e9}{}", "a".repeat(MAX_SEARCH_PREVIEW_CHARS + 32));

    let mid_char_preview = search_preview(&line, 3);
    let oversized_preview = search_preview(&line, usize::MAX);

    assert!(mid_char_preview.chars().count() <= MAX_SEARCH_PREVIEW_CHARS + 6);
    assert!(mid_char_preview.ends_with("..."));
    assert!(oversized_preview.chars().count() <= MAX_SEARCH_PREVIEW_CHARS + 6);
    assert!(oversized_preview.starts_with("..."));
}

#[test]
fn cancellable_project_search_can_cancel_during_preparation() {
    let options = SearchOptions {
        query: "needle".to_owned(),
        exclude_globs: vec!["[".to_owned()],
        ..SearchOptions::default()
    };
    let cancellation_checks = Cell::new(0usize);

    let prepared = PreparedProjectSearch::new_with_cancel(&options, &|| {
        let next = cancellation_checks.get().saturating_add(1);
        cancellation_checks.set(next);
        next >= 2
    });

    assert!(prepared.is_none());
    assert_eq!(cancellation_checks.get(), 2);
}

#[test]
fn cancellable_project_search_checks_before_stale_live_reread() {
    let root = search_fixture("cancel-before-stale-reread", &[("src/lib.rs", "needle\n")]);
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&project, SearchOptions::default().max_file_bytes);
    fs::write(root.join("src/lib.rs"), "changed needle on disk\n").unwrap();
    let options = SearchOptions {
        query: "needle".to_owned(),
        ..SearchOptions::default()
    };
    let prepared = PreparedProjectSearch::new(&options)
        .expect("valid search options")
        .expect("non-empty search query");
    let is_cancelled = || true;
    let context = ProjectSearchContext {
        root: search_index.root(),
        indexed_max_file_bytes: search_index.max_file_bytes(),
        line_needle: &prepared.line_needle,
        options: &options,
        include_globs: prepared.include_globs.as_ref(),
        exclude_globs: prepared.exclude_globs.as_ref(),
        is_cancelled: &is_cancelled,
    };
    let mut result_budget = SearchResultBudget::new(10);
    let mut matches = Vec::new();

    let outcome = search_project_index_file(
        &context,
        &search_index.files[0],
        &mut result_budget,
        &mut matches,
    );

    assert!(matches!(outcome, FileSearchOutcome::Cancelled));
    assert!(matches.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_refreshes_stale_file_text() {
    let root = search_fixture("indexed-text", &[("src/lib.rs", "needle\n")]);
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&project, SearchOptions::default().max_file_bytes);

    fs::write(root.join("src/lib.rs"), "changed needle on disk\n").unwrap();
    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 1);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].preview, "changed needle on disk");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_reports_deleted_stale_file_as_unreadable() {
    let root = search_fixture(
        "indexed-deleted-file",
        &[
            ("src/lib.rs", "needle\n"),
            ("src/deleted.rs", "needle should disappear\n"),
        ],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&project, SearchOptions::default().max_file_bytes);

    fs::remove_file(root.join("src/deleted.rs")).unwrap();
    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 2);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(
        result.matches[0].path.strip_prefix(&root).unwrap(),
        Path::new("src/lib.rs")
    );
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.matched_files, 1);
    assert_eq!(result.stats.skipped_unreadable_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_preserves_skip_stats() {
    let root = search_fixture("indexed-skip-stats", &[("src/lib.rs", "needle\n")]);
    fs::write(root.join("src/binary.dat"), b"needle\0").unwrap();
    fs::write(root.join("src/too-large.rs"), "needle needle needle\n").unwrap();
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&project, 12);

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 12,
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 3);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.skipped_binary_files, 1);
    assert_eq!(result.stats.skipped_large_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_rereads_too_large_files_when_search_limit_increases() {
    let root = search_fixture(
        "indexed-larger-search-limit",
        &[
            ("src/small.rs", "small needle\n"),
            ("src/later.rs", "expanded needle text\n"),
        ],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&project, 12);

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
    );

    let paths = result
        .matches
        .iter()
        .map(|matched| matched.path.strip_prefix(&root).unwrap().to_path_buf())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![PathBuf::from("src/later.rs"), PathBuf::from("src/small.rs")]
    );
    assert_eq!(result.stats.searched_files, 2);
    assert_eq!(result.stats.matched_files, 2);
    assert_eq!(result.stats.skipped_large_files, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_rereads_stale_too_large_file_when_it_shrinks() {
    let root = search_fixture(
        "indexed-stale-large-shrinks",
        &[("src/later.rs", "expanded needle text\n")],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build(&project, 12);

    fs::write(root.join("src/later.rs"), "needle\n").unwrap();
    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 12,
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 1);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].preview, "needle");
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.skipped_large_files, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_respects_aggregate_text_budget() {
    let root = search_fixture(
        "indexed-text-budget",
        &[("src/first.rs", "needle\n"), ("src/second.rs", "needle\n")],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build_with_text_budget(&project, 64, 6);

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 2);
    assert!(result.matches.is_empty());
    assert_eq!(result.stats.searched_files, 0);
    assert_eq!(result.stats.skipped_index_budget_files, 2);
    assert_eq!(result.stats.skipped_files(), 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_rereads_stale_budget_entries() {
    let first = "first needle\n";
    let root = search_fixture(
        "indexed-text-budget-stale-entry",
        &[
            ("src/000_first.rs", first),
            ("src/001_budget.rs", "budget needle\n"),
        ],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build_with_text_budget(&project, 64, first.len() as u64);

    fs::write(root.join("src/001_budget.rs"), "changed budget needle\n").unwrap();
    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
    );

    assert_eq!(result.matches.len(), 2);
    assert!(
        result
            .matches
            .iter()
            .any(|entry| entry.path.ends_with("src/001_budget.rs"))
    );
    assert_eq!(result.stats.searched_files, 2);
    assert_eq!(result.stats.skipped_index_budget_files, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_ignores_binary_invalid_files_for_text_budget() {
    let valid_text = "needle\n";
    let root = search_fixture(
        "indexed-text-budget-binary-invalid",
        &[("src/002_valid.rs", valid_text)],
    );
    fs::write(root.join("src/000_binary.dat"), b"needle\0").unwrap();
    fs::write(
        root.join("src/001_invalid.txt"),
        vec![b'n', b'e', b'e', 0xff],
    )
    .unwrap();
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index =
        ProjectSearchIndex::build_with_text_budget(&project, 64, valid_text.len() as u64);

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 3);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(
        result.matches[0].path.strip_prefix(&root).unwrap(),
        Path::new("src/002_valid.rs")
    );
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.matched_files, 1);
    assert_eq!(result.stats.skipped_binary_files, 2);
    assert_eq!(result.stats.skipped_index_budget_files, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_zero_text_budget_is_unlimited() {
    let root = search_fixture(
        "indexed-zero-text-budget",
        &[
            ("src/000_first.rs", "first needle\n"),
            ("src/001_second.rs", "second needle\n"),
        ],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build_with_text_budget(&project, 64, 0);

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 2);
    assert_eq!(result.matches.len(), 2);
    assert_eq!(result.stats.searched_files, 2);
    assert_eq!(result.stats.skipped_index_budget_files, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_reports_unreadable_after_text_budget_exhausted() {
    let first = "first needle\n";
    let root = search_fixture(
        "indexed-text-budget-unreadable",
        &[
            ("src/000_first.rs", first),
            ("src/001_missing.rs", "missing needle\n"),
        ],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    fs::remove_file(root.join("src/001_missing.rs")).unwrap();
    let search_index = ProjectSearchIndex::build_with_text_budget(&project, 64, first.len() as u64);

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 2);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.skipped_unreadable_files, 1);
    assert_eq!(result.stats.skipped_index_budget_files, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_counts_later_binary_invalid_as_budget_after_exact_exhaustion() {
    let first = "first needle\n";
    let root = search_fixture(
        "indexed-text-budget-later-binary",
        &[("src/000_first.rs", first)],
    );
    fs::write(root.join("src/001_binary.dat"), b"needle\0").unwrap();
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build_with_text_budget(&project, 64, first.len() as u64);

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 2);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.skipped_binary_files, 0);
    assert_eq!(result.stats.skipped_index_budget_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_counts_later_too_large_before_budget_after_exact_exhaustion() {
    let first = "needle\n";
    let root = search_fixture(
        "indexed-text-budget-later-large",
        &[
            ("src/000_first.rs", first),
            ("src/001_large.rs", "needle needle\n"),
        ],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build_with_text_budget(
        &project,
        first.len() as u64,
        first.len() as u64,
    );

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: first.len() as u64,
            ..SearchOptions::default()
        },
    );

    assert_eq!(search_index.len(), 2);
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.skipped_large_files, 1);
    assert_eq!(result.stats.skipped_index_budget_files, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_index_applies_text_budget_in_project_order() {
    let first = "first needle\n";
    let second = "second needle\n";
    let third = "third needle\n";
    let root = search_fixture(
        "indexed-text-budget-order",
        &[
            ("src/000_first.rs", first),
            ("src/001_second.rs", second),
            ("src/002_third.rs", third),
        ],
    );
    let project = ProjectIndex::rebuild(&root, 40_000);
    let search_index = ProjectSearchIndex::build_with_text_budget(
        &project,
        64,
        (first.len() + second.len()) as u64,
    );

    let result = search_project_index(
        &search_index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 64,
            ..SearchOptions::default()
        },
    );

    let matched_paths = result
        .matches
        .iter()
        .map(|matched| matched.path.strip_prefix(&root).unwrap().to_path_buf())
        .collect::<Vec<_>>();

    assert_eq!(
        matched_paths,
        vec![
            PathBuf::from("src/000_first.rs"),
            PathBuf::from("src/001_second.rs"),
        ]
    );
    assert_eq!(result.stats.searched_files, 2);
    assert_eq!(result.stats.matched_files, 2);
    assert_eq!(result.stats.skipped_index_budget_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_same_line_matches_respect_whole_word() {
    let root = search_fixture(
        "same-line-whole-word",
        &[("src/lib.rs", "needle needlex needle-needle\n")],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            whole_word: true,
            ..SearchOptions::default()
        },
    );

    let columns = result
        .matches
        .iter()
        .map(|matched| matched.column)
        .collect::<Vec<_>>();
    assert_eq!(columns, vec![1, 16, 23]);
    assert!(result.matches.iter().all(|matched| matched.line == 1));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_include_globs_limit_files() {
    let root = search_fixture(
        "include-globs",
        &[
            ("src/lib.rs", "needle\n"),
            ("tests/lib.rs", "needle\n"),
            ("README.md", "needle\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            include_globs: vec!["src/**/*.rs".to_owned(), "*.md".to_owned()],
            ..SearchOptions::default()
        },
    );

    let paths = result
        .matches
        .iter()
        .map(|result| result.path.strip_prefix(&root).unwrap().to_path_buf())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![PathBuf::from("README.md"), PathBuf::from("src/lib.rs")]
    );
    assert_eq!(result.stats.searched_files, 2);
    assert_eq!(result.stats.matched_files, 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_bare_include_globs_match_descendants() {
    let root = search_fixture(
        "bare-include-globs",
        &[
            ("src/lib.rs", "needle\n"),
            ("crates/app/src/main.rs", "needle\n"),
            ("tests/lib.rs", "needle\n"),
            ("README.md", "needle\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            include_globs: vec!["src".to_owned()],
            ..SearchOptions::default()
        },
    );

    let paths = result
        .matches
        .iter()
        .map(|result| result.path.strip_prefix(&root).unwrap().to_path_buf())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            PathBuf::from("crates/app/src/main.rs"),
            PathBuf::from("src/lib.rs")
        ]
    );
    assert_eq!(result.stats.searched_files, 2);
    assert_eq!(result.stats.matched_files, 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_exclude_globs_skip_files() {
    let root = search_fixture(
        "exclude-globs",
        &[
            ("src/lib.rs", "needle\n"),
            ("target/generated.rs", "needle\n"),
            ("src/generated.snap", "needle\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            exclude_globs: vec!["target/**".to_owned(), "*.snap".to_owned()],
            ..SearchOptions::default()
        },
    );

    assert_eq!(result.matches.len(), 1);
    assert_eq!(
        result.matches[0].path.strip_prefix(&root).unwrap(),
        Path::new("src/lib.rs")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_bare_exclude_globs_skip_descendants() {
    let root = search_fixture(
        "bare-exclude-globs",
        &[
            ("src/lib.rs", "needle\n"),
            ("target/generated.rs", "needle\n"),
            ("crates/app/target/generated.rs", "needle\n"),
            ("src/target_name.rs", "needle\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            exclude_globs: vec!["target".to_owned()],
            ..SearchOptions::default()
        },
    );

    let paths = result
        .matches
        .iter()
        .map(|result| result.path.strip_prefix(&root).unwrap().to_path_buf())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            PathBuf::from("src/lib.rs"),
            PathBuf::from("src/target_name.rs")
        ]
    );
    assert_eq!(result.stats.searched_files, 2);
    assert_eq!(result.stats.matched_files, 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_skips_binary_invalid_utf8_and_oversized_files() {
    let root = search_fixture(
        "skip-non-text",
        &[
            ("src/lib.rs", "needle\n"),
            ("src/too-large.rs", "needle needle needle\n"),
        ],
    );
    fs::write(root.join("src/binary.dat"), b"needle\0").unwrap();
    fs::write(root.join("src/invalid.txt"), vec![b'n', b'e', b'e', 0xff]).unwrap();
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_file_bytes: 12,
            ..SearchOptions::default()
        },
    );

    assert_eq!(result.matches.len(), 1);
    assert_eq!(
        result.matches[0].path.strip_prefix(&root).unwrap(),
        Path::new("src/lib.rs")
    );
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.matched_files, 1);
    assert_eq!(result.stats.skipped_large_files, 1);
    assert_eq!(result.stats.skipped_binary_files, 2);
    assert_eq!(result.stats.skipped_unreadable_files, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_discards_streamed_matches_when_later_chunk_is_binary() {
    let root = search_fixture("streamed-late-binary", &[]);
    fs::create_dir_all(root.join("src")).unwrap();
    let content = format!(
        "needle\n{}\0\n",
        "a".repeat(SEARCH_STREAM_BUFFER_BYTES.saturating_add(16))
    );
    fs::write(root.join("src/lib.rs"), content).unwrap();
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
    );

    assert!(result.matches.is_empty());
    assert_eq!(result.stats.searched_files, 0);
    assert_eq!(result.stats.matched_files, 0);
    assert_eq!(result.stats.skipped_binary_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_reports_files_removed_after_indexing() {
    let root = search_fixture(
        "deleted-after-index",
        &[
            ("src/lib.rs", "needle\n"),
            ("src/deleted.rs", "needle should not panic\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);
    fs::remove_file(root.join("src/deleted.rs")).unwrap();

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
    );

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.matched_files, 1);
    assert_eq!(result.stats.skipped_unreadable_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn search_text_reader_rechecks_size_after_reading() {
    let root = search_fixture("post-read-limit", &[("src/lib.rs", "needle\n")]);
    let path = root.join("src/lib.rs");

    assert!(matches!(
        read_searchable_text(&path, 3),
        SearchTextRead::TooLarge
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn search_text_reader_with_reused_metadata_keeps_post_read_size_guard() {
    let root = search_fixture("reused-metadata-post-read-limit", &[("src/lib.rs", "nee")]);
    let path = root.join("src/lib.rs");
    let metadata = fs::metadata(&path).unwrap();
    fs::write(&path, "needle\n").unwrap();
    let mut file = fs::File::open(&path).unwrap();

    assert!(matches!(
        read_searchable_text_with_metadata(&mut file, 3, &metadata),
        SearchTextRead::TooLarge
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn search_text_reader_reads_only_one_byte_past_limit() {
    let root = search_fixture("bounded-post-read-limit", &[("src/lib.rs", "needle\n")]);
    let path = root.join("src/lib.rs");
    let mut file = fs::File::open(&path).unwrap();

    let bytes = read_file_prefix(&mut file, 3).unwrap();

    assert_eq!(bytes, b"need");
    assert!(matches!(
        read_searchable_text(&path, 3),
        SearchTextRead::TooLarge
    ));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_reports_invalid_globs() {
    let root = search_fixture("invalid-glob", &[("src/lib.rs", "needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            include_globs: vec!["[".to_owned()],
            ..SearchOptions::default()
        },
    );

    assert!(result.matches.is_empty());
    assert!(result.error.is_some());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_empty_query_returns_before_invalid_globs() {
    let root = search_fixture("empty-query-invalid-glob", &[("src/lib.rs", "needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: " \t\n ".to_owned(),
            include_globs: vec!["[".to_owned()],
            exclude_globs: vec!["bad[".to_owned()],
            ..SearchOptions::default()
        },
    );

    assert!(result.matches.is_empty());
    assert!(!result.truncated);
    assert!(result.error.is_none());
    assert_eq!(result.stats, SearchStats::default());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_sanitizes_invalid_exclude_glob_errors() {
    let root = search_fixture("invalid-exclude-glob", &[("src/lib.rs", "needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            exclude_globs: vec!["bad\n\u{202e}[".to_owned()],
            ..SearchOptions::default()
        },
    );

    let error = result.error.expect("invalid exclude glob should fail");
    assert!(result.matches.is_empty());
    assert!(!error.contains('\n'));
    assert!(!error.contains('\u{202e}'));
    assert!(error.contains("\\n"));
    assert!(error.contains("\\u{202e}"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_glob_error_formatter_bounds_pattern_and_detail() {
    let long_pattern = format!(
        "prefix\n\u{202e}{}",
        "x".repeat(GLOB_ERROR_PATTERN_MAX_CHARS * 4)
    );
    let long_detail = format!(
        "detail\r\u{2066}{}",
        "y".repeat(GLOB_ERROR_DETAIL_MAX_CHARS * 4)
    );

    let error = format_invalid_glob_pattern_error(&long_pattern, &long_detail);
    let pattern_display = sanitize_glob_error_text(&long_pattern, GLOB_ERROR_PATTERN_MAX_CHARS);
    let detail_display = sanitize_glob_error_text(&long_detail, GLOB_ERROR_DETAIL_MAX_CHARS);

    assert!(!error.contains('\n'));
    assert!(!error.contains('\r'));
    assert!(!error.contains('\u{202e}'));
    assert!(!error.contains('\u{2066}'));
    assert!(pattern_display.contains("\\n"));
    assert!(pattern_display.contains("\\u{202e}"));
    assert!(detail_display.contains("\\r"));
    assert!(detail_display.contains("\\u{2066}"));
    assert!(pattern_display.ends_with("..."));
    assert!(detail_display.ends_with("..."));
    assert!(pattern_display.chars().count() <= GLOB_ERROR_PATTERN_MAX_CHARS);
    assert!(detail_display.chars().count() <= GLOB_ERROR_DETAIL_MAX_CHARS);
    assert!(error.contains(&pattern_display));
    assert!(error.contains(&detail_display));
}

#[test]
fn project_search_truncates_to_max_results() {
    let root = search_fixture(
        "max-results",
        &[(
            "src/lib.rs",
            "needle one\nneedle two\nneedle three\nneedle four\n",
        )],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 2,
            ..SearchOptions::default()
        },
    );

    assert!(result.truncated);
    assert_eq!(result.matches.len(), 2);
    assert_eq!(result.matches[0].line, 1);
    assert_eq!(result.matches[1].line, 2);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_zero_max_results_reports_hidden_truncation_match() {
    let root = search_fixture("zero-max-results", &[("src/lib.rs", "needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 0,
            ..SearchOptions::default()
        },
    );

    assert!(result.truncated);
    assert!(result.matches.is_empty());
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.matched_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_result_budget_counts_hidden_match_without_collecting_it() {
    let root = search_fixture(
        "budget-hidden-match",
        &[
            (
                "src/000_first.rs",
                "needle one\nneedle two\nneedle hidden\n",
            ),
            ("src/001_second.rs", "needle later\n"),
        ],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 2,
            ..SearchOptions::default()
        },
    );

    let lines = result
        .matches
        .iter()
        .map(|matched| matched.line)
        .collect::<Vec<_>>();
    assert!(result.truncated);
    assert_eq!(lines, vec![1, 2]);
    assert!(
        result
            .matches
            .iter()
            .all(|matched| matched.path.ends_with("src/000_first.rs"))
    );
    assert_eq!(result.stats.searched_files, 1);
    assert_eq!(result.stats.matched_files, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_bounds_collected_matches_across_file_chunks() {
    let root = search_fixture("bounded-global-results", &[]);
    let total_files = SEARCH_FILE_CHUNK_SIZE + 12;
    for index in 0..total_files {
        let path = root.join(format!("src/{index:04}.rs"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "needle\n").unwrap();
    }
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 3,
            ..SearchOptions::default()
        },
    );

    assert!(result.truncated);
    assert_eq!(result.stats.searched_files, 4);
    assert_eq!(result.stats.matched_files, 4);
    assert_eq!(result.stats.skipped_files(), 0);
    assert_eq!(result.matches.len(), 3);
    let paths = result
        .matches
        .iter()
        .map(|matched| matched.path.strip_prefix(&root).unwrap().to_path_buf())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            PathBuf::from("src/0000.rs"),
            PathBuf::from("src/0001.rs"),
            PathBuf::from("src/0002.rs")
        ]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_global_budget_skips_remaining_files_after_limit() {
    let root = search_fixture("global-budget-skips-remaining", &[]);
    let total_files = SEARCH_FILE_CHUNK_SIZE + 12;
    for index in 0..total_files {
        let path = root.join(format!("src/{index:04}.rs"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text = if index < 4 {
            "needle\n"
        } else {
            "needle sentinel\n"
        };
        fs::write(path, text).unwrap();
    }
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            max_results: 3,
            ..SearchOptions::default()
        },
    );

    assert!(result.truncated);
    assert_eq!(result.matches.len(), 3);
    assert_eq!(result.stats.searched_files, 4);
    assert_eq!(result.stats.matched_files, 4);
    assert_eq!(result.stats.skipped_files(), 0);
    assert!(
        result
            .matches
            .iter()
            .all(|matched| !matched.preview.contains("sentinel"))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_previews_are_bounded_around_match() {
    let long_prefix = "a".repeat(400);
    let long_suffix = "b".repeat(400);
    let text = format!("{long_prefix}needle{long_suffix}\n");
    let root = search_fixture("bounded-preview", &[("src/minified.rs", &text)]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
    );

    let preview = &result.matches[0].preview;
    assert!(preview.contains("needle"));
    assert!(preview.starts_with("..."));
    assert!(preview.ends_with("..."));
    assert!(preview.chars().count() <= MAX_SEARCH_PREVIEW_CHARS + 6);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_regex_matches_like_literal_for_plain_patterns() {
    let root = search_fixture("regex-literal", &[("src/lib.rs", "needle x needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let literal = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            ..SearchOptions::default()
        },
    );
    let regex = search_project(
        &index,
        &SearchOptions {
            query: "needle".to_owned(),
            regex: true,
            ..SearchOptions::default()
        },
    );

    assert_eq!(literal.matches, regex.matches);
    assert_eq!(regex.matches.len(), 2);
    assert_eq!(regex.matches[0].line, 1);
    assert_eq!(regex.matches[0].column, 1);
    assert_eq!(regex.matches[1].column, 10);
    assert!(regex.error.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_regex_supports_pattern_syntax() {
    let root = search_fixture("regex-pattern", &[("src/lib.rs", "needle x nada\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "n(ee|a)dle".to_owned(),
            regex: true,
            ..SearchOptions::default()
        },
    );

    assert!(result.error.is_none());
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].line, 1);
    assert_eq!(result.matches[0].column, 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_regex_is_case_insensitive_by_default() {
    let root = search_fixture("regex-case", &[("src/lib.rs", "Alpha\nbeta\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let insensitive = search_project(
        &index,
        &SearchOptions {
            query: "alpha".to_owned(),
            regex: true,
            ..SearchOptions::default()
        },
    );
    let sensitive = search_project(
        &index,
        &SearchOptions {
            query: "alpha".to_owned(),
            regex: true,
            case_sensitive: true,
            ..SearchOptions::default()
        },
    );

    assert!(insensitive.error.is_none());
    assert_eq!(insensitive.matches.len(), 1);
    assert_eq!(insensitive.matches[0].line, 1);
    assert_eq!(insensitive.matches[0].column, 1);
    assert!(sensitive.matches.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_reports_invalid_regex_query() {
    let root = search_fixture("invalid-regex", &[("src/lib.rs", "needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "n(eedle".to_owned(),
            regex: true,
            ..SearchOptions::default()
        },
    );

    let error = result.error.expect("invalid regex should fail");
    assert!(result.matches.is_empty());
    assert!(error.contains("Invalid regular expression"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_regex_rejects_multiline_constructs() {
    let root = search_fixture("multiline-regex", &[("src/lib.rs", "needle\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    for query in ["needle\\n", "(?s)needle", "n[^\\n]eedle"] {
        let result = search_project(
            &index,
            &SearchOptions {
                query: query.to_owned(),
                regex: true,
                ..SearchOptions::default()
            },
        );

        let error = result.error.expect("multiline regex should fail");
        assert!(result.matches.is_empty());
        assert_eq!(error, REGEX_MULTILINE_QUERY_ERROR);
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_regex_whole_word_respects_boundaries() {
    let root = search_fixture(
        "regex-whole-word",
        &[("src/lib.rs", "alpha alphabet alpha_1\n")],
    );
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "alpha".to_owned(),
            regex: true,
            whole_word: true,
            ..SearchOptions::default()
        },
    );

    assert!(result.error.is_none());
    let columns = result
        .matches
        .iter()
        .map(|matched| matched.column)
        .collect::<Vec<_>>();
    assert_eq!(columns, vec![1]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_regex_reports_multiple_matches_per_line() {
    let root = search_fixture("regex-multi-line", &[("src/lib.rs", "ab xaab\n")]);
    let index = ProjectIndex::rebuild(&root, 40_000);

    let result = search_project(
        &index,
        &SearchOptions {
            query: "a+b".to_owned(),
            regex: true,
            ..SearchOptions::default()
        },
    );

    assert!(result.error.is_none());
    let columns = result
        .matches
        .iter()
        .map(|matched| matched.column)
        .collect::<Vec<_>>();
    assert_eq!(columns, vec![1, 5]);
    fs::remove_dir_all(root).unwrap();
}

fn search_fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "kuroya-search-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for (relative, text) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }
    root
}

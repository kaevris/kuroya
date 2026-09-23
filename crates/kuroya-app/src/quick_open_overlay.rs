use crate::{
    KuroyaApp,
    picker_ui::{PICKER_ROW_HEIGHT, picker_scroll_area, picker_selectable_row, picker_window_size},
    quick_open::{
        MAX_QUICK_OPEN_QUERY_MEMORY, QUICK_OPEN_RESULT_LIMIT, QUICK_OPEN_REUSE_LIMIT,
        QuickOpenBackgroundRank, QuickOpenCompletedRanking, QuickOpenMatchQuery, QuickOpenQuery,
        QuickOpenRankKey, QuickOpenResult, QuickOpenResultsCache, next_quick_open_rank_request_id,
        quick_open_latest_navigation_locations_from_history,
        quick_open_ranked_results_from_open_paths,
        quick_open_result_label_with_navigation_line_column,
        quick_open_target_with_navigation_line_column, record_quick_open_query_memory,
        sanitize_quick_open_query_input,
    },
    ui_event_channel::send_ui_event,
    ui_events::UiEvent,
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
};
use eframe::egui::{self, Context, Key, TextEdit};
use fuzzy_matcher::skim::SkimMatcherV2;
use kuroya_core::Command;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const QUICK_OPEN_RANK_DEBOUNCE: Duration = Duration::from_millis(150);

fn ranking_due(last_change: Instant, now: Instant) -> bool {
    now.saturating_duration_since(last_change) >= QUICK_OPEN_RANK_DEBOUNCE
}

fn quick_open_rank_debounce_delay(last_change: Instant, now: Instant) -> Duration {
    QUICK_OPEN_RANK_DEBOUNCE.saturating_sub(now.saturating_duration_since(last_change))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CandidateSource<'a> {
    Full,

    Reuse(&'a [PathBuf]),
}

fn candidates_for_query<'a>(
    previous: Option<(&'a str, &'a [PathBuf], bool)>,
    new_query: &str,
    same_generation: bool,
) -> CandidateSource<'a> {
    let Some((previous_query, previous_paths, previous_truncated)) = previous else {
        return CandidateSource::Full;
    };
    if previous_truncated {
        return CandidateSource::Full;
    }
    if previous_query.is_empty() {
        return CandidateSource::Full;
    }
    if !same_generation || !new_query.starts_with(previous_query) {
        return CandidateSource::Full;
    }
    CandidateSource::Reuse(previous_paths)
}

#[derive(Debug)]
enum QuickOpenRankCandidates {
    Index(kuroya_core::ProjectIndex),
    Paths(Vec<PathBuf>),
}

impl QuickOpenRankCandidates {
    fn paths(&self) -> Vec<&Path> {
        match self {
            Self::Index(index) => index.files().iter().map(PathBuf::as_path).collect(),
            Self::Paths(paths) => paths.iter().map(Path::new).collect(),
        }
    }
}

impl KuroyaApp {
    pub(crate) fn render_quick_open(&mut self, ctx: &Context) {
        self.ensure_workspace_index_started();
        let mut open_target = None;

        egui::Window::new("Quick Open")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 96.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
            .fixed_size(picker_window_size(ctx))
            .show(ctx, |ui| {
                let mut scroll_to_selection = false;
                let response = ui.add(
                    TextEdit::singleline(&mut self.quick_open_query)
                        .hint_text("Type a filename or file:line:column")
                        .desired_width(f32::INFINITY),
                );
                response.request_focus();
                if response.changed() {
                    let sanitized_query = sanitize_quick_open_query_input(&self.quick_open_query);
                    if sanitized_query != self.quick_open_query {
                        self.quick_open_query = sanitized_query;
                    }
                    self.quick_open_selected = 0;
                    scroll_to_selection = true;
                    if let Some(cache) = self.quick_open_results_cache.as_mut() {
                        cache.last_query_changed_at = Some(Instant::now());
                    }
                }

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    self.quick_open = false;
                }

                let row_count = self.refresh_quick_open_results_cache(ctx);
                let indexing_pending = quick_open_indexing_pending(
                    self.workspace_placeholder,
                    self.index.files().len(),
                    self.workspace_index_in_flight_request_id,
                );
                if indexing_pending {
                    ctx.request_repaint_after(Duration::from_millis(32));
                }
                let mut selected = self.quick_open_selected;
                let mut close_quick_open = false;
                if let Some(cache) = self.quick_open_results_cache.as_ref() {
                    clamp_selection(&mut selected, row_count);
                    let viewport_height = ui.available_height();

                    scroll_to_selection |= ui.input(|input| {
                        handle_list_navigation_keys(
                            input,
                            &mut selected,
                            row_count,
                            selection_page_step(PICKER_ROW_HEIGHT, viewport_height),
                        )
                    });
                    if ui.input(|input| input.key_pressed(Key::Enter)) {
                        open_target = quick_open_open_target_at(
                            cache,
                            selected,
                            row_count,
                            &self.quick_open_query,
                        );
                    }

                    if row_count == 0 {
                        let message = quick_open_empty_state_message(indexing_pending);
                        picker_scroll_area().show(ui, |ui| {
                            ui.add_space(20.0);
                            ui.centered_and_justified(|ui| {
                                ui.label(message);
                            });
                        });
                    } else {
                        let mut scroll_area = picker_scroll_area();
                        if scroll_to_selection {
                            scroll_area =
                                scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                                    selected,
                                    row_count,
                                    PICKER_ROW_HEIGHT,
                                    viewport_height,
                                ));
                        }
                        scroll_area.show_rows(ui, PICKER_ROW_HEIGHT, row_count, |ui, rows| {
                            for row in quick_open_prepare_visible_rows(cache, rows, row_count) {
                                let is_selected = row.index == selected;
                                if picker_selectable_row(ui, is_selected, row.label).clicked() {
                                    close_quick_open = true;
                                    open_target = quick_open_open_target_at(
                                        cache,
                                        row.index,
                                        row_count,
                                        &self.quick_open_query,
                                    );
                                }
                            }
                        });
                    }
                } else {
                    selected = 0;
                    let message = quick_open_empty_state_message(indexing_pending);
                    picker_scroll_area().show(ui, |ui| {
                        ui.add_space(20.0);
                        ui.centered_and_justified(|ui| {
                            ui.label(message);
                        });
                    });
                }
                self.quick_open_selected = selected;
                if close_quick_open {
                    self.quick_open = false;
                }
            });

        if let Some(target) = open_target {
            self.quick_open = false;
            record_quick_open_query_memory(
                &mut self.quick_open_query_memory,
                &self.workspace.root,
                &target.query_pattern,
                &target.path,
                MAX_QUICK_OPEN_QUERY_MEMORY,
            );
            if let Some((line, column)) = target.line_column {
                self.open_file_at_known_openable(target.path, line, column);
            } else {
                self.command_bus.push(Command::OpenFile(target.path));
            }
        }
    }

    fn refresh_quick_open_results_cache(&mut self, ctx: &Context) -> usize {
        let index_generation = self.project_index_generation;
        let current_navigation_location = self.current_navigation_location();
        if let Some(cache) = self.quick_open_results_cache.as_mut()
            && cache.matches(
                &self.quick_open_query,
                index_generation,
                &self.quick_open_recent_files,
                self.buffers
                    .iter()
                    .filter_map(|buffer| buffer.path().map(|path| path.as_path())),
                &self.quick_open_query_memory,
                &self.navigation_back,
                &self.navigation_forward,
                current_navigation_location.as_ref(),
            )
        {
            return quick_open_refresh_stale_display_metadata(cache);
        }

        let mut open_file_paths = Vec::with_capacity(self.buffers.len());
        open_file_paths.extend(
            self.buffers
                .iter()
                .filter_map(|buffer| buffer.path().cloned()),
        );
        let navigation_locations = quick_open_latest_navigation_locations_from_history(
            &self.navigation_back,
            &self.navigation_forward,
            current_navigation_location.as_ref(),
        );
        let parsed_query = crate::quick_open::parse_quick_open_query(&self.quick_open_query);
        if let Some(cache) = self.quick_open_results_cache.as_mut()
            && quick_open_cache_ranking_inputs_match_ignoring_query_target(
                cache,
                &parsed_query,
                index_generation,
                &self.quick_open_recent_files,
                open_file_paths.iter().map(|path| path.as_path()),
                &self.quick_open_query_memory,
                &navigation_locations,
            )
        {
            return quick_open_refresh_cached_query_metadata(
                cache,
                &self.quick_open_query,
                parsed_query,
                &self.navigation_back,
                &self.navigation_forward,
                current_navigation_location,
                &navigation_locations,
            );
        }
        let previous_ranking = self
            .quick_open_results_cache
            .as_ref()
            .and_then(|cache| cache.completed_ranking.clone());
        let same_generation = previous_ranking
            .as_ref()
            .is_some_and(|ranking| ranking.generation == index_generation);
        let candidate_source = candidates_for_query(
            previous_ranking.as_ref().map(|ranking| {
                (
                    ranking.query.as_str(),
                    ranking.matched_paths.as_slice(),
                    ranking.matched_paths_truncated,
                )
            }),
            &parsed_query.pattern,
            same_generation,
        );
        self.refresh_quick_open_results_via_background_rank(
            ctx,
            index_generation,
            current_navigation_location,
            open_file_paths,
            navigation_locations,
            parsed_query,
            candidate_source,
        )
    }

    fn refresh_quick_open_results_via_background_rank(
        &mut self,
        ctx: &Context,
        index_generation: u64,
        current_navigation_location: Option<crate::history::NavigationLocation>,
        open_file_paths: Vec<std::path::PathBuf>,
        navigation_locations: Vec<crate::history::NavigationLocation>,
        parsed_query: QuickOpenQuery,
        candidate_source: CandidateSource<'_>,
    ) -> usize {
        let key = QuickOpenRankKey {
            query_input: self.quick_open_query.clone(),
            index_generation,
            recent_files: self.quick_open_recent_files.clone(),
            open_files: open_file_paths,
            query_memory: self.quick_open_query_memory.clone(),
            navigation_back: self.navigation_back.clone(),
            navigation_forward: self.navigation_forward.clone(),
            current_navigation_location,
        };
        let outstanding_is_current = self
            .quick_open_results_cache
            .as_ref()
            .and_then(|cache| cache.background_rank.as_ref())
            .is_some_and(|rank| *rank.key == key);
        if outstanding_is_current && let Some(cache) = self.quick_open_results_cache.as_mut() {
            return quick_open_refresh_stale_display_metadata(cache);
        }

        let now = Instant::now();
        if let Some(last_change) = self
            .quick_open_results_cache
            .as_ref()
            .and_then(|cache| cache.last_query_changed_at)
            && !ranking_due(last_change, now)
        {
            ctx.request_repaint_after(quick_open_rank_debounce_delay(last_change, now));
            if let Some(cache) = self.quick_open_results_cache.as_mut() {
                return quick_open_refresh_stale_display_metadata(cache);
            }
        }

        let request_id = next_quick_open_rank_request_id();
        let spawn_key = Arc::new(key.clone());
        let match_query = QuickOpenMatchQuery::from_sanitized_query(parsed_query.pattern.clone());
        let workspace_root = self.workspace.root.clone();
        let spawn_candidates = match candidate_source {
            CandidateSource::Full => QuickOpenRankCandidates::Index(self.index.clone()),
            CandidateSource::Reuse(paths) => QuickOpenRankCandidates::Paths(paths.to_vec()),
        };
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            let matcher = SkimMatcherV2::default();
            let candidate_paths = spawn_candidates.paths();
            let results = quick_open_ranked_results_from_open_paths(
                &matcher,
                &workspace_root,
                candidate_paths.iter().copied(),
                &spawn_key.recent_files,
                &spawn_key.open_files,
                &spawn_key.query_memory,
                &navigation_locations,
                &match_query,
                QUICK_OPEN_REUSE_LIMIT,
            );
            let _ = send_ui_event(
                &tx,
                UiEvent::QuickOpenRanked {
                    request_id,
                    key: spawn_key,
                    results,
                },
            );
        });

        let record = QuickOpenBackgroundRank {
            request_id,
            key: Arc::new(key),
        };
        if let Some(cache) = self.quick_open_results_cache.as_mut() {
            cache.background_rank = Some(record);
            return quick_open_refresh_stale_display_metadata(cache);
        }
        self.quick_open_results_cache = Some(QuickOpenResultsCache {
            query_input: record.key.query_input.clone(),
            index_generation: record.key.index_generation,
            recent_files: record.key.recent_files.clone(),
            open_files: record.key.open_files.clone(),
            query_memory: record.key.query_memory.clone(),
            navigation_back: record.key.navigation_back.clone(),
            navigation_forward: record.key.navigation_forward.clone(),
            current_navigation_location: record.key.current_navigation_location.clone(),
            parsed_query,
            result_labels: Vec::new(),
            results: Vec::new(),
            background_rank: Some(record),
            last_query_changed_at: None,
            completed_ranking: None,
        });
        0
    }

    pub(crate) fn apply_quick_open_ranked_results(
        &mut self,
        request_id: u64,
        key: &QuickOpenRankKey,
        results: Vec<QuickOpenResult>,
    ) -> bool {
        let Some(cache) = self.quick_open_results_cache.as_mut() else {
            return false;
        };
        let Some(rank) = cache.background_rank.as_ref() else {
            return false;
        };
        if !rank.matches(request_id, key) {
            return false;
        }

        let parsed_query = crate::quick_open::parse_quick_open_query(&key.query_input);
        cache.query_input.clone_from(&key.query_input);
        cache.index_generation = key.index_generation;
        cache.recent_files.clone_from(&key.recent_files);
        cache.open_files.clone_from(&key.open_files);
        cache.query_memory.clone_from(&key.query_memory);
        cache.navigation_back.clone_from(&key.navigation_back);
        cache.navigation_forward.clone_from(&key.navigation_forward);
        cache
            .current_navigation_location
            .clone_from(&key.current_navigation_location);
        cache.parsed_query = parsed_query;

        cache.completed_ranking = Some(QuickOpenCompletedRanking {
            query: cache.parsed_query.pattern.clone(),
            generation: key.index_generation,
            matched_paths_truncated: results.len() >= QUICK_OPEN_REUSE_LIMIT,
            matched_paths: results.iter().map(|result| result.path.clone()).collect(),
        });
        cache.results = results;
        cache.results.truncate(QUICK_OPEN_RESULT_LIMIT);
        cache.result_labels = quick_open_result_labels(&cache.results, &cache.parsed_query);
        cache.background_rank = None;
        true
    }
}

fn quick_open_indexing_pending(
    workspace_placeholder: bool,
    index_file_count: usize,
    workspace_index_in_flight_request_id: Option<u64>,
) -> bool {
    !workspace_placeholder
        && index_file_count == 0
        && workspace_index_in_flight_request_id.is_some()
}

fn quick_open_empty_state_message(indexing_pending: bool) -> &'static str {
    if indexing_pending {
        "Indexing workspace..."
    } else {
        "No matching files"
    }
}

#[cfg(test)]
fn quick_open_window_size_for_available(available_width: f32, available_height: f32) -> [f32; 2] {
    crate::picker_ui::picker_window_size_for_available(available_width, available_height)
}

fn quick_open_refresh_stale_display_metadata(cache: &mut QuickOpenResultsCache) -> usize {
    let visible_count = quick_open_visible_result_count(cache);
    if visible_count != cache.results.len() || cache.result_labels.len() != cache.results.len() {
        cache.result_labels = quick_open_result_labels(&cache.results, &cache.parsed_query);
    }
    cache.results.len()
}

fn quick_open_visible_result_count(cache: &QuickOpenResultsCache) -> usize {
    cache
        .results
        .iter()
        .zip(&cache.result_labels)
        .take_while(|(result, label)| {
            quick_open_result_label_matches_result(result, label, &cache.parsed_query)
        })
        .count()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QuickOpenPreparedVisibleRow<'a> {
    index: usize,
    label: &'a str,
}

fn quick_open_prepare_visible_rows<'a>(
    cache: &'a QuickOpenResultsCache,
    rows: Range<usize>,
    visible_count: usize,
) -> impl Iterator<Item = QuickOpenPreparedVisibleRow<'a>> + 'a {
    let end = rows
        .end
        .min(visible_count)
        .min(cache.results.len())
        .min(cache.result_labels.len());
    let start = rows.start.min(end);
    (start..end).map(move |index| QuickOpenPreparedVisibleRow {
        index,
        label: cache.result_labels[index].as_str(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuickOpenOpenTarget {
    path: std::path::PathBuf,
    line_column: Option<(usize, usize)>,
    query_pattern: String,
}

fn quick_open_open_target_at(
    cache: &QuickOpenResultsCache,
    index: usize,
    visible_count: usize,
    expected_query_input: &str,
) -> Option<QuickOpenOpenTarget> {
    if cache.query_input != expected_query_input {
        return None;
    }

    if index >= visible_count {
        return None;
    }

    cache.result_labels.get(index)?;
    cache.results.get(index).map(|result| {
        let (path, line_column) = quick_open_target_with_navigation_line_column(
            result.path.clone(),
            &cache.parsed_query,
            result.navigation_line_column,
        );
        QuickOpenOpenTarget {
            path,
            line_column,
            query_pattern: cache.parsed_query.pattern.clone(),
        }
    })
}

fn quick_open_cache_ranking_inputs_match_ignoring_query_target<'a>(
    cache: &QuickOpenResultsCache,
    parsed_query: &QuickOpenQuery,
    index_generation: u64,
    recent_files: &std::collections::VecDeque<std::path::PathBuf>,
    open_files: impl IntoIterator<Item = &'a std::path::Path>,
    query_memory: &std::collections::VecDeque<crate::quick_open::QuickOpenQueryMemoryEntry>,
    navigation_locations: &[crate::history::NavigationLocation],
) -> bool {
    cache.parsed_query.pattern == parsed_query.pattern
        && cache.ranking_inputs_match(
            &cache.query_input,
            index_generation,
            recent_files,
            open_files,
            query_memory,
            navigation_locations,
        )
}

fn quick_open_refresh_cached_query_metadata(
    cache: &mut QuickOpenResultsCache,
    query_input: &str,
    parsed_query: QuickOpenQuery,
    navigation_back: &std::collections::VecDeque<crate::history::NavigationLocation>,
    navigation_forward: &std::collections::VecDeque<crate::history::NavigationLocation>,
    current_navigation_location: Option<crate::history::NavigationLocation>,
    navigation_locations: &[crate::history::NavigationLocation],
) -> usize {
    let navigation_metadata_matches = cache.navigation_back.iter().eq(navigation_back.iter())
        && cache
            .navigation_forward
            .iter()
            .eq(navigation_forward.iter())
        && cache.current_navigation_location.as_ref() == current_navigation_location.as_ref();

    cache.query_input.clear();
    cache.query_input.push_str(query_input);
    if navigation_metadata_matches {
        if !quick_open_result_label_query_metadata_matches(&cache.parsed_query, &parsed_query)
            || !quick_open_result_labels_match_results(
                &cache.results,
                &cache.result_labels,
                &parsed_query,
            )
        {
            cache.result_labels = quick_open_result_labels(&cache.results, &parsed_query);
        }
        cache.parsed_query = parsed_query;
    } else {
        cache.parsed_query = parsed_query;
        cache.refresh_navigation_metadata(
            navigation_back,
            navigation_forward,
            current_navigation_location,
            navigation_locations,
        );
    }
    cache.results.len()
}

fn quick_open_result_label_query_metadata_matches(
    previous_query: &QuickOpenQuery,
    query: &QuickOpenQuery,
) -> bool {
    previous_query.line == query.line
        && (query.line.is_none() || previous_query.column == query.column)
}

fn quick_open_result_labels_match_results(
    results: &[QuickOpenResult],
    labels: &[String],
    parsed_query: &QuickOpenQuery,
) -> bool {
    labels.len() == results.len()
        && results.iter().zip(labels).all(|(result, label)| {
            quick_open_result_label_matches_result(result, label, parsed_query)
        })
}

fn quick_open_result_label_matches_result(
    result: &QuickOpenResult,
    label: &str,
    parsed_query: &QuickOpenQuery,
) -> bool {
    label
        == quick_open_result_label_with_navigation_line_column(
            &result.rel,
            parsed_query,
            result.navigation_line_column,
        )
}

fn quick_open_result_labels(
    results: &[QuickOpenResult],
    parsed_query: &QuickOpenQuery,
) -> Vec<String> {
    results
        .iter()
        .map(|result| {
            quick_open_result_label_with_navigation_line_column(
                &result.rel,
                parsed_query,
                result.navigation_line_column,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CandidateSource, candidates_for_query,
        quick_open_cache_ranking_inputs_match_ignoring_query_target,
        quick_open_empty_state_message, quick_open_indexing_pending, quick_open_open_target_at,
        quick_open_prepare_visible_rows, quick_open_rank_debounce_delay,
        quick_open_refresh_cached_query_metadata, quick_open_refresh_stale_display_metadata,
        quick_open_visible_result_count, quick_open_window_size_for_available, ranking_due,
    };
    use crate::quick_open::{
        QUICK_OPEN_RESULT_LABEL_MAX_CHARS, QuickOpenCompletedRanking, QuickOpenQuery,
        QuickOpenResult, QuickOpenResultsCache,
    };
    use std::{collections::VecDeque, path::PathBuf};

    fn quick_open_result(
        rel: &str,
        navigation_line_column: Option<(usize, usize)>,
    ) -> QuickOpenResult {
        QuickOpenResult {
            rank_score: 120,
            fuzzy_score: 40,
            path: PathBuf::from("workspace").join(rel),
            rel: rel.to_owned(),
            navigation_line_column,
        }
    }

    fn quick_open_results_cache(
        parsed_query: QuickOpenQuery,
        results: Vec<QuickOpenResult>,
        result_labels: Vec<String>,
    ) -> QuickOpenResultsCache {
        QuickOpenResultsCache {
            query_input: parsed_query.pattern.clone(),
            index_generation: 7,
            recent_files: VecDeque::new(),
            open_files: Vec::new(),
            query_memory: VecDeque::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            parsed_query,
            result_labels,
            results,
            background_rank: None,
            last_query_changed_at: None,
            completed_ranking: None,
        }
    }

    #[test]
    fn quick_open_window_size_uses_preferred_size_when_roomy() {
        assert_eq!(
            quick_open_window_size_for_available(1200.0, 900.0),
            [620.0, 420.0]
        );
    }

    #[test]
    fn quick_open_window_size_shrinks_to_available_viewport() {
        assert_eq!(
            quick_open_window_size_for_available(360.0, 260.0),
            [328.0, 164.0]
        );
    }

    #[test]
    fn quick_open_window_size_keeps_soft_minimum_when_possible() {
        assert_eq!(
            quick_open_window_size_for_available(300.0, 300.0),
            [268.0, 204.0]
        );
        assert_eq!(
            quick_open_window_size_for_available(f32::NAN, f32::INFINITY),
            [620.0, 420.0]
        );
    }

    #[test]
    fn quick_open_empty_state_reports_indexing_only_while_first_index_is_pending() {
        assert!(quick_open_indexing_pending(false, 0, Some(7)));
        assert!(!quick_open_indexing_pending(true, 0, Some(7)));
        assert!(!quick_open_indexing_pending(false, 2, Some(7)));
        assert!(!quick_open_indexing_pending(false, 0, None));
        assert_eq!(
            quick_open_empty_state_message(true),
            "Indexing workspace..."
        );
        assert_eq!(quick_open_empty_state_message(false), "No matching files");
    }

    #[test]
    fn quick_open_visible_result_count_requires_result_and_label() {
        let query = QuickOpenQuery {
            pattern: "src".to_owned(),
            line: None,
            column: 1,
        };
        let cache = quick_open_results_cache(
            query,
            vec![
                quick_open_result("src/main.rs", Some((4, 2))),
                quick_open_result("src/lib.rs", Some((8, 1))),
            ],
            vec!["src/main.rs:4:2".to_owned()],
        );

        assert_eq!(quick_open_visible_result_count(&cache), 1);
    }

    #[test]
    fn quick_open_open_target_ignores_rows_without_labels() {
        let query = QuickOpenQuery {
            pattern: "src".to_owned(),
            line: None,
            column: 1,
        };
        let cache = quick_open_results_cache(
            query,
            vec![
                quick_open_result("src/main.rs", Some((4, 2))),
                quick_open_result("src/lib.rs", Some((8, 1))),
            ],
            vec!["src/main.rs:4:2".to_owned()],
        );
        let visible_count = quick_open_visible_result_count(&cache);

        assert!(quick_open_open_target_at(&cache, 1, visible_count, "src").is_none());
    }

    #[test]
    fn quick_open_open_target_preserves_path_query_and_target_selection() {
        let query = QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: Some(9),
            column: 3,
        };
        let result = quick_open_result("src/main.rs", Some((4, 2)));
        let expected_path = result.path.clone();
        let cache =
            quick_open_results_cache(query, vec![result], vec!["src/main.rs:9:3".to_owned()]);

        let visible_count = quick_open_visible_result_count(&cache);
        let target = quick_open_open_target_at(&cache, 0, visible_count, "src/main.rs")
            .expect("row should open");

        assert_eq!(target.path, expected_path);
        assert_eq!(target.line_column, Some((9, 3)));
        assert_eq!(target.query_pattern, "src/main.rs");
    }

    #[test]
    fn quick_open_open_target_rejects_stale_query_cache() {
        let query = QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: None,
            column: 1,
        };
        let cache = quick_open_results_cache(
            query,
            vec![quick_open_result("src/main.rs", Some((4, 2)))],
            vec!["src/main.rs:4:2".to_owned()],
        );

        let visible_count = quick_open_visible_result_count(&cache);

        assert!(quick_open_open_target_at(&cache, 0, visible_count, "src/lib.rs").is_none());
    }

    #[test]
    fn quick_open_visible_rows_and_open_target_reject_stale_result_label() {
        let query = QuickOpenQuery {
            pattern: "src".to_owned(),
            line: None,
            column: 1,
        };
        let cache = quick_open_results_cache(
            query,
            vec![quick_open_result("src/main.rs", Some((4, 2)))],
            vec!["src/lib.rs:8:1".to_owned()],
        );

        let visible_count = quick_open_visible_result_count(&cache);

        assert_eq!(visible_count, 0);
        assert_eq!(
            quick_open_prepare_visible_rows(&cache, 0..1, visible_count).count(),
            0
        );
        assert!(quick_open_open_target_at(&cache, 0, visible_count, "src").is_none());
    }

    #[test]
    fn quick_open_prepare_visible_rows_clamps_to_prepared_labels() {
        let query = QuickOpenQuery {
            pattern: "src".to_owned(),
            line: None,
            column: 1,
        };
        let cache = quick_open_results_cache(
            query,
            vec![
                quick_open_result("src/main.rs", Some((4, 2))),
                quick_open_result("src/lib.rs", Some((8, 1))),
            ],
            vec!["src/main.rs:4:2".to_owned()],
        );

        let visible_count = quick_open_visible_result_count(&cache);
        let rows = quick_open_prepare_visible_rows(&cache, 0..3, visible_count).collect::<Vec<_>>();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].index, 0);
        assert_eq!(rows[0].label, "src/main.rs:4:2");
    }

    #[test]
    fn quick_open_reuses_cached_ranking_when_only_explicit_target_changes() {
        let previous_query = QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: None,
            column: 1,
        };
        let mut cache = quick_open_results_cache(
            previous_query,
            vec![quick_open_result("src/main.rs", Some((4, 2)))],
            vec!["src/main.rs:4:2".to_owned()],
        );
        let cached_result_rel = cache.results[0].rel.as_ptr();
        let parsed_query = QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: Some(9),
            column: 3,
        };
        let recent_files = VecDeque::new();
        let open_files: Vec<PathBuf> = Vec::new();
        let query_memory = VecDeque::new();

        assert!(quick_open_cache_ranking_inputs_match_ignoring_query_target(
            &cache,
            &parsed_query,
            7,
            &recent_files,
            open_files.iter().map(PathBuf::as_path),
            &query_memory,
            &[],
        ));

        let navigation_back = VecDeque::new();
        let navigation_forward = VecDeque::new();
        quick_open_refresh_cached_query_metadata(
            &mut cache,
            "src/main.rs:9:3",
            parsed_query,
            &navigation_back,
            &navigation_forward,
            None,
            &[],
        );

        assert_eq!(cache.query_input, "src/main.rs:9:3");
        assert_eq!(cache.results[0].rel.as_ptr(), cached_result_rel);
        assert_eq!(cache.result_labels, vec!["src/main.rs:9:3"]);
    }

    #[test]
    fn quick_open_keeps_cached_labels_when_query_target_metadata_matches() {
        let query = QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: Some(9),
            column: 3,
        };
        let cached_label = "src/main.rs:9:3".to_owned();
        let cached_label_text = cached_label.as_ptr();
        let mut cache = quick_open_results_cache(
            query.clone(),
            vec![quick_open_result("src/main.rs", Some((4, 2)))],
            vec![cached_label],
        );
        let navigation_back = VecDeque::new();
        let navigation_forward = VecDeque::new();

        quick_open_refresh_cached_query_metadata(
            &mut cache,
            "src/main.rs:09:03",
            query,
            &navigation_back,
            &navigation_forward,
            None,
            &[],
        );

        assert_eq!(cache.query_input, "src/main.rs:09:03");
        assert_eq!(cache.result_labels[0].as_ptr(), cached_label_text);
        assert_eq!(cache.result_labels, vec!["src/main.rs:9:3"]);
    }

    #[test]
    fn quick_open_refresh_rebuilds_stale_cached_labels_when_query_metadata_matches() {
        let query = QuickOpenQuery {
            pattern: "src/main.rs".to_owned(),
            line: None,
            column: 1,
        };
        let mut cache = quick_open_results_cache(
            query.clone(),
            vec![quick_open_result("src/main.rs", Some((4, 2)))],
            vec![format!(
                "src/main.rs:999:999\n{}",
                "x".repeat(QUICK_OPEN_RESULT_LABEL_MAX_CHARS)
            )],
        );
        let navigation_back = VecDeque::new();
        let navigation_forward = VecDeque::new();

        quick_open_refresh_cached_query_metadata(
            &mut cache,
            "src/main.rs",
            query,
            &navigation_back,
            &navigation_forward,
            None,
            &[],
        );

        assert_eq!(cache.result_labels, vec!["src/main.rs:4:2"]);
        assert!(cache.result_labels[0].chars().count() <= QUICK_OPEN_RESULT_LABEL_MAX_CHARS);
    }

    #[test]
    fn quick_open_exact_cache_hit_repairs_stale_display_metadata() {
        let query = QuickOpenQuery {
            pattern: "src".to_owned(),
            line: None,
            column: 1,
        };
        let mut cache = quick_open_results_cache(
            query,
            vec![
                quick_open_result("src/main.rs", Some((4, 2))),
                quick_open_result("src/lib.rs", Some((8, 1))),
            ],
            vec!["src/lib.rs:8:1".to_owned()],
        );

        assert_eq!(quick_open_visible_result_count(&cache), 0);

        let visible_count = quick_open_refresh_stale_display_metadata(&mut cache);

        assert_eq!(
            cache.result_labels,
            vec!["src/main.rs:4:2", "src/lib.rs:8:1"]
        );
        assert_eq!(visible_count, 2);
        assert_eq!(quick_open_visible_result_count(&cache), visible_count);
    }

    use super::UiEvent;
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        terminal::TerminalPane,
        ui_event_channel::{Receiver, ui_event_channel},
    };
    use kuroya_core::{EditorSettings, ProjectIndex, Workspace};
    use std::time::{Duration, Instant};
    use tokio::runtime::Runtime;

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    fn synthetic_project_index(root: &str, file_count: usize) -> ProjectIndex {
        let files = (0..file_count)
            .map(|index| format!("{root}/src/file_{index:05}.rs"))
            .collect::<Vec<_>>();
        serde_json::from_value(serde_json::json!({
            "root": root,
            "files": files,
            "entries": [],
            "symbols": [],
            "truncated": false,
        }))
        .expect("project index should deserialize")
    }

    fn synthetic_project_index_with_files(files: Vec<&str>) -> ProjectIndex {
        serde_json::from_value(serde_json::json!({
            "root": "workspace",
            "files": files,
            "entries": [],
            "symbols": [],
            "truncated": false,
        }))
        .expect("project index should deserialize")
    }

    fn seeded_completed_cache(
        generation: u64,
        query: &str,
        rels: &[&str],
    ) -> QuickOpenResultsCache {
        let results = rels
            .iter()
            .map(|rel| quick_open_result(rel, None))
            .collect::<Vec<_>>();
        QuickOpenResultsCache {
            query_input: query.to_owned(),
            index_generation: generation,
            recent_files: VecDeque::new(),
            open_files: Vec::new(),
            query_memory: VecDeque::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            parsed_query: QuickOpenQuery {
                pattern: query.to_owned(),
                line: None,
                column: 1,
            },
            result_labels: rels.iter().map(|rel| (*rel).to_owned()).collect(),
            results,
            background_rank: None,
            last_query_changed_at: None,
            completed_ranking: Some(QuickOpenCompletedRanking {
                query: query.to_owned(),
                generation,
                matched_paths_truncated: false,
                matched_paths: rels
                    .iter()
                    .map(|rel| PathBuf::from("workspace").join(rel))
                    .collect(),
            }),
        }
    }

    fn ranked_event(rx: &Receiver<UiEvent>) -> UiEvent {
        rx.recv_timeout(Duration::from_secs(30))
            .expect("ranked event should arrive")
    }

    #[test]
    fn quick_open_small_universe_ranks_on_background_thread() {
        let ctx = egui::Context::default();
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.index = synthetic_project_index("workspace", 4);
        app.project_index_generation = 3;
        app.quick_open_query = "file".to_owned();

        let visible = app.refresh_quick_open_results_cache(&ctx);

        assert_eq!(visible, 0);
        let cache = app.quick_open_results_cache.as_ref().unwrap();
        let first_request_id = cache
            .background_rank
            .as_ref()
            .expect("ranking should run on the background thread")
            .request_id;
        assert_eq!(cache.index_generation, 3);
        assert!(cache.results.is_empty());

        match ranked_event(&app.rx) {
            UiEvent::QuickOpenRanked {
                request_id,
                key,
                results,
            } => {
                assert_eq!(request_id, first_request_id);
                assert_eq!(key.query_input, "file");
                assert!(!results.is_empty());
                assert!(app.apply_quick_open_ranked_results(request_id, &key, results));
            }
            event => panic!("unexpected event: {event:?}"),
        }
        let cache = app.quick_open_results_cache.as_ref().unwrap();
        assert!(cache.background_rank.is_none());
        assert_eq!(cache.results.len(), cache.result_labels.len());
        let ranking = cache
            .completed_ranking
            .as_ref()
            .expect("completed ranking should be kept for prefix reuse");
        assert_eq!(ranking.query, "file");
        assert_eq!(ranking.generation, 3);
        assert_eq!(ranking.matched_paths.len(), cache.results.len());
    }

    #[test]
    fn quick_open_background_ranking_shows_stale_rows_until_results_arrive() {
        let ctx = egui::Context::default();
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.index = synthetic_project_index("workspace", 4);
        app.quick_open_query = "old".to_owned();
        app.quick_open_results_cache = Some(QuickOpenResultsCache {
            query_input: "old".to_owned(),
            index_generation: app.project_index_generation,
            recent_files: VecDeque::new(),
            open_files: Vec::new(),
            query_memory: VecDeque::new(),
            navigation_back: VecDeque::new(),
            navigation_forward: VecDeque::new(),
            current_navigation_location: None,
            parsed_query: QuickOpenQuery {
                pattern: "old".to_owned(),
                line: None,
                column: 1,
            },
            result_labels: vec!["stale".to_owned()],
            results: vec![quick_open_result("stale", None)],
            background_rank: None,
            last_query_changed_at: None,
            completed_ranking: None,
        });
        app.quick_open_query = "file_00001".to_owned();

        let first_visible = app.refresh_quick_open_results_cache(&ctx);

        assert_eq!(first_visible, 1);
        let first_request_id = app
            .quick_open_results_cache
            .as_ref()
            .and_then(|cache| cache.background_rank.as_ref())
            .expect("background rank should be outstanding")
            .request_id;
        assert!(
            app.quick_open_results_cache.as_ref().unwrap().results[0]
                .rel
                .starts_with("stale")
        );

        let second_visible = app.refresh_quick_open_results_cache(&ctx);

        assert_eq!(second_visible, first_visible);
        assert_eq!(
            app.quick_open_results_cache
                .as_ref()
                .and_then(|cache| cache.background_rank.as_ref())
                .expect("outstanding rank should persist")
                .request_id,
            first_request_id
        );

        match ranked_event(&app.rx) {
            UiEvent::QuickOpenRanked {
                request_id,
                key,
                results,
            } => {
                assert_eq!(request_id, first_request_id);
                assert_eq!(key.query_input, "file_00001");
                assert!(!results.is_empty());
                assert!(app.apply_quick_open_ranked_results(request_id, &key, results));
            }
            event => panic!("unexpected event: {event:?}"),
        }
        let cache = app.quick_open_results_cache.as_ref().unwrap();
        assert!(cache.background_rank.is_none());
        assert_eq!(cache.query_input, "file_00001");
        assert_eq!(cache.results.len(), cache.result_labels.len());
        assert!(
            cache
                .results
                .iter()
                .any(|result| result.rel.contains("file_00001"))
        );
    }

    #[test]
    fn quick_open_query_edits_debounce_background_ranking() {
        let ctx = egui::Context::default();
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.index = synthetic_project_index("workspace", 4);
        app.project_index_generation = 3;
        app.quick_open_query = "file".to_owned();
        app.refresh_quick_open_results_cache(&ctx);
        let first_request_id = app
            .quick_open_results_cache
            .as_ref()
            .and_then(|cache| cache.background_rank.as_ref())
            .expect("initial rank should be outstanding")
            .request_id;

        app.quick_open_results_cache
            .as_mut()
            .unwrap()
            .last_query_changed_at = Some(Instant::now());
        app.quick_open_query = "file_0".to_owned();

        let visible = app.refresh_quick_open_results_cache(&ctx);

        assert_eq!(visible, 0);
        let cache = app.quick_open_results_cache.as_ref().unwrap();
        assert_eq!(
            cache
                .background_rank
                .as_ref()
                .expect("debounced edit should keep the outstanding rank")
                .request_id,
            first_request_id
        );

        let last_change = Instant::now()
            .checked_sub(Duration::from_millis(151))
            .expect("test instant underflow");
        app.quick_open_results_cache
            .as_mut()
            .unwrap()
            .last_query_changed_at = Some(last_change);

        app.refresh_quick_open_results_cache(&ctx);

        let second_request_id = app
            .quick_open_results_cache
            .as_ref()
            .and_then(|cache| cache.background_rank.as_ref())
            .expect("re-run should be outstanding after the debounce window")
            .request_id;
        assert_ne!(second_request_id, first_request_id);

        let mut accepted = false;
        for _ in 0..2 {
            match ranked_event(&app.rx) {
                UiEvent::QuickOpenRanked {
                    request_id,
                    key,
                    results,
                } => {
                    let applied = app.apply_quick_open_ranked_results(request_id, &key, results);
                    if request_id == second_request_id {
                        assert!(applied, "current ranking should be accepted");
                        accepted = true;
                    } else {
                        assert!(!applied, "late ranking for the old query must be rejected");
                    }
                }
                event => panic!("unexpected event: {event:?}"),
            }
        }
        assert!(accepted);
        assert!(
            app.quick_open_results_cache
                .as_ref()
                .unwrap()
                .results
                .iter()
                .any(|result| result.rel.contains("file_00001"))
        );
    }

    #[test]
    fn quick_open_stale_ranked_events_are_ignored() {
        let ctx = egui::Context::default();
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.index = synthetic_project_index("workspace", 4);
        app.quick_open_query = "file_00002".to_owned();
        app.refresh_quick_open_results_cache(&ctx);
        let outstanding_request_id = app
            .quick_open_results_cache
            .as_ref()
            .and_then(|cache| cache.background_rank.as_ref())
            .expect("background rank should be outstanding")
            .request_id;
        let key = {
            let outstanding_key = app
                .quick_open_results_cache
                .as_ref()
                .and_then(|cache| cache.background_rank.as_ref())
                .map(|rank| rank.key.clone())
                .expect("outstanding key");
            (*outstanding_key).clone()
        };

        let wrong_request_id = outstanding_request_id.wrapping_add(1);
        assert!(!app.apply_quick_open_ranked_results(
            wrong_request_id,
            &key,
            vec![quick_open_result("spoofed", None)],
        ));

        let mut mismatched_key = key.clone();
        mismatched_key.query_input = "changed".to_owned();
        assert!(!app.apply_quick_open_ranked_results(
            outstanding_request_id,
            &mismatched_key,
            vec![quick_open_result("spoofed", None)],
        ));

        let cache = app.quick_open_results_cache.as_ref().unwrap();
        assert_eq!(
            cache
                .background_rank
                .as_ref()
                .expect("outstanding rank should survive stale events")
                .request_id,
            outstanding_request_id
        );
        assert!(cache.results.is_empty());

        match ranked_event(&app.rx) {
            UiEvent::QuickOpenRanked {
                request_id,
                key,
                results,
            } => {
                assert!(app.apply_quick_open_ranked_results(request_id, &key, results));
            }
            event => panic!("unexpected event: {event:?}"),
        }
        assert!(
            app.quick_open_results_cache
                .as_ref()
                .unwrap()
                .results
                .iter()
                .any(|result| result.rel.contains("file_00002"))
        );
    }

    #[test]
    fn ranking_due_requires_full_debounce_window_since_last_edit() {
        let base = Instant::now();
        assert!(!ranking_due(base, base));
        assert!(!ranking_due(base, base + Duration::from_millis(149)));
        assert!(ranking_due(base, base + Duration::from_millis(150)));
        assert!(ranking_due(base, base + Duration::from_millis(5_000)));
    }

    #[test]
    fn quick_open_rank_debounce_delay_covers_the_remaining_window() {
        let base = Instant::now();
        assert_eq!(
            quick_open_rank_debounce_delay(base, base),
            Duration::from_millis(150)
        );
        assert_eq!(
            quick_open_rank_debounce_delay(base, base + Duration::from_millis(149)),
            Duration::from_millis(1)
        );
        assert_eq!(
            quick_open_rank_debounce_delay(base, base + Duration::from_millis(150)),
            Duration::ZERO
        );
    }

    #[test]
    fn candidates_for_query_reuses_previous_paths_for_prefix_extensions() {
        let previous_paths = [
            PathBuf::from("workspace/src/alpha.rs"),
            PathBuf::from("workspace/src/alpha_beta.rs"),
        ];

        let source =
            candidates_for_query(Some(("al", previous_paths.as_slice(), false)), "alph", true);

        assert_eq!(source, CandidateSource::Reuse(previous_paths.as_slice()));

        assert_eq!(
            candidates_for_query(
                Some(("alph", previous_paths.as_slice(), false)),
                "alph",
                true
            ),
            CandidateSource::Reuse(previous_paths.as_slice())
        );
    }

    #[test]
    fn candidates_for_query_falls_back_to_full_after_an_empty_query() {
        let default_view_paths = [PathBuf::from("workspace/src/alpha.rs")];

        assert_eq!(
            candidates_for_query(
                Some(("", default_view_paths.as_slice(), false)),
                "alph",
                true
            ),
            CandidateSource::Full
        );
    }

    #[test]
    fn candidates_for_query_falls_back_to_full_scan_without_prefix_extension() {
        let previous_paths = [PathBuf::from("workspace/src/alpha.rs")];

        assert_eq!(
            candidates_for_query(None, "alph", true),
            CandidateSource::Full
        );

        assert_eq!(
            candidates_for_query(Some(("alph", previous_paths.as_slice(), false)), "al", true),
            CandidateSource::Full
        );

        assert_eq!(
            candidates_for_query(
                Some(("alph", previous_paths.as_slice(), false)),
                "beta",
                true
            ),
            CandidateSource::Full
        );

        assert_eq!(
            candidates_for_query(Some(("alph", previous_paths.as_slice(), false)), "", true),
            CandidateSource::Full
        );
    }

    #[test]
    fn candidates_for_query_falls_back_to_full_scan_when_generation_changes() {
        let previous_paths = [PathBuf::from("workspace/src/alpha.rs")];

        assert_eq!(
            candidates_for_query(
                Some(("alph", previous_paths.as_slice(), false)),
                "alpha",
                false
            ),
            CandidateSource::Full
        );
    }

    #[test]
    fn quick_open_prefix_extension_reranks_only_previous_matched_paths() {
        let ctx = egui::Context::default();
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.index = synthetic_project_index_with_files(vec![
            "workspace/src/alpha.rs",
            "workspace/src/gamma_2.rs",
            "workspace/src/xray_2.rs",
        ]);
        app.project_index_generation = 3;

        app.quick_open_results_cache = Some(seeded_completed_cache(
            3,
            "a",
            &["src/alpha.rs", "src/gamma_2.rs"],
        ));
        app.quick_open_query = "a_2".to_owned();

        let visible = app.refresh_quick_open_results_cache(&ctx);

        assert_eq!(visible, 2);
        match ranked_event(&app.rx) {
            UiEvent::QuickOpenRanked {
                request_id,
                key,
                results,
            } => {
                assert_eq!(key.query_input, "a_2");
                assert!(results.iter().any(|result| result.rel.contains("gamma_2")));
                assert!(
                    !results.iter().any(|result| result.rel.contains("xray_2")),
                    "candidates outside the previous matched set must be skipped"
                );
                assert!(app.apply_quick_open_ranked_results(request_id, &key, results));
            }
            event => panic!("unexpected event: {event:?}"),
        }
        let cache = app.quick_open_results_cache.as_ref().unwrap();
        assert!(
            cache
                .results
                .iter()
                .any(|result| result.rel.contains("gamma_2"))
        );
        assert!(
            !cache
                .results
                .iter()
                .any(|result| result.rel.contains("xray_2"))
        );
        assert_eq!(
            cache
                .completed_ranking
                .as_ref()
                .map(|ranking| ranking.query.as_str()),
            Some("a_2")
        );
    }

    #[test]
    fn quick_open_backspace_falls_back_to_full_candidates() {
        let ctx = egui::Context::default();
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.index = synthetic_project_index_with_files(vec![
            "workspace/src/alpha.rs",
            "workspace/src/arc.rs",
        ]);
        app.project_index_generation = 3;
        app.quick_open_results_cache = Some(seeded_completed_cache(3, "a", &["src/alpha.rs"]));
        app.quick_open_query = String::new();

        app.refresh_quick_open_results_cache(&ctx);

        match ranked_event(&app.rx) {
            UiEvent::QuickOpenRanked {
                request_id,
                key,
                results,
            } => {
                assert_eq!(key.query_input, "");
                assert!(
                    results.iter().any(|result| result.rel.contains("alpha"))
                        && results.iter().any(|result| result.rel.contains("arc")),
                    "a shortened query must rescan the full index"
                );
                assert!(app.apply_quick_open_ranked_results(request_id, &key, results));
            }
            event => panic!("unexpected event: {event:?}"),
        }
    }

    #[test]
    fn quick_open_generation_change_falls_back_to_full_candidates() {
        let ctx = egui::Context::default();
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.index = synthetic_project_index_with_files(vec![
            "workspace/src/alpha.rs",
            "workspace/src/arc.rs",
        ]);
        app.project_index_generation = 4;
        app.quick_open_results_cache = Some(seeded_completed_cache(3, "a", &["src/alpha.rs"]));
        app.quick_open_query = "ar".to_owned();

        app.refresh_quick_open_results_cache(&ctx);

        match ranked_event(&app.rx) {
            UiEvent::QuickOpenRanked {
                request_id,
                key,
                results,
            } => {
                assert_eq!(key.query_input, "ar");
                assert!(
                    results.iter().any(|result| result.rel.contains("arc")),
                    "an index generation change must rescan the full index"
                );
                assert!(app.apply_quick_open_ranked_results(request_id, &key, results));
            }
            event => panic!("unexpected event: {event:?}"),
        }
    }
}

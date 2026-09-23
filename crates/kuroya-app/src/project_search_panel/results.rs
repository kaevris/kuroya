use crate::{
    path_display::display_error_label_cow,
    picker_ui::{PICKER_ROW_HEIGHT, picker_scroll_area, picker_selectable_row},
    ui_state::selected_row_scroll_offset,
};
use eframe::egui::{self, Color32, RichText, Ui};
use kuroya_core::{SearchMatch, SearchResult};
use std::{
    borrow::Cow,
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
};

#[path = "results/labels.rs"]
mod labels;

#[cfg(test)]
use labels::{
    MAX_PROJECT_SEARCH_SUMMARY_FILE_SCAN, MAX_QUERY_LABEL_CHARS, MAX_RESULT_PATH_CHARS,
    MAX_RESULT_PREVIEW_CHARS, ProjectSearchMatchedFileCount, project_search_matched_files_label,
    project_search_query_label, project_search_result_path_label,
    project_search_result_preview_label, project_search_result_row_label,
    project_search_visible_matched_file_count, project_search_visible_matched_files,
};
use labels::{
    project_search_empty_stats_detail, project_search_query_label_cow,
    project_search_result_path_label_cow, project_search_result_preview_label_cow,
    project_search_result_row_label_with_display_fragments, project_search_result_summary,
};

const MAX_PROJECT_SEARCH_PREPARED_ROWS: usize = 512;

pub(super) fn project_search_result_row_height(_ui: &Ui) -> f32 {
    PICKER_ROW_HEIGHT
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectSearchOpenTarget {
    pub(super) path: PathBuf,
    pub(super) line: usize,
    pub(super) column: usize,
}

impl ProjectSearchOpenTarget {
    fn from_match(result_match: &SearchMatch) -> Self {
        Self {
            path: result_match.path.clone(),
            line: result_match.line,
            column: result_match.column,
        }
    }
}

pub(super) fn project_search_open_target(
    result: &SearchResult,
    selected_index: usize,
    results_match_query: bool,
) -> Option<ProjectSearchOpenTarget> {
    let result_match = result.matches.get(selected_index)?;
    project_search_open_target_for_match(result_match, results_match_query)
}

fn project_search_open_target_for_match(
    result_match: &SearchMatch,
    results_match_query: bool,
) -> Option<ProjectSearchOpenTarget> {
    if !results_match_query {
        return None;
    }

    Some(ProjectSearchOpenTarget::from_match(result_match))
}

pub(super) fn render_project_search_results(
    ui: &mut Ui,
    workspace_root: &Path,
    result: &SearchResult,
    current_query: &str,
    results_match_query: bool,
    selected_index: &mut usize,
    scroll_to_selection: bool,
) -> Option<ProjectSearchOpenTarget> {
    let mut open_target = None;
    clamp_project_search_result_selection(selected_index, result);
    if result.matches.is_empty() {
        render_project_search_empty_state(
            ui,
            project_search_empty_state(result, current_query, results_match_query),
        );
        return None;
    }

    if let Some(summary) = project_search_result_summary(result, results_match_query) {
        ui.label(
            RichText::new(summary)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(3.0);
    }
    if !results_match_query {
        render_project_search_notice(
            ui,
            "Previous results",
            "Search is updating for the current filters.",
            Color32::from_rgb(231, 185, 87),
        );
    }
    let row_count = project_search_render_row_count(result);
    let row_height = project_search_result_row_height(ui);
    let viewport_height = ui.available_height();
    let mut scroll_area = picker_scroll_area();
    if scroll_to_selection {
        scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
            *selected_index,
            row_count,
            row_height,
            viewport_height,
        ));
    }
    scroll_area.show_rows(ui, row_height, row_count, |ui, rows| {
        let includes_truncated_row = project_search_visible_rows_include_truncated(result, &rows);
        let match_rows = project_search_visible_match_rows(result, rows);
        let mut display_rows = ProjectSearchResultRowsDisplay::default();
        display_rows.for_each_prepared_row(workspace_root, result, match_rows, |row| {
            let ProjectSearchPreparedResultRow {
                index,
                result_match,
                label,
            } = row;
            let selected = index == *selected_index;
            let response = render_project_search_result_row(ui, &label, selected);
            let clicked = response.clicked();
            if clicked {
                *selected_index = index;
                if results_match_query {
                    open_target = Some(ProjectSearchOpenTarget::from_match(result_match));
                }
            }
        });
        if includes_truncated_row {
            render_project_search_truncated_row(ui, row_height);
        }
    });
    open_target
}

fn render_project_search_result_row(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    picker_selectable_row(ui, selected, label).on_hover_text(label)
}

fn render_project_search_truncated_row(ui: &mut Ui, row_height: f32) {
    ui.add_sized(
        [ui.available_width(), row_height],
        egui::Label::new(RichText::new("More matches not shown").weak()).truncate(),
    );
}

fn clamp_project_search_result_selection(selected_index: &mut usize, result: &SearchResult) {
    *selected_index = (*selected_index).min(result.matches.len().saturating_sub(1));
}

fn project_search_visible_match_rows(result: &SearchResult, rows: Range<usize>) -> Range<usize> {
    let match_count = result.matches.len();
    rows.start.min(match_count)..rows.end.min(match_count)
}

fn project_search_render_row_count(result: &SearchResult) -> usize {
    result
        .matches
        .len()
        .saturating_add(usize::from(result.truncated))
}

fn project_search_visible_rows_include_truncated(
    result: &SearchResult,
    rows: &Range<usize>,
) -> bool {
    result.truncated && rows.contains(&result.matches.len())
}

fn render_project_search_empty_state(ui: &mut Ui, state: ProjectSearchEmptyState) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        let mut title = RichText::new(state.title).strong();
        if let Some(color) = state.color {
            title = title.color(color);
        }
        ui.label(title);
        if let Some(detail) = state.detail {
            ui.label(
                RichText::new(detail)
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
    });
}

fn render_project_search_notice(ui: &mut Ui, title: &str, detail: &str, color: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(title).small().strong().color(color));
        ui.label(
            RichText::new(detail)
                .small()
                .color(ui.visuals().weak_text_color()),
        );
    });
}

#[derive(Debug, PartialEq)]
struct ProjectSearchEmptyState {
    title: String,
    detail: Option<String>,
    color: Option<Color32>,
}

fn project_search_empty_state(
    result: &SearchResult,
    current_query: &str,
    results_match_query: bool,
) -> ProjectSearchEmptyState {
    if current_query.is_empty() {
        return ProjectSearchEmptyState {
            title: "Enter a search query".to_owned(),
            detail: None,
            color: None,
        };
    }
    if !results_match_query {
        return ProjectSearchEmptyState {
            title: "Search updating".to_owned(),
            detail: Some("Results will appear for the current filters.".to_owned()),
            color: Some(Color32::from_rgb(231, 185, 87)),
        };
    }
    if let Some(error) = &result.error {
        return ProjectSearchEmptyState {
            title: "Search failed".to_owned(),
            detail: Some(display_error_label_cow(error).into_owned()),
            color: Some(Color32::from_rgb(232, 98, 98)),
        };
    }

    let query = project_search_query_label_cow(current_query);
    let query = query.as_ref();
    let mut title = String::with_capacity(query.len().saturating_add("No matches for \"\"".len()));
    title.push_str("No matches for \"");
    title.push_str(query);
    title.push('"');
    ProjectSearchEmptyState {
        title,
        detail: project_search_empty_stats_detail(result.stats),
        color: None,
    }
}

#[derive(Default)]
struct ProjectSearchResultRowsDisplay<'a> {
    path_fragments: Vec<ProjectSearchResultPathDisplayFragment<'a>>,
    preview_fragments: Vec<ProjectSearchResultPreviewDisplayFragment<'a>>,
    path_fragment_indices: HashMap<&'a Path, usize>,
    preview_fragment_indices: HashMap<&'a str, usize>,
}

impl<'a> ProjectSearchResultRowsDisplay<'a> {
    fn for_each_prepared_row(
        &mut self,
        workspace_root: &Path,
        result: &'a SearchResult,
        rows: Range<usize>,
        render_row: impl FnMut(ProjectSearchPreparedResultRow<'a>),
    ) {
        self.for_each_prepared_row_with(
            result,
            rows,
            |path| project_search_result_path_label_cow(workspace_root, path),
            project_search_result_preview_label_cow,
            render_row,
        );
    }

    fn for_each_prepared_row_with(
        &mut self,
        result: &'a SearchResult,
        rows: impl IntoIterator<Item = usize>,
        path_label: impl FnMut(&'a Path) -> Cow<'a, str>,
        preview_label: impl FnMut(&'a str) -> Cow<'a, str>,
        mut render_row: impl FnMut(ProjectSearchPreparedResultRow<'a>),
    ) {
        let _ =
            self.try_for_each_prepared_row_with(result, rows, path_label, preview_label, |row| {
                render_row(row);
                true
            });
    }

    fn try_for_each_prepared_row_with(
        &mut self,
        result: &'a SearchResult,
        rows: impl IntoIterator<Item = usize>,
        mut path_label: impl FnMut(&'a Path) -> Cow<'a, str>,
        mut preview_label: impl FnMut(&'a str) -> Cow<'a, str>,
        mut render_row: impl FnMut(ProjectSearchPreparedResultRow<'a>) -> bool,
    ) -> bool {
        let rows = rows.into_iter().take(MAX_PROJECT_SEARCH_PREPARED_ROWS);
        let (min_rows, max_rows) = rows.size_hint();
        let fragment_capacity = max_rows
            .unwrap_or(min_rows)
            .min(MAX_PROJECT_SEARCH_PREPARED_ROWS);
        self.clear_fragments();
        self.reserve_fragment_capacity(fragment_capacity);
        let mut expected_index = None;
        for index in rows {
            if let Some(expected_index) = expected_index {
                if index != expected_index {
                    break;
                }
            }
            let Some(result_match) = result.matches.get(index) else {
                break;
            };
            expected_index = index.checked_add(1);
            let path_label_index =
                self.path_label_index(result_match.path.as_path(), &mut path_label);
            let preview_label_index =
                self.preview_label_index(&result_match.preview, &mut preview_label);
            let label = {
                let path_label = &self.path_fragments[path_label_index].label;
                let preview_label = &self.preview_fragments[preview_label_index].label;
                project_search_result_row_label_with_display_fragments(
                    result_match,
                    path_label,
                    preview_label,
                )
            };
            if !render_row(ProjectSearchPreparedResultRow {
                index,
                result_match,
                label,
            }) {
                return false;
            }
        }
        true
    }

    fn clear_fragments(&mut self) {
        self.path_fragments.clear();
        self.preview_fragments.clear();
        self.path_fragment_indices.clear();
        self.preview_fragment_indices.clear();
    }

    fn reserve_fragment_capacity(&mut self, fragment_capacity: usize) {
        self.path_fragments
            .reserve(fragment_capacity.saturating_sub(self.path_fragments.capacity()));
        self.preview_fragments
            .reserve(fragment_capacity.saturating_sub(self.preview_fragments.capacity()));
        self.path_fragment_indices
            .reserve(fragment_capacity.saturating_sub(self.path_fragment_indices.capacity()));
        self.preview_fragment_indices
            .reserve(fragment_capacity.saturating_sub(self.preview_fragment_indices.capacity()));
    }

    fn path_label_index(
        &mut self,
        path: &'a Path,
        path_label: &mut impl FnMut(&'a Path) -> Cow<'a, str>,
    ) -> usize {
        if let Some(index) = self.path_fragment_indices.get(path) {
            return *index;
        }

        let index = self.path_fragments.len();
        self.path_fragments
            .push(ProjectSearchResultPathDisplayFragment {
                label: path_label(path),
            });
        self.path_fragment_indices.insert(path, index);
        index
    }

    fn preview_label_index(
        &mut self,
        preview: &'a str,
        preview_label: &mut impl FnMut(&'a str) -> Cow<'a, str>,
    ) -> usize {
        if let Some(index) = self.preview_fragment_indices.get(preview) {
            return *index;
        }

        let index = self.preview_fragments.len();
        self.preview_fragments
            .push(ProjectSearchResultPreviewDisplayFragment {
                label: preview_label(preview),
            });
        self.preview_fragment_indices.insert(preview, index);
        index
    }
}

struct ProjectSearchResultPathDisplayFragment<'a> {
    label: Cow<'a, str>,
}

struct ProjectSearchResultPreviewDisplayFragment<'a> {
    label: Cow<'a, str>,
}

struct ProjectSearchPreparedResultRow<'a> {
    index: usize,
    result_match: &'a SearchMatch,
    label: String,
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_PROJECT_SEARCH_PREPARED_ROWS, MAX_PROJECT_SEARCH_SUMMARY_FILE_SCAN,
        MAX_QUERY_LABEL_CHARS, MAX_RESULT_PATH_CHARS, MAX_RESULT_PREVIEW_CHARS,
        ProjectSearchEmptyState, ProjectSearchMatchedFileCount, ProjectSearchOpenTarget,
        ProjectSearchPreparedResultRow, ProjectSearchResultRowsDisplay,
        clamp_project_search_result_selection, project_search_empty_state,
        project_search_empty_stats_detail, project_search_matched_files_label,
        project_search_open_target, project_search_open_target_for_match,
        project_search_query_label, project_search_query_label_cow,
        project_search_render_row_count, project_search_result_path_label,
        project_search_result_path_label_cow, project_search_result_preview_label,
        project_search_result_preview_label_cow, project_search_result_row_label,
        project_search_result_row_label_with_display_fragments, project_search_result_summary,
        project_search_visible_match_rows, project_search_visible_matched_file_count,
        project_search_visible_matched_files, project_search_visible_rows_include_truncated,
    };
    use crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS;
    use eframe::egui::Color32;
    use kuroya_core::{SearchMatch, SearchResult, SearchStats};
    use std::{
        borrow::Cow,
        path::{Path, PathBuf},
    };

    fn collect_project_search_result_rows<'a>(
        display: &mut ProjectSearchResultRowsDisplay<'a>,
        result: &'a SearchResult,
        rows: impl IntoIterator<Item = usize>,
        path_label: impl FnMut(&'a Path) -> Cow<'a, str>,
        preview_label: impl FnMut(&'a str) -> Cow<'a, str>,
    ) -> Vec<ProjectSearchPreparedResultRow<'a>> {
        let mut prepared_rows = Vec::new();
        display.for_each_prepared_row_with(result, rows, path_label, preview_label, |row| {
            prepared_rows.push(row);
        });
        prepared_rows
    }

    #[test]
    fn project_search_result_row_label_uses_workspace_relative_path() {
        let root = PathBuf::from("workspace");
        let result_match = SearchMatch {
            path: root.join("src/main.rs"),
            line: 3,
            column: 5,
            preview: "let needle = true;".to_owned(),
        };

        assert_eq!(
            project_search_result_row_label(&root, &result_match),
            format!(
                "{}:3:5  let needle = true;",
                PathBuf::from("src/main.rs").display()
            )
        );
    }

    #[test]
    fn project_search_result_row_label_uses_exact_capacity() {
        let result_match = SearchMatch {
            path: PathBuf::from("ignored.rs"),
            line: 12345,
            column: 0,
            preview: "ignored".to_owned(),
        };

        let label = project_search_result_row_label_with_display_fragments(
            &result_match,
            "src/main.rs",
            "let \u{03bb} = needle;",
        );

        assert_eq!(label, "src/main.rs:12345:0  let \u{03bb} = needle;");
        assert_eq!(label.capacity(), label.len());
    }

    #[test]
    fn project_search_result_preview_label_cow_borrows_clean_ascii_and_unicode() {
        for preview in ["let needle = true;", "let \u{03bb} = \"needle\";"] {
            match project_search_result_preview_label_cow(preview) {
                Cow::Borrowed(label) => assert_eq!(label, preview),
                Cow::Owned(label) => panic!("expected borrowed clean preview label, got {label:?}"),
            }
            assert_eq!(project_search_result_preview_label(preview), preview);
        }
    }

    #[test]
    fn project_search_result_preview_label_cow_owns_dirty_truncated_and_fallback_labels() {
        let long = format!("preview-{}", "text".repeat(MAX_RESULT_PREVIEW_CHARS));
        for preview in [
            " first match ",
            "first line\nsecond line \u{202e}",
            long.as_str(),
            "\n\u{202e}\u{0007}",
        ] {
            let label = project_search_result_preview_label_cow(preview);

            assert!(matches!(&label, Cow::Owned(_)));
            assert_eq!(project_search_result_preview_label(preview), label.as_ref());
            assert!(!label.contains('\n'));
            assert!(!label.contains('\u{202e}'));
            assert!(label.chars().count() <= MAX_RESULT_PREVIEW_CHARS);
        }
    }

    #[test]
    fn project_search_result_path_label_cow_borrows_clean_ascii_and_unicode_relative_paths() {
        let root = PathBuf::from("workspace");
        for path in [
            root.join("src/main.rs"),
            root.join("src").join("clean-\u{03bb}.rs"),
        ] {
            let expected = path
                .strip_prefix(&root)
                .expect("relative test path")
                .display()
                .to_string();

            match project_search_result_path_label_cow(&root, &path) {
                Cow::Borrowed(label) => assert_eq!(label, expected),
                Cow::Owned(label) => panic!("expected borrowed clean path label, got {label:?}"),
            }
            assert_eq!(project_search_result_path_label(&root, &path), expected);
        }
    }

    #[test]
    fn project_search_result_path_label_normalizes_lexically_equivalent_relative_paths() {
        let root = PathBuf::from("workspace");
        let path = root.join("src").join("..").join("src").join("main.rs");

        assert_eq!(
            project_search_result_path_label(&root, &path),
            PathBuf::from("src").join("main.rs").display().to_string()
        );
    }

    #[test]
    fn project_search_result_path_label_cow_owns_dirty_truncated_and_fallback_labels() {
        let root = PathBuf::from("workspace");
        let long = root.join(format!(
            "src/{}-tail.rs",
            "very-long-component".repeat(MAX_RESULT_PATH_CHARS)
        ));
        for path in [root.join("src/bad\nname\u{202e}.rs"), long, PathBuf::new()] {
            let label = project_search_result_path_label_cow(&root, &path);

            assert!(matches!(&label, Cow::Owned(_)));
            assert_eq!(
                project_search_result_path_label(&root, &path),
                label.as_ref()
            );
            assert!(!label.contains('\n'));
            assert!(!label.contains('\u{202e}'));
            assert!(label.chars().count() <= MAX_RESULT_PATH_CHARS);
        }
        assert_eq!(
            project_search_result_path_label_cow(&root, &PathBuf::new()).as_ref(),
            "."
        );
    }

    #[test]
    fn project_search_query_label_cow_borrows_clean_ascii_and_unicode() {
        for query in ["needle", "needle-\u{03bb}"] {
            match project_search_query_label_cow(query) {
                Cow::Borrowed(label) => assert_eq!(label, query),
                Cow::Owned(label) => panic!("expected borrowed clean query label, got {label:?}"),
            }
            assert_eq!(project_search_query_label(query), query);
        }
    }

    #[test]
    fn project_search_query_label_cow_owns_dirty_truncated_and_fallback_labels() {
        let long = format!("query-{}", "text".repeat(MAX_QUERY_LABEL_CHARS));
        for query in [
            " needle ",
            "first line\nsecond line \u{202e}",
            long.as_str(),
            "",
            "\n\u{202e}\u{0007}",
        ] {
            let label = project_search_query_label_cow(query);

            assert!(matches!(&label, Cow::Owned(_)));
            assert_eq!(project_search_query_label(query), label.as_ref());
            assert!(!label.contains('\n'));
            assert!(!label.contains('\u{202e}'));
            assert!(label.chars().count() <= MAX_QUERY_LABEL_CHARS);
        }
        assert_eq!(project_search_query_label_cow("").as_ref(), "search text");
    }

    #[test]
    fn project_search_result_rows_display_reuses_repeated_path_and_preview_fragments() {
        let root = PathBuf::from("workspace");
        let repeated_path = root.join("src/main.rs");
        let repeated_preview = "needle repeat\nwith control".to_owned();
        let matches = [
            SearchMatch {
                path: repeated_path.clone(),
                line: 3,
                column: 5,
                preview: repeated_preview.clone(),
            },
            SearchMatch {
                path: repeated_path.clone(),
                line: 7,
                column: 1,
                preview: "needle two".to_owned(),
            },
            SearchMatch {
                path: root.join("src/lib.rs"),
                line: 9,
                column: 4,
                preview: repeated_preview,
            },
            SearchMatch {
                path: repeated_path,
                line: 11,
                column: 2,
                preview: "needle four".to_owned(),
            },
        ];
        let expected = matches
            .iter()
            .map(|result_match| project_search_result_row_label(&root, result_match))
            .collect::<Vec<_>>();
        let result = SearchResult {
            matches: matches.to_vec(),
            ..SearchResult::default()
        };
        let mut path_label_calls = 0;
        let mut preview_label_calls = 0;

        let mut display = ProjectSearchResultRowsDisplay::default();
        let labels = collect_project_search_result_rows(
            &mut display,
            &result,
            0..result.matches.len(),
            |path| {
                path_label_calls += 1;
                project_search_result_path_label_cow(&root, path)
            },
            |preview| {
                preview_label_calls += 1;
                project_search_result_preview_label_cow(preview)
            },
        )
        .into_iter()
        .map(|row| row.label)
        .collect::<Vec<_>>();
        assert_eq!(
            display.path_fragment_indices.len(),
            display.path_fragments.len()
        );
        assert_eq!(
            display.preview_fragment_indices.len(),
            display.preview_fragments.len()
        );
        assert_eq!(display.path_fragments.len(), 2);
        assert_eq!(display.preview_fragments.len(), 3);

        assert_eq!(labels, expected);
        assert_eq!(path_label_calls, 2);
        assert_eq!(preview_label_calls, 3);
    }

    #[test]
    fn project_search_result_rows_display_stops_streaming_when_consumer_stops() {
        let root = PathBuf::from("workspace");
        let result = SearchResult {
            matches: vec![
                SearchMatch {
                    path: root.join("src/main.rs"),
                    line: 3,
                    column: 5,
                    preview: "needle one".to_owned(),
                },
                SearchMatch {
                    path: root.join("src/lib.rs"),
                    line: 7,
                    column: 2,
                    preview: "needle two".to_owned(),
                },
                SearchMatch {
                    path: root.join("src/late.rs"),
                    line: 11,
                    column: 9,
                    preview: "needle late".to_owned(),
                },
            ],
            ..SearchResult::default()
        };
        let mut display = ProjectSearchResultRowsDisplay::default();
        let mut path_label_calls = 0;
        let mut preview_label_calls = 0;
        let mut streamed_indices = Vec::new();

        let completed = display.try_for_each_prepared_row_with(
            &result,
            0..result.matches.len(),
            |path| {
                path_label_calls += 1;
                Cow::Owned(path.display().to_string())
            },
            |preview| {
                preview_label_calls += 1;
                Cow::Owned(preview.to_owned())
            },
            |row| {
                streamed_indices.push(row.index);
                false
            },
        );

        assert!(!completed);
        assert_eq!(streamed_indices, vec![0]);
        assert_eq!(path_label_calls, 1);
        assert_eq!(preview_label_calls, 1);
        assert_eq!(display.path_fragments.len(), 1);
        assert_eq!(display.preview_fragments.len(), 1);
    }

    #[test]
    fn project_search_visible_match_rows_excludes_truncated_sentinel_from_preparation() {
        let result = SearchResult {
            matches: vec![
                SearchMatch {
                    path: PathBuf::from("src/main.rs"),
                    line: 3,
                    column: 5,
                    preview: "needle one".to_owned(),
                },
                SearchMatch {
                    path: PathBuf::from("src/lib.rs"),
                    line: 7,
                    column: 2,
                    preview: "needle two".to_owned(),
                },
            ],
            truncated: true,
            ..SearchResult::default()
        };
        let visible_rows = 1..3;
        let match_rows = project_search_visible_match_rows(&result, visible_rows.clone());
        let mut path_label_calls = 0;
        let mut preview_label_calls = 0;

        let mut display = ProjectSearchResultRowsDisplay::default();
        let rows = collect_project_search_result_rows(
            &mut display,
            &result,
            match_rows,
            |path| {
                path_label_calls += 1;
                Cow::Owned(path.display().to_string())
            },
            |preview| {
                preview_label_calls += 1;
                Cow::Owned(preview.to_owned())
            },
        );

        assert!(project_search_visible_rows_include_truncated(
            &result,
            &visible_rows
        ));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].index, 1);
        assert_eq!(path_label_calls, 1);
        assert_eq!(preview_label_calls, 1);
    }

    #[test]
    fn project_search_result_rows_display_caps_pathological_visible_ranges() {
        let root = PathBuf::from("workspace");
        let matches = (0..(MAX_PROJECT_SEARCH_PREPARED_ROWS + 20))
            .map(|index| SearchMatch {
                path: root.join(format!("src/file_{index}.rs")),
                line: index + 1,
                column: 1,
                preview: "needle".to_owned(),
            })
            .collect::<Vec<_>>();
        let result = SearchResult {
            matches,
            ..SearchResult::default()
        };

        let mut display = ProjectSearchResultRowsDisplay::default();
        let rows = collect_project_search_result_rows(
            &mut display,
            &result,
            0..usize::MAX,
            |path| Cow::Owned(path.display().to_string()),
            |preview| Cow::Owned(preview.to_owned()),
        );

        assert_eq!(rows.len(), MAX_PROJECT_SEARCH_PREPARED_ROWS);
        assert_eq!(rows[0].index, 0);
        assert_eq!(
            rows[MAX_PROJECT_SEARCH_PREPARED_ROWS - 1].index,
            MAX_PROJECT_SEARCH_PREPARED_ROWS - 1
        );
    }

    #[test]
    fn project_search_result_rows_display_rejects_rows_after_index_gap() {
        let root = PathBuf::from("workspace");
        let result = SearchResult {
            matches: vec![
                SearchMatch {
                    path: root.join("src/main.rs"),
                    line: 3,
                    column: 5,
                    preview: "needle one".to_owned(),
                },
                SearchMatch {
                    path: root.join("src/lib.rs"),
                    line: 7,
                    column: 2,
                    preview: "needle two".to_owned(),
                },
                SearchMatch {
                    path: root.join("src/late.rs"),
                    line: 11,
                    column: 9,
                    preview: "needle late".to_owned(),
                },
            ],
            ..SearchResult::default()
        };
        let mut path_label_calls = 0;
        let mut preview_label_calls = 0;

        let mut display = ProjectSearchResultRowsDisplay::default();
        let rows = collect_project_search_result_rows(
            &mut display,
            &result,
            [0, 2, 1],
            |path| {
                path_label_calls += 1;
                Cow::Owned(path.display().to_string())
            },
            |preview| {
                preview_label_calls += 1;
                Cow::Owned(preview.to_owned())
            },
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].index, 0);
        assert_eq!(
            ProjectSearchOpenTarget::from_match(rows[0].result_match),
            ProjectSearchOpenTarget {
                path: root.join("src/main.rs"),
                line: 3,
                column: 5,
            }
        );
        assert_eq!(path_label_calls, 1);
        assert_eq!(preview_label_calls, 1);
    }

    #[test]
    fn project_search_render_row_count_adds_only_the_truncated_sentinel() {
        let result = SearchResult {
            matches: vec![
                SearchMatch {
                    path: PathBuf::from("src/main.rs"),
                    line: 3,
                    column: 5,
                    preview: "needle one".to_owned(),
                },
                SearchMatch {
                    path: PathBuf::from("src/lib.rs"),
                    line: 7,
                    column: 2,
                    preview: "needle two".to_owned(),
                },
            ],
            truncated: true,
            ..SearchResult::default()
        };
        let mut complete_result = result.clone();
        complete_result.truncated = false;

        assert_eq!(project_search_render_row_count(&result), 3);
        assert_eq!(project_search_render_row_count(&complete_result), 2);
    }

    #[test]
    fn project_search_result_selection_clamps_empty_and_shrinking_results() {
        let empty_result = SearchResult::default();
        let mut selected = usize::MAX;

        clamp_project_search_result_selection(&mut selected, &empty_result);

        assert_eq!(selected, 0);

        let result = SearchResult {
            matches: vec![
                SearchMatch {
                    path: PathBuf::from("src/main.rs"),
                    line: 3,
                    column: 5,
                    preview: "needle one".to_owned(),
                },
                SearchMatch {
                    path: PathBuf::from("src/lib.rs"),
                    line: 7,
                    column: 2,
                    preview: "needle two".to_owned(),
                },
            ],
            ..SearchResult::default()
        };
        selected = 9;

        clamp_project_search_result_selection(&mut selected, &result);

        assert_eq!(selected, 1);
    }

    #[test]
    fn project_search_result_row_label_sanitizes_and_bounds_display_fields() {
        let root = PathBuf::from("workspace");
        let result_match = SearchMatch {
            path: root.join(format!(
                "src/bad\n{}\u{202e}tail.rs",
                "very-long-".repeat(MAX_RESULT_PATH_CHARS)
            )),
            line: 3,
            column: 5,
            preview: format!(
                "first line\nsecond line \u{202e}{}",
                "preview".repeat(MAX_RESULT_PREVIEW_CHARS)
            ),
        };

        let label = project_search_result_row_label(&root, &result_match);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.matches("...").count() >= 2);
        assert!(
            label.chars().count()
                <= MAX_RESULT_PATH_CHARS + ":3:5  ".chars().count() + MAX_RESULT_PREVIEW_CHARS
        );
    }

    #[test]
    fn project_search_prepared_result_row_preserves_raw_path_for_open_target() {
        let root = PathBuf::from("workspace");
        let raw_path = root.join("src/bad\n\u{202e}tail.rs");
        let raw_preview = "first line\nsecond line \u{202e}".to_owned();
        let result = SearchResult {
            matches: vec![SearchMatch {
                path: raw_path.clone(),
                line: 3,
                column: 5,
                preview: raw_preview.clone(),
            }],
            ..SearchResult::default()
        };

        let mut rows = Vec::new();
        let mut display = ProjectSearchResultRowsDisplay::default();
        display.for_each_prepared_row(&root, &result, 0..1, |row| rows.push(row));
        let row = rows.pop().expect("prepared search result row");

        assert_eq!(
            ProjectSearchOpenTarget::from_match(row.result_match),
            ProjectSearchOpenTarget {
                path: raw_path.clone(),
                line: 3,
                column: 5,
            }
        );
        assert_eq!(result.matches[0].preview, raw_preview);
        assert!(!row.label.contains('\n'));
        assert!(!row.label.contains('\u{202e}'));
        assert_eq!(
            project_search_open_target_for_match(&result.matches[0], false),
            None,
            "stale rows should keep previous open behavior"
        );
        assert_eq!(
            project_search_open_target_for_match(&result.matches[0], true),
            Some(ProjectSearchOpenTarget {
                path: raw_path,
                line: 3,
                column: 5,
            })
        );
    }

    #[test]
    fn project_search_result_summary_reports_result_health() {
        let root = PathBuf::from("workspace");
        let result = SearchResult {
            matches: vec![
                SearchMatch {
                    path: root.join("src/main.rs"),
                    line: 3,
                    column: 5,
                    preview: "needle".to_owned(),
                },
                SearchMatch {
                    path: root.join("src/lib.rs"),
                    line: 9,
                    column: 1,
                    preview: "needle".to_owned(),
                },
                SearchMatch {
                    path: root.join("src/lib.rs"),
                    line: 11,
                    column: 2,
                    preview: "needle".to_owned(),
                },
            ],
            truncated: true,
            error: None,
            stats: SearchStats {
                searched_files: 12,
                matched_files: 2,
                skipped_large_files: 1,
                skipped_binary_files: 0,
                skipped_unreadable_files: 1,
            },
        };

        assert_eq!(
            project_search_result_summary(&result, false),
            Some(
                "3 matches in 2 files from 12 files searched - 2 files skipped - truncated - previous search"
                    .to_owned()
            )
        );
    }

    #[test]
    fn project_search_result_summary_uses_singular_labels() {
        let result = SearchResult {
            matches: vec![SearchMatch {
                path: PathBuf::from("src/main.rs"),
                line: 3,
                column: 5,
                preview: "needle".to_owned(),
            }],
            truncated: false,
            error: None,
            stats: SearchStats {
                searched_files: 1,
                matched_files: 1,
                ..SearchStats::default()
            },
        };

        assert_eq!(
            project_search_result_summary(&result, true),
            Some("1 match in 1 file from 1 file searched".to_owned())
        );
    }

    #[test]
    fn project_search_result_summary_derives_visible_file_count_when_stats_are_missing() {
        let result = SearchResult {
            matches: vec![
                SearchMatch {
                    path: PathBuf::from("src/main.rs"),
                    line: 3,
                    column: 5,
                    preview: "needle".to_owned(),
                },
                SearchMatch {
                    path: PathBuf::from("src/lib.rs"),
                    line: 7,
                    column: 1,
                    preview: "needle".to_owned(),
                },
                SearchMatch {
                    path: PathBuf::from("src/main.rs"),
                    line: 11,
                    column: 9,
                    preview: "needle".to_owned(),
                },
            ],
            truncated: false,
            error: None,
            stats: SearchStats::default(),
        };

        assert_eq!(project_search_visible_matched_files(&result), 2);
        assert_eq!(
            project_search_result_summary(&result, true),
            Some("3 matches in 2 files".to_owned())
        );
    }

    #[test]
    fn project_search_visible_matched_files_deduplicates_large_fallback_sets() {
        let mut matches = Vec::new();
        for line in 1..=128 {
            matches.push(SearchMatch {
                path: PathBuf::from("src/main.rs"),
                line,
                column: 1,
                preview: "needle".to_owned(),
            });
        }
        matches.push(SearchMatch {
            path: PathBuf::from("src/lib.rs"),
            line: 1,
            column: 1,
            preview: "needle".to_owned(),
        });

        let result = SearchResult {
            matches,
            truncated: false,
            error: None,
            stats: SearchStats::default(),
        };

        assert_eq!(project_search_visible_matched_files(&result), 2);
        assert_eq!(
            project_search_result_summary(&result, true),
            Some("129 matches in 2 files".to_owned())
        );
    }

    #[test]
    fn project_search_result_summary_caps_missing_stats_file_count_scan() {
        let matches = (0..(MAX_PROJECT_SEARCH_SUMMARY_FILE_SCAN + 20))
            .map(|index| SearchMatch {
                path: PathBuf::from(format!("src/file_{index}.rs")),
                line: 1,
                column: 1,
                preview: "needle".to_owned(),
            })
            .collect::<Vec<_>>();
        let result = SearchResult {
            matches,
            truncated: true,
            error: None,
            stats: SearchStats::default(),
        };

        assert_eq!(
            project_search_visible_matched_file_count(&result),
            ProjectSearchMatchedFileCount {
                count: MAX_PROJECT_SEARCH_SUMMARY_FILE_SCAN,
                exact: false,
            }
        );
        assert_eq!(
            project_search_matched_files_label(project_search_visible_matched_file_count(&result)),
            format!("at least {} files", MAX_PROJECT_SEARCH_SUMMARY_FILE_SCAN)
        );
        assert_eq!(
            project_search_result_summary(&result, true),
            Some(format!(
                "{} matches in at least {} files - truncated",
                MAX_PROJECT_SEARCH_SUMMARY_FILE_SCAN + 20,
                MAX_PROJECT_SEARCH_SUMMARY_FILE_SCAN
            ))
        );
    }

    #[test]
    fn project_search_empty_state_labels_include_query_state() {
        let mut result = SearchResult::default();

        assert_eq!(
            project_search_empty_state(&result, "", true),
            ProjectSearchEmptyState {
                title: "Enter a search query".to_owned(),
                detail: None,
                color: None,
            }
        );
        assert_eq!(
            project_search_empty_state(&result, "needle", true),
            ProjectSearchEmptyState {
                title: "No matches for \"needle\"".to_owned(),
                detail: None,
                color: None,
            }
        );
        assert_eq!(
            project_search_empty_state(&result, "needle", false),
            ProjectSearchEmptyState {
                title: "Search updating".to_owned(),
                detail: Some("Results will appear for the current filters.".to_owned()),
                color: Some(Color32::from_rgb(231, 185, 87)),
            }
        );

        result.error = Some("Invalid glob `[`".to_owned());
        assert_eq!(
            project_search_empty_state(&result, "needle", true),
            ProjectSearchEmptyState {
                title: "Search failed".to_owned(),
                detail: Some("Invalid glob `[`".to_owned()),
                color: Some(Color32::from_rgb(232, 98, 98)),
            }
        );
        assert_eq!(
            project_search_empty_state(&result, "needle", false),
            ProjectSearchEmptyState {
                title: "Search updating".to_owned(),
                detail: Some("Results will appear for the current filters.".to_owned()),
                color: Some(Color32::from_rgb(231, 185, 87)),
            }
        );
    }

    #[test]
    fn project_search_empty_state_sanitizes_and_bounds_dynamic_labels() {
        let mut result = SearchResult {
            error: Some(format!(
                "Invalid glob\n{}\u{202e}",
                "error-detail".repeat(MAX_QUERY_LABEL_CHARS)
            )),
            ..SearchResult::default()
        };

        let error_state = project_search_empty_state(&result, "needle", true);

        assert_eq!(error_state.title, "Search failed");
        let error_detail = error_state.detail.expect("search error detail");
        assert!(!error_detail.contains('\n'));
        assert!(!error_detail.contains('\u{202e}'));
        assert!(error_detail.contains("..."));
        assert!(error_detail.chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);

        result.error = None;
        let query = format!(
            "needle\n{}\u{202e}",
            "query-text".repeat(MAX_QUERY_LABEL_CHARS)
        );
        let empty_state = project_search_empty_state(&result, &query, true);
        assert!(!empty_state.title.contains('\n'));
        assert!(!empty_state.title.contains('\u{202e}'));
        assert!(empty_state.title.contains("..."));
        assert!(project_search_query_label(&query).chars().count() <= MAX_QUERY_LABEL_CHARS);
    }

    #[test]
    fn project_search_empty_state_reports_search_scope_when_no_matches() {
        assert_eq!(
            project_search_empty_stats_detail(SearchStats {
                searched_files: 3,
                skipped_large_files: 1,
                skipped_binary_files: 1,
                ..SearchStats::default()
            }),
            Some("3 files searched, 2 files skipped".to_owned())
        );
    }

    #[test]
    fn project_search_open_target_rejects_stale_and_out_of_range_results() {
        let result = SearchResult {
            matches: vec![SearchMatch {
                path: PathBuf::from("src/main.rs"),
                line: 3,
                column: 5,
                preview: "needle".to_owned(),
            }],
            ..SearchResult::default()
        };

        assert_eq!(project_search_open_target(&result, 0, false), None);
        assert_eq!(project_search_open_target(&result, 1, true), None);
        assert_eq!(
            project_search_open_target(&result, 0, true),
            Some(ProjectSearchOpenTarget {
                path: PathBuf::from("src/main.rs"),
                line: 3,
                column: 5,
            })
        );
    }
}

use crate::{
    KuroyaApp,
    lsp_symbol_popups::results::{WORKSPACE_SYMBOL_ROW_HEIGHT, render_workspace_symbol_results},
    path_display::sanitized_display_label_cow,
    popup_buttons::{PopupButtonKind, popup_button},
    ui_state::{handle_list_navigation_keys, selection_page_step},
};
use eframe::egui::{self, Align, Context, Key, RichText, TextEdit};
use std::{borrow::Cow, fmt::Write as _};

mod results;

const WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS: usize = 160;
const WORKSPACE_SYMBOL_RENDER_MAX_ROWS: usize = 1_000;
const WORKSPACE_SYMBOL_NO_QUERY_LABEL: &str = "No query submitted";
const WORKSPACE_SYMBOL_QUERY_LABEL_PREFIX: &str = "Query: `";
const WORKSPACE_SYMBOL_QUERY_LABEL_SUFFIX: &str = "`";
const WORKSPACE_SYMBOL_QUERY_LABEL_FALLBACK: &str = "<empty>";
const WORKSPACE_SYMBOL_RESULT_COUNT_LABELS: [&str; 17] = [
    "0 results",
    "1 result",
    "2 results",
    "3 results",
    "4 results",
    "5 results",
    "6 results",
    "7 results",
    "8 results",
    "9 results",
    "10 results",
    "11 results",
    "12 results",
    "13 results",
    "14 results",
    "15 results",
    "16 results",
];

impl KuroyaApp {
    pub(crate) fn render_workspace_symbols(&mut self, ctx: &Context) {
        let mut close = false;
        let mut request = false;
        let mut open = None;

        egui::Window::new("Workspace Symbols")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 108.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 84.0])
            .default_width(660.0)
            .show(ctx, |ui| {
                let mut query_changed = false;
                ui.horizontal(|ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.workspace_symbol_query)
                            .hint_text("Search symbols")
                            .desired_width(400.0),
                    );
                    response.request_focus();
                    query_changed = response.changed();
                    if query_changed {
                        self.workspace_symbols_selected = 0;
                    }
                    if popup_button(ui, "Search", PopupButtonKind::Primary).clicked() {
                        request = true;
                    }
                    if popup_button(ui, "Close", PopupButtonKind::Secondary).clicked() {
                        close = true;
                    }
                });

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
                let render_symbol_count =
                    workspace_symbol_render_count(self.workspace_symbols.len());
                let viewport_height = ui.available_height();
                let selection_changed = ui.input(|input| {
                    handle_list_navigation_keys(
                        input,
                        &mut self.workspace_symbols_selected,
                        render_symbol_count,
                        selection_page_step(WORKSPACE_SYMBOL_ROW_HEIGHT, viewport_height),
                    )
                }) || query_changed;
                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    if workspace_symbol_enter_should_request(
                        &self.workspace_symbol_query,
                        &self.workspace_symbol_submitted_query,
                    ) {
                        request = true;
                    } else {
                        open = workspace_symbol_render_slice(
                            &self.workspace_symbols,
                            render_symbol_count,
                        )
                        .get(self.workspace_symbols_selected)
                        .cloned();
                    }
                }

                ui.horizontal(|ui| {
                    workspace_symbol_status_label(
                        ui,
                        workspace_symbol_submitted_query_label(
                            &self.workspace_symbol_submitted_query,
                        )
                        .into_display_text(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        workspace_symbol_status_label(
                            ui,
                            workspace_symbol_result_count_label(self.workspace_symbols.len()),
                        );
                    });
                });

                ui.separator();
                if let Some(symbol) = render_workspace_symbol_results(
                    ui,
                    workspace_symbol_render_slice(&self.workspace_symbols, render_symbol_count),
                    &mut self.workspace_symbols_selected,
                    selection_changed,
                ) {
                    open = Some(symbol);
                }
            });

        if close {
            self.workspace_symbols_open = false;
            self.workspace_symbols.clear();
            self.workspace_symbol_submitted_path = None;
            self.status = "Closed workspace symbols".to_owned();
        } else if request {
            self.request_lsp_workspace_symbols();
        } else if let Some(symbol) = open {
            self.open_workspace_symbol(symbol);
        }
    }
}

fn workspace_symbol_enter_should_request(current_query: &str, submitted_query: &str) -> bool {
    current_query.trim() != submitted_query
}

fn workspace_symbol_render_count(symbol_count: usize) -> usize {
    symbol_count.min(WORKSPACE_SYMBOL_RENDER_MAX_ROWS)
}

fn workspace_symbol_render_slice<T>(symbols: &[T], render_count: usize) -> &[T] {
    &symbols[..render_count.min(symbols.len())]
}

fn workspace_symbol_status_label(ui: &mut egui::Ui, text: Cow<'_, str>) {
    ui.label(
        RichText::new(text)
            .small()
            .color(ui.visuals().weak_text_color()),
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WorkspaceSymbolSubmittedQueryLabel<'a> {
    NoQuery,
    Query(Cow<'a, str>),
}

impl<'a> WorkspaceSymbolSubmittedQueryLabel<'a> {
    fn into_display_text(self) -> Cow<'a, str> {
        match self {
            Self::NoQuery => Cow::Borrowed(WORKSPACE_SYMBOL_NO_QUERY_LABEL),
            Self::Query(query) => {
                let mut label = String::with_capacity(
                    WORKSPACE_SYMBOL_QUERY_LABEL_PREFIX.len()
                        + query.len()
                        + WORKSPACE_SYMBOL_QUERY_LABEL_SUFFIX.len(),
                );
                label.push_str(WORKSPACE_SYMBOL_QUERY_LABEL_PREFIX);
                label.push_str(query.as_ref());
                label.push_str(WORKSPACE_SYMBOL_QUERY_LABEL_SUFFIX);
                Cow::Owned(label)
            }
        }
    }
}

fn workspace_symbol_submitted_query_label(query: &str) -> WorkspaceSymbolSubmittedQueryLabel<'_> {
    if query.is_empty() {
        return WorkspaceSymbolSubmittedQueryLabel::NoQuery;
    }

    WorkspaceSymbolSubmittedQueryLabel::Query(workspace_symbol_submitted_query_fragment(query))
}

fn workspace_symbol_submitted_query_fragment(query: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(
        query,
        WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS,
        WORKSPACE_SYMBOL_QUERY_LABEL_FALLBACK,
    )
}

fn workspace_symbol_result_count_label(count: usize) -> Cow<'static, str> {
    if let Some(label) = WORKSPACE_SYMBOL_RESULT_COUNT_LABELS.get(count) {
        return Cow::Borrowed(*label);
    }

    let mut label = String::with_capacity(decimal_digit_count(count) + " results".len());
    let _ = write!(label, "{count} results");
    Cow::Owned(label)
}

fn decimal_digit_count(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::{
        WORKSPACE_SYMBOL_QUERY_LABEL_FALLBACK, WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS,
        WORKSPACE_SYMBOL_RENDER_MAX_ROWS, WorkspaceSymbolSubmittedQueryLabel,
        workspace_symbol_enter_should_request, workspace_symbol_render_count,
        workspace_symbol_result_count_label, workspace_symbol_submitted_query_fragment,
        workspace_symbol_submitted_query_label,
    };
    use crate::path_display::sanitized_display_label;
    use std::borrow::Cow;

    #[test]
    fn workspace_symbol_enter_requests_when_query_is_cleared_after_submission() {
        assert!(workspace_symbol_enter_should_request("", "task"));
        assert!(workspace_symbol_enter_should_request("   ", "task"));
    }

    #[test]
    fn workspace_symbol_enter_opens_only_current_submitted_results() {
        assert!(!workspace_symbol_enter_should_request(" task ", "task"));
        assert!(workspace_symbol_enter_should_request("other", "task"));
    }

    #[test]
    fn workspace_symbol_enter_uses_raw_query_not_sanitized_display_label() {
        assert!(!workspace_symbol_enter_should_request(
            " find\nsymbol ",
            "find\nsymbol"
        ));
        assert!(workspace_symbol_enter_should_request(
            "find symbol",
            "find\nsymbol"
        ));
        assert!(workspace_symbol_enter_should_request(
            "findsymbol",
            "find\u{202e}symbol"
        ));
    }

    #[test]
    fn workspace_symbol_submitted_query_label_borrows_clean_ascii_and_unicode_query_fragments() {
        let ascii_fragment = workspace_symbol_submitted_query_fragment("find_task");
        let unicode_query = "find_\u{03bb}_symbol";
        let unicode_fragment = workspace_symbol_submitted_query_fragment(unicode_query);

        assert!(matches!(ascii_fragment, Cow::Borrowed("find_task")));
        match unicode_fragment {
            Cow::Borrowed(label) => assert_eq!(label, unicode_query),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }

        let label = workspace_symbol_submitted_query_label("find_task");
        assert_eq!(
            label.clone().into_display_text().as_ref(),
            "Query: `find_task`"
        );
        assert!(matches!(
            label,
            WorkspaceSymbolSubmittedQueryLabel::Query(Cow::Borrowed("find_task"))
        ));

        match workspace_symbol_submitted_query_label(unicode_query) {
            WorkspaceSymbolSubmittedQueryLabel::Query(Cow::Borrowed(label)) => {
                assert_eq!(label, unicode_query);
            }
            label => panic!("expected borrowed unicode query label, got {label:?}"),
        }
    }

    #[test]
    fn workspace_symbol_submitted_query_label_empty_query_returns_no_query() {
        let label = workspace_symbol_submitted_query_label("");

        assert!(matches!(
            &label,
            WorkspaceSymbolSubmittedQueryLabel::NoQuery
        ));
        assert!(matches!(
            label.into_display_text(),
            Cow::Borrowed("No query submitted")
        ));
    }

    #[test]
    fn workspace_symbol_submitted_query_label_sanitizes_display_text() {
        assert_eq!(
            workspace_symbol_submitted_query_label("find\n\t\u{202e}task")
                .into_display_text()
                .as_ref(),
            "Query: `find task`"
        );
        assert_eq!(
            workspace_symbol_submitted_query_label("\n\t\u{2066}")
                .into_display_text()
                .as_ref(),
            "Query: `<empty>`"
        );
        assert!(matches!(
            workspace_symbol_submitted_query_label("find\n\t\u{202e}task"),
            WorkspaceSymbolSubmittedQueryLabel::Query(Cow::Owned(_))
        ));
    }

    #[test]
    fn workspace_symbol_submitted_query_label_owns_dirty_truncated_and_fallback_query_fragments() {
        let dirty = workspace_symbol_submitted_query_fragment(" find\n\t\u{202e}task ");
        let fallback = workspace_symbol_submitted_query_fragment("\n\t\u{2066}");
        let long_query = "q".repeat(WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS + 1);
        let truncated = workspace_symbol_submitted_query_fragment(&long_query);

        assert_eq!(dirty.as_ref(), "find task");
        assert_eq!(fallback.as_ref(), WORKSPACE_SYMBOL_QUERY_LABEL_FALLBACK);
        assert!(truncated.contains("..."), "{truncated}");
        assert!(truncated.chars().count() <= WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS);
        assert!(matches!(dirty, Cow::Owned(_)));
        assert!(matches!(fallback, Cow::Owned(_)));
        assert!(matches!(truncated, Cow::Owned(_)));
    }

    #[test]
    fn workspace_symbol_submitted_query_label_query_fragments_match_sanitized_display_label() {
        let cases = [
            "find_task",
            "find_\u{03bb}_symbol",
            " find_task ",
            "find\n\t\u{202e}task",
            "\n\t\u{2066}",
            "\u{200b}\u{200c}\u{feff}",
        ];

        for query in cases {
            assert_eq!(
                workspace_symbol_submitted_query_fragment(query).as_ref(),
                sanitized_display_label(
                    query,
                    WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS,
                    WORKSPACE_SYMBOL_QUERY_LABEL_FALLBACK
                )
            );
        }

        let long_query = "q".repeat(WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS + 1);
        assert_eq!(
            workspace_symbol_submitted_query_fragment(&long_query).as_ref(),
            sanitized_display_label(
                &long_query,
                WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS,
                WORKSPACE_SYMBOL_QUERY_LABEL_FALLBACK
            )
        );
    }

    #[test]
    fn workspace_symbol_submitted_query_label_bounds_display_text() {
        let long_query = "q".repeat(400);
        let label = workspace_symbol_submitted_query_label(&long_query).into_display_text();
        let query = label
            .strip_prefix("Query: `")
            .and_then(|label| label.strip_suffix('`'))
            .expect("workspace query label should keep query wrapper");

        assert_eq!(
            query.chars().count(),
            WORKSPACE_SYMBOL_QUERY_LABEL_MAX_CHARS
        );
        assert!(query.contains("..."));
    }

    #[test]
    fn workspace_symbol_result_count_label_reuses_common_counts() {
        assert!(matches!(
            workspace_symbol_result_count_label(0),
            Cow::Borrowed("0 results")
        ));
        assert!(matches!(
            workspace_symbol_result_count_label(1),
            Cow::Borrowed("1 result")
        ));
        assert!(matches!(
            workspace_symbol_result_count_label(16),
            Cow::Borrowed("16 results")
        ));
    }

    #[test]
    fn workspace_symbol_result_count_label_formats_large_counts() {
        assert_eq!(
            workspace_symbol_result_count_label(42).as_ref(),
            "42 results"
        );
        assert!(matches!(
            workspace_symbol_result_count_label(42),
            Cow::Owned(_)
        ));
    }

    #[test]
    fn workspace_symbol_render_count_caps_prepared_rows() {
        assert_eq!(workspace_symbol_render_count(42), 42);
        assert_eq!(
            workspace_symbol_render_count(WORKSPACE_SYMBOL_RENDER_MAX_ROWS + 1),
            WORKSPACE_SYMBOL_RENDER_MAX_ROWS
        );
    }
}

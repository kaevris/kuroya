#[cfg(test)]
use crate::path_display::sanitized_display_label_cow;
use crate::{
    KuroyaApp,
    buffer_find_history::{
        BufferFindHistoryDirection, MAX_BUFFER_FIND_HISTORY, apply_buffer_find_history_navigation,
        buffer_find_history_enabled, record_buffer_find_query_history,
        record_buffer_find_replacement_history,
    },
    popup_buttons::{PopupButtonKind, popup_button, popup_compact_button},
    ui_switch::ui_switch_with_label,
};
use eframe::egui::{self, Color32, Context, Id, Key, RichText, TextEdit};
use kuroya_core::validate_find_regex;
#[cfg(test)]
use std::borrow::Cow;
use std::sync::Arc;

pub(crate) const BUFFER_FIND_WINDOW_TOP_OFFSET: f32 = 78.0;
pub(crate) const BUFFER_FIND_WINDOW_HEIGHT: f32 = 156.0;

const BUFFER_FIND_INPUT_WIDTH: f32 = 260.0;
#[cfg(test)]
const BUFFER_FIND_STATUS_LABEL_MAX_CHARS: usize = 48;
const BUFFER_FIND_PANEL_DISPLAY_CACHE_ID: &str = "kuroya.buffer_find_panel.display_cache";

impl KuroyaApp {
    pub(crate) fn render_buffer_find(&mut self, ctx: &Context) {
        let mut close = false;
        let mut next = false;
        let mut previous = false;
        let mut replace = false;
        let mut replace_all = false;
        let mut query_changed = false;
        let mut options_changed = false;

        egui::Window::new("Find")
            .max_size(crate::layout::popup_window_max_size(ctx))
            .collapsible(false)
            .resizable(false)
            .anchor(
                egui::Align2::RIGHT_TOP,
                [-24.0, BUFFER_FIND_WINDOW_TOP_OFFSET],
            )
            .fixed_size([460.0, BUFFER_FIND_WINDOW_HEIGHT])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let response = ui.add(buffer_find_text_edit(
                        &mut self.buffer_find_query,
                        "Find in file",
                    ));
                    response.request_focus();
                    query_changed = response.changed();
                    if query_changed {
                        self.buffer_find_query_history_cursor = None;
                        self.buffer_find_query_history_draft = None;
                    } else if response.has_focus()
                        && buffer_find_history_enabled(self.settings.find_history)
                    {
                        if ui.input(|input| input.key_pressed(Key::ArrowUp)) {
                            query_changed |= apply_buffer_find_history_navigation(
                                &mut self.buffer_find_query,
                                &self.buffer_find_query_history,
                                &mut self.buffer_find_query_history_cursor,
                                &mut self.buffer_find_query_history_draft,
                                BufferFindHistoryDirection::Older,
                            );
                        } else if ui.input(|input| input.key_pressed(Key::ArrowDown)) {
                            query_changed |= apply_buffer_find_history_navigation(
                                &mut self.buffer_find_query,
                                &self.buffer_find_query_history,
                                &mut self.buffer_find_query_history_cursor,
                                &mut self.buffer_find_query_history_draft,
                                BufferFindHistoryDirection::Newer,
                            );
                        }
                    }
                    if ui.input(|input| input.key_pressed(Key::Enter)) {
                        next = true;
                    }
                    if ui.input(|input| input.key_pressed(Key::Escape)) {
                        close = true;
                    }

                    if popup_compact_button(ui, "Prev", PopupButtonKind::Secondary).clicked() {
                        previous = true;
                    }
                    if popup_compact_button(ui, "Next", PopupButtonKind::Primary).clicked() {
                        next = true;
                    }
                    if popup_compact_button(ui, "Close", PopupButtonKind::Secondary).clicked() {
                        close = true;
                    }
                });
                ui.horizontal(|ui| {
                    let response = ui.add(buffer_find_text_edit(
                        &mut self.buffer_find_replacement,
                        "Replace",
                    ));
                    let replacement_changed = response.changed();
                    if replacement_changed {
                        self.buffer_find_replacement_history_cursor = None;
                        self.buffer_find_replacement_history_draft = None;
                    } else if response.has_focus()
                        && buffer_find_history_enabled(self.settings.find_replace_history)
                    {
                        if ui.input(|input| input.key_pressed(Key::ArrowUp)) {
                            let _ = apply_buffer_find_history_navigation(
                                &mut self.buffer_find_replacement,
                                &self.buffer_find_replacement_history,
                                &mut self.buffer_find_replacement_history_cursor,
                                &mut self.buffer_find_replacement_history_draft,
                                BufferFindHistoryDirection::Older,
                            );
                        } else if ui.input(|input| input.key_pressed(Key::ArrowDown)) {
                            let _ = apply_buffer_find_history_navigation(
                                &mut self.buffer_find_replacement,
                                &self.buffer_find_replacement_history,
                                &mut self.buffer_find_replacement_history_cursor,
                                &mut self.buffer_find_replacement_history_draft,
                                BufferFindHistoryDirection::Newer,
                            );
                        }
                    }
                    if popup_button(ui, "Replace", PopupButtonKind::Primary).clicked() {
                        replace = true;
                    }
                    if popup_compact_button(ui, "All", PopupButtonKind::Secondary)
                        .on_hover_text("Replace all")
                        .clicked()
                    {
                        replace_all = true;
                    }
                });
                ui.horizontal(|ui| {
                    options_changed |=
                        ui_switch_with_label(ui, &mut self.buffer_find_case_sensitive, "Case")
                            .on_hover_text("Match case")
                            .changed();
                    options_changed |=
                        ui_switch_with_label(ui, &mut self.buffer_find_whole_word, "Word")
                            .on_hover_text("Match whole word")
                            .changed();
                    options_changed |=
                        ui_switch_with_label(ui, &mut self.buffer_find_regex, "Regex")
                            .on_hover_text("Use regular expression")
                            .changed();
                    options_changed |=
                        ui_switch_with_label(ui, &mut self.buffer_find_preserve_case, "Preserve")
                            .on_hover_text("Preserve case while replacing")
                            .changed();

                    let large_file_find_blocked = self.active_find_blocked_by_large_file_mode();
                    let query_too_large =
                        crate::buffer_find::buffer_find_query_too_large(&self.buffer_find_query);
                    let count = if large_file_find_blocked || query_too_large {
                        0
                    } else {
                        self.active_find_match_count()
                    };
                    let status = cached_buffer_find_status_display(
                        ui.ctx(),
                        self.buffer_find_regex,
                        &self.buffer_find_query,
                        self.buffer_find_case_sensitive,
                        large_file_find_blocked,
                        query_too_large,
                        self.buffer_find_match,
                        count,
                        BufferFindStatusColors {
                            warning: ui.visuals().warn_fg_color,
                            error: ui.visuals().error_fg_color,
                            muted: ui.visuals().weak_text_color(),
                        },
                    );
                    ui.label(
                        RichText::new(status.label.as_ref())
                            .small()
                            .color(status.color),
                    );
                });
            });

        let mut close_after_result = false;
        if next || previous || replace || replace_all {
            self.record_buffer_find_query_history_if_enabled();
        }
        if replace || replace_all {
            self.record_buffer_find_replacement_history_if_enabled();
        }
        if close {
            self.buffer_find_open = false;
            self.buffer_find_scope = None;
        } else if query_changed || options_changed {
            self.buffer_find_match = 0;
            if crate::buffer_find::live_find_query_should_move_cursor(
                self.settings.find_on_type,
                self.settings.find_cursor_move_on_type,
            ) {
                close_after_result = self.select_find_match_with_result();
            } else {
                self.update_find_match_count_status();
            }
        } else if replace_all {
            self.replace_all_find_matches();
        } else if replace {
            self.replace_current_find_match();
        } else if previous {
            close_after_result = self.goto_find_match_with_result(-1);
        } else if next {
            close_after_result = self.goto_find_match_with_result(1);
        }

        if close_after_result && self.settings.find_close_on_result {
            self.buffer_find_open = false;
            self.buffer_find_scope = None;
        }
    }

    fn record_buffer_find_query_history_if_enabled(&mut self) {
        if !buffer_find_history_enabled(self.settings.find_history) {
            return;
        }
        if record_buffer_find_query_history(
            &mut self.buffer_find_query_history,
            &self.buffer_find_query,
            MAX_BUFFER_FIND_HISTORY,
        ) {
            self.buffer_find_query_history_cursor = None;
            self.buffer_find_query_history_draft = None;
        }
    }

    fn record_buffer_find_replacement_history_if_enabled(&mut self) {
        if !buffer_find_history_enabled(self.settings.find_replace_history) {
            return;
        }
        if record_buffer_find_replacement_history(
            &mut self.buffer_find_replacement_history,
            &self.buffer_find_replacement,
            MAX_BUFFER_FIND_HISTORY,
        ) {
            self.buffer_find_replacement_history_cursor = None;
            self.buffer_find_replacement_history_draft = None;
        }
    }
}

pub(crate) fn buffer_find_extra_top_space(find_open: bool, add_extra_space_on_top: bool) -> usize {
    if find_open && add_extra_space_on_top {
        BUFFER_FIND_WINDOW_HEIGHT.ceil() as usize
    } else {
        0
    }
}

fn buffer_find_text_edit<'a>(text: &'a mut String, hint_text: &'static str) -> TextEdit<'a> {
    TextEdit::singleline(text)
        .hint_text(hint_text)
        .desired_width(BUFFER_FIND_INPUT_WIDTH)
        .clip_text(true)
}

fn cached_buffer_find_status_display(
    ctx: &Context,
    regex_enabled: bool,
    query: &str,
    case_sensitive: bool,
    large_file_find_blocked: bool,
    query_too_large: bool,
    match_index: usize,
    count: usize,
    colors: BufferFindStatusColors,
) -> Arc<BufferFindStatusDisplay> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<BufferFindPanelDisplayCache>(Id::new(
            BUFFER_FIND_PANEL_DISPLAY_CACHE_ID,
        ))
        .status_display(
            regex_enabled,
            query,
            case_sensitive,
            large_file_find_blocked,
            query_too_large,
            match_index,
            count,
            colors,
        )
    })
}

#[derive(Clone, Default)]
struct BufferFindPanelDisplayCache {
    regex_invalid: BufferFindRegexInvalidCache,
    status: BufferFindStatusDisplayCache,
}

impl BufferFindPanelDisplayCache {
    fn status_display(
        &mut self,
        regex_enabled: bool,
        query: &str,
        case_sensitive: bool,
        large_file_find_blocked: bool,
        query_too_large: bool,
        match_index: usize,
        count: usize,
        colors: BufferFindStatusColors,
    ) -> Arc<BufferFindStatusDisplay> {
        if large_file_find_blocked || query_too_large {
            self.regex_invalid.clear();
        }
        let invalid_regex = !large_file_find_blocked
            && !query_too_large
            && self
                .regex_invalid
                .is_invalid(regex_enabled, query, case_sensitive);
        let state = buffer_find_status_state(
            large_file_find_blocked,
            query_too_large,
            invalid_regex,
            match_index,
            count,
        );
        self.status.display_for(state, colors)
    }
}

#[derive(Clone, Default)]
struct BufferFindRegexInvalidCache {
    valid: bool,
    normalized_query: String,
    case_sensitive: bool,
    invalid: bool,
}

impl BufferFindRegexInvalidCache {
    fn is_invalid(&mut self, regex_enabled: bool, query: &str, case_sensitive: bool) -> bool {
        if !regex_enabled {
            self.clear();
            return false;
        }

        let normalized_query = buffer_find_normalized_regex_query(query);
        if self.valid
            && self.normalized_query == normalized_query
            && self.case_sensitive == case_sensitive
        {
            return self.invalid;
        }

        self.valid = true;
        self.normalized_query.clear();
        self.normalized_query.push_str(normalized_query);
        self.case_sensitive = case_sensitive;
        self.invalid = buffer_find_normalized_regex_invalid(normalized_query, case_sensitive);
        self.invalid
    }

    fn clear(&mut self) {
        self.valid = false;
        self.normalized_query.clear();
        self.invalid = false;
    }
}

#[derive(Clone, Default)]
struct BufferFindStatusDisplayCache {
    state: Option<BufferFindStatusState>,
    colors: Option<BufferFindStatusColors>,
    display: Option<Arc<BufferFindStatusDisplay>>,
}

impl BufferFindStatusDisplayCache {
    fn display_for(
        &mut self,
        state: BufferFindStatusState,
        colors: BufferFindStatusColors,
    ) -> Arc<BufferFindStatusDisplay> {
        if self.state != Some(state) || self.colors != Some(colors) {
            self.state = Some(state);
            self.colors = Some(colors);
            self.display = Some(Arc::new(buffer_find_status_display_for_state(
                state, colors,
            )));
        }

        Arc::clone(
            self.display
                .as_ref()
                .expect("buffer find status display should be populated"),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BufferFindStatusState {
    LargeFileMode,
    QueryTooLong,
    InvalidRegex,
    Matches { current: usize, count: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BufferFindStatusColors {
    warning: Color32,
    error: Color32,
    muted: Color32,
}

#[derive(Clone, Debug)]
struct BufferFindStatusDisplay {
    label: Arc<str>,
    color: Color32,
}

fn buffer_find_status_state(
    large_file_find_blocked: bool,
    query_too_large: bool,
    invalid_regex: bool,
    match_index: usize,
    count: usize,
) -> BufferFindStatusState {
    if large_file_find_blocked {
        BufferFindStatusState::LargeFileMode
    } else if query_too_large {
        BufferFindStatusState::QueryTooLong
    } else if invalid_regex {
        BufferFindStatusState::InvalidRegex
    } else {
        BufferFindStatusState::Matches {
            current: buffer_find_current_match_label(match_index, count),
            count,
        }
    }
}

fn buffer_find_status_display_for_state(
    state: BufferFindStatusState,
    colors: BufferFindStatusColors,
) -> BufferFindStatusDisplay {
    match state {
        BufferFindStatusState::LargeFileMode => BufferFindStatusDisplay {
            label: Arc::from("Large file mode"),
            color: colors.warning,
        },
        BufferFindStatusState::QueryTooLong => BufferFindStatusDisplay {
            label: Arc::from("Query too long"),
            color: colors.warning,
        },
        BufferFindStatusState::InvalidRegex => BufferFindStatusDisplay {
            label: Arc::from("Invalid regex"),
            color: colors.error,
        },
        BufferFindStatusState::Matches { current, count } => BufferFindStatusDisplay {
            label: Arc::from(format!("{current} / {count}")),
            color: colors.muted,
        },
    }
}

fn buffer_find_current_match_label(match_index: usize, count: usize) -> usize {
    if count == 0 {
        0
    } else {
        match_index.min(count - 1) + 1
    }
}

#[cfg(test)]
fn buffer_find_status_label(label: &str) -> String {
    buffer_find_status_label_cow(label).into_owned()
}

#[cfg(test)]
fn buffer_find_status_label_cow(label: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(label, BUFFER_FIND_STATUS_LABEL_MAX_CHARS, "0 / 0")
}

#[cfg(test)]
fn buffer_find_regex_invalid(regex_enabled: bool, query: &str, case_sensitive: bool) -> bool {
    if !regex_enabled {
        return false;
    }
    if crate::buffer_find::buffer_find_query_too_large(query) {
        return true;
    }

    buffer_find_normalized_regex_invalid(buffer_find_normalized_regex_query(query), case_sensitive)
}

fn buffer_find_normalized_regex_query(query: &str) -> &str {
    query.trim()
}

fn buffer_find_normalized_regex_invalid(query: &str, case_sensitive: bool) -> bool {
    !query.is_empty() && validate_find_regex(query, case_sensitive).is_err()
}

#[cfg(test)]
mod tests {
    use super::{
        BUFFER_FIND_STATUS_LABEL_MAX_CHARS, BUFFER_FIND_WINDOW_HEIGHT, BufferFindPanelDisplayCache,
        BufferFindRegexInvalidCache, BufferFindStatusColors, BufferFindStatusState,
        buffer_find_extra_top_space, buffer_find_regex_invalid,
        buffer_find_status_display_for_state, buffer_find_status_label,
        buffer_find_status_label_cow,
    };
    use eframe::egui::Color32;
    use std::{borrow::Cow, sync::Arc};

    fn test_status_colors() -> BufferFindStatusColors {
        BufferFindStatusColors {
            warning: Color32::from_rgb(242, 178, 90),
            error: Color32::from_rgb(220, 76, 70),
            muted: Color32::from_rgb(126, 136, 150),
        }
    }

    #[test]
    fn regex_validation_does_not_require_a_buffer_search() {
        assert!(!buffer_find_regex_invalid(false, "(", true));
        assert!(!buffer_find_regex_invalid(true, "", true));
        assert!(!buffer_find_regex_invalid(true, "item-\\d+", false));
        assert!(buffer_find_regex_invalid(true, "(", true));
    }

    #[test]
    fn find_extra_top_space_follows_open_panel_and_setting() {
        assert_eq!(buffer_find_extra_top_space(false, true), 0);
        assert_eq!(buffer_find_extra_top_space(true, false), 0);
        assert_eq!(
            buffer_find_extra_top_space(true, true),
            BUFFER_FIND_WINDOW_HEIGHT.ceil() as usize
        );
    }

    #[test]
    fn find_status_display_reports_panel_states() {
        assert_eq!(
            buffer_find_status_display_for_state(
                BufferFindStatusState::LargeFileMode,
                test_status_colors()
            )
            .label
            .as_ref(),
            "Large file mode"
        );
        assert_eq!(
            buffer_find_status_display_for_state(
                BufferFindStatusState::QueryTooLong,
                test_status_colors()
            )
            .label
            .as_ref(),
            "Query too long"
        );
        assert_eq!(
            buffer_find_status_display_for_state(
                BufferFindStatusState::InvalidRegex,
                test_status_colors()
            )
            .label
            .as_ref(),
            "Invalid regex"
        );
        assert_eq!(
            buffer_find_status_display_for_state(
                BufferFindStatusState::Matches {
                    current: 0,
                    count: 0
                },
                test_status_colors()
            )
            .label
            .as_ref(),
            "0 / 0"
        );
        assert_eq!(
            buffer_find_status_display_for_state(
                BufferFindStatusState::Matches {
                    current: 4,
                    count: 4
                },
                test_status_colors()
            )
            .label
            .as_ref(),
            "4 / 4"
        );
    }

    #[test]
    fn find_status_display_sanitizes_and_bounds_label_text() {
        let label = buffer_find_status_label(&format!(
            "  searching\n{}{}\u{202e}done  ",
            "x".repeat(BUFFER_FIND_STATUS_LABEL_MAX_CHARS),
            "\u{0000}",
        ));

        assert!(label.chars().count() <= BUFFER_FIND_STATUS_LABEL_MAX_CHARS);
        assert!(!label.chars().any(char::is_control));
        assert!(!label.contains('\u{202e}'));
        assert!(label.starts_with("searching "));
        assert_eq!(buffer_find_status_label("\n\t\u{0}\u{202e}"), "0 / 0");
    }

    #[test]
    fn find_status_label_cow_borrows_clean_ascii_and_unicode_labels() {
        assert!(matches!(
            buffer_find_status_label_cow("12 / 42"),
            Cow::Borrowed("12 / 42")
        ));

        let unicode = "result-\u{03bb} / 42";
        match buffer_find_status_label_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn find_status_label_cow_owns_dirty_truncated_and_fallback_labels() {
        let dirty = buffer_find_status_label_cow("searching\n\u{202e}done");
        assert_eq!(dirty.as_ref(), "searching done");
        assert!(matches!(dirty, Cow::Owned(_)));

        let long = format!(
            "start-{}-finish",
            "x".repeat(BUFFER_FIND_STATUS_LABEL_MAX_CHARS * 2)
        );
        let truncated = buffer_find_status_label_cow(&long);
        assert!(truncated.as_ref().starts_with("start-"));
        assert!(truncated.as_ref().contains("..."));
        assert!(truncated.as_ref().ends_with("-finish"));
        assert_eq!(
            truncated.as_ref().chars().count(),
            BUFFER_FIND_STATUS_LABEL_MAX_CHARS
        );
        assert!(matches!(truncated, Cow::Owned(_)));

        let fallback = buffer_find_status_label_cow("\n\t\u{0}\u{202e}");
        assert_eq!(fallback.as_ref(), "0 / 0");
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[test]
    fn find_status_label_matches_cow_helper() {
        for value in [
            "12 / 42",
            "result-\u{03bb} / 42",
            "  12 / 42  ",
            "searching\n\u{202e}done",
            "\n\t\u{0}\u{202e}",
        ] {
            assert_eq!(
                buffer_find_status_label(value),
                buffer_find_status_label_cow(value).into_owned()
            );
        }

        let long = format!(
            "start-{}-finish",
            "x".repeat(BUFFER_FIND_STATUS_LABEL_MAX_CHARS * 2)
        );
        assert_eq!(
            buffer_find_status_label(&long),
            buffer_find_status_label_cow(&long).into_owned()
        );
    }

    #[test]
    fn find_status_display_cache_reuses_unchanged_display() {
        let mut cache = BufferFindPanelDisplayCache::default();
        let colors = test_status_colors();
        let first = cache.status_display(false, "needle", true, false, false, 2, 4, colors);
        let second = cache.status_display(
            false,
            "different raw query",
            false,
            false,
            false,
            2,
            4,
            colors,
        );

        assert!(Arc::ptr_eq(&first, &second));

        let changed = cache.status_display(
            false,
            "different raw query",
            false,
            false,
            false,
            3,
            4,
            colors,
        );
        assert!(!Arc::ptr_eq(&first, &changed));
        assert_eq!(changed.label.as_ref(), "4 / 4");
    }

    #[test]
    fn find_status_display_cache_updates_when_theme_colors_change() {
        let mut cache = BufferFindPanelDisplayCache::default();
        let first_colors = test_status_colors();
        let second_colors = BufferFindStatusColors {
            muted: Color32::from_rgb(20, 40, 60),
            ..first_colors
        };
        let first = cache.status_display(false, "needle", true, false, false, 2, 4, first_colors);
        let second = cache.status_display(false, "needle", true, false, false, 2, 4, second_colors);

        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second.label.as_ref(), "3 / 4");
        assert_eq!(second.color, second_colors.muted);
    }

    #[test]
    fn find_status_display_reports_oversized_query_without_regex_validation() {
        let mut cache = BufferFindPanelDisplayCache::default();
        let status = cache.status_display(true, "(", true, false, true, 0, 0, test_status_colors());

        assert_eq!(status.label.as_ref(), "Query too long");
    }

    #[test]
    fn regex_status_cache_uses_raw_query_without_rewriting_it() {
        let mut cache = BufferFindPanelDisplayCache::default();
        let query = format!("  ({}{}\n  ", "x".repeat(32), "\u{202e}");
        let original = query.clone();

        let status =
            cache.status_display(true, &query, true, false, false, 0, 0, test_status_colors());

        assert_eq!(query, original);
        assert_eq!(status.label.as_ref(), "Invalid regex");
    }

    #[test]
    fn regex_status_cache_keys_validation_by_normalized_query() {
        let mut cache = BufferFindRegexInvalidCache::default();

        assert!(cache.is_invalid(true, "  (\n", true));
        assert_eq!(cache.normalized_query, "(");

        assert!(cache.is_invalid(true, "\t(  ", true));
        assert_eq!(cache.normalized_query, "(");
    }
}

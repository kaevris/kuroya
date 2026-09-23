use crate::{
    KuroyaApp,
    project_search_state::{
        MAX_PROJECT_SEARCH_QUERY_CHARS, MAX_PROJECT_SEARCH_RECENT_QUERIES, ProjectSearchQuery,
        project_search_recent_label,
    },
    ui_switch::ui_switch_with_label,
};
use eframe::egui::{self, Align, Layout, TextEdit};

const MAX_PROJECT_SEARCH_GLOB_DRAFT_CHARS: usize = 4096;
const PROJECT_SEARCH_CONTROL_INPUT_SCAN_MULTIPLIER: usize = 4;

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct ProjectSearchControlState {
    pub search_requested: bool,
    pub input_has_focus: bool,
}

pub(super) fn render_project_search_controls(
    app: &mut KuroyaApp,
    ui: &mut egui::Ui,
) -> ProjectSearchControlState {
    let mut state = ProjectSearchControlState::default();
    let mut controls_changed = false;
    let response = ui.add(
        TextEdit::singleline(&mut app.project_search_query)
            .hint_text("Search text")
            .desired_width(f32::INFINITY),
    );
    if app.project_search_focus_query {
        response.request_focus();
        app.project_search_focus_query = false;
    }
    state.input_has_focus |= response.has_focus();
    if response.changed() {
        sanitize_project_search_query_input(&mut app.project_search_query);
        controls_changed = true;
    }

    ui.horizontal(|ui| {
        if ui_switch_with_label(ui, &mut app.project_search_case_sensitive, "Case")
            .on_hover_text("Match case")
            .changed()
        {
            controls_changed = true;
        }
        if ui_switch_with_label(ui, &mut app.project_search_whole_word, "Word")
            .on_hover_text("Match whole word")
            .changed()
        {
            controls_changed = true;
        }
        if ui_switch_with_label(ui, &mut app.project_search_regex, "Regex")
            .on_hover_text("Interpret the query as a regular expression")
            .changed()
        {
            controls_changed = true;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if render_project_search_recent(app, ui) {
                controls_changed = true;
            }
        });
    });

    ui.columns(2, |columns| {
        let include_response = columns[0].add(
            TextEdit::singleline(&mut app.project_search_include)
                .hint_text("Include files")
                .desired_width(f32::INFINITY),
        );
        let exclude_response = columns[1].add(
            TextEdit::singleline(&mut app.project_search_exclude)
                .hint_text("Exclude files")
                .desired_width(f32::INFINITY),
        );
        state.input_has_focus |= include_response.has_focus() || exclude_response.has_focus();
        if include_response.changed() {
            sanitize_project_search_glob_input(&mut app.project_search_include);
            controls_changed = true;
        }
        if exclude_response.changed() {
            sanitize_project_search_glob_input(&mut app.project_search_exclude);
            controls_changed = true;
        }
    });

    if controls_changed {
        mark_project_search_controls_changed(app);
        state.search_requested = true;
    }
    state
}

pub(super) fn sanitize_project_search_inputs(app: &mut KuroyaApp) {
    let query_sanitized = sanitize_project_search_query_input(&mut app.project_search_query);
    let include_sanitized = sanitize_project_search_glob_input(&mut app.project_search_include);
    let exclude_sanitized = sanitize_project_search_glob_input(&mut app.project_search_exclude);
    if query_sanitized || include_sanitized || exclude_sanitized {
        app.project_search_selected = 0;
    }
}

fn render_project_search_recent(app: &mut KuroyaApp, ui: &mut egui::Ui) -> bool {
    if app.project_search_recent.is_empty() {
        return false;
    }

    let mut selected_recent_index = None;
    egui::ComboBox::from_id_salt("project_search_recent")
        .selected_text("Recent")
        .show_ui(ui, |ui| {
            for (index, entry) in app
                .project_search_recent
                .iter()
                .take(MAX_PROJECT_SEARCH_RECENT_QUERIES)
                .enumerate()
            {
                let label = project_search_recent_label(entry);
                if ui.selectable_label(false, label).clicked() {
                    selected_recent_index = Some(index);
                    ui.close();
                }
            }
        });
    if let Some(entry) = selected_recent_index
        .and_then(|index| app.project_search_recent.get(index))
        .cloned()
    {
        apply_project_search_query(app, entry);
        return true;
    }
    false
}

fn apply_project_search_query(app: &mut KuroyaApp, entry: ProjectSearchQuery) {
    let ProjectSearchQuery {
        mut query,
        case_sensitive,
        whole_word,
        regex,
        mut include,
        mut exclude,
    } = entry;
    sanitize_project_search_query_input(&mut query);
    sanitize_project_search_glob_input(&mut include);
    sanitize_project_search_glob_input(&mut exclude);

    app.project_search_query = query;
    app.project_search_case_sensitive = case_sensitive;
    app.project_search_whole_word = whole_word;
    app.project_search_regex = regex;
    app.project_search_include = include;
    app.project_search_exclude = exclude;
}

fn mark_project_search_controls_changed(app: &mut KuroyaApp) {
    app.project_search_selected = 0;
    app.invalidate_project_search_requests();
}

fn sanitize_project_search_query_input(value: &mut String) -> bool {
    sanitize_single_line_input(
        value,
        MAX_PROJECT_SEARCH_QUERY_CHARS,
        ControlReplacement::Space,
    )
}

fn sanitize_project_search_glob_input(value: &mut String) -> bool {
    sanitize_single_line_input(
        value,
        MAX_PROJECT_SEARCH_GLOB_DRAFT_CHARS,
        ControlReplacement::GlobSeparator,
    )
}

#[derive(Debug, Clone, Copy)]
enum ControlReplacement {
    Space,
    GlobSeparator,
}

fn sanitize_single_line_input(
    value: &mut String,
    max_chars: usize,
    replacement: ControlReplacement,
) -> bool {
    if value.len() <= max_chars
        && value.is_ascii()
        && !value.as_bytes().iter().any(u8::is_ascii_control)
    {
        return false;
    }

    let mut sanitize_from = None;
    for (input_chars, (byte_idx, ch)) in value.char_indices().enumerate() {
        if input_chars >= max_chars || project_search_input_char_needs_sanitizing(ch) {
            sanitize_from = Some((byte_idx, input_chars));
            break;
        }
    }
    let Some((sanitize_byte, sanitize_chars)) = sanitize_from else {
        return false;
    };

    let mut sanitized = String::with_capacity(value.len().min(max_chars));
    sanitized.push_str(&value[..sanitize_byte]);
    let mut char_count = sanitize_chars;
    let mut skip_spaces_after_replacement = false;

    for (scanned_chars, ch) in value[sanitize_byte..].chars().enumerate() {
        let scanned_chars = scanned_chars + sanitize_chars;
        if scanned_chars >= project_search_control_input_scan_chars(max_chars) {
            break;
        }
        if is_project_search_format_control(ch) {
            continue;
        }

        if char_count >= max_chars {
            break;
        }

        if is_project_search_line_or_control(ch) {
            skip_spaces_after_replacement = true;
            match replacement {
                ControlReplacement::Space => {
                    push_limited_space(&mut sanitized, &mut char_count, max_chars);
                }
                ControlReplacement::GlobSeparator => {
                    push_limited_glob_separator(&mut sanitized, &mut char_count, max_chars);
                }
            }
            continue;
        }

        if skip_spaces_after_replacement && ch == ' ' {
            continue;
        }

        skip_spaces_after_replacement = false;
        sanitized.push(ch);
        char_count += 1;
    }

    *value = sanitized;
    true
}

fn project_search_control_input_scan_chars(max_chars: usize) -> usize {
    max_chars
        .saturating_mul(PROJECT_SEARCH_CONTROL_INPUT_SCAN_MULTIPLIER)
        .max(max_chars)
}

fn project_search_input_char_needs_sanitizing(ch: char) -> bool {
    is_project_search_format_control(ch) || is_project_search_line_or_control(ch)
}

fn is_project_search_line_or_control(ch: char) -> bool {
    ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}')
}

fn is_project_search_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn push_limited_space(output: &mut String, char_count: &mut usize, max_chars: usize) {
    if *char_count >= max_chars || output.is_empty() || output.ends_with(' ') {
        return;
    }
    output.push(' ');
    *char_count += 1;
}

fn push_limited_glob_separator(output: &mut String, char_count: &mut usize, max_chars: usize) {
    if *char_count >= max_chars || output.is_empty() || output.ends_with(", ") {
        return;
    }
    if !output.ends_with(',') {
        output.push(',');
        *char_count += 1;
    }
    if *char_count < max_chars && !output.ends_with(' ') {
        output.push(' ');
        *char_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_PROJECT_SEARCH_GLOB_DRAFT_CHARS, MAX_PROJECT_SEARCH_QUERY_CHARS,
        apply_project_search_query, mark_project_search_controls_changed,
        project_search_control_input_scan_chars, sanitize_project_search_glob_input,
        sanitize_project_search_query_input,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext,
        project_search_state::ProjectSearchQuery, terminal::TerminalPane,
        ui_event_channel::ui_event_channel,
    };
    use kuroya_core::{EditorSettings, Workspace};
    use std::{path::PathBuf, sync::atomic::Ordering, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn project_search_query_input_without_control_chars_is_left_untouched() {
        let mut value = "needle in source".to_owned();

        let changed = sanitize_project_search_query_input(&mut value);

        assert!(!changed);
        assert_eq!(value, "needle in source");
    }

    #[test]
    fn project_search_query_input_is_single_line_and_bounded() {
        let mut value = format!(
            "needle\nother\t{}",
            "x".repeat(MAX_PROJECT_SEARCH_QUERY_CHARS)
        );

        sanitize_project_search_query_input(&mut value);

        assert!(!value.contains('\n'));
        assert!(!value.contains('\t'));
        assert!(value.chars().count() <= MAX_PROJECT_SEARCH_QUERY_CHARS);
        assert!(value.starts_with("needle other"));
    }

    #[test]
    fn project_search_query_input_strips_hidden_format_controls() {
        let mut value = "needle\u{202e}\u{2066}\u{200f}\nvalue".to_owned();

        sanitize_project_search_query_input(&mut value);

        assert_eq!(value, "needle value");
    }

    #[test]
    fn project_search_query_input_bounds_hidden_control_only_prefixes() {
        let mut value = "\u{202e}".repeat(project_search_control_input_scan_chars(
            MAX_PROJECT_SEARCH_QUERY_CHARS,
        ));
        value.push_str("needle");

        let changed = sanitize_project_search_query_input(&mut value);

        assert!(changed);
        assert!(value.is_empty());
    }

    #[test]
    fn project_search_query_input_collapses_indentation_after_pasted_lines() {
        let mut value = "needle\n  other\t  value".to_owned();

        sanitize_project_search_query_input(&mut value);

        assert_eq!(value, "needle other value");
    }

    #[test]
    fn project_search_query_input_preserves_clean_unicode_prefix_before_sanitizing_tail() {
        let mut value = "føø needle\n  βeta\t  value".to_owned();

        let changed = sanitize_project_search_query_input(&mut value);

        assert!(changed);
        assert_eq!(value, "føø needle βeta value");
    }

    #[test]
    fn project_search_glob_input_preserves_pasted_line_boundaries_as_separators() {
        let mut value = "src/**/*.rs\r\ntests/**/*.rs\u{0000}target/**".to_owned();

        sanitize_project_search_glob_input(&mut value);

        assert_eq!(value, "src/**/*.rs, tests/**/*.rs, target/**");
    }

    #[test]
    fn project_search_glob_input_skips_indentation_after_pasted_lines() {
        let mut value = "src/**/*.rs\n  tests/**/*.rs\r\n  target/**".to_owned();

        sanitize_project_search_glob_input(&mut value);

        assert_eq!(value, "src/**/*.rs, tests/**/*.rs, target/**");
    }

    #[test]
    fn project_search_glob_input_treats_unicode_line_separators_as_separators() {
        let mut value =
            "src/**/*.rs\u{2028}tests/**/*.rs\u{2029}\u{202e}\u{2069}target/**".to_owned();

        sanitize_project_search_glob_input(&mut value);

        assert_eq!(value, "src/**/*.rs, tests/**/*.rs, target/**");
        assert!(!value.contains('\u{2028}'));
        assert!(!value.contains('\u{2029}'));
        assert!(!value.contains('\u{202e}'));
        assert!(!value.contains('\u{2069}'));
    }

    #[test]
    fn project_search_glob_input_is_bounded() {
        let mut value = "x".repeat(MAX_PROJECT_SEARCH_GLOB_DRAFT_CHARS + 32);

        sanitize_project_search_glob_input(&mut value);

        assert_eq!(value.chars().count(), MAX_PROJECT_SEARCH_GLOB_DRAFT_CHARS);
    }

    #[test]
    fn project_search_glob_input_bounds_hidden_control_only_prefixes() {
        let mut value = "\u{202e}".repeat(project_search_control_input_scan_chars(
            MAX_PROJECT_SEARCH_GLOB_DRAFT_CHARS,
        ));
        value.push_str("src/**");

        let changed = sanitize_project_search_glob_input(&mut value);

        assert!(changed);
        assert!(value.is_empty());
    }

    #[test]
    fn project_search_recent_application_sanitizes_malformed_control_state() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_controls_test(root);

        apply_project_search_query(
            &mut app,
            ProjectSearchQuery {
                query: format!(
                    "needle\n{}\u{202e}",
                    "x".repeat(MAX_PROJECT_SEARCH_QUERY_CHARS + 32)
                ),
                case_sensitive: true,
                whole_word: true,
                regex: true,
                include: format!(
                    "src/**/*.rs\r\n{}",
                    "x".repeat(MAX_PROJECT_SEARCH_GLOB_DRAFT_CHARS + 32)
                ),
                exclude: "target/**\u{2029}\u{202e}*.snap".to_owned(),
            },
        );

        assert!(app.project_search_case_sensitive);
        assert!(app.project_search_whole_word);
        assert!(app.project_search_regex);
        assert!(!app.project_search_query.chars().any(char::is_control));
        assert!(!app.project_search_query.contains('\u{202e}'));
        assert!(app.project_search_query.chars().count() <= MAX_PROJECT_SEARCH_QUERY_CHARS);
        assert!(!app.project_search_include.chars().any(char::is_control));
        assert!(app.project_search_include.chars().count() <= MAX_PROJECT_SEARCH_GLOB_DRAFT_CHARS);
        assert_eq!(app.project_search_exclude, "target/**, *.snap");
    }

    #[test]
    fn project_search_control_changes_cancel_active_request_and_reset_selection() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_project_search_controls_test(root);
        app.project_search_next_request_id = 41;
        app.project_search_active_request_id = 41;
        app.project_search_cancel_generation
            .store(41, Ordering::Relaxed);
        app.project_search_selected = 3;

        mark_project_search_controls_changed(&mut app);

        assert_eq!(app.project_search_next_request_id, 42);
        assert_eq!(app.project_search_active_request_id, 42);
        assert_eq!(
            app.project_search_cancel_generation.load(Ordering::Relaxed),
            42
        );
        assert_eq!(app.project_search_selected, 0);
    }

    fn app_for_project_search_controls_test(root: PathBuf) -> KuroyaApp {
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
}

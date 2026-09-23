use super::content::{
    completion_commit_text, completion_selected_item, completion_tab_accepts,
    normalize_completion_selection,
};
use crate::{
    KuroyaApp,
    editor_input::editor_text_input_from_event,
    editor_suggest::completion_request_for_typed_text,
    lsp_completion_ranking::{filter_completion_items_by_settings, selected_completion_index},
    lsp_edits::apply_completion_passthrough_events_with_editor_keys,
};
use eframe::egui::{Context, Event, Key};
use kuroya_core::{BufferId, EditorSettings, LspCompletionItem, buffer::AutoPairSettings};

impl KuroyaApp {
    pub(super) fn apply_completion_passthrough_input(&mut self, ctx: &Context) -> bool {
        let selected_item = completion_passthrough_selected_item(
            &mut self.completion_selected,
            &self.completion_items,
        );
        let tab_accepts = selected_item
            .map(|(_, item)| completion_tab_accepts(item, &self.settings))
            .unwrap_or(self.settings.accept_suggestion_on_tab);
        let input = ctx.input(|input| {
            let commit_text = selected_item
                .and_then(|(_, item)| completion_commit_text(item, &self.settings, &input.events));
            let close_for_tab_focus =
                self.settings.tab_focus_mode && !tab_accepts && tab_focus_event(&input.events);
            let edit_events = if commit_text.is_none() && !close_for_tab_focus {
                completion_passthrough_edit_events(
                    &input.events,
                    self.settings.accept_suggestion_on_enter,
                    tab_accepts,
                )
            } else {
                None
            };

            CompletionPassthroughInput {
                commit_text,
                close_for_tab_focus,
                edit_events,
            }
        });
        let commit_apply = input
            .commit_text
            .and_then(|commit_text| selected_item.map(|(_, item)| (item.clone(), commit_text)));
        if let Some((item, commit_text)) = commit_apply {
            self.apply_completion_item_with_commit(item, Some(commit_text));
            return true;
        }
        if input.close_for_tab_focus {
            self.clear_completion_popup_state();
            self.status = "Closed completions for Tab focus navigation".to_owned();
            return true;
        }
        let Some(events) = input.edit_events else {
            return false;
        };

        let Some(id) = self.active else {
            self.clear_completion_popup_state();
            self.status = "Closed completions; no active file".to_owned();
            return true;
        };
        if self.block_protected_preview_edit(id) {
            self.clear_completion_popup_state();
            return true;
        }

        let auto_pair_settings = AutoPairSettings {
            brackets: self.settings.auto_closing_brackets,
            quotes: self.settings.auto_closing_quotes,
            surround: self.settings.auto_surround,
            overtype: !matches!(
                self.settings.auto_closing_overtype,
                kuroya_core::EditorAutoClosingEditStrategy::Never
            ),
        };
        let tab = self.indent_options_for_buffer(id).unit;
        let auto_indent = self.settings.auto_indent;

        let popup_for_buffer = self.completion_buffer_id == Some(id);
        let popup_items = if popup_for_buffer {
            std::mem::take(&mut self.completion_items)
        } else {
            Vec::new()
        };
        let changed = self.buffer_mut(id).is_some_and(|buffer| {
            apply_completion_passthrough_events_with_editor_keys(
                buffer,
                &events,
                auto_pair_settings,
                &tab,
                auto_indent,
            )
        });
        if changed {
            self.mark_buffer_changed(id);
            if events
                .iter()
                .any(|event| matches!(event, eframe::egui::Event::Paste(_)))
                && self.settings.paste_as_enabled
                && self.settings.format_on_paste
            {
                let _ =
                    self.request_lsp_formatting_for_buffer(id, Some("Formatting paste in"), false);
            }
        }
        self.refresh_completion_popup_after_edit(
            ctx,
            id,
            &events,
            changed,
            popup_for_buffer,
            popup_items,
        );
        true
    }

    fn refresh_completion_popup_after_edit(
        &mut self,
        ctx: &Context,
        id: BufferId,
        events: &[Event],
        changed: bool,
        popup_for_buffer: bool,
        popup_items: Vec<LspCompletionItem>,
    ) {
        if !popup_for_buffer {
            self.clear_completion_popup_state();
            self.status = completion_popup_closed_status(changed);
            return;
        }
        let Some((prefix, origin)) = self
            .completion_popup_refilter_prefix(id)
            .zip(self.lsp_position_for_buffer(id))
        else {
            self.clear_completion_popup_state();
            self.status = completion_popup_closed_status(changed);
            return;
        };
        if prefix.is_empty() {
            self.clear_completion_popup_state();
            self.status = completion_popup_closed_status(changed);
            return;
        }
        let mut items = popup_items;
        filter_completion_items_by_settings(&mut items, &self.settings, &prefix);
        if items.is_empty() {
            if completion_edit_requests_refresh(events, &self.settings) {
                self.completion_open = true;
                self.completion_buffer_id = Some(id);
                self.completion_path = Some(origin.1);
                self.completion_selected = 0;
                self.completion_prefix = prefix;
                self.completion_preview_resolve_in_flight.clear();
                self.completion_preview_resolve_recent_attempts.clear();
                self.schedule_lsp_completion_for_buffer(ctx, id);
                self.status = "Requested completions while typing".to_owned();
            } else {
                self.clear_completion_popup_state();
                self.status = completion_popup_closed_status(changed);
            }
            return;
        }
        let selected = selected_completion_index(
            &items,
            &prefix,
            &self.settings,
            &self.completion_recent_labels,
            &self.completion_recent_prefix_labels,
        );
        self.completion_open = true;
        self.completion_items = items;
        self.completion_prefix = prefix;
        self.completion_selected = selected;
        self.completion_buffer_id = Some(id);
        let (_, path, version, line, character) = origin;
        self.completion_path = Some(path);
        self.completion_version = Some(version);
        self.completion_line = line + 1;
        self.completion_column = character + 1;
        self.completion_preview_resolve_in_flight.clear();
        self.completion_preview_resolve_recent_attempts.clear();
        self.status = "Refiltered completions while typing".to_owned();
    }

    fn completion_popup_refilter_prefix(&self, id: BufferId) -> Option<String> {
        let buffer = self.buffer(id)?;
        let range = buffer.completion_prefix_range()?;
        buffer.text_range(range)
    }
}

fn completion_popup_closed_status(changed: bool) -> String {
    if changed {
        "Closed completions while editing".to_owned()
    } else {
        "Closed completions".to_owned()
    }
}

fn completion_edit_requests_refresh(events: &[Event], settings: &EditorSettings) -> bool {
    events
        .iter()
        .filter_map(editor_text_input_from_event)
        .next_back()
        .is_some_and(|text| {
            completion_request_for_typed_text(
                text,
                settings.quick_suggestions,
                settings.suggest_on_trigger_characters,
                false,
            )
        })
}

struct CompletionPassthroughInput {
    commit_text: Option<String>,
    close_for_tab_focus: bool,
    edit_events: Option<Vec<Event>>,
}

fn completion_passthrough_selected_item<'a>(
    selected: &mut usize,
    items: &'a [LspCompletionItem],
) -> Option<(usize, &'a LspCompletionItem)> {
    normalize_completion_selection(selected, items.len());
    completion_selected_item(items, *selected)
}

fn tab_focus_event(events: &[Event]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            Event::Key {
                key: Key::Tab,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.ctrl && !modifiers.command && !modifiers.alt
        )
    })
}

fn completion_passthrough_edit_events(
    events: &[Event],
    accept_suggestion_on_enter: bool,
    accept_suggestion_on_tab: bool,
) -> Option<Vec<Event>> {
    let mut edit_events = None;
    for event in events {
        if completion_passthrough_edit_event(
            event,
            accept_suggestion_on_enter,
            accept_suggestion_on_tab,
        ) {
            edit_events.get_or_insert_with(Vec::new).push(event.clone());
        }
    }
    edit_events
}

fn completion_passthrough_edit_event(
    event: &Event,
    accept_suggestion_on_enter: bool,
    accept_suggestion_on_tab: bool,
) -> bool {
    match event {
        event if editor_text_input_from_event(event).is_some() => true,
        Event::Paste(_) => true,
        Event::Key {
            key: Key::Backspace | Key::Delete,
            pressed: true,
            modifiers,
            ..
        } => !modifiers.ctrl && !modifiers.command && !modifiers.alt,
        Event::Key {
            key: Key::Enter,
            pressed: true,
            modifiers,
            ..
        } => !accept_suggestion_on_enter && !modifiers.ctrl && !modifiers.command && !modifiers.alt,
        Event::Key {
            key: Key::Tab,
            pressed: true,
            modifiers,
            ..
        } => !accept_suggestion_on_tab && !modifiers.ctrl && !modifiers.command && !modifiers.alt,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        completion_edit_requests_refresh, completion_passthrough_edit_events,
        completion_passthrough_selected_item, completion_popup_closed_status, tab_focus_event,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, lsp_client::LspClientHandle,
        terminal::TerminalPane,
    };
    use eframe::egui::{Context, Event, Key, Modifiers};
    use kuroya_core::{EditorSettings, LanguageId, LspCompletionItem, TextBuffer, Workspace};
    use serde_json::json;
    use std::{
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn completion_passthrough_selection_clamps_and_snapshots_raw_item() {
        let mut selected = 8;
        let mut raw_item = completion_item("Raw\nHashMap\u{202e}");
        raw_item.detail = Some("raw detail".to_owned());
        raw_item.resolve_payload = Some(Arc::new(json!({
            "label": raw_item.label.clone(),
            "data": {
                "token": "raw-item"
            }
        })));
        let expected_item = raw_item.clone();
        let items = [completion_item("Vec"), raw_item];

        let (selected_index, selected_item) =
            completion_passthrough_selected_item(&mut selected, &items).expect("selected");
        let selected_item = selected_item.clone();

        assert_eq!(selected, 1);
        assert_eq!(selected_index, 1);
        assert_eq!(selected_item, expected_item);
        assert_eq!(items[1], expected_item);
    }

    #[test]
    fn completion_passthrough_selection_rejects_empty_items() {
        let mut selected = 8;
        let items = [];

        assert!(completion_passthrough_selected_item(&mut selected, &items).is_none());
        assert_eq!(selected, 0);
    }

    #[test]
    fn completion_passthrough_edit_events_respect_acceptance_keys() {
        let enter = key_event(Key::Enter, Modifiers::NONE);
        let tab = key_event(Key::Tab, Modifiers::NONE);

        assert_eq!(
            completion_passthrough_edit_events(std::slice::from_ref(&enter), true, false),
            None
        );
        assert_eq!(
            completion_passthrough_edit_events(std::slice::from_ref(&enter), false, false),
            Some(vec![enter])
        );
        assert_eq!(
            completion_passthrough_edit_events(std::slice::from_ref(&tab), false, true),
            None
        );
        assert_eq!(
            completion_passthrough_edit_events(std::slice::from_ref(&tab), false, false),
            Some(vec![tab])
        );
    }

    #[test]
    fn completion_passthrough_tab_focus_ignores_modified_tab() {
        assert!(tab_focus_event(&[key_event(Key::Tab, Modifiers::NONE)]));
        assert!(!tab_focus_event(&[key_event(Key::Tab, Modifiers::CTRL)]));
    }

    #[test]
    fn completion_edit_requests_refresh_follows_typed_trigger_settings() {
        let mut settings = EditorSettings::default();
        let typing = [text_event("n")];
        let trigger = [text_event(".")];
        let backspace = [Event::Key {
            key: Key::Backspace,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }];

        assert!(!completion_edit_requests_refresh(&typing, &settings));
        settings.quick_suggestions = true;
        assert!(completion_edit_requests_refresh(&typing, &settings));
        settings.quick_suggestions = false;
        assert!(completion_edit_requests_refresh(&trigger, &settings));
        assert!(!completion_edit_requests_refresh(&backspace, &settings));

        assert_eq!(
            completion_popup_closed_status(true),
            "Closed completions while editing"
        );
        assert_eq!(completion_popup_closed_status(false), "Closed completions");
    }

    #[test]
    fn passthrough_text_edit_keeps_popup_open_and_refilters_by_prefix() {
        let root = std::env::temp_dir().join("kuroya-passthrough-refilter-test");
        let (mut app, _, _) = popup_app_for_test(root, "fn main() {\n    pri\n}\n", 1, 7);
        app.completion_items = vec![
            completion_item("Vec"),
            completion_item("println!"),
            completion_item("print"),
        ];
        let ctx = Context::default();
        ctx.input_mut(|input| input.events.push(text_event("n")));

        assert!(app.apply_completion_passthrough_input(&ctx));

        assert!(app.completion_open);
        let (text, version) = {
            let buffer = app.buffer(7).expect("buffer");
            (buffer.text(), buffer.version())
        };
        assert_eq!(text, "fn main() {\n    prin\n}\n");
        let labels = app
            .completion_items
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["println!", "print"]);
        assert_eq!(app.completion_prefix, "prin");
        assert_eq!(app.completion_version, Some(version));
        assert_eq!(app.completion_line, 2);
        assert_eq!(app.completion_column, 9);
        assert!(app.pending_completion_requests.is_empty());
    }

    #[test]
    fn passthrough_edit_without_matching_word_context_closes_popup() {
        let root = std::env::temp_dir().join("kuroya-passthrough-word-end-close-test");
        let (mut app, _, _) = popup_app_for_test(root, "fn main() {\n    pri\n}\n", 1, 7);
        app.completion_items = vec![completion_item("println!")];
        let ctx = Context::default();
        ctx.input_mut(|input| input.events.push(text_event(" ")));

        assert!(app.apply_completion_passthrough_input(&ctx));

        assert!(!app.completion_open);
        assert!(app.completion_items.is_empty());
        assert_eq!(app.completion_buffer_id, None);
        assert_eq!(app.completion_prefix, "");
        assert_eq!(app.status, "Closed completions while editing");
        assert_eq!(
            app.buffer(7).map(|buffer| buffer.text()),
            Some("fn main() {\n    pri \n}\n".to_owned())
        );
        assert!(app.pending_completion_requests.is_empty());
    }

    #[test]
    fn passthrough_word_edit_past_local_items_schedules_fresh_completion_request() {
        let root = std::env::temp_dir().join("kuroya-passthrough-requery-test");
        let (mut app, _, _) = popup_app_for_test(root, "fn main() {\n    pri\n}\n", 1, 7);
        app.settings.quick_suggestions = true;
        app.completion_items = vec![completion_item("Vec")];
        let ctx = Context::default();
        ctx.input_mut(|input| input.events.push(text_event("n")));

        assert!(app.apply_completion_passthrough_input(&ctx));

        assert!(app.completion_open);
        assert!(app.completion_items.is_empty());
        assert_eq!(app.completion_prefix, "prin");
        assert_eq!(app.completion_version, None);
        assert_eq!(app.completion_buffer_id, Some(7));
        assert!(app.pending_completion_requests.contains_key(&7));
        assert_eq!(app.status, "Requested completions while typing");
        assert_eq!(
            app.buffer(7).map(|buffer| buffer.text()),
            Some("fn main() {\n    prin\n}\n".to_owned())
        );

        app.pending_completion_requests
            .insert(7, Instant::now() - Duration::from_millis(50));
        app.lsp_clients
            .insert("rust".to_owned(), LspClientHandle::accepting_for_test());

        assert_eq!(app.flush_pending_completion_requests(), 1);

        let version = app.buffer(7).map(|buffer| buffer.version());
        assert!(app.completion_open);
        assert_eq!(app.completion_version, version);
        assert_eq!(app.completion_line, 2);
        assert_eq!(app.completion_column, 9);
        assert!(app.completion_prefix.is_empty());
        assert!(app.pending_completion_requests.is_empty());
    }

    #[test]
    fn passthrough_commit_character_still_accepts_item_and_closes() {
        let root = std::env::temp_dir().join("kuroya-passthrough-commit-char-test");
        let (mut app, _, _) = popup_app_for_test(root, "fn main() {\n    pri\n}\n", 1, 7);
        let mut item = completion_item("println!");
        item.commit_characters = vec![".".to_owned()];
        app.completion_items = vec![item];
        let ctx = Context::default();
        ctx.input_mut(|input| input.events.push(text_event(".")));

        assert!(app.apply_completion_passthrough_input(&ctx));

        assert!(!app.completion_open);
        assert!(app.completion_items.is_empty());
        assert_eq!(
            app.buffer(7).map(|buffer| buffer.text()),
            Some("fn main() {\n    println!.\n}\n".to_owned())
        );
        assert_eq!(app.status, "Inserted completion `println!` and `.`");
    }

    fn text_event(text: &str) -> Event {
        Event::Text(text.to_owned())
    }

    fn popup_app_for_test(
        root: PathBuf,
        text: &str,
        line: usize,
        column: usize,
    ) -> (KuroyaApp, PathBuf, u64) {
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        let mut buffer = TextBuffer::from_text_with_language(
            7,
            Some(path.clone()),
            text.to_owned(),
            LanguageId::Rust,
        );
        buffer.set_single_cursor(buffer.line_column_to_char(line, column));
        let version = buffer.version();
        app.active = Some(7);
        app.buffers.push(buffer);
        app.completion_open = true;
        app.completion_selected = 0;
        app.completion_buffer_id = Some(7);
        app.completion_path = Some(path.clone());
        app.completion_version = Some(version);
        app.completion_line = line + 1;
        app.completion_column = column + 1;
        (app, path, version)
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
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

    fn key_event(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn completion_item(label: &str) -> LspCompletionItem {
        LspCompletionItem {
            label: label.to_owned(),
            detail: None,
            documentation: None,
            kind: None,
            deprecated: false,
            is_snippet: false,
            sort_text: None,
            filter_text: None,
            preselect: false,
            commit_characters: Vec::new(),
            insert_text: label.to_owned(),
            snippet_selection: None,
            snippet_tabstops: Vec::new(),
            snippet_tabstop_groups: Vec::new(),
            text_edit: None,
            insert_text_edit: None,
            additional_text_edits: Vec::new(),
            resolve_payload: None,
        }
    }
}

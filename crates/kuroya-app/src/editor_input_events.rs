use crate::{
    KuroyaApp,
    editor_clipboard_context_actions::editor_clipboard_text,
    editor_input::{
        editor_key_can_mutate, normalized_editor_ime_commit_text, normalized_editor_paste_text,
        normalized_editor_text_input, normalized_editor_text_input_with_fast_coalesce,
    },
    editor_key_events::handle_editor_key_event,
    editor_suggest::{
        completion_request_after_text_edit, format_on_type_request_after_text_edit,
        signature_help_request_after_text_edit,
    },
    editor_vim_key_events::{
        EditorVimMode, handle_vim_editor_key_event_with_settings_and_indent,
        vim_clear_command_input, vim_clear_marks, vim_clear_named_registers,
        vim_clear_search_input, vim_clear_searches, vim_collapse_multi_cursor_selection,
        vim_collapse_selection_for_insert, vim_events_include_mutation_with_settings,
        vim_record_insert_replay_key_with_auto_indent_for_buffer,
        vim_record_inserted_text_for_buffer, vim_text_after_suppression,
    },
    transient_state::EditorImePreedit,
    workspace_state::PaneId,
};
use eframe::egui::{Context, Event, ImeEvent, Key, Modifiers};
use kuroya_core::{
    BufferId, EditorAutoClosingEditStrategy, EditorMultiCursorPaste,
    EditorPasteAsShowPasteSelector, EditorVimSettings, TextBuffer, buffer::AutoPairSettings,
};
use std::{borrow::Cow, collections::VecDeque, ops::Range};

const MAX_EDITOR_IME_PREEDIT_CHARS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditorInputEvent {
    Copy,
    Cut,
    Text { text: String, ime_commit: bool },
    ImePreedit(String),
    ImeClearPreedit,
    Paste(String),
    Key { key: Key, modifiers: Modifiers },
}

#[derive(Debug, Clone, Copy)]
struct EditorInputEventClassifier {
    keep_all_key_events: bool,
    track_mutation: bool,
}

impl EditorInputEventClassifier {
    fn for_mode(vim_keybindings: bool, vim_mode: EditorVimMode) -> Self {
        Self {
            keep_all_key_events: vim_keybindings && !vim_mode.accepts_text_input(),
            track_mutation: !vim_keybindings,
        }
    }

    fn classify_egui_event(self, event: &Event) -> Option<ClassifiedEditorInputEvent> {
        match event {
            Event::Copy => Some(ClassifiedEditorInputEvent::new(
                EditorInputEvent::Copy,
                false,
            )),
            Event::Cut => Some(ClassifiedEditorInputEvent::new(
                EditorInputEvent::Cut,
                self.track_mutation,
            )),
            Event::Ime(ImeEvent::Commit(_) | ImeEvent::Enabled | ImeEvent::Disabled) => Some(
                ClassifiedEditorInputEvent::new(EditorInputEvent::ImeClearPreedit, false),
            ),
            Event::Ime(ImeEvent::Preedit(text)) => Some(
                normalized_ime_preedit_text(text)
                    .map(EditorInputEvent::ImePreedit)
                    .unwrap_or(EditorInputEvent::ImeClearPreedit),
            )
            .map(|event| ClassifiedEditorInputEvent::new(event, false)),
            Event::Paste(text) => normalized_editor_paste_text(text).map(|text| {
                ClassifiedEditorInputEvent::new(
                    EditorInputEvent::Paste(text.into_owned()),
                    self.track_mutation,
                )
            }),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if self.key_event_is_relevant(*key, *modifiers) => {
                Some(ClassifiedEditorInputEvent::new(
                    EditorInputEvent::Key {
                        key: *key,
                        modifiers: *modifiers,
                    },
                    self.track_mutation && editor_key_can_mutate(*key, *modifiers),
                ))
            }
            _ => None,
        }
    }

    fn key_event_is_relevant(self, key: Key, modifiers: Modifiers) -> bool {
        self.keep_all_key_events || !editor_plain_text_key_event_is_redundant(key, modifiers)
    }
}

#[derive(Debug)]
struct ClassifiedEditorInputEvent {
    event: EditorInputEvent,
    includes_mutation: bool,
}

impl ClassifiedEditorInputEvent {
    fn new(event: EditorInputEvent, includes_mutation: bool) -> Self {
        Self {
            event,
            includes_mutation,
        }
    }
}

#[derive(Debug, Default)]
struct EditorInputEventsSnapshot {
    includes_mutation: bool,
    events: Vec<EditorInputEvent>,
}

#[cfg(test)]
fn editor_input_events_snapshot(
    events: &[Event],
    vim_keybindings: bool,
    vim_mode: EditorVimMode,
    vim_pending_key: Option<crate::editor_vim_key_events::EditorVimPendingKey>,
    coalesce_fast_text: bool,
) -> EditorInputEventsSnapshot {
    let default_vim_settings = EditorVimSettings::default();
    editor_input_events_snapshot_with_vim_settings(
        events,
        vim_keybindings,
        vim_mode,
        vim_pending_key,
        coalesce_fast_text,
        &default_vim_settings,
    )
}

fn editor_input_events_snapshot_with_vim_settings(
    events: &[Event],
    vim_keybindings: bool,
    vim_mode: EditorVimMode,
    vim_pending_key: Option<crate::editor_vim_key_events::EditorVimPendingKey>,
    coalesce_fast_text: bool,
    vim_settings: &EditorVimSettings,
) -> EditorInputEventsSnapshot {
    let classifier = EditorInputEventClassifier::for_mode(vim_keybindings, vim_mode);
    let mut snapshot = EditorInputEventsSnapshot {
        includes_mutation: false,
        events: Vec::with_capacity(events.len()),
    };
    let mut tail_accepts_fast_text = false;
    let mut pending_text_echo = None;
    let mut vim_mutation_scan_required = false;
    for event in events {
        vim_mutation_scan_required |= vim_raw_event_can_require_mutation_scan(event);
        let includes_event_mutation = push_editor_input_event_from_egui_event(
            &mut snapshot.events,
            &mut tail_accepts_fast_text,
            &mut pending_text_echo,
            event,
            classifier,
            coalesce_fast_text,
        );
        snapshot.includes_mutation |= includes_event_mutation;
    }
    if vim_keybindings {
        snapshot.includes_mutation = vim_mutation_scan_required
            && vim_events_include_mutation_with_settings(
                events,
                vim_mode,
                vim_pending_key,
                vim_settings,
            );
    }
    snapshot
}

fn vim_raw_event_can_require_mutation_scan(event: &Event) -> bool {
    match event {
        Event::Cut => true,
        Event::Paste(text) | Event::Text(text) | Event::Ime(ImeEvent::Commit(text)) => {
            !text.is_empty()
        }
        Event::Key { pressed: true, .. } => true,
        _ => false,
    }
}

fn push_editor_input_event_from_egui_event(
    events: &mut Vec<EditorInputEvent>,
    tail_accepts_fast_text: &mut bool,
    pending_text_echo: &mut Option<usize>,
    event: &Event,
    classifier: EditorInputEventClassifier,
    coalesce_fast_text: bool,
) -> bool {
    match event {
        Event::Text(text) => {
            let text_and_fast_coalesce = if coalesce_fast_text {
                normalized_editor_text_input_with_fast_coalesce(text)
            } else {
                normalized_editor_text_input(text).map(|text| (text, false))
            };
            if let Some((text, accepts_fast_text)) = text_and_fast_coalesce {
                if pending_editor_text_echo_matches(events, *pending_text_echo, text) {
                    *pending_text_echo = None;
                    *tail_accepts_fast_text = false;
                    return false;
                }
                *pending_text_echo = None;
                push_editor_text_input_event(
                    events,
                    tail_accepts_fast_text,
                    text,
                    false,
                    accepts_fast_text,
                );
                classifier.track_mutation
            } else {
                false
            }
        }
        Event::Ime(ImeEvent::Commit(text)) => {
            *pending_text_echo = None;
            if let Some(text) = normalized_editor_ime_commit_text(text) {
                let event_index = push_editor_text_input_event(
                    events,
                    tail_accepts_fast_text,
                    text.as_ref(),
                    true,
                    false,
                );
                *pending_text_echo = Some(event_index);
                classifier.track_mutation
            } else {
                *tail_accepts_fast_text = false;
                events.push(EditorInputEvent::ImeClearPreedit);
                false
            }
        }
        _ => {
            if let Some(classified) = classifier.classify_egui_event(event) {
                *pending_text_echo = None;
                *tail_accepts_fast_text = false;
                let starts_text_echo = editor_input_event_can_start_text_echo(&classified.event);
                events.push(classified.event);
                if starts_text_echo {
                    *pending_text_echo = Some(events.len() - 1);
                }
                classified.includes_mutation
            } else {
                if matches!(event, Event::Key { pressed: true, .. }) {
                    *pending_text_echo = None;
                }
                false
            }
        }
    }
}

fn push_editor_text_input_event(
    events: &mut Vec<EditorInputEvent>,
    tail_accepts_fast_text: &mut bool,
    text: &str,
    ime_commit: bool,
    accepts_fast_text: bool,
) -> usize {
    let accepts_fast_text = accepts_fast_text && !ime_commit;
    let previous_event_index = events.len().saturating_sub(1);
    if accepts_fast_text
        && *tail_accepts_fast_text
        && let Some(EditorInputEvent::Text {
            text: previous,
            ime_commit: false,
        }) = events.last_mut()
    {
        previous.push_str(text);
        return previous_event_index;
    }

    *tail_accepts_fast_text = accepts_fast_text;
    events.push(EditorInputEvent::Text {
        text: text.to_owned(),
        ime_commit,
    });
    events.len() - 1
}

fn editor_input_event_can_start_text_echo(event: &EditorInputEvent) -> bool {
    match event {
        EditorInputEvent::Paste(text) => normalized_editor_text_input(text).is_some(),
        EditorInputEvent::Text {
            ime_commit: true, ..
        } => true,
        _ => false,
    }
}

fn pending_editor_text_echo_matches(
    events: &[EditorInputEvent],
    pending_text_echo: Option<usize>,
    text: &str,
) -> bool {
    let Some(event) = pending_text_echo.and_then(|index| events.get(index)) else {
        return false;
    };

    match event {
        EditorInputEvent::Paste(previous) => previous == text,
        EditorInputEvent::Text {
            text: previous,
            ime_commit: true,
        } => previous == text,
        _ => false,
    }
}

#[cfg(test)]
fn editor_key_event_is_relevant_for_input_mode(
    key: Key,
    modifiers: Modifiers,
    vim_keybindings: bool,
    vim_mode: EditorVimMode,
) -> bool {
    EditorInputEventClassifier::for_mode(vim_keybindings, vim_mode)
        .key_event_is_relevant(key, modifiers)
}

fn editor_plain_text_key_event_is_redundant(key: Key, modifiers: Modifiers) -> bool {
    if modifiers.ctrl || modifiers.command || modifiers.alt {
        return false;
    }

    editor_key_is_plain_text_input(key)
}

fn editor_key_is_plain_text_input(key: Key) -> bool {
    matches!(
        key,
        Key::A
            | Key::B
            | Key::C
            | Key::D
            | Key::E
            | Key::F
            | Key::G
            | Key::H
            | Key::I
            | Key::J
            | Key::K
            | Key::L
            | Key::M
            | Key::N
            | Key::O
            | Key::P
            | Key::Q
            | Key::R
            | Key::S
            | Key::T
            | Key::U
            | Key::V
            | Key::W
            | Key::X
            | Key::Y
            | Key::Z
            | Key::Num0
            | Key::Num1
            | Key::Num2
            | Key::Num3
            | Key::Num4
            | Key::Num5
            | Key::Num6
            | Key::Num7
            | Key::Num8
            | Key::Num9
            | Key::Space
            | Key::Colon
            | Key::Comma
            | Key::Minus
            | Key::Period
            | Key::Plus
            | Key::Equals
            | Key::Semicolon
            | Key::Backtick
            | Key::Backslash
            | Key::OpenBracket
            | Key::CloseBracket
            | Key::Slash
            | Key::Pipe
            | Key::Questionmark
            | Key::Exclamationmark
            | Key::OpenCurlyBracket
            | Key::CloseCurlyBracket
            | Key::Quote
    )
}

impl KuroyaApp {
    pub(crate) fn handle_editor_input(
        &mut self,
        ctx: &Context,
        pane_id: PaneId,
        buffer_id: BufferId,
    ) {
        if self.focused_pane == Some(pane_id) && !self.editor_accepts_text_input(ctx, pane_id) {
            self.vim_discard_half_typed_sequence();
            return;
        }
        if !self.editor_accepts_text_input(ctx, pane_id) {
            return;
        }

        let vim_keybindings = self.settings.vim_keybindings;
        let coalesce_fast_text =
            editor_text_event_coalescing_allowed_for_mode(vim_keybindings, self.editor_vim_mode)
                && editor_text_event_coalescing_enabled_for_buffer(self, buffer_id);
        let events = ctx.input(|input| {
            editor_input_events_snapshot_with_vim_settings(
                &input.events,
                vim_keybindings,
                self.editor_vim_mode,
                self.editor_vim_pending_key,
                coalesce_fast_text,
                &self.settings.vim,
            )
        });
        if events.events.is_empty() {
            return;
        }
        if events.includes_mutation && self.block_protected_preview_edit(buffer_id) {
            self.clear_editor_ime_preedit_for_buffer(buffer_id);
            return;
        }

        let auto_indent = self.settings.auto_indent;
        let auto_pair_settings = AutoPairSettings {
            brackets: self.settings.auto_closing_brackets,
            quotes: self.settings.auto_closing_quotes,
            surround: self.settings.auto_surround,
            overtype: auto_closing_edit_enabled(self.settings.auto_closing_overtype),
        };
        let auto_closing_delete = auto_closing_edit_enabled(self.settings.auto_closing_delete);
        let paste_transform_plan = editor_paste_transform_plan(
            self.settings.paste_as_enabled,
            self.settings.multi_cursor_paste,
            self.settings.auto_indent_on_paste,
            self.settings.auto_indent_on_paste_within_string,
            self.settings.format_on_paste,
        );
        let mut changed = false;
        let mut pasted = false;
        let mut show_paste_selector = false;
        let mut request_completion = false;
        let mut request_signature_help = false;
        let mut request_format_on_type = false;
        let mut keep_cursor_visible = false;
        let mut vim_suppressed_text = VecDeque::new();
        let mut indent_options = None;
        for event in events.events {
            match event {
                EditorInputEvent::Copy => {
                    if let Some(text) = self.buffer(buffer_id).and_then(|buffer| {
                        editor_clipboard_text(buffer, self.settings.empty_selection_clipboard)
                    }) {
                        ctx.copy_text(text);
                    }
                }
                EditorInputEvent::Cut => {
                    let empty_selection_clipboard = self.settings.empty_selection_clipboard;
                    if let Some(buffer) = self.buffer_mut(buffer_id)
                        && let Some(text) = editor_clipboard_text(buffer, empty_selection_clipboard)
                    {
                        ctx.copy_text(text);
                        changed |= buffer.delete_selection_or_lines();
                        if changed {
                            self.clear_editor_ime_preedit_for_buffer(buffer_id);
                            self.clear_snippet_session_for_buffer(buffer_id);
                        }
                    }
                }
                EditorInputEvent::Text { text, .. } => {
                    self.clear_editor_ime_preedit_for_buffer(buffer_id);
                    debug_assert!(normalized_editor_text_input(&text).is_some());
                    let text = text.as_str();
                    let text = if vim_keybindings {
                        if !self.editor_vim_mode.accepts_text_input() {
                            let _ = vim_text_after_suppression(text, &mut vim_suppressed_text);
                            continue;
                        }
                        let Some(text) = vim_text_after_suppression(text, &mut vim_suppressed_text)
                        else {
                            continue;
                        };
                        text
                    } else {
                        Cow::Borrowed(text)
                    };
                    let text = text.as_ref();
                    let mut inserted = false;
                    let vim_insert_mode =
                        vim_keybindings && self.editor_vim_mode.accepts_text_input();
                    let snippet_ranges = active_snippet_ranges_for_buffer(self, buffer_id);
                    let mut snippet_snapshot = None;
                    let mut snippet_mode = false;
                    let mut snippet_after_edit = None;
                    if let Some(buffer) = self.buffer_mut(buffer_id) {
                        if vim_insert_mode && snippet_ranges.is_none() {
                            vim_collapse_selection_for_insert(buffer);
                        }
                        snippet_snapshot = snippet_ranges.as_deref().and_then(|ranges| {
                            active_snippet_edit_snapshot_from_ranges(buffer, ranges)
                        });
                        snippet_mode = snippet_snapshot.is_some();
                        inserted =
                            buffer.insert_text_with_auto_pair_settings(text, auto_pair_settings);
                        changed |= inserted;
                        if inserted && snippet_snapshot.is_some() {
                            snippet_after_edit = Some(snippet_post_edit_snapshot(buffer));
                        }
                    }
                    if inserted {
                        if vim_keybindings {
                            vim_record_inserted_text_for_buffer(
                                &mut self.editor_vim_last_change,
                                buffer_id,
                                text,
                            );
                        }
                        if let Some(snapshot) = snippet_snapshot {
                            if let Some(after) = snippet_after_edit {
                                update_snippet_session_after_edit_snapshot(
                                    self,
                                    buffer_id,
                                    &snapshot.ranges,
                                    snapshot.before_len,
                                    after,
                                );
                            } else {
                                self.clear_snippet_session_for_buffer(buffer_id);
                            }
                        } else {
                            self.clear_snippet_session_for_buffer(buffer_id);
                        }
                    }
                    keep_cursor_visible = true;
                    request_completion |= completion_request_after_text_edit(
                        text,
                        inserted,
                        self.settings.quick_suggestions,
                        self.settings.suggest_on_trigger_characters,
                        self.settings.suggest_snippets_prevent_quick_suggestions && snippet_mode,
                    );
                    request_signature_help |= signature_help_request_after_text_edit(
                        text,
                        inserted,
                        self.settings.parameter_hints_enabled,
                        self.settings.parameter_hints_on_trigger_characters,
                    );
                    request_format_on_type |= format_on_type_request_after_text_edit(
                        text,
                        inserted,
                        self.settings.format_on_type,
                    );
                }
                EditorInputEvent::ImePreedit(text) => {
                    if !vim_keybindings || self.editor_vim_mode.accepts_text_input() {
                        self.set_editor_ime_preedit(buffer_id, text);
                        keep_cursor_visible = true;
                    }
                }
                EditorInputEvent::ImeClearPreedit => {
                    self.clear_editor_ime_preedit_for_buffer(buffer_id);
                }
                EditorInputEvent::Paste(text) => {
                    self.clear_editor_ime_preedit_for_buffer(buffer_id);
                    if vim_keybindings && !self.editor_vim_mode.accepts_text_input() {
                        continue;
                    }
                    let text = text.as_str();
                    let vim_insert_mode =
                        vim_keybindings && self.editor_vim_mode.accepts_text_input();
                    let snippet_ranges = active_snippet_ranges_for_buffer(self, buffer_id);
                    let mut cursor_count = 0;
                    let mut snippet_snapshot = None;
                    let mut inserted = false;
                    let mut snippet_after_edit = None;
                    if let Some(buffer) = self.buffer_mut(buffer_id) {
                        if vim_insert_mode && snippet_ranges.is_none() {
                            vim_collapse_selection_for_insert(buffer);
                        }
                        cursor_count = buffer.selections().len();
                        snippet_snapshot = snippet_ranges.as_deref().and_then(|ranges| {
                            active_snippet_edit_snapshot_from_ranges(buffer, ranges)
                        });
                        inserted = paste_normalized_text_at_editor_cursors(
                            buffer,
                            text,
                            paste_transform_plan.multi_cursor_paste,
                            paste_transform_plan.auto_indent_on_paste,
                            paste_transform_plan.auto_indent_on_paste_within_string,
                        );
                        changed |= inserted;
                        pasted |= inserted;
                        keep_cursor_visible |= inserted;
                        if inserted && snippet_snapshot.is_some() {
                            snippet_after_edit = Some(snippet_post_edit_snapshot(buffer));
                        }
                    }
                    let selector_visible = paste_selector_visible_after_paste(
                        self.settings.paste_as_enabled,
                        self.settings.paste_as_show_paste_selector,
                        paste_transform_plan,
                        cursor_count,
                        text,
                    );
                    show_paste_selector |= inserted && selector_visible;
                    if inserted {
                        if vim_keybindings {
                            vim_record_inserted_text_for_buffer(
                                &mut self.editor_vim_last_change,
                                buffer_id,
                                text,
                            );
                        }
                        if let Some(snapshot) = snippet_snapshot {
                            if let Some(after) = snippet_after_edit {
                                update_snippet_session_after_edit_snapshot(
                                    self,
                                    buffer_id,
                                    &snapshot.ranges,
                                    snapshot.before_len,
                                    after,
                                );
                            } else {
                                self.clear_snippet_session_for_buffer(buffer_id);
                            }
                        } else {
                            self.clear_snippet_session_for_buffer(buffer_id);
                        }
                    }
                }
                EditorInputEvent::Key { key, modifiers } => {
                    if indent_options.is_none() {
                        indent_options = Some(self.indent_options_for_buffer(buffer_id));
                    }
                    let indent_options = indent_options
                        .as_ref()
                        .expect("indent options were initialized for editor key input");
                    keep_cursor_visible = true;
                    if vim_keybindings {
                        let previous_mode = self.editor_vim_mode;
                        let mut next_mode = self.editor_vim_mode;
                        let mut next_pending = self.editor_vim_pending_key;
                        let mut next_last_char_find = self.editor_vim_last_char_find;
                        let mut next_unnamed_register = self.editor_vim_unnamed_register.take();

                        let mut next_last_change = self.editor_vim_last_change.remove(&buffer_id);
                        let vim_settings = self.settings.vim.clone();
                        let vim_result = self.buffer_mut(buffer_id).map(|buffer| {
                            if !previous_mode.accepts_text_input() {
                                vim_collapse_multi_cursor_selection(buffer);
                            }
                            handle_vim_editor_key_event_with_settings_and_indent(
                                buffer,
                                key,
                                modifiers,
                                &mut next_mode,
                                &mut next_pending,
                                &mut next_last_char_find,
                                &mut next_unnamed_register,
                                &mut next_last_change,
                                &vim_settings,
                                &indent_options.unit,
                            )
                        });
                        self.editor_vim_mode = next_mode;
                        self.editor_vim_pending_key = next_pending;
                        self.editor_vim_last_char_find = next_last_char_find;
                        self.editor_vim_unnamed_register = next_unnamed_register;
                        if let Some(last_change) = next_last_change {
                            self.editor_vim_last_change.insert(buffer_id, last_change);
                        }

                        self.editor_vim_insert_undo_group_buffer =
                            matches!(self.editor_vim_mode, EditorVimMode::Insert)
                                .then_some(buffer_id);
                        if let Some(result) = vim_result
                            && result.handled
                        {
                            if let Some(ch) = result.suppress_text {
                                vim_suppressed_text.push_back(ch);
                            }
                            if let Some(command) = result.command {
                                self.command_bus.push(command);
                            }
                            changed |= result.changed;
                            if previous_mode != self.editor_vim_mode {
                                self.status = editor_vim_mode_status(self.editor_vim_mode);
                                if !self.editor_vim_mode.accepts_text_input() {
                                    self.clear_editor_ime_preedit_for_buffer(buffer_id);
                                }
                            }
                            if result.changed {
                                self.clear_editor_ime_preedit_for_buffer(buffer_id);
                                self.clear_snippet_session_for_buffer(buffer_id);
                            }
                            continue;
                        }
                    }
                    let vim_insert_mode =
                        vim_keybindings && self.editor_vim_mode.accepts_text_input();
                    let mut key_changed = false;
                    let release_editor_focus = handle_editor_key_event(
                        self,
                        buffer_id,
                        key,
                        modifiers,
                        &indent_options.unit,
                        auto_indent,
                        self.settings.tab_focus_mode,
                        indent_options.tab_size,
                        indent_options.insert_spaces,
                        self.settings.use_tab_stops,
                        self.settings.trim_whitespace_on_delete,
                        auto_closing_delete,
                        &mut key_changed,
                    );
                    changed |= key_changed;
                    if key_changed && vim_insert_mode {
                        vim_record_insert_replay_key_with_auto_indent_for_buffer(
                            &mut self.editor_vim_last_change,
                            buffer_id,
                            key,
                            modifiers,
                            auto_indent,
                        );
                    }
                    if release_editor_focus {
                        self.focused_pane = None;
                        break;
                    }
                    request_format_on_type |= key == eframe::egui::Key::Enter
                        && key_changed
                        && self.settings.format_on_type;
                    if key_changed {
                        self.clear_editor_ime_preedit_for_buffer(buffer_id);
                        self.clear_snippet_session_for_buffer(buffer_id);
                    }
                }
            }
        }

        if changed {
            self.mark_buffer_changed(buffer_id);
            ctx.request_repaint();
        }
        if keep_cursor_visible {
            self.queue_editor_cursor_scroll(buffer_id);
            ctx.request_repaint();
        }
        let formatting_requested = pasted
            && paste_transform_plan.format_on_paste
            && self
                .request_lsp_formatting_for_buffer(buffer_id, Some("Formatting paste in"), false)
                .is_some();
        if show_paste_selector && !formatting_requested {
            self.status = "Paste options available".to_owned();
        }
        if request_format_on_type {
            self.schedule_lsp_format_on_type_for_buffer(ctx, buffer_id);
        }
        if request_completion {
            self.schedule_lsp_completion_for_buffer(ctx, buffer_id);
        }
        if request_signature_help {
            self.schedule_lsp_signature_help_for_buffer(ctx, buffer_id);
        }
    }

    fn queue_editor_cursor_scroll(&mut self, buffer_id: BufferId) {
        if let Some(line) = self
            .buffer(buffer_id)
            .map(|buffer| buffer.cursor_position().line)
        {
            self.pending_scroll_lines.insert(buffer_id, line);
        }
    }

    pub(crate) fn editor_accepts_text_input(&self, ctx: &Context, pane_id: PaneId) -> bool {
        self.focused_pane == Some(pane_id)
            && !self.terminal.input_focused(ctx)
            && !self.quick_open
            && !self.buffer_find_open
            && !self.goto_line_open
            && !self.command_palette
            && !self.workspace_symbols_open
            && !self.local_history_browser_open
            && !self.open_workspace_picker_in_flight
            && !self.open_workspace_open
            && !self.save_as_open
            && !self.settings_panel_open
            && !self.theme_picker_open
            && !self.keybindings_open
            && !self.lsp_rename_open
            && !self.completion_open
            && !self.references_open
            && !self.call_hierarchy_open
            && !self.type_hierarchy_open
            && !self.code_actions_open
            && self.dirty_close_buffer.is_none()
            && self.dirty_reload_buffer.is_none()
            && self.save_conflict_buffer.is_none()
            && self.pending_workspace_switch.is_none()
            && self.pending_exit.is_none()
            && self.pending_editor_file_drop.is_none()
            && self.explorer_file_action.is_none()
            && self.explorer_delete_target.is_none()
            && !self.project_search
    }

    fn set_editor_ime_preedit(&mut self, buffer_id: BufferId, text: String) {
        self.ime_preedit = Some(EditorImePreedit { buffer_id, text });
    }

    pub(crate) fn vim_discard_half_typed_sequence(&mut self) {
        if self.editor_vim_pending_key.take().is_some() {
            vim_clear_search_input();
            vim_clear_command_input();
        }
    }

    pub(crate) fn close_stale_vim_insert_undo_group(&mut self, active_buffer_id: BufferId) {
        let Some(session_buffer) = self.editor_vim_insert_undo_group_buffer.take() else {
            return;
        };
        if session_buffer == active_buffer_id {
            self.editor_vim_insert_undo_group_buffer = Some(session_buffer);
            return;
        }
        if let Some(buffer) = self.buffer_mut(session_buffer) {
            buffer.end_undo_group();
        }
    }

    pub(crate) fn close_vim_insert_undo_group(&mut self) {
        if let Some(session_buffer) = self.editor_vim_insert_undo_group_buffer.take()
            && let Some(buffer) = self.buffer_mut(session_buffer)
        {
            buffer.end_undo_group();
        }
    }

    pub(crate) fn vim_reset_session_state(&mut self) {
        self.editor_vim_mode = EditorVimMode::Normal;
        self.editor_vim_pending_key = None;
        self.editor_vim_last_char_find = None;
        self.editor_vim_unnamed_register = None;
        self.editor_vim_last_change.clear();
        self.close_vim_insert_undo_group();
        vim_clear_named_registers();
        vim_clear_marks();
        vim_clear_searches();
        vim_clear_search_input();
        vim_clear_command_input();
    }

    fn clear_editor_ime_preedit_for_buffer(&mut self, buffer_id: BufferId) {
        if self
            .ime_preedit
            .as_ref()
            .is_some_and(|preedit| preedit.buffer_id == buffer_id)
        {
            self.ime_preedit = None;
        }
    }
}

fn editor_text_event_coalescing_enabled_for_buffer(app: &KuroyaApp, buffer_id: BufferId) -> bool {
    !app.has_active_snippet_ranges_for_buffer(buffer_id)
        && app.buffer(buffer_id).is_some_and(|buffer| {
            buffer
                .selections()
                .iter()
                .all(|selection| selection.is_caret())
        })
}

fn editor_text_event_coalescing_allowed_for_mode(
    vim_keybindings: bool,
    vim_mode: EditorVimMode,
) -> bool {
    !vim_keybindings || vim_mode.accepts_text_input()
}

fn editor_vim_mode_status(mode: EditorVimMode) -> String {
    match mode {
        EditorVimMode::Normal => "Vim normal mode".to_owned(),
        EditorVimMode::Insert => "Vim insert mode".to_owned(),
    }
}

pub(crate) fn normalized_ime_preedit_text(text: &str) -> Option<String> {
    let mut output = String::new();
    for ch in text
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_EDITOR_IME_PREEDIT_CHARS)
    {
        output.push(ch);
    }

    (!output.is_empty()).then_some(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveSnippetEditSnapshot {
    ranges: Vec<Range<usize>>,
    before_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnippetPostEditSnapshot {
    ranges: Vec<Range<usize>>,
    after_len: usize,
    cursor: usize,
}

fn active_snippet_ranges_for_buffer(
    app: &KuroyaApp,
    buffer_id: BufferId,
) -> Option<Vec<Range<usize>>> {
    app.active_snippet_ranges_slice_for_buffer(buffer_id)
        .map(<[Range<usize>]>::to_vec)
}

fn active_snippet_edit_snapshot_from_ranges(
    buffer: &TextBuffer,
    ranges: &[Range<usize>],
) -> Option<ActiveSnippetEditSnapshot> {
    let selections = buffer.selections();
    if selections.len() != ranges.len()
        || !selections
            .iter()
            .zip(ranges)
            .all(|(selection, range)| selection.range() == *range)
    {
        return None;
    }
    Some(ActiveSnippetEditSnapshot {
        ranges: ranges.to_vec(),
        before_len: buffer.len_chars(),
    })
}

fn snippet_post_edit_snapshot(buffer: &TextBuffer) -> SnippetPostEditSnapshot {
    SnippetPostEditSnapshot {
        ranges: buffer
            .selections()
            .iter()
            .map(|selection| selection.range())
            .collect(),
        after_len: buffer.len_chars(),
        cursor: buffer.cursor(),
    }
}

fn update_snippet_session_after_edit_snapshot(
    app: &mut KuroyaApp,
    buffer_id: BufferId,
    old_ranges: &[Range<usize>],
    before_len: usize,
    after: SnippetPostEditSnapshot,
) {
    if !app.update_snippet_session_after_active_group_edit(buffer_id, old_ranges, &after.ranges) {
        let delta = after.after_len as isize - before_len as isize;
        if let Some(old_range) = old_ranges.first().cloned() {
            app.update_snippet_session_after_active_edit(buffer_id, old_range, after.cursor, delta);
        }
    }
}

fn auto_closing_edit_enabled(strategy: EditorAutoClosingEditStrategy) -> bool {
    !matches!(strategy, EditorAutoClosingEditStrategy::Never)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PasteTransformPlan {
    pub(crate) multi_cursor_paste: EditorMultiCursorPaste,
    pub(crate) auto_indent_on_paste: bool,
    pub(crate) auto_indent_on_paste_within_string: bool,
    pub(crate) format_on_paste: bool,
}

pub(crate) fn editor_paste_transform_plan(
    paste_as_enabled: bool,
    multi_cursor_paste: EditorMultiCursorPaste,
    auto_indent_on_paste: bool,
    auto_indent_on_paste_within_string: bool,
    format_on_paste: bool,
) -> PasteTransformPlan {
    if paste_as_enabled {
        PasteTransformPlan {
            multi_cursor_paste,
            auto_indent_on_paste,
            auto_indent_on_paste_within_string,
            format_on_paste,
        }
    } else {
        PasteTransformPlan {
            multi_cursor_paste: EditorMultiCursorPaste::Full,
            auto_indent_on_paste: false,
            auto_indent_on_paste_within_string: false,
            format_on_paste: false,
        }
    }
}

fn paste_selector_visible_after_paste(
    paste_as_enabled: bool,
    show_selector: EditorPasteAsShowPasteSelector,
    plan: PasteTransformPlan,
    cursor_count: usize,
    text: &str,
) -> bool {
    paste_as_enabled
        && matches!(show_selector, EditorPasteAsShowPasteSelector::AfterPaste)
        && paste_has_transform_choice(plan, cursor_count, text)
}

fn paste_has_transform_choice(plan: PasteTransformPlan, cursor_count: usize, text: &str) -> bool {
    plan.format_on_paste
        || (plan.auto_indent_on_paste && paste_text_has_line_ending(text))
        || (matches!(plan.multi_cursor_paste, EditorMultiCursorPaste::Spread)
            && spread_paste_segment_count_matches(text, cursor_count))
}

#[cfg(test)]
fn paste_text_at_editor_cursors(
    buffer: &mut TextBuffer,
    text: &str,
    mode: EditorMultiCursorPaste,
    auto_indent_on_paste: bool,
    auto_indent_on_paste_within_string: bool,
) -> bool {
    let Some(text) = normalized_editor_paste_text(text) else {
        return false;
    };

    paste_normalized_text_at_editor_cursors(
        buffer,
        text.as_ref(),
        mode,
        auto_indent_on_paste,
        auto_indent_on_paste_within_string,
    )
}

fn paste_normalized_text_at_editor_cursors(
    buffer: &mut TextBuffer,
    text: &str,
    mode: EditorMultiCursorPaste,
    auto_indent_on_paste: bool,
    auto_indent_on_paste_within_string: bool,
) -> bool {
    let has_line_ending = paste_text_has_line_ending(text);
    if (!auto_indent_on_paste || !has_line_ending)
        && !paste_requires_distinct_cursor_texts(buffer, text, mode, has_line_ending)
    {
        buffer.insert_at_cursors(text);
        return true;
    }

    let texts = paste_texts_for_editor_cursors(
        buffer,
        text,
        mode,
        auto_indent_on_paste,
        auto_indent_on_paste_within_string,
    );
    buffer.insert_texts_at_cursors(texts)
}

fn paste_requires_distinct_cursor_texts(
    buffer: &TextBuffer,
    text: &str,
    mode: EditorMultiCursorPaste,
    has_line_ending: bool,
) -> bool {
    has_line_ending
        && matches!(mode, EditorMultiCursorPaste::Spread)
        && spread_paste_segment_count_matches(text, buffer.selections().len())
}

fn paste_texts_for_editor_cursors(
    buffer: &TextBuffer,
    text: &str,
    mode: EditorMultiCursorPaste,
    auto_indent_on_paste: bool,
    auto_indent_on_paste_within_string: bool,
) -> Vec<String> {
    let selections = buffer.selections();
    let cursor_count = selections.len();
    let segments = if matches!(mode, EditorMultiCursorPaste::Spread) {
        spread_paste_segments(text, cursor_count)
            .unwrap_or_else(|| vec![text.to_owned(); cursor_count])
    } else {
        vec![text.to_owned(); cursor_count]
    };

    if !auto_indent_on_paste {
        return segments;
    }

    segments
        .into_iter()
        .zip(selections)
        .map(|(segment, selection)| {
            auto_indented_paste_text(
                buffer,
                selection.cursor,
                &segment,
                auto_indent_on_paste_within_string,
            )
        })
        .collect()
}

fn spread_paste_segments(text: &str, cursor_count: usize) -> Option<Vec<String>> {
    if !spread_paste_segment_count_matches(text, cursor_count) {
        return None;
    }

    let normalized = paste_text_with_lf_line_endings(text);
    let segment_text = normalized.strip_suffix('\n').unwrap_or(normalized.as_ref());
    let mut lines = Vec::with_capacity(cursor_count);
    for line in segment_text.split('\n') {
        lines.push(line.to_owned());
    }

    Some(lines)
}

fn spread_paste_segment_count_matches(text: &str, cursor_count: usize) -> bool {
    if cursor_count <= 1 || text.is_empty() {
        return false;
    }

    let bytes = text.as_bytes();
    let mut line_count = 1usize;
    let mut ended_with_line_ending = false;
    let mut index = 0usize;
    let max_possible_match_line_count = cursor_count.saturating_add(1);
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                line_count = line_count.saturating_add(1);
                ended_with_line_ending = true;
                index += 1;
                if index < bytes.len() && bytes[index] == b'\n' {
                    index += 1;
                }
            }
            b'\n' => {
                line_count = line_count.saturating_add(1);
                ended_with_line_ending = true;
                index += 1;
            }
            _ => {
                ended_with_line_ending = false;
                index += 1;
            }
        }
        if line_count > max_possible_match_line_count {
            return false;
        }
    }

    if ended_with_line_ending {
        line_count = line_count.saturating_sub(1);
    }
    line_count == cursor_count
}

fn paste_text_with_lf_line_endings(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }

    let bytes = text.as_bytes();
    let mut normalized = String::with_capacity(text.len());
    let mut segment_start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            normalized.push_str(&text[segment_start..index]);
            normalized.push('\n');
            index += 1;
            if index < bytes.len() && bytes[index] == b'\n' {
                index += 1;
            }
            segment_start = index;
        } else {
            index += 1;
        }
    }
    normalized.push_str(&text[segment_start..]);
    Cow::Owned(normalized)
}

fn paste_text_has_line_ending(text: &str) -> bool {
    text.as_bytes()
        .iter()
        .any(|byte| matches!(*byte, b'\n' | b'\r'))
}

fn auto_indented_paste_text(
    buffer: &TextBuffer,
    cursor: usize,
    text: &str,
    auto_indent_on_paste_within_string: bool,
) -> String {
    if !paste_text_has_line_ending(text) {
        return text.to_owned();
    }

    let position = buffer.char_position(cursor);
    let prefix = line_prefix_info(buffer, position.line, position.column).unwrap_or_default();
    if !auto_indent_on_paste_within_string && prefix.inside_string {
        return text.to_owned();
    }

    reindent_multiline_paste(text, &prefix.leading_whitespace)
}

fn reindent_multiline_paste(text: &str, target_indent: &str) -> String {
    let normalized = paste_text_with_lf_line_endings(text);
    let lines = normalized.split('\n');
    let common_indent = common_non_empty_indent_len(lines.clone());
    if target_indent.is_empty() && common_indent == 0 {
        return normalized.into_owned();
    }
    let mut output = String::with_capacity(normalized.len());

    for (index, line) in lines.enumerate() {
        if index > 0 {
            output.push('\n');
        }
        let stripped = strip_indent_chars(line, common_indent);
        if index != 0 && !stripped.trim().is_empty() {
            output.push_str(target_indent);
        }
        output.push_str(stripped);
    }

    output
}

fn common_non_empty_indent_len<'a>(lines: impl Iterator<Item = &'a str>) -> usize {
    lines
        .filter(|line| !line.trim().is_empty())
        .map(leading_whitespace_len)
        .min()
        .unwrap_or(0)
}

fn strip_indent_chars(text: &str, count: usize) -> &str {
    if count == 0 {
        return text;
    }

    let mut removed = 0usize;
    for (byte_idx, ch) in text.char_indices() {
        if removed == count || !matches!(ch, ' ' | '\t') {
            return &text[byte_idx..];
        }
        removed += 1;
    }
    if removed == count { "" } else { text }
}

fn leading_whitespace_len(text: &str) -> usize {
    text.chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .count()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct LinePrefixInfo {
    leading_whitespace: String,
    inside_string: bool,
}

fn line_prefix_info(buffer: &TextBuffer, line: usize, column: usize) -> Option<LinePrefixInfo> {
    if line >= buffer.len_lines() {
        return None;
    }

    let start = buffer.line_column_to_char(line, 0);
    let end = buffer.line_column_to_char(line, column);
    let mut info = LinePrefixInfo::default();
    let mut in_leading_whitespace = true;
    let mut single = false;
    let mut double = false;
    let mut escaped = false;

    for char_idx in start..end {
        let Some(ch) = buffer.char_at(char_idx) else {
            break;
        };
        if in_leading_whitespace && matches!(ch, ' ' | '\t') {
            info.leading_whitespace.push(ch);
        } else {
            in_leading_whitespace = false;
        }

        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match ch {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            _ => {}
        }
    }

    info.inside_string = single || double;
    Some(info)
}

#[cfg(test)]
fn line_prefix_looks_inside_string(prefix: &str) -> bool {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for ch in prefix.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        match ch {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            _ => {}
        }
    }
    single || double
}

#[cfg(test)]
mod tests;

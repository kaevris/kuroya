use eframe::egui::{Event, ImeEvent, Key, Modifiers};
use kuroya_core::EditorVimSettings;
use std::collections::VecDeque;

mod insert;
mod normal;

use self::insert::vim_insert_key_event_includes_mutation_for_scan;
use self::normal::vim_normal_key_event_includes_mutation_for_scan;
use super::super::{EditorVimMode, EditorVimPendingKey};
use super::suppression::vim_text_after_suppression;

#[cfg(test)]
pub(in crate::editor_vim_key_events) fn vim_events_include_mutation(
    events: &[Event],
    initial_mode: EditorVimMode,
    initial_pending: Option<EditorVimPendingKey>,
) -> bool {
    vim_events_include_mutation_impl(events, initial_mode, initial_pending, None)
}

pub(crate) fn vim_events_include_mutation_with_settings(
    events: &[Event],
    initial_mode: EditorVimMode,
    initial_pending: Option<EditorVimPendingKey>,
    vim_settings: &EditorVimSettings,
) -> bool {
    vim_events_include_mutation_impl(events, initial_mode, initial_pending, Some(vim_settings))
}

fn vim_events_include_mutation_impl(
    events: &[Event],
    initial_mode: EditorVimMode,
    initial_pending: Option<EditorVimPendingKey>,
    vim_settings: Option<&EditorVimSettings>,
) -> bool {
    let mut mode = initial_mode;
    let mut pending = initial_pending;
    let mut suppressed_text = VecDeque::new();
    for event in events {
        match event {
            Event::Cut => return true,
            Event::Paste(text) if mode.accepts_text_input() => {
                if !text.is_empty() {
                    return true;
                }
            }
            Event::Text(text) | Event::Ime(ImeEvent::Commit(text)) => {
                let text = vim_text_after_suppression(text, &mut suppressed_text);
                if mode.accepts_text_input() && text.is_some() {
                    return true;
                }
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if vim_key_event_includes_mutation_for_scan(
                *key,
                *modifiers,
                &mut mode,
                &mut pending,
                &mut suppressed_text,
                vim_settings,
                true,
            ) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn vim_key_event_includes_mutation_for_scan(
    key: Key,
    modifiers: Modifiers,
    mode: &mut EditorVimMode,
    pending: &mut Option<EditorVimPendingKey>,
    suppressed_text: &mut VecDeque<char>,
    vim_settings: Option<&EditorVimSettings>,
    suppress_text_echo: bool,
) -> bool {
    match *mode {
        EditorVimMode::Insert => vim_insert_key_event_includes_mutation_for_scan(
            key,
            modifiers,
            mode,
            pending,
            suppressed_text,
            vim_settings,
        ),
        EditorVimMode::Normal => vim_normal_key_event_includes_mutation_for_scan(
            key,
            modifiers,
            mode,
            pending,
            suppressed_text,
            vim_settings,
            suppress_text_echo,
        ),
    }
}

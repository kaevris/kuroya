use super::{
    EditorVimCharFind, EditorVimCharFindMotion, EditorVimMode, EditorVimNamedRegister,
    EditorVimPendingKey, EditorVimRegister, EditorVimRegisterKind, VIM_DEFAULT_CTRL_SCROLL_LINES,
    VIM_DEFAULT_PAGE_SCROLL_LINES, VIM_SEARCH_INPUT, VIM_SEARCHES, handle_vim_editor_key_event,
    handle_vim_editor_key_event_with_repeat_state,
    handle_vim_editor_key_event_with_settings_and_indent, handle_vim_editor_key_event_with_state,
    handle_vim_editor_key_event_with_state_and_indent, sanitize_vim_settings_for_runtime,
    vim_apply_char_find, vim_clear_marks, vim_clear_named_registers, vim_effective_cursor_style,
    vim_events_include_mutation, vim_events_include_mutation_with_settings,
    vim_key_sequence_is_normal_mode_supported, vim_mode_status_label, vim_named_register,
    vim_open_line_above_text, vim_open_line_below_text, vim_pending_command_status_label,
    vim_pending_key_sequence_status_label, vim_pending_search_status_label,
    vim_record_insert_replay_key_with_auto_indent, vim_record_inserted_text,
    vim_search_highlight_ranges_for_buffer, vim_search_word_target, vim_set_last_search,
    vim_text_after_suppression,
};
use eframe::egui::{Event, Key, Modifiers};
use kuroya_core::{EditorCursorStyle, EditorVimKeyOverride, EditorVimSettings, TextBuffer};
use std::collections::VecDeque;

mod case;
mod char_find;
mod edits;
mod ex_commands;
mod insert;
mod motion_audit;
mod motions;
mod operators;
mod overrides;
mod qa_ex;
mod qa_insert;
mod qa_marks;
mod qa_operators;
mod qa_registers;
mod qa_text_objects;
mod qa_undo_repeat;
mod qa_visual;
mod registers;
mod scroll;
mod search;
mod substitute;
mod text_objects;
mod visual;

#[test]
fn vim_status_labels_expose_mode_and_pending_commands() {
    assert_eq!(vim_mode_status_label(EditorVimMode::Normal, None), "NORMAL");
    assert_eq!(vim_mode_status_label(EditorVimMode::Insert, None), "INSERT");
    assert_eq!(
        vim_mode_status_label(
            EditorVimMode::Normal,
            Some(EditorVimPendingKey::VisualCharacter {
                anchor: 1,
                cursor: 3,
            }),
        ),
        "VISUAL"
    );
    assert_eq!(
        vim_mode_status_label(
            EditorVimMode::Normal,
            Some(EditorVimPendingKey::ReplaceChar(1))
        ),
        "REPLACE"
    );

    let register_a = EditorVimNamedRegister {
        index: 0,
        append: false,
    };
    assert_eq!(
        vim_pending_command_status_label(Some(EditorVimPendingKey::Count(3))).as_deref(),
        Some("3")
    );
    assert_eq!(
        vim_pending_command_status_label(Some(EditorVimPendingKey::DeleteLine(3))).as_deref(),
        Some("3d")
    );
    assert_eq!(
        vim_pending_command_status_label(Some(EditorVimPendingKey::ChangeTextObject {
            operator_count: 1,
            motion_count: 1,
            scope: super::EditorVimTextObjectScope::Inner,
        }))
        .as_deref(),
        Some("ci")
    );
    assert_eq!(
        vim_pending_command_status_label(Some(EditorVimPendingKey::RegisterCommand {
            prefix_count: 1,
            command_count: None,
            register: register_a,
        }))
        .as_deref(),
        Some("\"a")
    );
    assert_eq!(
        vim_pending_command_status_label(Some(EditorVimPendingKey::YankLineIntoRegister {
            operator_count: 1,
            register: register_a,
        }))
        .as_deref(),
        Some("\"ay")
    );
    assert_eq!(
        vim_pending_command_status_label(Some(EditorVimPendingKey::SearchInput {
            count: 1,
            forward: true,
        })),
        None
    );
}

#[test]
fn vim_effective_cursor_style_tracks_mode_when_enabled() {
    assert_eq!(
        vim_effective_cursor_style(
            EditorCursorStyle::Underline,
            false,
            EditorVimMode::Normal,
            None,
        ),
        EditorCursorStyle::Underline
    );
    assert_eq!(
        vim_effective_cursor_style(EditorCursorStyle::Line, true, EditorVimMode::Normal, None),
        EditorCursorStyle::Block
    );
    assert_eq!(
        vim_effective_cursor_style(EditorCursorStyle::Block, true, EditorVimMode::Insert, None),
        EditorCursorStyle::LineThin
    );
    assert_eq!(
        vim_effective_cursor_style(
            EditorCursorStyle::Line,
            true,
            EditorVimMode::Insert,
            Some(EditorVimPendingKey::VisualCharacter {
                anchor: 1,
                cursor: 3,
            }),
        ),
        EditorCursorStyle::Block
    );
}

#[test]
fn vim_normal_mode_sequence_support_rejects_parseable_unhandled_keys() {
    assert!(vim_key_sequence_is_normal_mode_supported("gg"));
    assert!(vim_key_sequence_is_normal_mode_supported("diw"));
    assert!(vim_key_sequence_is_normal_mode_supported("<C-n>"));
    assert!(vim_key_sequence_is_normal_mode_supported("\"ayy"));

    assert!(!vim_key_sequence_is_normal_mode_supported("<Left>"));
    assert!(!vim_key_sequence_is_normal_mode_supported("<Tab>"));
    assert!(!vim_key_sequence_is_normal_mode_supported("z"));
}

#[test]
fn vim_normal_mode_sequence_support_does_not_mutate_registers() {
    vim_clear_named_registers();

    assert!(vim_key_sequence_is_normal_mode_supported("\"ayy"));
    assert_eq!(
        vim_named_register(EditorVimNamedRegister {
            index: 0,
            append: false,
        }),
        None
    );
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

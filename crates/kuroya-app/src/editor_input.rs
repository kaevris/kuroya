use eframe::egui::{Event, ImeEvent, Key, Modifiers};
use kuroya_core::BufferId;
use std::{borrow::Cow, collections::HashSet};

#[derive(Debug, Clone, Copy)]
pub(crate) enum EditorContextAction {
    Copy,
    Cut,
    Undo,
    Redo,
    SelectAll,
    SelectLines,
    SelectRectangularBlock,
    ExpandSelection,
    FindSelection,
    ShowHover,
    DocumentHighlights,
    GoToDefinition,
    FindReferences,
    ShowCallHierarchy,
    ShowTypeHierarchy,
    RenameSymbol,
    ShowSymbols,
    WorkspaceSymbols,
    ShowCompletions,
    SignatureHelp,
    LoadFolds,
    ToggleFold,
    ExpandAllFolds,
    FormatDocument,
    CodeActions,
    CopyDiffPatch,
    CopyDiffHunkPatch,
    RefreshDiff,
    SwapDiffSides,
    PreviousDiffHunk,
    NextDiffHunk,
    PreviousGitChange,
    NextGitChange,
    OpenActiveFileChanges,
    OpenActiveFileStagedChanges,
    CopyActiveFilePatch,
    CopyActiveFileStagedPatch,
    OpenActiveFileHunks,
    OpenActiveFileStagedHunks,
    OpenActiveFileHunkDiff,
    OpenActiveFileStagedHunkDiff,
    OpenAccessibleDiffViewer,
    CopyActiveFileHunkPatch,
    CopyActiveFileStagedHunkPatch,
    SelectActiveFileForCompare,
    CompareActiveFileWithSelected,
    CompareActiveFileWithSaved,
    CopyActivePath,
    CopyActiveRelativePath,
    OpenDiffBaseFile,
    OpenDiffBaseAtCurrentHunk,
    OpenDiffSourceFile,
    OpenDiffSourceAtCurrentHunk,
    OpenDiffSourceBlame,
    OpenActiveFileHeadChanges,
    OpenActiveFileHeadRevision,
    OpenActiveFileIndexRevision,
    RevealActiveFileInExplorer,
    RevealActiveFileInSourceControl,
    AcceptCurrentConflictAtLine(usize),
    AcceptIncomingConflictAtLine(usize),
    AcceptBothConflictsAtLine(usize),
    StageActiveFileChanges,
    StageActiveFileHunk,
    StageActiveDiffHunk,
    UnstageActiveFileChanges,
    UnstageActiveFileHunk,
    UnstageActiveDiffHunk,
    DiscardActiveFileChanges,
    DiscardActiveFileHunk,
    DiscardActiveDiffHunk,
    DuplicateLines,
    MoveLineUp,
    MoveLineDown,
    ToggleLineComment,
    DeleteLines,
    JoinLines,
    IndentLines,
    OutdentLines,
    AddCursorsToLineEnds,
}

pub(crate) fn protected_preview_edit_block_reason(
    id: BufferId,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> Option<&'static str> {
    if binary_buffers.contains(&id) {
        Some("binary previews are read-only")
    } else if lossy_buffers.contains(&id) {
        Some("UTF-8 replacement previews are read-only")
    } else {
        None
    }
}

pub(crate) fn editor_context_action_edits_buffer(action: EditorContextAction) -> bool {
    matches!(
        action,
        EditorContextAction::Cut
            | EditorContextAction::DuplicateLines
            | EditorContextAction::MoveLineUp
            | EditorContextAction::MoveLineDown
            | EditorContextAction::ToggleLineComment
            | EditorContextAction::DeleteLines
            | EditorContextAction::JoinLines
            | EditorContextAction::IndentLines
            | EditorContextAction::OutdentLines
            | EditorContextAction::AcceptCurrentConflictAtLine(_)
            | EditorContextAction::AcceptIncomingConflictAtLine(_)
            | EditorContextAction::AcceptBothConflictsAtLine(_)
            | EditorContextAction::DiscardActiveFileHunk
    )
}

#[cfg(test)]
pub(crate) fn editor_events_include_mutation(events: &[Event]) -> bool {
    events.iter().any(|event| match event {
        Event::Cut => true,
        Event::Paste(text) => editor_paste_text_has_insertable_content(text),
        Event::Text(text) => normalized_editor_text_input(text).is_some(),
        Event::Ime(ImeEvent::Commit(text)) => normalized_editor_ime_commit_text(text).is_some(),
        Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } => editor_key_can_mutate(*key, *modifiers),
        _ => false,
    })
}

pub(crate) fn normalized_editor_paste_text(text: &str) -> Option<Cow<'_, str>> {
    if text.is_empty() {
        return None;
    }

    let Some(first_slow_byte) = text
        .bytes()
        .position(|byte| !editor_paste_ascii_byte_allowed(byte))
    else {
        return Some(Cow::Borrowed(text));
    };

    let mut sanitized = None::<String>;
    for (offset, ch) in text[first_slow_byte..].char_indices() {
        let index = first_slow_byte + offset;
        if editor_paste_char_allowed(ch) {
            if let Some(output) = sanitized.as_mut() {
                output.push(ch);
            }
        } else {
            sanitized.get_or_insert_with(|| text[..index].to_owned());
        }
    }

    let text = sanitized.map(Cow::Owned).unwrap_or(Cow::Borrowed(text));
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
pub(crate) fn editor_paste_text_has_insertable_content(text: &str) -> bool {
    for (index, byte) in text.bytes().enumerate() {
        if editor_paste_ascii_byte_allowed(byte) {
            return true;
        }
        if byte >= 0x80 {
            return text[index..].chars().any(editor_paste_char_allowed);
        }
    }
    false
}

pub(crate) fn editor_text_input_from_event(event: &Event) -> Option<&str> {
    match event {
        Event::Text(text) | Event::Ime(ImeEvent::Commit(text)) => {
            normalized_editor_text_input(text)
        }
        _ => None,
    }
}

pub(crate) fn normalized_editor_text_input(text: &str) -> Option<&str> {
    let scan = scan_editor_text_input_ascii(text)?;
    if scan.has_non_ascii && text.chars().any(char::is_control) {
        return None;
    }
    Some(text)
}

pub(crate) fn normalized_editor_ime_commit_text(text: &str) -> Option<Cow<'_, str>> {
    if text.is_empty() {
        return None;
    }

    if let Some(scan) = scan_editor_text_input_ascii(text)
        && (!scan.has_non_ascii || !text.chars().any(char::is_control))
    {
        return Some(Cow::Borrowed(text));
    }

    let mut sanitized = String::with_capacity(text.len());
    for ch in text.chars() {
        if !ch.is_control() {
            sanitized.push(ch);
        }
    }
    (!sanitized.is_empty()).then_some(Cow::Owned(sanitized))
}

pub(crate) fn normalized_editor_text_input_with_fast_coalesce(text: &str) -> Option<(&str, bool)> {
    let scan = scan_editor_text_input_ascii(text)?;
    if !scan.has_non_ascii {
        return Some((text, scan.fast_ascii));
    }

    let mut fast_chars = true;
    for ch in text.chars() {
        if ch.is_control() {
            return None;
        }
        if !editor_fast_text_char_allowed(ch) {
            fast_chars = false;
        }
    }
    Some((text, scan.fast_ascii && fast_chars))
}

fn editor_paste_char_allowed(ch: char) -> bool {
    !ch.is_control() || matches!(ch, '\n' | '\r' | '\t')
}

fn editor_paste_ascii_byte_allowed(byte: u8) -> bool {
    byte.is_ascii() && (!byte.is_ascii_control() || matches!(byte, b'\n' | b'\r' | b'\t'))
}

#[derive(Clone, Copy)]
struct EditorTextInputAsciiScan {
    has_non_ascii: bool,
    fast_ascii: bool,
}

fn scan_editor_text_input_ascii(text: &str) -> Option<EditorTextInputAsciiScan> {
    if text.is_empty() {
        return None;
    }

    let mut has_non_ascii = false;
    let mut fast_ascii = true;
    for byte in text.bytes() {
        if byte < b' ' || byte == 0x7f {
            return None;
        }
        if editor_fast_text_ascii_byte_allowed(byte) {
            continue;
        }
        if byte >= 0x80 {
            has_non_ascii = true;
        } else {
            fast_ascii = false;
        }
    }

    Some(EditorTextInputAsciiScan {
        has_non_ascii,
        fast_ascii,
    })
}

fn editor_fast_text_ascii_byte_allowed(byte: u8) -> bool {
    matches!(byte, b'_' | b'.' | b':') || byte.is_ascii_alphanumeric()
}

fn editor_fast_text_char_allowed(ch: char) -> bool {
    matches!(ch, '_' | '.' | ':') || ch.is_alphanumeric()
}

pub(crate) fn editor_key_can_mutate(key: Key, modifiers: Modifiers) -> bool {
    if (modifiers.ctrl || modifiers.alt) && matches!(key, Key::Backspace | Key::Delete) {
        true
    } else if modifiers.command || modifiers.ctrl {
        matches!(key, Key::Z | Key::Y)
    } else {
        matches!(key, Key::Backspace | Key::Delete | Key::Enter | Key::Tab)
    }
}

#[cfg(test)]
mod tests {
    use super::{normalized_editor_ime_commit_text, normalized_editor_paste_text};
    use eframe::egui::{Event, ImeEvent};
    use std::borrow::Cow;

    #[test]
    fn normalized_editor_paste_text_borrows_clean_ascii() {
        let text = "alpha_beta\r\nnext\tline";

        assert!(matches!(
            normalized_editor_paste_text(text),
            Some(Cow::Borrowed("alpha_beta\r\nnext\tline"))
        ));
    }

    #[test]
    fn normalized_editor_paste_text_strips_unicode_controls() {
        assert_eq!(
            normalized_editor_paste_text("ab\u{85}cd\u{e9}").as_deref(),
            Some("abcd\u{e9}")
        );
        assert!(matches!(
            normalized_editor_paste_text("ab\u{e9}"),
            Some(Cow::Borrowed("ab\u{e9}"))
        ));
    }

    #[test]
    fn normalized_editor_ime_commit_text_strips_controls_without_dropping_text() {
        assert!(matches!(
            normalized_editor_ime_commit_text("\u{6587}\u{5b57}"),
            Some(Cow::Borrowed("\u{6587}\u{5b57}"))
        ));
        assert_eq!(
            normalized_editor_ime_commit_text("\u{6587}\u{0}\u{85}\u{5b57}").as_deref(),
            Some("\u{6587}\u{5b57}")
        );
        assert_eq!(normalized_editor_ime_commit_text("\u{0}\n\t"), None);
    }

    #[test]
    fn paste_text_insertable_content_check_matches_sanitizer() {
        assert!(super::editor_paste_text_has_insertable_content("a\u{0}"));
        assert!(super::editor_paste_text_has_insertable_content("\n"));
        assert!(super::editor_paste_text_has_insertable_content("\u{e9}"));
        assert!(!super::editor_paste_text_has_insertable_content(
            "\u{0}\u{1b}\u{7f}"
        ));
    }

    #[test]
    fn ime_commit_mutation_check_matches_sanitizer() {
        assert!(super::editor_events_include_mutation(&[Event::Ime(
            ImeEvent::Commit("\u{6587}\u{0}".to_owned())
        )]));
        assert!(!super::editor_events_include_mutation(&[Event::Ime(
            ImeEvent::Commit("\u{0}\n\t".to_owned())
        )]));
    }
}

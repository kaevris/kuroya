use super::{
    EditorVimCaseConversion, EditorVimNamedRegister, EditorVimOperatorMotion,
    EditorVimTextObjectKind, EditorVimTextObjectScope,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorVimLastChange {
    pub(crate) action: EditorVimRepeatAction,
    pub(crate) count: usize,
    pub(crate) insert_replay: Vec<EditorVimInsertReplayStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditorVimRepeatAction {
    AppendAfterCursor,
    ChangeLines,
    ChangeLinesIntoRegister(EditorVimNamedRegister),
    ChangeOperatorMotion(EditorVimOperatorMotion),
    ChangeOperatorMotionIntoRegister {
        motion: EditorVimOperatorMotion,
        register: EditorVimNamedRegister,
    },
    ChangeTextObject {
        scope: EditorVimTextObjectScope,
        kind: EditorVimTextObjectKind,
    },
    ChangeTextObjectIntoRegister {
        scope: EditorVimTextObjectScope,
        kind: EditorVimTextObjectKind,
        register: EditorVimNamedRegister,
    },
    ChangeToLineEnd,
    ChangeToLineEndIntoRegister(EditorVimNamedRegister),
    DeleteBackwardChars,
    DeleteBackwardCharsIntoRegister(EditorVimNamedRegister),
    DeleteForwardChars,
    DeleteForwardCharsIntoRegister(EditorVimNamedRegister),
    DeleteLines,
    DeleteLinesIntoRegister(EditorVimNamedRegister),
    DeleteOperatorMotion(EditorVimOperatorMotion),
    DeleteOperatorMotionIntoRegister {
        motion: EditorVimOperatorMotion,
        register: EditorVimNamedRegister,
    },
    DeleteTextObject {
        scope: EditorVimTextObjectScope,
        kind: EditorVimTextObjectKind,
    },
    DeleteTextObjectIntoRegister {
        scope: EditorVimTextObjectScope,
        kind: EditorVimTextObjectKind,
        register: EditorVimNamedRegister,
    },
    DeleteToLineEnd,
    DeleteToLineEndIntoRegister(EditorVimNamedRegister),
    IndentLines,
    InsertAtCursor,
    InsertLineEnd,
    InsertLineFirstNonWhitespace,
    JoinLines,
    JoinLinesWithoutWhitespace,
    OpenLineAbove,
    OpenLineBelow,
    OutdentLines,
    PutAfter,
    PutAfterNamed(EditorVimNamedRegister),
    PutBefore,
    PutBeforeNamed(EditorVimNamedRegister),
    ReplaceForwardChars(char),

    Substitute {
        query: String,
        replacement: String,
        global: bool,
    },
    SubstituteForwardChars,
    SubstituteForwardCharsIntoRegister(EditorVimNamedRegister),
    ConvertCaseForwardChars(EditorVimCaseConversion),
    ConvertCaseLines(EditorVimCaseConversion),
    ConvertCaseOperatorMotion {
        motion: EditorVimOperatorMotion,
        conversion: EditorVimCaseConversion,
    },
    ConvertCaseTextObject {
        scope: EditorVimTextObjectScope,
        kind: EditorVimTextObjectKind,
        conversion: EditorVimCaseConversion,
    },
    ToggleCaseForwardChars,
    ToggleCaseOperatorMotion(EditorVimOperatorMotion),
    ToggleCaseTextObject {
        scope: EditorVimTextObjectScope,
        kind: EditorVimTextObjectKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditorVimInsertReplayStep {
    Backspace,
    DeleteLineBackward,
    DeleteWordBackward,
    Enter,
    EnterAutoIndent,
    InsertText(String),
    Tab,
    ShiftTab,
}

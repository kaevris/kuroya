use super::{EditorVimCaseConversion, EditorVimCharFindMotion, EditorVimNamedRegister};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorVimOperatorMotion {
    BigWordBackward,
    BigWordEnd,
    BigWordEndBackward,
    BigWordForward,
    CharFind {
        motion: EditorVimCharFindMotion,
        target: char,
    },
    CharacterBackward,
    CharacterForward,
    FirstLine,
    LastLine,
    LineColumn,
    LineColumnStart,
    LineDown,
    LineEnd,
    LineFirstNonWhitespace,
    LineUp,
    MatchingBracket,
    ParagraphBackward,
    ParagraphForward,
    SearchRepeat {
        reverse: bool,
    },
    SearchMatch {
        reverse: bool,
    },
    SearchWordUnderCursor {
        forward: bool,
        whole_word: bool,
    },
    WordBackward,
    WordEnd,
    WordEndBackward,
    WordForward,
    WordForwardChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorVimOperatorGoKind {
    Change,
    ChangeIntoRegister(EditorVimNamedRegister),
    ConvertCase(EditorVimCaseConversion),
    Delete,
    DeleteIntoRegister(EditorVimNamedRegister),
    ToggleCase,
    Yank,
    YankIntoRegister(EditorVimNamedRegister),
}

use super::{
    EditorVimCaseConversion, EditorVimCharFindMotion, EditorVimNamedRegister,
    EditorVimOperatorGoKind, EditorVimTextObjectScope,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorVimPendingKey {
    ChangeMotionCount {
        operator_count: usize,
        motion_count: usize,
    },
    ChangeMotionCountIntoRegister {
        operator_count: usize,
        motion_count: usize,
        register: EditorVimNamedRegister,
    },
    ChangeTextObject {
        operator_count: usize,
        motion_count: usize,
        scope: EditorVimTextObjectScope,
    },
    ChangeTextObjectIntoRegister {
        operator_count: usize,
        motion_count: usize,
        scope: EditorVimTextObjectScope,
        register: EditorVimNamedRegister,
    },
    ChangeCharFind {
        operator_count: usize,
        motion_count: usize,
        motion: EditorVimCharFindMotion,
    },
    ChangeCharFindIntoRegister {
        operator_count: usize,
        motion_count: usize,
        motion: EditorVimCharFindMotion,
        register: EditorVimNamedRegister,
    },
    CommandInput,
    Count(usize),
    CustomKeySequence {
        binding_index: usize,
        matched: usize,
    },

    VisualCharacter {
        anchor: usize,
        cursor: usize,
    },

    VisualLine {
        anchor: usize,
        cursor: usize,
        count: Option<usize>,
    },

    VisualLineGo {
        anchor: usize,
        cursor: usize,
        count: Option<usize>,
    },
    VisualCharacterCount {
        anchor: usize,
        cursor: usize,
        count: usize,
    },
    VisualCharacterGo {
        anchor: usize,
        cursor: usize,
        count: Option<usize>,
    },
    VisualCharacterReplace {
        anchor: usize,
        cursor: usize,
    },

    VisualLineReplace {
        anchor: usize,
        cursor: usize,
    },
    VisualCharacterCharFind {
        anchor: usize,
        cursor: usize,
        count: Option<usize>,
        motion: EditorVimCharFindMotion,
    },
    VisualCharacterTextObject {
        anchor: usize,
        cursor: usize,
        count: Option<usize>,
        scope: EditorVimTextObjectScope,
    },
    VisualCharacterRegisterPrefix {
        anchor: usize,
        cursor: usize,
        count: Option<usize>,
    },
    VisualCharacterRegisterCommand {
        anchor: usize,
        cursor: usize,
        count: Option<usize>,
        register: EditorVimNamedRegister,
    },
    RegisterPrefix(usize),
    RegisterCommand {
        prefix_count: usize,
        command_count: Option<usize>,
        register: EditorVimNamedRegister,
    },
    DeleteMotionCount {
        operator_count: usize,
        motion_count: usize,
    },
    DeleteMotionCountIntoRegister {
        operator_count: usize,
        motion_count: usize,
        register: EditorVimNamedRegister,
    },
    DeleteTextObject {
        operator_count: usize,
        motion_count: usize,
        scope: EditorVimTextObjectScope,
    },
    DeleteTextObjectIntoRegister {
        operator_count: usize,
        motion_count: usize,
        scope: EditorVimTextObjectScope,
        register: EditorVimNamedRegister,
    },
    DeleteCharFind {
        operator_count: usize,
        motion_count: usize,
        motion: EditorVimCharFindMotion,
    },
    DeleteCharFindIntoRegister {
        operator_count: usize,
        motion_count: usize,
        motion: EditorVimCharFindMotion,
        register: EditorVimNamedRegister,
    },
    ChangeLine(usize),
    ChangeLineIntoRegister {
        operator_count: usize,
        register: EditorVimNamedRegister,
    },
    DeleteLine(usize),
    DeleteLineIntoRegister {
        operator_count: usize,
        register: EditorVimNamedRegister,
    },
    FindCharBackward(usize),
    FindCharForward(usize),
    Go(Option<usize>),
    IndentLine(usize),
    JumpMark {
        linewise: bool,
    },
    OutdentLine(usize),
    ReplaceChar(usize),
    SearchInput {
        count: usize,
        forward: bool,
    },
    SetMark,
    TillCharBackward(usize),
    TillCharForward(usize),
    OperatorGoMotion {
        operator_count: usize,
        motion_count: usize,
        operator: EditorVimOperatorGoKind,
    },
    ConvertCaseOperator {
        operator_count: usize,
        conversion: EditorVimCaseConversion,
    },
    ConvertCaseMotionCount {
        operator_count: usize,
        motion_count: usize,
        conversion: EditorVimCaseConversion,
    },
    ConvertCaseCharFind {
        operator_count: usize,
        motion_count: usize,
        motion: EditorVimCharFindMotion,
        conversion: EditorVimCaseConversion,
    },
    ConvertCaseTextObject {
        operator_count: usize,
        motion_count: usize,
        scope: EditorVimTextObjectScope,
        conversion: EditorVimCaseConversion,
    },
    ToggleCaseOperator(usize),
    ToggleCaseMotionCount {
        operator_count: usize,
        motion_count: usize,
    },
    ToggleCaseCharFind {
        operator_count: usize,
        motion_count: usize,
        motion: EditorVimCharFindMotion,
    },
    ToggleCaseTextObject {
        operator_count: usize,
        motion_count: usize,
        scope: EditorVimTextObjectScope,
    },
    YankLine(usize),
    YankLineIntoRegister {
        operator_count: usize,
        register: EditorVimNamedRegister,
    },
    YankMotionCount {
        operator_count: usize,
        motion_count: usize,
    },
    YankMotionCountIntoRegister {
        operator_count: usize,
        motion_count: usize,
        register: EditorVimNamedRegister,
    },
    YankTextObject {
        operator_count: usize,
        motion_count: usize,
        scope: EditorVimTextObjectScope,
    },
    YankTextObjectIntoRegister {
        operator_count: usize,
        motion_count: usize,
        scope: EditorVimTextObjectScope,
        register: EditorVimNamedRegister,
    },
    YankCharFind {
        operator_count: usize,
        motion_count: usize,
        motion: EditorVimCharFindMotion,
    },
    YankCharFindIntoRegister {
        operator_count: usize,
        motion_count: usize,
        motion: EditorVimCharFindMotion,
        register: EditorVimNamedRegister,
    },
}

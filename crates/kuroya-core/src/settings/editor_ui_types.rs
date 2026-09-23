use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorAutoSaveMode {
    Off,
    #[default]
    AfterDelay,
    OnFocusChange,
    OnWindowChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditorCursorStyle {
    #[default]
    Line,
    Block,
    Underline,
    LineThin,
    BlockOutline,
    UnderlineThin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMouseStyle {
    #[default]
    Text,
    #[serde(rename = "default")]
    SystemDefault,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorCursorSmoothCaretAnimation {
    #[default]
    Off,
    Explicit,
    On,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorAccessibilitySupport {
    #[default]
    Auto,
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorPeekWidgetDefaultFocus {
    #[default]
    Tree,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorUnusualLineTerminators {
    Auto,
    Off,
    #[default]
    Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorGotoLocationMultiple {
    #[default]
    Peek,
    GotoAndPeek,
    Goto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorCursorSurroundingLinesStyle {
    #[default]
    Default,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMinimapSide {
    Left,
    #[default]
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMinimapAutohide {
    #[default]
    None,
    Mouseover,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMinimapSize {
    #[default]
    Proportional,
    Fill,
    Fit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMinimapShowSlider {
    Always,
    #[default]
    Mouseover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorScrollbarVisibility {
    #[default]
    Auto,
    Visible,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorDefaultColorDecorators {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorColorDecoratorsActivatedOn {
    #[default]
    ClickAndHover,
    Hover,
    Click,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorWordBreak {
    #[default]
    Normal,
    KeepAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorStickyScrollDefaultModel {
    #[default]
    OutlineModel,
    FoldingProviderModel,
    IndentationModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorMultiCursorModifier {
    #[default]
    Alt,
    CtrlCmd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorMultiCursorPaste {
    #[default]
    Spread,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorMouseMiddleClickAction {
    #[default]
    Default,
    OpenLink,
    CtrlLeftClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorShowFoldingControls {
    Always,
    Never,
    #[default]
    Mouseover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorFoldingStrategy {
    #[default]
    Auto,
    Indentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorWordWrap {
    #[default]
    Off,
    On,
    WordWrapColumn,
    Bounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorWordWrapOverride {
    Off,
    On,
    #[default]
    Inherit,
}

impl EditorWordWrapOverride {
    pub fn resolve(self, base: EditorWordWrap) -> EditorWordWrap {
        match self {
            Self::Off => EditorWordWrap::Off,
            Self::On => EditorWordWrap::On,
            Self::Inherit => base,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorWrappingIndent {
    None,
    #[default]
    Same,
    Indent,
    DeepIndent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorWrappingStrategy {
    #[default]
    Simple,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorDropIntoEditorShowDropSelector {
    #[default]
    AfterDrop,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EditorPasteAsShowPasteSelector {
    #[default]
    AfterPaste,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffWordWrap {
    Off,
    On,
    #[default]
    Inherit,
}

impl DiffWordWrap {
    pub fn resolve(self, inherited: EditorWordWrap) -> EditorWordWrap {
        match self {
            Self::Off => EditorWordWrap::Off,
            Self::On => EditorWordWrap::On,
            Self::Inherit => inherited,
        }
    }
}

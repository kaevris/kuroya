#[cfg(test)]
use crate::buffer::TextBuffer;
use crate::{
    buffer::DEFAULT_WORD_SEPARATORS,
    command::Command,
    git::{
        DEFAULT_DIFF_CONTEXT_LINES, DEFAULT_DIFF_HIDE_UNCHANGED_REGIONS_MINIMUM_LINE_COUNT,
        DEFAULT_DIFF_HIDE_UNCHANGED_REGIONS_REVEAL_LINE_COUNT,
        DEFAULT_DIFF_MAX_COMPUTATION_TIME_MS, DEFAULT_DIFF_MAX_FILE_SIZE_MB,
        DEFAULT_GIT_COMMIT_SHORT_HASH_LENGTH, DEFAULT_GIT_DETECT_SUBMODULES_LIMIT,
        DEFAULT_GIT_SIMILARITY_THRESHOLD, DEFAULT_GIT_STATUS_LIMIT, DiffAlgorithm, GitCheckoutType,
        GitSmartCommitChanges, GitTimelineDate,
    },
    keymap::Keymap,
    lsp::{LspServerConfig, default_server_configs},
    project::{
        DEFAULT_PROJECT_INDEX_MAX_FILES, MAX_PROJECT_INDEX_MAX_FILES, MIN_PROJECT_INDEX_MAX_FILES,
        default_project_index_exclude_globs,
    },
};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

mod clamp;
mod deserialize;
mod editor_option_types;
mod editor_settings_impl;
mod editor_ui_types;
mod git_branch;
mod git_scm_types;
mod io;
mod minimap;
mod sanitize;
mod terminal_types;
use clamp::clamp_editor_line_decorations_width_ch;
pub use clamp::*;
use deserialize::{
    default_editor_font_ligatures, default_editor_font_variations,
    deserialize_editor_font_ligatures, deserialize_editor_font_variations,
    deserialize_optional_string_list,
};
pub use deserialize::{normalize_editor_font_ligatures, normalize_editor_font_variations};
pub use editor_option_types::*;
pub use editor_ui_types::*;
pub use git_branch::git_branch_validation_error;
pub use git_scm_types::*;
use io::{
    atomic_write, parse_settings_text_with_known_recovery, quarantine_corrupt_settings,
    read_settings_text_with_limit, settings_read_error_is_not_found,
    settings_schema_version_newer_than_supported,
};
#[cfg(test)]
use io::{parse_settings_text, settings_schema_version_from_toml};
#[cfg(test)]
use minimap::MINIMAP_SECTION_HEADER_SCAN_CHAR_LIMIT;
pub use minimap::minimap_section_header_lines;
use sanitize::*;
pub use terminal_types::*;

pub const DEFAULT_TERMINAL_SCROLLBACK_ROWS: usize = 10_000;
pub const MIN_TERMINAL_SCROLLBACK_ROWS: usize = 100;
pub const MAX_TERMINAL_SCROLLBACK_ROWS: usize = 50_000;
pub const DEFAULT_TERMINAL_MOUSE_WHEEL_SCROLL_SENSITIVITY: f32 = 1.0;
pub const DEFAULT_TERMINAL_FAST_SCROLL_SENSITIVITY: f32 = 5.0;
pub const MIN_TERMINAL_SCROLL_SENSITIVITY: f32 = 0.0;
pub const MAX_TERMINAL_SCROLL_SENSITIVITY: f32 = 100.0;
pub const DEFAULT_TERMINAL_MOUSE_WHEEL_ZOOM: bool = false;
pub const DEFAULT_TERMINAL_MIN_ROWS: u16 = 4;
pub const MIN_TERMINAL_MIN_ROWS: u16 = 1;
pub const MAX_TERMINAL_MIN_ROWS: u16 = 80;
pub const DEFAULT_TERMINAL_MIN_COLUMNS: u16 = 20;
pub const MIN_TERMINAL_MIN_COLUMNS: u16 = 10;
pub const MAX_TERMINAL_MIN_COLUMNS: u16 = 240;
pub const DEFAULT_TERMINAL_FONT_SIZE: f32 = 12.0;
pub const MIN_TERMINAL_FONT_SIZE: f32 = 8.0;
pub const MAX_TERMINAL_FONT_SIZE: f32 = 32.0;
pub const DEFAULT_TERMINAL_LINE_HEIGHT: f32 = 1.35;
pub const MIN_TERMINAL_LINE_HEIGHT: f32 = 1.0;
pub const MAX_TERMINAL_LINE_HEIGHT: f32 = 2.0;
pub const DEFAULT_TERMINAL_LETTER_SPACING: f32 = 0.0;
pub const MIN_TERMINAL_LETTER_SPACING: f32 = 0.0;
pub const MAX_TERMINAL_LETTER_SPACING: f32 = 8.0;
pub const DEFAULT_TERMINAL_CURSOR_WIDTH: f32 = 1.0;
pub const MIN_TERMINAL_CURSOR_WIDTH: f32 = 1.0;
pub const MAX_TERMINAL_CURSOR_WIDTH: f32 = 8.0;
pub const DEFAULT_TERMINAL_MINIMUM_CONTRAST_RATIO: f32 = 4.5;
pub const MIN_TERMINAL_MINIMUM_CONTRAST_RATIO: f32 = 1.0;
pub const MAX_TERMINAL_MINIMUM_CONTRAST_RATIO: f32 = 21.0;
pub const DEFAULT_TERMINAL_ENABLE_BELL: bool = false;
pub const DEFAULT_TERMINAL_BELL_DURATION_MS: u64 = 1_000;
pub const MIN_TERMINAL_BELL_DURATION_MS: u64 = 100;
pub const MAX_TERMINAL_BELL_DURATION_MS: u64 = 5_000;
pub const DEFAULT_TERMINAL_SHOW_EXIT_ALERT: bool = true;
pub const DEFAULT_TERMINAL_HIDE_ON_LAST_CLOSED: bool = true;
pub const DEFAULT_TERMINAL_TABS_ENABLED: bool = true;
pub const DEFAULT_TERMINAL_TABS_DEFAULT_ICON: &str = "terminal";
pub const DEFAULT_TERMINAL_TABS_ALLOW_AGENT_CLI_TITLE: bool = true;
pub const DEFAULT_TERMINAL_TABS_TITLE: &str = "${process}";
pub const DEFAULT_TERMINAL_ALT_CLICK_MOVES_CURSOR: bool = true;
pub const DEFAULT_TERMINAL_COPY_ON_SELECTION: bool = true;
pub const DEFAULT_TERMINAL_IGNORE_BRACKETED_PASTE_MODE: bool = false;
pub const DEFAULT_TERMINAL_WORD_SEPARATORS: &str =
    " ()[]{}',\"`\u{2500}\u{2018}\u{2019}\u{201c}\u{201d}|";
pub const DEFAULT_EDITOR_CURSOR_WIDTH: f32 = 1.5;
pub const MIN_EDITOR_CURSOR_WIDTH: f32 = 1.0;
pub const MAX_EDITOR_CURSOR_WIDTH: f32 = 8.0;
pub const DEFAULT_EDITOR_CURSOR_HEIGHT: usize = 0;
pub const MIN_EDITOR_CURSOR_HEIGHT: usize = 0;
pub const MAX_EDITOR_CURSOR_HEIGHT: usize = 1_000;
pub const DEFAULT_EDITOR_CURSOR_SURROUNDING_LINES: usize = 0;
pub const MAX_EDITOR_CURSOR_SURROUNDING_LINES: usize = 100;
pub const DEFAULT_EDITOR_ACCESSIBILITY_PAGE_SIZE: usize = 500;
pub const MIN_EDITOR_ACCESSIBILITY_PAGE_SIZE: usize = 1;
pub const MAX_EDITOR_ACCESSIBILITY_PAGE_SIZE: usize = 100_000;
pub const DEFAULT_EDITOR_ARIA_LABEL: &str = "Editor content";
pub const DEFAULT_EDITOR_PLACEHOLDER: &str = "";
#[cfg(target_os = "macos")]
pub const DEFAULT_EDITOR_FONT_FAMILY: &str = "Menlo, Monaco, 'Courier New', monospace";
#[cfg(target_os = "windows")]
pub const DEFAULT_EDITOR_FONT_FAMILY: &str = "Consolas, 'Courier New', monospace";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const DEFAULT_EDITOR_FONT_FAMILY: &str = "'Droid Sans Mono', 'monospace', monospace";
pub const DEFAULT_EDITOR_FONT_WEIGHT: &str = "normal";
pub const DEFAULT_EDITOR_FONT_LIGATURES: &str = "\"liga\" off, \"calt\" off";
pub const EDITOR_FONT_LIGATURES_ON: &str = "\"liga\" on, \"calt\" on";
pub const DEFAULT_EDITOR_FONT_VARIATIONS: &str = "normal";
pub const EDITOR_FONT_VARIATIONS_TRANSLATE: &str = "translate";
pub const MIN_EDITOR_FONT_SIZE: f32 = 6.0;
pub const MAX_EDITOR_FONT_SIZE: f32 = 72.0;
pub const DEFAULT_EDITOR_LETTER_SPACING: f32 = 0.0;
pub const MIN_EDITOR_LETTER_SPACING: f32 = -5.0;
pub const MAX_EDITOR_LETTER_SPACING: f32 = 20.0;
pub const DEFAULT_EDITOR_BACKGROUND_IMAGE_DIM: f32 = 0.35;
pub const MIN_EDITOR_BACKGROUND_IMAGE_DIM: f32 = 0.0;
pub const MAX_EDITOR_BACKGROUND_IMAGE_DIM: f32 = 1.0;
pub const DEFAULT_EDITOR_TAB_INDEX: i64 = 0;
pub const MIN_EDITOR_TAB_INDEX: i64 = -1;
pub const MAX_EDITOR_TAB_INDEX: i64 = 1_000_000;
pub const DEFAULT_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING: usize = 15;
pub const MIN_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING: usize = 0;
pub const MAX_EDITOR_REVEAL_HORIZONTAL_RIGHT_PADDING: usize = 1_000;
pub const DEFAULT_EDITOR_OVERVIEW_RULER_LANES: usize = 3;
pub const MIN_EDITOR_OVERVIEW_RULER_LANES: usize = 0;
pub const MAX_EDITOR_OVERVIEW_RULER_LANES: usize = 3;
pub const DEFAULT_AUTOSAVE_DELAY_MS: u64 = 4_000;
pub const MIN_AUTOSAVE_DELAY_MS: u64 = 250;
pub const MAX_AUTOSAVE_DELAY_MS: u64 = 60_000;
pub const DEFAULT_EDITOR_LINE_HEIGHT: f32 = 0.0;
pub const MIN_EDITOR_LINE_HEIGHT: f32 = 0.0;
pub const MAX_EDITOR_LINE_HEIGHT: f32 = 64.0;
pub const DEFAULT_EDITOR_RULER_COLUMN: usize = 0;
pub const MAX_EDITOR_RULER_COLUMN: usize = 400;
pub const DEFAULT_EDITOR_LINE_DECORATIONS_WIDTH: f32 = 10.0;
pub const MIN_EDITOR_LINE_DECORATIONS_WIDTH: f32 = 0.0;
pub const MAX_EDITOR_LINE_DECORATIONS_WIDTH: f32 = 120.0;
pub const DEFAULT_EDITOR_WORD_WRAP_COLUMN: usize = 80;
pub const MIN_EDITOR_WORD_WRAP_COLUMN: usize = 1;
pub const MAX_EDITOR_WORD_WRAP_COLUMN: usize = 500;
pub const DEFAULT_EDITOR_WORD_WRAP_BREAK_AFTER_CHARACTERS: &str = " \t})]?|/&.,;¢°′″‰℃、。｡､￠，．：；？！％・･ゝゞヽヾーァィゥェォッャュョヮヵヶぁぃぅぇぉっゃゅょゎゕゖㇰㇱㇲㇳㇴㇵㇶㇷㇸㇹㇺㇻㇼㇽㇾㇿ々〻ｧｨｩｪｫｬｭｮｯｰ”〉》」』】〕）］｝｣";
pub const DEFAULT_EDITOR_WORD_WRAP_BREAK_BEFORE_CHARACTERS: &str =
    "([{‘“〈《「『【〔（［｛｢£¥＄￡￥+＋";
pub const DEFAULT_EDITOR_STOP_RENDERING_LINE_AFTER: i64 = 10_000;
pub const MIN_EDITOR_STOP_RENDERING_LINE_AFTER: i64 = -1;
pub const MAX_EDITOR_STOP_RENDERING_LINE_AFTER: i64 = 1_000_000;
pub const DEFAULT_EDITOR_SELECT_ON_LINE_NUMBERS: bool = true;
pub const DEFAULT_EDITOR_LINE_NUMBERS_MIN_CHARS: usize = 5;
pub const MIN_EDITOR_LINE_NUMBERS_MIN_CHARS: usize = 1;
pub const MAX_EDITOR_LINE_NUMBERS_MIN_CHARS: usize = 20;
pub const DEFAULT_EDITOR_MINIMAP_MAX_COLUMN: usize = 120;
pub const MIN_EDITOR_MINIMAP_MAX_COLUMN: usize = 20;
pub const MAX_EDITOR_MINIMAP_MAX_COLUMN: usize = 500;
pub const DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX: &str =
    r"\bMARK:\s*(?<separator>\-?)\s*(?<label>.*)$";
pub const DEFAULT_EDITOR_MINIMAP_SCALE: usize = 1;
pub const MIN_EDITOR_MINIMAP_SCALE: usize = 1;
pub const MAX_EDITOR_MINIMAP_SCALE: usize = 3;
pub const DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE: f32 = 9.0;
pub const MIN_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE: f32 = 4.0;
pub const MAX_EDITOR_MINIMAP_SECTION_HEADER_FONT_SIZE: f32 = 32.0;
pub const DEFAULT_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING: f32 = 1.0;
pub const MIN_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING: f32 = 0.0;
pub const MAX_EDITOR_MINIMAP_SECTION_HEADER_LETTER_SPACING: f32 = 5.0;
pub const DEFAULT_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT: usize = 5;
pub const MIN_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT: usize = 1;
pub const MAX_EDITOR_STICKY_SCROLL_MAX_LINE_COUNT: usize = 20;
pub const DEFAULT_EDITOR_PADDING_TOP: usize = 0;
pub const DEFAULT_EDITOR_PADDING_BOTTOM: usize = 0;
pub const MIN_EDITOR_PADDING: usize = 0;
pub const MAX_EDITOR_PADDING: usize = 1_000;
pub const DEFAULT_EDITOR_SCROLLBAR_VERTICAL_SCROLLBAR_SIZE: usize = 6;
pub const DEFAULT_EDITOR_SCROLLBAR_HORIZONTAL_SCROLLBAR_SIZE: usize = 6;
pub const MIN_EDITOR_SCROLLBAR_SIZE: usize = 0;
pub const MAX_EDITOR_SCROLLBAR_SIZE: usize = 1_000;
pub const DEFAULT_EDITOR_SCROLL_BEYOND_LAST_COLUMN: usize = 4;
pub const MIN_EDITOR_SCROLL_BEYOND_LAST_COLUMN: usize = 0;
pub const MAX_EDITOR_SCROLL_BEYOND_LAST_COLUMN: usize = 1_000;
pub const DEFAULT_EDITOR_MOUSE_WHEEL_SCROLL_SENSITIVITY: f32 = 1.0;
pub const DEFAULT_EDITOR_FAST_SCROLL_SENSITIVITY: f32 = 5.0;
pub const MIN_EDITOR_SCROLL_SENSITIVITY: f32 = 0.0;
pub const MAX_EDITOR_SCROLL_SENSITIVITY: f32 = 100.0;
pub const DEFAULT_EDITOR_COLOR_DECORATORS_LIMIT: usize = 500;
pub const MIN_EDITOR_COLOR_DECORATORS_LIMIT: usize = 1;
pub const MAX_EDITOR_COLOR_DECORATORS_LIMIT: usize = 1_000_000;
pub const DEFAULT_EDITOR_FIND_SEED_SEARCH_STRING_FROM_SELECTION:
    EditorFindSeedSearchStringFromSelection = EditorFindSeedSearchStringFromSelection::Always;
pub const DEFAULT_EDITOR_FIND_AUTO_FIND_IN_SELECTION: EditorFindAutoFindInSelection =
    EditorFindAutoFindInSelection::Never;
pub const DEFAULT_EDITOR_FIND_ON_TYPE: bool = true;
pub const DEFAULT_EDITOR_FIND_CURSOR_MOVE_ON_TYPE: bool = true;
pub const DEFAULT_EDITOR_FIND_LOOP: bool = true;
pub const DEFAULT_EDITOR_FIND_CLOSE_ON_RESULT: bool = false;
pub const DEFAULT_EDITOR_FIND_GLOBAL_FIND_CLIPBOARD: bool = false;
pub const DEFAULT_EDITOR_FIND_ADD_EXTRA_SPACE_ON_TOP: bool = true;
pub const DEFAULT_QUICK_SUGGESTIONS_DELAY_MS: usize = 10;
pub const MIN_QUICK_SUGGESTIONS_DELAY_MS: usize = 0;
pub const MAX_QUICK_SUGGESTIONS_DELAY_MS: usize = 60_000;
pub const DEFAULT_HOVER_DELAY_MS: usize = 300;
pub const MIN_HOVER_DELAY_MS: usize = 0;
pub const MAX_HOVER_DELAY_MS: usize = 10_000;
pub const DEFAULT_HOVER_HIDING_DELAY_MS: usize = 300;
pub const MIN_HOVER_HIDING_DELAY_MS: usize = 0;
pub const MAX_HOVER_HIDING_DELAY_MS: usize = 600_000;
pub const DEFAULT_SUGGEST_FONT_SIZE: usize = 0;
pub const MIN_SUGGEST_FONT_SIZE: usize = 0;
pub const MAX_SUGGEST_FONT_SIZE: usize = 1_000;
pub const DEFAULT_SUGGEST_LINE_HEIGHT: usize = 0;
pub const MIN_SUGGEST_LINE_HEIGHT: usize = 0;
pub const MAX_SUGGEST_LINE_HEIGHT: usize = 1_000;
pub const DEFAULT_EDITOR_INLAY_HINTS_FONT_FAMILY: &str = "";
pub const DEFAULT_EDITOR_INLAY_HINTS_FONT_SIZE: usize = 0;
pub const MIN_EDITOR_INLAY_HINTS_FONT_SIZE: usize = 0;
pub const MAX_EDITOR_INLAY_HINTS_FONT_SIZE: usize = 100;
pub const DEFAULT_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH: usize = 43;
pub const MIN_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH: usize = 0;
pub const MAX_EDITOR_INLAY_HINTS_MAXIMUM_LENGTH: usize = 10_000;
pub const DEFAULT_EDITOR_CODE_LENS_FONT_FAMILY: &str = "";
pub const DEFAULT_EDITOR_CODE_LENS_FONT_SIZE: usize = 0;
pub const MIN_EDITOR_CODE_LENS_FONT_SIZE: usize = 0;
pub const MAX_EDITOR_CODE_LENS_FONT_SIZE: usize = 100;
pub const DEFAULT_GOTO_LOCATION_ALTERNATIVE_DEFINITION_COMMAND: &str =
    "editor.action.goToReferences";
pub const DEFAULT_GOTO_LOCATION_ALTERNATIVE_TYPE_DEFINITION_COMMAND: &str =
    "editor.action.goToReferences";
pub const DEFAULT_GOTO_LOCATION_ALTERNATIVE_DECLARATION_COMMAND: &str =
    "editor.action.goToReferences";
pub const DEFAULT_GOTO_LOCATION_ALTERNATIVE_IMPLEMENTATION_COMMAND: &str = "";
pub const DEFAULT_GOTO_LOCATION_ALTERNATIVE_REFERENCE_COMMAND: &str = "";
pub const DEFAULT_GOTO_LOCATION_ALTERNATIVE_TESTS_COMMAND: &str = "";
pub const DEFAULT_INLINE_SUGGEST_FONT_FAMILY: &str = "default";
pub const DEFAULT_INLINE_SUGGEST_MIN_SHOW_DELAY_MS: usize = 0;
pub const MIN_INLINE_SUGGEST_MIN_SHOW_DELAY_MS: usize = 0;
pub const MAX_INLINE_SUGGEST_MIN_SHOW_DELAY_MS: usize = 10_000;
pub const DEFAULT_EDITOR_MULTI_CURSOR_LIMIT: usize = 10_000;
pub const MIN_EDITOR_MULTI_CURSOR_LIMIT: usize = 1;
pub const MAX_EDITOR_MULTI_CURSOR_LIMIT: usize = 100_000;
pub const DEFAULT_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT: usize = 900;
pub const MIN_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT: usize = 0;
pub const MAX_DIFF_RENDER_SIDE_BY_SIDE_INLINE_BREAKPOINT: usize = 10_000;
pub const DEFAULT_DIFF_SPLIT_VIEW_DEFAULT_RATIO: f32 = 0.5;
pub const MIN_DIFF_SPLIT_VIEW_DEFAULT_RATIO: f32 = 0.0;
pub const MAX_DIFF_SPLIT_VIEW_DEFAULT_RATIO: f32 = 1.0;
pub const DEFAULT_EDITOR_FOLDING_MAXIMUM_REGIONS: usize = 5_000;
pub const MIN_EDITOR_FOLDING_MAXIMUM_REGIONS: usize = 0;
pub const MAX_EDITOR_FOLDING_MAXIMUM_REGIONS: usize = 100_000;
pub const DEFAULT_SCM_DIFF_DECORATIONS_GUTTER_WIDTH: usize = 3;
pub const MIN_SCM_DIFF_DECORATIONS_GUTTER_WIDTH: usize = 1;
pub const MAX_SCM_DIFF_DECORATIONS_GUTTER_WIDTH: usize = 5;
pub const DEFAULT_GIT_BLAME_STATUS_BAR_ITEM_TEMPLATE: &str = "${authorName} (${authorDateAgo})";
pub const DEFAULT_GIT_BLAME_EDITOR_DECORATION_TEMPLATE: &str =
    "${subject}, ${authorName} (${authorDateAgo})";
pub const DEFAULT_GIT_BRANCH_PREFIX: &str = "";
pub const DEFAULT_GIT_BRANCH_VALIDATION_REGEX: &str = "";
pub const MAX_GIT_BRANCH_VALIDATION_ERROR_CHARS: usize = 160;
pub const DEFAULT_GIT_BRANCH_WHITESPACE_CHAR: &str = "-";
pub const DEFAULT_GIT_REPOSITORY_SCAN_MAX_DEPTH: usize = 1;
pub const MIN_GIT_REPOSITORY_SCAN_MAX_DEPTH: usize = 0;
pub const MAX_GIT_REPOSITORY_SCAN_MAX_DEPTH: usize = 25;
pub const DEFAULT_GIT_AUTOFETCH_PERIOD: usize = 180;
pub const MIN_GIT_AUTOFETCH_PERIOD: usize = 0;
pub const MAX_GIT_AUTOFETCH_PERIOD: usize = 86_400;
pub const DEFAULT_GIT_DETECT_WORKTREES_LIMIT: usize = 50;
pub const MIN_GIT_DETECT_WORKTREES_LIMIT: usize = 0;
pub const MAX_GIT_DETECT_WORKTREES_LIMIT: usize = 10_000;
pub const DEFAULT_GIT_DEFAULT_BRANCH_NAME: &str = "main";
pub const DEFAULT_GIT_INPUT_VALIDATION_LENGTH: usize = 72;
pub const DEFAULT_GIT_INPUT_VALIDATION_SUBJECT_LENGTH: usize = 50;
pub const MIN_GIT_INPUT_VALIDATION_LENGTH: usize = 1;
pub const MAX_GIT_INPUT_VALIDATION_LENGTH: usize = 1_000;
pub const DEFAULT_SCM_INPUT_MIN_LINE_COUNT: usize = 1;
pub const DEFAULT_SCM_INPUT_MAX_LINE_COUNT: usize = 10;
pub const MIN_SCM_INPUT_LINE_COUNT: usize = 1;
pub const MAX_SCM_INPUT_LINE_COUNT: usize = 50;
pub const DEFAULT_SCM_INPUT_FONT_FAMILY: &str = "default";
pub const DEFAULT_SCM_INPUT_FONT_SIZE: f32 = 13.0;
pub const MIN_SCM_INPUT_FONT_SIZE: f32 = 8.0;
pub const MAX_SCM_INPUT_FONT_SIZE: f32 = 32.0;
pub const DEFAULT_SCM_AUTO_REVEAL: bool = true;
pub const DEFAULT_SCM_REPOSITORIES_VISIBLE: usize = 10;
pub const MIN_SCM_REPOSITORIES_VISIBLE: usize = 0;
pub const MAX_SCM_REPOSITORIES_VISIBLE: usize = 128;
pub const DEFAULT_SCM_GRAPH_PAGE_ON_SCROLL: bool = true;
pub const DEFAULT_SCM_GRAPH_PAGE_SIZE: usize = 50;
pub const MIN_SCM_GRAPH_PAGE_SIZE: usize = 1;
pub const MAX_SCM_GRAPH_PAGE_SIZE: usize = 1_000;
pub const DEFAULT_WINDOW_ZOOM_LEVEL: f32 = 0.0;
pub const MIN_WINDOW_ZOOM_LEVEL: f32 = -5.0;
pub const MAX_WINDOW_ZOOM_LEVEL: f32 = 5.0;
pub const SETTINGS_SCHEMA_VERSION: u32 = 5;
pub const DEFAULT_PROJECT_SEARCH_MAX_FILE_SIZE_MB: u64 = 2;
pub const MIN_PROJECT_SEARCH_MAX_FILE_SIZE_MB: u64 = 1;
pub const MAX_PROJECT_SEARCH_MAX_FILE_SIZE_MB: u64 = 256;
pub const DEFAULT_PROJECT_SEARCH_MAX_RESULTS: usize = 500;
pub const MIN_PROJECT_SEARCH_MAX_RESULTS: usize = 1;
pub const MAX_PROJECT_SEARCH_MAX_RESULTS: usize = 50_000;
pub const MAX_PLUGIN_SETTINGS_DISABLED_IDS: usize = 128;
pub const MAX_PLUGIN_SETTINGS_DISABLED_ID_CHARS: usize = 128;
const SETTINGS_FILE_MAX_BYTES: u64 = 512 * 1024;
const SETTINGS_STRING_MAX_CHARS: usize = 4096;
const SETTINGS_DISPLAY_TEXT_MAX_CHARS: usize = 512;
const SETTINGS_LIST_MAX_ITEMS: usize = 256;
const SETTINGS_DESERIALIZE_LIST_HARD_CAP: usize = SETTINGS_LIST_MAX_ITEMS * 4;
const SETTINGS_MAP_MAX_ITEMS: usize = 256;
const SETTINGS_MAP_KEY_MAX_CHARS: usize = 256;
const SETTINGS_MAP_VALUE_MAX_CHARS: usize = 4096;
const SETTINGS_DISPLAY_TRUNCATION_MARKER: &str = "...";

pub fn default_project_search_exclude_globs() -> Vec<String> {
    default_project_index_exclude_globs()
}

pub fn clamp_project_index_max_files(value: usize) -> usize {
    value.clamp(MIN_PROJECT_INDEX_MAX_FILES, MAX_PROJECT_INDEX_MAX_FILES)
}

pub fn clamp_project_search_max_file_size_mb(value: u64) -> u64 {
    value.clamp(
        MIN_PROJECT_SEARCH_MAX_FILE_SIZE_MB,
        MAX_PROJECT_SEARCH_MAX_FILE_SIZE_MB,
    )
}

pub fn clamp_project_search_max_results(value: usize) -> usize {
    value.clamp(
        MIN_PROJECT_SEARCH_MAX_RESULTS,
        MAX_PROJECT_SEARCH_MAX_RESULTS,
    )
}

pub fn clamp_editor_background_image_dim(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(
            MIN_EDITOR_BACKGROUND_IMAGE_DIM,
            MAX_EDITOR_BACKGROUND_IMAGE_DIM,
        )
    } else {
        DEFAULT_EDITOR_BACKGROUND_IMAGE_DIM
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    pub name: String,
    pub background: [u8; 3],
    pub panel: [u8; 3],
    pub panel_alt: [u8; 3],
    pub text: [u8; 3],
    pub muted_text: [u8; 3],
    pub accent: [u8; 3],
    pub selection: Option<[u8; 3]>,
    pub warning: [u8; 3],
    pub error: [u8; 3],
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            name: "Matte Dark".to_owned(),
            background: [18, 20, 24],
            panel: [25, 28, 34],
            panel_alt: [31, 35, 42],
            text: [222, 226, 233],
            muted_text: [126, 136, 150],
            accent: [91, 141, 239],
            selection: None,
            warning: [231, 185, 87],
            error: [232, 98, 98],
        }
    }
}

impl ThemeSettings {
    pub fn built_in_presets() -> Vec<Self> {
        vec![
            Self::default(),
            Self {
                name: "Graphite".to_owned(),
                background: [14, 15, 17],
                panel: [23, 24, 27],
                panel_alt: [34, 36, 40],
                text: [229, 231, 235],
                muted_text: [143, 149, 161],
                accent: [116, 199, 154],
                selection: None,
                warning: [231, 185, 87],
                error: [232, 98, 98],
            },
            Self {
                name: "Carbon Blue".to_owned(),
                background: [15, 19, 25],
                panel: [22, 28, 37],
                panel_alt: [31, 40, 53],
                text: [224, 231, 240],
                muted_text: [133, 146, 164],
                accent: [91, 141, 239],
                selection: None,
                warning: [231, 185, 87],
                error: [232, 98, 98],
            },
            Self {
                name: "Soft Light".to_owned(),
                background: [244, 246, 248],
                panel: [232, 236, 241],
                panel_alt: [218, 225, 233],
                text: [36, 41, 49],
                muted_text: [91, 99, 113],
                accent: [47, 111, 237],
                selection: None,
                warning: [161, 104, 24],
                error: [190, 50, 50],
            },
            Self {
                name: "Midnight Teal".to_owned(),
                background: [9, 24, 28],
                panel: [14, 34, 39],
                panel_alt: [23, 47, 54],
                text: [221, 234, 232],
                muted_text: [128, 157, 160],
                accent: [45, 212, 191],
                selection: None,
                warning: [230, 179, 71],
                error: [238, 99, 91],
            },
            Self {
                name: "Forest Dusk".to_owned(),
                background: [15, 22, 18],
                panel: [22, 32, 26],
                panel_alt: [31, 45, 36],
                text: [226, 232, 224],
                muted_text: [139, 153, 140],
                accent: [116, 196, 118],
                selection: None,
                warning: [217, 168, 70],
                error: [225, 92, 92],
            },
            Self {
                name: "Plum Dusk".to_owned(),
                background: [24, 18, 29],
                panel: [34, 26, 41],
                panel_alt: [45, 36, 55],
                text: [232, 226, 237],
                muted_text: [157, 140, 166],
                accent: [203, 137, 229],
                selection: None,
                warning: [230, 181, 92],
                error: [235, 105, 116],
            },
            Self {
                name: "Warm Paper".to_owned(),
                background: [250, 247, 239],
                panel: [240, 235, 225],
                panel_alt: [229, 222, 210],
                text: [49, 43, 36],
                muted_text: [111, 101, 88],
                accent: [28, 111, 145],
                selection: None,
                warning: [157, 99, 31],
                error: [177, 55, 58],
            },
            Self {
                name: "Clear Day".to_owned(),
                background: [248, 250, 252],
                panel: [235, 240, 246],
                panel_alt: [224, 231, 240],
                text: [31, 41, 55],
                muted_text: [91, 103, 120],
                accent: [37, 99, 235],
                selection: None,
                warning: [166, 113, 32],
                error: [190, 50, 70],
            },
            Self {
                name: "Sage Light".to_owned(),
                background: [245, 248, 244],
                panel: [232, 239, 232],
                panel_alt: [219, 229, 219],
                text: [36, 48, 40],
                muted_text: [88, 106, 92],
                accent: [38, 139, 111],
                selection: None,
                warning: [153, 101, 24],
                error: [181, 63, 70],
            },
            Self {
                name: "Sakura Milk".to_owned(),
                background: [255, 246, 250],
                panel: [248, 230, 239],
                panel_alt: [239, 213, 226],
                text: [58, 45, 55],
                muted_text: [118, 90, 105],
                accent: [204, 73, 128],
                selection: Some([247, 194, 214]),
                warning: [164, 103, 40],
                error: [196, 72, 87],
            },
            Self {
                name: "Mochi Mint".to_owned(),
                background: [242, 251, 248],
                panel: [225, 244, 240],
                panel_alt: [206, 232, 229],
                text: [35, 53, 54],
                muted_text: [82, 115, 113],
                accent: [37, 143, 132],
                selection: Some([185, 226, 218]),
                warning: [170, 111, 35],
                error: [190, 62, 82],
            },
            Self {
                name: "Sky Ribbon".to_owned(),
                background: [242, 248, 255],
                panel: [225, 238, 250],
                panel_alt: [207, 225, 242],
                text: [35, 48, 62],
                muted_text: [82, 102, 124],
                accent: [65, 121, 211],
                selection: Some([188, 217, 249]),
                warning: [173, 116, 36],
                error: [191, 64, 93],
            },
            Self {
                name: "Starry Idol".to_owned(),
                background: [19, 18, 34],
                panel: [28, 26, 48],
                panel_alt: [41, 37, 67],
                text: [239, 235, 255],
                muted_text: [159, 150, 187],
                accent: [255, 142, 194],
                selection: Some([83, 61, 114]),
                warning: [244, 190, 96],
                error: [255, 111, 136],
            },
            Self {
                name: "Peach Festival".to_owned(),
                background: [31, 22, 28],
                panel: [44, 31, 39],
                panel_alt: [59, 41, 50],
                text: [247, 232, 226],
                muted_text: [184, 139, 141],
                accent: [255, 159, 116],
                selection: Some([93, 58, 64]),
                warning: [245, 187, 83],
                error: [246, 100, 113],
            },
        ]
    }

    pub fn built_in_names() -> Vec<String> {
        Self::built_in_presets()
            .into_iter()
            .map(|theme| theme.name)
            .collect()
    }

    pub fn built_in_by_name(name: &str) -> Option<Self> {
        Self::built_in_presets()
            .into_iter()
            .find(|theme| theme.name.eq_ignore_ascii_case(name))
    }

    pub fn next_built_in_after(name: &str) -> Self {
        let presets = Self::built_in_presets();
        let next = presets
            .iter()
            .position(|theme| theme.name.eq_ignore_ascii_case(name))
            .map(|index| (index + 1) % presets.len())
            .unwrap_or_default();
        presets[next].clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorVimSettings {
    pub disabled_bindings: Vec<String>,
    pub key_overrides: Vec<EditorVimKeyOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorVimKeyOverride {
    pub before: String,
    pub after: String,
    pub command: Option<Command>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSettings {
    #[serde(default = "default_plugin_settings_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub disabled_ids: Vec<String>,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            enabled: default_plugin_settings_enabled(),
            disabled_ids: Vec::new(),
        }
    }
}

impl PluginSettings {
    pub fn sanitize(&mut self) -> bool {
        sanitize_settings_string_list(
            &mut self.disabled_ids,
            MAX_PLUGIN_SETTINGS_DISABLED_IDS,
            MAX_PLUGIN_SETTINGS_DISABLED_ID_CHARS,
            true,
        )
    }
}

fn default_plugin_settings_enabled() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscordSettings {
    pub presence_enabled: bool,
    pub client_id: String,
    pub show_details: bool,
    pub show_workspace: bool,
    pub show_elapsed: bool,
}

impl Default for DiscordSettings {
    fn default() -> Self {
        Self {
            presence_enabled: false,
            client_id: String::new(),
            show_details: true,
            show_workspace: true,
            show_elapsed: true,
        }
    }
}

impl DiscordSettings {
    pub fn presence_is_configurable(&self) -> bool {
        self.presence_enabled && !self.client_id.is_empty()
    }

    pub(super) fn sanitize(&mut self) -> bool {
        sanitize_settings_plain_string(&mut self.client_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorBackgroundImageFit {
    #[default]
    Cover,
    Contain,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditorBackgroundImageScope {
    #[default]
    Editor,
    FullApp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorBackgroundImagePosition {
    Top,
    #[default]
    Center,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorSettings {
    #[serde(default = "current_settings_schema_version")]
    pub schema_version: u32,
    pub font_size: f32,
    pub ui_font_size: f32,
    pub editor_font_path: Option<String>,
    pub ui_font_path: Option<String>,
    pub font_family: String,
    pub font_weight: String,
    #[serde(
        default = "default_editor_font_ligatures",
        deserialize_with = "deserialize_editor_font_ligatures"
    )]
    pub font_ligatures: String,
    #[serde(
        default = "default_editor_font_variations",
        deserialize_with = "deserialize_editor_font_variations"
    )]
    pub font_variations: String,
    pub letter_spacing: f32,
    pub automatic_layout: bool,
    pub disable_layer_hinting: bool,
    pub disable_monospace_optimizations: bool,
    pub extra_editor_class_name: String,
    pub allow_variable_line_heights: bool,
    pub allow_variable_fonts: bool,
    pub allow_variable_fonts_in_accessibility_mode: bool,
    pub accessibility_support: EditorAccessibilitySupport,
    pub accessibility_page_size: usize,
    pub aria_label: String,
    pub aria_required: bool,
    pub screen_reader_announce_inline_suggestion: bool,
    pub tab_index: i64,
    pub read_only: bool,
    pub read_only_message: String,
    pub dom_read_only: bool,
    pub edit_context: bool,
    pub render_rich_screen_reader_content: bool,
    pub trim_whitespace_on_delete: bool,
    pub unusual_line_terminators: EditorUnusualLineTerminators,
    pub use_shadow_dom: bool,
    pub use_tab_stops: bool,
    pub fixed_overflow_widgets: bool,
    pub allow_overflow: bool,
    pub tab_width: usize,
    pub insert_spaces: bool,
    pub detect_indentation: bool,
    pub word_separators: String,
    #[serde(default, deserialize_with = "deserialize_optional_string_list")]
    pub word_segmenter_locales: Vec<String>,
    pub auto_indent: bool,
    pub auto_closing_brackets: bool,
    pub auto_closing_quotes: bool,
    pub experimental_gpu_acceleration: EditorExperimentalGpuAcceleration,
    pub experimental_whitespace_rendering: EditorExperimentalWhitespaceRendering,
    pub auto_closing_comments: EditorAutoClosingStrategy,
    pub auto_closing_delete: EditorAutoClosingEditStrategy,
    pub auto_closing_overtype: EditorAutoClosingEditStrategy,
    pub auto_surround: bool,
    pub auto_indent_on_paste: bool,
    pub auto_indent_on_paste_within_string: bool,
    pub sticky_tab_stops: bool,
    pub linked_editing: bool,
    pub rename_on_type: bool,
    pub tab_focus_mode: bool,
    #[serde(alias = "vim_mode")]
    pub vim_keybindings: bool,
    pub vim: EditorVimSettings,
    pub discord: DiscordSettings,
    pub quick_suggestions: bool,
    pub quick_suggestions_delay_ms: usize,
    pub suggest_on_trigger_characters: bool,
    pub accept_suggestion_on_enter: bool,
    pub accept_suggestion_on_tab: bool,
    pub accept_suggestion_on_commit_character: bool,
    pub suggest_selection: EditorSuggestSelection,
    pub suggest_insert_mode: EditorSuggestInsertMode,
    pub suggest_filter_graceful: bool,
    pub suggest_snippets_prevent_quick_suggestions: bool,
    pub suggest_locality_bonus: bool,
    pub suggest_share_suggest_selections: bool,
    pub suggest_selection_mode: EditorSuggestSelectionMode,
    pub suggest_show_icons: bool,
    pub suggest_show_status_bar: bool,
    pub suggest_preview: bool,
    pub suggest_preview_mode: EditorSuggestPreviewMode,
    pub suggest_show_inline_details: bool,
    pub suggest_show_methods: bool,
    pub suggest_show_functions: bool,
    pub suggest_show_constructors: bool,
    pub suggest_show_deprecated: bool,
    pub suggest_show_fields: bool,
    pub suggest_show_variables: bool,
    pub suggest_show_classes: bool,
    pub suggest_show_structs: bool,
    pub suggest_show_interfaces: bool,
    pub suggest_show_modules: bool,
    pub suggest_show_properties: bool,
    pub suggest_show_events: bool,
    pub suggest_show_operators: bool,
    pub suggest_show_units: bool,
    pub suggest_show_values: bool,
    pub suggest_show_constants: bool,
    pub suggest_show_enums: bool,
    pub suggest_show_enum_members: bool,
    pub suggest_show_keywords: bool,
    pub suggest_show_words: bool,
    pub suggest_show_colors: bool,
    pub suggest_show_files: bool,
    pub suggest_show_references: bool,
    pub suggest_show_customcolors: bool,
    pub suggest_show_folders: bool,
    pub suggest_show_type_parameters: bool,
    pub suggest_show_snippets: bool,
    pub suggest_show_users: bool,
    pub suggest_show_issues: bool,
    pub suggest_match_on_word_start_only: bool,
    pub suggest_font_size: usize,
    pub suggest_line_height: usize,
    pub tab_completion: EditorTabCompletion,
    pub snippet_suggestions: EditorSnippetSuggestions,
    pub hover_enabled: bool,
    pub hover_delay_ms: usize,
    pub hover_hiding_delay_ms: usize,
    pub hover_sticky: bool,
    pub hover_above: bool,
    pub hover_show_long_line_warning: bool,
    pub inline_suggest_enabled: bool,
    pub inline_suggest_mode: EditorInlineSuggestMode,
    pub inline_suggest_show_toolbar: EditorInlineSuggestShowToolbar,
    pub inline_suggest_keep_on_blur: bool,
    pub inline_suggest_font_family: String,
    pub inline_suggest_syntax_highlighting_enabled: bool,
    pub inline_suggest_suppress_suggestions: bool,
    pub inline_suggest_suppress_in_snippet_mode: bool,
    pub inline_suggest_min_show_delay_ms: usize,
    pub inline_suggest_edits_enabled: bool,
    pub inline_suggest_edits_show_collapsed: bool,
    pub inline_suggest_edits_render_side_by_side: EditorInlineSuggestEditsRenderSideBySide,
    pub inline_suggest_edits_allow_code_shifting: EditorInlineSuggestEditsAllowCodeShifting,
    pub inline_suggest_edits_show_long_distance_hint: bool,
    pub inline_suggest_trigger_command_on_provider_change: bool,
    pub inline_suggest_experimental_suppress_inline_suggestions: String,
    pub inline_suggest_experimental_show_on_suggest_conflict:
        EditorInlineSuggestShowOnSuggestConflict,
    pub inline_suggest_experimental_empty_response_information: bool,
    pub inline_completions_accessibility_verbose: bool,
    pub lightbulb: EditorLightbulbMode,
    pub render_validation_decorations: EditorRenderValidationDecorations,
    pub document_highlights_enabled: bool,
    pub code_lens: bool,
    pub code_lens_font_family: String,
    pub code_lens_font_size: usize,
    pub goto_location_multiple_definitions: EditorGotoLocationMultiple,
    pub goto_location_multiple_type_definitions: EditorGotoLocationMultiple,
    pub goto_location_multiple_declarations: EditorGotoLocationMultiple,
    pub goto_location_multiple_implementations: EditorGotoLocationMultiple,
    pub goto_location_multiple_references: EditorGotoLocationMultiple,
    pub goto_location_multiple_tests: EditorGotoLocationMultiple,
    pub goto_location_alternative_definition_command: String,
    pub goto_location_alternative_type_definition_command: String,
    pub goto_location_alternative_declaration_command: String,
    pub goto_location_alternative_implementation_command: String,
    pub goto_location_alternative_reference_command: String,
    pub goto_location_alternative_tests_command: String,
    pub peek_widget_default_focus: EditorPeekWidgetDefaultFocus,
    pub placeholder: String,
    pub definition_link_opens_in_peek: bool,
    pub inlay_hints: bool,
    pub inlay_hints_font_family: String,
    pub inlay_hints_font_size: usize,
    pub inlay_hints_padding: bool,
    pub inlay_hints_maximum_length: usize,
    pub parameter_hints_enabled: bool,
    pub parameter_hints_on_trigger_characters: bool,
    pub parameter_hints_cycle: bool,
    pub comments_insert_space: bool,
    pub comments_ignore_empty_lines: bool,
    pub format_on_save: bool,
    pub format_on_type: bool,
    pub format_on_paste: bool,
    pub paste_as_enabled: bool,
    pub paste_as_show_paste_selector: EditorPasteAsShowPasteSelector,
    pub autosave: bool,
    pub autosave_mode: EditorAutoSaveMode,
    pub autosave_delay_ms: u64,
    pub smooth_scrolling: bool,
    pub scroll_beyond_last_line: bool,
    pub scroll_beyond_last_column: usize,
    pub scroll_on_middle_click: bool,
    pub scroll_predominant_axis: bool,
    pub inertial_scroll: bool,
    pub mouse_wheel_scroll_sensitivity: f32,
    pub fast_scroll_sensitivity: f32,
    pub mouse_wheel_zoom: bool,
    pub scrollbar_vertical: EditorScrollbarVisibility,
    pub scrollbar_horizontal: EditorScrollbarVisibility,
    pub scrollbar_vertical_scrollbar_size: usize,
    pub scrollbar_horizontal_scrollbar_size: usize,
    pub scrollbar_ignore_horizontal_scrollbar_in_content_height: bool,
    pub explorer_scrollbar: EditorScrollbarVisibility,
    pub padding_top: usize,
    pub padding_bottom: usize,
    pub links: bool,
    pub show_unused: bool,
    pub show_deprecated: bool,
    pub contextmenu: bool,
    pub color_decorators: bool,
    pub color_decorators_activated_on: EditorColorDecoratorsActivatedOn,
    pub color_decorators_limit: usize,
    pub default_color_decorators: EditorDefaultColorDecorators,
    pub sticky_scroll: bool,
    pub sticky_scroll_max_line_count: usize,
    pub sticky_scroll_default_model: EditorStickyScrollDefaultModel,
    pub sticky_scroll_scroll_with_editor: bool,
    pub line_height: f32,
    pub minimap: bool,
    pub minimap_side: EditorMinimapSide,
    pub minimap_autohide: EditorMinimapAutohide,
    pub minimap_size: EditorMinimapSize,
    pub minimap_show_slider: EditorMinimapShowSlider,
    pub minimap_scale: usize,
    pub minimap_render_characters: bool,
    pub minimap_max_column: usize,
    pub minimap_show_region_section_headers: bool,
    pub minimap_show_mark_section_headers: bool,
    pub minimap_mark_section_header_regex: String,
    pub minimap_section_header_font_size: f32,
    pub minimap_section_header_letter_spacing: f32,
    pub multi_cursor_modifier: EditorMultiCursorModifier,
    pub multi_cursor_merge_overlapping: bool,
    pub multi_cursor_paste: EditorMultiCursorPaste,
    pub multi_cursor_limit: usize,
    pub column_selection: bool,
    pub mouse_middle_click_action: EditorMouseMiddleClickAction,
    pub empty_selection_clipboard: bool,
    pub selection_clipboard: bool,
    pub copy_with_syntax_highlighting: bool,
    pub double_click_selects_block: bool,
    pub drag_and_drop: bool,
    pub drop_into_editor_enabled: bool,
    pub drop_into_editor_show_drop_selector: EditorDropIntoEditorShowDropSelector,
    pub glyph_margin: bool,
    pub ruler_column: usize,
    pub overview_ruler_border: bool,
    pub overview_ruler_lanes: usize,
    pub hide_cursor_in_overview_ruler: bool,
    pub status_bar_visible: bool,
    pub devtools_verbose_logging: bool,
    pub devtools_profiling_enabled: bool,
    #[serde(default = "default_server_configs")]
    pub lsp_servers: Vec<LspServerConfig>,

    #[serde(default)]
    pub lsp_suggest_missing_servers: bool,
    pub window_zoom_level: f32,
    pub line_numbers: EditorLineNumbers,
    pub line_decorations_width: EditorLineDecorationsWidth,
    pub line_numbers_min_chars: usize,
    pub select_on_line_numbers: bool,
    pub word_wrap: EditorWordWrap,
    pub word_wrap_override1: EditorWordWrapOverride,
    pub word_wrap_override2: EditorWordWrapOverride,
    pub word_wrap_break_after_characters: String,
    pub word_wrap_break_before_characters: String,
    pub word_wrap_column: usize,
    pub wrapping_indent: EditorWrappingIndent,
    pub wrapping_strategy: EditorWrappingStrategy,
    pub wrap_on_escaped_line_feeds: bool,
    pub word_break: EditorWordBreak,
    pub reveal_horizontal_right_padding: usize,
    pub rounded_selection: bool,
    pub stop_rendering_line_after: i64,
    pub render_whitespace: EditorRenderWhitespace,
    pub render_final_newline: EditorRenderFinalNewline,
    pub render_control_characters: bool,
    pub unicode_highlight_ambiguous_characters: bool,
    pub unicode_highlight_invisible_characters: bool,
    pub unicode_highlight_non_basic_ascii: EditorUnicodeHighlightNonBasicAscii,
    pub unicode_highlight_include_comments: EditorUnicodeHighlightScope,
    pub unicode_highlight_include_strings: EditorUnicodeHighlightScope,
    pub unicode_highlight_allowed_characters: BTreeMap<String, bool>,
    pub unicode_highlight_allowed_locales: BTreeMap<String, bool>,
    pub render_line_highlight: EditorRenderLineHighlight,
    pub render_line_highlight_only_when_focus: bool,
    pub smart_select_select_leading_and_trailing_whitespace: bool,
    pub smart_select_select_subwords: bool,
    pub find_seed_search_string_from_selection: EditorFindSeedSearchStringFromSelection,
    pub find_auto_find_in_selection: EditorFindAutoFindInSelection,
    pub find_on_type: bool,
    pub find_cursor_move_on_type: bool,
    pub find_loop: bool,
    pub find_close_on_result: bool,
    pub find_global_find_clipboard: bool,
    pub find_add_extra_space_on_top: bool,
    pub find_history: EditorFindHistory,
    pub find_replace_history: EditorFindHistory,
    pub diff_ignore_trim_whitespace: bool,
    pub diff_algorithm: DiffAlgorithm,
    pub diff_render_side_by_side: bool,
    pub diff_enable_split_view_resizing: bool,
    pub diff_split_view_default_ratio: f32,
    pub diff_render_side_by_side_inline_breakpoint: usize,
    pub diff_use_inline_view_when_space_is_limited: bool,
    pub diff_compact_mode: bool,
    pub diff_original_editable: bool,
    pub diff_code_lens: bool,
    pub diff_accessibility_verbose: bool,
    pub diff_hide_unchanged_regions: bool,
    pub diff_context_lines: usize,
    pub diff_hide_unchanged_regions_minimum_line_count: usize,
    pub diff_hide_unchanged_regions_reveal_line_count: usize,
    pub diff_max_computation_time_ms: usize,
    pub diff_max_file_size_mb: usize,
    pub diff_render_gutter_menu: bool,
    pub diff_render_indicators: bool,
    pub diff_render_margin_revert_icon: bool,
    pub diff_render_overview_ruler: bool,
    pub diff_experimental_show_moves: bool,
    pub diff_experimental_show_empty_decorations: bool,
    pub diff_experimental_use_true_inline_view: bool,
    pub diff_word_wrap: DiffWordWrap,
    pub diff_only_show_accessible_viewer: bool,
    pub diff_is_in_embedded_editor: bool,
    pub background_image_enabled: bool,
    pub background_image_path: Option<String>,
    pub background_image_scope: EditorBackgroundImageScope,
    pub background_image_dim: f32,
    pub background_image_fit: EditorBackgroundImageFit,
    pub background_image_position: EditorBackgroundImagePosition,
    pub background_image_loop: bool,
    pub project_index_max_files: usize,
    #[serde(
        default = "default_project_index_exclude_globs",
        deserialize_with = "deserialize_optional_string_list"
    )]
    pub project_index_exclude_globs: Vec<String>,
    #[serde(default)]
    pub project_index_include_hidden_dirs: bool,
    #[serde(
        default = "default_project_search_exclude_globs",
        deserialize_with = "deserialize_optional_string_list"
    )]
    pub project_search_exclude_globs: Vec<String>,
    pub project_search_max_file_size_mb: u64,
    pub project_search_max_results: usize,
    pub git_enabled: bool,
    pub git_add_ai_co_author: GitAddAiCoAuthor,
    pub git_allow_force_push: bool,
    pub git_allow_no_verify_commit: bool,
    pub git_auto_repository_detection: GitAutoRepositoryDetection,
    pub git_autofetch: GitAutoFetch,
    pub git_autofetch_period: usize,
    pub git_autorefresh: bool,
    pub git_auto_stash: bool,
    pub git_commands_to_log: Vec<String>,
    pub git_confirm_force_push: bool,
    pub git_confirm_no_verify_commit: bool,
    pub git_confirm_sync: bool,
    pub git_ignore_limit_warning: bool,
    pub git_ignore_submodules: bool,
    pub git_ignored_repositories: Vec<String>,
    pub git_repository_scan_ignored_folders: Vec<String>,
    pub git_open_repository_in_parent_folders: GitOpenRepositoryInParentFolders,
    pub git_detect_submodules: bool,
    pub git_detect_submodules_limit: usize,
    pub git_repository_scan_max_depth: usize,
    pub git_detect_worktrees: bool,
    pub git_detect_worktrees_limit: usize,
    pub git_discard_untracked_changes_to_trash: bool,
    pub git_diagnostics_commit_hook_enabled: bool,
    pub git_diagnostics_commit_hook_sources: BTreeMap<String, String>,
    pub git_enable_commit_signing: bool,
    pub git_enable_status_bar_sync: bool,
    pub git_fetch_on_pull: bool,
    pub git_follow_tags_when_sync: bool,
    pub git_ignore_legacy_warning: bool,
    pub git_ignore_missing_git_warning: bool,
    pub git_ignore_rebase_warning: bool,
    pub git_ignore_windows_git27_warning: bool,
    pub git_merge_editor: bool,
    pub git_open_after_clone: GitOpenAfterClone,
    pub git_optimistic_update: bool,
    #[serde(default, deserialize_with = "deserialize_optional_string_list")]
    pub git_path: Vec<String>,
    pub git_post_commit_command: GitPostCommitCommand,
    pub git_prune_on_fetch: bool,
    pub git_pull_before_checkout: bool,
    pub git_pull_tags: bool,
    pub git_rebase_when_sync: bool,
    pub git_remember_post_commit_command: bool,
    pub git_replace_tags_when_pull: bool,
    pub git_support_cancellation: bool,
    pub git_terminal_authentication: bool,
    pub git_terminal_git_editor: bool,
    pub git_use_force_push_if_includes: bool,
    pub git_use_force_push_with_lease: bool,
    pub git_use_integrated_ask_pass: bool,
    pub git_worktree_include_files: Vec<String>,
    pub git_default_branch_name: String,
    pub git_default_clone_directory: Option<String>,
    pub git_similarity_threshold: usize,
    pub scm_default_view_mode: ScmDefaultViewMode,
    pub scm_default_view_sort_key: ScmDefaultViewSortKey,
    pub scm_auto_reveal: bool,
    pub scm_count_badge: ScmCountBadge,
    pub scm_provider_count_badge: ScmProviderCountBadge,
    pub scm_always_show_repositories: bool,
    pub scm_repositories_visible: usize,
    pub scm_compact_folders: bool,
    pub scm_always_show_actions: bool,
    pub scm_show_action_button: bool,
    pub git_show_commit_input: bool,
    pub git_show_push_success_notification: bool,
    pub git_use_editor_as_commit_input: bool,
    pub git_verbose_commit: bool,
    pub git_show_action_button_commit: bool,
    pub git_always_sign_off: bool,
    pub git_confirm_committed_delete: bool,
    pub git_confirm_empty_commits: bool,
    pub git_require_user_config: bool,
    pub git_show_progress: bool,
    pub git_show_reference_details: bool,
    pub git_timeline_show_author: bool,
    pub git_timeline_show_uncommitted: bool,
    pub git_timeline_date: GitTimelineDate,
    pub git_show_inline_open_file_action: bool,
    pub git_count_badge: GitCountBadge,
    pub git_untracked_changes: GitUntrackedChanges,
    pub git_open_diff_on_click: bool,
    pub git_close_diff_on_operation: bool,
    pub git_always_show_staged_changes_resource_group: bool,
    pub git_checkout_type: Vec<GitCheckoutType>,
    pub git_branch_sort_order: GitBranchSortOrder,
    pub git_branch_prefix: String,
    pub git_branch_random_name_enable: bool,
    pub git_branch_random_name_dictionary: Vec<String>,
    pub git_branch_validation_regex: String,
    pub git_branch_whitespace_char: String,
    pub git_decorations_enabled: bool,
    pub git_enable_smart_commit: bool,
    pub git_suggest_smart_commit: bool,
    pub git_smart_commit_changes: GitSmartCommitChanges,
    pub git_prompt_to_save_files_before_commit: GitPromptToSaveFilesBeforeCommit,
    pub git_prompt_to_save_files_before_stash: GitPromptToSaveFilesBeforeCommit,
    pub git_branch_protection: Vec<String>,
    pub git_branch_protection_prompt: GitBranchProtectionPrompt,
    pub git_status_limit: usize,
    pub git_use_commit_input_as_stash_message: bool,
    pub git_commit_short_hash_length: usize,
    pub git_input_validation: bool,
    pub git_input_validation_length: usize,
    pub git_input_validation_subject_length: GitInputValidationSubjectLength,
    pub git_blame_status_bar_item_enabled: bool,
    pub git_blame_editor_decoration_enabled: bool,
    pub git_blame_editor_decoration_disable_hover: bool,
    pub git_blame_ignore_whitespace: bool,
    pub git_blame_status_bar_item_template: String,
    pub git_blame_editor_decoration_template: String,
    pub scm_show_input_action_button: bool,
    pub scm_input_min_line_count: usize,
    pub scm_input_max_line_count: usize,
    pub scm_input_font_family: String,
    pub scm_input_font_size: f32,
    pub scm_diff_decorations: ScmDiffDecorations,
    pub scm_diff_decorations_gutter_action: ScmDiffDecorationsGutterAction,
    pub scm_diff_decorations_gutter_visibility: ScmDiffDecorationsGutterVisibility,
    pub scm_diff_decorations_gutter_width: usize,
    pub scm_diff_decorations_gutter_pattern: ScmDiffDecorationsGutterPattern,
    pub scm_diff_decorations_ignore_trim_whitespace: ScmDiffDecorationsIgnoreTrimWhitespace,
    pub scm_graph_page_on_scroll: bool,
    pub scm_graph_page_size: usize,
    pub scm_graph_badges: ScmGraphBadges,
    pub scm_graph_show_incoming_changes: bool,
    pub scm_graph_show_outgoing_changes: bool,
    pub bracket_pair_colorization: bool,
    pub bracket_pair_colorization_independent_color_pool_per_bracket_type: bool,
    pub bracket_pair_guides: EditorBracketPairGuideMode,
    pub bracket_pair_guides_horizontal: EditorBracketPairGuideMode,
    pub highlight_active_bracket_pair: bool,
    pub match_brackets: EditorMatchBrackets,
    pub folding: bool,
    pub folding_highlight: bool,
    pub folding_imports_by_default: bool,
    pub folding_maximum_regions: usize,
    pub folding_strategy: EditorFoldingStrategy,
    pub unfold_on_click_after_end_of_line: bool,
    pub show_folding_controls: EditorShowFoldingControls,
    pub indent_guides: bool,
    pub highlight_active_indentation: EditorHighlightActiveIndentation,
    pub mouse_style: EditorMouseStyle,
    pub cursor_smooth_caret_animation: EditorCursorSmoothCaretAnimation,
    pub cursor_style: EditorCursorStyle,
    pub overtype_cursor_style: EditorCursorStyle,
    pub overtype_on_paste: bool,
    pub cursor_blinking: bool,
    pub cursor_width: f32,
    pub cursor_height: usize,
    pub cursor_surrounding_lines: usize,
    pub cursor_surrounding_lines_style: EditorCursorSurroundingLinesStyle,
    pub terminal_scrollback_rows: usize,
    pub terminal_shell_path: Option<String>,
    pub terminal_shell_args: Vec<String>,
    pub terminal_cwd: Option<String>,
    pub terminal_split_cwd: TerminalSplitCwd,
    pub terminal_min_rows: u16,
    pub terminal_min_columns: u16,
    pub terminal_font_size: f32,
    pub terminal_line_height: f32,
    pub terminal_letter_spacing: f32,
    pub terminal_cursor_style: TerminalCursorStyle,
    pub terminal_cursor_width: f32,
    pub terminal_cursor_blinking: bool,
    pub terminal_cursor_style_inactive: TerminalInactiveCursorStyle,
    pub terminal_draw_bold_text_in_bright_colors: bool,
    pub terminal_minimum_contrast_ratio: f32,
    pub terminal_enable_bell: bool,
    pub terminal_bell_duration_ms: u64,
    pub terminal_show_exit_alert: bool,
    pub terminal_hide_on_startup: TerminalHideOnStartup,
    pub terminal_hide_on_last_closed: bool,
    pub terminal_confirm_on_exit: TerminalConfirmOnExit,
    pub terminal_confirm_on_kill: TerminalConfirmOnKill,
    pub terminal_tabs_enabled: bool,
    pub terminal_tabs_default_icon: String,
    pub terminal_tabs_default_color: Option<String>,
    pub terminal_tabs_allow_agent_cli_title: bool,
    pub terminal_tabs_title: String,
    pub terminal_tabs_hide_condition: TerminalTabsHideCondition,
    pub terminal_tabs_show_active_terminal: TerminalTabsShowActiveTerminal,
    pub terminal_tabs_show_actions: TerminalTabsShowActions,
    pub terminal_tabs_focus_mode: TerminalTabsFocusMode,
    pub terminal_tabs_location: TerminalTabsLocation,
    pub terminal_right_click_behavior: TerminalRightClickBehavior,
    pub terminal_middle_click_behavior: TerminalMiddleClickBehavior,
    pub terminal_alt_click_moves_cursor: bool,
    pub terminal_copy_on_selection: bool,
    pub terminal_ignore_bracketed_paste_mode: bool,
    pub terminal_enable_multi_line_paste_warning: TerminalMultiLinePasteWarning,
    pub terminal_word_separators: String,
    pub terminal_mouse_wheel_scroll_sensitivity: f32,
    pub terminal_fast_scroll_sensitivity: f32,
    pub terminal_mouse_wheel_zoom: bool,
    pub keymap: Keymap,
    pub trim_trailing_whitespace: bool,
    pub insert_final_newline: bool,
    pub trim_final_newlines: bool,
    pub updates_github_repository: String,
    pub theme: ThemeSettings,
    pub custom_theme_paths: Vec<String>,
    pub active_custom_theme_path: Option<String>,
    #[serde(default)]
    pub plugins: PluginSettings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorSettingsLoad {
    pub settings: EditorSettings,
    pub quarantined_path: Option<PathBuf>,

    pub future_schema_version: Option<u32>,
}

impl EditorSettingsLoad {
    pub fn future_schema_version_status(&self) -> Option<String> {
        self.future_schema_version.map(|version| {
            format!(
                "Settings were written by a newer version of Kuroya (schema {version}); \
                this build uses them as defaults. Upgrade Kuroya or remove the settings \
                file to reset."
            )
        })
    }
}

pub fn effective_lsp_server_configs(settings_servers: &[LspServerConfig]) -> Vec<LspServerConfig> {
    let (servers, _) = normalize_lsp_server_configs(settings_servers.iter().cloned());
    servers
}

pub(crate) fn merge_lsp_server_configs_with_defaults(
    settings_servers: &[LspServerConfig],
) -> Vec<LspServerConfig> {
    let mut servers = default_server_configs();
    let (settings_servers, _) = normalize_lsp_server_configs(settings_servers.iter().cloned());
    let mut replaced_defaults: Vec<usize> = Vec::new();
    for server in settings_servers {
        let index = servers
            .iter()
            .enumerate()
            .find(|(index, existing)| {
                existing.language == server.language && !replaced_defaults.contains(index)
            })
            .map(|(index, _)| index);
        match index {
            Some(index) => {
                servers[index] = server;
                replaced_defaults.push(index);
            }
            None => servers.push(server),
        }
    }
    servers
}

pub fn default_lsp_server_for_language(language: &str) -> Option<LspServerConfig> {
    default_server_configs()
        .into_iter()
        .find(|server| server.language == language)
}

pub fn lsp_server_matches_builtin(server: &LspServerConfig) -> bool {
    default_lsp_server_for_language(&server.language).is_some_and(|default| {
        default.command == server.command
            && default.args == server.args
            && default.extensions == server.extensions
            && default.root_markers == server.root_markers
    })
}

pub fn missing_builtin_lsp_servers(configured: &[LspServerConfig]) -> Vec<LspServerConfig> {
    default_server_configs()
        .into_iter()
        .filter(|default| {
            !configured
                .iter()
                .any(|server| server.language == default.language)
        })
        .collect()
}

pub fn restore_builtin_lsp_servers(servers: &mut Vec<LspServerConfig>) -> bool {
    let missing = missing_builtin_lsp_servers(servers);
    let changed = !missing.is_empty();
    servers.extend(missing);
    changed
}

fn sanitize_lsp_server_configs(servers: &mut Vec<LspServerConfig>) -> bool {
    let (normalized, changed) = normalize_lsp_server_configs(std::mem::take(servers));
    *servers = normalized;
    changed
}

fn normalize_lsp_server_configs(
    settings_servers: impl IntoIterator<Item = LspServerConfig>,
) -> (Vec<LspServerConfig>, bool) {
    let original: Vec<LspServerConfig> = settings_servers.into_iter().collect();
    let mut normalized: Vec<LspServerConfig> =
        Vec::with_capacity(original.len().min(SETTINGS_LIST_MAX_ITEMS));
    let mut changed = original.len() > SETTINGS_LIST_MAX_ITEMS;

    for server in original {
        if normalized.len() >= SETTINGS_LIST_MAX_ITEMS {
            changed = true;
            continue;
        }

        let original_server = server.clone();
        let Some(server) = normalized_lsp_server_config(server) else {
            changed = true;
            continue;
        };
        changed |= server != original_server;

        if normalized.contains(&server) {
            changed = true;
        } else {
            normalized.push(server);
        }
    }

    (normalized, changed)
}

fn normalized_lsp_server_config(mut server: LspServerConfig) -> Option<LspServerConfig> {
    server.language =
        normalize_settings_plain_string(&server.language, SETTINGS_MAP_KEY_MAX_CHARS, true);
    server.language.make_ascii_lowercase();
    server.command =
        normalize_settings_plain_string(&server.command, SETTINGS_STRING_MAX_CHARS, true);
    sanitize_settings_string_list(
        &mut server.args,
        SETTINGS_LIST_MAX_ITEMS,
        SETTINGS_STRING_MAX_CHARS,
        false,
    );
    sanitize_settings_string_list(
        &mut server.extensions,
        SETTINGS_LIST_MAX_ITEMS,
        SETTINGS_STRING_MAX_CHARS,
        true,
    );
    normalize_lsp_server_extensions(&mut server.extensions);
    sanitize_settings_string_list(
        &mut server.root_markers,
        SETTINGS_LIST_MAX_ITEMS,
        SETTINGS_STRING_MAX_CHARS,
        true,
    );

    if server.language.is_empty() || server.command.is_empty() {
        None
    } else {
        Some(server)
    }
}

fn normalize_lsp_server_extensions(extensions: &mut Vec<String>) -> bool {
    let original = std::mem::take(extensions);
    let mut normalized = Vec::with_capacity(original.len());
    let mut changed = false;

    for extension in original {
        let trimmed = extension.trim_start_matches('.').to_owned();
        if trimmed.is_empty() {
            changed = true;
            continue;
        }
        changed |= trimmed != extension;
        if normalized.contains(&trimmed) {
            changed = true;
            continue;
        }
        normalized.push(trimmed);
    }

    *extensions = normalized;
    changed
}

fn current_settings_schema_version() -> u32 {
    SETTINGS_SCHEMA_VERSION
}

fn replace_if_changed<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

#[cfg(test)]
mod tests;

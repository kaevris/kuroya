use crate::{
    buffer_find_history::MAX_BUFFER_FIND_HISTORY,
    command_palette_items::CommandPaletteQueryMemoryEntry,
    command_palette_items::{
        MAX_COMMAND_PALETTE_QUERY_MEMORY, MAX_COMMAND_PALETTE_RECENT_COMMANDS,
    },
    history::{
        CLOSED_FILE_HISTORY_LIMIT, ClosedFileEntry, NAVIGATION_HISTORY_LIMIT, NavigationLocation,
    },
    lsp_workspace_symbol_ranking::{
        MAX_WORKSPACE_SYMBOL_QUERY_MEMORY, WorkspaceSymbolQueryMemoryEntry,
    },
    panel_layout::PanelPlacement,
    project_search_state::{MAX_PROJECT_SEARCH_RECENT_QUERIES, ProjectSearchQuery},
    quick_open::{
        MAX_QUICK_OPEN_QUERY_MEMORY, MAX_QUICK_OPEN_RECENT_FILES, QuickOpenQueryMemoryEntry,
    },
    source_control_panel::SOURCE_CONTROL_COMMIT_HISTORY_LIMIT,
};
use kuroya_core::{BufferHistorySnapshot, Command, EditorVimSettings, ThemeSettings};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{IgnoredAny, SeqAccess, Visitor},
};
use std::{fmt, marker::PhantomData, path::PathBuf};

pub(crate) const APP_STATE_RECENT_PROJECTS_MAX: usize = 12;
pub(crate) const APP_STATE_TRUSTED_WORKSPACES_MAX: usize = 128;
pub(crate) const APP_STATE_CUSTOM_THEME_PATHS_MAX: usize = 256;
pub(crate) const APP_STATE_SETTING_TEXT_MAX_CHARS: usize = 4096;
pub(crate) const PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS: usize = 4 * 1024;
pub(crate) const PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS: usize = 2 * 1024 * 1024;
pub(crate) const PERSISTED_SESSION_TERMINAL_SCROLLBACK_MAX_CHARS: usize = 256 * 1024;
pub(crate) const PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS: usize = 1024;
pub(crate) const PERSISTED_SESSION_PATH_TEXT_MAX_CHARS: usize = 4096;
pub(crate) const PERSISTED_SESSION_PATHS_MAX: usize = 1024;
pub(crate) const PERSISTED_SESSION_PANES_MAX: usize = 64;
pub(crate) const PERSISTED_SESSION_VIEW_STATES_MAX: usize = 1024;
pub(crate) const PERSISTED_SESSION_HISTORY_STATES_MAX: usize = 256;
pub(crate) const PERSISTED_SESSION_RECOVERY_BUFFERS_MAX: usize = 128;
pub(crate) const PERSISTED_SESSION_RECOVERY_SKIPPED_MAX: usize = 128;
pub(crate) const PERSISTED_SESSION_TERMINAL_SESSIONS_MAX: usize = 12;
pub(crate) const PERSISTED_SESSION_SELECTIONS_MAX: usize = 64;
pub(crate) const PERSISTED_SESSION_FOLD_RANGES_MAX: usize = 4096;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AppState {
    #[serde(default, deserialize_with = "deserialize_app_state_recent_projects")]
    pub recent_projects: Vec<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_app_state_trusted_workspaces")]
    pub trusted_workspaces: Vec<PathBuf>,
    #[serde(default)]
    pub vim_keybindings: Option<bool>,
    #[serde(default)]
    pub vim: Option<EditorVimSettings>,
    #[serde(default)]
    pub theme: Option<ThemeSettings>,
    #[serde(default, deserialize_with = "deserialize_app_state_custom_theme_paths")]
    pub custom_theme_paths: Vec<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_app_state_optional_setting_text"
    )]
    pub active_custom_theme_path: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_app_state_optional_setting_text"
    )]
    pub editor_font_path: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_app_state_optional_setting_text"
    )]
    pub ui_font_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PersistedSession {
    #[serde(default, deserialize_with = "deserialize_session_path")]
    pub workspace_root: PathBuf,
    #[serde(default, deserialize_with = "deserialize_session_paths")]
    pub open_files: Vec<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_optional_session_path")]
    pub active_path: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_session_pane_paths")]
    pub pane_paths: Vec<Option<PathBuf>>,
    #[serde(default, deserialize_with = "deserialize_session_pane_weights")]
    pub pane_weights: Vec<f32>,
    #[serde(default)]
    pub active_pane_index: Option<usize>,
    #[serde(default, deserialize_with = "deserialize_session_view_states")]
    pub view_states: Vec<BufferViewState>,
    #[serde(default, deserialize_with = "deserialize_session_pane_view_states")]
    pub pane_view_states: Vec<PaneBufferViewState>,
    #[serde(default, deserialize_with = "deserialize_session_history_states")]
    pub history_states: Vec<BufferHistoryState>,
    #[serde(default, deserialize_with = "deserialize_session_recovery_view_states")]
    pub recovery_view_states: Vec<RecoveredBufferViewState>,
    #[serde(
        default,
        deserialize_with = "deserialize_session_recovery_history_states"
    )]
    pub recovery_history_states: Vec<RecoveredBufferHistoryState>,
    #[serde(default, deserialize_with = "deserialize_session_fold_states")]
    pub fold_states: Vec<BufferFoldState>,
    #[serde(default = "default_explorer_width")]
    pub explorer_width: f32,
    #[serde(default, deserialize_with = "deserialize_session_paths")]
    pub explorer_expanded: Vec<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_optional_session_path")]
    pub explorer_revealed_path: Option<PathBuf>,
    #[serde(default)]
    pub project_search_open: bool,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub project_search_query: String,
    #[serde(default)]
    pub project_search_case_sensitive: bool,
    #[serde(default)]
    pub project_search_whole_word: bool,
    #[serde(default)]
    pub project_search_regex: bool,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub project_search_include: String,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub project_search_exclude: String,
    #[serde(default, deserialize_with = "deserialize_project_search_recent")]
    pub project_search_recent: Vec<ProjectSearchQuery>,
    #[serde(default)]
    pub buffer_find_open: bool,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub buffer_find_query: String,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub buffer_find_replacement: String,
    #[serde(default)]
    pub buffer_find_case_sensitive: bool,
    #[serde(default)]
    pub buffer_find_whole_word: bool,
    #[serde(default)]
    pub buffer_find_regex: bool,
    #[serde(default)]
    pub buffer_find_preserve_case: bool,
    #[serde(default, deserialize_with = "deserialize_buffer_find_history")]
    pub buffer_find_query_history: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_buffer_find_history")]
    pub buffer_find_replacement_history: Vec<String>,
    #[serde(default)]
    pub settings_panel_open: bool,
    #[serde(default)]
    pub theme_picker_open: bool,
    #[serde(default)]
    pub keybindings_open: bool,
    #[serde(default)]
    pub symbols_panel_open: bool,
    #[serde(default)]
    pub symbols_panel_placement: PanelPlacement,
    #[serde(default = "default_symbols_panel_width")]
    pub symbols_panel_width: f32,
    #[serde(default)]
    pub diagnostics_panel_open: bool,
    #[serde(default)]
    pub diagnostics_panel_placement: PanelPlacement,
    #[serde(default = "default_diagnostics_panel_width")]
    pub diagnostics_panel_width: f32,
    #[serde(default)]
    pub source_control_open: bool,
    #[serde(default)]
    pub source_control_placement: PanelPlacement,
    #[serde(default = "default_source_control_width")]
    pub source_control_width: f32,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub source_control_query: String,
    #[serde(default)]
    pub source_control_view: PersistedSourceControlViewMode,
    #[serde(default)]
    pub source_control_sort: PersistedSourceControlSortMode,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub source_control_commit_message: String,
    #[serde(
        default,
        deserialize_with = "deserialize_source_control_commit_history"
    )]
    pub source_control_commit_history: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub source_control_stash_message: String,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub source_control_stash_query: String,
    #[serde(default)]
    pub source_control_stashes_open: bool,
    #[serde(default)]
    pub source_control_history_open: bool,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    pub source_control_history_query: String,
    #[serde(default)]
    pub source_control_unstaged_collapsed: bool,
    #[serde(default)]
    pub source_control_untracked_collapsed: bool,
    #[serde(default)]
    pub source_control_staged_collapsed: bool,
    #[serde(default = "default_terminal_visible")]
    pub terminal_visible: bool,
    #[serde(default = "default_terminal_height")]
    pub terminal_height: f32,
    #[serde(default, deserialize_with = "deserialize_terminal_sessions")]
    pub terminal_sessions: Vec<PersistedTerminalSession>,
    #[serde(default)]
    pub terminal_active_session: usize,
    #[serde(default)]
    pub terminal_split_view: bool,
    #[serde(default, deserialize_with = "deserialize_terminal_split_weights")]
    pub terminal_split_weights: Vec<f32>,
    #[serde(default, deserialize_with = "deserialize_session_paths")]
    pub recent_projects: Vec<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_quick_open_recent_files")]
    pub quick_open_recent_files: Vec<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_quick_open_query_memory")]
    pub quick_open_query_memory: Vec<QuickOpenQueryMemoryEntry>,
    #[serde(
        default,
        deserialize_with = "deserialize_workspace_symbol_query_memory"
    )]
    pub workspace_symbol_query_memory: Vec<WorkspaceSymbolQueryMemoryEntry>,
    #[serde(default, deserialize_with = "deserialize_command_recent")]
    pub command_recent: Vec<Command>,
    #[serde(default, deserialize_with = "deserialize_command_query_memory")]
    pub command_query_memory: Vec<CommandPaletteQueryMemoryEntry>,
    #[serde(default, deserialize_with = "deserialize_navigation_history")]
    pub navigation_back: Vec<PersistedNavigationLocation>,
    #[serde(default, deserialize_with = "deserialize_navigation_history")]
    pub navigation_forward: Vec<PersistedNavigationLocation>,
    #[serde(default, deserialize_with = "deserialize_closed_files")]
    pub closed_files: Vec<PersistedClosedFileEntry>,
    #[serde(default, deserialize_with = "deserialize_recovered_buffers")]
    pub recovery: Vec<RecoveredBuffer>,
    #[serde(default, deserialize_with = "deserialize_skipped_recovered_buffers")]
    pub recovery_skipped: Vec<SkippedRecoveredBuffer>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PersistedSourceControlViewMode {
    #[default]
    List,
    Tree,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PersistedSourceControlSortMode {
    #[default]
    Path,
    Name,
    Status,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PersistedTerminalSession {
    #[serde(default, deserialize_with = "deserialize_optional_session_path")]
    pub cwd: Option<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_terminal_scrollback")]
    pub scrollback: String,
    #[serde(default)]
    pub scrollback_offset: usize,
    #[serde(default, deserialize_with = "deserialize_optional_display_text")]
    pub custom_title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_display_text")]
    pub process_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_status: Option<PersistedTerminalProcessStatus>,
    #[serde(default, deserialize_with = "deserialize_optional_display_text")]
    pub window_title: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PersistedTerminalProcessStatus {
    Running,
    Stopped,
    Exited {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
    TerminalError,
    #[default]
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveredBuffer {
    #[serde(default, deserialize_with = "deserialize_optional_session_path")]
    pub path: Option<PathBuf>,
    #[serde(deserialize_with = "deserialize_display_text")]
    pub display_name: String,
    #[serde(deserialize_with = "deserialize_recovery_text")]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BufferViewState {
    #[serde(deserialize_with = "deserialize_session_path")]
    pub path: PathBuf,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub scroll_line: usize,
    #[serde(default)]
    pub horizontal_scroll_offset: f32,
    #[serde(default, deserialize_with = "deserialize_buffer_selections")]
    pub selections: Vec<BufferSelectionState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaneBufferViewState {
    pub pane_index: usize,
    #[serde(deserialize_with = "deserialize_session_path")]
    pub path: PathBuf,
    pub scroll_line: usize,
    #[serde(default)]
    pub horizontal_scroll_offset: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BufferSelectionState {
    pub anchor_line: usize,
    pub anchor_column: usize,
    pub cursor_line: usize,
    pub cursor_column: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BufferHistoryState {
    #[serde(deserialize_with = "deserialize_session_path")]
    pub path: PathBuf,
    pub history: BufferHistorySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveredBufferViewState {
    pub recovery_index: usize,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub scroll_line: usize,
    #[serde(default)]
    pub horizontal_scroll_offset: f32,
    #[serde(default, deserialize_with = "deserialize_buffer_selections")]
    pub selections: Vec<BufferSelectionState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveredBufferHistoryState {
    pub recovery_index: usize,
    pub history: BufferHistorySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BufferFoldState {
    #[serde(deserialize_with = "deserialize_session_path")]
    pub path: PathBuf,
    #[serde(default, deserialize_with = "deserialize_fold_ranges")]
    pub ranges: Vec<PersistedFoldRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedNavigationLocation {
    #[serde(deserialize_with = "deserialize_session_path")]
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

impl PersistedNavigationLocation {
    pub(crate) fn from_navigation_location(location: &NavigationLocation) -> Self {
        Self {
            path: location.path.clone(),
            line: location.line,
            column: location.column,
        }
    }

    pub(crate) fn into_navigation_location(self) -> NavigationLocation {
        NavigationLocation::new(self.path, self.line, self.column)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedClosedFileEntry {
    #[serde(deserialize_with = "deserialize_session_path")]
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
}

impl PersistedClosedFileEntry {
    pub(crate) fn from_closed_file_entry(entry: &ClosedFileEntry) -> Self {
        Self {
            path: entry.path.clone(),
            line: entry.line,
            column: entry.column,
        }
    }

    pub(crate) fn into_closed_file_entry(self) -> ClosedFileEntry {
        ClosedFileEntry::new(self.path, self.line, self.column)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PersistedFoldRange {
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkippedRecoveredBuffer {
    #[serde(default, deserialize_with = "deserialize_optional_session_path")]
    pub path: Option<PathBuf>,
    #[serde(deserialize_with = "deserialize_display_text")]
    pub display_name: String,
    pub bytes: usize,
    #[serde(deserialize_with = "deserialize_display_text")]
    pub reason: String,
}

fn default_explorer_width() -> f32 {
    260.0
}

fn default_symbols_panel_width() -> f32 {
    300.0
}

fn default_diagnostics_panel_width() -> f32 {
    340.0
}

fn default_source_control_width() -> f32 {
    320.0
}

fn default_terminal_visible() -> bool {
    false
}

fn default_terminal_height() -> f32 {
    220.0
}

macro_rules! bounded_vec_deserializer {
    ($name:ident, $ty:ty, $limit:expr) => {
        fn $name<'de, D>(deserializer: D) -> Result<Vec<$ty>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_bounded_vec::<D, $ty, $limit>(deserializer)
        }
    };
}

macro_rules! bounded_mapped_vec_deserializer {
    ($name:ident, $raw:ty, $ty:ty, $limit:expr) => {
        fn $name<'de, D>(deserializer: D) -> Result<Vec<$ty>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_bounded_mapped_vec::<D, $raw, $ty, $limit>(deserializer)
        }
    };
}

macro_rules! bounded_string_vec_deserializer {
    ($name:ident, $limit:expr, $chars:expr) => {
        fn $name<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_bounded_string_vec::<D, $limit, $chars>(deserializer)
        }
    };
}

macro_rules! bounded_string_deserializer {
    ($name:ident, $chars:expr) => {
        fn $name<'de, D>(deserializer: D) -> Result<String, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_bounded_string::<D, $chars>(deserializer)
        }
    };
}

bounded_mapped_vec_deserializer!(
    deserialize_app_state_recent_projects,
    BoundedPath,
    PathBuf,
    APP_STATE_RECENT_PROJECTS_MAX
);
bounded_mapped_vec_deserializer!(
    deserialize_app_state_trusted_workspaces,
    BoundedPath,
    PathBuf,
    APP_STATE_TRUSTED_WORKSPACES_MAX
);
bounded_string_vec_deserializer!(
    deserialize_app_state_custom_theme_paths,
    APP_STATE_CUSTOM_THEME_PATHS_MAX,
    APP_STATE_SETTING_TEXT_MAX_CHARS
);
bounded_mapped_vec_deserializer!(
    deserialize_session_paths,
    BoundedPath,
    PathBuf,
    PERSISTED_SESSION_PATHS_MAX
);
bounded_mapped_vec_deserializer!(
    deserialize_session_pane_paths,
    BoundedOptionalPath,
    Option<PathBuf>,
    PERSISTED_SESSION_PANES_MAX
);
bounded_vec_deserializer!(
    deserialize_session_pane_weights,
    f32,
    PERSISTED_SESSION_PANES_MAX
);
bounded_vec_deserializer!(
    deserialize_session_view_states,
    BufferViewState,
    PERSISTED_SESSION_VIEW_STATES_MAX
);
bounded_vec_deserializer!(
    deserialize_session_pane_view_states,
    PaneBufferViewState,
    PERSISTED_SESSION_VIEW_STATES_MAX
);
bounded_vec_deserializer!(
    deserialize_session_history_states,
    BufferHistoryState,
    PERSISTED_SESSION_HISTORY_STATES_MAX
);
bounded_vec_deserializer!(
    deserialize_session_recovery_view_states,
    RecoveredBufferViewState,
    PERSISTED_SESSION_VIEW_STATES_MAX
);
bounded_vec_deserializer!(
    deserialize_session_recovery_history_states,
    RecoveredBufferHistoryState,
    PERSISTED_SESSION_HISTORY_STATES_MAX
);
bounded_vec_deserializer!(
    deserialize_session_fold_states,
    BufferFoldState,
    PERSISTED_SESSION_VIEW_STATES_MAX
);
bounded_mapped_vec_deserializer!(
    deserialize_project_search_recent,
    RestoredProjectSearchQuery,
    ProjectSearchQuery,
    MAX_PROJECT_SEARCH_RECENT_QUERIES
);
bounded_string_vec_deserializer!(
    deserialize_buffer_find_history,
    MAX_BUFFER_FIND_HISTORY,
    PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
);
bounded_string_vec_deserializer!(
    deserialize_source_control_commit_history,
    SOURCE_CONTROL_COMMIT_HISTORY_LIMIT,
    PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
);
bounded_vec_deserializer!(
    deserialize_terminal_sessions,
    PersistedTerminalSession,
    PERSISTED_SESSION_TERMINAL_SESSIONS_MAX
);
bounded_vec_deserializer!(
    deserialize_terminal_split_weights,
    f32,
    PERSISTED_SESSION_TERMINAL_SESSIONS_MAX
);
bounded_mapped_vec_deserializer!(
    deserialize_quick_open_recent_files,
    BoundedPath,
    PathBuf,
    MAX_QUICK_OPEN_RECENT_FILES
);
bounded_mapped_vec_deserializer!(
    deserialize_quick_open_query_memory,
    RestoredQuickOpenQueryMemoryEntry,
    QuickOpenQueryMemoryEntry,
    MAX_QUICK_OPEN_QUERY_MEMORY
);
bounded_mapped_vec_deserializer!(
    deserialize_workspace_symbol_query_memory,
    RestoredWorkspaceSymbolQueryMemoryEntry,
    WorkspaceSymbolQueryMemoryEntry,
    MAX_WORKSPACE_SYMBOL_QUERY_MEMORY
);
bounded_vec_deserializer!(
    deserialize_command_recent,
    Command,
    MAX_COMMAND_PALETTE_RECENT_COMMANDS
);
bounded_mapped_vec_deserializer!(
    deserialize_command_query_memory,
    RestoredCommandPaletteQueryMemoryEntry,
    CommandPaletteQueryMemoryEntry,
    MAX_COMMAND_PALETTE_QUERY_MEMORY
);
bounded_vec_deserializer!(
    deserialize_navigation_history,
    PersistedNavigationLocation,
    NAVIGATION_HISTORY_LIMIT
);
bounded_vec_deserializer!(
    deserialize_closed_files,
    PersistedClosedFileEntry,
    CLOSED_FILE_HISTORY_LIMIT
);
bounded_vec_deserializer!(
    deserialize_recovered_buffers,
    RecoveredBuffer,
    PERSISTED_SESSION_RECOVERY_BUFFERS_MAX
);
bounded_vec_deserializer!(
    deserialize_skipped_recovered_buffers,
    SkippedRecoveredBuffer,
    PERSISTED_SESSION_RECOVERY_SKIPPED_MAX
);
bounded_vec_deserializer!(
    deserialize_buffer_selections,
    BufferSelectionState,
    PERSISTED_SESSION_SELECTIONS_MAX
);
bounded_vec_deserializer!(
    deserialize_fold_ranges,
    PersistedFoldRange,
    PERSISTED_SESSION_FOLD_RANGES_MAX
);
bounded_string_deserializer!(
    deserialize_session_text,
    PERSISTED_SESSION_VOLATILE_TEXT_MAX_CHARS
);
bounded_string_deserializer!(
    deserialize_display_text,
    PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS
);
bounded_string_deserializer!(
    deserialize_recovery_text,
    PERSISTED_SESSION_RECOVERY_TEXT_MAX_CHARS
);
bounded_string_deserializer!(
    deserialize_terminal_scrollback,
    PERSISTED_SESSION_TERMINAL_SCROLLBACK_MAX_CHARS
);

fn deserialize_optional_display_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_optional_string::<D, PERSISTED_SESSION_DISPLAY_TEXT_MAX_CHARS>(deserializer)
}

fn deserialize_app_state_optional_setting_text<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_optional_string::<D, APP_STATE_SETTING_TEXT_MAX_CHARS>(deserializer)
}

fn deserialize_session_path<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(BoundedPath::deserialize(deserializer)?
        .into_restored_entry()
        .unwrap_or_default())
}

fn deserialize_optional_session_path<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(BoundedOptionalPath::deserialize(deserializer)?
        .into_restored_entry()
        .flatten())
}

fn deserialize_bounded_vec<'de, D, T, const LIMIT: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::<T, LIMIT> {
        marker: PhantomData,
    })
}

fn deserialize_bounded_mapped_vec<'de, D, Raw, T, const LIMIT: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    Raw: Deserialize<'de> + RestoredEntry<T>,
{
    deserializer.deserialize_seq(BoundedMappedVecVisitor::<Raw, T, LIMIT> {
        marker: PhantomData,
    })
}

struct BoundedVecVisitor<T, const LIMIT: usize> {
    marker: PhantomData<T>,
}

impl<'de, T, const LIMIT: usize> Visitor<'de> for BoundedVecVisitor<T, LIMIT>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a list with at most {LIMIT} restored entries")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let capacity = seq.size_hint().unwrap_or_default().min(LIMIT);
        let mut values = Vec::with_capacity(capacity);
        while values.len() < LIMIT {
            let Some(value) = seq.next_element()? else {
                return Ok(values);
            };
            values.push(value);
        }
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(values)
    }
}

struct BoundedMappedVecVisitor<Raw, T, const LIMIT: usize> {
    marker: PhantomData<(Raw, T)>,
}

impl<'de, Raw, T, const LIMIT: usize> Visitor<'de> for BoundedMappedVecVisitor<Raw, T, LIMIT>
where
    Raw: Deserialize<'de> + RestoredEntry<T>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a list with at most {LIMIT} restored entries")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let capacity = seq.size_hint().unwrap_or_default().min(LIMIT);
        let mut values = Vec::with_capacity(capacity);
        while values.len() < LIMIT {
            let Some(value) = seq.next_element::<Raw>()? else {
                return Ok(values);
            };

            if let Some(mapped) = value.into_restored_entry() {
                values.push(mapped);
            }
        }
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(values)
    }
}

trait RestoredEntry<T> {
    fn into_restored_entry(self) -> Option<T>;
}

fn deserialize_bounded_string_vec<'de, D, const LIMIT: usize, const CHARS: usize>(
    deserializer: D,
) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedStringVecVisitor::<LIMIT, CHARS>)
}

struct BoundedStringVecVisitor<const LIMIT: usize, const CHARS: usize>;

impl<'de, const LIMIT: usize, const CHARS: usize> Visitor<'de>
    for BoundedStringVecVisitor<LIMIT, CHARS>
{
    type Value = Vec<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a string list with at most {LIMIT} entries and {CHARS} chars per entry"
        )
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let capacity = seq.size_hint().unwrap_or_default().min(LIMIT);
        let mut values = Vec::with_capacity(capacity);
        while values.len() < LIMIT {
            let Some(value) = seq.next_element::<BoundedString<CHARS>>()? else {
                return Ok(values);
            };
            values.push(value.0);
        }
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(values)
    }
}

fn deserialize_bounded_optional_string<'de, D, const CHARS: usize>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<BoundedString<CHARS>>::deserialize(deserializer)?.map(|value| value.0))
}

fn deserialize_bounded_string<'de, D, const CHARS: usize>(
    deserializer: D,
) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(BoundedString::<CHARS>::deserialize(deserializer)?.0)
}

struct BoundedString<const CHARS: usize>(String);

impl<'de, const CHARS: usize> Deserialize<'de> for BoundedString<CHARS> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(BoundedStringVisitor::<CHARS>)
    }
}

struct BoundedPath(Option<PathBuf>);

impl<'de> Deserialize<'de> for BoundedPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(BoundedPathVisitor)
    }
}

impl RestoredEntry<PathBuf> for BoundedPath {
    fn into_restored_entry(self) -> Option<PathBuf> {
        self.0
    }
}

struct BoundedPathVisitor;

impl<'de> Visitor<'de> for BoundedPathVisitor {
    type Value = BoundedPath;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a path with at most {PERSISTED_SESSION_PATH_TEXT_MAX_CHARS} chars restored"
        )
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BoundedPath(bounded_path(value)))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BoundedPath(bounded_path(&value)))
    }
}

fn bounded_path(value: &str) -> Option<PathBuf> {
    if value.chars().count() > PERSISTED_SESSION_PATH_TEXT_MAX_CHARS {
        return None;
    }
    if path_text_contains_control(value) {
        return None;
    }
    Some(PathBuf::from(value))
}

fn path_text_contains_control(value: &str) -> bool {
    value.chars().any(is_unsafe_path_char)
}

fn is_unsafe_path_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{feff}'
        )
}

enum BoundedOptionalPath {
    Kept(Option<PathBuf>),
    Rejected,
}

impl<'de> Deserialize<'de> for BoundedOptionalPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match Option::<BoundedPath>::deserialize(deserializer)? {
            None => Self::Kept(None),
            Some(bounded) => match bounded.0 {
                Some(path) => Self::Kept(Some(path)),
                None => Self::Rejected,
            },
        })
    }
}

impl RestoredEntry<Option<PathBuf>> for BoundedOptionalPath {
    fn into_restored_entry(self) -> Option<Option<PathBuf>> {
        match self {
            Self::Kept(path) => Some(path),
            Self::Rejected => None,
        }
    }
}

#[derive(Deserialize)]
struct RestoredProjectSearchQuery {
    #[serde(default, deserialize_with = "deserialize_session_text")]
    query: String,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default)]
    whole_word: bool,
    #[serde(default)]
    regex: bool,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    include: String,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    exclude: String,
}

impl From<RestoredProjectSearchQuery> for ProjectSearchQuery {
    fn from(value: RestoredProjectSearchQuery) -> Self {
        Self {
            query: value.query,
            case_sensitive: value.case_sensitive,
            whole_word: value.whole_word,
            regex: value.regex,
            include: value.include,
            exclude: value.exclude,
        }
    }
}

impl RestoredEntry<ProjectSearchQuery> for RestoredProjectSearchQuery {
    fn into_restored_entry(self) -> Option<ProjectSearchQuery> {
        Some(self.into())
    }
}

#[derive(Deserialize)]
struct RestoredQuickOpenQueryMemoryEntry {
    #[serde(default, deserialize_with = "deserialize_session_text")]
    query: String,
    #[serde(deserialize_with = "deserialize_session_path")]
    path: PathBuf,
    #[serde(default = "default_restored_query_memory_uses")]
    uses: u32,
}

impl From<RestoredQuickOpenQueryMemoryEntry> for QuickOpenQueryMemoryEntry {
    fn from(value: RestoredQuickOpenQueryMemoryEntry) -> Self {
        Self {
            query: value.query,
            path: value.path,
            uses: value.uses,
        }
    }
}

impl RestoredEntry<QuickOpenQueryMemoryEntry> for RestoredQuickOpenQueryMemoryEntry {
    fn into_restored_entry(self) -> Option<QuickOpenQueryMemoryEntry> {
        Some(self.into())
    }
}

#[derive(Deserialize)]
struct RestoredWorkspaceSymbolQueryMemoryEntry {
    #[serde(default, deserialize_with = "deserialize_session_text")]
    query: String,
    #[serde(deserialize_with = "deserialize_session_path")]
    path: PathBuf,
    #[serde(default, deserialize_with = "deserialize_session_text")]
    name: String,
    #[serde(default)]
    kind: u8,
    line: usize,
    column: usize,
    #[serde(default = "default_restored_query_memory_uses")]
    uses: u32,
}

impl From<RestoredWorkspaceSymbolQueryMemoryEntry> for WorkspaceSymbolQueryMemoryEntry {
    fn from(value: RestoredWorkspaceSymbolQueryMemoryEntry) -> Self {
        Self {
            query: value.query,
            path: value.path,
            name: value.name,
            kind: value.kind,
            line: value.line,
            column: value.column,
            uses: value.uses,
        }
    }
}

impl RestoredEntry<WorkspaceSymbolQueryMemoryEntry> for RestoredWorkspaceSymbolQueryMemoryEntry {
    fn into_restored_entry(self) -> Option<WorkspaceSymbolQueryMemoryEntry> {
        Some(self.into())
    }
}

#[derive(Deserialize)]
struct RestoredCommandPaletteQueryMemoryEntry {
    #[serde(default, deserialize_with = "deserialize_session_text")]
    query: String,
    command: Command,
    #[serde(default = "default_restored_query_memory_uses")]
    uses: u32,
}

impl From<RestoredCommandPaletteQueryMemoryEntry> for CommandPaletteQueryMemoryEntry {
    fn from(value: RestoredCommandPaletteQueryMemoryEntry) -> Self {
        Self {
            query: value.query,
            command: value.command,
            uses: value.uses,
        }
    }
}

impl RestoredEntry<CommandPaletteQueryMemoryEntry> for RestoredCommandPaletteQueryMemoryEntry {
    fn into_restored_entry(self) -> Option<CommandPaletteQueryMemoryEntry> {
        Some(self.into())
    }
}

fn default_restored_query_memory_uses() -> u32 {
    1
}

struct BoundedStringVisitor<const CHARS: usize>;

impl<'de, const CHARS: usize> Visitor<'de> for BoundedStringVisitor<CHARS> {
    type Value = BoundedString<CHARS>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a string with at most {CHARS} chars restored")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(BoundedString(bounded_string_chars(value, CHARS)))
    }

    fn visit_string<E>(self, mut value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        truncate_string_chars(&mut value, CHARS);
        Ok(BoundedString(value))
    }
}

fn bounded_string_chars(value: &str, max_chars: usize) -> String {
    let mut value = value.to_owned();
    truncate_string_chars(&mut value, max_chars);
    value
}

fn truncate_string_chars(value: &mut String, max_chars: usize) {
    if max_chars == 0 {
        value.clear();
        return;
    }
    if let Some((byte_index, _)) = value.char_indices().nth(max_chars) {
        value.truncate(byte_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_parses_when_workspace_root_is_missing() {
        let value = serde_json::json!({
            "open_files": ["src/main.rs"],
            "recovery": [{
                "path": "src/main.rs",
                "display_name": "main.rs",
                "text": "unsaved",
            }],
        });

        let session = serde_json::from_value::<PersistedSession>(value).unwrap();

        assert!(session.workspace_root.as_os_str().is_empty());
        assert_eq!(session.recovery.len(), 1);
        assert_eq!(session.recovery[0].display_name, "main.rs");
    }

    #[test]
    fn restore_normalizes_placeholder_session_with_missing_workspace_root() {
        let value = serde_json::json!({
            "open_files": ["src/main.rs"],
            "recovery": [{
                "path": "src/main.rs",
                "display_name": "main.rs",
                "text": "unsaved",
            }],
        });
        let mut session = serde_json::from_value::<PersistedSession>(value).unwrap();
        let root = PathBuf::from("workspace").join("current");

        crate::persistence_session::normalize_persisted_session_paths_for_restore(
            &root,
            &mut session,
        );

        assert_eq!(session.open_files, vec![root.join("src/main.rs")]);
        assert_eq!(session.recovery[0].path, Some(root.join("src/main.rs")));
    }

    #[test]
    fn bounded_paths_prune_oversized_and_control_char_entries() {
        let over_long = "a".repeat(PERSISTED_SESSION_PATH_TEXT_MAX_CHARS + 1);
        let value = serde_json::json!({
            "open_files": [
                "src/kept.rs",
                over_long,
                "src/\u{0000}null.rs",
                "src/\u{202e}reverse.rs",
                "src/kept-too.rs",
            ],
        });

        let session = serde_json::from_value::<PersistedSession>(value).unwrap();

        assert_eq!(
            session.open_files,
            vec![
                PathBuf::from("src/kept.rs"),
                PathBuf::from("src/kept-too.rs"),
            ]
        );
    }

    #[test]
    fn bounded_pane_paths_keep_null_slots_and_prune_rejected_paths() {
        let over_long = "a".repeat(PERSISTED_SESSION_PATH_TEXT_MAX_CHARS + 1);
        let value = serde_json::json!({
            "pane_paths": [null, "src/main.rs", over_long],
        });

        let session = serde_json::from_value::<PersistedSession>(value).unwrap();

        assert_eq!(
            session.pane_paths,
            vec![None, Some(PathBuf::from("src/main.rs"))]
        );
    }

    #[test]
    fn bounded_paths_accept_paths_at_the_char_budget() {
        let at_budget = "b".repeat(PERSISTED_SESSION_PATH_TEXT_MAX_CHARS);
        let value = serde_json::json!({ "open_files": [at_budget] });

        let session = serde_json::from_value::<PersistedSession>(value).unwrap();

        assert_eq!(session.open_files.len(), 1);
        assert_eq!(
            session.open_files[0].as_os_str().to_string_lossy().len(),
            PERSISTED_SESSION_PATH_TEXT_MAX_CHARS
        );
    }
}

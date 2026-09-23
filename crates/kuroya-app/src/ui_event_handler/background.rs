use crate::{
    KuroyaApp,
    path_display::display_error_label_cow,
    plugin_activation_runtime::{
        activate_plugin_languages_for_buffers, plugin_language_reclassified_buffer_ids,
    },
    project_search::{
        effective_project_search_exclude_globs, effective_project_search_max_file_bytes,
        effective_project_search_max_results,
    },
    project_search_state::{parse_project_globs, project_search_request_is_current},
    source_control_runtime::source_control_git_operation_root_for_snapshot,
    startup_tasks::GitScanRootCacheEntry,
    syntax::PluginSyntaxLoad,
    workspace_state::{background_workspace_event_matches, workspace_event_matches},
};
use kuroya_core::{
    BufferId, Diagnostic, GitSnapshot, PluginActivationState, PluginCommandRegistry,
    PluginDescriptor, PluginDiscoveryError, PluginLanguageRegistry, PluginRuntimeRegistry,
    PluginThemeRegistry, ProjectIndex, SearchProgress, SearchResult, SearchStats, TextBuffer,
};
use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

pub(super) fn handle_cached_index_event(
    app: &mut KuroyaApp,
    request_id: u64,
    root: PathBuf,
    index: ProjectIndex,
) -> bool {
    if !background_workspace_event_matches(
        &app.workspace.root,
        &root,
        request_id,
        app.workspace_index_active_request_id,
    ) {
        return false;
    }
    let count = index.files().len();
    let truncated = index.truncated();
    app.index = index;
    app.project_index_generation = app.project_index_generation.saturating_add(1);
    app.project_search_index_generation = app.project_search_index_generation.saturating_add(1);
    app.project_search_metadata_cache.clear();
    app.status = workspace_cached_index_status(count, truncated);
    true
}

pub(super) fn handle_indexed_event(
    app: &mut KuroyaApp,
    request_id: u64,
    root: PathBuf,
    index: ProjectIndex,
) -> bool {
    if !background_workspace_event_matches(
        &app.workspace.root,
        &root,
        request_id,
        app.workspace_index_active_request_id,
    ) {
        return false;
    }
    let count = index.files().len();
    let truncated = index.truncated();
    app.index = index;
    app.project_index_generation = app.project_index_generation.saturating_add(1);
    app.project_search_index_generation = app.project_search_index_generation.saturating_add(1);
    app.project_search_metadata_cache.clear();
    app.status = workspace_index_status(count, truncated);
    true
}

pub(crate) fn workspace_index_status(count: usize, truncated: bool) -> String {
    if truncated {
        format!("Indexed {count} files (limit reached)")
    } else {
        format!("Indexed {count} files")
    }
}

pub(crate) fn workspace_cached_index_status(count: usize, truncated: bool) -> String {
    let _ = count;
    if truncated {
        "Refreshing index (limit reached)".to_owned()
    } else {
        "Refreshing index".to_owned()
    }
}

pub(super) fn handle_search_finished_event(
    app: &mut KuroyaApp,
    request_id: u64,
    index_generation: u64,
    workspace_root: PathBuf,
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    include_globs: Vec<String>,
    exclude_globs: Vec<String>,
    max_file_bytes: u64,
    max_results: usize,
    result: SearchResult,
) -> bool {
    if !search_event_is_current(
        app,
        request_id,
        index_generation,
        &workspace_root,
        &query,
        case_sensitive,
        whole_word,
        &include_globs,
        &exclude_globs,
        max_file_bytes,
        max_results,
    ) {
        return false;
    }
    let count = result.matches.len();
    let error = result.error.clone();
    let stats = result.stats;
    app.project_search_result = result;
    apply_project_search_result_metadata(
        app,
        query,
        index_generation,
        case_sensitive,
        whole_word,
        include_globs,
        exclude_globs,
        max_file_bytes,
        max_results,
    );
    app.project_search_selected = 0;
    if let Some(error) = error {
        app.status = display_error_label_cow(&error).into_owned();
    } else {
        app.status = project_search_status(count, app.project_search_result.truncated, stats);
    }
    true
}

pub(super) fn handle_search_progress_event(
    app: &mut KuroyaApp,
    request_id: u64,
    index_generation: u64,
    workspace_root: PathBuf,
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    include_globs: Vec<String>,
    exclude_globs: Vec<String>,
    max_file_bytes: u64,
    max_results: usize,
    progress: SearchProgress,
) -> bool {
    if !search_event_is_current(
        app,
        request_id,
        index_generation,
        &workspace_root,
        &query,
        case_sensitive,
        whole_word,
        &include_globs,
        &exclude_globs,
        max_file_bytes,
        max_results,
    ) {
        return false;
    }

    if !project_search_result_metadata_matches(
        app,
        index_generation,
        &query,
        case_sensitive,
        whole_word,
        &include_globs,
        &exclude_globs,
        max_file_bytes,
        max_results,
    ) {
        app.project_search_result = SearchResult::default();
        apply_project_search_result_metadata(
            app,
            query.clone(),
            index_generation,
            case_sensitive,
            whole_word,
            include_globs.clone(),
            exclude_globs.clone(),
            max_file_bytes,
            max_results,
        );
        app.project_search_selected = 0;
    }

    app.project_search_result.matches.extend(progress.matches);
    app.project_search_result.truncated |= progress.truncated;
    app.project_search_result.stats.merge(progress.stats);
    app.project_search_result.error = None;
    let count = app.project_search_result.matches.len();
    let stats = app.project_search_result.stats;
    app.status = project_search_progress_status(count, app.project_search_result.truncated, stats);
    true
}

fn search_event_is_current(
    app: &KuroyaApp,
    request_id: u64,
    index_generation: u64,
    workspace_root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    include_globs: &[String],
    exclude_globs: &[String],
    max_file_bytes: u64,
    max_results: usize,
) -> bool {
    let panel_exclude_globs = parse_project_globs(&app.project_search_exclude);
    let current_exclude_globs =
        effective_project_search_exclude_globs(&app.settings, &panel_exclude_globs);
    workspace_event_matches(&app.workspace.root, workspace_root)
        && project_search_request_is_current(
            request_id,
            app.project_search_active_request_id,
            index_generation,
            app.project_search_index_generation,
            query,
            case_sensitive,
            whole_word,
            include_globs,
            exclude_globs,
            max_file_bytes,
            max_results,
            app.project_search_query.trim(),
            app.project_search_case_sensitive,
            app.project_search_whole_word,
            &parse_project_globs(&app.project_search_include),
            &current_exclude_globs,
            effective_project_search_max_file_bytes(&app.settings),
            effective_project_search_max_results(&app.settings),
        )
}

fn apply_project_search_result_metadata(
    app: &mut KuroyaApp,
    query: String,
    index_generation: u64,
    case_sensitive: bool,
    whole_word: bool,
    include_globs: Vec<String>,
    exclude_globs: Vec<String>,
    max_file_bytes: u64,
    max_results: usize,
) {
    app.project_search_result_query = query;
    app.project_search_result_index_generation = index_generation;
    app.project_search_result_case_sensitive = case_sensitive;
    app.project_search_result_whole_word = whole_word;
    app.project_search_result_include_globs = include_globs;
    app.project_search_result_exclude_globs = exclude_globs;
    app.project_search_result_max_file_bytes = max_file_bytes;
    app.project_search_result_max_results = max_results;
}

fn project_search_result_metadata_matches(
    app: &KuroyaApp,
    index_generation: u64,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    include_globs: &[String],
    exclude_globs: &[String],
    max_file_bytes: u64,
    max_results: usize,
) -> bool {
    app.project_search_result_index_generation == index_generation
        && app.project_search_result_query == query
        && app.project_search_result_case_sensitive == case_sensitive
        && app.project_search_result_whole_word == whole_word
        && app.project_search_result_include_globs == include_globs
        && app.project_search_result_exclude_globs == exclude_globs
        && app.project_search_result_max_file_bytes == max_file_bytes
        && app.project_search_result_max_results == max_results
}

pub(crate) fn project_search_status(count: usize, truncated: bool, stats: SearchStats) -> String {
    let mut status = match count {
        0 => "No project matches".to_owned(),
        1 if stats.matched_files == 1 => "1 project match in 1 file".to_owned(),
        1 => "1 project match".to_owned(),
        count if stats.matched_files == 1 => format!("{count} project matches in 1 file"),
        count if stats.matched_files > 1 => {
            format!("{count} project matches in {} files", stats.matched_files)
        }
        count => format!("{count} project matches"),
    };
    let skipped = stats.skipped_files();
    if !truncated && skipped == 0 {
        return status;
    }

    status.push_str(" (");
    if truncated {
        status.push_str("results truncated");
    }
    if skipped > 0 {
        if truncated {
            status.push_str("; ");
        }
        append_project_search_skipped_summary(&mut status, stats, skipped);
    }
    status.push(')');
    status
}

pub(crate) fn project_search_progress_status(
    count: usize,
    truncated: bool,
    stats: SearchStats,
) -> String {
    let mut status = match count {
        0 => "Searching project...".to_owned(),
        1 if stats.matched_files == 1 => "Searching project: 1 match in 1 file".to_owned(),
        1 => "Searching project: 1 match".to_owned(),
        count if stats.matched_files == 1 => {
            format!("Searching project: {count} matches in 1 file")
        }
        count if stats.matched_files > 1 => {
            format!(
                "Searching project: {count} matches in {} files",
                stats.matched_files
            )
        }
        count => format!("Searching project: {count} matches"),
    };
    if stats.searched_files > 0 {
        let _ = write!(status, ", {} files searched", stats.searched_files);
    }
    if truncated {
        status.push_str(", truncated");
    }
    status
}

fn append_project_search_skipped_summary(status: &mut String, stats: SearchStats, skipped: usize) {
    let files = if skipped == 1 { "file" } else { "files" };
    write!(status, "skipped {skipped} {files}").expect("writing to a String cannot fail");

    let mut wrote_reason = false;
    append_project_search_skip_count(
        status,
        &mut wrote_reason,
        stats.skipped_large_files,
        "large",
    );
    append_project_search_skip_count(
        status,
        &mut wrote_reason,
        stats.skipped_binary_files,
        "binary",
    );
    append_project_search_skip_count(
        status,
        &mut wrote_reason,
        stats.skipped_unreadable_files,
        "unreadable",
    );
}

fn append_project_search_skip_count(
    status: &mut String,
    wrote_reason: &mut bool,
    count: usize,
    reason: &str,
) {
    if count > 0 {
        if *wrote_reason {
            status.push_str(", ");
        } else {
            status.push_str(": ");
        }
        write!(status, "{count} {reason}").expect("writing to a String cannot fail");
        *wrote_reason = true;
    }
}

pub(super) fn handle_git_scanned_event(
    app: &mut KuroyaApp,
    request_id: u64,
    root: PathBuf,
    scan_root: Option<PathBuf>,
    root_cache_entry: Option<GitScanRootCacheEntry>,
    git: GitSnapshot,
) -> bool {
    if !background_workspace_event_matches(
        &app.workspace.root,
        &root,
        request_id,
        app.git_scan_active_request_id,
    ) {
        return false;
    }
    let previous_operation_root = app.source_control_git_operation_root();
    if !app.settings.git_enabled {
        app.invalidate_source_control_load_requests();
        app.git = GitSnapshot::default();
        app.clear_pending_restored_source_control_loads();
        return false;
    }
    app.git_scan_root_cache = root_cache_entry;
    if scan_root.is_none() {
        let next_operation_root =
            source_control_git_operation_root_for_snapshot(&app.workspace.root, None);
        if previous_operation_root != next_operation_root {
            app.invalidate_source_control_load_requests();
        }
        app.git = GitSnapshot::default();
        app.source_control_selected = 0;
        app.clear_pending_restored_source_control_loads();
        return true;
    }
    let count = git.len();
    let limit_warning = git.status_limited() && !app.settings.git_ignore_limit_warning;
    let next_operation_root =
        source_control_git_operation_root_for_snapshot(&app.workspace.root, git.root());
    if previous_operation_root != next_operation_root {
        app.invalidate_source_control_load_requests();
    }

    let previous_git_revision = app.git.revision();
    app.git = git;
    app.git.advance_revision_past(previous_git_revision);
    app.drain_pending_restored_source_control_loads();
    if limit_warning {
        app.status = git_status_limit_warning(app.settings.git_status_limit, count);
    } else if count > 0 {
        app.status = format!("{count} git changes");
    }
    true
}

pub(crate) fn git_status_limit_warning(limit: usize, count: usize) -> String {
    format!("Git changes hit status limit {limit}; showing {count} changes")
}

pub(super) fn handle_workspace_plugins_loaded_event(
    app: &mut KuroyaApp,
    request_id: u64,
    root: PathBuf,
    plugins: Vec<PluginDescriptor>,
    errors: Vec<PluginDiscoveryError>,
    syntax_load: PluginSyntaxLoad,
) -> bool {
    if !background_workspace_event_matches(
        &app.workspace.root,
        &root,
        request_id,
        app.workspace_plugins_active_request_id,
    ) {
        return false;
    }
    if !app.workspace_trusted {
        app.clear_workspace_plugins();
        return false;
    }
    let plugin_commands = PluginCommandRegistry::from_plugins(&plugins);
    let plugin_languages = PluginLanguageRegistry::from_plugins(&plugins);
    let plugin_themes = PluginThemeRegistry::from_plugins(&plugins);
    let plugin_runtimes = PluginRuntimeRegistry::from_plugins(&plugins);
    let mut plugin_activations = PluginActivationState::default();
    let startup_activations = plugin_activations.activate_startup(&plugin_runtimes).len();
    let language_activations = activate_plugin_languages_for_buffers(
        &mut plugin_activations,
        &plugin_runtimes,
        &plugin_languages,
        &app.buffers,
        &app.lossy_decoded_buffers,
        &app.binary_preview_buffers,
    )
    .len();
    let reclassified_buffer_ids = plugin_language_reclassified_buffer_ids(
        &app.plugin_languages,
        &plugin_languages,
        &app.buffers,
        &app.lossy_decoded_buffers,
        &app.binary_preview_buffers,
    );
    let status_counts = WorkspacePluginStatusCounts {
        plugins: plugins.len(),
        errors: errors.len(),
        commands: plugin_commands.len(),
        language_extensions: plugin_languages.len(),
        themes: plugin_themes.len(),
        syntax_definitions: syntax_load.registry.len(),
        startup_activations,
        language_activations,
    };
    app.plugin_syntaxes = syntax_load.registry.clone();
    app.highlighter.install_plugin_syntaxes(syntax_load);
    app.plugin_runtimes = plugin_runtimes;
    app.plugin_activations = plugin_activations;
    app.plugin_commands = plugin_commands;
    app.plugin_languages = plugin_languages;
    for id in reclassified_buffer_ids {
        app.syntax_tree_cache.clear_for_buffer(id);
        app.schedule_language_sync(id);
    }
    app.plugin_themes = plugin_themes;
    app.plugins = plugins;
    app.plugin_errors = errors;
    app.theme_picker_selected =
        crate::theme::selected_theme_index_with_plugins(&app.settings.theme, &app.plugin_themes);
    if status_counts.plugins > 0 || status_counts.errors > 0 {
        app.status = workspace_plugins_status(status_counts);
    }
    true
}

pub(super) fn handle_workspace_plugins_failed_event(
    app: &mut KuroyaApp,
    request_id: u64,
    root: PathBuf,
    error: String,
) -> bool {
    if !background_workspace_event_matches(
        &app.workspace.root,
        &root,
        request_id,
        app.workspace_plugins_active_request_id,
    ) {
        return false;
    }
    if !app.workspace_trusted {
        app.clear_workspace_plugins();
        return false;
    }
    app.clear_workspace_plugins();
    let error = display_error_label_cow(&error);
    app.status = format!("Could not load workspace plugins: {}", error.as_ref());
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WorkspacePluginStatusCounts {
    pub(crate) plugins: usize,
    pub(crate) errors: usize,
    pub(crate) commands: usize,
    pub(crate) language_extensions: usize,
    pub(crate) themes: usize,
    pub(crate) syntax_definitions: usize,
    pub(crate) startup_activations: usize,
    pub(crate) language_activations: usize,
}

pub(crate) fn workspace_plugins_status(counts: WorkspacePluginStatusCounts) -> String {
    let mut status = match (counts.plugins, counts.errors) {
        (1, 0) => "Loaded 1 workspace plugin".to_owned(),
        (count, 0) => format!("Loaded {count} workspace plugins"),
        (0, 1) => "Could not load 1 workspace plugin".to_owned(),
        (0, count) => format!("Could not load {count} workspace plugins"),
        (plugins, 1) => format!("Loaded {plugins} workspace plugins; 1 plugin failed"),
        (plugins, errors) => {
            format!("Loaded {plugins} workspace plugins; {errors} plugins failed")
        }
    };
    if workspace_plugin_status_has_contributions(counts) {
        status.push_str(" (");
        append_workspace_plugin_contribution_summary(&mut status, counts);
        status.push(')');
    }
    status
}

fn workspace_plugin_status_has_contributions(counts: WorkspacePluginStatusCounts) -> bool {
    counts.commands > 0
        || counts.language_extensions > 0
        || counts.themes > 0
        || counts.syntax_definitions > 0
        || counts.startup_activations > 0
        || counts.language_activations > 0
}

fn append_workspace_plugin_contribution_summary(
    status: &mut String,
    counts: WorkspacePluginStatusCounts,
) {
    let mut wrote_count = false;
    append_plugin_status_count(
        status,
        &mut wrote_count,
        counts.commands,
        "command",
        "commands",
    );
    append_plugin_status_count(
        status,
        &mut wrote_count,
        counts.language_extensions,
        "language extension",
        "language extensions",
    );
    append_plugin_status_count(status, &mut wrote_count, counts.themes, "theme", "themes");
    append_plugin_status_count(
        status,
        &mut wrote_count,
        counts.syntax_definitions,
        "syntax definition",
        "syntax definitions",
    );
    append_plugin_status_count(
        status,
        &mut wrote_count,
        counts.startup_activations,
        "startup activation",
        "startup activations",
    );
    append_plugin_status_count(
        status,
        &mut wrote_count,
        counts.language_activations,
        "language activation",
        "language activations",
    );
}

fn append_plugin_status_count(
    status: &mut String,
    wrote_count: &mut bool,
    count: usize,
    singular: &str,
    plural: &str,
) {
    if count == 0 {
        return;
    }
    if *wrote_count {
        status.push_str(", ");
    }
    let label = if count == 1 { singular } else { plural };
    write!(status, "{count} {label}").expect("writing to a String cannot fail");
    *wrote_count = true;
}

pub(super) fn handle_diagnostics_computed_event(
    app: &mut KuroyaApp,
    request_id: u64,
    id: BufferId,
    path: PathBuf,
    version: u64,
    diagnostics: Vec<Diagnostic>,
) {
    if static_diagnostics_event_matches(
        app.buffer(id),
        &path,
        version,
        request_id,
        app.static_diagnostics_active_request_ids.get(&id).copied(),
    ) {
        app.diagnostics.replace_static(path, diagnostics);
    }
}

pub(crate) fn static_diagnostics_event_matches(
    buffer: Option<&TextBuffer>,
    path: &Path,
    version: u64,
    request_id: u64,
    active_request_id: Option<u64>,
) -> bool {
    if active_request_id != Some(request_id) {
        return false;
    }
    buffer.is_some_and(|buffer| {
        buffer.version() == version && static_diagnostics_path_matches(buffer, path)
    })
}

fn static_diagnostics_path_matches(buffer: &TextBuffer, path: &Path) -> bool {
    if let Some(buffer_path) = buffer.path() {
        buffer_path == path
    } else {
        path == Path::new(&format!("<untitled-{}>", buffer.id()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KuroyaApp, WorkspacePluginStatusCounts, git_status_limit_warning,
        handle_cached_index_event, handle_indexed_event, handle_search_finished_event,
        handle_workspace_plugins_failed_event, project_search_status,
        static_diagnostics_event_matches, workspace_cached_index_status, workspace_index_status,
        workspace_plugins_status,
    };
    use crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS;
    use crate::{
        app_startup_context::AppStartupContext, terminal::TerminalPane,
        ui_event_channel::ui_event_channel,
    };
    use kuroya_core::{
        EditorSettings, ProjectIndex, SearchMatch, SearchOptions, SearchResult, SearchStats,
        TextBuffer, Workspace, merged_exclude_globs,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn workspace_index_status_reports_truncated_index() {
        assert_eq!(workspace_index_status(2, false), "Indexed 2 files");
        assert_eq!(
            workspace_index_status(40_000, true),
            "Indexed 40000 files (limit reached)"
        );
        assert_eq!(workspace_cached_index_status(2, false), "Refreshing index");
        assert_eq!(
            workspace_cached_index_status(40_000, true),
            "Refreshing index (limit reached)"
        );
    }

    #[test]
    fn cached_index_event_applies_preview_for_current_request() {
        let root = temp_root("cached-index-current");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut app = app_for_test(root.clone());
        app.workspace_index_active_request_id = 7;
        let index = ProjectIndex::rebuild(&root, 40_000);

        assert!(handle_cached_index_event(
            &mut app,
            7,
            root.clone(),
            index.clone()
        ));

        assert_eq!(app.index.files(), index.files());
        assert_eq!(app.project_index_generation, 1);
        assert_eq!(app.project_search_index_generation, 1);
        assert_eq!(app.status, workspace_cached_index_status(1, false));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cached_index_event_invalidates_active_search_generation() {
        let root = temp_root("cached-index-invalidates-search");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut app = app_for_test(root.clone());
        app.workspace_index_active_request_id = 7;
        app.project_search_active_request_id = 9;
        app.project_search_query = "needle".to_owned();
        let search_generation = app.project_search_index_generation;

        assert!(handle_cached_index_event(
            &mut app,
            7,
            root.clone(),
            ProjectIndex::rebuild(&root, 40_000),
        ));

        assert!(!handle_search_finished_event(
            &mut app,
            9,
            search_generation,
            root.clone(),
            "needle".to_owned(),
            false,
            false,
            Vec::new(),
            Vec::new(),
            default_search_max_file_bytes(),
            default_search_max_results(),
            search_result_with_one_match(root.join("src/main.rs")),
        ));
        assert!(app.project_search_result.matches.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cached_index_event_ignores_stale_request() {
        let root = temp_root("cached-index-stale");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut app = app_for_test(root.clone());
        app.workspace_index_active_request_id = 7;
        let index = ProjectIndex::rebuild(&root, 40_000);

        assert!(!handle_cached_index_event(&mut app, 6, root.clone(), index));

        assert!(app.index.files().is_empty());
        assert_eq!(app.project_index_generation, 0);
        assert_eq!(app.project_search_index_generation, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn indexed_events_advance_search_index_generation_only_when_current() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.workspace_index_active_request_id = 7;

        assert!(handle_indexed_event(
            &mut app,
            7,
            root.clone(),
            ProjectIndex::default(),
        ));
        assert_eq!(app.project_search_index_generation, 1);

        assert!(!handle_indexed_event(
            &mut app,
            6,
            root,
            ProjectIndex::default(),
        ));
        assert_eq!(app.project_search_index_generation, 1);
    }

    #[test]
    fn search_finished_events_reject_stale_index_generations() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.project_search_index_generation = 2;
        app.project_search_active_request_id = 9;
        app.project_search_query = "needle".to_owned();

        assert!(!handle_search_finished_event(
            &mut app,
            9,
            1,
            root.clone(),
            "needle".to_owned(),
            false,
            false,
            Vec::new(),
            default_search_exclude_globs(),
            default_search_max_file_bytes(),
            default_search_max_results(),
            search_result_with_one_match(root.join("src/main.rs")),
        ));
        assert!(app.project_search_result.matches.is_empty());

        assert!(handle_search_finished_event(
            &mut app,
            9,
            2,
            root.clone(),
            "needle".to_owned(),
            false,
            false,
            Vec::new(),
            default_search_exclude_globs(),
            default_search_max_file_bytes(),
            default_search_max_results(),
            search_result_with_one_match(root.join("src/main.rs")),
        ));
        assert_eq!(app.project_search_result.matches.len(), 1);
        assert_eq!(app.project_search_result_index_generation, 2);
    }

    #[test]
    fn stale_same_root_search_finished_event_after_reset_is_ignored() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.project_search_next_request_id = 1;
        app.project_search_active_request_id = 1;
        app.project_search_query = "needle".to_owned();

        app.reset_open_workspace_state();
        app.project_search_query = "needle".to_owned();

        assert_eq!(app.project_search_next_request_id, 2);
        assert_eq!(app.project_search_active_request_id, 2);
        assert!(!handle_search_finished_event(
            &mut app,
            1,
            0,
            root.clone(),
            "needle".to_owned(),
            false,
            false,
            Vec::new(),
            Vec::new(),
            default_search_max_file_bytes(),
            default_search_max_results(),
            search_result_with_one_match(root.join("src/main.rs")),
        ));
        assert!(app.project_search_result.matches.is_empty());
    }

    #[test]
    fn search_finished_event_sanitizes_status_error_without_mutating_result_error() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.project_search_index_generation = 2;
        app.project_search_active_request_id = 9;
        app.project_search_query = "needle".to_owned();

        let raw_error = format!(
            "first line\nsecond line \u{202e}{}",
            "x".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
        );
        let result = SearchResult {
            error: Some(raw_error.clone()),
            ..SearchResult::default()
        };

        assert!(handle_search_finished_event(
            &mut app,
            9,
            2,
            root,
            "needle".to_owned(),
            false,
            false,
            Vec::new(),
            default_search_exclude_globs(),
            default_search_max_file_bytes(),
            default_search_max_results(),
            result,
        ));

        assert_eq!(
            app.project_search_result.error.as_deref(),
            Some(raw_error.as_str())
        );
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(app.status.contains("..."));
        assert!(app.status.chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);
    }

    #[test]
    fn git_status_limit_warning_names_limit_and_visible_count() {
        assert_eq!(
            git_status_limit_warning(10000, 10000),
            "Git changes hit status limit 10000; showing 10000 changes"
        );
    }

    #[test]
    fn workspace_plugin_status_summarizes_loaded_and_failed_counts() {
        assert_eq!(
            workspace_plugins_status(WorkspacePluginStatusCounts {
                plugins: 1,
                ..WorkspacePluginStatusCounts::default()
            }),
            "Loaded 1 workspace plugin"
        );
        assert_eq!(
            workspace_plugins_status(WorkspacePluginStatusCounts {
                plugins: 2,
                commands: 3,
                language_extensions: 1,
                themes: 1,
                syntax_definitions: 2,
                startup_activations: 1,
                language_activations: 2,
                ..WorkspacePluginStatusCounts::default()
            }),
            "Loaded 2 workspace plugins (3 commands, 1 language extension, 1 theme, 2 syntax definitions, 1 startup activation, 2 language activations)"
        );
        assert_eq!(
            workspace_plugins_status(WorkspacePluginStatusCounts {
                errors: 1,
                ..WorkspacePluginStatusCounts::default()
            }),
            "Could not load 1 workspace plugin"
        );
        assert_eq!(
            workspace_plugins_status(WorkspacePluginStatusCounts {
                plugins: 2,
                errors: 3,
                commands: 1,
                ..WorkspacePluginStatusCounts::default()
            }),
            "Loaded 2 workspace plugins; 3 plugins failed (1 command)"
        );
        assert_eq!(
            workspace_plugins_status(WorkspacePluginStatusCounts {
                plugins: 1,
                commands: 1,
                themes: 2,
                language_activations: 1,
                ..WorkspacePluginStatusCounts::default()
            }),
            "Loaded 1 workspace plugin (1 command, 2 themes, 1 language activation)"
        );
        assert_eq!(
            workspace_plugins_status(WorkspacePluginStatusCounts {
                plugins: 2,
                errors: 3,
                ..WorkspacePluginStatusCounts::default()
            }),
            "Loaded 2 workspace plugins; 3 plugins failed"
        );
    }

    #[test]
    fn workspace_plugins_failed_event_sanitizes_status_error() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.workspace_plugins_active_request_id = 12;

        let raw_error = format!(
            "discovery failed\nbecause \u{202e}{}",
            "x".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
        );

        assert!(handle_workspace_plugins_failed_event(
            &mut app, 12, root, raw_error,
        ));

        let prefix = "Could not load workspace plugins: ";
        assert!(app.status.starts_with(prefix));
        assert!(!app.status.contains('\n'));
        assert!(!app.status.contains('\u{202e}'));
        assert!(app.status.contains("..."));
        assert!(app.status[prefix.len()..].chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);
    }

    #[test]
    fn project_search_status_reports_counts_and_truncation() {
        assert_eq!(
            project_search_status(0, false, SearchStats::default()),
            "No project matches"
        );
        assert_eq!(
            project_search_status(1, false, SearchStats::default()),
            "1 project match"
        );
        assert_eq!(
            project_search_status(7, false, SearchStats::default()),
            "7 project matches"
        );
        assert_eq!(
            project_search_status(
                7,
                false,
                SearchStats {
                    matched_files: 3,
                    ..SearchStats::default()
                }
            ),
            "7 project matches in 3 files"
        );
        assert_eq!(
            project_search_status(100, true, SearchStats::default()),
            "100 project matches (results truncated)"
        );
        assert_eq!(
            project_search_status(
                2,
                true,
                SearchStats {
                    skipped_large_files: 2,
                    skipped_binary_files: 1,
                    skipped_unreadable_files: 1,
                    ..SearchStats::default()
                }
            ),
            "2 project matches (results truncated; skipped 4 files: 2 large, 1 binary, 1 unreadable)"
        );
    }

    #[test]
    fn project_search_status_reports_skipped_reasons_in_order() {
        assert_eq!(
            project_search_status(
                4,
                false,
                SearchStats {
                    skipped_unreadable_files: 3,
                    skipped_binary_files: 2,
                    skipped_large_files: 1,
                    ..SearchStats::default()
                }
            ),
            "4 project matches (skipped 6 files: 1 large, 2 binary, 3 unreadable)"
        );
    }

    #[test]
    fn static_diagnostics_events_match_current_path_and_version() {
        let path = PathBuf::from("workspace/src/main.rs");
        let mut buffer = TextBuffer::from_text(7, Some(path.clone()), "fn main() {}".to_owned());
        let version = buffer.version();

        assert!(static_diagnostics_event_matches(
            Some(&buffer),
            &path,
            version,
            3,
            Some(3),
        ));
        assert!(!static_diagnostics_event_matches(
            Some(&buffer),
            Path::new("workspace/src/old.rs"),
            version,
            3,
            Some(3),
        ));
        assert!(!static_diagnostics_event_matches(
            Some(&buffer),
            &path,
            version + 1,
            3,
            Some(3),
        ));
        assert!(!static_diagnostics_event_matches(
            Some(&buffer),
            &path,
            version,
            3,
            Some(4),
        ));

        buffer.set_path(PathBuf::from("workspace/src/renamed.rs"));
        assert!(!static_diagnostics_event_matches(
            Some(&buffer),
            &path,
            version,
            3,
            Some(3),
        ));
        assert!(!static_diagnostics_event_matches(
            None,
            &path,
            version,
            3,
            Some(3),
        ));
    }

    #[test]
    fn static_diagnostics_events_match_untitled_buffers() {
        let buffer = TextBuffer::from_text(8, None, "scratch".to_owned());

        assert!(static_diagnostics_event_matches(
            Some(&buffer),
            Path::new("<untitled-8>"),
            buffer.version(),
            5,
            Some(5),
        ));
        assert!(!static_diagnostics_event_matches(
            Some(&buffer),
            Path::new("workspace/src/main.rs"),
            buffer.version(),
            5,
            Some(5),
        ));
    }

    fn search_result_with_one_match(path: PathBuf) -> SearchResult {
        SearchResult {
            matches: vec![SearchMatch {
                path,
                line: 1,
                column: 1,
                preview: "needle".to_owned(),
            }],
            stats: SearchStats {
                searched_files: 1,
                matched_files: 1,
                ..SearchStats::default()
            },
            ..SearchResult::default()
        }
    }

    fn default_search_max_file_bytes() -> u64 {
        SearchOptions::default().max_file_bytes
    }

    fn default_search_exclude_globs() -> Vec<String> {
        merged_exclude_globs(&[])
    }

    fn default_search_max_results() -> usize {
        SearchOptions::default().max_results
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = ui_event_channel();
        let mut settings = EditorSettings::default();
        settings.project_search_exclude_globs.clear();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-ui-event-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}

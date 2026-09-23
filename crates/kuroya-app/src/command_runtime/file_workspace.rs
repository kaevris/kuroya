use crate::{
    KuroyaApp,
    devtools_async_tasks::plugin_command_task_detail,
    path_display::{display_error_label_cow, sanitized_display_label_cow},
    plugin_command_runtime::{
        ActiveBufferSnapshot, PluginCommandExecution, PluginHostContext, execute_plugin_command,
    },
    ui_events::UiEvent,
};
use kuroya_core::{
    Command, PluginActivationRecord, PluginActivationState, PluginCommandRegistry,
    PluginRuntimeRegistration, PluginRuntimeRegistry, TextBuffer,
};
use std::borrow::Cow;
use std::path::PathBuf;

const PLUGIN_COMMAND_STATUS_FRAGMENT_MAX_CHARS: usize = 96;
#[cfg(test)]
const PLUGIN_COMMAND_STATUS_ENTRY_MAX_CHARS: usize = 120;
const PLUGIN_COMMAND_STATUS_ERROR_MAX_CHARS: usize = 160;

pub(super) fn run_file_workspace_command(app: &mut KuroyaApp, command: Command) -> Option<Command> {
    match command {
        Command::NewFile => {
            let id = app.next_id();
            let mut buffer = TextBuffer::new_untitled(id);
            buffer.set_word_separators(app.settings.word_separators.clone());
            app.buffers.push(buffer);
            app.spawn_diagnostics_for(id);
            app.set_active_buffer(id);
            None
        }
        Command::OpenFile(path) => {
            app.spawn_open_file(path);
            None
        }
        Command::OpenFileAt { path, line, column } => {
            app.open_file_at(path, line, column);
            None
        }
        Command::SelectFileForCompare(path) => {
            app.select_file_for_compare(path);
            None
        }
        Command::CompareFileWithSelected(path) => {
            app.compare_file_with_selected(path);
            None
        }
        Command::RevealFileInExplorer(path) => {
            app.reveal_file_in_explorer(path);
            None
        }
        Command::RevealFileInSourceControl(path) => {
            app.reveal_file_in_source_control(path);
            None
        }
        Command::OpenFileChanges(path) => {
            app.open_file_changes(path);
            None
        }
        Command::OpenStagedFileChanges(path) => {
            app.open_staged_file_changes(path);
            None
        }
        Command::OpenFileHeadChanges(path) => {
            app.open_file_head_changes(path);
            None
        }
        Command::OpenFileHeadRevision(path) => {
            app.open_file_head_revision(path);
            None
        }
        Command::OpenFileIndexRevision(path) => {
            app.open_file_index_revision(path);
            None
        }
        Command::OpenAllChanges => {
            app.open_all_file_changes();
            None
        }
        Command::OpenAllUnstagedChanges => {
            app.open_all_unstaged_file_changes();
            None
        }
        Command::OpenAllStagedChanges => {
            app.open_all_staged_file_changes();
            None
        }
        Command::OpenFileHunks(path) => {
            app.begin_source_control_hunks(path);
            None
        }
        Command::OpenStagedFileHunks(path) => {
            app.begin_source_control_staged_hunks(path);
            None
        }
        Command::OpenFileBlame(path) => {
            app.open_file_blame(path);
            None
        }
        Command::StageFileChange(path) => {
            app.stage_file_change(path);
            None
        }
        Command::StageAllChanges => {
            app.stage_all_changes();
            None
        }
        Command::UnstageFileChange(path) => {
            app.unstage_file_change(path);
            None
        }
        Command::UnstageAllChanges => {
            app.unstage_all_changes();
            None
        }
        Command::DiscardFileChanges(path) => {
            app.begin_discard_file_changes(path);
            None
        }
        Command::DiscardAllChanges => {
            app.begin_discard_all_changes();
            None
        }
        Command::StageFileHunk {
            path,
            hunk_index,
            hunk_fingerprint,
        } => {
            if let Some(hunk_fingerprint) = hunk_fingerprint {
                app.stage_source_control_hunk(path, hunk_index, hunk_fingerprint);
            } else {
                app.reject_stale_source_control_hunk_stage(path, hunk_index);
            }
            None
        }
        Command::UnstageFileHunk {
            path,
            hunk_index,
            hunk_fingerprint,
        } => {
            if let Some(hunk_fingerprint) = hunk_fingerprint {
                app.unstage_source_control_hunk(path, hunk_index, hunk_fingerprint);
            } else {
                app.reject_stale_source_control_hunk_unstage(path, hunk_index);
            }
            None
        }
        Command::DiscardFileHunk {
            path,
            hunk_index,
            hunk_fingerprint,
        } => {
            if let Some(hunk_fingerprint) = hunk_fingerprint {
                app.discard_source_control_hunk(path, hunk_index, hunk_fingerprint);
            } else {
                app.reject_stale_source_control_hunk_discard(path, hunk_index);
            }
            None
        }
        Command::CommitStagedChanges => {
            app.commit_staged_changes();
            None
        }
        Command::SaveGitStash => {
            app.save_git_stash_from_input();
            None
        }
        Command::ApplyGitStash(index) => {
            app.apply_git_stash(index);
            None
        }
        Command::PopGitStash(index) => {
            app.pop_git_stash(index);
            None
        }
        Command::DropGitStash(index) => {
            app.drop_git_stash(index);
            None
        }
        Command::OpenWorkspace(path) => {
            app.request_open_workspace(path);
            None
        }
        Command::OpenWorkspacePrompt => {
            app.begin_open_workspace();
            None
        }
        Command::CreateFileIn(parent) => {
            app.begin_create_file(parent);
            None
        }
        Command::CreateFolderIn(parent) => {
            app.begin_create_folder(parent);
            None
        }
        Command::RenamePath(path) => {
            app.begin_rename_path(path);
            None
        }
        Command::DeletePath(path) => {
            app.begin_delete_path(path);
            None
        }
        Command::RefreshWorkspace => {
            app.spawn_index();
            app.spawn_git_scan();
            app.spawn_workspace_task_load();
            app.spawn_plugin_discovery();
            app.status = "Refreshing workspace".to_owned();
            None
        }
        Command::RunPluginCommand {
            plugin_id,
            command_id,
        } => {
            app.run_plugin_command(plugin_id, command_id);
            None
        }
        Command::SaveActive => {
            if let Some(id) = app.active {
                app.spawn_save(id);
            }
            None
        }
        Command::SaveAs => {
            if let Some(id) = app.active {
                app.begin_save_as(id);
            }
            None
        }
        Command::SaveAll => {
            app.save_all_dirty_buffers();
            None
        }
        Command::ReloadActiveFromDisk => {
            if let Some(id) = app.active {
                app.begin_reload_buffer_from_disk(id);
            } else {
                app.status = "No active file to reload".to_owned();
            }
            None
        }
        command => Some(command),
    }
}

impl KuroyaApp {
    pub(crate) fn run_plugin_command(&mut self, plugin_id: String, command_id: String) {
        let prepared = prepare_plugin_command_run(
            &self.plugin_commands,
            &self.plugin_runtimes,
            &mut self.plugin_activations,
            &plugin_id,
            &command_id,
        );
        self.status = prepared.status;
        let Some(runtime) = prepared.runtime else {
            return;
        };

        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        let tx = self.tx.clone();
        // Capture the active buffer's path and full text on the UI thread
        // before the blocking run starts, so guest reads see a consistent
        // snapshot and staged writes can be attributed after the run.
        let active_buffer = self
            .active
            .and_then(|id| self.buffer(id))
            .and_then(ActiveBufferSnapshot::capture);
        let host = PluginHostContext {
            plugin_id: runtime.plugin_id.clone(),
            workspace_root: root.clone(),
            active_buffer_path: active_buffer.as_ref().map(|snapshot| snapshot.path.clone()),
            active_buffer,
            capabilities: runtime.capabilities.clone(),
            events: tx.clone(),
        };
        self.record_async_task_started("Plugin Command", plugin_command_task_detail(&command_id));
        self.runtime.spawn_blocking(move || {
            let result = execute_plugin_command(&runtime, &command_id, &host)
                .map_err(|error| error.to_string());
            for event in
                plugin_command_completion_events(root, generation, plugin_id, command_id, result)
            {
                let _ = crate::ui_event_channel::send_critical_ui_event(&tx, event);
            }
        });
    }

    pub(crate) fn apply_plugin_command_finished(
        &mut self,
        plugin_id: String,
        command_id: String,
        result: Result<PluginCommandExecution, String>,
    ) {
        self.status =
            plugin_command_finished_status(&self.plugin_commands, &plugin_id, &command_id, result);
    }
}

/// Events emitted when a plugin command run finishes. The completion event is
/// always sent first; a run that staged buffer text through
/// `kuroya.buffer_set_text` additionally emits `PluginBufferTextApply`, even
/// when the run failed — a plugin that set the buffer text and then errored
/// still has its last staged write applied.
fn plugin_command_completion_events(
    root: PathBuf,
    generation: u64,
    plugin_id: String,
    command_id: String,
    result: Result<PluginCommandExecution, String>,
) -> Vec<UiEvent> {
    let pending_buffer_text = result
        .as_ref()
        .ok()
        .and_then(|execution| execution.pending_buffer_text.clone());
    let mut events = Vec::with_capacity(2);
    events.push(UiEvent::PluginCommandFinished {
        root,
        generation,
        plugin_id: plugin_id.clone(),
        command_id,
        result,
    });
    if let Some((path, text)) = pending_buffer_text {
        events.push(UiEvent::PluginBufferTextApply {
            plugin_id,
            path,
            text,
        });
    }
    events
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedPluginCommand {
    status: String,
    runtime: Option<PluginRuntimeRegistration>,
}

#[cfg(test)]
pub(crate) fn plugin_command_status(
    registry: &PluginCommandRegistry,
    runtimes: &PluginRuntimeRegistry,
    activations: &mut PluginActivationState,
    plugin_id: &str,
    command_id: &str,
) -> String {
    prepare_plugin_command_run(registry, runtimes, activations, plugin_id, command_id).status
}

fn prepare_plugin_command_run(
    registry: &PluginCommandRegistry,
    runtimes: &PluginRuntimeRegistry,
    activations: &mut PluginActivationState,
    plugin_id: &str,
    command_id: &str,
) -> PreparedPluginCommand {
    let Some(command) = registry.command(plugin_id, command_id) else {
        let plugin_id = plugin_command_status_fragment(plugin_id, "plugin");
        let command_id = plugin_command_status_fragment(command_id, "command");
        return PreparedPluginCommand {
            status: format!("Plugin command {plugin_id}:{command_id} is not registered"),
            runtime: None,
        };
    };
    let command_label = plugin_command_status_fragment(&command.label, "plugin command");
    let plugin_label = plugin_command_status_fragment(plugin_id, "plugin");
    let Some(runtime) = runtimes.plugin(plugin_id) else {
        return PreparedPluginCommand {
            status: format!(
                "Plugin command {command_label} is registered; runtime metadata for {plugin_label} is unavailable"
            ),
            runtime: None,
        };
    };
    let runtime = runtime.clone();
    let activated = activations.activate_plugin_command(runtimes, plugin_id, command_id);
    let activation =
        plugin_command_activation_status(&activated, activations, plugin_id, &runtime.name);
    if runtime.command_entry().is_some() {
        PreparedPluginCommand {
            status: format!(
                "Plugin command {command_label} {activation}; running sandboxed plugin"
            ),
            runtime: Some(runtime),
        }
    } else {
        PreparedPluginCommand {
            status: format!(
                "Plugin command {command_label} {activation}; plugin {plugin_label} has no sandbox entry"
            ),
            runtime: None,
        }
    }
}

pub(crate) fn plugin_command_finished_status(
    registry: &PluginCommandRegistry,
    plugin_id: &str,
    command_id: &str,
    result: Result<PluginCommandExecution, String>,
) -> String {
    let command_label = plugin_command_label_for_status(registry, plugin_id, command_id);
    match result {
        Ok(execution) => plugin_command_success_status(&command_label, execution),
        Err(error) => {
            let error = plugin_command_error_status_fragment(&error);
            format!("Plugin command {command_label} failed: {error}")
        }
    }
}

fn plugin_command_success_status(command_label: &str, execution: PluginCommandExecution) -> String {
    match (execution.exit_code, execution.status) {
        (0, Some(status)) => format!("Plugin command {command_label} completed: {status}"),
        (0, None) => format!("Plugin command {command_label} completed"),
        (exit_code, Some(status)) => {
            format!("Plugin command {command_label} failed with exit code {exit_code}: {status}")
        }
        (exit_code, None) => {
            format!("Plugin command {command_label} failed with exit code {exit_code}")
        }
    }
}

fn plugin_command_label_for_status(
    registry: &PluginCommandRegistry,
    plugin_id: &str,
    command_id: &str,
) -> String {
    registry
        .command(plugin_id, command_id)
        .map(|command| plugin_command_status_fragment(&command.label, "plugin command"))
        .unwrap_or_else(|| {
            format!(
                "{}:{}",
                plugin_command_status_fragment(plugin_id, "plugin"),
                plugin_command_status_fragment(command_id, "command")
            )
        })
}

fn plugin_command_activation_status(
    activated: &[PluginActivationRecord],
    activations: &PluginActivationState,
    plugin_id: &str,
    plugin_name: &str,
) -> String {
    let plugin_name = plugin_command_status_fragment(plugin_name, "plugin");
    if activated.iter().any(|record| record.plugin_id == plugin_id) {
        format!("activated plugin {plugin_name}")
    } else if activations.is_active(plugin_id) {
        format!("found plugin {plugin_name} already active")
    } else {
        format!("did not activate plugin {plugin_name}")
    }
}

fn plugin_command_status_fragment(value: &str, fallback: &str) -> String {
    plugin_command_status_fragment_cow(value, fallback).into_owned()
}

fn plugin_command_status_fragment_cow<'a>(value: &'a str, fallback: &str) -> Cow<'a, str> {
    sanitized_display_label_cow(value, PLUGIN_COMMAND_STATUS_FRAGMENT_MAX_CHARS, fallback)
}

#[cfg(test)]
fn plugin_command_entry_status_fragment(value: &str) -> String {
    plugin_command_entry_status_fragment_cow(value).into_owned()
}

#[cfg(test)]
fn plugin_command_entry_status_fragment_cow(value: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(value, PLUGIN_COMMAND_STATUS_ENTRY_MAX_CHARS, ".")
}

fn plugin_command_error_status_fragment(value: &str) -> Cow<'_, str> {
    if value.chars().count() <= PLUGIN_COMMAND_STATUS_ERROR_MAX_CHARS {
        display_error_label_cow(value)
    } else {
        sanitized_display_label_cow(
            value,
            PLUGIN_COMMAND_STATUS_ERROR_MAX_CHARS,
            "unknown error",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PLUGIN_COMMAND_STATUS_ENTRY_MAX_CHARS, PLUGIN_COMMAND_STATUS_FRAGMENT_MAX_CHARS,
        plugin_command_entry_status_fragment, plugin_command_entry_status_fragment_cow,
        plugin_command_finished_status, plugin_command_status, plugin_command_status_fragment,
        plugin_command_status_fragment_cow,
    };
    use crate::plugin_command_runtime::PluginCommandExecution;
    use crate::ui_events::UiEvent;
    use kuroya_core::{
        PLUGIN_API_VERSION, PluginActivationState, PluginCapabilities, PluginCommandContribution,
        PluginCommandRegistry, PluginContributions, PluginDescriptor, PluginManifest,
        PluginRuntimeRegistry,
    };
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};

    #[test]
    fn plugin_command_completion_events_apply_staged_text_after_success() {
        let events = super::plugin_command_completion_events(
            PathBuf::from("workspace"),
            3,
            "example.plugin".to_owned(),
            "example.run".to_owned(),
            Ok(PluginCommandExecution {
                exit_code: 0,
                status: None,
                used_default_export: false,
                logs: Vec::new(),
                pending_buffer_text: Some((
                    PathBuf::from("workspace/notes.md"),
                    "replacement".to_owned(),
                )),
            }),
        );

        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            UiEvent::PluginCommandFinished { plugin_id, command_id, result, .. }
                if plugin_id == "example.plugin"
                    && command_id == "example.run"
                    && result.is_ok()
        ));
        assert!(matches!(
            &events[1],
            UiEvent::PluginBufferTextApply { path, text, .. }
                if path.as_path() == Path::new("workspace/notes.md") && text == "replacement"
        ));
    }

    #[test]
    fn plugin_command_completion_events_apply_staged_text_after_failure() {
        // A plugin that set the buffer text and then errored still has its
        // last staged write applied.
        let events = super::plugin_command_completion_events(
            PathBuf::from("workspace"),
            3,
            "example.plugin".to_owned(),
            "example.run".to_owned(),
            Ok(PluginCommandExecution {
                exit_code: 1,
                status: None,
                used_default_export: false,
                logs: Vec::new(),
                pending_buffer_text: Some((PathBuf::from("notes.md"), "staged".to_owned())),
            }),
        );

        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            UiEvent::PluginCommandFinished { result: Ok(execution), .. }
                if execution.exit_code == 1
        ));
        assert!(matches!(
            &events[1],
            UiEvent::PluginBufferTextApply { text, .. } if text == "staged"
        ));
    }

    #[test]
    fn plugin_command_completion_events_without_staged_text_are_completion_only() {
        let events = super::plugin_command_completion_events(
            PathBuf::from("workspace"),
            3,
            "example.plugin".to_owned(),
            "example.run".to_owned(),
            Err("trapped".to_owned()),
        );

        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            UiEvent::PluginCommandFinished { result: Err(error), .. } if error == "trapped"
        ));
    }

    #[test]
    fn plugin_command_status_names_registered_command_without_executing() {
        let plugin = PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/example"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "example.plugin".to_owned(),
                name: "Example".to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    commands: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    commands: vec![PluginCommandContribution {
                        id: "example.sayHello".to_owned(),
                        title: "Say Hello".to_owned(),
                        category: None,
                    }],
                    ..PluginContributions::default()
                },
            },
        };
        let registry = PluginCommandRegistry::from_plugins(std::slice::from_ref(&plugin));
        let runtimes = PluginRuntimeRegistry::from_plugins(std::slice::from_ref(&plugin));
        let mut activations = PluginActivationState::default();

        assert_eq!(
            plugin_command_status(
                &registry,
                &runtimes,
                &mut activations,
                "example.plugin",
                "example.sayHello"
            ),
            "Plugin command example.plugin:example.sayHello is not registered"
        );
        assert_eq!(
            plugin_command_status(
                &registry,
                &runtimes,
                &mut activations,
                "example.plugin",
                "example.sayHello"
            ),
            "Plugin command example.plugin:example.sayHello is not registered"
        );
        assert_eq!(
            plugin_command_status(
                &registry,
                &runtimes,
                &mut activations,
                "example.plugin",
                "missing"
            ),
            "Plugin command example.plugin:missing is not registered"
        );
    }

    #[test]
    fn plugin_command_status_reports_registered_command_entry_boundary() {
        let plugin = PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/example"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "example.plugin".to_owned(),
                name: "Example".to_owned(),
                version: "0.1.0".to_owned(),
                entry: Some(PathBuf::from(
                    "workspace/.kuroya/plugins/example/plugin.wasm",
                )),
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    commands: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    commands: vec![PluginCommandContribution {
                        id: "example.sayHello".to_owned(),
                        title: "Say Hello".to_owned(),
                        category: Some("Example".to_owned()),
                    }],
                    ..PluginContributions::default()
                },
            },
        };
        let registry = PluginCommandRegistry::from_plugins(std::slice::from_ref(&plugin));
        let runtimes = PluginRuntimeRegistry::from_plugins(std::slice::from_ref(&plugin));
        let mut activations = PluginActivationState::default();

        assert_eq!(
            plugin_command_status(
                &registry,
                &runtimes,
                &mut activations,
                "example.plugin",
                "example.sayHello"
            ),
            "Plugin command Example: Say Hello activated plugin Example; running sandboxed plugin"
        );
    }

    #[test]
    fn plugin_command_status_sanitizes_missing_command_identifiers() {
        let plugin_id = format!("plugin\n{}\u{202e}\u{0007}", "id-".repeat(64));
        let command_id = format!("command\n{}\u{2066}\u{001b}", "id-".repeat(64));
        let mut activations = PluginActivationState::default();

        let status = plugin_command_status(
            &PluginCommandRegistry::default(),
            &PluginRuntimeRegistry::default(),
            &mut activations,
            &plugin_id,
            &command_id,
        );

        assert_status_display_safe(&status);
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Plugin command ".chars().count()
                    + ":".chars().count()
                    + " is not registered".chars().count()
                    + PLUGIN_COMMAND_STATUS_FRAGMENT_MAX_CHARS * 2
        );
    }

    #[test]
    fn plugin_command_status_sanitizes_registered_command_runtime_and_entry() {
        let plugin = PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/unsafe"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "unsafe.plugin".to_owned(),
                name: format!("Runtime\n{}\u{202e}\u{0007}", "name-".repeat(64)),
                version: "0.1.0".to_owned(),
                entry: Some(PathBuf::from(format!(
                    "workspace/.kuroya/plugins/unsafe\n{}\u{2066}\u{0007}/plugin.wasm",
                    "entry-".repeat(64)
                ))),
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    commands: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    commands: vec![PluginCommandContribution {
                        id: "unsafe.run".to_owned(),
                        title: format!("Run\n{}\u{202e}\u{0008}", "command-".repeat(64)),
                        category: Some(format!("Tools\n{}\u{2066}", "category-".repeat(64))),
                    }],
                    ..PluginContributions::default()
                },
            },
        };
        let registry = PluginCommandRegistry::from_plugins(std::slice::from_ref(&plugin));
        let runtimes = PluginRuntimeRegistry::from_plugins(std::slice::from_ref(&plugin));
        let mut activations = PluginActivationState::default();

        let status = plugin_command_status(
            &registry,
            &runtimes,
            &mut activations,
            "unsafe.plugin",
            "unsafe.run",
        );

        assert_status_display_safe(&status);
        assert!(status.contains("..."));
        assert!(
            status.chars().count()
                <= "Plugin command ".chars().count()
                    + " activated plugin ".chars().count()
                    + "; running sandboxed plugin".chars().count()
                    + PLUGIN_COMMAND_STATUS_FRAGMENT_MAX_CHARS * 2
        );
    }

    #[test]
    fn plugin_command_finished_status_reports_success_failure_and_runtime_errors() {
        let plugin = PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/example"),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: "example.plugin".to_owned(),
                name: "Example".to_owned(),
                version: "0.1.0".to_owned(),
                entry: Some(PathBuf::from(
                    "workspace/.kuroya/plugins/example/plugin.wasm",
                )),
                activation_events: Vec::new(),
                capabilities: PluginCapabilities {
                    commands: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    commands: vec![PluginCommandContribution {
                        id: "example.sayHello".to_owned(),
                        title: "Say Hello".to_owned(),
                        category: None,
                    }],
                    ..PluginContributions::default()
                },
            },
        };
        let registry = PluginCommandRegistry::from_plugins(std::slice::from_ref(&plugin));

        assert_eq!(
            plugin_command_finished_status(
                &registry,
                "example.plugin",
                "example.sayHello",
                Ok(PluginCommandExecution {
                    exit_code: 0,
                    status: Some("done".to_owned()),
                    used_default_export: false,
                    logs: Vec::new(),
                    pending_buffer_text: None,
                }),
            ),
            "Plugin command Example: Say Hello completed: done"
        );
        assert_eq!(
            plugin_command_finished_status(
                &registry,
                "example.plugin",
                "example.sayHello",
                Ok(PluginCommandExecution {
                    exit_code: 5,
                    status: None,
                    used_default_export: false,
                    logs: Vec::new(),
                    pending_buffer_text: None,
                }),
            ),
            "Plugin command Example: Say Hello failed with exit code 5"
        );

        let error_status = plugin_command_finished_status(
            &PluginCommandRegistry::default(),
            "unsafe\nplugin",
            "run\u{202e}",
            Err(format!("boom\n{}\u{202e}", "x".repeat(256))),
        );
        assert_status_display_safe(&error_status);
        assert!(error_status.contains("..."));
    }

    #[test]
    fn plugin_command_status_fragment_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            plugin_command_status_fragment_cow("example.plugin", "plugin"),
            Cow::Borrowed("example.plugin")
        ));

        let status_unicode = "Example \u{03bb}";
        match plugin_command_status_fragment_cow(status_unicode, "plugin") {
            Cow::Borrowed(label) => assert_eq!(label, status_unicode),
            Cow::Owned(label) => panic!("expected borrowed status label, got {label:?}"),
        }

        assert!(matches!(
            plugin_command_entry_status_fragment_cow("workspace/.kuroya/plugin.wasm"),
            Cow::Borrowed("workspace/.kuroya/plugin.wasm")
        ));

        let entry_unicode = "workspace/plugins/plugin-\u{03bb}.wasm";
        match plugin_command_entry_status_fragment_cow(entry_unicode) {
            Cow::Borrowed(label) => assert_eq!(label, entry_unicode),
            Cow::Owned(label) => panic!("expected borrowed entry label, got {label:?}"),
        }
    }

    #[test]
    fn plugin_command_status_fragment_cow_owns_dirty_truncated_and_fallback_output() {
        assert_owned_cow_eq(
            plugin_command_status_fragment_cow("alpha\nbeta\u{202e}", "plugin"),
            "alpha beta",
        );
        assert_owned_cow_eq(
            plugin_command_entry_status_fragment_cow("workspace\nplugin.wasm\u{2066}"),
            "workspace plugin.wasm",
        );

        let long_status = format!("command-{}", "id".repeat(128));
        let status_label = plugin_command_status_fragment_cow(&long_status, "command");
        assert!(matches!(status_label, Cow::Owned(_)));
        assert!(status_label.contains("..."));
        assert!(status_label.chars().count() <= PLUGIN_COMMAND_STATUS_FRAGMENT_MAX_CHARS);

        let long_entry = format!("workspace/{}", "entry".repeat(128));
        let entry_label = plugin_command_entry_status_fragment_cow(&long_entry);
        assert!(matches!(entry_label, Cow::Owned(_)));
        assert!(entry_label.contains("..."));
        assert!(entry_label.chars().count() <= PLUGIN_COMMAND_STATUS_ENTRY_MAX_CHARS);

        assert_owned_cow_eq(
            plugin_command_status_fragment_cow(" \n \u{202e}", "plugin"),
            "plugin",
        );
        assert_owned_cow_eq(plugin_command_entry_status_fragment_cow("\n\u{202e}"), ".");
    }

    #[test]
    fn plugin_command_status_fragment_wrappers_match_cow_helpers() {
        let long_status = format!("command-{}", "id".repeat(128));
        for (value, fallback) in [
            ("example.plugin", "plugin"),
            ("Example \u{03bb}", "plugin"),
            ("alpha\nbeta\u{202e}", "plugin"),
            (" \n \u{202e}", "plugin"),
            (long_status.as_str(), "command"),
        ] {
            assert_eq!(
                plugin_command_status_fragment(value, fallback),
                plugin_command_status_fragment_cow(value, fallback).into_owned()
            );
        }

        let long_entry = format!("workspace/{}", "entry".repeat(128));
        for value in [
            "workspace/.kuroya/plugin.wasm",
            "workspace/plugins/plugin-\u{03bb}.wasm",
            "workspace\nplugin.wasm\u{2066}",
            "\n\u{202e}",
            long_entry.as_str(),
        ] {
            assert_eq!(
                plugin_command_entry_status_fragment(value),
                plugin_command_entry_status_fragment_cow(value).into_owned()
            );
        }
    }

    fn assert_owned_cow_eq(value: Cow<'_, str>, expected: &str) {
        match value {
            Cow::Owned(label) => assert_eq!(label, expected),
            Cow::Borrowed(label) => panic!("expected owned label, got borrowed {label:?}"),
        }
    }

    fn assert_status_display_safe(value: &str) {
        assert!(!value.chars().any(char::is_control), "{value:?}");
        assert!(!value.chars().any(is_bidi_format_control), "{value:?}");
    }

    fn is_bidi_format_control(ch: char) -> bool {
        matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
    }
}

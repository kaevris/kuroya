use crate::large_file_mode::buffer_uses_large_file_performance_mode;
use kuroya_core::{
    BufferId, LanguageId, PluginActivationRecord, PluginActivationState, PluginLanguageRegistry,
    PluginRuntimeRegistry, TextBuffer,
};
use std::{
    collections::HashSet,
    fmt::{self, Write as _},
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum PluginLanguageActivationError {
    EmptyLanguageId,
    UnsupportedCharacters { language_id: String },
}

impl fmt::Display for PluginLanguageActivationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLanguageId => write!(f, "plugin language activation id cannot be empty"),
            Self::UnsupportedCharacters { language_id } => write!(
                f,
                "plugin language activation id {language_id:?} contains unsupported characters"
            ),
        }
    }
}

pub(crate) fn activate_plugin_language_for_id(
    activations: &mut PluginActivationState,
    runtimes: &PluginRuntimeRegistry,
    language_id: Option<&str>,
) -> Vec<PluginActivationRecord> {
    let Some(language_id) = language_id else {
        return Vec::new();
    };
    activate_plugin_language_for_validated_id(activations, runtimes, language_id)
        .unwrap_or_default()
}

pub(crate) fn activate_plugin_languages_for_buffers(
    activations: &mut PluginActivationState,
    runtimes: &PluginRuntimeRegistry,
    plugin_languages: &PluginLanguageRegistry,
    buffers: &[TextBuffer],
    lossy_buffers: &HashSet<u64>,
    binary_buffers: &HashSet<u64>,
) -> Vec<PluginActivationRecord> {
    let mut records = Vec::new();
    let mut attempted_language_ids = HashSet::<&str>::new();
    for buffer in buffers {
        let Some(language_id) = plugin_language_activation_id_if_allowed_ref(
            buffer,
            plugin_languages,
            lossy_buffers,
            binary_buffers,
        ) else {
            continue;
        };
        let Ok(language_id) = validate_plugin_language_activation_id(language_id) else {
            continue;
        };
        if !attempted_language_ids.insert(language_id) {
            continue;
        }
        records.extend(
            activate_plugin_language_for_validated_id(activations, runtimes, language_id)
                .unwrap_or_default(),
        );
    }
    records
}

pub(crate) fn plugin_language_reclassified_buffer_ids(
    previous_languages: &PluginLanguageRegistry,
    current_languages: &PluginLanguageRegistry,
    buffers: &[TextBuffer],
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> Vec<BufferId> {
    buffers
        .iter()
        .filter(|buffer| {
            let id = buffer.id();
            !lossy_buffers.contains(&id)
                && !binary_buffers.contains(&id)
                && !buffer_uses_large_file_performance_mode(buffer)
        })
        .filter_map(|buffer| {
            let previous = plugin_language_activation_id_ref(buffer, previous_languages);
            (previous != plugin_language_activation_id_ref(buffer, current_languages))
                .then_some(buffer.id())
        })
        .collect()
}

pub(crate) fn plugin_language_activation_id(
    buffer: &TextBuffer,
    plugin_languages: &PluginLanguageRegistry,
) -> String {
    plugin_language_activation_id_ref(buffer, plugin_languages).to_owned()
}

fn plugin_language_activation_id_ref<'a>(
    buffer: &'a TextBuffer,
    plugin_languages: &'a PluginLanguageRegistry,
) -> &'a str {
    if buffer.language() == LanguageId::PlainText {
        if let Some(language) = buffer
            .path()
            .and_then(|path| plugin_languages.language_for_path(path))
        {
            return &language.language_id;
        }
    }

    buffer.language().activation_id()
}

pub(crate) fn plugin_language_activation_id_if_allowed(
    buffer: &TextBuffer,
    plugin_languages: &PluginLanguageRegistry,
    lossy_buffers: &HashSet<u64>,
    binary_buffers: &HashSet<u64>,
) -> Option<String> {
    plugin_language_activation_id_if_allowed_ref(
        buffer,
        plugin_languages,
        lossy_buffers,
        binary_buffers,
    )
    .map(str::to_owned)
}

fn plugin_language_activation_id_if_allowed_ref<'a>(
    buffer: &'a TextBuffer,
    plugin_languages: &'a PluginLanguageRegistry,
    lossy_buffers: &HashSet<u64>,
    binary_buffers: &HashSet<u64>,
) -> Option<&'a str> {
    if lossy_buffers.contains(&buffer.id())
        || binary_buffers.contains(&buffer.id())
        || buffer_uses_large_file_performance_mode(buffer)
    {
        return None;
    }

    Some(plugin_language_activation_id_ref(buffer, plugin_languages))
}

pub(crate) fn append_plugin_language_activation_status(
    mut status: String,
    activations: &[PluginActivationRecord],
) -> String {
    if activations.is_empty() {
        return status;
    }

    status.push_str("; ");
    append_plugin_language_activation_summary(&mut status, activations);
    status
}

fn append_plugin_language_activation_summary(
    status: &mut String,
    activations: &[PluginActivationRecord],
) {
    let Some(first) = activations.first() else {
        return;
    };

    if activations.len() == 1 {
        append_single_plugin_language_activation_status(status, first);
        return;
    }

    let mut seen = HashSet::with_capacity(activations.len());
    seen.insert(first.plugin_id.as_str());
    let mut count = 1usize;
    for activation in &activations[1..] {
        if seen.insert(activation.plugin_id.as_str()) {
            count += 1;
        }
    }

    if count == 1 {
        append_single_plugin_language_activation_status(status, first);
    } else {
        let _ = write!(status, "activated {count} language plugins");
    }
}

fn append_single_plugin_language_activation_status(
    status: &mut String,
    activation: &PluginActivationRecord,
) {
    status.reserve("activated language plugin ".len() + activation.name.len());
    status.push_str("activated language plugin ");
    status.push_str(&activation.name);
}

fn activate_plugin_language_for_validated_id(
    activations: &mut PluginActivationState,
    runtimes: &PluginRuntimeRegistry,
    language_id: &str,
) -> Result<Vec<PluginActivationRecord>, PluginLanguageActivationError> {
    let language_id = validate_plugin_language_activation_id(language_id)?;
    Ok(activations.activate_language(runtimes, language_id))
}

fn validate_plugin_language_activation_id(
    language_id: &str,
) -> Result<&str, PluginLanguageActivationError> {
    let language_id = language_id.trim();
    if language_id.is_empty() {
        return Err(PluginLanguageActivationError::EmptyLanguageId);
    }
    if !language_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(PluginLanguageActivationError::UnsupportedCharacters {
            language_id: language_id.to_owned(),
        });
    }
    Ok(language_id)
}

#[cfg(test)]
mod tests {
    use super::{
        activate_plugin_language_for_id, activate_plugin_languages_for_buffers,
        append_plugin_language_activation_status, plugin_language_activation_id,
        plugin_language_activation_id_if_allowed, plugin_language_reclassified_buffer_ids,
        validate_plugin_language_activation_id,
    };
    use crate::large_file_mode::LARGE_FILE_PERFORMANCE_MODE_MAX_LINES;
    use kuroya_core::{
        LanguageId, PluginActivationEvent, PluginActivationState, PluginActivationTrigger,
        PluginCapabilities, PluginContributions, PluginDescriptor, PluginLanguageContribution,
        PluginLanguageRegistry, PluginManifest, PluginRuntimeRegistry, TextBuffer,
    };
    use std::time::Instant;
    use std::{collections::HashSet, path::PathBuf};
    use tokio::runtime::Runtime;

    #[test]
    fn plugin_language_registry_change_reclassifies_affected_disk_buffers() {
        let previous = PluginLanguageRegistry::default();
        let current = PluginLanguageRegistry::from_plugins(&[plugin_descriptor(
            "example.plugin",
            "Example",
            vec![PluginActivationEvent::OnLanguage("example-lang".to_owned())],
        )]);
        let buffers = vec![
            TextBuffer::from_text(1, Some(PathBuf::from("src/main.ex")), String::new()),
            TextBuffer::from_text(2, Some(PathBuf::from("src/main.rs")), String::new()),
            TextBuffer::from_text(3, None, String::new()),
        ];

        assert_eq!(
            plugin_language_reclassified_buffer_ids(
                &previous,
                &current,
                &buffers,
                &HashSet::new(),
                &HashSet::new(),
            ),
            vec![1]
        );
        assert!(
            plugin_language_reclassified_buffer_ids(
                &current,
                &current,
                &buffers,
                &HashSet::new(),
                &HashSet::new(),
            )
            .is_empty()
        );
        assert_eq!(
            plugin_language_reclassified_buffer_ids(
                &current,
                &previous,
                &buffers,
                &HashSet::new(),
                &HashSet::new(),
            ),
            vec![1]
        );
        assert!(
            plugin_language_reclassified_buffer_ids(
                &previous,
                &current,
                &buffers,
                &HashSet::from([1]),
                &HashSet::new(),
            )
            .is_empty()
        );
        assert!(
            plugin_language_reclassified_buffer_ids(
                &previous,
                &current,
                &buffers,
                &HashSet::new(),
                &HashSet::from([1]),
            )
            .is_empty()
        );
    }

    #[test]
    fn workspace_plugins_loaded_reclassifies_affected_open_buffers() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.ex")),
            String::new(),
        ));
        app.workspace_plugins_next_request_id = 1;
        app.workspace_plugins_active_request_id = 1;
        let configs = crate::lsp_runtime::lsp_server_configs_for_settings(&app.settings);
        let before = crate::lsp_lifecycle::lsp_language_id_for_buffer(
            &configs,
            &app.plugin_languages,
            app.buffer(7).expect("buffer"),
        )
        .into_owned();

        assert!(crate::ui_event_channel::send_ui_event(
            &app.tx,
            crate::ui_events::UiEvent::WorkspacePluginsLoaded {
                request_id: 1,
                root,
                plugins: vec![plugin_descriptor(
                    "example.plugin",
                    "Example",
                    vec![PluginActivationEvent::OnLanguage("example-lang".to_owned())],
                )],
                errors: Vec::new(),
                syntax_load: crate::syntax::PluginSyntaxLoad::empty(),
            }
        ));

        assert_eq!(app.handle_events(), 1);

        let buffer = app.buffer(7).expect("buffer");
        assert_eq!(buffer.language(), LanguageId::PlainText);
        let after = crate::lsp_lifecycle::lsp_language_id_for_buffer(
            &configs,
            &app.plugin_languages,
            buffer,
        )
        .into_owned();
        assert_ne!(before.as_str(), after.as_str());
        assert_eq!(after.as_str(), "example-lang");
        assert!(app.pending_language_sync.contains_key(&7));
    }

    fn app_for_test(root: PathBuf) -> crate::KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = kuroya_core::EditorSettings::default();
        crate::KuroyaApp::from_startup_context(crate::app_startup_context::AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: kuroya_core::Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: crate::terminal::TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    #[test]
    fn plugin_language_activation_id_prefers_contributed_plain_text_language() {
        let registry = PluginLanguageRegistry::from_plugins(&[plugin_descriptor(
            "example.plugin",
            "Example",
            vec![PluginActivationEvent::OnLanguage("example-lang".to_owned())],
        )]);
        let plugin_buffer =
            TextBuffer::from_text(1, Some(PathBuf::from("src/main.ex")), "example".to_owned());
        let rust_buffer =
            TextBuffer::from_text(2, Some(PathBuf::from("src/main.rs")), String::new());

        assert_eq!(
            plugin_language_activation_id(&plugin_buffer, &registry),
            "example-lang"
        );
        assert_eq!(
            plugin_language_activation_id(&rust_buffer, &registry),
            "rust"
        );
    }

    #[test]
    fn plugin_language_activation_skips_protected_buffers() {
        let registry = PluginLanguageRegistry::default();
        let lossy = TextBuffer::from_text(7, Some(PathBuf::from("src/main.rs")), String::new());
        let binary = TextBuffer::from_text(8, Some(PathBuf::from("src/main.py")), String::new());
        let mut performance_text = "x\n".repeat(LARGE_FILE_PERFORMANCE_MODE_MAX_LINES);
        performance_text.push('x');
        let performance = TextBuffer::from_text(
            9,
            Some(PathBuf::from("src/performance.rs")),
            performance_text,
        );

        assert_eq!(
            plugin_language_activation_id_if_allowed(
                &lossy,
                &registry,
                &HashSet::from([lossy.id()]),
                &HashSet::new(),
            ),
            None
        );
        assert_eq!(
            plugin_language_activation_id_if_allowed(
                &binary,
                &registry,
                &HashSet::new(),
                &HashSet::from([binary.id()]),
            ),
            None
        );
        assert_eq!(
            plugin_language_activation_id_if_allowed(
                &performance,
                &registry,
                &HashSet::new(),
                &HashSet::new(),
            ),
            None
        );
    }

    #[test]
    fn plugin_language_activation_marks_matching_runtime_once() {
        let descriptor = plugin_descriptor(
            "example.plugin",
            "Example",
            vec![PluginActivationEvent::OnLanguage("example-lang".to_owned())],
        );
        let runtimes = PluginRuntimeRegistry::from_plugins(&[descriptor]);
        let mut activations = PluginActivationState::default();

        let records =
            activate_plugin_language_for_id(&mut activations, &runtimes, Some("example-lang"));

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].plugin_id, "example.plugin");
        assert_eq!(records[0].name, "Example");
        assert_eq!(
            records[0].trigger,
            PluginActivationTrigger::Language("example-lang".to_owned())
        );
        assert!(
            activate_plugin_language_for_id(&mut activations, &runtimes, Some("example-lang"))
                .is_empty()
        );
    }

    #[test]
    fn plugin_language_activation_validates_requested_language_id_before_runtime_lookup() {
        let descriptor = plugin_descriptor(
            "example.plugin",
            "Example",
            vec![PluginActivationEvent::Any],
        );
        let runtimes = PluginRuntimeRegistry::from_plugins(&[descriptor]);
        let mut activations = PluginActivationState::default();

        assert!(activate_plugin_language_for_id(&mut activations, &runtimes, Some(" ")).is_empty());
        assert!(
            activate_plugin_language_for_id(&mut activations, &runtimes, Some("bad/language"))
                .is_empty()
        );
        assert_eq!(activations.active_count(), 0);

        let records =
            activate_plugin_language_for_id(&mut activations, &runtimes, Some(" example-lang "));

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].plugin_id, "example.plugin");
        assert_eq!(
            records[0].trigger,
            PluginActivationTrigger::Language("example-lang".to_owned())
        );
    }

    #[test]
    fn plugin_language_activation_batch_skips_duplicate_and_invalid_languages() {
        let invalid_descriptor = plugin_descriptor_with_language(
            "invalid.plugin",
            "Invalid",
            "bad/language",
            "bad",
            Vec::new(),
        );
        let valid_descriptor = plugin_descriptor(
            "example.plugin",
            "Example",
            vec![PluginActivationEvent::OnLanguage("example-lang".to_owned())],
        );
        let plugin_languages = PluginLanguageRegistry::from_plugins(&[
            invalid_descriptor.clone(),
            valid_descriptor.clone(),
        ]);
        let runtimes = PluginRuntimeRegistry::from_plugins(&[invalid_descriptor, valid_descriptor]);
        let buffers = vec![
            TextBuffer::from_text(1, Some(PathBuf::from("src/first.ex")), String::new()),
            TextBuffer::from_text(2, Some(PathBuf::from("src/second.ex")), String::new()),
            TextBuffer::from_text(3, Some(PathBuf::from("src/invalid.bad")), String::new()),
        ];
        let mut activations = PluginActivationState::default();

        let records = activate_plugin_languages_for_buffers(
            &mut activations,
            &runtimes,
            &plugin_languages,
            &buffers,
            &HashSet::new(),
            &HashSet::new(),
        );

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].plugin_id, "example.plugin");
        assert!(activations.is_active("example.plugin"));
        assert!(!activations.is_active("invalid.plugin"));
    }

    #[test]
    fn plugin_language_activation_validation_errors_are_specific() {
        assert_eq!(
            validate_plugin_language_activation_id("  rust  "),
            Ok("rust")
        );
        assert_eq!(
            validate_plugin_language_activation_id(""),
            Err(super::PluginLanguageActivationError::EmptyLanguageId)
        );
        assert_eq!(
            validate_plugin_language_activation_id("bad/language")
                .unwrap_err()
                .to_string(),
            "plugin language activation id \"bad/language\" contains unsupported characters"
        );
    }

    #[test]
    fn plugin_language_activation_status_summarizes_new_records() {
        let descriptor = plugin_descriptor(
            "example.plugin",
            "Example",
            vec![PluginActivationEvent::OnLanguage("example-lang".to_owned())],
        );
        let runtimes = PluginRuntimeRegistry::from_plugins(&[descriptor]);
        let mut activations = PluginActivationState::default();
        let records =
            activate_plugin_language_for_id(&mut activations, &runtimes, Some("example-lang"));

        assert_eq!(
            append_plugin_language_activation_status(
                "Opened src/main.ex in 1ms".to_owned(),
                &records
            ),
            "Opened src/main.ex in 1ms; activated language plugin Example"
        );
        assert_eq!(
            append_plugin_language_activation_status("Opened src/main.ex in 1ms".to_owned(), &[]),
            "Opened src/main.ex in 1ms"
        );
        assert_eq!(
            append_plugin_language_activation_status(
                "Opened src/main.ex in 1ms".to_owned(),
                &[records[0].clone(), records[0].clone()]
            ),
            "Opened src/main.ex in 1ms; activated language plugin Example"
        );
    }

    fn plugin_descriptor(
        id: &str,
        name: &str,
        activation_events: Vec<PluginActivationEvent>,
    ) -> PluginDescriptor {
        plugin_descriptor_with_language(id, name, "example-lang", "ex", activation_events)
    }

    fn plugin_descriptor_with_language(
        id: &str,
        name: &str,
        language_id: &str,
        extension: &str,
        activation_events: Vec<PluginActivationEvent>,
    ) -> PluginDescriptor {
        PluginDescriptor {
            root: PathBuf::from("workspace/.kuroya/plugins/example"),
            manifest: PluginManifest {
                api_version: "1".to_owned(),
                id: id.to_owned(),
                name: name.to_owned(),
                version: "0.1.0".to_owned(),
                entry: None,
                activation_events,
                capabilities: PluginCapabilities {
                    languages: true,
                    ..PluginCapabilities::default()
                },
                contributes: PluginContributions {
                    languages: vec![PluginLanguageContribution {
                        id: language_id.to_owned(),
                        extensions: vec![extension.to_owned()],
                        aliases: vec!["ExampleLang".to_owned()],
                    }],
                    ..PluginContributions::default()
                },
            },
        }
    }
}

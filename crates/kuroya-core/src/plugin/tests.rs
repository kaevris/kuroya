use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

mod runtime_registry;

fn temp_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kuroya-plugin-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn plugin_manifest_with_path_field(field: &str, path: &str) -> String {
    let path = toml_string(path);
    match field {
        "entry" => format!(
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"
                    entry = {path}
                "#
        ),
        "theme" => format!(
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"

                    [[contributes.themes]]
                    id = "example-dark"
                    label = "Example Dark"
                    path = {path}
                "#
        ),
        "syntax" => format!(
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"

                    [[contributes.syntaxes]]
                    language = "example-lang"
                    path = {path}
                "#
        ),
        _ => unreachable!(),
    }
}

fn toml_string(value: &str) -> String {
    format!("{value:?}")
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn assert_bounded_display_label(value: &str) {
    assert!(value.chars().count() <= MAX_PLUGIN_DISPLAY_LABEL_CHARS);
}

#[test]
fn plugin_text_reader_enforces_byte_limit_and_utf8() {
    let root = temp_root("bounded-read");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("file.txt");

    fs::write(&path, "12345").unwrap();
    assert_eq!(read_plugin_text_file_with_limit(&path, 5).unwrap(), "12345");

    fs::write(&path, "123456").unwrap();
    let oversized = read_plugin_text_file_with_limit(&path, 5)
        .unwrap_err()
        .to_string();
    assert!(oversized.contains("plugin file limit"));

    fs::write(&path, [0xff]).unwrap();
    let invalid_utf8 = read_plugin_text_file_with_limit(&path, 5)
        .unwrap_err()
        .to_string();
    assert!(invalid_utf8.contains("valid UTF-8"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn plugin_manifest_normalizes_capabilities_contributions_and_paths() {
    let root = temp_root("manifest");
    let manifest = parse_plugin_manifest_toml(
        &root,
        r#"
                api_version = "1"
                id = " example.plugin "
                name = " Example Plugin "
                version = " 0.1.0 "
                entry = "bin/plugin.wasm"
                activation_events = [
                    " onCommand: example.sayHello ",
                    "onLanguage: example-lang",
                    "onStartupFinished",
                    "onCommand:example.sayHello",
                ]

                [capabilities]
                commands = true
                languages = true
                themes = true
                syntax = true
                workspace_read = true

                [[contributes.commands]]
                id = "example.sayHello"
                title = " Say Hello "
                category = " Example "

                [[contributes.languages]]
                id = "example-lang"
                extensions = [".ex", "EX", " ex2 "]
                aliases = [" Example ", "Example", ""]

                [[contributes.themes]]
                id = "example-dark"
                label = " Example Dark "
                path = "themes/dark.toml"

                [[contributes.syntaxes]]
                language = "example-lang"
                path = "syntax/example.sublime-syntax"
            "#,
    )
    .unwrap();

    assert_eq!(manifest.root, root);
    assert_eq!(manifest.manifest.id, "example.plugin");
    assert_eq!(manifest.manifest.name, "Example Plugin");
    assert_eq!(manifest.manifest.version, "0.1.0");
    assert_eq!(
        manifest.manifest.entry,
        Some(manifest.root.join("bin/plugin.wasm"))
    );
    assert_eq!(
        manifest.manifest.activation_events,
        vec![
            PluginActivationEvent::OnCommand("example.sayHello".to_owned()),
            PluginActivationEvent::OnLanguage("example-lang".to_owned()),
            PluginActivationEvent::OnStartupFinished,
        ]
    );
    assert!(manifest.manifest.capabilities.workspace_read);
    assert!(!manifest.manifest.capabilities.workspace_write);
    assert_eq!(
        manifest.manifest.contributes.commands[0],
        PluginCommandContribution {
            id: "example.sayHello".to_owned(),
            title: "Say Hello".to_owned(),
            category: Some("Example".to_owned())
        }
    );
    assert_eq!(
        manifest.manifest.contributes.languages[0].extensions,
        vec!["ex", "ex2"]
    );
    assert_eq!(
        manifest.manifest.contributes.languages[0].aliases,
        vec!["Example"]
    );
    assert_eq!(
        manifest.manifest.contributes.themes[0].path,
        manifest.root.join("themes/dark.toml")
    );
    assert_eq!(
        manifest.manifest.contributes.syntaxes[0].path,
        manifest.root.join("syntax/example.sublime-syntax")
    );
}

#[test]
fn plugin_manifest_sanitizes_and_bounds_display_labels() {
    let root = temp_root("display-labels");
    let long_label = "L".repeat(MAX_PLUGIN_DISPLAY_LABEL_CHARS + 8);
    let manifest = normalize_plugin_manifest(
        &root,
        PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "example.plugin".to_owned(),
            name: " Example\tPlugin\nName ".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities::default(),
            contributes: PluginContributions {
                commands: vec![PluginCommandContribution {
                    id: "example.run".to_owned(),
                    title: format!(" Run\n{long_label} "),
                    category: Some(" Tools\rMenu ".to_owned()),
                }],
                languages: vec![PluginLanguageContribution {
                    id: "example-lang".to_owned(),
                    extensions: vec!["ex".to_owned()],
                    aliases: vec![
                        " Example\nLanguage ".to_owned(),
                        long_label.clone(),
                        "\u{7}\u{8}".to_owned(),
                        "Example Language".to_owned(),
                    ],
                }],
                themes: vec![PluginThemeContribution {
                    id: "example-dark".to_owned(),
                    label: format!("{long_label} overflow"),
                    path: PathBuf::from("themes/dark.toml"),
                }],
                ..PluginContributions::default()
            },
        },
    )
    .unwrap();

    assert_eq!(manifest.name, "Example Plugin Name");
    assert!(!manifest.name.chars().any(is_plugin_display_format_control));
    let command = &manifest.contributes.commands[0];
    assert_eq!(command.category.as_deref(), Some("Tools Menu"));
    assert_bounded_display_label(&command.title);
    assert!(command.title.ends_with(DISPLAY_LABEL_OMISSION));
    assert!(!command.title.contains('\n'));
    assert!(!command.title.chars().any(is_plugin_display_format_control));

    let language = &manifest.contributes.languages[0];
    assert_eq!(
        language.aliases,
        vec![
            "Example Language".to_owned(),
            format!(
                "{}{DISPLAY_LABEL_OMISSION}",
                "L".repeat(MAX_PLUGIN_DISPLAY_LABEL_CHARS - DISPLAY_LABEL_OMISSION.len())
            ),
        ]
    );
    for alias in &language.aliases {
        assert_bounded_display_label(alias);
        assert!(!alias.chars().any(is_plugin_display_format_control));
    }

    let theme = &manifest.contributes.themes[0];
    assert_bounded_display_label(&theme.label);
    assert!(theme.label.ends_with(DISPLAY_LABEL_OMISSION));
}

#[test]
fn plugin_manifest_rejects_display_labels_that_sanitize_empty() {
    let root = temp_root("empty-display-label");
    let error = normalize_plugin_manifest(
        &root,
        PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "example.plugin".to_owned(),
            name: "\u{7}\n\t".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities::default(),
            contributes: PluginContributions::default(),
        },
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("plugin name cannot be empty"));
}

#[test]
fn plugin_manifest_rejects_duplicate_contribution_ids() {
    let root = temp_root("duplicate-contributions");
    for (manifest, expected) in [
        (
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"

                    [[contributes.commands]]
                    id = "example.run"
                    title = "Run"

                    [[contributes.commands]]
                    id = "example.run"
                    title = "Run Again"
                "#,
            "plugin command id example.run is duplicated",
        ),
        (
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"

                    [[contributes.languages]]
                    id = "example-lang"

                    [[contributes.languages]]
                    id = "example-lang"
                "#,
            "plugin language id example-lang is duplicated",
        ),
        (
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"

                    [[contributes.themes]]
                    id = "example-dark"
                    label = "Example Dark"
                    path = "themes/dark.toml"

                    [[contributes.themes]]
                    id = "example-dark"
                    label = "Example Dark Again"
                    path = "themes/dark-again.toml"
                "#,
            "plugin theme id example-dark is duplicated",
        ),
        (
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"

                    [[contributes.syntaxes]]
                    language = "example-lang"
                    path = "syntax/example.sublime-syntax"

                    [[contributes.syntaxes]]
                    language = "example-lang"
                    path = "syntax/example-again.sublime-syntax"
                "#,
            "plugin syntax language example-lang is duplicated",
        ),
    ] {
        let error = parse_plugin_manifest_toml(&root, manifest)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn plugin_manifest_rejects_hidden_controls_in_versions_and_extensions() {
    let root = temp_root("hidden-controls");

    let version_error = parse_plugin_manifest_toml(
        &root,
        r#"
                id = "example.plugin"
                name = "Example"
                version = "0.1.0\u202e"
            "#,
    )
    .unwrap_err()
    .to_string();
    assert!(
        version_error.contains("plugin version contains unsupported characters"),
        "{version_error}"
    );

    for extension in [
        r#""rs\u202e""#.to_owned(),
        toml_string(&"x".repeat(MAX_PLUGIN_LANGUAGE_EXTENSION_CHARS + 1)),
    ] {
        let manifest = format!(
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"

                    [[contributes.languages]]
                    id = "example-lang"
                    extensions = [{extension}]
                "#
        );
        let error = parse_plugin_manifest_toml(&root, &manifest)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("plugin language extension is invalid"),
            "{error}"
        );
    }
}

#[test]
fn plugin_manifest_rejects_dense_manifest_lists() {
    let root = temp_root("dense-manifest");
    let activation_events = (0..=MAX_PLUGIN_ACTIVATION_EVENTS)
        .map(|index| format!(r#""onCommand:example.run{index}""#))
        .collect::<Vec<_>>()
        .join(", ");
    let manifest = format!(
        r#"
                id = "example.plugin"
                name = "Example"
                version = "0.1.0"
                activation_events = [{activation_events}]
            "#
    );
    let error = parse_plugin_manifest_toml(&root, &manifest)
        .unwrap_err()
        .to_string();
    assert!(error.contains("plugin activation events contains too many items"));

    let commands = (0..=MAX_PLUGIN_COMMAND_CONTRIBUTIONS)
        .map(|index| {
            format!(
                r#"
                    [[contributes.commands]]
                    id = "example.run{index}"
                    title = "Run {index}"
                    "#
            )
        })
        .collect::<String>();
    let manifest = format!(
        r#"
                id = "example.plugin"
                name = "Example"
                version = "0.1.0"
                {commands}
            "#
    );
    let error = parse_plugin_manifest_toml(&root, &manifest)
        .unwrap_err()
        .to_string();
    assert!(error.contains("plugin command contributions contains too many items"));

    let extensions = (0..=MAX_PLUGIN_LANGUAGE_EXTENSIONS)
        .map(|index| format!(r#""ex{index}""#))
        .collect::<Vec<_>>()
        .join(", ");
    let manifest = format!(
        r#"
                id = "example.plugin"
                name = "Example"
                version = "0.1.0"

                [[contributes.languages]]
                id = "example-lang"
                extensions = [{extensions}]
            "#
    );
    let error = parse_plugin_manifest_toml(&root, &manifest)
        .unwrap_err()
        .to_string();
    assert!(error.contains("plugin language extensions contains too many items"));
}

#[test]
fn plugin_manifest_applies_path_guard_to_all_path_fields() {
    let root = temp_root("path-fields");
    for (field, escaped_path, expected) in [
        ("entry", "", "plugin entry cannot be empty"),
        ("theme", "", "plugin theme path cannot be empty"),
        ("syntax", "", "plugin syntax path cannot be empty"),
        (
            "entry",
            "../escape.wasm",
            "plugin entry must stay inside the plugin root",
        ),
        (
            "theme",
            "../escape.toml",
            "plugin theme path must stay inside the plugin root",
        ),
        (
            "syntax",
            "../escape.sublime-syntax",
            "plugin syntax path must stay inside the plugin root",
        ),
    ] {
        let manifest = plugin_manifest_with_path_field(field, escaped_path);
        let error = parse_plugin_manifest_toml(&root, &manifest)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{field}: {error}");
    }
}

#[test]
fn plugin_manifest_rejects_absolute_sibling_prefix_path_escapes() {
    let root = temp_root("plugin-root");
    let sibling = root.with_file_name(format!(
        "{}-sibling",
        root.file_name().unwrap().to_string_lossy()
    ));
    let sibling_entry = sibling.join("bin/plugin.wasm");
    let error = parse_plugin_manifest_toml(
        &root,
        &plugin_manifest_with_path_field("entry", &path_string(&sibling_entry)),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("plugin entry must be relative to the plugin root"));

    let sibling_theme = sibling.join("themes/dark.toml");
    let error = parse_plugin_manifest_toml(
        &root,
        &plugin_manifest_with_path_field("theme", &path_string(&sibling_theme)),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("plugin theme path must be relative to the plugin root"));

    let sibling_syntax = sibling.join("syntax/example.sublime-syntax");
    let error = parse_plugin_manifest_toml(
        &root,
        &plugin_manifest_with_path_field("syntax", &path_string(&sibling_syntax)),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("plugin syntax path must be relative to the plugin root"));
}

#[cfg(windows)]
#[test]
fn plugin_manifest_rejects_windows_absolute_manifest_paths() {
    let root = PathBuf::from(r"C:\Workspace\Plugin");
    for path in [
        r"c:\workspace\plugin\bin\plugin.wasm",
        r"\Workspace\Plugin\bin\plugin.wasm",
        r"C:bin\plugin.wasm",
        r"D:\Workspace\Plugin\bin\plugin.wasm",
    ] {
        assert!(
            parse_plugin_manifest_toml(&root, &plugin_manifest_with_path_field("entry", path))
                .is_err(),
            "{path}"
        );
    }
}

#[test]
fn plugin_command_registry_uses_declared_capability_and_dedupes_plugin_commands() {
    let enabled = PluginDescriptor {
        root: PathBuf::from("plugins/enabled"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "enabled.plugin".to_owned(),
            name: "Enabled Plugin".to_owned(),
            version: "0.1.0".to_owned(),
            entry: Some(PathBuf::from("plugins/enabled/plugin.wasm")),
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                commands: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                commands: vec![
                    PluginCommandContribution {
                        id: "enabled.sayHello".to_owned(),
                        title: "Say Hello".to_owned(),
                        category: Some("Enabled".to_owned()),
                    },
                    PluginCommandContribution {
                        id: "enabled.sayHello".to_owned(),
                        title: "Duplicate".to_owned(),
                        category: Some("Enabled".to_owned()),
                    },
                    PluginCommandContribution {
                        id: "enabled.open".to_owned(),
                        title: "Open".to_owned(),
                        category: None,
                    },
                ],
                ..PluginContributions::default()
            },
        },
    };
    let disabled = PluginDescriptor {
        root: PathBuf::from("plugins/disabled"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "disabled.plugin".to_owned(),
            name: "Disabled Plugin".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities::default(),
            contributes: PluginContributions {
                commands: vec![PluginCommandContribution {
                    id: "disabled.hidden".to_owned(),
                    title: "Hidden".to_owned(),
                    category: None,
                }],
                ..PluginContributions::default()
            },
        },
    };

    let registry = PluginCommandRegistry::from_plugins(&[enabled, disabled]);

    assert_eq!(registry.commands().len(), 2);
    assert_eq!(registry.commands()[0].label, "Enabled: Say Hello");
    assert_eq!(registry.commands()[1].label, "Enabled Plugin: Open");
    assert!(
        registry
            .command("enabled.plugin", "enabled.sayHello")
            .is_some()
    );
    assert!(
        registry
            .command("disabled.plugin", "disabled.hidden")
            .is_none()
    );
}

#[test]
fn plugin_command_registry_registers_workspace_read_commands() {
    let plugin = PluginDescriptor {
        root: PathBuf::from("plugins/workspace-read"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "workspace-read.plugin".to_owned(),
            name: "Workspace Read".to_owned(),
            version: "0.1.0".to_owned(),
            entry: Some(PathBuf::from("plugins/workspace-read/plugin.wasm")),
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                commands: true,
                workspace_read: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                commands: vec![PluginCommandContribution {
                    id: "workspace-read.run".to_owned(),
                    title: "Run".to_owned(),
                    category: None,
                }],
                ..PluginContributions::default()
            },
        },
    };

    let registry = PluginCommandRegistry::from_plugins(&[plugin]);

    assert!(!registry.is_empty());
    assert!(
        registry
            .command("workspace-read.plugin", "workspace-read.run")
            .is_some()
    );
}

#[test]
fn plugin_manifest_accepts_workspace_write_capability() {
    let root = temp_root("workspace-write-manifest");
    let manifest = parse_plugin_manifest_toml(
        &root,
        r#"
                id = "example.plugin"
                name = "Example"
                version = "0.1.0"
                entry = "bin/plugin.wasm"

                [capabilities]
                commands = true
                workspace_write = true

                [[contributes.commands]]
                id = "example.run"
                title = "Run"
            "#,
    )
    .unwrap();

    assert!(manifest.manifest.capabilities.workspace_write);
}

#[test]
fn plugin_command_registry_registers_workspace_write_commands() {
    let plugin = PluginDescriptor {
        root: PathBuf::from("plugins/workspace-write"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "workspace-write.plugin".to_owned(),
            name: "Workspace Write".to_owned(),
            version: "0.1.0".to_owned(),
            entry: Some(PathBuf::from("plugins/workspace-write/plugin.wasm")),
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                commands: true,
                workspace_write: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                commands: vec![PluginCommandContribution {
                    id: "workspace-write.run".to_owned(),
                    title: "Run".to_owned(),
                    category: None,
                }],
                ..PluginContributions::default()
            },
        },
    };

    let registry = PluginCommandRegistry::from_plugins(&[plugin]);

    assert!(!registry.is_empty());
    assert!(
        registry
            .command("workspace-write.plugin", "workspace-write.run")
            .is_some()
    );
}

#[test]
fn plugin_command_registry_skips_commands_with_unsupported_runtime_capabilities() {
    for (id, command_id, capabilities) in [
        (
            "process-spawn.plugin",
            "process-spawn.run",
            PluginCapabilities {
                commands: true,
                process_spawn: true,
                ..PluginCapabilities::default()
            },
        ),
        (
            "network.plugin",
            "network.run",
            PluginCapabilities {
                commands: true,
                network: true,
                ..PluginCapabilities::default()
            },
        ),
    ] {
        let plugin = PluginDescriptor {
            root: PathBuf::from(format!("plugins/{id}")),
            manifest: PluginManifest {
                api_version: PLUGIN_API_VERSION.to_owned(),
                id: id.to_owned(),
                name: "Unsupported".to_owned(),
                version: "0.1.0".to_owned(),
                entry: Some(PathBuf::from(format!("plugins/{id}/plugin.wasm"))),
                activation_events: Vec::new(),
                capabilities,
                contributes: PluginContributions {
                    commands: vec![PluginCommandContribution {
                        id: command_id.to_owned(),
                        title: "Run".to_owned(),
                        category: None,
                    }],
                    ..PluginContributions::default()
                },
            },
        };

        let registry = PluginCommandRegistry::from_plugins(&[plugin]);

        assert!(registry.is_empty(), "{id}");
        assert!(registry.command(id, command_id).is_none(), "{id}");
    }
}

#[test]
fn plugin_command_registry_bounds_combined_labels() {
    let plugin = PluginDescriptor {
        root: PathBuf::from("plugins/long-label"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "long-label.plugin".to_owned(),
            name: "N".repeat(MAX_PLUGIN_DISPLAY_LABEL_CHARS),
            version: "0.1.0".to_owned(),
            entry: Some(PathBuf::from("plugins/long-label/plugin.wasm")),
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                commands: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                commands: vec![PluginCommandContribution {
                    id: "long-label.run".to_owned(),
                    title: "T".repeat(MAX_PLUGIN_DISPLAY_LABEL_CHARS),
                    category: None,
                }],
                ..PluginContributions::default()
            },
        },
    };

    let registry = PluginCommandRegistry::from_plugins(&[plugin]);
    let label = &registry.commands()[0].label;

    assert_bounded_display_label(label);
    assert!(label.ends_with(DISPLAY_LABEL_OMISSION));
}

#[test]
fn plugin_manifest_rejects_unsupported_api_and_escaping_paths() {
    let root = temp_root("rejects");
    assert!(
        parse_plugin_manifest_toml(
            &root,
            r#"
                    api_version = "2"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"
                "#,
        )
        .is_err()
    );

    assert!(
        parse_plugin_manifest_toml(
            &root,
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"

                    [[contributes.themes]]
                    id = "example-dark"
                    label = "Example Dark"
                    path = "../escape.toml"
                "#,
        )
        .is_err()
    );

    assert!(
        parse_plugin_manifest_toml(
            &root,
            r#"
                    id = "example.plugin"
                    name = "Example"
                    version = "0.1.0"
                    activation_events = ["onCommand:bad command"]
                "#,
        )
        .is_err()
    );
}

#[test]
fn load_plugin_manifest_rejects_oversized_manifest_file() {
    let root = temp_root("oversized-manifest");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        plugin_manifest_path(&root),
        vec![b'a'; usize::try_from(MAX_PLUGIN_MANIFEST_BYTES + 1).unwrap()],
    )
    .unwrap();

    let error = load_plugin_manifest(&root).unwrap_err().to_string();

    assert!(error.contains("plugin file limit"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn workspace_plugin_discovery_is_sorted_and_manifest_only() {
    let workspace = temp_root("discovery");
    let plugins_dir = workspace_plugins_dir(&workspace);
    let a = plugins_dir.join("a-plugin");
    let b = plugins_dir.join("b-plugin");
    let ignored = plugins_dir.join("ignored");
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::create_dir_all(&ignored).unwrap();
    fs::write(
        plugin_manifest_path(&b),
        r#"
                id = "b.plugin"
                name = "B"
                version = "0.1.0"
            "#,
    )
    .unwrap();
    fs::write(
        plugin_manifest_path(&a),
        r#"
                id = "a.plugin"
                name = "A"
                version = "0.1.0"
            "#,
    )
    .unwrap();

    let discovery = discover_workspace_plugins(&workspace).unwrap();

    assert_eq!(
        discovery
            .plugins
            .iter()
            .map(|plugin| plugin.manifest.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a.plugin", "b.plugin"]
    );
    assert!(discovery.errors.is_empty());

    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn workspace_plugin_discovery_bounds_plugin_roots() {
    let workspace = temp_root("bounded-discovery");
    let plugins_dir = workspace_plugins_dir(&workspace);
    for (folder, id, name) in [
        ("c-plugin", "c.plugin", "C"),
        ("a-plugin", "a.plugin", "A"),
        ("b-plugin", "b.plugin", "B"),
    ] {
        let root = plugins_dir.join(folder);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            plugin_manifest_path(&root),
            format!(
                r#"
                    id = "{id}"
                    name = "{name}"
                    version = "0.1.0"
                    "#
            ),
        )
        .unwrap();
    }

    let discovery = discover_workspace_plugins_with_limits(&workspace, 2, 16).unwrap();

    assert_eq!(
        discovery
            .plugins
            .iter()
            .map(|plugin| plugin.manifest.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a.plugin", "b.plugin"]
    );
    assert_eq!(discovery.errors.len(), 1);
    assert!(discovery.errors[0].error.contains("limited to 2 plugins"));

    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn workspace_plugin_discovery_keeps_valid_plugins_when_one_manifest_fails() {
    let workspace = temp_root("partial-discovery");
    let plugins_dir = workspace_plugins_dir(&workspace);
    let valid = plugins_dir.join("valid");
    let invalid = plugins_dir.join("invalid");
    fs::create_dir_all(&valid).unwrap();
    fs::create_dir_all(&invalid).unwrap();
    fs::write(
        plugin_manifest_path(&valid),
        r#"
                id = "valid.plugin"
                name = "Valid"
                version = "0.1.0"
            "#,
    )
    .unwrap();
    fs::write(
        plugin_manifest_path(&invalid),
        r#"
                id = "invalid plugin"
                name = "Invalid"
                version = "0.1.0"
            "#,
    )
    .unwrap();

    let discovery = discover_workspace_plugins(&workspace).unwrap();

    assert_eq!(discovery.plugins.len(), 1);
    assert_eq!(discovery.plugins[0].manifest.id, "valid.plugin");
    assert_eq!(discovery.errors.len(), 1);
    assert_eq!(discovery.errors[0].root, invalid);

    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn workspace_plugin_discovery_reports_duplicate_plugin_ids() {
    let workspace = temp_root("duplicate-id-discovery");
    let plugins_dir = workspace_plugins_dir(&workspace);
    let first = plugins_dir.join("first");
    let duplicate = plugins_dir.join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&duplicate).unwrap();
    for root in [&first, &duplicate] {
        fs::write(
            plugin_manifest_path(root),
            r#"
                id = "same.plugin"
                name = "Same"
                version = "0.1.0"
            "#,
        )
        .unwrap();
    }

    let discovery = discover_workspace_plugins(&workspace).unwrap();

    assert_eq!(discovery.plugins.len(), 1);
    assert_eq!(discovery.plugins[0].manifest.id, "same.plugin");
    assert_eq!(discovery.errors.len(), 1);
    assert_eq!(discovery.errors[0].root, duplicate);
    assert!(discovery.errors[0].error.contains("duplicated"));
    // The error names the plugin that won, so the author can find the copy
    // to remove or rename.
    assert!(
        discovery.errors[0].error.contains(&path_string(&first)),
        "{}",
        discovery.errors[0].error
    );

    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn workspace_plugin_discovery_rejects_unknown_manifest_keys() {
    let workspace = temp_root("unknown-manifest-key");
    let plugins_dir = workspace_plugins_dir(&workspace);
    let root = plugins_dir.join("typo");
    fs::create_dir_all(&root).unwrap();
    // A typo'd capability key must not parse into an invisible no-op plugin.
    fs::write(
        plugin_manifest_path(&root),
        r#"
                id = "typo.plugin"
                name = "Typo"
                version = "0.1.0"
                capabilites = true
            "#,
    )
    .unwrap();

    let discovery = discover_workspace_plugins(&workspace).unwrap();

    assert!(discovery.plugins.is_empty());
    assert_eq!(discovery.errors.len(), 1);
    assert_eq!(discovery.errors[0].root, root);
    assert!(
        discovery.errors[0]
            .error
            .contains("unknown field `capabilites`"),
        "{}",
        discovery.errors[0].error
    );

    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn workspace_plugin_discovery_reports_missing_entry_file() {
    let workspace = temp_root("missing-entry-discovery");
    let plugins_dir = workspace_plugins_dir(&workspace);
    let missing = plugins_dir.join("missing-entry");
    let present = plugins_dir.join("present-entry");
    fs::create_dir_all(&missing).unwrap();
    fs::create_dir_all(&present).unwrap();
    for (root, id) in [(&missing, "missing.plugin"), (&present, "present.plugin")] {
        fs::write(
            plugin_manifest_path(root),
            format!(
                r#"
                    id = "{id}"
                    name = "Entry"
                    version = "0.1.0"
                    entry = "bin/plugin.wasm"
                "#
            ),
        )
        .unwrap();
    }
    // Only the second plugin gets the wasm its manifest promises.
    let entry = present.join("bin/plugin.wasm");
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(&entry, b"wasm").unwrap();

    let discovery = discover_workspace_plugins(&workspace).unwrap();

    assert_eq!(
        discovery
            .plugins
            .iter()
            .map(|plugin| plugin.manifest.id.as_str())
            .collect::<Vec<_>>(),
        vec!["present.plugin"]
    );
    assert_eq!(discovery.errors.len(), 1);
    assert_eq!(discovery.errors[0].root, missing);
    assert!(
        discovery.errors[0].error.contains("plugin entry")
            && discovery.errors[0].error.contains("is missing"),
        "{}",
        discovery.errors[0].error
    );

    fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn plugin_language_registry_uses_declared_capability_and_first_provider() {
    let first = PluginDescriptor {
        root: PathBuf::from("plugins/first"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "first.plugin".to_owned(),
            name: "First".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                languages: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                languages: vec![PluginLanguageContribution {
                    id: "first-lang".to_owned(),
                    extensions: vec!["one".to_owned(), "shared".to_owned()],
                    aliases: vec!["First Language".to_owned()],
                }],
                ..PluginContributions::default()
            },
        },
    };
    let second = PluginDescriptor {
        root: PathBuf::from("plugins/second"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "second.plugin".to_owned(),
            name: "Second".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                languages: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                languages: vec![PluginLanguageContribution {
                    id: "second-lang".to_owned(),
                    extensions: vec!["shared".to_owned(), "two".to_owned()],
                    aliases: vec!["Second Language".to_owned()],
                }],
                ..PluginContributions::default()
            },
        },
    };
    let disabled = PluginDescriptor {
        root: PathBuf::from("plugins/disabled"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "disabled.plugin".to_owned(),
            name: "Disabled".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities::default(),
            contributes: PluginContributions {
                languages: vec![PluginLanguageContribution {
                    id: "disabled-lang".to_owned(),
                    extensions: vec!["disabled".to_owned()],
                    aliases: Vec::new(),
                }],
                ..PluginContributions::default()
            },
        },
    };

    let registry = PluginLanguageRegistry::from_plugins(&[first, second, disabled]);

    let one = registry
        .language_for_path(Path::new("src/main.one"))
        .expect("language should be registered");
    assert_eq!(one.plugin_id, "first.plugin");
    assert_eq!(one.display_name(), "First Language");
    assert_eq!(
        registry
            .language_for_path(Path::new("src/main.shared"))
            .map(|language| language.plugin_id.as_str()),
        Some("first.plugin")
    );
    let two = registry
        .language_for_path(Path::new("src/main.two"))
        .expect("second language should keep its new extension");
    assert_eq!(two.plugin_id, "second.plugin");
    assert_eq!(two.extensions, vec!["two"]);
    assert!(
        registry
            .language_for_path(Path::new("src/main.disabled"))
            .is_none()
    );
    assert!(
        registry
            .language_for_path(Path::new("src/main.rs"))
            .is_none()
    );
}

#[test]
fn plugin_theme_registry_uses_declared_capability_and_first_provider() {
    let first = PluginDescriptor {
        root: PathBuf::from("plugins/first"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "first.plugin".to_owned(),
            name: "First".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                themes: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                themes: vec![PluginThemeContribution {
                    id: "shared-theme".to_owned(),
                    label: "First Theme".to_owned(),
                    path: PathBuf::from("plugins/first/themes/first.toml"),
                }],
                ..PluginContributions::default()
            },
        },
    };
    let second = PluginDescriptor {
        root: PathBuf::from("plugins/second"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "second.plugin".to_owned(),
            name: "Second".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                themes: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                themes: vec![PluginThemeContribution {
                    id: "shared-theme".to_owned(),
                    label: "Second Theme".to_owned(),
                    path: PathBuf::from("plugins/second/themes/second.toml"),
                }],
                ..PluginContributions::default()
            },
        },
    };
    let disabled = PluginDescriptor {
        root: PathBuf::from("plugins/disabled"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "disabled.plugin".to_owned(),
            name: "Disabled".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities::default(),
            contributes: PluginContributions {
                themes: vec![PluginThemeContribution {
                    id: "disabled-theme".to_owned(),
                    label: "Disabled Theme".to_owned(),
                    path: PathBuf::from("plugins/disabled/themes/disabled.toml"),
                }],
                ..PluginContributions::default()
            },
        },
    };

    let registry = PluginThemeRegistry::from_plugins(&[first, second, disabled]);

    assert_eq!(registry.themes().len(), 1);
    let theme = registry
        .theme("shared-theme")
        .expect("theme should be registered");
    assert_eq!(theme.plugin_id, "first.plugin");
    assert_eq!(theme.label, "First Theme");
    assert!(registry.theme("disabled-theme").is_none());
}

#[test]
fn load_theme_settings_from_path_preserves_file_name() {
    let root = temp_root("custom-theme-file");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("custom.toml");
    fs::write(
        &path,
        r#"
                name = "Custom Night"
                background = [1, 2, 3]
                panel = [4, 5, 6]
                panel_alt = [7, 8, 9]
                text = [10, 11, 12]
                muted_text = [13, 14, 15]
                accent = [16, 17, 18]
                warning = [19, 20, 21]
                error = [22, 23, 24]
            "#,
    )
    .unwrap();

    let theme = load_theme_settings_from_path(&path).unwrap();

    assert_eq!(theme.name, "Custom Night");
    assert_eq!(theme.background, [1, 2, 3]);
    assert_eq!(theme.error, [22, 23, 24]);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_parses_friendly_palette_hex_colors() {
    let root = temp_root("friendly-theme-palette");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("friendly.toml");
    fs::write(
        &path,
        r##"
                name = "Friendly Night"

                [palette]
                background = "#010203"
                panel = "#112233"
                panel_alt = "#abc"
                text = "#AABBCC"
                muted_text = "#000"
                accent = "#5B8DEF"
                selection = "#102a56"
                warning = [19, 20, 21]
                error = "#e86262"
            "##,
    )
    .unwrap();

    let theme = load_theme_settings_from_path(&path).unwrap();

    assert_eq!(theme.name, "Friendly Night");
    assert_eq!(theme.background, [1, 2, 3]);
    assert_eq!(theme.panel_alt, [170, 187, 204]);
    assert_eq!(theme.text, [170, 187, 204]);
    assert_eq!(theme.muted_text, [0, 0, 0]);
    assert_eq!(theme.accent, [91, 141, 239]);
    assert_eq!(theme.selection, Some([16, 42, 86]));
    assert_eq!(theme.warning, [19, 20, 21]);
    assert_eq!(theme.error, [232, 98, 98]);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_accepts_colors_alias() {
    let root = temp_root("friendly-theme-colors");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("friendly.toml");
    fs::write(
        &path,
        r##"
                name = "Alias Night"

                [colors]
                background = "#102030"
                accent = "#123456"
                error = [3, 2, 1]
            "##,
    )
    .unwrap();

    let theme = load_theme_settings_from_path(&path).unwrap();

    assert_eq!(theme.name, "Alias Night");
    assert_eq!(theme.background, [16, 32, 48]);
    assert_eq!(theme.accent, [18, 52, 86]);
    assert_eq!(theme.error, [3, 2, 1]);
    assert_eq!(theme.panel, ThemeSettings::default().panel);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_caps_friendly_theme_name_length() {
    let root = temp_root("friendly-theme-overlong-name");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("friendly.toml");
    let overlong_name = "N".repeat(MAX_PLUGIN_DISPLAY_LABEL_CHARS + 80);
    fs::write(
        &path,
        format!(
            r##"
                name = "{overlong_name}"

                [palette]
                background = "#010203"
                accent = "#5B8DEF"
            "##
        ),
    )
    .unwrap();

    let theme = load_theme_settings_from_path(&path).unwrap();

    assert_eq!(theme.name.chars().count(), MAX_PLUGIN_DISPLAY_LABEL_CHARS);
    assert!(theme.name.starts_with("NNN"));
    assert!(theme.name.ends_with(DISPLAY_LABEL_OMISSION));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_strips_control_chars_from_friendly_theme_name() {
    let root = temp_root("friendly-theme-hostile-name");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("friendly.toml");
    // TOML escapes keep the invisible characters out of this source file:
    // U+200B (zero-width space), a tab, and U+202E (right-to-left override).
    fs::write(
        &path,
        r##"
                name = "Night\u200B\tOwl\u202E"

                [palette]
                background = "#010203"
                accent = "#5B8DEF"
            "##,
    )
    .unwrap();

    let theme = load_theme_settings_from_path(&path).unwrap();

    assert_eq!(theme.name, "Night Owl");
    assert!(!theme.name.contains('\u{200b}'));
    assert!(!theme.name.contains('\u{202e}'));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_keeps_default_name_when_friendly_name_sanitizes_empty() {
    let root = temp_root("friendly-theme-invisible-name");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("friendly.toml");
    fs::write(
        &path,
        r##"
                name = "\u200B\t\u202E"

                [palette]
                background = "#010203"
                accent = "#5B8DEF"
            "##,
    )
    .unwrap();

    let theme = load_theme_settings_from_path(&path).unwrap();

    assert_eq!(theme.name, ThemeSettings::default().name);
    assert_eq!(theme.background, [1, 2, 3]);
    assert_eq!(theme.accent, [91, 141, 239]);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_rejects_invalid_friendly_color_value() {
    let root = temp_root("friendly-theme-invalid-color");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("invalid.toml");
    fs::write(
        &path,
        r#"
                name = "Broken Night"

                [palette]
                accent = "blue"
            "#,
    )
    .unwrap();

    let error = load_theme_settings_from_path(&path)
        .unwrap_err()
        .to_string();

    assert!(error.contains("theme color accent"), "{error}");
    assert!(error.contains("#RRGGBB"), "{error}");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_keeps_raw_theme_settings_format() {
    let root = temp_root("raw-theme-compatibility");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("raw.toml");
    fs::write(
        &path,
        r#"
                name = "Raw Night"
                background = [1, 2, 3]
                panel = [4, 5, 6]
                panel_alt = [7, 8, 9]
                text = [10, 11, 12]
                muted_text = [13, 14, 15]
                accent = [16, 17, 18]
                selection = [25, 26, 27]
                warning = [19, 20, 21]
                error = [22, 23, 24]
            "#,
    )
    .unwrap();

    let theme = load_theme_settings_from_path(&path).unwrap();

    assert_eq!(theme.name, "Raw Night");
    assert_eq!(theme.background, [1, 2, 3]);
    assert_eq!(theme.selection, Some([25, 26, 27]));
    assert_eq!(theme.error, [22, 23, 24]);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_plugin_theme_settings_uses_contribution_label() {
    let root = temp_root("theme-file");
    let theme_dir = root.join("themes");
    fs::create_dir_all(&theme_dir).unwrap();
    let path = theme_dir.join("dark.toml");
    fs::write(
        &path,
        r#"
                background = [1, 2, 3]
                panel = [4, 5, 6]
                panel_alt = [7, 8, 9]
                text = [10, 11, 12]
                muted_text = [13, 14, 15]
                accent = [16, 17, 18]
                warning = [19, 20, 21]
                error = [22, 23, 24]
            "#,
    )
    .unwrap();
    let registration = PluginThemeRegistration {
        plugin_id: "example.plugin".to_owned(),
        theme_id: "example-dark".to_owned(),
        label: "Example Dark".to_owned(),
        path,
    };

    let theme = load_plugin_theme_settings(&registration).unwrap();

    assert_eq!(theme.name, "Example Dark");
    assert_eq!(theme.background, [1, 2, 3]);
    assert_eq!(theme.error, [22, 23, 24]);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_rejects_oversized_theme_file() {
    let root = temp_root("oversized-custom-theme");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("huge.toml");
    fs::write(
        &path,
        vec![b'a'; usize::try_from(MAX_PLUGIN_THEME_BYTES + 1).unwrap()],
    )
    .unwrap();

    let error = load_theme_settings_from_path(&path)
        .unwrap_err()
        .to_string();

    assert!(error.contains("plugin file limit"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_reports_unreadable_path() {
    let root = temp_root("missing-custom-theme");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("missing.toml");

    let error = load_theme_settings_from_path(&path)
        .unwrap_err()
        .to_string();

    assert!(error.contains("could not read"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_theme_settings_from_path_reports_parse_errors() {
    let root = temp_root("invalid-custom-theme");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("invalid.toml");
    fs::write(&path, "name = [").unwrap();

    let error = load_theme_settings_from_path(&path)
        .unwrap_err()
        .to_string();

    assert!(error.contains("could not parse"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_plugin_theme_settings_rejects_oversized_theme_file() {
    let root = temp_root("oversized-theme");
    let theme_dir = root.join("themes");
    fs::create_dir_all(&theme_dir).unwrap();
    let path = theme_dir.join("huge.toml");
    fs::write(
        &path,
        vec![b'a'; usize::try_from(MAX_PLUGIN_THEME_BYTES + 1).unwrap()],
    )
    .unwrap();
    let registration = PluginThemeRegistration {
        plugin_id: "example.plugin".to_owned(),
        theme_id: "example-huge".to_owned(),
        label: "Example Huge".to_owned(),
        path,
    };

    let error = load_plugin_theme_settings(&registration)
        .unwrap_err()
        .to_string();

    assert!(error.contains("plugin file limit"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn plugin_syntax_registry_uses_declared_capability_and_first_provider() {
    let first = PluginDescriptor {
        root: PathBuf::from("plugins/first"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "first.plugin".to_owned(),
            name: "First".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                languages: true,
                syntax: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                languages: vec![PluginLanguageContribution {
                    id: "shared-lang".to_owned(),
                    extensions: vec!["one".to_owned()],
                    aliases: Vec::new(),
                }],
                syntaxes: vec![PluginSyntaxContribution {
                    language: "shared-lang".to_owned(),
                    path: PathBuf::from("plugins/first/syntax/first.sublime-syntax"),
                }],
                ..PluginContributions::default()
            },
        },
    };
    let second = PluginDescriptor {
        root: PathBuf::from("plugins/second"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "second.plugin".to_owned(),
            name: "Second".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                syntax: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                syntaxes: vec![PluginSyntaxContribution {
                    language: "shared-lang".to_owned(),
                    path: PathBuf::from("plugins/second/syntax/second.sublime-syntax"),
                }],
                ..PluginContributions::default()
            },
        },
    };
    let disabled = PluginDescriptor {
        root: PathBuf::from("plugins/disabled"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "disabled.plugin".to_owned(),
            name: "Disabled".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities::default(),
            contributes: PluginContributions {
                syntaxes: vec![PluginSyntaxContribution {
                    language: "disabled-lang".to_owned(),
                    path: PathBuf::from("plugins/disabled/syntax/disabled.sublime-syntax"),
                }],
                ..PluginContributions::default()
            },
        },
    };

    let registry = PluginSyntaxRegistry::from_plugins(&[first, second, disabled]);

    assert_eq!(registry.syntaxes().len(), 1);
    let syntax = registry
        .syntax_for_language("shared-lang")
        .expect("syntax should be registered");
    assert_eq!(syntax.plugin_id, "first.plugin");
    assert_eq!(
        syntax.path,
        PathBuf::from("plugins/first/syntax/first.sublime-syntax")
    );
    assert_eq!(syntax.extensions, vec!["one"]);
    assert!(registry.syntax_for_language("disabled-lang").is_none());
}

#[test]
fn plugin_contribution_registries_ignore_duplicate_plugin_ids() {
    let first = PluginDescriptor {
        root: PathBuf::from("plugins/first"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "same.plugin".to_owned(),
            name: "First".to_owned(),
            version: "0.1.0".to_owned(),
            entry: Some(PathBuf::from("plugins/first/plugin.wasm")),
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                commands: true,
                languages: true,
                themes: true,
                syntax: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                commands: vec![PluginCommandContribution {
                    id: "same.first".to_owned(),
                    title: "First Command".to_owned(),
                    category: None,
                }],
                languages: vec![PluginLanguageContribution {
                    id: "first-lang".to_owned(),
                    extensions: vec!["first".to_owned()],
                    aliases: Vec::new(),
                }],
                themes: vec![PluginThemeContribution {
                    id: "first-theme".to_owned(),
                    label: "First Theme".to_owned(),
                    path: PathBuf::from("plugins/first/themes/first.toml"),
                }],
                syntaxes: vec![PluginSyntaxContribution {
                    language: "first-lang".to_owned(),
                    path: PathBuf::from("plugins/first/syntax/first.sublime-syntax"),
                }],
            },
        },
    };
    let duplicate = PluginDescriptor {
        root: PathBuf::from("plugins/duplicate"),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "same.plugin".to_owned(),
            name: "Duplicate".to_owned(),
            version: "9.9.9".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                commands: true,
                languages: true,
                themes: true,
                syntax: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                commands: vec![PluginCommandContribution {
                    id: "same.duplicate".to_owned(),
                    title: "Duplicate Command".to_owned(),
                    category: None,
                }],
                languages: vec![PluginLanguageContribution {
                    id: "duplicate-lang".to_owned(),
                    extensions: vec!["duplicate".to_owned()],
                    aliases: Vec::new(),
                }],
                themes: vec![PluginThemeContribution {
                    id: "duplicate-theme".to_owned(),
                    label: "Duplicate Theme".to_owned(),
                    path: PathBuf::from("plugins/duplicate/themes/duplicate.toml"),
                }],
                syntaxes: vec![PluginSyntaxContribution {
                    language: "duplicate-lang".to_owned(),
                    path: PathBuf::from("plugins/duplicate/syntax/duplicate.sublime-syntax"),
                }],
            },
        },
    };
    let plugins = [first, duplicate];

    let commands = PluginCommandRegistry::from_plugins(&plugins);
    assert_eq!(commands.len(), 1);
    assert!(commands.command("same.plugin", "same.first").is_some());
    assert!(commands.command("same.plugin", "same.duplicate").is_none());

    let languages = PluginLanguageRegistry::from_plugins(&plugins);
    assert_eq!(languages.len(), 1);
    assert!(
        languages
            .language_for_path(Path::new("src/main.first"))
            .is_some()
    );
    assert!(
        languages
            .language_for_path(Path::new("src/main.duplicate"))
            .is_none()
    );

    let themes = PluginThemeRegistry::from_plugins(&plugins);
    assert_eq!(themes.len(), 1);
    assert!(themes.theme("first-theme").is_some());
    assert!(themes.theme("duplicate-theme").is_none());

    let syntaxes = PluginSyntaxRegistry::from_plugins(&plugins);
    assert_eq!(syntaxes.len(), 1);
    assert!(syntaxes.syntax_for_language("first-lang").is_some());
    assert!(syntaxes.syntax_for_language("duplicate-lang").is_none());
}

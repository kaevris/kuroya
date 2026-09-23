use anyhow::{Context, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use crate::{settings::ThemeSettings, workspace_paths::normalize_child_path};

pub const PLUGIN_API_VERSION: &str = "1";
pub const MAX_PLUGIN_MANIFEST_BYTES: u64 = 128 * 1024;
pub const MAX_PLUGIN_THEME_BYTES: u64 = 256 * 1024;
pub const MAX_PLUGIN_SYNTAX_BYTES: u64 = 1024 * 1024;
pub const MAX_WORKSPACE_PLUGIN_ROOTS: usize = 128;
pub const MAX_WORKSPACE_PLUGIN_DIRECTORY_ENTRIES: usize = 4_096;
const MAX_PLUGIN_DISPLAY_LABEL_CHARS: usize = 120;
const MAX_PLUGIN_IDENTIFIER_CHARS: usize = 128;
const MAX_PLUGIN_VERSION_CHARS: usize = 128;
const MAX_PLUGIN_ACTIVATION_EVENTS: usize = 128;
const MAX_PLUGIN_COMMAND_CONTRIBUTIONS: usize = 256;
const MAX_PLUGIN_LANGUAGE_CONTRIBUTIONS: usize = 128;
const MAX_PLUGIN_LANGUAGE_EXTENSIONS: usize = 128;
const MAX_PLUGIN_LANGUAGE_EXTENSION_CHARS: usize = 64;
const MAX_PLUGIN_LANGUAGE_ALIASES: usize = 128;
const MAX_PLUGIN_THEME_CONTRIBUTIONS: usize = 128;
const MAX_PLUGIN_SYNTAX_CONTRIBUTIONS: usize = 128;
const DISPLAY_LABEL_OMISSION: &str = "...";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDescriptor {
    pub root: PathBuf,
    pub manifest: PluginManifest,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginDiscovery {
    pub plugins: Vec<PluginDescriptor>,
    pub errors: Vec<PluginDiscoveryError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginDiscoveryError {
    pub root: PathBuf,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginActivationState {
    active: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginActivationRecord {
    pub plugin_id: String,
    pub name: String,
    pub entry: Option<PathBuf>,
    pub trigger: PluginActivationTrigger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginActivationTrigger {
    Startup,
    Command(String),
    Language(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginRuntimeRegistry {
    plugins: Vec<PluginRuntimeRegistration>,
    by_id: BTreeMap<String, usize>,
    by_command: BTreeMap<String, Vec<usize>>,
    by_language: BTreeMap<String, Vec<usize>>,
    startup: Vec<usize>,
    any: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRuntimeRegistration {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub root: PathBuf,
    pub entry: Option<PathBuf>,
    pub activation_events: Vec<PluginActivationEvent>,
    pub capabilities: PluginCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PluginActivationEvent {
    OnCommand(String),
    OnLanguage(String),
    OnStartupFinished,
    Any,
}

impl PluginActivationState {
    pub fn activate_startup(
        &mut self,
        registry: &PluginRuntimeRegistry,
    ) -> Vec<PluginActivationRecord> {
        self.activate_plugins(registry.startup_plugin_iter(), || {
            PluginActivationTrigger::Startup
        })
    }

    pub fn activate_command(
        &mut self,
        registry: &PluginRuntimeRegistry,
        command_id: &str,
    ) -> Vec<PluginActivationRecord> {
        self.activate_plugins(registry.command_plugin_iter(command_id), || {
            PluginActivationTrigger::Command(command_id.to_owned())
        })
    }

    pub fn activate_plugin_command(
        &mut self,
        registry: &PluginRuntimeRegistry,
        plugin_id: &str,
        command_id: &str,
    ) -> Vec<PluginActivationRecord> {
        self.activate_plugins(
            registry
                .plugin_command_activation_plugins(plugin_id, command_id)
                .into_iter(),
            || PluginActivationTrigger::Command(command_id.to_owned()),
        )
    }

    pub fn activate_language(
        &mut self,
        registry: &PluginRuntimeRegistry,
        language_id: &str,
    ) -> Vec<PluginActivationRecord> {
        self.activate_plugins(registry.language_plugin_iter(language_id), || {
            PluginActivationTrigger::Language(language_id.to_owned())
        })
    }

    pub fn is_active(&self, plugin_id: &str) -> bool {
        self.active.contains(plugin_id)
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }

    fn activate_plugins<'a>(
        &mut self,
        plugins: impl Iterator<Item = &'a PluginRuntimeRegistration>,
        mut trigger: impl FnMut() -> PluginActivationTrigger,
    ) -> Vec<PluginActivationRecord> {
        let (lower, upper) = plugins.size_hint();
        let mut records = Vec::with_capacity(upper.unwrap_or(lower));
        for plugin in plugins {
            if self.active.contains(&plugin.plugin_id) {
                continue;
            }
            self.active.insert(plugin.plugin_id.clone());
            records.push(PluginActivationRecord {
                plugin_id: plugin.plugin_id.clone(),
                name: plugin.name.clone(),
                entry: plugin.entry.clone(),
                trigger: trigger(),
            });
        }
        records
    }
}

impl PluginRuntimeRegistry {
    pub fn from_plugins(plugins: &[PluginDescriptor]) -> Self {
        let mut registry = Self::default();
        registry.plugins.reserve(plugins.len());
        registry.startup.reserve(plugins.len());
        registry.any.reserve(plugins.len());
        for plugin in plugins {
            if registry.by_id.contains_key(&plugin.manifest.id) {
                continue;
            }
            let registration = PluginRuntimeRegistration {
                plugin_id: plugin.manifest.id.clone(),
                name: plugin.manifest.name.clone(),
                version: plugin.manifest.version.clone(),
                root: plugin.root.clone(),
                entry: plugin.manifest.entry.clone(),
                activation_events: runtime_activation_events(plugin),
                capabilities: plugin.manifest.capabilities.clone(),
            };
            registry
                .by_id
                .insert(registration.plugin_id.clone(), registry.plugins.len());
            registry.plugins.push(registration);
            let index = registry.plugins.len() - 1;
            registry.index_runtime_activation_events(index);
        }
        registry
    }

    pub fn plugins(&self) -> &[PluginRuntimeRegistration] {
        &self.plugins
    }

    pub fn plugin(&self, plugin_id: &str) -> Option<&PluginRuntimeRegistration> {
        self.by_id
            .get(plugin_id)
            .and_then(|index| self.plugins.get(*index))
    }

    pub fn plugins_for_command(&self, command_id: &str) -> Vec<&PluginRuntimeRegistration> {
        self.command_plugin_iter(command_id).collect()
    }

    pub fn plugins_for_language(&self, language_id: &str) -> Vec<&PluginRuntimeRegistration> {
        self.language_plugin_iter(language_id).collect()
    }

    pub fn startup_plugins(&self) -> Vec<&PluginRuntimeRegistration> {
        self.startup_plugin_iter().collect()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    fn index_runtime_activation_events(&mut self, index: usize) {
        let Some(plugin) = self.plugins.get(index) else {
            return;
        };
        for event in &plugin.activation_events {
            match event {
                PluginActivationEvent::OnCommand(command) => {
                    self.by_command
                        .entry(command.clone())
                        .or_default()
                        .push(index);
                }
                PluginActivationEvent::OnLanguage(language) => {
                    self.by_language
                        .entry(language.clone())
                        .or_default()
                        .push(index);
                }
                PluginActivationEvent::OnStartupFinished => {
                    self.startup.push(index);
                }
                PluginActivationEvent::Any => {
                    self.any.push(index);
                }
            }
        }
    }

    fn command_plugin_iter<'a>(
        &'a self,
        command_id: &str,
    ) -> impl Iterator<Item = &'a PluginRuntimeRegistration> + 'a {
        let indexes = self
            .by_command
            .get(command_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        self.activation_plugin_iter(indexes)
    }

    fn language_plugin_iter<'a>(
        &'a self,
        language_id: &str,
    ) -> impl Iterator<Item = &'a PluginRuntimeRegistration> + 'a {
        let indexes = self
            .by_language
            .get(language_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        self.activation_plugin_iter(indexes)
    }

    fn startup_plugin_iter(&self) -> impl Iterator<Item = &PluginRuntimeRegistration> {
        self.activation_plugin_iter(&self.startup)
    }

    fn plugin_command_activation_plugins<'a>(
        &'a self,
        plugin_id: &str,
        command_id: &str,
    ) -> Vec<&'a PluginRuntimeRegistration> {
        let mut emitted = BTreeSet::new();
        let mut plugins = Vec::new();

        if let Some(index) = self.by_id.get(plugin_id)
            && let Some(plugin) = self.plugins.get(*index)
            && plugin.activates_on_command(command_id)
            && emitted.insert(*index)
        {
            plugins.push(plugin);
        }

        for index in &self.any {
            if emitted.insert(*index)
                && let Some(plugin) = self.plugins.get(*index)
            {
                plugins.push(plugin);
            }
        }

        plugins
    }

    fn activation_plugin_iter<'a>(
        &'a self,
        indexes: &'a [usize],
    ) -> impl Iterator<Item = &'a PluginRuntimeRegistration> + 'a {
        let mut emitted = BTreeSet::new();
        indexes
            .iter()
            .chain(self.any.iter())
            .filter_map(move |index| {
                if emitted.insert(*index) {
                    self.plugins.get(*index)
                } else {
                    None
                }
            })
    }
}

impl PluginRuntimeRegistration {
    pub fn command_entry(&self) -> Option<&Path> {
        if self.capabilities.commands {
            self.entry.as_deref()
        } else {
            None
        }
    }

    pub fn activates_on_command(&self, command_id: &str) -> bool {
        self.activation_events
            .iter()
            .any(|event| event.activates_on_command(command_id))
    }

    pub fn activates_on_language(&self, language_id: &str) -> bool {
        self.activation_events
            .iter()
            .any(|event| event.activates_on_language(language_id))
    }

    pub fn activates_on_startup(&self) -> bool {
        self.activation_events
            .iter()
            .any(PluginActivationEvent::activates_on_startup)
    }
}

impl PluginActivationEvent {
    pub fn manifest_string(&self) -> String {
        match self {
            Self::OnCommand(command) => format!("onCommand:{command}"),
            Self::OnLanguage(language) => format!("onLanguage:{language}"),
            Self::OnStartupFinished => "onStartupFinished".to_owned(),
            Self::Any => "*".to_owned(),
        }
    }

    pub fn activates_on_command(&self, command_id: &str) -> bool {
        matches!(self, Self::Any)
            || matches!(self, Self::OnCommand(command) if command == command_id)
    }

    pub fn activates_on_language(&self, language_id: &str) -> bool {
        matches!(self, Self::Any)
            || matches!(self, Self::OnLanguage(language) if language == language_id)
    }

    pub fn activates_on_startup(&self) -> bool {
        matches!(self, Self::Any | Self::OnStartupFinished)
    }
}

impl Serialize for PluginActivationEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.manifest_string())
    }
}

impl<'de> Deserialize<'de> for PluginActivationEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_plugin_activation_event(&value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginLanguageRegistry {
    languages: Vec<PluginLanguageRegistration>,
    by_extension: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginCommandRegistry {
    commands: Vec<PluginCommandRegistration>,
    by_plugin_command: BTreeMap<String, BTreeMap<String, usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCommandRegistration {
    pub plugin_id: String,
    pub command_id: String,
    pub title: String,
    pub category: Option<String>,
    pub label: String,
}

impl PluginCommandRegistry {
    pub fn from_plugins(plugins: &[PluginDescriptor]) -> Self {
        let command_count = plugins
            .iter()
            .filter(|plugin| plugin.manifest.capabilities.commands)
            .map(|plugin| plugin.manifest.contributes.commands.len())
            .sum();
        let mut registry = Self {
            commands: Vec::with_capacity(command_count),
            ..Self::default()
        };
        let mut seen_plugin_ids = BTreeSet::new();
        for plugin in plugins {
            if !seen_plugin_ids.insert(plugin.manifest.id.as_str()) {
                continue;
            }
            if !plugin_command_contributions_are_runnable(plugin) {
                continue;
            }

            for command in &plugin.manifest.contributes.commands {
                if registry.command(&plugin.manifest.id, &command.id).is_some() {
                    continue;
                }

                let registration = PluginCommandRegistration {
                    plugin_id: plugin.manifest.id.clone(),
                    command_id: command.id.clone(),
                    title: command.title.clone(),
                    category: command.category.clone(),
                    label: plugin_command_label(&plugin.manifest, command),
                };
                registry
                    .by_plugin_command
                    .entry(registration.plugin_id.clone())
                    .or_default()
                    .insert(registration.command_id.clone(), registry.commands.len());
                registry.commands.push(registration);
            }
        }
        registry
    }

    pub fn commands(&self) -> &[PluginCommandRegistration] {
        &self.commands
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn command(&self, plugin_id: &str, command_id: &str) -> Option<&PluginCommandRegistration> {
        self.by_plugin_command
            .get(plugin_id)
            .and_then(|commands| commands.get(command_id))
            .and_then(|index| self.commands.get(*index))
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

fn plugin_command_contributions_are_runnable(plugin: &PluginDescriptor) -> bool {
    plugin.manifest.capabilities.commands
        && plugin.manifest.entry.is_some()
        && plugin_command_runtime_capabilities_are_supported(&plugin.manifest.capabilities)
}

fn plugin_command_runtime_capabilities_are_supported(capabilities: &PluginCapabilities) -> bool {
    !capabilities.process_spawn && !capabilities.network
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLanguageRegistration {
    pub plugin_id: String,
    pub language_id: String,
    pub aliases: Vec<String>,
    pub extensions: Vec<String>,
}

impl PluginLanguageRegistry {
    pub fn from_plugins(plugins: &[PluginDescriptor]) -> Self {
        let language_count = plugins
            .iter()
            .filter(|plugin| plugin.manifest.capabilities.languages)
            .map(|plugin| plugin.manifest.contributes.languages.len())
            .sum();
        let mut registry = Self {
            languages: Vec::with_capacity(language_count),
            ..Self::default()
        };
        let mut seen_plugin_ids = BTreeSet::new();
        for plugin in plugins {
            if !seen_plugin_ids.insert(plugin.manifest.id.as_str()) {
                continue;
            }
            if !plugin.manifest.capabilities.languages {
                continue;
            }

            for language in &plugin.manifest.contributes.languages {
                let index = registry.languages.len();
                let mut registered_extensions = Vec::with_capacity(language.extensions.len());
                for extension in &language.extensions {
                    if !registry.by_extension.contains_key(extension) {
                        registry.by_extension.insert(extension.clone(), index);
                        registered_extensions.push(extension.clone());
                    }
                }
                if !registered_extensions.is_empty() {
                    registry.languages.push(PluginLanguageRegistration {
                        plugin_id: plugin.manifest.id.clone(),
                        language_id: language.id.clone(),
                        aliases: language.aliases.clone(),
                        extensions: registered_extensions,
                    });
                }
            }
        }
        registry
    }

    pub fn language_for_path(&self, path: &Path) -> Option<&PluginLanguageRegistration> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())?
            .trim_start_matches('.');
        let index = if extension
            .as_bytes()
            .iter()
            .any(|byte| byte.is_ascii_uppercase())
        {
            self.by_extension.get(&extension.to_ascii_lowercase())
        } else {
            self.by_extension.get(extension)
        }?;
        self.languages.get(*index)
    }

    pub fn len(&self) -> usize {
        self.by_extension.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_extension.is_empty()
    }
}

impl PluginLanguageRegistration {
    pub fn display_name(&self) -> &str {
        self.aliases
            .first()
            .map(String::as_str)
            .unwrap_or(&self.language_id)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginThemeRegistry {
    themes: Vec<PluginThemeRegistration>,
    by_id: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginThemeRegistration {
    pub plugin_id: String,
    pub theme_id: String,
    pub label: String,
    pub path: PathBuf,
}

impl PluginThemeRegistry {
    pub fn from_plugins(plugins: &[PluginDescriptor]) -> Self {
        let theme_count = plugins
            .iter()
            .filter(|plugin| plugin.manifest.capabilities.themes)
            .map(|plugin| plugin.manifest.contributes.themes.len())
            .sum();
        let mut registry = Self {
            themes: Vec::with_capacity(theme_count),
            ..Self::default()
        };
        let mut seen_plugin_ids = BTreeSet::new();
        for plugin in plugins {
            if !seen_plugin_ids.insert(plugin.manifest.id.as_str()) {
                continue;
            }
            if !plugin.manifest.capabilities.themes {
                continue;
            }

            for theme in &plugin.manifest.contributes.themes {
                if registry.by_id.contains_key(&theme.id) {
                    continue;
                }

                let registration = PluginThemeRegistration {
                    plugin_id: plugin.manifest.id.clone(),
                    theme_id: theme.id.clone(),
                    label: theme.label.clone(),
                    path: theme.path.clone(),
                };
                registry
                    .by_id
                    .insert(registration.theme_id.clone(), registry.themes.len());
                registry.themes.push(registration);
            }
        }
        registry
    }

    pub fn themes(&self) -> &[PluginThemeRegistration] {
        &self.themes
    }

    pub fn len(&self) -> usize {
        self.themes.len()
    }

    pub fn theme(&self, theme_id: &str) -> Option<&PluginThemeRegistration> {
        self.by_id
            .get(theme_id)
            .and_then(|index| self.themes.get(*index))
    }

    pub fn is_empty(&self) -> bool {
        self.themes.is_empty()
    }
}

pub fn load_plugin_theme_settings(
    registration: &PluginThemeRegistration,
) -> anyhow::Result<ThemeSettings> {
    let mut theme = load_theme_settings_from_path(&registration.path)?;
    theme.name = registration.label.clone();
    Ok(theme)
}

pub fn load_theme_settings_from_path(path: &Path) -> anyhow::Result<ThemeSettings> {
    let text = read_plugin_text_file_with_limit(path, MAX_PLUGIN_THEME_BYTES)?;
    parse_theme_settings_toml(path, &text)
}

fn parse_theme_settings_toml(path: &Path, text: &str) -> anyhow::Result<ThemeSettings> {
    let value: toml::Value =
        toml::from_str(text).with_context(|| format!("could not parse {}", path.display()))?;
    if root_theme_color_table_name(&value).is_some() {
        parse_friendly_theme_settings_toml(&value)
            .map_err(|error| anyhow::anyhow!("could not parse {}: {error}", path.display()))
    } else {
        toml::from_str(text).with_context(|| format!("could not parse {}", path.display()))
    }
}

fn root_theme_color_table_name(value: &toml::Value) -> Option<&'static str> {
    let table = value.as_table()?;
    if table.contains_key("palette") {
        Some("palette")
    } else if table.contains_key("colors") {
        Some("colors")
    } else {
        None
    }
}

fn parse_friendly_theme_settings_toml(value: &toml::Value) -> anyhow::Result<ThemeSettings> {
    let root = value
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("theme file must be a TOML table"))?;
    if root.contains_key("palette") && root.contains_key("colors") {
        bail!("theme file must use either [palette] or [colors], not both");
    }

    let color_table_name =
        root_theme_color_table_name(value).expect("friendly theme table should exist");
    let color_table = root
        .get(color_table_name)
        .and_then(toml::Value::as_table)
        .ok_or_else(|| anyhow::anyhow!("[{color_table_name}] must be a table"))?;

    let mut theme = ThemeSettings::default();
    if let Some(name) = root.get("name") {
        let name = name
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("theme name must be a string"))?;

        let sanitized = sanitize_display_label(name);
        if !sanitized.is_empty() {
            theme.name = sanitized;
        }
    }

    for role in THEME_COLOR_ROLES {
        if let Some(value) = color_table.get(role) {
            let color = parse_theme_color_value(role, value)?;
            set_theme_color(&mut theme, role, color);
        }
    }

    Ok(theme)
}

const THEME_COLOR_ROLES: [&str; 9] = [
    "background",
    "panel",
    "panel_alt",
    "text",
    "muted_text",
    "accent",
    "selection",
    "warning",
    "error",
];

fn set_theme_color(theme: &mut ThemeSettings, role: &str, color: [u8; 3]) {
    match role {
        "background" => theme.background = color,
        "panel" => theme.panel = color,
        "panel_alt" => theme.panel_alt = color,
        "text" => theme.text = color,
        "muted_text" => theme.muted_text = color,
        "accent" => theme.accent = color,
        "selection" => theme.selection = Some(color),
        "warning" => theme.warning = color,
        "error" => theme.error = color,
        _ => {}
    }
}

fn parse_theme_color_value(role: &str, value: &toml::Value) -> anyhow::Result<[u8; 3]> {
    match value {
        toml::Value::String(value) => parse_theme_hex_color(role, value),
        toml::Value::Array(value) => parse_theme_rgb_array(role, value),
        _ => bail!("theme color {role} must be a hex string or RGB array"),
    }
}

fn parse_theme_hex_color(role: &str, value: &str) -> anyhow::Result<[u8; 3]> {
    let Some(hex) = value.trim().strip_prefix('#') else {
        bail!("theme color {role} must be #RRGGBB or #RGB hex");
    };
    let digits: Vec<char> = hex.chars().collect();
    match digits.as_slice() {
        [r, g, b] => Ok([
            parse_theme_hex_digit(role, *r)? * 17,
            parse_theme_hex_digit(role, *g)? * 17,
            parse_theme_hex_digit(role, *b)? * 17,
        ]),
        [r1, r2, g1, g2, b1, b2] => Ok([
            parse_theme_hex_component(role, *r1, *r2)?,
            parse_theme_hex_component(role, *g1, *g2)?,
            parse_theme_hex_component(role, *b1, *b2)?,
        ]),
        _ => bail!("theme color {role} must be #RRGGBB or #RGB hex"),
    }
}

fn parse_theme_hex_component(role: &str, high: char, low: char) -> anyhow::Result<u8> {
    Ok(parse_theme_hex_digit(role, high)? * 16 + parse_theme_hex_digit(role, low)?)
}

fn parse_theme_hex_digit(role: &str, value: char) -> anyhow::Result<u8> {
    match value.to_digit(16) {
        Some(value) => Ok(value as u8),
        None => bail!("theme color {role} must be #RRGGBB or #RGB hex"),
    }
}

fn parse_theme_rgb_array(role: &str, values: &[toml::Value]) -> anyhow::Result<[u8; 3]> {
    if values.len() != 3 {
        bail!("theme color {role} RGB array must contain 3 values");
    }

    let mut color = [0; 3];
    for (index, value) in values.iter().enumerate() {
        let Some(component) = value.as_integer() else {
            bail!("theme color {role} RGB array values must be integers from 0 to 255");
        };
        let Ok(component) = u8::try_from(component) else {
            bail!("theme color {role} RGB array values must be integers from 0 to 255");
        };
        color[index] = component;
    }
    Ok(color)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginSyntaxRegistry {
    syntaxes: Vec<PluginSyntaxRegistration>,
    by_language: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSyntaxRegistration {
    pub plugin_id: String,
    pub language_id: String,
    pub path: PathBuf,
    pub extensions: Vec<String>,
}

impl PluginSyntaxRegistry {
    pub fn from_plugins(plugins: &[PluginDescriptor]) -> Self {
        let syntax_count = plugins
            .iter()
            .filter(|plugin| plugin.manifest.capabilities.syntax)
            .map(|plugin| plugin.manifest.contributes.syntaxes.len())
            .sum();
        let mut registry = Self {
            syntaxes: Vec::with_capacity(syntax_count),
            ..Self::default()
        };
        let mut seen_plugin_ids = BTreeSet::new();
        for plugin in plugins {
            if !seen_plugin_ids.insert(plugin.manifest.id.as_str()) {
                continue;
            }
            if !plugin.manifest.capabilities.syntax {
                continue;
            }

            for syntax in &plugin.manifest.contributes.syntaxes {
                if registry.by_language.contains_key(&syntax.language) {
                    continue;
                }

                registry
                    .by_language
                    .insert(syntax.language.clone(), registry.syntaxes.len());
                registry.syntaxes.push(PluginSyntaxRegistration {
                    plugin_id: plugin.manifest.id.clone(),
                    language_id: syntax.language.clone(),
                    path: syntax.path.clone(),
                    extensions: language_extensions_for_syntax(plugin, &syntax.language),
                });
            }
        }
        registry
    }

    pub fn from_registrations(registrations: Vec<PluginSyntaxRegistration>) -> Self {
        let mut registry = Self {
            syntaxes: Vec::with_capacity(registrations.len()),
            ..Self::default()
        };
        for registration in registrations {
            if registry.by_language.contains_key(&registration.language_id) {
                continue;
            }
            registry
                .by_language
                .insert(registration.language_id.clone(), registry.syntaxes.len());
            registry.syntaxes.push(registration);
        }
        registry
    }

    pub fn syntaxes(&self) -> &[PluginSyntaxRegistration] {
        &self.syntaxes
    }

    pub fn len(&self) -> usize {
        self.syntaxes.len()
    }

    pub fn syntax_for_language(&self, language_id: &str) -> Option<&PluginSyntaxRegistration> {
        self.by_language
            .get(language_id)
            .and_then(|index| self.syntaxes.get(*index))
    }

    pub fn is_empty(&self) -> bool {
        self.syntaxes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    #[serde(default = "default_plugin_api_version")]
    pub api_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub entry: Option<PathBuf>,
    #[serde(default)]
    pub activation_events: Vec<PluginActivationEvent>,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub contributes: PluginContributions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub commands: bool,
    #[serde(default)]
    pub languages: bool,
    #[serde(default)]
    pub themes: bool,
    #[serde(default)]
    pub syntax: bool,
    #[serde(default)]
    pub workspace_read: bool,
    #[serde(default)]
    pub workspace_write: bool,
    #[serde(default)]
    pub process_spawn: bool,
    #[serde(default)]
    pub network: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginContributions {
    #[serde(default)]
    pub commands: Vec<PluginCommandContribution>,
    #[serde(default)]
    pub languages: Vec<PluginLanguageContribution>,
    #[serde(default)]
    pub themes: Vec<PluginThemeContribution>,
    #[serde(default)]
    pub syntaxes: Vec<PluginSyntaxContribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginCommandContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginLanguageContribution {
    pub id: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginThemeContribution {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginSyntaxContribution {
    pub language: String,
    pub path: PathBuf,
}

pub fn workspace_plugins_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".kuroya").join("plugins")
}

pub fn plugin_manifest_path(plugin_root: &Path) -> PathBuf {
    plugin_root.join("plugin.toml")
}

pub fn read_plugin_text_file_with_limit(path: &Path, max_bytes: u64) -> anyhow::Result<String> {
    let file =
        fs::File::open(path).with_context(|| format!("could not read {}", path.display()))?;
    let capacity = usize::try_from(max_bytes.saturating_add(1))
        .unwrap_or(usize::MAX)
        .min(64 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .with_context(|| format!("could not read {}", path.display()))?;

    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        bail!(
            "{} is larger than the plugin file limit of {max_bytes} bytes",
            path.display()
        );
    }

    String::from_utf8(bytes).with_context(|| format!("{} is not valid UTF-8", path.display()))
}

pub fn load_plugin_manifest(plugin_root: &Path) -> anyhow::Result<PluginDescriptor> {
    let path = plugin_manifest_path(plugin_root);
    let text = read_plugin_text_file_with_limit(&path, MAX_PLUGIN_MANIFEST_BYTES)?;
    parse_plugin_manifest_toml(plugin_root, &text)
        .with_context(|| format!("could not parse {}", path.display()))
}

pub fn discover_workspace_plugins(workspace_root: &Path) -> anyhow::Result<PluginDiscovery> {
    discover_workspace_plugins_with_limits(
        workspace_root,
        MAX_WORKSPACE_PLUGIN_ROOTS,
        MAX_WORKSPACE_PLUGIN_DIRECTORY_ENTRIES,
    )
}

fn discover_workspace_plugins_with_limits(
    workspace_root: &Path,
    max_plugin_roots: usize,
    max_directory_entries: usize,
) -> anyhow::Result<PluginDiscovery> {
    let dir = workspace_plugins_dir(workspace_root);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(PluginDiscovery::default());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", dir.display()));
        }
    };

    let mut discovery = PluginDiscovery::default();
    let mut roots = BTreeSet::new();
    let mut root_limit_reached = false;
    let mut directory_limit_reached = false;

    for (index, entry) in entries.enumerate() {
        if index >= max_directory_entries {
            directory_limit_reached = true;
            break;
        }
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        if !plugin_manifest_path(&path).is_file() {
            continue;
        }
        if max_plugin_roots == 0 {
            root_limit_reached = true;
            continue;
        }

        roots.insert(path);
        if roots.len() > max_plugin_roots {
            root_limit_reached = true;
            if let Some(last) = roots.iter().next_back().cloned() {
                roots.remove(&last);
            }
        }
    }

    if directory_limit_reached {
        discovery.errors.push(PluginDiscoveryError {
            root: dir.clone(),
            error: format!(
                "workspace plugin discovery stopped after {max_directory_entries} directory entries"
            ),
        });
    }
    if root_limit_reached {
        discovery.errors.push(PluginDiscoveryError {
            root: dir.clone(),
            error: format!("workspace plugin discovery limited to {max_plugin_roots} plugins"),
        });
    }

    let mut seen_plugin_ids: BTreeMap<String, PathBuf> = BTreeMap::new();
    for root in roots {
        match load_plugin_manifest(&root) {
            Ok(plugin) => {
                if let Some(entry) = &plugin.manifest.entry
                    && !entry.is_file()
                {
                    discovery.errors.push(PluginDiscoveryError {
                        root,
                        error: format!("plugin entry {} is missing", entry.display()),
                    });
                    continue;
                }
                if let Some(winner_root) = seen_plugin_ids.get(&plugin.manifest.id) {
                    discovery.errors.push(PluginDiscoveryError {
                        root,
                        error: format!(
                            "plugin id {} is duplicated (first registered from {})",
                            plugin.manifest.id,
                            winner_root.display()
                        ),
                    });
                } else {
                    seen_plugin_ids.insert(plugin.manifest.id.clone(), root.clone());
                    discovery.plugins.push(plugin);
                }
            }
            Err(error) => discovery.errors.push(PluginDiscoveryError {
                root,

                error: format!("{error:#}"),
            }),
        }
    }

    Ok(discovery)
}

pub fn parse_plugin_manifest_toml(
    plugin_root: &Path,
    text: &str,
) -> anyhow::Result<PluginDescriptor> {
    let manifest: PluginManifest = toml::from_str(text)?;
    let manifest = normalize_plugin_manifest(plugin_root, manifest)?;
    Ok(PluginDescriptor {
        root: plugin_root.to_path_buf(),
        manifest,
    })
}

fn normalize_plugin_manifest(
    plugin_root: &Path,
    mut manifest: PluginManifest,
) -> anyhow::Result<PluginManifest> {
    manifest.api_version = trim_owned(manifest.api_version);
    if manifest.api_version != PLUGIN_API_VERSION {
        bail!(
            "plugin {} uses unsupported API version {}",
            manifest.id,
            manifest.api_version
        );
    }

    manifest.id = normalize_identifier(manifest.id, "plugin id")?;
    manifest.name = normalize_display_label(manifest.name, "plugin name")?;
    manifest.version = normalize_plugin_version(manifest.version)?;
    manifest.entry = manifest
        .entry
        .map(|path| normalize_manifest_path(plugin_root, path, "plugin entry"))
        .transpose()?;
    ensure_plugin_list_len(
        manifest.activation_events.len(),
        MAX_PLUGIN_ACTIVATION_EVENTS,
        "plugin activation events",
    )?;
    manifest.activation_events =
        normalize_activation_events(std::mem::take(&mut manifest.activation_events))?;

    ensure_plugin_list_len(
        manifest.contributes.commands.len(),
        MAX_PLUGIN_COMMAND_CONTRIBUTIONS,
        "plugin command contributions",
    )?;
    let mut command_ids = BTreeSet::new();
    for command in &mut manifest.contributes.commands {
        command.id = normalize_identifier(std::mem::take(&mut command.id), "plugin command id")?;
        if !command_ids.insert(command.id.clone()) {
            bail!("plugin command id {} is duplicated", command.id);
        }
        command.title =
            normalize_display_label(std::mem::take(&mut command.title), "plugin command title")?;
        command.category = command
            .category
            .take()
            .map(|category| normalize_display_label(category, "plugin command category"))
            .transpose()?;
    }

    ensure_plugin_list_len(
        manifest.contributes.languages.len(),
        MAX_PLUGIN_LANGUAGE_CONTRIBUTIONS,
        "plugin language contributions",
    )?;
    let mut language_ids = BTreeSet::new();
    for language in &mut manifest.contributes.languages {
        language.id = normalize_identifier(std::mem::take(&mut language.id), "plugin language id")?;
        if !language_ids.insert(language.id.clone()) {
            bail!("plugin language id {} is duplicated", language.id);
        }
        language.extensions = normalize_extensions(std::mem::take(&mut language.extensions))?;
        language.aliases = normalize_string_list(
            std::mem::take(&mut language.aliases),
            "plugin language aliases",
            MAX_PLUGIN_LANGUAGE_ALIASES,
        )?;
    }

    ensure_plugin_list_len(
        manifest.contributes.themes.len(),
        MAX_PLUGIN_THEME_CONTRIBUTIONS,
        "plugin theme contributions",
    )?;
    let mut theme_ids = BTreeSet::new();
    for theme in &mut manifest.contributes.themes {
        theme.id = normalize_identifier(std::mem::take(&mut theme.id), "plugin theme id")?;
        if !theme_ids.insert(theme.id.clone()) {
            bail!("plugin theme id {} is duplicated", theme.id);
        }
        theme.label =
            normalize_display_label(std::mem::take(&mut theme.label), "plugin theme label")?;
        theme.path = normalize_manifest_path(
            plugin_root,
            std::mem::take(&mut theme.path),
            "plugin theme path",
        )?;
    }

    ensure_plugin_list_len(
        manifest.contributes.syntaxes.len(),
        MAX_PLUGIN_SYNTAX_CONTRIBUTIONS,
        "plugin syntax contributions",
    )?;
    let mut syntax_languages = BTreeSet::new();
    for syntax in &mut manifest.contributes.syntaxes {
        syntax.language = normalize_identifier(
            std::mem::take(&mut syntax.language),
            "plugin syntax language",
        )?;
        if !syntax_languages.insert(syntax.language.clone()) {
            bail!("plugin syntax language {} is duplicated", syntax.language);
        }
        syntax.path = normalize_manifest_path(
            plugin_root,
            std::mem::take(&mut syntax.path),
            "plugin syntax path",
        )?;
    }

    Ok(manifest)
}

fn ensure_plugin_list_len(len: usize, max_len: usize, field: &str) -> anyhow::Result<()> {
    if len > max_len {
        bail!("{field} contains too many items ({len} > {max_len})");
    }
    Ok(())
}

fn normalize_non_empty(value: String, field: &str) -> anyhow::Result<String> {
    let value = trim_owned(value);
    if value.is_empty() {
        bail!("{field} cannot be empty");
    }
    Ok(value)
}

fn normalize_plugin_version(value: String) -> anyhow::Result<String> {
    let value = normalize_non_empty(value, "plugin version")?;
    if value.chars().count() > MAX_PLUGIN_VERSION_CHARS {
        bail!("plugin version is too long");
    }
    if value
        .chars()
        .any(|ch| ch.is_control() || is_plugin_display_format_control(ch))
    {
        bail!("plugin version contains unsupported characters");
    }
    Ok(value)
}

fn normalize_display_label(value: String, field: &str) -> anyhow::Result<String> {
    let value = sanitize_display_label(&value);
    if value.is_empty() {
        bail!("{field} cannot be empty");
    }
    Ok(value)
}

fn sanitize_display_label(value: &str) -> String {
    let value = value.trim();
    let mut normalized = String::with_capacity(value.len().min(MAX_PLUGIN_DISPLAY_LABEL_CHARS));
    let mut char_count = 0;
    let mut pending_space = false;
    let mut overflow = false;

    for ch in value.chars() {
        if ch.is_control() || ch.is_whitespace() || is_plugin_display_format_control(ch) {
            if !normalized.is_empty() {
                pending_space = true;
            }
            continue;
        }

        if pending_space {
            if char_count < MAX_PLUGIN_DISPLAY_LABEL_CHARS {
                normalized.push(' ');
                char_count += 1;
            } else {
                overflow = true;
                break;
            }
            pending_space = false;
        }

        if char_count < MAX_PLUGIN_DISPLAY_LABEL_CHARS {
            normalized.push(ch);
            char_count += 1;
        } else {
            overflow = true;
            break;
        }
    }

    if overflow {
        truncate_display_label_with_omission(normalized)
    } else {
        normalized
    }
}

fn truncate_display_label(value: String) -> String {
    if value.chars().nth(MAX_PLUGIN_DISPLAY_LABEL_CHARS).is_none() {
        return value;
    }

    truncate_display_label_with_omission(value)
}

fn truncate_display_label_with_omission(value: String) -> String {
    let prefix_chars = MAX_PLUGIN_DISPLAY_LABEL_CHARS.saturating_sub(DISPLAY_LABEL_OMISSION.len());
    let mut truncated = String::with_capacity(MAX_PLUGIN_DISPLAY_LABEL_CHARS);
    for ch in value.chars().take(prefix_chars) {
        truncated.push(ch);
    }
    while truncated.chars().last().is_some_and(char::is_whitespace) {
        truncated.pop();
    }
    truncated.push_str(DISPLAY_LABEL_OMISSION);
    truncated
}

fn is_plugin_display_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
    )
}

fn trim_owned(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.len() == value.len() {
        value
    } else {
        trimmed.to_owned()
    }
}

fn normalize_identifier(value: String, field: &str) -> anyhow::Result<String> {
    let value = normalize_non_empty(value, field)?;
    if value.chars().count() > MAX_PLUGIN_IDENTIFIER_CHARS {
        bail!("{field} is too long");
    }
    let valid = value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':'));
    if !valid {
        bail!("{field} contains unsupported characters");
    }
    Ok(value)
}

fn normalize_manifest_path(
    plugin_root: &Path,
    path: PathBuf,
    field: &str,
) -> anyhow::Result<PathBuf> {
    if path.as_os_str().is_empty() {
        bail!("{field} cannot be empty");
    }
    if path.is_absolute() {
        bail!("{field} must be relative to the plugin root");
    }
    normalize_child_path(plugin_root, &path)
        .ok_or_else(|| anyhow::anyhow!("{field} must stay inside the plugin root"))
}

fn normalize_extensions(extensions: Vec<String>) -> anyhow::Result<Vec<String>> {
    ensure_plugin_list_len(
        extensions.len(),
        MAX_PLUGIN_LANGUAGE_EXTENSIONS,
        "plugin language extensions",
    )?;
    let mut normalized = Vec::with_capacity(extensions.len());
    let mut seen = BTreeSet::new();
    for extension in extensions {
        let extension = normalize_extension(extension)?;
        if seen.insert(extension.clone()) {
            normalized.push(extension);
        }
    }
    Ok(normalized)
}

fn normalize_extension(extension: String) -> anyhow::Result<String> {
    let trimmed = extension.trim().trim_start_matches('.');
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.chars().any(char::is_whitespace)
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || is_plugin_display_format_control(ch))
        || trimmed.chars().count() > MAX_PLUGIN_LANGUAGE_EXTENSION_CHARS
    {
        bail!("plugin language extension is invalid");
    }

    let needs_lowercase = trimmed.bytes().any(|byte| byte.is_ascii_uppercase());
    if trimmed.len() == extension.len() && !needs_lowercase {
        return Ok(extension);
    }

    let extension = if needs_lowercase {
        trimmed.to_ascii_lowercase()
    } else {
        trimmed.to_owned()
    };
    Ok(extension)
}

fn normalize_activation_events(
    events: Vec<PluginActivationEvent>,
) -> anyhow::Result<Vec<PluginActivationEvent>> {
    let mut normalized = Vec::with_capacity(events.len());
    let mut seen = BTreeSet::new();
    for event in events {
        let event = match event {
            PluginActivationEvent::OnCommand(command) => PluginActivationEvent::OnCommand(
                normalize_identifier(command, "plugin activation command")?,
            ),
            PluginActivationEvent::OnLanguage(language) => PluginActivationEvent::OnLanguage(
                normalize_identifier(language, "plugin activation language")?,
            ),
            PluginActivationEvent::OnStartupFinished => PluginActivationEvent::OnStartupFinished,
            PluginActivationEvent::Any => PluginActivationEvent::Any,
        };
        if seen.insert(event.clone()) {
            normalized.push(event);
        }
    }
    Ok(normalized)
}

fn normalize_string_list(
    values: Vec<String>,
    field: &str,
    max_len: usize,
) -> anyhow::Result<Vec<String>> {
    ensure_plugin_list_len(values.len(), max_len, field)?;
    let mut normalized = Vec::with_capacity(values.len());
    let mut seen = BTreeSet::new();
    for value in values {
        let value = sanitize_display_label(&value);
        if !value.is_empty() && seen.insert(value.clone()) {
            normalized.push(value);
        }
    }
    Ok(normalized)
}

fn runtime_activation_events(plugin: &PluginDescriptor) -> Vec<PluginActivationEvent> {
    let command_events = if plugin.manifest.capabilities.commands {
        plugin.manifest.contributes.commands.len()
    } else {
        0
    };
    let language_events = if plugin.manifest.capabilities.languages {
        plugin.manifest.contributes.languages.len()
    } else {
        0
    };
    let mut events = Vec::with_capacity(
        plugin.manifest.activation_events.len() + command_events + language_events,
    );
    let mut seen = BTreeSet::new();
    for event in &plugin.manifest.activation_events {
        if seen.insert(event.clone()) {
            events.push(event.clone());
        }
    }
    if plugin.manifest.capabilities.commands {
        for command in &plugin.manifest.contributes.commands {
            let event = PluginActivationEvent::OnCommand(command.id.clone());
            if seen.insert(event.clone()) {
                events.push(event);
            }
        }
    }
    if plugin.manifest.capabilities.languages {
        for language in &plugin.manifest.contributes.languages {
            let event = PluginActivationEvent::OnLanguage(language.id.clone());
            if seen.insert(event.clone()) {
                events.push(event);
            }
        }
    }
    events
}

fn language_extensions_for_syntax(plugin: &PluginDescriptor, language_id: &str) -> Vec<String> {
    plugin
        .manifest
        .contributes
        .languages
        .iter()
        .find(|language| language.id == language_id)
        .map(|language| language.extensions.clone())
        .unwrap_or_default()
}

fn parse_plugin_activation_event(raw: &str) -> Result<PluginActivationEvent, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("plugin activation event cannot be empty".to_owned());
    }
    if value == "*" {
        return Ok(PluginActivationEvent::Any);
    }
    if value == "onStartupFinished" {
        return Ok(PluginActivationEvent::OnStartupFinished);
    }
    if let Some(command) = value.strip_prefix("onCommand:") {
        return Ok(PluginActivationEvent::OnCommand(command.trim().to_owned()));
    }
    if let Some(language) = value.strip_prefix("onLanguage:") {
        return Ok(PluginActivationEvent::OnLanguage(
            language.trim().to_owned(),
        ));
    }
    Err(format!("unsupported plugin activation event {value}"))
}

fn plugin_command_label(manifest: &PluginManifest, command: &PluginCommandContribution) -> String {
    let prefix = command.category.as_deref().unwrap_or(&manifest.name);
    let mut label = String::with_capacity(prefix.len() + 2 + command.title.len());
    label.push_str(prefix);
    label.push_str(": ");
    label.push_str(&command.title);
    truncate_display_label(label)
}

fn default_plugin_api_version() -> String {
    PLUGIN_API_VERSION.to_owned()
}

#[cfg(test)]
mod tests;

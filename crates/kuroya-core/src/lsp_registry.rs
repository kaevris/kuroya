use serde::Deserialize;
use std::{path::Path, sync::LazyLock};

const EMBEDDED_DEFINITIONS: &[&str] = &[
    include_str!("../../../lsps/c.toml"),
    include_str!("../../../lsps/cpp.toml"),
    include_str!("../../../lsps/css.toml"),
    include_str!("../../../lsps/go.toml"),
    include_str!("../../../lsps/html.toml"),
    include_str!("../../../lsps/javascript.toml"),
    include_str!("../../../lsps/json.toml"),
    include_str!("../../../lsps/lua.toml"),
    include_str!("../../../lsps/markdown.toml"),
    include_str!("../../../lsps/python.toml"),
    include_str!("../../../lsps/rust.toml"),
    include_str!("../../../lsps/shellscript.toml"),
    include_str!("../../../lsps/typescript.toml"),
    include_str!("../../../lsps/yaml.toml"),
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LspInstallDefinition {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub languages: Vec<String>,
    pub verify: LspInstallVerifyCommand,
    #[serde(default)]
    pub install: LspInstallPlatforms,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LspInstallVerifyCommand {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct LspInstallPlatforms {
    #[serde(default)]
    pub windows: Option<LspInstallShellCommand>,
    #[serde(default)]
    pub linux: Option<LspInstallShellCommand>,
    #[serde(default)]
    pub macos: Option<LspInstallShellCommand>,
}

impl LspInstallPlatforms {
    pub fn is_empty(&self) -> bool {
        self.windows.is_none() && self.linux.is_none() && self.macos.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LspInstallShellCommand {
    pub shell: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspInstallPlatform {
    Windows,
    Linux,
    MacOS,
}

pub fn lsp_install_registry() -> &'static [LspInstallDefinition] {
    static REGISTRY: LazyLock<Vec<LspInstallDefinition>> = LazyLock::new(|| {
        EMBEDDED_DEFINITIONS
            .iter()
            .map(|text| parse_embedded_lsp_install_definition(text))
            .collect()
    });
    &REGISTRY
}

pub fn registry_entry_for(server_id: &str) -> Option<&'static LspInstallDefinition> {
    lsp_install_registry()
        .iter()
        .find(|definition| definition.id == server_id)
}

pub fn lsp_platform_install_command(
    definition: &LspInstallDefinition,
    platform: LspInstallPlatform,
) -> Option<&str> {
    match platform {
        LspInstallPlatform::Windows => definition
            .install
            .windows
            .as_ref()
            .map(|command| command.shell.as_str()),
        LspInstallPlatform::Linux => definition
            .install
            .linux
            .as_ref()
            .map(|command| command.shell.as_str()),
        LspInstallPlatform::MacOS => definition
            .install
            .macos
            .as_ref()
            .map(|command| command.shell.as_str()),
    }
}

pub fn install_command_for(definition: &LspInstallDefinition) -> Option<&str> {
    lsp_platform_install_command(definition, current_lsp_install_platform())
}

pub fn current_lsp_install_platform() -> LspInstallPlatform {
    if cfg!(windows) {
        LspInstallPlatform::Windows
    } else if cfg!(target_os = "macos") {
        LspInstallPlatform::MacOS
    } else {
        LspInstallPlatform::Linux
    }
}

pub fn lsp_binary_on_path(command: &str) -> bool {
    let command = command.trim();
    if command.is_empty() {
        return false;
    }
    if command.contains('/') || command.contains('\\') {
        return lsp_binary_candidate_exists(Path::new(command));
    }
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|directory| {
        let candidate = directory.join(command);
        lsp_binary_candidate_exists(&candidate)
    })
}

fn lsp_binary_candidate_exists(path: &Path) -> bool {
    if path.is_file() {
        return true;
    }
    #[cfg(windows)]
    {
        for extension in [".exe", ".cmd", ".bat"] {
            let mut candidate = path.as_os_str().to_os_string();
            candidate.push(extension);
            if Path::new(&candidate).is_file() {
                return true;
            }
        }
    }
    false
}

fn parse_embedded_lsp_install_definition(text: &'static str) -> LspInstallDefinition {
    toml::from_str(text)
        .unwrap_or_else(|error| panic!("invalid embedded LSP install definition: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{
        current_lsp_install_platform, install_command_for, lsp_binary_on_path,
        lsp_install_registry, lsp_platform_install_command, registry_entry_for,
    };
    use crate::{default_server_configs, lsp_registry::LspInstallPlatform};

    #[test]
    fn registry_entries_parse_and_match_default_server_config_ids() {
        let default_languages: Vec<_> = default_server_configs()
            .into_iter()
            .map(|config| config.language)
            .collect();

        let registry = lsp_install_registry();
        assert!(!registry.is_empty());
        for definition in registry {
            assert!(
                default_languages.contains(&definition.id),
                "registry id {:?} must match a default LspServerConfig language",
                definition.id
            );
            assert!(!definition.display_name.is_empty());
            assert!(definition.languages.contains(&definition.id));
            assert!(
                !definition.install.is_empty(),
                "registry entry {:?} must provide at least one install platform",
                definition.id
            );
            assert!(
                !definition.verify.command.trim().is_empty(),
                "registry entry {:?} must provide a verify command",
                definition.id
            );
        }
    }

    #[test]
    fn registry_install_shells_are_nonempty_and_control_character_free() {
        for definition in lsp_install_registry() {
            for shell in [
                definition
                    .install
                    .windows
                    .as_ref()
                    .map(|command| &command.shell),
                definition
                    .install
                    .linux
                    .as_ref()
                    .map(|command| &command.shell),
                definition
                    .install
                    .macos
                    .as_ref()
                    .map(|command| &command.shell),
            ]
            .into_iter()
            .flatten()
            {
                assert!(!shell.trim().is_empty());
                assert_eq!(shell.trim(), shell);
                assert!(
                    !shell.chars().any(char::is_control),
                    "install shell {shell:?} must not contain control characters"
                );
            }
        }
    }

    #[test]
    fn platform_selection_picks_the_matching_platform_shell() {
        for definition in lsp_install_registry() {
            for (platform, command) in [
                (
                    LspInstallPlatform::Windows,
                    definition
                        .install
                        .windows
                        .as_ref()
                        .map(|command| command.shell.as_str()),
                ),
                (
                    LspInstallPlatform::Linux,
                    definition
                        .install
                        .linux
                        .as_ref()
                        .map(|command| command.shell.as_str()),
                ),
                (
                    LspInstallPlatform::MacOS,
                    definition
                        .install
                        .macos
                        .as_ref()
                        .map(|command| command.shell.as_str()),
                ),
            ] {
                assert_eq!(
                    lsp_platform_install_command(definition, platform),
                    command,
                    "platform {platform:?} selection mismatch for {:?}",
                    definition.id
                );
            }
            assert_eq!(
                install_command_for(definition),
                lsp_platform_install_command(definition, current_lsp_install_platform())
            );
        }
    }

    #[test]
    fn registry_lookup_returns_none_for_unknown_ids() {
        assert!(registry_entry_for("definitely-not-a-server").is_none());
        assert!(registry_entry_for("").is_none());
        let rust = registry_entry_for("rust").expect("rust registry entry");
        assert_eq!(rust.display_name, "rust-analyzer");
        assert!(install_command_for(rust).is_some());
    }

    #[test]
    fn binary_on_path_detects_system_binaries_and_rejects_unknown_commands() {
        let known_command = if cfg!(windows) { "cmd" } else { "sh" };

        assert!(lsp_binary_on_path(known_command));
        assert!(!lsp_binary_on_path("definitely-not-a-real-binary-xyz"));
        assert!(!lsp_binary_on_path(""));
        assert!(!lsp_binary_on_path("   "));

        let current_exe = std::env::current_exe().expect("current test binary");
        assert!(
            lsp_binary_on_path(&current_exe.to_string_lossy()),
            "path-qualified commands should resolve directly"
        );
    }
}

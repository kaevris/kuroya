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

pub const LSP_ASSET_PLATFORM_TOKEN: &str = "{platform}";
pub const LSP_ASSET_ARCHIVE_TOKEN: &str = "{archive}";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LspInstallDefinition {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub verify: Option<LspInstallVerifyCommand>,
    pub install: LspInstallRecipe,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LspInstallRecipe {
    pub kind: LspInstallKind,
    #[serde(default)]
    pub launch: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub fallback_shell: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LspInstallKind {
    Download,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LspInstallVerifyCommand {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
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

pub fn lsp_install_platform_token(platform: LspInstallPlatform) -> &'static str {
    match platform {
        LspInstallPlatform::Windows => "windows-x64",
        LspInstallPlatform::Linux => "linux-x64",
        LspInstallPlatform::MacOS => "macos-x64",
    }
}

pub fn lsp_install_archive_token(platform: LspInstallPlatform) -> &'static str {
    match platform {
        LspInstallPlatform::Windows => "zip",
        LspInstallPlatform::Linux | LspInstallPlatform::MacOS => "tar.gz",
    }
}

pub fn lsp_release_asset_name(
    definition: &LspInstallDefinition,
    platform: LspInstallPlatform,
) -> Option<String> {
    let asset = definition.asset.as_deref()?;
    let name = asset
        .replace(LSP_ASSET_PLATFORM_TOKEN, lsp_install_platform_token(platform))
        .replace(LSP_ASSET_ARCHIVE_TOKEN, lsp_install_archive_token(platform));
    (!name.trim().is_empty()).then_some(name)
}

pub fn lsp_launch_binary_name(
    definition: &LspInstallDefinition,
    platform: LspInstallPlatform,
) -> Option<String> {
    let launch = definition.install.launch.as_deref()?.trim();
    if launch.is_empty() {
        return None;
    }
    if platform == LspInstallPlatform::Windows {
        return Some(format!("{launch}.exe"));
    }
    Some(launch.to_owned())
}

pub fn lsp_fallback_shell(definition: &LspInstallDefinition) -> Option<&str> {
    definition
        .install
        .fallback_shell
        .as_deref()
        .map(str::trim)
        .filter(|shell| !shell.is_empty())
}

pub fn lsp_fallback_verify(definition: &LspInstallDefinition) -> Option<(&str, &[String])> {
    let verify = definition.verify.as_ref()?;
    Some((verify.command.as_str(), &verify.args))
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
        current_lsp_install_platform, lsp_binary_on_path, lsp_fallback_shell, lsp_fallback_verify,
        lsp_install_archive_token, lsp_install_platform_token, lsp_install_registry,
        lsp_launch_binary_name, lsp_release_asset_name, registry_entry_for, LspInstallDefinition,
        LspInstallKind, LspInstallPlatform,
    };
    use crate::default_server_configs;

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
            assert!(
                definition.languages.contains(&definition.id),
                "registry entry {:?} must list its own language",
                definition.id
            );
        }
    }

    #[test]
    fn download_entries_define_release_assets_and_launch_binaries() {
        for definition in lsp_install_registry() {
            if definition.install.kind != LspInstallKind::Download {
                continue;
            }
            let asset = definition.asset.as_deref().unwrap_or_else(|| {
                panic!("download entry {:?} must define an asset", definition.id)
            });
            assert!(
                asset.contains("{platform}") && asset.contains("{archive}"),
                "download asset {asset:?} must use the platform and archive placeholders"
            );
            assert!(
                !asset.contains(char::is_whitespace),
                "download asset {asset:?} must be a single token"
            );
            let launch = definition.install.launch.as_deref().unwrap_or_else(|| {
                panic!("download entry {:?} must define a launch binary", definition.id)
            });
            assert!(
                !launch.trim().is_empty() && !launch.contains(['/', '\\']),
                "launch binary {launch:?} must be a bare file name"
            );
        }
    }

    #[test]
    fn unsupported_entries_define_reason_and_npm_fallback() {
        for definition in lsp_install_registry() {
            if definition.install.kind != LspInstallKind::Unsupported {
                continue;
            }
            let reason = definition.install.reason.as_deref().unwrap_or_else(|| {
                panic!("unsupported entry {:?} must define a reason", definition.id)
            });
            assert!(
                reason.contains("Node.js"),
                "unsupported reason {reason:?} should explain the Node.js requirement"
            );
            let shell = definition
                .install
                .fallback_shell
                .as_deref()
                .unwrap_or_else(|| {
                    panic!("unsupported entry {:?} must define a fallback shell", definition.id)
                });
            assert!(
                !shell.trim().is_empty() && shell.trim() == shell,
                "fallback shell {shell:?} must be a trimmed single command"
            );
            assert!(
                definition.asset.is_none(),
                "unsupported entry {:?} must not ship a release asset",
                definition.id
            );
            let verify = definition.verify.as_ref().unwrap_or_else(|| {
                panic!("unsupported entry {:?} must define a verify command", definition.id)
            });
            assert!(!verify.command.trim().is_empty());
        }
    }

    #[test]
    fn release_asset_names_resolve_platform_and_archive_tokens() {
        let rust = registry_entry_for("rust").expect("rust registry entry");
        assert_eq!(
            lsp_release_asset_name(rust, LspInstallPlatform::Windows).as_deref(),
            Some("lsp-rust-analyzer-windows-x64.zip")
        );
        assert_eq!(
            lsp_release_asset_name(rust, LspInstallPlatform::Linux).as_deref(),
            Some("lsp-rust-analyzer-linux-x64.tar.gz")
        );
        assert_eq!(
            lsp_release_asset_name(rust, LspInstallPlatform::MacOS).as_deref(),
            Some("lsp-rust-analyzer-macos-x64.tar.gz")
        );

        let python = registry_entry_for("python").expect("python registry entry");
        assert_eq!(lsp_release_asset_name(python, LspInstallPlatform::Windows), None);
    }

    #[test]
    fn launch_binary_names_append_the_windows_executable_suffix() {
        let rust = registry_entry_for("rust").expect("rust registry entry");
        assert_eq!(
            lsp_launch_binary_name(rust, LspInstallPlatform::Windows).as_deref(),
            Some("rust-analyzer.exe")
        );
        assert_eq!(
            lsp_launch_binary_name(rust, LspInstallPlatform::Linux).as_deref(),
            Some("rust-analyzer")
        );
    }

    #[test]
    fn platform_tokens_map_to_release_asset_naming() {
        assert_eq!(
            lsp_install_platform_token(LspInstallPlatform::Windows),
            "windows-x64"
        );
        assert_eq!(
            lsp_install_platform_token(LspInstallPlatform::Linux),
            "linux-x64"
        );
        assert_eq!(
            lsp_install_platform_token(LspInstallPlatform::MacOS),
            "macos-x64"
        );
        assert_eq!(lsp_install_archive_token(LspInstallPlatform::Windows), "zip");
        assert_eq!(lsp_install_archive_token(LspInstallPlatform::Linux), "tar.gz");
        assert_eq!(lsp_install_archive_token(LspInstallPlatform::MacOS), "tar.gz");
    }

    #[test]
    fn fallback_shell_resolves_only_for_unsupported_entries() {
        let python = registry_entry_for("python").expect("python registry entry");
        assert_eq!(lsp_fallback_shell(python), Some("npm install -g pyright"));
        let (command, args) = lsp_fallback_verify(python).expect("python verify command");
        assert_eq!(command, "pyright-langserver");
        assert_eq!(args, &["--version".to_owned()]);

        let rust = registry_entry_for("rust").expect("rust registry entry");
        assert_eq!(lsp_fallback_shell(rust), None);
    }

    #[test]
    fn registry_lookup_returns_none_for_unknown_ids() {
        assert!(registry_entry_for("definitely-not-a-server").is_none());
        assert!(registry_entry_for("").is_none());
        let rust = registry_entry_for("rust").expect("rust registry entry");
        assert_eq!(rust.display_name, "rust-analyzer");
        assert_eq!(rust.install.kind, LspInstallKind::Download);
    }

    #[test]
    fn every_registry_entry_has_a_consistent_current_platform_view() {
        let platform = current_lsp_install_platform();
        for definition in lsp_install_registry() {
            match definition.install.kind {
                LspInstallKind::Download => {
                    assert!(lsp_release_asset_name(definition, platform).is_some());
                    assert!(lsp_launch_binary_name(definition, platform).is_some());
                }
                LspInstallKind::Unsupported => {
                    assert!(lsp_fallback_shell(definition).is_some());
                }
            }
        }
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

    #[test]
    fn definition_parsing_requires_an_install_kind() {
        let error = toml::from_str::<LspInstallDefinition>(
            "id = \"x\"\ndisplay_name = \"X\"\nlanguages = [\"x\"]\n\n[install]\nlaunch = \"x\"\n",
        )
        .expect_err("a missing install kind must fail to parse");
        assert!(error.to_string().contains("kind"));
    }
}

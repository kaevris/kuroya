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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LspInstallDefinition {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub asset: Option<String>,
    pub launch: String,
    pub install: LspInstallKind,
    #[serde(default)]
    pub npm_fallback: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    pub verify: LspInstallVerifyCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LspInstallKind {
    Repo,
    Npm,
    RustupThenRepo,
}

impl LspInstallKind {
    pub fn supports_repo_download(self) -> bool {
        matches!(self, Self::Repo | Self::RustupThenRepo)
    }
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

impl LspInstallDefinition {
    pub fn supports_repo_download(&self) -> bool {
        self.install.supports_repo_download()
    }

    pub fn repo_asset_name(&self, platform: LspInstallPlatform) -> Option<String> {
        let asset = self.asset.as_deref()?;
        Some(format!(
            "{}{}",
            asset.replace(LSP_ASSET_PLATFORM_TOKEN, lsp_platform_token(platform)),
            lsp_archive_extension(platform)
        ))
    }

    pub fn npm_fallback(&self) -> Option<&str> {
        self.npm_fallback.as_deref()
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
}

pub fn lsp_platform_token(platform: LspInstallPlatform) -> &'static str {
    match platform {
        LspInstallPlatform::Windows => "windows-x64",
        LspInstallPlatform::Linux => "linux-x64",
        LspInstallPlatform::MacOS => "macos-x64",
    }
}

pub fn lsp_archive_extension(platform: LspInstallPlatform) -> &'static str {
    match platform {
        LspInstallPlatform::Windows => ".zip",
        LspInstallPlatform::Linux | LspInstallPlatform::MacOS => ".tar.gz",
    }
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
        LspInstallKind, LspInstallPlatform, current_lsp_install_platform, lsp_archive_extension,
        lsp_binary_on_path, lsp_install_registry, lsp_platform_token, registry_entry_for,
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
            assert!(definition.languages.contains(&definition.id));
            assert!(!definition.launch.trim().is_empty());
            assert!(
                !definition.verify.command.trim().is_empty(),
                "registry entry {:?} must provide a verify command",
                definition.id
            );
        }
    }

    #[test]
    fn registry_entries_declare_the_confirmed_install_kinds() {
        let expected_kinds = [
            ("c", LspInstallKind::Repo),
            ("cpp", LspInstallKind::Repo),
            ("css", LspInstallKind::Npm),
            ("go", LspInstallKind::Repo),
            ("html", LspInstallKind::Npm),
            ("javascript", LspInstallKind::Npm),
            ("json", LspInstallKind::Npm),
            ("lua", LspInstallKind::Repo),
            ("markdown", LspInstallKind::Repo),
            ("python", LspInstallKind::Npm),
            ("rust", LspInstallKind::RustupThenRepo),
            ("shellscript", LspInstallKind::Npm),
            ("typescript", LspInstallKind::Npm),
            ("yaml", LspInstallKind::Npm),
        ];

        for (id, kind) in expected_kinds {
            let definition = registry_entry_for(id).unwrap_or_else(|| panic!("{id} entry"));
            assert_eq!(definition.install, kind, "install kind for {id}");
            assert_eq!(
                definition.supports_repo_download(),
                kind.supports_repo_download(),
                "repo-download support for {id}"
            );
        }
    }

    #[test]
    fn repo_capable_entries_resolve_platform_asset_names_with_archive_extensions() {
        let expected_assets = [
            ("c", "lsp-clangd-{platform}", "clangd"),
            ("cpp", "lsp-clangd-{platform}", "clangd"),
            ("go", "lsp-gopls-{platform}", "gopls"),
            (
                "lua",
                "lsp-lua-language-server-{platform}",
                "lua-language-server",
            ),
            ("markdown", "lsp-marksman-{platform}", "marksman"),
            ("rust", "lsp-rust-analyzer-{platform}", "rust-analyzer"),
        ];

        for (id, asset, launch) in expected_assets {
            let definition = registry_entry_for(id).unwrap_or_else(|| panic!("{id} entry"));
            assert_eq!(definition.asset.as_deref(), Some(asset), "asset for {id}");
            assert_eq!(definition.launch, launch, "launch for {id}");

            for (platform, token, extension) in [
                (LspInstallPlatform::Windows, "windows-x64", ".zip"),
                (LspInstallPlatform::Linux, "linux-x64", ".tar.gz"),
                (LspInstallPlatform::MacOS, "macos-x64", ".tar.gz"),
            ] {
                assert_eq!(
                    definition.repo_asset_name(platform),
                    Some(format!(
                        "{}{}",
                        asset.replace("{platform}", token),
                        extension
                    )),
                    "asset name for {id} on {token}"
                );
            }
        }

        for id in [
            "css",
            "html",
            "javascript",
            "json",
            "python",
            "shellscript",
            "typescript",
            "yaml",
        ] {
            let definition = registry_entry_for(id).unwrap_or_else(|| panic!("{id} entry"));
            assert!(definition.asset.is_none(), "npm entry {id} has no asset");
            for platform in [
                LspInstallPlatform::Windows,
                LspInstallPlatform::Linux,
                LspInstallPlatform::MacOS,
            ] {
                assert_eq!(
                    definition.repo_asset_name(platform),
                    None,
                    "npm entry {id} must not resolve a repo asset"
                );
            }
        }
    }

    #[test]
    fn platform_tokens_and_archive_extensions_follow_the_release_assets() {
        assert_eq!(
            lsp_platform_token(LspInstallPlatform::Windows),
            "windows-x64"
        );
        assert_eq!(lsp_platform_token(LspInstallPlatform::Linux), "linux-x64");
        assert_eq!(lsp_platform_token(LspInstallPlatform::MacOS), "macos-x64");
        assert_eq!(lsp_archive_extension(LspInstallPlatform::Windows), ".zip");
        assert_eq!(lsp_archive_extension(LspInstallPlatform::Linux), ".tar.gz");
        assert_eq!(lsp_archive_extension(LspInstallPlatform::MacOS), ".tar.gz");
    }

    #[test]
    fn npm_entries_expose_the_fallback_command_and_node_reason() {
        for id in [
            "css",
            "html",
            "javascript",
            "json",
            "python",
            "shellscript",
            "typescript",
            "yaml",
        ] {
            let definition = registry_entry_for(id).unwrap_or_else(|| panic!("{id} entry"));
            let fallback = definition
                .npm_fallback()
                .unwrap_or_else(|| panic!("{id} npm fallback"));
            assert!(
                fallback.starts_with("npm install -g "),
                "npm fallback for {id} should install globally: {fallback}"
            );
            let reason = definition.reason().unwrap_or_else(|| panic!("{id} reason"));
            assert!(
                reason.contains("Node.js"),
                "reason for {id} should mention Node.js: {reason}"
            );
        }

        for id in ["c", "cpp", "go", "lua", "markdown", "rust"] {
            let definition = registry_entry_for(id).unwrap_or_else(|| panic!("{id} entry"));
            assert!(
                definition.npm_fallback().is_none(),
                "repo entry {id} has no npm fallback"
            );
        }
    }

    #[test]
    fn rust_entry_installs_via_rustup_before_falling_back_to_the_repo() {
        let rust = registry_entry_for("rust").expect("rust registry entry");
        assert_eq!(rust.display_name, "rust-analyzer");
        assert_eq!(rust.launch, "rust-analyzer");
        assert_eq!(rust.install, LspInstallKind::RustupThenRepo);
        assert_eq!(rust.verify.command, "rust-analyzer");
        assert_eq!(rust.verify.args, vec!["--version".to_owned()]);
    }

    #[test]
    fn registry_lookup_returns_none_for_unknown_ids() {
        assert!(registry_entry_for("definitely-not-a-server").is_none());
        assert!(registry_entry_for("").is_none());
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
    fn current_platform_matches_the_compilation_target() {
        let platform = current_lsp_install_platform();
        if cfg!(windows) {
            assert_eq!(platform, LspInstallPlatform::Windows);
        } else if cfg!(target_os = "macos") {
            assert_eq!(platform, LspInstallPlatform::MacOS);
        } else {
            assert_eq!(platform, LspInstallPlatform::Linux);
        }
    }
}

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageBracketPair {
    pub open: char,
    pub close: char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LanguageConfiguration {
    line_comment_prefix: Option<&'static str>,
    brackets: &'static [LanguageBracketPair],
    auto_closing_pairs: &'static [LanguageBracketPair],
    increase_indent_line_suffixes: &'static [&'static str],
}

const C_STYLE_BRACKETS: &[LanguageBracketPair] = &[
    LanguageBracketPair {
        open: '(',
        close: ')',
    },
    LanguageBracketPair {
        open: '[',
        close: ']',
    },
    LanguageBracketPair {
        open: '{',
        close: '}',
    },
];

const C_STYLE_AUTO_CLOSING_PAIRS: &[LanguageBracketPair] = &[
    LanguageBracketPair {
        open: '(',
        close: ')',
    },
    LanguageBracketPair {
        open: '[',
        close: ']',
    },
    LanguageBracketPair {
        open: '{',
        close: '}',
    },
    LanguageBracketPair {
        open: '"',
        close: '"',
    },
    LanguageBracketPair {
        open: '\'',
        close: '\'',
    },
    LanguageBracketPair {
        open: '`',
        close: '`',
    },
];

const NO_AUTO_CLOSING_PAIRS: &[LanguageBracketPair] = &[];

const RUST_INDENT_SUFFIXES: &[&str] = &["{", "(", "["];
const PYTHON_INDENT_SUFFIXES: &[&str] = &[":", "{", "(", "["];
const YAML_INDENT_SUFFIXES: &[&str] = &[":"];
const SHELL_INDENT_SUFFIXES: &[&str] = &["then", "do", "case", "{"];
const DEFAULT_INDENT_SUFFIXES: &[&str] = &["{", "(", "["];
const NO_INDENT_SUFFIXES: &[&str] = &[];
const SUPPORTED_FILE_TYPES: &str = include_str!("../supported-file-types.txt");

fn supported_file_type_entries() -> impl Iterator<Item = (&'static str, &'static str)> {
    SUPPORTED_FILE_TYPES.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let mut fields = line.split_whitespace();
        Some((fields.next()?, fields.next()?))
    })
}

pub fn supported_file_extensions() -> impl Iterator<Item = &'static str> {
    supported_file_type_entries().map(|(extension, _)| extension)
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.len() == candidate.len() && value.eq_ignore_ascii_case(candidate))
}

impl LanguageConfiguration {
    pub fn line_comment_prefix(self) -> Option<&'static str> {
        self.line_comment_prefix
    }

    pub fn brackets(self) -> &'static [LanguageBracketPair] {
        self.brackets
    }

    pub fn auto_closing_pairs(self) -> &'static [LanguageBracketPair] {
        self.auto_closing_pairs
    }

    pub fn increase_indent_after_line(self, line_prefix: &str) -> bool {
        let trimmed = line_prefix.trim_end();
        !trimmed.is_empty()
            && self
                .increase_indent_line_suffixes
                .iter()
                .any(|suffix| trimmed.ends_with(suffix))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LanguageId {
    Rust,
    Toml,
    Json,
    Sql,
    Markdown,
    PowerShell,
    Python,
    TypeScript,
    JavaScript,
    Css,
    Html,
    Yaml,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Php,
    Ruby,
    Lua,
    Dart,
    Kotlin,
    Swift,
    Vue,
    Svelte,
    Xml,
    Dockerfile,
    Terraform,
    Shell,
    Diff,
    PlainText,
}

impl LanguageId {
    pub fn from_path(path: &Path) -> Self {
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(Self::from_file_name)
            .or_else(|| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(Self::from_extension)
            })
            .unwrap_or(Self::PlainText)
    }

    fn from_file_name(file_name: &str) -> Option<Self> {
        if matches_ignore_ascii_case(file_name, &["README"]) {
            Some(Self::Markdown)
        } else if matches_ignore_ascii_case(file_name, &["Cargo.lock", "Pipfile"]) {
            Some(Self::Toml)
        } else if matches_ignore_ascii_case(file_name, &["go.mod", "go.work"]) {
            Some(Self::Go)
        } else if matches_ignore_ascii_case(
            file_name,
            &[
                ".bashrc",
                ".bash_profile",
                ".bash_login",
                ".profile",
                ".zshrc",
                ".zprofile",
            ],
        ) {
            Some(Self::Shell)
        } else if matches_ignore_ascii_case(file_name, &["Gemfile", "Rakefile", "Podfile"]) {
            Some(Self::Ruby)
        } else if dockerfile_like_file_name(file_name) {
            Some(Self::Dockerfile)
        } else {
            None
        }
    }

    fn from_extension(ext: &str) -> Self {
        supported_file_type_entries()
            .find(|(extension, _)| extension.eq_ignore_ascii_case(ext))
            .and_then(|(_, language_id)| Self::from_activation_id(language_id))
            .unwrap_or(Self::PlainText)
    }

    fn from_activation_id(language_id: &str) -> Option<Self> {
        match language_id {
            "rust" => Some(Self::Rust),
            "toml" => Some(Self::Toml),
            "json" => Some(Self::Json),
            "sql" => Some(Self::Sql),
            "markdown" => Some(Self::Markdown),
            "powershell" => Some(Self::PowerShell),
            "python" => Some(Self::Python),
            "typescript" => Some(Self::TypeScript),
            "javascript" => Some(Self::JavaScript),
            "css" => Some(Self::Css),
            "html" => Some(Self::Html),
            "yaml" => Some(Self::Yaml),
            "go" => Some(Self::Go),
            "java" => Some(Self::Java),
            "c" => Some(Self::C),
            "cpp" => Some(Self::Cpp),
            "csharp" => Some(Self::CSharp),
            "php" => Some(Self::Php),
            "ruby" => Some(Self::Ruby),
            "lua" => Some(Self::Lua),
            "dart" => Some(Self::Dart),
            "kotlin" => Some(Self::Kotlin),
            "swift" => Some(Self::Swift),
            "vue" => Some(Self::Vue),
            "svelte" => Some(Self::Svelte),
            "xml" => Some(Self::Xml),
            "dockerfile" => Some(Self::Dockerfile),
            "terraform" => Some(Self::Terraform),
            "shellscript" => Some(Self::Shell),
            "diff" => Some(Self::Diff),
            "plaintext" => Some(Self::PlainText),
            _ => None,
        }
    }

    pub fn syntect_extension(self) -> &'static str {
        match self {
            Self::Rust => "rs",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Sql => "sql",
            Self::Markdown => "md",
            Self::PowerShell => "ps1",
            Self::Python => "py",
            Self::TypeScript => "ts",
            Self::JavaScript => "js",
            Self::Css => "css",
            Self::Html => "html",
            Self::Yaml => "yaml",
            Self::Go => "go",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "cs",
            Self::Php => "php",
            Self::Ruby => "rb",
            Self::Lua => "lua",
            Self::Dart => "dart",
            Self::Kotlin => "kt",
            Self::Swift => "swift",
            Self::Vue => "vue",
            Self::Svelte => "svelte",
            Self::Xml => "xml",
            Self::Dockerfile => "Dockerfile",
            Self::Terraform => "tf",
            Self::Shell => "sh",
            Self::Diff => "diff",
            Self::PlainText => "txt",
        }
    }

    pub fn activation_id(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Sql => "sql",
            Self::Markdown => "markdown",
            Self::PowerShell => "powershell",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Css => "css",
            Self::Html => "html",
            Self::Yaml => "yaml",
            Self::Go => "go",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Php => "php",
            Self::Ruby => "ruby",
            Self::Lua => "lua",
            Self::Dart => "dart",
            Self::Kotlin => "kotlin",
            Self::Swift => "swift",
            Self::Vue => "vue",
            Self::Svelte => "svelte",
            Self::Xml => "xml",
            Self::Dockerfile => "dockerfile",
            Self::Terraform => "terraform",
            Self::Shell => "shellscript",
            Self::Diff => "diff",
            Self::PlainText => "plaintext",
        }
    }

    pub fn line_comment_prefix(self) -> Option<&'static str> {
        self.configuration().line_comment_prefix()
    }

    pub fn configuration(self) -> LanguageConfiguration {
        match self {
            Self::Rust => LanguageConfiguration {
                line_comment_prefix: Some("//"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: RUST_INDENT_SUFFIXES,
            },
            Self::TypeScript | Self::JavaScript => LanguageConfiguration {
                line_comment_prefix: Some("//"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
            Self::Go
            | Self::Java
            | Self::C
            | Self::Cpp
            | Self::CSharp
            | Self::Dart
            | Self::Kotlin
            | Self::Swift => LanguageConfiguration {
                line_comment_prefix: Some("//"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
            Self::Toml | Self::PowerShell | Self::Ruby | Self::Dockerfile | Self::Terraform => {
                LanguageConfiguration {
                    line_comment_prefix: Some("#"),
                    brackets: C_STYLE_BRACKETS,
                    auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                    increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
                }
            }
            Self::Lua => LanguageConfiguration {
                line_comment_prefix: Some("--"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
            Self::Php => LanguageConfiguration {
                line_comment_prefix: Some("//"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
            Self::Xml => LanguageConfiguration {
                line_comment_prefix: None,
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
            Self::Shell => LanguageConfiguration {
                line_comment_prefix: Some("#"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: SHELL_INDENT_SUFFIXES,
            },
            Self::Python => LanguageConfiguration {
                line_comment_prefix: Some("#"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: PYTHON_INDENT_SUFFIXES,
            },
            Self::Yaml => LanguageConfiguration {
                line_comment_prefix: Some("#"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: YAML_INDENT_SUFFIXES,
            },
            Self::Json => LanguageConfiguration {
                line_comment_prefix: None,
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
            Self::Sql => LanguageConfiguration {
                line_comment_prefix: Some("--"),
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
            Self::Markdown => LanguageConfiguration {
                line_comment_prefix: None,
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: NO_INDENT_SUFFIXES,
            },
            Self::Css | Self::Html | Self::Vue | Self::Svelte => LanguageConfiguration {
                line_comment_prefix: None,
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
            Self::Diff => LanguageConfiguration {
                line_comment_prefix: None,
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: NO_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: NO_INDENT_SUFFIXES,
            },
            Self::PlainText => LanguageConfiguration {
                line_comment_prefix: None,
                brackets: C_STYLE_BRACKETS,
                auto_closing_pairs: C_STYLE_AUTO_CLOSING_PAIRS,
                increase_indent_line_suffixes: DEFAULT_INDENT_SUFFIXES,
            },
        }
    }
}

fn dockerfile_like_file_name(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case("Dockerfile")
        || file_name.eq_ignore_ascii_case("Containerfile")
        || file_name
            .get(..11)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Dockerfile."))
        || file_name
            .get(..14)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Containerfile."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_comment_prefixes_cover_line_comment_languages() {
        assert_eq!(LanguageId::Rust.line_comment_prefix(), Some("//"));
        assert_eq!(LanguageId::JavaScript.line_comment_prefix(), Some("//"));
        assert_eq!(LanguageId::Python.line_comment_prefix(), Some("#"));
        assert_eq!(LanguageId::Toml.line_comment_prefix(), Some("#"));
        assert_eq!(LanguageId::Sql.line_comment_prefix(), Some("--"));
        assert_eq!(LanguageId::Go.line_comment_prefix(), Some("//"));
        assert_eq!(LanguageId::Java.line_comment_prefix(), Some("//"));
        assert_eq!(LanguageId::Cpp.line_comment_prefix(), Some("//"));
        assert_eq!(LanguageId::Php.line_comment_prefix(), Some("//"));
        assert_eq!(LanguageId::Lua.line_comment_prefix(), Some("--"));
        assert_eq!(LanguageId::Ruby.line_comment_prefix(), Some("#"));
        assert_eq!(LanguageId::Terraform.line_comment_prefix(), Some("#"));
        assert_eq!(LanguageId::Dockerfile.line_comment_prefix(), Some("#"));
        assert_eq!(LanguageId::Shell.line_comment_prefix(), Some("#"));
        assert_eq!(LanguageId::Yaml.line_comment_prefix(), Some("#"));
        assert_eq!(LanguageId::Json.line_comment_prefix(), None);
        assert_eq!(LanguageId::Css.line_comment_prefix(), None);
        assert_eq!(LanguageId::Html.line_comment_prefix(), None);
        assert_eq!(LanguageId::Vue.line_comment_prefix(), None);
        assert_eq!(LanguageId::Svelte.line_comment_prefix(), None);
        assert_eq!(LanguageId::Xml.line_comment_prefix(), None);
        assert_eq!(LanguageId::Diff.line_comment_prefix(), None);
        assert_eq!(LanguageId::PlainText.line_comment_prefix(), None);
    }

    #[test]
    fn language_detection_covers_common_daily_driver_file_types() {
        assert_eq!(LanguageId::from_path(Path::new("app.css")), LanguageId::Css);
        assert_eq!(
            LanguageId::from_path(Path::new("app.module.scss")),
            LanguageId::Css
        );
        assert_eq!(
            LanguageId::from_path(Path::new("settings.jsonc")),
            LanguageId::Json
        );
        assert_eq!(
            LanguageId::from_path(Path::new("index.html")),
            LanguageId::Html
        );
        assert_eq!(
            LanguageId::from_path(Path::new("docs/page.mdx")),
            LanguageId::Markdown
        );
        assert_eq!(
            LanguageId::from_path(Path::new("compose.yml")),
            LanguageId::Yaml
        );
        assert_eq!(LanguageId::from_path(Path::new("main.go")), LanguageId::Go);
        assert_eq!(
            LanguageId::from_path(Path::new("App.java")),
            LanguageId::Java
        );
        assert_eq!(LanguageId::from_path(Path::new("lib.cpp")), LanguageId::Cpp);
        assert_eq!(
            LanguageId::from_path(Path::new("include/app.h")),
            LanguageId::C
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Program.cs")),
            LanguageId::CSharp
        );
        assert_eq!(
            LanguageId::from_path(Path::new("scripts/build.sh")),
            LanguageId::Shell
        );
        assert_eq!(
            LanguageId::from_path(Path::new("index.php")),
            LanguageId::Php
        );
        assert_eq!(LanguageId::from_path(Path::new("app.rb")), LanguageId::Ruby);
        assert_eq!(
            LanguageId::from_path(Path::new("init.lua")),
            LanguageId::Lua
        );
        assert_eq!(
            LanguageId::from_path(Path::new("main.dart")),
            LanguageId::Dart
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Main.kt")),
            LanguageId::Kotlin
        );
        assert_eq!(
            LanguageId::from_path(Path::new("module.mts")),
            LanguageId::TypeScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("module.cts")),
            LanguageId::TypeScript
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Package.swift")),
            LanguageId::Swift
        );
        assert_eq!(LanguageId::from_path(Path::new("App.vue")), LanguageId::Vue);
        assert_eq!(
            LanguageId::from_path(Path::new("App.svelte")),
            LanguageId::Svelte
        );
        assert_eq!(LanguageId::from_path(Path::new("pom.xml")), LanguageId::Xml);
        assert_eq!(
            LanguageId::from_path(Path::new("main.tf")),
            LanguageId::Terraform
        );
    }

    #[test]
    fn language_detection_uses_known_filenames_without_standard_extensions() {
        assert_eq!(
            LanguageId::from_path(Path::new("README")),
            LanguageId::Markdown
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Cargo.lock")),
            LanguageId::Toml
        );
        assert_eq!(
            LanguageId::from_path(Path::new("services/api/Pipfile")),
            LanguageId::Toml
        );
        assert_eq!(LanguageId::from_path(Path::new("go.mod")), LanguageId::Go);
        assert_eq!(
            LanguageId::from_path(Path::new("workspace/go.work")),
            LanguageId::Go
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Gemfile")),
            LanguageId::Ruby
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Dockerfile")),
            LanguageId::Dockerfile
        );
        assert_eq!(
            LanguageId::from_path(Path::new("docker/Dockerfile.dev")),
            LanguageId::Dockerfile
        );
        assert_eq!(
            LanguageId::from_path(Path::new("Containerfile.test")),
            LanguageId::Dockerfile
        );
    }

    #[test]
    fn language_detection_handles_extensionless_shell_profiles() {
        assert_eq!(
            LanguageId::from_path(Path::new(".bashrc")),
            LanguageId::Shell
        );
        assert_eq!(
            LanguageId::from_path(Path::new(".bash_profile")),
            LanguageId::Shell
        );
        assert_eq!(
            LanguageId::from_path(Path::new("home/.profile")),
            LanguageId::Shell
        );
        assert_eq!(
            LanguageId::from_path(Path::new(".zshrc")),
            LanguageId::Shell
        );
    }

    #[test]
    fn language_detection_is_case_insensitive_for_extensions() {
        assert_eq!(
            LanguageId::from_path(Path::new("MAIN.RS")),
            LanguageId::Rust
        );
        assert_eq!(
            LanguageId::from_path(Path::new("README.MarkDown")),
            LanguageId::Markdown
        );
        assert_eq!(
            LanguageId::from_path(Path::new("SCRIPT.PS1")),
            LanguageId::PowerShell
        );
        assert_eq!(
            LanguageId::from_path(Path::new("component.TSX")),
            LanguageId::TypeScript
        );
    }

    #[test]
    fn supported_file_type_manifest_is_sorted_unique_and_resolvable() {
        let mut extensions = std::collections::BTreeSet::new();
        let mut previous_extension = None;

        for line in SUPPORTED_FILE_TYPES.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<_> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 2, "invalid supported file type row: {line}");
            let extension = fields[0];
            let language_id = fields[1];

            assert_eq!(extension, extension.to_ascii_lowercase());
            assert!(!extension.starts_with('.'));
            assert!(
                extension
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '_'))
            );
            if let Some(previous) = previous_extension {
                assert!(previous < extension, "manifest is not sorted");
            }
            previous_extension = Some(extension);
            assert!(
                extensions.insert(extension),
                "duplicate extension {extension}"
            );

            let language = LanguageId::from_activation_id(language_id)
                .unwrap_or_else(|| panic!("unknown language ID {language_id}"));
            assert_eq!(LanguageId::from_extension(extension), language);
            assert_eq!(language.activation_id(), language_id);
        }

        assert!(extensions.contains("txt"));
        assert_eq!(
            supported_file_extensions().collect::<Vec<_>>(),
            extensions.into_iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn language_configurations_expose_indentation_boundaries() {
        assert!(
            LanguageId::Rust
                .configuration()
                .increase_indent_after_line("fn main() {")
        );
        assert!(
            LanguageId::Python
                .configuration()
                .increase_indent_after_line("if ready:")
        );
        assert!(
            !LanguageId::PlainText
                .configuration()
                .increase_indent_after_line("label:")
        );
        assert!(
            LanguageId::Yaml
                .configuration()
                .increase_indent_after_line("services:")
        );
        assert!(
            LanguageId::Shell
                .configuration()
                .increase_indent_after_line("if ready; then")
        );
        assert_eq!(
            LanguageId::Rust.configuration().brackets()[0],
            LanguageBracketPair {
                open: '(',
                close: ')'
            }
        );
        assert!(LanguageId::Rust.configuration().auto_closing_pairs().len() > 3);
        assert!(
            LanguageId::Diff
                .configuration()
                .auto_closing_pairs()
                .is_empty()
        );
    }

    #[test]
    fn language_activation_ids_are_stable_plugin_keys() {
        assert_eq!(LanguageId::Rust.activation_id(), "rust");
        assert_eq!(LanguageId::Sql.activation_id(), "sql");
        assert_eq!(LanguageId::TypeScript.activation_id(), "typescript");
        assert_eq!(LanguageId::Go.activation_id(), "go");
        assert_eq!(LanguageId::Cpp.activation_id(), "cpp");
        assert_eq!(LanguageId::CSharp.activation_id(), "csharp");
        assert_eq!(LanguageId::Php.activation_id(), "php");
        assert_eq!(LanguageId::Ruby.activation_id(), "ruby");
        assert_eq!(LanguageId::Lua.activation_id(), "lua");
        assert_eq!(LanguageId::Dart.activation_id(), "dart");
        assert_eq!(LanguageId::Kotlin.activation_id(), "kotlin");
        assert_eq!(LanguageId::Swift.activation_id(), "swift");
        assert_eq!(LanguageId::Vue.activation_id(), "vue");
        assert_eq!(LanguageId::Svelte.activation_id(), "svelte");
        assert_eq!(LanguageId::Xml.activation_id(), "xml");
        assert_eq!(LanguageId::Dockerfile.activation_id(), "dockerfile");
        assert_eq!(LanguageId::Terraform.activation_id(), "terraform");
        assert_eq!(LanguageId::Shell.activation_id(), "shellscript");
        assert_eq!(LanguageId::PlainText.activation_id(), "plaintext");
    }
}

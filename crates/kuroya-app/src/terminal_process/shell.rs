use crate::path_display::sanitized_display_label_cow;
use portable_pty::CommandBuilder;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalShellProfile {
    pub(crate) label: String,
    pub(crate) path: String,
    pub(crate) args: Vec<String>,
}

const TERMINAL_SHELL_LABEL_MAX_CHARS: usize = 120;
const TERMINAL_SHELL_LABEL_FALLBACK: &str = "System Shell";

pub(super) fn configured_shell(shell_path: Option<&str>, shell_args: &[String]) -> CommandBuilder {
    let mut command = if let Some(path) = normalized_shell_component(shell_path) {
        CommandBuilder::new(path)
    } else if shell_args.is_empty() {
        default_shell()
    } else {
        default_shell_executable()
    };
    if !shell_args.is_empty() {
        for arg in shell_args
            .iter()
            .filter_map(|arg| normalized_shell_component(Some(arg)))
        {
            command.arg(arg);
        }
    }
    command
}

pub(super) fn configured_process(
    program: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> CommandBuilder {
    let mut command = CommandBuilder::new(program);
    command.args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    command
}

fn default_shell() -> CommandBuilder {
    let profile = default_shell_profile();
    let mut command = CommandBuilder::new(profile.path);
    command.args(profile.args);
    command
}

fn default_shell_executable() -> CommandBuilder {
    CommandBuilder::new(default_shell_profile().path)
}

fn default_shell_profile() -> TerminalShellProfile {
    #[cfg(windows)]
    {
        default_windows_shell_profile(resolve_windows_program)
    }

    #[cfg(not(windows))]
    {
        let program = default_unix_shell_program(std::env::var("SHELL").ok().as_deref(), |path| {
            Path::new(path).is_file()
        });
        let label = shell_label_from_path(&program)
            .unwrap_or_else(|| TERMINAL_SHELL_LABEL_FALLBACK.to_owned());
        TerminalShellProfile {
            label,
            path: program,
            args: Vec::new(),
        }
    }
}

pub(crate) fn detected_shell_profiles() -> Vec<TerminalShellProfile> {
    #[cfg(windows)]
    {
        detected_windows_shell_profiles(resolve_windows_program)
    }

    #[cfg(not(windows))]
    {
        detected_unix_shell_profiles(std::env::var("SHELL").ok().as_deref(), |path| {
            Path::new(path).is_file()
        })
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct ShellProfileCandidate {
    label: &'static str,
    path: &'static str,
    args: &'static [&'static str],
}

#[cfg(windows)]
const WINDOWS_FALLBACK_SHELL: ShellProfileCandidate = ShellProfileCandidate {
    label: "Windows PowerShell",
    path: "powershell.exe",
    args: &["-NoLogo"],
};

#[cfg(windows)]
const WINDOWS_DEFAULT_SHELL_CANDIDATES: &[ShellProfileCandidate] = &[
    ShellProfileCandidate {
        label: "PowerShell",
        path: "pwsh.exe",
        args: &["-NoLogo"],
    },
    WINDOWS_FALLBACK_SHELL,
];

#[cfg(windows)]
const WINDOWS_DETECTED_SHELL_CANDIDATES: &[ShellProfileCandidate] = &[
    ShellProfileCandidate {
        label: "PowerShell",
        path: "pwsh.exe",
        args: &["-NoLogo"],
    },
    WINDOWS_FALLBACK_SHELL,
    ShellProfileCandidate {
        label: "Command Prompt",
        path: "cmd.exe",
        args: &[],
    },
    ShellProfileCandidate {
        label: "Git Bash",
        path: r"C:\Program Files\Git\bin\bash.exe",
        args: &[],
    },
    ShellProfileCandidate {
        label: "Git Bash",
        path: r"C:\Program Files\Git\usr\bin\bash.exe",
        args: &[],
    },
    ShellProfileCandidate {
        label: "Bash",
        path: "bash.exe",
        args: &[],
    },
];

#[cfg(windows)]
impl ShellProfileCandidate {
    fn into_profile(self) -> TerminalShellProfile {
        shell_profile(self.label, self.path, self.args)
    }
}

#[cfg(windows)]
fn default_windows_shell_profile(
    mut resolve_program: impl FnMut(&str) -> Option<String>,
) -> TerminalShellProfile {
    WINDOWS_DEFAULT_SHELL_CANDIDATES
        .iter()
        .copied()
        .find(|candidate| resolve_program(candidate.path).is_some())
        .unwrap_or(WINDOWS_FALLBACK_SHELL)
        .into_profile()
}

#[cfg(all(windows, test))]
fn first_available_windows_shell(
    mut resolve_program: impl FnMut(&str) -> Option<String>,
) -> &'static str {
    WINDOWS_DEFAULT_SHELL_CANDIDATES
        .iter()
        .copied()
        .find(|candidate| resolve_program(candidate.path).is_some())
        .unwrap_or(WINDOWS_FALLBACK_SHELL)
        .path
}

#[cfg(windows)]
fn detected_windows_shell_profiles(
    mut resolve_program: impl FnMut(&str) -> Option<String>,
) -> Vec<TerminalShellProfile> {
    let mut profiles = Vec::new();
    let mut seen_keys = BTreeSet::new();
    for candidate in WINDOWS_DETECTED_SHELL_CANDIDATES {
        if let Some(key) = resolve_program(candidate.path) {
            push_profile_with_key(&mut profiles, &mut seen_keys, candidate.into_profile(), key);
        }
    }
    if profiles.is_empty() {
        profiles.push(WINDOWS_FALLBACK_SHELL.into_profile());
    }
    profiles
}

#[cfg(windows)]
fn resolve_windows_program(program: &str) -> Option<String> {
    let program = normalized_shell_component(Some(program))?;

    let path = Path::new(program);
    if path.is_absolute() || program.contains('/') || program.contains('\\') {
        return path.is_file().then(|| shell_profile_key(path, true));
    }

    find_program_on_path(program).map(|path| shell_profile_key(&path, true))
}

#[cfg(windows)]
fn find_program_on_path(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    find_program_on_path_in(program, path.as_os_str(), |candidate| candidate.is_file())
}

#[cfg(windows)]
fn find_program_on_path_in(
    program: &str,
    path: &std::ffi::OsStr,
    mut is_file: impl FnMut(&Path) -> bool,
) -> Option<std::path::PathBuf> {
    let candidate_names = windows_path_search_names(program);
    let mut probed_candidates = BTreeSet::new();
    for directory in std::env::split_paths(path) {
        for name in &candidate_names {
            let candidate = directory.join(name);
            if candidate.is_absolute()
                && !probed_candidates.insert(shell_probe_key(&candidate, true))
            {
                continue;
            }
            if is_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(windows)]
fn windows_path_search_names(program: &str) -> Vec<String> {
    if Path::new(program).extension().is_some() {
        return vec![program.to_owned()];
    }

    let extensions = std::env::var_os("PATHEXT");
    let extensions = extensions
        .as_deref()
        .map(|value| value.to_string_lossy())
        .unwrap_or(Cow::Borrowed(".COM;.EXE;.BAT;.CMD"));
    let mut names = Vec::new();
    for extension in extensions.split(';').map(str::trim) {
        if extension.is_empty() {
            continue;
        }
        let needs_dot = !extension.starts_with('.');
        let mut name =
            String::with_capacity(program.len() + extension.len() + usize::from(needs_dot));
        name.push_str(program);
        if needs_dot {
            name.push('.');
        }
        name.push_str(extension);
        names.push(name);
    }
    names
}

#[cfg(not(windows))]
const UNIX_SHELL_CANDIDATES: &[&str] = &[
    "/bin/zsh",
    "/usr/bin/zsh",
    "/bin/bash",
    "/usr/bin/bash",
    "/bin/fish",
    "/usr/bin/fish",
    "/bin/sh",
    "/usr/bin/sh",
];

#[cfg(not(windows))]
fn default_unix_shell_program(shell: Option<&str>, is_file: impl FnMut(&str) -> bool) -> String {
    let mut probe_cache = UnixShellProbeCache::new(is_file);
    if let Some(shell) = normalized_shell_env_path(shell) {
        if probe_cache.is_file(shell) {
            return shell.to_owned();
        }
    }
    UNIX_SHELL_CANDIDATES
        .iter()
        .copied()
        .find(|path| probe_cache.is_file(path))
        .unwrap_or("/bin/sh")
        .to_owned()
}

#[cfg(not(windows))]
fn detected_unix_shell_profiles(
    shell_env: Option<&str>,
    is_file: impl FnMut(&str) -> bool,
) -> Vec<TerminalShellProfile> {
    detected_unix_shell_profiles_in(shell_env, UNIX_SHELL_CANDIDATES, is_file)
}

#[cfg(not(windows))]
fn detected_unix_shell_profiles_in(
    shell_env: Option<&str>,
    candidates: &[&str],
    is_file: impl FnMut(&str) -> bool,
) -> Vec<TerminalShellProfile> {
    let mut profiles = Vec::new();
    let mut probe_cache = UnixShellProbeCache::new(is_file);
    let mut seen_keys = BTreeSet::new();
    let mut seen_probe_paths = BTreeSet::new();
    if let Some(path) = normalized_shell_env_path(shell_env) {
        seen_probe_paths.insert(path.to_owned());
        if probe_cache.is_file(path) {
            push_unix_profile(&mut profiles, &mut seen_keys, path);
        }
    }

    for path in candidates {
        if !seen_probe_paths.insert((*path).to_owned()) {
            continue;
        }
        if probe_cache.is_file(path) {
            push_unix_profile(&mut profiles, &mut seen_keys, path);
        }
    }

    if profiles.is_empty() {
        push_unix_profile(&mut profiles, &mut seen_keys, "/bin/sh");
    }
    profiles
}

#[cfg(not(windows))]
struct UnixShellProbeCache<F>
where
    F: FnMut(&str) -> bool,
{
    is_file: F,
    results: BTreeMap<String, bool>,
}

#[cfg(not(windows))]
impl<F> UnixShellProbeCache<F>
where
    F: FnMut(&str) -> bool,
{
    fn new(is_file: F) -> Self {
        Self {
            is_file,
            results: BTreeMap::new(),
        }
    }

    fn is_file(&mut self, path: &str) -> bool {
        if let Some(result) = self.results.get(path).copied() {
            return result;
        }
        let result = (self.is_file)(path);
        self.results.insert(path.to_owned(), result);
        result
    }
}

#[cfg(not(windows))]
fn normalized_shell_env_path(shell: Option<&str>) -> Option<&str> {
    normalized_shell_component(shell).filter(|shell| Path::new(shell).is_absolute())
}

#[cfg(not(windows))]
fn push_unix_profile(
    profiles: &mut Vec<TerminalShellProfile>,
    seen_keys: &mut BTreeSet<String>,
    path: &str,
) {
    let label =
        shell_label_from_path(path).unwrap_or_else(|| TERMINAL_SHELL_LABEL_FALLBACK.to_owned());
    push_profile_with_key(
        profiles,
        seen_keys,
        TerminalShellProfile {
            label,
            path: path.to_owned(),
            args: Vec::new(),
        },
        shell_profile_key(Path::new(path), false),
    );
}

pub(crate) fn default_shell_label() -> String {
    default_shell_profile().label
}

pub(crate) fn terminal_shell_label(shell_path: Option<&str>) -> String {
    normalized_shell_component(shell_path)
        .and_then(shell_label_from_path)
        .unwrap_or_else(default_shell_label)
}

fn shell_label_from_path(path: &str) -> Option<String> {
    let path = path.trim().trim_end_matches(['/', '\\']);
    if contains_shell_profile_control(path) {
        return None;
    }
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let label = shell_file_stem(file_name).unwrap_or(file_name).trim();
    let label = bounded_shell_label(label, "");
    (!label.is_empty()).then_some(label)
}

fn shell_file_stem(file_name: &str) -> Option<&str> {
    let (stem, extension) = file_name.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty() && !extension.chars().any(char::is_whitespace))
        .then_some(stem)
}

#[cfg(windows)]
fn shell_profile(label: &str, path: &str, args: &[&str]) -> TerminalShellProfile {
    TerminalShellProfile {
        label: bounded_shell_label(label, TERMINAL_SHELL_LABEL_FALLBACK),
        path: path.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

fn push_profile_with_key(
    profiles: &mut Vec<TerminalShellProfile>,
    seen_keys: &mut BTreeSet<String>,
    profile: TerminalShellProfile,
    key: String,
) {
    if seen_keys.insert(key) {
        profiles.push(profile);
    }
}

fn normalized_shell_component(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty() && !contains_shell_profile_control(value)).then_some(value)
}

fn contains_shell_profile_control(value: &str) -> bool {
    value.chars().any(is_shell_profile_unsafe_char)
}

fn is_shell_profile_unsafe_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

fn bounded_shell_label_cow<'a>(label: &'a str, fallback: &str) -> Cow<'a, str> {
    sanitized_display_label_cow(label, TERMINAL_SHELL_LABEL_MAX_CHARS, fallback)
}

fn bounded_shell_label(label: &str, fallback: &str) -> String {
    bounded_shell_label_cow(label, fallback).into_owned()
}

fn shell_profile_key(path: &Path, case_insensitive: bool) -> String {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    shell_probe_key(&path, case_insensitive)
}

fn shell_probe_key(path: &Path, case_insensitive: bool) -> String {
    let key = path.to_string_lossy();
    if case_insensitive {
        ascii_lowercase_cow(key)
    } else {
        key.into_owned()
    }
}

fn ascii_lowercase_cow(value: Cow<'_, str>) -> String {
    if !value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return value.into_owned();
    }
    match value {
        Cow::Borrowed(value) => value.to_ascii_lowercase(),
        Cow::Owned(mut value) => {
            value.make_ascii_lowercase();
            value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn default_shell_uses_detected_windows_powershell_without_logo() {
        use std::ffi::OsStr;

        let command = configured_shell(None, &[]);
        let argv = command.get_argv();

        assert!(matches!(
            argv.first().map(|arg| arg.as_os_str()),
            Some(program)
                if program == OsStr::new("pwsh.exe")
                    || program == OsStr::new("powershell.exe")
        ));
        assert!(
            argv.iter()
                .any(|arg| arg.as_os_str() == OsStr::new("-NoLogo"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn default_shell_label_matches_windows_profile() {
        assert!(matches!(
            default_shell_label().as_str(),
            "PowerShell" | "Windows PowerShell"
        ));
    }

    #[cfg(windows)]
    #[test]
    fn default_windows_shell_profile_label_matches_selected_program() {
        assert_eq!(
            default_windows_shell_profile(
                |program| (program == "powershell.exe").then(|| program.to_owned())
            ),
            shell_profile("Windows PowerShell", "powershell.exe", &["-NoLogo"])
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_shell_detection_prefers_pwsh_then_windows_powershell() {
        assert_eq!(
            first_available_windows_shell(
                |program| (program == "pwsh.exe").then(|| program.to_owned())
            ),
            "pwsh.exe"
        );
        assert_eq!(
            first_available_windows_shell(
                |program| (program == "powershell.exe").then(|| program.to_owned())
            ),
            "powershell.exe"
        );
        assert_eq!(first_available_windows_shell(|_| None), "powershell.exe");
    }

    #[cfg(windows)]
    #[test]
    fn detected_windows_shell_profiles_include_available_common_shells() {
        let profiles = detected_windows_shell_profiles(|program| {
            matches!(program, "pwsh.exe" | "cmd.exe" | "bash.exe").then(|| program.to_owned())
        });

        let labels = profiles
            .iter()
            .map(|profile| profile.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["PowerShell", "Command Prompt", "Bash"]);
        assert_eq!(profiles[0].path, "pwsh.exe");
        assert_eq!(profiles[0].args, ["-NoLogo"]);
    }

    #[cfg(windows)]
    #[test]
    fn detected_windows_shell_profiles_dedupe_resolved_paths() {
        let profiles = detected_windows_shell_profiles(|program| match program {
            r"C:\Program Files\Git\bin\bash.exe" | "bash.exe" => {
                Some(r"c:\program files\git\bin\bash.exe".to_owned())
            }
            _ => None,
        });

        assert_eq!(
            profiles,
            vec![shell_profile(
                "Git Bash",
                r"C:\Program Files\Git\bin\bash.exe",
                &[]
            )]
        );
    }

    #[cfg(windows)]
    #[test]
    fn detected_windows_shell_profiles_fall_back_to_windows_powershell() {
        let profiles = detected_windows_shell_profiles(|_| None);

        assert_eq!(
            profiles,
            vec![shell_profile(
                "Windows PowerShell",
                "powershell.exe",
                &["-NoLogo"]
            )]
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_search_skips_duplicate_absolute_candidates_case_insensitively() {
        let path = std::ffi::OsString::from(r"C:\Tools;c:\tools");
        let mut probed = Vec::new();

        let found = find_program_on_path_in("pwsh.exe", path.as_os_str(), |candidate| {
            probed.push(candidate.to_path_buf());
            false
        });

        assert_eq!(found, None);
        assert_eq!(probed, vec![std::path::PathBuf::from(r"C:\Tools\pwsh.exe")]);
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_shell_detection_uses_non_empty_shell_env() {
        assert_eq!(
            default_unix_shell_program(Some(" /bin/zsh "), |path| path == "/bin/zsh"),
            "/bin/zsh"
        );
        assert_eq!(default_unix_shell_program(Some(" "), |_| false), "/bin/sh");
        assert_eq!(default_unix_shell_program(None, |_| false), "/bin/sh");
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_default_shell_reuses_duplicate_shell_candidate_probe() {
        let mut probed = Vec::new();

        let program = default_unix_shell_program(Some(" /bin/zsh "), |path| {
            probed.push(path.to_owned());
            path == "/bin/bash"
        });

        assert_eq!(program, "/bin/bash");
        assert_eq!(
            probed
                .iter()
                .filter(|path| path.as_str() == "/bin/zsh")
                .count(),
            1
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_shell_detection_rejects_invalid_shell_env() {
        assert_eq!(
            default_unix_shell_program(Some(" /missing/zsh "), |path| path == "/bin/bash"),
            "/bin/bash"
        );
        assert_eq!(
            default_unix_shell_program(Some(" bash "), |path| path == "/bin/bash"),
            "/bin/bash"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_shell_detection_rejects_control_character_shell_env() {
        assert_eq!(
            default_unix_shell_program(Some(" /bin/zsh\n--login "), |path| path == "/bin/bash"),
            "/bin/bash"
        );

        let profiles =
            detected_unix_shell_profiles_in(Some(" /bin/zsh\u{7} "), &["/bin/bash"], |path| {
                matches!(path, "/bin/zsh" | "/bin/bash")
            });

        let paths = profiles
            .iter()
            .map(|profile| profile.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["/bin/bash"]);
    }

    #[cfg(not(windows))]
    #[test]
    fn detected_unix_shell_profiles_include_shell_env_first_and_dedupe() {
        let profiles = detected_unix_shell_profiles(Some(" /bin/zsh "), |path| {
            matches!(path, "/bin/zsh" | "/bin/bash")
        });

        let paths = profiles
            .iter()
            .map(|profile| profile.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["/bin/zsh", "/bin/bash"]);
        assert_eq!(profiles[0].label, "zsh");
    }

    #[cfg(not(windows))]
    #[test]
    fn detected_unix_shell_profiles_skips_duplicate_shell_candidate_probe() {
        let mut probed = Vec::new();

        let profiles = detected_unix_shell_profiles(Some(" /bin/zsh "), |path| {
            probed.push(path.to_owned());
            matches!(path, "/bin/zsh" | "/bin/bash")
        });

        let paths = profiles
            .iter()
            .map(|profile| profile.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["/bin/zsh", "/bin/bash"]);
        assert_eq!(
            probed
                .iter()
                .filter(|path| path.as_str() == "/bin/zsh")
                .count(),
            1
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn detected_unix_shell_profiles_reject_invalid_shell_env() {
        let profiles =
            detected_unix_shell_profiles(Some(" /missing/zsh "), |path| path == "/bin/bash");

        let paths = profiles
            .iter()
            .map(|profile| profile.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["/bin/bash"]);
    }

    #[cfg(not(windows))]
    #[test]
    fn detected_unix_shell_profiles_fall_back_to_sh() {
        let profiles = detected_unix_shell_profiles(None, |_| false);

        assert_eq!(
            profiles,
            vec![TerminalShellProfile {
                label: "sh".to_owned(),
                path: "/bin/sh".to_owned(),
                args: Vec::new(),
            }]
        );
    }

    #[test]
    fn configured_shell_uses_custom_path_and_args() {
        use std::ffi::OsStr;

        let command = configured_shell(
            Some("pwsh.exe"),
            &["-NoLogo".to_owned(), "-NoProfile".to_owned()],
        );
        let argv = command.get_argv();

        assert_eq!(
            argv.first().map(|arg| arg.as_os_str()),
            Some(OsStr::new("pwsh.exe"))
        );
        assert!(
            argv.iter()
                .any(|arg| arg.as_os_str() == OsStr::new("-NoProfile"))
        );
    }

    #[test]
    fn configured_shell_rejects_control_character_path_and_args() {
        use std::ffi::OsStr;

        let command = configured_shell(
            Some("pwsh.exe\n-NoProfile"),
            &[
                " -NoLogo ".to_owned(),
                "\u{7}".to_owned(),
                "-NoProfile".to_owned(),
            ],
        );
        let argv = command.get_argv();

        assert_ne!(
            argv.first().map(|arg| arg.as_os_str()),
            Some(OsStr::new("pwsh.exe\n-NoProfile"))
        );
        assert!(
            argv.iter()
                .any(|arg| arg.as_os_str() == OsStr::new("-NoLogo"))
        );
        assert!(
            argv.iter()
                .any(|arg| arg.as_os_str() == OsStr::new("-NoProfile"))
        );
        assert!(
            !argv
                .iter()
                .any(|arg| arg.as_os_str() == OsStr::new("\u{7}"))
        );
    }

    #[test]
    fn configured_shell_rejects_bidi_profile_path_and_args() {
        use std::ffi::OsStr;

        let command = configured_shell(
            Some("pwsh.exe\u{202e}"),
            &["-NoLogo".to_owned(), "-NoProfile\u{2066}".to_owned()],
        );
        let argv = command.get_argv();

        assert_ne!(
            argv.first().map(|arg| arg.as_os_str()),
            Some(OsStr::new("pwsh.exe\u{202e}"))
        );
        assert!(
            argv.iter()
                .any(|arg| arg.as_os_str() == OsStr::new("-NoLogo"))
        );
        assert!(
            !argv
                .iter()
                .any(|arg| arg.as_os_str() == OsStr::new("-NoProfile\u{2066}"))
        );
    }

    #[test]
    fn configured_process_preserves_raw_program_args_and_env() {
        use std::ffi::OsStr;

        let raw_arg = " \x1b[31mraw\r\n\u{7} ";
        let raw_env = " line\r\nvalue\u{202e} ";
        let command = configured_process(
            "cargo",
            &["test".to_owned(), raw_arg.to_owned()],
            &BTreeMap::from([
                ("RAW_ENV".to_owned(), raw_env.to_owned()),
                ("RUST_BACKTRACE".to_owned(), "1".to_owned()),
            ]),
        );
        let argv = command.get_argv();

        assert_eq!(
            argv.first().map(|arg| arg.as_os_str()),
            Some(OsStr::new("cargo"))
        );
        assert!(argv.iter().any(|arg| arg.as_os_str() == OsStr::new("test")));
        assert!(
            argv.iter()
                .any(|arg| arg.as_os_str() == OsStr::new(raw_arg))
        );
        assert_eq!(command.get_env("RAW_ENV"), Some(OsStr::new(raw_env)));
        assert_eq!(command.get_env("RUST_BACKTRACE"), Some(OsStr::new("1")));
    }

    #[test]
    fn terminal_shell_label_prefers_configured_executable_name() {
        assert_eq!(
            terminal_shell_label(Some(r"C:\Program Files\PowerShell\7\pwsh.exe")),
            "pwsh"
        );
        assert_eq!(terminal_shell_label(Some("")), default_shell_label());
    }

    #[test]
    fn terminal_shell_label_bounds_configured_executable_name() {
        let path = format!(r"C:\Tools\{}.exe", "shell-".repeat(48));

        let label = terminal_shell_label(Some(&path));

        assert!(label.contains("..."));
        assert!(label.chars().count() <= TERMINAL_SHELL_LABEL_MAX_CHARS);
        assert!(!label.ends_with(".exe"));
    }

    #[test]
    fn terminal_shell_label_cow_borrows_clean_ascii_and_unicode_labels() {
        assert!(matches!(
            bounded_shell_label_cow("pwsh", TERMINAL_SHELL_LABEL_FALLBACK),
            Cow::Borrowed("pwsh")
        ));

        let unicode = "PowerShell \u{03bb}";
        match bounded_shell_label_cow(unicode, TERMINAL_SHELL_LABEL_FALLBACK) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed shell label, got {label:?}"),
        }
    }

    #[test]
    fn terminal_shell_label_cow_owns_dirty_truncated_and_fallback_labels() {
        let dirty = bounded_shell_label_cow("pwsh\n-NoProfile", TERMINAL_SHELL_LABEL_FALLBACK);
        assert_eq!(
            dirty.as_ref(),
            bounded_shell_label("pwsh\n-NoProfile", TERMINAL_SHELL_LABEL_FALLBACK)
        );
        assert!(matches!(dirty, Cow::Owned(_)));

        let long = "shell-".repeat(48);
        let truncated = bounded_shell_label_cow(&long, TERMINAL_SHELL_LABEL_FALLBACK);
        assert!(truncated.contains("..."));
        assert!(truncated.chars().count() <= TERMINAL_SHELL_LABEL_MAX_CHARS);
        assert!(matches!(truncated, Cow::Owned(_)));

        let fallback = bounded_shell_label_cow("   ", TERMINAL_SHELL_LABEL_FALLBACK);
        assert_eq!(fallback, TERMINAL_SHELL_LABEL_FALLBACK);
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[test]
    fn terminal_shell_label_string_wrapper_matches_cow_output() {
        let cases = [
            "pwsh",
            "PowerShell \u{03bb}",
            "pwsh\n-NoProfile",
            "   ",
            "shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-shell-",
        ];

        for label in cases {
            assert_eq!(
                bounded_shell_label(label, TERMINAL_SHELL_LABEL_FALLBACK),
                bounded_shell_label_cow(label, TERMINAL_SHELL_LABEL_FALLBACK).as_ref()
            );
        }
    }

    #[test]
    fn terminal_shell_label_keeps_extension_when_suffix_looks_like_arguments() {
        assert_eq!(
            terminal_shell_label(Some("pwsh.exe -NoProfile")),
            "pwsh.exe -NoProfile"
        );
    }

    #[test]
    fn terminal_shell_label_rejects_control_character_paths() {
        assert_eq!(
            terminal_shell_label(Some("pwsh.exe\n-NoProfile")),
            default_shell_label()
        );
        assert_eq!(
            terminal_shell_label(Some("pwsh.exe\u{7}")),
            default_shell_label()
        );
    }

    #[test]
    fn terminal_shell_label_rejects_bidi_paths() {
        assert_eq!(
            terminal_shell_label(Some("pwsh.exe\u{202e}")),
            default_shell_label()
        );
    }
}

use crate::{WorkspaceTaskKind, command::Command};
use serde::{Deserialize, Serialize};

pub const KEYMAP_MAX_BINDINGS: usize = 512;
pub const KEYMAP_CHORD_MAX_CHARS: usize = 64;
pub const KEYMAP_CHORD_MAX_BYTES: usize = KEYMAP_CHORD_MAX_CHARS * 4;
const KEYMAP_CHORD_MAX_PARTS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    pub chord: String,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    pub bindings: Vec<KeyBinding>,
}

impl Keymap {
    pub fn sanitize(&mut self) -> usize {
        let mut sanitized = Vec::with_capacity(self.bindings.len().min(KEYMAP_MAX_BINDINGS));
        let mut changes = 0usize;

        for binding in self.bindings.drain(..) {
            if sanitized.len() >= KEYMAP_MAX_BINDINGS {
                changes += 1;
                continue;
            }

            let mut command = binding.command;
            let command_changed = command.normalize_keymap_metadata();
            if !command.is_stable_keymap_command() {
                changes += 1;
                continue;
            }

            let Some(chord) = normalize_keymap_chord(&binding.chord) else {
                changes += 1;
                continue;
            };

            if sanitized
                .iter()
                .any(|kept: &KeyBinding| kept.command == command || kept.chord == chord)
            {
                changes += 1;
                continue;
            }

            if command_changed || binding.chord != chord {
                changes += 1;
            }
            sanitized.push(KeyBinding { chord, command });
        }

        self.bindings = sanitized;
        changes
    }

    pub fn ensure_default_command_palette_binding(&mut self) -> bool {
        const DEFAULT_CHORD: &str = "Ctrl+Shift+P";

        if self.bindings.len() >= KEYMAP_MAX_BINDINGS
            || self
                .bindings
                .iter()
                .any(|binding| binding.command == Command::ToggleCommandPalette)
            || self
                .bindings
                .iter()
                .any(|binding| binding.chord == DEFAULT_CHORD)
        {
            return false;
        }

        self.bindings.push(KeyBinding {
            chord: DEFAULT_CHORD.to_owned(),
            command: Command::ToggleCommandPalette,
        });
        true
    }
}

pub fn normalize_keymap_chord(chord: &str) -> Option<String> {
    let parsed = parse_keymap_chord(chord)?;
    let mut normalized = String::with_capacity(parsed.key.name.len() + 20);
    if parsed.has_ctrl {
        push_keymap_chord_part(&mut normalized, "Ctrl");
    }
    if parsed.has_alt {
        push_keymap_chord_part(&mut normalized, "Alt");
    }
    if parsed.has_shift {
        push_keymap_chord_part(&mut normalized, "Shift");
    }
    if parsed.has_command {
        push_keymap_chord_part(&mut normalized, "Cmd");
    }
    push_keymap_chord_part(&mut normalized, parsed.key.name);
    Some(normalized)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedKeymapChord {
    key: ParsedKeymapKey,
    has_ctrl: bool,
    has_shift: bool,
    has_alt: bool,
    has_command: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedKeymapKey {
    name: &'static str,
    requires_primary_modifier: bool,
}

fn parse_keymap_chord(chord: &str) -> Option<ParsedKeymapChord> {
    if chord.len() > KEYMAP_CHORD_MAX_BYTES {
        return None;
    }

    if chord.chars().count() > KEYMAP_CHORD_MAX_CHARS {
        return None;
    }

    let mut key = None;
    let mut has_ctrl = false;
    let mut has_shift = false;
    let mut has_alt = false;
    let mut has_command = false;
    let mut part_count = 0usize;

    for part in chord.split('+').map(str::trim) {
        part_count += 1;
        if part_count > KEYMAP_CHORD_MAX_PARTS || part.is_empty() {
            return None;
        }

        if keymap_chord_part_has_hidden_control(part) {
            return None;
        }

        if part.eq_ignore_ascii_case("ctrl") || part.eq_ignore_ascii_case("control") {
            if has_ctrl {
                return None;
            }
            has_ctrl = true;
        } else if part.eq_ignore_ascii_case("shift") {
            if has_shift {
                return None;
            }
            has_shift = true;
        } else if part.eq_ignore_ascii_case("alt") || part.eq_ignore_ascii_case("option") {
            if has_alt {
                return None;
            }
            has_alt = true;
        } else if part.eq_ignore_ascii_case("cmd")
            || part.eq_ignore_ascii_case("command")
            || part.eq_ignore_ascii_case("super")
        {
            if has_command {
                return None;
            }
            has_command = true;
        } else {
            let parsed_key = parse_keymap_key_name(part)?;
            if key.replace(parsed_key).is_some() {
                return None;
            }
        }
    }

    let key = key?;
    if key.requires_primary_modifier && !(has_ctrl || has_alt || has_command) {
        return None;
    }

    Some(ParsedKeymapChord {
        key,
        has_ctrl,
        has_shift,
        has_alt,
        has_command,
    })
}

fn parse_keymap_key_name(name: &str) -> Option<ParsedKeymapKey> {
    parse_keymap_single_char_key_name(name)
        .or_else(|| parse_keymap_function_key_name(name))
        .or_else(|| parse_keymap_named_key_name(name))
}

fn parse_keymap_single_char_key_name(name: &str) -> Option<ParsedKeymapKey> {
    if name.len() != 1 {
        return None;
    }

    let key_name = match name.as_bytes()[0].to_ascii_lowercase() {
        b'a' => "A",
        b'b' => "B",
        b'c' => "C",
        b'd' => "D",
        b'e' => "E",
        b'f' => "F",
        b'g' => "G",
        b'h' => "H",
        b'i' => "I",
        b'j' => "J",
        b'k' => "K",
        b'l' => "L",
        b'm' => "M",
        b'n' => "N",
        b'o' => "O",
        b'p' => "P",
        b'q' => "Q",
        b'r' => "R",
        b's' => "S",
        b't' => "T",
        b'u' => "U",
        b'v' => "V",
        b'w' => "W",
        b'x' => "X",
        b'y' => "Y",
        b'z' => "Z",
        b'0' => "0",
        b'1' => "1",
        b'2' => "2",
        b'3' => "3",
        b'4' => "4",
        b'5' => "5",
        b'6' => "6",
        b'7' => "7",
        b'8' => "8",
        b'9' => "9",
        b'`' => "`",
        b'\\' => "\\",
        b'[' => "[",
        b']' => "]",
        b',' => ",",
        b'.' => ".",
        b'/' => "/",
        b'-' => "-",
        b'=' => "=",
        b';' => ";",
        b'\'' => "'",
        _ => return None,
    };

    Some(ParsedKeymapKey {
        name: key_name,
        requires_primary_modifier: true,
    })
}

fn parse_keymap_function_key_name(name: &str) -> Option<ParsedKeymapKey> {
    let (prefix, number) = name.as_bytes().split_first()?;
    if !prefix.eq_ignore_ascii_case(&b'f') {
        return None;
    }

    let key_name = match number {
        [b'1'] => "F1",
        [b'2'] => "F2",
        [b'3'] => "F3",
        [b'4'] => "F4",
        [b'5'] => "F5",
        [b'6'] => "F6",
        [b'7'] => "F7",
        [b'8'] => "F8",
        [b'9'] => "F9",
        [b'1', b'0'] => "F10",
        [b'1', b'1'] => "F11",
        [b'1', b'2'] => "F12",
        _ => return None,
    };

    Some(ParsedKeymapKey {
        name: key_name,
        requires_primary_modifier: false,
    })
}

fn parse_keymap_named_key_name(name: &str) -> Option<ParsedKeymapKey> {
    if keymap_key_name_matches(name, &["up", "arrowup"]) {
        keymap_navigation_key("Up")
    } else if keymap_key_name_matches(name, &["down", "arrowdown"]) {
        keymap_navigation_key("Down")
    } else if keymap_key_name_matches(name, &["left", "arrowleft"]) {
        keymap_navigation_key("Left")
    } else if keymap_key_name_matches(name, &["right", "arrowright"]) {
        keymap_navigation_key("Right")
    } else if keymap_key_name_matches(name, &["home"]) {
        keymap_navigation_key("Home")
    } else if keymap_key_name_matches(name, &["end"]) {
        keymap_navigation_key("End")
    } else if keymap_key_name_matches(name, &["pageup"]) {
        keymap_navigation_key("PageUp")
    } else if keymap_key_name_matches(name, &["pagedown"]) {
        keymap_navigation_key("PageDown")
    } else if keymap_key_name_matches(name, &["enter"]) {
        keymap_text_key("Enter")
    } else if keymap_key_name_matches(name, &["tab"]) {
        keymap_text_key("Tab")
    } else if keymap_key_name_matches(name, &["space"]) {
        keymap_text_key("Space")
    } else if keymap_key_name_matches(name, &["escape", "esc"]) {
        keymap_navigation_key("Escape")
    } else if keymap_key_name_matches(name, &["backspace"]) {
        keymap_text_key("Backspace")
    } else if keymap_key_name_matches(name, &["delete", "del"]) {
        keymap_text_key("Delete")
    } else if keymap_key_name_matches(name, &["backtick"]) {
        keymap_text_key("`")
    } else if keymap_key_name_matches(name, &["backslash"]) {
        keymap_text_key("\\")
    } else if keymap_key_name_matches(name, &["openbracket", "leftbracket"]) {
        keymap_text_key("[")
    } else if keymap_key_name_matches(name, &["closebracket", "rightbracket"]) {
        keymap_text_key("]")
    } else if keymap_key_name_matches(name, &["comma"]) {
        keymap_text_key(",")
    } else if keymap_key_name_matches(name, &["period"]) {
        keymap_text_key(".")
    } else if keymap_key_name_matches(name, &["slash"]) {
        keymap_text_key("/")
    } else if keymap_key_name_matches(name, &["minus"]) {
        keymap_text_key("-")
    } else if keymap_key_name_matches(name, &["equals"]) {
        keymap_text_key("=")
    } else if keymap_key_name_matches(name, &["semicolon"]) {
        keymap_text_key(";")
    } else if keymap_key_name_matches(name, &["quote"]) {
        keymap_text_key("'")
    } else {
        None
    }
}

fn keymap_navigation_key(name: &'static str) -> Option<ParsedKeymapKey> {
    Some(ParsedKeymapKey {
        name,
        requires_primary_modifier: false,
    })
}

fn keymap_text_key(name: &'static str) -> Option<ParsedKeymapKey> {
    Some(ParsedKeymapKey {
        name,
        requires_primary_modifier: true,
    })
}

fn keymap_key_name_matches(name: &str, aliases: &[&str]) -> bool {
    aliases.iter().any(|alias| name.eq_ignore_ascii_case(alias))
}

fn keymap_chord_part_has_hidden_control(part: &str) -> bool {
    part.chars().any(|ch| {
        ch.is_control()
            || matches!(
                ch,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{202a}'..='\u{202e}'
                    | '\u{2066}'..='\u{2069}'
            )
    })
}

fn push_keymap_chord_part(chord: &mut String, part: &str) {
    if !chord.is_empty() {
        chord.push('+');
    }
    chord.push_str(part);
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: vec![
                KeyBinding {
                    chord: "Ctrl+P".to_owned(),
                    command: Command::ToggleQuickOpen,
                },
                KeyBinding {
                    chord: "Ctrl+F".to_owned(),
                    command: Command::ToggleBufferFind,
                },
                KeyBinding {
                    chord: "Ctrl+G".to_owned(),
                    command: Command::ToggleGoToLine,
                },
                KeyBinding {
                    chord: "Ctrl+L".to_owned(),
                    command: Command::SelectLines,
                },
                KeyBinding {
                    chord: "Ctrl+Alt+L".to_owned(),
                    command: Command::SelectRectangularBlock,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+Right".to_owned(),
                    command: Command::ExpandSelection,
                },
                KeyBinding {
                    chord: "Ctrl+D".to_owned(),
                    command: Command::SelectNextOccurrence,
                },
                KeyBinding {
                    chord: "Ctrl+Alt+D".to_owned(),
                    command: Command::SelectAllOccurrences,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+\\".to_owned(),
                    command: Command::GoToMatchingBracket,
                },
                KeyBinding {
                    chord: "Ctrl+/".to_owned(),
                    command: Command::ToggleLineComment,
                },
                KeyBinding {
                    chord: "F3".to_owned(),
                    command: Command::FindNext,
                },
                KeyBinding {
                    chord: "Shift+F3".to_owned(),
                    command: Command::FindPrevious,
                },
                KeyBinding {
                    chord: "F4".to_owned(),
                    command: Command::NextProjectSearchResult,
                },
                KeyBinding {
                    chord: "Shift+F4".to_owned(),
                    command: Command::PreviousProjectSearchResult,
                },
                KeyBinding {
                    chord: "F7".to_owned(),
                    command: Command::NextDiffHunk,
                },
                KeyBinding {
                    chord: "Shift+F7".to_owned(),
                    command: Command::PreviousDiffHunk,
                },
                KeyBinding {
                    chord: "F8".to_owned(),
                    command: Command::NextDiagnostic,
                },
                KeyBinding {
                    chord: "Shift+F8".to_owned(),
                    command: Command::PreviousDiagnostic,
                },
                KeyBinding {
                    chord: "Alt+F5".to_owned(),
                    command: Command::NextGitChange,
                },
                KeyBinding {
                    chord: "Alt+Shift+F5".to_owned(),
                    command: Command::PreviousGitChange,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+G".to_owned(),
                    command: Command::ToggleSourceControl,
                },
                KeyBinding {
                    chord: "Alt+Left".to_owned(),
                    command: Command::NavigateBack,
                },
                KeyBinding {
                    chord: "Alt+Right".to_owned(),
                    command: Command::NavigateForward,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+L".to_owned(),
                    command: Command::RequestDocumentHighlights,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+H".to_owned(),
                    command: Command::RequestHover,
                },
                KeyBinding {
                    chord: "F12".to_owned(),
                    command: Command::GoToDefinition,
                },
                KeyBinding {
                    chord: "Shift+F12".to_owned(),
                    command: Command::FindReferences,
                },
                KeyBinding {
                    chord: "F2".to_owned(),
                    command: Command::RenameSymbol,
                },
                KeyBinding {
                    chord: "Ctrl+Alt+O".to_owned(),
                    command: Command::ToggleSymbolsPanel,
                },
                KeyBinding {
                    chord: "Ctrl+T".to_owned(),
                    command: Command::ToggleWorkspaceSymbols,
                },
                KeyBinding {
                    chord: "Ctrl+Space".to_owned(),
                    command: Command::RequestCompletions,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+Space".to_owned(),
                    command: Command::RequestSignatureHelp,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+F9".to_owned(),
                    command: Command::ToggleFold,
                },
                KeyBinding {
                    chord: "Ctrl+Alt+F9".to_owned(),
                    command: Command::ExpandAllFolds,
                },
                KeyBinding {
                    chord: "Alt+Shift+F".to_owned(),
                    command: Command::FormatDocument,
                },
                KeyBinding {
                    chord: "Ctrl+.".to_owned(),
                    command: Command::RequestCodeActions,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+P".to_owned(),
                    command: Command::ToggleCommandPalette,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+M".to_owned(),
                    command: Command::ToggleDiagnosticsPanel,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+F".to_owned(),
                    command: Command::ToggleProjectSearch,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+O".to_owned(),
                    command: Command::OpenWorkspacePrompt,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+B".to_owned(),
                    command: Command::RunWorkspaceTaskKind(WorkspaceTaskKind::Build),
                },
                KeyBinding {
                    chord: "Ctrl+S".to_owned(),
                    command: Command::SaveActive,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+S".to_owned(),
                    command: Command::SaveAs,
                },
                KeyBinding {
                    chord: "Ctrl+Alt+S".to_owned(),
                    command: Command::SaveAll,
                },
                KeyBinding {
                    chord: "Ctrl+N".to_owned(),
                    command: Command::NewFile,
                },
                KeyBinding {
                    chord: "Ctrl+Alt+R".to_owned(),
                    command: Command::ReloadActiveFromDisk,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+,".to_owned(),
                    command: Command::ReloadSettings,
                },
                KeyBinding {
                    chord: "Ctrl+,".to_owned(),
                    command: Command::OpenSettingsFile,
                },
                KeyBinding {
                    chord: "Ctrl+;".to_owned(),
                    command: Command::ToggleSettingsPanel,
                },
                KeyBinding {
                    chord: "Ctrl+Alt+K".to_owned(),
                    command: Command::ToggleKeybindingsPanel,
                },
                KeyBinding {
                    chord: "Ctrl+Alt+T".to_owned(),
                    command: Command::ToggleThemePicker,
                },
                KeyBinding {
                    chord: "Ctrl+Tab".to_owned(),
                    command: Command::NextTab,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+Tab".to_owned(),
                    command: Command::PreviousTab,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+T".to_owned(),
                    command: Command::ReopenClosedFile,
                },
                KeyBinding {
                    chord: "Ctrl+W".to_owned(),
                    command: Command::CloseActive,
                },
                KeyBinding {
                    chord: "Ctrl+Z".to_owned(),
                    command: Command::Undo,
                },
                KeyBinding {
                    chord: "Ctrl+Y".to_owned(),
                    command: Command::Redo,
                },
                KeyBinding {
                    chord: "Ctrl+\\".to_owned(),
                    command: Command::SplitEditorRight,
                },
                KeyBinding {
                    chord: "Ctrl+]".to_owned(),
                    command: Command::IndentLines,
                },
                KeyBinding {
                    chord: "Ctrl+[".to_owned(),
                    command: Command::OutdentLines,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+D".to_owned(),
                    command: Command::DuplicateLines,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+K".to_owned(),
                    command: Command::DeleteLines,
                },
                KeyBinding {
                    chord: "Ctrl+J".to_owned(),
                    command: Command::JoinLines,
                },
                KeyBinding {
                    chord: "Alt+Shift+Up".to_owned(),
                    command: Command::MoveLineUp,
                },
                KeyBinding {
                    chord: "Alt+Shift+Down".to_owned(),
                    command: Command::MoveLineDown,
                },
                KeyBinding {
                    chord: "Ctrl+`".to_owned(),
                    command: Command::ToggleTerminal,
                },
                KeyBinding {
                    chord: "Alt+F".to_owned(),
                    command: Command::ToggleTerminalSearch,
                },
                KeyBinding {
                    chord: "Ctrl+PageDown".to_owned(),
                    command: Command::NextTerminalSession,
                },
                KeyBinding {
                    chord: "Ctrl+PageUp".to_owned(),
                    command: Command::PreviousTerminalSession,
                },
                KeyBinding {
                    chord: "Alt+Up".to_owned(),
                    command: Command::AddCursorAbove,
                },
                KeyBinding {
                    chord: "Alt+Down".to_owned(),
                    command: Command::AddCursorBelow,
                },
                KeyBinding {
                    chord: "Alt+Shift+I".to_owned(),
                    command: Command::AddCursorsToLineEnds,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn normalize_keymap_chord_accepts_supported_aliases_and_rejects_malformed_input() {
        assert_eq!(
            normalize_keymap_chord(" shift + control + p "),
            Some("Ctrl+Shift+P".to_owned())
        );
        assert_eq!(
            normalize_keymap_chord("OPTION + arrowleft"),
            Some("Alt+Left".to_owned())
        );
        assert_eq!(
            normalize_keymap_chord("command + slash"),
            Some("Cmd+/".to_owned())
        );
        assert_eq!(normalize_keymap_chord("Esc"), Some("Escape".to_owned()));

        for chord in [
            "+P",
            "Ctrl++P",
            "Ctrl+Unknown+P",
            "Ctrl+P+Q",
            "Ctrl+Control+P",
            "Alt+Option+P",
            "Shift+Shift+P",
            "Cmd+Super+P",
            "P",
            "Ctrl+\u{202e}P",
        ] {
            assert_eq!(normalize_keymap_chord(chord), None, "{chord:?}");
        }

        let long_chord = format!("Ctrl+{}", "P".repeat(KEYMAP_CHORD_MAX_CHARS));
        assert_eq!(normalize_keymap_chord(&long_chord), None);
    }

    #[test]
    fn keymap_sanitize_normalizes_and_prunes_stale_duplicate_bindings() {
        let mut keymap = Keymap {
            bindings: vec![
                KeyBinding {
                    chord: " shift + control + p ".to_owned(),
                    command: Command::ToggleQuickOpen,
                },
                KeyBinding {
                    chord: "Ctrl+Shift+P".to_owned(),
                    command: Command::ToggleCommandPalette,
                },
                KeyBinding {
                    chord: "Ctrl+P".to_owned(),
                    command: Command::ToggleQuickOpen,
                },
                KeyBinding {
                    chord: "Ctrl+Unknown+P".to_owned(),
                    command: Command::ToggleTerminal,
                },
                KeyBinding {
                    chord: "P".to_owned(),
                    command: Command::Redo,
                },
                KeyBinding {
                    chord: "Ctrl+O".to_owned(),
                    command: Command::OpenFile(PathBuf::from("src/main.rs")),
                },
                KeyBinding {
                    chord: "Esc".to_owned(),
                    command: Command::ToggleTerminal,
                },
                KeyBinding {
                    chord: " command + slash ".to_owned(),
                    command: Command::RunPluginCommand {
                        plugin_id: " example.plugin ".to_owned(),
                        command_id: " command:run ".to_owned(),
                    },
                },
            ],
        };

        assert_eq!(keymap.sanitize(), 8);
        assert_eq!(
            keymap.bindings,
            vec![
                KeyBinding {
                    chord: "Ctrl+Shift+P".to_owned(),
                    command: Command::ToggleQuickOpen,
                },
                KeyBinding {
                    chord: "Escape".to_owned(),
                    command: Command::ToggleTerminal,
                },
                KeyBinding {
                    chord: "Cmd+/".to_owned(),
                    command: Command::RunPluginCommand {
                        plugin_id: "example.plugin".to_owned(),
                        command_id: "command:run".to_owned(),
                    },
                },
            ]
        );
    }

    #[test]
    fn default_keymap_chords_are_canonical_unique_and_keymap_safe() {
        let mut keymap = Keymap::default();
        let mut seen_chords = Vec::new();
        let mut seen_commands = Vec::new();

        assert_eq!(keymap.sanitize(), 0);
        for binding in &keymap.bindings {
            assert_eq!(
                normalize_keymap_chord(&binding.chord),
                Some(binding.chord.clone()),
                "{:?}",
                binding.command
            );
            assert!(
                binding.command.is_stable_keymap_command(),
                "{:?}",
                binding.command
            );
            assert!(
                !seen_chords.iter().any(|chord| chord == &binding.chord),
                "duplicate chord {}",
                binding.chord
            );
            assert!(
                !seen_commands
                    .iter()
                    .any(|command| command == &binding.command),
                "duplicate command {:?}",
                binding.command
            );
            seen_chords.push(binding.chord.clone());
            seen_commands.push(binding.command.clone());
        }
    }

    #[test]
    fn default_keymap_includes_navigation_history_shortcuts() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Alt+Left" && binding.command == Command::NavigateBack
        }));
        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Alt+Right" && binding.command == Command::NavigateForward
        }));
    }

    #[test]
    fn default_keymap_includes_reopen_closed_file_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+Shift+T" && binding.command == Command::ReopenClosedFile
        }));
    }

    #[test]
    fn default_keymap_includes_new_file_shortcut() {
        let keymap = Keymap::default();

        let new_file_bindings: Vec<&KeyBinding> = keymap
            .bindings
            .iter()
            .filter(|binding| binding.command == Command::NewFile)
            .collect();
        assert_eq!(new_file_bindings.len(), 1);
        assert_eq!(new_file_bindings[0].chord, "Ctrl+N");
        assert_eq!(
            normalize_keymap_chord("Ctrl+N").as_deref(),
            Some(new_file_bindings[0].chord.as_str())
        );

        assert_eq!(
            keymap
                .bindings
                .iter()
                .filter(|binding| binding.chord == "Ctrl+N")
                .count(),
            1
        );
    }

    #[test]
    fn default_keymap_includes_close_active_file_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+W" && binding.command == Command::CloseActive
        }));
    }

    #[test]
    fn default_keymap_includes_build_task_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+Shift+B"
                && binding.command == Command::RunWorkspaceTaskKind(WorkspaceTaskKind::Build)
        }));
    }

    #[test]
    fn default_keymap_includes_undo_redo_shortcuts() {
        let keymap = Keymap::default();

        assert!(
            keymap
                .bindings
                .iter()
                .any(|binding| binding.chord == "Ctrl+Z" && binding.command == Command::Undo)
        );
        assert!(
            keymap
                .bindings
                .iter()
                .any(|binding| binding.chord == "Ctrl+Y" && binding.command == Command::Redo)
        );
    }

    #[test]
    fn default_keymap_includes_git_change_navigation_shortcuts() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Alt+F5" && binding.command == Command::NextGitChange
        }));
        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Alt+Shift+F5" && binding.command == Command::PreviousGitChange
        }));
        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+Shift+G" && binding.command == Command::ToggleSourceControl
        }));
    }

    #[test]
    fn default_keymap_includes_project_search_result_navigation_shortcuts() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "F4" && binding.command == Command::NextProjectSearchResult
        }));
        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Shift+F4" && binding.command == Command::PreviousProjectSearchResult
        }));
    }

    #[test]
    fn default_keymap_includes_diff_hunk_navigation_shortcuts() {
        let keymap = Keymap::default();

        assert!(
            keymap.bindings.iter().any(|binding| {
                binding.chord == "F7" && binding.command == Command::NextDiffHunk
            })
        );
        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Shift+F7" && binding.command == Command::PreviousDiffHunk
        }));
    }

    #[test]
    fn default_keymap_includes_matching_bracket_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+Shift+\\" && binding.command == Command::GoToMatchingBracket
        }));
    }

    #[test]
    fn default_keymap_includes_toggle_line_comment_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+/" && binding.command == Command::ToggleLineComment
        }));
    }

    #[test]
    fn default_keymap_includes_select_lines_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+L" && binding.command == Command::SelectLines
        }));
    }

    #[test]
    fn default_keymap_includes_rectangular_selection_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+Alt+L" && binding.command == Command::SelectRectangularBlock
        }));
    }

    #[test]
    fn default_keymap_includes_expand_selection_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+Shift+Right" && binding.command == Command::ExpandSelection
        }));
    }

    #[test]
    fn default_keymap_includes_delete_lines_shortcut() {
        let keymap = Keymap::default();

        assert!(
            keymap
                .bindings
                .iter()
                .any(|binding| binding.chord == "Ctrl+Shift+K"
                    && binding.command == Command::DeleteLines)
        );
    }

    #[test]
    fn default_keymap_includes_join_lines_shortcut() {
        let keymap = Keymap::default();

        assert!(
            keymap
                .bindings
                .iter()
                .any(|binding| binding.chord == "Ctrl+J" && binding.command == Command::JoinLines)
        );
    }

    #[test]
    fn default_keymap_includes_indent_outdent_shortcuts() {
        let keymap = Keymap::default();

        assert!(keymap
            .bindings
            .iter()
            .any(|binding| binding.chord == "Ctrl+]" && binding.command == Command::IndentLines));
        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+[" && binding.command == Command::OutdentLines
        }));
    }

    #[test]
    fn default_keymap_includes_line_end_multicursor_shortcut() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Alt+Shift+I" && binding.command == Command::AddCursorsToLineEnds
        }));
    }

    #[test]
    fn default_keymap_includes_terminal_session_navigation_shortcuts() {
        let keymap = Keymap::default();

        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+PageDown" && binding.command == Command::NextTerminalSession
        }));
        assert!(keymap.bindings.iter().any(|binding| {
            binding.chord == "Ctrl+PageUp" && binding.command == Command::PreviousTerminalSession
        }));
    }
}

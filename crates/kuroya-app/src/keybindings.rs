use crate::{
    command_aliases::command_label_aliases,
    command_catalog::command_catalog_slice,
    commands::command_label,
    keybinding_chords::keybinding_requires_primary_modifier,
    keybinding_parse::{normalize_key_chord, parse_key_chord},
};
use eframe::egui::Modifiers;
use kuroya_core::{Command, keymap::KeyBinding, keymap::Keymap};

#[cfg(test)]
pub(crate) fn keybinding_matches_query(chord: &str, label: &str, query: &str) -> bool {
    keybinding_matches_trimmed_query(chord, label, query.trim())
}

#[cfg(test)]
pub(crate) fn keybinding_matches_trimmed_query(chord: &str, label: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let search_text = keybinding_search_text(chord, label);
    keybinding_search_text_matches_query(&search_text, query)
}

pub(crate) fn keybinding_search_text(chord: &str, label: &str) -> String {
    let aliases = command_label_aliases(label);
    let aliases_len = aliases.iter().map(|alias| alias.len() + 1).sum::<usize>();
    let mut text = String::with_capacity(chord.len() * 2 + label.len() + aliases_len + 16);

    push_keybinding_search_text_part(&mut text, chord);
    push_keybinding_shortcut_words(&mut text, chord);
    push_keybinding_search_text_part(&mut text, label);
    for alias in aliases {
        push_keybinding_search_text_part(&mut text, alias);
    }
    if chord.is_empty() {
        push_keybinding_search_text_part(&mut text, "unassigned");
    }

    text
}

#[cfg(test)]
pub(crate) fn keybinding_search_text_matches_query(search_text: &str, query: &str) -> bool {
    let mut terms = query.split_whitespace();
    let Some(first) = terms.next() else {
        return true;
    };

    kuroya_core::text_match::ascii_case_insensitive_contains(search_text, first)
        && terms
            .all(|term| kuroya_core::text_match::ascii_case_insensitive_contains(search_text, term))
}

fn push_keybinding_shortcut_words(text: &mut String, chord: &str) {
    if chord.is_empty() || !chord.contains('+') {
        return;
    }

    let mut words = String::with_capacity(chord.len());
    for character in chord.chars() {
        if character == '+' {
            words.push(' ');
        } else {
            words.push(character);
        }
    }
    push_keybinding_search_text_part(text, &words);
}

fn push_keybinding_search_text_part(text: &mut String, part: &str) {
    if part.is_empty() {
        return;
    }
    if !text.is_empty() {
        text.push(' ');
    }
    text.push_str(part);
}

pub(crate) fn keybinding_items(bindings: &[KeyBinding]) -> Vec<(String, Command, String)> {
    let catalog = command_catalog_slice();
    let bindings = normalized_supported_keybinding_bindings(bindings);
    let catalog_chords = catalog_keybinding_chords(catalog, &bindings);
    let mut items = Vec::with_capacity(catalog.len().saturating_add(bindings.len()));
    for (index, command) in catalog.iter().enumerate() {
        push_keybinding_item_with_chord(
            &mut items,
            command.clone(),
            catalog_chords
                .chord_for_catalog_index(index)
                .unwrap_or_default()
                .to_owned(),
        );
    }

    let mut extra_commands = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        if catalog_chords.is_catalog_binding(index) || extra_commands.contains(&&binding.command) {
            continue;
        }
        extra_commands.push(&binding.command);
        push_keybinding_item_with_chord(&mut items, binding.command.clone(), binding.chord.clone());
    }

    items
}

fn normalized_supported_keybinding_bindings(bindings: &[KeyBinding]) -> Vec<KeyBinding> {
    let mut normalized_bindings = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let Some(chord) = normalized_supported_keybinding_chord(&binding.chord) else {
            continue;
        };
        if normalized_bindings
            .iter()
            .any(|kept: &KeyBinding| kept.command == binding.command || kept.chord == chord)
        {
            continue;
        }
        normalized_bindings.push(KeyBinding {
            chord,
            command: binding.command.clone(),
        });
    }
    normalized_bindings
}

fn normalized_supported_keybinding_chord(chord: &str) -> Option<String> {
    let chord = normalize_key_chord(chord)?;
    keybinding_chord_is_supported(&chord).then_some(chord)
}

fn keybinding_chord_is_supported(chord: &str) -> bool {
    let Some(shortcut) = parse_key_chord(chord) else {
        return false;
    };
    !keybinding_requires_primary_modifier(shortcut.logical_key)
        || shortcut.modifiers.ctrl
        || shortcut.modifiers.alt
        || shortcut.modifiers.mac_cmd
        || shortcut.modifiers.command
}

pub(crate) struct CatalogKeybindingChords<'a> {
    chords: Vec<Option<&'a str>>,
    binding_catalog_indices: Vec<Option<usize>>,
}

pub(crate) fn catalog_keybinding_chords<'a>(
    catalog: &[Command],
    bindings: &'a [KeyBinding],
) -> CatalogKeybindingChords<'a> {
    let mut chords = vec![None; catalog.len()];
    let mut binding_catalog_indices = Vec::with_capacity(bindings.len());

    for binding in bindings {
        let catalog_index = catalog
            .iter()
            .position(|command| command == &binding.command);
        if let Some(index) = catalog_index
            && chords[index].is_none()
        {
            chords[index] = Some(binding.chord.as_str());
        }
        binding_catalog_indices.push(catalog_index);
    }

    CatalogKeybindingChords {
        chords,
        binding_catalog_indices,
    }
}

impl<'a> CatalogKeybindingChords<'a> {
    pub(crate) fn chord_for_catalog_index(&self, index: usize) -> Option<&'a str> {
        self.chords.get(index).copied().flatten()
    }

    fn is_catalog_binding(&self, binding_index: usize) -> bool {
        self.binding_catalog_indices
            .get(binding_index)
            .is_some_and(|index| index.is_some())
    }
}

fn push_keybinding_item_with_chord(
    items: &mut Vec<(String, Command, String)>,
    command: Command,
    chord: String,
) {
    let label = command_label(&command);
    items.push((chord, command, label));
}

#[cfg(test)]
pub(crate) fn assign_keybinding_chord(
    bindings: &mut Vec<KeyBinding>,
    command: Command,
    chord: impl AsRef<str>,
) -> Option<Command> {
    let chord = chord.as_ref();
    let (chord, chord_is_normalized) = match normalize_key_chord(chord) {
        Some(chord) => (chord, true),
        None => (chord.to_owned(), false),
    };
    assign_keybinding_chord_prepared(bindings, command, chord, chord_is_normalized)
}

pub(crate) fn assign_normalized_keybinding_chord(
    bindings: &mut Vec<KeyBinding>,
    command: Command,
    chord: String,
) -> Option<Command> {
    assign_keybinding_chord_prepared(bindings, command, chord, true)
}

fn assign_keybinding_chord_prepared(
    bindings: &mut Vec<KeyBinding>,
    command: Command,
    chord: String,
    chord_is_normalized: bool,
) -> Option<Command> {
    if let Some(conflict) = bindings
        .iter()
        .find(|binding| {
            binding.command != command
                && keybinding_chord_matches_prepared(&binding.chord, &chord, chord_is_normalized)
        })
        .map(|binding| binding.command.clone())
    {
        return Some(conflict);
    }

    let mut kept_command_binding = false;
    bindings.retain(|binding| {
        if binding.command == command {
            let keep = !kept_command_binding;
            kept_command_binding = true;
            keep
        } else {
            true
        }
    });

    if let Some(binding) = bindings
        .iter_mut()
        .find(|binding| binding.command == command)
    {
        binding.chord = chord;
    } else {
        bindings.push(KeyBinding { chord, command });
    }
    None
}

fn keybinding_chord_matches_prepared(
    chord: &str,
    prepared: &str,
    prepared_is_normalized: bool,
) -> bool {
    if !prepared_is_normalized {
        return chord.eq_ignore_ascii_case(prepared);
    }

    normalize_key_chord(chord).is_some_and(|chord| chord == prepared)
        || chord.eq_ignore_ascii_case(prepared)
}

/// How a candidate chord collides with an existing chord of another command
/// through modifier subsetting instead of exact equality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeybindingChordShadow {
    /// The candidate chord can never dispatch: `existing_chord` matches the same
    /// key with a subset of the candidate's modifiers and shadows it.
    ShadowedBy { existing_chord: String },
    /// The candidate chord shadows `existing_chord`, which can no longer dispatch.
    Shadows { existing_chord: String },
}

/// True when `chord` shadows `other`: both parse to the same key while `chord`
/// uses a strict subset of `other`'s modifiers, so `other` can never dispatch.
pub(crate) fn keybinding_chord_shadows(chord: &str, other: &str) -> bool {
    let (Some(chord), Some(other)) = (parse_key_chord(chord), parse_key_chord(other)) else {
        return false;
    };
    chord != other
        && chord.logical_key == other.logical_key
        && keybinding_modifiers_subset_of(chord.modifiers, other.modifiers)
}

fn keybinding_modifiers_subset_of(subset: Modifiers, superset: Modifiers) -> bool {
    (!subset.ctrl || superset.ctrl)
        && (!subset.shift || superset.shift)
        && (!subset.alt || superset.alt)
        && (!subset.mac_cmd || superset.mac_cmd)
        && (!subset.command || superset.command)
}

/// Finds a modifier-superset collision between `chord` and the bindings of
/// other commands, in either shadowing direction.
pub(crate) fn keybinding_chord_shadow_conflict(
    bindings: &[KeyBinding],
    command: &Command,
    chord: &str,
) -> Option<KeybindingChordShadow> {
    bindings
        .iter()
        .filter(|binding| &binding.command != command)
        .find_map(|binding| {
            if keybinding_chord_shadows(&binding.chord, chord) {
                Some(KeybindingChordShadow::ShadowedBy {
                    existing_chord: binding.chord.clone(),
                })
            } else if keybinding_chord_shadows(chord, &binding.chord) {
                Some(KeybindingChordShadow::Shadows {
                    existing_chord: binding.chord.clone(),
                })
            } else {
                None
            }
        })
}

/// Returns the factory default chord for `command` from a fresh `Keymap::default()`.
pub(crate) fn keybinding_default_chord_for_command(command: &Command) -> Option<String> {
    Keymap::default()
        .bindings
        .into_iter()
        .find(|binding| &binding.command == command)
        .map(|binding| binding.chord)
}

pub(crate) fn remove_keybinding_assignment(
    bindings: &mut Vec<KeyBinding>,
    command: &Command,
) -> bool {
    let original_len = bindings.len();
    bindings.retain(|binding| &binding.command != command);
    bindings.len() != original_len
}

#[cfg(test)]
mod tests {
    use super::{
        KeybindingChordShadow, keybinding_chord_shadow_conflict, keybinding_chord_shadows,
        keybinding_default_chord_for_command, keybinding_items, keybinding_matches_trimmed_query,
        keybinding_search_text,
    };
    use kuroya_core::{Command, keymap::KeyBinding};
    use std::path::PathBuf;

    #[test]
    fn keybinding_items_reuses_first_catalog_and_custom_binding() {
        let custom_command = Command::OpenFile(PathBuf::from("notes.md"));
        let bindings = vec![
            KeyBinding {
                chord: "Ctrl+P".to_owned(),
                command: Command::ToggleQuickOpen,
            },
            KeyBinding {
                chord: "Ctrl+Shift+P".to_owned(),
                command: Command::ToggleQuickOpen,
            },
            KeyBinding {
                chord: "Ctrl+O".to_owned(),
                command: custom_command.clone(),
            },
            KeyBinding {
                chord: "Ctrl+Alt+O".to_owned(),
                command: custom_command.clone(),
            },
        ];

        let items = keybinding_items(&bindings);
        let quick_open_chords = matching_chords(&items, &Command::ToggleQuickOpen);
        let custom_chords = matching_chords(&items, &custom_command);

        assert_eq!(quick_open_chords, vec!["Ctrl+P"]);
        assert_eq!(custom_chords, vec!["Ctrl+O"]);
    }

    #[test]
    fn keybinding_items_skip_stale_duplicate_and_unsupported_bindings() {
        let custom_command = Command::OpenFile(PathBuf::from("notes.md"));
        let invalid_custom_command = Command::OpenFile(PathBuf::from("bad.md"));
        let bindings = vec![
            KeyBinding {
                chord: " control + p ".to_owned(),
                command: Command::ToggleQuickOpen,
            },
            KeyBinding {
                chord: "Ctrl+P".to_owned(),
                command: Command::ToggleCommandPalette,
            },
            KeyBinding {
                chord: "T".to_owned(),
                command: Command::ToggleTerminal,
            },
            KeyBinding {
                chord: " option + arrowleft ".to_owned(),
                command: Command::NavigateBack,
            },
            KeyBinding {
                chord: "Ctrl+Alt+O".to_owned(),
                command: custom_command.clone(),
            },
            KeyBinding {
                chord: "Ctrl+Alt+P".to_owned(),
                command: custom_command.clone(),
            },
            KeyBinding {
                chord: "Ctrl+\u{202e}P".to_owned(),
                command: invalid_custom_command.clone(),
            },
        ];

        let items = keybinding_items(&bindings);

        assert_eq!(
            matching_chords(&items, &Command::ToggleQuickOpen),
            vec!["Ctrl+P"]
        );
        assert_eq!(
            matching_chords(&items, &Command::ToggleCommandPalette),
            vec![""]
        );
        assert_eq!(matching_chords(&items, &Command::ToggleTerminal), vec![""]);
        assert_eq!(
            matching_chords(&items, &Command::NavigateBack),
            vec!["Alt+Left"]
        );
        assert_eq!(matching_chords(&items, &custom_command), vec!["Ctrl+Alt+O"]);
        assert!(matching_chords(&items, &invalid_custom_command).is_empty());
    }

    #[test]
    fn keybinding_search_text_matches_shortcut_words_and_query_terms() {
        let search_text = keybinding_search_text("Ctrl+Alt+K", "Keyboard Shortcuts");

        assert!(keybinding_matches_trimmed_query(
            "Ctrl+Alt+K",
            "Keyboard Shortcuts",
            "alt+k"
        ));
        assert!(keybinding_matches_trimmed_query(
            "Ctrl+Alt+K",
            "Keyboard Shortcuts",
            "ctrl k"
        ));
        assert!(keybinding_matches_trimmed_query(
            "",
            "Keyboard Shortcuts",
            "key binds"
        ));
        assert!(search_text.contains("Ctrl Alt K"));
        assert!(!keybinding_matches_trimmed_query(
            "Ctrl+Alt+K",
            "Keyboard Shortcuts",
            "terminal find"
        ));
    }

    fn matching_chords<'a>(
        items: &'a [(String, Command, String)],
        command: &Command,
    ) -> Vec<&'a str> {
        items
            .iter()
            .filter(|(_, item_command, _)| item_command == command)
            .map(|(chord, _, _)| chord.as_str())
            .collect()
    }

    #[test]
    fn keybinding_chord_shadows_detects_modifier_superset_collisions() {
        assert!(keybinding_chord_shadows("Ctrl+S", "Ctrl+Shift+S"));
        assert!(keybinding_chord_shadows(" ctrl + s ", "Ctrl+Shift+S"));
        assert!(keybinding_chord_shadows(
            "Ctrl+Shift+S",
            " ctrl + shift + alt + s "
        ));
        assert!(!keybinding_chord_shadows("Ctrl+Shift+S", "Ctrl+S"));
        assert!(!keybinding_chord_shadows("Ctrl+S", "Ctrl+S"));
        assert!(!keybinding_chord_shadows("Ctrl+S", "Ctrl+P"));
        assert!(!keybinding_chord_shadows("Ctrl+S", "not a chord"));
        assert!(keybinding_chord_shadows("F3", "Shift+F3"));
        assert!(!keybinding_chord_shadows("Shift+F3", "F3"));
    }

    #[test]
    fn keybinding_chord_shadow_conflict_reports_shadowing_direction() {
        let bindings = vec![
            KeyBinding {
                chord: "Ctrl+S".to_owned(),
                command: Command::SaveActive,
            },
            KeyBinding {
                chord: "Ctrl+Alt+S".to_owned(),
                command: Command::SaveAll,
            },
        ];

        assert_eq!(
            keybinding_chord_shadow_conflict(&bindings, &Command::SaveAs, "Ctrl+Shift+S"),
            Some(KeybindingChordShadow::ShadowedBy {
                existing_chord: "Ctrl+S".to_owned()
            })
        );
        assert_eq!(
            keybinding_chord_shadow_conflict(&bindings, &Command::SaveActive, "Ctrl+S"),
            Some(KeybindingChordShadow::Shadows {
                existing_chord: "Ctrl+Alt+S".to_owned()
            })
        );
        assert_eq!(
            keybinding_chord_shadow_conflict(&bindings, &Command::Undo, "Ctrl+P"),
            None
        );
        // Exact chord equality is a hard conflict, never a shadow.
        let own_chord_only = vec![KeyBinding {
            chord: "Ctrl+S".to_owned(),
            command: Command::SaveActive,
        }];
        assert_eq!(
            keybinding_chord_shadow_conflict(&own_chord_only, &Command::Undo, "Ctrl+S"),
            None
        );
    }

    #[test]
    fn keybinding_default_chord_for_command_returns_factory_chord() {
        assert_eq!(
            keybinding_default_chord_for_command(&Command::Undo),
            Some("Ctrl+Z".to_owned())
        );
        assert_eq!(
            keybinding_default_chord_for_command(&Command::OpenFile(PathBuf::from("notes.md"))),
            None
        );
    }
}

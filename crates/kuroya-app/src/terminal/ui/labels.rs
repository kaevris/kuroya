#[cfg(test)]
use super::super::TERMINAL_DEFAULT_DISPLAY_LABEL;
use super::super::TerminalSession;
use super::colors::{terminal_ansi_palette, terminal_foreground_color, terminal_muted_text};
use crate::ui_icons::IconKind;
use egui::{Color32, RichText};
use std::{borrow::Cow, fmt::Write as _, path::Path};

pub(super) fn terminal_session_sequence_title(session: &TerminalSession) -> Option<Cow<'_, str>> {
    session
        .parser
        .callbacks()
        .window_title
        .as_deref()
        .and_then(terminal_display_label)
        .filter(|title| !terminal_sequence_title_is_executable_path(title.as_ref()))
}

fn terminal_sequence_title_is_executable_path(title: &str) -> bool {
    let trimmed = title.trim();
    if !trimmed.contains('\\') && !trimmed.contains('/') {
        return false;
    }

    trimmed
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|file_name| file_name.to_ascii_lowercase().ends_with(".exe"))
}

pub(super) fn terminal_template_path(path: &Path) -> Cow<'_, str> {
    terminal_display_path(path).unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn terminal_compact_path_for_test(path: &Path) -> String {
    compact_terminal_path(path).into_owned()
}

#[cfg(test)]
pub(crate) fn terminal_path_tooltip_for_test(path: &Path) -> String {
    terminal_path_tooltip(path).into_owned()
}

#[cfg(test)]
pub(crate) fn terminal_session_label(id: usize) -> String {
    terminal_session_label_for_shell(id, TERMINAL_DEFAULT_DISPLAY_LABEL).into_owned()
}

#[cfg(test)]
pub(crate) fn terminal_tab_icon_kind(icon: &str) -> IconKind {
    terminal_tab_icon_kind_from_setting(icon)
}

#[cfg(not(test))]
pub(super) fn terminal_tab_icon_kind(icon: &str) -> IconKind {
    terminal_tab_icon_kind_from_setting(icon)
}

fn terminal_tab_icon_kind_from_setting(icon: &str) -> IconKind {
    let normalized = icon
        .trim()
        .trim_start_matches('$')
        .trim_start_matches("codicon-");
    if terminal_setting_matches(normalized, &["code"]) {
        IconKind::Code
    } else if terminal_setting_matches(normalized, &["command", "terminal-cmd"]) {
        IconKind::Command
    } else if terminal_setting_matches(normalized, &["cursor"]) {
        IconKind::Cursor
    } else if terminal_setting_matches(
        normalized,
        &["debug-console", "diagnostics", "error", "warning"],
    ) {
        IconKind::Diagnostics
    } else if terminal_setting_matches(normalized, &["file"]) {
        IconKind::File
    } else if terminal_setting_matches(normalized, &["folder"]) {
        IconKind::Folder
    } else if terminal_setting_matches(normalized, &["git-branch", "source-control"]) {
        IconKind::GitBranch
    } else if terminal_setting_matches(normalized, &["lsp", "symbol-method"]) {
        IconKind::Lsp
    } else if terminal_setting_matches(normalized, &["search"]) {
        IconKind::Search
    } else if terminal_setting_matches(normalized, &["settings", "gear"]) {
        IconKind::Settings
    } else if terminal_setting_matches(normalized, &["theme", "color-mode"]) {
        IconKind::Theme
    } else if terminal_setting_matches(normalized, &["trash"]) {
        IconKind::Trash
    } else {
        IconKind::Terminal
    }
}

pub(super) fn terminal_tab_color_from_setting(
    color: Option<&str>,
    ui: &egui::Ui,
    fallback: Color32,
) -> Color32 {
    let Some(color) = color.map(str::trim).filter(|color| !color.is_empty()) else {
        return fallback;
    };
    if let Some(color) = parse_hex_color(color) {
        return color;
    }

    if terminal_setting_matches(
        color,
        &[
            "accent",
            "focusborder",
            "terminalcommanddecoration.defaultbackground",
        ],
    ) {
        fallback
    } else if terminal_setting_matches(color, &["foreground", "editor.foreground", "text"]) {
        ui.visuals().text_color()
    } else if terminal_setting_matches(color, &["descriptionforeground", "muted"]) {
        terminal_muted_text(ui)
    } else if let Some(index) = terminal_tab_ansi_color_index(color) {
        let ansi_palette = terminal_ansi_palette(ui);
        terminal_foreground_color(vt100::Color::Idx(index), fallback, &ansi_palette)
    } else {
        fallback
    }
}

pub(super) fn terminal_tab_ansi_color_index(color: &str) -> Option<u8> {
    if terminal_setting_matches(color, &["terminal.ansiblack"]) {
        Some(0)
    } else if terminal_setting_matches(color, &["terminal.ansired"]) {
        Some(1)
    } else if terminal_setting_matches(color, &["terminal.ansigreen"]) {
        Some(2)
    } else if terminal_setting_matches(color, &["terminal.ansiyellow"]) {
        Some(3)
    } else if terminal_setting_matches(color, &["terminal.ansiblue"]) {
        Some(4)
    } else if terminal_setting_matches(color, &["terminal.ansimagenta"]) {
        Some(5)
    } else if terminal_setting_matches(color, &["terminal.ansicyan"]) {
        Some(6)
    } else if terminal_setting_matches(color, &["terminal.ansiwhite"]) {
        Some(7)
    } else if terminal_setting_matches(color, &["terminal.ansibrightblack"]) {
        Some(8)
    } else if terminal_setting_matches(color, &["terminal.ansibrightred"]) {
        Some(9)
    } else if terminal_setting_matches(color, &["terminal.ansibrightgreen"]) {
        Some(10)
    } else if terminal_setting_matches(color, &["terminal.ansibrightyellow"]) {
        Some(11)
    } else if terminal_setting_matches(color, &["terminal.ansibrightblue"]) {
        Some(12)
    } else if terminal_setting_matches(color, &["terminal.ansibrightmagenta"]) {
        Some(13)
    } else if terminal_setting_matches(color, &["terminal.ansibrightcyan"]) {
        Some(14)
    } else if terminal_setting_matches(color, &["terminal.ansibrightwhite"]) {
        Some(15)
    } else {
        None
    }
}

fn terminal_setting_matches(value: &str, names: &[&str]) -> bool {
    names.iter().any(|name| value.eq_ignore_ascii_case(name))
}

#[cfg(test)]
pub(crate) fn parse_terminal_tab_hex_color(color: &str) -> Option<Color32> {
    parse_hex_color(color)
}

fn parse_hex_color(color: &str) -> Option<Color32> {
    let hex = color.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color32::from_rgb(red, green, blue))
}

#[cfg(test)]
pub(super) fn terminal_session_label_for_shell(id: usize, shell_label: &str) -> Cow<'_, str> {
    let label = terminal_display_label(shell_label).unwrap_or_default();
    terminal_session_label_from_display_label(id, label)
}

pub(super) fn terminal_session_label_from_display_label(
    id: usize,
    label: Cow<'_, str>,
) -> Cow<'_, str> {
    let mut numbered_label = String::with_capacity(
        label
            .len()
            .saturating_add(terminal_session_label_id_suffix_chars(id)),
    );
    numbered_label.push_str(label.as_ref());
    append_terminal_session_label_id(&mut numbered_label, id);
    Cow::Owned(numbered_label)
}

fn append_terminal_session_label_id(label: &mut String, id: usize) {
    let suffix_chars = terminal_session_label_id_suffix_chars(id);
    let base_char_limit = TERMINAL_DISPLAY_LABEL_MAX_CHARS.saturating_sub(suffix_chars);
    truncate_terminal_display_label(label, base_char_limit);
    if label.is_empty() {
        let _ = write!(label, "{id}");
    } else {
        let _ = write!(label, " {id}");
    }
}

fn terminal_session_label_id_suffix_chars(id: usize) -> usize {
    let mut value = id;
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits + 1
}

fn truncate_terminal_display_label(label: &mut String, max_chars: usize) {
    if max_chars == 0 {
        label.clear();
        return;
    }

    let mut truncate_at = label.len();
    for (char_count, (index, _)) in label.char_indices().enumerate() {
        if char_count == max_chars {
            truncate_at = index;
            break;
        }
    }
    label.truncate(truncate_at);
    while label.chars().next_back().is_some_and(char::is_whitespace) {
        label.pop();
    }
}

pub(super) const TERMINAL_DISPLAY_LABEL_MAX_CHARS: usize = 120;
const TERMINAL_DISPLAY_LABEL_MAX_UTF8_BYTES: usize = TERMINAL_DISPLAY_LABEL_MAX_CHARS * 4;
const TERMINAL_DISPLAY_LABEL_MAX_SCAN_CHARS: usize = 4096;
pub(super) const TERMINAL_DISPLAY_LABEL_MAX_EXACT_UTF8_BYTES: usize =
    TERMINAL_DISPLAY_LABEL_MAX_SCAN_CHARS * 4;

pub(super) fn terminal_display_label(label: &str) -> Option<Cow<'_, str>> {
    if label.is_empty() {
        return None;
    }
    if is_borrowed_terminal_display_label(label) {
        return Some(Cow::Borrowed(label));
    }

    let max_scan_chars = (label.len() > TERMINAL_DISPLAY_LABEL_MAX_EXACT_UTF8_BYTES)
        .then_some(TERMINAL_DISPLAY_LABEL_MAX_SCAN_CHARS);
    terminal_display_label_normalized(label, max_scan_chars).map(Cow::Owned)
}

pub(super) fn terminal_display_label_normalized(
    label: &str,
    max_scan_chars: Option<usize>,
) -> Option<String> {
    let mut normalized =
        String::with_capacity(label.len().min(TERMINAL_DISPLAY_LABEL_MAX_UTF8_BYTES));
    let mut char_count = 0;
    let mut inserted_separator = false;
    let mut past_leading_trim = false;

    for (scanned_chars, ch) in label.chars().enumerate() {
        if max_scan_chars.is_some_and(|max_scan_chars| scanned_chars == max_scan_chars) {
            break;
        }

        if !past_leading_trim {
            if ch.is_whitespace() {
                continue;
            }
            past_leading_trim = true;
        }

        if is_terminal_bidi_control(ch) {
            continue;
        }

        if is_terminal_label_separator(ch) {
            if !normalized.is_empty() {
                if !normalized.ends_with(' ') {
                    if char_count == TERMINAL_DISPLAY_LABEL_MAX_CHARS {
                        break;
                    }
                    normalized.push(' ');
                    char_count += 1;
                }
                inserted_separator = true;
            }
            continue;
        }

        if inserted_separator && ch == ' ' {
            continue;
        }
        inserted_separator = false;

        if char_count == TERMINAL_DISPLAY_LABEL_MAX_CHARS {
            break;
        }
        normalized.push(ch);
        char_count += 1;
    }

    while normalized
        .chars()
        .next_back()
        .is_some_and(char::is_whitespace)
    {
        normalized.pop();
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn is_borrowed_terminal_display_label(label: &str) -> bool {
    is_borrowed_simple_ascii_terminal_display_label(label)
        || is_borrowed_clean_unicode_terminal_display_label(label)
}

fn is_borrowed_simple_ascii_terminal_display_label(label: &str) -> bool {
    is_simple_ascii_terminal_display_label(label)
        && label.as_bytes().first().is_none_or(|byte| *byte != b' ')
        && label.as_bytes().last().is_none_or(|byte| *byte != b' ')
}

fn is_simple_ascii_terminal_display_label(label: &str) -> bool {
    label.len() <= TERMINAL_DISPLAY_LABEL_MAX_CHARS
        && label
            .as_bytes()
            .iter()
            .all(|byte| (b' '..=b'~').contains(byte))
}

fn is_borrowed_clean_unicode_terminal_display_label(label: &str) -> bool {
    let mut last = None;
    for (index, ch) in label.chars().enumerate() {
        if index == TERMINAL_DISPLAY_LABEL_MAX_CHARS {
            return false;
        }
        if is_terminal_label_separator(ch) || is_terminal_bidi_control(ch) {
            return false;
        }
        if index == 0 && ch.is_whitespace() {
            return false;
        }
        last = Some(ch);
    }

    last.is_some_and(|ch| !ch.is_whitespace())
}

fn is_terminal_label_separator(ch: char) -> bool {
    ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}')
}

fn is_terminal_bidi_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn terminal_display_path(path: &Path) -> Option<Cow<'_, str>> {
    if let Some(path) = path.as_os_str().to_str() {
        return terminal_display_label(path);
    }

    let path = path.to_string_lossy();
    terminal_display_label(path.as_ref()).map(|label| Cow::Owned(label.into_owned()))
}

pub(super) fn compact_terminal_path(path: &std::path::Path) -> Cow<'_, str> {
    if let Some(label) = path.file_name().and_then(|name| name.to_str()) {
        return terminal_display_label(label).unwrap_or(Cow::Borrowed("."));
    }
    terminal_display_path(path).unwrap_or(Cow::Borrowed("."))
}

pub(super) fn terminal_path_tooltip(path: &Path) -> Cow<'_, str> {
    terminal_display_path(path).unwrap_or(Cow::Borrowed("."))
}

pub(super) fn terminal_path_label(ui: &mut egui::Ui, path: &Path, muted_text: Color32) {
    ui.label(
        RichText::new(compact_terminal_path(path))
            .small()
            .color(muted_text),
    )
    .on_hover_ui(|ui| {
        let tooltip = terminal_path_tooltip(path);
        ui.label(tooltip.as_ref());
    });
}

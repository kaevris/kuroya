use super::{CommandPaletteCatalogEntry, CommandPaletteResult};
use crate::ui_state::selected_row_scroll_offset;
#[cfg(test)]
use crate::ui_text::truncate_middle;
use eframe::egui::{
    self, FontFamily, FontId, Rect, RichText, ScrollArea, Sense, Ui, WidgetInfo, WidgetType, pos2,
    vec2,
};
use kuroya_core::Command;
use std::{borrow::Cow, fmt::Write};

pub(super) const COMMAND_PALETTE_ROW_HEIGHT: f32 = 32.0;
const COMMAND_PALETTE_ROW_PADDING_X: f32 = 10.0;
const COMMAND_PALETTE_CHORD_SLOT_WIDTH: f32 = 150.0;
const COMMAND_PALETTE_LABEL_CHORD_GAP: f32 = 12.0;
const COMMAND_PALETTE_ROW_LABEL_MAX_CHARS: usize = 160;
const COMMAND_PALETTE_ROW_CHORD_MAX_CHARS: usize = 80;
const COMMAND_PALETTE_ROW_SECTION_MAX_CHARS: usize = 80;
const COMMAND_PALETTE_ROW_DISPLAY_SCAN_CHARS: usize = 4096;

pub(super) fn render_command_palette_result_list(
    ui: &mut Ui,
    commands: &[CommandPaletteResult],
    commands_catalog: &[CommandPaletteCatalogEntry],
    selected_index: usize,
    ui_font_size: f32,
    summary_label: &str,
    empty_label: &str,
    scroll_to_selection: bool,
) -> Option<Command> {
    let mut command_to_run = None;

    if commands.is_empty() {
        ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(20.0);
            ui.centered_and_justified(|ui| {
                ui.label(empty_label);
            });
        });
        return None;
    }

    ui.label(RichText::new(summary_label).small());
    let viewport_height = ui.available_height();
    let display_rows = command_palette_display_rows(commands, commands_catalog);
    let mut scroll_area = ScrollArea::vertical();
    if scroll_to_selection {
        let selected_display_index =
            command_palette_selected_display_row_index(&display_rows, selected_index);
        scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
            selected_display_index,
            display_rows.len(),
            COMMAND_PALETTE_ROW_HEIGHT,
            viewport_height,
        ));
    }
    let label_font = FontId::new(ui_font_size, FontFamily::Proportional);
    let section_font = FontId::new((ui_font_size * 0.78).max(10.0), FontFamily::Proportional);
    let chord_font = FontId::new(ui_font_size, FontFamily::Monospace);

    scroll_area.show_rows(
        ui,
        COMMAND_PALETTE_ROW_HEIGHT,
        display_rows.len(),
        |ui, rows| {
            for display_idx in rows {
                match display_rows.get(display_idx) {
                    Some(CommandPaletteDisplayRow::SectionHeader { section }) => {
                        render_command_palette_section_header(ui, section, &section_font);
                    }
                    Some(CommandPaletteDisplayRow::Command { result_index }) => {
                        let Some(result) = commands.get(*result_index) else {
                            continue;
                        };
                        let Some(row) = CommandPalettePreparedRow::new(
                            result,
                            commands_catalog,
                            *result_index,
                            commands.len(),
                        ) else {
                            continue;
                        };
                        let selected = *result_index == selected_index;
                        let response = render_command_palette_result_row(
                            ui,
                            &row.display,
                            selected,
                            &label_font,
                            &chord_font,
                        );
                        if response.clicked() {
                            command_to_run = Some(row.command.clone());
                        }
                    }
                    None => {}
                }
            }
        },
    );
    command_to_run
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteDisplayRow<'a> {
    SectionHeader { section: &'a str },
    Command { result_index: usize },
}

fn command_palette_display_rows<'a>(
    commands: &[CommandPaletteResult],
    commands_catalog: &'a [CommandPaletteCatalogEntry],
) -> Vec<CommandPaletteDisplayRow<'a>> {
    let mut rows = Vec::with_capacity(commands.len().saturating_mul(2));
    let mut previous_section = None;
    for (result_index, result) in commands.iter().enumerate() {
        let Some((_, _, _, section)) = result.catalog_entry(commands_catalog) else {
            continue;
        };
        if !section.is_empty() && previous_section != Some(section) {
            rows.push(CommandPaletteDisplayRow::SectionHeader { section });
        }
        rows.push(CommandPaletteDisplayRow::Command { result_index });
        previous_section = Some(section);
    }
    rows
}

fn command_palette_selected_display_row_index(
    display_rows: &[CommandPaletteDisplayRow<'_>],
    selected_index: usize,
) -> usize {
    display_rows
        .iter()
        .position(|row| {
            matches!(
                row,
                CommandPaletteDisplayRow::Command { result_index }
                    if *result_index == selected_index
            )
        })
        .unwrap_or_else(|| display_rows.len().saturating_sub(1))
}

fn render_command_palette_section_header(
    ui: &mut Ui,
    section: &str,
    section_font: &FontId,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), COMMAND_PALETTE_ROW_HEIGHT),
        Sense::hover(),
    );
    let painter = ui.painter();
    let visuals = ui.visuals();
    let section = command_palette_row_display_text(
        section,
        COMMAND_PALETTE_ROW_SECTION_MAX_CHARS,
        "Commands",
    );
    let enabled = ui.is_enabled();

    response.widget_info(|| command_palette_section_widget_info(section.as_ref(), enabled));
    painter.text(
        pos2(
            rect.left() + COMMAND_PALETTE_ROW_PADDING_X,
            rect.top() + 11.0,
        ),
        egui::Align2::LEFT_TOP,
        section.as_ref(),
        section_font.clone(),
        visuals.weak_text_color(),
    );

    response
}

fn command_palette_section_widget_info(section: &str, enabled: bool) -> WidgetInfo {
    let mut info = WidgetInfo::new(WidgetType::Label);
    info.enabled = enabled;
    info.label = Some(format!("Section {section}"));
    info
}

fn render_command_palette_result_row(
    ui: &mut Ui,
    display: &CommandPaletteRowDisplay<'_>,
    selected: bool,
    label_font: &FontId,
    chord_font: &FontId,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), COMMAND_PALETTE_ROW_HEIGHT),
        Sense::click(),
    );
    let painter = ui.painter();
    let visuals = ui.visuals();
    if selected {
        painter.rect_filled(rect, 4.0, visuals.widgets.active.weak_bg_fill);
    } else if response.hovered() {
        painter.rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
    }

    let enabled = ui.is_enabled();
    response.widget_info(|| command_palette_row_widget_info(display, enabled, selected));

    let label_clip = command_palette_label_clip_rect(rect, display.has_shortcut());
    painter.with_clip_rect(label_clip).text(
        pos2(
            rect.left() + COMMAND_PALETTE_ROW_PADDING_X,
            rect.top() + 7.0,
        ),
        egui::Align2::LEFT_TOP,
        display.label(),
        label_font.clone(),
        visuals.text_color(),
    );
    if let Some(shortcut) = display.shortcut() {
        let chord_clip = command_palette_chord_clip_rect(rect);
        painter.with_clip_rect(chord_clip).text(
            pos2(
                rect.right() - COMMAND_PALETTE_ROW_PADDING_X,
                rect.top() + 7.0,
            ),
            egui::Align2::RIGHT_TOP,
            shortcut,
            chord_font.clone(),
            visuals.weak_text_color(),
        );
    }

    response.on_hover_ui(|ui| display.tooltip_ui(ui))
}

struct CommandPalettePreparedRow<'a> {
    command: &'a Command,
    display: CommandPaletteRowDisplay<'a>,
}

impl<'a> CommandPalettePreparedRow<'a> {
    fn new(
        result: &CommandPaletteResult,
        commands_catalog: &'a [CommandPaletteCatalogEntry],
        row_index: usize,
        row_count: usize,
    ) -> Option<Self> {
        let (label, command, chord, _section) = result.catalog_entry(commands_catalog)?;
        Some(Self {
            command,
            display: CommandPaletteRowDisplay::new(label, chord, row_index, row_count),
        })
    }
}

struct CommandPaletteRowDisplay<'a> {
    label: Cow<'a, str>,
    shortcut: Option<Cow<'a, str>>,
    accessibility_label: String,
}

impl<'a> CommandPaletteRowDisplay<'a> {
    fn new(label: &'a str, chord: &'a str, row_index: usize, row_count: usize) -> Self {
        let label = command_palette_row_display_text(
            label,
            COMMAND_PALETTE_ROW_LABEL_MAX_CHARS,
            "<unnamed command>",
        );
        let shortcut =
            command_palette_row_display_text(chord, COMMAND_PALETTE_ROW_CHORD_MAX_CHARS, "");
        let shortcut = (!shortcut.is_empty()).then_some(shortcut);
        let accessibility_label = command_palette_row_accessibility_label(
            label.as_ref(),
            shortcut.as_deref(),
            row_index,
            row_count,
        );

        Self {
            label,
            shortcut,
            accessibility_label,
        }
    }

    fn label(&self) -> &str {
        self.label.as_ref()
    }

    fn shortcut(&self) -> Option<&str> {
        self.shortcut.as_deref()
    }

    fn has_shortcut(&self) -> bool {
        self.shortcut.is_some()
    }

    fn tooltip_ui(&self, ui: &mut Ui) {
        ui.label(command_palette_row_run_label(self.label()));
        if let Some(shortcut) = self.shortcut() {
            ui.horizontal(|ui| {
                ui.label("Shortcut:");
                ui.label(RichText::new(shortcut).monospace());
            });
        }
    }

    fn accessibility_label(&self) -> &str {
        self.accessibility_label.as_str()
    }

    #[cfg(test)]
    fn tooltip(&self) -> String {
        command_palette_row_tooltip_text(self.label(), self.shortcut())
    }
}

fn command_palette_row_widget_info(
    display: &CommandPaletteRowDisplay<'_>,
    enabled: bool,
    selected: bool,
) -> WidgetInfo {
    let mut info = WidgetInfo::new(WidgetType::SelectableLabel);
    info.enabled = enabled;
    info.selected = Some(selected);
    info.label = Some(display.accessibility_label().to_owned());
    info
}

fn command_palette_row_run_label(label: &str) -> String {
    let mut run_label = String::with_capacity("Run ".len() + label.len());
    command_palette_write_row_run_label(&mut run_label, label);
    run_label
}

fn command_palette_write_row_run_label(text: &mut String, label: &str) {
    text.push_str("Run ");
    text.push_str(label);
}

#[cfg(test)]
fn command_palette_row_tooltip_text(label: &str, shortcut: Option<&str>) -> String {
    let mut text = String::with_capacity(
        "Run ".len()
            + label.len()
            + shortcut.map_or(0, |shortcut| "\nShortcut: ".len() + shortcut.len()),
    );
    command_palette_write_row_tooltip_text(&mut text, label, shortcut);
    text
}

#[cfg(test)]
fn command_palette_write_row_tooltip_text(text: &mut String, label: &str, shortcut: Option<&str>) {
    command_palette_write_row_run_label(text, label);
    if let Some(shortcut) = shortcut {
        text.push_str("\nShortcut: ");
        text.push_str(shortcut);
    }
}

fn command_palette_row_display_text<'a>(
    value: &'a str,
    max_chars: usize,
    fallback: &'static str,
) -> Cow<'a, str> {
    if max_chars == 0 {
        return Cow::Borrowed("");
    }

    if value.is_empty() {
        return Cow::Borrowed(fallback);
    }

    if command_palette_row_display_text_is_simple(value, max_chars) {
        return Cow::Borrowed(value);
    }

    let mut display = String::with_capacity(value.len().min(max_chars));
    let mut display_chars = 0usize;
    let mut pending_space = false;
    let mut truncated = false;

    for (scanned_chars, ch) in value.chars().enumerate() {
        if scanned_chars >= COMMAND_PALETTE_ROW_DISPLAY_SCAN_CHARS {
            truncated = true;
            break;
        }

        if is_command_palette_row_format_control(ch) {
            continue;
        }

        if ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}') {
            pending_space = display_chars > 0;
            continue;
        }

        if ch.is_whitespace() {
            pending_space = display_chars > 0;
            continue;
        }

        if pending_space {
            if !push_command_palette_row_display_char(
                &mut display,
                &mut display_chars,
                ' ',
                max_chars,
            ) {
                truncated = true;
                break;
            }
            pending_space = false;
        }

        if !push_command_palette_row_display_char(&mut display, &mut display_chars, ch, max_chars) {
            truncated = true;
            break;
        }
    }

    if display.is_empty() {
        return Cow::Borrowed(fallback);
    }

    if truncated {
        mark_command_palette_row_display_text_truncated(&mut display, max_chars);
    }
    Cow::Owned(display)
}

fn command_palette_row_display_text_is_simple(value: &str, max_chars: usize) -> bool {
    if value.len() <= max_chars && value.is_ascii() {
        let bytes = value.as_bytes();
        return !matches!(bytes.first(), Some(b' '))
            && !matches!(bytes.last(), Some(b' '))
            && bytes.iter().all(|byte| (b' '..=b'~').contains(byte));
    }

    let mut previous_was_space = false;
    let mut chars = value.chars().peekable();
    let mut char_count = 0usize;

    while let Some(ch) = chars.next() {
        if char_count >= max_chars || char_count >= COMMAND_PALETTE_ROW_DISPLAY_SCAN_CHARS {
            return false;
        }

        if is_command_palette_row_format_control(ch)
            || ch.is_control()
            || matches!(ch, '\u{2028}' | '\u{2029}')
        {
            return false;
        }

        if ch.is_whitespace() {
            if ch != ' ' || char_count == 0 || previous_was_space || chars.peek().is_none() {
                return false;
            }
            previous_was_space = true;
        } else {
            previous_was_space = false;
        }

        char_count += 1;
    }

    true
}

fn push_command_palette_row_display_char(
    display: &mut String,
    display_chars: &mut usize,
    ch: char,
    max_chars: usize,
) -> bool {
    if *display_chars >= max_chars {
        return false;
    }

    display.push(ch);
    *display_chars += 1;
    true
}

fn mark_command_palette_row_display_text_truncated(display: &mut String, max_chars: usize) {
    if display.is_empty() {
        return;
    }

    if max_chars <= 3 {
        display.clear();
        for _ in 0..max_chars {
            display.push('.');
        }
        return;
    }

    truncate_command_palette_row_display_text(display, max_chars - 3);
    display.push_str("...");
}

fn truncate_command_palette_row_display_text(display: &mut String, max_chars: usize) {
    if max_chars == 0 {
        display.clear();
        return;
    }

    if let Some((byte_index, _)) = display.char_indices().nth(max_chars) {
        display.truncate(byte_index);
    }
}

fn is_command_palette_row_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{00ad}'
            | '\u{061c}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
    )
}

fn command_palette_label_clip_rect(row_rect: Rect, has_chord: bool) -> Rect {
    let content_left = row_rect.left() + COMMAND_PALETTE_ROW_PADDING_X;
    let content_right = (row_rect.right() - COMMAND_PALETTE_ROW_PADDING_X).max(content_left);
    let right = if has_chord {
        content_right - command_palette_chord_slot_width(row_rect) - COMMAND_PALETTE_LABEL_CHORD_GAP
    } else {
        content_right
    };
    let right = right.max(content_left);
    Rect::from_min_max(
        pos2(row_rect.left(), row_rect.top()),
        pos2(right, row_rect.bottom()),
    )
}

fn command_palette_chord_clip_rect(row_rect: Rect) -> Rect {
    let content_left = row_rect.left() + COMMAND_PALETTE_ROW_PADDING_X;
    let content_right = (row_rect.right() - COMMAND_PALETTE_ROW_PADDING_X).max(content_left);
    let slot_width = command_palette_chord_slot_width(row_rect);
    Rect::from_min_max(
        pos2(
            (content_right - slot_width).max(content_left),
            row_rect.top(),
        ),
        pos2(content_right, row_rect.bottom()),
    )
}

fn command_palette_chord_slot_width(row_rect: Rect) -> f32 {
    let content_width = (row_rect.width() - COMMAND_PALETTE_ROW_PADDING_X * 2.0).max(0.0);
    let responsive_width = (content_width - COMMAND_PALETTE_LABEL_CHORD_GAP).max(0.0) * 0.45;
    COMMAND_PALETTE_CHORD_SLOT_WIDTH.min(responsive_width)
}

#[cfg(test)]
fn command_palette_row_tooltip(label: &str, chord: &str) -> String {
    CommandPaletteRowDisplay::new(label, chord, 0, 1).tooltip()
}

fn command_palette_row_accessibility_label(
    label: &str,
    shortcut: Option<&str>,
    row_index: usize,
    row_count: usize,
) -> String {
    let position = row_index.saturating_add(1);
    let row_count = row_count.max(position);
    let mut status = String::with_capacity(label.len() + shortcut.map_or(0, str::len) + 48);
    let _ = write!(
        status,
        "Command {label}, position {} of {}",
        position, row_count
    );
    if let Some(shortcut) = shortcut {
        status.push_str(", shortcut ");
        status.push_str(shortcut);
    }
    status
}

#[cfg(test)]
fn command_palette_empty_state_label(query: &str) -> String {
    let query = query.trim();
    if query.is_empty() {
        "No commands available".to_owned()
    } else {
        format!(
            "No commands match \"{}\"",
            truncate_middle(query, COMMAND_PALETTE_EMPTY_QUERY_LIMIT)
        )
    }
}

#[cfg(test)]
fn command_palette_result_summary(command_count: usize, query: &str) -> String {
    let noun = if command_count == 1 {
        "command"
    } else {
        "commands"
    };
    let mut summary = String::with_capacity(32);
    if query.trim().is_empty() {
        summary.push_str("Showing ");
        let _ = write!(summary, "{command_count} {noun}");
    } else {
        let _ = write!(summary, "{command_count} {noun} matched");
    }
    summary
}

#[cfg(test)]
const COMMAND_PALETTE_EMPTY_QUERY_LIMIT: usize = 48;

#[cfg(test)]
mod tests {
    use super::super::CommandPaletteResult;
    use super::{
        COMMAND_PALETTE_ROW_CHORD_MAX_CHARS, COMMAND_PALETTE_ROW_DISPLAY_SCAN_CHARS,
        COMMAND_PALETTE_ROW_LABEL_MAX_CHARS, CommandPaletteCatalogEntry, CommandPaletteDisplayRow,
        CommandPalettePreparedRow, CommandPaletteRowDisplay, command_palette_display_rows,
        command_palette_empty_state_label, command_palette_result_summary,
        command_palette_row_accessibility_label, command_palette_row_display_text,
        command_palette_row_tooltip, command_palette_row_widget_info,
        command_palette_section_widget_info, command_palette_selected_display_row_index,
        command_palette_write_row_run_label, command_palette_write_row_tooltip_text,
    };
    use eframe::egui::WidgetType;
    use kuroya_core::Command;
    use std::borrow::Cow;

    fn command_palette_catalog_row(
        label: &str,
        command: Command,
        chord: &str,
        section: &str,
    ) -> CommandPaletteCatalogEntry {
        (
            label.to_owned(),
            command,
            chord.to_owned(),
            section.to_owned(),
        )
    }

    #[test]
    fn command_palette_empty_state_names_failed_query() {
        assert_eq!(
            command_palette_empty_state_label(" workspace task "),
            "No commands match \"workspace task\""
        );
        assert_eq!(
            command_palette_empty_state_label(
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
            ),
            "No commands match \"abcdefghijklmnopqrstuv...DEFGHIJKLMNOPQRSTUVWXYZ\""
        );
    }

    #[test]
    fn command_palette_empty_state_handles_empty_catalog() {
        assert_eq!(
            command_palette_empty_state_label(""),
            "No commands available"
        );
    }

    #[test]
    fn command_palette_result_summary_reports_query_context() {
        assert_eq!(
            command_palette_result_summary(1, "git"),
            "1 command matched"
        );
        assert_eq!(
            command_palette_result_summary(12, "git"),
            "12 commands matched"
        );
        assert_eq!(
            command_palette_result_summary(12, " "),
            "Showing 12 commands"
        );
    }

    #[test]
    fn command_palette_row_tooltip_surfaces_shortcut_when_present() {
        assert_eq!(
            command_palette_row_tooltip("Quick Open", "Ctrl+P"),
            "Run Quick Open\nShortcut: Ctrl+P"
        );
        assert_eq!(
            command_palette_row_tooltip("Quick Open", ""),
            "Run Quick Open"
        );
    }

    #[test]
    fn command_palette_row_tooltip_writers_append_exact_text() {
        let mut text = String::from("prefix: ");
        command_palette_write_row_run_label(&mut text, "Quick Open");
        assert_eq!(text, "prefix: Run Quick Open");

        text.clear();
        command_palette_write_row_tooltip_text(&mut text, "Quick Open", Some("Ctrl+P"));
        assert_eq!(text, "Run Quick Open\nShortcut: Ctrl+P");

        text.clear();
        command_palette_write_row_tooltip_text(&mut text, "Quick Open", None);
        assert_eq!(text, "Run Quick Open");
    }

    #[test]
    fn command_palette_row_accessibility_label_names_command_and_shortcut() {
        assert_eq!(
            command_palette_row_accessibility_label("Quick Open", Some("Ctrl+P"), 1, 4),
            "Command Quick Open, position 2 of 4, shortcut Ctrl+P"
        );
        assert_eq!(
            command_palette_row_accessibility_label("Quick Open", None, 0, 4),
            "Command Quick Open, position 1 of 4"
        );
    }

    #[test]
    fn command_palette_row_accessibility_label_repairs_empty_count() {
        assert_eq!(
            command_palette_row_accessibility_label("Quick Open", None, 2, 0),
            "Command Quick Open, position 3 of 3"
        );
    }

    #[test]
    fn command_palette_row_display_sanitizes_visible_fields_once() {
        let display =
            CommandPaletteRowDisplay::new("  Run\n\t\u{202e}\u{0}Now  ", " Ctrl\nP\u{2066} ", 1, 4);

        assert_eq!(display.label(), "Run Now");
        assert_eq!(display.shortcut(), Some("Ctrl P"));
        assert_eq!(display.tooltip(), "Run Run Now\nShortcut: Ctrl P");
        assert_eq!(
            display.accessibility_label(),
            "Command Run Now, position 2 of 4, shortcut Ctrl P"
        );
    }

    #[test]
    fn command_palette_row_display_text_borrows_clean_unicode_labels() {
        let label = "Ouvrir R\u{00e9}sum\u{00e9} \u{4ed5}\u{4e8b} \u{1f680}";

        match command_palette_row_display_text(label, 80, "<unnamed command>") {
            Cow::Borrowed(display) => assert_eq!(display, label),
            Cow::Owned(display) => panic!("clean Unicode label allocated: {display}"),
        }
    }

    #[test]
    fn command_palette_row_display_text_preserves_ascii_simple_spacing() {
        let label = "Run  Workspace  Task";

        match command_palette_row_display_text(label, 80, "<unnamed command>") {
            Cow::Borrowed(display) => assert_eq!(display, label),
            Cow::Owned(display) => panic!("simple ASCII label allocated: {display}"),
        }
    }

    #[test]
    fn command_palette_row_display_text_owns_dirty_unicode_and_whitespace() {
        for (label, expected) in [
            (
                "\u{2003}R\u{00e9}sum\u{00e9}\u{00a0}\t\u{8a2d}\u{5b9a}\u{200b}",
                "R\u{00e9}sum\u{00e9} \u{8a2d}\u{5b9a}",
            ),
            (
                "R\u{00e9}sum\u{00e9}  \u{8a2d}\u{5b9a}",
                "R\u{00e9}sum\u{00e9} \u{8a2d}\u{5b9a}",
            ),
            (
                "R\u{00e9}sum\u{00e9} \u{2028}\u{8a2d}\u{5b9a}",
                "R\u{00e9}sum\u{00e9} \u{8a2d}\u{5b9a}",
            ),
            ("R\u{00e9}sum\u{00e9}\u{200f}", "R\u{00e9}sum\u{00e9}"),
        ] {
            match command_palette_row_display_text(label, 80, "<unnamed command>") {
                Cow::Owned(display) => assert_eq!(display, expected),
                Cow::Borrowed(display) => {
                    panic!("dirty Unicode label borrowed unchanged: {display}")
                }
            }
        }
    }

    #[test]
    fn command_palette_row_display_text_owns_overlong_unicode_labels() {
        let label = "\u{540d}".repeat(12);

        match command_palette_row_display_text(&label, 6, "<unnamed command>") {
            Cow::Owned(display) => assert_eq!(display, "\u{540d}\u{540d}\u{540d}..."),
            Cow::Borrowed(display) => panic!("overlong Unicode label borrowed: {display}"),
        }
    }

    #[test]
    fn command_palette_row_display_text_owns_scan_truncated_unicode_labels() {
        let label = "\u{754c}".repeat(COMMAND_PALETTE_ROW_DISPLAY_SCAN_CHARS + 1);

        match command_palette_row_display_text(
            &label,
            COMMAND_PALETTE_ROW_DISPLAY_SCAN_CHARS + 10,
            "<unnamed command>",
        ) {
            Cow::Owned(display) => {
                assert!(display.ends_with("..."));
                assert!(display.chars().count() <= COMMAND_PALETTE_ROW_DISPLAY_SCAN_CHARS + 3);
            }
            Cow::Borrowed(display) => panic!("scan-truncated Unicode label borrowed: {display}"),
        }
    }

    #[test]
    fn command_palette_row_display_bounds_huge_unsafe_fields() {
        let label = format!(
            "Open\n{}\u{202e}\u{0000}Tail",
            "very-long-command-".repeat(512)
        );
        let chord = format!("Ctrl+{}\u{2066}", "Shift+".repeat(128));
        let display = CommandPaletteRowDisplay::new(&label, &chord, 0, 1);
        let shortcut = display.shortcut().expect("shortcut should be present");

        assert!(display.label().chars().count() <= COMMAND_PALETTE_ROW_LABEL_MAX_CHARS);
        assert!(shortcut.chars().count() <= COMMAND_PALETTE_ROW_CHORD_MAX_CHARS);
        assert!(display.label().contains("..."));
        assert!(shortcut.contains("..."));
        let accessibility_label = display.accessibility_label();
        for text in [display.label(), shortcut, accessibility_label] {
            assert!(!text.chars().any(|ch| {
                ch.is_control()
                    || matches!(
                        ch,
                        '\u{2028}' | '\u{2029}' | '\u{202e}' | '\u{2066}' | '\u{0000}'
                    )
            }));
        }
        assert!(!display.tooltip().contains('\u{202e}'));
        assert!(!display.tooltip().contains('\u{2066}'));
        assert!(!display.tooltip().contains('\u{0000}'));
        let accessibility_frame_chars =
            "Command ".chars().count() + ", position 1 of 1, shortcut ".chars().count();
        assert!(
            accessibility_label.chars().count()
                <= accessibility_frame_chars
                    + COMMAND_PALETTE_ROW_LABEL_MAX_CHARS
                    + COMMAND_PALETTE_ROW_CHORD_MAX_CHARS
        );
    }

    #[test]
    fn command_palette_row_display_names_blank_label_and_drops_blank_shortcut() {
        let display = CommandPaletteRowDisplay::new("\n\t\u{202e}", "\r\u{2066}", 0, 1);

        assert_eq!(display.label(), "<unnamed command>");
        assert_eq!(display.shortcut(), None);
        assert_eq!(display.tooltip(), "Run <unnamed command>");
        assert_eq!(
            display.accessibility_label(),
            "Command <unnamed command>, position 1 of 1"
        );
    }

    #[test]
    fn command_palette_row_widget_info_preserves_accessibility_metadata() {
        let display = CommandPaletteRowDisplay::new("Quick Open", "Ctrl+P", 1, 4);
        let info = command_palette_row_widget_info(&display, false, true);

        assert_eq!(info.typ, WidgetType::SelectableLabel);
        assert!(!info.enabled);
        assert_eq!(info.selected, Some(true));
        assert_eq!(
            info.label.as_deref(),
            Some("Command Quick Open, position 2 of 4, shortcut Ctrl+P")
        );
    }

    #[test]
    fn command_palette_section_widget_info_names_section() {
        let info = command_palette_section_widget_info("Workspace", false);

        assert_eq!(info.typ, WidgetType::Label);
        assert!(!info.enabled);
        assert_eq!(info.label.as_deref(), Some("Section Workspace"));
    }

    #[test]
    fn command_palette_display_rows_insert_headers_on_section_switches() {
        let commands_catalog = vec![
            command_palette_catalog_row("Quick Open", Command::ToggleQuickOpen, "", "Commands"),
            command_palette_catalog_row("Toggle Terminal", Command::ToggleTerminal, "", "Commands"),
            command_palette_catalog_row(
                "Open Recent Workspace",
                Command::OpenWorkspace("workspace".into()),
                "",
                "Recent Workspaces",
            ),
            command_palette_catalog_row(
                "Toggle Palette",
                Command::ToggleCommandPalette,
                "",
                "Commands",
            ),
        ];
        let commands = vec![
            CommandPaletteResult {
                catalog_index: 0,
                score: 0,
                match_score: 0,
            },
            CommandPaletteResult {
                catalog_index: 1,
                score: 0,
                match_score: 0,
            },
            CommandPaletteResult {
                catalog_index: 2,
                score: 0,
                match_score: 0,
            },
            CommandPaletteResult {
                catalog_index: 3,
                score: 0,
                match_score: 0,
            },
        ];

        let display_rows = command_palette_display_rows(&commands, &commands_catalog);

        assert_eq!(
            display_rows,
            vec![
                CommandPaletteDisplayRow::SectionHeader {
                    section: "Commands"
                },
                CommandPaletteDisplayRow::Command { result_index: 0 },
                CommandPaletteDisplayRow::Command { result_index: 1 },
                CommandPaletteDisplayRow::SectionHeader {
                    section: "Recent Workspaces"
                },
                CommandPaletteDisplayRow::Command { result_index: 2 },
                CommandPaletteDisplayRow::SectionHeader {
                    section: "Commands"
                },
                CommandPaletteDisplayRow::Command { result_index: 3 },
            ]
        );
        assert_eq!(
            command_palette_selected_display_row_index(&display_rows, 2),
            4
        );
    }

    #[test]
    fn command_palette_prepared_row_preserves_catalog_command_for_dispatch() {
        let commands_catalog = vec![command_palette_catalog_row(
            "  Quick\nOpen  ",
            Command::ToggleCommandPalette,
            " Ctrl\nP ",
            "Commands",
        )];
        let result = CommandPaletteResult {
            catalog_index: 0,
            score: 42,
            match_score: 7,
        };

        let row = CommandPalettePreparedRow::new(&result, &commands_catalog, 2, 5)
            .expect("catalog entry should resolve");

        assert_eq!(row.display.label(), "Quick Open");
        assert_eq!(row.display.shortcut(), Some("Ctrl P"));
        assert_eq!(
            row.display.accessibility_label(),
            "Command Quick Open, position 3 of 5, shortcut Ctrl P"
        );
        assert_eq!(row.command, &Command::ToggleCommandPalette);
        assert_eq!(commands_catalog[0].0, "  Quick\nOpen  ");
        assert_eq!(commands_catalog[0].2, " Ctrl\nP ");
    }

    #[test]
    fn command_palette_prepared_row_skips_stale_catalog_index() {
        let commands_catalog = vec![command_palette_catalog_row(
            "Quick Open",
            Command::ToggleCommandPalette,
            "Ctrl+P",
            "Commands",
        )];
        let result = CommandPaletteResult {
            catalog_index: 3,
            score: 42,
            match_score: 7,
        };

        assert!(CommandPalettePreparedRow::new(&result, &commands_catalog, 0, 1).is_none());
    }
}

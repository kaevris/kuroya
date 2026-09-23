use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_compact_button},
    quick_open::parse_line_column,
};
use eframe::egui::{self, Context, Key, RichText, TextEdit};
use kuroya_core::TextBuffer;
use std::fmt::Write as _;

const GOTO_LINE_INPUT_MAX_CHARS: usize = 64;

impl KuroyaApp {
    pub(crate) fn begin_goto_line(&mut self) {
        let Some(buffer) = self.active_buffer() else {
            self.goto_line_open = false;
            self.goto_line_input.clear();
            self.status = "No active file".to_owned();
            return;
        };

        let cursor = buffer.cursor_position();
        self.goto_line_input.clear();
        let _ = write!(self.goto_line_input, "{}", cursor.line + 1);
        self.goto_line_open = true;
    }

    fn submit_goto_line(&mut self) {
        normalize_goto_line_input_in_place(&mut self.goto_line_input);
        let Some((line, column)) = parse_line_column(&self.goto_line_input) else {
            self.status = "Use line or line:column".to_owned();
            return;
        };
        let Some(id) = self.active else {
            self.status = "No active file".to_owned();
            self.goto_line_open = false;
            return;
        };
        let Some(target) = self
            .buffer(id)
            .map(|buffer| clamped_goto_line_target(buffer, line, column))
        else {
            self.status = "No active file".to_owned();
            self.goto_line_open = false;
            return;
        };

        self.apply_file_jump_with_history(id, target.line, target.column);
        self.goto_line_open = false;
        self.status = target.status_text();
    }

    pub(crate) fn render_goto_line(&mut self, ctx: &Context) {
        if self.active_buffer().is_none() {
            self.goto_line_open = false;
            return;
        }

        let mut close = false;
        let mut submit = false;
        let mut window_open = self.goto_line_open;

        egui::Window::new("Go to Line")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 150.0,
            ))
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 126.0])
            .fixed_size([360.0, 90.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.goto_line_input)
                            .hint_text("line[:column]")
                            .desired_width(210.0),
                    );
                    response.request_focus();

                    if ui.input(|input| input.key_pressed(Key::Enter)) {
                        submit = true;
                    }
                    if ui.input(|input| input.key_pressed(Key::Escape)) {
                        close = true;
                    }

                    if popup_compact_button(ui, "Go", PopupButtonKind::Primary).clicked() {
                        submit = true;
                    }
                    if popup_compact_button(ui, "Close", PopupButtonKind::Secondary).clicked() {
                        close = true;
                    }
                    normalize_goto_line_input_in_place(&mut self.goto_line_input);
                });
                ui.label(
                    RichText::new("One-based line and column")
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            });

        if close || !window_open {
            self.goto_line_open = false;
        } else if submit {
            self.submit_goto_line();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GotoLineTarget {
    line: usize,
    column: usize,
}

impl GotoLineTarget {
    fn status_text(self) -> String {
        format!("Jumped to line {}, column {}", self.line, self.column)
    }
}

fn clamped_goto_line_target(buffer: &TextBuffer, line: usize, column: usize) -> GotoLineTarget {
    let line_count = buffer.len_lines().max(1);
    let line = line.clamp(1, line_count);
    let line_idx = line.saturating_sub(1);
    GotoLineTarget {
        line,
        column: clamped_goto_line_column(buffer, line_idx, column),
    }
}

fn clamped_goto_line_column(buffer: &TextBuffer, line_idx: usize, column: usize) -> usize {
    let column = column.max(1);
    let content_cap = column.saturating_sub(1);
    let content_chars = buffer.line_content_char_count_capped(line_idx, content_cap);
    content_chars.saturating_add(1).min(column)
}

fn normalize_goto_line_input_in_place(input: &mut String) {
    if let Some(normalized) = normalized_goto_line_input_if_needed(input) {
        *input = normalized;
    }
}

fn normalized_goto_line_input_if_needed(input: &str) -> Option<String> {
    let mut normalize_from = None;
    for (chars, (byte_idx, ch)) in input.char_indices().enumerate() {
        if chars >= GOTO_LINE_INPUT_MAX_CHARS
            || goto_line_input_char_is_hidden(ch)
            || ch.is_control()
        {
            normalize_from = Some((byte_idx, chars));
            break;
        }
    }
    let (normalize_byte, mut chars) = normalize_from?;

    let mut normalized = String::with_capacity(input.len().min(GOTO_LINE_INPUT_MAX_CHARS));
    normalized.push_str(&input[..normalize_byte]);
    for ch in input[normalize_byte..].chars() {
        if chars >= GOTO_LINE_INPUT_MAX_CHARS {
            break;
        }
        if goto_line_input_char_is_hidden(ch) {
            continue;
        }
        if ch.is_control() {
            if ch.is_whitespace() {
                normalized.push(' ');
                chars += 1;
            }
            continue;
        }
        normalized.push(ch);
        chars += 1;
    }
    Some(normalized)
}

#[cfg(test)]
fn normalized_goto_line_input(input: &str) -> String {
    normalized_goto_line_input_if_needed(input).unwrap_or_else(|| input.to_owned())
}

fn goto_line_input_char_is_hidden(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}'
    )
}

#[cfg(test)]
mod tests {
    use super::{
        GOTO_LINE_INPUT_MAX_CHARS, GotoLineTarget, clamped_goto_line_target,
        normalize_goto_line_input_in_place, normalized_goto_line_input,
    };
    use kuroya_core::TextBuffer;
    use std::path::PathBuf;

    #[test]
    fn goto_line_target_clamps_to_buffer_line_count() {
        let buffer = TextBuffer::from_text(
            1,
            Some(PathBuf::from("workspace/src/main.rs")),
            "one\ntwo".to_owned(),
        );

        assert_eq!(
            clamped_goto_line_target(&buffer, 99, 1),
            GotoLineTarget { line: 2, column: 1 }
        );
    }

    #[test]
    fn goto_line_target_clamps_column_to_line_content_end() {
        let buffer = TextBuffer::from_text(
            1,
            Some(PathBuf::from("workspace/src/main.rs")),
            "one\ntwo".to_owned(),
        );

        assert_eq!(
            clamped_goto_line_target(&buffer, 1, 3),
            GotoLineTarget { line: 1, column: 3 }
        );
        assert_eq!(
            clamped_goto_line_target(&buffer, 1, 4),
            GotoLineTarget { line: 1, column: 4 }
        );
        assert_eq!(
            clamped_goto_line_target(&buffer, 1, 99),
            GotoLineTarget { line: 1, column: 4 }
        );
        assert_eq!(
            clamped_goto_line_target(&buffer, 2, 99),
            GotoLineTarget { line: 2, column: 4 }
        );
    }

    #[test]
    fn goto_line_target_handles_empty_buffers() {
        let buffer = TextBuffer::from_text(
            1,
            Some(PathBuf::from("workspace/src/main.rs")),
            String::new(),
        );

        assert_eq!(
            clamped_goto_line_target(&buffer, 99, 99),
            GotoLineTarget { line: 1, column: 1 }
        );
    }

    #[test]
    fn goto_line_target_clamps_saturated_raw_values() {
        let buffer = TextBuffer::from_text(
            1,
            Some(PathBuf::from("workspace/src/main.rs")),
            "one\ntwo".to_owned(),
        );

        assert_eq!(
            clamped_goto_line_target(&buffer, usize::MAX, usize::MAX),
            GotoLineTarget { line: 2, column: 4 }
        );
    }

    #[test]
    fn goto_line_input_normalization_strips_hidden_controls_and_bounds_text() {
        let input = format!(
            "12\u{202e}:\u{200b}3\n{}\u{0}",
            "9".repeat(GOTO_LINE_INPUT_MAX_CHARS * 2)
        );

        let normalized = normalized_goto_line_input(&input);

        assert!(normalized.starts_with("12:3 "));
        assert!(normalized.chars().count() <= GOTO_LINE_INPUT_MAX_CHARS);
        assert!(!normalized.contains('\u{202e}'));
        assert!(!normalized.contains('\u{200b}'));
        assert!(!normalized.contains('\u{0}'));
    }

    #[test]
    fn goto_line_input_normalization_leaves_clean_input_in_place() {
        let mut input = "12:34".to_owned();
        let ptr = input.as_ptr();

        normalize_goto_line_input_in_place(&mut input);

        assert_eq!(input, "12:34");
        assert_eq!(input.as_ptr(), ptr);
    }
}

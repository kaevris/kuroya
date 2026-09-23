use super::text::rope_slice_text;
use super::{TextBuffer, TextEdit};
use std::cmp::Ordering;

impl TextBuffer {
    pub fn apply_save_cleanup(
        &mut self,
        trim_trailing_whitespace: bool,
        insert_final_newline: bool,
        trim_final_newlines: bool,
    ) -> bool {
        if self.read_only
            || (!trim_trailing_whitespace && !insert_final_newline && !trim_final_newlines)
        {
            return false;
        }

        let cleaned = self.cleaned_text_for_save(
            trim_trailing_whitespace,
            insert_final_newline,
            trim_final_newlines,
        );
        if self.text_equals(&cleaned) {
            return false;
        }

        self.apply_transaction(vec![TextEdit {
            range: 0..self.len_chars(),
            inserted: cleaned,
        }])
    }

    fn cleaned_text_for_save(
        &self,
        trim_trailing_whitespace: bool,
        insert_final_newline: bool,
        trim_final_newlines: bool,
    ) -> String {
        let line_ending = self.preferred_line_ending();
        let mut cleaned = String::with_capacity(self.len_bytes());

        if trim_trailing_whitespace {
            for line_idx in 0..self.len_lines() {
                let line = self.rope.line(line_idx);
                let line = rope_slice_text(&line);
                let line = line.as_ref();
                let (content, ending) = if let Some(content) = line.strip_suffix("\r\n") {
                    (content, "\r\n")
                } else if let Some(content) = line.strip_suffix('\n') {
                    (content, "\n")
                } else {
                    (line, "")
                };
                cleaned.push_str(content.trim_end_matches([' ', '\t']));
                cleaned.push_str(ending);
            }
        } else {
            for chunk in self.rope.chunks() {
                cleaned.push_str(chunk);
            }
        }

        if trim_final_newlines {
            cleaned = trim_extra_final_newlines(&cleaned, line_ending);
        }

        if insert_final_newline
            && !cleaned.is_empty()
            && !cleaned.ends_with('\n')
            && !cleaned.ends_with('\r')
        {
            cleaned.push_str(line_ending);
        }

        cleaned
    }

    pub(super) fn preferred_line_ending(&self) -> &'static str {
        let mut crlf_count = 0usize;
        let mut lf_count = 0usize;
        let mut previous_was_cr = false;
        let mut first_line_ending = None;
        for chunk in self.rope.chunks() {
            for ch in chunk.chars() {
                if ch == '\n' {
                    if previous_was_cr {
                        crlf_count += 1;
                        if first_line_ending.is_none() {
                            first_line_ending = Some("\r\n");
                        }
                    } else {
                        lf_count += 1;
                        if first_line_ending.is_none() {
                            first_line_ending = Some("\n");
                        }
                    }
                }
                previous_was_cr = ch == '\r';
            }
        }
        dominant_line_ending(crlf_count, lf_count, first_line_ending)
    }
}

fn dominant_line_ending(
    crlf_count: usize,
    lf_count: usize,
    first_line_ending: Option<&'static str>,
) -> &'static str {
    match crlf_count.cmp(&lf_count) {
        Ordering::Greater => "\r\n",
        Ordering::Equal => first_line_ending.unwrap_or("\n"),
        Ordering::Less => "\n",
    }
}

pub fn clean_text_for_save(
    text: &str,
    trim_trailing_whitespace: bool,
    insert_final_newline: bool,
    trim_final_newlines: bool,
) -> String {
    let line_ending = preferred_line_ending(text);
    let mut cleaned = if trim_trailing_whitespace {
        trim_line_trailing_whitespace(text)
    } else {
        text.to_owned()
    };

    if trim_final_newlines {
        cleaned = trim_extra_final_newlines(&cleaned, line_ending);
    }

    if insert_final_newline
        && !cleaned.is_empty()
        && !cleaned.ends_with('\n')
        && !cleaned.ends_with('\r')
    {
        cleaned.push_str(line_ending);
    }

    cleaned
}

fn preferred_line_ending(text: &str) -> &'static str {
    let mut crlf_count = 0usize;
    let mut lf_count = 0usize;
    let mut previous_was_cr = false;
    let mut first_line_ending = None;
    for ch in text.chars() {
        if ch == '\n' {
            if previous_was_cr {
                crlf_count += 1;
                if first_line_ending.is_none() {
                    first_line_ending = Some("\r\n");
                }
            } else {
                lf_count += 1;
                if first_line_ending.is_none() {
                    first_line_ending = Some("\n");
                }
            }
        }
        previous_was_cr = ch == '\r';
    }
    dominant_line_ending(crlf_count, lf_count, first_line_ending)
}

fn trim_line_trailing_whitespace(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());
    for segment in text.split_inclusive('\n') {
        let (content, ending) = if let Some(content) = segment.strip_suffix("\r\n") {
            (content, "\r\n")
        } else if let Some(content) = segment.strip_suffix('\n') {
            (content, "\n")
        } else {
            (segment, "")
        };
        cleaned.push_str(content.trim_end_matches([' ', '\t']));
        cleaned.push_str(ending);
    }
    cleaned
}

pub(super) fn trim_extra_final_newlines(text: &str, line_ending: &str) -> String {
    let mut trimmed = text;
    let mut ending_count = 0usize;
    loop {
        if let Some(rest) = trimmed.strip_suffix("\r\n") {
            trimmed = rest;
        } else if let Some(rest) = trimmed.strip_suffix('\n') {
            trimmed = rest;
        } else {
            break;
        }
        ending_count += 1;
    }

    let mut trimmed = trimmed.to_owned();
    if ending_count > 0 {
        trimmed.push_str(line_ending);
    }
    trimmed
}

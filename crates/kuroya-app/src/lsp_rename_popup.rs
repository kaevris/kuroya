use crate::{
    KuroyaApp,
    lsp_rename_requests::lsp_rename_bound_input,
    path_display::{compact_path, sanitized_display_label_cow},
    popup_buttons::{PopupButtonKind, popup_button},
};
use eframe::egui::{self, Context, Key, RichText, TextEdit};
use std::{borrow::Cow, path::Path};

const LSP_RENAME_POPUP_PATH_LABEL_MAX_CHARS: usize = 56;
const LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS: usize = 48;

impl KuroyaApp {
    pub(crate) fn render_lsp_rename(&mut self, ctx: &Context) {
        let mut rename = false;
        let mut cancel = false;
        lsp_rename_bound_input(&mut self.lsp_rename_input);

        egui::Window::new("Rename Symbol")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 188.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 164.0])
            .fixed_size([420.0, 120.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.lsp_rename_input)
                            .hint_text("New symbol name")
                            .desired_width(280.0),
                    );
                    if response.changed() {
                        lsp_rename_bound_input(&mut self.lsp_rename_input);
                    }
                    response.request_focus();

                    if ui.input(|input| input.key_pressed(Key::Enter)) {
                        rename = true;
                    }
                    if ui.input(|input| input.key_pressed(Key::Escape)) {
                        cancel = true;
                    }

                    if popup_button(ui, "Rename", PopupButtonKind::Primary).clicked() {
                        rename = true;
                    }
                });
                let status_line = self.lsp_rename_popup_status_line();
                ui.label(
                    RichText::new(status_line)
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            });

        if cancel {
            let status = lsp_rename_popup_cancel_status(&self.lsp_rename_input);
            self.lsp_rename_open = false;
            self.lsp_rename_input.clear();
            self.status = status;
        } else if rename {
            self.submit_lsp_rename();
        }
    }

    fn lsp_rename_popup_status_line(&self) -> String {
        let location = self
            .active_lsp_position()
            .map(|(_, path, _, line, column)| (path, line, column));
        let old_name = self.lsp_rename_popup_old_name();

        lsp_rename_popup_status_line(
            location
                .as_ref()
                .map(|(path, line, column)| (path.as_path(), *line, *column)),
            old_name.as_deref(),
            &self.lsp_rename_input,
        )
    }

    fn lsp_rename_popup_old_name(&self) -> Option<String> {
        self.active_buffer().and_then(|buffer| {
            buffer
                .selected_text()
                .filter(|text| !text.contains('\n'))
                .or_else(|| buffer.word_at_cursor())
        })
    }
}

fn lsp_rename_popup_status_line(
    location: Option<(&Path, usize, usize)>,
    old_name: Option<&str>,
    new_name: &str,
) -> String {
    let new_name = lsp_rename_popup_name_label(new_name, "new name");
    let rename_label = match old_name.and_then(lsp_rename_popup_optional_name_label) {
        Some(old_name) => format!("`{}` -> `{}`", old_name.as_ref(), new_name.as_ref()),
        None => format!("Rename to `{}`", new_name.as_ref()),
    };

    match location {
        Some((path, line, column)) => {
            format!(
                "{}  {rename_label}",
                lsp_rename_popup_location_label(path, line, column)
            )
        }
        None => rename_label,
    }
}

fn lsp_rename_popup_cancel_status(new_name: &str) -> String {
    match lsp_rename_popup_optional_name_label(new_name) {
        Some(new_name) => format!("Rename canceled for `{}`", new_name.as_ref()),
        None => "Rename canceled".to_owned(),
    }
}

fn lsp_rename_popup_location_label(path: &Path, line: usize, column: usize) -> String {
    format!(
        "{}:{}:{}",
        lsp_rename_popup_path_label(path),
        line.saturating_add(1),
        column.saturating_add(1)
    )
}

fn lsp_rename_popup_path_label(path: &Path) -> String {
    lsp_rename_popup_owned_display_label(
        compact_path(path),
        LSP_RENAME_POPUP_PATH_LABEL_MAX_CHARS,
        "file",
    )
}

fn lsp_rename_popup_owned_display_label(value: String, max_chars: usize, fallback: &str) -> String {
    match sanitized_display_label_cow(&value, max_chars, fallback) {
        Cow::Borrowed(label) if label.as_ptr() == value.as_ptr() && label.len() == value.len() => {
            value
        }
        Cow::Borrowed(label) => label.to_owned(),
        Cow::Owned(label) => label,
    }
}

fn lsp_rename_popup_name_label<'a>(name: &'a str, fallback: &str) -> Cow<'a, str> {
    sanitized_display_label_cow(name, LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS, fallback)
}

fn lsp_rename_popup_optional_name_label(name: &str) -> Option<Cow<'_, str>> {
    let label = sanitized_display_label_cow(name, LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS, "");
    (!label.is_empty()).then_some(label)
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS, LSP_RENAME_POPUP_PATH_LABEL_MAX_CHARS,
        lsp_rename_popup_cancel_status, lsp_rename_popup_location_label,
        lsp_rename_popup_name_label, lsp_rename_popup_optional_name_label,
        lsp_rename_popup_status_line,
    };
    use crate::path_display::sanitized_display_label;
    use std::{borrow::Cow, path::Path};

    #[test]
    fn rename_popup_status_line_sanitizes_and_bounds_display_fragments() {
        let path = Path::new("workspace/src").join(format!(
            "bad\n{}\u{202e}tail.rs",
            "path-".repeat(LSP_RENAME_POPUP_PATH_LABEL_MAX_CHARS)
        ));
        let old_name = format!(
            "old\n{}\u{202e}name",
            "symbol-".repeat(LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS)
        );
        let new_name = format!(
            "new\r\n{}\u{2066}name",
            "target-".repeat(LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS)
        );

        let status = lsp_rename_popup_status_line(Some((&path, 4, 8)), Some(&old_name), &new_name);

        assert!(old_name.contains('\n'));
        assert!(new_name.contains('\r'));
        assert_clean_popup_status(&status);
        assert!(status.contains("..."), "{status}");
        assert!(
            status.chars().count()
                <= LSP_RENAME_POPUP_PATH_LABEL_MAX_CHARS
                    + (2 * LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS)
                    + ":5:9  `` -> ``".chars().count()
        );
    }

    #[test]
    fn rename_popup_status_line_falls_back_for_blank_control_names() {
        let status = lsp_rename_popup_status_line(None, Some("\n\u{202e}\t"), "\r\n\u{2066}");

        assert_eq!(status, "Rename to `new name`");
    }

    #[test]
    fn rename_popup_cancel_status_sanitizes_user_text_without_requiring_it() {
        let status = lsp_rename_popup_cancel_status(" target\n\u{202e}name ");

        assert_eq!(status, "Rename canceled for `target name`");
        assert_eq!(
            lsp_rename_popup_cancel_status("\n\u{202e}\t"),
            "Rename canceled"
        );
    }

    #[test]
    fn rename_popup_location_label_sanitizes_path_and_uses_one_based_position() {
        let path = Path::new("workspace/src").join("name\r\u{202e}.rs");
        let location = lsp_rename_popup_location_label(&path, 0, 2);

        assert_eq!(location, "name .rs:1:3");
        assert_clean_popup_status(&location);
    }

    #[test]
    fn rename_popup_location_label_saturates_extreme_position_values() {
        let location = lsp_rename_popup_location_label(
            Path::new("workspace/src/main.rs"),
            usize::MAX,
            usize::MAX,
        );
        let expected = format!("main.rs:{}:{}", usize::MAX, usize::MAX);

        assert_eq!(location, expected);
    }

    #[test]
    fn rename_popup_name_labels_borrow_clean_ascii_and_unicode_display_text() {
        let ascii = "renamed_symbol";
        let unicode = "renamed_\u{03bb}";
        let required_ascii = lsp_rename_popup_name_label(ascii, "new name");
        let optional_ascii = lsp_rename_popup_optional_name_label(ascii).expect("label");
        let required_unicode = lsp_rename_popup_name_label(unicode, "new name");
        let optional_unicode = lsp_rename_popup_optional_name_label(unicode).expect("label");

        assert!(matches!(required_ascii, Cow::Borrowed("renamed_symbol")));
        assert!(matches!(optional_ascii, Cow::Borrowed("renamed_symbol")));
        match required_unicode {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
        match optional_unicode {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn rename_popup_name_labels_own_dirty_truncated_and_fallback_labels() {
        let dirty_required = lsp_rename_popup_name_label(" target\n\u{202e}name ", "new name");
        let dirty_optional =
            lsp_rename_popup_optional_name_label(" target\n\u{202e}name ").expect("label");
        let fallback = lsp_rename_popup_name_label("\n\u{202e}\t", "new name");
        let long_name = "x".repeat(LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS + 1);
        let truncated = lsp_rename_popup_name_label(&long_name, "new name");

        assert_eq!(dirty_required.as_ref(), "target name");
        assert_eq!(dirty_optional.as_ref(), "target name");
        assert_eq!(fallback.as_ref(), "new name");
        assert!(truncated.contains("..."), "{truncated}");
        assert!(truncated.chars().count() <= LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS);
        assert!(matches!(dirty_required, Cow::Owned(_)));
        assert!(matches!(dirty_optional, Cow::Owned(_)));
        assert!(matches!(fallback, Cow::Owned(_)));
        assert!(matches!(truncated, Cow::Owned(_)));
    }

    #[test]
    fn rename_popup_optional_name_labels_keep_blank_and_hidden_only_names_empty() {
        assert!(lsp_rename_popup_optional_name_label("").is_none());
        assert!(lsp_rename_popup_optional_name_label("   ").is_none());
        assert!(lsp_rename_popup_optional_name_label("\n\u{202e}\t").is_none());
        assert!(lsp_rename_popup_optional_name_label("\u{200b}\u{200c}\u{feff}").is_none());
    }

    #[test]
    fn rename_popup_name_labels_match_sanitized_display_output() {
        let cases = [
            ("renamed_symbol", "new name"),
            ("renamed_\u{03bb}", "new name"),
            (" renamed_symbol ", "new name"),
            ("target\n\u{202e}name", "new name"),
            ("\n\u{202e}\t", "new name"),
        ];

        for (name, fallback) in cases {
            assert_eq!(
                lsp_rename_popup_name_label(name, fallback).as_ref(),
                sanitized_display_label(name, LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS, fallback)
            );
        }

        let optional_cases = [
            "renamed_symbol",
            "renamed_\u{03bb}",
            " renamed_symbol ",
            "target\n\u{202e}name",
        ];
        for name in optional_cases {
            assert_eq!(
                lsp_rename_popup_optional_name_label(name)
                    .expect("optional label")
                    .as_ref(),
                sanitized_display_label(name, LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS, "")
            );
        }

        let long_name = "x".repeat(LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS + 1);
        assert_eq!(
            lsp_rename_popup_name_label(&long_name, "new name").as_ref(),
            sanitized_display_label(
                &long_name,
                LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS,
                "new name"
            )
        );
        assert_eq!(
            lsp_rename_popup_optional_name_label(&long_name)
                .expect("optional label")
                .as_ref(),
            sanitized_display_label(&long_name, LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS, "")
        );
    }

    #[test]
    fn rename_popup_name_labels_prepare_bounded_display_without_mutating_raw_text() {
        let raw_name = format!(
            " target\n{}\u{202e}name ",
            "symbol-".repeat(LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS)
        );
        let label = lsp_rename_popup_name_label(&raw_name, "new name");

        assert!(raw_name.contains('\n'));
        assert!(raw_name.contains('\u{202e}'));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= LSP_RENAME_POPUP_NAME_LABEL_MAX_CHARS);
    }

    fn assert_clean_popup_status(status: &str) {
        assert!(!status.contains('\n'));
        assert!(!status.contains('\r'));
        assert!(!status.contains('\u{202e}'));
        assert!(!status.contains('\u{2066}'));
    }
}

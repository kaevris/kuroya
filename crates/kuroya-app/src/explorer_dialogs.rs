use crate::{
    KuroyaApp,
    explorer::{ExplorerEntryKind, ExplorerFileAction},
    path_display::{display_path_label_cow, sanitized_display_label_cow},
    popup_buttons::{PopupButtonKind, popup_button},
};
use eframe::egui::{self, Align, Context, Key, TextEdit};
use std::{borrow::Cow, path::Path};

const EXPLORER_DIALOG_LABEL_MAX_CHARS: usize = 160;

impl KuroyaApp {
    pub(crate) fn render_explorer_file_action(&mut self, ctx: &Context) {
        let Some(action) = self.explorer_file_action.as_ref() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        let ExplorerDialogText { title, label, hint } = explorer_dialog_text(action);

        egui::Window::new(title)
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([520.0, 152.0])
            .show(ctx, |ui| {
                ui.label(label);
                let response = ui.add(
                    TextEdit::singleline(&mut self.explorer_file_input)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY),
                );
                response.request_focus();

                let (pressed_enter, pressed_escape) = ui.input(|input| {
                    (
                        input.key_pressed(Key::Enter),
                        input.key_pressed(Key::Escape),
                    )
                });
                if pressed_enter {
                    submit = true;
                }
                if pressed_escape {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "OK", PopupButtonKind::Primary).clicked() {
                        submit = true;
                    }
                });
            });

        if cancel {
            self.explorer_file_action = None;
            self.explorer_file_input.clear();
            self.status = "Explorer action canceled".to_owned();
        } else if submit {
            self.submit_explorer_file_action();
        }
    }
}

struct ExplorerDialogText {
    title: &'static str,
    label: String,
    hint: &'static str,
}

fn explorer_dialog_text(action: &ExplorerFileAction) -> ExplorerDialogText {
    match action {
        ExplorerFileAction::Rename { path, kind } => ExplorerDialogText {
            title: match kind {
                ExplorerEntryKind::File => "Rename File",
                ExplorerEntryKind::Folder => "Rename Folder",
            },
            label: explorer_dialog_label("Rename", path),
            hint: "New name",
        },
    }
}

fn explorer_dialog_label(prefix: &str, path: &Path) -> String {
    let label = format!("{prefix} {}", explorer_dialog_path_label(path));
    explorer_dialog_constructed_label_cow(Cow::Owned(label), prefix).into_owned()
}

fn explorer_dialog_constructed_label_cow<'a>(
    label: Cow<'a, str>,
    fallback_prefix: &str,
) -> Cow<'a, str> {
    match sanitized_display_label_cow(
        label.as_ref(),
        EXPLORER_DIALOG_LABEL_MAX_CHARS,
        fallback_prefix,
    ) {
        Cow::Borrowed(_) => label,
        Cow::Owned(label) => Cow::Owned(label),
    }
}

fn explorer_dialog_path_label(path: &Path) -> String {
    display_path_label_cow(path).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        EXPLORER_DIALOG_LABEL_MAX_CHARS, explorer_dialog_constructed_label_cow,
        explorer_dialog_label, explorer_dialog_path_label, explorer_dialog_text,
    };
    use crate::explorer::{ExplorerEntryKind, ExplorerFileAction};
    use crate::path_display::DISPLAY_PATH_LABEL_MAX_CHARS;
    use std::{borrow::Cow, path::PathBuf};

    #[test]
    fn explorer_dialog_path_labels_are_display_safe_and_bounded() {
        let path = PathBuf::from("workspace").join(format!(
            "bad\n\u{202e}dialog{}",
            "-segment".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));

        let label = explorer_dialog_path_label(&path);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn explorer_dialog_action_labels_are_display_safe_and_bounded() {
        let path = PathBuf::from("workspace").join(format!(
            "bad\n\u{202e}dialog{}",
            "-segment".repeat(EXPLORER_DIALOG_LABEL_MAX_CHARS)
        ));
        let actions = [
            ExplorerFileAction::Rename {
                path: path.clone(),
                kind: ExplorerEntryKind::File,
            },
            ExplorerFileAction::Rename {
                path,
                kind: ExplorerEntryKind::Folder,
            },
        ];

        for action in actions {
            let text = explorer_dialog_text(&action);
            assert!(!text.label.contains('\n'));
            assert!(!text.label.contains('\u{202e}'));
            assert!(text.label.contains("..."));
            assert!(text.label.chars().count() <= EXPLORER_DIALOG_LABEL_MAX_CHARS);
        }
    }

    #[test]
    fn explorer_dialog_constructed_label_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            explorer_dialog_constructed_label_cow(
                Cow::Borrowed("Create file in main.rs"),
                "Create file in"
            ),
            Cow::Borrowed("Create file in main.rs")
        ));

        let unicode = "Rename clean-\u{03bb}.rs";
        match explorer_dialog_constructed_label_cow(Cow::Borrowed(unicode), "Rename") {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn explorer_dialog_constructed_label_cow_owns_dirty_truncated_and_fallback_output() {
        let dirty = explorer_dialog_constructed_label_cow(
            Cow::Borrowed("Rename bad\nname\u{202e}.rs"),
            "Rename",
        );
        assert_eq!(dirty.as_ref(), "Rename bad name.rs");
        assert!(matches!(dirty, Cow::Owned(_)));

        let long = format!(
            "Rename main-{}.rs",
            "x".repeat(EXPLORER_DIALOG_LABEL_MAX_CHARS * 2)
        );
        let truncated = explorer_dialog_constructed_label_cow(Cow::Borrowed(&long), "Rename");
        assert!(truncated.contains("..."));
        assert!(truncated.chars().count() <= EXPLORER_DIALOG_LABEL_MAX_CHARS);
        assert!(matches!(truncated, Cow::Owned(_)));

        let fallback =
            explorer_dialog_constructed_label_cow(Cow::Borrowed("\n\u{202e}\u{0007}"), "Rename");
        assert_eq!(fallback.as_ref(), "Rename");
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[test]
    fn explorer_dialog_label_wrapper_matches_constructed_label_cow_helper() {
        let paths = [
            PathBuf::from("workspace/main.rs"),
            PathBuf::from("workspace/clean-\u{03bb}.rs"),
            PathBuf::from("workspace/bad\nname\u{202e}.rs"),
            PathBuf::from("workspace").join(format!(
                "main-{}.rs",
                "x".repeat(EXPLORER_DIALOG_LABEL_MAX_CHARS * 2)
            )),
        ];
        let prefixes = ["Create file in", "Create folder in", "Rename"];

        for prefix in prefixes {
            for path in &paths {
                let constructed = format!("{prefix} {}", explorer_dialog_path_label(path));
                assert_eq!(
                    explorer_dialog_label(prefix, path),
                    explorer_dialog_constructed_label_cow(Cow::Borrowed(&constructed), prefix)
                        .into_owned()
                );
            }
        }
    }

    #[test]
    fn explorer_dialog_text_labels_match_dialog_label_wrapper() {
        let path = PathBuf::from("workspace/bad\nname\u{202e}.rs");
        let actions = [
            (
                ExplorerFileAction::Rename {
                    path: path.clone(),
                    kind: ExplorerEntryKind::File,
                },
                "Rename File",
                "Rename",
                "New name",
            ),
            (
                ExplorerFileAction::Rename {
                    path: path.clone(),
                    kind: ExplorerEntryKind::Folder,
                },
                "Rename Folder",
                "Rename",
                "New name",
            ),
        ];

        for (action, title, prefix, hint) in actions {
            let text = explorer_dialog_text(&action);
            assert_eq!(text.title, title);
            assert_eq!(text.hint, hint);
            assert_eq!(text.label, explorer_dialog_label(prefix, &path));
        }
    }

    #[test]
    fn explorer_dialog_action_labels_preserve_raw_action_paths() {
        let path = PathBuf::from("workspace").join("raw\n\u{202e}dialog.rs");
        let original = path.clone();
        let action = ExplorerFileAction::Rename {
            path,
            kind: ExplorerEntryKind::File,
        };

        let text = explorer_dialog_text(&action);

        assert_eq!(text.title, "Rename File");
        assert_eq!(text.hint, "New name");
        assert!(!text.label.contains('\n'));
        assert!(!text.label.contains('\u{202e}'));
        match action {
            ExplorerFileAction::Rename { path, .. } => {
                assert_eq!(path, original);
                assert!(path.as_os_str().to_string_lossy().contains('\n'));
                assert!(path.as_os_str().to_string_lossy().contains('\u{202e}'));
            }
        }
    }
}

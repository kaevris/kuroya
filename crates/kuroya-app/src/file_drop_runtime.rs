use crate::{
    KuroyaApp,
    file_runtime::file_path_open_buffer_or_known_openable,
    path_display::display_path_label_cow,
    popup_buttons::{PopupButtonKind, popup_button},
    transient_state::PendingEditorFileDrop,
};
use eframe::egui::{self, Align2, Context, DroppedFile, RichText};
use kuroya_core::{EditorDropIntoEditorShowDropSelector, TextBuffer};
use std::path::{Path, PathBuf};

impl KuroyaApp {
    pub(crate) fn handle_editor_file_drops(&mut self, ctx: &Context) {
        let paths = ctx.input(|input| dropped_file_paths(&input.raw.dropped_files));
        if paths.is_empty() {
            return;
        }

        if !self.settings.drop_into_editor_enabled {
            self.status = format!(
                "Ignored {} dropped file{} because drop into editor is disabled",
                paths.len(),
                plural_s(paths.len())
            );
            return;
        }

        if drop_selector_should_queue(self.settings.drop_into_editor_show_drop_selector) {
            let count = paths.len();
            self.pending_editor_file_drop = Some(PendingEditorFileDrop { paths });
            self.status = pending_dropped_file_status(count);
            return;
        }

        let (opened, skipped) = self.open_dropped_file_paths(paths);
        self.status = dropped_file_status(opened, skipped);
    }

    pub(crate) fn render_editor_file_drop_selector(&mut self, ctx: &Context) {
        let Some(drop) = self.pending_editor_file_drop.as_ref() else {
            return;
        };

        let mut open_files = false;
        let mut cancel = false;
        egui::Window::new("Dropped Files")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_min_width(360.0);
                ui.label(
                    RichText::new(format!(
                        "Open {} dropped item{}?",
                        drop.paths.len(),
                        plural_s(drop.paths.len())
                    ))
                    .strong(),
                );
                ui.add_space(8.0);
                for path in drop.paths.iter().take(6) {
                    ui.label(dropped_file_selector_path_label(path));
                }
                if drop.paths.len() > 6 {
                    ui.label(format!(
                        "... and {} more",
                        drop.paths.len().saturating_sub(6)
                    ));
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Open", PopupButtonKind::Primary).clicked() {
                        open_files = true;
                    }
                });
            });

        if cancel {
            self.cancel_pending_editor_file_drop();
        } else if open_files {
            self.open_pending_editor_file_drop();
        }
    }

    fn open_pending_editor_file_drop(&mut self) {
        let Some(drop) = self.pending_editor_file_drop.take() else {
            return;
        };
        let (opened, skipped) = self.open_dropped_file_paths(drop.paths);
        self.status = dropped_file_status(opened, skipped);
    }

    fn cancel_pending_editor_file_drop(&mut self) {
        let Some(drop) = self.pending_editor_file_drop.take() else {
            return;
        };
        self.status = cancelled_dropped_file_status(drop.paths.len());
    }

    fn open_dropped_file_paths(&mut self, paths: Vec<PathBuf>) -> (usize, usize) {
        let mut opened = 0;
        let mut skipped = 0;
        for path in paths {
            let openable = dropped_file_path_is_openable(
                &self.buffers,
                self.index.files(),
                &path,
                Path::is_file,
            );
            if openable {
                self.spawn_open_file(path);
                opened += 1;
            } else {
                skipped += 1;
            }
        }
        (opened, skipped)
    }
}

pub(crate) fn dropped_file_paths(files: &[DroppedFile]) -> Vec<PathBuf> {
    let mut paths = Vec::with_capacity(files.len());
    for file in files {
        let Some(path) = &file.path else {
            continue;
        };
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
    paths
}

fn dropped_file_selector_path_label(path: &Path) -> String {
    display_path_label_cow(path).into_owned()
}

fn dropped_file_path_is_openable(
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    path_is_file: impl FnOnce(&Path) -> bool,
) -> bool {
    file_path_open_buffer_or_known_openable(buffers, indexed_files, path, path_is_file)
}

fn drop_selector_should_queue(mode: EditorDropIntoEditorShowDropSelector) -> bool {
    matches!(mode, EditorDropIntoEditorShowDropSelector::AfterDrop)
}

fn pending_dropped_file_status(count: usize) -> String {
    format!(
        "Waiting for drop action for {count} dropped item{}",
        plural_s(count)
    )
}

fn cancelled_dropped_file_status(count: usize) -> String {
    format!("Cancelled {} dropped item{}", count, plural_s(count))
}

fn dropped_file_status(opened: usize, skipped: usize) -> String {
    match (opened, skipped) {
        (0, 0) => "No dropped files to open".to_owned(),
        (0, skipped) => format!(
            "Skipped {skipped} dropped item{} because no readable file path was available",
            plural_s(skipped)
        ),
        (1, 0) => "Opening dropped file".to_owned(),
        (opened, 0) => format!("Opening {opened} dropped files"),
        (1, skipped) => format!(
            "Opening dropped file, skipped {skipped} dropped item{}",
            plural_s(skipped)
        ),
        (opened, skipped) => format!(
            "Opening {opened} dropped files, skipped {skipped} dropped item{}",
            plural_s(skipped)
        ),
    }
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_display::DISPLAY_PATH_LABEL_MAX_CHARS;
    use kuroya_core::EditorDropIntoEditorShowDropSelector;
    use std::{cell::Cell, path::PathBuf};

    #[test]
    fn dropped_file_paths_keeps_unique_native_paths() {
        let src = PathBuf::from("src/main.rs");
        let readme = PathBuf::from("README.md");
        let files = [
            DroppedFile {
                path: Some(src.clone()),
                ..Default::default()
            },
            DroppedFile {
                path: None,
                name: "web-payload.txt".to_owned(),
                ..Default::default()
            },
            DroppedFile {
                path: Some(src.clone()),
                ..Default::default()
            },
            DroppedFile {
                path: Some(readme.clone()),
                ..Default::default()
            },
        ];

        assert_eq!(dropped_file_paths(&files), vec![src, readme]);
    }

    #[test]
    fn dropped_file_paths_preserves_raw_paths_for_opening() {
        let path = PathBuf::from("workspace").join(format!(
            "dropped\n{}\u{202e}tail.rs",
            "very-long-component-".repeat(8)
        ));
        let files = [DroppedFile {
            path: Some(path.clone()),
            ..Default::default()
        }];

        assert_eq!(dropped_file_paths(&files), vec![path]);
    }

    #[test]
    fn dropped_file_openability_uses_open_buffer_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let buffers = vec![TextBuffer::from_text(7, Some(path), "open\n".to_owned())];
        let probes = Cell::new(0usize);

        assert!(dropped_file_path_is_openable(
            &buffers,
            &[],
            &equivalent_path,
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn dropped_file_openability_uses_index_before_filesystem_probe() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let indexed = vec![path];
        let probes = Cell::new(0usize);

        assert!(dropped_file_path_is_openable(
            &[],
            &indexed,
            &equivalent_path,
            |_| {
                probes.set(probes.get() + 1);
                false
            },
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn dropped_file_openability_rejects_stale_unknown_targets() {
        let path = PathBuf::from("workspace/src/missing.rs");
        let probes = Cell::new(0usize);

        assert!(!dropped_file_path_is_openable(&[], &[], &path, |_| {
            probes.set(probes.get() + 1);
            false
        }));

        assert_eq!(probes.get(), 1);
    }

    #[test]
    fn dropped_file_status_reports_opened_and_skipped_items() {
        assert_eq!(dropped_file_status(1, 0), "Opening dropped file");
        assert_eq!(dropped_file_status(2, 0), "Opening 2 dropped files");
        assert_eq!(
            dropped_file_status(0, 1),
            "Skipped 1 dropped item because no readable file path was available"
        );
        assert_eq!(
            dropped_file_status(2, 1),
            "Opening 2 dropped files, skipped 1 dropped item"
        );
    }

    #[test]
    fn dropped_file_selector_path_label_sanitizes_control_and_bidi_text() {
        let path = PathBuf::from("workspace").join("dropped\n\u{202e}file.rs");

        let label = dropped_file_selector_path_label(&path);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert_eq!(label, "dropped file.rs");
    }

    #[test]
    fn dropped_file_selector_path_label_bounds_long_text() {
        let path = PathBuf::from("workspace").join(format!("{}tail.rs", "very-long-".repeat(24)));

        let label = dropped_file_selector_path_label(&path);

        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn dropped_file_selector_respects_show_selector_setting() {
        assert!(drop_selector_should_queue(
            EditorDropIntoEditorShowDropSelector::AfterDrop
        ));
        assert!(!drop_selector_should_queue(
            EditorDropIntoEditorShowDropSelector::Never
        ));
    }

    #[test]
    fn dropped_file_selector_statuses_report_pending_and_cancelled_items() {
        assert_eq!(
            pending_dropped_file_status(2),
            "Waiting for drop action for 2 dropped items"
        );
        assert_eq!(cancelled_dropped_file_status(1), "Cancelled 1 dropped item");
    }
}

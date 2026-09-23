use crate::{
    KuroyaApp,
    lsp_rename_requests::{lsp_rename_bounded_display_label, lsp_rename_display_label},
    path_display::{display_path_label_cow, sanitized_display_label_cow},
    popup_buttons::{PopupButtonKind, popup_button},
    ui_text::truncate_middle,
    workspace_state::paths_match_lexically,
    workspace_trust::workspace_path_contains_lexically,
};
use eframe::egui::{self, Context, Key, RichText, ScrollArea};
use kuroya_core::LspTextEdit;
#[cfg(test)]
use std::collections::BTreeMap;
use std::{
    borrow::Cow,
    collections::BTreeSet,
    ops::Range,
    path::{Path, PathBuf},
};

const LSP_RENAME_PREVIEW_ROW_HEIGHT: f32 = 24.0;
const LSP_RENAME_PREVIEW_PATH_LABEL_CHARS: usize = 64;
const LSP_RENAME_PREVIEW_FULL_PATH_LABEL_CHARS: usize = 120;
const LSP_RENAME_PREVIEW_REPLACEMENT_LABEL_CHARS: usize = 80;
const LSP_RENAME_PREVIEW_NEW_NAME_LABEL_CHARS: usize = 80;
const LSP_RENAME_PREVIEW_MAX_EDITS: usize = 2_000;
const LSP_RENAME_PREVIEW_MAX_FILES: usize = 512;
const LSP_RENAME_PREVIEW_MAX_ROWS: usize =
    LSP_RENAME_PREVIEW_MAX_EDITS + LSP_RENAME_PREVIEW_MAX_FILES;

const LSP_RENAME_PREVIEW_UNOPENED_VERSION: u64 = u64::MAX;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LspRenamePreviewRow {
    #[cfg(test)]
    Header {
        path: PathBuf,
    },
    #[cfg(test)]
    Edit {
        edit_index: usize,
    },
    CachedHeader {
        path: PathBuf,
        label: String,
    },
    CachedEdit {
        edit_index: usize,
        edit: LspTextEdit,
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LspRenamePreviewDisplayRow<'a> {
    Header { label: Cow<'a, str> },
    Edit { label: Cow<'a, str> },
}

impl KuroyaApp {
    pub(crate) fn open_lsp_rename_preview(&mut self, new_name: String, edits: Vec<LspTextEdit>) {
        let new_name_label =
            lsp_rename_preview_status_name_label(&lsp_rename_display_label(&new_name));
        if edits.is_empty() {
            self.close_lsp_rename_preview(&format!("Rename `{new_name_label}` returned no edits"));
            return;
        }
        if let Some(reason) = lsp_rename_preview_reject_reason(&self.workspace.root, &edits) {
            self.close_lsp_rename_preview(&format!("Rename rejected: {reason}"));
            return;
        }

        self.lsp_rename_preview_versions.clear();
        for path in lsp_rename_preview_paths(&edits) {
            let version = lsp_rename_preview_captured_version(
                self.buffer_by_lexical_path(&path)
                    .map(|buffer| buffer.version()),
            );
            self.lsp_rename_preview_versions.insert(path, version);
        }

        let (file_count, edit_count) = lsp_rename_preview_counts(&edits);
        self.lsp_rename_preview_new_name = new_name_label;
        self.lsp_rename_preview_rows = lsp_rename_preview_rows(&edits);
        self.lsp_rename_preview_edits = edits;
        self.lsp_rename_preview_open = true;
        self.status = format!("Previewing rename across {file_count} files, {edit_count} edits");
    }

    pub(crate) fn render_lsp_rename_preview(&mut self, ctx: &Context) {
        let mut apply = false;
        let mut cancel = false;
        let file_count = self.lsp_rename_preview_versions.len();
        let edit_count = self.lsp_rename_preview_edits.len();

        egui::Window::new("Rename Preview")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 160.0,
            ))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 136.0])
            .default_size([620.0, 420.0])
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }
                if ui.input(|input| {
                    input.key_pressed(Key::Enter)
                        && (input.modifiers.command || input.modifiers.ctrl)
                }) {
                    apply = true;
                }

                ui.label(
                    RichText::new(format!("Rename to `{}`", self.lsp_rename_preview_new_name))
                        .strong(),
                );
                ui.label(
                    RichText::new(format!("{file_count} files, {edit_count} edits"))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.separator();

                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(
                        ui,
                        LSP_RENAME_PREVIEW_ROW_HEIGHT,
                        self.lsp_rename_preview_rows
                            .len()
                            .min(LSP_RENAME_PREVIEW_MAX_ROWS),
                        |ui, visible_rows| {
                            lsp_rename_preview_for_each_visible_display_row(
                                &self.lsp_rename_preview_rows,
                                &self.lsp_rename_preview_edits,
                                visible_rows,
                                |display_row| match display_row {
                                    LspRenamePreviewDisplayRow::Header { label } => {
                                        ui.label(RichText::new(label.into_owned()).strong());
                                    }
                                    LspRenamePreviewDisplayRow::Edit { label } => {
                                        ui.monospace(label.into_owned());
                                    }
                                },
                            );
                        },
                    );

                ui.separator();
                ui.horizontal(|ui| {
                    if popup_button(ui, "Apply", PopupButtonKind::Primary)
                        .on_hover_text("Ctrl+Enter")
                        .clicked()
                    {
                        apply = true;
                    }
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                });
            });

        if cancel {
            self.close_lsp_rename_preview("Rename canceled");
        } else if apply {
            self.apply_lsp_rename_preview();
        }
    }

    fn apply_lsp_rename_preview(&mut self) {
        if !self.lsp_rename_preview_open {
            return;
        }
        if self.lsp_rename_preview_edits.is_empty() {
            self.close_lsp_rename_preview("Rename preview is empty");
            return;
        }
        if let Some(reason) =
            lsp_rename_preview_reject_reason(&self.workspace.root, &self.lsp_rename_preview_edits)
        {
            self.status = format!("Rename rejected: {reason}");
            return;
        }
        if !lsp_rename_preview_rows_are_current(
            &self.lsp_rename_preview_rows,
            &self.lsp_rename_preview_edits,
        ) {
            self.status = "Rename preview is stale".to_owned();
            return;
        }
        if let Some(path) = self.lsp_rename_preview_stale_path() {
            self.status = format!(
                "Rename preview is stale for {}",
                lsp_rename_preview_path_label(&path)
            );
            return;
        }

        let edits = std::mem::take(&mut self.lsp_rename_preview_edits);
        let new_name = std::mem::take(&mut self.lsp_rename_preview_new_name);
        self.lsp_rename_preview_rows.clear();
        self.lsp_rename_preview_versions.clear();
        self.lsp_rename_preview_open = false;
        self.apply_lsp_workspace_edits(edits, &format!("Rename `{new_name}`"));
    }

    pub(crate) fn clear_lsp_rename_preview_state(&mut self) {
        self.lsp_rename_preview_open = false;
        self.lsp_rename_preview_new_name.clear();
        self.lsp_rename_preview_edits.clear();
        self.lsp_rename_preview_rows.clear();
        self.lsp_rename_preview_versions.clear();
    }

    fn close_lsp_rename_preview(&mut self, status: &str) {
        self.clear_lsp_rename_preview_state();
        self.status = status.to_owned();
    }

    pub(crate) fn clear_lsp_rename_preview_for_path(&mut self, path: &Path) -> bool {
        if !self.lsp_rename_preview_open
            || !lsp_rename_preview_touches_path(&self.lsp_rename_preview_edits, path)
        {
            return false;
        }

        self.close_lsp_rename_preview(&format!(
            "Rename preview cleared for {}",
            lsp_rename_preview_path_label(path)
        ));
        true
    }

    fn lsp_rename_preview_stale_path(&self) -> Option<PathBuf> {
        self.lsp_rename_preview_versions
            .iter()
            .find_map(|(path, preview_version)| {
                let current_version = self
                    .buffer_by_lexical_path(path)
                    .map(|buffer| buffer.version());
                lsp_rename_preview_path_is_stale(*preview_version, current_version)
                    .then(|| path.clone())
            })
    }
}

pub(crate) fn lsp_rename_preview_counts(edits: &[LspTextEdit]) -> (usize, usize) {
    (lsp_rename_preview_paths(edits).len(), edits.len())
}

pub(crate) fn lsp_rename_preview_edit_label(edit: &LspTextEdit) -> String {
    let replacement = lsp_rename_preview_replacement_label(&edit.new_text);
    lsp_rename_preview_bound_display_label(format!(
        "{}:{}-{}:{} -> {}",
        edit.start_line, edit.start_column, edit.end_line, edit.end_column, replacement
    ))
}

fn lsp_rename_preview_for_each_visible_display_row<'a>(
    rows: &'a [LspRenamePreviewRow],
    edits: &'a [LspTextEdit],
    visible_rows: Range<usize>,
    mut visit: impl FnMut(LspRenamePreviewDisplayRow<'a>),
) {
    let start = visible_rows.start.min(rows.len());
    let end = visible_rows
        .end
        .min(rows.len().min(LSP_RENAME_PREVIEW_MAX_ROWS));
    if start >= end {
        return;
    }

    for row in &rows[start..end] {
        if let Some(display_row) = lsp_rename_preview_display_row(row, edits) {
            visit(display_row);
        }
    }
}

#[cfg(test)]
fn lsp_rename_preview_visible_display_rows<'a>(
    rows: &'a [LspRenamePreviewRow],
    edits: &'a [LspTextEdit],
    visible_rows: Range<usize>,
) -> Vec<LspRenamePreviewDisplayRow<'a>> {
    let mut display_rows = Vec::new();
    lsp_rename_preview_for_each_visible_display_row(rows, edits, visible_rows, |display_row| {
        display_rows.push(display_row);
    });
    display_rows
}

fn lsp_rename_preview_display_row<'a>(
    row: &'a LspRenamePreviewRow,
    edits: &'a [LspTextEdit],
) -> Option<LspRenamePreviewDisplayRow<'a>> {
    match row {
        #[cfg(test)]
        LspRenamePreviewRow::Header { path } => Some(LspRenamePreviewDisplayRow::Header {
            label: Cow::Owned(lsp_rename_preview_path_label(path)),
        }),
        #[cfg(test)]
        LspRenamePreviewRow::Edit { edit_index } => {
            edits
                .get(*edit_index)
                .map(|edit| LspRenamePreviewDisplayRow::Edit {
                    label: Cow::Owned(lsp_rename_preview_edit_label(edit)),
                })
        }
        LspRenamePreviewRow::CachedHeader { path, label } => edits
            .iter()
            .any(|edit| edit.path.as_path() == path.as_path())
            .then_some(LspRenamePreviewDisplayRow::Header {
                label: Cow::Borrowed(label.as_str()),
            }),
        LspRenamePreviewRow::CachedEdit {
            edit_index,
            edit,
            label,
        } => edits.get(*edit_index).and_then(|current| {
            (current == edit).then_some(LspRenamePreviewDisplayRow::Edit {
                label: Cow::Borrowed(label.as_str()),
            })
        }),
    }
}

fn lsp_rename_preview_replacement_label(text: &str) -> String {
    if text.is_empty() {
        "(delete)".to_owned()
    } else {
        lsp_rename_preview_bound_display_label(lsp_rename_bounded_display_label(text, 48))
    }
}

fn lsp_rename_preview_path_label(path: &Path) -> String {
    truncate_middle(
        display_path_label_cow(path).as_ref(),
        LSP_RENAME_PREVIEW_PATH_LABEL_CHARS,
    )
}

fn lsp_rename_preview_full_path_label(path: &Path) -> String {
    lsp_rename_preview_owned_display_label(
        path.display().to_string(),
        LSP_RENAME_PREVIEW_FULL_PATH_LABEL_CHARS,
        ".",
    )
}

fn lsp_rename_preview_owned_display_label(
    value: String,
    max_chars: usize,
    fallback: &str,
) -> String {
    match sanitized_display_label_cow(&value, max_chars, fallback) {
        Cow::Borrowed(label) if label.as_ptr() == value.as_ptr() && label.len() == value.len() => {
            value
        }
        Cow::Borrowed(label) => label.to_owned(),
        Cow::Owned(label) => label,
    }
}

fn lsp_rename_preview_status_name_label(new_name_label: &str) -> String {
    truncate_middle(new_name_label, LSP_RENAME_PREVIEW_NEW_NAME_LABEL_CHARS)
}

fn lsp_rename_preview_bound_display_label(label: String) -> String {
    truncate_middle(&label, LSP_RENAME_PREVIEW_REPLACEMENT_LABEL_CHARS)
}

fn lsp_rename_preview_paths(edits: &[LspTextEdit]) -> BTreeSet<PathBuf> {
    edits.iter().map(|edit| edit.path.clone()).collect()
}

fn lsp_rename_preview_touches_path(edits: &[LspTextEdit], path: &Path) -> bool {
    edits
        .iter()
        .any(|edit| paths_match_lexically(&edit.path, path))
}

fn lsp_rename_preview_captured_version(current_version: Option<u64>) -> u64 {
    current_version.unwrap_or(LSP_RENAME_PREVIEW_UNOPENED_VERSION)
}

fn lsp_rename_preview_path_is_stale(preview_version: u64, current_version: Option<u64>) -> bool {
    if preview_version == LSP_RENAME_PREVIEW_UNOPENED_VERSION {
        return current_version.is_some();
    }

    current_version != Some(preview_version)
}

fn lsp_rename_preview_reject_reason(root: &Path, edits: &[LspTextEdit]) -> Option<String> {
    if edits.len() > LSP_RENAME_PREVIEW_MAX_EDITS {
        return Some(format!(
            "too many edits ({}, max {LSP_RENAME_PREVIEW_MAX_EDITS})",
            edits.len()
        ));
    }

    let mut paths = BTreeSet::new();
    for edit in edits {
        if paths.insert(edit.path.clone()) && paths.len() > LSP_RENAME_PREVIEW_MAX_FILES {
            return Some(format!(
                "too many files ({}, max {LSP_RENAME_PREVIEW_MAX_FILES})",
                paths.len()
            ));
        }
        if !path_is_within_workspace(root, &edit.path) {
            return Some(format!(
                "edit outside workspace: {}",
                lsp_rename_preview_full_path_label(&edit.path)
            ));
        }
        if !lsp_rename_preview_edit_range_is_valid(edit) {
            return Some(format!(
                "invalid edit range: {}",
                lsp_rename_preview_edit_location_label(edit)
            ));
        }
    }

    None
}

fn path_is_within_workspace(root: &Path, path: &Path) -> bool {
    workspace_path_contains_lexically(root, path)
}

fn lsp_rename_preview_edit_range_is_valid(edit: &LspTextEdit) -> bool {
    edit.start_line > 0
        && edit.start_column > 0
        && edit.end_line > 0
        && edit.end_column > 0
        && (edit.end_line > edit.start_line
            || (edit.end_line == edit.start_line && edit.end_column >= edit.start_column))
}

fn lsp_rename_preview_edit_location_label(edit: &LspTextEdit) -> String {
    format!(
        "{}:{}:{}-{}:{}",
        lsp_rename_preview_path_label(&edit.path),
        edit.start_line,
        edit.start_column,
        edit.end_line,
        edit.end_column
    )
}

fn lsp_rename_preview_rows_are_current(
    rows: &[LspRenamePreviewRow],
    edits: &[LspTextEdit],
) -> bool {
    rows == lsp_rename_preview_rows(edits)
}

pub(crate) fn lsp_rename_preview_rows(edits: &[LspTextEdit]) -> Vec<LspRenamePreviewRow> {
    let mut indices = (0..edits.len()).collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        edits[*left]
            .path
            .cmp(&edits[*right].path)
            .then_with(|| left.cmp(right))
    });

    let mut file_count = 0;
    let mut current_path: Option<&Path> = None;
    for &index in &indices {
        let path = edits[index].path.as_path();
        if current_path != Some(path) {
            file_count += 1;
            current_path = Some(path);
        }
    }

    let mut rows = Vec::with_capacity(indices.len().saturating_add(file_count));
    let mut current_path: Option<&Path> = None;
    for index in indices {
        let path = edits[index].path.as_path();
        if current_path != Some(path) {
            rows.push(LspRenamePreviewRow::CachedHeader {
                path: path.to_path_buf(),
                label: lsp_rename_preview_path_label(path),
            });
            current_path = Some(path);
        }
        rows.push(LspRenamePreviewRow::CachedEdit {
            edit_index: index,
            edit: edits[index].clone(),
            label: lsp_rename_preview_edit_label(&edits[index]),
        });
    }
    rows
}

#[cfg(test)]
fn lsp_rename_preview_edits_by_path(edits: &[LspTextEdit]) -> BTreeMap<PathBuf, Vec<&LspTextEdit>> {
    let mut by_path = BTreeMap::new();
    for edit in edits {
        by_path
            .entry(edit.path.clone())
            .or_insert_with(Vec::new)
            .push(edit);
    }
    by_path
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_RENAME_PREVIEW_MAX_EDITS, LSP_RENAME_PREVIEW_MAX_FILES, LSP_RENAME_PREVIEW_MAX_ROWS,
        LSP_RENAME_PREVIEW_PATH_LABEL_CHARS, LSP_RENAME_PREVIEW_REPLACEMENT_LABEL_CHARS,
        LSP_RENAME_PREVIEW_UNOPENED_VERSION, LspRenamePreviewDisplayRow, LspRenamePreviewRow,
        lsp_rename_preview_counts, lsp_rename_preview_edit_label,
        lsp_rename_preview_edit_range_is_valid, lsp_rename_preview_edits_by_path,
        lsp_rename_preview_path_is_stale, lsp_rename_preview_path_label,
        lsp_rename_preview_reject_reason, lsp_rename_preview_replacement_label,
        lsp_rename_preview_rows, lsp_rename_preview_visible_display_rows, path_is_within_workspace,
    };
    use crate::{KuroyaApp, app_startup_context::AppStartupContext, terminal::TerminalPane};
    use kuroya_core::{EditorSettings, LspTextEdit, TextBuffer, Workspace};
    use std::{
        borrow::Cow,
        path::{Path, PathBuf},
        time::Instant,
    };
    use tokio::runtime::Runtime;

    #[test]
    fn rename_preview_counts_unique_files_and_edits() {
        let edits = vec![
            edit("src/lib.rs", 8, "renamed"),
            edit("src/lib.rs", 2, "renamed"),
            edit("src/main.rs", 4, "renamed"),
        ];

        assert_eq!(lsp_rename_preview_counts(&edits), (2, 3));
    }

    #[test]
    fn rename_preview_groups_edits_by_sorted_path() {
        let edits = vec![
            edit("src/main.rs", 9, "main_renamed"),
            edit("src/lib.rs", 12, "lib_renamed"),
            edit("src/lib.rs", 3, "lib_renamed"),
        ];
        let grouped = lsp_rename_preview_edits_by_path(&edits);
        let paths: Vec<_> = grouped.keys().cloned().collect();

        assert_eq!(
            paths,
            vec![PathBuf::from("src/lib.rs"), PathBuf::from("src/main.rs")]
        );
        assert_eq!(grouped[&PathBuf::from("src/lib.rs")].len(), 2);
        assert_eq!(grouped[&PathBuf::from("src/main.rs")].len(), 1);
    }

    #[test]
    fn rename_preview_rows_group_once_and_reference_original_edits() {
        let main = edit("src/main.rs", 9, "main_renamed");
        let lib_twelve = edit("src/lib.rs", 12, "lib_renamed");
        let lib_three = edit("src/lib.rs", 3, "lib_renamed");
        let edits = vec![main.clone(), lib_twelve.clone(), lib_three.clone()];

        assert_eq!(
            lsp_rename_preview_rows(&edits),
            vec![
                LspRenamePreviewRow::CachedHeader {
                    path: PathBuf::from("src/lib.rs"),
                    label: "lib.rs".to_owned(),
                },
                LspRenamePreviewRow::CachedEdit {
                    edit_index: 1,
                    edit: lib_twelve,
                    label: "12:1-12:5 -> lib_renamed".to_owned(),
                },
                LspRenamePreviewRow::CachedEdit {
                    edit_index: 2,
                    edit: lib_three,
                    label: "3:1-3:5 -> lib_renamed".to_owned(),
                },
                LspRenamePreviewRow::CachedHeader {
                    path: PathBuf::from("src/main.rs"),
                    label: "main.rs".to_owned(),
                },
                LspRenamePreviewRow::CachedEdit {
                    edit_index: 0,
                    edit: main,
                    label: "9:1-9:5 -> main_renamed".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn rename_preview_labels_locations_and_sanitized_replacements() {
        let edit = edit("src/lib.rs", 3, "new\nname");

        assert_eq!(
            lsp_rename_preview_edit_label(&edit),
            "3:1-3:5 -> new\\nname"
        );
        assert_eq!(lsp_rename_preview_replacement_label(""), "(delete)");
        assert_eq!(
            lsp_rename_preview_replacement_label(
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
            ),
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUV..."
        );
        assert_eq!(
            lsp_rename_preview_replacement_label("new\tname\u{7}"),
            "new\\tname\\u{7}"
        );

        let control_heavy_label = lsp_rename_preview_replacement_label(&"\n".repeat(96));
        assert!(control_heavy_label.contains("..."));
        assert!(control_heavy_label.chars().count() <= LSP_RENAME_PREVIEW_REPLACEMENT_LABEL_CHARS);
    }

    #[test]
    fn rename_preview_visible_display_rows_materialize_labels_without_mutating_edits() {
        let edits = vec![
            edit("src/main.rs", 9, "main\nrenamed"),
            edit(
                "src/lib.rs",
                12,
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
            ),
            edit("src/lib.rs", 3, ""),
        ];
        let original_edits = edits.clone();
        let rows = lsp_rename_preview_rows(&edits);

        let display_rows = lsp_rename_preview_visible_display_rows(&rows, &edits, 1..4);

        assert_eq!(
            display_rows,
            vec![
                LspRenamePreviewDisplayRow::Edit {
                    label: Cow::Borrowed(
                        "12:1-12:5 -> abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUV...",
                    ),
                },
                LspRenamePreviewDisplayRow::Edit {
                    label: Cow::Borrowed("3:1-3:5 -> (delete)"),
                },
                LspRenamePreviewDisplayRow::Header {
                    label: Cow::Borrowed("main.rs"),
                },
            ]
        );
        assert_eq!(edits, original_edits);
    }

    #[test]
    fn rename_preview_visible_display_rows_borrow_cached_labels() {
        let edits = vec![edit("src/lib.rs", 12, "lib_renamed")];
        let rows = lsp_rename_preview_rows(&edits);

        let display_rows = lsp_rename_preview_visible_display_rows(&rows, &edits, 0..rows.len());

        assert!(matches!(
            &display_rows[..],
            [
                LspRenamePreviewDisplayRow::Header {
                    label: Cow::Borrowed("lib.rs"),
                },
                LspRenamePreviewDisplayRow::Edit {
                    label: Cow::Borrowed("12:1-12:5 -> lib_renamed"),
                },
            ]
        ));
    }

    #[test]
    fn rename_preview_raw_rows_materialize_labels_for_manual_preview_state() {
        let edits = vec![edit("src/lib.rs", 12, "lib_renamed")];
        let rows = vec![
            LspRenamePreviewRow::Header {
                path: PathBuf::from("src/lib.rs"),
            },
            LspRenamePreviewRow::Edit { edit_index: 0 },
        ];

        assert_eq!(
            lsp_rename_preview_visible_display_rows(&rows, &edits, 0..rows.len()),
            vec![
                LspRenamePreviewDisplayRow::Header {
                    label: Cow::Borrowed("lib.rs"),
                },
                LspRenamePreviewDisplayRow::Edit {
                    label: Cow::Borrowed("12:1-12:5 -> lib_renamed"),
                },
            ]
        );
    }

    #[test]
    fn rename_preview_visible_display_rows_skip_stale_edit_indices() {
        let edits = vec![edit("src/lib.rs", 12, "lib_renamed")];
        let rows = vec![
            LspRenamePreviewRow::CachedEdit {
                edit_index: 9,
                edit: edit("src/stale.rs", 1, "stale"),
                label: "stale cached edit".to_owned(),
            },
            LspRenamePreviewRow::Edit { edit_index: 9 },
            LspRenamePreviewRow::CachedHeader {
                path: PathBuf::from("src/lib.rs"),
                label: "lib.rs".to_owned(),
            },
            LspRenamePreviewRow::Edit { edit_index: 0 },
        ];

        assert_eq!(
            lsp_rename_preview_visible_display_rows(&rows, &edits, 0..99),
            vec![
                LspRenamePreviewDisplayRow::Header {
                    label: Cow::Borrowed("lib.rs"),
                },
                LspRenamePreviewDisplayRow::Edit {
                    label: Cow::Borrowed("12:1-12:5 -> lib_renamed"),
                },
            ]
        );
    }

    #[test]
    fn rename_preview_visible_display_rows_clamps_reversed_and_oversized_ranges() {
        let edits = vec![edit("src/lib.rs", 12, "lib_renamed")];
        let rows = lsp_rename_preview_rows(&edits);
        let reversed_start = 2usize;
        let reversed_end = 1usize;

        assert_eq!(
            lsp_rename_preview_visible_display_rows(&rows, &edits, reversed_start..reversed_end),
            Vec::new()
        );
        assert_eq!(
            lsp_rename_preview_visible_display_rows(&rows, &edits, usize::MAX..usize::MAX),
            Vec::new()
        );
        assert_eq!(
            lsp_rename_preview_visible_display_rows(&rows, &edits, 1..usize::MAX),
            vec![LspRenamePreviewDisplayRow::Edit {
                label: Cow::Borrowed("12:1-12:5 -> lib_renamed"),
            }]
        );
    }

    #[test]
    fn rename_preview_visible_display_rows_obeys_hard_row_cap() {
        let edit_count = LSP_RENAME_PREVIEW_MAX_ROWS + 1;
        let edits = (0..edit_count)
            .map(|idx| edit("src/lib.rs", idx + 1, "renamed"))
            .collect::<Vec<_>>();
        let rows = (0..edit_count)
            .map(|idx| LspRenamePreviewRow::Edit { edit_index: idx })
            .collect::<Vec<_>>();

        assert_eq!(
            lsp_rename_preview_visible_display_rows(
                &rows,
                &edits,
                LSP_RENAME_PREVIEW_MAX_ROWS..edit_count,
            ),
            Vec::new()
        );
    }

    #[test]
    fn rename_preview_visible_display_rows_skip_cached_rows_with_stale_identities() {
        let original_edits = vec![edit("src/lib.rs", 12, "lib_renamed")];
        let rows = lsp_rename_preview_rows(&original_edits);
        let current_edits = vec![edit("src/other.rs", 12, "other_renamed")];

        assert_eq!(
            lsp_rename_preview_visible_display_rows(&rows, &current_edits, 0..rows.len()),
            Vec::new()
        );
    }

    #[test]
    fn rename_preview_path_labels_are_sanitized_and_bounded() {
        let label = lsp_rename_preview_path_label(Path::new(
            "workspace/src/abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ\n.rs",
        ));
        let long_path = format!("workspace/src/{}", "a".repeat(80));

        assert_eq!(
            label,
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ .rs"
        );
        let long_label = lsp_rename_preview_path_label(Path::new(&long_path));
        assert!(long_label.contains("..."));
        assert!(long_label.chars().count() <= LSP_RENAME_PREVIEW_PATH_LABEL_CHARS);
    }

    #[test]
    fn rename_preview_rejects_edits_outside_workspace() {
        let root = PathBuf::from("workspace");
        let edits = vec![
            edit("workspace/src/lib.rs", 1, "renamed"),
            edit("outside/src/lib.rs", 1, "renamed"),
        ];

        let reason = lsp_rename_preview_reject_reason(&root, &edits)
            .expect("outside edit should reject preview");

        assert!(reason.contains("edit outside workspace"));
        assert!(reason.contains("outside"));
    }

    #[test]
    fn rename_preview_rejects_zero_or_reversed_edit_ranges() {
        let root = PathBuf::from("workspace");
        let mut invalid_zero = edit("workspace/src/lib.rs", 1, "renamed");
        invalid_zero.start_column = 0;
        let mut invalid_reversed = edit("workspace/src/lib.rs", 3, "renamed");
        invalid_reversed.end_line = 2;
        let valid = edit("workspace/src/lib.rs", 3, "renamed");

        assert!(!lsp_rename_preview_edit_range_is_valid(&invalid_zero));
        assert!(!lsp_rename_preview_edit_range_is_valid(&invalid_reversed));
        assert!(lsp_rename_preview_edit_range_is_valid(&valid));

        let zero_reason = lsp_rename_preview_reject_reason(&root, &[invalid_zero])
            .expect("zero coordinate should reject preview");
        let reversed_reason = lsp_rename_preview_reject_reason(&root, &[invalid_reversed])
            .expect("reversed range should reject preview");

        assert!(zero_reason.starts_with("invalid edit range: lib.rs:1:0-1:5"));
        assert!(reversed_reason.starts_with("invalid edit range: lib.rs:3:1-2:5"));
    }

    #[test]
    fn rename_preview_rejects_oversized_edit_sets_before_building_rows() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/lib.rs");
        let mut app = app_for_test(root);
        let edits = (0..=LSP_RENAME_PREVIEW_MAX_EDITS)
            .map(|idx| LspTextEdit {
                path: path.clone(),
                start_line: idx + 1,
                start_column: 1,
                end_line: idx + 1,
                end_column: 5,
                new_text: "renamed".to_owned(),
            })
            .collect::<Vec<_>>();

        app.open_lsp_rename_preview("renamed".to_owned(), edits);

        assert!(!app.lsp_rename_preview_open);
        assert!(app.lsp_rename_preview_edits.is_empty());
        assert!(app.lsp_rename_preview_rows.is_empty());
        assert_eq!(
            app.status,
            format!(
                "Rename rejected: too many edits ({}, max {LSP_RENAME_PREVIEW_MAX_EDITS})",
                LSP_RENAME_PREVIEW_MAX_EDITS + 1
            )
        );
    }

    #[test]
    fn rename_preview_rejects_oversized_file_sets_before_building_rows() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        let edits = (0..=LSP_RENAME_PREVIEW_MAX_FILES)
            .map(|idx| LspTextEdit {
                path: root.join(format!("src/file-{idx}.rs")),
                start_line: 1,
                start_column: 1,
                end_line: 1,
                end_column: 5,
                new_text: "renamed".to_owned(),
            })
            .collect::<Vec<_>>();

        app.open_lsp_rename_preview("renamed".to_owned(), edits);

        assert!(!app.lsp_rename_preview_open);
        assert!(app.lsp_rename_preview_edits.is_empty());
        assert!(app.lsp_rename_preview_rows.is_empty());
        assert_eq!(
            app.status,
            format!(
                "Rename rejected: too many files ({}, max {LSP_RENAME_PREVIEW_MAX_FILES})",
                LSP_RENAME_PREVIEW_MAX_FILES + 1
            )
        );
    }

    #[test]
    fn rename_preview_path_guard_rejects_parent_escape_segments() {
        assert!(path_is_within_workspace(
            Path::new("workspace"),
            Path::new("workspace/src/lib.rs")
        ));
        assert!(path_is_within_workspace(
            Path::new("workspace/current"),
            Path::new("workspace/current/src/../lib.rs")
        ));
        assert!(!path_is_within_workspace(
            Path::new("workspace"),
            Path::new("workspace/../outside/lib.rs")
        ));
        assert!(!path_is_within_workspace(
            Path::new("workspace"),
            Path::new("workspace-old/src/lib.rs")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn rename_preview_path_guard_matches_windows_paths_case_insensitively() {
        assert!(path_is_within_workspace(
            Path::new(r"C:\Repo\Project"),
            Path::new(r"c:\repo\project\src\main.rs")
        ));
    }

    #[test]
    fn rename_preview_marks_opened_or_changed_paths_stale() {
        assert!(!lsp_rename_preview_path_is_stale(3, Some(3)));
        assert!(lsp_rename_preview_path_is_stale(3, Some(4)));
        assert!(lsp_rename_preview_path_is_stale(3, None));
        assert!(!lsp_rename_preview_path_is_stale(u64::MAX, None));
        assert!(lsp_rename_preview_path_is_stale(u64::MAX, Some(1)));
    }

    #[test]
    fn rename_preview_captures_open_buffer_version_for_equivalent_edit_path() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let edit_path = root.join("src").join("..").join("src").join("main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path),
            "fn main() {}\n".to_owned(),
        ));
        let version = app.buffer(7).expect("buffer").version();

        app.open_lsp_rename_preview(
            "renamed".to_owned(),
            vec![LspTextEdit {
                path: edit_path.clone(),
                start_line: 1,
                start_column: 4,
                end_line: 1,
                end_column: 8,
                new_text: "renamed".to_owned(),
            }],
        );

        assert_eq!(
            app.lsp_rename_preview_versions.get(&edit_path),
            Some(&version)
        );
        assert_ne!(
            app.lsp_rename_preview_versions.get(&edit_path),
            Some(&LSP_RENAME_PREVIEW_UNOPENED_VERSION)
        );
    }

    #[test]
    fn rename_preview_clears_when_equivalent_open_buffer_changes() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let edit_path = root.join("src").join("..").join("src").join("main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path),
            "fn main() {}\n".to_owned(),
        ));
        app.open_lsp_rename_preview(
            "renamed".to_owned(),
            vec![LspTextEdit {
                path: edit_path,
                start_line: 1,
                start_column: 4,
                end_line: 1,
                end_column: 8,
                new_text: "renamed".to_owned(),
            }],
        );

        app.mark_buffer_changed(7);

        assert!(!app.lsp_rename_preview_open);
        assert!(app.lsp_rename_preview_edits.is_empty());
        assert!(app.lsp_rename_preview_versions.is_empty());
    }

    #[test]
    fn rename_preview_apply_ignores_closed_preview_state() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let preview_edit = LspTextEdit {
            path,
            start_line: 1,
            start_column: 4,
            end_line: 1,
            end_column: 8,
            new_text: "renamed".to_owned(),
        };
        let mut app = app_for_test(root);
        app.lsp_rename_preview_open = false;
        app.lsp_rename_preview_new_name = "renamed".to_owned();
        app.lsp_rename_preview_edits = vec![preview_edit.clone()];
        app.lsp_rename_preview_rows = lsp_rename_preview_rows(&app.lsp_rename_preview_edits);
        app.status = "unchanged".to_owned();

        app.apply_lsp_rename_preview();

        assert!(!app.lsp_rename_preview_open);
        assert_eq!(app.lsp_rename_preview_edits, vec![preview_edit]);
        assert_eq!(app.status, "unchanged");
    }

    #[test]
    fn rename_preview_apply_closes_empty_open_preview() {
        let mut app = app_for_test(PathBuf::from("workspace"));
        app.lsp_rename_preview_open = true;
        app.status = "unchanged".to_owned();

        app.apply_lsp_rename_preview();

        assert!(!app.lsp_rename_preview_open);
        assert_eq!(app.status, "Rename preview is empty");
    }

    #[test]
    fn rename_preview_apply_rechecks_workspace_paths_before_applying() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.open_lsp_rename_preview(
            "renamed".to_owned(),
            vec![LspTextEdit {
                path,
                start_line: 1,
                start_column: 4,
                end_line: 1,
                end_column: 8,
                new_text: "renamed".to_owned(),
            }],
        );
        app.lsp_rename_preview_edits[0].path = PathBuf::from("outside/main.rs");

        app.apply_lsp_rename_preview();

        assert!(app.lsp_rename_preview_open);
        assert_eq!(
            app.lsp_rename_preview_edits[0].path,
            PathBuf::from("outside/main.rs")
        );
        assert!(
            app.status
                .starts_with("Rename rejected: edit outside workspace")
        );
    }

    #[test]
    fn rename_preview_apply_preserves_preview_when_cached_rows_are_stale() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.open_lsp_rename_preview(
            "renamed".to_owned(),
            vec![LspTextEdit {
                path,
                start_line: 1,
                start_column: 4,
                end_line: 1,
                end_column: 8,
                new_text: "renamed".to_owned(),
            }],
        );
        app.lsp_rename_preview_edits[0].new_text = "different".to_owned();

        app.apply_lsp_rename_preview();

        assert!(app.lsp_rename_preview_open);
        assert_eq!(app.lsp_rename_preview_edits[0].new_text, "different");
        assert_eq!(app.status, "Rename preview is stale");
    }

    #[test]
    fn rename_preview_apply_preserves_stale_preview_for_retry_or_cancel() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = app_for_test(root);
        app.open_lsp_rename_preview(
            "renamed".to_owned(),
            vec![LspTextEdit {
                path: path.clone(),
                start_line: 1,
                start_column: 4,
                end_line: 1,
                end_column: 8,
                new_text: "renamed".to_owned(),
            }],
        );
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path),
            "fn main() {}\n".to_owned(),
        ));

        app.apply_lsp_rename_preview();

        assert!(app.lsp_rename_preview_open);
        assert_eq!(app.lsp_rename_preview_edits.len(), 1);
        assert_eq!(app.status, "Rename preview is stale for main.rs");
    }

    fn edit(path: &str, line: usize, new_text: &str) -> LspTextEdit {
        LspTextEdit {
            path: PathBuf::from(path),
            start_line: line,
            start_column: 1,
            end_line: line,
            end_column: 5,
            new_text: new_text.to_owned(),
        }
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }
}

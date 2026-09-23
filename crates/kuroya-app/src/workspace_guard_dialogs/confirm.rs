use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button, popup_button_enabled},
    save_lifecycle::{dirty_buffer_ids, workspace_switch_save_block_reason},
    ui_text::count_label,
    workspace_guard_runtime::{
        workspace_guard_display_path, workspace_guard_status_message,
        workspace_guard_unsaved_changes_phrase,
    },
};
use eframe::egui::{self, Align, Context, Key, RichText};
use kuroya_core::{BufferId, TextBuffer};
use std::{collections::HashSet, path::PathBuf};

pub(super) fn render_workspace_switch_confirm_guard(
    app: &mut KuroyaApp,
    ctx: &Context,
    target: PathBuf,
) {
    let dirty = dirty_buffer_ids(&app.buffers);
    let dirty_count = dirty.len();
    let changed_on_disk = app.observed_external_change_buffer_ids();
    let changed_on_disk_count =
        workspace_switch_changed_on_disk_buffer_count(&app.buffers, &changed_on_disk);
    if dirty_count == 0 && changed_on_disk_count == 0 {
        app.open_workspace_now(target);
        return;
    }

    let save_block = (dirty_count > 0)
        .then(|| {
            workspace_switch_save_block_reason(
                &dirty,
                &app.buffers,
                &changed_on_disk,
                &app.lossy_decoded_buffers,
                &app.binary_preview_buffers,
            )
        })
        .flatten()
        .map(|reason| workspace_guard_status_message(&reason));
    let can_save = dirty_count > 0 && save_block.is_none();
    let mut save = false;
    let mut discard = false;
    let mut cancel = false;

    egui::Window::new(workspace_switch_confirm_window_title(
        dirty_count,
        changed_on_disk_count,
    ))
    .collapsible(false)
    .resizable(false)
    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
    .max_size(crate::layout::popup_window_max_size(ctx))
    .fixed_size([520.0, 164.0])
    .show(ctx, |ui| {
        ui.label(RichText::new(format!("Open {}", workspace_guard_display_path(&target))).strong());
        ui.label(workspace_switch_confirm_summary(
            dirty_count,
            changed_on_disk_count,
        ));
        if let Some(reason) = &save_block {
            ui.label(
                RichText::new(reason)
                    .small()
                    .color(ui.visuals().warn_fg_color),
            );
        }

        if ui.input(|input| input.key_pressed(Key::Escape)) {
            cancel = true;
        }

        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                cancel = true;
            }
            if popup_button(
                ui,
                workspace_switch_discard_button_label(dirty_count),
                PopupButtonKind::Danger,
            )
            .clicked()
            {
                discard = true;
            }
            if dirty_count > 0 {
                if popup_button_enabled(
                    ui,
                    can_save,
                    workspace_switch_save_button_label(dirty_count),
                    PopupButtonKind::Primary,
                )
                .clicked()
                {
                    save = true;
                }
            }
        });
    });

    if cancel {
        app.pending_workspace_switch = None;
        app.status = "Workspace switch canceled".to_owned();
    } else if discard {
        app.pending_workspace_switch = None;
        app.open_workspace_now(target);
    } else if save {
        app.start_pending_workspace_switch_save(target);
    }
}

fn workspace_switch_confirm_summary(dirty_count: usize, changed_on_disk_count: usize) -> String {
    if dirty_count == 0 {
        workspace_switch_changed_on_disk_summary(changed_on_disk_count)
    } else {
        workspace_switch_unsaved_changes_summary(dirty_count)
    }
}

fn workspace_switch_unsaved_changes_summary(dirty_count: usize) -> String {
    format!(
        "{} before switching.",
        workspace_guard_unsaved_changes_phrase(dirty_count)
    )
}

fn workspace_switch_changed_on_disk_summary(changed_on_disk_count: usize) -> String {
    format!(
        "{} changed on disk before switching.",
        count_label(changed_on_disk_count, "file", "files")
    )
}

fn workspace_switch_save_button_label(dirty_count: usize) -> &'static str {
    if dirty_count == 1 {
        "Save and Open"
    } else {
        "Save All and Open"
    }
}

fn workspace_switch_discard_button_label(dirty_count: usize) -> &'static str {
    if dirty_count == 0 {
        "Open Anyway"
    } else {
        "Discard and Open"
    }
}

fn workspace_switch_confirm_window_title(
    dirty_count: usize,
    changed_on_disk_count: usize,
) -> &'static str {
    if dirty_count == 0 && changed_on_disk_count > 0 {
        "Changed Files Before Workspace Switch"
    } else {
        "Unsaved Changes Before Workspace Switch"
    }
}

fn workspace_switch_changed_on_disk_buffer_count(
    buffers: &[TextBuffer],
    changed_on_disk: &HashSet<BufferId>,
) -> usize {
    if changed_on_disk.is_empty() {
        return 0;
    }

    let mut seen = HashSet::with_capacity(changed_on_disk.len());
    buffers
        .iter()
        .map(TextBuffer::id)
        .filter(|id| changed_on_disk.contains(id) && seen.insert(*id))
        .count()
}

#[cfg(test)]
mod tests {
    use super::{
        workspace_switch_changed_on_disk_buffer_count, workspace_switch_confirm_summary,
        workspace_switch_confirm_window_title, workspace_switch_discard_button_label,
        workspace_switch_save_button_label, workspace_switch_unsaved_changes_summary,
    };
    use kuroya_core::TextBuffer;
    use std::{collections::HashSet, path::PathBuf};

    #[test]
    fn workspace_switch_confirm_copy_uses_file_count_labels() {
        assert_eq!(
            workspace_switch_confirm_summary(1, 0),
            "1 file has unsaved changes before switching."
        );
        assert_eq!(
            workspace_switch_unsaved_changes_summary(1),
            "1 file has unsaved changes before switching."
        );
        assert_eq!(
            workspace_switch_unsaved_changes_summary(2),
            "2 files have unsaved changes before switching."
        );
        assert_eq!(workspace_switch_save_button_label(1), "Save and Open");
        assert_eq!(workspace_switch_save_button_label(2), "Save All and Open");
        assert_eq!(workspace_switch_discard_button_label(1), "Discard and Open");
        assert_eq!(
            workspace_switch_confirm_window_title(1, 0),
            "Unsaved Changes Before Workspace Switch"
        );
    }

    #[test]
    fn workspace_switch_confirm_copy_handles_clean_changed_on_disk_files() {
        assert_eq!(
            workspace_switch_confirm_summary(0, 1),
            "1 file changed on disk before switching."
        );
        assert_eq!(
            workspace_switch_confirm_summary(0, 2),
            "2 files changed on disk before switching."
        );
        assert_eq!(workspace_switch_discard_button_label(0), "Open Anyway");
        assert_eq!(
            workspace_switch_confirm_window_title(0, 1),
            "Changed Files Before Workspace Switch"
        );
    }

    #[test]
    fn workspace_switch_changed_on_disk_buffer_count_dedupes_open_buffers() {
        let first = TextBuffer::from_text(7, Some(PathBuf::from("a.rs")), "a".to_owned());
        let duplicate = TextBuffer::from_text(7, Some(PathBuf::from("b.rs")), "b".to_owned());
        let second = TextBuffer::from_text(8, Some(PathBuf::from("c.rs")), "c".to_owned());
        let unchanged = TextBuffer::from_text(9, Some(PathBuf::from("d.rs")), "d".to_owned());

        assert_eq!(
            workspace_switch_changed_on_disk_buffer_count(
                &[first, duplicate, second, unchanged],
                &HashSet::from([7, 8, 10]),
            ),
            2
        );
    }
}

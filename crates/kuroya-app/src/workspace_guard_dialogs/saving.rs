use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button},
    save_lifecycle::has_active_save_work,
    transient_state::PendingWorkspaceSwitch,
    workspace_guard_runtime::workspace_guard_display_path,
};
use eframe::egui::{self, Align, Context, Key, RichText};
use kuroya_core::{BufferId, TextBuffer};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

pub(super) fn render_workspace_switch_saving_guard(app: &mut KuroyaApp, ctx: &Context) {
    app.advance_pending_workspace_switch_after_save();
    let Some(PendingWorkspaceSwitch::Saving { target, ids }) =
        app.pending_workspace_switch.as_ref()
    else {
        return;
    };

    let target_label = workspace_guard_display_path(target);
    let remaining = workspace_switch_remaining_save_count(
        ids,
        &app.buffers,
        &app.in_flight_saves,
        &app.queued_save_paths,
        &app.pending_format_on_save,
    );
    let mut cancel = false;

    egui::Window::new(workspace_switch_saving_window_title())
        .max_size(crate::layout::popup_window_max_size_with_top_margin(
            ctx, 24.0,
        ))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([520.0, 132.0])
        .show(ctx, |ui| {
            ui.label(RichText::new(format!("Opening {target_label}")).strong());
            ui.label(workspace_switch_saving_body(remaining));

            if ui.input(|input| input.key_pressed(Key::Escape)) {
                cancel = true;
            }

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                    cancel = true;
                }
            });
        });

    if cancel {
        app.pending_workspace_switch = None;
        app.status = "Workspace switch canceled; in-flight saves will still finish".to_owned();
    }
}

fn workspace_switch_saving_body(remaining: usize) -> String {
    let noun = if remaining == 1 { "file" } else { "files" };
    format!("Saving {remaining} {noun} before switching.")
}

fn workspace_switch_remaining_save_count<T>(
    ids: &[BufferId],
    buffers: &[TextBuffer],
    in_flight_saves: &HashSet<BufferId>,
    queued_save_paths: &HashMap<BufferId, PathBuf>,
    pending_format_on_save: &HashMap<BufferId, T>,
) -> usize {
    if ids.is_empty() {
        return 0;
    }

    let pending_ids = workspace_switch_pending_buffer_ids(ids);
    let mut active_ids = HashSet::with_capacity(pending_ids.len());
    for &id in &pending_ids {
        if has_active_save_work(
            id,
            in_flight_saves,
            queued_save_paths,
            pending_format_on_save,
        ) {
            active_ids.insert(id);
        }
    }

    let active = active_ids.len();
    let dirty_without_active = buffers
        .iter()
        .filter(|buffer| {
            let id = buffer.id();
            buffer.is_dirty() && pending_ids.contains(&id) && !active_ids.contains(&id)
        })
        .count();
    active.saturating_add(dirty_without_active)
}

fn workspace_switch_pending_buffer_ids(ids: &[BufferId]) -> HashSet<BufferId> {
    let mut pending_ids = HashSet::with_capacity(ids.len());
    pending_ids.extend(ids.iter().copied());
    pending_ids
}

#[cfg(test)]
fn workspace_switch_pending_active_buffer_ids<T>(
    ids: &[BufferId],
    in_flight_saves: &HashSet<BufferId>,
    queued_save_paths: &HashMap<BufferId, PathBuf>,
    pending_format_on_save: &HashMap<BufferId, T>,
) -> HashSet<BufferId> {
    workspace_switch_pending_buffer_ids(ids)
        .into_iter()
        .filter(|id| {
            has_active_save_work(
                *id,
                in_flight_saves,
                queued_save_paths,
                pending_format_on_save,
            )
        })
        .collect()
}

fn workspace_switch_saving_window_title() -> &'static str {
    "Saving Before Workspace Switch"
}

#[cfg(test)]
mod tests {
    use super::{
        workspace_switch_pending_active_buffer_ids, workspace_switch_pending_buffer_ids,
        workspace_switch_remaining_save_count, workspace_switch_saving_body,
        workspace_switch_saving_window_title,
    };
    use kuroya_core::TextBuffer;
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };

    #[test]
    fn workspace_switch_saving_body_uses_file_count_labels() {
        assert_eq!(
            workspace_switch_saving_body(1),
            "Saving 1 file before switching."
        );
        assert_eq!(
            workspace_switch_saving_body(2),
            "Saving 2 files before switching."
        );
        assert_eq!(
            workspace_switch_saving_window_title(),
            "Saving Before Workspace Switch"
        );
    }

    #[test]
    fn workspace_switch_remaining_save_count_dedupes_pending_ids() {
        let mut dirty =
            TextBuffer::from_text(7, Some(PathBuf::from("src/main.rs")), "dirty".to_owned());
        dirty.mark_dirty();
        let clean =
            TextBuffer::from_text(8, Some(PathBuf::from("src/clean.rs")), "clean".to_owned());
        let buffers = vec![dirty, clean];
        let in_flight_saves = HashSet::from([7]);
        let queued_save_paths = HashMap::new();
        let pending_format_on_save = HashMap::<_, ()>::new();

        let remaining = workspace_switch_remaining_save_count(
            &[7, 7, 8, 9],
            &buffers,
            &in_flight_saves,
            &queued_save_paths,
            &pending_format_on_save,
        );

        assert_eq!(remaining, 1);
    }

    #[test]
    fn workspace_switch_remaining_save_count_reuses_pending_identity_sets() {
        let mut dirty_pending =
            TextBuffer::from_text(1, Some(PathBuf::from("src/dirty.rs")), "dirty".to_owned());
        dirty_pending.mark_dirty();
        let mut dirty_active =
            TextBuffer::from_text(2, Some(PathBuf::from("src/active.rs")), "dirty".to_owned());
        dirty_active.mark_dirty();
        let mut dirty_other =
            TextBuffer::from_text(3, Some(PathBuf::from("src/other.rs")), "dirty".to_owned());
        dirty_other.mark_dirty();
        let buffers = vec![dirty_pending, dirty_active, dirty_other];
        let in_flight_saves = HashSet::from([2]);
        let queued_save_paths = HashMap::from([(4, PathBuf::from("src/queued.rs"))]);
        let pending_format_on_save = HashMap::from([(5, ())]);
        let pending_ids = [1, 1, 2, 2, 4, 5, 6];

        assert_eq!(
            workspace_switch_pending_buffer_ids(&pending_ids),
            HashSet::from([1, 2, 4, 5, 6])
        );
        assert_eq!(
            workspace_switch_pending_active_buffer_ids(
                &pending_ids,
                &in_flight_saves,
                &queued_save_paths,
                &pending_format_on_save,
            ),
            HashSet::from([2, 4, 5])
        );
        assert_eq!(
            workspace_switch_remaining_save_count(
                &pending_ids,
                &buffers,
                &in_flight_saves,
                &queued_save_paths,
                &pending_format_on_save,
            ),
            4
        );
    }
}

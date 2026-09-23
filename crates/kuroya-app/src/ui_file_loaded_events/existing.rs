use super::LoadedFileTargets;
use crate::{
    KuroyaApp,
    path_display::display_path_label_cow,
    plugin_activation_runtime::{
        activate_plugin_language_for_id, append_plugin_language_activation_status,
        plugin_language_activation_id_if_allowed,
    },
    session_state::{
        apply_buffer_history_state, apply_buffer_view_state,
        horizontal_scroll_offset_from_pane_view_state, horizontal_scroll_offset_from_view_state,
        pane_scroll_line_from_view_state,
    },
    workspace_state::should_activate_loaded_file,
};
use kuroya_core::BufferId;
use std::{path::Path, time::Duration};

impl KuroyaApp {
    pub(super) fn apply_existing_loaded_file(
        &mut self,
        path: &Path,
        _elapsed: Duration,
        activate: bool,
        existing_id: BufferId,
        mut targets: LoadedFileTargets,
    ) {
        if targets.pending_jump.is_some() {
            self.pending_scroll_lines.remove(&existing_id);
            self.pending_horizontal_scroll_offsets.remove(&existing_id);
            self.pending_pane_scroll_lines
                .retain(|(_, buffer_id), _| *buffer_id != existing_id);
            self.pending_pane_horizontal_scroll_offsets
                .retain(|(_, buffer_id), _| *buffer_id != existing_id);
        }
        let has_pane_view_states = !targets.pending_pane_view_states.is_empty();
        if targets.pending_jump.is_none()
            && let Some(view_state) = targets.pending_view_state.take()
            && let Some(buffer) = self.buffer_mut(existing_id)
        {
            let scroll_line = apply_buffer_view_state(buffer, &view_state);
            if !has_pane_view_states {
                if targets.pending_panes.len() > 1 {
                    for pane_id in &targets.pending_panes {
                        self.pending_pane_scroll_lines
                            .insert((*pane_id, existing_id), scroll_line);
                    }
                    let horizontal_scroll_offset =
                        horizontal_scroll_offset_from_view_state(&view_state);
                    if horizontal_scroll_offset > 0.0 {
                        for pane_id in &targets.pending_panes {
                            self.pending_pane_horizontal_scroll_offsets
                                .insert((*pane_id, existing_id), horizontal_scroll_offset);
                        }
                    }
                } else {
                    self.pending_scroll_lines.insert(existing_id, scroll_line);
                    let horizontal_scroll_offset =
                        horizontal_scroll_offset_from_view_state(&view_state);
                    if horizontal_scroll_offset > 0.0 {
                        self.pending_horizontal_scroll_offsets
                            .insert(existing_id, horizontal_scroll_offset);
                    }
                }
            }
        }
        if targets.pending_jump.is_none() && has_pane_view_states {
            let pending_pane_scrolls = self
                .buffer(existing_id)
                .map(|buffer| {
                    targets
                        .pending_pane_view_states
                        .iter()
                        .map(|(pane_id, view_state)| {
                            (
                                *pane_id,
                                pane_scroll_line_from_view_state(buffer, view_state),
                                horizontal_scroll_offset_from_pane_view_state(view_state),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            for (pane_id, scroll_line, horizontal_scroll_offset) in pending_pane_scrolls {
                self.pending_pane_scroll_lines
                    .insert((pane_id, existing_id), scroll_line);
                if horizontal_scroll_offset > 0.0 {
                    self.pending_pane_horizontal_scroll_offsets
                        .insert((pane_id, existing_id), horizontal_scroll_offset);
                }
            }
        }
        if let Some(history_state) = targets.pending_history_state.take()
            && let Some(buffer) = self.buffer_mut(existing_id)
        {
            apply_buffer_history_state(buffer, history_state);
        }
        for pane_id in &targets.pending_panes {
            self.assign_buffer_to_pane(*pane_id, existing_id);
        }
        if targets.pending_active {
            let pane_id = self
                .active_pane_holding_buffer(existing_id)
                .or_else(|| self.pane_id_for_buffer(existing_id))
                .or_else(|| targets.pending_panes.first().copied())
                .unwrap_or(self.active_pane);
            self.set_active_buffer_in_pane(pane_id, existing_id);
            self.pending_active_path = None;
        } else if self.active.is_none()
            && self.pending_active_path.is_none()
            && !targets.pending_panes.is_empty()
        {
            self.set_active_buffer_in_pane(targets.pending_panes[0], existing_id);
        } else if should_activate_loaded_file(
            activate,
            targets.pending_jump.is_some(),
            self.active.is_some(),
            self.pending_active_path.is_some(),
        ) {
            self.set_active_buffer(existing_id);
        }
        let plugin_language_id = self.buffer(existing_id).and_then(|buffer| {
            plugin_language_activation_id_if_allowed(
                buffer,
                &self.plugin_languages,
                &self.lossy_decoded_buffers,
                &self.binary_preview_buffers,
            )
        });
        let language_activations = activate_plugin_language_for_id(
            &mut self.plugin_activations,
            &self.plugin_runtimes,
            plugin_language_id.as_deref(),
        );
        if let Some(jump) = targets.pending_jump {
            self.apply_file_jump_with_encoding_and_selection(
                existing_id,
                jump.line,
                jump.column,
                jump.column_encoding,
                jump.selection_length,
            );
            self.status = append_plugin_language_activation_status(
                format!(
                    "Opened {} at {}:{}",
                    display_path_label_cow(path),
                    jump.line,
                    jump.column
                ),
                &language_activations,
            );
        } else {
            self.status = append_plugin_language_activation_status(
                format!("Opened {}", display_path_label_cow(path)),
                &language_activations,
            );
        }
    }
}

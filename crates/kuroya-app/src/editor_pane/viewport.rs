use crate::{
    KuroyaApp,
    buffer_find_panel::buffer_find_extra_top_space,
    editor_focus_runtime::editor_click_drag_sense_for_tab_index,
    editor_pane_actions::PendingEditorPaneActions,
    editor_pane_data::EditorPaneData,
    editor_pane_scroll::{
        editor_inertial_scroll_offsets, editor_middle_click_scroll_enabled,
        editor_middle_click_scroll_offset, record_editor_horizontal_scroll_offset,
        record_editor_scroll_offset, resolve_editor_scroll_offset,
    },
    image_preview::render_image_preview,
    large_file_mode::{LARGE_FILE_MODE_MAX_BYTES, LARGE_FILE_MODE_MAX_LINES},
    minimap::render_minimap,
    workspace_state::PaneId,
};
use eframe::egui::{
    self, Align, Align2, FontFamily, FontId, Key, Layout, PointerButton, Rect, ScrollArea,
    UiBuilder, pos2, vec2,
};
use kuroya_core::settings::clamp_editor_font_size;
use kuroya_core::{
    BufferId, EditorCursorSurroundingLinesStyle, TextBuffer, buffer::CursorPosition,
};
use layout::{
    editor_content_rect_with_padding, editor_horizontal_scrollbar_needed, editor_minimap_visible,
    editor_minimap_width, editor_mouse_wheel_zoom_delta_y, editor_mouse_wheel_zoom_modifier,
    editor_rect_finite, editor_row_width, editor_scroll_source, editor_scrollbar_axes,
    editor_scrollbar_rect_for_axes, editor_vertical_scrollbar_needed, editor_viewport_rects,
    editor_viewport_row_height, editor_visible_rows_for_render, editor_wheel_scroll_multiplier,
    editor_zoomed_font_size, finite_non_negative_or, minimap_decoration_line_sets,
};
#[cfg(test)]
use overview::{
    overview_ruler_border_rect, overview_ruler_cursor_marker_rect, scm_diff_overview_marker_rect,
};
use overview::{overview_ruler_cursor_lines, paint_scm_diff_overview_ruler};
use rows::render_visible_editor_rows;
use std::ops::Range;
use std::{collections::BTreeMap, time::Duration};
use sticky::{
    first_visible_row_from_scroll, paint_sticky_scroll_row, sticky_scroll_lines,
    sticky_scroll_max_visible_line_count,
};

pub(crate) use layout::editor_scroll_row_count;
#[cfg(test)]
use layout::{editor_scrollbar_visibility, editor_scrollbar_visibility_for_axes};
pub(crate) use overview::diff_patch_overview_lines;

mod layout;
mod overview;
mod rows;
mod sticky;

const EDITOR_MIN_FONT_SIZE: f32 = 10.0;
const EDITOR_MAX_FONT_SIZE: f32 = 28.0;
const EDITOR_MINIMAP_WIDTH: f32 = 88.0;
const EDITOR_MINIMAP_MIN_VIEWPORT_WIDTH: f32 = 220.0;
const EDITOR_PLACEHOLDER_MAX_CHARS: usize = 256;
const DIFF_PATCH_OVERVIEW_MAX_SCAN_BYTES: usize = LARGE_FILE_MODE_MAX_BYTES;
const DIFF_PATCH_OVERVIEW_MAX_SCAN_LINES: usize = LARGE_FILE_MODE_MAX_LINES;

impl KuroyaApp {
    pub(super) fn render_editor_pane_viewport(
        &mut self,
        ui: &mut egui::Ui,
        pane_id: PaneId,
        active_id: BufferId,
        buffer_index: usize,
        data: EditorPaneData,
    ) -> PendingEditorPaneActions {
        let scroll_id = ui.make_persistent_id(("editor-scroll", pane_id, active_id));
        let scroll_key = (pane_id, active_id);
        let mut data = data;
        data.row_height = editor_viewport_row_height(data.row_height);
        let base_line_total = data.visible_line_count.max(1);
        self.refresh_editor_selection_clipboard_from_buffer(buffer_index);
        let active_find_match = if self.buffer_find_open {
            self.visible_pane_active_find_match(pane_id, active_id)
        } else {
            active_find_match_for_cursor(&data.cursor_positions, &data.find_matches)
        };
        let pending_scroll_to_line = self
            .pending_pane_scroll_lines
            .remove(&scroll_key)
            .or_else(|| self.pending_scroll_lines.remove(&active_id));
        let pending_horizontal_scroll_offset = self
            .pending_pane_horizontal_scroll_offsets
            .remove(&scroll_key)
            .or_else(|| self.pending_horizontal_scroll_offsets.remove(&active_id));
        let mut pending_actions = PendingEditorPaneActions::default();
        let viewport_size = ui.available_size_before_wrap();
        let viewport_sense = editor_click_drag_sense_for_tab_index(self.settings.tab_index);
        let (viewport_rect, viewport_response) =
            ui.allocate_exact_size(viewport_size, viewport_sense);
        let viewport_id = ui.make_persistent_id(("editor-viewport", pane_id, active_id));
        let viewport_response =
            viewport_response.union(ui.interact(viewport_rect, viewport_id, viewport_sense));
        if viewport_response.clicked() {
            viewport_response.request_focus();
        }
        if viewport_response.clicked()
            || (viewport_response.has_focus() && self.focused_pane != Some(pane_id))
        {
            pending_actions.focus_editor = true;
        }
        let image_preview_open = self.image_preview_buffers.contains_key(&active_id);
        let background_image_active = self.background_image_is_ready();
        if image_preview_open || !background_image_active {
            ui.painter()
                .rect_filled(viewport_rect, 0.0, ui.visuals().code_bg_color);
        }

        if let Some(preview) = self.image_preview_buffers.get_mut(&active_id) {
            render_image_preview(ui, viewport_rect, active_id, preview, data.font_size);
            return pending_actions;
        }

        let buffer = &self.buffers[buffer_index];
        let highlighter = &mut self.highlighter;
        let bracket_overlay_cache = &mut self.editor_bracket_overlay_cache;
        let gpu_row_render_enabled =
            crate::editor_row_render_cache::editor_row_render_cache_enabled(
                self.settings.experimental_gpu_acceleration,
            );
        let gpu_row_theme_revision = gpu_row_render_enabled.then(|| {
            crate::editor_row_render_cache::editor_row_theme_revision(&self.settings.theme.name)
        });
        let mut gpu_row_render_scope = gpu_row_theme_revision.map(|theme_revision| {
            crate::editor_row_render_cache::GpuRowRenderScope {
                cache: &mut self.editor_row_render_cache,
                theme_revision,
            }
        });
        let minimap_width = editor_minimap_width(
            viewport_rect.width(),
            editor_minimap_visible(
                data.show_minimap,
                data.minimap_autohide,
                viewport_response.hovered() || viewport_response.dragged(),
            ),
            data.minimap_size,
            data.minimap_max_column,
            data.minimap_scale,
        );
        let (left_minimap_rect, scroll_rect, right_minimap_rect) =
            editor_viewport_rects(viewport_rect, minimap_width, data.minimap_side);
        let mut minimap_decoration_lines = None;
        let padding_top = self
            .settings
            .padding_top
            .saturating_add(buffer_find_extra_top_space(
                self.buffer_find_open,
                self.settings.find_add_extra_space_on_top,
            ));
        let content_rect = editor_content_rect_with_padding(
            scroll_rect,
            padding_top,
            self.settings.padding_bottom,
            data.row_height,
        );
        let row_width = editor_row_width(
            content_rect.width(),
            data.gutter_width,
            data.char_width,
            self.settings.scroll_beyond_last_column,
            self.settings.reveal_horizontal_right_padding,
        );
        let viewport_height = content_rect.height().max(data.row_height);
        let line_total = editor_scroll_row_count(
            base_line_total,
            viewport_height,
            data.row_height,
            self.settings.scroll_beyond_last_line,
        );
        let vertical_scrollbar_needed =
            editor_vertical_scrollbar_needed(line_total, data.row_height, viewport_height);
        let horizontal_scrollbar_needed =
            editor_horizontal_scrollbar_needed(row_width, content_rect.width());
        let scrollbar_axes = editor_scrollbar_axes(
            self.settings.scrollbar_vertical,
            self.settings.scrollbar_horizontal,
            vertical_scrollbar_needed,
            horizontal_scrollbar_needed,
        );
        let scroll_to_visible_row = pending_scroll_to_line
            .or_else(|| {
                (self.settings.cursor_surrounding_lines_style
                    == EditorCursorSurroundingLinesStyle::All
                    && data.focused)
                    .then(|| data.cursor_positions.last().map(|cursor| cursor.line))
                    .flatten()
            })
            .map(|line_idx| data.visible_row_for_line_idx(line_idx));
        let mut forced_scroll_offset = resolve_editor_scroll_offset(
            scroll_to_visible_row,
            line_total,
            data.row_height,
            viewport_height,
            self.settings.cursor_surrounding_lines,
            self.settings.smooth_scrolling,
            scroll_key,
            &self.editor_scroll_offsets,
            &mut self.editor_scroll_targets,
        );
        let middle_click_scroll_enabled = editor_middle_click_scroll_enabled(
            self.settings.scroll_on_middle_click,
            self.settings.mouse_middle_click_action,
        );
        let mut middle_click_scroll_started = false;
        if middle_click_scroll_enabled
            && viewport_response.middle_clicked()
            && let Some(pos) = viewport_response.interact_pointer_pos()
        {
            self.editor_middle_click_scroll =
                Some(crate::transient_state::EditorMiddleClickScroll {
                    pane_id,
                    buffer_id: active_id,
                    anchor_y: pos.y,
                });
            pending_actions.focus_editor = true;
            middle_click_scroll_started = true;
        }
        if self
            .editor_middle_click_scroll
            .as_ref()
            .is_some_and(|scroll| scroll.pane_id == pane_id && scroll.buffer_id == active_id)
        {
            if !middle_click_scroll_enabled {
                self.editor_middle_click_scroll = None;
            } else {
                let stop_middle_click_scroll = ui.input(|input| {
                    input.key_pressed(Key::Escape)
                        || input.pointer.button_clicked(PointerButton::Primary)
                        || input.pointer.button_clicked(PointerButton::Secondary)
                        || (!middle_click_scroll_started
                            && input.pointer.button_clicked(PointerButton::Middle))
                        || input.pointer.latest_pos().is_none()
                });
                if stop_middle_click_scroll {
                    self.editor_middle_click_scroll = None;
                } else if let Some(pointer_pos) = ui.input(|input| input.pointer.latest_pos()) {
                    let current_offset = self
                        .editor_scroll_offsets
                        .get(&scroll_key)
                        .copied()
                        .unwrap_or_default();
                    if let Some(offset) = editor_middle_click_scroll_offset(
                        self.editor_middle_click_scroll.as_ref(),
                        pane_id,
                        active_id,
                        pointer_pos.y,
                        current_offset,
                        line_total,
                        data.row_height,
                        viewport_height,
                    ) {
                        forced_scroll_offset = Some(offset);
                        ui.ctx().request_repaint_after(Duration::from_millis(16));
                    }
                }
            }
        }
        if self.settings.mouse_wheel_zoom
            && viewport_response.hovered()
            && let Some(font_size) = editor_zoomed_font_size(
                self.settings.font_size,
                editor_mouse_wheel_zoom_delta_y(ui),
            )
        {
            self.settings.font_size = font_size;
            self.settings_panel_draft.font_size = font_size;
            self.fonts_dirty = true;
            ui.ctx().request_repaint();
        }

        if let Some(minimap_rect) = left_minimap_rect {
            let (find_match_lines, cursor_lines) =
                minimap_decoration_lines.get_or_insert_with(|| {
                    minimap_decoration_line_sets(buffer, &data.find_matches, &data.cursor_positions)
                });
            let scroll_offset = self
                .editor_scroll_offsets
                .get(&scroll_key)
                .copied()
                .unwrap_or_default();
            let minimap_jump = ui
                .scope_builder(
                    UiBuilder::new()
                        .max_rect(minimap_rect)
                        .layout(Layout::top_down(Align::Min)),
                    |ui| {
                        ui.set_min_size(minimap_rect.size());
                        ui.set_max_size(minimap_rect.size());
                        render_minimap(
                            ui,
                            buffer,
                            &mut self.minimap_line_length_cache,
                            scroll_offset,
                            viewport_height,
                            data.row_height,
                            data.minimap_max_column,
                            data.minimap_show_slider,
                            data.minimap_scale,
                            data.minimap_render_characters,
                            background_image_active,
                            &data.minimap_section_headers,
                            data.minimap_section_header_font_size,
                            data.minimap_section_header_letter_spacing,
                            &data.diff_lines,
                            data.show_scm_diff_minimap,
                            &data.diagnostics_by_line,
                            find_match_lines,
                            cursor_lines,
                            &data.visible_line_indices,
                            data.visible_line_count,
                        )
                    },
                )
                .inner;
            if let Some(line) = minimap_jump {
                minimap_jump_pending_actions(&mut pending_actions, buffer, line);
            }
        }

        let scroll_output = ui
            .scope_builder(
                UiBuilder::new()
                    .max_rect(content_rect)
                    .layout(Layout::top_down(Align::Min)),
                |ui| {
                    ui.set_min_size(content_rect.size());
                    ui.set_max_size(content_rect.size());
                    let content_style = ui.style().clone();
                    ui.visuals_mut().clip_rect_margin = 0.0;
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    ui.spacing_mut().scroll = layout::editor_scrollbar_style(
                        self.settings.scrollbar_vertical_scrollbar_size,
                        self.settings.scrollbar_horizontal_scrollbar_size,
                        self.settings
                            .scrollbar_ignore_horizontal_scrollbar_in_content_height,
                    );
                    layout::apply_editor_scrollbar_visuals(ui);
                    let zoom_modifier_active = self.settings.mouse_wheel_zoom
                        && ui.input(|input| editor_mouse_wheel_zoom_modifier(input.modifiers));
                    let smooth_scroll_delta = ui.input(|input| input.smooth_scroll_delta);
                    let wheel_scroll_multiplier = editor_wheel_scroll_multiplier(
                        self.settings.mouse_wheel_scroll_sensitivity,
                        self.settings.fast_scroll_sensitivity,
                        ui.input(|input| input.modifiers.alt),
                        zoom_modifier_active,
                        self.settings.scroll_predominant_axis,
                        smooth_scroll_delta,
                    );
                    let mut horizontal_scroll_offset = pending_horizontal_scroll_offset
                        .or_else(|| {
                            self.editor_horizontal_scroll_offsets
                                .get(&scroll_key)
                                .copied()
                        })
                        .filter(|offset| offset.is_finite() && *offset >= 0.0);
                    let content_hovered = ui.input(|input| {
                        input
                            .pointer
                            .hover_pos()
                            .is_some_and(|pos| content_rect.contains(pos))
                    });
                    let current_vertical_offset = self
                        .editor_scroll_offsets
                        .get(&scroll_key)
                        .copied()
                        .unwrap_or_default();
                    let current_horizontal_offset = horizontal_scroll_offset.unwrap_or_default();
                    let inertial_offsets = editor_inertial_scroll_offsets(
                        &mut self.editor_inertial_scrolls,
                        scroll_key,
                        self.settings.inertial_scroll && !zoom_modifier_active,
                        content_hovered,
                        current_vertical_offset,
                        current_horizontal_offset,
                        line_total,
                        data.row_height,
                        viewport_height,
                        row_width,
                        content_rect.width(),
                        smooth_scroll_delta,
                        wheel_scroll_multiplier,
                        ui.input(|input| input.stable_dt),
                    );
                    if inertial_offsets.consumed_wheel || inertial_offsets.active {
                        self.editor_scroll_targets.remove(&scroll_key);
                    }
                    if inertial_offsets.consumed_wheel {
                        forced_scroll_offset = inertial_offsets.vertical;
                        ui.ctx().input_mut(|input| {
                            input.smooth_scroll_delta = egui::Vec2::ZERO;
                        });
                    } else if let Some(offset) = inertial_offsets.vertical {
                        forced_scroll_offset = Some(offset);
                    }
                    if let Some(offset) = inertial_offsets.horizontal {
                        horizontal_scroll_offset = Some(offset);
                    }
                    if inertial_offsets.active {
                        ui.ctx().request_repaint_after(Duration::from_millis(16));
                    }
                    let mut scroll_area = ScrollArea::new([true, true])
                        .id_salt(scroll_id)
                        .scroll_bar_visibility(scrollbar_axes.visibility)
                        .scroll_bar_rect(editor_scrollbar_rect_for_axes(
                            content_rect,
                            scrollbar_axes,
                        ))
                        .scroll_source(editor_scroll_source(self.settings.inertial_scroll))
                        .auto_shrink([false, false])
                        .wheel_scroll_multiplier(wheel_scroll_multiplier)
                        .animated(self.settings.smooth_scrolling);
                    if let Some(offset) = forced_scroll_offset {
                        scroll_area = scroll_area.vertical_scroll_offset(offset);
                    }
                    if let Some(offset) = horizontal_scroll_offset {
                        scroll_area = scroll_area.horizontal_scroll_offset(offset);
                    }
                    scroll_area.show_rows(ui, data.row_height, line_total, |ui, rows| {
                        ui.set_style(content_style.clone());
                        ui.set_min_width(row_width);
                        if let Some(rows) = editor_visible_rows_for_render(rows, line_total) {
                            render_visible_editor_rows(
                                ui,
                                rows,
                                0.0,
                                row_width,
                                buffer,
                                highlighter,
                                bracket_overlay_cache,
                                gpu_row_render_scope.as_mut(),
                                &data,
                                background_image_active,
                                active_find_match,
                                &mut pending_actions,
                            );
                        }
                    })
                },
            )
            .inner;
        let scroll_offset = scroll_output.state.offset.y;
        let horizontal_scroll_offset = scroll_output.state.offset.x;
        record_editor_scroll_offset(
            &mut self.editor_scroll_offsets,
            &mut self.editor_scroll_targets,
            scroll_key,
            scroll_offset,
            data.row_height,
        );
        record_editor_horizontal_scroll_offset(
            &mut self.editor_horizontal_scroll_offsets,
            scroll_key,
            horizontal_scroll_offset,
        );
        let first_visible_row = first_visible_row_from_scroll(scroll_offset, data.row_height);
        if data.sticky_scroll {
            let sticky_scroll_max_line_count = sticky_scroll_max_visible_line_count(
                content_rect.height(),
                data.row_height,
                data.sticky_scroll_max_line_count,
            );
            for (sticky_row_index, sticky_line_idx) in sticky_scroll_lines(
                &data.visible_line_indices,
                data.visible_line_count,
                &data.folding_ranges,
                first_visible_row,
                sticky_scroll_max_line_count,
            )
            .into_iter()
            .enumerate()
            {
                paint_sticky_scroll_row(
                    ui,
                    content_rect,
                    row_width,
                    buffer,
                    highlighter,
                    bracket_overlay_cache,
                    &data,
                    background_image_active,
                    active_find_match,
                    sticky_line_idx,
                    sticky_row_index,
                    horizontal_scroll_offset,
                );
            }
        }

        if let Some(minimap_rect) = right_minimap_rect {
            let (find_match_lines, cursor_lines) =
                minimap_decoration_lines.get_or_insert_with(|| {
                    minimap_decoration_line_sets(buffer, &data.find_matches, &data.cursor_positions)
                });
            let minimap_jump = ui
                .scope_builder(
                    UiBuilder::new()
                        .max_rect(minimap_rect)
                        .layout(Layout::top_down(Align::Min)),
                    |ui| {
                        ui.set_min_size(minimap_rect.size());
                        ui.set_max_size(minimap_rect.size());
                        render_minimap(
                            ui,
                            buffer,
                            &mut self.minimap_line_length_cache,
                            scroll_offset,
                            viewport_height,
                            data.row_height,
                            data.minimap_max_column,
                            data.minimap_show_slider,
                            data.minimap_scale,
                            data.minimap_render_characters,
                            background_image_active,
                            &data.minimap_section_headers,
                            data.minimap_section_header_font_size,
                            data.minimap_section_header_letter_spacing,
                            &data.diff_lines,
                            data.show_scm_diff_minimap,
                            &data.diagnostics_by_line,
                            find_match_lines,
                            cursor_lines,
                            &data.visible_line_indices,
                            data.visible_line_count,
                        )
                    },
                )
                .inner;
            if let Some(line) = minimap_jump {
                minimap_jump_pending_actions(&mut pending_actions, buffer, line);
            }
        }

        let overview_line_count = buffer.len_lines().max(1);
        let overview_cursor_lines = overview_ruler_cursor_lines(
            &data.cursor_positions,
            data.hide_cursor_in_overview_ruler,
            overview_line_count,
        );
        if data.diff_patch_actions && data.diff_render_overview_ruler {
            paint_scm_diff_overview_ruler(
                ui,
                scroll_rect,
                overview_line_count,
                &diff_patch_overview_lines(buffer),
                &overview_cursor_lines,
                data.overview_ruler_border,
                data.overview_ruler_lanes,
            );
        } else if data.show_scm_diff_overview {
            paint_scm_diff_overview_ruler(
                ui,
                scroll_rect,
                overview_line_count,
                &data.diff_lines,
                &overview_cursor_lines,
                data.overview_ruler_border,
                data.overview_ruler_lanes,
            );
        } else if !overview_cursor_lines.is_empty() {
            paint_scm_diff_overview_ruler(
                ui,
                scroll_rect,
                overview_line_count,
                &BTreeMap::new(),
                &overview_cursor_lines,
                data.overview_ruler_border,
                data.overview_ruler_lanes,
            );
        }
        if buffer.len_chars() == 0 {
            paint_editor_placeholder(
                ui,
                content_rect,
                data.gutter_width,
                data.row_height,
                data.font_size,
                &data.placeholder,
            );
        }

        pending_actions
    }
}

fn active_find_match_for_cursor(
    cursor_positions: &[CursorPosition],
    find_matches: &[Range<usize>],
) -> usize {
    let Some(cursor) = cursor_positions.first() else {
        return usize::MAX;
    };
    find_matches
        .iter()
        .position(|range| range.start <= cursor.char_idx && cursor.char_idx < range.end)
        .unwrap_or(usize::MAX)
}

fn minimap_jump_pending_actions(
    pending_actions: &mut PendingEditorPaneActions,
    buffer: &TextBuffer,
    line: usize,
) {
    pending_actions.minimap_jump = Some(line);
    // The cursor-surrounding-lines=All style scrolls back to the cursor line
    // every frame while the pane is focused, so a jump that only moved the
    // viewport would snap back one frame later. Move the cursor to the start
    // of the target line instead, like a text click would, so the follow
    // scroll and the jump agree.
    pending_actions.cursor = Some((buffer.line_column_to_char(line, 0), false, false));
}

fn paint_editor_placeholder(
    ui: &egui::Ui,
    content_rect: Rect,
    gutter_width: f32,
    row_height: f32,
    font_size: f32,
    placeholder: &str,
) {
    if !editor_rect_finite(content_rect) {
        return;
    }

    let font_size = clamp_editor_font_size(font_size, 13.0);
    let gutter_width = finite_non_negative_or(gutter_width, 0.0).min(content_rect.width().max(0.0));
    let row_height = finite_non_negative_or(row_height, font_size);
    let text = editor_placeholder_display_text(
        placeholder,
        content_rect.width().max(0.0),
        gutter_width,
        font_size,
    );
    if text.is_empty() {
        return;
    }

    let x = content_rect.left() + gutter_width + 8.0;
    let y = content_rect.top() + ((row_height - font_size) / 2.0).max(0.0);
    ui.painter().text(
        pos2(x, y),
        Align2::LEFT_TOP,
        text,
        FontId::new(font_size, FontFamily::Monospace),
        ui.visuals().weak_text_color(),
    );
}

fn editor_placeholder_display_text(
    placeholder: &str,
    content_width: f32,
    gutter_width: f32,
    font_size: f32,
) -> String {
    let text = placeholder.trim();
    let content_width = finite_non_negative_or(content_width, 0.0);
    let gutter_width = finite_non_negative_or(gutter_width, 0.0);
    let available_width = (content_width - gutter_width - 16.0).max(0.0);
    let font_size = clamp_editor_font_size(font_size, 13.0);
    let char_width = (font_size * 0.62).max(1.0);
    let capacity =
        ((available_width / char_width).floor() as usize).min(EDITOR_PLACEHOLDER_MAX_CHARS);
    if capacity == 0 {
        return String::new();
    }

    let mut visible = String::new();
    let mut chars = text.chars();
    for _ in 0..capacity {
        let Some(ch) = chars.next() else {
            return visible;
        };
        visible.push(ch);
    }
    if chars.next().is_none() {
        return visible;
    }
    if capacity <= 3 {
        return ".".repeat(capacity);
    }
    visible
        .chars()
        .take(capacity - 3)
        .chain("...".chars())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        DIFF_PATCH_OVERVIEW_MAX_SCAN_BYTES, DIFF_PATCH_OVERVIEW_MAX_SCAN_LINES,
        EDITOR_PLACEHOLDER_MAX_CHARS, active_find_match_for_cursor, diff_patch_overview_lines,
        editor_content_rect_with_padding, editor_minimap_visible, editor_minimap_width,
        editor_placeholder_display_text, editor_row_width, editor_scroll_row_count,
        editor_scroll_source, editor_scrollbar_axes, editor_scrollbar_visibility,
        editor_scrollbar_visibility_for_axes, editor_viewport_rects, editor_viewport_row_height,
        editor_visible_rows_for_render, editor_wheel_scroll_multiplier, editor_zoomed_font_size,
        layout::{
            editor_horizontal_scrollbar_needed, editor_scrollbar_axis_enabled,
            editor_scrollbar_handle_colors, editor_scrollbar_rect_for_axes, editor_scrollbar_style,
            editor_scrollbar_width, editor_vertical_scrollbar_needed,
        },
        minimap_decoration_line_sets, minimap_jump_pending_actions, overview_ruler_border_rect,
        overview_ruler_cursor_lines, overview_ruler_cursor_marker_rect,
        scm_diff_overview_marker_rect,
    };
    use eframe::egui::{Color32, Context, Rect, pos2};
    use egui::scroll_area::ScrollBarVisibility;
    use kuroya_core::{
        EditorMinimapAutohide, EditorMinimapSide, EditorMinimapSize, EditorScrollbarVisibility,
        GitLineChangeKind, MAX_EDITOR_LINE_HEIGHT, TextBuffer, buffer::CursorPosition,
    };
    use std::{collections::HashSet, ops::Range};

    #[test]
    fn minimap_jump_pending_actions_move_cursor_to_target_line_start() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta\ngamma\n".to_owned());
        let mut actions = crate::editor_pane_actions::PendingEditorPaneActions::default();

        minimap_jump_pending_actions(&mut actions, &buffer, 2);

        assert_eq!(actions.minimap_jump, Some(2));
        assert_eq!(actions.cursor, Some((11, false, false)));
    }

    #[test]
    fn applied_minimap_jump_moves_cursor_and_keeps_pending_scroll() {
        let mut app = viewport_app_for_test();
        app.buffers.push(TextBuffer::from_text(
            1,
            None,
            "alpha\nbeta\ngamma\n".to_owned(),
        ));
        app.panes[0].active = Some(1);

        let mut actions = crate::editor_pane_actions::PendingEditorPaneActions::default();
        minimap_jump_pending_actions(&mut actions, app.buffer(1).expect("buffer"), 2);
        app.apply_editor_pane_actions(&Context::default(), 1, 1, actions);

        let buffer = app.buffer(1).expect("buffer");
        let cursor = buffer
            .cursor_positions()
            .first()
            .copied()
            .expect("cursor position");
        assert_eq!(cursor.line, 2);
        assert_eq!(cursor.column, 0);
        assert_eq!(app.pending_scroll_lines.get(&1), Some(&2));
        assert_eq!(app.focused_pane, Some(1));
    }

    #[test]
    fn active_find_match_for_cursor_follows_current_match() {
        let cursors = [CursorPosition {
            line: 0,
            column: 12,
            char_idx: 12,
        }];
        let matches: Vec<Range<usize>> = vec![0..5, 11..16, 22..27];

        assert_eq!(active_find_match_for_cursor(&cursors, &matches), 1);
    }

    #[test]
    fn active_find_match_for_cursor_uses_no_match_sentinel() {
        let cursors = [CursorPosition {
            line: 0,
            column: 6,
            char_idx: 6,
        }];
        let matches: Vec<Range<usize>> = vec![0..5, 11..16];

        assert_eq!(active_find_match_for_cursor(&cursors, &matches), usize::MAX);
        assert_eq!(active_find_match_for_cursor(&[], &matches), usize::MAX);
    }

    #[test]
    fn editor_scroll_row_count_can_allow_last_line_to_scroll_to_top() {
        assert_eq!(editor_scroll_row_count(10, 100.0, 20.0, true), 14);
        assert_eq!(editor_scroll_row_count(10, 100.0, 20.0, false), 10);
        assert_eq!(editor_scroll_row_count(0, 100.0, 20.0, true), 5);
    }

    #[test]
    fn editor_scroll_row_count_rejects_invalid_or_overflowing_geometry() {
        assert_eq!(editor_scroll_row_count(10, f32::NAN, 20.0, true), 10);
        assert_eq!(editor_scroll_row_count(10, 100.0, f32::NAN, true), 10);
        assert_eq!(editor_scroll_row_count(10, 100.0, f32::INFINITY, true), 10);
        assert_eq!(
            editor_scroll_row_count(usize::MAX, f32::MAX, f32::MIN_POSITIVE, true),
            usize::MAX
        );
    }

    #[test]
    fn editor_viewport_row_height_stabilizes_invalid_or_tiny_values() {
        assert_eq!(editor_viewport_row_height(18.5), 18.5);
        assert_eq!(editor_viewport_row_height(f32::NAN), 1.0);
        assert_eq!(editor_viewport_row_height(f32::INFINITY), 1.0);
        assert_eq!(editor_viewport_row_height(0.0), 1.0);
        assert_eq!(editor_viewport_row_height(-4.0), 1.0);
        assert_eq!(editor_viewport_row_height(f32::MIN_POSITIVE), 1.0);
        assert_eq!(editor_viewport_row_height(f32::MAX), MAX_EDITOR_LINE_HEIGHT);
    }

    #[test]
    fn editor_minimap_width_is_disabled_for_narrow_viewports() {
        assert_eq!(
            editor_minimap_width(600.0, false, EditorMinimapSize::Proportional, 120, 1),
            0.0
        );
        assert_eq!(
            editor_minimap_width(160.0, true, EditorMinimapSize::Proportional, 120, 1),
            0.0
        );
        assert_eq!(
            editor_minimap_width(600.0, true, EditorMinimapSize::Proportional, 120, 1),
            88.0
        );
        assert_eq!(
            editor_minimap_width(600.0, true, EditorMinimapSize::Fill, 120, 1),
            168.0
        );
        assert_eq!(
            editor_minimap_width(600.0, true, EditorMinimapSize::Fit, 80, 2),
            68.0
        );
        assert_eq!(
            editor_minimap_width(f32::NAN, true, EditorMinimapSize::Fit, 80, 2),
            0.0
        );
        assert_eq!(
            editor_minimap_width(f32::INFINITY, true, EditorMinimapSize::Fill, 120, 1),
            0.0
        );
        assert_eq!(
            editor_minimap_width(600.0, true, EditorMinimapSize::Fit, usize::MAX, usize::MAX),
            168.0
        );
    }

    #[test]
    fn editor_minimap_visibility_follows_autohide_mode() {
        assert!(editor_minimap_visible(
            true,
            EditorMinimapAutohide::None,
            false
        ));
        assert!(!editor_minimap_visible(
            false,
            EditorMinimapAutohide::None,
            true
        ));
        assert!(!editor_minimap_visible(
            true,
            EditorMinimapAutohide::Mouseover,
            false
        ));
        assert!(editor_minimap_visible(
            true,
            EditorMinimapAutohide::Mouseover,
            true
        ));
        assert!(!editor_minimap_visible(
            true,
            EditorMinimapAutohide::Scroll,
            false
        ));
    }

    #[test]
    fn minimap_decoration_line_sets_drop_stale_buffer_positions() {
        let buffer = TextBuffer::from_text(1, None, "alpha\nbeta\n".to_owned());
        let find_matches = [
            0..5,
            6..10,
            buffer.len_chars()..buffer.len_chars().saturating_add(1),
            usize::MAX..usize::MAX,
        ];
        let cursor_positions = [
            CursorPosition {
                line: 0,
                column: 0,
                char_idx: 0,
            },
            CursorPosition {
                line: 1,
                column: 2,
                char_idx: 8,
            },
            CursorPosition {
                line: buffer.len_lines(),
                column: 0,
                char_idx: buffer.len_chars(),
            },
            CursorPosition {
                line: usize::MAX,
                column: 0,
                char_idx: usize::MAX,
            },
        ];

        let (find_lines, cursor_lines) =
            minimap_decoration_line_sets(&buffer, &find_matches, &cursor_positions);

        assert_eq!(find_lines, HashSet::from([1, 2]));
        assert_eq!(cursor_lines, HashSet::from([1, 2]));
    }

    #[test]
    fn editor_viewport_rects_keep_scroll_area_inside_full_surface() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(500.0, 300.0));
        let (left, scroll, right) = editor_viewport_rects(rect, 88.0, EditorMinimapSide::Right);

        assert!(left.is_none());
        assert_eq!(scroll.left(), 0.0);
        assert_eq!(scroll.right(), 412.0);
        let right = right.unwrap();
        assert_eq!(right.left(), 412.0);
        assert_eq!(right.top(), 0.0);
        assert_eq!(right.bottom(), 300.0);
    }

    #[test]
    fn editor_viewport_rects_dock_left_minimap_like_an_editor_strip() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(500.0, 300.0));
        let (left, scroll, right) = editor_viewport_rects(rect, 88.0, EditorMinimapSide::Left);

        let left = left.unwrap();
        assert_eq!(left.left(), 0.0);
        assert_eq!(left.right(), 88.0);
        assert_eq!(left.top(), 0.0);
        assert_eq!(left.bottom(), 300.0);
        assert_eq!(scroll.left(), 88.0);
        assert_eq!(scroll.right(), 500.0);
        assert!(right.is_none());
    }

    #[test]
    fn editor_viewport_rects_reject_non_finite_minimap_width() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(500.0, 300.0));
        let (left, scroll, right) = editor_viewport_rects(rect, f32::NAN, EditorMinimapSide::Right);

        assert!(left.is_none());
        assert_eq!(scroll, rect);
        assert!(right.is_none());
    }

    #[test]
    fn editor_viewport_rects_clamp_oversized_minimap_width() {
        let rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(110.0, 120.0));
        let (left, scroll, right) = editor_viewport_rects(rect, 500.0, EditorMinimapSide::Right);

        assert!(left.is_none());
        assert_eq!(scroll.left(), 10.0);
        assert_eq!(scroll.right(), 10.0);
        let right = right.unwrap();
        assert_eq!(right.left(), 10.0);
        assert_eq!(right.right(), 110.0);
    }

    #[test]
    fn editor_content_rect_applies_padding_without_collapsing_rows() {
        let rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(410.0, 320.0));

        let padded = editor_content_rect_with_padding(rect, 12, 8, 20.0);

        assert_eq!(padded.left(), 10.0);
        assert_eq!(padded.right(), 410.0);
        assert_eq!(padded.top(), 32.0);
        assert_eq!(padded.bottom(), 312.0);

        let cramped = editor_content_rect_with_padding(rect, 240, 240, 240.0);
        assert!(cramped.height() >= 240.0);

        let invalid_row_height = editor_content_rect_with_padding(rect, 20, 20, f32::NAN);
        assert!(invalid_row_height.height().is_finite());
    }

    #[test]
    fn editor_row_width_accounts_for_scroll_beyond_last_column() {
        assert_eq!(editor_row_width(100.0, 40.0, 8.0, 5, 30), 170.0);
        assert_eq!(editor_row_width(20.0, 40.0, 8.0, 5, 0), 88.0);
        assert_eq!(editor_row_width(100.0, 40.0, 0.0, 5, 30), 130.0);
        assert_eq!(editor_row_width(100.0, 40.0, f32::NAN, 5, 30), 130.0);
        assert_eq!(
            editor_row_width(100.0, 40.0, f32::MAX, usize::MAX, usize::MAX),
            f32::MAX
        );
    }

    #[test]
    fn editor_scrollbar_needed_checks_axis_overflow() {
        assert!(editor_vertical_scrollbar_needed(11, 10.0, 100.0));
        assert!(!editor_vertical_scrollbar_needed(10, 10.0, 100.0));
        assert!(!editor_vertical_scrollbar_needed(11, f32::NAN, 100.0));
        assert!(!editor_vertical_scrollbar_needed(11, 10.0, f32::INFINITY));

        assert!(editor_horizontal_scrollbar_needed(120.0, 100.0));
        assert!(!editor_horizontal_scrollbar_needed(100.0, 100.0));
        assert!(!editor_horizontal_scrollbar_needed(f32::NAN, 100.0));
        assert!(!editor_horizontal_scrollbar_needed(120.0, f32::NAN));
    }

    #[test]
    fn editor_visible_rows_for_render_clamps_huge_ranges() {
        assert_eq!(
            editor_visible_rows_for_render(5..usize::MAX, 10),
            Some(5..10)
        );
        assert_eq!(editor_visible_rows_for_render(0..usize::MAX, 0), Some(0..1));
        assert_eq!(editor_visible_rows_for_render(10..usize::MAX, 10), None);
        assert_eq!(
            editor_visible_rows_for_render(usize::MAX..usize::MAX, 10),
            None
        );
    }

    #[test]
    fn editor_scrollbar_settings_map_to_egui_scroll_area() {
        assert_eq!(
            editor_scrollbar_visibility(EditorScrollbarVisibility::Auto),
            ScrollBarVisibility::VisibleWhenNeeded
        );
        assert_eq!(
            editor_scrollbar_visibility(EditorScrollbarVisibility::Visible),
            ScrollBarVisibility::AlwaysVisible
        );
        assert_eq!(
            editor_scrollbar_visibility(EditorScrollbarVisibility::Hidden),
            ScrollBarVisibility::AlwaysHidden
        );
        assert!(editor_scrollbar_axis_enabled(
            EditorScrollbarVisibility::Auto
        ));
        assert!(editor_scrollbar_axis_enabled(
            EditorScrollbarVisibility::Visible
        ));
        assert!(!editor_scrollbar_axis_enabled(
            EditorScrollbarVisibility::Hidden
        ));

        let axes = editor_scrollbar_axes(
            EditorScrollbarVisibility::Auto,
            EditorScrollbarVisibility::Auto,
            true,
            false,
        );
        assert!(axes.vertical_visible);
        assert!(!axes.horizontal_visible);
        assert_eq!(axes.visibility, ScrollBarVisibility::AlwaysVisible);

        let axes = editor_scrollbar_axes(
            EditorScrollbarVisibility::Hidden,
            EditorScrollbarVisibility::Hidden,
            true,
            true,
        );
        assert!(!axes.vertical_visible);
        assert!(!axes.horizontal_visible);
        assert_eq!(axes.visibility, ScrollBarVisibility::AlwaysHidden);

        let axes = editor_scrollbar_axes(
            EditorScrollbarVisibility::Visible,
            EditorScrollbarVisibility::Hidden,
            false,
            true,
        );
        assert!(axes.vertical_visible);
        assert!(!axes.horizontal_visible);
        assert_eq!(axes.visibility, ScrollBarVisibility::AlwaysVisible);
        let rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(110.0, 220.0));
        let vertical_only_rect = editor_scrollbar_rect_for_axes(rect, axes);
        assert!(vertical_only_rect.left() > rect.right());
        assert_eq!(vertical_only_rect.top(), rect.top());
        assert_eq!(vertical_only_rect.bottom(), rect.bottom());

        let axes = editor_scrollbar_axes(
            EditorScrollbarVisibility::Hidden,
            EditorScrollbarVisibility::Visible,
            true,
            false,
        );
        assert!(!axes.vertical_visible);
        assert!(axes.horizontal_visible);
        assert_eq!(axes.visibility, ScrollBarVisibility::AlwaysVisible);
        let horizontal_only_rect = editor_scrollbar_rect_for_axes(rect, axes);
        assert_eq!(horizontal_only_rect.left(), rect.left());
        assert_eq!(horizontal_only_rect.right(), rect.right());
        assert!(horizontal_only_rect.top() > rect.bottom());

        let axes = editor_scrollbar_axes(
            EditorScrollbarVisibility::Auto,
            EditorScrollbarVisibility::Hidden,
            true,
            true,
        );
        assert!(axes.vertical_visible);
        assert!(!axes.horizontal_visible);
        assert_eq!(axes.visibility, ScrollBarVisibility::AlwaysVisible);

        let axes = editor_scrollbar_axes(
            EditorScrollbarVisibility::Hidden,
            EditorScrollbarVisibility::Auto,
            true,
            true,
        );
        assert!(!axes.vertical_visible);
        assert!(axes.horizontal_visible);
        assert_eq!(axes.visibility, ScrollBarVisibility::AlwaysVisible);

        let axes = editor_scrollbar_axes(
            EditorScrollbarVisibility::Auto,
            EditorScrollbarVisibility::Auto,
            false,
            false,
        );
        assert!(!axes.vertical_visible);
        assert!(!axes.horizontal_visible);
        assert_eq!(axes.visibility, ScrollBarVisibility::AlwaysHidden);
        let hidden_rect = editor_scrollbar_rect_for_axes(rect, axes);
        assert!(hidden_rect.left() > rect.right());
        assert!(hidden_rect.top() > rect.bottom());

        assert_eq!(
            editor_scrollbar_visibility_for_axes(
                EditorScrollbarVisibility::Visible,
                EditorScrollbarVisibility::Visible,
                false,
                false,
            ),
            ScrollBarVisibility::AlwaysVisible
        );
        assert_eq!(
            editor_scrollbar_visibility_for_axes(
                EditorScrollbarVisibility::Visible,
                EditorScrollbarVisibility::Auto,
                false,
                false,
            ),
            ScrollBarVisibility::AlwaysVisible
        );
        assert_eq!(
            editor_scrollbar_visibility_for_axes(
                EditorScrollbarVisibility::Auto,
                EditorScrollbarVisibility::Visible,
                false,
                false,
            ),
            ScrollBarVisibility::AlwaysVisible
        );
        assert_eq!(
            editor_scrollbar_visibility_for_axes(
                EditorScrollbarVisibility::Auto,
                EditorScrollbarVisibility::Auto,
                false,
                false,
            ),
            ScrollBarVisibility::AlwaysHidden
        );
        assert_eq!(editor_scrollbar_width(18, 12), 18.0);
        assert_eq!(editor_scrollbar_width(0, 16), 16.0);
        assert_eq!(editor_scrollbar_width(0, 0), 1.0);
    }

    #[test]
    fn editor_scrollbar_style_is_thin_floating_and_theme_derived() {
        let style = editor_scrollbar_style(8, 8, true);
        assert!(style.floating);
        assert_eq!(style.bar_width, 8.0);
        assert!((style.floating_width - 3.04).abs() < f32::EPSILON);
        assert_eq!(style.floating_allocated_width, 0.0);
        assert!(style.foreground_color);
        assert!(style.dormant_handle_opacity < style.active_handle_opacity);
        assert!(style.active_handle_opacity < style.interact_handle_opacity);
        assert!(style.interact_background_opacity < style.interact_handle_opacity);

        let solid = editor_scrollbar_style(8, 8, false);
        assert!(!solid.floating);
        assert_eq!(solid.bar_width, 8.0);

        let mut visuals = egui::Visuals::dark();
        visuals.code_bg_color = Color32::from_rgb(20, 30, 40);
        visuals.selection.stroke.color = Color32::from_rgb(120, 180, 240);
        visuals.override_text_color = Some(Color32::from_rgb(220, 220, 220));

        let (inactive, hovered, active) = editor_scrollbar_handle_colors(&visuals);

        assert_eq!(inactive, Color32::from_rgb(60, 90, 120));
        assert_eq!(hovered, Color32::from_rgb(82, 123, 164));
        assert_eq!(active, Color32::from_rgb(164, 167, 170));
    }

    #[test]
    fn editor_wheel_scroll_settings_map_to_multiplier_and_zoom() {
        assert_eq!(
            editor_wheel_scroll_multiplier(2.0, 8.0, false, false, false, egui::Vec2::ZERO),
            egui::Vec2::splat(2.0)
        );
        assert_eq!(
            editor_wheel_scroll_multiplier(2.0, 8.0, true, false, false, egui::Vec2::ZERO),
            egui::Vec2::splat(16.0)
        );
        assert_eq!(
            editor_wheel_scroll_multiplier(2.0, 8.0, true, true, true, egui::Vec2::new(3.0, 8.0)),
            egui::Vec2::ZERO
        );
        assert_eq!(
            editor_wheel_scroll_multiplier(
                2.0,
                8.0,
                false,
                false,
                true,
                egui::Vec2::new(12.0, 3.0)
            ),
            egui::Vec2::new(2.0, 0.0)
        );
        assert_eq!(
            editor_wheel_scroll_multiplier(
                2.0,
                8.0,
                false,
                false,
                true,
                egui::Vec2::new(3.0, 12.0)
            ),
            egui::Vec2::new(0.0, 2.0)
        );
        assert_eq!(
            editor_wheel_scroll_multiplier(2.0, 8.0, false, false, true, egui::Vec2::new(4.0, 4.0)),
            egui::Vec2::splat(2.0)
        );
        assert_eq!(
            editor_wheel_scroll_multiplier(
                f32::MAX,
                f32::MAX,
                true,
                false,
                false,
                egui::Vec2::ZERO
            ),
            egui::Vec2::splat(f32::MAX)
        );
        assert_eq!(editor_zoomed_font_size(13.0, 1.0), Some(14.0));
        assert_eq!(editor_zoomed_font_size(13.0, -1.0), Some(12.0));
        assert_eq!(editor_zoomed_font_size(28.0, 1.0), None);
        assert_eq!(editor_zoomed_font_size(10.0, -1.0), None);
        assert_eq!(editor_zoomed_font_size(13.0, 0.0), None);
    }

    #[test]
    fn editor_inertial_scroll_uses_manual_wheel_source() {
        let normal = editor_scroll_source(false);
        assert!(normal.mouse_wheel);
        assert!(normal.scroll_bar);
        assert!(normal.drag);

        let inertial = editor_scroll_source(true);
        assert!(!inertial.mouse_wheel);
        assert!(inertial.scroll_bar);
        assert!(inertial.drag);
    }

    #[test]
    fn scm_diff_overview_marker_rect_tracks_file_position() {
        let rect = Rect::from_min_max(pos2(20.0, 10.0), pos2(120.0, 210.0));

        let first = scm_diff_overview_marker_rect(rect, 101, 1, 3);
        assert_eq!(first.left(), 117.0);
        assert_eq!(first.top(), 10.0);

        let middle = scm_diff_overview_marker_rect(rect, 101, 51, 3);
        assert_eq!(middle.left(), 117.0);
        assert_eq!(middle.top(), 109.0);

        let last = scm_diff_overview_marker_rect(rect, 101, 101, 3);
        assert_eq!(last.left(), 117.0);
        assert_eq!(last.top(), 209.0);
    }

    #[test]
    fn overview_ruler_border_rect_tracks_the_overview_strip_edge() {
        let rect = Rect::from_min_max(pos2(10.0, 20.0), pos2(410.0, 320.0));

        let border = overview_ruler_border_rect(rect, 3);

        assert_eq!(border.left(), 400.0);
        assert_eq!(border.right(), 401.0);
        assert_eq!(border.top(), 20.0);
        assert_eq!(border.bottom(), 320.0);
    }

    #[test]
    fn overview_ruler_cursor_marker_rect_tracks_file_position() {
        let rect = Rect::from_min_max(pos2(20.0, 10.0), pos2(120.0, 210.0));

        let marker = overview_ruler_cursor_marker_rect(rect, 101, 51, 3);

        assert_eq!(marker.left(), 114.5);
        assert_eq!(marker.right(), 116.5);
        assert_eq!(marker.top(), 109.0);
        assert_eq!(marker.bottom(), 111.0);
    }

    #[test]
    fn overview_ruler_rects_follow_lane_count() {
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0));

        let border = overview_ruler_border_rect(rect, 1);
        let cursor = overview_ruler_cursor_marker_rect(rect, 10, 1, 1);
        let diff = scm_diff_overview_marker_rect(rect, 10, 1, 1);

        assert_eq!(border.left(), 96.0);
        assert_eq!(border.right(), 97.0);
        assert_eq!(cursor.left(), 97.5);
        assert_eq!(diff.left(), 97.0);
    }

    #[test]
    fn overview_ruler_rects_reject_non_finite_geometry() {
        let rect = Rect::from_min_max(pos2(f32::NAN, 0.0), pos2(100.0, 100.0));

        assert_eq!(overview_ruler_border_rect(rect, 3).width(), 0.0);
        assert_eq!(
            overview_ruler_cursor_marker_rect(rect, 10, 1, 3).width(),
            0.0
        );
        assert_eq!(scm_diff_overview_marker_rect(rect, 10, 1, 3).width(), 0.0);
    }

    #[test]
    fn editor_placeholder_text_is_trimmed_and_bounded() {
        assert_eq!(
            editor_placeholder_display_text("  Type here  ", 180.0, 20.0, 13.0),
            "Type here"
        );
        assert_eq!(
            editor_placeholder_display_text("Long placeholder text", 48.0, 20.0, 13.0),
            "."
        );
        assert_eq!(editor_placeholder_display_text("Text", 0.0, 0.0, 13.0), "");
        assert_eq!(
            editor_placeholder_display_text("Text", 120.0, 0.0, f32::NAN),
            "Text"
        );
        assert_eq!(
            editor_placeholder_display_text("Text", f32::NAN, 0.0, 13.0),
            ""
        );

        let long = "x".repeat(EDITOR_PLACEHOLDER_MAX_CHARS + 50);
        let bounded = editor_placeholder_display_text(&long, f32::MAX, 0.0, 13.0);
        assert_eq!(bounded.chars().count(), EDITOR_PLACEHOLDER_MAX_CHARS);
        assert!(bounded.ends_with("..."));
    }

    #[test]
    fn overview_ruler_cursor_lines_follow_hide_setting_and_deduplicate() {
        let cursors = [
            CursorPosition {
                line: 2,
                column: 0,
                char_idx: 12,
            },
            CursorPosition {
                line: 0,
                column: 4,
                char_idx: 4,
            },
            CursorPosition {
                line: 2,
                column: 8,
                char_idx: 20,
            },
        ];

        assert_eq!(overview_ruler_cursor_lines(&cursors, false, 3), vec![1, 3]);
        assert!(overview_ruler_cursor_lines(&cursors, true, 3).is_empty());
    }

    #[test]
    fn overview_ruler_cursor_lines_drop_stale_rows() {
        let cursors = [
            CursorPosition {
                line: 1,
                column: 0,
                char_idx: 4,
            },
            CursorPosition {
                line: 8,
                column: 0,
                char_idx: 20,
            },
            CursorPosition {
                line: usize::MAX,
                column: 0,
                char_idx: usize::MAX,
            },
        ];

        assert_eq!(overview_ruler_cursor_lines(&cursors, false, 2), vec![2]);
    }

    #[test]
    fn diff_patch_overview_lines_track_added_and_removed_patch_rows() {
        let buffer = TextBuffer::from_text(
            1,
            None,
            "diff --git a/main.rs b/main.rs\n\
             --- a/main.rs\n\
             +++ b/main.rs\n\
             @@ -1,2 +1,2 @@\n\
              same\n\
             -old\n\
             +new\n\
              tail\n"
                .to_owned(),
        );

        let lines = diff_patch_overview_lines(&buffer);

        assert_eq!(lines.get(&6), Some(&GitLineChangeKind::Deleted));
        assert_eq!(lines.get(&7), Some(&GitLineChangeKind::Added));
        assert!(!lines.contains_key(&2));
        assert!(!lines.contains_key(&3));
    }

    #[test]
    fn diff_patch_overview_lines_skip_buffers_over_line_budget() {
        let text = std::iter::repeat_n("+changed", DIFF_PATCH_OVERVIEW_MAX_SCAN_LINES + 1)
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = TextBuffer::from_text(1, None, text);

        assert!(buffer.len_lines() > DIFF_PATCH_OVERVIEW_MAX_SCAN_LINES);
        assert!(diff_patch_overview_lines(&buffer).is_empty());
    }

    #[test]
    fn diff_patch_overview_lines_skip_buffers_over_byte_budget() {
        let buffer = TextBuffer::from_text(
            1,
            None,
            format!("+{}", "x".repeat(DIFF_PATCH_OVERVIEW_MAX_SCAN_BYTES)),
        );

        assert!(buffer.len_bytes() > DIFF_PATCH_OVERVIEW_MAX_SCAN_BYTES);
        assert_eq!(buffer.len_lines(), 1);
        assert!(diff_patch_overview_lines(&buffer).is_empty());
    }

    fn viewport_app_for_test() -> crate::KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = kuroya_core::EditorSettings::default();
        crate::KuroyaApp::from_startup_context(crate::app_startup_context::AppStartupContext {
            runtime: tokio::runtime::Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: kuroya_core::Workspace::new(std::path::PathBuf::from("workspace")),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: crate::terminal::TerminalPane::new(
                std::path::PathBuf::from("workspace"),
                100,
                12.0,
                1.2,
            ),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![std::path::PathBuf::from("workspace")],
            now: std::time::Instant::now() - std::time::Duration::from_secs(1),
            startup_timings: Vec::new(),
        })
    }
}

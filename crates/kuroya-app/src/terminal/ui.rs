use super::{
    TERMINAL_DEFAULT_DISPLAY_LABEL, TerminalCommandStatus, TerminalPane, terminal_input_id,
    terminal_rename_input_id, terminal_search_input_id,
};
use crate::{
    popup_buttons::{PopupButtonKind, popup_button},
    ui_icons::{IconKind, icon_button, icon_label},
};
#[cfg(test)]
use egui::pos2;
use egui::{
    Align, Align2, Color32, CursorIcon, Event, FontFamily, FontId, ImeEvent, Key, PointerButton,
    Rect, Response, RichText, Sense, Stroke, TextEdit, Vec2, ViewportCommand, vec2,
};
use kuroya_core::{
    Command, CommandBus, DEFAULT_TERMINAL_TABS_TITLE, TerminalMiddleClickBehavior,
    TerminalRightClickBehavior, TerminalTabsLocation,
};
use std::{borrow::Cow, time::Duration};

mod actions;
mod colors;
mod cursor;
mod input;
mod labels;
mod layout;
mod render;
mod status;

use super::search::terminal_visible_search_spans_with_normalized_query;
use actions::{terminal_action_button, terminal_action_button_enabled, terminal_tab_rail_width};
#[cfg(test)]
pub(super) use colors::terminal_ansi_palette_from_colors;
#[cfg(test)]
use colors::terminal_background_color;
use colors::{blend_color, terminal_accent, terminal_background, terminal_muted_text};
#[cfg(test)]
pub(super) use colors::{
    terminal_bold_foreground_color, terminal_contrast_color, terminal_foreground_color,
};
use cursor::{draw_terminal_cursor, terminal_cursor_visible};
pub(crate) use input::terminal_key_input;
use input::{terminal_copy_shortcut, terminal_scroll_key_delta, wheel_scroll_rows};
#[cfg(not(test))]
use labels::terminal_tab_icon_kind;
#[cfg(test)]
use labels::{
    TERMINAL_DISPLAY_LABEL_MAX_CHARS, TERMINAL_DISPLAY_LABEL_MAX_EXACT_UTF8_BYTES,
    compact_terminal_path, terminal_display_label_normalized, terminal_path_tooltip,
    terminal_session_label_for_shell, terminal_tab_ansi_color_index,
};
#[cfg(test)]
pub(super) use labels::{
    parse_terminal_tab_hex_color, terminal_compact_path_for_test, terminal_path_tooltip_for_test,
    terminal_session_label, terminal_tab_icon_kind,
};
use labels::{
    terminal_display_label, terminal_path_label, terminal_session_label_from_display_label,
    terminal_session_sequence_title, terminal_tab_color_from_setting, terminal_template_path,
};
use layout::{
    bounded_terminal_layout_size, bounded_terminal_layout_value, terminal_cell_position_at_pointer,
    terminal_content_rect, terminal_link_click_modifier, terminal_mouse_wheel_zoom_modifier,
    terminal_path_link_scan_allowed, terminal_rect_contains_pointer, terminal_render_grid,
    terminal_safe_cell_size, terminal_safe_font_size, terminal_split_separator_line_rect,
    terminal_split_separator_width,
};
#[cfg(test)]
use render::TerminalRenderBaseColors;
#[cfg(test)]
pub(super) use render::terminal_rendered_text_color;
use render::{
    TerminalRenderColorCache, prepare_terminal_text_runs, terminal_cell_text_is_single_char,
    terminal_selection_contains_cell,
};
pub(super) use render::{
    push_terminal_text_run, terminal_text_runs_can_merge, terminal_word_selection_at_cell,
};
#[cfg(test)]
use status::terminal_command_status_tooltip;
use status::{
    TerminalSessionLabelContext, terminal_command_status_dot, terminal_profile_tab_tooltip,
};

const TERMINAL_CHROME_RADIUS: u8 = 5;

fn terminal_input_hover_text() -> &'static str {
    "Terminal input\nRight-click for terminal actions"
}

#[cfg(test)]
pub(super) fn terminal_text_input_from_event(event: &Event) -> Option<&str> {
    match event {
        Event::Text(text) | Event::Ime(ImeEvent::Commit(text)) if !text.is_empty() => {
            Some(text.as_str())
        }
        _ => None,
    }
}

impl TerminalPane {
    pub fn ui(&mut self, ui: &mut egui::Ui, command_bus: &mut CommandBus) {
        self.prune_stale_session_state();
        let panel_size = bounded_terminal_layout_size(ui.available_size_before_wrap());
        ui.set_min_size(panel_size);
        let panel_rect = ui.min_rect();
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        self.render_header(ui);
        if self.search_open {
            self.render_search_bar(ui);
        }
        let available = bounded_terminal_layout_size(ui.available_size_before_wrap());
        self.render_terminal_area(
            ui,
            bounded_terminal_layout_size(vec2(available.x, available.y.max(64.0))),
            command_bus,
        );
        self.render_paste_notice(ui, panel_rect);
        self.render_multiline_paste_warning(ui.ctx());
        self.render_kill_confirmation(ui.ctx());
        self.render_rename_terminal_dialog(ui.ctx());
    }

    fn render_paste_notice(&mut self, ui: &mut egui::Ui, panel_rect: Rect) {
        self.expire_paste_notice();
        let Some((message, remaining)) = self
            .active_paste_notice()
            .map(|(message, remaining)| (message.to_owned(), remaining))
        else {
            return;
        };
        ui.ctx().request_repaint_after(remaining);

        let text_color = ui.visuals().warn_fg_color;
        let background = blend_color(terminal_background(ui), text_color, 0.08);
        let painter = ui.painter_at(panel_rect);
        let font = FontId::proportional(12.5);
        let galley = painter.layout_no_wrap(message, font, text_color);
        let margin = vec2(10.0, 6.0);
        let size = galley.size() + margin * 2.0;
        let rect = Align2::RIGHT_BOTTOM
            .align_size_within_rect(size, panel_rect.shrink(8.0))
            .intersect(panel_rect);
        painter.rect_filled(rect, 6.0, background);
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, text_color),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.left_top() + margin, galley, text_color);
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        let text_color = ui.visuals().text_color();
        let background = terminal_background(ui);
        let muted_text = terminal_muted_text(ui);
        let chrome_fill = blend_color(background, ui.visuals().widgets.inactive.weak_bg_fill, 0.52);
        let tab_fill = blend_color(background, ui.visuals().widgets.inactive.weak_bg_fill, 0.70);
        let tab_icon_kind = self.terminal_tab_icon_kind();
        let tab_icon_color = self.terminal_tab_icon_color(ui, muted_text);
        let shell_label = self.shell_label().to_owned();
        let label_context = self.terminal_session_label_context(shell_label.as_str());
        let shell_tooltip = label_context.shell_tooltip_label();

        egui::Frame::new()
            .fill(chrome_fill)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_height(34.0);
                ui.horizontal(|ui| {
                    if self.terminal_tabs_rail_location().is_none()
                        && self.terminal_session_tabs_visible()
                    {
                        let session_count = self.sessions.len();
                        for index in 0..session_count {
                            let Some((label, command_status)) =
                                self.sessions.get(index).map(|session| {
                                    (
                                        self.terminal_session_label_with_context(
                                            session,
                                            &label_context,
                                        ),
                                        session.command_status(),
                                    )
                                })
                            else {
                                continue;
                            };
                            let selected = index == self.active_session;
                            let response = self.render_profile_tab(
                                ui,
                                label.as_ref(),
                                shell_tooltip.as_ref(),
                                command_status,
                                selected,
                                tab_fill,
                                text_color,
                                tab_icon_kind,
                                tab_icon_color,
                                190.0,
                            );
                            if response.clicked() || response.double_clicked() {
                                self.activate_session_tab(
                                    index,
                                    response.clicked(),
                                    response.double_clicked(),
                                );
                            } else if response.secondary_clicked() {
                                self.set_active_session_without_focus(index);
                            }
                            response
                                .context_menu(|ui| self.render_terminal_context_menu(ui, index));
                        }
                    } else if self.terminal_active_session_dropdown_visible() {
                        self.render_active_session_dropdown(ui, &label_context);
                    } else if self.terminal_active_info_visible() {
                        self.render_active_terminal_chip(
                            ui,
                            tab_fill,
                            text_color,
                            tab_icon_kind,
                            tab_icon_color,
                            &label_context,
                        );
                    }
                    if terminal_action_button_enabled(
                        ui,
                        self.can_open_session(),
                        IconKind::Plus,
                        "New",
                        "New terminal",
                    )
                    .clicked()
                    {
                        self.open_new_session();
                    }
                    let show_actions = self.terminal_action_buttons_visible();
                    let has_active_session = self.active_session_index().is_some();
                    if show_actions
                        && terminal_action_button_enabled(
                            ui,
                            has_active_session && self.can_open_session(),
                            IconKind::Panes,
                            "Split",
                            "Split terminal",
                        )
                        .clicked()
                    {
                        self.split_active_session();
                    }
                    ui.add_space(8.0);
                    let launch_cwd = self.active_launch_cwd();
                    terminal_path_label(ui, &launch_cwd, muted_text);

                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if terminal_action_button(ui, IconKind::Close, "Hide", "Hide terminal pane")
                            .clicked()
                        {
                            self.set_visible(false);
                        }
                        if show_actions
                            && terminal_action_button_enabled(
                                ui,
                                has_active_session,
                                IconKind::Trash,
                                "Kill",
                                "Close terminal",
                            )
                            .clicked()
                        {
                            self.request_close_active_session();
                        }
                        let fullscreen_icon = if self.fullscreen {
                            IconKind::Restore
                        } else {
                            IconKind::Maximize
                        };
                        let fullscreen_tooltip = if self.fullscreen {
                            "Restore terminal panel"
                        } else {
                            "Maximize terminal"
                        };
                        let fullscreen_label = if self.fullscreen {
                            "Restore"
                        } else {
                            "Maximize"
                        };
                        if terminal_action_button(
                            ui,
                            fullscreen_icon,
                            fullscreen_label,
                            fullscreen_tooltip,
                        )
                        .clicked()
                        {
                            self.toggle_fullscreen();
                        }
                        if show_actions
                            && terminal_action_button_enabled(
                                ui,
                                has_active_session,
                                IconKind::Search,
                                "Find",
                                "Search terminal output",
                            )
                            .clicked()
                        {
                            self.toggle_terminal_search();
                        }
                    });
                });
            });
    }

    fn render_search_bar(&mut self, ui: &mut egui::Ui) {
        let background = terminal_background(ui);
        let accent = terminal_accent(ui);
        let muted_text = terminal_muted_text(ui);
        let fill = blend_color(background, ui.visuals().widgets.inactive.weak_bg_fill, 0.44);
        let mut close_search = false;

        egui::Frame::new()
            .fill(fill)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_height(34.0);
                ui.horizontal(|ui| {
                    icon_label(ui, IconKind::Search, accent, "Terminal search");
                    let search_response = ui.add(
                        TextEdit::singleline(&mut self.search_query)
                            .id(terminal_search_input_id())
                            .desired_width(280.0)
                            .hint_text("Search terminal"),
                    );
                    if self.take_terminal_search_focus_request() {
                        search_response.request_focus();
                    }
                    if search_response.changed() {
                        self.reset_terminal_search_cursor();
                    }
                    if search_response.has_focus() {
                        if ui.input(|input| input.key_pressed(Key::Escape)) {
                            close_search = true;
                        }
                        if ui.input(|input| input.key_pressed(Key::Enter)) {
                            let backwards = ui.input(|input| input.modifiers.shift);
                            self.advance_terminal_search(if backwards { -1 } else { 1 });
                        }
                    }

                    let has_matches = !self.active_terminal_search_matches().is_empty();
                    let mut search_delta = 0;
                    if ui
                        .add_enabled(has_matches, egui::Button::new("Previous"))
                        .clicked()
                    {
                        search_delta = -1;
                    }
                    if ui
                        .add_enabled(has_matches, egui::Button::new("Next"))
                        .clicked()
                    {
                        search_delta = 1;
                    }
                    if search_delta != 0 {
                        self.advance_terminal_search(search_delta);
                    }
                    let match_count = self.search_cache.matches.len();

                    ui.label(
                        RichText::new(self.active_terminal_search_result_label(match_count))
                            .small()
                            .color(muted_text),
                    );
                    let selected_match = self.search_match.min(match_count.saturating_sub(1));
                    let current_preview = self
                        .search_cache
                        .matches
                        .get(selected_match)
                        .map(|current| current.preview.as_str())
                        .and_then(terminal_search_preview_display_label);
                    if let Some(current_preview) = current_preview {
                        let current_preview = current_preview.as_ref();
                        ui.add_space(8.0);
                        ui.label(RichText::new(current_preview).small().color(muted_text))
                            .on_hover_text(current_preview);
                    }

                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if icon_button(ui, IconKind::Close, "Close terminal search").clicked() {
                            close_search = true;
                        }
                    });
                });
            });

        if close_search {
            self.close_terminal_search();
        }
    }

    fn render_profile_tab(
        &self,
        ui: &mut egui::Ui,
        label: &str,
        shell_tooltip: &str,
        command_status: TerminalCommandStatus,
        selected: bool,
        fill: Color32,
        text_color: Color32,
        icon_kind: IconKind,
        icon_color: Color32,
        width: f32,
    ) -> egui::Response {
        let profile_response = ui.allocate_ui_with_layout(
            vec2(width, 32.0),
            egui::Layout::left_to_right(Align::Center),
            |ui| {
                let rect = ui.max_rect().shrink(1.0);
                let tab_fill = if selected {
                    blend_color(fill, text_color, 0.06)
                } else {
                    fill
                };
                ui.painter()
                    .rect_filled(rect, TERMINAL_CHROME_RADIUS, tab_fill);

                ui.add_space(8.0);
                icon_label(ui, icon_kind, icon_color, shell_tooltip);
                terminal_command_status_dot(ui, command_status, text_color);
                ui.label(RichText::new(label).strong().color(text_color));
            },
        );
        profile_response
            .response
            .on_hover_text(terminal_profile_tab_tooltip(command_status))
    }

    fn render_active_terminal_chip(
        &mut self,
        ui: &mut egui::Ui,
        fill: Color32,
        text_color: Color32,
        icon_kind: IconKind,
        icon_color: Color32,
        label_context: &TerminalSessionLabelContext<'_>,
    ) {
        let Some(active) = self.active_session_index() else {
            return;
        };
        let (label, command_status) = self
            .sessions
            .get(active)
            .map(|session| {
                (
                    self.terminal_session_label_with_context(session, label_context),
                    session.command_status(),
                )
            })
            .unwrap_or((Cow::Borrowed("Terminal"), TerminalCommandStatus::Unknown));
        let shell_tooltip = label_context.shell_tooltip_label();
        let response = self.render_profile_tab(
            ui,
            label.as_ref(),
            shell_tooltip.as_ref(),
            command_status,
            true,
            fill,
            text_color,
            icon_kind,
            icon_color,
            190.0,
        );
        if response.clicked() || response.double_clicked() {
            self.activate_session_tab(active, response.clicked(), response.double_clicked());
        } else if response.secondary_clicked() {
            self.set_active_session_without_focus(active);
        }
        response.context_menu(|ui| self.render_terminal_context_menu(ui, active));
    }

    fn render_active_session_dropdown(
        &mut self,
        ui: &mut egui::Ui,
        label_context: &TerminalSessionLabelContext<'_>,
    ) {
        let active_label = self
            .sessions
            .get(self.active_session)
            .map(|session| self.terminal_session_label_with_context(session, label_context))
            .unwrap_or(Cow::Borrowed("Terminal"));

        egui::ComboBox::from_id_salt("terminal_active_session_dropdown")
            .selected_text(active_label.as_ref())
            .width(190.0)
            .show_ui(ui, |ui| {
                let session_count = self.sessions.len();
                for index in 0..session_count {
                    let Some(label) = self.sessions.get(index).map(|session| {
                        self.terminal_session_label_with_context(session, label_context)
                    }) else {
                        continue;
                    };
                    if ui
                        .selectable_label(index == self.active_session, label.as_ref())
                        .clicked()
                    {
                        self.set_active_session(index);
                        ui.close();
                    }
                }
            });
    }

    fn render_terminal_area(
        &mut self,
        ui: &mut egui::Ui,
        desired_size: Vec2,
        command_bus: &mut CommandBus,
    ) {
        let desired_size = bounded_terminal_layout_size(desired_size);
        let Some(location) = self.terminal_tabs_rail_location() else {
            self.render_terminal_screens(ui, desired_size, command_bus);
            return;
        };

        let rail_width = terminal_tab_rail_width(desired_size.x);
        let screen_width = bounded_terminal_layout_value(desired_size.x - rail_width);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            match location {
                TerminalTabsLocation::Top => {
                    self.render_terminal_screens(ui, desired_size, command_bus);
                }
                TerminalTabsLocation::Left => {
                    self.render_terminal_tab_rail(ui, vec2(rail_width, desired_size.y));
                    self.render_terminal_screens(
                        ui,
                        vec2(screen_width, desired_size.y),
                        command_bus,
                    );
                }
                TerminalTabsLocation::Right => {
                    self.render_terminal_screens(
                        ui,
                        vec2(screen_width, desired_size.y),
                        command_bus,
                    );
                    self.render_terminal_tab_rail(ui, vec2(rail_width, desired_size.y));
                }
            }
        });
    }

    fn render_terminal_tab_rail(&mut self, ui: &mut egui::Ui, desired_size: Vec2) {
        let desired_size = bounded_terminal_layout_size(desired_size);
        let text_color = ui.visuals().text_color();
        let background = terminal_background(ui);
        let muted_text = terminal_muted_text(ui);
        let chrome_fill = blend_color(background, ui.visuals().widgets.inactive.weak_bg_fill, 0.52);
        let tab_fill = blend_color(background, ui.visuals().widgets.inactive.weak_bg_fill, 0.70);
        let tab_icon_kind = self.terminal_tab_icon_kind();
        let tab_icon_color = self.terminal_tab_icon_color(ui, muted_text);
        let shell_label = self.shell_label().to_owned();
        let label_context = self.terminal_session_label_context(shell_label.as_str());
        let shell_tooltip = label_context.shell_tooltip_label();

        ui.allocate_ui_with_layout(desired_size, egui::Layout::top_down(Align::Min), |ui| {
            egui::Frame::new()
                .fill(chrome_fill)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .show(ui, |ui| {
                    ui.set_min_size(desired_size);
                    egui::ScrollArea::vertical()
                        .id_salt("terminal_tab_rail")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 4.0);
                            let tab_width = bounded_terminal_layout_value(desired_size.x - 16.0);
                            let session_count = self.sessions.len();
                            for index in 0..session_count {
                                let Some((label, command_status)) =
                                    self.sessions.get(index).map(|session| {
                                        (
                                            self.terminal_session_label_with_context(
                                                session,
                                                &label_context,
                                            ),
                                            session.command_status(),
                                        )
                                    })
                                else {
                                    continue;
                                };
                                let selected = index == self.active_session;
                                let response = self.render_profile_tab(
                                    ui,
                                    label.as_ref(),
                                    shell_tooltip.as_ref(),
                                    command_status,
                                    selected,
                                    tab_fill,
                                    text_color,
                                    tab_icon_kind,
                                    tab_icon_color,
                                    tab_width,
                                );
                                if response.clicked() || response.double_clicked() {
                                    self.activate_session_tab(
                                        index,
                                        response.clicked(),
                                        response.double_clicked(),
                                    );
                                } else if response.secondary_clicked() {
                                    self.set_active_session_without_focus(index);
                                }
                                response.context_menu(|ui| {
                                    self.render_terminal_context_menu(ui, index)
                                });
                            }
                        });
                });
        });
    }

    fn render_terminal_screens(
        &mut self,
        ui: &mut egui::Ui,
        desired_size: Vec2,
        command_bus: &mut CommandBus,
    ) {
        let desired_size = bounded_terminal_layout_size(desired_size);
        if self.split_view && self.sessions.len() > 1 {
            let count = self.sessions.len();
            let separator_width = terminal_split_separator_width(desired_size.x, count);
            let widths = self.split_widths(desired_size.x, separator_width);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for (index, width) in widths.iter().copied().enumerate() {
                    self.render_terminal_screen(
                        ui,
                        index,
                        vec2(width, desired_size.y),
                        command_bus,
                    );
                    if index + 1 < count {
                        self.render_split_separator(ui, index, desired_size.y, separator_width);
                    }
                }
            });
        } else if let Some(active) = self.active_session_index() {
            self.render_terminal_screen(ui, active, desired_size, command_bus);
        }
    }

    fn render_split_separator(
        &mut self,
        ui: &mut egui::Ui,
        left_index: usize,
        height: f32,
        width: f32,
    ) {
        let size = bounded_terminal_layout_size(vec2(width, height));
        if size.x <= 0.0 || size.y <= 0.0 {
            return;
        }

        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let response = response
            .on_hover_cursor(CursorIcon::ResizeHorizontal)
            .on_hover_text("Resize terminal split");
        if response.dragged() {
            let delta_x = ui.input(|input| input.pointer.delta().x);
            self.resize_split_at(left_index, delta_x);
        }

        response.context_menu(|ui| {
            if ui
                .add_enabled(
                    self.can_open_session(),
                    egui::Button::new("Split Terminal Right"),
                )
                .clicked()
            {
                self.set_active_session(left_index);
                self.split_active_session();
                ui.close();
            }
            if ui
                .add_enabled(
                    self.split_view && self.sessions.len() > 1,
                    egui::Button::new("Join Terminals"),
                )
                .clicked()
            {
                self.unsplit_sessions();
                ui.close();
            }
        });

        let hovered_or_dragged = response.hovered() || response.dragged();
        let accent = terminal_accent(ui);
        let line_color = if hovered_or_dragged {
            accent
        } else {
            ui.visuals().widgets.inactive.bg_stroke.color
        };
        if hovered_or_dragged {
            ui.painter().rect_filled(
                rect,
                0.0,
                Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 20),
            );
        }
        if let Some(line_rect) = terminal_split_separator_line_rect(rect) {
            ui.painter().rect_filled(line_rect, 0.0, line_color);
        }
        if hovered_or_dragged {
            for offset in [-6.0, 0.0, 6.0] {
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.center().x - 2.0, rect.center().y + offset),
                        egui::pos2(rect.center().x + 2.0, rect.center().y + offset),
                    ],
                    Stroke::new(1.0, ui.visuals().widgets.hovered.fg_stroke.color),
                );
            }
        }
    }

    fn render_terminal_screen(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        desired_size: Vec2,
        command_bus: &mut CommandBus,
    ) {
        let desired_size = bounded_terminal_layout_size(desired_size);
        let session_id = self.sessions.get(index).map(|session| session.id);
        let (rect, allocated_response) = ui.allocate_exact_size(desired_size, Sense::hover());
        let inner = terminal_content_rect(rect);
        self.resize_session_to_fit(index, inner.width(), inner.height());
        let response = session_id
            .map(|session_id| {
                ui.interact(rect, terminal_input_id(session_id), Sense::click_and_drag())
            })
            .unwrap_or(allocated_response);
        let response = response.on_hover_text(terminal_input_hover_text());
        let font_size = terminal_safe_font_size(self.font_size);
        let (cell_width, cell_height) =
            terminal_safe_cell_size(font_size, self.line_height, self.letter_spacing);
        let alt_primary_click = response.clicked()
            && self.alt_click_moves_cursor
            && ui.input(|input| input.modifiers.alt);
        let link_click =
            response.clicked() && ui.input(|input| terminal_link_click_modifier(input.modifiers));
        let link_under_pointer = session_id.is_some_and(|session_id| {
            response.hovered()
                && ui.input(|input| terminal_link_click_modifier(input.modifiers))
                && self
                    .terminal_path_link_at_pointer(
                        index,
                        session_id,
                        response.hover_pos(),
                        inner,
                        cell_width,
                        cell_height,
                    )
                    .is_some()
        });
        if link_under_pointer {
            ui.output_mut(|output| output.cursor_icon = CursorIcon::PointingHand);
        }
        let pointer_interaction = response.clicked()
            || response.secondary_clicked()
            || response.middle_clicked()
            || response.drag_started_by(PointerButton::Primary)
            || response.dragged_by(PointerButton::Primary)
            || response.drag_stopped_by(PointerButton::Primary);

        if link_click
            && let Some(session_id) = session_id
            && let Some(link) = self.terminal_path_link_at_pointer(
                index,
                session_id,
                response.interact_pointer_pos(),
                inner,
                cell_width,
                cell_height,
            )
        {
            self.set_active_session(index);
            self.selected_session_id = None;
            self.selected_text = None;
            command_bus.push(Command::OpenFileAt {
                path: link.path,
                line: link.line,
                column: link.column,
            });
        } else if alt_primary_click {
            self.set_active_session(index);
            if let Some(position) = self.cell_position_at_pointer(
                index,
                response.interact_pointer_pos(),
                inner,
                cell_width,
                cell_height,
            ) {
                self.send_alt_click_cursor_input(index, position);
            }
        } else if response.clicked() {
            self.set_active_session(index);
            self.selected_session_id = None;
            self.selected_text = None;
        } else if response.secondary_clicked() {
            self.set_active_session(index);
        } else if response.middle_clicked() {
            self.handle_terminal_middle_click(ui, index);
        }
        if pointer_interaction || (self.focus_input_on_show && index == self.active_session) {
            response.request_focus();
            self.focus_input_on_show = false;
        }
        let handles_pending_paste = self
            .sessions
            .get(index)
            .is_some_and(|session| self.pending_paste_session_id == Some(session.id));
        if response.has_focus() || handles_pending_paste {
            self.handle_terminal_input(
                ui,
                index,
                &response,
                handles_pending_paste && !response.has_focus(),
            );
        } else if response.hovered() {
            self.handle_terminal_scroll(ui, index, &response);
        }

        let default_text = ui.visuals().text_color();
        let muted_text = terminal_muted_text(ui);
        let terminal_background = terminal_background(ui);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, terminal_background);

        self.handle_terminal_selection_drag(ui, index, &response, inner, cell_width, cell_height);
        if response.secondary_clicked()
            && self.right_click_behavior == TerminalRightClickBehavior::SelectWord
        {
            self.selected_session_id = None;
            self.selected_text = response.interact_pointer_pos().and_then(|pointer| {
                self.word_selection_at_pointer(index, pointer, inner, cell_width, cell_height)
            });
        }
        response.context_menu(|ui| self.render_terminal_context_menu(ui, index));

        let Some(session) = self.sessions.get(index) else {
            return;
        };
        let select_all = self.selected_session_id == Some(session.id);
        let selected_word = self
            .selected_text
            .as_ref()
            .filter(|selection| selection.session_id == session.id);
        let screen = session.parser.screen();
        let (rows, cols) = screen.size();
        let font = FontId::new(font_size, FontFamily::Monospace);
        let measured_monospace_width = ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap("m".to_owned(), font.clone(), default_text)
                .rect
                .width()
        });
        let can_merge_text_runs =
            terminal_text_runs_can_merge(cell_width, measured_monospace_width, self.letter_spacing);
        let accent = terminal_accent(ui);
        let selection_fill = blend_color(accent, terminal_background, 0.58);
        let search_fill = blend_color(Color32::from_rgb(231, 185, 87), terminal_background, 0.42);

        let active_search_fill =
            blend_color(Color32::from_rgb(255, 152, 0), terminal_background, 0.85);
        let ansi_palette = colors::terminal_ansi_palette_from_colors(
            terminal_background,
            default_text,
            muted_text,
            accent,
            ui.visuals().warn_fg_color,
            ui.visuals().error_fg_color,
        );
        let mut color_cache = TerminalRenderColorCache::new(
            default_text,
            terminal_background,
            self.draw_bold_text_in_bright_colors,
            self.minimum_contrast_ratio,
            &ansi_palette,
        );
        let search_spans = if self.search_open {
            let active = self.active_terminal_search_screen_match(screen);
            self.cached_terminal_search_query()
                .map(|query| {
                    terminal_visible_search_spans_with_normalized_query(screen, query, active)
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        if select_all {
            painter.rect_filled(inner, 0.0, selection_fill);
        }

        let render_grid = terminal_render_grid(inner, rows, cols, cell_width, cell_height);
        let mut text_runs = Vec::with_capacity(
            render_grid
                .map(|grid| usize::from(grid.visible_rows).saturating_mul(4))
                .unwrap_or_default(),
        );
        let mut search_span_index = 0usize;
        if let Some(render_grid) = render_grid {
            for row in 0..render_grid.visible_rows {
                while search_spans
                    .get(search_span_index)
                    .is_some_and(|span| span.row < row)
                {
                    search_span_index += 1;
                }
                let mut row_search_span_index = search_span_index;
                for col in 0..render_grid.visible_cols {
                    let Some(cell) = screen.cell(row, col) else {
                        continue;
                    };
                    if cell.is_wide_continuation() {
                        continue;
                    }

                    let foreground_color = cell.fgcolor();
                    let colors =
                        color_cache.base_colors(foreground_color, cell.bgcolor(), cell.inverse());
                    let width_cols = if cell.is_wide() { 2 } else { 1 };
                    let Some(cell_rect) = render_grid.cell_rect(row, col, width_cols) else {
                        continue;
                    };
                    let selected_text_cell = selected_word.is_some_and(|selection| {
                        terminal_selection_contains_cell(selection, row, col)
                    });
                    while search_spans
                        .get(row_search_span_index)
                        .is_some_and(|span| span.row == row && col >= span.end_col)
                    {
                        row_search_span_index += 1;
                    }
                    let search_match_cell = search_spans
                        .get(row_search_span_index)
                        .is_some_and(|span| span.contains_cell(row, col));
                    let active_search_match_cell = search_match_cell
                        && search_spans
                            .get(row_search_span_index)
                            .is_some_and(|span| span.active);
                    if !select_all && selected_text_cell {
                        painter.rect_filled(cell_rect, 0.0, selection_fill);
                    } else if active_search_match_cell {
                        painter.rect_filled(cell_rect, 0.0, active_search_fill);
                    } else if search_match_cell {
                        painter.rect_filled(cell_rect, 0.0, search_fill);
                    } else if colors.background != terminal_background {
                        painter.rect_filled(cell_rect, 0.0, colors.background);
                    }

                    let text = cell.contents();
                    if !text.is_empty() {
                        let text_background = if select_all || selected_text_cell {
                            selection_fill
                        } else if active_search_match_cell {
                            active_search_fill
                        } else if search_match_cell {
                            search_fill
                        } else {
                            colors.background
                        };
                        let text_color = color_cache.text_color(
                            foreground_color,
                            colors.foreground,
                            text_background,
                            cell.bold(),
                            cell.dim(),
                        );
                        push_terminal_text_run(
                            &mut text_runs,
                            row,
                            col,
                            width_cols,
                            text,
                            text_color,
                            cell.underline(),
                            can_merge_text_runs
                                && !cell.is_wide()
                                && terminal_cell_text_is_single_char(text),
                        );
                    }
                }
            }
        }

        for run in prepare_terminal_text_runs(&text_runs, render_grid) {
            painter.text(
                run.position,
                Align2::LEFT_TOP,
                run.text,
                font.clone(),
                run.color,
            );
            if let Some((start, end)) = run.underline {
                painter.line_segment([start, end], Stroke::new(1.0, run.color));
            }
        }

        if !screen.hide_cursor()
            && session.scrollback() == 0
            && terminal_cursor_visible(ui, response.has_focus(), self.cursor_blinking)
        {
            let (cursor_row, cursor_col) = screen.cursor_position();
            if let Some(render_grid) = render_grid
                && let Some(cursor_rect) = render_grid.cell_rect(cursor_row, cursor_col, 1)
                && inner.intersects(cursor_rect)
            {
                let cursor_fill = if response.has_focus() {
                    blend_color(accent, Color32::WHITE, 0.18)
                } else {
                    muted_text
                };
                draw_terminal_cursor(
                    &painter,
                    cursor_rect,
                    cursor_fill,
                    response.has_focus(),
                    self.cursor_style,
                    self.cursor_width,
                    self.cursor_style_inactive,
                );
            }
        }

        if let Some(alpha) = self.visual_bell_alpha() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
            painter.rect_filled(
                rect,
                0.0,
                Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), alpha),
            );
        }
    }

    fn render_terminal_context_menu(&mut self, ui: &mut egui::Ui, index: usize) {
        if self.sessions.get(index).is_none() {
            ui.add_enabled(false, egui::Button::new("Terminal unavailable"));
            return;
        }
        self.set_active_session(index);
        let has_session = self.sessions.get(index).is_some();
        let has_copyable_text = self.copyable_text_for_session(index).is_some();
        let has_selection = self.has_selection_for_session(index);

        if ui
            .add_enabled(has_copyable_text, egui::Button::new("Copy"))
            .clicked()
        {
            if let Some(text) = self.copyable_text_for_session(index) {
                ui.ctx().copy_text(text);
            }
            ui.close();
        }
        if ui
            .add_enabled(has_session, egui::Button::new("Paste"))
            .clicked()
        {
            self.request_paste_for_session(index);
            ui.ctx().send_viewport_cmd(ViewportCommand::RequestPaste);
            ui.close();
        }
        if ui
            .add_enabled(has_session, egui::Button::new("Select All"))
            .clicked()
        {
            self.select_all_session(index);
            ui.close();
        }
        if has_selection && ui.button("Clear Selection").clicked() {
            self.selected_session_id = None;
            self.selected_text = None;
            self.selection_drag = None;
            ui.close();
        }
        if ui
            .add_enabled(has_session, egui::Button::new("Clear Buffer"))
            .clicked()
        {
            self.clear_session(index);
            ui.close();
        }
        if ui
            .add_enabled(has_session, egui::Button::new("Rename Terminal..."))
            .clicked()
        {
            self.begin_rename_session(index);
            ui.close();
        }

        ui.separator();

        if ui
            .add_enabled(self.can_open_session(), egui::Button::new("New Terminal"))
            .clicked()
        {
            self.open_new_session();
            ui.close();
        }
        if ui
            .add_enabled(
                has_session && self.can_open_session(),
                egui::Button::new("Split Terminal Right"),
            )
            .clicked()
        {
            self.split_active_session();
            ui.close();
        }
        if ui
            .add_enabled(
                self.split_view && self.sessions.len() > 1,
                egui::Button::new("Join Terminals"),
            )
            .clicked()
        {
            self.unsplit_sessions();
            ui.close();
        }
        if ui
            .add_enabled(has_session, egui::Button::new("Kill Terminal"))
            .clicked()
        {
            self.request_close_active_session();
            ui.close();
        }

        ui.separator();

        if ui
            .add_enabled(has_session, egui::Button::new("Scroll to Top"))
            .clicked()
        {
            self.scroll_session_to_top(index);
            ui.close();
        }
        if ui
            .add_enabled(has_session, egui::Button::new("Scroll to Bottom"))
            .clicked()
        {
            self.scroll_session_to_bottom(index);
            ui.close();
        }

        ui.separator();

        let fullscreen_label = if self.fullscreen {
            "Restore Terminal"
        } else {
            "Maximize Terminal"
        };
        if ui.button(fullscreen_label).clicked() {
            self.toggle_fullscreen();
            ui.close();
        }
        if ui.button("Hide Terminal").clicked() {
            self.set_visible(false);
            ui.close();
        }
    }

    fn handle_terminal_input(
        &mut self,
        ui: &egui::Ui,
        index: usize,
        response: &Response,
        paste_only: bool,
    ) {
        let events = ui.input(|input| input.events.clone());
        for event in events {
            match event {
                Event::Text(text) | Event::Ime(ImeEvent::Commit(text))
                    if !paste_only && !text.is_empty() =>
                {
                    self.set_active_session(index);
                    self.send_input(text);
                }
                Event::Paste(text) if !text.is_empty() => {
                    self.paste_text(index, text);
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if !paste_only => {
                    if terminal_copy_shortcut(key, modifiers) {
                        self.copy_terminal_text(ui, index);
                    } else if let Some(delta) = terminal_scroll_key_delta(key, modifiers) {
                        self.scroll_terminal(index, delta);
                    } else if let Some(input) = terminal_key_input(key, modifiers) {
                        self.set_active_session(index);
                        self.send_input(input);
                    }
                }
                Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                } if response.hovered() => {
                    self.handle_terminal_mouse_wheel(ui, index, response, unit, delta, modifiers);
                }
                _ => {}
            }
        }
    }

    fn copy_terminal_text(&self, ui: &egui::Ui, index: usize) -> bool {
        let Some(text) = self.copyable_text_for_session(index) else {
            return false;
        };
        ui.ctx().copy_text(text);
        true
    }

    fn handle_terminal_scroll(&mut self, ui: &egui::Ui, index: usize, response: &Response) {
        let events = ui.input(|input| input.events.clone());
        for event in events {
            if let Event::MouseWheel {
                unit,
                delta,
                modifiers,
            } = event
            {
                self.handle_terminal_mouse_wheel(ui, index, response, unit, delta, modifiers);
            }
        }
    }

    fn handle_terminal_mouse_wheel(
        &mut self,
        ui: &egui::Ui,
        index: usize,
        response: &Response,
        unit: egui::MouseWheelUnit,
        delta: Vec2,
        modifiers: egui::Modifiers,
    ) {
        if self.mouse_wheel_zoom && terminal_mouse_wheel_zoom_modifier(modifiers) {
            if self.zoom_terminal_font(delta.y) {
                self.set_active_session(index);
                ui.ctx().request_repaint();
            }
            return;
        }

        let delta_rows = wheel_scroll_rows(
            unit,
            delta,
            response.rect.height(),
            self.font_size,
            self.line_height,
            self.letter_spacing,
            self.wheel_scroll_sensitivity(modifiers),
        );
        if delta_rows == 0 {
            return;
        }

        let font_size = terminal_safe_font_size(self.font_size);
        let (cell_width, cell_height) =
            terminal_safe_cell_size(font_size, self.line_height, self.letter_spacing);
        let position = self.cell_position_at_pointer(
            index,
            response.hover_pos(),
            terminal_content_rect(response.rect),
            cell_width,
            cell_height,
        );
        if self.send_terminal_wheel_input(index, position, delta_rows, modifiers) {
            return;
        }

        self.scroll_terminal(index, delta_rows);
    }

    fn handle_terminal_selection_drag(
        &mut self,
        ui: &egui::Ui,
        index: usize,
        response: &Response,
        inner: Rect,
        cell_width: f32,
        cell_height: f32,
    ) {
        if self.alt_click_moves_cursor && ui.input(|input| input.modifiers.alt) {
            self.selection_drag = None;
            return;
        }

        let mut completed_selection = false;
        if response.drag_started_by(PointerButton::Primary)
            && let Some(position) = self.cell_position_at_pointer(
                index,
                response.interact_pointer_pos(),
                inner,
                cell_width,
                cell_height,
            )
        {
            self.set_active_session(index);
            self.selected_session_id = None;
            self.selected_text = None;
            if let Some(session) = self.sessions.get(index) {
                self.selection_drag = Some(super::TerminalSelectionDrag {
                    session_id: session.id,
                    anchor: position,
                });
            }
        }

        if (response.dragged_by(PointerButton::Primary)
            || response.drag_stopped_by(PointerButton::Primary))
            && let Some(drag) = self.selection_drag
            && self
                .sessions
                .get(index)
                .is_some_and(|session| session.id == drag.session_id)
            && let Some(position) = self.cell_position_at_pointer(
                index,
                response.interact_pointer_pos(),
                inner,
                cell_width,
                cell_height,
            )
        {
            completed_selection = self
                .select_text_range_for_session(index, drag.anchor, position)
                .is_some();
        }

        if response.drag_stopped_by(PointerButton::Primary) {
            self.selection_drag = None;
            if completed_selection
                && self.copy_on_selection
                && let Some(text) = self.copyable_text_for_session(index)
            {
                ui.ctx().copy_text(text);
            }
        }
    }

    fn cell_position_at_pointer(
        &self,
        index: usize,
        pointer: Option<egui::Pos2>,
        inner: Rect,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<super::TerminalCellPosition> {
        let pointer = pointer?;
        let session = self.sessions.get(index)?;
        let (rows, cols) = session.parser.screen().size();
        terminal_cell_position_at_pointer(Some(pointer), inner, cell_width, cell_height, rows, cols)
    }

    fn terminal_path_link_at_pointer(
        &self,
        index: usize,
        expected_session_id: usize,
        pointer: Option<egui::Pos2>,
        inner: Rect,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<super::links::TerminalPathLink> {
        let session = self.sessions.get(index)?;
        if session.id != expected_session_id
            || !terminal_path_link_scan_allowed(session.parser.screen().size())
        {
            return None;
        }

        let position =
            self.cell_position_at_pointer(index, pointer, inner, cell_width, cell_height)?;
        let link = self.terminal_path_link_at_cell(index, position)?;
        self.sessions
            .get(index)
            .is_some_and(|session| session.id == expected_session_id)
            .then_some(link)
    }

    fn scroll_terminal(&mut self, index: usize, delta_rows: i32) {
        if delta_rows == 0 {
            return;
        }
        if let Some(session) = self.sessions.get_mut(index) {
            session.scroll_scrollback(delta_rows);
        }
    }

    fn wheel_scroll_sensitivity(&self, modifiers: egui::Modifiers) -> f32 {
        self.mouse_wheel_scroll_sensitivity
            * if modifiers.alt {
                self.fast_scroll_sensitivity
            } else {
                1.0
            }
    }

    fn handle_terminal_middle_click(&mut self, ui: &egui::Ui, index: usize) {
        self.set_active_session(index);
        match self.middle_click_behavior {
            TerminalMiddleClickBehavior::Default => {}
            TerminalMiddleClickBehavior::Paste => {
                self.request_paste_for_session(index);
                ui.ctx().send_viewport_cmd(ViewportCommand::RequestPaste);
            }
        }
    }

    fn render_multiline_paste_warning(&mut self, ctx: &egui::Context) {
        let Some(line_count) = self.pending_multiline_paste_line_count() else {
            return;
        };
        let mut paste = false;
        let mut cancel = false;

        egui::Window::new("Paste Multiple Lines")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([480.0, 142.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("Paste {line_count} lines?")).strong());
                ui.label("Commands in pasted text may run immediately.");

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Paste", PopupButtonKind::Primary).clicked() {
                        paste = true;
                    }
                });
            });

        if cancel {
            self.cancel_pending_multiline_paste();
        } else if paste {
            self.confirm_pending_multiline_paste();
        }
    }

    fn render_kill_confirmation(&mut self, ctx: &egui::Context) {
        let Some(session_id) = self.pending_kill_session_id() else {
            return;
        };
        let mut kill = false;
        let mut cancel = false;

        egui::Window::new("Kill Terminal")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([460.0, 136.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("Kill terminal {session_id}?")).strong());
                ui.label("The running shell process will be closed.");

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Kill", PopupButtonKind::Danger).clicked() {
                        kill = true;
                    }
                });
            });

        if cancel {
            self.cancel_pending_kill();
        } else if kill {
            self.confirm_pending_kill();
        }
    }

    fn render_rename_terminal_dialog(&mut self, ctx: &egui::Context) {
        let Some(session_id) = self.pending_rename_session_id() else {
            return;
        };
        if self.sessions.iter().all(|session| session.id != session_id) {
            self.cancel_pending_rename();
            return;
        }

        let mut save = false;
        let mut cancel = false;

        egui::Window::new("Rename Terminal")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([460.0, 142.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("Terminal {session_id}")).strong());
                let response = ui.add(
                    TextEdit::singleline(&mut self.rename_session_input)
                        .id(terminal_rename_input_id(session_id))
                        .desired_width(f32::INFINITY)
                        .hint_text(TERMINAL_DEFAULT_DISPLAY_LABEL),
                );
                response.request_focus();

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }
                if response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                    save = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Save", PopupButtonKind::Primary).clicked() {
                        save = true;
                    }
                });
            });

        if cancel {
            self.cancel_pending_rename();
        } else if save {
            self.submit_pending_rename();
        }
    }

    fn word_selection_at_pointer(
        &self,
        index: usize,
        pointer: egui::Pos2,
        inner: Rect,
        cell_width: f32,
        cell_height: f32,
    ) -> Option<super::TerminalTextSelection> {
        if !terminal_rect_contains_pointer(inner, pointer) {
            return None;
        }

        let session = self.sessions.get(index)?;
        let screen = session.parser.screen();
        let (rows, cols) = screen.size();
        let position = terminal_cell_position_at_pointer(
            Some(pointer),
            inner,
            cell_width,
            cell_height,
            rows,
            cols,
        )?;

        terminal_word_selection_at_cell(session, position.row, position.col, &self.word_separators)
    }

    fn visual_bell_alpha(&self) -> Option<u8> {
        let bell_at = self.last_bell_at?;
        let duration = Duration::from_millis(self.bell_duration_ms);
        let elapsed = bell_at.elapsed();
        if elapsed >= duration {
            return None;
        }

        let progress = elapsed.as_secs_f32() / duration.as_secs_f32().max(f32::EPSILON);
        Some(((1.0 - progress) * 72.0).round() as u8)
    }
}

impl TerminalPane {
    fn shell_label(&self) -> &str {
        &self.shell_label
    }

    #[cfg(test)]
    pub(super) fn terminal_session_label(&self, session: &super::TerminalSession) -> String {
        let shell_label = self.shell_label();
        let label_context = self.terminal_session_label_context(shell_label);
        self.terminal_session_label_with_context(session, &label_context)
            .into_owned()
    }

    fn terminal_session_label_context<'a>(
        &self,
        shell_label: &'a str,
    ) -> TerminalSessionLabelContext<'a> {
        TerminalSessionLabelContext::new(shell_label, &self.tabs_title_template)
    }

    fn terminal_session_label_with_context<'a>(
        &self,
        session: &'a super::TerminalSession,
        label_context: &'a TerminalSessionLabelContext<'a>,
    ) -> Cow<'a, str> {
        if let Some(custom_title) = session
            .custom_title
            .as_deref()
            .and_then(terminal_display_label)
        {
            return custom_title;
        }

        if label_context.uses_default_title_template
            && self.tabs_allow_agent_cli_title
            && let Some(title) = terminal_session_sequence_title(session)
        {
            return terminal_session_label_from_display_label(session.id, title);
        }

        let shell_label = session
            .process_label
            .as_deref()
            .and_then(terminal_display_label)
            .or_else(|| label_context.shell_display_label())
            .unwrap_or(Cow::Borrowed(TERMINAL_DEFAULT_DISPLAY_LABEL));
        if label_context.uses_default_title_template {
            return terminal_session_label_from_display_label(session.id, shell_label);
        }

        Cow::Owned(self.render_tabs_title_template(session, shell_label.as_ref()))
    }

    fn render_tabs_title_template(
        &self,
        session: &super::TerminalSession,
        shell_label: &str,
    ) -> String {
        let template = self.tabs_title_template.as_str();
        let mut rendered = String::with_capacity(template.len().saturating_add(shell_label.len()));
        let mut rest = template;
        let mut sequence = None::<Cow<'_, str>>;
        let mut cwd = None::<Cow<'_, str>>;
        let mut workspace_folder = None::<Cow<'_, str>>;
        let mut workspace_folder_name = None::<Cow<'_, str>>;

        while let Some(start) = rest.find("${") {
            rendered.push_str(&rest[..start]);
            let placeholder = &rest[start..];
            let Some(end) = placeholder.find('}') else {
                rendered.push_str(placeholder);
                rest = "";
                break;
            };
            let token = &placeholder[..=end];
            match token {
                "${process}" => rendered.push_str(shell_label),
                "${sequence}" => {
                    let sequence = sequence.get_or_insert_with(|| {
                        if self.tabs_allow_agent_cli_title {
                            terminal_session_sequence_title(session).unwrap_or_default()
                        } else {
                            Cow::Borrowed("")
                        }
                    });
                    rendered.push_str(sequence.as_ref());
                }
                "${cwd}" => {
                    let cwd = cwd.get_or_insert_with(|| {
                        terminal_template_path(session.initial_cwd.as_deref().unwrap_or(&self.cwd))
                    });
                    rendered.push_str(cwd.as_ref());
                }
                "${workspaceFolder}" => {
                    let workspace_folder =
                        workspace_folder.get_or_insert_with(|| terminal_template_path(&self.cwd));
                    rendered.push_str(workspace_folder.as_ref());
                }
                "${workspaceFolderName}" => {
                    let workspace_folder_name = workspace_folder_name.get_or_insert_with(|| {
                        self.cwd
                            .file_name()
                            .and_then(|name| name.to_str())
                            .and_then(terminal_display_label)
                            .unwrap_or_else(|| {
                                workspace_folder
                                    .get_or_insert_with(|| terminal_template_path(&self.cwd))
                                    .clone()
                            })
                    });
                    rendered.push_str(workspace_folder_name.as_ref());
                }
                "${separator}" => rendered.push_str(" - "),
                _ => rendered.push_str(token),
            }
            rest = &placeholder[end + 1..];
        }
        rendered.push_str(rest);

        if let Some(rendered) = terminal_display_label(rendered.as_str()) {
            rendered.into_owned()
        } else {
            shell_label.to_owned()
        }
    }

    fn terminal_tab_icon_kind(&self) -> IconKind {
        terminal_tab_icon_kind(&self.tabs_default_icon)
    }

    fn terminal_tab_icon_color(&self, ui: &egui::Ui, fallback: Color32) -> Color32 {
        terminal_tab_color_from_setting(self.tabs_default_color.as_deref(), ui, fallback)
    }
}

fn terminal_search_preview_display_label(preview: &str) -> Option<Cow<'_, str>> {
    terminal_display_label(preview)
}

#[cfg(test)]
mod tests;

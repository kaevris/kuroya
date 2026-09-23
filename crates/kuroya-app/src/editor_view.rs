use crate::{
    KuroyaApp,
    layout::{EDITOR_SPLIT_HANDLE_WIDTH, adjust_split_weights},
    session_state::EditorPane,
};
use eframe::egui::{self, Align, Sense, pos2, vec2};

mod breadcrumbs;

impl KuroyaApp {
    pub(crate) fn render_editor(&mut self, ui: &mut egui::Ui) {
        if self.buffers.is_empty() {
            self.render_dashboard(ui);
            return;
        }

        if self.panes.is_empty() {
            self.panes.push(EditorPane {
                id: 1,
                active: self.active,
                weight: 1.0,
            });
            self.active_pane = 1;
            self.next_pane_id = self.next_pane_id.max(2);
        }

        self.normalize_pane_weights();
        let available = ui.available_size_before_wrap();
        let pane_count = self.panes.len();
        let handle_total = EDITOR_SPLIT_HANDLE_WIDTH * pane_count.saturating_sub(1) as f32;
        let content_width = (available.x - handle_total).max(0.0);
        self.editor_content_width = content_width;
        let height = available.y.max(0.0);
        let panes = self.panes.clone();
        let mut split_drag = None;

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(|ui| {
                let mut remaining_width = content_width;
                for (index, pane) in panes.into_iter().enumerate() {
                    let pane_width = if index + 1 == pane_count {
                        remaining_width.max(0.0)
                    } else {
                        let width = (content_width * pane.weight).max(0.0);
                        remaining_width = (remaining_width - width).max(0.0);
                        width
                    };

                    ui.allocate_ui_with_layout(
                        vec2(pane_width, height),
                        egui::Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_width(pane_width);
                            ui.set_max_width(pane_width);
                            // A pane whose load is still pending (`active: None`)
                            // must render as empty instead of falling back to the
                            // globally active buffer; otherwise it would accept
                            // input for an unrelated buffer and never become
                            // focusable once the load fails.
                            self.render_editor_pane(ui, pane.id, pane.active);
                        },
                    );

                    if index + 1 < pane_count {
                        let (rect, response) = ui.allocate_exact_size(
                            vec2(EDITOR_SPLIT_HANDLE_WIDTH, height),
                            Sense::click_and_drag(),
                        );
                        let response = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                        let visuals = ui.visuals();
                        let fill = if response.dragged() || response.hovered() {
                            visuals.selection.stroke.color
                        } else {
                            visuals.widgets.noninteractive.bg_stroke.color
                        };
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                pos2(rect.center().x - 1.0, rect.top()),
                                pos2(rect.center().x + 1.0, rect.bottom()),
                            ),
                            0.0,
                            fill,
                        );
                        if response.dragged() {
                            split_drag = Some((index, response.drag_delta().x));
                        }
                    }
                }
            });
        });

        if let Some((split_index, delta)) = split_drag {
            let mut weights = self
                .panes
                .iter()
                .map(|pane| pane.weight)
                .collect::<Vec<_>>();
            if adjust_split_weights(&mut weights, split_index, delta, content_width) {
                for (pane, weight) in self.panes.iter_mut().zip(weights) {
                    pane.weight = weight;
                }
            }
        }
    }
}

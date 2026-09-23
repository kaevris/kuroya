use crate::{
    KuroyaApp,
    app_update_overlays::PopupDismissalGuard,
    picker_ui::picker_window_size,
    project_search_panel::{
        controls::{render_project_search_controls, sanitize_project_search_inputs},
        results::{
            ProjectSearchOpenTarget, project_search_open_target, project_search_result_row_height,
        },
    },
    ui_state::{
        clamp_selection, handle_list_navigation_keys, plain_key_pressed, selection_page_step,
    },
};
use eframe::egui::{self, Context, Key};

mod controls;
mod results;

impl KuroyaApp {
    pub(crate) fn render_project_search_overlay(&mut self, ctx: &Context) {
        let dismissal = PopupDismissalGuard::capture(ctx);
        let window_response = egui::Window::new("Project Search")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 96.0,
            ))
            .id(egui::Id::new("project_search"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
            .fixed_size(picker_window_size(ctx))
            .show(ctx, |ui| self.render_project_search(ui));
        let clicked_outside = window_response
            .as_ref()
            .is_some_and(|response| dismissal.clicked_outside(ctx, response.response.rect));
        if clicked_outside || ctx.input(|input| input.key_pressed(Key::Escape)) {
            self.close_project_search();
        }
    }

    fn render_project_search(&mut self, ui: &mut egui::Ui) {
        let controls = render_project_search_controls(self, ui);

        let mut results_match_query = self.project_search_results_match_current_inputs();
        let mut open_selected = false;
        let mut run_search = controls.search_requested;

        let result_count = self.project_search_result.matches.len();
        clamp_selection(&mut self.project_search_selected, result_count);
        let selection_changed = if controls.input_has_focus || result_count == 0 {
            false
        } else {
            let row_height = project_search_result_row_height(ui);
            let viewport_height = ui.available_height();
            ui.input(|input| {
                handle_list_navigation_keys(
                    input,
                    &mut self.project_search_selected,
                    result_count,
                    selection_page_step(row_height, viewport_height),
                )
            })
        };
        if ui.input(|input| plain_key_pressed(input, Key::Enter)) {
            if !controls.input_has_focus && results_match_query && result_count > 0 {
                open_selected = true;
            } else {
                run_search = true;
            }
        }
        ui.separator();

        if open_selected {
            if let Some(target) = project_search_open_target(
                &self.project_search_result,
                self.project_search_selected,
                results_match_query,
            ) {
                open_project_search_target(self, target);
            }
        }
        if run_search {
            sanitize_project_search_inputs(self);
            self.spawn_project_search();
            results_match_query = self.project_search_results_match_current_inputs();
        }

        let current_query = self.project_search_query.trim();
        if let Some(target) = results::render_project_search_results(
            ui,
            &self.workspace.root,
            &self.project_search_result,
            current_query,
            results_match_query,
            &mut self.project_search_selected,
            selection_changed,
        ) {
            open_project_search_target(self, target);
        }
    }
}

fn open_project_search_target(app: &mut KuroyaApp, target: ProjectSearchOpenTarget) {
    let selection_length = app.project_search_result_query.chars().count();
    app.open_file_selection_at_known_openable(
        target.path,
        target.line,
        target.column,
        selection_length,
    );
}

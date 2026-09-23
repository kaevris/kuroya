use crate::{
    KuroyaApp,
    layout::{
        DIAGNOSTICS_PANEL_MAX_WIDTH, DIAGNOSTICS_PANEL_MIN_WIDTH, EXPLORER_MAX_WIDTH,
        EXPLORER_MIN_WIDTH, SOURCE_CONTROL_MAX_WIDTH, SOURCE_CONTROL_MIN_WIDTH,
        SYMBOLS_PANEL_MAX_WIDTH, SYMBOLS_PANEL_MIN_WIDTH, TERMINAL_MIN_HEIGHT,
        clamp_diagnostics_panel_width, clamp_explorer_width, clamp_source_control_width,
        clamp_symbols_panel_width, clamp_terminal_height_for_available_height,
        responsive_side_panel_max_width, responsive_terminal_max_height, terminal_open_height,
    },
    panel_layout::{PanelDockSide, PanelPlacement},
};
use eframe::egui::{self, Context, Frame, Id, TopBottomPanel};
use std::ops::RangeInclusive;

impl KuroyaApp {
    pub(super) fn render_main_panels(&mut self, ctx: &Context) {
        let full_app_background = self.full_app_background_image_is_ready();
        if main_tabs_visible(self.terminal.visible, self.terminal.is_fullscreen()) {
            TopBottomPanel::top("tabs")
                .exact_height(40.0)
                .frame(side_top_panel_frame(ctx, full_app_background))
                .show(ctx, |ui| {
                    self.render_tabs(ui);
                });
        }

        if self.terminal.visible && self.terminal.is_fullscreen() {
            egui::CentralPanel::default()
                .show(ctx, |ui| self.terminal.ui(ui, &mut self.command_bus));
            return;
        }

        let content_width = ctx.available_rect().width();
        let mut docked_widths = DockedSidePanelWidths::from_app(self);
        let explorer_response = egui::SidePanel::left("explorer")
            .resizable(true)
            .default_width(self.explorer_width)
            .frame(side_top_panel_frame(ctx, full_app_background))
            .width_range(docked_widths.responsive_width_range(
                content_width,
                DockedSidePanel::Explorer,
                EXPLORER_MIN_WIDTH,
                EXPLORER_MAX_WIDTH,
            ))
            .show(ctx, |ui| self.render_explorer(ui));
        self.explorer_width = clamp_explorer_width(explorer_response.response.rect.width());
        docked_widths.set_width(DockedSidePanel::Explorer, self.explorer_width);

        self.render_source_control_container(ctx, content_width, &mut docked_widths);
        self.render_symbols_container(ctx, content_width, &mut docked_widths);
        self.render_diagnostics_container(ctx, content_width, &mut docked_widths);

        if self.settings.status_bar_visible {
            TopBottomPanel::bottom("status")
                .exact_height(30.0)
                .frame(side_top_panel_frame(ctx, full_app_background))
                .show(ctx, |ui| self.render_status_bar(ui));
        }

        if self.terminal.visible {
            let available_height = ctx.available_rect().height();
            let force_open_height = self.terminal_open_height_pending;
            if self.terminal_open_height_pending {
                self.terminal_height = terminal_open_height(available_height);
                self.terminal_open_height_pending = false;
            } else {
                self.terminal_height = clamp_terminal_height_for_available_height(
                    self.terminal_height,
                    available_height,
                );
            }
            let terminal_max_height = responsive_terminal_max_height(available_height);
            let terminal_panel = TopBottomPanel::bottom("terminal")
                .resizable(true)
                .default_height(self.terminal_height)
                .frame(side_top_panel_frame(ctx, full_app_background));
            let terminal_panel = if force_open_height {
                terminal_panel.exact_height(self.terminal_height)
            } else {
                terminal_panel.height_range(TERMINAL_MIN_HEIGHT..=terminal_max_height)
            };
            let terminal_response =
                terminal_panel.show(ctx, |ui| self.terminal.ui(ui, &mut self.command_bus));
            self.terminal_height = clamp_terminal_height_for_available_height(
                terminal_response.response.rect.height(),
                available_height,
            );
        }

        let editor_fill = ctx.style().visuals.code_bg_color;
        let editor_frame = if self.background_image_is_ready() {
            Frame::NONE
        } else {
            Frame::NONE.fill(editor_fill)
        };
        egui::CentralPanel::default()
            .frame(editor_frame)
            .show(ctx, |ui| {
                self.render_editor_background_image(ui);
                self.render_editor(ui);
            });
    }

    fn render_source_control_container(
        &mut self,
        ctx: &Context,
        content_width: f32,
        docked_widths: &mut DockedSidePanelWidths,
    ) {
        if !self.source_control {
            return;
        }

        let placement = self.source_control_placement;
        if placement.is_floating() {
            let mut open = true;
            let response = egui::Window::new("Source Control")
                .max_size(crate::layout::popup_window_max_size(ctx))
                .id(Id::new("source_control_floating"))
                .resizable(true)
                .default_width(self.source_control_width)
                .open(&mut open)
                .show(ctx, |ui| self.render_source_control_panel(ui));
            self.source_control = open;
            if let Some(response) = response {
                self.source_control_width =
                    clamp_source_control_width(response.response.rect.width());
            }
            return;
        }

        let width_range = docked_widths.responsive_width_range(
            content_width,
            DockedSidePanel::SourceControl,
            SOURCE_CONTROL_MIN_WIDTH,
            SOURCE_CONTROL_MAX_WIDTH,
        );
        let panel_frame = side_top_panel_frame(ctx, self.full_app_background_image_is_ready());
        let response = match panel_dock_side(placement) {
            PanelDockSide::Left => egui::SidePanel::left("source_control")
                .resizable(true)
                .default_width(self.source_control_width)
                .frame(panel_frame)
                .width_range(width_range)
                .show(ctx, |ui| self.render_source_control_panel(ui)),
            PanelDockSide::Right => egui::SidePanel::right("source_control")
                .resizable(true)
                .default_width(self.source_control_width)
                .frame(panel_frame)
                .width_range(width_range)
                .show(ctx, |ui| self.render_source_control_panel(ui)),
        };
        self.source_control_width = clamp_source_control_width(response.response.rect.width());
        docked_widths.set_width(DockedSidePanel::SourceControl, self.source_control_width);
    }

    fn render_symbols_container(
        &mut self,
        ctx: &Context,
        content_width: f32,
        docked_widths: &mut DockedSidePanelWidths,
    ) {
        if !self.symbols_panel {
            return;
        }

        let placement = self.symbols_panel_placement;
        if placement.is_floating() {
            let mut open = true;
            let response = egui::Window::new("File Symbols")
                .max_size(crate::layout::popup_window_max_size(ctx))
                .id(Id::new("symbols_floating"))
                .resizable(true)
                .default_width(self.symbols_panel_width)
                .open(&mut open)
                .show(ctx, |ui| self.render_symbols_panel(ui));
            self.symbols_panel = open;
            if let Some(response) = response {
                self.symbols_panel_width =
                    clamp_symbols_panel_width(response.response.rect.width());
            }
            return;
        }

        let width_range = docked_widths.responsive_width_range(
            content_width,
            DockedSidePanel::Symbols,
            SYMBOLS_PANEL_MIN_WIDTH,
            SYMBOLS_PANEL_MAX_WIDTH,
        );
        let panel_frame = side_top_panel_frame(ctx, self.full_app_background_image_is_ready());
        let response = match panel_dock_side(placement) {
            PanelDockSide::Left => egui::SidePanel::left("symbols")
                .resizable(true)
                .default_width(self.symbols_panel_width)
                .frame(panel_frame)
                .width_range(width_range)
                .show(ctx, |ui| self.render_symbols_panel(ui)),
            PanelDockSide::Right => egui::SidePanel::right("symbols")
                .resizable(true)
                .default_width(self.symbols_panel_width)
                .frame(panel_frame)
                .width_range(width_range)
                .show(ctx, |ui| self.render_symbols_panel(ui)),
        };
        self.symbols_panel_width = clamp_symbols_panel_width(response.response.rect.width());
        docked_widths.set_width(DockedSidePanel::Symbols, self.symbols_panel_width);
    }

    fn render_diagnostics_container(
        &mut self,
        ctx: &Context,
        content_width: f32,
        docked_widths: &mut DockedSidePanelWidths,
    ) {
        if !self.diagnostics_panel {
            return;
        }

        let placement = self.diagnostics_panel_placement;
        if placement.is_floating() {
            let mut open = true;
            let response = egui::Window::new("Diagnostics")
                .max_size(crate::layout::popup_window_max_size(ctx))
                .id(Id::new("diagnostics_floating"))
                .resizable(true)
                .default_width(self.diagnostics_panel_width)
                .open(&mut open)
                .show(ctx, |ui| self.render_diagnostics_panel(ui));
            self.diagnostics_panel = open;
            if let Some(response) = response {
                self.diagnostics_panel_width =
                    clamp_diagnostics_panel_width(response.response.rect.width());
            }
            return;
        }

        let width_range = docked_widths.responsive_width_range(
            content_width,
            DockedSidePanel::Diagnostics,
            DIAGNOSTICS_PANEL_MIN_WIDTH,
            DIAGNOSTICS_PANEL_MAX_WIDTH,
        );
        let panel_frame = side_top_panel_frame(ctx, self.full_app_background_image_is_ready());
        let response = match panel_dock_side(placement) {
            PanelDockSide::Left => egui::SidePanel::left("diagnostics")
                .resizable(true)
                .default_width(self.diagnostics_panel_width)
                .frame(panel_frame)
                .width_range(width_range)
                .show(ctx, |ui| self.render_diagnostics_panel(ui)),
            PanelDockSide::Right => egui::SidePanel::right("diagnostics")
                .resizable(true)
                .default_width(self.diagnostics_panel_width)
                .frame(panel_frame)
                .width_range(width_range)
                .show(ctx, |ui| self.render_diagnostics_panel(ui)),
        };
        self.diagnostics_panel_width =
            clamp_diagnostics_panel_width(response.response.rect.width());
        docked_widths.set_width(DockedSidePanel::Diagnostics, self.diagnostics_panel_width);
    }
}

fn main_tabs_visible(terminal_visible: bool, terminal_fullscreen: bool) -> bool {
    !(terminal_visible && terminal_fullscreen)
}

fn side_top_panel_frame(ctx: &Context, full_app_background: bool) -> Frame {
    let frame = Frame::side_top_panel(ctx.style().as_ref());
    if full_app_background {
        frame.fill(egui::Color32::TRANSPARENT)
    } else {
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::{main_tabs_visible, side_top_panel_frame};
    use eframe::egui::{Color32, Context};

    #[test]
    fn main_tabs_hide_while_terminal_is_fullscreen() {
        assert!(main_tabs_visible(false, false));
        assert!(main_tabs_visible(true, false));
        assert!(main_tabs_visible(false, true));
        assert!(!main_tabs_visible(true, true));
    }

    #[test]
    fn full_app_background_makes_main_panel_frames_transparent() {
        let ctx = Context::default();
        assert_ne!(side_top_panel_frame(&ctx, false).fill, Color32::TRANSPARENT);
        assert_eq!(side_top_panel_frame(&ctx, true).fill, Color32::TRANSPARENT);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DockedSidePanel {
    Explorer,
    SourceControl,
    Symbols,
    Diagnostics,
}

#[derive(Debug, Clone, Copy)]
struct DockedSidePanelWidths {
    explorer: f32,
    source_control: f32,
    symbols: f32,
    diagnostics: f32,
    total: f32,
}

impl DockedSidePanelWidths {
    fn from_app(app: &KuroyaApp) -> Self {
        let explorer = clamp_explorer_width(app.explorer_width);
        let source_control = docked_panel_width(
            app.source_control,
            app.source_control_placement,
            app.source_control_width,
            clamp_source_control_width,
        );
        let symbols = docked_panel_width(
            app.symbols_panel,
            app.symbols_panel_placement,
            app.symbols_panel_width,
            clamp_symbols_panel_width,
        );
        let diagnostics = docked_panel_width(
            app.diagnostics_panel,
            app.diagnostics_panel_placement,
            app.diagnostics_panel_width,
            clamp_diagnostics_panel_width,
        );
        let total = explorer + source_control + symbols + diagnostics;

        Self {
            explorer,
            source_control,
            symbols,
            diagnostics,
            total,
        }
    }

    fn responsive_width_range(
        &self,
        total_width: f32,
        panel: DockedSidePanel,
        panel_min_width: f32,
        panel_max_width: f32,
    ) -> RangeInclusive<f32> {
        let other_width = self.width_excluding(panel);
        panel_min_width
            ..=responsive_side_panel_max_width(
                total_width,
                other_width,
                panel_min_width,
                panel_max_width,
            )
    }

    fn set_width(&mut self, panel: DockedSidePanel, width: f32) {
        let previous = self.width(panel);
        let width = match panel {
            DockedSidePanel::Explorer => clamp_explorer_width(width),
            DockedSidePanel::SourceControl => clamp_source_control_width(width),
            DockedSidePanel::Symbols => clamp_symbols_panel_width(width),
            DockedSidePanel::Diagnostics => clamp_diagnostics_panel_width(width),
        };

        match panel {
            DockedSidePanel::Explorer => self.explorer = width,
            DockedSidePanel::SourceControl => self.source_control = width,
            DockedSidePanel::Symbols => self.symbols = width,
            DockedSidePanel::Diagnostics => self.diagnostics = width,
        }
        self.total += width - previous;
    }

    fn width_excluding(&self, excluded: DockedSidePanel) -> f32 {
        (self.total - self.width(excluded)).max(0.0)
    }

    fn width(&self, panel: DockedSidePanel) -> f32 {
        match panel {
            DockedSidePanel::Explorer => self.explorer,
            DockedSidePanel::SourceControl => self.source_control,
            DockedSidePanel::Symbols => self.symbols,
            DockedSidePanel::Diagnostics => self.diagnostics,
        }
    }
}

fn docked_panel_width(
    is_open: bool,
    placement: PanelPlacement,
    width: f32,
    clamp_width: fn(f32) -> f32,
) -> f32 {
    if is_open && !placement.is_floating() {
        clamp_width(width)
    } else {
        0.0
    }
}

fn panel_dock_side(placement: PanelPlacement) -> PanelDockSide {
    placement.dock_side().unwrap_or(PanelDockSide::Right)
}

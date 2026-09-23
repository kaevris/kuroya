use crate::{
    KuroyaApp,
    ui_icons::{IconKind, icon_text_button},
    workspace_state::PaneId,
};
use eframe::egui::{
    self, Align, Button, Layout, Rect, RichText, Sense, Stroke, UiBuilder, pos2, vec2,
};
use kuroya_core::{BufferId, Command, GitChangeStage};
use std::path::Path;

const EDITOR_HEADER_HEIGHT: f32 = 28.0;
const EDITOR_HEADER_HORIZONTAL_PADDING: f32 = 10.0;
const DIFF_TOOLBAR_BREADCRUMB_GAP: f32 = 8.0;

impl KuroyaApp {
    pub(crate) fn render_empty_editor_pane(&mut self, ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            if icon_text_button(ui, IconKind::Search, "Open file", None, 132.0).clicked() {
                self.command_bus.push(Command::ToggleQuickOpen);
            }
        });
    }

    pub(crate) fn render_editor_pane_header(
        &mut self,
        ui: &mut egui::Ui,
        pane_id: PaneId,
        active_id: BufferId,
        active_path: Option<&Path>,
        diff_source_file_actions: bool,
    ) {
        let selected = self.active_pane == pane_id;
        let diff_compact_mode = self.settings.diff_compact_mode;
        let width = stable_nonnegative_extent(ui.available_width());
        let (rect, response) =
            ui.allocate_exact_size(vec2(width, EDITOR_HEADER_HEIGHT), Sense::click());

        let visuals = ui.visuals();
        let fill = if selected {
            visuals.widgets.active.bg_fill
        } else {
            visuals.panel_fill
        };
        ui.painter().rect_filled(rect, 0.0, fill);
        ui.painter().line_segment(
            [
                pos2(rect.left(), rect.bottom() - 0.5),
                pos2(rect.right(), rect.bottom() - 0.5),
            ],
            Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color),
        );
        if response.clicked() {
            self.active_pane = pane_id;
            self.focused_pane = Some(pane_id);
            self.set_active_buffer(active_id);
            response.request_focus();
        }
        response.context_menu(|ui| {
            if ui.button("Split Right").clicked() {
                self.active_pane = pane_id;
                self.split_buffer_right(active_id);
                ui.close();
            }
            if ui
                .add_enabled(self.panes.len() > 1, Button::new("Close Pane"))
                .clicked()
            {
                self.active_pane = pane_id;
                self.close_active_pane();
                ui.close();
            }
            if ui
                .add_enabled(self.panes.len() > 1, Button::new("Reset Split Widths"))
                .clicked()
            {
                self.reset_pane_weights();
                ui.close();
            }
        });

        let horizontal_padding = EDITOR_HEADER_HORIZONTAL_PADDING.min(rect.width() * 0.5);
        let inner = rect.shrink2(vec2(horizontal_padding, 0.0));
        let diff_source = self.diff_buffer_sources.get(&active_id);
        let diff_toolbar_actions = diff_editor_toolbar_action_kinds(
            diff_source.and_then(|source| source.hunk_stage),
            diff_source_file_actions,
            self.can_copy_diff_buffer_patch(active_id),
            diff_source.is_some(),
            diff_source.is_some(),
            diff_source.is_some_and(|source| source.base_path.is_some()),
        );
        let max_toolbar_width = stable_nonnegative_extent(inner.width() * 0.72);
        let diff_toolbar_actions = diff_editor_toolbar_fitted_actions(
            diff_toolbar_actions,
            max_toolbar_width,
            diff_compact_mode,
        );
        let toolbar_width =
            diff_editor_toolbar_width_with_mode(&diff_toolbar_actions, diff_compact_mode);
        let breadcrumb_right = if diff_toolbar_actions.is_empty() {
            inner.right()
        } else {
            (inner.right() - toolbar_width - DIFF_TOOLBAR_BREADCRUMB_GAP).max(inner.left())
        };
        let breadcrumb_rect =
            Rect::from_min_max(inner.left_top(), pos2(breadcrumb_right, inner.bottom()));
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(breadcrumb_rect)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                if let Some(path) = active_path {
                    self.render_breadcrumbs(ui, active_id, path);
                } else {
                    ui.label(
                        RichText::new("Untitled")
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                }
                if self
                    .buffer(active_id)
                    .is_some_and(kuroya_core::TextBuffer::is_read_only)
                {
                    ui.label(
                        RichText::new("read-only")
                            .small()
                            .color(ui.visuals().warn_fg_color),
                    );
                }
            },
        );
        if !diff_toolbar_actions.is_empty() && toolbar_width > 0.0 {
            let toolbar_left = (inner.right() - toolbar_width)
                .max(inner.left())
                .min(inner.right());
            let toolbar_rect =
                Rect::from_min_max(pos2(toolbar_left, inner.top()), inner.right_bottom());
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(toolbar_rect)
                    .layout(Layout::right_to_left(Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                    for action in diff_toolbar_actions.iter().rev().copied() {
                        if diff_editor_toolbar_button(ui, action, diff_compact_mode).clicked() {
                            self.active_pane = pane_id;
                            self.focused_pane = Some(pane_id);
                            self.set_active_buffer(active_id);
                            self.command_bus.push(diff_editor_toolbar_command(action));
                        }
                    }
                },
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffEditorToolbarActionKind {
    RefreshDiff,
    SwapCompareSides,
    PreviousHunk,
    NextHunk,
    CopyPatch,
    CopyHunkPatch,
    OpenAccessibleDiffViewer,
    OpenBaseFile,
    OpenBaseAtHunk,
    OpenSourceFile,
    OpenSourceAtHunk,
    StageHunk,
    UnstageHunk,
    DiscardHunk,
}

fn diff_editor_toolbar_action_kinds(
    stage: Option<GitChangeStage>,
    source_exists: bool,
    patch_enabled: bool,
    refresh_enabled: bool,
    base_enabled: bool,
    swap_enabled: bool,
) -> Vec<DiffEditorToolbarActionKind> {
    let mut actions = Vec::with_capacity(12);
    if refresh_enabled {
        actions.push(DiffEditorToolbarActionKind::RefreshDiff);
    }
    if swap_enabled {
        actions.push(DiffEditorToolbarActionKind::SwapCompareSides);
    }
    if patch_enabled {
        actions.push(DiffEditorToolbarActionKind::PreviousHunk);
        actions.push(DiffEditorToolbarActionKind::NextHunk);
        actions.push(DiffEditorToolbarActionKind::CopyPatch);
        actions.push(DiffEditorToolbarActionKind::CopyHunkPatch);
        actions.push(DiffEditorToolbarActionKind::OpenAccessibleDiffViewer);
    }
    if base_enabled {
        actions.push(DiffEditorToolbarActionKind::OpenBaseFile);
        if patch_enabled {
            actions.push(DiffEditorToolbarActionKind::OpenBaseAtHunk);
        }
    }
    if source_exists {
        actions.push(DiffEditorToolbarActionKind::OpenSourceFile);
        if patch_enabled {
            actions.push(DiffEditorToolbarActionKind::OpenSourceAtHunk);
        }
    }
    match (stage, patch_enabled) {
        (Some(GitChangeStage::Unstaged), true) => {
            actions.push(DiffEditorToolbarActionKind::StageHunk);
            actions.push(DiffEditorToolbarActionKind::DiscardHunk);
        }
        (Some(GitChangeStage::Staged), true) => {
            actions.push(DiffEditorToolbarActionKind::UnstageHunk);
        }
        _ => {}
    }
    actions
}

#[cfg(test)]
pub(crate) fn diff_editor_toolbar_action_labels(
    stage: Option<GitChangeStage>,
    source_exists: bool,
    patch_enabled: bool,
    refresh_enabled: bool,
    base_enabled: bool,
    swap_enabled: bool,
) -> Vec<&'static str> {
    diff_editor_toolbar_action_kinds(
        stage,
        source_exists,
        patch_enabled,
        refresh_enabled,
        base_enabled,
        swap_enabled,
    )
    .into_iter()
    .map(diff_editor_toolbar_tooltip)
    .collect()
}

fn diff_editor_toolbar_width_with_mode(
    actions: &[DiffEditorToolbarActionKind],
    compact_mode: bool,
) -> f32 {
    actions
        .iter()
        .map(|action| diff_editor_toolbar_button_width(*action, compact_mode))
        .sum::<f32>()
        + actions.len().saturating_sub(1) as f32 * 4.0
}

fn diff_editor_toolbar_fitted_actions(
    mut actions: Vec<DiffEditorToolbarActionKind>,
    max_width: f32,
    compact_mode: bool,
) -> Vec<DiffEditorToolbarActionKind> {
    if !max_width.is_finite() || max_width <= 0.0 {
        return Vec::new();
    }
    let mut current_width = diff_editor_toolbar_width_with_mode(&actions, compact_mode);
    for action in [
        DiffEditorToolbarActionKind::CopyHunkPatch,
        DiffEditorToolbarActionKind::OpenAccessibleDiffViewer,
        DiffEditorToolbarActionKind::OpenBaseAtHunk,
        DiffEditorToolbarActionKind::CopyPatch,
        DiffEditorToolbarActionKind::OpenSourceAtHunk,
        DiffEditorToolbarActionKind::OpenSourceFile,
        DiffEditorToolbarActionKind::OpenBaseFile,
        DiffEditorToolbarActionKind::SwapCompareSides,
        DiffEditorToolbarActionKind::PreviousHunk,
        DiffEditorToolbarActionKind::NextHunk,
        DiffEditorToolbarActionKind::RefreshDiff,
        DiffEditorToolbarActionKind::DiscardHunk,
        DiffEditorToolbarActionKind::StageHunk,
        DiffEditorToolbarActionKind::UnstageHunk,
    ] {
        if current_width <= max_width {
            break;
        }
        if let Some(index) = actions.iter().position(|candidate| *candidate == action) {
            current_width -= diff_editor_toolbar_removed_width(actions.len(), action, compact_mode);
            actions.remove(index);
        }
    }
    if current_width > max_width {
        Vec::new()
    } else {
        actions
    }
}

fn diff_editor_toolbar_removed_width(
    action_count: usize,
    action: DiffEditorToolbarActionKind,
    compact_mode: bool,
) -> f32 {
    let gap_width = if action_count > 1 { 4.0 } else { 0.0 };
    diff_editor_toolbar_button_width(action, compact_mode) + gap_width
}

#[cfg(test)]
pub(crate) fn diff_editor_toolbar_action_labels_for_width(
    stage: Option<GitChangeStage>,
    source_exists: bool,
    patch_enabled: bool,
    refresh_enabled: bool,
    base_enabled: bool,
    swap_enabled: bool,
    max_width: f32,
) -> Vec<&'static str> {
    diff_editor_toolbar_fitted_actions(
        diff_editor_toolbar_action_kinds(
            stage,
            source_exists,
            patch_enabled,
            refresh_enabled,
            base_enabled,
            swap_enabled,
        ),
        max_width,
        false,
    )
    .into_iter()
    .map(diff_editor_toolbar_tooltip)
    .collect()
}

#[cfg(test)]
pub(crate) fn diff_editor_toolbar_button_labels_for_width(
    stage: Option<GitChangeStage>,
    source_exists: bool,
    patch_enabled: bool,
    refresh_enabled: bool,
    base_enabled: bool,
    swap_enabled: bool,
    max_width: f32,
    compact_mode: bool,
) -> Vec<&'static str> {
    diff_editor_toolbar_fitted_actions(
        diff_editor_toolbar_action_kinds(
            stage,
            source_exists,
            patch_enabled,
            refresh_enabled,
            base_enabled,
            swap_enabled,
        ),
        max_width,
        compact_mode,
    )
    .into_iter()
    .map(|action| diff_editor_toolbar_button_label(action, compact_mode))
    .collect()
}

fn diff_editor_toolbar_button(
    ui: &mut egui::Ui,
    action: DiffEditorToolbarActionKind,
    compact_mode: bool,
) -> egui::Response {
    ui.add_sized(
        vec2(diff_editor_toolbar_button_width(action, compact_mode), 22.0),
        Button::new(RichText::new(diff_editor_toolbar_button_label(action, compact_mode)).small()),
    )
    .on_hover_text(diff_editor_toolbar_tooltip(action))
}

fn diff_editor_toolbar_button_width(
    action: DiffEditorToolbarActionKind,
    compact_mode: bool,
) -> f32 {
    if compact_mode {
        return (diff_editor_toolbar_button_label(action, compact_mode).len() as f32 * 6.0 + 18.0)
            .ceil()
            .max(36.0);
    }
    match action {
        DiffEditorToolbarActionKind::RefreshDiff => 58.0,
        DiffEditorToolbarActionKind::SwapCompareSides => 46.0,
        DiffEditorToolbarActionKind::PreviousHunk | DiffEditorToolbarActionKind::NextHunk => 42.0,
        DiffEditorToolbarActionKind::CopyPatch => 50.0,
        DiffEditorToolbarActionKind::CopyHunkPatch => 46.0,
        DiffEditorToolbarActionKind::OpenAccessibleDiffViewer => 74.0,
        DiffEditorToolbarActionKind::OpenBaseFile => 44.0,
        DiffEditorToolbarActionKind::OpenBaseAtHunk => 76.0,
        DiffEditorToolbarActionKind::OpenSourceFile => 42.0,
        DiffEditorToolbarActionKind::OpenSourceAtHunk => 54.0,
        DiffEditorToolbarActionKind::StageHunk => 48.0,
        DiffEditorToolbarActionKind::UnstageHunk => 62.0,
        DiffEditorToolbarActionKind::DiscardHunk => 62.0,
    }
}

fn diff_editor_toolbar_button_label(
    action: DiffEditorToolbarActionKind,
    compact_mode: bool,
) -> &'static str {
    if compact_mode {
        return match action {
            DiffEditorToolbarActionKind::RefreshDiff => "Ref",
            DiffEditorToolbarActionKind::SwapCompareSides => "Swap",
            DiffEditorToolbarActionKind::PreviousHunk => "Prev",
            DiffEditorToolbarActionKind::NextHunk => "Next",
            DiffEditorToolbarActionKind::CopyPatch => "Patch",
            DiffEditorToolbarActionKind::CopyHunkPatch => "Hunk",
            DiffEditorToolbarActionKind::OpenAccessibleDiffViewer => "A11y",
            DiffEditorToolbarActionKind::OpenBaseFile => "Base",
            DiffEditorToolbarActionKind::OpenBaseAtHunk => "Base",
            DiffEditorToolbarActionKind::OpenSourceFile => "File",
            DiffEditorToolbarActionKind::OpenSourceAtHunk => "Src",
            DiffEditorToolbarActionKind::StageHunk => "Stage",
            DiffEditorToolbarActionKind::UnstageHunk => "Unstg",
            DiffEditorToolbarActionKind::DiscardHunk => "Disc",
        };
    }
    match action {
        DiffEditorToolbarActionKind::RefreshDiff => "Refresh",
        DiffEditorToolbarActionKind::SwapCompareSides => "Swap",
        DiffEditorToolbarActionKind::PreviousHunk => "Prev",
        DiffEditorToolbarActionKind::NextHunk => "Next",
        DiffEditorToolbarActionKind::CopyPatch => "Patch",
        DiffEditorToolbarActionKind::CopyHunkPatch => "Hunk",
        DiffEditorToolbarActionKind::OpenAccessibleDiffViewer => "A11y Diff",
        DiffEditorToolbarActionKind::OpenBaseFile => "Base",
        DiffEditorToolbarActionKind::OpenBaseAtHunk => "Base Hunk",
        DiffEditorToolbarActionKind::OpenSourceFile => "File",
        DiffEditorToolbarActionKind::OpenSourceAtHunk => "Source",
        DiffEditorToolbarActionKind::StageHunk => "Stage",
        DiffEditorToolbarActionKind::UnstageHunk => "Unstage",
        DiffEditorToolbarActionKind::DiscardHunk => "Discard",
    }
}

fn diff_editor_toolbar_tooltip(action: DiffEditorToolbarActionKind) -> &'static str {
    match action {
        DiffEditorToolbarActionKind::RefreshDiff => "Refresh Diff",
        DiffEditorToolbarActionKind::SwapCompareSides => "Swap Compare Sides",
        DiffEditorToolbarActionKind::PreviousHunk => "Previous Diff Hunk",
        DiffEditorToolbarActionKind::NextHunk => "Next Diff Hunk",
        DiffEditorToolbarActionKind::CopyPatch => "Copy Diff Patch",
        DiffEditorToolbarActionKind::CopyHunkPatch => "Copy Current Diff Hunk Patch",
        DiffEditorToolbarActionKind::OpenAccessibleDiffViewer => "Open Accessible Diff Viewer",
        DiffEditorToolbarActionKind::OpenBaseFile => "Open Diff Base File",
        DiffEditorToolbarActionKind::OpenBaseAtHunk => "Open Base at Current Diff Hunk",
        DiffEditorToolbarActionKind::OpenSourceFile => "Open Diff Source File",
        DiffEditorToolbarActionKind::OpenSourceAtHunk => "Open Source at Current Diff Hunk",
        DiffEditorToolbarActionKind::StageHunk => "Stage Current Diff Hunk",
        DiffEditorToolbarActionKind::UnstageHunk => "Unstage Current Diff Hunk",
        DiffEditorToolbarActionKind::DiscardHunk => "Discard Current Diff Hunk",
    }
}

fn diff_editor_toolbar_command(action: DiffEditorToolbarActionKind) -> Command {
    match action {
        DiffEditorToolbarActionKind::RefreshDiff => Command::RefreshActiveDiff,
        DiffEditorToolbarActionKind::SwapCompareSides => Command::SwapActiveDiffSides,
        DiffEditorToolbarActionKind::PreviousHunk => Command::PreviousDiffHunk,
        DiffEditorToolbarActionKind::NextHunk => Command::NextDiffHunk,
        DiffEditorToolbarActionKind::CopyPatch => Command::CopyActiveDiffPatch,
        DiffEditorToolbarActionKind::CopyHunkPatch => Command::CopyActiveDiffHunkPatch,
        DiffEditorToolbarActionKind::OpenAccessibleDiffViewer => {
            Command::OpenActiveAccessibleDiffViewer
        }
        DiffEditorToolbarActionKind::OpenBaseFile => Command::OpenActiveDiffBaseFile,
        DiffEditorToolbarActionKind::OpenBaseAtHunk => Command::OpenActiveDiffHunkBase,
        DiffEditorToolbarActionKind::OpenSourceFile => Command::OpenActiveDiffSourceFile,
        DiffEditorToolbarActionKind::OpenSourceAtHunk => Command::OpenActiveDiffHunkSource,
        DiffEditorToolbarActionKind::StageHunk => Command::StageActiveDiffHunk,
        DiffEditorToolbarActionKind::UnstageHunk => Command::UnstageActiveDiffHunk,
        DiffEditorToolbarActionKind::DiscardHunk => Command::DiscardActiveDiffHunk,
    }
}

fn stable_nonnegative_extent(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

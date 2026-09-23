use crate::{editor_pane_rows::EditorRowContext, theme::diagnostic_color};
use eframe::egui::{self, Color32, FontFamily, FontId, Rect, pos2, vec2};
use kuroya_core::settings::clamp_editor_font_size;
use kuroya_core::{
    EditorLightbulbMode, EditorLineNumbers, EditorRenderFinalNewline, EditorShowFoldingControls,
    GitChangeStage, GitLineChangeKind, ScmDiffDecorationsGutterAction,
    ScmDiffDecorationsGutterPattern, ScmDiffDecorationsGutterVisibility, TextBuffer,
    buffer::CursorPosition,
};

const CODE_ACTION_MARKER_LEFT: f32 = 8.0;
const CODE_ACTION_MARKER_WIDTH: f32 = 8.0;
const DIFF_GUTTER_ACTION_WIDTH: f32 = 12.0;
const DIFF_GUTTER_ACTION_GAP: f32 = 3.0;
const EMPTY_DIFF_GUTTER_ACTIONS: &[(DiffGutterAction, &str)] = &[];
const STAGE_DIFF_GUTTER_ACTIONS: &[(DiffGutterAction, &str)] = &[(DiffGutterAction::Stage, "S")];
const STAGE_DISCARD_DIFF_GUTTER_ACTIONS: &[(DiffGutterAction, &str)] = &[
    (DiffGutterAction::Stage, "S"),
    (DiffGutterAction::Discard, "D"),
];
const UNSTAGE_DIFF_GUTTER_ACTIONS: &[(DiffGutterAction, &str)] =
    &[(DiffGutterAction::Unstage, "U")];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffGutterAction {
    Stage,
    Unstage,
    Discard,
}

pub(crate) fn paint_row_gutter(
    ui: &egui::Ui,
    rect: egui::Rect,
    line_idx: usize,
    line_text: &str,
    folded_here: bool,
    foldable_here: bool,
    row: &EditorRowContext<'_>,
    row_hovered: bool,
) {
    if !row_gutter_paint_geometry_valid(rect, row.row_height) {
        return;
    }

    let line_number = line_idx.saturating_add(1);
    let row_height = gutter_non_negative_finite(row.row_height);
    let gutter_width = gutter_non_negative_finite(row.gutter_width);
    let font_size = gutter_font_size(row.font_size);
    let is_final_newline_line = row_gutter_is_final_newline_line(row.buffer, line_idx);
    let painter = ui.painter();
    let diff_indicator = diff_indicator_label(
        row.diff_render_indicators,
        row.diff_patch_actions,
        line_text,
    );
    let diff_actions = diff_gutter_action_labels(
        row.diff_stage,
        row.diff_render_gutter_menu,
        row.diff_render_margin_revert_icon,
        line_text,
        row_hovered,
    );
    if row.glyph_margin {
        let visible_change_kind = visible_line_change_kind(
            row.show_scm_diff_gutter,
            row.scm_diff_decorations_gutter_visibility,
            row_hovered,
            row.diff_lines.get(&line_number).copied(),
        );
        paint_line_change_marker(
            painter,
            rect,
            row.row_height,
            row.scm_diff_decorations_gutter_width,
            visible_change_kind,
            row.scm_diff_decorations_gutter_pattern,
            if row.background_image_active {
                Color32::TRANSPARENT
            } else {
                ui.visuals().code_bg_color
            },
        );

        if let Some(severity) = row.diagnostics_by_line.get(&line_number).copied() {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    pos2(rect.left() + 5.0, rect.top()),
                    vec2(3.0, row_height),
                ),
                0.0,
                diagnostic_color(severity),
            );
        }

        if code_action_marker_visible(
            row.lightbulb,
            row.glyph_margin,
            row.diagnostics_by_line.contains_key(&line_number),
            row_hovered,
            line_text,
        ) {
            painter.text(
                pos2(
                    rect.left() + CODE_ACTION_MARKER_LEFT + 1.0,
                    rect.top() + 3.0,
                ),
                egui::Align2::LEFT_TOP,
                "*",
                FontId::new((font_size * 0.9).max(8.0), FontFamily::Monospace),
                Color32::from_rgb(236, 196, 94),
            );
        }
    }

    if let Some(indicator) = diff_indicator {
        painter.text(
            pos2(rect.left() + 4.0, rect.top() + 3.0),
            egui::Align2::LEFT_TOP,
            indicator,
            FontId::new((font_size * 0.9).max(8.0), FontFamily::Monospace),
            diff_indicator_color(indicator),
        );
    }

    if !diff_actions.is_empty() {
        paint_diff_gutter_actions(painter, rect, row, diff_actions);
    } else if final_newline_line_number_visible(row.render_final_newline, is_final_newline_line)
        && let Some(label) = line_number_label(row.line_numbers, line_idx, row.cursor_positions)
    {
        let line_number_x = if row.glyph_margin || diff_indicator.is_some() {
            16.0
        } else {
            8.0
        };
        let color = line_number_color(
            ui.visuals().widgets.inactive.fg_stroke.color,
            row.render_final_newline,
            is_final_newline_line,
        );
        painter.text(
            pos2(rect.left() + line_number_x, rect.top() + 3.0),
            egui::Align2::LEFT_TOP,
            label,
            FontId::new(font_size, FontFamily::Monospace),
            color,
        );
    }

    let fold_marker = fold_marker_label(
        row.folding,
        row.show_folding_controls,
        row_hovered,
        folded_here,
        foldable_here,
    );
    if let Some(marker) = fold_marker {
        painter.text(
            pos2(
                rect.left() + (gutter_width - 16.0).max(12.0),
                rect.top() + 3.0,
            ),
            egui::Align2::LEFT_TOP,
            marker,
            FontId::new(font_size, FontFamily::Monospace),
            ui.visuals().widgets.inactive.fg_stroke.color,
        );
    }
}

pub(crate) fn diff_indicator_label(
    render_indicators: bool,
    diff_patch_actions: bool,
    line_text: &str,
) -> Option<&'static str> {
    if !render_indicators || !diff_patch_actions {
        return None;
    }
    match line_text.as_bytes() {
        [b'+', b'+', b'+', ..] | [b'-', b'-', b'-', ..] => None,
        [b'+', ..] => Some("+"),
        [b'-', ..] => Some("-"),
        _ => None,
    }
}

fn diff_indicator_color(indicator: &str) -> Color32 {
    match indicator {
        "+" => Color32::from_rgb(116, 199, 154),
        "-" => Color32::from_rgb(232, 98, 98),
        _ => Color32::TRANSPARENT,
    }
}

fn paint_diff_gutter_actions(
    painter: &egui::Painter,
    rect: egui::Rect,
    row: &EditorRowContext<'_>,
    actions: &[(DiffGutterAction, &'static str)],
) {
    let font_size = gutter_font_size(row.font_size);
    for ((action, label), action_rect) in actions.iter().zip(diff_gutter_action_rects(
        rect,
        row.row_height,
        row.gutter_width,
        actions.len(),
    )) {
        painter.text(
            action_rect.center(),
            egui::Align2::CENTER_CENTER,
            *label,
            FontId::new((font_size * 0.82).max(8.0), FontFamily::Monospace),
            diff_gutter_action_color(*action),
        );
    }
}

fn diff_gutter_action_color(action: DiffGutterAction) -> Color32 {
    match action {
        DiffGutterAction::Stage => Color32::from_rgb(116, 199, 154),
        DiffGutterAction::Unstage => Color32::from_rgb(91, 141, 239),
        DiffGutterAction::Discard => Color32::from_rgb(232, 98, 98),
    }
}

pub(crate) fn diff_gutter_action_labels(
    stage: Option<GitChangeStage>,
    render_gutter_menu: bool,
    render_margin_revert_icon: bool,
    line_text: &str,
    row_hovered: bool,
) -> &'static [(DiffGutterAction, &'static str)] {
    if !render_gutter_menu || !row_hovered || !diff_line_can_show_gutter_action(line_text) {
        return EMPTY_DIFF_GUTTER_ACTIONS;
    }

    match stage {
        Some(GitChangeStage::Unstaged) if render_margin_revert_icon => {
            STAGE_DISCARD_DIFF_GUTTER_ACTIONS
        }
        Some(GitChangeStage::Unstaged) => STAGE_DIFF_GUTTER_ACTIONS,
        Some(GitChangeStage::Staged) => UNSTAGE_DIFF_GUTTER_ACTIONS,
        None => EMPTY_DIFF_GUTTER_ACTIONS,
    }
}

pub(crate) fn diff_gutter_action_hit(
    rect: egui::Rect,
    row_height: f32,
    gutter_width: f32,
    pos: egui::Pos2,
    stage: Option<GitChangeStage>,
    render_gutter_menu: bool,
    render_margin_revert_icon: bool,
    line_text: &str,
    row_hovered: bool,
) -> Option<DiffGutterAction> {
    let actions = diff_gutter_action_labels(
        stage,
        render_gutter_menu,
        render_margin_revert_icon,
        line_text,
        row_hovered,
    );
    let action_count = actions.len();
    if action_count == 0 {
        return None;
    }

    actions
        .iter()
        .zip(diff_gutter_action_rects(
            rect,
            row_height,
            gutter_width,
            action_count,
        ))
        .find_map(|((action, _), action_rect)| action_rect.contains(pos).then_some(*action))
}

fn diff_gutter_action_rects(
    rect: egui::Rect,
    row_height: f32,
    gutter_width: f32,
    action_count: usize,
) -> impl Iterator<Item = egui::Rect> {
    let row_height = gutter_non_negative_finite(row_height);
    let action_count = if action_count == 0
        || row_height <= 0.0
        || !gutter_width.is_finite()
        || !rect.left().is_finite()
        || !rect.top().is_finite()
    {
        0
    } else {
        action_count
    };
    let gutter_width = gutter_width.max(0.0);
    let total_width = action_count as f32 * DIFF_GUTTER_ACTION_WIDTH
        + action_count.saturating_sub(1) as f32 * DIFF_GUTTER_ACTION_GAP;
    let left = rect.left() + (gutter_width - total_width - 4.0).max(4.0);
    (0..action_count).map(move |index| {
        egui::Rect::from_min_size(
            pos2(
                left + index as f32 * (DIFF_GUTTER_ACTION_WIDTH + DIFF_GUTTER_ACTION_GAP),
                rect.top(),
            ),
            vec2(DIFF_GUTTER_ACTION_WIDTH, row_height),
        )
    })
}

fn diff_line_can_show_gutter_action(line_text: &str) -> bool {
    match line_text.as_bytes() {
        [b'@', b'@', b' ', ..] | [b'@', b'@', b'@', b' ', ..] => true,
        [b'+', b'+', b'+', ..] | [b'-', b'-', b'-', ..] => false,
        [b'+', ..] | [b'-', ..] => true,
        _ => false,
    }
}

pub(crate) fn line_change_marker_color(
    kind: Option<GitLineChangeKind>,
    fallback: Color32,
) -> Color32 {
    match kind {
        Some(GitLineChangeKind::Added) => Color32::from_rgb(76, 175, 80),
        Some(GitLineChangeKind::Modified) => Color32::from_rgb(91, 141, 239),
        Some(GitLineChangeKind::Deleted) => Color32::from_rgb(232, 98, 98),
        None => fallback,
    }
}

fn paint_line_change_marker(
    painter: &egui::Painter,
    rect: egui::Rect,
    row_height: f32,
    marker_width: usize,
    kind: Option<GitLineChangeKind>,
    pattern: ScmDiffDecorationsGutterPattern,
    fallback_color: Color32,
) {
    let marker_rect = line_change_marker_rect(rect, row_height, marker_width);
    if marker_rect.height() <= 0.0 {
        return;
    }

    let color = line_change_marker_color(kind, fallback_color);
    if color == Color32::TRANSPARENT {
        return;
    }
    if let Some(kind) = kind
        && line_change_marker_uses_pattern(kind, pattern)
    {
        paint_line_change_marker_pattern(painter, marker_rect, color);
        return;
    }
    painter.rect_filled(marker_rect, 0.0, color);
}

fn paint_line_change_marker_pattern(painter: &egui::Painter, rect: Rect, color: Color32) {
    let stripe_height = 2.0;
    let stripe_gap = 2.0;
    let mut y = rect.top();
    while y < rect.bottom() {
        let bottom = (y + stripe_height).min(rect.bottom());
        painter.rect_filled(
            Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right(), bottom)),
            0.0,
            color,
        );
        y += stripe_height + stripe_gap;
    }
}

pub(crate) fn line_change_marker_rect(
    rect: egui::Rect,
    row_height: f32,
    marker_width: usize,
) -> egui::Rect {
    let marker_height = if rect.left().is_finite() && rect.top().is_finite() {
        gutter_non_negative_finite(row_height)
    } else {
        0.0
    };
    let marker_width = marker_width.max(1) as f32;
    egui::Rect::from_min_size(
        pos2(
            gutter_finite_or_zero(rect.left()),
            gutter_finite_or_zero(rect.top()),
        ),
        vec2(marker_width, marker_height),
    )
}

pub(crate) fn visible_line_change_kind(
    show_scm_diff_gutter: bool,
    visibility: ScmDiffDecorationsGutterVisibility,
    row_hovered: bool,
    change_kind: Option<GitLineChangeKind>,
) -> Option<GitLineChangeKind> {
    if !show_scm_diff_gutter {
        return None;
    }

    match visibility {
        ScmDiffDecorationsGutterVisibility::Always => change_kind,
        ScmDiffDecorationsGutterVisibility::Hover => row_hovered.then_some(change_kind).flatten(),
    }
}

pub(crate) fn line_change_marker_uses_pattern(
    kind: GitLineChangeKind,
    pattern: ScmDiffDecorationsGutterPattern,
) -> bool {
    match kind {
        GitLineChangeKind::Added => pattern.added,
        GitLineChangeKind::Modified => pattern.modified,
        GitLineChangeKind::Deleted => false,
    }
}

pub(crate) fn line_change_marker_hit(
    rect: egui::Rect,
    row_height: f32,
    pos: egui::Pos2,
    glyph_margin: bool,
    change_kind: Option<GitLineChangeKind>,
    marker_width: usize,
    gutter_action: ScmDiffDecorationsGutterAction,
) -> bool {
    let marker_rect = line_change_marker_rect(rect, row_height, marker_width);
    glyph_margin
        && pos.x.is_finite()
        && pos.y.is_finite()
        && change_kind.is_some()
        && gutter_action == ScmDiffDecorationsGutterAction::Diff
        && marker_rect.height() > 0.0
        && marker_rect.contains(pos)
}

pub(crate) fn code_action_marker_visible(
    mode: EditorLightbulbMode,
    glyph_margin: bool,
    has_diagnostic: bool,
    row_hovered: bool,
    line_text: &str,
) -> bool {
    if !glyph_margin || !row_hovered {
        return false;
    }

    match mode {
        EditorLightbulbMode::Off => false,
        EditorLightbulbMode::On => true,
        EditorLightbulbMode::OnCode => {
            has_diagnostic || line_text.chars().any(|ch| !ch.is_whitespace())
        }
    }
}

pub(crate) fn code_action_marker_rect(rect: egui::Rect, row_height: f32) -> egui::Rect {
    let marker_height = if rect.left().is_finite() && rect.top().is_finite() {
        gutter_non_negative_finite(row_height)
    } else {
        0.0
    };
    egui::Rect::from_min_size(
        pos2(
            gutter_finite_or_zero(rect.left()) + CODE_ACTION_MARKER_LEFT,
            gutter_finite_or_zero(rect.top()),
        ),
        vec2(CODE_ACTION_MARKER_WIDTH, marker_height),
    )
}

pub(crate) fn code_action_marker_hit(
    rect: egui::Rect,
    row_height: f32,
    pos: egui::Pos2,
    mode: EditorLightbulbMode,
    glyph_margin: bool,
    has_diagnostic: bool,
    row_hovered: bool,
    line_text: &str,
) -> bool {
    code_action_marker_visible(mode, glyph_margin, has_diagnostic, row_hovered, line_text)
        && pos.x.is_finite()
        && pos.y.is_finite()
        && code_action_marker_rect(rect, row_height).contains(pos)
}

pub(crate) fn fold_marker_label(
    folding: bool,
    controls: EditorShowFoldingControls,
    row_hovered: bool,
    folded_here: bool,
    foldable_here: bool,
) -> Option<&'static str> {
    if !folding {
        return None;
    }

    let visible = match controls {
        EditorShowFoldingControls::Always => true,
        EditorShowFoldingControls::Never => false,
        EditorShowFoldingControls::Mouseover => row_hovered || folded_here,
    };
    if !visible {
        return None;
    }

    if folded_here {
        Some("+")
    } else if foldable_here {
        Some("-")
    } else {
        None
    }
}

pub(crate) fn line_number_label(
    mode: EditorLineNumbers,
    line_idx: usize,
    cursor_positions: &[CursorPosition],
) -> Option<String> {
    let line_number = line_idx.saturating_add(1);
    match mode {
        EditorLineNumbers::On => Some(format!("{line_number:>4}")),
        EditorLineNumbers::Off => None,
        EditorLineNumbers::Relative => {
            let cursor_line = cursor_positions
                .first()
                .map(|cursor| cursor.line.saturating_add(1))
                .unwrap_or(line_number);
            if cursor_line == line_number {
                Some(format!("{line_number:>4}"))
            } else {
                Some(format!("{:>4}", cursor_line.abs_diff(line_number)))
            }
        }
        EditorLineNumbers::Interval => {
            if line_number == 1 || line_number.is_multiple_of(10) {
                Some(format!("{line_number:>4}"))
            } else {
                None
            }
        }
    }
}

fn gutter_non_negative_finite(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

fn gutter_finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn gutter_font_size(value: f32) -> f32 {
    clamp_editor_font_size(value, 13.0)
}

fn row_gutter_paint_geometry_valid(rect: Rect, row_height: f32) -> bool {
    rect.left().is_finite()
        && rect.right().is_finite()
        && rect.top().is_finite()
        && rect.bottom().is_finite()
        && row_height.is_finite()
        && row_height > 0.0
}

fn row_gutter_is_final_newline_line(buffer: &TextBuffer, line_idx: usize) -> bool {
    line_idx != usize::MAX && buffer.is_final_newline_line(line_idx)
}

pub(crate) fn final_newline_line_number_visible(
    mode: EditorRenderFinalNewline,
    is_final_newline_line: bool,
) -> bool {
    !is_final_newline_line || mode != EditorRenderFinalNewline::Off
}

pub(crate) fn line_number_color(
    base: Color32,
    mode: EditorRenderFinalNewline,
    is_final_newline_line: bool,
) -> Color32 {
    if is_final_newline_line && mode == EditorRenderFinalNewline::Dimmed {
        base.gamma_multiply(0.55)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DiffGutterAction, code_action_marker_hit, code_action_marker_rect,
        code_action_marker_visible, diff_gutter_action_hit, diff_gutter_action_labels,
        diff_indicator_label, final_newline_line_number_visible, fold_marker_label,
        gutter_font_size, line_change_marker_color, line_change_marker_hit,
        line_change_marker_rect, line_change_marker_uses_pattern, line_number_color,
        line_number_label, row_gutter_is_final_newline_line, row_gutter_paint_geometry_valid,
        visible_line_change_kind,
    };
    use eframe::egui::{Color32, Rect, pos2, vec2};
    use kuroya_core::{
        EditorLightbulbMode, EditorLineNumbers, EditorRenderFinalNewline,
        EditorShowFoldingControls, GitChangeStage, GitLineChangeKind,
        ScmDiffDecorationsGutterAction, ScmDiffDecorationsGutterPattern,
        ScmDiffDecorationsGutterVisibility, TextBuffer, buffer::CursorPosition,
    };

    #[test]
    fn line_number_labels_follow_configured_mode() {
        let cursors = [CursorPosition {
            line: 9,
            column: 0,
            char_idx: 0,
        }];

        assert_eq!(
            line_number_label(EditorLineNumbers::On, 4, &cursors),
            Some("   5".to_owned())
        );
        assert_eq!(line_number_label(EditorLineNumbers::Off, 4, &cursors), None);
        assert_eq!(
            line_number_label(EditorLineNumbers::Relative, 4, &cursors),
            Some("   5".to_owned())
        );
        assert_eq!(
            line_number_label(EditorLineNumbers::Interval, 9, &cursors),
            Some("  10".to_owned())
        );
        assert_eq!(
            line_number_label(EditorLineNumbers::Interval, 8, &cursors),
            None
        );

        assert_eq!(
            line_number_label(EditorLineNumbers::On, usize::MAX, &cursors),
            Some(format!("{:>4}", usize::MAX))
        );
        let huge_cursor = [CursorPosition {
            line: usize::MAX,
            column: 0,
            char_idx: 0,
        }];
        assert_eq!(
            line_number_label(EditorLineNumbers::Relative, 0, &huge_cursor),
            Some(format!("{:>4}", usize::MAX - 1))
        );
    }

    #[test]
    fn row_gutter_paint_geometry_rejects_non_finite_dimensions() {
        let row_rect = Rect::from_min_size(pos2(40.0, 20.0), vec2(500.0, 18.0));

        assert!(row_gutter_paint_geometry_valid(row_rect, 18.0));
        assert!(!row_gutter_paint_geometry_valid(row_rect, f32::NAN));
        assert!(!row_gutter_paint_geometry_valid(row_rect, 0.0));
        assert!(!row_gutter_paint_geometry_valid(
            Rect::from_min_size(pos2(f32::NAN, 20.0), vec2(500.0, 18.0)),
            18.0
        ));
        assert!(!row_gutter_paint_geometry_valid(
            Rect::from_min_size(pos2(40.0, 20.0), vec2(f32::INFINITY, 18.0)),
            18.0
        ));
        assert!(!row_gutter_paint_geometry_valid(
            Rect::from_min_size(pos2(40.0, 20.0), vec2(500.0, f32::INFINITY)),
            18.0
        ));
    }

    #[test]
    fn gutter_font_size_uses_finite_clamped_values() {
        assert_eq!(gutter_font_size(f32::NAN), 13.0);
        assert_eq!(gutter_font_size(0.0), 13.0);
        assert_eq!(
            gutter_font_size(f32::MAX),
            kuroya_core::settings::MAX_EDITOR_FONT_SIZE
        );
    }

    #[test]
    fn final_newline_line_numbers_follow_render_mode() {
        assert!(!final_newline_line_number_visible(
            EditorRenderFinalNewline::Off,
            true
        ));
        assert!(final_newline_line_number_visible(
            EditorRenderFinalNewline::On,
            true
        ));
        assert!(final_newline_line_number_visible(
            EditorRenderFinalNewline::Dimmed,
            true
        ));
        assert!(final_newline_line_number_visible(
            EditorRenderFinalNewline::Off,
            false
        ));

        let base = Color32::from_rgb(200, 180, 160);
        assert_eq!(
            line_number_color(base, EditorRenderFinalNewline::On, true),
            base
        );
        assert_eq!(
            line_number_color(base, EditorRenderFinalNewline::Off, false),
            base
        );
        assert_ne!(
            line_number_color(base, EditorRenderFinalNewline::Dimmed, true),
            base
        );

        let buffer = TextBuffer::from_text(1, None, "a\n".to_owned());
        assert!(row_gutter_is_final_newline_line(&buffer, 1));
        assert!(!row_gutter_is_final_newline_line(&buffer, usize::MAX));
    }

    #[test]
    fn fold_marker_labels_follow_visibility_mode() {
        assert_eq!(
            fold_marker_label(true, EditorShowFoldingControls::Always, false, false, true),
            Some("-")
        );
        assert_eq!(
            fold_marker_label(true, EditorShowFoldingControls::Never, true, true, true),
            None
        );
        assert_eq!(
            fold_marker_label(
                true,
                EditorShowFoldingControls::Mouseover,
                false,
                false,
                true
            ),
            None
        );
        assert_eq!(
            fold_marker_label(
                true,
                EditorShowFoldingControls::Mouseover,
                true,
                false,
                true
            ),
            Some("-")
        );
        assert_eq!(
            fold_marker_label(
                true,
                EditorShowFoldingControls::Mouseover,
                false,
                true,
                true
            ),
            Some("+")
        );
    }

    #[test]
    fn code_action_marker_visibility_follows_lightbulb_mode() {
        assert!(code_action_marker_visible(
            EditorLightbulbMode::OnCode,
            true,
            true,
            true,
            ""
        ));
        assert!(code_action_marker_visible(
            EditorLightbulbMode::OnCode,
            true,
            false,
            true,
            "let x = 1;"
        ));
        assert!(!code_action_marker_visible(
            EditorLightbulbMode::OnCode,
            true,
            false,
            true,
            "   "
        ));
        assert!(code_action_marker_visible(
            EditorLightbulbMode::On,
            true,
            false,
            true,
            "   "
        ));
        assert!(!code_action_marker_visible(
            EditorLightbulbMode::Off,
            true,
            true,
            true,
            "let x = 1;"
        ));
        assert!(!code_action_marker_visible(
            EditorLightbulbMode::On,
            false,
            true,
            true,
            "let x = 1;"
        ));
        assert!(!code_action_marker_visible(
            EditorLightbulbMode::On,
            true,
            true,
            false,
            "let x = 1;"
        ));
    }

    #[test]
    fn code_action_marker_hit_uses_dedicated_gutter_zone() {
        let row_rect = Rect::from_min_size(pos2(40.0, 20.0), vec2(500.0, 18.0));
        let marker_rect = code_action_marker_rect(row_rect, 18.0);

        assert!(marker_rect.contains(pos2(49.0, 24.0)));
        assert!(code_action_marker_hit(
            row_rect,
            18.0,
            pos2(49.0, 24.0),
            EditorLightbulbMode::OnCode,
            true,
            true,
            true,
            ""
        ));
        assert!(!code_action_marker_hit(
            row_rect,
            18.0,
            pos2(57.0, 24.0),
            EditorLightbulbMode::OnCode,
            true,
            true,
            true,
            ""
        ));
        assert!(!code_action_marker_hit(
            row_rect,
            18.0,
            pos2(49.0, 24.0),
            EditorLightbulbMode::OnCode,
            false,
            true,
            true,
            ""
        ));
        assert!(!code_action_marker_hit(
            row_rect,
            18.0,
            pos2(49.0, 24.0),
            EditorLightbulbMode::OnCode,
            true,
            false,
            true,
            ""
        ));
        assert!(!code_action_marker_hit(
            row_rect,
            18.0,
            pos2(49.0, 24.0),
            EditorLightbulbMode::Off,
            true,
            true,
            true,
            "let x = 1;"
        ));

        assert_eq!(code_action_marker_rect(row_rect, f32::NAN).height(), 0.0);
        assert!(!code_action_marker_hit(
            row_rect,
            f32::NAN,
            pos2(49.0, 24.0),
            EditorLightbulbMode::OnCode,
            true,
            true,
            true,
            ""
        ));

        let invalid_rect = Rect::from_min_size(pos2(f32::NAN, 20.0), vec2(500.0, 18.0));
        assert_eq!(code_action_marker_rect(invalid_rect, 18.0).height(), 0.0);
    }

    #[test]
    fn line_change_marker_hit_follows_glyph_margin_change_and_setting() {
        let row_rect = Rect::from_min_size(pos2(40.0, 20.0), vec2(500.0, 18.0));
        assert!(line_change_marker_hit(
            row_rect,
            18.0,
            pos2(41.0, 25.0),
            true,
            Some(GitLineChangeKind::Modified),
            3,
            ScmDiffDecorationsGutterAction::Diff,
        ));
        assert!(!line_change_marker_hit(
            row_rect,
            18.0,
            pos2(45.0, 25.0),
            true,
            Some(GitLineChangeKind::Modified),
            3,
            ScmDiffDecorationsGutterAction::Diff,
        ));
        assert!(line_change_marker_hit(
            row_rect,
            18.0,
            pos2(43.0, 25.0),
            true,
            Some(GitLineChangeKind::Modified),
            4,
            ScmDiffDecorationsGutterAction::Diff,
        ));
        assert!(!line_change_marker_hit(
            row_rect,
            18.0,
            pos2(41.0, 25.0),
            false,
            Some(GitLineChangeKind::Modified),
            3,
            ScmDiffDecorationsGutterAction::Diff,
        ));
        assert!(!line_change_marker_hit(
            row_rect,
            18.0,
            pos2(41.0, 25.0),
            true,
            None,
            3,
            ScmDiffDecorationsGutterAction::Diff,
        ));
        assert!(!line_change_marker_hit(
            row_rect,
            18.0,
            pos2(41.0, 25.0),
            true,
            Some(GitLineChangeKind::Modified),
            3,
            ScmDiffDecorationsGutterAction::None,
        ));
    }

    #[test]
    fn line_change_marker_geometry_clamps_invalid_dimensions() {
        let row_rect = Rect::from_min_size(pos2(40.0, 20.0), vec2(500.0, 18.0));

        let zero_width = line_change_marker_rect(row_rect, 18.0, 0);
        assert_eq!(zero_width.width(), 1.0);
        assert_eq!(zero_width.height(), 18.0);

        let negative_height = line_change_marker_rect(row_rect, -18.0, 4);
        assert_eq!(negative_height.width(), 4.0);
        assert_eq!(negative_height.height(), 0.0);

        let nan_height = line_change_marker_rect(row_rect, f32::NAN, 4);
        assert_eq!(nan_height.height(), 0.0);

        let invalid_rect = Rect::from_min_size(pos2(f32::NAN, 20.0), vec2(500.0, 18.0));
        let invalid_marker = line_change_marker_rect(invalid_rect, 18.0, 4);
        assert_eq!(invalid_marker.height(), 0.0);

        assert!(!line_change_marker_hit(
            row_rect,
            f32::NAN,
            pos2(41.0, 20.0),
            true,
            Some(GitLineChangeKind::Modified),
            4,
            ScmDiffDecorationsGutterAction::Diff,
        ));
        assert!(!line_change_marker_hit(
            invalid_rect,
            18.0,
            pos2(41.0, 20.0),
            true,
            Some(GitLineChangeKind::Modified),
            4,
            ScmDiffDecorationsGutterAction::Diff,
        ));
    }

    #[test]
    fn visible_line_change_kind_follows_surface_visibility_and_hover() {
        let change = Some(GitLineChangeKind::Modified);
        assert_eq!(
            visible_line_change_kind(
                true,
                ScmDiffDecorationsGutterVisibility::Always,
                false,
                change
            ),
            change
        );
        assert_eq!(
            visible_line_change_kind(
                true,
                ScmDiffDecorationsGutterVisibility::Hover,
                false,
                change
            ),
            None
        );
        assert_eq!(
            visible_line_change_kind(
                true,
                ScmDiffDecorationsGutterVisibility::Hover,
                true,
                change
            ),
            change
        );
        assert_eq!(
            visible_line_change_kind(
                false,
                ScmDiffDecorationsGutterVisibility::Always,
                true,
                change
            ),
            None
        );
    }

    #[test]
    fn line_change_marker_patterns_follow_added_and_modified_settings() {
        let pattern = ScmDiffDecorationsGutterPattern {
            added: true,
            modified: false,
        };

        assert!(line_change_marker_uses_pattern(
            GitLineChangeKind::Added,
            pattern
        ));
        assert!(!line_change_marker_uses_pattern(
            GitLineChangeKind::Modified,
            pattern
        ));
        assert!(!line_change_marker_uses_pattern(
            GitLineChangeKind::Deleted,
            pattern
        ));
        assert!(line_change_marker_uses_pattern(
            GitLineChangeKind::Modified,
            ScmDiffDecorationsGutterPattern::default()
        ));
    }

    #[test]
    fn diff_gutter_action_labels_follow_stage_line_hover_and_setting() {
        assert_eq!(
            diff_gutter_action_labels(Some(GitChangeStage::Unstaged), true, true, "+new", true),
            &[
                (DiffGutterAction::Stage, "S"),
                (DiffGutterAction::Discard, "D")
            ]
        );
        assert_eq!(
            diff_gutter_action_labels(Some(GitChangeStage::Unstaged), true, false, "+new", true),
            &[(DiffGutterAction::Stage, "S")]
        );
        assert_eq!(
            diff_gutter_action_labels(
                Some(GitChangeStage::Staged),
                true,
                false,
                "@@ -1 +1 @@",
                true
            ),
            &[(DiffGutterAction::Unstage, "U")]
        );
        assert!(
            diff_gutter_action_labels(Some(GitChangeStage::Unstaged), false, true, "+new", true)
                .is_empty()
        );
        assert!(
            diff_gutter_action_labels(Some(GitChangeStage::Unstaged), true, true, "+new", false)
                .is_empty()
        );
        assert!(
            diff_gutter_action_labels(
                Some(GitChangeStage::Unstaged),
                true,
                true,
                "+++ b/main.rs",
                true
            )
            .is_empty()
        );
        assert!(diff_gutter_action_labels(None, true, true, "+new", true).is_empty());
    }

    #[test]
    fn diff_indicator_labels_follow_diff_line_and_setting() {
        assert_eq!(diff_indicator_label(true, true, "+new"), Some("+"));
        assert_eq!(diff_indicator_label(true, true, "-old"), Some("-"));
        assert_eq!(diff_indicator_label(true, true, " context"), None);
        assert_eq!(diff_indicator_label(true, true, "+++ b/main.rs"), None);
        assert_eq!(diff_indicator_label(true, true, "--- a/main.rs"), None);
        assert_eq!(diff_indicator_label(false, true, "+new"), None);
        assert_eq!(diff_indicator_label(true, false, "-old"), None);
    }

    #[test]
    fn diff_gutter_action_hit_uses_action_cells_inside_gutter() {
        let row_rect = Rect::from_min_size(pos2(40.0, 20.0), vec2(500.0, 18.0));
        assert_eq!(
            diff_gutter_action_hit(
                row_rect,
                18.0,
                84.0,
                pos2(94.0, 25.0),
                Some(GitChangeStage::Unstaged),
                true,
                true,
                "-old",
                true,
            ),
            Some(DiffGutterAction::Stage)
        );
        assert_eq!(
            diff_gutter_action_hit(
                row_rect,
                18.0,
                84.0,
                pos2(109.0, 25.0),
                Some(GitChangeStage::Unstaged),
                true,
                true,
                "-old",
                true,
            ),
            Some(DiffGutterAction::Discard)
        );
        assert_eq!(
            diff_gutter_action_hit(
                row_rect,
                18.0,
                84.0,
                pos2(109.0, 25.0),
                Some(GitChangeStage::Staged),
                true,
                true,
                "@@ -1 +1 @@",
                true,
            ),
            Some(DiffGutterAction::Unstage)
        );
        assert_eq!(
            diff_gutter_action_hit(
                row_rect,
                18.0,
                84.0,
                pos2(30.0, 25.0),
                Some(GitChangeStage::Unstaged),
                true,
                true,
                "-old",
                true,
            ),
            None
        );
        assert_eq!(
            diff_gutter_action_hit(
                row_rect,
                18.0,
                84.0,
                pos2(109.0, 25.0),
                Some(GitChangeStage::Unstaged),
                true,
                false,
                "-old",
                true,
            ),
            Some(DiffGutterAction::Stage)
        );
        assert_eq!(
            diff_gutter_action_hit(
                row_rect,
                f32::NAN,
                84.0,
                pos2(94.0, 25.0),
                Some(GitChangeStage::Unstaged),
                true,
                true,
                "-old",
                true,
            ),
            None
        );
        assert_eq!(
            diff_gutter_action_hit(
                row_rect,
                18.0,
                f32::NAN,
                pos2(94.0, 25.0),
                Some(GitChangeStage::Unstaged),
                true,
                true,
                "-old",
                true,
            ),
            None
        );
    }

    #[test]
    fn line_change_marker_colors_distinguish_git_change_kinds() {
        assert_eq!(
            line_change_marker_color(
                Some(GitLineChangeKind::Added),
                Color32::from_rgb(35, 39, 46)
            ),
            Color32::from_rgb(76, 175, 80)
        );
        assert_eq!(
            line_change_marker_color(
                Some(GitLineChangeKind::Modified),
                Color32::from_rgb(35, 39, 46)
            ),
            Color32::from_rgb(91, 141, 239)
        );
        assert_eq!(
            line_change_marker_color(
                Some(GitLineChangeKind::Deleted),
                Color32::from_rgb(35, 39, 46)
            ),
            Color32::from_rgb(232, 98, 98)
        );
        assert_eq!(
            line_change_marker_color(None, Color32::from_rgb(220, 225, 232)),
            Color32::from_rgb(220, 225, 232)
        );
    }
}

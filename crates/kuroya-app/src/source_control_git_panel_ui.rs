use eframe::egui::{
    Color32, Context, FontId, Rect, Response, Sense, Stroke, StrokeKind, TextStyle, Ui, pos2, vec2,
};

pub(crate) const SOURCE_CONTROL_GIT_ROW_HEIGHT: f32 = 24.0;
pub(crate) const SOURCE_CONTROL_GIT_STASH_PANEL_DEFAULT_SIZE: [f32; 2] = [520.0, 320.0];
pub(crate) const SOURCE_CONTROL_GIT_HUNK_PANEL_DEFAULT_SIZE: [f32; 2] = [520.0, 320.0];

const GIT_PANEL_LIST_CHROME_HEIGHT: f32 = 150.0;

const GIT_PANEL_ROW_MIN_WIDTH: f32 = 180.0;
const GIT_PANEL_ROW_CORNER_RADIUS: f32 = 4.0;
const GIT_PANEL_ROW_HORIZONTAL_PADDING: f32 = 8.0;
const GIT_PANEL_ROW_BADGE_WIDTH: f32 = 76.0;
const GIT_PANEL_ROW_GAP: f32 = 10.0;
const GIT_PANEL_ROW_MAX_DETAIL_RATIO: f32 = 0.36;

pub(crate) fn apply_git_panel_spacing(ui: &mut Ui) {
    let spacing = ui.spacing_mut();
    spacing.item_spacing = vec2(6.0, 5.0);
    spacing.button_padding = vec2(8.0, 3.0);
}

pub(crate) fn git_panel_list_max_height(ctx: &Context, anchor_offset: f32) -> f32 {
    (crate::layout::popup_window_max_size_with_top_margin(ctx, anchor_offset).y
        - GIT_PANEL_LIST_CHROME_HEIGHT)
        .max(120.0)
}

pub(crate) fn render_git_panel_row(
    ui: &mut Ui,
    selected: bool,
    badge: &str,
    primary: &str,
    detail: &str,
) -> Response {
    let width = ui.available_width().max(GIT_PANEL_ROW_MIN_WIDTH);
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, SOURCE_CONTROL_GIT_ROW_HEIGHT), Sense::click());
    paint_git_panel_row_background(ui, rect, &response, selected);

    let visuals = ui.visuals();
    let center_y = rect.center().y;
    let badge_color = if selected {
        visuals.selection.stroke.color
    } else {
        visuals.weak_text_color()
    };
    let primary_color = visuals.widgets.inactive.fg_stroke.color;
    let detail_color = visuals.weak_text_color();

    let badge_font = FontId::monospace(11.0);
    let primary_font = TextStyle::Body.resolve(ui.style());
    let detail_font = TextStyle::Small.resolve(ui.style());
    let badge_galley =
        ui.fonts_mut(|fonts| fonts.layout_no_wrap(badge.to_owned(), badge_font, badge_color));
    let detail_galley = (!detail.is_empty()).then(|| {
        ui.fonts_mut(|fonts| {
            fonts.layout_no_wrap(detail.to_owned(), detail_font.clone(), detail_color)
        })
    });
    let primary_galley =
        ui.fonts_mut(|fonts| fonts.layout_no_wrap(primary.to_owned(), primary_font, primary_color));

    let text_left = rect.left() + GIT_PANEL_ROW_HORIZONTAL_PADDING;
    let text_right = rect.right() - GIT_PANEL_ROW_HORIZONTAL_PADDING;
    let badge_clip = Rect::from_min_max(
        pos2(text_left, rect.top()),
        pos2(
            (text_left + GIT_PANEL_ROW_BADGE_WIDTH).min(text_right),
            rect.bottom(),
        ),
    );
    ui.painter().with_clip_rect(badge_clip).galley(
        pos2(text_left, center_y - badge_galley.rect.height() / 2.0),
        badge_galley,
        badge_color,
    );

    let primary_left = (text_left + GIT_PANEL_ROW_BADGE_WIDTH + GIT_PANEL_ROW_GAP).min(text_right);
    let detail_width = detail_galley
        .as_ref()
        .map(|galley| git_panel_row_detail_width(width, galley.rect.width()))
        .unwrap_or_default();
    let detail_left = text_right - detail_width;
    let primary_right = if detail_width > 0.0 {
        (detail_left - GIT_PANEL_ROW_GAP).max(primary_left)
    } else {
        text_right
    };
    let primary_clip = Rect::from_min_max(
        pos2(primary_left, rect.top()),
        pos2(primary_right, rect.bottom()),
    );
    ui.painter().with_clip_rect(primary_clip).galley(
        pos2(primary_left, center_y - primary_galley.rect.height() / 2.0),
        primary_galley,
        primary_color,
    );

    if let Some(detail_galley) = detail_galley {
        let detail_clip = Rect::from_min_max(
            pos2(detail_left, rect.top()),
            pos2(text_right, rect.bottom()),
        );
        ui.painter().with_clip_rect(detail_clip).galley(
            pos2(detail_left, center_y - detail_galley.rect.height() / 2.0),
            detail_galley,
            detail_color,
        );
    }

    response
}

fn paint_git_panel_row_background(ui: &Ui, rect: Rect, response: &Response, selected: bool) {
    let visuals = ui.visuals();
    let fill = if response.is_pointer_button_down_on() {
        visuals.widgets.active.bg_fill
    } else if selected {
        visuals.widgets.active.weak_bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.bg_fill
    } else {
        Color32::TRANSPARENT
    };
    let row_rect = rect.shrink2(vec2(2.0, 1.0));
    if fill != Color32::TRANSPARENT {
        ui.painter()
            .rect_filled(row_rect, GIT_PANEL_ROW_CORNER_RADIUS, fill);
    }
    if selected || response.hovered() {
        ui.painter().rect_stroke(
            row_rect,
            GIT_PANEL_ROW_CORNER_RADIUS,
            Stroke::new(
                1.0,
                if selected {
                    visuals.widgets.active.bg_stroke.color
                } else {
                    visuals.widgets.hovered.bg_stroke.color
                },
            ),
            StrokeKind::Inside,
        );
    }
}

pub(crate) fn git_panel_row_detail_width(row_width: f32, measured_detail_width: f32) -> f32 {
    if !row_width.is_finite() || !measured_detail_width.is_finite() {
        return 0.0;
    }
    let row_width = row_width.max(0.0);
    let measured_detail_width = measured_detail_width.max(0.0);
    measured_detail_width.min(row_width * GIT_PANEL_ROW_MAX_DETAIL_RATIO)
}

#[cfg(test)]
mod tests {
    use super::git_panel_row_detail_width;

    #[test]
    fn git_panel_row_detail_width_keeps_primary_text_room() {
        assert_eq!(git_panel_row_detail_width(500.0, 400.0), 180.0);
        assert_eq!(git_panel_row_detail_width(500.0, 120.0), 120.0);
    }

    #[test]
    fn git_panel_row_detail_width_ignores_invalid_geometry() {
        assert_eq!(git_panel_row_detail_width(f32::NAN, 120.0), 0.0);
        assert_eq!(git_panel_row_detail_width(500.0, f32::INFINITY), 0.0);
        assert_eq!(git_panel_row_detail_width(-500.0, 120.0), 0.0);
        assert_eq!(git_panel_row_detail_width(500.0, -120.0), 0.0);
    }
}

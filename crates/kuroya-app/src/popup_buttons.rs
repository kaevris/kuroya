use eframe::egui::{self, Color32, Response, RichText, Stroke, Ui, vec2};

#[derive(Clone, Copy)]
pub(crate) enum PopupButtonKind {
    Secondary,
    Primary,
    Danger,
}

pub(crate) fn popup_button(
    ui: &mut Ui,
    label: impl Into<String>,
    kind: PopupButtonKind,
) -> Response {
    ui.add(popup_button_widget(ui, label, kind, 78.0))
}

pub(crate) fn popup_compact_button(
    ui: &mut Ui,
    label: impl Into<String>,
    kind: PopupButtonKind,
) -> Response {
    ui.add(popup_button_widget(ui, label, kind, 58.0))
}

pub(crate) fn popup_compact_button_enabled(
    ui: &mut Ui,
    enabled: bool,
    label: impl Into<String>,
    kind: PopupButtonKind,
) -> Response {
    ui.add_enabled(enabled, popup_button_widget(ui, label, kind, 58.0))
}

pub(crate) fn popup_button_enabled(
    ui: &mut Ui,
    enabled: bool,
    label: impl Into<String>,
    kind: PopupButtonKind,
) -> Response {
    ui.add_enabled(enabled, popup_button_widget(ui, label, kind, 78.0))
}

fn popup_button_widget(
    ui: &Ui,
    label: impl Into<String>,
    kind: PopupButtonKind,
    min_width: f32,
) -> egui::Button<'static> {
    let visuals = ui.visuals();
    let inactive = visuals.widgets.inactive;
    let active = visuals.widgets.active;
    let text_color = visuals.text_color();
    let error_color = visuals.error_fg_color;
    let (fill, stroke, text, strong) = match kind {
        PopupButtonKind::Secondary => (
            inactive.bg_fill,
            inactive.bg_stroke.color,
            text_color,
            false,
        ),
        PopupButtonKind::Primary => (
            active.bg_fill,
            visuals.selection.stroke.color,
            active.fg_stroke.color,
            true,
        ),
        PopupButtonKind::Danger => (
            mix_color(inactive.bg_fill, error_color, 0.10),
            error_color,
            error_color,
            true,
        ),
    };
    let mut text = RichText::new(label.into()).color(text);
    if strong {
        text = text.strong();
    }
    egui::Button::new(text)
        .min_size(vec2(min_width, 28.0))
        .fill(fill)
        .stroke(Stroke::new(1.0_f32, stroke))
        .corner_radius(egui::CornerRadius::same(4))
}

fn mix_color(base: Color32, overlay: Color32, amount: f32) -> Color32 {
    let amount = if amount.is_finite() {
        amount.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix = |base: u8, overlay: u8| base as f32 + ((overlay as f32 - base as f32) * amount);
    Color32::from_rgb(
        mix(base.r(), overlay.r()).round() as u8,
        mix(base.g(), overlay.g()).round() as u8,
        mix(base.b(), overlay.b()).round() as u8,
    )
}

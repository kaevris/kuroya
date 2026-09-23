use crate::{
    KuroyaApp,
    status_bar::{status_bar_message, status_message_is_persistent},
    ui_icons::IconKind,
};
use eframe::egui::{
    self, Align2, Area, Color32, Context, CornerRadius, CursorIcon, FontId, Id, Order, Rect, Sense,
    Stroke, Vec2,
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

const TOAST_TTL: Duration = Duration::from_secs(6);
const TOAST_ERROR_TTL: Duration = Duration::from_secs(30);
const TOAST_FADE_SECS: f32 = 0.4;
const MAX_TOASTS: usize = 5;
const TOAST_WIDTH: f32 = 340.0;
const TOAST_PADDING: f32 = 10.0;
const TOAST_GAP: f32 = 8.0;
const TOAST_ACCENT_WIDTH: f32 = 3.0;
const TOAST_ICON_SLOT: f32 = 24.0;
const TOAST_TEXT_FONT_SIZE: f32 = 13.5;

#[derive(Debug, Clone)]
pub(crate) struct StatusToast {
    pub(crate) id: u64,
    pub(crate) message: String,
    pub(crate) error: bool,
    pub(crate) created: Instant,
}

static NEXT_TOAST_ID: AtomicU64 = AtomicU64::new(1);

fn next_toast_id() -> u64 {
    NEXT_TOAST_ID.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn toast_ttl(error: bool) -> Duration {
    if error { TOAST_ERROR_TTL } else { TOAST_TTL }
}

pub(crate) fn toast_is_alive(created: Instant, now: Instant, error: bool) -> bool {
    now.duration_since(created) < toast_ttl(error)
}

fn toast_alpha(created: Instant, error: bool) -> f32 {
    let elapsed = created.elapsed().as_secs_f32();
    let ttl = toast_ttl(error).as_secs_f32();
    if elapsed >= ttl {
        return 0.0;
    }
    let fade = ((ttl - elapsed) / TOAST_FADE_SECS).clamp(0.0, 1.0);
    let slide_in = (elapsed / 0.16).clamp(0.0, 1.0);
    fade * slide_in
}

fn alpha_color(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (color.a() as f32 * alpha).clamp(0.0, 255.0) as u8,
    )
}

fn toast_text_wrap_width() -> f32 {
    TOAST_WIDTH - TOAST_PADDING * 2.0 - TOAST_ACCENT_WIDTH - TOAST_ICON_SLOT
}

impl KuroyaApp {
    pub(crate) fn set_status_with_toast(&mut self, status: impl Into<String>) {
        self.status = status.into();
        self.ingest_status_toast();
    }

    pub(crate) fn ingest_status_toast(&mut self) {
        if self.status == self.last_status {
            return;
        }
        self.last_status = self.status.clone();
        self.status_shown_since = Instant::now();

        let message = status_bar_message(&self.status).to_string();
        if message.is_empty() {
            return;
        }
        if let Some(head) = self.status_toasts.first_mut()
            && head.message == message
        {
            head.created = Instant::now();
            return;
        }
        let error = status_message_is_persistent(&self.status);
        self.status_toasts.insert(
            0,
            StatusToast {
                id: next_toast_id(),
                message,
                error,
                created: Instant::now(),
            },
        );
        self.status_toasts.truncate(MAX_TOASTS);
    }

    pub(crate) fn render_status_toasts(&mut self, ctx: &Context) {
        let now = Instant::now();
        self.status_toasts
            .retain(|toast| toast_is_alive(toast.created, now, toast.error));
        if self.status_toasts.is_empty() {
            return;
        }

        let visuals = ctx.style().visuals.clone();
        let mut dismissed_ids: Vec<u64> = Vec::new();

        Area::new(Id::new("status-toast-stack"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_TOP, [-16.0, 16.0])
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_width(TOAST_WIDTH);
                ui.spacing_mut().item_spacing = Vec2::new(0.0, TOAST_GAP);

                for index in 0..self.status_toasts.len() {
                    let Some(toast) = self.status_toasts.get(index).cloned() else {
                        break;
                    };
                    let alpha = toast_alpha(toast.created, toast.error);
                    if alpha <= 0.0 {
                        continue;
                    }

                    let accent = if toast.error {
                        visuals.warn_fg_color
                    } else {
                        visuals.selection.stroke.color
                    };
                    let text_color = visuals.text_color();
                    let bg = alpha_color(visuals.window_fill, alpha);
                    let stroke = Stroke::new(
                        1.0_f32,
                        alpha_color(visuals.widgets.noninteractive.bg_stroke.color, alpha),
                    );

                    let galley = ui.painter().layout(
                        toast.message.clone(),
                        FontId::proportional(TOAST_TEXT_FONT_SIZE),
                        text_color,
                        toast_text_wrap_width(),
                    );
                    let height = galley.size().y.max(18.0) + TOAST_PADDING * 2.0;

                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(TOAST_WIDTH, height), Sense::hover());
                    let response =
                        ui.interact(rect, Id::new(("status-toast", toast.id)), Sense::click());
                    if response.clicked() {
                        dismissed_ids.push(toast.id);
                    }
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                    }

                    ui.painter().rect_filled(rect, CornerRadius::same(8), bg);
                    ui.painter().rect_stroke(
                        rect,
                        CornerRadius::same(8),
                        stroke,
                        egui::StrokeKind::Inside,
                    );

                    let accent_rect = Rect::from_min_max(
                        rect.left_top() + Vec2::new(TOAST_PADDING * 0.4, TOAST_PADDING * 0.5),
                        rect.left_bottom()
                            + Vec2::new(
                                TOAST_PADDING * 0.4 + TOAST_ACCENT_WIDTH,
                                -TOAST_PADDING * 0.5,
                            ),
                    );
                    ui.painter()
                        .rect_filled(accent_rect, CornerRadius::same(2), accent);

                    let icon_rect = Rect::from_center_size(
                        rect.left_center()
                            + Vec2::new(
                                TOAST_ACCENT_WIDTH + TOAST_PADDING + TOAST_ICON_SLOT * 0.5,
                                0.0,
                            ),
                        Vec2::splat(15.0),
                    );
                    crate::ui_icon_shapes::draw_icon(
                        ui,
                        icon_rect,
                        if toast.error {
                            IconKind::Diagnostics
                        } else {
                            IconKind::Code
                        },
                        accent,
                    );

                    ui.painter().galley(
                        rect.left_top()
                            + Vec2::new(
                                TOAST_ACCENT_WIDTH + TOAST_PADDING + TOAST_ICON_SLOT,
                                TOAST_PADDING,
                            ),
                        galley,
                        text_color,
                    );
                }
            });

        if !dismissed_ids.is_empty() {
            self.status_toasts
                .retain(|toast| !dismissed_ids.contains(&toast.id));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{KuroyaApp, app_startup_context::AppStartupContext, terminal::TerminalPane};
    use kuroya_core::{EditorSettings, Workspace};
    use std::{path::PathBuf, time::Duration};
    use tokio::runtime::Runtime;

    #[test]
    fn set_status_with_toast_enqueues_toast_without_render() {
        let mut app = app_for_test();

        app.set_status_with_toast("Saved settings".to_owned());

        assert_eq!(app.status, "Saved settings");
        assert_eq!(app.status_toasts.len(), 1);
        assert_eq!(app.status_toasts[0].message, "Saved settings");
        assert!(!app.status_toasts[0].error);
    }

    #[test]
    fn same_frame_error_then_benign_statuses_each_produce_a_toast() {
        let mut app = app_for_test();

        app.set_status_with_toast("Could not save main.rs: disk full".to_owned());
        app.set_status_with_toast("Saved settings".to_owned());

        assert_eq!(app.status, "Saved settings");
        assert_eq!(app.status_toasts.len(), 2);
        assert_eq!(app.status_toasts[0].message, "Saved settings");
        assert!(!app.status_toasts[0].error);
        assert_eq!(
            app.status_toasts[1].message,
            "Could not save main.rs: disk full"
        );
        assert!(app.status_toasts[1].error);
    }

    #[test]
    fn duplicate_head_toast_refreshes_created_timestamp() {
        let mut app = app_for_test();
        app.set_status_with_toast("Could not save main.rs: disk full".to_owned());
        let first_id = app.status_toasts[0].id;
        let first_created = app.status_toasts[0].created;

        std::thread::sleep(Duration::from_millis(5));
        app.set_status_with_toast("  Could not save main.rs: disk full  ".to_owned());

        assert_eq!(app.status_toasts.len(), 1);
        assert_eq!(app.status_toasts[0].id, first_id);
        assert!(
            app.status_toasts[0].created > first_created,
            "duplicate head toast should refresh its created timestamp"
        );
    }

    #[test]
    fn ingest_ignores_unchanged_status_between_frames() {
        let mut app = app_for_test();
        app.set_status_with_toast("Saved settings".to_owned());

        app.ingest_status_toast();

        assert_eq!(app.status_toasts.len(), 1);
    }

    fn app_for_test() -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(PathBuf::from("workspace")),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(PathBuf::from("workspace"), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![PathBuf::from("workspace")],
            now: std::time::Instant::now(),
            startup_timings: Vec::new(),
        })
    }
}

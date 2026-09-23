use crate::{
    KuroyaApp,
    devtools::{FrameTimingSample, frame_timing_summary},
};
use eframe::egui::{self, Align2, Context, Rect, RichText, Stroke};
use kuroya_core::{EditorExperimentalGpuAcceleration, EditorSettings};
use std::collections::VecDeque;

const LAG_DETECTION_MIN_SAMPLES: usize = 24;
const LAG_DETECTION_SLOW_FRAME_MS: f32 = 50.0;
const LAG_DETECTION_AVERAGE_FRAME_MS: f32 = 28.0;
const LAG_DETECTION_P95_FRAME_MS: f32 = 48.0;
const LAG_DETECTION_MIN_SLOW_FRAMES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GpuAccelerationPrompt {
    pub(crate) latest_ms: f32,
    pub(crate) average_ms: f32,
    pub(crate) p95_ms: f32,
    pub(crate) slow_frame_count: usize,
}

impl KuroyaApp {
    pub(crate) fn maybe_show_gpu_acceleration_prompt(&mut self) {
        if !gpu_acceleration_prompt_should_open(
            &self.settings,
            self.gpu_acceleration_prompt_dismissed,
            self.gpu_acceleration_prompt.is_some(),
        ) {
            return;
        }

        if let Some(prompt) = gpu_acceleration_prompt_from_frame_timings(&self.frame_timings) {
            self.gpu_acceleration_prompt = Some(prompt);
            self.status = "Lag detected; GPU acceleration option available".to_owned();
        }
    }

    pub(crate) fn render_gpu_acceleration_prompt(&mut self, ctx: &Context) {
        let Some(prompt) = self.gpu_acceleration_prompt else {
            return;
        };

        let mut action = GpuAccelerationPromptAction::None;
        let visuals = ctx.style().visuals.clone();
        egui::Area::new(egui::Id::new("gpu-acceleration-prompt-card"))
            .order(egui::Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, [-18.0, -42.0])
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_width(344.0);
                egui::Frame::new()
                    .fill(visuals.window_fill)
                    .stroke(Stroke::new(
                        1.0,
                        visuals.widgets.noninteractive.bg_stroke.color,
                    ))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        let warn_color = visuals.warn_fg_color;

                        ui.horizontal(|ui| {
                            let icon_rect = Rect::from_center_size(
                                ui.cursor().left_center() + egui::vec2(11.0, 0.0),
                                egui::vec2(22.0, 22.0),
                            );
                            crate::ui_icon_shapes::draw_icon(
                                ui,
                                icon_rect,
                                crate::ui_icons::IconKind::Diagnostics,
                                warn_color,
                            );
                            ui.advance_cursor_after_rect(icon_rect);
                            ui.label(RichText::new("Slow frames detected").heading().strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let close_rect = ui
                                        .allocate_exact_size(
                                            egui::vec2(22.0, 22.0),
                                            egui::Sense::click(),
                                        )
                                        .0;
                                    crate::ui_icon_shapes::draw_icon(
                                        ui,
                                        close_rect.shrink(4.0),
                                        crate::ui_icons::IconKind::Close,
                                        text_color_or(ui),
                                    );
                                    if ui
                                        .interact(
                                            close_rect,
                                            egui::Id::new("gpu-prompt-close"),
                                            egui::Sense::click(),
                                        )
                                        .clicked()
                                    {
                                        action = GpuAccelerationPromptAction::Later;
                                    }
                                },
                            );
                        });

                        ui.label(
                            RichText::new("Editor frames are taking longer than they should.")
                                .weak(),
                        );

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            prompt_metric(ui, "NOW", &format!("{:.1} ms", prompt.latest_ms));
                            prompt_metric(ui, "AVG", &format!("{:.1} ms", prompt.average_ms));
                            prompt_metric(ui, "P95", &format!("{:.1} ms", prompt.p95_ms));
                            prompt_metric(ui, "SLOW", &prompt.slow_frame_count.to_string());
                        });

                        ui.add_space(2.0);
                        ui.label("Enables the editor row render cache.");

                        ui.add_space(8.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("Enable").strong())
                                        .fill(ui.visuals().selection.bg_fill),
                                )
                                .clicked()
                            {
                                action = GpuAccelerationPromptAction::Enable;
                            }
                            if ui
                                .add(egui::Button::new(
                                    RichText::new("Open Editor settings").strong(),
                                ))
                                .clicked()
                            {
                                action = GpuAccelerationPromptAction::OpenEditorSettings;
                            }
                            if ui.button("Dismiss").clicked() {
                                action = GpuAccelerationPromptAction::Later;
                            }
                        });
                    });
            });

        match action {
            GpuAccelerationPromptAction::Enable => self.enable_gpu_acceleration_from_prompt(),
            GpuAccelerationPromptAction::OpenEditorSettings => {
                self.open_editor_settings_from_gpu_prompt()
            }
            GpuAccelerationPromptAction::Later => self.dismiss_gpu_acceleration_prompt(),
            GpuAccelerationPromptAction::None => {}
        }
    }

    fn enable_gpu_acceleration_from_prompt(&mut self) {
        let enabled = EditorExperimentalGpuAcceleration::On;
        self.settings.experimental_gpu_acceleration = enabled;
        self.settings_panel_draft.experimental_gpu_acceleration = enabled;
        match self
            .settings
            .save(&crate::workspace_state::settings_path(&self.workspace.root))
        {
            Ok(()) => self.status = "Editor GPU acceleration enabled".to_owned(),
            Err(error) => {
                self.status = format!(
                    "Editor GPU acceleration enabled, but settings were not saved: {error}"
                );
            }
        }
        self.gpu_acceleration_prompt = None;
        self.gpu_acceleration_prompt_dismissed = true;
    }

    fn open_editor_settings_from_gpu_prompt(&mut self) {
        self.settings_panel_open = true;
        self.settings_panel_section = SETTINGS_EDITOR_SECTION_INDEX;
        self.sync_settings_panel_inputs();
        self.dismiss_gpu_acceleration_prompt();
    }

    fn dismiss_gpu_acceleration_prompt(&mut self) {
        self.gpu_acceleration_prompt = None;
        self.gpu_acceleration_prompt_dismissed = true;
        self.status = "GPU acceleration prompt dismissed".to_owned();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GpuAccelerationPromptAction {
    None,
    Enable,
    OpenEditorSettings,
    Later,
}

const SETTINGS_EDITOR_SECTION_INDEX: usize = 1;

fn text_color_or(ui: &egui::Ui) -> egui::Color32 {
    ui.visuals().text_color()
}

fn prompt_metric(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.set_min_width(64.0);
        ui.label(RichText::new(label).small().weak());
        ui.label(RichText::new(value).strong());
    });
}

pub(crate) fn gpu_acceleration_prompt_should_open(
    settings: &EditorSettings,
    dismissed: bool,
    prompt_open: bool,
) -> bool {
    matches!(
        settings.experimental_gpu_acceleration,
        EditorExperimentalGpuAcceleration::Off
    ) && !dismissed
        && !prompt_open
}

pub(crate) fn gpu_acceleration_prompt_from_frame_timings(
    samples: &VecDeque<FrameTimingSample>,
) -> Option<GpuAccelerationPrompt> {
    if samples.len() < LAG_DETECTION_MIN_SAMPLES {
        return None;
    }

    let summary = frame_timing_summary(samples)?;
    let slow_frame_count = samples
        .iter()
        .filter(|sample| sample.frame_ms >= LAG_DETECTION_SLOW_FRAME_MS)
        .count();
    let sustained_slow_frames = slow_frame_count >= LAG_DETECTION_MIN_SLOW_FRAMES;
    let broad_frame_lag = summary.average_ms >= LAG_DETECTION_AVERAGE_FRAME_MS
        && summary.p95_ms >= LAG_DETECTION_P95_FRAME_MS;

    (sustained_slow_frames || broad_frame_lag).then_some(GpuAccelerationPrompt {
        latest_ms: summary.latest_ms,
        average_ms: summary.average_ms,
        p95_ms: summary.p95_ms,
        slow_frame_count,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        GpuAccelerationPrompt, LAG_DETECTION_MIN_SAMPLES,
        gpu_acceleration_prompt_from_frame_timings, gpu_acceleration_prompt_should_open,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, devtools::FrameTimingSample,
        terminal::TerminalPane, workspace_state::settings_path,
    };
    use kuroya_core::{EditorExperimentalGpuAcceleration, EditorSettings, Workspace};
    use std::{
        collections::VecDeque,
        path::PathBuf,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn lag_prompt_waits_for_enough_frame_samples() {
        let samples = frame_samples(&[80.0; LAG_DETECTION_MIN_SAMPLES - 1]);

        assert_eq!(gpu_acceleration_prompt_from_frame_timings(&samples), None);
    }

    #[test]
    fn lag_prompt_ignores_a_single_slow_frame() {
        let mut values = vec![16.0; LAG_DETECTION_MIN_SAMPLES];
        values[0] = 120.0;
        let samples = frame_samples(&values);

        assert_eq!(gpu_acceleration_prompt_from_frame_timings(&samples), None);
    }

    #[test]
    fn lag_prompt_detects_sustained_slow_frames() {
        let mut values = vec![18.0; LAG_DETECTION_MIN_SAMPLES];
        values.extend([55.0, 60.0, 64.0, 58.0, 70.0, 66.0]);
        let samples = frame_samples(&values);

        let prompt = gpu_acceleration_prompt_from_frame_timings(&samples)
            .expect("sustained slow frames should show prompt");

        assert_eq!(prompt.slow_frame_count, 6);
        assert!(prompt.p95_ms >= 60.0);
    }

    #[test]
    fn lag_prompt_open_gate_requires_gpu_off_and_no_dismissal() {
        let mut settings = EditorSettings::default();

        assert!(gpu_acceleration_prompt_should_open(&settings, false, false));
        assert!(!gpu_acceleration_prompt_should_open(&settings, true, false));
        assert!(!gpu_acceleration_prompt_should_open(&settings, false, true));

        settings.experimental_gpu_acceleration = EditorExperimentalGpuAcceleration::On;

        assert!(!gpu_acceleration_prompt_should_open(
            &settings, false, false
        ));
    }

    fn frame_samples(values: &[f32]) -> VecDeque<FrameTimingSample> {
        values
            .iter()
            .copied()
            .map(|frame_ms| FrameTimingSample { frame_ms })
            .collect()
    }

    #[test]
    fn enable_gpu_acceleration_prompt_action_flips_settings_and_saves() {
        let root = unique_temp_dir("gpu-acceleration-prompt-enable");
        let mut app = app_for_test(root.clone());
        app.gpu_acceleration_prompt = Some(GpuAccelerationPrompt {
            latest_ms: 60.0,
            average_ms: 40.0,
            p95_ms: 55.0,
            slow_frame_count: 8,
        });
        app.gpu_acceleration_prompt_dismissed = false;

        app.enable_gpu_acceleration_from_prompt();

        assert_eq!(
            app.settings.experimental_gpu_acceleration,
            EditorExperimentalGpuAcceleration::On
        );
        assert_eq!(
            app.settings_panel_draft.experimental_gpu_acceleration,
            EditorExperimentalGpuAcceleration::On
        );
        assert_eq!(app.gpu_acceleration_prompt, None);
        assert!(app.gpu_acceleration_prompt_dismissed);
        assert_eq!(app.status, "Editor GPU acceleration enabled");

        let saved = EditorSettings::load_or_create(&settings_path(&root))
            .expect("saved settings should load");
        assert_eq!(
            saved.experimental_gpu_acceleration,
            EditorExperimentalGpuAcceleration::On
        );
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
    }

    fn app_for_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }
}

use crate::{
    KuroyaApp,
    devtools::{FrameTimingSample, frame_timing_summary},
    path_display::display_error_label_cow,
    workspace_state::settings_path,
};
use eframe::egui::{self, Align2, Context, RichText};
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

        let mut open = true;
        let mut action = GpuAccelerationPromptAction::None;
        egui::Window::new("Performance")
            .anchor(Align2::RIGHT_BOTTOM, [-18.0, -42.0])
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(320.0);
                ui.label(RichText::new("Kuroya detected UI lag.").strong());
                ui.label("Native wgpu rendering is active. Enable editor GPU acceleration?");
                ui.label(
                    RichText::new(format!(
                        "Latest {:.1} ms  Avg {:.1} ms  P95 {:.1} ms  Slow frames {}",
                        prompt.latest_ms, prompt.average_ms, prompt.p95_ms, prompt.slow_frame_count
                    ))
                    .small(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Enable GPU acceleration").clicked() {
                        action = GpuAccelerationPromptAction::Enable;
                    }
                    if ui.button("Later").clicked() {
                        action = GpuAccelerationPromptAction::Later;
                    }
                });
            });

        if !open && matches!(action, GpuAccelerationPromptAction::None) {
            action = GpuAccelerationPromptAction::Later;
        }

        match action {
            GpuAccelerationPromptAction::Enable => self.enable_gpu_acceleration_from_prompt(),
            GpuAccelerationPromptAction::Later => self.dismiss_gpu_acceleration_prompt(),
            GpuAccelerationPromptAction::None => {}
        }
    }

    fn enable_gpu_acceleration_from_prompt(&mut self) {
        self.settings.experimental_gpu_acceleration = EditorExperimentalGpuAcceleration::On;
        self.settings_panel_draft.experimental_gpu_acceleration =
            EditorExperimentalGpuAcceleration::On;
        self.gpu_acceleration_prompt = None;
        self.gpu_acceleration_prompt_dismissed = true;

        let path = settings_path(&self.workspace.root);
        match self.settings.save(&path) {
            Ok(()) => {
                self.status = "GPU acceleration enabled; native wgpu renderer is active".to_owned();
            }
            Err(error) => {
                let error = error.to_string();
                let error = display_error_label_cow(&error);
                self.status = format!(
                    "GPU acceleration enabled for this session, but save failed: {}",
                    error.as_ref()
                );
            }
        }
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
    Later,
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
        LAG_DETECTION_MIN_SAMPLES, gpu_acceleration_prompt_from_frame_timings,
        gpu_acceleration_prompt_should_open,
    };
    use crate::devtools::FrameTimingSample;
    use kuroya_core::{EditorExperimentalGpuAcceleration, EditorSettings};
    use std::collections::VecDeque;

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
}

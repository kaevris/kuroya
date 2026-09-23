use crate::{
    KuroyaApp, devtools_async_tasks::render_async_task_panel,
    devtools_lsp_trace::render_lsp_trace_panel, devtools_memory::render_memory_diagnostics_panel,
    devtools_profile::render_profile_panel,
    devtools_repaint_diagnostics::render_repaint_diagnostics_panel,
    devtools_startup::render_startup_timing_panel, devtools_trace_id::next_devtools_trace_id,
    path_display::sanitized_display_label_cow, ui_text::count_label,
};
use eframe::egui::{self, Color32, Context, RichText, Stroke, pos2, vec2};
use std::{
    borrow::Cow,
    collections::{HashSet, VecDeque},
    time::Duration,
};

pub(crate) const MAX_FRAME_TIMING_SAMPLES: usize = 180;
pub(crate) const MAX_RECORDED_FRAME_TIMING_MS: f32 = 5_000.0;
pub(crate) const MAX_COMMAND_TRACE_ENTRIES: usize = 80;
pub(crate) const MAX_COMMAND_TRACE_LABEL_CHARS: usize = 160;
pub(crate) const MAX_VERBOSE_LOG_ENTRIES: usize = 240;
pub(crate) const MAX_VERBOSE_LOG_CATEGORY_CHARS: usize = 80;
pub(crate) const MAX_VERBOSE_LOG_MESSAGE_CHARS: usize = 1_000;
const DEVTOOLS_MONOSPACE_ROW_HEIGHT: f32 = 18.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FrameTimingSample {
    pub(crate) frame_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandTraceEntry {
    pub(crate) id: u64,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerboseLogEntry {
    pub(crate) id: u64,
    pub(crate) category: String,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FrameTimingSummary {
    pub(crate) latest_ms: f32,
    pub(crate) average_ms: f32,
    pub(crate) p50_ms: f32,
    pub(crate) p95_ms: f32,
    pub(crate) max_ms: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommandTraceSummary {
    pub(crate) entry_count: usize,
    pub(crate) command_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VerboseLogSummary {
    pub(crate) entry_count: usize,
    pub(crate) category_count: usize,
}

impl KuroyaApp {
    pub(crate) fn record_frame_timing(&mut self, duration: Duration) {
        record_frame_timing_sample(
            &mut self.frame_timings,
            FrameTimingSample {
                frame_ms: bounded_frame_timing_ms(duration.as_secs_f32() * 1000.0),
            },
            MAX_FRAME_TIMING_SAMPLES,
        );
    }

    pub(crate) fn record_verbose_log(
        &mut self,
        category: impl Into<String>,
        message: impl Into<String>,
    ) {
        if !self.settings.devtools_verbose_logging {
            return;
        }
        let id = next_devtools_trace_id(&mut self.next_verbose_log_id);
        record_verbose_log_entry(
            &mut self.verbose_log,
            VerboseLogEntry {
                id,
                category: category.into(),
                message: message.into(),
            },
            MAX_VERBOSE_LOG_ENTRIES,
        );
    }

    pub(crate) fn render_devtools_overlay(&mut self, ctx: &Context) {
        let mut open = self.devtools_open;
        egui::Window::new("Internal Devtools")
            .max_size(crate::layout::popup_window_max_size(ctx))
            .open(&mut open)
            .default_width(360.0)
            .resizable(true)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_startup_timing_panel(ui, &self.startup_timings);
                        ui.separator();
                        render_frame_timing_panel(ui, &self.frame_timings);
                        ui.separator();
                        render_row_render_cache_panel(
                            ui,
                            crate::editor_row_render_cache::editor_row_render_cache_stats(
                                self.settings.experimental_gpu_acceleration,
                                Some(&self.editor_row_render_cache),
                            ),
                        );
                        ui.separator();
                        render_repaint_diagnostics_panel(ui, &self.repaint_diagnostics);
                        ui.separator();
                        let memory = self.memory_diagnostics_summary();
                        render_memory_diagnostics_panel(ui, &memory);
                        ui.separator();
                        render_command_trace_panel(ui, &self.command_trace);
                        ui.separator();
                        render_async_task_panel(
                            ui,
                            &self.active_async_tasks,
                            &self.async_task_trace,
                        );
                        ui.separator();
                        render_lsp_trace_panel(ui, &self.lsp_trace, &self.lsp_progress_titles);
                        ui.separator();
                        render_profile_panel(
                            ui,
                            self.settings.devtools_profiling_enabled,
                            &self.profile_samples,
                        );
                        ui.separator();
                        render_verbose_log_panel(
                            ui,
                            self.settings.devtools_verbose_logging,
                            &self.verbose_log,
                        );
                    });
            });
        self.devtools_open = open;
    }
}

fn render_row_render_cache_panel(
    ui: &mut egui::Ui,
    stats: crate::editor_row_render_cache::EditorRowRenderCacheStats,
) {
    ui.label(RichText::new("Editor Row Render Cache").strong());
    if !stats.enabled {
        ui.label(
            RichText::new("Disabled — enable experimental GPU acceleration to turn it on.").small(),
        );
        return;
    }
    let total = stats.hits.saturating_add(stats.misses);
    let hit_rate = if total == 0 {
        0.0
    } else {
        stats.hits as f32 / total as f32 * 100.0
    };
    egui::Grid::new("devtools_row_render_cache")
        .num_columns(2)
        .spacing([16.0, 2.0])
        .show(ui, |ui| {
            ui.label("Hits");
            ui.label(RichText::new(stats.hits.to_string()).monospace());
            ui.end_row();
            ui.label("Misses");
            ui.label(RichText::new(stats.misses.to_string()).monospace());
            ui.end_row();
            ui.label("Hit rate");
            ui.label(RichText::new(format!("{hit_rate:.1}%")).monospace());
            ui.end_row();
            ui.label("Cached rows");
            ui.label(RichText::new(format!("{} / {}", stats.entries, stats.capacity)).monospace());
            ui.end_row();
        });
}

pub(crate) fn record_frame_timing_sample(
    samples: &mut VecDeque<FrameTimingSample>,
    mut sample: FrameTimingSample,
    max_samples: usize,
) {
    sample.frame_ms = bounded_frame_timing_ms(sample.frame_ms);
    push_bounded(samples, sample, max_samples);
}

pub(crate) fn record_command_trace_entry(
    entries: &mut VecDeque<CommandTraceEntry>,
    mut entry: CommandTraceEntry,
    max_entries: usize,
) {
    normalize_command_trace_label(&mut entry.label);
    push_bounded(entries, entry, max_entries);
}

pub(crate) fn record_verbose_log_entry(
    entries: &mut VecDeque<VerboseLogEntry>,
    mut entry: VerboseLogEntry,
    max_entries: usize,
) {
    normalize_verbose_log_text(&mut entry.category, MAX_VERBOSE_LOG_CATEGORY_CHARS);
    normalize_verbose_log_text(&mut entry.message, MAX_VERBOSE_LOG_MESSAGE_CHARS);
    push_bounded(entries, entry, max_entries);
}

pub(crate) fn frame_timing_summary(
    samples: &VecDeque<FrameTimingSample>,
) -> Option<FrameTimingSummary> {
    let latest = samples.back().copied()?;
    let sample_count = samples.len();
    let mut inline_values = [0.0; MAX_FRAME_TIMING_SAMPLES];
    let (sorted_values, total, max) = if sample_count <= inline_values.len() {
        let values = &mut inline_values[..sample_count];
        let (total, max) = copy_bounded_frame_timing_values(samples, values);
        values.sort_by(f32::total_cmp);
        (&values[..], total, max)
    } else {
        return frame_timing_summary_heap(samples, latest);
    };

    Some(FrameTimingSummary {
        latest_ms: bounded_frame_timing_ms(latest.frame_ms),
        average_ms: total / sample_count as f32,
        p50_ms: nearest_rank_frame_timing_percentile_sorted(sorted_values, 0.50),
        p95_ms: nearest_rank_frame_timing_percentile_sorted(sorted_values, 0.95),
        max_ms: max,
    })
}

fn frame_timing_summary_heap(
    samples: &VecDeque<FrameTimingSample>,
    latest: FrameTimingSample,
) -> Option<FrameTimingSummary> {
    let sample_count = samples.len();
    if sample_count == 0 {
        return None;
    }

    let mut sorted_values = Vec::with_capacity(sample_count);
    let mut total = 0.0f32;
    let mut max = 0.0f32;
    for sample in samples {
        let frame_ms = bounded_frame_timing_ms(sample.frame_ms);
        sorted_values.push(frame_ms);
        total += frame_ms;
        max = max.max(frame_ms);
    }
    sorted_values.sort_by(f32::total_cmp);

    Some(FrameTimingSummary {
        latest_ms: bounded_frame_timing_ms(latest.frame_ms),
        average_ms: total / sample_count as f32,
        p50_ms: nearest_rank_frame_timing_percentile_sorted(&sorted_values, 0.50),
        p95_ms: nearest_rank_frame_timing_percentile_sorted(&sorted_values, 0.95),
        max_ms: max,
    })
}

pub(crate) fn command_trace_summary(
    entries: &VecDeque<CommandTraceEntry>,
) -> Option<CommandTraceSummary> {
    if entries.is_empty() {
        return None;
    }

    Some(CommandTraceSummary {
        entry_count: entries.len(),
        command_count: unique_command_label_count(entries),
    })
}

pub(crate) fn verbose_log_summary(
    entries: &VecDeque<VerboseLogEntry>,
) -> Option<VerboseLogSummary> {
    if entries.is_empty() {
        return None;
    }

    Some(VerboseLogSummary {
        entry_count: entries.len(),
        category_count: unique_verbose_category_count(entries),
    })
}

fn push_bounded<T>(items: &mut VecDeque<T>, item: T, max_items: usize) {
    if max_items == 0 {
        items.clear();
        return;
    }
    items.push_back(item);
    while items.len() > max_items {
        items.pop_front();
    }
}

fn copy_bounded_frame_timing_values(
    samples: &VecDeque<FrameTimingSample>,
    values: &mut [f32],
) -> (f32, f32) {
    debug_assert_eq!(samples.len(), values.len());
    let mut total = 0.0f32;
    let mut max = 0.0f32;
    for (value, sample) in values.iter_mut().zip(samples) {
        let frame_ms = bounded_frame_timing_ms(sample.frame_ms);
        *value = frame_ms;
        total += frame_ms;
        max = max.max(frame_ms);
    }
    (total, max)
}

fn nearest_rank_frame_timing_percentile_sorted(sorted_values: &[f32], percentile: f32) -> f32 {
    if sorted_values.is_empty() {
        return 0.0;
    }

    let rank = (percentile.clamp(0.0, 1.0) * sorted_values.len() as f32).ceil() as usize;
    let rank_index = rank.saturating_sub(1);
    sorted_values[rank_index.min(sorted_values.len() - 1)]
}

fn unique_command_label_count(entries: &VecDeque<CommandTraceEntry>) -> usize {
    let mut labels = HashSet::with_capacity(entries.len());
    for entry in entries {
        labels.insert(entry.label.as_str());
    }
    labels.len()
}

fn unique_verbose_category_count(entries: &VecDeque<VerboseLogEntry>) -> usize {
    let mut categories = HashSet::with_capacity(entries.len());
    for entry in entries {
        categories.insert(entry.category.as_str());
    }
    categories.len()
}

#[cfg(test)]
fn bounded_verbose_log_text(value: &str, max_chars: usize) -> String {
    bounded_verbose_log_text_cow(value, max_chars).into_owned()
}

fn normalize_verbose_log_text(text: &mut String, max_chars: usize) {
    let normalized_text = match bounded_verbose_log_text_cow(text, max_chars) {
        Cow::Borrowed(_) => None,
        Cow::Owned(text) => Some(text),
    };
    if let Some(normalized_text) = normalized_text {
        *text = normalized_text;
    }
}

fn bounded_verbose_log_text_cow(value: &str, max_chars: usize) -> Cow<'_, str> {
    sanitized_display_label_cow(value, max_chars, "")
}

#[cfg(test)]
fn bounded_command_trace_label(value: &str) -> String {
    bounded_command_trace_label_cow(value).into_owned()
}

fn normalize_command_trace_label(label: &mut String) {
    let normalized_label = match bounded_command_trace_label_cow(label) {
        Cow::Borrowed(_) => None,
        Cow::Owned(label) => Some(label),
    };
    if let Some(normalized_label) = normalized_label {
        *label = normalized_label;
    }
}

fn bounded_command_trace_label_cow(value: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(value, MAX_COMMAND_TRACE_LABEL_CHARS, "command")
}

fn render_frame_timing_panel(ui: &mut egui::Ui, samples: &VecDeque<FrameTimingSample>) {
    ui.label(RichText::new("Frame Timing").strong());
    if let Some(summary) = frame_timing_summary(samples) {
        ui.horizontal(|ui| {
            ui.label(format!("Latest {:.1} ms", summary.latest_ms));
            ui.label(format!("Avg {:.1} ms", summary.average_ms));
            ui.label(format!("P50 {:.1} ms", summary.p50_ms));
            ui.label(format!("P95 {:.1} ms", summary.p95_ms));
            ui.label(format!("Max {:.1} ms", summary.max_ms));
        });
        render_frame_timing_graph(ui, samples, summary.max_ms);
    } else {
        ui.label(RichText::new("No frame samples yet").small());
    }
}

fn render_frame_timing_graph(
    ui: &mut egui::Ui,
    samples: &VecDeque<FrameTimingSample>,
    max_ms: f32,
) {
    let width = ui.available_width().max(120.0);
    let height = 72.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0_f32, visuals.widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    );

    if samples.is_empty() {
        return;
    }

    let max_ms = max_ms.max(16.0);
    let bar_width = (rect.width() / samples.len() as f32).max(1.0);
    for (index, sample) in samples.iter().enumerate() {
        let frame_ms = bounded_frame_timing_ms(sample.frame_ms);
        let ratio = (frame_ms / max_ms).clamp(0.0, 1.0);
        let x = rect.left() + index as f32 * bar_width;
        let bottom = rect.bottom();
        let top = bottom - rect.height() * ratio;
        let color = if frame_ms > 32.0 {
            Color32::from_rgb(224, 108, 117)
        } else if frame_ms > 16.0 {
            Color32::from_rgb(244, 191, 117)
        } else {
            Color32::from_rgb(89, 168, 105)
        };
        painter.line_segment(
            [pos2(x, bottom), pos2(x, top)],
            Stroke::new(bar_width.min(3.0), color),
        );
    }
}

fn bounded_frame_timing_ms(frame_ms: f32) -> f32 {
    if frame_ms.is_nan() {
        0.0
    } else {
        frame_ms.clamp(0.0, MAX_RECORDED_FRAME_TIMING_MS)
    }
}

fn render_command_trace_panel(ui: &mut egui::Ui, entries: &VecDeque<CommandTraceEntry>) {
    ui.label(RichText::new("Command Trace").strong());
    if entries.is_empty() {
        ui.label(RichText::new("No commands recorded yet").small());
        return;
    }
    if let Some(summary) = command_trace_summary(entries) {
        ui.horizontal(|ui| {
            ui.label(count_label(summary.entry_count, "entry", "entries"));
            ui.label(count_label(summary.command_count, "command", "commands"));
        });
    }

    let row_height = devtools_row_height(ui);
    egui::ScrollArea::vertical()
        .max_height(180.0)
        .auto_shrink([false, false])
        .show_rows(ui, row_height, entries.len(), |ui, rows| {
            let len = entries.len();
            for display_index in rows {
                if let Some(entry) = reversed_vec_deque_item(entries, len, display_index) {
                    ui.monospace(format!("#{:04} {}", entry.id, entry.label));
                }
            }
        });
}

fn render_verbose_log_panel(ui: &mut egui::Ui, enabled: bool, entries: &VecDeque<VerboseLogEntry>) {
    ui.label(RichText::new("Verbose Log").strong());
    if !enabled {
        ui.label(RichText::new("Disabled").small());
        return;
    }
    if entries.is_empty() {
        ui.label(RichText::new("No verbose logs recorded yet").small());
        return;
    }
    if let Some(summary) = verbose_log_summary(entries) {
        ui.horizontal(|ui| {
            ui.label(count_label(summary.entry_count, "entry", "entries"));
            ui.label(count_label(
                summary.category_count,
                "category",
                "categories",
            ));
        });
    }

    let row_height = devtools_row_height(ui);
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .auto_shrink([false, false])
        .show_rows(ui, row_height, entries.len(), |ui, rows| {
            let len = entries.len();
            for display_index in rows {
                if let Some(entry) = reversed_vec_deque_item(entries, len, display_index) {
                    ui.monospace(format!(
                        "#{:04} [{}] {}",
                        entry.id, entry.category, entry.message
                    ));
                }
            }
        });
}

fn devtools_row_height(ui: &egui::Ui) -> f32 {
    ui.spacing()
        .interact_size
        .y
        .max(DEVTOOLS_MONOSPACE_ROW_HEIGHT)
}

fn reversed_vec_deque_item<T>(items: &VecDeque<T>, len: usize, display_index: usize) -> Option<&T> {
    len.checked_sub(display_index + 1)
        .and_then(|index| items.get(index))
}

#[cfg(test)]
mod tests {
    use super::{
        CommandTraceEntry, CommandTraceSummary, FrameTimingSample, MAX_COMMAND_TRACE_LABEL_CHARS,
        MAX_RECORDED_FRAME_TIMING_MS, MAX_VERBOSE_LOG_CATEGORY_CHARS,
        MAX_VERBOSE_LOG_MESSAGE_CHARS, VerboseLogEntry, VerboseLogSummary,
        bounded_command_trace_label, bounded_command_trace_label_cow, bounded_verbose_log_text,
        bounded_verbose_log_text_cow, command_trace_summary, frame_timing_summary,
        record_command_trace_entry, record_frame_timing_sample, record_verbose_log_entry,
        verbose_log_summary,
    };
    use std::{borrow::Cow, collections::VecDeque};

    #[test]
    fn frame_timing_samples_are_bounded_and_summarized() {
        let mut samples = VecDeque::new();
        record_frame_timing_sample(&mut samples, FrameTimingSample { frame_ms: 8.0 }, 2);
        record_frame_timing_sample(&mut samples, FrameTimingSample { frame_ms: 16.0 }, 2);
        record_frame_timing_sample(&mut samples, FrameTimingSample { frame_ms: 24.0 }, 2);

        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.frame_ms)
                .collect::<Vec<_>>(),
            vec![16.0, 24.0]
        );
        assert_eq!(
            frame_timing_summary(&samples),
            Some(super::FrameTimingSummary {
                latest_ms: 24.0,
                average_ms: 20.0,
                p50_ms: 16.0,
                p95_ms: 24.0,
                max_ms: 24.0,
            })
        );
    }

    #[test]
    fn frame_timing_samples_are_bounded_before_storage() {
        let mut samples = VecDeque::new();
        for frame_ms in [
            MAX_RECORDED_FRAME_TIMING_MS + 250.0,
            f32::INFINITY,
            f32::NAN,
            -12.0,
            24.0,
        ] {
            record_frame_timing_sample(&mut samples, FrameTimingSample { frame_ms }, 8);
        }

        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.frame_ms)
                .collect::<Vec<_>>(),
            vec![
                MAX_RECORDED_FRAME_TIMING_MS,
                MAX_RECORDED_FRAME_TIMING_MS,
                0.0,
                0.0,
                24.0
            ]
        );

        let summary = frame_timing_summary(&samples).expect("bounded samples should summarize");
        assert_eq!(summary.latest_ms, 24.0);
        assert_eq!(summary.max_ms, MAX_RECORDED_FRAME_TIMING_MS);
        assert_eq!(summary.p50_ms, 24.0);
        assert_eq!(summary.p95_ms, MAX_RECORDED_FRAME_TIMING_MS);
        assert_close(
            summary.average_ms,
            (MAX_RECORDED_FRAME_TIMING_MS * 2.0 + 24.0) / 5.0,
        );
    }

    #[test]
    fn frame_timing_summary_reports_recent_percentiles() {
        let samples = VecDeque::from([
            FrameTimingSample { frame_ms: 40.0 },
            FrameTimingSample { frame_ms: 4.0 },
            FrameTimingSample { frame_ms: 24.0 },
            FrameTimingSample { frame_ms: 8.0 },
            FrameTimingSample { frame_ms: 16.0 },
        ]);

        let summary = frame_timing_summary(&samples).expect("samples should summarize");

        assert_eq!(summary.latest_ms, 16.0);
        assert_eq!(summary.p50_ms, 16.0);
        assert_eq!(summary.p95_ms, 40.0);
        assert_eq!(summary.max_ms, 40.0);
        assert_close(summary.average_ms, 18.4);
    }

    #[test]
    fn command_trace_entries_are_bounded() {
        let mut entries = VecDeque::new();
        record_command_trace_entry(
            &mut entries,
            CommandTraceEntry {
                id: 1,
                label: "One".to_owned(),
            },
            2,
        );
        record_command_trace_entry(
            &mut entries,
            CommandTraceEntry {
                id: 2,
                label: "Two".to_owned(),
            },
            2,
        );
        record_command_trace_entry(
            &mut entries,
            CommandTraceEntry {
                id: 3,
                label: "Three".to_owned(),
            },
            2,
        );

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.id, entry.label.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, "Two"), (3, "Three")]
        );
    }

    #[test]
    fn command_trace_entries_normalize_and_cap_labels() {
        let mut entries = VecDeque::new();
        record_command_trace_entry(
            &mut entries,
            CommandTraceEntry {
                id: 1,
                label: format!(
                    "Open\nWorkspace \u{202e}{}",
                    "label-fragment-".repeat(MAX_COMMAND_TRACE_LABEL_CHARS)
                ),
            },
            4,
        );

        let entry = entries
            .front()
            .expect("command trace entry should be stored");
        assert!(!entry.label.contains('\n'));
        assert!(!entry.label.contains('\u{202e}'));
        assert!(entry.label.contains("Open Workspace"));
        assert!(entry.label.contains("..."));
        assert!(entry.label.chars().count() <= MAX_COMMAND_TRACE_LABEL_CHARS);
    }

    #[test]
    fn devtools_label_cows_borrow_clean_ascii_and_unicode() {
        assert_cow_borrowed_eq(bounded_command_trace_label_cow("Quick Open"), "Quick Open");
        assert_cow_borrowed_eq(
            bounded_verbose_log_text_cow("lsp", MAX_VERBOSE_LOG_CATEGORY_CHARS),
            "lsp",
        );

        let unicode_command = "Find \u{03bb} symbol";
        let unicode_message = "Resolved \u{03bb} hover";
        assert_cow_borrowed_eq(
            bounded_command_trace_label_cow(unicode_command),
            unicode_command,
        );
        assert_cow_borrowed_eq(
            bounded_verbose_log_text_cow(unicode_message, MAX_VERBOSE_LOG_MESSAGE_CHARS),
            unicode_message,
        );
    }

    #[test]
    fn devtools_label_cows_own_dirty_truncated_and_fallback_text() {
        let dirty_long_command = format!(
            "Open\nWorkspace \u{202e}{}",
            "label-fragment-".repeat(MAX_COMMAND_TRACE_LABEL_CHARS)
        );
        let command = assert_cow_owned(bounded_command_trace_label_cow(&dirty_long_command));
        assert!(!command.contains('\n'));
        assert!(!command.contains('\u{202e}'));
        assert!(command.contains("Open Workspace"));
        assert!(command.contains("..."));
        assert!(command.chars().count() <= MAX_COMMAND_TRACE_LABEL_CHARS);

        assert_cow_owned_eq(bounded_command_trace_label_cow("\r\n\t\u{202e}"), "command");

        let dirty_long_message = format!(
            "first\r\n\tsecond \u{2066}{}",
            "message-fragment-".repeat(MAX_VERBOSE_LOG_MESSAGE_CHARS)
        );
        let message = assert_cow_owned(bounded_verbose_log_text_cow(
            &dirty_long_message,
            MAX_VERBOSE_LOG_MESSAGE_CHARS,
        ));
        assert!(!message.contains('\r'));
        assert!(!message.contains('\t'));
        assert!(!message.contains('\u{2066}'));
        assert!(message.contains("first second"));
        assert!(message.contains("..."));
        assert!(message.chars().count() <= MAX_VERBOSE_LOG_MESSAGE_CHARS);
    }

    #[test]
    fn verbose_log_text_cow_uses_blank_fallback() {
        assert_cow_owned_eq(
            bounded_verbose_log_text_cow("\r\n\t\x7f\u{202e}", MAX_VERBOSE_LOG_CATEGORY_CHARS),
            "",
        );
        assert_eq!(
            bounded_verbose_log_text("   ", MAX_VERBOSE_LOG_MESSAGE_CHARS),
            ""
        );
    }

    #[test]
    fn devtools_string_wrappers_match_cow_helpers() {
        let long_command = "command-fragment-".repeat(MAX_COMMAND_TRACE_LABEL_CHARS);
        for value in [
            "Quick Open",
            "Find \u{03bb} symbol",
            "  trimmed command  ",
            "\r\n\t\u{202e}",
            long_command.as_str(),
        ] {
            assert_eq!(
                bounded_command_trace_label(value),
                bounded_command_trace_label_cow(value).into_owned()
            );
        }

        let long_message = "message-fragment-".repeat(MAX_VERBOSE_LOG_MESSAGE_CHARS);
        for (value, max_chars) in [
            ("lsp", MAX_VERBOSE_LOG_CATEGORY_CHARS),
            ("Resolved \u{03bb} hover", MAX_VERBOSE_LOG_MESSAGE_CHARS),
            ("  trimmed verbose text  ", MAX_VERBOSE_LOG_MESSAGE_CHARS),
            ("\r\n\t\x7f\u{202e}", MAX_VERBOSE_LOG_CATEGORY_CHARS),
            (long_message.as_str(), MAX_VERBOSE_LOG_MESSAGE_CHARS),
        ] {
            assert_eq!(
                bounded_verbose_log_text(value, max_chars),
                bounded_verbose_log_text_cow(value, max_chars).into_owned()
            );
        }
    }

    #[test]
    fn command_trace_summary_counts_unique_commands() {
        let entries = VecDeque::from([
            CommandTraceEntry {
                id: 1,
                label: "Quick Open".to_owned(),
            },
            CommandTraceEntry {
                id: 2,
                label: "Save Active File".to_owned(),
            },
            CommandTraceEntry {
                id: 3,
                label: "Quick Open".to_owned(),
            },
        ]);

        assert_eq!(
            command_trace_summary(&entries),
            Some(CommandTraceSummary {
                entry_count: 3,
                command_count: 2,
            })
        );
    }

    #[test]
    fn command_trace_summary_is_empty_without_entries() {
        assert_eq!(command_trace_summary(&VecDeque::new()), None);
    }

    #[test]
    fn verbose_log_entries_are_bounded() {
        let mut entries = VecDeque::new();
        record_verbose_log_entry(
            &mut entries,
            VerboseLogEntry {
                id: 1,
                category: "command".to_owned(),
                message: "One".to_owned(),
            },
            2,
        );
        record_verbose_log_entry(
            &mut entries,
            VerboseLogEntry {
                id: 2,
                category: "lsp".to_owned(),
                message: "Two".to_owned(),
            },
            2,
        );
        record_verbose_log_entry(
            &mut entries,
            VerboseLogEntry {
                id: 3,
                category: "async".to_owned(),
                message: "Three".to_owned(),
            },
            2,
        );

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.id, entry.category.as_str(), entry.message.as_str()))
                .collect::<Vec<_>>(),
            vec![(2, "lsp", "Two"), (3, "async", "Three")]
        );
    }

    #[test]
    fn verbose_log_entries_normalize_and_cap_payload_text() {
        let mut entries = VecDeque::new();
        record_verbose_log_entry(
            &mut entries,
            VerboseLogEntry {
                id: 1,
                category: format!(
                    "lsp\n\u{202e}{}",
                    "c".repeat(MAX_VERBOSE_LOG_CATEGORY_CHARS + 16)
                ),
                message: format!(
                    "first\r\n\tsecond \u{2066}{}",
                    "m".repeat(MAX_VERBOSE_LOG_MESSAGE_CHARS + 32)
                ),
            },
            8,
        );

        let entry = entries.front().expect("verbose log entry should be stored");
        assert!(!entry.category.contains('\n'));
        assert!(!entry.category.contains('\u{202e}'));
        assert!(!entry.message.contains('\r'));
        assert!(!entry.message.contains('\t'));
        assert!(!entry.message.contains('\u{2066}'));
        assert!(entry.category.chars().count() <= MAX_VERBOSE_LOG_CATEGORY_CHARS);
        assert!(entry.message.chars().count() <= MAX_VERBOSE_LOG_MESSAGE_CHARS);
        assert!(entry.category.contains("..."));
        assert!(entry.message.contains("first second"));
        assert!(entry.message.contains("..."));
    }

    #[test]
    fn verbose_log_summary_counts_unique_categories() {
        let entries = VecDeque::from([
            VerboseLogEntry {
                id: 1,
                category: "command".to_owned(),
                message: "Quick Open".to_owned(),
            },
            VerboseLogEntry {
                id: 2,
                category: "lsp".to_owned(),
                message: "textDocument/hover".to_owned(),
            },
            VerboseLogEntry {
                id: 3,
                category: "command".to_owned(),
                message: "Save Active File".to_owned(),
            },
        ]);

        assert_eq!(
            verbose_log_summary(&entries),
            Some(VerboseLogSummary {
                entry_count: 3,
                category_count: 2,
            })
        );
    }

    #[test]
    fn verbose_log_summary_is_empty_without_entries() {
        assert_eq!(verbose_log_summary(&VecDeque::new()), None);
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {actual} to be close to {expected}"
        );
    }

    fn assert_cow_borrowed_eq(actual: Cow<'_, str>, expected: &str) {
        match actual {
            Cow::Borrowed(actual) => assert_eq!(actual, expected),
            Cow::Owned(actual) => panic!("expected borrowed {expected:?}, got owned {actual:?}"),
        }
    }

    fn assert_cow_owned(actual: Cow<'_, str>) -> String {
        match actual {
            Cow::Borrowed(actual) => panic!("expected owned label, got borrowed {actual:?}"),
            Cow::Owned(actual) => actual,
        }
    }

    fn assert_cow_owned_eq(actual: Cow<'_, str>, expected: &str) {
        assert_eq!(assert_cow_owned(actual), expected);
    }
}

use crate::{KuroyaApp, devtools_memory::MemoryDiagnosticsSummary};
use eframe::egui::{self, Align2, Context, Order, RichText, Stroke};
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub(crate) const PERF_MONITOR_FRAME_WINDOW: usize = 60;
pub(crate) const PERF_MONITOR_RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_millis(1_000);
const PERF_MONITOR_MAX_FRAME_MS: f32 = 5_000.0;
const PERF_MONITOR_TOP_BREAKDOWN_ROWS: usize = 3;
const PERF_MONITOR_OVERLAY_WIDTH: f32 = 252.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PerfFrameStats {
    pub(crate) fps: f32,
    pub(crate) average_frame_ms: f32,
    pub(crate) p95_frame_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PerfResourceSnapshot {
    ram_label: String,
    disk_label: String,
    breakdown_rows: Vec<(String, String)>,
}

#[derive(Debug, Default)]
pub(crate) struct PerfMonitor {
    frames: PerfFrameTimes,
    last_resource_sample: Option<Instant>,
    snapshot: Option<PerfResourceSnapshot>,
}

impl PerfMonitor {
    pub(crate) fn record_frame_ms(&mut self, frame_ms: f32) {
        self.frames.record_frame_ms(frame_ms);
    }

    pub(crate) fn frame_stats(&self) -> Option<PerfFrameStats> {
        self.frames.stats()
    }

    pub(crate) fn snapshot(&self) -> Option<&PerfResourceSnapshot> {
        self.snapshot.as_ref()
    }

    pub(crate) fn refresh_due(&self, now: Instant) -> bool {
        self.last_resource_sample
            .is_none_or(|last| now.duration_since(last) >= PERF_MONITOR_RESOURCE_SAMPLE_INTERVAL)
    }

    pub(crate) fn store_resource_snapshot(
        &mut self,
        now: Instant,
        ram_bytes: Option<u64>,
        drive: String,
        disk: Option<(u64, u64)>,
        breakdown_rows: Vec<(String, String)>,
    ) {
        self.last_resource_sample = Some(now);
        self.snapshot = Some(PerfResourceSnapshot {
            ram_label: ram_label(ram_bytes),
            disk_label: disk_line(drive, disk),
            breakdown_rows,
        });
    }
}

#[derive(Debug)]
struct PerfFrameTimes {
    samples: [f32; PERF_MONITOR_FRAME_WINDOW],
    head: usize,
    filled: usize,
}

impl Default for PerfFrameTimes {
    fn default() -> Self {
        Self {
            samples: [0.0; PERF_MONITOR_FRAME_WINDOW],
            head: 0,
            filled: 0,
        }
    }
}

impl PerfFrameTimes {
    fn record_frame_ms(&mut self, frame_ms: f32) {
        self.samples[self.head] = bounded_perf_frame_ms(frame_ms);
        self.head = (self.head + 1) % PERF_MONITOR_FRAME_WINDOW;
        self.filled = (self.filled + 1).min(PERF_MONITOR_FRAME_WINDOW);
    }

    fn stats(&self) -> Option<PerfFrameStats> {
        if self.filled == 0 {
            return None;
        }
        let mut sorted = [0.0_f32; PERF_MONITOR_FRAME_WINDOW];
        sorted[..self.filled].copy_from_slice(&self.samples[..self.filled]);
        sorted[..self.filled].sort_by(f32::total_cmp);
        let total: f32 = self.samples[..self.filled].iter().sum();
        let average_frame_ms = total / self.filled as f32;
        let fps = if average_frame_ms > 0.0 {
            1000.0 / average_frame_ms
        } else {
            0.0
        };
        Some(PerfFrameStats {
            fps,
            average_frame_ms,
            p95_frame_ms: nearest_rank_percentile_sorted(&sorted[..self.filled], 0.95),
        })
    }
}

fn bounded_perf_frame_ms(frame_ms: f32) -> f32 {
    if frame_ms.is_finite() {
        frame_ms.clamp(0.0, PERF_MONITOR_MAX_FRAME_MS)
    } else {
        PERF_MONITOR_MAX_FRAME_MS
    }
}

fn nearest_rank_percentile_sorted(sorted_values: &[f32], percentile: f32) -> f32 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    let rank = (percentile.clamp(0.0, 1.0) * sorted_values.len() as f32).ceil() as usize;
    sorted_values[rank.saturating_sub(1).min(sorted_values.len() - 1)]
}

pub(crate) fn perf_monitor_overlay_should_render(
    enabled: bool,
    frame_stats_ready: bool,
    resources_ready: bool,
) -> bool {
    enabled && frame_stats_ready && resources_ready
}

pub(crate) fn format_perf_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

pub(crate) fn format_perf_gigabytes(bytes: u64) -> u64 {
    bytes / (1024 * 1024 * 1024)
}

fn ram_label(ram_bytes: Option<u64>) -> String {
    ram_bytes
        .map(format_perf_bytes)
        .unwrap_or_else(|| "n/a".to_owned())
}

fn disk_line(drive: String, disk: Option<(u64, u64)>) -> String {
    match disk {
        Some((used, total)) => format!(
            "Disk {} {}/{} GB",
            drive,
            format_perf_gigabytes(used),
            format_perf_gigabytes(total)
        ),
        None => "Disk n/a".to_owned(),
    }
}

fn fps_line(stats: &PerfFrameStats) -> String {
    format!(
        "{:.0} FPS ({:.1} ms, p95 {:.1} ms)",
        stats.fps, stats.average_frame_ms, stats.p95_frame_ms
    )
}

fn resource_line(snapshot: Option<&PerfResourceSnapshot>) -> String {
    match snapshot {
        Some(snapshot) => format!("RAM {} | {}", snapshot.ram_label, snapshot.disk_label),
        None => "RAM n/a".to_owned(),
    }
}

fn resource_breakdown_rows(summary: &MemoryDiagnosticsSummary) -> Vec<(String, String)> {
    vec![
        (
            "Buffers".to_owned(),
            format_perf_bytes(summary.buffers.bytes as u64),
        ),
        (
            "Terminal".to_owned(),
            format_perf_bytes(summary.terminal.search_buffer_bytes as u64),
        ),
        ("LSP".to_owned(), summary.lsp.clients.to_string()),
        ("Plugins".to_owned(), summary.plugins.loaded.to_string()),
        ("Search".to_owned(), summary.search.matches.to_string()),
        ("Project".to_owned(), summary.project.files.to_string()),
    ]
}

pub(crate) fn drive_label(path: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let text = path.to_string_lossy();
        let mut chars = text.chars();
        if let (Some(letter), Some(':')) = (chars.next(), chars.next()) {
            return format!("{}:", letter.to_ascii_uppercase());
        }
        "Disk".to_owned()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "/".to_owned()
    }
}

pub(crate) fn process_resident_bytes() -> Option<u64> {
    #[cfg(target_os = "windows")]
    {
        windows_process_resident_bytes()
    }
    #[cfg(target_os = "linux")]
    {
        linux_process_resident_bytes()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

pub(crate) fn disk_used_total_bytes(path: &Path) -> Option<(u64, u64)> {
    #[cfg(target_os = "windows")]
    {
        windows_disk_used_total_bytes(path)
    }
    #[cfg(all(unix, target_pointer_width = "64"))]
    {
        unix_disk_used_total_bytes(path)
    }
    #[cfg(not(any(target_os = "windows", all(unix, target_pointer_width = "64"))))]
    {
        let _ = path;
        None
    }
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut std::os::raw::c_void;
    fn K32GetProcessMemoryInfo(
        process: *mut std::os::raw::c_void,
        counters: *mut ProcessMemoryCounters,
        cb: u32,
    ) -> i32;
    fn GetDiskFreeSpaceExW(
        directory_name: *const u16,
        free_bytes_available_to_caller: *mut u64,
        total_number_of_bytes: *mut u64,
        total_number_of_free_bytes: *mut u64,
    ) -> i32;
}

#[cfg(target_os = "windows")]
fn windows_process_resident_bytes() -> Option<u64> {
    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    let process = unsafe { GetCurrentProcess() };
    let ok = unsafe { K32GetProcessMemoryInfo(process, &mut counters, counters.cb) };
    (ok != 0).then_some(counters.working_set_size as u64)
}

#[cfg(target_os = "windows")]
fn windows_disk_used_total_bytes(path: &Path) -> Option<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut free_bytes_available = 0_u64;
    let mut total_bytes = 0_u64;
    let mut total_free_bytes = 0_u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_bytes_available,
            &mut total_bytes,
            &mut total_free_bytes,
        )
    };
    if ok == 0 || total_bytes == 0 {
        None
    } else {
        Some((total_bytes.saturating_sub(total_free_bytes), total_bytes))
    }
}

#[cfg(target_os = "linux")]
fn linux_process_resident_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmRSS:")?;
        let kilobytes: u64 = value.trim().trim_end_matches("kB").trim().parse().ok()?;
        Some(kilobytes.saturating_mul(1024))
    })
}

#[cfg(all(unix, target_pointer_width = "64"))]
#[repr(C)]
struct StatVfs {
    f_bsize: u64,
    f_frsize: u64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_favail: u64,
    f_fsid: u64,
    f_flag: u64,
    f_namemax: u64,
    f_spare: [u64; 3],
}

#[cfg(all(unix, target_pointer_width = "64"))]
unsafe extern "C" {
    fn statvfs(path: *const std::ffi::c_char, buf: *mut StatVfs) -> i32;
}

#[cfg(all(unix, target_pointer_width = "64"))]
fn unix_disk_used_total_bytes(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut buf = StatVfs {
        f_bsize: 0,
        f_frsize: 0,
        f_blocks: 0,
        f_bfree: 0,
        f_bavail: 0,
        f_files: 0,
        f_ffree: 0,
        f_favail: 0,
        f_fsid: 0,
        f_flag: 0,
        f_namemax: 0,
        f_spare: [0; 3],
    };
    let ok = unsafe { statvfs(c_path.as_ptr(), &mut buf) };
    if ok != 0 || buf.f_blocks == 0 {
        return None;
    }
    let block = if buf.f_frsize > 0 {
        buf.f_frsize
    } else {
        buf.f_bsize
    };
    let total = buf.f_blocks.saturating_mul(block);
    let free = buf.f_bfree.saturating_mul(block);
    Some((total.saturating_sub(free), total))
}

impl KuroyaApp {
    pub(crate) fn record_perf_monitor_frame(&mut self, frame_ms: f32) {
        if !self.settings.perf_monitor_enabled {
            return;
        }
        self.perf_monitor.record_frame_ms(frame_ms);
    }

    pub(crate) fn render_perf_monitor_overlay(&mut self, ctx: &Context) {
        if !self.settings.perf_monitor_enabled {
            return;
        }
        let now = Instant::now();
        if self.perf_monitor.refresh_due(now) {
            let breakdown_rows = resource_breakdown_rows(&self.memory_diagnostics_summary());
            let ram_bytes = process_resident_bytes();
            let drive = drive_label(&self.workspace.root);
            let disk = disk_used_total_bytes(&self.workspace.root);
            self.perf_monitor
                .store_resource_snapshot(now, ram_bytes, drive, disk, breakdown_rows);
        }
        let frame_stats = self.perf_monitor.frame_stats();
        let snapshot = self.perf_monitor.snapshot().cloned();
        if !perf_monitor_overlay_should_render(
            self.settings.perf_monitor_enabled,
            frame_stats.is_some(),
            snapshot.is_some(),
        ) {
            return;
        }
        let Some(stats) = frame_stats else {
            return;
        };
        let visuals = ctx.style().visuals.clone();
        egui::Area::new(egui::Id::new("perf-monitor-overlay"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_TOP, [-10.0, 10.0])
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(visuals.window_fill.gamma_multiply(0.8))
                    .stroke(Stroke::new(
                        1.0_f32,
                        visuals.widgets.noninteractive.bg_stroke.color,
                    ))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(PERF_MONITOR_OVERLAY_WIDTH);
                        ui.label(RichText::new(fps_line(&stats)).monospace().small());
                        ui.label(
                            RichText::new(resource_line(snapshot.as_ref()))
                                .monospace()
                                .small(),
                        );
                        if let Some(snapshot) = snapshot {
                            for (label, value) in snapshot
                                .breakdown_rows
                                .iter()
                                .take(PERF_MONITOR_TOP_BREAKDOWN_ROWS)
                            {
                                ui.label(
                                    RichText::new(format!("{label} {value}"))
                                        .monospace()
                                        .small()
                                        .weak(),
                                );
                            }
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PERF_MONITOR_FRAME_WINDOW, PerfFrameStats, PerfFrameTimes, PerfMonitor, disk_line,
        disk_used_total_bytes, drive_label, format_perf_bytes, format_perf_gigabytes, fps_line,
        nearest_rank_percentile_sorted, perf_monitor_overlay_should_render, process_resident_bytes,
        ram_label, resource_breakdown_rows,
    };
    use crate::KuroyaApp;
    use crate::devtools_memory::{
        BufferMemoryDiagnostics, DiagnosticMemoryDiagnostics, LspMemoryDiagnostics,
        MemoryDiagnosticsSummary, PluginMemoryDiagnostics, ProjectMemoryDiagnostics,
        SearchMemoryDiagnostics,
    };
    use crate::terminal::TerminalDiagnosticsStats;
    use std::time::{Duration, Instant};

    #[test]
    fn perf_frame_times_fps_uses_the_average_of_the_rolling_window() {
        let mut frames = PerfFrameTimes::default();
        for _ in 0..PERF_MONITOR_FRAME_WINDOW {
            frames.record_frame_ms(1000.0 / 60.0);
        }

        let stats = frames.stats().expect("filled window should have stats");

        assert!((stats.fps - 60.0).abs() < 0.1);
        assert!((stats.average_frame_ms - 1000.0 / 60.0).abs() < 0.01);
    }

    #[test]
    fn perf_frame_times_p95_uses_the_nearest_rank_percentile() {
        let mut frames = PerfFrameTimes::default();
        for _ in 0..20 {
            frames.record_frame_ms(10.0);
        }
        for _ in 0..40 {
            frames.record_frame_ms(20.0);
        }

        let stats = frames.stats().expect("filled window should have stats");

        assert_eq!(stats.p95_frame_ms, 20.0);
    }

    #[test]
    fn perf_frame_times_window_keeps_only_the_most_recent_frames() {
        let mut frames = PerfFrameTimes::default();
        for _ in 0..PERF_MONITOR_FRAME_WINDOW {
            frames.record_frame_ms(10.0);
        }
        for _ in 0..PERF_MONITOR_FRAME_WINDOW {
            frames.record_frame_ms(20.0);
        }

        let stats = frames.stats().expect("filled window should have stats");

        assert_eq!(stats.average_frame_ms, 20.0);
        assert!((stats.fps - 50.0).abs() < 0.01);
    }

    #[test]
    fn perf_frame_times_stats_are_absent_before_any_frame_is_recorded() {
        assert!(PerfFrameTimes::default().stats().is_none());
    }

    #[test]
    fn perf_frame_times_bounds_non_finite_frame_durations() {
        let mut frames = PerfFrameTimes::default();
        frames.record_frame_ms(f32::NAN);

        let stats = frames.stats().expect("recorded frame should have stats");

        assert_eq!(stats.average_frame_ms, 5_000.0);
    }

    #[test]
    fn perf_percentile_helper_clamps_rank_into_range() {
        let sorted = [4.0_f32];

        assert_eq!(nearest_rank_percentile_sorted(&sorted, 0.95), 4.0);
        assert_eq!(nearest_rank_percentile_sorted(&[], 0.95), 0.0);
    }

    #[test]
    fn perf_byte_formatting_scales_across_bytes_kb_mb_and_gb() {
        assert_eq!(format_perf_bytes(0), "0 B");
        assert_eq!(format_perf_bytes(512), "512 B");
        assert_eq!(format_perf_bytes(1536), "1.5 KB");
        assert_eq!(format_perf_bytes(2 * 1024 * 1024), "2.0 MB");
        assert_eq!(format_perf_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn perf_gigabyte_formatting_truncates_to_whole_gigabytes() {
        assert_eq!(format_perf_gigabytes(0), 0);
        assert_eq!(format_perf_gigabytes(195_421_011_968), 182);
        assert_eq!(format_perf_gigabytes(511_101_108_224), 476);
    }

    #[test]
    fn perf_ram_label_reports_placeholder_when_sampling_is_unavailable() {
        assert_eq!(ram_label(None), "n/a");
        assert_eq!(ram_label(Some(431_712_384)), "411.7 MB");
    }

    #[test]
    fn perf_disk_line_formats_drive_used_and_total_gigabytes() {
        assert_eq!(
            disk_line("C:".to_owned(), Some((195_421_011_968, 511_101_108_224))),
            "Disk C: 182/476 GB"
        );
        assert_eq!(disk_line("C:".to_owned(), None), "Disk n/a");
    }

    #[test]
    fn perf_fps_line_combines_fps_average_and_p95() {
        let stats = PerfFrameStats {
            fps: 123.4,
            average_frame_ms: 8.1,
            p95_frame_ms: 12.6,
        };

        assert_eq!(fps_line(&stats), "123 FPS (8.1 ms, p95 12.6 ms)");
    }

    #[test]
    fn perf_overlay_gate_requires_enabled_and_ready_stats_and_resources() {
        assert!(perf_monitor_overlay_should_render(true, true, true));
        assert!(!perf_monitor_overlay_should_render(false, true, true));
        assert!(!perf_monitor_overlay_should_render(true, false, true));
        assert!(!perf_monitor_overlay_should_render(true, true, false));
    }

    #[test]
    fn perf_monitor_refresh_cadence_is_at_most_one_hertz() {
        let mut monitor = PerfMonitor::default();
        let start = Instant::now();

        assert!(monitor.refresh_due(start));
        monitor.store_resource_snapshot(start, None, "C:".to_owned(), None, Vec::new());
        assert!(!monitor.refresh_due(start + Duration::from_millis(500)));
        assert!(monitor.refresh_due(start + Duration::from_millis(1_000)));
    }

    #[test]
    fn perf_monitor_keeps_the_last_resource_snapshot_between_refreshes() {
        let mut monitor = PerfMonitor::default();
        let start = Instant::now();

        assert!(monitor.snapshot().is_none());
        monitor.store_resource_snapshot(
            start,
            Some(1024 * 1024),
            "C:".to_owned(),
            Some((1, 2)),
            vec![("Buffers".to_owned(), "2.0 MB".to_owned())],
        );

        let snapshot = monitor.snapshot().expect("snapshot should be stored");
        assert_eq!(snapshot.ram_label, "1.0 MB");
        assert_eq!(snapshot.disk_label, "Disk C: 0/0 GB");
        assert_eq!(
            snapshot.breakdown_rows,
            vec![("Buffers".to_owned(), "2.0 MB".to_owned())]
        );
    }

    #[test]
    fn perf_resource_breakdown_rows_cover_subsystems_with_memory_or_counts() {
        let summary = MemoryDiagnosticsSummary {
            buffers: BufferMemoryDiagnostics {
                bytes: 2_097_152,
                ..BufferMemoryDiagnostics::default()
            },
            terminal: TerminalDiagnosticsStats {
                search_buffer_bytes: 4_096,
                ..TerminalDiagnosticsStats::default()
            },
            project: ProjectMemoryDiagnostics {
                files: 128,
                ..ProjectMemoryDiagnostics::default()
            },
            diagnostics: DiagnosticMemoryDiagnostics::default(),
            search: SearchMemoryDiagnostics {
                matches: 64,
                ..SearchMemoryDiagnostics::default()
            },
            lsp: LspMemoryDiagnostics {
                clients: 2,
                ..LspMemoryDiagnostics::default()
            },
            plugins: PluginMemoryDiagnostics {
                loaded: 3,
                ..PluginMemoryDiagnostics::default()
            },
        };

        let rows = resource_breakdown_rows(&summary);

        assert_eq!(
            rows,
            vec![
                ("Buffers".to_owned(), "2.0 MB".to_owned()),
                ("Terminal".to_owned(), "4.0 KB".to_owned()),
                ("LSP".to_owned(), "2".to_owned()),
                ("Plugins".to_owned(), "3".to_owned()),
                ("Search".to_owned(), "64".to_owned()),
                ("Project".to_owned(), "128".to_owned()),
            ]
        );
    }

    #[test]
    fn perf_drive_label_uses_the_workspace_drive_letter_on_windows() {
        let label = drive_label(std::path::Path::new("C:/workspaces/demo"));

        if cfg!(target_os = "windows") {
            assert_eq!(label, "C:");
        } else {
            assert_eq!(label, "/");
        }
    }

    #[test]
    fn perf_monitor_records_frames_only_while_the_setting_is_enabled() {
        let root = unique_temp_dir("perf-monitor-gating");
        let mut app = app_for_test(root.clone());

        assert!(!app.settings.perf_monitor_enabled);
        app.record_perf_monitor_frame(16.0);
        assert!(app.perf_monitor.frame_stats().is_none());

        app.settings.perf_monitor_enabled = true;
        app.record_perf_monitor_frame(16.0);
        assert!(app.perf_monitor.frame_stats().is_some());

        app.settings.perf_monitor_enabled = false;
        app.record_perf_monitor_frame(16.0);
        assert_eq!(
            app.perf_monitor
                .frame_stats()
                .expect("existing stats are kept")
                .average_frame_ms,
            16.0
        );

        std::fs::remove_dir_all(root).ok();
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
    }

    fn app_for_test(root: std::path::PathBuf) -> KuroyaApp {
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = kuroya_core::EditorSettings::default();
        KuroyaApp::from_startup_context(crate::app_startup_context::AppStartupContext {
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime"),
            tx,
            rx,
            workspace: kuroya_core::Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: crate::terminal::TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root.clone()],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn process_resident_bytes_samples_the_current_process_working_set() {
        let ram = process_resident_bytes().expect("kernel32 memory info should be available");

        assert!(ram > 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn disk_used_total_bytes_reports_the_workspace_drive() {
        let (used, total) = disk_used_total_bytes(std::path::Path::new("C:/"))
            .expect("disk info should be available");

        assert!(total > 0);
        assert!(used <= total);
    }
}

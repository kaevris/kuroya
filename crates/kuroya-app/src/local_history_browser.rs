use crate::{
    KuroyaApp,
    file_history::{
        LOCAL_HISTORY_MAX_BYTES, LocalHistorySnapshot, local_history_snapshots_for_file_async,
    },
    layout::popup_window_max_size_with_top_margin,
    local_history_runtime::{local_history_display_path, local_history_loaded_text_block_reason},
    path_display::sanitized_display_label_cow,
    persistence_storage::read_file_bytes_with_limit_async,
    popup_buttons::{PopupButtonKind, popup_button},
    ui_events::UiEvent,
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
    workspace_event_guards::paths_match_lexically,
};
use eframe::egui::{self, Align, Context, Key, RichText};
use std::{
    borrow::Cow,
    io::ErrorKind,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const LOCAL_HISTORY_BROWSER_ROW_HEIGHT: f32 = 24.0;
const LOCAL_HISTORY_BROWSER_MAX_VISIBLE_ROWS: usize = 12;
const LOCAL_HISTORY_BROWSER_MODIFIED_LABEL_MAX_CHARS: usize = 64;
const LOCAL_HISTORY_BROWSER_NO_SNAPSHOTS_LABEL: &str = "No local history for this file";
const LOCAL_HISTORY_BROWSER_LOADING_LABEL: &str = "Loading local history...";

impl KuroyaApp {
    /// Toggles the Local History browser for the active file. Opening always
    /// re-enumerates the (bounded, cheap) snapshot list so the rows reflect
    /// the newest saved snapshots.
    pub(crate) fn toggle_local_history_browser(&mut self) {
        if self.local_history_browser_open {
            self.close_local_history_browser();
            return;
        }
        let Some(path) = self.active_file_or_diff_source_path("browse local history") else {
            return;
        };
        self.close_command_palette();
        self.close_quick_open();
        self.close_workspace_symbols(false);
        self.close_project_search();
        self.local_history_browser_open = true;
        self.local_history_browser_path = Some(path);
        self.local_history_browser_snapshots.clear();
        self.local_history_browser_selected = 0;
        self.spawn_local_history_browser_refresh();
    }

    pub(crate) fn close_local_history_browser(&mut self) {
        if !self.local_history_browser_open {
            return;
        }
        self.local_history_browser_open = false;
        self.local_history_browser_path = None;
        self.local_history_browser_snapshots.clear();
        self.local_history_browser_selected = 0;
        self.local_history_browser_loading = false;
        self.status = "Closed local history browser".to_owned();
    }

    /// Re-enumerates snapshots for the tracked browser path. Invoked when the
    /// browser opens and whenever a save completes for the tracked file.
    pub(crate) fn spawn_local_history_browser_refresh(&mut self) {
        let Some(path) = self.local_history_browser_path.clone() else {
            return;
        };
        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        let tx = self.tx.clone();
        self.local_history_browser_loading = true;
        self.record_async_task_started(
            "Local History Browser",
            format!("{} snapshots", local_history_display_path(&path).as_ref()),
        );
        self.runtime.spawn(async move {
            let snapshots = local_history_snapshots_for_file_async(&root, &path)
                .await
                .unwrap_or_default();
            let _ = crate::ui_event_channel::send_ui_event(
                &tx,
                UiEvent::LocalHistoryBrowserLoaded {
                    root,
                    generation,
                    path,
                    snapshots,
                },
            );
        });
    }

    /// Refresh hook for save completion: only re-enumerates while the browser
    /// is open and the saved file is the one being browsed.
    pub(crate) fn refresh_local_history_browser_after_save(&mut self, path: &Path) {
        if !self.local_history_browser_open {
            return;
        }
        let tracked = self
            .local_history_browser_path
            .as_deref()
            .is_some_and(|tracked| paths_match_lexically(tracked, path));
        if tracked {
            self.spawn_local_history_browser_refresh();
        }
    }

    pub(crate) fn apply_local_history_browser_loaded(
        &mut self,
        root: std::path::PathBuf,
        generation: u64,
        path: std::path::PathBuf,
        snapshots: Vec<LocalHistorySnapshot>,
    ) {
        if !self.workspace_event_is_current(&root, generation) {
            return;
        }
        let tracked = self
            .local_history_browser_path
            .as_deref()
            .is_some_and(|tracked| paths_match_lexically(tracked, &path));
        if !self.local_history_browser_open || !tracked {
            return;
        }
        self.local_history_browser_snapshots = snapshots;
        clamp_selection(
            &mut self.local_history_browser_selected,
            self.local_history_browser_snapshots.len(),
        );
        self.local_history_browser_loading = false;
    }

    /// Opens the selected snapshot as a read-only virtual revision buffer.
    /// Reads the snapshot off the UI thread and never touches the file on
    /// disk; the opened buffer is the same virtual revision kind the
    /// "Open Latest Local History Snapshot" command produces.
    pub(crate) fn open_local_history_browser_selection(&mut self, index: usize) {
        let Some(path) = self.local_history_browser_path.clone() else {
            return;
        };
        let Some(snapshot) = self.local_history_browser_snapshots.get(index).cloned() else {
            return;
        };
        let root = self.workspace.root.clone();
        let generation = self.workspace_event_generation;
        let tx = self.tx.clone();
        let snapshot_path = snapshot.path;
        let sequence = snapshot.sequence;
        let modified = snapshot.modified;
        let path_label = local_history_display_path(&path).into_owned();
        self.status = local_history_browser_loading_status(&path_label, sequence);
        self.record_async_task_started(
            "Local History",
            format!("{path_label} snapshot {sequence}"),
        );
        self.runtime.spawn(async move {
            let result = read_local_history_browser_snapshot_text(&snapshot_path).await;
            let _ = crate::ui_event_channel::send_ui_event(
                &tx,
                UiEvent::LocalHistoryBrowserSnapshotLoaded {
                    root,
                    generation,
                    path,
                    snapshot_path,
                    sequence,
                    modified,
                    result,
                },
            );
        });
    }

    pub(crate) fn apply_local_history_browser_snapshot_loaded(
        &mut self,
        root: std::path::PathBuf,
        generation: u64,
        path: std::path::PathBuf,
        snapshot_path: std::path::PathBuf,
        sequence: u128,
        modified: Option<SystemTime>,
        result: Result<String, String>,
    ) {
        if !self.workspace_event_is_current(&root, generation) {
            return;
        }
        let path_label = local_history_display_path(&path).into_owned();
        let text = match result {
            Ok(text) => text,
            Err(error) => {
                self.status = local_history_browser_failed_status(&path_label, sequence, &error);
                return;
            }
        };
        if let Some(reason) = local_history_loaded_text_block_reason(&text) {
            self.status = local_history_browser_failed_status(&path_label, sequence, reason);
            return;
        }
        let label = local_history_browser_revision_label(&path_label, sequence, modified);
        let target = local_history_browser_revision_target(&path_label, sequence);
        let snapshot_label = local_history_display_path(&snapshot_path);
        self.open_virtual_revision_buffer(label, path, text, target, "local history");
        self.status =
            local_history_browser_opened_status(&path_label, sequence, snapshot_label.as_ref());
    }

    pub(crate) fn render_local_history_browser(&mut self, ctx: &Context) {
        let mut close = false;
        let mut open_index = None;

        egui::Window::new("Local History")
            .max_size(popup_window_max_size_with_top_margin(ctx, 108.0))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 84.0])
            .default_width(660.0)
            .show(ctx, |ui| {
                let path_label = self
                    .local_history_browser_path
                    .as_deref()
                    .map(local_history_display_path)
                    .unwrap_or_else(|| Cow::Borrowed("."));
                ui.horizontal(|ui| {
                    local_history_browser_status_label(
                        ui,
                        Cow::Owned(local_history_browser_header_label(
                            path_label.as_ref(),
                            self.local_history_browser_snapshots.len(),
                        )),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if popup_button(ui, "Close", PopupButtonKind::Secondary).clicked() {
                            close = true;
                        }
                        if self.local_history_browser_loading {
                            local_history_browser_status_label(
                                ui,
                                Cow::Borrowed(LOCAL_HISTORY_BROWSER_LOADING_LABEL),
                            );
                        }
                    });
                });

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
                let snapshot_count = self.local_history_browser_snapshots.len();
                let viewport_height = ui.available_height();
                let selection_changed = ui.input(|input| {
                    handle_list_navigation_keys(
                        input,
                        &mut self.local_history_browser_selected,
                        snapshot_count,
                        selection_page_step(LOCAL_HISTORY_BROWSER_ROW_HEIGHT, viewport_height),
                    )
                });
                if snapshot_count > 0 && ui.input(|input| input.key_pressed(Key::Enter)) {
                    open_index = Some(self.local_history_browser_selected);
                }

                ui.separator();
                if snapshot_count == 0 {
                    ui.add_space(24.0);
                    ui.centered_and_justified(|ui| {
                        ui.label(LOCAL_HISTORY_BROWSER_NO_SNAPSHOTS_LABEL);
                    });
                    return;
                }

                let mut clicked = None;
                let max_height = (snapshot_count.min(LOCAL_HISTORY_BROWSER_MAX_VISIBLE_ROWS)
                    as f32)
                    * LOCAL_HISTORY_BROWSER_ROW_HEIGHT;
                let mut scroll_area = egui::ScrollArea::vertical().max_height(max_height);
                if selection_changed {
                    scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                        self.local_history_browser_selected,
                        snapshot_count,
                        LOCAL_HISTORY_BROWSER_ROW_HEIGHT,
                        viewport_height,
                    ));
                }
                scroll_area.show_rows(
                    ui,
                    LOCAL_HISTORY_BROWSER_ROW_HEIGHT,
                    snapshot_count,
                    |ui, rows| {
                        for index in rows {
                            let Some(snapshot) = self.local_history_browser_snapshots.get(index)
                            else {
                                continue;
                            };
                            let response = ui.selectable_label(
                                index == self.local_history_browser_selected,
                                local_history_browser_row_label(
                                    snapshot.sequence,
                                    snapshot.modified,
                                ),
                            );
                            if response.clicked() {
                                clicked = Some(index);
                            }
                        }
                    },
                );
                if let Some(index) = clicked {
                    self.local_history_browser_selected = index;
                    open_index = Some(index);
                }
            });

        if close {
            self.close_local_history_browser();
        } else if let Some(index) = open_index {
            self.open_local_history_browser_selection(index);
        }
    }
}

async fn read_local_history_browser_snapshot_text(snapshot_path: &Path) -> Result<String, String> {
    let bytes = match read_file_bytes_with_limit_async(snapshot_path, LOCAL_HISTORY_MAX_BYTES).await
    {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err("snapshot is no longer on disk".to_owned());
        }
        Err(error) if error.kind() == ErrorKind::InvalidData => {
            return Err("snapshot exceeds the local history size limit".to_owned());
        }
        Err(error) => return Err(error.to_string()),
    };
    String::from_utf8(bytes).map_err(|_| "snapshot contains binary data".to_owned())
}

fn local_history_browser_row_label(sequence: u128, modified: Option<SystemTime>) -> String {
    let mut label = format!("#{sequence}");
    if let Some(modified) = modified {
        label.push_str("   ");
        label.push_str(&local_history_browser_modified_label(modified));
    }
    label
}

fn local_history_browser_revision_label(
    path_label: &str,
    sequence: u128,
    modified: Option<SystemTime>,
) -> String {
    let mut label = format!("{path_label} (Local History #{sequence}");
    if let Some(modified) = modified {
        label.push_str(", ");
        label.push_str(&local_history_browser_modified_label(modified));
    }
    label.push(')');
    label
}

fn local_history_browser_revision_target(path_label: &str, sequence: u128) -> String {
    format!("{path_label} local history snapshot {sequence}")
}

fn local_history_browser_modified_label(modified: SystemTime) -> String {
    sanitized_display_label_cow(
        &format_snapshot_timestamp_utc(modified),
        LOCAL_HISTORY_BROWSER_MODIFIED_LABEL_MAX_CHARS,
        "unknown time",
    )
    .into_owned()
}

fn local_history_browser_header_label(path_label: &str, snapshot_count: usize) -> String {
    format!("{path_label}: {snapshot_count} snapshots")
}

fn local_history_browser_loading_status(path_label: &str, sequence: u128) -> String {
    format!("Loading local history snapshot {sequence} for {path_label}")
}

fn local_history_browser_opened_status(
    path_label: &str,
    sequence: u128,
    snapshot_label: &str,
) -> String {
    format!("Opened local history snapshot {sequence} for {path_label} from {snapshot_label}")
}

fn local_history_browser_failed_status(path_label: &str, sequence: u128, reason: &str) -> String {
    format!("Could not open local history snapshot {sequence} for {path_label}: {reason}")
}

/// Formats a snapshot mtime as a fixed-width `YYYY-MM-DD HH:MM:SS UTC`
/// label. UTC keeps the conversion deterministic without pulling in a
/// timezone database; the label identifies the revision rather than
/// scheduling work in a specific zone.
fn format_snapshot_timestamp_utc(modified: SystemTime) -> String {
    let seconds = match modified.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => -(error.duration().as_secs() as i64),
    };
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

/// Inverse of days-from-civil: converts a count of days since 1970-01-01 to
/// a proleptic Gregorian calendar date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

fn local_history_browser_status_label(ui: &mut egui::Ui, text: Cow<'_, str>) {
    ui.label(
        RichText::new(text)
            .small()
            .color(ui.visuals().weak_text_color()),
    );
}

#[cfg(test)]
mod tests {
    use super::{
        civil_from_days, format_snapshot_timestamp_utc, local_history_browser_failed_status,
        local_history_browser_header_label, local_history_browser_loading_status,
        local_history_browser_modified_label, local_history_browser_opened_status,
        local_history_browser_revision_label, local_history_browser_revision_target,
        local_history_browser_row_label,
    };
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn snapshot_time(seconds: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn local_history_browser_timestamps_format_utc_calendar_dates() {
        assert_eq!(
            format_snapshot_timestamp_utc(UNIX_EPOCH),
            "1970-01-01 00:00:00 UTC"
        );
        assert_eq!(
            format_snapshot_timestamp_utc(snapshot_time(1_700_000_000)),
            "2023-11-14 22:13:20 UTC"
        );
        assert_eq!(
            format_snapshot_timestamp_utc(snapshot_time(951_782_400)),
            "2000-02-29 00:00:00 UTC"
        );
        assert_eq!(
            format_snapshot_timestamp_utc(UNIX_EPOCH - Duration::from_secs(86_400)),
            "1969-12-31 00:00:00 UTC"
        );
    }

    #[test]
    fn local_history_browser_civil_from_days_resolves_leap_years() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(19_675), (2023, 11, 14));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn local_history_browser_row_labels_pair_sequence_with_timestamp() {
        let modified = snapshot_time(1_700_000_000);
        assert_eq!(
            local_history_browser_row_label(7, Some(modified)),
            "#7   2023-11-14 22:13:20 UTC"
        );
        assert_eq!(local_history_browser_row_label(3, None), "#3");
    }

    #[test]
    fn local_history_browser_revision_labels_include_sequence_and_timestamp() {
        let modified = snapshot_time(1_700_000_000);
        assert_eq!(
            local_history_browser_revision_label("main.rs", 7, Some(modified)),
            "main.rs (Local History #7, 2023-11-14 22:13:20 UTC)"
        );
        assert_eq!(
            local_history_browser_revision_label("main.rs", 7, None),
            "main.rs (Local History #7)"
        );
        assert_eq!(
            local_history_browser_revision_target("main.rs", 7),
            "main.rs local history snapshot 7"
        );
        assert_eq!(
            local_history_browser_modified_label(modified),
            "2023-11-14 22:13:20 UTC"
        );
    }

    #[test]
    fn local_history_browser_statuses_name_target_snapshot_and_failures() {
        assert_eq!(
            local_history_browser_loading_status("main.rs", 7),
            "Loading local history snapshot 7 for main.rs"
        );
        assert_eq!(
            local_history_browser_opened_status("main.rs", 7, "7.main.rs.bak"),
            "Opened local history snapshot 7 for main.rs from 7.main.rs.bak"
        );
        assert_eq!(
            local_history_browser_failed_status("main.rs", 7, "snapshot contains binary data"),
            "Could not open local history snapshot 7 for main.rs: snapshot contains binary data"
        );
        assert_eq!(
            local_history_browser_header_label("main.rs", 3),
            "main.rs: 3 snapshots"
        );
    }
}

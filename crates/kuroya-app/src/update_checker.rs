use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button, popup_button_enabled},
    transient_state::PendingExit,
    ui_event_channel::send_ui_event,
    ui_events::UiEvent,
    ui_icons::{IconKind, draw_icon, icon_button},
};
use anyhow::Context;
use eframe::egui::{self, Align, Color32, Context as EguiContext, Key, RichText, Stroke, Ui, vec2};
use kuroya_core::EditorSettings;
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const GITHUB_API_BASE: &str = "https://api.github.com/repos";
pub(crate) const DEFAULT_UPDATE_GITHUB_REPOSITORY: &str = "kaevris/kuroya";
const UPDATE_USER_AGENT: &str = concat!("Kuroya/", env!("CARGO_PKG_VERSION"));
const UPDATE_DOWNLOAD_DIR: &str = "kuroya-updates";
pub(crate) const AUTOMATIC_UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AvailableUpdate {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) asset: UpdateInstallerAsset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateInstallerAsset {
    pub(crate) name: String,
    pub(crate) browser_download_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateInstallerReady {
    pub(crate) latest_version: String,
    pub(crate) installer_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpdateCheckOutcome {
    UpToDate {
        current_version: String,
        latest_version: String,
    },
    UpdateAvailable(AvailableUpdate),
    MissingInstallerAsset {
        latest_version: String,
        release_url: String,
    },
}

impl UpdateCheckOutcome {
    pub(crate) fn status_text(&self) -> String {
        match self {
            Self::UpToDate {
                current_version,
                latest_version,
            } => format!("Kuroya is up to date ({current_version}, latest {latest_version})"),
            Self::UpdateAvailable(update) => update.available_status_text(),
            Self::MissingInstallerAsset {
                latest_version,
                release_url,
            } => format!(
                "Kuroya {latest_version} is available, but no Windows installer asset was found: {release_url}"
            ),
        }
    }
}

impl AvailableUpdate {
    pub(crate) fn available_status_text(&self) -> String {
        format!(
            "Kuroya {} is available; installer {} is ready",
            self.latest_version, self.asset.name
        )
    }
}

impl UpdateInstallerReady {
    pub(crate) fn ready_status_text(&self) -> String {
        format!(
            "Kuroya {} installer is ready; restart to install",
            self.latest_version
        )
    }

    pub(crate) fn restart_status_text(&self) -> String {
        format!("Restarting Kuroya to install {}", self.latest_version)
    }

    pub(crate) fn launched_status_text(&self) -> String {
        format!(
            "Launched Kuroya {} installer {}",
            self.latest_version,
            self.installer_path.display()
        )
    }
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

impl KuroyaApp {
    pub(crate) fn check_for_updates(&mut self) {
        self.start_update_check(true);
    }

    pub(crate) fn flush_due_update_checks(&mut self, now: Instant) -> usize {
        if !automatic_update_check_due(
            now,
            self.next_automatic_update_check_at,
            self.update_check_in_flight,
            self.update_download_in_flight,
            self.available_update.is_some() || self.pending_update_install.is_some(),
        ) {
            return 0;
        }

        self.next_automatic_update_check_at = next_automatic_update_check_at(now);
        usize::from(self.start_update_check(false))
    }

    fn start_update_check(&mut self, manual: bool) -> bool {
        if self.update_check_in_flight {
            if manual {
                self.status = "Already checking for updates".to_owned();
            }
            return false;
        }

        if self.update_download_in_flight {
            if manual {
                self.status = "Update installer is already downloading".to_owned();
            }
            return false;
        }

        if let Some(update) = &self.available_update {
            if manual {
                self.status = update.available_status_text();
            }
            return false;
        }

        if let Some(update) = &self.pending_update_install {
            if manual {
                self.status = update.ready_status_text();
            }
            return false;
        }

        let Some(repository) = configured_update_repository(&self.settings) else {
            if manual {
                self.status = update_repository_not_configured_status();
            }
            return false;
        };

        self.update_check_in_flight = true;
        self.update_check_manual = manual;
        if manual {
            self.status = format!("Checking GitHub releases for {repository}");
        }
        self.record_async_task_started("Update Check", "GitHub Releases");
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let event = match check_latest_github_release(&repository).await {
                Ok(outcome) => UiEvent::UpdateCheckFinished(outcome),
                Err(error) => UiEvent::UpdateCheckFailed {
                    error: error.to_string(),
                },
            };
            send_ui_event(&tx, event);
        });
        true
    }

    pub(crate) fn apply_update_check_finished(&mut self, outcome: UpdateCheckOutcome) {
        let manual = self.finish_update_check();
        if matches!(outcome, UpdateCheckOutcome::UpToDate { .. }) && !manual {
            return;
        }

        if matches!(outcome, UpdateCheckOutcome::MissingInstallerAsset { .. }) && !manual {
            return;
        }

        match outcome {
            UpdateCheckOutcome::UpdateAvailable(update) => {
                self.status = update.available_status_text();
                self.available_update = Some(update);
            }
            outcome => {
                self.status = outcome.status_text();
            }
        }
    }

    pub(crate) fn apply_update_check_failed(&mut self, error: String) {
        let manual = self.finish_update_check();
        if !manual {
            return;
        }

        let error = display_update_error(&error);
        self.status = format!("Could not check for updates: {error}");
    }

    pub(crate) fn install_available_update(&mut self) {
        if self.update_download_in_flight {
            self.status = "Update installer is already downloading".to_owned();
            return;
        }

        let Some(update) = self.available_update.clone() else {
            self.status = "No update is ready to install".to_owned();
            return;
        };

        self.available_update = None;
        self.update_download_in_flight = true;
        self.status = format!(
            "Downloading Kuroya {} installer {}",
            update.latest_version, update.asset.name
        );
        self.record_async_task_started("Update Download", &update.latest_version);
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let event = match download_update_installer(update).await {
                Ok(update) => UiEvent::UpdateInstallerReady(update),
                Err(UpdateDownloadError {
                    latest_version,
                    error,
                }) => UiEvent::UpdateDownloadFailed {
                    latest_version,
                    error: error.to_string(),
                },
            };
            send_ui_event(&tx, event);
        });
    }

    pub(crate) fn apply_update_installer_ready(&mut self, update: UpdateInstallerReady) {
        self.update_download_in_flight = false;
        self.available_update = None;
        self.pending_update_install = Some(update);
        self.next_automatic_update_check_at = next_automatic_update_check_at(Instant::now());
        self.restart_to_install_update();
    }

    pub(crate) fn apply_update_download_failed(&mut self, latest_version: String, error: String) {
        self.update_download_in_flight = false;
        let error = display_update_error(&error);
        self.status = format!("Could not download Kuroya {latest_version}: {error}");
    }

    pub(crate) fn restart_to_install_update(&mut self) {
        let Some(update) = self.pending_update_install.as_ref() else {
            self.status = "No update installer is ready".to_owned();
            return;
        };
        if self.exit_confirmed || self.pending_exit.is_some() {
            self.status = "Update restart is already pending".to_owned();
            return;
        }

        let restart_status = update.restart_status_text();
        self.clear_pending_workspace_switch_for_exit();
        self.status = restart_status;
        let dirty_count = self
            .buffers
            .iter()
            .filter(|buffer| buffer.is_dirty())
            .count();
        let terminal_count = self.terminal_exit_confirmation_count();
        if dirty_count == 0 && terminal_count == 0 {
            self.exit_confirmed = true;
        } else {
            self.pending_exit = Some(PendingExit::Confirm);
        }
    }

    pub(crate) fn launch_pending_update_installer_before_exit(&mut self) -> bool {
        let Some(update) = self.pending_update_install.take() else {
            return true;
        };
        match launch_update_installer(&update.installer_path) {
            Ok(()) => {
                self.status = update.launched_status_text();
                true
            }
            Err(error) => {
                let error = display_update_error(&error.to_string());
                self.exit_confirmed = false;
                self.pending_update_install = Some(update);
                self.status = format!("Could not launch update installer: {error}");
                false
            }
        }
    }

    pub(crate) fn dismiss_update_prompt(&mut self) {
        if let Some(update) = self.available_update.take() {
            self.next_automatic_update_check_at = next_automatic_update_check_at(Instant::now());
            self.status = format!("Kuroya {} update postponed", update.latest_version);
        }
    }

    pub(crate) fn dismiss_pending_update_install(&mut self) {
        if let Some(update) = self.pending_update_install.take() {
            self.next_automatic_update_check_at = next_automatic_update_check_at(Instant::now());
            self.status = format!("Kuroya {} update postponed", update.latest_version);
        }
    }

    pub(crate) fn render_update_prompt(&mut self, ctx: &EguiContext) {
        let Some(update) = self.available_update.clone() else {
            return;
        };

        let mut action = UpdatePromptAction::None;
        let mut window_open = true;
        egui::Window::new("Update Available")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(ctx, 24.0))
            .open(&mut window_open)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([520.0, 292.0])
            .frame(update_prompt_frame(ctx))
            .show(ctx, |ui| {
                render_update_dialog_header(
                    ui,
                    IconKind::Refresh,
                    "Update Available",
                    &format!("Kuroya {} is ready to download", update.latest_version),
                    &mut action,
                );
                ui.add_space(14.0);
                ui.label("Install the latest Windows release now. Kuroya will download the installer first.");
                ui.add_space(14.0);
                render_update_version_rows(
                    ui,
                    "update_available_versions",
                    &[
                        ("Current", update.current_version.as_str()),
                        ("Available", update.latest_version.as_str()),
                    ],
                );
                ui.add_space(12.0);
                render_update_installer_row(ui, &update.asset.name);

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    action = UpdatePromptAction::Later;
                }

                render_update_dialog_footer(
                    ui,
                    "Install",
                    !self.update_download_in_flight,
                    UpdatePromptAction::Install,
                    &mut action,
                );
            });

        if !window_open && matches!(action, UpdatePromptAction::None) {
            action = UpdatePromptAction::Later;
        }

        match action {
            UpdatePromptAction::Install => self.install_available_update(),
            UpdatePromptAction::Restart => {}
            UpdatePromptAction::Later => self.dismiss_update_prompt(),
            UpdatePromptAction::None => {}
        }
    }

    pub(crate) fn render_update_ready_prompt(&mut self, ctx: &EguiContext) {
        if self.pending_exit.is_some() || self.exit_confirmed {
            return;
        }
        let Some(update) = self.pending_update_install.clone() else {
            return;
        };

        let mut action = UpdatePromptAction::None;
        let mut window_open = true;
        egui::Window::new("Update Ready")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .open(&mut window_open)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([520.0, 270.0])
            .frame(update_prompt_frame(ctx))
            .show(ctx, |ui| {
                render_update_dialog_header(
                    ui,
                    IconKind::Refresh,
                    "Update Ready",
                    &format!("Kuroya {} has been downloaded", update.latest_version),
                    &mut action,
                );
                ui.add_space(14.0);
                ui.label("Restart Kuroya to replace the current installation.");
                ui.add_space(14.0);
                render_update_version_rows(
                    ui,
                    "update_ready_version",
                    &[("Version", update.latest_version.as_str())],
                );
                ui.add_space(12.0);
                render_update_installer_row(ui, &installer_path_label(&update.installer_path));

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    action = UpdatePromptAction::Later;
                }

                render_update_dialog_footer(
                    ui,
                    "Restart",
                    true,
                    UpdatePromptAction::Restart,
                    &mut action,
                );
            });

        if !window_open && matches!(action, UpdatePromptAction::None) {
            action = UpdatePromptAction::Later;
        }

        match action {
            UpdatePromptAction::Restart => self.restart_to_install_update(),
            UpdatePromptAction::Later => self.dismiss_pending_update_install(),
            UpdatePromptAction::Install | UpdatePromptAction::None => {}
        }
    }

    fn finish_update_check(&mut self) -> bool {
        let manual = self.update_check_manual;
        self.update_check_in_flight = false;
        self.update_check_manual = false;
        manual
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdatePromptAction {
    None,
    Install,
    Restart,
    Later,
}

fn update_prompt_frame(ctx: &EguiContext) -> egui::Frame {
    egui::Frame::window(&ctx.style())
        .inner_margin(egui::Margin::symmetric(18, 16))
        .corner_radius(egui::CornerRadius::same(6))
}

fn render_update_dialog_header(
    ui: &mut Ui,
    icon: IconKind,
    title: &str,
    subtitle: &str,
    action: &mut UpdatePromptAction,
) {
    ui.horizontal(|ui| {
        render_update_icon_badge(ui, icon);
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.add_space(1.0);
            ui.label(RichText::new(title).size(18.0).strong());
            ui.label(
                RichText::new(subtitle)
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, IconKind::Close, "Dismiss update prompt").clicked() {
                *action = UpdatePromptAction::Later;
            }
        });
    });
    ui.add_space(12.0);
    ui.separator();
}

fn render_update_icon_badge(ui: &mut Ui, icon: IconKind) {
    let accent = ui.visuals().selection.stroke.color;
    let fill = translucent_color(accent, 32);
    let stroke = Stroke::new(1.0, translucent_color(accent, 108));
    let (rect, response) = ui.allocate_exact_size(vec2(42.0, 42.0), egui::Sense::hover());

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter()
        .rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);
    let icon_rect = egui::Rect::from_center_size(rect.center(), vec2(24.0, 24.0));
    draw_icon(ui, icon_rect, icon, accent);
    response.on_hover_text("Kuroya update");
}

fn render_update_version_rows(ui: &mut Ui, id: &'static str, rows: &[(&str, &str)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .spacing(vec2(18.0, 7.0))
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(
                    RichText::new(*label)
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
                ui.label(RichText::new(*value).monospace().strong());
                ui.end_row();
            }
        });
}

fn render_update_installer_row(ui: &mut Ui, installer: &str) {
    ui.label(
        RichText::new("Installer")
            .small()
            .color(ui.visuals().weak_text_color()),
    );
    egui::Frame::new()
        .fill(ui.visuals().code_bg_color)
        .stroke(Stroke::new(
            1.0,
            ui.visuals().widgets.inactive.bg_stroke.color,
        ))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                crate::ui_icons::icon_label(
                    ui,
                    IconKind::File,
                    ui.visuals().weak_text_color(),
                    "Installer file",
                );
                ui.add(egui::Label::new(RichText::new(installer).monospace().small()).wrap());
            });
        });
}

fn render_update_dialog_footer(
    ui: &mut Ui,
    primary_label: &str,
    primary_enabled: bool,
    primary_action: UpdatePromptAction,
    action: &mut UpdatePromptAction,
) {
    ui.add_space(14.0);
    ui.separator();
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
        if popup_button_enabled(ui, primary_enabled, primary_label, PopupButtonKind::Primary)
            .clicked()
        {
            *action = primary_action;
        }
        if popup_button(ui, "Later", PopupButtonKind::Secondary).clicked() {
            *action = UpdatePromptAction::Later;
        }
    });
}

fn translucent_color(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

struct UpdateDownloadError {
    latest_version: String,
    error: anyhow::Error,
}

pub(crate) fn configured_update_repository(settings: &EditorSettings) -> Option<String> {
    normalize_github_repository(&settings.updates_github_repository)
        .or_else(|| option_env!("KUROYA_UPDATE_REPOSITORY").and_then(normalize_github_repository))
        .or_else(|| normalize_github_repository(DEFAULT_UPDATE_GITHUB_REPOSITORY))
}

pub(crate) fn update_repository_not_configured_status() -> String {
    "Updates are not configured; set updates_github_repository to owner/repo in settings.toml"
        .to_owned()
}

async fn check_latest_github_release(repository: &str) -> anyhow::Result<UpdateCheckOutcome> {
    let release = fetch_latest_release(repository).await?;
    Ok(update_check_outcome_from_release(
        release,
        env!("CARGO_PKG_VERSION"),
    ))
}

fn update_check_outcome_from_release(
    release: GitHubRelease,
    current_version: &str,
) -> UpdateCheckOutcome {
    let latest_version = display_release_version(&release.tag_name);
    if !release_is_newer(current_version, &release.tag_name) {
        return UpdateCheckOutcome::UpToDate {
            current_version: current_version.to_owned(),
            latest_version,
        };
    }

    let Some(asset) = select_windows_installer_asset(&release.assets) else {
        return UpdateCheckOutcome::MissingInstallerAsset {
            latest_version,
            release_url: release.html_url,
        };
    };

    UpdateCheckOutcome::UpdateAvailable(AvailableUpdate {
        current_version: current_version.to_owned(),
        latest_version,
        asset: UpdateInstallerAsset {
            name: asset.name.clone(),
            browser_download_url: asset.browser_download_url.clone(),
        },
    })
}

async fn fetch_latest_release(repository: &str) -> anyhow::Result<GitHubRelease> {
    let client = reqwest::Client::builder()
        .user_agent(UPDATE_USER_AGENT)
        .timeout(Duration::from_secs(60))
        .build()
        .context("could not create update HTTP client")?;
    let url = format!("{GITHUB_API_BASE}/{repository}/releases/latest");
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("could not reach GitHub releases")?
        .error_for_status()
        .context("GitHub releases request failed")?;
    response
        .json::<GitHubRelease>()
        .await
        .context("could not parse GitHub release")
}

async fn download_update_installer(
    update: AvailableUpdate,
) -> Result<UpdateInstallerReady, UpdateDownloadError> {
    let latest_version = update.latest_version.clone();
    let installer_path = download_release_asset(&update.asset)
        .await
        .map_err(|error| UpdateDownloadError {
            latest_version: latest_version.clone(),
            error,
        })?;
    Ok(UpdateInstallerReady {
        latest_version,
        installer_path,
    })
}

async fn download_release_asset(asset: &UpdateInstallerAsset) -> anyhow::Result<PathBuf> {
    let client = reqwest::Client::builder()
        .user_agent(UPDATE_USER_AGENT)
        .timeout(Duration::from_secs(300))
        .build()
        .context("could not create update download client")?;
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .with_context(|| format!("could not download {}", asset.name))?
        .error_for_status()
        .with_context(|| format!("download failed for {}", asset.name))?
        .bytes()
        .await
        .with_context(|| format!("could not read {}", asset.name))?;

    let download_dir = std::env::temp_dir().join(UPDATE_DOWNLOAD_DIR);
    tokio::fs::create_dir_all(&download_dir)
        .await
        .with_context(|| format!("could not create {}", download_dir.display()))?;
    let file_name = safe_installer_file_name(&asset.name);
    let installer_path = download_dir.join(file_name);
    tokio::fs::write(&installer_path, bytes)
        .await
        .with_context(|| format!("could not write {}", installer_path.display()))?;
    Ok(installer_path)
}

fn launch_update_installer(installer_path: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new(installer_path)
            .args(inno_update_installer_args(
                current_update_install_dir().as_deref(),
            ))
            .spawn()
            .with_context(|| format!("could not launch {}", installer_path.display()))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = installer_path;
        anyhow::bail!("automatic installer launch is only supported on Windows")
    }
}

fn inno_update_installer_args(install_dir: Option<&Path>) -> Vec<String> {
    let mut args = vec![
        "/SP-".to_owned(),
        "/SILENT".to_owned(),
        "/SUPPRESSMSGBOXES".to_owned(),
        "/NORESTART".to_owned(),
        "/CLOSEAPPLICATIONS".to_owned(),
        "/RESTARTAPPLICATIONS".to_owned(),
        "/KuroyaRestart=1".to_owned(),
    ];
    if let Some(install_dir) = install_dir {
        args.push(format!("/DIR={}", install_dir.display()));
    }
    args
}

fn current_update_install_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    update_install_dir_from_exe(&exe)
}

fn update_install_dir_from_exe(exe: &Path) -> Option<PathBuf> {
    let file_name = exe.file_name()?.to_string_lossy();
    if !file_name.eq_ignore_ascii_case("kuroya.exe") {
        return None;
    }
    let install_dir = exe.parent()?;
    (!is_cargo_build_output_dir(install_dir)).then(|| install_dir.to_path_buf())
}

fn is_cargo_build_output_dir(dir: &Path) -> bool {
    let components = dir
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    components
        .windows(2)
        .any(|window| window[0] == "target" && matches!(window[1].as_str(), "debug" | "release"))
}

fn select_windows_installer_asset(assets: &[GitHubReleaseAsset]) -> Option<&GitHubReleaseAsset> {
    assets
        .iter()
        .filter(|asset| asset.name.to_ascii_lowercase().ends_with(".exe"))
        .max_by_key(|asset| installer_asset_score(&asset.name))
}

fn installer_asset_score(name: &str) -> i32 {
    let lower = name.to_ascii_lowercase();
    let mut score = 1;
    if lower.contains("setup") {
        score += 8;
    }
    if lower.contains("install") {
        score += 6;
    }
    if lower.contains("kuroya") {
        score += 4;
    }
    if lower.contains("x64") || lower.contains("amd64") {
        score += 2;
    }
    score
}

fn safe_installer_file_name(name: &str) -> String {
    let mut output = String::with_capacity(name.len().max("Kuroya-Setup.exe".len()));
    for ch in name.chars().take(160) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            output.push(ch);
        } else if ch.is_whitespace() {
            output.push('-');
        }
    }
    if output.is_empty() {
        output.push_str("Kuroya-Setup.exe");
    }
    if !output.to_ascii_lowercase().ends_with(".exe") {
        output.push_str(".exe");
    }
    output
}

fn display_release_version(tag: &str) -> String {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        "unknown".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn release_is_newer(current_version: &str, release_tag: &str) -> bool {
    let Some(current) = parse_release_version(current_version) else {
        return false;
    };
    let Some(latest) = parse_release_version(release_tag) else {
        return false;
    };
    latest > current
}

fn parse_release_version(value: &str) -> Option<Vec<u64>> {
    let value = value.trim().trim_start_matches(['v', 'V']);
    let value = value.split(['-', '+']).next().unwrap_or_default();
    let mut parts = Vec::new();
    for part in value.split('.') {
        if part.is_empty() {
            return None;
        }
        parts.push(part.parse::<u64>().ok()?);
    }
    if parts.is_empty() {
        return None;
    }
    while parts.len() < 3 {
        parts.push(0);
    }
    Some(parts)
}

fn normalize_github_repository(input: &str) -> Option<String> {
    let mut value = input.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(rest) = value.strip_prefix("https://github.com/") {
        value = rest;
    } else if let Some(rest) = value.strip_prefix("http://github.com/") {
        value = rest;
    } else if let Some(rest) = value.strip_prefix("github.com/") {
        value = rest;
    }
    value = value.trim_matches('/');
    let mut parts = value.split('/');
    let owner = parts.next()?;
    let mut repo = parts.next()?;
    if let Some(stripped) = repo.strip_suffix(".git") {
        repo = stripped;
    }
    if !github_repository_segment_is_valid(owner) || !github_repository_segment_is_valid(repo) {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn github_repository_segment_is_valid(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 100
        && segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        && !segment.starts_with('.')
        && !segment.ends_with('.')
        && !segment.contains("..")
}

pub(crate) fn initial_automatic_update_check_at(now: Instant) -> Instant {
    now
}

pub(crate) fn next_automatic_update_check_at(now: Instant) -> Instant {
    checked_instant_add(now, AUTOMATIC_UPDATE_CHECK_INTERVAL)
}

pub(crate) fn automatic_update_check_due(
    now: Instant,
    next_check_at: Instant,
    check_in_flight: bool,
    install_in_flight: bool,
    prompt_open: bool,
) -> bool {
    now >= next_check_at && !check_in_flight && !install_in_flight && !prompt_open
}

pub(crate) fn automatic_update_wakeup_after(
    next_check_at: Instant,
    now: Instant,
    blocked: bool,
) -> Option<Duration> {
    (!blocked).then_some(next_check_at.saturating_duration_since(now))
}

fn checked_instant_add(now: Instant, delay: Duration) -> Instant {
    now.checked_add(delay).unwrap_or(now)
}

fn display_update_error(error: &str) -> String {
    crate::path_display::display_error_label_cow(error).into_owned()
}

fn installer_path_label(path: &Path) -> String {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .filter(|file_name| !file_name.trim().is_empty())
        .unwrap_or("installer")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn normalizes_supported_github_repository_inputs() {
        assert_eq!(
            normalize_github_repository("owner/repo"),
            Some("owner/repo".to_owned())
        );
        assert_eq!(
            normalize_github_repository("https://github.com/owner/repo"),
            Some("owner/repo".to_owned())
        );
        assert_eq!(
            normalize_github_repository("github.com/owner/repo.git"),
            Some("owner/repo".to_owned())
        );
        assert_eq!(
            normalize_github_repository("https://github.com/owner/repo/releases/latest"),
            Some("owner/repo".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_github_repository_inputs() {
        assert_eq!(normalize_github_repository(""), None);
        assert_eq!(normalize_github_repository("owner"), None);
        assert_eq!(normalize_github_repository("owner/re po"), None);
        assert_eq!(normalize_github_repository("../repo"), None);
        assert_eq!(normalize_github_repository("owner/.."), None);
    }

    #[test]
    fn compares_release_versions() {
        assert!(release_is_newer("0.1.0", "v0.1.1"));
        assert!(release_is_newer("0.1.9", "v0.2.0"));
        assert!(!release_is_newer("0.2.0", "v0.1.9"));
        assert!(!release_is_newer("0.1.0", "v0.1.0"));
        assert!(!release_is_newer("0.1.0", "nightly"));
    }

    #[test]
    fn selects_best_windows_installer_asset() {
        let assets = vec![
            GitHubReleaseAsset {
                name: "source.zip".to_owned(),
                browser_download_url: "https://example.test/source.zip".to_owned(),
            },
            GitHubReleaseAsset {
                name: "kuroya-portable.exe".to_owned(),
                browser_download_url: "https://example.test/portable.exe".to_owned(),
            },
            GitHubReleaseAsset {
                name: "Kuroya-Setup-0.2.0.exe".to_owned(),
                browser_download_url: "https://example.test/setup.exe".to_owned(),
            },
        ];

        assert_eq!(
            select_windows_installer_asset(&assets).map(|asset| asset.name.as_str()),
            Some("Kuroya-Setup-0.2.0.exe")
        );
    }

    #[test]
    fn sanitizes_installer_asset_file_names() {
        assert_eq!(
            safe_installer_file_name("Kuroya Setup 0.2.0.exe"),
            "Kuroya-Setup-0.2.0.exe"
        );
        assert_eq!(safe_installer_file_name(""), "Kuroya-Setup.exe");
        assert_eq!(safe_installer_file_name("installer"), "installer.exe");
    }

    #[test]
    fn release_outcome_reports_available_installer_without_launching_it() {
        let release = GitHubRelease {
            tag_name: "v0.2.0".to_owned(),
            html_url: "https://github.com/owner/repo/releases/tag/v0.2.0".to_owned(),
            assets: vec![GitHubReleaseAsset {
                name: "Kuroya-Setup-0.2.0.exe".to_owned(),
                browser_download_url: "https://example.test/Kuroya-Setup-0.2.0.exe".to_owned(),
            }],
        };

        let outcome = update_check_outcome_from_release(release, "0.1.0");

        assert_eq!(
            outcome,
            UpdateCheckOutcome::UpdateAvailable(AvailableUpdate {
                current_version: "0.1.0".to_owned(),
                latest_version: "v0.2.0".to_owned(),
                asset: UpdateInstallerAsset {
                    name: "Kuroya-Setup-0.2.0.exe".to_owned(),
                    browser_download_url: "https://example.test/Kuroya-Setup-0.2.0.exe".to_owned(),
                },
            })
        );
    }

    #[test]
    fn release_outcome_reports_missing_installer_for_new_release_without_exe() {
        let release = GitHubRelease {
            tag_name: "v0.2.0".to_owned(),
            html_url: "https://github.com/owner/repo/releases/tag/v0.2.0".to_owned(),
            assets: vec![GitHubReleaseAsset {
                name: "source.zip".to_owned(),
                browser_download_url: "https://example.test/source.zip".to_owned(),
            }],
        };

        assert_eq!(
            update_check_outcome_from_release(release, "0.1.0"),
            UpdateCheckOutcome::MissingInstallerAsset {
                latest_version: "v0.2.0".to_owned(),
                release_url: "https://github.com/owner/repo/releases/tag/v0.2.0".to_owned(),
            }
        );
    }

    #[test]
    fn automatic_update_check_gate_waits_for_due_time_and_idle_updater() {
        let now = Instant::now();
        let due = now - Duration::from_secs(1);
        let future = now + Duration::from_secs(1);

        assert!(automatic_update_check_due(now, due, false, false, false));
        assert!(!automatic_update_check_due(
            now, future, false, false, false
        ));
        assert!(!automatic_update_check_due(now, due, true, false, false));
        assert!(!automatic_update_check_due(now, due, false, true, false));
        assert!(!automatic_update_check_due(now, due, false, false, true));
    }

    #[test]
    fn initial_automatic_update_check_is_due_immediately() {
        let now = Instant::now();

        assert_eq!(initial_automatic_update_check_at(now), now);
        assert!(automatic_update_check_due(
            now,
            initial_automatic_update_check_at(now),
            false,
            false,
            false
        ));
    }

    #[test]
    fn automatic_update_wakeup_reports_remaining_delay_when_unblocked() {
        let now = Instant::now();
        let due = now + Duration::from_secs(30);

        assert_eq!(
            automatic_update_wakeup_after(due, now, false),
            Some(Duration::from_secs(30))
        );
        assert_eq!(automatic_update_wakeup_after(due, now, true), None);
    }

    #[test]
    fn inno_update_installer_args_use_silent_restart_mode_and_current_dir() {
        let install_dir = Path::new(r"C:\Users\ESA\AppData\Local\Programs\Kuroya");

        assert_eq!(
            inno_update_installer_args(Some(install_dir)),
            vec![
                "/SP-",
                "/SILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART",
                "/CLOSEAPPLICATIONS",
                "/RESTARTAPPLICATIONS",
                "/KuroyaRestart=1",
                r"/DIR=C:\Users\ESA\AppData\Local\Programs\Kuroya",
            ]
        );
    }

    #[test]
    fn update_install_dir_uses_installed_exe_but_skips_cargo_build_output() {
        assert_eq!(
            update_install_dir_from_exe(Path::new(
                r"C:\Users\ESA\AppData\Local\Programs\Kuroya\kuroya.exe"
            )),
            Some(PathBuf::from(r"C:\Users\ESA\AppData\Local\Programs\Kuroya"))
        );
        assert_eq!(
            update_install_dir_from_exe(Path::new(
                r"C:\Users\ESA\Desktop\anime\test\target\release\kuroya.exe"
            )),
            None
        );
        assert_eq!(
            update_install_dir_from_exe(Path::new(
                r"C:\Users\ESA\AppData\Local\Programs\Kuroya\other.exe"
            )),
            None
        );
    }
}

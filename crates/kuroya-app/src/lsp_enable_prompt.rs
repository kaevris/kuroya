use crate::{
    KuroyaApp,
    lsp_installer::{
        LspInstallFailure, RepoBundleInstall, RustInstallDecision, detect_rustup,
        download_and_install_lsp_bundle, lsp_bundle_command_override, rust_install_decision,
        spawn_lsp_download_progress_task,
    },
    lsp_runtime::lsp_client_key_language,
    ui_event_channel::send_ui_event,
    ui_events::UiEvent,
    update_checker::format_byte_size,
};
use eframe::egui::{self, Align2, Context, RichText};
use kuroya_core::{
    LspServerConfig, default_lsp_server_for_language, lsp_language_id_for_path,
    lsp_registry::{
        LspInstallKind, current_lsp_install_platform, lsp_binary_on_path, registry_entry_for,
    },
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

const LSP_INSTALL_VERIFY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LspEnablePrompt {
    pub(crate) language: String,
    pub(crate) command: String,
    pub(crate) file_name: String,
    pub(crate) binary_present: bool,
    pub(crate) install: Option<LspInstallAction>,
    pub(crate) install_in_flight: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LspInstallAction {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) verify_command: String,
    pub(crate) verify_args: Vec<String>,
    pub(crate) plan: LspInstallPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LspInstallPlan {
    Repo {
        asset: String,
        launch: String,
    },
    Npm {
        npm_fallback: String,
        reason: String,
    },
    RustupThenRepo {
        component_args: Vec<String>,
        asset: String,
        launch: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LspEnablePromptAction {
    None,
    Enable,
    Install,
    NotNow,
}

pub(crate) fn lsp_prompt_install_action(config: &LspServerConfig) -> Option<LspInstallAction> {
    if lsp_binary_on_path(&config.command) || lsp_bundle_command_override(config).is_some() {
        return None;
    }
    let definition = registry_entry_for(&config.language)?;
    let plan = match definition.install {
        LspInstallKind::Repo => {
            let asset = definition.repo_asset_name(current_lsp_install_platform())?;
            LspInstallPlan::Repo {
                asset,
                launch: definition.launch.clone(),
            }
        }
        LspInstallKind::Npm => LspInstallPlan::Npm {
            npm_fallback: definition.npm_fallback()?.to_owned(),
            reason: definition.reason().unwrap_or_default().to_owned(),
        },
        LspInstallKind::RustupThenRepo => {
            let asset = definition.repo_asset_name(current_lsp_install_platform())?;
            LspInstallPlan::RustupThenRepo {
                component_args: vec![
                    "component".to_owned(),
                    "add".to_owned(),
                    definition.launch.clone(),
                ],
                asset,
                launch: definition.launch.clone(),
            }
        }
    };
    Some(LspInstallAction {
        id: definition.id.clone(),
        display_name: definition.display_name.clone(),
        verify_command: definition.verify.command.clone(),
        verify_args: definition.verify.args.clone(),
        plan,
    })
}

pub(crate) fn lsp_install_button_label(action: &LspInstallAction) -> String {
    match action.plan {
        LspInstallPlan::Npm { .. } => "Install via npm".to_owned(),
        _ => format!("Install {}", action.display_name),
    }
}

pub(crate) fn lsp_install_note(action: &LspInstallAction) -> Option<String> {
    match &action.plan {
        LspInstallPlan::Npm { reason, .. } if !reason.is_empty() => Some(reason.clone()),
        _ => None,
    }
}

impl KuroyaApp {
    pub(crate) fn maybe_show_lsp_enable_prompt(&mut self) {
        if self.lsp_enable_prompt.is_some() {
            return;
        }
        if !self.settings.lsp_suggest_missing_servers || !self.workspace_trusted {
            return;
        }

        let enabled_languages: Vec<String> = self
            .settings
            .lsp_server_configs()
            .iter()
            .map(|config| config.language.clone())
            .collect();
        let declined = self.lsp_enable_prompt_declined.clone();
        let Some(buffer) = self.active_buffer() else {
            return;
        };
        let language =
            lsp_language_id_for_path(buffer.language(), buffer.path().map(|path| path.as_path()))
                .to_owned();
        let file_name = buffer
            .path()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned());

        let Some(language) =
            lsp_enable_prompt_candidate(&declined, &enabled_languages, Some(&language))
        else {
            return;
        };
        let Some(default) = default_lsp_server_for_language(&language) else {
            return;
        };
        let mut command = default.command.clone();
        for arg in &default.args {
            command.push(' ');
            command.push_str(arg);
        }
        let binary_present =
            lsp_binary_on_path(&default.command) || lsp_bundle_command_override(&default).is_some();
        let install = if binary_present {
            None
        } else {
            lsp_prompt_install_action(&default)
        };
        self.lsp_enable_prompt = Some(LspEnablePrompt {
            language,
            command,
            file_name: file_name.unwrap_or_else(|| "This file".to_owned()),
            binary_present,
            install,
            install_in_flight: false,
        });
    }

    pub(crate) fn render_lsp_enable_prompt(&mut self, ctx: &Context) {
        let Some(prompt) = self.lsp_enable_prompt.clone() else {
            return;
        };

        let mut action = LspEnablePromptAction::None;
        let visuals = ctx.style().visuals.clone();
        egui::Area::new(egui::Id::new("lsp-enable-prompt-card"))
            .order(egui::Order::Foreground)
            .anchor(Align2::RIGHT_BOTTOM, [-18.0, -42.0])
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_width(380.0);
                egui::Frame::new()
                    .fill(visuals.window_fill)
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        visuals.widgets.noninteractive.bg_stroke.color,
                    ))
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        let accent = visuals.selection.bg_fill;

                        ui.horizontal(|ui| {
                            let icon_rect = egui::Rect::from_center_size(
                                ui.cursor().left_center() + egui::vec2(11.0, 0.0),
                                egui::vec2(22.0, 22.0),
                            );
                            crate::ui_icon_shapes::draw_icon(
                                ui,
                                icon_rect,
                                crate::ui_icons::IconKind::Lsp,
                                accent,
                            );
                            ui.advance_cursor_after_rect(icon_rect);
                            ui.label(
                                RichText::new(format!(
                                    "Enable the {} language server?",
                                    prompt.language
                                ))
                                .heading()
                                .strong(),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let close_enabled = !prompt.install_in_flight;
                                    ui.add_enabled_ui(close_enabled, |ui| {
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
                                                egui::Id::new("lsp-enable-prompt-close"),
                                                egui::Sense::click(),
                                            )
                                            .clicked()
                                        {
                                            action = LspEnablePromptAction::NotNow;
                                        }
                                    });
                                },
                            );
                        });

                        ui.label(
                            RichText::new(format!(
                                "{} uses the built-in \"{}\" server, which is disabled.",
                                prompt.file_name, prompt.command
                            ))
                            .weak(),
                        );

                        ui.add_space(8.0);
                        if prompt.install_in_flight {
                            if let Some(install) = prompt.install.as_ref() {
                                ui.label(RichText::new(format!(
                                    "Installing {}…",
                                    install.display_name
                                )));
                            }
                        } else {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if let Some(install) = prompt.install.as_ref() {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(lsp_install_button_label(
                                                        install,
                                                    ))
                                                    .strong(),
                                                )
                                                .fill(ui.visuals().selection.bg_fill),
                                            )
                                            .clicked()
                                        {
                                            action = LspEnablePromptAction::Install;
                                        }
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new(RichText::new("Enable").strong())
                                                .fill(ui.visuals().selection.bg_fill),
                                        )
                                        .clicked()
                                    {
                                        action = LspEnablePromptAction::Enable;
                                    }
                                    if ui.button("Not now").clicked() {
                                        action = LspEnablePromptAction::NotNow;
                                    }
                                },
                            );
                            if let Some(install) = prompt.install.as_ref()
                                && let Some(note) = lsp_install_note(install)
                            {
                                ui.label(RichText::new(note).weak());
                            }
                            if prompt.install.is_none() && !prompt.binary_present {
                                ui.label(RichText::new(
                                    "No automatic install available for this platform",
                                ));
                            }
                        }
                    });
            });

        match action {
            LspEnablePromptAction::Enable => self.enable_lsp_from_prompt(&prompt.language),
            LspEnablePromptAction::Install => self.start_lsp_install_from_prompt(),
            LspEnablePromptAction::NotNow => self.decline_lsp_enable_prompt(&prompt.language),
            LspEnablePromptAction::None => {}
        }
    }

    fn start_lsp_install_from_prompt(&mut self) {
        let Some(prompt) = self.lsp_enable_prompt.as_ref() else {
            return;
        };
        if prompt.install_in_flight {
            return;
        }
        let Some(install) = prompt.install.clone() else {
            return;
        };
        if self.lsp_installs_in_flight.contains(&install.id) {
            return;
        }
        let language = prompt.language.clone();
        if let Some(prompt) = self.lsp_enable_prompt.as_mut() {
            prompt.install_in_flight = true;
        }
        self.lsp_installs_in_flight.push(install.id.clone());
        self.status = match install.plan {
            LspInstallPlan::Repo { .. } => lsp_downloading_status(&install.display_name),
            LspInstallPlan::Npm { .. } | LspInstallPlan::RustupThenRepo { .. } => {
                lsp_installing_status(&install.display_name)
            }
        };
        self.record_async_task_started("LSP Install", install.display_name.clone());
        let tx = self.tx.clone();
        let bytes_downloaded = Arc::clone(&self.lsp_downloaded_bytes);
        bytes_downloaded.store(0, Ordering::Relaxed);
        self.runtime.spawn(async move {
            let progress = match install.plan {
                LspInstallPlan::Repo { .. } | LspInstallPlan::RustupThenRepo { .. } => {
                    Some(spawn_lsp_download_progress_task(
                        tx.clone(),
                        Arc::clone(&bytes_downloaded),
                        language.clone(),
                        install.display_name.clone(),
                    ))
                }
                LspInstallPlan::Npm { .. } => None,
            };
            let result = run_lsp_install_and_verify(&install, Arc::clone(&bytes_downloaded)).await;
            if let Some(progress) = progress {
                progress.abort();
            }
            let _ = send_ui_event(
                &tx,
                UiEvent::LspInstallFinished {
                    language,
                    display_name: install.display_name.clone(),
                    result,
                },
            );
        });
    }

    pub(crate) fn apply_lsp_install_progress(
        &mut self,
        language: &str,
        display_name: &str,
        bytes_downloaded: u64,
    ) {
        let Some(prompt) = self.lsp_enable_prompt.as_ref() else {
            return;
        };
        if prompt.language != language || !prompt.install_in_flight {
            return;
        }
        self.status = format!(
            "Downloading {display_name}… {}",
            format_byte_size(bytes_downloaded)
        );
    }

    pub(crate) fn apply_lsp_install_finished(
        &mut self,
        language: &str,
        display_name: &str,
        result: Result<(), LspInstallFailure>,
    ) {
        if let Some(prompt) = self.lsp_enable_prompt.as_mut()
            && prompt.language == language
        {
            if let Some(install) = prompt.install.as_ref() {
                self.lsp_installs_in_flight
                    .retain(|in_flight| *in_flight != install.id);
            }
            prompt.install_in_flight = false;
        }
        match result {
            Ok(()) => {
                self.lsp_unavailable
                    .retain(|client_key| lsp_client_key_language(client_key) != language);
                self.lsp_restart_attempts
                    .retain(|client_key, _| lsp_client_key_language(client_key) != language);
                self.enable_lsp_from_prompt(language);
                self.status = lsp_install_enabled_status(display_name);
            }
            Err(failure) => {
                self.status = lsp_install_failure_status(display_name, &failure);
            }
        }
    }

    fn enable_lsp_from_prompt(&mut self, language: &str) {
        if let Some(entry) = self
            .settings
            .lsp_servers
            .iter_mut()
            .find(|server| server.language == language)
        {
            entry.enabled = true;
        } else if let Some(mut default) = default_lsp_server_for_language(language) {
            default.enabled = true;
            self.settings.lsp_servers.push(default);
        }
        self.settings_panel_draft.lsp_servers = self.settings.lsp_servers.clone();
        self.lsp_enable_prompt = None;
        let path = crate::workspace_state::settings_path(&self.workspace.root);
        self.status = match self.settings.save(&path) {
            Ok(()) => format!("{language} LSP enabled"),
            Err(error) => format!("{language} LSP enabled, but settings were not saved: {error}"),
        };
        if let Some(id) = self.active {
            self.ensure_lsp_clients_for_buffer(id);
        }
    }

    fn decline_lsp_enable_prompt(&mut self, language: &str) {
        self.lsp_enable_prompt = None;
        self.lsp_enable_prompt_declined.push(language.to_owned());
        self.status = format!("{language} LSP suggestion dismissed");
    }
}

async fn run_lsp_install_and_verify(
    install: &LspInstallAction,
    bytes_downloaded: Arc<AtomicU64>,
) -> Result<(), LspInstallFailure> {
    match &install.plan {
        LspInstallPlan::Npm { npm_fallback, .. } => {
            run_lsp_install_shell(npm_fallback).await?;
            run_lsp_verify_command(&install.verify_command, &install.verify_args).await
        }
        LspInstallPlan::Repo { asset, launch } => {
            let binary_path = download_and_install_lsp_bundle(
                &repo_bundle_install(install, asset, launch),
                bytes_downloaded,
            )
            .await?;
            run_lsp_verify_command(&binary_path.to_string_lossy(), &install.verify_args).await
        }
        LspInstallPlan::RustupThenRepo {
            component_args,
            asset,
            launch,
        } => match rust_install_decision(detect_rustup().await) {
            RustInstallDecision::Rustup => {
                run_rustup_component_add(component_args).await?;
                run_lsp_verify_command(&install.verify_command, &install.verify_args).await
            }
            RustInstallDecision::RepoDownload => {
                let binary_path = download_and_install_lsp_bundle(
                    &repo_bundle_install(install, asset, launch),
                    bytes_downloaded,
                )
                .await?;
                run_lsp_verify_command(&binary_path.to_string_lossy(), &install.verify_args).await
            }
        },
    }
}

fn repo_bundle_install(install: &LspInstallAction, asset: &str, launch: &str) -> RepoBundleInstall {
    RepoBundleInstall {
        id: install.id.clone(),
        asset: asset.to_owned(),
        launch: launch.to_owned(),
    }
}

async fn run_lsp_install_shell(shell: &str) -> Result<(), LspInstallFailure> {
    let mut command = lsp_install_process_command(shell);
    let output = match command.output().await {
        Ok(output) => output,
        Err(error) => {
            return Err(LspInstallFailure::Shell {
                detail: format!("could not start installer: {error}"),
            });
        }
    };
    if output.status.success() {
        return Ok(());
    }
    Err(LspInstallFailure::Shell {
        detail: lsp_install_output_failure_detail(&output.status, &output.stderr),
    })
}

async fn run_rustup_component_add(args: &[String]) -> Result<(), LspInstallFailure> {
    let output = match tokio::process::Command::new("rustup")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
    {
        Ok(output) => output,
        Err(error) => {
            return Err(LspInstallFailure::Shell {
                detail: format!("could not start rustup: {error}"),
            });
        }
    };
    if output.status.success() {
        return Ok(());
    }
    Err(LspInstallFailure::Shell {
        detail: lsp_install_output_failure_detail(&output.status, &output.stderr),
    })
}

async fn run_lsp_verify_command(command: &str, args: &[String]) -> Result<(), LspInstallFailure> {
    let mut process = tokio::process::Command::new(command);
    process
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(LspInstallFailure::Verify {
                detail: format!("could not start {command}: {error}"),
            });
        }
    };
    let status = match tokio::time::timeout(LSP_INSTALL_VERIFY_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            return Err(LspInstallFailure::Verify {
                detail: error.to_string(),
            });
        }
        Err(_) => {
            return Err(LspInstallFailure::Verify {
                detail: format!("{command} did not exit within the verify timeout"),
            });
        }
    };
    if status.success() {
        Ok(())
    } else {
        Err(LspInstallFailure::Verify {
            detail: format!("{command} exited with {status}"),
        })
    }
}

fn lsp_install_process_command(shell: &str) -> tokio::process::Command {
    #[cfg(windows)]
    let mut command = {
        let mut command = tokio::process::Command::new("cmd");
        command.args(["/C", shell]);
        command
    };

    #[cfg(not(windows))]
    let mut command = {
        let mut command = tokio::process::Command::new("sh");
        command.args(["-c", shell]);
        command
    };

    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    command
}

fn lsp_install_output_failure_detail(status: &std::process::ExitStatus, stderr: &[u8]) -> String {
    let stderr_text = String::from_utf8_lossy(stderr);
    let stderr_tail = stderr_text
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(160)
        .collect::<String>();
    if stderr_tail.is_empty() {
        format!("installer exited with {status}")
    } else {
        format!("installer exited with {status}: {stderr_tail}")
    }
}

fn lsp_installing_status(display_name: &str) -> String {
    format!("Installing {display_name}…")
}

fn lsp_downloading_status(display_name: &str) -> String {
    format!("Downloading {display_name}…")
}

fn lsp_install_not_detected_status(display_name: &str) -> String {
    format!(
        "{display_name} installed, but it was not detected on PATH yet; restart Kuroya and enable it again"
    )
}

fn lsp_install_enabled_status(display_name: &str) -> String {
    format!("{display_name} installed and enabled")
}

pub(crate) fn lsp_install_failure_status(
    display_name: &str,
    failure: &LspInstallFailure,
) -> String {
    match failure {
        LspInstallFailure::Shell { .. } => "Install failed — see notes".to_owned(),
        LspInstallFailure::Verify { .. } => lsp_install_not_detected_status(display_name),
        LspInstallFailure::Download { detail } => {
            format!("Could not download {display_name}: {detail}")
        }
        LspInstallFailure::Checksum { .. } => format!(
            "{display_name} failed the checksum check and was deleted; try installing again"
        ),
        LspInstallFailure::MissingAsset { .. } => {
            format!("{display_name} is not published yet — run the lsp-bundles workflow")
        }
        LspInstallFailure::Extract { detail } => {
            format!("Could not install {display_name}: {detail}")
        }
    }
}

pub(crate) fn lsp_enable_prompt_candidate(
    declined: &[String],
    configured_enabled_languages: &[String],
    buffer_language: Option<&str>,
) -> Option<String> {
    let language = buffer_language?;
    if declined.iter().any(|entry| entry == language) {
        return None;
    }
    if configured_enabled_languages
        .iter()
        .any(|entry| entry == language)
    {
        return None;
    }
    Some(language.to_owned())
}

fn text_color_or(ui: &egui::Ui) -> egui::Color32 {
    ui.visuals().text_color()
}

#[cfg(test)]
mod tests {
    use super::{
        LspEnablePrompt, LspInstallAction, LspInstallFailure, LspInstallPlan,
        lsp_enable_prompt_candidate, lsp_install_button_label, lsp_install_enabled_status,
        lsp_install_failure_status, lsp_install_not_detected_status, lsp_install_note,
        lsp_installing_status, lsp_prompt_install_action,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, terminal::TerminalPane,
        workspace_state::settings_path,
    };
    use kuroya_core::{
        EditorSettings, Workspace,
        lsp_registry::{
            current_lsp_install_platform, lsp_archive_extension, lsp_binary_on_path,
            lsp_platform_token,
        },
    };
    use std::{path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn prompt_candidate_skips_declined_enabled_and_unknown_languages() {
        let declined = vec!["ruby".to_owned()];
        let enabled = vec!["rust".to_owned(), "python".to_owned()];

        assert_eq!(
            lsp_enable_prompt_candidate(&declined, &enabled, Some("dart")),
            Some("dart".to_owned())
        );
        assert_eq!(
            lsp_enable_prompt_candidate(&declined, &enabled, Some("ruby")),
            None
        );
        assert_eq!(
            lsp_enable_prompt_candidate(&declined, &enabled, Some("rust")),
            None
        );
        assert_eq!(lsp_enable_prompt_candidate(&declined, &enabled, None), None);
    }

    #[test]
    fn lsp_suggest_missing_servers_defaults_to_off() {
        assert!(!EditorSettings::default().lsp_suggest_missing_servers);
    }

    #[test]
    fn enable_prompt_action_flips_server_enabled_state_and_saves() {
        let root = unique_temp_dir("lsp-enable-prompt");
        let mut app = app_for_test(root.clone());
        app.settings.lsp_suggest_missing_servers = true;
        let dart = kuroya_core::default_lsp_server_for_language("dart")
            .expect("dart default should exist");
        app.settings
            .lsp_servers
            .retain(|server| server.language != "dart");
        app.settings.lsp_servers.push(kuroya_core::LspServerConfig {
            enabled: false,
            ..dart
        });

        app.enable_lsp_from_prompt("dart");

        assert!(
            app.settings
                .lsp_servers
                .iter()
                .find(|server| server.language == "dart")
                .expect("dart entry should remain")
                .enabled,
            "enable should flip the entry on"
        );
        assert_eq!(app.lsp_enable_prompt, None);
        let saved = EditorSettings::load_or_create(&settings_path(&root))
            .expect("saved settings should load");
        assert!(
            saved
                .lsp_servers
                .iter()
                .find(|server| server.language == "dart")
                .expect("saved dart entry")
                .enabled
        );
    }

    #[test]
    fn install_action_plans_follow_the_confirmed_install_design() {
        let platform = current_lsp_install_platform();

        let go = default_config("go");
        let go_action = lsp_prompt_install_action(&go).expect("go install action");
        assert_eq!(go_action.id, "go");
        assert_eq!(go_action.display_name, "gopls");
        assert_eq!(go_action.verify_command, "gopls");
        assert_eq!(
            go_action.plan,
            LspInstallPlan::Repo {
                asset: format!(
                    "lsp-gopls-{}{}",
                    lsp_platform_token(platform),
                    lsp_archive_extension(platform)
                ),
                launch: "gopls".to_owned(),
            }
        );

        let python = default_config("python");
        let python_action = lsp_prompt_install_action(&python).expect("python install action");
        assert_eq!(python_action.id, "python");
        assert_eq!(python_action.display_name, "pyright");
        assert_eq!(
            python_action.plan,
            LspInstallPlan::Npm {
                npm_fallback: "npm install -g pyright".to_owned(),
                reason: "requires Node.js and npm".to_owned(),
            }
        );

        let rust = default_config("rust");
        if lsp_binary_on_path("rust-analyzer") {
            assert!(
                lsp_prompt_install_action(&rust).is_none(),
                "runner images shipping rust-analyzer keep today's no-install behavior"
            );
            return;
        }
        let rust_action = lsp_prompt_install_action(&rust).expect("rust install action");
        assert_eq!(rust_action.id, "rust");
        assert_eq!(rust_action.display_name, "rust-analyzer");
        assert_eq!(
            rust_action.plan,
            LspInstallPlan::RustupThenRepo {
                component_args: vec![
                    "component".to_owned(),
                    "add".to_owned(),
                    "rust-analyzer".to_owned()
                ],
                asset: format!(
                    "lsp-rust-analyzer-{}{}",
                    lsp_platform_token(platform),
                    lsp_archive_extension(platform)
                ),
                launch: "rust-analyzer".to_owned(),
            }
        );
    }

    #[test]
    fn install_action_is_skipped_for_unknown_or_present_servers() {
        assert!(
            lsp_prompt_install_action(&config_with_command(
                "java",
                "definitely-not-a-real-binary-xyz"
            ))
            .is_none(),
            "languages without a registry entry keep today's behavior"
        );

        let current_exe = std::env::current_exe().expect("current test binary");
        assert!(
            lsp_prompt_install_action(&config_with_command(
                "rust",
                current_exe.to_string_lossy().as_ref()
            ))
            .is_none(),
            "no install action is offered while the server binary is present"
        );
    }

    #[test]
    fn npm_actions_use_the_npm_button_label_and_node_note() {
        let python_action =
            lsp_prompt_install_action(&default_config("python")).expect("python install action");
        assert_eq!(lsp_install_button_label(&python_action), "Install via npm");
        assert_eq!(
            lsp_install_note(&python_action).as_deref(),
            Some("requires Node.js and npm")
        );

        let go_action =
            lsp_prompt_install_action(&default_config("go")).expect("go install action");
        assert_eq!(lsp_install_button_label(&go_action), "Install gopls");
        assert_eq!(lsp_install_note(&go_action), None);

        if !lsp_binary_on_path("rust-analyzer") {
            let rust_action =
                lsp_prompt_install_action(&default_config("rust")).expect("rust install action");
            assert_eq!(
                lsp_install_button_label(&rust_action),
                "Install rust-analyzer"
            );
        }
    }

    #[test]
    fn install_statuses_match_outcomes() {
        assert_eq!(
            lsp_installing_status("rust-analyzer"),
            "Installing rust-analyzer…"
        );
        assert_eq!(
            lsp_install_failure_status(
                "gopls",
                &LspInstallFailure::MissingAsset {
                    detail: "missing".to_owned()
                }
            ),
            "gopls is not published yet — run the lsp-bundles workflow"
        );
        assert_eq!(
            lsp_install_failure_status(
                "clangd",
                &LspInstallFailure::Checksum {
                    detail: "checksum mismatch".to_owned()
                }
            ),
            "clangd failed the checksum check and was deleted; try installing again"
        );
        let download_status = lsp_install_failure_status(
            "gopls",
            &LspInstallFailure::Download {
                detail: "connection reset".to_owned(),
            },
        );
        assert!(download_status.starts_with("Could not download gopls:"));
        assert!(
            lsp_install_failure_status(
                "marksman",
                &LspInstallFailure::Extract {
                    detail: "boom".to_owned()
                }
            )
            .starts_with("Could not install marksman:")
        );
        assert_eq!(
            lsp_install_failure_status(
                "bash-language-server",
                &LspInstallFailure::Shell {
                    detail: "npm missing".to_owned()
                }
            ),
            "Install failed — see notes"
        );
        assert!(
            lsp_install_failure_status(
                "clangd",
                &LspInstallFailure::Verify {
                    detail: "not found".to_owned()
                }
            )
            .contains("PATH")
        );
        assert!(
            lsp_install_not_detected_status("clangd").contains("clangd")
                && lsp_install_not_detected_status("clangd").contains("PATH")
        );
    }

    #[test]
    fn apply_lsp_install_finished_failure_keeps_prompt_usable_and_releases_the_guard() {
        let root = unique_temp_dir("lsp-install-failed");
        let mut app = app_for_test(root);
        app.lsp_enable_prompt = Some(sample_prompt());
        app.lsp_installs_in_flight = vec!["rust".to_owned()];

        app.apply_lsp_install_finished(
            "rust",
            "rust-analyzer",
            Err(LspInstallFailure::Shell {
                detail: "boom".to_owned(),
            }),
        );

        let prompt = app.lsp_enable_prompt.as_ref().expect("prompt stays open");
        assert_eq!(prompt.language, "rust");
        assert!(
            !prompt.install_in_flight,
            "a failed install must be retryable"
        );
        assert!(
            app.lsp_installs_in_flight.is_empty(),
            "the per-server guard must be released"
        );
        assert_eq!(app.status, "Install failed — see notes");

        app.lsp_installs_in_flight = vec!["rust".to_owned()];
        app.apply_lsp_install_finished(
            "rust",
            "rust-analyzer",
            Err(LspInstallFailure::MissingAsset {
                detail: "not found".to_owned(),
            }),
        );
        assert!(app.lsp_enable_prompt.is_some());
        assert_eq!(
            app.status,
            "rust-analyzer is not published yet — run the lsp-bundles workflow"
        );
    }

    #[test]
    fn apply_lsp_install_finished_success_enables_and_starts_the_server() {
        let root = unique_temp_dir("lsp-install-success");
        let mut app = app_for_test(root);
        let dart = kuroya_core::default_lsp_server_for_language("dart")
            .expect("dart default should exist");
        app.settings
            .lsp_servers
            .retain(|server| server.language != "dart");
        app.settings.lsp_servers.push(kuroya_core::LspServerConfig {
            enabled: false,
            ..dart
        });
        app.lsp_enable_prompt = Some(sample_prompt_with_language("dart"));

        app.apply_lsp_install_finished("dart", "dart", Ok(()));

        assert_eq!(app.lsp_enable_prompt, None);
        assert_eq!(app.status, lsp_install_enabled_status("dart"));
        assert!(
            app.settings
                .lsp_servers
                .iter()
                .find(|server| server.language == "dart")
                .expect("dart entry should remain")
                .enabled,
            "a successful install flips the server on through the existing enable path"
        );
    }

    #[test]
    fn install_in_flight_guard_blocks_reentry() {
        let root = unique_temp_dir("lsp-install-guard");
        let mut app = app_for_test(root);
        let mut prompt = sample_prompt();
        prompt.install_in_flight = true;
        app.lsp_enable_prompt = Some(prompt);
        app.status = "prior status".to_owned();

        app.start_lsp_install_from_prompt();

        let prompt = app.lsp_enable_prompt.as_ref().expect("prompt stays open");
        assert!(prompt.install_in_flight);
        assert_eq!(app.status, "prior status");
    }

    #[test]
    fn per_server_in_flight_guard_blocks_a_second_install_of_the_same_id() {
        let root = unique_temp_dir("lsp-install-id-guard");
        let mut app = app_for_test(root);
        let mut prompt = sample_prompt();
        prompt.install_in_flight = false;
        app.lsp_enable_prompt = Some(prompt);
        app.lsp_installs_in_flight = vec!["rust".to_owned()];
        app.status = "prior status".to_owned();

        app.start_lsp_install_from_prompt();

        let prompt = app.lsp_enable_prompt.as_ref().expect("prompt stays open");
        assert!(
            !prompt.install_in_flight,
            "the per-id guard must reject the install before flipping the prompt state"
        );
        assert_eq!(app.status, "prior status");
    }

    #[test]
    fn install_progress_updates_the_status_only_while_in_flight() {
        let root = unique_temp_dir("lsp-install-progress");
        let mut app = app_for_test(root);
        let mut prompt = sample_prompt();
        prompt.install_in_flight = true;
        app.lsp_enable_prompt = Some(prompt);

        app.apply_lsp_install_progress("rust", "rust-analyzer", 4_404_019);
        assert_eq!(app.status, "Downloading rust-analyzer… 4.2 MB");

        app.apply_lsp_install_progress("rust", "rust-analyzer", 512);
        assert_eq!(app.status, "Downloading rust-analyzer… 512 B");

        if let Some(prompt) = app.lsp_enable_prompt.as_mut() {
            prompt.install_in_flight = false;
        }
        app.apply_lsp_install_progress("rust", "rust-analyzer", 1_048_576);
        assert_eq!(app.status, "Downloading rust-analyzer… 512 B");

        app.apply_lsp_install_progress("go", "gopls", 1_048_576);
        assert_eq!(app.status, "Downloading rust-analyzer… 512 B");
    }

    #[test]
    fn sample_prompt_structures_are_self_consistent() {
        let prompt = sample_prompt();
        let install = prompt.install.as_ref().expect("rust sample has an install");
        assert_eq!(prompt.language, "rust");
        assert_eq!(install.display_name, "rust-analyzer");
        assert_eq!(install.verify_command, "rust-analyzer");
        assert!(matches!(
            install.plan,
            LspInstallPlan::RustupThenRepo { .. }
        ));
        assert!(!prompt.binary_present);
        assert!(!prompt.install_in_flight);
    }

    fn default_config(language: &str) -> kuroya_core::LspServerConfig {
        kuroya_core::default_lsp_server_for_language(language)
            .unwrap_or_else(|| panic!("{language} default config"))
    }

    fn config_with_command(language: &str, command: &str) -> kuroya_core::LspServerConfig {
        kuroya_core::LspServerConfig {
            language: language.to_owned(),
            command: command.to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
            enabled: false,
        }
    }

    fn sample_prompt() -> LspEnablePrompt {
        sample_prompt_with_language("rust")
    }

    fn sample_prompt_with_language(language: &str) -> LspEnablePrompt {
        LspEnablePrompt {
            language: language.to_owned(),
            command: "sample-server".to_owned(),
            file_name: "main.txt".to_owned(),
            binary_present: false,
            install: Some(LspInstallAction {
                id: "rust".to_owned(),
                display_name: "rust-analyzer".to_owned(),
                verify_command: "rust-analyzer".to_owned(),
                verify_args: vec!["--version".to_owned()],
                plan: LspInstallPlan::RustupThenRepo {
                    component_args: vec![
                        "component".to_owned(),
                        "add".to_owned(),
                        "rust-analyzer".to_owned(),
                    ],
                    asset: "lsp-rust-analyzer-windows-x64.zip".to_owned(),
                    launch: "rust-analyzer".to_owned(),
                },
            }),
            install_in_flight: false,
        }
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
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

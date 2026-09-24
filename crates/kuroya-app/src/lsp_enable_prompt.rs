use crate::{
    KuroyaApp,
    lsp_install::{
        LspBundleInstallPlan, LspInstallFailure, LspInstallOutcome, lsp_bundle_install_plan,
        lsp_bundle_unavailable_status, lsp_checksum_failed_status, lsp_download_failed_status,
        run_lsp_bundle_install,
    },
    lsp_runtime::lsp_client_key_language,
    ui_event_channel::send_ui_event,
    ui_events::UiEvent,
};
use eframe::egui::{self, Align2, Context, RichText};
use kuroya_core::{
    default_lsp_server_for_language, lsp_language_id_for_path,
    lsp_registry::{
        LspInstallKind, current_lsp_install_platform, lsp_binary_on_path, lsp_fallback_shell,
        lsp_fallback_verify, registry_entry_for,
    },
};
use std::time::Duration;

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
pub(crate) enum LspInstallAction {
    Download {
        display_name: String,
        plan: LspBundleInstallPlan,
    },
    Shell {
        display_name: String,
        reason: String,
        shell: String,
        verify_command: String,
        verify_args: Vec<String>,
    },
}

impl LspInstallAction {
    pub(crate) fn display_name(&self) -> &str {
        match self {
            Self::Download { display_name, .. } | Self::Shell { display_name, .. } => display_name,
        }
    }

    pub(crate) fn is_download(&self) -> bool {
        matches!(self, Self::Download { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LspEnablePromptAction {
    None,
    Enable,
    Install,
    NotNow,
}

pub(crate) fn lsp_prompt_install_action(
    language: &str,
    server_command: &str,
) -> Option<LspInstallAction> {
    if lsp_binary_on_path(server_command) {
        return None;
    }
    let definition = registry_entry_for(language)?;
    let platform = current_lsp_install_platform();
    match definition.install.kind {
        LspInstallKind::Download => {
            let plan = lsp_bundle_install_plan(definition, platform)?;
            Some(LspInstallAction::Download {
                display_name: definition.display_name.clone(),
                plan,
            })
        }
        LspInstallKind::Unsupported => {
            let (verify_command, verify_args) = lsp_fallback_verify(definition)?;
            Some(LspInstallAction::Shell {
                display_name: definition.display_name.clone(),
                reason: definition.install.reason.clone()?,
                shell: lsp_fallback_shell(definition)?.to_owned(),
                verify_command: verify_command.to_owned(),
                verify_args: verify_args.to_vec(),
            })
        }
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
        let binary_present = lsp_binary_on_path(&default.command);
        let install = if binary_present {
            None
        } else {
            lsp_prompt_install_action(&language, &default.command)
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
                                    "Fetching {}…",
                                    install.display_name()
                                )));
                            }
                        } else {
                            if let Some(LspInstallAction::Shell { reason, .. }) =
                                prompt.install.as_ref()
                            {
                                ui.label(RichText::new(reason.clone()).weak());
                                ui.add_space(4.0);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if let Some(install) = prompt.install.as_ref() {
                                        match install {
                                            LspInstallAction::Download { display_name, .. } => {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            RichText::new(format!(
                                                                "Install {display_name}"
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
                                            LspInstallAction::Shell { .. } => {
                                                if crate::popup_buttons::popup_button(
                                                    ui,
                                                    "Install via npm",
                                                    crate::popup_buttons::PopupButtonKind::Secondary,
                                                )
                                                .clicked()
                                                {
                                                    action = LspEnablePromptAction::Install;
                                                }
                                            }
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
                            if prompt.install.is_none() && !prompt.binary_present {
                                ui.label(RichText::new(
                                    "No automatic install available for this platform",
                                ));
                            }
                        }
                    });
            });

        match action {
            LspEnablePromptAction::Enable => self.enable_lsp_from_prompt(&prompt.language, None),
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
        let language = prompt.language.clone();
        if let Some(prompt) = self.lsp_enable_prompt.as_mut() {
            prompt.install_in_flight = true;
        }
        let display_name = install.display_name().to_owned();
        self.status = lsp_installing_status(&display_name);
        self.record_async_task_started("LSP Install", display_name.clone());
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = match install {
                LspInstallAction::Download { plan, .. } => {
                    let bytes_downloaded =
                        std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                    run_lsp_bundle_install(plan, language.clone(), tx.clone(), &bytes_downloaded)
                        .await
                }
                LspInstallAction::Shell {
                    shell,
                    verify_command,
                    verify_args,
                    ..
                } => run_lsp_shell_install(&shell, &verify_command, &verify_args).await,
            };
            let _ = send_ui_event(
                &tx,
                UiEvent::LspInstallFinished {
                    language,
                    display_name,
                    result,
                },
            );
        });
    }

    pub(crate) fn apply_lsp_install_finished(
        &mut self,
        language: &str,
        display_name: &str,
        result: Result<LspInstallOutcome, LspInstallFailure>,
    ) {
        if let Some(prompt) = self.lsp_enable_prompt.as_mut()
            && prompt.language == language
        {
            prompt.install_in_flight = false;
        }
        match result {
            Ok(outcome) => {
                self.lsp_unavailable
                    .retain(|client_key| lsp_client_key_language(client_key) != language);
                self.lsp_restart_attempts
                    .retain(|client_key, _| lsp_client_key_language(client_key) != language);
                self.enable_lsp_from_prompt(language, outcome.command_override.as_deref());
                self.status = lsp_install_enabled_status(display_name);
            }
            Err(failure) => {
                self.status = lsp_install_failure_status(display_name, &failure);
            }
        }
    }

    fn enable_lsp_from_prompt(&mut self, language: &str, command_override: Option<&str>) {
        if let Some(entry) = self
            .settings
            .lsp_servers
            .iter_mut()
            .find(|server| server.language == language)
        {
            entry.enabled = true;
            if let Some(command) = command_override {
                entry.command = command.to_owned();
            }
        } else if let Some(mut default) = default_lsp_server_for_language(language) {
            default.enabled = true;
            if let Some(command) = command_override {
                default.command = command.to_owned();
            }
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

async fn run_lsp_shell_install(
    shell: &str,
    verify_command: &str,
    verify_args: &[String],
) -> Result<LspInstallOutcome, LspInstallFailure> {
    run_lsp_install_shell(shell).await?;
    run_lsp_verify_command(verify_command, verify_args).await?;
    Ok(LspInstallOutcome {
        command_override: None,
    })
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

pub(crate) fn lsp_installing_status(display_name: &str) -> String {
    format!("Installing {display_name}…")
}

fn lsp_install_failure_status(display_name: &str, failure: &LspInstallFailure) -> String {
    match failure {
        LspInstallFailure::Shell { .. } => "Install failed — see notes".to_owned(),
        LspInstallFailure::Verify { .. } => lsp_install_not_detected_status(display_name),
        LspInstallFailure::Download { detail } => lsp_download_failed_status(display_name, detail),
        LspInstallFailure::Checksum { .. } => lsp_checksum_failed_status(display_name),
        LspInstallFailure::Unavailable { .. } => lsp_bundle_unavailable_status(display_name),
    }
}

fn lsp_install_not_detected_status(display_name: &str) -> String {
    format!(
        "{display_name} installed, but it was not detected on PATH yet; restart Kuroya and enable it again"
    )
}

fn lsp_install_enabled_status(display_name: &str) -> String {
    format!("{display_name} installed and enabled")
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
        LspEnablePrompt, LspInstallAction, lsp_enable_prompt_candidate, lsp_install_enabled_status,
        lsp_install_failure_status, lsp_installing_status, lsp_prompt_install_action,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, lsp_install::LspInstallFailure,
        terminal::TerminalPane, workspace_state::settings_path,
    };
    use kuroya_core::{EditorSettings, Workspace};
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

        app.enable_lsp_from_prompt("dart", None);

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
    fn download_action_resolves_from_registry_only_while_binary_is_missing() {
        let action = lsp_prompt_install_action("rust", "definitely-not-a-real-binary-xyz")
            .expect("rust should have a download action while the binary is missing");
        let LspInstallAction::Download { display_name, plan } = &action else {
            panic!("rust must resolve to a download action");
        };
        assert_eq!(display_name, "rust-analyzer");
        assert_eq!(plan.server_id, "rust");
        assert!(plan.asset.starts_with("lsp-rust-analyzer-"));
        assert!(plan.download_url.contains("/releases/download/lsps/"));

        assert!(
            lsp_prompt_install_action("java", "definitely-not-a-real-binary-xyz").is_none(),
            "languages without a registry entry keep today's behavior"
        );

        let current_exe = std::env::current_exe().expect("current test binary");
        assert!(
            lsp_prompt_install_action("rust", current_exe.to_string_lossy().as_ref()).is_none(),
            "no install action is offered while the server binary is present"
        );
    }

    #[test]
    fn npm_fallback_action_resolves_for_node_dependent_servers() {
        let action = lsp_prompt_install_action("python", "definitely-not-a-real-binary-xyz")
            .expect("python should resolve an npm fallback action");
        let LspInstallAction::Shell {
            display_name,
            reason,
            shell,
            verify_command,
            ..
        } = &action
        else {
            panic!("python must resolve to a shell fallback action");
        };
        assert_eq!(display_name, "pyright");
        assert!(reason.contains("Node.js"));
        assert_eq!(shell, "npm install -g pyright");
        assert_eq!(verify_command, "pyright-langserver");
        assert!(!action.is_download());
    }

    #[test]
    fn install_statuses_match_outcomes() {
        assert_eq!(
            lsp_install_failure_status(
                "rust-analyzer",
                &LspInstallFailure::Shell {
                    detail: "boom".to_owned()
                }
            ),
            "Install failed — see notes"
        );
        assert_eq!(
            lsp_installing_status("rust-analyzer"),
            "Installing rust-analyzer…"
        );
        assert_eq!(
            lsp_install_enabled_status("rust-analyzer"),
            "rust-analyzer installed and enabled"
        );
        assert!(
            lsp_install_failure_status(
                "clangd",
                &LspInstallFailure::Verify {
                    detail: "missing".to_owned()
                }
            )
            .contains("clangd")
        );
    }

    #[test]
    fn install_failure_statuses_cover_download_checksum_and_unavailable() {
        let download = lsp_install_failure_status(
            "rust-analyzer",
            &LspInstallFailure::Download {
                detail: "timeout".to_owned(),
            },
        );
        assert!(download.contains("Could not download rust-analyzer"));
        assert!(download.contains("timeout"));

        let checksum = lsp_install_failure_status(
            "rust-analyzer",
            &LspInstallFailure::Checksum {
                detail: "mismatch".to_owned(),
            },
        );
        assert!(checksum.contains("checksum verification"));

        let unavailable = lsp_install_failure_status(
            "gopls",
            &LspInstallFailure::Unavailable {
                release_url: "https://github.com/kaevris/kuroya/releases/tag/lsps".to_owned(),
            },
        );
        assert!(unavailable.contains("not yet available on the release page"));
        assert!(unavailable.contains("https://github.com/kaevris/kuroya/releases/tag/lsps"));
    }

    #[test]
    fn apply_lsp_install_finished_failure_keeps_prompt_usable() {
        let root = unique_temp_dir("lsp-install-failed");
        let mut app = app_for_test(root);
        app.lsp_enable_prompt = Some(sample_prompt());

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
        assert_eq!(app.status, "Install failed — see notes");

        app.apply_lsp_install_finished(
            "rust",
            "clangd",
            Err(LspInstallFailure::Verify {
                detail: "not found".to_owned(),
            }),
        );
        assert!(app.lsp_enable_prompt.is_some());
        assert!(
            app.status.contains("clangd"),
            "verify failures point at the missing PATH entry"
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

        app.apply_lsp_install_finished(
            "dart",
            "dart",
            Ok(crate::lsp_install::LspInstallOutcome {
                command_override: None,
            }),
        );

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
    fn apply_lsp_install_finished_download_success_points_the_command_at_the_installed_binary() {
        let root = unique_temp_dir("lsp-install-override");
        let mut app = app_for_test(root);
        let rust = kuroya_core::default_lsp_server_for_language("rust")
            .expect("rust default should exist");
        app.settings
            .lsp_servers
            .retain(|server| server.language != "rust");
        app.settings.lsp_servers.push(kuroya_core::LspServerConfig {
            enabled: false,
            ..rust
        });
        app.lsp_enable_prompt = Some(sample_prompt());

        let installed_path = std::env::current_exe()
            .expect("current test binary")
            .to_string_lossy()
            .into_owned();
        app.apply_lsp_install_finished(
            "rust",
            "rust-analyzer",
            Ok(crate::lsp_install::LspInstallOutcome {
                command_override: Some(installed_path.clone()),
            }),
        );

        let entry = app
            .settings
            .lsp_servers
            .iter()
            .find(|server| server.language == "rust")
            .expect("rust entry should remain");
        assert!(entry.enabled);
        assert_eq!(
            entry.command, installed_path,
            "the LSP command must resolve to the installed binary path"
        );
        assert_eq!(app.lsp_enable_prompt, None);
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
    fn sample_prompt_structures_are_self_consistent() {
        let prompt = sample_prompt();
        let install = prompt.install.as_ref().expect("rust sample has an install");
        assert!(install.is_download());
        assert_eq!(install.display_name(), "rust-analyzer");
        assert!(!prompt.binary_present);
        assert!(!prompt.install_in_flight);
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
            install: lsp_prompt_install_action(language, "definitely-not-a-real-binary-xyz"),
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

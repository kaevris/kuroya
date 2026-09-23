use crate::KuroyaApp;
use eframe::egui::{self, Align2, Context, RichText};
use kuroya_core::{default_lsp_server_for_language, lsp_language_id_for_path};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LspEnablePrompt {
    pub(crate) language: String,
    pub(crate) command: String,
    pub(crate) file_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LspEnablePromptAction {
    None,
    Enable,
    NotNow,
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
        self.lsp_enable_prompt = Some(LspEnablePrompt {
            language,
            command,
            file_name: file_name.unwrap_or_else(|| "This file".to_owned()),
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
                        1.0,
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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        });
                    });
            });

        match action {
            LspEnablePromptAction::Enable => self.enable_lsp_from_prompt(&prompt.language),
            LspEnablePromptAction::NotNow => self.decline_lsp_enable_prompt(&prompt.language),
            LspEnablePromptAction::None => {}
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
    use super::lsp_enable_prompt_candidate;
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, terminal::TerminalPane,
        workspace_state::settings_path,
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

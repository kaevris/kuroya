use eframe::egui::{self, Context, Key};

use crate::{
    KuroyaApp,
    popup_buttons::{PopupButtonKind, popup_button_enabled},
    ui_icons::{IconKind, icon_button, icon_label},
};

mod actions;
mod apply;
mod font_files;
mod sections;

use actions::PendingSettingsPanelActions;
use sections::{
    SETTINGS_SECTION_APPEARANCE, SETTINGS_SECTION_DEVELOPER, SETTINGS_SECTION_EDITOR,
    SETTINGS_SECTION_FILES, SETTINGS_SECTION_GENERAL, SETTINGS_SECTION_LSP,
    SETTINGS_SECTION_SOURCE_CONTROL, SETTINGS_SECTION_TERMINAL, SETTINGS_SECTION_VIM,
    SETTINGS_SECTIONS, SETTINGS_TARGET_APPEARANCE, SETTINGS_TARGET_DEVELOPER,
    SETTINGS_TARGET_EDITOR_CODE_VIEW, SETTINGS_TARGET_EDITOR_CURSOR, SETTINGS_TARGET_EDITOR_DIFF,
    SETTINGS_TARGET_EDITOR_DISPLAY, SETTINGS_TARGET_EDITOR_LANGUAGE,
    SETTINGS_TARGET_EDITOR_TEXT_LAYOUT, SETTINGS_TARGET_EDITOR_TYPING,
    SETTINGS_TARGET_FILES_SAVE_ACTIONS, SETTINGS_TARGET_FILES_SAVE_CLEANUP,
    SETTINGS_TARGET_GENERAL, SETTINGS_TARGET_LSP, SETTINGS_TARGET_SCROLLBARS,
    SETTINGS_TARGET_SOURCE_CONTROL, SETTINGS_TARGET_TERMINAL_BUFFER,
    SETTINGS_TARGET_TERMINAL_COLOR, SETTINGS_TARGET_TERMINAL_CURSOR,
    SETTINGS_TARGET_TERMINAL_INTERACTION, SETTINGS_TARGET_TERMINAL_PROFILE,
    SETTINGS_TARGET_VIM_KEYBINDINGS, SettingsHighlightState, bounded_settings_singleline_input,
    render_appearance_settings, render_developer_settings, render_editor_settings,
    render_files_settings, render_general_settings, render_lsp_settings, render_settings_sidebar,
    render_source_control_settings, render_terminal_settings, render_vim_settings,
    vim_key_capture_active, vim_key_capture_clear,
};

const SETTINGS_WINDOW_SIZE: [f32; 2] = [620.0, 440.0];
const SETTINGS_WINDOW_MIN_SIZE: [f32; 2] = [380.0, 320.0];
const SETTINGS_SIDEBAR_WIDTH: f32 = 142.0;
const SETTINGS_FOOTER_HEIGHT: f32 = 38.0;
const SETTINGS_FOOTER_BUTTON_WIDTH: f32 = 78.0;
const SETTINGS_SEARCH_QUERY_ID: &str = "settings-panel-search-query";
const SETTINGS_HIGHLIGHT_TARGET_ID: &str = "settings-panel-highlight-target";
const SETTINGS_PENDING_SCROLL_TARGET_ID: &str = "settings-panel-pending-scroll-target";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SettingsSearchEntry {
    section: usize,
    group: &'static str,
    title: &'static str,
    keywords: &'static str,
}

const SETTINGS_SEARCH_ENTRIES: &[SettingsSearchEntry] = &[
    SettingsSearchEntry {
        section: SETTINGS_SECTION_FILES,
        group: "Save Actions",
        title: "Autosave",
        keywords: "auto save after delay focus window off file write",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_FILES,
        group: "Save Actions",
        title: "Autosave delay",
        keywords: "auto save delay milliseconds ms timer",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_GENERAL,
        group: "General",
        title: "Window zoom",
        keywords: "zoom scale interface ui size",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_GENERAL,
        group: "General",
        title: "Minimap",
        keywords: "overview map scroll preview code visible",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_GENERAL,
        group: "General",
        title: "Smooth scrolling",
        keywords: "scroll smooth beyond last line",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_GENERAL,
        group: "General",
        title: "Scroll beyond last line",
        keywords: "scroll beyond last line end file editor",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_GENERAL,
        group: "General",
        title: "Status bar",
        keywords: "footer diagnostics git branch visible",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_DEVELOPER,
        group: "Developer",
        title: "Devtools",
        keywords: "verbose logging profiling debug diagnostics",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Text and Layout",
        title: "Editor font size",
        keywords: "font text size zoom pixels code",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_GENERAL,
        group: "General",
        title: "UI font size",
        keywords: "interface font size panels labels",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Text and Layout",
        title: "Font family",
        keywords: "font face monospace editor ui ligatures variations weight",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Text and Layout",
        title: "Line height",
        keywords: "line height spacing rows text layout",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Text and Layout",
        title: "Word wrap",
        keywords: "wrap wrapping columns indent overflow long lines",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Text and Layout",
        title: "Tabs and indentation",
        keywords: "tab size insert spaces detect indentation indent guides",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Display",
        title: "Render whitespace",
        keywords: "space tab invisible whitespace control characters unicode",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Scrollbars",
        title: "Scrollbars",
        keywords: "vertical horizontal scrollbar scrollbars scroller editor explorer code container visible hidden auto size theme track",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Display",
        title: "Color decorators",
        keywords: "color picker inline preview swatches decorators",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Display",
        title: "GPU acceleration",
        keywords: "gpu render acceleration performance graphics",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Display",
        title: "Unicode highlight",
        keywords: "unicode invisible ambiguous non ascii control characters allowed locales",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Typing Assistance",
        title: "Auto indent",
        keywords: "indent typing new lines context",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Typing Assistance",
        title: "Auto close brackets",
        keywords: "brackets parentheses pairs auto closing delete overtype",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Typing Assistance",
        title: "Auto close quotes",
        keywords: "quotes strings pairs auto closing delete overtype",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Typing Assistance",
        title: "Auto surround",
        keywords: "surround wrap selection pairs brackets quotes",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Typing Assistance",
        title: "Format on paste/type",
        keywords: "format paste type pasted typing formatter",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_VIM,
        group: "Vim",
        title: "Vim keybindings",
        keywords: "vim vi modal mode normal insert command keybindings keyboard",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_VIM,
        group: "Vim",
        title: "Disabled Vim bindings",
        keywords: "vim disable disabled bindings keys normal mode remove ignore",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_VIM,
        group: "Vim",
        title: "Vim overrides",
        keywords: "vim override overrides remap mapping custom keys command after before",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Typing Assistance",
        title: "Read-only",
        keywords: "readonly read only locked edit disabled message",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Typing Assistance",
        title: "Multi cursor",
        keywords: "multi cursor column selection modifier paste limit alt ctrl",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Typing Assistance",
        title: "Clipboard selection",
        keywords: "copy selection clipboard highlight empty middle click",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Language Features",
        title: "Quick suggestions",
        keywords: "suggest autocomplete completion quick delay trigger characters",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Language Features",
        title: "Suggest widget",
        keywords: "completion popup widget icons status details items methods functions snippets",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_LSP,
        group: "LSP",
        title: "Hover",
        keywords: "lsp hover tooltip delay sticky above long line warning",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_LSP,
        group: "LSP",
        title: "LSP servers",
        keywords: "lsp language server servers command args arguments root markers rust analyzer pyright typescript gopls clangd jdtls intelephense ruby lua dart kotlin swift vue svelte docker terraform powershell",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Language Features",
        title: "Inline suggestions",
        keywords: "inline suggest completions ghost text ai edits toolbar delay",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_LSP,
        group: "LSP",
        title: "Code lens",
        keywords: "code lens inline actions font size",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_LSP,
        group: "LSP",
        title: "Inlay hints",
        keywords: "inlay hints type parameter inline labels padding font",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_LSP,
        group: "LSP",
        title: "Parameter hints",
        keywords: "signature help lsp parameter trigger cycle",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_LSP,
        group: "LSP",
        title: "Lightbulb",
        keywords: "lightbulb code actions quick fix refactor lsp",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_LSP,
        group: "LSP",
        title: "Document highlights",
        keywords: "document highlights symbol references lsp",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_LSP,
        group: "LSP",
        title: "Go to definitions",
        keywords: "go to definition declarations implementations references tests lsp navigation peek",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Cursor and Highlight",
        title: "Cursor style",
        keywords: "caret cursor style width height blink blinking smooth overtype",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Cursor and Highlight",
        title: "Line highlight",
        keywords: "line highlight active focus surrounding lines",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Code View",
        title: "Folding",
        keywords: "fold folding controls imports regions maximum unfold",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Code View",
        title: "Sticky scroll",
        keywords: "sticky scroll pinned scope headers lines",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Code View",
        title: "Indent guides",
        keywords: "indent indentation guides active guide gutter",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Code View",
        title: "Bracket guides",
        keywords: "bracket pair guides colorization match active horizontal",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Code View",
        title: "Find",
        keywords: "find search replace history selection loop cursor result",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Code View",
        title: "Editor minimap",
        keywords: "minimap side autohide slider scale characters max column section headers",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_EDITOR,
        group: "Diff Editor",
        title: "Diff editor",
        keywords: "diff side by side inline whitespace unchanged algorithm word wrap",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_SOURCE_CONTROL,
        group: "Source Control",
        title: "Source Control",
        keywords: "scm source control commit input actions badges repositories",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_SOURCE_CONTROL,
        group: "Git",
        title: "Git repository detection",
        keywords: "git repository detection scan parent folders submodules worktrees",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_SOURCE_CONTROL,
        group: "Git",
        title: "Git fetch, pull, and sync",
        keywords: "git fetch pull sync push prune tags rebase stash autofetch",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_SOURCE_CONTROL,
        group: "Git",
        title: "Git blame",
        keywords: "git blame status bar decoration hover whitespace template",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Profile",
        title: "Terminal profile",
        keywords: "terminal shell profile provider detected powershell cmd nushell bash",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Profile",
        title: "Shell executable",
        keywords: "terminal shell executable path command powershell cmd",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Profile",
        title: "Shell arguments",
        keywords: "terminal shell args arguments flags command line",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Profile",
        title: "Start directory",
        keywords: "terminal cwd working directory start folder workspace root home",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Profile",
        title: "Split start directory",
        keywords: "terminal split cwd working directory pane",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Buffer and Text",
        title: "Scrollback rows",
        keywords: "terminal scrollback history buffer rows",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Buffer and Text",
        title: "Terminal font size",
        keywords: "terminal font text size zoom wheel",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Buffer and Text",
        title: "Terminal line height",
        keywords: "terminal line height letter spacing rows columns",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Cursor",
        title: "Terminal cursor",
        keywords: "terminal cursor style width blink blinking",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Color and Feedback",
        title: "Terminal colors",
        keywords: "terminal color contrast minimum theme bright ansi",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Color and Feedback",
        title: "Bell",
        keywords: "terminal bell sound flash notification duration",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Right click",
        keywords: "terminal right click context menu copy paste select word mouse",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Middle click",
        keywords: "terminal middle click paste mouse",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Copy on selection",
        keywords: "terminal copy selection clipboard select text",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Terminal tabs",
        keywords: "terminal tabs active tab icons action buttons focus hide condition",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Tab title",
        keywords: "terminal tab title rename name label process cwd workspace",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Tab location",
        keywords: "terminal tab location top left right split panel",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Startup visibility",
        keywords: "terminal startup hide show empty last closed panel",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Paste handling",
        keywords: "terminal paste bracketed multiline warning clipboard",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_TERMINAL,
        group: "Interaction",
        title: "Word separators",
        keywords: "terminal word separators selection double click text",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_FILES,
        group: "Save Actions",
        title: "Format on save",
        keywords: "format save formatter files write",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_FILES,
        group: "Save Cleanup",
        title: "Trim trailing whitespace",
        keywords: "trim trailing whitespace spaces tabs save cleanup",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_FILES,
        group: "Save Cleanup",
        title: "Insert final newline",
        keywords: "final newline insert end file save cleanup",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_FILES,
        group: "Save Cleanup",
        title: "Trim final newlines",
        keywords: "trim final newlines trailing blank lines save cleanup",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_APPEARANCE,
        group: "Theme",
        title: "Theme",
        keywords: "theme appearance color palette built in custom plugin dropdown",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_APPEARANCE,
        group: "Theme",
        title: "Custom theme files",
        keywords: "theme appearance color palette custom style file toml input",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_APPEARANCE,
        group: "Fonts",
        title: "Editor font file",
        keywords: "font file editor custom choose clear bundled ttf otf",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_APPEARANCE,
        group: "Fonts",
        title: "UI font file",
        keywords: "font file ui interface custom choose clear bundled ttf otf",
    },
];

impl KuroyaApp {
    pub(crate) fn render_settings_panel(&mut self, ctx: &Context) {
        let mut actions = PendingSettingsPanelActions::default();
        let window_size = settings_window_size(ctx);
        let mut search_query = settings_panel_search_query(ctx);
        let mut highlighted_target = settings_panel_highlight_target(ctx);
        let mut pending_scroll_target = settings_panel_pending_scroll_target(ctx);
        let vim_capture_active = vim_key_capture_active(ctx);

        egui::Window::new("Settings")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
            .fixed_size(window_size)
            .min_size(SETTINGS_WINDOW_MIN_SIZE)
            .max_size(SETTINGS_WINDOW_SIZE)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(Key::Escape))
                    && settings_panel_escape_should_apply(vim_capture_active)
                {
                    apply_settings_panel_escape(&mut search_query, &mut actions);
                }

                self.settings_panel_section = self
                    .settings_panel_section
                    .min(SETTINGS_SECTIONS.len().saturating_sub(1));

                render_settings_search(
                    ui,
                    &mut search_query,
                    settings_search_enabled(vim_capture_active),
                );
                ui.add_space(6.0);

                let search_query_trimmed = search_query.trim().to_owned();
                let search_active = !search_query_trimmed.is_empty();
                let search_results = if search_active {
                    settings_search_results(&search_query_trimmed)
                } else {
                    Vec::new()
                };
                let body_height = (ui.available_height() - SETTINGS_FOOTER_HEIGHT).max(220.0);
                ui.horizontal(|ui| {
                    ui.set_height(body_height);
                    ui.vertical(|ui| {
                        ui.set_width(SETTINGS_SIDEBAR_WIDTH);
                        let previous_section = self.settings_panel_section;
                        ui.add_enabled_ui(
                            settings_panel_navigation_enabled(vim_capture_active),
                            |ui| render_settings_sidebar(ui, &mut self.settings_panel_section),
                        );
                        if self.settings_panel_section != previous_section {
                            if previous_section == SETTINGS_SECTION_VIM {
                                vim_key_capture_clear(ctx);
                            }
                            highlighted_target = None;
                            pending_scroll_target = None;
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        if search_active {
                            let clicked_result = render_settings_search_results(
                                ui,
                                &search_query_trimmed,
                                &search_results,
                                (body_height - 44.0).max(160.0),
                            );
                            if let Some(entry) = clicked_result {
                                search_query.clear();
                                self.settings_panel_section = entry.section;
                                let target = settings_search_entry_target(entry).to_owned();
                                highlighted_target = Some(target.clone());
                                pending_scroll_target = Some(target);
                            }
                        } else {
                            ui.heading(SETTINGS_SECTIONS[self.settings_panel_section]);
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .id_salt((
                                    "settings_panel_section_scroll",
                                    self.settings_panel_section,
                                ))
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                )
                                .max_height((body_height - 44.0).max(160.0))
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    let mut highlight = SettingsHighlightState::new(
                                        highlighted_target.as_deref(),
                                        &mut pending_scroll_target,
                                    );
                                    match self.settings_panel_section {
                                        SETTINGS_SECTION_GENERAL => render_general_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &mut highlight,
                                        ),
                                        SETTINGS_SECTION_EDITOR => render_editor_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &mut highlight,
                                        ),
                                        SETTINGS_SECTION_LSP => render_lsp_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &mut highlight,
                                        ),
                                        SETTINGS_SECTION_VIM => render_vim_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &mut highlight,
                                        ),
                                        SETTINGS_SECTION_TERMINAL => render_terminal_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &mut highlight,
                                        ),
                                        SETTINGS_SECTION_FILES => render_files_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &mut highlight,
                                        ),
                                        SETTINGS_SECTION_APPEARANCE => render_appearance_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &self.workspace.root,
                                            &self.plugin_themes,
                                            &self.settings_editor_font_path,
                                            &self.settings_ui_font_path,
                                            &mut actions.choose_editor_font,
                                            &mut actions.clear_editor_font,
                                            &mut actions.choose_ui_font,
                                            &mut actions.clear_ui_font,
                                            &mut actions.status,
                                            &mut highlight,
                                        ),
                                        SETTINGS_SECTION_SOURCE_CONTROL => {
                                            render_source_control_settings(
                                                ui,
                                                &mut self.settings_panel_draft,
                                                &mut highlight,
                                            )
                                        }
                                        SETTINGS_SECTION_DEVELOPER => render_developer_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &mut highlight,
                                        ),
                                        _ => {}
                                    }
                                });
                        }
                    });
                });

                ui.separator();
                ui.horizontal(|ui| {
                    let validation = self.settings_panel_draft_validation();
                    let footer_text = validation.footer_message();
                    let footer_color = if validation.has_warnings() {
                        ui.visuals().warn_fg_color
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    let has_pending_inputs = validation.has_pending_inputs();
                    let footer_actions_enabled =
                        settings_panel_footer_actions_enabled(vim_capture_active);
                    let can_reset = has_pending_inputs
                        || self.settings_panel_default_candidate() != self.settings;
                    let footer_action_width =
                        4.0 * SETTINGS_FOOTER_BUTTON_WIDTH + 4.0 * ui.spacing().item_spacing.x;
                    let footer_width = (ui.available_width() - footer_action_width).max(0.0);
                    ui.add_sized(
                        [footer_width, ui.spacing().interact_size.y],
                        egui::Label::new(egui::RichText::new(footer_text).color(footer_color))
                            .truncate(),
                    );

                    if popup_button_enabled(
                        ui,
                        footer_actions_enabled && has_pending_inputs,
                        "Apply",
                        PopupButtonKind::Primary,
                    )
                    .clicked()
                    {
                        actions.apply = true;
                    }
                    if popup_button_enabled(
                        ui,
                        footer_actions_enabled && can_reset,
                        "Reset",
                        PopupButtonKind::Secondary,
                    )
                    .on_hover_text("Reset settings to defaults in the draft; Apply saves them")
                    .clicked()
                    {
                        actions.reset = true;
                    }
                    if popup_button_enabled(
                        ui,
                        footer_actions_enabled,
                        "Reload",
                        PopupButtonKind::Secondary,
                    )
                    .clicked()
                    {
                        actions.reload = true;
                    }
                    if popup_button_enabled(
                        ui,
                        footer_actions_enabled,
                        settings_panel_close_button_label(has_pending_inputs),
                        PopupButtonKind::Secondary,
                    )
                    .on_hover_text(settings_panel_close_button_hover_text(has_pending_inputs))
                    .clicked()
                    {
                        actions.close = true;
                    }
                });
            });

        if actions.close {
            vim_key_capture_clear(ctx);
            search_query.clear();
            highlighted_target = None;
            pending_scroll_target = None;
        }
        set_settings_panel_search_query(ctx, search_query);
        set_settings_panel_highlight_target(ctx, highlighted_target);
        set_settings_panel_pending_scroll_target(ctx, pending_scroll_target);
        self.apply_settings_panel_actions(actions);
    }
}

fn settings_window_size(ctx: &Context) -> [f32; 2] {
    let available = ctx.available_rect().size();
    [
        SETTINGS_WINDOW_SIZE[0].min((available.x - 32.0).max(SETTINGS_WINDOW_MIN_SIZE[0])),
        SETTINGS_WINDOW_SIZE[1].min((available.y - 96.0).max(SETTINGS_WINDOW_MIN_SIZE[1])),
    ]
}

fn settings_panel_close_button_label(has_pending_inputs: bool) -> &'static str {
    if has_pending_inputs {
        "Cancel"
    } else {
        "Close"
    }
}

fn settings_panel_close_button_hover_text(has_pending_inputs: bool) -> &'static str {
    if has_pending_inputs {
        "Close settings without applying changes"
    } else {
        "Close settings"
    }
}

fn apply_settings_panel_escape(
    search_query: &mut String,
    actions: &mut PendingSettingsPanelActions,
) {
    if search_query.trim().is_empty() {
        actions.close = true;
    } else {
        search_query.clear();
    }
}

fn settings_panel_escape_should_apply(vim_capture_active: bool) -> bool {
    !vim_capture_active
}

fn settings_search_enabled(vim_capture_active: bool) -> bool {
    !vim_capture_active
}

fn settings_panel_navigation_enabled(vim_capture_active: bool) -> bool {
    !vim_capture_active
}

fn settings_panel_footer_actions_enabled(vim_capture_active: bool) -> bool {
    !vim_capture_active
}

fn settings_panel_search_query(ctx: &Context) -> String {
    ctx.data_mut(|data| {
        data.get_temp::<String>(egui::Id::new(SETTINGS_SEARCH_QUERY_ID))
            .unwrap_or_default()
    })
}

fn set_settings_panel_search_query(ctx: &Context, query: String) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(SETTINGS_SEARCH_QUERY_ID), query));
}

fn settings_panel_highlight_target(ctx: &Context) -> Option<String> {
    ctx.data_mut(|data| data.get_temp::<String>(egui::Id::new(SETTINGS_HIGHLIGHT_TARGET_ID)))
}

fn set_settings_panel_highlight_target(ctx: &Context, target: Option<String>) {
    ctx.data_mut(|data| {
        if let Some(target) = target {
            data.insert_temp(egui::Id::new(SETTINGS_HIGHLIGHT_TARGET_ID), target);
        } else {
            data.remove::<String>(egui::Id::new(SETTINGS_HIGHLIGHT_TARGET_ID));
        }
    });
}

fn settings_panel_pending_scroll_target(ctx: &Context) -> Option<String> {
    ctx.data_mut(|data| data.get_temp::<String>(egui::Id::new(SETTINGS_PENDING_SCROLL_TARGET_ID)))
}

fn set_settings_panel_pending_scroll_target(ctx: &Context, target: Option<String>) {
    ctx.data_mut(|data| {
        if let Some(target) = target {
            data.insert_temp(egui::Id::new(SETTINGS_PENDING_SCROLL_TARGET_ID), target);
        } else {
            data.remove::<String>(egui::Id::new(SETTINGS_PENDING_SCROLL_TARGET_ID));
        }
    });
}

fn render_settings_search(ui: &mut egui::Ui, query: &mut String, enabled: bool) {
    let sanitized = bounded_settings_singleline_input(query);
    if sanitized != *query {
        *query = sanitized;
    }

    ui.horizontal(|ui| {
        icon_label(
            ui,
            IconKind::Search,
            ui.visuals().weak_text_color(),
            "Search settings",
        );
        let clear_button_width = if query.is_empty() {
            0.0
        } else {
            ui.spacing().interact_size.y + ui.spacing().item_spacing.x
        };
        let search_width = (ui.available_width() - clear_button_width).max(120.0);
        let search_response = ui
            .add_enabled_ui(enabled, |ui| {
                ui.add_sized(
                    [search_width, ui.spacing().interact_size.y],
                    egui::TextEdit::singleline(query)
                        .hint_text("Search settings")
                        .clip_text(true),
                )
            })
            .inner;
        if enabled
            && !query.is_empty()
            && icon_button(ui, IconKind::Close, "Clear search").clicked()
        {
            query.clear();
            search_response.request_focus();
        }
    });
}

fn render_settings_search_results(
    ui: &mut egui::Ui,
    query: &str,
    results: &[SettingsSearchEntry],
    max_height: f32,
) -> Option<SettingsSearchEntry> {
    ui.horizontal(|ui| {
        ui.heading("Search results");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(settings_search_count_label(results.len()))
                    .color(ui.visuals().weak_text_color()),
            );
        });
    });
    ui.separator();

    let mut clicked_result = None;
    egui::ScrollArea::vertical()
        .id_salt("settings_panel_search_results_scroll")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .max_height(max_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if results.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!("No settings found for \"{}\"", query))
                        .color(ui.visuals().weak_text_color()),
                );
                return;
            }

            for entry in results {
                let label = format!("{} > {}", SETTINGS_SECTIONS[entry.section], entry.title);
                let detail = format!("{} section", entry.group);
                let response = ui.add_sized(
                    [ui.available_width(), 32.0],
                    egui::Button::new(egui::RichText::new(label).strong())
                        .fill(egui::Color32::TRANSPARENT),
                );
                response
                    .clone()
                    .on_hover_text(format!("Open {} in Settings", entry.title));
                if response.clicked() {
                    clicked_result = Some(*entry);
                }
                ui.add_sized(
                    [ui.available_width(), 18.0],
                    egui::Label::new(
                        egui::RichText::new(detail).color(ui.visuals().weak_text_color()),
                    )
                    .truncate(),
                );
                ui.add_space(4.0);
            }
        });

    clicked_result
}

fn settings_search_count_label(count: usize) -> String {
    if count == 1 {
        "1 result".to_owned()
    } else {
        format!("{count} results")
    }
}

fn settings_search_entry_target(entry: SettingsSearchEntry) -> &'static str {
    match (entry.section, entry.group) {
        (SETTINGS_SECTION_GENERAL, _) => SETTINGS_TARGET_GENERAL,
        (SETTINGS_SECTION_EDITOR, "Text and Layout") => SETTINGS_TARGET_EDITOR_TEXT_LAYOUT,
        (SETTINGS_SECTION_EDITOR, "Display") => SETTINGS_TARGET_EDITOR_DISPLAY,
        (SETTINGS_SECTION_EDITOR, "Typing Assistance") => SETTINGS_TARGET_EDITOR_TYPING,
        (SETTINGS_SECTION_EDITOR, "Language Features") => SETTINGS_TARGET_EDITOR_LANGUAGE,
        (SETTINGS_SECTION_EDITOR, "Cursor and Highlight") => SETTINGS_TARGET_EDITOR_CURSOR,
        (SETTINGS_SECTION_EDITOR, "Code View") => SETTINGS_TARGET_EDITOR_CODE_VIEW,
        (SETTINGS_SECTION_EDITOR, "Diff Editor") => SETTINGS_TARGET_EDITOR_DIFF,
        (SETTINGS_SECTION_EDITOR, "Scrollbars") => SETTINGS_TARGET_SCROLLBARS,
        (SETTINGS_SECTION_VIM, _) => SETTINGS_TARGET_VIM_KEYBINDINGS,
        (SETTINGS_SECTION_TERMINAL, "Profile") => SETTINGS_TARGET_TERMINAL_PROFILE,
        (SETTINGS_SECTION_TERMINAL, "Buffer and Text") => SETTINGS_TARGET_TERMINAL_BUFFER,
        (SETTINGS_SECTION_TERMINAL, "Cursor") => SETTINGS_TARGET_TERMINAL_CURSOR,
        (SETTINGS_SECTION_TERMINAL, "Color and Feedback") => SETTINGS_TARGET_TERMINAL_COLOR,
        (SETTINGS_SECTION_TERMINAL, "Interaction") => SETTINGS_TARGET_TERMINAL_INTERACTION,
        (SETTINGS_SECTION_FILES, "Save Actions") => SETTINGS_TARGET_FILES_SAVE_ACTIONS,
        (SETTINGS_SECTION_FILES, "Save Cleanup") => SETTINGS_TARGET_FILES_SAVE_CLEANUP,
        (SETTINGS_SECTION_LSP, _) => SETTINGS_TARGET_LSP,
        (SETTINGS_SECTION_APPEARANCE, _) => SETTINGS_TARGET_APPEARANCE,
        (SETTINGS_SECTION_SOURCE_CONTROL, _) => SETTINGS_TARGET_SOURCE_CONTROL,
        (SETTINGS_SECTION_DEVELOPER, _) => SETTINGS_TARGET_DEVELOPER,
        _ => SETTINGS_TARGET_GENERAL,
    }
}

fn settings_search_results(query: &str) -> Vec<SettingsSearchEntry> {
    let tokens = settings_search_tokens(query);
    if tokens.is_empty() {
        return Vec::new();
    }

    SETTINGS_SEARCH_ENTRIES
        .iter()
        .copied()
        .filter(|entry| settings_search_entry_matches(entry, &tokens))
        .collect()
}

fn settings_search_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_alphanumeric())
        .filter_map(|part| {
            let token = part.trim().to_lowercase();
            (!token.is_empty()).then_some(token)
        })
        .collect()
}

fn settings_search_entry_matches(entry: &SettingsSearchEntry, tokens: &[String]) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        SETTINGS_SECTIONS[entry.section], entry.group, entry.title, entry.keywords
    )
    .to_lowercase();
    tokens.iter().all(|token| haystack.contains(token))
}

#[cfg(test)]
mod tests {
    use super::actions::PendingSettingsPanelActions;
    use super::{
        SETTINGS_SECTION_APPEARANCE, SETTINGS_SECTION_DEVELOPER, SETTINGS_SECTION_EDITOR,
        SETTINGS_SECTION_FILES, SETTINGS_SECTION_GENERAL, SETTINGS_SECTION_LSP,
        SETTINGS_SECTION_SOURCE_CONTROL, SETTINGS_SECTION_TERMINAL, SETTINGS_SECTION_VIM,
        SETTINGS_TARGET_APPEARANCE, SETTINGS_TARGET_DEVELOPER, SETTINGS_TARGET_FILES_SAVE_ACTIONS,
        SETTINGS_TARGET_GENERAL, SETTINGS_TARGET_LSP, SETTINGS_TARGET_SCROLLBARS,
        SETTINGS_TARGET_SOURCE_CONTROL, SETTINGS_TARGET_TERMINAL_INTERACTION,
        SETTINGS_TARGET_VIM_KEYBINDINGS, apply_settings_panel_escape,
        settings_panel_close_button_hover_text, settings_panel_close_button_label,
        settings_panel_escape_should_apply, settings_panel_footer_actions_enabled,
        settings_panel_navigation_enabled, settings_search_count_label, settings_search_enabled,
        settings_search_entry_target, settings_search_results, settings_search_tokens,
    };

    #[test]
    fn settings_search_finds_vim_keybindings() {
        let results = settings_search_results("vim mode");

        assert!(results.iter().any(|entry| {
            entry.section == SETTINGS_SECTION_VIM && entry.title == "Vim keybindings"
        }));
    }

    #[test]
    fn settings_search_finds_all_vim_binding_entries() {
        for (query, title) in [
            ("vim disable binding", "Disabled Vim bindings"),
            ("vim override remap", "Vim overrides"),
            ("vim custom command", "Vim overrides"),
        ] {
            let results = settings_search_results(query);

            assert!(
                results
                    .iter()
                    .any(|entry| entry.section == SETTINGS_SECTION_VIM && entry.title == title),
                "{query:?} should find {title:?}"
            );
        }
    }

    #[test]
    fn settings_search_finds_terminal_right_click() {
        let results = settings_search_results("terminal right click");

        assert!(results.iter().any(|entry| {
            entry.section == SETTINGS_SECTION_TERMINAL && entry.title == "Right click"
        }));
    }

    #[test]
    fn settings_search_finds_lsp_servers() {
        let results = settings_search_results("lsp server");

        let entry = results
            .iter()
            .find(|entry| entry.section == SETTINGS_SECTION_LSP && entry.title == "LSP servers")
            .copied()
            .expect("lsp server search result");
        assert_eq!(settings_search_entry_target(entry), SETTINGS_TARGET_LSP);
    }

    #[test]
    fn settings_search_finds_appearance_themes() {
        let results = settings_search_results("custom theme");

        assert!(results.iter().any(|entry| {
            entry.section == SETTINGS_SECTION_APPEARANCE && entry.title == "Custom theme files"
        }));
    }

    #[test]
    fn settings_search_routes_split_entries_to_logical_sections() {
        for (query, title, section, target) in [
            (
                "autosave delay",
                "Autosave delay",
                SETTINGS_SECTION_FILES,
                SETTINGS_TARGET_FILES_SAVE_ACTIONS,
            ),
            (
                "window zoom",
                "Window zoom",
                SETTINGS_SECTION_GENERAL,
                SETTINGS_TARGET_GENERAL,
            ),
            (
                "status bar visible",
                "Status bar",
                SETTINGS_SECTION_GENERAL,
                SETTINGS_TARGET_GENERAL,
            ),
            (
                "ui font size",
                "UI font size",
                SETTINGS_SECTION_GENERAL,
                SETTINGS_TARGET_GENERAL,
            ),
            (
                "font family",
                "Font family",
                SETTINGS_SECTION_EDITOR,
                super::SETTINGS_TARGET_EDITOR_TEXT_LAYOUT,
            ),
            (
                "minimap",
                "Minimap",
                SETTINGS_SECTION_GENERAL,
                SETTINGS_TARGET_GENERAL,
            ),
            (
                "minimap side",
                "Editor minimap",
                SETTINGS_SECTION_EDITOR,
                super::SETTINGS_TARGET_EDITOR_CODE_VIEW,
            ),
            (
                "smooth scroll",
                "Smooth scrolling",
                SETTINGS_SECTION_GENERAL,
                SETTINGS_TARGET_GENERAL,
            ),
            (
                "scroll beyond last line",
                "Scroll beyond last line",
                SETTINGS_SECTION_GENERAL,
                SETTINGS_TARGET_GENERAL,
            ),
            (
                "scrollbar visible",
                "Scrollbars",
                SETTINGS_SECTION_EDITOR,
                SETTINGS_TARGET_SCROLLBARS,
            ),
            (
                "devtools profiling",
                "Devtools",
                SETTINGS_SECTION_DEVELOPER,
                SETTINGS_TARGET_DEVELOPER,
            ),
            (
                "lsp server",
                "LSP servers",
                SETTINGS_SECTION_LSP,
                SETTINGS_TARGET_LSP,
            ),
            (
                "lsp hover",
                "Hover",
                SETTINGS_SECTION_LSP,
                SETTINGS_TARGET_LSP,
            ),
            (
                "code lens",
                "Code lens",
                SETTINGS_SECTION_LSP,
                SETTINGS_TARGET_LSP,
            ),
            (
                "inlay hints",
                "Inlay hints",
                SETTINGS_SECTION_LSP,
                SETTINGS_TARGET_LSP,
            ),
            (
                "signature help",
                "Parameter hints",
                SETTINGS_SECTION_LSP,
                SETTINGS_TARGET_LSP,
            ),
            (
                "git autofetch",
                "Git fetch, pull, and sync",
                SETTINGS_SECTION_SOURCE_CONTROL,
                SETTINGS_TARGET_SOURCE_CONTROL,
            ),
        ] {
            let entry = search_result(query, title);

            assert_eq!(
                entry.section, section,
                "{query:?} should route to {section}"
            );
            assert_eq!(settings_search_entry_target(entry), target);
        }
    }

    #[test]
    fn settings_search_entries_resolve_to_scroll_targets() {
        let vim = settings_search_results("vim")
            .into_iter()
            .find(|entry| entry.title == "Vim keybindings")
            .expect("vim search result");
        let disabled_vim = settings_search_results("vim disabled")
            .into_iter()
            .find(|entry| entry.title == "Disabled Vim bindings")
            .expect("disabled vim search result");
        let vim_overrides = settings_search_results("vim override")
            .into_iter()
            .find(|entry| entry.title == "Vim overrides")
            .expect("vim override search result");
        let right_click = settings_search_results("terminal right click")
            .into_iter()
            .find(|entry| entry.title == "Right click")
            .expect("terminal right click result");
        let custom_theme = settings_search_results("custom theme")
            .into_iter()
            .find(|entry| entry.title == "Custom theme files")
            .expect("custom theme result");
        let scrollbars = settings_search_results("horizontal scrollbar")
            .into_iter()
            .find(|entry| entry.title == "Scrollbars")
            .expect("scrollbars result");
        let lsp_servers = settings_search_results("lsp server")
            .into_iter()
            .find(|entry| entry.title == "LSP servers")
            .expect("lsp server result");
        let hover = settings_search_results("lsp hover")
            .into_iter()
            .find(|entry| entry.title == "Hover")
            .expect("hover result");
        let code_lens = settings_search_results("code lens")
            .into_iter()
            .find(|entry| entry.title == "Code lens")
            .expect("code lens result");
        let inlay_hints = settings_search_results("inlay hints")
            .into_iter()
            .find(|entry| entry.title == "Inlay hints")
            .expect("inlay hints result");

        assert_eq!(
            settings_search_entry_target(vim),
            SETTINGS_TARGET_VIM_KEYBINDINGS
        );
        assert_eq!(
            settings_search_entry_target(disabled_vim),
            SETTINGS_TARGET_VIM_KEYBINDINGS
        );
        assert_eq!(
            settings_search_entry_target(vim_overrides),
            SETTINGS_TARGET_VIM_KEYBINDINGS
        );
        assert_eq!(
            settings_search_entry_target(right_click),
            SETTINGS_TARGET_TERMINAL_INTERACTION
        );
        assert_eq!(
            settings_search_entry_target(custom_theme),
            SETTINGS_TARGET_APPEARANCE
        );
        assert_eq!(
            settings_search_entry_target(scrollbars),
            SETTINGS_TARGET_SCROLLBARS
        );
        assert_eq!(
            settings_search_entry_target(lsp_servers),
            SETTINGS_TARGET_LSP
        );
        assert_eq!(settings_search_entry_target(hover), SETTINGS_TARGET_LSP);
        assert_eq!(settings_search_entry_target(code_lens), SETTINGS_TARGET_LSP);
        assert_eq!(
            settings_search_entry_target(inlay_hints),
            SETTINGS_TARGET_LSP
        );
    }

    #[test]
    fn settings_search_splits_symbols_and_ignores_empty_tokens() {
        assert_eq!(
            settings_search_tokens("  font-size / vim  "),
            vec!["font", "size", "vim"]
        );
    }

    #[test]
    fn settings_search_returns_empty_for_blank_or_unknown_queries() {
        assert!(settings_search_results("  ").is_empty());
        assert!(settings_search_results("zzzz-not-a-setting").is_empty());
    }

    #[test]
    fn settings_search_count_label_uses_singular_and_plural() {
        assert_eq!(settings_search_count_label(1), "1 result");
        assert_eq!(settings_search_count_label(2), "2 results");
    }

    #[test]
    fn settings_close_button_reflects_pending_inputs() {
        assert_eq!(settings_panel_close_button_label(false), "Close");
        assert_eq!(
            settings_panel_close_button_hover_text(false),
            "Close settings"
        );
        assert_eq!(settings_panel_close_button_label(true), "Cancel");
        assert_eq!(
            settings_panel_close_button_hover_text(true),
            "Close settings without applying changes"
        );
    }

    #[test]
    fn settings_escape_clears_search_before_closing() {
        let mut query = "vim".to_owned();
        let mut actions = PendingSettingsPanelActions::default();

        apply_settings_panel_escape(&mut query, &mut actions);

        assert!(query.is_empty());
        assert!(!actions.close);

        apply_settings_panel_escape(&mut query, &mut actions);

        assert!(actions.close);
    }

    #[test]
    fn settings_escape_is_blocked_while_vim_key_capture_is_active() {
        assert!(settings_panel_escape_should_apply(false));
        assert!(!settings_panel_escape_should_apply(true));
    }

    #[test]
    fn settings_search_is_disabled_while_vim_key_capture_is_active() {
        assert!(settings_search_enabled(false));
        assert!(!settings_search_enabled(true));
    }

    #[test]
    fn settings_navigation_is_disabled_while_vim_key_capture_is_active() {
        assert!(settings_panel_navigation_enabled(false));
        assert!(!settings_panel_navigation_enabled(true));
    }

    #[test]
    fn settings_footer_actions_are_disabled_while_vim_key_capture_is_active() {
        assert!(settings_panel_footer_actions_enabled(false));
        assert!(!settings_panel_footer_actions_enabled(true));
    }

    fn search_result(query: &str, title: &str) -> super::SettingsSearchEntry {
        settings_search_results(query)
            .into_iter()
            .find(|entry| entry.title == title)
            .unwrap_or_else(|| panic!("{query:?} should find {title:?}"))
    }
}

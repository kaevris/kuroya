use eframe::egui::{self, Context, Key};

use crate::{
    KuroyaApp,
    app_update_overlays::PopupDismissalGuard,
    popup_buttons::{PopupButtonKind, popup_button_enabled},
    ui_icons::{IconKind, icon_button, icon_label},
    ui_state::{handle_list_navigation_keys, selected_row_scroll_offset, selection_page_step},
};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};

mod actions;
mod apply;
mod background_files;
mod font_files;
mod sections;

use actions::PendingSettingsPanelActions;
use sections::{
    SETTINGS_SECTION_APPEARANCE, SETTINGS_SECTION_DEVELOPER, SETTINGS_SECTION_DISCORD,
    SETTINGS_SECTION_EDITOR, SETTINGS_SECTION_FILES, SETTINGS_SECTION_GENERAL,
    SETTINGS_SECTION_LSP, SETTINGS_SECTION_PLUGINS, SETTINGS_SECTION_SOURCE_CONTROL,
    SETTINGS_SECTION_TERMINAL, SETTINGS_SECTION_VIM, SETTINGS_SECTIONS, SETTINGS_TARGET_APPEARANCE,
    SETTINGS_TARGET_DEVELOPER, SETTINGS_TARGET_EDITOR_CODE_VIEW, SETTINGS_TARGET_EDITOR_CURSOR,
    SETTINGS_TARGET_EDITOR_DIFF, SETTINGS_TARGET_EDITOR_DISPLAY, SETTINGS_TARGET_EDITOR_LANGUAGE,
    SETTINGS_TARGET_EDITOR_TEXT_LAYOUT, SETTINGS_TARGET_EDITOR_TYPING,
    SETTINGS_TARGET_FILES_SAVE_ACTIONS, SETTINGS_TARGET_FILES_SAVE_CLEANUP,
    SETTINGS_TARGET_GENERAL, SETTINGS_TARGET_LSP, SETTINGS_TARGET_SCROLLBARS,
    SETTINGS_TARGET_SOURCE_CONTROL, SETTINGS_TARGET_TERMINAL_BUFFER,
    SETTINGS_TARGET_TERMINAL_COLOR, SETTINGS_TARGET_TERMINAL_CURSOR,
    SETTINGS_TARGET_TERMINAL_INTERACTION, SETTINGS_TARGET_TERMINAL_PROFILE,
    SETTINGS_TARGET_VIM_KEYBINDINGS, SettingsHighlightState, bounded_settings_singleline_input,
    render_appearance_settings, render_developer_settings, render_discord_settings,
    render_editor_settings, render_files_settings, render_general_settings, render_lsp_settings,
    render_plugins_settings, render_settings_section_picker, render_settings_sidebar,
    render_source_control_settings, render_terminal_settings, render_vim_settings,
    vim_key_capture_active, vim_key_capture_clear,
};

const SETTINGS_WINDOW_PREFERRED_SIZE: [f32; 2] = [920.0, 640.0];
const SETTINGS_WINDOW_MIN_SIZE: [f32; 2] = [380.0, 320.0];
const SETTINGS_WINDOW_MARGIN: [f32; 2] = [48.0, 120.0];
const SETTINGS_WIDE_LAYOUT_MIN_WIDTH: f32 = 720.0;
const SETTINGS_SIDEBAR_WIDTH_RATIO: f32 = 0.21;
const SETTINGS_SIDEBAR_MIN_WIDTH: f32 = 180.0;
const SETTINGS_SIDEBAR_MAX_WIDTH: f32 = 240.0;
const SETTINGS_FOOTER_HEIGHT: f32 = 38.0;
const SETTINGS_FOOTER_BUTTON_WIDTH: f32 = 78.0;
const SETTINGS_CONTENT_HEADER_HEIGHT: f32 = 44.0;
const SETTINGS_SEARCH_QUERY_ID: &str = "settings-panel-search-query";
const SETTINGS_SEARCH_SELECTION_ID: &str = "settings-panel-search-selection";
const SETTINGS_HIGHLIGHT_TARGET_ID: &str = "settings-panel-highlight-target";
const SETTINGS_PENDING_SCROLL_TARGET_ID: &str = "settings-panel-pending-scroll-target";
const SETTINGS_SEARCH_RESULT_BUTTON_HEIGHT: f32 = 32.0;
const SETTINGS_SEARCH_RESULT_DETAIL_HEIGHT: f32 = 18.0;
const SETTINGS_SEARCH_RESULT_ROW_GAP: f32 = 4.0;
const SETTINGS_CONFIRM_DISCARD_ID: &str = "settings-panel-confirm-discard";
const SETTINGS_CONFIRM_DISCARD_PROMPT: &str = "Unsaved changes — click again to discard";

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
        title: "Application info",
        keywords: "app version package build profile target platform settings schema update source repository",
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
    SettingsSearchEntry {
        section: SETTINGS_SECTION_APPEARANCE,
        group: "Editor background",
        title: "Editor background image",
        keywords: "editor background image wallpaper picture choose clear enable dim opacity",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_APPEARANCE,
        group: "Editor background",
        title: "Editor background area",
        keywords: "editor background image wallpaper scope area full app editor only",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_APPEARANCE,
        group: "Editor background",
        title: "Editor background scaling",
        keywords: "editor background image scaling fit cover contain stretch size",
    },
    SettingsSearchEntry {
        section: SETTINGS_SECTION_APPEARANCE,
        group: "Editor background",
        title: "Editor background position",
        keywords: "editor background image vertical position top center bottom alignment",
    },
];

impl KuroyaApp {
    pub(crate) fn render_settings_panel(&mut self, ctx: &Context) {
        let mut actions = PendingSettingsPanelActions::default();
        let dismissal = PopupDismissalGuard::capture(ctx);
        let opened_key = egui::Id::new("settings_panel_was_open_last_frame");
        let was_open = ctx.data_mut(|data| *data.get_temp::<bool>(opened_key).get_or_insert(false));
        if !was_open {
            reset_settings_panel_memory(ctx);
            self.sync_settings_panel_inputs();
        }

        if !self.settings_panel_has_pending_inputs() {
            set_settings_panel_confirm_discard_armed(ctx, false);
        }
        record_settings_panel_focus(ctx);
        let window_size = settings_window_size(ctx);
        let mut search_query = settings_panel_search_query(ctx);
        let mut highlighted_target = settings_panel_highlight_target(ctx);
        let mut pending_scroll_target = settings_panel_pending_scroll_target(ctx);
        let vim_capture_active = vim_key_capture_active(ctx);

        let window_response = egui::Window::new("Settings")
            .min_size(egui::Vec2::from(window_size))
            .max_size(egui::Vec2::from(window_size))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
            .fixed_size(window_size)
            .show(ctx, |ui| {
                ui.set_min_width(window_size[0]);
                ui.set_max_width(window_size[0]);
                if ui.input(|input| input.key_pressed(Key::Escape))
                    && settings_panel_escape_should_apply(vim_capture_active)
                {
                    let mut discard_confirmed = settings_panel_confirm_discard_armed(ctx);
                    let has_pending_inputs =
                        self.settings_panel_draft_validation().has_pending_inputs();
                    apply_settings_panel_escape(
                        &mut search_query,
                        &mut actions,
                        &mut discard_confirmed,
                        has_pending_inputs,
                    );
                    set_settings_panel_confirm_discard_armed(ctx, discard_confirmed);
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
                let wide_layout = settings_panel_uses_sidebar(ui.available_width());
                if !wide_layout {
                    let previous_section = self.settings_panel_section;
                    ui.add_enabled_ui(
                        settings_panel_navigation_enabled(vim_capture_active),
                        |ui| render_settings_section_picker(ui, &mut self.settings_panel_section),
                    );
                    apply_settings_panel_section_change(
                        ctx,
                        previous_section,
                        self.settings_panel_section,
                        &mut highlighted_target,
                        &mut pending_scroll_target,
                    );
                    ui.add_space(ui.spacing().item_spacing.y);
                }

                let body_height = (ui.available_height() - SETTINGS_FOOTER_HEIGHT).max(0.0);
                ui.horizontal(|ui| {
                    ui.set_height(body_height);
                    if wide_layout {
                        ui.vertical(|ui| {
                            ui.set_width(settings_sidebar_width(ui.available_width()));
                            let previous_section = self.settings_panel_section;
                            ui.add_enabled_ui(
                                settings_panel_navigation_enabled(vim_capture_active),
                                |ui| render_settings_sidebar(ui, &mut self.settings_panel_section),
                            );
                            apply_settings_panel_section_change(
                                ctx,
                                previous_section,
                                self.settings_panel_section,
                                &mut highlighted_target,
                                &mut pending_scroll_target,
                            );
                        });
                        ui.separator();
                    }
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        if search_active {
                            let clicked_result = render_settings_search_results(
                                ui,
                                &search_query_trimmed,
                                &search_results,
                                settings_content_height(body_height),
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

                            egui::ScrollArea::both()
                                .id_salt((
                                    "settings_panel_section_scroll",
                                    self.settings_panel_section,
                                ))
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                )
                                .max_height(settings_content_height(body_height))
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
                                            &mut actions.choose_background_image,
                                            &mut actions.clear_background_image,
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
                                        SETTINGS_SECTION_PLUGINS => render_plugins_settings(
                                            ui,
                                            &mut self.settings_panel_draft,
                                            &self.plugins,
                                            &mut highlight,
                                        ),
                                        SETTINGS_SECTION_DISCORD => render_discord_settings(
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
                    let confirm_discard_armed = settings_panel_confirm_discard_armed(ctx)
                        && validation.has_pending_inputs();
                    let footer_text = if confirm_discard_armed {
                        SETTINGS_CONFIRM_DISCARD_PROMPT.to_owned()
                    } else {
                        validation.footer_message()
                    };
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

        if window_response
            .as_ref()
            .is_some_and(|response| dismissal.clicked_outside(ctx, response.response.rect))
        {
            let mut discard_confirmed = settings_panel_confirm_discard_armed(ctx);
            apply_settings_panel_outside_click(
                &mut actions,
                &mut discard_confirmed,
                self.settings_panel_has_pending_inputs(),
            );
            set_settings_panel_confirm_discard_armed(ctx, discard_confirmed);
        }

        #[cfg(debug_assertions)]
        if std::env::var("KUROYA_DEBUG_SETTINGS").is_ok() {
            if let Some(response) = window_response.as_ref() {
                let rect = response.response.rect;
                use std::fmt::Write as _;
                let mut line = String::new();
                let _ = writeln!(
                    line,
                    "{}, {:?}",
                    SETTINGS_SECTIONS[self.settings_panel_section],
                    (rect.left(), rect.top(), rect.width(), rect.height())
                );
                let path = std::env::temp_dir().join("kuroya_settings_debug.log");
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
            }
            static DEBUG_FRAME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let frame = DEBUG_FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if frame != 0 && frame.is_multiple_of(120) {
                self.settings_panel_section =
                    (self.settings_panel_section + 1) % SETTINGS_SECTIONS.len();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        if actions.close {
            vim_key_capture_clear(ctx);
            search_query.clear();
            highlighted_target = None;
            pending_scroll_target = None;
        }
        set_settings_panel_search_query(ctx, search_query);
        set_settings_panel_highlight_target(ctx, highlighted_target);
        set_settings_panel_pending_scroll_target(ctx, pending_scroll_target);
        ctx.data_mut(|data| data.insert_temp(opened_key, true));
        let closed = actions.close;
        self.apply_settings_panel_actions(actions);
        if closed {
            restore_settings_panel_focus(ctx);
        }
    }
}

fn reset_settings_panel_memory(ctx: &Context) {
    let options = ctx.memory(|memory| memory.options.clone());
    let focused = ctx.memory(|memory| memory.focused());
    ctx.memory_mut(|memory| *memory = egui::Memory::default());
    ctx.memory_mut(|memory| {
        memory.options = options;
        if let Some(id) = focused {
            memory.request_focus(id);
        }
    });
}

fn record_settings_panel_focus(ctx: &Context) {
    if let Some(id) = ctx.memory(|memory| memory.focused()) {
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("settings_panel_focus_to_restore"), Some(id))
        });
    }
}

fn restore_settings_panel_focus(ctx: &Context) {
    let focused = ctx.data_mut(|data| {
        data.remove_temp::<Option<egui::Id>>(egui::Id::new("settings_panel_focus_to_restore"))
    });
    if let Some(id) = focused.flatten() {
        ctx.memory_mut(|memory| memory.request_focus(id));
    }
}

fn settings_window_size(ctx: &Context) -> [f32; 2] {
    let available = ctx.content_rect().size();
    settings_window_size_for_available(available.x, available.y)
}

fn settings_window_size_for_available(available_width: f32, available_height: f32) -> [f32; 2] {
    [
        settings_window_dimension(
            SETTINGS_WINDOW_PREFERRED_SIZE[0],
            SETTINGS_WINDOW_MIN_SIZE[0],
            available_width,
            SETTINGS_WINDOW_MARGIN[0],
        ),
        settings_window_dimension(
            SETTINGS_WINDOW_PREFERRED_SIZE[1],
            SETTINGS_WINDOW_MIN_SIZE[1],
            available_height,
            SETTINGS_WINDOW_MARGIN[1],
        ),
    ]
}

fn settings_window_dimension(preferred: f32, minimum: f32, available: f32, margin: f32) -> f32 {
    if !available.is_finite() || available <= 0.0 {
        return preferred;
    }

    let usable = (available - margin).max(1.0);
    preferred.min(usable).max(minimum.min(usable))
}

fn settings_panel_uses_sidebar(available_width: f32) -> bool {
    available_width.is_finite() && available_width >= SETTINGS_WIDE_LAYOUT_MIN_WIDTH
}

fn settings_sidebar_width(available_width: f32) -> f32 {
    if !available_width.is_finite() || available_width <= 0.0 {
        return SETTINGS_SIDEBAR_MIN_WIDTH;
    }

    (available_width * SETTINGS_SIDEBAR_WIDTH_RATIO)
        .clamp(SETTINGS_SIDEBAR_MIN_WIDTH, SETTINGS_SIDEBAR_MAX_WIDTH)
        .min(available_width)
}

fn settings_content_height(body_height: f32) -> f32 {
    if !body_height.is_finite() {
        return 0.0;
    }
    (body_height - SETTINGS_CONTENT_HEADER_HEIGHT).max(0.0)
}

fn apply_settings_panel_section_change(
    ctx: &Context,
    previous_section: usize,
    current_section: usize,
    highlighted_target: &mut Option<String>,
    pending_scroll_target: &mut Option<String>,
) {
    if previous_section == current_section {
        return;
    }
    if previous_section == SETTINGS_SECTION_VIM {
        vim_key_capture_clear(ctx);
    }
    *highlighted_target = None;
    *pending_scroll_target = None;
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
    discard_confirmed: &mut bool,
    has_pending_inputs: bool,
) {
    if !search_query.trim().is_empty() {
        search_query.clear();
        return;
    }
    if has_pending_inputs && !*discard_confirmed {
        *discard_confirmed = true;
        return;
    }
    *discard_confirmed = false;
    actions.close = true;
}

fn apply_settings_panel_outside_click(
    actions: &mut PendingSettingsPanelActions,
    discard_confirmed: &mut bool,
    has_pending_inputs: bool,
) {
    if has_pending_inputs && !*discard_confirmed {
        *discard_confirmed = true;
        return;
    }
    *discard_confirmed = false;
    actions.close = true;
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

fn settings_panel_confirm_discard_armed(ctx: &Context) -> bool {
    ctx.data_mut(|data| {
        data.get_temp::<bool>(egui::Id::new(SETTINGS_CONFIRM_DISCARD_ID))
            .unwrap_or(false)
    })
}

fn set_settings_panel_confirm_discard_armed(ctx: &Context, armed: bool) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(SETTINGS_CONFIRM_DISCARD_ID), armed));
}

fn render_settings_search(ui: &mut egui::Ui, query: &mut String, enabled: bool) {
    let sanitized = bounded_settings_singleline_input(query);
    if sanitized != *query {
        *query = sanitized;
    }

    let visuals = ui.visuals();
    let fill = visuals.widgets.inactive.weak_bg_fill;
    let stroke = visuals.widgets.inactive.bg_stroke;
    egui::Frame::new()
        .fill(fill)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 3))
        .show(ui, |ui| {
            ui.set_min_height(34.0);
            ui.horizontal(|ui| {
                icon_label(
                    ui,
                    IconKind::Search,
                    ui.visuals().weak_text_color(),
                    "Search settings",
                );

                let clear_button_side = ui.spacing().interact_size.y.max(34.0);
                let clear_button_width = clear_button_side + ui.spacing().item_spacing.x;
                let search_width = (ui.available_width() - clear_button_width).max(0.0);
                let search_response = ui
                    .add_enabled_ui(enabled, |ui| {
                        ui.add_sized(
                            [search_width, 34.0],
                            egui::TextEdit::singleline(query)
                                .hint_text("Search settings")
                                .clip_text(true)
                                .frame(false)
                                .margin(egui::Margin::symmetric(4, 6)),
                        )
                    })
                    .inner;

                if query.is_empty() {
                    ui.allocate_exact_size(
                        egui::vec2(clear_button_side, clear_button_side),
                        egui::Sense::hover(),
                    );
                } else if enabled && icon_button(ui, IconKind::Close, "Clear search").clicked() {
                    query.clear();
                    search_response.request_focus();
                }
            });
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

    let row_height = settings_search_result_row_height(ui);
    let mut selected = settings_search_selected_index(ui.ctx(), query, results.len());
    let selection_changed = ui.input(|input| {
        handle_list_navigation_keys(
            input,
            &mut selected,
            results.len(),
            selection_page_step(row_height, max_height),
        )
    });
    let enter_pressed = ui.input(|input| input.key_pressed(Key::Enter));

    let mut clicked_result = None;
    let mut scroll_area = egui::ScrollArea::both()
        .id_salt("settings_panel_search_results_scroll")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
        .max_height(max_height)
        .auto_shrink([false, false]);
    if selection_changed {
        scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
            selected,
            results.len(),
            row_height,
            max_height,
        ));
    }
    scroll_area.show(ui, |ui| {
        ui.set_width(ui.available_width());
        if results.is_empty() {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!("No settings found for \"{query}\""))
                    .color(ui.visuals().weak_text_color()),
            );
            return;
        }

        let selected_fill = ui.visuals().selection.bg_fill;
        for (index, entry) in results.iter().enumerate() {
            let label = format!("{} > {}", SETTINGS_SECTIONS[entry.section], entry.title);
            let detail = format!("{} section", entry.group);
            let mut button = egui::Button::new(egui::RichText::new(label).strong())
                .fill(egui::Color32::TRANSPARENT);
            if index == selected {
                button = button.fill(selected_fill);
            }
            let response = ui.add_sized(
                [ui.available_width(), SETTINGS_SEARCH_RESULT_BUTTON_HEIGHT],
                button,
            );
            response
                .clone()
                .on_hover_text(format!("Open {} in Settings", entry.title));
            if response.clicked() {
                clicked_result = Some(*entry);
            }
            ui.add_sized(
                [ui.available_width(), SETTINGS_SEARCH_RESULT_DETAIL_HEIGHT],
                egui::Label::new(egui::RichText::new(detail).color(ui.visuals().weak_text_color()))
                    .truncate(),
            );
            ui.add_space(SETTINGS_SEARCH_RESULT_ROW_GAP);
        }
    });

    set_settings_search_selected_index(ui.ctx(), query, selected);
    clicked_result.or_else(|| results.get(selected).copied().filter(|_| enter_pressed))
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
    let haystacks = settings_search_haystacks();

    let mut matches: Vec<SettingsSearchMatch> = Vec::new();
    for (index, haystack) in haystacks.iter().enumerate() {
        let mut score = 0;
        let mut tokens_matched = 0;
        for token in &tokens {
            let Some(token_score) = settings_search_token_score(haystack, token) else {
                break;
            };
            score += token_score;
            tokens_matched += 1;
        }
        if tokens_matched == tokens.len() {
            matches.push(SettingsSearchMatch {
                entry: SETTINGS_SEARCH_ENTRIES[index],
                score,
                tokens_matched,
                index,
            });
        }
    }
    if !matches.is_empty() {
        matches.sort_unstable_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(b.tokens_matched.cmp(&a.tokens_matched))
                .then(a.index.cmp(&b.index))
        });
        return matches.into_iter().map(|hit| hit.entry).collect();
    }

    settings_search_fuzzy_results(&tokens.join(" "))
}

fn settings_search_fuzzy_results(query: &str) -> Vec<SettingsSearchEntry> {
    let matcher = SkimMatcherV2::default();
    let mut matches: Vec<(usize, i64)> = settings_search_haystacks()
        .iter()
        .enumerate()
        .filter_map(|(index, haystack)| {
            matcher
                .fuzzy_match(&haystack.haystack, query)
                .map(|score| (index, score))
        })
        .collect();
    matches.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    matches
        .into_iter()
        .map(|(index, _)| SETTINGS_SEARCH_ENTRIES[index])
        .collect()
}

struct SettingsSearchHaystack {
    haystack: String,

    title: String,
}

fn settings_search_haystacks() -> &'static Vec<SettingsSearchHaystack> {
    static HAYSTACKS: std::sync::OnceLock<Vec<SettingsSearchHaystack>> = std::sync::OnceLock::new();
    HAYSTACKS.get_or_init(|| {
        SETTINGS_SEARCH_ENTRIES
            .iter()
            .map(|entry| {
                let haystack = format!(
                    "{} {} {} {}",
                    SETTINGS_SECTIONS[entry.section], entry.group, entry.title, entry.keywords
                )
                .to_lowercase();
                let title = entry.title.to_lowercase();
                SettingsSearchHaystack { haystack, title }
            })
            .collect()
    })
}

const SETTINGS_SEARCH_SCORE_EXACT_TITLE: u32 = 4;
const SETTINGS_SEARCH_SCORE_TITLE_PREFIX: u32 = 3;
const SETTINGS_SEARCH_SCORE_TITLE_CONTAINS: u32 = 2;
const SETTINGS_SEARCH_SCORE_KEYWORD: u32 = 1;

fn settings_search_token_score(haystack: &SettingsSearchHaystack, token: &str) -> Option<u32> {
    if haystack.title == token {
        Some(SETTINGS_SEARCH_SCORE_EXACT_TITLE)
    } else if haystack.title.starts_with(token) {
        Some(SETTINGS_SEARCH_SCORE_TITLE_PREFIX)
    } else if haystack.title.contains(token) {
        Some(SETTINGS_SEARCH_SCORE_TITLE_CONTAINS)
    } else if haystack.haystack.contains(token) {
        Some(SETTINGS_SEARCH_SCORE_KEYWORD)
    } else {
        None
    }
}

struct SettingsSearchMatch {
    entry: SettingsSearchEntry,

    score: u32,

    tokens_matched: usize,

    index: usize,
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

fn settings_search_selected_index(ctx: &Context, query: &str, results_len: usize) -> usize {
    let stored = ctx.data_mut(|data| {
        data.get_temp::<(String, usize)>(egui::Id::new(SETTINGS_SEARCH_SELECTION_ID))
    });
    settings_search_selection_for_query(stored, query, results_len)
}

fn settings_search_selection_for_query(
    stored: Option<(String, usize)>,
    query: &str,
    results_len: usize,
) -> usize {
    stored
        .filter(|(stored_query, _)| stored_query == query)
        .map(|(_, selection)| selection)
        .unwrap_or_default()
        .min(results_len.saturating_sub(1))
}

fn set_settings_search_selected_index(ctx: &Context, query: &str, selection: usize) {
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new(SETTINGS_SEARCH_SELECTION_ID),
            (query.to_owned(), selection),
        )
    });
}

fn settings_search_result_row_height(ui: &egui::Ui) -> f32 {
    let spacing = ui.spacing().item_spacing.y;
    SETTINGS_SEARCH_RESULT_BUTTON_HEIGHT
        + SETTINGS_SEARCH_RESULT_DETAIL_HEIGHT
        + SETTINGS_SEARCH_RESULT_ROW_GAP
        + spacing * 2.0
}

#[cfg(test)]
mod tests {
    use super::actions::PendingSettingsPanelActions;
    use super::{
        SETTINGS_SEARCH_SCORE_EXACT_TITLE, SETTINGS_SEARCH_SCORE_KEYWORD,
        SETTINGS_SEARCH_SCORE_TITLE_CONTAINS, SETTINGS_SEARCH_SCORE_TITLE_PREFIX,
        SETTINGS_SECTION_APPEARANCE, SETTINGS_SECTION_DEVELOPER, SETTINGS_SECTION_EDITOR,
        SETTINGS_SECTION_FILES, SETTINGS_SECTION_GENERAL, SETTINGS_SECTION_LSP,
        SETTINGS_SECTION_PLUGINS, SETTINGS_SECTION_SOURCE_CONTROL, SETTINGS_SECTION_TERMINAL,
        SETTINGS_SECTION_VIM, SETTINGS_TARGET_APPEARANCE, SETTINGS_TARGET_DEVELOPER,
        SETTINGS_TARGET_FILES_SAVE_ACTIONS, SETTINGS_TARGET_GENERAL, SETTINGS_TARGET_LSP,
        SETTINGS_TARGET_SCROLLBARS, SETTINGS_TARGET_SOURCE_CONTROL,
        SETTINGS_TARGET_TERMINAL_INTERACTION, SETTINGS_TARGET_VIM_KEYBINDINGS,
        apply_settings_panel_escape, apply_settings_panel_outside_click,
        record_settings_panel_focus, reset_settings_panel_memory, restore_settings_panel_focus,
        set_settings_panel_search_query, settings_content_height,
        settings_panel_close_button_hover_text, settings_panel_close_button_label,
        settings_panel_confirm_discard_armed, settings_panel_escape_should_apply,
        settings_panel_footer_actions_enabled, settings_panel_navigation_enabled,
        settings_panel_search_query, settings_panel_uses_sidebar, settings_search_count_label,
        settings_search_enabled, settings_search_entry_target, settings_search_haystacks,
        settings_search_results, settings_search_selection_for_query, settings_search_token_score,
        settings_search_tokens, settings_sidebar_width, settings_window_size_for_available,
    };

    #[test]
    fn settings_window_size_uses_large_preferred_size_and_shrinks_to_viewport() {
        assert_eq!(
            settings_window_size_for_available(1600.0, 900.0),
            [920.0, 640.0]
        );
        assert_eq!(
            settings_window_size_for_available(800.0, 600.0),
            [752.0, 480.0]
        );
        assert_eq!(
            settings_window_size_for_available(360.0, 260.0),
            [312.0, 140.0]
        );
        assert_eq!(
            settings_window_size_for_available(f32::NAN, f32::INFINITY),
            [920.0, 640.0]
        );
    }

    #[test]
    fn settings_navigation_switches_layout_and_bounds_sidebar_width() {
        assert!(!settings_panel_uses_sidebar(719.0));
        assert!(settings_panel_uses_sidebar(720.0));
        assert!(!settings_panel_uses_sidebar(f32::NAN));
        assert_eq!(settings_sidebar_width(720.0), 180.0);
        assert!((settings_sidebar_width(1100.0) - 231.0).abs() < f32::EPSILON);
        assert_eq!(settings_sidebar_width(100.0), 100.0);
    }

    #[test]
    fn settings_content_height_never_overflows_short_viewports() {
        assert_eq!(settings_content_height(500.0), 456.0);
        assert_eq!(settings_content_height(30.0), 0.0);
        assert_eq!(settings_content_height(f32::NAN), 0.0);
    }

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
    fn settings_search_finds_editor_background_controls() {
        for title in [
            "Editor background image",
            "Editor background area",
            "Editor background scaling",
            "Editor background position",
        ] {
            let results = settings_search_results(title);
            let entry = results
                .iter()
                .find(|entry| entry.section == SETTINGS_SECTION_APPEARANCE && entry.title == title)
                .copied()
                .expect("editor background search result");
            assert_eq!(
                settings_search_entry_target(entry),
                SETTINGS_TARGET_APPEARANCE
            );
        }
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
                "app version",
                "Application info",
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
    fn settings_search_scores_exact_title_above_prefix_contains_and_keywords() {
        let exact = settings_search_token_score(haystack_with_title("Minimap"), "minimap");
        let prefix = settings_search_token_score(haystack_with_title("Scrollbars"), "scroll");
        let contains =
            settings_search_token_score(haystack_with_title("Smooth scrolling"), "scroll");
        let keyword = settings_search_token_score(haystack_with_title("Minimap"), "overview");
        let miss = settings_search_token_score(haystack_with_title("Minimap"), "terminal");

        assert_eq!(exact, Some(SETTINGS_SEARCH_SCORE_EXACT_TITLE));
        assert_eq!(prefix, Some(SETTINGS_SEARCH_SCORE_TITLE_PREFIX));
        assert_eq!(contains, Some(SETTINGS_SEARCH_SCORE_TITLE_CONTAINS));
        assert_eq!(keyword, Some(SETTINGS_SEARCH_SCORE_KEYWORD));
        assert_eq!(miss, None);
        assert!(exact.unwrap() > prefix.unwrap());
        assert!(prefix.unwrap() > contains.unwrap());
        assert!(contains.unwrap() > keyword.unwrap());
    }

    #[test]
    fn settings_search_orders_results_by_relevance_then_table_order() {
        let minimap = settings_search_results("minimap");
        assert_eq!(minimap[0].title, "Minimap");
        assert_eq!(minimap[1].title, "Editor minimap");

        let scroll = settings_search_results("scroll");
        assert_eq!(scroll[0].title, "Scroll beyond last line");
        assert_eq!(scroll[1].title, "Scrollbars");
        assert_eq!(scroll[2].title, "Scrollback rows");
        assert_eq!(scroll[3].title, "Smooth scrolling");
    }

    #[test]
    fn settings_search_fuzzy_fallback_finds_subsequence_matches() {
        let results = settings_search_results("srch");

        assert!(!results.is_empty());
        assert!(
            results.iter().any(|entry| entry.title == "Find"),
            "\"srch\" should fuzzy-match the Find entry"
        );
    }

    #[test]
    fn settings_search_haystacks_are_precomputed_lowercase() {
        let haystacks = settings_search_haystacks();

        assert_eq!(haystacks.len(), super::SETTINGS_SEARCH_ENTRIES.len());
        assert!(
            haystacks
                .iter()
                .all(|haystack| haystack.haystack.chars().all(|ch| !ch.is_uppercase()))
        );
    }

    #[test]
    fn settings_search_selection_resets_on_query_change_and_clamps() {
        assert_eq!(settings_search_selection_for_query(None, "minimap", 2), 0);
        assert_eq!(
            settings_search_selection_for_query(Some(("minimap".to_owned(), 1)), "minimap", 2),
            1
        );
        assert_eq!(
            settings_search_selection_for_query(Some(("minimap".to_owned(), 1)), "vim", 2),
            0
        );
        assert_eq!(
            settings_search_selection_for_query(Some(("minimap".to_owned(), 5)), "minimap", 2),
            1
        );
        assert_eq!(
            settings_search_selection_for_query(Some(("minimap".to_owned(), 1)), "minimap", 0),
            0
        );
    }

    #[test]
    fn settings_search_keyboard_navigation_opens_selected_result() {
        let root = settings_test_root("settings-search-keyboard");
        let mut app = settings_test_app(root);
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 850.0));
        app.settings_panel_open = true;

        let key = |key: egui::Key| egui::Event::Key {
            key,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
            physical_key: None,
        };
        let frame = |app: &mut crate::KuroyaApp, ctx: &egui::Context, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..egui::RawInput::default()
            };
            let _ = ctx.run(input, |ctx| app.render_settings_panel(ctx));
        };

        frame(&mut app, &ctx, Vec::new());

        set_settings_panel_search_query(&ctx, "minimap".to_owned());
        frame(&mut app, &ctx, Vec::new());
        assert_eq!(app.settings_panel_section, SETTINGS_SECTION_GENERAL);

        frame(&mut app, &ctx, vec![key(egui::Key::ArrowDown)]);
        frame(&mut app, &ctx, vec![key(egui::Key::Enter)]);

        assert_eq!(app.settings_panel_section, SETTINGS_SECTION_EDITOR);
        assert!(settings_panel_search_query(&ctx).is_empty());
    }

    #[test]
    fn settings_outside_click_needs_a_second_click_to_discard_unsaved_edits() {
        let root = settings_test_root("settings-discard-click");
        let mut app = settings_test_app(root);
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 850.0));
        app.settings_panel_open = true;

        settings_frame(&mut app, &ctx, screen, Vec::new());
        app.settings_panel_draft.font_size = 22.0;

        settings_frame(&mut app, &ctx, screen, settings_outside_click_events());

        assert!(app.settings_panel_open);
        assert_eq!(app.settings_panel_draft.font_size, 22.0);
        assert!(settings_panel_confirm_discard_armed(&ctx));

        settings_frame(&mut app, &ctx, screen, Vec::new());
        assert!(app.settings_panel_open);
        assert!(settings_panel_confirm_discard_armed(&ctx));

        settings_frame(&mut app, &ctx, screen, settings_outside_click_events());

        assert!(!app.settings_panel_open);
        assert_eq!(
            app.settings_panel_draft.font_size,
            kuroya_core::EditorSettings::default().font_size
        );
        assert_eq!(app.status, "Closed settings");
        assert!(!settings_panel_confirm_discard_armed(&ctx));
    }

    #[test]
    fn settings_outside_click_closes_immediately_without_unsaved_edits() {
        let root = settings_test_root("settings-outside-no-pending");
        let mut app = settings_test_app(root);
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 850.0));
        app.settings_panel_open = true;

        settings_frame(&mut app, &ctx, screen, Vec::new());
        settings_frame(&mut app, &ctx, screen, settings_outside_click_events());

        assert!(!app.settings_panel_open);
        assert_eq!(app.status, "Closed settings");
        assert!(!settings_panel_confirm_discard_armed(&ctx));
    }

    #[test]
    fn settings_escape_needs_a_second_press_to_discard_unsaved_edits() {
        let root = settings_test_root("settings-discard-escape");
        let mut app = settings_test_app(root);
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 850.0));
        app.settings_panel_open = true;

        settings_frame(&mut app, &ctx, screen, Vec::new());
        app.settings_panel_draft.font_size = 22.0;

        settings_frame(&mut app, &ctx, screen, settings_escape_events());

        assert!(app.settings_panel_open);
        assert_eq!(app.settings_panel_draft.font_size, 22.0);
        assert!(settings_panel_confirm_discard_armed(&ctx));

        settings_frame(&mut app, &ctx, screen, settings_escape_events());

        assert!(!app.settings_panel_open);
        assert_eq!(
            app.settings_panel_draft.font_size,
            kuroya_core::EditorSettings::default().font_size
        );
        assert_eq!(app.status, "Closed settings");
        assert!(!settings_panel_confirm_discard_armed(&ctx));
    }

    #[test]
    fn settings_apply_wins_over_the_armed_discard_confirmation() {
        let root = settings_test_root("settings-apply-beats-discard");
        let mut app = settings_test_app(root);
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 850.0));
        app.settings_panel_open = true;

        settings_frame(&mut app, &ctx, screen, Vec::new());
        app.settings_panel_draft.font_size = 22.0;
        settings_frame(&mut app, &ctx, screen, settings_outside_click_events());

        assert!(app.settings_panel_open);
        assert!(settings_panel_confirm_discard_armed(&ctx));

        app.apply_settings_panel_actions(PendingSettingsPanelActions {
            apply: true,
            ..PendingSettingsPanelActions::default()
        });

        assert_eq!(app.settings.font_size, 22.0);
        assert!(app.settings_panel_open);
        assert!(!app.settings_panel_has_pending_inputs());

        settings_frame(&mut app, &ctx, screen, Vec::new());
        assert!(!settings_panel_confirm_discard_armed(&ctx));

        settings_frame(&mut app, &ctx, screen, settings_outside_click_events());

        assert!(!app.settings_panel_open);
    }

    #[test]
    fn previewing_a_theme_in_the_settings_panel_reverts_when_the_panel_closes() {
        let root = settings_test_root("settings-theme-preview-revert");
        let mut app = settings_test_app(root.clone());
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 850.0));
        app.settings_panel_open = true;

        settings_frame(&mut app, &ctx, screen, Vec::new());
        let draft_theme = kuroya_core::ThemeSettings::built_in_presets()
            .into_iter()
            .find(|theme| theme.name == "Graphite")
            .expect("Graphite preset should exist");
        app.settings_panel_draft.theme = draft_theme.clone();
        settings_frame(&mut app, &ctx, screen, Vec::new());
        app.sync_theme_preview(&ctx);

        assert_eq!(app.theme_preview.as_ref(), Some(&draft_theme));
        assert_eq!(
            ctx.style().visuals.extreme_bg_color,
            crate::theme::theme_palette(&draft_theme).background
        );
        assert_eq!(app.settings.theme, kuroya_core::ThemeSettings::default());

        settings_frame(&mut app, &ctx, screen, settings_outside_click_events());
        settings_frame(&mut app, &ctx, screen, settings_outside_click_events());
        assert!(!app.settings_panel_open);

        app.sync_theme_preview(&ctx);

        assert!(app.theme_preview.is_none());
        assert_eq!(
            ctx.style().visuals.extreme_bg_color,
            crate::theme::theme_palette(&app.settings.theme).background
        );
        assert_eq!(app.settings_panel_draft.theme, app.settings.theme);
        assert!(!crate::workspace_state::settings_path(&root).exists());
    }

    fn settings_frame(
        app: &mut crate::KuroyaApp,
        ctx: &egui::Context,
        screen: egui::Rect,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..egui::RawInput::default()
        };
        let _ = ctx.run(input, |ctx| app.render_settings_panel(ctx));
    }

    fn settings_outside_click_events() -> Vec<egui::Event> {
        let button = |pressed| egui::Event::PointerButton {
            pos: egui::Pos2::new(40.0, 20.0),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        vec![button(true), button(false)]
    }

    fn settings_escape_events() -> Vec<egui::Event> {
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
            physical_key: None,
        }]
    }

    fn haystack_with_title(title: &str) -> &'static super::SettingsSearchHaystack {
        settings_search_haystacks()
            .iter()
            .find(|haystack| haystack.title == title.to_lowercase())
            .unwrap_or_else(|| panic!("no search haystack for {title:?}"))
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
        let mut discard_confirmed = false;

        apply_settings_panel_escape(&mut query, &mut actions, &mut discard_confirmed, true);

        assert!(query.is_empty());
        assert!(!actions.close);
        assert!(!discard_confirmed);

        apply_settings_panel_escape(&mut query, &mut actions, &mut discard_confirmed, true);

        assert!(!actions.close);
        assert!(discard_confirmed);

        apply_settings_panel_escape(&mut query, &mut actions, &mut discard_confirmed, true);

        assert!(actions.close);
        assert!(!discard_confirmed);

        let mut actions = PendingSettingsPanelActions::default();
        let mut discard_confirmed = false;

        apply_settings_panel_escape(&mut query, &mut actions, &mut discard_confirmed, false);

        assert!(actions.close);
        assert!(!discard_confirmed);
    }

    #[test]
    fn settings_outside_click_only_closes_once_the_draft_is_clean() {
        let mut actions = PendingSettingsPanelActions::default();
        let mut discard_confirmed = false;

        apply_settings_panel_outside_click(&mut actions, &mut discard_confirmed, true);

        assert!(!actions.close);
        assert!(discard_confirmed);

        apply_settings_panel_outside_click(&mut actions, &mut discard_confirmed, true);

        assert!(actions.close);
        assert!(!discard_confirmed);

        let mut actions = PendingSettingsPanelActions::default();
        let mut discard_confirmed = false;

        apply_settings_panel_outside_click(&mut actions, &mut discard_confirmed, false);

        assert!(actions.close);
        assert!(!discard_confirmed);
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

    #[test]
    fn settings_panel_memory_reset_preserves_style() {
        let ctx = egui::Context::default();
        let panel_fill = egui::Color32::from_rgb(12, 34, 56);
        ctx.set_visuals(egui::Visuals {
            panel_fill,
            ..egui::Visuals::default()
        });

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            reset_settings_panel_memory(ctx)
        });

        assert_eq!(ctx.style().visuals.panel_fill, panel_fill);
    }

    #[test]
    fn settings_panel_memory_reset_preserves_focused_widget() {
        let ctx = egui::Context::default();
        let terminal_input_id = egui::Id::new("terminal_input");

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.memory_mut(|memory| memory.request_focus(terminal_input_id));
            reset_settings_panel_memory(ctx);

            assert!(ctx.memory(|memory| memory.has_focus(terminal_input_id)));
        });
    }

    #[test]
    fn settings_panel_focus_restore_returns_focus_to_recorded_widget() {
        let ctx = egui::Context::default();
        let terminal_input_id = egui::Id::new("terminal_input");

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.memory_mut(|memory| memory.request_focus(terminal_input_id));
            record_settings_panel_focus(ctx);
            restore_settings_panel_focus(ctx);

            assert!(ctx.memory(|memory| memory.has_focus(terminal_input_id)));
        });
    }

    #[test]
    fn settings_panel_focus_restore_without_a_recorded_widget_is_a_no_op() {
        let ctx = egui::Context::default();
        let terminal_input_id = egui::Id::new("terminal_input");

        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            restore_settings_panel_focus(ctx);

            assert!(!ctx.memory(|memory| memory.has_focus(terminal_input_id)));
        });
    }

    fn search_result(query: &str, title: &str) -> super::SettingsSearchEntry {
        settings_search_results(query)
            .into_iter()
            .find(|entry| entry.title == title)
            .unwrap_or_else(|| panic!("{query:?} should find {title:?}"))
    }

    #[test]
    fn settings_window_rect_is_independent_of_the_selected_section() {
        let root = settings_test_root("settings-window-rect");
        let mut app = settings_test_app(root);
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 850.0));
        app.settings_panel_open = true;

        let render_section = |app: &mut crate::KuroyaApp, section: usize| -> [f32; 2] {
            app.settings_panel_section = section;
            let mut rect = egui::Rect::NOTHING;
            for _ in 0..4 {
                let input = egui::RawInput {
                    screen_rect: Some(screen),
                    ..egui::RawInput::default()
                };
                let _ = ctx.run(input, |ctx| {
                    app.render_settings_panel(ctx);
                });
                rect = largest_settings_window_rect(&ctx);
            }
            [rect.width(), rect.height()]
        };

        let baseline = render_section(&mut app, SETTINGS_SECTION_LSP);
        println!("settings rect LSP:        {baseline:?}");
        for (name, section) in [
            ("General", SETTINGS_SECTION_GENERAL),
            ("Editor", SETTINGS_SECTION_EDITOR),
            ("Vim", SETTINGS_SECTION_VIM),
            ("Terminal", SETTINGS_SECTION_TERMINAL),
            ("Files", SETTINGS_SECTION_FILES),
            ("Appearance", SETTINGS_SECTION_APPEARANCE),
            ("Source Control", SETTINGS_SECTION_SOURCE_CONTROL),
            ("Developer", SETTINGS_SECTION_DEVELOPER),
            ("Plugins", SETTINGS_SECTION_PLUGINS),
        ] {
            let size = render_section(&mut app, section);
            println!("settings rect {name:<12}: {size:?}");
            assert_eq!(size, baseline, "{name} section resizes the settings window");
        }
    }

    fn largest_settings_window_rect(ctx: &egui::Context) -> egui::Rect {
        ctx.memory(|memory| {
            memory
                .area_rect(egui::Id::new("Settings"))
                .unwrap_or(egui::Rect::NOTHING)
        })
    }

    fn settings_test_root(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("kuroya-{name}-{}-{nanos}", std::process::id()))
    }

    fn settings_test_app(root: std::path::PathBuf) -> crate::KuroyaApp {
        use crate::{app_startup_context::AppStartupContext, terminal::TerminalPane};
        use kuroya_core::{EditorSettings, Workspace};
        use tokio::runtime::Runtime;

        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let settings = EditorSettings::default();
        crate::KuroyaApp::from_startup_context(AppStartupContext {
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
            now: std::time::Instant::now(),
            startup_timings: Vec::new(),
        })
    }
}

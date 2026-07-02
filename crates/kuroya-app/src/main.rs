#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
#![allow(clippy::collapsible_if, clippy::too_many_arguments)]

mod app_exit_guard_dialogs;
mod app_frame_scheduler;
mod app_session;
mod app_session_restore;
mod app_startup;
mod app_startup_context;
mod app_startup_state;
mod app_state;
mod app_tabs;
mod app_update;
mod app_update_overlays;
mod buffer_close_guard_dialog;
mod buffer_close_lifecycle;
mod buffer_find;
mod buffer_find_history;
mod buffer_find_panel;
mod buffer_lifecycle;
mod buffer_reload_guard_dialog;
mod command_aliases;
mod command_bus_runtime;
mod command_catalog;
mod command_editor_runtime;
mod command_palette_items;
mod command_palette_overlay;
mod command_runtime;
mod command_ui_runtime;
mod commands;
mod completion_preview;
mod dashboard;
mod devtools;
mod devtools_async_tasks;
mod devtools_lsp_trace;
mod devtools_memory;
mod devtools_profile;
mod devtools_repaint_diagnostics;
mod devtools_startup;
mod devtools_trace_id;
mod diagnostic_location;
mod diagnostic_navigation;
mod diagnostics_panel;
mod diagnostics_runtime;
mod editor_bracket_overlay_cache;
mod editor_buffer_context_actions;
mod editor_clipboard_context_actions;
mod editor_comment_runtime;
mod editor_context_menu;
mod editor_diff_lines;
mod editor_drag_drop_runtime;
mod editor_focus_runtime;
mod editor_indent;
mod editor_input;
mod editor_input_events;
mod editor_interactions;
mod editor_key_events;
mod editor_lsp_context_actions;
mod editor_pane;
mod editor_pane_actions;
mod editor_pane_chrome;
mod editor_pane_data;
mod editor_pane_rows;
mod editor_pane_scroll;
mod editor_pane_support;
mod editor_readonly;
mod editor_row_gutter;
mod editor_row_overlays;
mod editor_row_paint;
mod editor_selection_clipboard_runtime;
mod editor_suggest;
mod editor_tabs;
mod editor_text_geometry;
mod editor_view;
mod editor_vim_key_events;
mod explorer;
mod explorer_buffer_runtime;
mod explorer_delete_dialog;
mod explorer_dialogs;
mod explorer_file_actions;
mod explorer_fs_runtime;
mod explorer_panel;
mod explorer_rows;
mod explorer_runtime;
mod explorer_tree_panel;
mod file_atomic_write;
mod file_compare_runtime;
mod file_decode;
mod file_dialogs;
mod file_drop_runtime;
mod file_history;
mod file_io;
mod file_reload_runtime;
mod file_runtime;
mod file_save_dispatch;
mod file_save_runtime;
mod folding;
mod font_candidates;
mod font_loading;
mod font_typography;
mod fonts;
mod fs_watcher;
mod git_diff_state;
mod git_diff_view;
mod goto_line_overlay;
mod gpu_acceleration_prompt;
mod history;
mod image_preview;
mod keybinding_chords;
mod keybinding_input;
mod keybinding_parse;
mod keybindings;
mod keybindings_panel;
mod keybindings_panel_actions;
mod keybindings_runtime;
mod large_file_mode;
mod layout;
mod local_history_runtime;
mod lsp_actions;
mod lsp_call_hierarchy_popup;
mod lsp_client;
mod lsp_code_action_popup;
mod lsp_code_actions;
mod lsp_completion_imports;
mod lsp_completion_popup;
mod lsp_completion_ranking;
mod lsp_completion_resolve;
mod lsp_diagnostics_batch;
mod lsp_disk_edit_actions;
mod lsp_edit_events;
mod lsp_edit_requests;
mod lsp_edits;
mod lsp_event_handler;
mod lsp_folding_runtime;
mod lsp_hover_cache;
mod lsp_hover_markdown;
mod lsp_hover_runtime;
mod lsp_info_popups;
mod lsp_labels;
mod lsp_lifecycle;
mod lsp_markdown_render;
mod lsp_navigation_events;
mod lsp_navigation_requests;
mod lsp_progress;
mod lsp_reference_events;
mod lsp_reference_popup;
mod lsp_rename_popup;
mod lsp_rename_preview;
mod lsp_rename_requests;
mod lsp_requests;
mod lsp_runtime;
mod lsp_symbol_events;
mod lsp_symbol_popups;
mod lsp_symbol_requests;
mod lsp_text_positions;
mod lsp_type_hierarchy_popup;
mod lsp_ui_events;
mod lsp_workspace_symbol_ranking;
mod merge_conflict_cache;
mod minimap;
mod native_paths;
mod navigation_history_runtime;
mod navigation_runtime;
mod navigation_targets;
mod pane_activation;
mod pane_lifecycle;
mod panel_layout;
mod path_clipboard;
mod path_display;
mod persistence;
mod persistence_models;
mod persistence_session;
mod persistence_storage;
mod persistence_workspace_snapshots;
mod plugin_activation_runtime;
mod plugin_command_runtime;
mod popup_buttons;
mod preference_panels;
mod preferences;
mod project_index_cache;
mod project_search;
mod project_search_panel;
mod project_search_state;
mod quick_open;
mod quick_open_overlay;
mod recovery;
mod runtime_ticks;
mod save_as_dialog;
mod save_conflict_dialog;
mod save_guard_reasons;
mod save_guards;
mod save_lifecycle;
mod session_state;
mod settings_form;
mod snippet_session;
mod source_control_blame_runtime;
mod source_control_branch_picker;
mod source_control_branch_runtime;
mod source_control_conflicts;
mod source_control_diff_runtime;
mod source_control_discard_dialog;
mod source_control_git_panel_ui;
mod source_control_history_panel;
mod source_control_history_runtime;
mod source_control_hunk_panel;
mod source_control_hunk_runtime;
mod source_control_panel;
mod source_control_patch_runtime;
mod source_control_runtime;
mod source_control_smart_commit_dialog;
mod source_control_stash_panel;
mod source_control_stash_runtime;
mod startup_tasks;
mod status_bar;
mod symbols_panel;
mod syntax;
mod syntax_cache;
mod syntax_layout;
mod syntax_tree_cache;
mod terminal;
mod terminal_process;
mod terminal_support;
mod theme;
mod theme_picker_panel;
mod transient_state;
mod ui_event_channel;
mod ui_event_handler;
mod ui_events;
mod ui_file_events;
mod ui_file_load_events;
mod ui_file_loaded_events;
mod ui_file_reload_events;
mod ui_file_save_events;
mod ui_icon_primitives;
mod ui_icon_shapes;
mod ui_icons;
mod ui_scrollbars;
mod ui_state;
mod ui_text;
mod update_checker;
mod virtual_diff_runtime;
mod virtual_revision_runtime;
mod workspace_event_guards;
mod workspace_guard_dialogs;
mod workspace_guard_runtime;
mod workspace_lifecycle;
mod workspace_reset_state;
mod workspace_snapshot_runtime;
mod workspace_state;
mod workspace_tasks_panel;
mod workspace_tasks_runtime;
mod workspace_trust;

use eframe::egui::ViewportBuilder;

pub(crate) use app_state::KuroyaApp;

fn main() -> eframe::Result {
    let mut viewport = ViewportBuilder::default()
        .with_title("Kuroya")
        .with_app_id("kuroya")
        .with_inner_size([1320.0, 860.0])
        .with_min_inner_size([920.0, 560.0]);
    if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/logos/kuroya.png"))
    {
        viewport = viewport.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Kuroya",
        native_options,
        Box::new(|cc| match KuroyaApp::new(cc) {
            Ok(app) => Ok(Box::new(app) as Box<dyn eframe::App>),
            Err(error) => Err(Box::new(std::io::Error::other(error.to_string()))
                as Box<dyn std::error::Error + Send + Sync>),
        }),
    )
}

#[cfg(test)]
mod tests;

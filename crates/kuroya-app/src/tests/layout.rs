use crate::{
    layout::{
        DIAGNOSTICS_PANEL_DEFAULT_WIDTH, DIAGNOSTICS_PANEL_MAX_WIDTH, DIAGNOSTICS_PANEL_MIN_WIDTH,
        EXPLORER_DEFAULT_WIDTH, EXPLORER_MAX_WIDTH, EXPLORER_MIN_WIDTH, MIN_EDITOR_PANE_WIDTH,
        SOURCE_CONTROL_DEFAULT_WIDTH, SOURCE_CONTROL_MAX_WIDTH, SOURCE_CONTROL_MIN_WIDTH,
        SYMBOLS_PANEL_DEFAULT_WIDTH, SYMBOLS_PANEL_MAX_WIDTH, SYMBOLS_PANEL_MIN_WIDTH,
        TERMINAL_DEFAULT_HEIGHT, TERMINAL_MAX_HEIGHT, TERMINAL_MIN_HEIGHT, adjust_split_weights,
        clamp_diagnostics_panel_width, clamp_explorer_width, clamp_source_control_width,
        clamp_symbols_panel_width, clamp_terminal_height,
        clamp_terminal_height_for_available_height, normalize_weights,
        responsive_side_panel_max_width, responsive_terminal_max_height, terminal_open_height,
    },
    minimap::{
        minimap_line_from_y, minimap_sample_line, minimap_target_line_from_y, minimap_viewport_rect,
    },
    panel_layout::{PanelDockSide, PanelPlacement, cycle_panel_placement},
    ui_state::{
        clamp_scroll_target, clamp_selection, handle_list_navigation_keys, move_selection,
        move_selection_by_page, move_selection_to_end, move_selection_to_start,
        next_smooth_scroll_offset, selected_row_scroll_offset, selection_page_step,
        smooth_scroll_finished, wrapped_index,
    },
};

use eframe::egui::{self, Event, Key, Modifiers, RawInput, pos2, vec2};

#[test]
fn modal_selection_wraps_and_clamps() {
    let mut selected = 0;
    move_selection(&mut selected, 3, 1);
    assert_eq!(selected, 1);
    move_selection(&mut selected, 3, -2);
    assert_eq!(selected, 2);
    clamp_selection(&mut selected, 2);
    assert_eq!(selected, 1);
    clamp_selection(&mut selected, 0);
    assert_eq!(selected, 0);
    move_selection(&mut selected, 0, 1);
    assert_eq!(selected, 0);
}

#[test]
fn modal_selection_pages_and_edges_clamp_without_wrapping() {
    let mut selected = 2;
    move_selection_by_page(&mut selected, 10, 4, 1);
    assert_eq!(selected, 6);
    move_selection_by_page(&mut selected, 10, 4, 1);
    assert_eq!(selected, 9);
    move_selection_by_page(&mut selected, 10, 4, -1);
    assert_eq!(selected, 5);
    move_selection_by_page(&mut selected, 10, 99, -1);
    assert_eq!(selected, 0);

    move_selection_to_end(&mut selected, 10);
    assert_eq!(selected, 9);
    move_selection_to_start(&mut selected);
    assert_eq!(selected, 0);
    move_selection_to_end(&mut selected, 0);
    assert_eq!(selected, 0);
}

#[test]
fn list_navigation_accepts_plain_navigation_keys() {
    let (changed, selection) = run_list_navigation_key(Key::PageDown, Modifiers::NONE);

    assert!(changed);
    assert_eq!(selection, 4);
}

#[test]
fn list_navigation_ignores_modified_navigation_keys() {
    for modifiers in [
        Modifiers::CTRL,
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::COMMAND,
    ] {
        let (changed, selection) = run_list_navigation_key(Key::ArrowDown, modifiers);

        assert!(!changed);
        assert_eq!(selection, 2);
    }
}

fn run_list_navigation_key(key: Key, modifiers: Modifiers) -> (bool, usize) {
    let ctx = egui::Context::default();
    let mut selection = 2usize;
    let mut changed = false;
    let input = RawInput {
        modifiers,
        events: vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        ..RawInput::default()
    };

    let _ = ctx.run(input, |ctx| {
        changed = ctx.input(|input| handle_list_navigation_keys(input, &mut selection, 5, 2));
    });

    (changed, selection)
}

#[test]
fn selection_page_step_tracks_visible_rows_safely() {
    assert_eq!(selection_page_step(20.0, 100.0), 5);
    assert_eq!(selection_page_step(32.0, 100.0), 3);
    assert_eq!(selection_page_step(0.0, 100.0), 1);
    assert_eq!(selection_page_step(20.0, f32::NAN), 1);
}

#[test]
fn wrapped_index_handles_tab_cycling() {
    assert_eq!(wrapped_index(0, 3, 1), 1);
    assert_eq!(wrapped_index(2, 3, 1), 0);
    assert_eq!(wrapped_index(0, 3, -1), 2);
    assert_eq!(wrapped_index(1, 3, -4), 0);
    assert_eq!(wrapped_index(4, 0, 1), 0);
    assert_eq!(wrapped_index(usize::MAX - 1, usize::MAX, 2), 0);
    assert_eq!(wrapped_index(5, usize::MAX, -1), 4);
    assert_eq!(
        wrapped_index(0, usize::MAX, isize::MIN),
        isize::MAX as usize
    );
}

#[test]
fn smooth_scroll_offsets_approach_and_clamp_targets() {
    assert_eq!(clamp_scroll_target(900.0, 20, 20.0, 100.0), 300.0);
    assert_eq!(clamp_scroll_target(-10.0, 20, 20.0, 100.0), 0.0);

    let next = next_smooth_scroll_offset(0.0, 100.0, 20.0);
    assert!(next > 0.0 && next < 100.0);
    assert!(smooth_scroll_finished(98.0, 100.0, 20.0));
    assert_eq!(next_smooth_scroll_offset(98.0, 100.0, 20.0), 100.0);
}

#[test]
fn selected_row_scroll_offset_centers_and_clamps_virtual_rows() {
    assert_eq!(selected_row_scroll_offset(0, 100, 20.0, 100.0), 0.0);
    assert_eq!(selected_row_scroll_offset(10, 100, 20.0, 100.0), 160.0);
    assert_eq!(selected_row_scroll_offset(99, 100, 20.0, 100.0), 1900.0);
    assert_eq!(selected_row_scroll_offset(4, 5, 20.0, 200.0), 0.0);
    assert_eq!(selected_row_scroll_offset(0, 0, 20.0, 100.0), 0.0);
    assert_eq!(selected_row_scroll_offset(4, 5, f32::NAN, 200.0), 0.0);
}

#[test]
fn pane_weights_normalize_invalid_values() {
    let mut weights = vec![2.0, 1.0, 1.0];
    normalize_weights(&mut weights);
    assert_eq!(weights, vec![0.5, 0.25, 0.25]);

    let mut invalid = vec![0.0, f32::NAN, -1.0];
    normalize_weights(&mut invalid);
    assert_eq!(invalid, vec![1.0 / 3.0; 3]);

    let mut huge = vec![f32::MAX, f32::MAX];
    normalize_weights(&mut huge);
    assert_eq!(huge, vec![0.5, 0.5]);
}

#[test]
fn terminal_height_is_clamped_for_session_restore() {
    assert_eq!(clamp_terminal_height(1.0), TERMINAL_MIN_HEIGHT);
    assert_eq!(clamp_terminal_height(900.0), 900.0);
    assert_eq!(
        clamp_terminal_height(TERMINAL_MAX_HEIGHT + 1.0),
        TERMINAL_MAX_HEIGHT
    );
    assert_eq!(clamp_terminal_height(260.0), 260.0);
    assert_eq!(clamp_terminal_height(f32::NAN), TERMINAL_DEFAULT_HEIGHT);
}

#[test]
fn terminal_open_height_uses_half_of_available_height() {
    assert_eq!(terminal_open_height(720.0), 360.0);
    assert_eq!(terminal_open_height(220.0), TERMINAL_MIN_HEIGHT);
    assert_eq!(terminal_open_height(f32::NAN), TERMINAL_DEFAULT_HEIGHT);
}

#[test]
fn terminal_height_respects_remaining_editor_space() {
    assert_eq!(responsive_terminal_max_height(720.0), 540.0);
    assert_eq!(responsive_terminal_max_height(240.0), TERMINAL_MIN_HEIGHT);
    assert_eq!(
        responsive_terminal_max_height(10_000.0),
        TERMINAL_MAX_HEIGHT
    );
    assert_eq!(
        clamp_terminal_height_for_available_height(900.0, 720.0),
        540.0
    );
}

#[test]
fn panel_widths_are_clamped_for_session_restore() {
    assert_eq!(clamp_explorer_width(1.0), EXPLORER_MIN_WIDTH);
    assert_eq!(clamp_explorer_width(900.0), EXPLORER_MAX_WIDTH);
    assert_eq!(clamp_explorer_width(f32::NAN), EXPLORER_DEFAULT_WIDTH);

    assert_eq!(clamp_symbols_panel_width(1.0), SYMBOLS_PANEL_MIN_WIDTH);
    assert_eq!(clamp_symbols_panel_width(900.0), SYMBOLS_PANEL_MAX_WIDTH);
    assert_eq!(
        clamp_symbols_panel_width(f32::NAN),
        SYMBOLS_PANEL_DEFAULT_WIDTH
    );

    assert_eq!(
        clamp_diagnostics_panel_width(1.0),
        DIAGNOSTICS_PANEL_MIN_WIDTH
    );
    assert_eq!(
        clamp_diagnostics_panel_width(900.0),
        DIAGNOSTICS_PANEL_MAX_WIDTH
    );
    assert_eq!(
        clamp_diagnostics_panel_width(f32::NAN),
        DIAGNOSTICS_PANEL_DEFAULT_WIDTH
    );

    assert_eq!(clamp_source_control_width(1.0), SOURCE_CONTROL_MIN_WIDTH);
    assert_eq!(clamp_source_control_width(900.0), SOURCE_CONTROL_MAX_WIDTH);
    assert_eq!(
        clamp_source_control_width(f32::NAN),
        SOURCE_CONTROL_DEFAULT_WIDTH
    );
}

#[test]
fn responsive_side_panel_width_preserves_editor_space_when_possible() {
    assert_eq!(
        responsive_side_panel_max_width(1400.0, 420.0, 240.0, 520.0),
        520.0
    );
    assert_eq!(
        responsive_side_panel_max_width(900.0, 420.0, 240.0, 520.0),
        260.0
    );
    assert_eq!(
        responsive_side_panel_max_width(720.0, 420.0, 240.0, 520.0),
        240.0
    );
    assert_eq!(
        responsive_side_panel_max_width(f32::NAN, 420.0, 240.0, 520.0),
        520.0
    );
}

#[test]
fn panel_placement_cycles_between_docked_and_floating_targets() {
    assert_eq!(
        PanelPlacement::DockedRight.cycle(),
        PanelPlacement::Floating
    );
    assert_eq!(PanelPlacement::Floating.cycle(), PanelPlacement::DockedLeft);
    assert_eq!(
        PanelPlacement::DockedLeft.cycle(),
        PanelPlacement::DockedRight
    );
    assert_eq!(
        PanelPlacement::DockedLeft.dock_side(),
        Some(PanelDockSide::Left)
    );
    assert_eq!(
        PanelPlacement::DockedRight.dock_side(),
        Some(PanelDockSide::Right)
    );
    assert_eq!(PanelPlacement::Floating.dock_side(), None);
    assert!(PanelPlacement::Floating.is_floating());

    let mut open = false;
    let mut placement = PanelPlacement::DockedRight;
    assert_eq!(
        cycle_panel_placement(&mut open, &mut placement, "Project Search"),
        "Project Search panel moved to floating window"
    );
    assert!(open);
    assert_eq!(placement, PanelPlacement::Floating);
}

#[test]
fn panel_placement_restore_defaults_unknown_values() {
    assert_eq!(
        serde_json::from_str::<PanelPlacement>("\"dockedLeft\"").unwrap(),
        PanelPlacement::DockedLeft
    );
    assert_eq!(
        serde_json::from_str::<PanelPlacement>("\"futureDock\"").unwrap(),
        PanelPlacement::DockedRight
    );
    assert_eq!(
        serde_json::from_str::<PanelPlacement>("null").unwrap(),
        PanelPlacement::DockedRight
    );
}

#[test]
fn split_weight_adjustment_preserves_sum_and_min_width() {
    let mut weights = vec![0.5, 0.5];
    assert!(adjust_split_weights(&mut weights, 0, 100.0, 1000.0));
    assert!((weights.iter().sum::<f32>() - 1.0).abs() < 0.001);
    assert!(weights[0] > weights[1]);

    assert!(adjust_split_weights(&mut weights, 0, -800.0, 1000.0));
    assert!(weights[0] >= MIN_EDITOR_PANE_WIDTH / 1000.0);
    assert!((weights.iter().sum::<f32>() - 1.0).abs() < 0.001);
}

#[test]
fn split_weight_adjustment_rejects_non_finite_geometry_without_corruption() {
    let mut weights = vec![2.0, 1.0];
    assert!(!adjust_split_weights(&mut weights, 0, f32::NAN, 1000.0));
    assert_eq!(weights, vec![2.0 / 3.0, 1.0 / 3.0]);

    assert!(!adjust_split_weights(&mut weights, 0, 10.0, f32::INFINITY));
    assert_eq!(weights, vec![2.0 / 3.0, 1.0 / 3.0]);
    assert!(weights.iter().all(|weight| weight.is_finite()));
}

#[test]
fn minimap_coordinate_mapping_clamps_to_file_lines() {
    let rect = egui::Rect::from_min_size(pos2(10.0, 20.0), vec2(80.0, 200.0));
    assert_eq!(minimap_line_from_y(20.0, rect, 100), 0);
    assert_eq!(minimap_line_from_y(120.0, rect, 100), 50);
    assert_eq!(minimap_line_from_y(220.0, rect, 100), 99);
    assert_eq!(minimap_line_from_y(500.0, rect, 100), 99);
    assert_eq!(minimap_line_from_y(-20.0, rect, 100), 0);
    assert_eq!(minimap_line_from_y(120.0, rect, 0), 0);
}

#[test]
fn minimap_target_keeps_clicked_line_near_viewport_center() {
    let rect = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 100.0));
    assert_eq!(minimap_target_line_from_y(50.0, rect, 100, 20), 40);
    assert_eq!(minimap_target_line_from_y(0.0, rect, 100, 20), 0);
    assert_eq!(minimap_target_line_from_y(100.0, rect, 100, 20), 80);
}

#[test]
fn minimap_viewport_rect_caps_partial_thumb_size_and_covers_fully_visible_file() {
    let rect = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 200.0));
    let viewport = minimap_viewport_rect(rect, 50, 20, 100);
    assert_eq!(viewport.top(), 100.0);
    assert_eq!(viewport.bottom(), 140.0);

    let short = minimap_viewport_rect(rect, 0, 20, 8);
    assert_eq!(short, rect);

    let near_bottom = minimap_viewport_rect(rect, 7, 20, 8);
    assert_eq!(near_bottom, rect);

    let tiny = minimap_viewport_rect(rect, 0, 1, 10_000);
    assert!(tiny.height() >= 4.0);
}

#[test]
fn minimap_viewport_rect_uses_clamped_thumb_travel() {
    let rect = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 200.0));

    let before_last = minimap_viewport_rect(rect, 9_800, 100, 10_000);
    assert!(before_last.bottom() < rect.bottom());

    let last = minimap_viewport_rect(rect, 9_900, 100, 10_000);
    assert_eq!(last.bottom(), rect.bottom());

    let stale = minimap_viewport_rect(rect, usize::MAX, 100, 10_000);
    assert_eq!(stale.bottom(), rect.bottom());
}

#[test]
fn minimap_sampling_covers_start_middle_and_end() {
    assert_eq!(minimap_sample_line(0, 5, 101), 0);
    assert_eq!(minimap_sample_line(2, 5, 101), 50);
    assert_eq!(minimap_sample_line(4, 5, 101), 100);
    assert_eq!(minimap_sample_line(9, 5, 101), 100);
}

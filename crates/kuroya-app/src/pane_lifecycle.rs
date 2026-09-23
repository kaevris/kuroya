mod weights;

use crate::{
    KuroyaApp,
    editor_pane_scroll::{
        clear_editor_horizontal_scroll_offsets_for_pane, clear_editor_inertial_scrolls_for_pane,
        clear_editor_middle_click_scroll_for_pane, clear_editor_scroll_state_for_pane,
    },
    session_state::EditorPane,
};
use kuroya_core::{BufferId, TextBuffer};
use std::path::PathBuf;

impl KuroyaApp {
    pub(crate) fn split_buffer_right(&mut self, id: BufferId) {
        // Prefer the active pane showing the buffer so the split inherits the
        // viewport the user is actually looking at, not the first pane that
        // happens to hold the same buffer.
        let source_pane = self
            .active_pane_holding_buffer(id)
            .or_else(|| self.pane_id_for_buffer(id))
            .unwrap_or(self.active_pane);
        let pane_id = self.insert_editor_pane_right(Some(id));
        self.copy_pane_viewport_state(source_pane, pane_id, id);
        self.set_active_buffer_in_pane(pane_id, id);
        self.status = format!("Split editor into {} panes", self.panes.len());
    }

    pub(crate) fn split_buffer_right_from_tab_menu(&mut self, id: BufferId) {
        // The tab menu is invoked from the tab bar, so the pane showing that
        // tab is not necessarily the active pane; focus it first so the split
        // is inserted next to it (mirroring the pane context menu).
        if let Some(pane_id) = self.pane_id_for_buffer(id) {
            self.active_pane = pane_id;
        }
        self.split_buffer_right(id);
    }

    pub(crate) fn insert_editor_pane_right(
        &mut self,
        active: Option<BufferId>,
    ) -> crate::workspace_state::PaneId {
        let pane_id = self.next_pane_id;
        self.next_pane_id += 1;
        let (insert_at, weight) = self.new_pane_insert_position_and_weight();
        self.panes.insert(
            insert_at,
            EditorPane {
                id: pane_id,
                active,
                weight,
            },
        );
        self.normalize_pane_weights();
        pane_id
    }

    pub(crate) fn open_path_in_new_pane(&mut self, path: PathBuf) {
        let pane_id = self.insert_editor_pane_right(None);
        self.active_pane = pane_id;
        self.focused_pane = Some(pane_id);
        if let Some(id) = self.buffer_by_lexical_path(&path).map(TextBuffer::id) {
            self.set_active_buffer_in_pane(pane_id, id);
            return;
        }
        self.pending_pane_paths.insert(pane_id, path.clone());
        self.spawn_open_file(path);
    }

    pub(crate) fn close_active_pane(&mut self) {
        if self.panes.len() <= 1 {
            self.status = "Cannot close the last pane".to_owned();
            return;
        }

        let position = self
            .panes
            .iter()
            .position(|pane| pane.id == self.active_pane)
            .unwrap_or(self.panes.len() - 1);
        let removed = self.panes.remove(position);
        clear_editor_scroll_state_for_pane(
            &mut self.editor_scroll_offsets,
            &mut self.editor_scroll_targets,
            removed.id,
        );
        clear_editor_horizontal_scroll_offsets_for_pane(
            &mut self.editor_horizontal_scroll_offsets,
            removed.id,
        );
        clear_editor_inertial_scrolls_for_pane(&mut self.editor_inertial_scrolls, removed.id);
        clear_editor_middle_click_scroll_for_pane(&mut self.editor_middle_click_scroll, removed.id);
        self.pending_pane_paths.remove(&removed.id);
        self.pending_pane_view_states.remove(&removed.id);
        self.pending_pane_scroll_lines
            .retain(|(pane_id, _), _| *pane_id != removed.id);
        self.pending_pane_horizontal_scroll_offsets
            .retain(|(pane_id, _), _| *pane_id != removed.id);
        let recipient = if position < self.panes.len() {
            position
        } else {
            self.panes.len() - 1
        };
        self.panes[recipient].weight += removed.weight;
        self.active_pane = self.panes[recipient].id;
        self.focused_pane = Some(self.active_pane);
        if let Some(active) = self.panes[recipient].active {
            self.set_active_buffer(active);
        } else {
            self.active = self.panes.iter().find_map(|pane| pane.active);
        }
        self.normalize_pane_weights();
        self.status = format!("Closed pane {}", removed.id);
    }

    /// Closes panes that are still waiting for a pending load (`active: None`)
    /// and can therefore never be focused. Panes that already show a buffer
    /// are left untouched.
    pub(crate) fn close_orphaned_panes(&mut self, pane_ids: &[crate::workspace_state::PaneId]) {
        for pane_id in pane_ids {
            self.close_orphaned_pane(*pane_id);
        }
    }

    fn close_orphaned_pane(&mut self, pane_id: crate::workspace_state::PaneId) {
        let Some(position) = self.panes.iter().position(|pane| pane.id == pane_id) else {
            return;
        };
        if self.panes[position].active.is_some() {
            return;
        }
        let removed = self.panes.remove(position);
        clear_editor_scroll_state_for_pane(
            &mut self.editor_scroll_offsets,
            &mut self.editor_scroll_targets,
            removed.id,
        );
        clear_editor_horizontal_scroll_offsets_for_pane(
            &mut self.editor_horizontal_scroll_offsets,
            removed.id,
        );
        clear_editor_inertial_scrolls_for_pane(&mut self.editor_inertial_scrolls, removed.id);
        clear_editor_middle_click_scroll_for_pane(&mut self.editor_middle_click_scroll, removed.id);
        self.pending_pane_paths.remove(&removed.id);
        self.pending_pane_view_states.remove(&removed.id);
        self.pending_pane_scroll_lines
            .retain(|(pane_id, _), _| *pane_id != removed.id);
        self.pending_pane_horizontal_scroll_offsets
            .retain(|(pane_id, _), _| *pane_id != removed.id);
        if self.panes.is_empty() {
            self.panes.push(EditorPane {
                id: 1,
                active: self.active,
                weight: 1.0,
            });
            self.active_pane = 1;
            self.next_pane_id = self.next_pane_id.max(2);
        } else if !self.panes.iter().any(|pane| pane.id == self.active_pane) {
            let recipient = position.min(self.panes.len() - 1);
            self.active_pane = self.panes[recipient].id;
        }
        if self.focused_pane == Some(removed.id) {
            self.focused_pane = Some(self.active_pane);
        }
        self.normalize_pane_weights();
    }

    fn copy_pane_viewport_state(
        &mut self,
        source_pane: crate::workspace_state::PaneId,
        target_pane: crate::workspace_state::PaneId,
        id: BufferId,
    ) {
        if let Some(offset) = self.editor_scroll_offsets.get(&(source_pane, id)).copied() {
            self.editor_scroll_offsets.insert((target_pane, id), offset);
        }
        if let Some(offset) = self
            .editor_horizontal_scroll_offsets
            .get(&(source_pane, id))
            .copied()
        {
            self.editor_horizontal_scroll_offsets
                .insert((target_pane, id), offset);
        }
        if let Some(offset) = self.editor_scroll_targets.get(&(source_pane, id)).copied() {
            self.editor_scroll_targets.insert((target_pane, id), offset);
        }
        if let Some(line) = self
            .pending_pane_scroll_lines
            .get(&(source_pane, id))
            .copied()
            .or_else(|| self.pending_scroll_lines.get(&id).copied())
        {
            self.pending_pane_scroll_lines
                .insert((target_pane, id), line);
        }
        if let Some(offset) = self
            .pending_pane_horizontal_scroll_offsets
            .get(&(source_pane, id))
            .copied()
            .or_else(|| self.pending_horizontal_scroll_offsets.get(&id).copied())
        {
            self.pending_pane_horizontal_scroll_offsets
                .insert((target_pane, id), offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_startup_context::AppStartupContext, terminal::TerminalPane,
        transient_state::EditorInertialScroll,
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use std::{path::PathBuf, time::Instant};
    use tokio::runtime::Runtime;

    #[test]
    fn close_active_pane_clears_editor_scroll_state_for_removed_pane() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "fn main() {}\n".to_owned(),
        ));
        app.panes[0].active = Some(7);
        app.active = Some(7);
        let removed_pane = app.insert_editor_pane_right(Some(7));
        app.active_pane = removed_pane;
        app.editor_scroll_offsets.insert((1, 7), 120.0);
        app.editor_scroll_offsets.insert((removed_pane, 7), 240.0);
        app.editor_horizontal_scroll_offsets.insert((1, 7), 12.0);
        app.editor_horizontal_scroll_offsets
            .insert((removed_pane, 7), 24.0);
        app.editor_scroll_targets.insert((1, 7), 300.0);
        app.editor_scroll_targets.insert((removed_pane, 7), 360.0);
        app.editor_inertial_scrolls.insert(
            (1, 7),
            EditorInertialScroll {
                velocity_x: 12.0,
                velocity_y: 24.0,
            },
        );
        app.editor_inertial_scrolls.insert(
            (removed_pane, 7),
            EditorInertialScroll {
                velocity_x: 36.0,
                velocity_y: 48.0,
            },
        );
        app.pending_pane_paths
            .insert(removed_pane, root.join("src/main.rs"));

        app.close_active_pane();

        assert!(!app.editor_scroll_offsets.contains_key(&(removed_pane, 7)));
        assert!(
            !app.editor_horizontal_scroll_offsets
                .contains_key(&(removed_pane, 7))
        );
        assert!(!app.editor_scroll_targets.contains_key(&(removed_pane, 7)));
        assert!(!app.editor_inertial_scrolls.contains_key(&(removed_pane, 7)));
        assert_eq!(app.editor_scroll_offsets.get(&(1, 7)), Some(&120.0));
        assert_eq!(
            app.editor_horizontal_scroll_offsets.get(&(1, 7)),
            Some(&12.0)
        );
        assert_eq!(app.editor_scroll_targets.get(&(1, 7)), Some(&300.0));
        assert_eq!(
            app.editor_inertial_scrolls.get(&(1, 7)),
            Some(&EditorInertialScroll {
                velocity_x: 12.0,
                velocity_y: 24.0,
            })
        );
        assert!(!app.pending_pane_paths.contains_key(&removed_pane));
    }

    #[test]
    fn split_buffer_right_copies_source_pane_viewport_state() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "one\ntwo\nthree\nfour\nfive\n".to_owned(),
        ));
        app.panes[0].active = Some(7);
        app.active = Some(7);
        app.active_pane = 1;
        app.editor_scroll_offsets.insert((1, 7), 120.0);
        app.editor_horizontal_scroll_offsets.insert((1, 7), 32.0);
        app.editor_scroll_targets.insert((1, 7), 180.0);

        app.split_buffer_right(7);

        let split_pane = app.active_pane;
        assert_ne!(split_pane, 1);
        assert_eq!(
            app.editor_scroll_offsets.get(&(split_pane, 7)),
            Some(&120.0)
        );
        assert_eq!(
            app.editor_horizontal_scroll_offsets.get(&(split_pane, 7)),
            Some(&32.0)
        );
        assert_eq!(
            app.editor_scroll_targets.get(&(split_pane, 7)),
            Some(&180.0)
        );
    }

    #[test]
    fn split_buffer_right_copies_viewport_from_pane_showing_inactive_tab() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/a.rs")),
            "one\ntwo\nthree\nfour\nfive\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(root.join("src/b.rs")),
            "alpha\nbeta\n".to_owned(),
        ));
        app.panes[0].active = Some(7);
        let second_pane = app.insert_editor_pane_right(Some(8));
        app.active_pane = second_pane;
        app.active = Some(8);
        app.editor_scroll_offsets.insert((1, 7), 120.0);
        app.editor_horizontal_scroll_offsets.insert((1, 7), 32.0);
        app.editor_scroll_offsets.insert((second_pane, 8), 12.0);
        app.editor_horizontal_scroll_offsets
            .insert((second_pane, 8), 6.0);

        app.split_buffer_right(7);

        let split_pane = app.active_pane;
        assert_ne!(split_pane, 1);
        assert_ne!(split_pane, second_pane);
        assert_eq!(
            app.editor_scroll_offsets.get(&(split_pane, 7)),
            Some(&120.0)
        );
        assert_eq!(
            app.editor_horizontal_scroll_offsets.get(&(split_pane, 7)),
            Some(&32.0)
        );
    }

    #[test]
    fn split_buffer_right_copies_viewport_from_active_pane_holding_buffer() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/a.rs")),
            "one\ntwo\nthree\nfour\nfive\n".to_owned(),
        ));
        app.panes[0].active = Some(7);
        let second_pane = app.insert_editor_pane_right(Some(7));
        app.active_pane = second_pane;
        app.active = Some(7);
        app.editor_scroll_offsets.insert((1, 7), 120.0);
        app.editor_scroll_offsets.insert((second_pane, 7), 40.0);
        app.editor_scroll_targets.insert((second_pane, 7), 200.0);

        app.split_buffer_right(7);

        let split_pane = app.active_pane;
        assert_ne!(split_pane, 1);
        assert_ne!(split_pane, second_pane);
        assert_eq!(app.editor_scroll_offsets.get(&(split_pane, 7)), Some(&40.0));
        assert_eq!(
            app.editor_scroll_targets.get(&(split_pane, 7)),
            Some(&200.0)
        );
    }

    #[test]
    fn split_from_tab_menu_splits_pane_showing_tab() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root.clone());
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(root.join("src/a.rs")),
            "one\ntwo\nthree\nfour\nfive\n".to_owned(),
        ));
        app.buffers.push(TextBuffer::from_text(
            8,
            Some(root.join("src/b.rs")),
            "alpha\nbeta\n".to_owned(),
        ));
        app.panes[0].active = Some(8);
        let tab_pane = app.insert_editor_pane_right(Some(7));
        app.editor_scroll_offsets.insert((tab_pane, 7), 33.0);
        app.editor_scroll_targets.insert((tab_pane, 7), 90.0);
        // The tab menu runs while a different pane is active.
        app.active_pane = 1;
        app.active = Some(8);

        app.split_buffer_right_from_tab_menu(7);

        let split_pane = app.active_pane;
        assert_ne!(split_pane, 1);
        assert_ne!(split_pane, tab_pane);
        assert_eq!(app.editor_scroll_offsets.get(&(split_pane, 7)), Some(&33.0));
        assert_eq!(app.editor_scroll_targets.get(&(split_pane, 7)), Some(&90.0));
        let split_position = app
            .panes
            .iter()
            .position(|pane| pane.id == split_pane)
            .expect("split pane exists");
        let tab_position = app
            .panes
            .iter()
            .position(|pane| pane.id == tab_pane)
            .expect("tab pane exists");
        assert_eq!(split_position, tab_position + 1);
        assert_eq!(app.active, Some(7));
    }

    #[test]
    fn open_path_in_new_pane_assigns_existing_open_buffer_without_pending_target() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let equivalent_path = root.join("src").join("..").join("src/main.rs");
        let mut app = app_for_test(root);
        app.buffers.push(TextBuffer::from_text(
            7,
            Some(path),
            "fn main() {}\n".to_owned(),
        ));

        app.open_path_in_new_pane(equivalent_path);

        let pane_id = app.active_pane;
        assert_eq!(app.active, Some(7));
        assert_eq!(
            app.panes
                .iter()
                .find(|pane| pane.id == pane_id)
                .and_then(|pane| pane.active),
            Some(7)
        );
        assert!(app.pending_pane_paths.is_empty());
        assert!(app.pending_open_paths.is_empty());
    }

    #[test]
    fn open_path_in_new_pane_preserves_raw_pending_target_for_duplicate_open() {
        let root = PathBuf::from("workspace");
        let pending_path = root.join("src/main.rs");
        let raw_path = root.join("src").join("..").join("src/main.rs");
        let mut app = app_for_test(root);
        app.pending_open_paths.insert(pending_path.clone());

        app.open_path_in_new_pane(raw_path.clone());

        let pane_id = app.active_pane;
        assert_eq!(app.pending_pane_paths.get(&pane_id), Some(&raw_path));
        assert!(app.pending_open_paths.contains(&pending_path));
        assert_eq!(app.pending_active_path, Some(raw_path));
        assert!(
            app.panes
                .iter()
                .find(|pane| pane.id == pane_id)
                .is_some_and(|pane| pane.active.is_none())
        );
    }

    #[test]
    fn insert_editor_pane_right_sanitizes_non_finite_active_weight() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        app.panes[0].weight = f32::NAN;

        app.insert_editor_pane_right(None);

        assert_eq!(app.panes.len(), 2);
        assert!(app.panes.iter().all(|pane| pane.weight.is_finite()));
        assert!(app.panes.iter().all(|pane| pane.weight > 0.0));
        assert!((pane_weight_sum(&app) - 1.0).abs() < 0.001);
    }

    #[test]
    fn close_active_pane_recovers_from_non_finite_merged_weights() {
        let root = PathBuf::from("workspace");
        let mut app = app_for_test(root);
        let second_pane = app.insert_editor_pane_right(None);
        app.panes[0].weight = f32::INFINITY;
        app.panes[1].weight = f32::NAN;
        app.active_pane = second_pane;

        app.close_active_pane();

        assert_eq!(app.panes.len(), 1);
        assert_eq!(app.panes[0].weight, 1.0);
    }

    fn pane_weight_sum(app: &KuroyaApp) -> f32 {
        app.panes.iter().map(|pane| pane.weight).sum()
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

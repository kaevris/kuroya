use crate::{
    KuroyaApp,
    app_startup_context::{AppStartupContext, spawn_startup_session_load},
    persistence::AppState,
    startup_arguments::StartupTarget,
};

impl KuroyaApp {
    pub(crate) fn new(
        cc: &eframe::CreationContext<'_>,
        startup_target: Option<StartupTarget>,
    ) -> anyhow::Result<Self> {
        let context = AppStartupContext::load(cc)?;
        // Startup no longer carries the session on the context: it is loaded
        // off-thread and delivered via `UiEvent::StartupSessionLoaded`. The
        // field stays because the crate's test context constructions set it.
        let _ = &context.saved_session;
        let mut app = Self::from_startup_context(context);
        app.set_background_image_repaint_context(&cc.egui_ctx);
        app.sync_background_image(false);
        // Discord Rich Presence is fully opt-in: this is inert (no thread, no
        // IPC) unless the user enabled it and configured their own client id.
        app.sync_discord_presence_runtime();
        if app.workspace_placeholder {
            let _ = app.save_app_state();
            match startup_target {
                Some(StartupTarget::File(path)) => app.spawn_open_file(path),
                Some(StartupTarget::Folder(path)) => app.open_workspace_now(path),
                None => {}
            }
            return Ok(app);
        }
        // The saved session is loaded off-thread and applied when the
        // critical `UiEvent::StartupSessionLoaded` arrives, so the first
        // frame is not blocked on session disk I/O. The startup target
        // (open file / open folder) rides along on that event: applying it
        // only after the restore keeps the ordering of the previous
        // synchronous startup, where `restore_session` ran before the target
        // was handled (opening a folder re-saves the restored session for
        // the previous workspace, so it must observe the restored state).
        app.record_recent_project(app.workspace.root.clone());
        let startup_target_deferred = startup_target.is_some();
        spawn_startup_session_load(
            &app.runtime,
            app.tx.clone(),
            app.workspace.root.clone(),
            startup_target,
        );
        app.spawn_save_app_state();
        app.spawn_index();
        app.spawn_git_scan();
        app.spawn_workspace_task_load();
        app.spawn_plugin_discovery();
        if !startup_target_deferred {
            app.arm_workspace_trust_prompt();
        }
        Ok(app)
    }

    /// Writes the persisted app state on the blocking thread pool instead of
    /// the startup path. The state snapshot is taken synchronously here; the
    /// disk write only needs to finish before exit, and the shutdown path
    /// (`prepare_shutdown`) saves the app state synchronously again.
    fn spawn_save_app_state(&mut self) {
        let app_state = AppState {
            recent_projects: self.recent_projects.clone(),
            trusted_workspaces: self.trusted_workspaces.clone(),
            vim_keybindings: Some(self.app_state_vim_keybindings),
            vim: Some(self.app_state_vim.clone()),
            theme: Some(self.settings.theme.clone()),
            custom_theme_paths: self.settings.custom_theme_paths.clone(),
            active_custom_theme_path: self.settings.active_custom_theme_path.clone(),
            editor_font_path: self.settings.editor_font_path.clone(),
            ui_font_path: self.settings.ui_font_path.clone(),
        };
        #[cfg(test)]
        let path_override = self.app_state_path_override.clone();
        self.runtime.spawn_blocking(move || {
            #[cfg(test)]
            if let Some(path) = path_override {
                let _ = app_state.save_to_path(&path);
                return;
            }
            let _ = app_state.save();
        });
    }
}

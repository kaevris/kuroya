use crate::{
    KuroyaApp,
    explorer::{ExplorerDeleteTarget, ExplorerEntryKind, explorer_kind_for_path},
    path_display::display_path_label_cow,
    popup_buttons::{PopupButtonKind, popup_button},
    workspace_state::paths_match_lexically,
    workspace_trust::workspace_path_stays_within_root_lexically,
};
use eframe::egui::{self, Align, Context, Id, Key, RichText};
use kuroya_core::{ProjectEntry, TextBuffer, path_is_committed};
use std::{
    collections::HashSet,
    ffi::OsStr,
    fs,
    path::Component,
    path::{Path, PathBuf},
    sync::Arc,
};

const EXPLORER_DELETE_PROMPT_CACHE_ID: &str = "explorer_delete_prompt_cache";

impl KuroyaApp {
    pub(crate) fn begin_delete_path(&mut self, path: PathBuf) {
        if let Some(status) = explorer_delete_rejected_status(&self.workspace.root, &path) {
            self.status = status;
            return;
        }
        let indexed_entries = self.index.all_entries();
        if let Some(status) = explorer_delete_missing_status(
            indexed_entries,
            &self.buffers,
            self.index.files(),
            &path,
            Path::exists,
        ) {
            self.status = status;
            return;
        }
        let kind = explorer_delete_kind_for_path(indexed_entries, &path, explorer_kind_for_path);
        if !explorer_delete_requires_confirmation(
            &self.workspace.root,
            &path,
            self.settings.git_confirm_committed_delete,
        ) {
            self.spawn_delete_path(path, kind);
            return;
        }

        self.explorer_delete_target = Some(ExplorerDeleteTarget { kind, path });
    }

    fn confirm_explorer_delete(&mut self) {
        let Some(target) = self.explorer_delete_target.take() else {
            return;
        };
        if let Some(status) = explorer_delete_rejected_status(&self.workspace.root, &target.path) {
            self.status = status;
            return;
        }
        if let Some(status) = explorer_delete_missing_status(
            self.index.all_entries(),
            &self.buffers,
            self.index.files(),
            &target.path,
            Path::exists,
        ) {
            self.status = status;
            return;
        }
        if let Some(status) = explorer_delete_target_kind_rejected_status(
            &target.path,
            target.kind,
            explorer_delete_probe_path_kind,
        ) {
            self.status = status;
            return;
        }
        self.spawn_delete_path(target.path, target.kind);
    }

    pub(crate) fn render_explorer_delete(&mut self, ctx: &Context) {
        let Some(target) = self.explorer_delete_target.as_ref() else {
            return;
        };
        let mut delete = false;
        let mut cancel = false;
        let prompt = cached_explorer_delete_prompt(ctx, target);

        egui::Window::new(Arc::clone(&prompt.title))
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([500.0, 150.0])
            .show(ctx, |ui| {
                ui.label(Arc::clone(&prompt.path_label));
                ui.label(Arc::clone(&prompt.body));

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Delete", PopupButtonKind::Danger).clicked() {
                        delete = true;
                    }
                });
            });

        if cancel {
            clear_cached_explorer_delete_prompt(ctx);
            self.explorer_delete_target = None;
            self.status = "Delete canceled".to_owned();
        } else if delete {
            clear_cached_explorer_delete_prompt(ctx);
            self.confirm_explorer_delete();
        }
    }
}

#[derive(Clone)]
struct ExplorerDeletePrompt {
    title: Arc<RichText>,
    path_label: Arc<RichText>,
    body: Arc<RichText>,
}

fn cached_explorer_delete_prompt(
    ctx: &Context,
    target: &ExplorerDeleteTarget,
) -> Arc<ExplorerDeletePrompt> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<ExplorerDeletePromptCache>(Id::new(
            EXPLORER_DELETE_PROMPT_CACHE_ID,
        ))
        .prompt(target)
    })
}

fn clear_cached_explorer_delete_prompt(ctx: &Context) {
    ctx.data_mut(|data| {
        data.remove::<ExplorerDeletePromptCache>(Id::new(EXPLORER_DELETE_PROMPT_CACHE_ID));
    });
}

#[derive(Clone, Default)]
struct ExplorerDeletePromptCache {
    key: Option<ExplorerDeletePromptKey>,
    prompt: Option<Arc<ExplorerDeletePrompt>>,
}

impl ExplorerDeletePromptCache {
    fn prompt(&mut self, target: &ExplorerDeleteTarget) -> Arc<ExplorerDeletePrompt> {
        if let (Some(key), Some(prompt)) = (&self.key, &self.prompt) {
            if key.matches(target) {
                return Arc::clone(prompt);
            }
        }

        let prompt = Arc::new(explorer_delete_prompt(target));
        self.key = Some(ExplorerDeletePromptKey {
            path: target.path.clone(),
            kind: target.kind,
        });
        self.prompt = Some(Arc::clone(&prompt));
        prompt
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ExplorerDeletePromptKey {
    path: PathBuf,
    kind: ExplorerEntryKind,
}

impl ExplorerDeletePromptKey {
    fn matches(&self, target: &ExplorerDeleteTarget) -> bool {
        self.kind == target.kind && self.path == target.path
    }
}

fn explorer_delete_prompt(target: &ExplorerDeleteTarget) -> ExplorerDeletePrompt {
    let (title, body) = match target.kind {
        ExplorerEntryKind::File => ("Delete File", "Delete this file from disk?"),
        ExplorerEntryKind::Folder => (
            "Delete Folder",
            "Delete this folder and everything inside it?",
        ),
    };

    ExplorerDeletePrompt {
        title: Arc::new(RichText::new(title)),
        path_label: Arc::new(RichText::new(explorer_delete_target_label(&target.path)).strong()),
        body: Arc::new(RichText::new(body)),
    }
}

pub(crate) fn explorer_delete_requires_confirmation(
    workspace_root: &std::path::Path,
    path: &std::path::Path,
    confirm_committed_delete: bool,
) -> bool {
    confirm_committed_delete || !path_is_committed(workspace_root, path).unwrap_or(false)
}

fn explorer_delete_target_label(path: &Path) -> String {
    display_path_label_cow(path).into_owned()
}

fn explorer_delete_rejected_status(workspace_root: &Path, path: &Path) -> Option<String> {
    if paths_match_lexically(path, workspace_root) {
        Some("Cannot delete the workspace root".to_owned())
    } else if !workspace_path_stays_within_root_lexically(workspace_root, path) {
        Some("Cannot delete paths outside the workspace".to_owned())
    } else {
        None
    }
}

fn explorer_delete_missing_status(
    indexed_entries: &[ProjectEntry],
    buffers: &[TextBuffer],
    indexed_files: &[PathBuf],
    path: &Path,
    path_exists: impl FnOnce(&Path) -> bool,
) -> Option<String> {
    if ExplorerDeletePathAvailability::from_sources(indexed_entries, buffers, indexed_files)
        .known_or_exists(path, path_exists)
    {
        None
    } else {
        Some(format!(
            "{} no longer exists",
            explorer_delete_target_label(path)
        ))
    }
}

fn explorer_delete_kind_for_path(
    indexed_entries: &[ProjectEntry],
    path: &Path,
    fallback_kind: impl FnOnce(&Path) -> ExplorerEntryKind,
) -> ExplorerEntryKind {
    indexed_entries
        .iter()
        .find(|entry| entry.path == path || paths_match_lexically(&entry.path, path))
        .map(|entry| {
            if entry.is_dir {
                ExplorerEntryKind::Folder
            } else {
                ExplorerEntryKind::File
            }
        })
        .unwrap_or_else(|| fallback_kind(path))
}

fn explorer_delete_target_kind_rejected_status(
    path: &Path,
    expected_kind: ExplorerEntryKind,
    probe_path_kind: impl FnOnce(&Path) -> ExplorerDeletePathKindProbe,
) -> Option<String> {
    match probe_path_kind(path) {
        ExplorerDeletePathKindProbe::Known(actual_kind) if actual_kind != expected_kind => Some(
            explorer_delete_target_kind_changed_status(path, expected_kind),
        ),
        ExplorerDeletePathKindProbe::Known(_)
        | ExplorerDeletePathKindProbe::Missing
        | ExplorerDeletePathKindProbe::Unknown => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExplorerDeletePathKindProbe {
    Known(ExplorerEntryKind),
    Missing,
    Unknown,
}

fn explorer_delete_probe_path_kind(path: &Path) -> ExplorerDeletePathKindProbe {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            ExplorerDeletePathKindProbe::Known(ExplorerEntryKind::File)
        }
        Ok(metadata) if metadata.is_dir() => {
            ExplorerDeletePathKindProbe::Known(ExplorerEntryKind::Folder)
        }
        Ok(_) => ExplorerDeletePathKindProbe::Unknown,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ExplorerDeletePathKindProbe::Missing
        }
        Err(_) => ExplorerDeletePathKindProbe::Unknown,
    }
}

fn explorer_delete_target_kind_changed_status(
    path: &Path,
    expected_kind: ExplorerEntryKind,
) -> String {
    let kind = match expected_kind {
        ExplorerEntryKind::File => "file",
        ExplorerEntryKind::Folder => "folder",
    };
    format!(
        "{} is no longer a {kind}",
        explorer_delete_target_label(path)
    )
}

#[derive(Debug, Default)]
struct ExplorerDeletePathAvailability {
    known_paths: HashSet<PathBuf>,
}

impl ExplorerDeletePathAvailability {
    fn from_sources(
        indexed_entries: &[ProjectEntry],
        buffers: &[TextBuffer],
        indexed_files: &[PathBuf],
    ) -> Self {
        let mut known_paths =
            HashSet::with_capacity(indexed_entries.len() + buffers.len() + indexed_files.len());
        for entry in indexed_entries {
            known_paths.insert(explorer_delete_path_key(&entry.path));
        }
        for buffer in buffers {
            if let Some(path) = buffer.path() {
                known_paths.insert(explorer_delete_path_key(path));
            }
        }
        for path in indexed_files {
            known_paths.insert(explorer_delete_path_key(path));
        }

        Self { known_paths }
    }

    fn known_or_exists(&self, path: &Path, path_exists: impl FnOnce(&Path) -> bool) -> bool {
        if self.known_paths.is_empty() {
            return path_exists(path);
        }

        self.known_paths.contains(&explorer_delete_path_key(path)) || path_exists(path)
    }
}

fn explorer_delete_path_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    let mut has_root = false;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                explorer_delete_push_component_key(&mut key, prefix.as_os_str());
                has_root = false;
            }
            Component::RootDir => {
                key.push(component.as_os_str());
                has_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let can_pop = matches!(key.components().next_back(), Some(Component::Normal(_)));
                if can_pop {
                    key.pop();
                } else if !has_root {
                    key.push("..");
                }
            }
            Component::Normal(component) => explorer_delete_push_component_key(&mut key, component),
        }
    }

    if key.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        key
    }
}

#[cfg(windows)]
fn explorer_delete_push_component_key(key: &mut PathBuf, component: &OsStr) {
    use std::borrow::Cow;

    match component.to_string_lossy() {
        Cow::Borrowed(text) if !explorer_delete_component_needs_lowercase(text) => {
            key.push(Path::new(component));
        }
        Cow::Borrowed(text) if text.is_ascii() => {
            let mut lowered = text.to_owned();
            lowered.make_ascii_lowercase();
            key.push(Path::new(&lowered));
        }
        component => {
            let lowered = component.to_lowercase();
            key.push(Path::new(&lowered));
        }
    }
}

#[cfg(windows)]
fn explorer_delete_component_needs_lowercase(component: &str) -> bool {
    if component.is_ascii() {
        return component.bytes().any(|byte| byte.is_ascii_uppercase());
    }

    component.chars().any(|ch| {
        let mut lowercase = ch.to_lowercase();
        lowercase.next() != Some(ch) || lowercase.next().is_some()
    })
}

#[cfg(not(windows))]
fn explorer_delete_push_component_key(key: &mut PathBuf, component: &OsStr) {
    key.push(Path::new(component));
}

#[cfg(test)]
mod tests {
    use super::{
        ExplorerDeletePathAvailability, ExplorerDeletePathKindProbe, ExplorerDeletePromptCache,
        explorer_delete_prompt, explorer_delete_target_kind_rejected_status,
        explorer_delete_target_label,
    };
    use crate::{
        KuroyaApp,
        app_startup_context::AppStartupContext,
        explorer::{ExplorerDeleteTarget, ExplorerEntryKind},
        path_display::DISPLAY_PATH_LABEL_MAX_CHARS,
        terminal::TerminalPane,
    };
    use kuroya_core::{EditorSettings, ProjectEntry, TextBuffer, Workspace};
    use std::{
        cell::Cell,
        fs,
        path::PathBuf,
        sync::Arc,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn explorer_delete_target_labels_are_display_safe_and_bounded() {
        let path = PathBuf::from("workspace").join(format!(
            "bad\n\u{202e}delete{}",
            "-segment".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));

        let label = explorer_delete_target_label(&path);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn explorer_delete_prompt_sanitizes_hostile_path_display_without_mutating_target() {
        let path = PathBuf::from("workspace").join("line\nname\u{0007}\u{202e}hidden.rs");
        let target = ExplorerDeleteTarget {
            path: path.clone(),
            kind: ExplorerEntryKind::File,
        };

        let prompt = explorer_delete_prompt(&target);

        assert_eq!(prompt.title.text(), "Delete File");
        assert_eq!(prompt.path_label.text(), "line name hidden.rs");
        assert_eq!(prompt.body.text(), "Delete this file from disk?");
        assert_eq!(target.path, path);
        let raw = target.path.as_os_str().to_string_lossy();
        assert!(raw.contains('\n'));
        assert!(raw.contains('\u{202e}'));
    }

    #[test]
    fn explorer_delete_prompt_cache_reuses_labels_for_same_raw_target() {
        let target = ExplorerDeleteTarget {
            path: PathBuf::from("workspace").join("src").join("main.rs"),
            kind: ExplorerEntryKind::File,
        };
        let mut cache = ExplorerDeletePromptCache::default();

        let first = cache.prompt(&target);
        let second = cache.prompt(&target);

        assert!(Arc::ptr_eq(&first, &second));

        let changed_target = ExplorerDeleteTarget {
            path: PathBuf::from("workspace").join("src").join("main.rs"),
            kind: ExplorerEntryKind::Folder,
        };
        let third = cache.prompt(&changed_target);

        assert!(!Arc::ptr_eq(&first, &third));
        assert_eq!(third.title.text(), "Delete Folder");
    }

    #[test]
    fn begin_delete_prompt_preserves_raw_target_path_and_kind() {
        let root = temp_workspace("delete-raw-target");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("raw\u{202e}hidden.rs");
        fs::write(&path, "delete me\n").unwrap();
        let mut app = app_for_test(root.clone());

        app.begin_delete_path(path.clone());

        let target = app
            .explorer_delete_target
            .as_ref()
            .expect("delete target should be prompted");
        assert_eq!(target.path, path);
        assert_eq!(target.kind, ExplorerEntryKind::File);
        let raw = target.path.as_os_str().to_string_lossy();
        assert!(raw.contains('\u{202e}'));
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_path_availability_uses_known_sources_before_probe() {
        let root = PathBuf::from("workspace");
        let indexed_entries = vec![project_entry(root.join("src"), true)];
        let buffers = vec![TextBuffer::from_text(
            7,
            Some(root.join("src/main.rs")),
            "open\n".to_owned(),
        )];
        let indexed_files = vec![root.join("src/lib.rs")];
        let availability = ExplorerDeletePathAvailability::from_sources(
            &indexed_entries,
            &buffers,
            &indexed_files,
        );
        let probes = Cell::new(0usize);

        assert!(
            availability.known_or_exists(&root.join("src").join("..").join("src"), |_| {
                probes.set(probes.get() + 1);
                false
            })
        );
        assert!(availability.known_or_exists(
            &root.join("src").join("..").join("src/main.rs"),
            |_| {
                probes.set(probes.get() + 1);
                false
            }
        ));
        assert!(availability.known_or_exists(
            &root.join("src").join("..").join("src/lib.rs"),
            |_| {
                probes.set(probes.get() + 1);
                false
            }
        ));

        assert_eq!(probes.get(), 0);
    }

    #[test]
    fn explorer_delete_path_availability_falls_back_for_unknown_paths() {
        let path = PathBuf::from("workspace/src/generated.rs");
        let availability = ExplorerDeletePathAvailability::from_sources(&[], &[], &[]);
        let probes = Cell::new(0usize);

        assert!(availability.known_or_exists(&path, |_| {
            probes.set(probes.get() + 1);
            true
        }));
        assert_eq!(probes.get(), 1);
    }

    #[test]
    fn explorer_delete_target_kind_rejects_changed_targets() {
        let path = PathBuf::from("workspace").join("src").join("main.rs");

        assert_eq!(
            explorer_delete_target_kind_rejected_status(&path, ExplorerEntryKind::File, |_| {
                ExplorerDeletePathKindProbe::Known(ExplorerEntryKind::Folder)
            },),
            Some("main.rs is no longer a file".to_owned())
        );
        assert_eq!(
            explorer_delete_target_kind_rejected_status(&path, ExplorerEntryKind::Folder, |_| {
                ExplorerDeletePathKindProbe::Known(ExplorerEntryKind::Folder)
            },),
            None
        );
        assert_eq!(
            explorer_delete_target_kind_rejected_status(&path, ExplorerEntryKind::File, |_| {
                ExplorerDeletePathKindProbe::Missing
            },),
            None
        );
    }

    #[test]
    fn explorer_delete_rejects_equivalent_workspace_root_before_prompt() {
        let root = temp_workspace("delete-equivalent-root");
        fs::create_dir_all(root.join("src")).unwrap();
        let equivalent_root = root.join("src").join("..");
        let mut app = app_for_test(root.clone());

        app.begin_delete_path(equivalent_root);

        assert_eq!(app.status, "Cannot delete the workspace root");
        assert!(app.explorer_delete_target.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_rejects_parent_reentry_before_prompt() {
        let root = temp_workspace("delete-parent-reentry");
        fs::create_dir_all(root.join("src")).unwrap();
        let reentry = root.join("..").join(root.file_name().unwrap()).join("src");
        let mut app = app_for_test(root.clone());

        app.begin_delete_path(reentry);

        assert_eq!(app.status, "Cannot delete paths outside the workspace");
        assert!(app.explorer_delete_target.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_rejects_outside_workspace_before_prompt() {
        let root = temp_workspace("delete-outside-root");
        fs::create_dir_all(root.join("src")).unwrap();
        let outside = root.join("..").join("outside.rs");
        let mut app = app_for_test(root.clone());

        app.begin_delete_path(outside);

        assert_eq!(app.status, "Cannot delete paths outside the workspace");
        assert!(app.explorer_delete_target.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_rejects_stale_target_before_prompt() {
        let root = temp_workspace("delete-stale-target");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());

        app.begin_delete_path(root.join("missing.rs"));

        assert_eq!(app.status, "missing.rs no longer exists");
        assert!(app.explorer_delete_target.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_confirm_rechecks_equivalent_workspace_root() {
        let root = temp_workspace("delete-confirm-equivalent-root");
        fs::create_dir_all(root.join("src")).unwrap();
        let equivalent_root = root.join("src").join("..");
        let mut app = app_for_test(root.clone());
        app.explorer_delete_target = Some(ExplorerDeleteTarget {
            path: equivalent_root,
            kind: ExplorerEntryKind::Folder,
        });

        app.confirm_explorer_delete();

        assert_eq!(app.status, "Cannot delete the workspace root");
        assert!(app.explorer_delete_target.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_confirm_rechecks_parent_reentry() {
        let root = temp_workspace("delete-confirm-parent-reentry");
        fs::create_dir_all(root.join("src")).unwrap();
        let reentry = root.join("..").join(root.file_name().unwrap()).join("src");
        let mut app = app_for_test(root.clone());
        app.explorer_delete_target = Some(ExplorerDeleteTarget {
            path: reentry,
            kind: ExplorerEntryKind::Folder,
        });

        app.confirm_explorer_delete();

        assert_eq!(app.status, "Cannot delete paths outside the workspace");
        assert!(app.explorer_delete_target.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_confirm_rechecks_outside_workspace() {
        let root = temp_workspace("delete-confirm-outside-root");
        fs::create_dir_all(root.join("src")).unwrap();
        let outside = root.join("..").join("outside.rs");
        let mut app = app_for_test(root.clone());
        app.explorer_delete_target = Some(ExplorerDeleteTarget {
            path: outside,
            kind: ExplorerEntryKind::File,
        });

        app.confirm_explorer_delete();

        assert_eq!(app.status, "Cannot delete paths outside the workspace");
        assert!(app.explorer_delete_target.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_confirm_rechecks_target_kind_change() {
        let root = temp_workspace("delete-confirm-kind-change");
        let path = root.join("main.rs");
        fs::create_dir_all(&path).unwrap();
        let mut app = app_for_test(root.clone());
        app.explorer_delete_target = Some(ExplorerDeleteTarget {
            path: path.clone(),
            kind: ExplorerEntryKind::File,
        });

        app.confirm_explorer_delete();

        assert_eq!(app.status, "main.rs is no longer a file");
        assert!(app.explorer_delete_target.is_none());
        assert!(path.is_dir());
        drop(app);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explorer_delete_confirm_rechecks_stale_target() {
        let root = temp_workspace("delete-confirm-stale-target");
        fs::create_dir_all(&root).unwrap();
        let mut app = app_for_test(root.clone());
        app.explorer_delete_target = Some(ExplorerDeleteTarget {
            path: root.join("missing.rs"),
            kind: ExplorerEntryKind::File,
        });

        app.confirm_explorer_delete();

        assert_eq!(app.status, "missing.rs no longer exists");
        assert!(app.explorer_delete_target.is_none());
        drop(app);
        fs::remove_dir_all(root).unwrap();
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

    fn temp_workspace(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-explorer-delete-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn project_entry(path: PathBuf, is_dir: bool) -> ProjectEntry {
        ProjectEntry {
            path,
            relative_path: PathBuf::new(),
            is_dir,
            depth: 0,
            ..Default::default()
        }
    }
}

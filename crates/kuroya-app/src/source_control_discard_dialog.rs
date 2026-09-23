use crate::{
    KuroyaApp,
    path_display::display_path_label_cow,
    popup_buttons::{PopupButtonKind, popup_button},
    transient_state::PendingSourceControlDiscard,
};
use eframe::egui::{self, Align, Context, Key, RichText};
use kuroya_core::normalize_child_path;
use std::{
    borrow::Cow,
    path::{Component, Path, PathBuf},
};

const NO_SOURCE_CONTROL_DISCARD_TARGETS_STATUS: &str = "No source control changes to discard";

impl KuroyaApp {
    pub(crate) fn begin_discard_file_changes(&mut self, path: PathBuf) {
        if !self.require_trusted_source_control_mutation("discarding changes") {
            return;
        }
        let Some(paths) = self.current_source_control_discard_paths(vec![path]) else {
            return;
        };
        self.pending_source_control_discard = Some(PendingSourceControlDiscard { paths });
    }

    pub(crate) fn begin_discard_all_changes(&mut self) {
        if !self.require_trusted_source_control_mutation("discarding changes") {
            return;
        }
        // Borrow the sorted snapshot entries instead of cloning the entry
        // list; the paths are collected eagerly because the iterator cannot
        // borrow `self.git` across the `&mut self` call below.
        let paths = self
            .git
            .entries_slice_sorted()
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let Some(paths) = self.current_source_control_discard_paths(paths) else {
            return;
        };

        self.pending_source_control_discard = Some(PendingSourceControlDiscard { paths });
    }

    fn confirm_source_control_discard(&mut self) {
        let Some(target) = self.pending_source_control_discard.take() else {
            return;
        };
        if !self.require_trusted_source_control_mutation("discarding changes") {
            return;
        }
        let Some(paths) = self.current_source_control_discard_paths(target.paths) else {
            return;
        };
        self.spawn_discard_changes(paths);
    }

    fn current_source_control_discard_paths(
        &mut self,
        paths: impl IntoIterator<Item = PathBuf>,
    ) -> Option<Vec<PathBuf>> {
        let operation_root = self.source_control_git_operation_root();
        let paths = normalize_source_control_discard_paths(&operation_root, paths);
        if paths.is_empty() {
            self.status = NO_SOURCE_CONTROL_DISCARD_TARGETS_STATUS.to_owned();
            return None;
        }
        if !self.source_control_discard_paths_current(&paths) {
            return None;
        }
        Some(paths)
    }

    fn normalize_pending_source_control_discard_paths(&mut self) -> bool {
        let operation_root = self.source_control_git_operation_root();
        let Some(target) = self.pending_source_control_discard.as_mut() else {
            return false;
        };

        target.paths =
            normalize_source_control_discard_paths(&operation_root, target.paths.drain(..));
        if target.paths.is_empty() {
            self.pending_source_control_discard = None;
            self.status = NO_SOURCE_CONTROL_DISCARD_TARGETS_STATUS.to_owned();
            return false;
        }
        true
    }

    pub(crate) fn render_source_control_discard(&mut self, ctx: &Context) {
        if self.pending_source_control_discard.is_none() {
            return;
        }
        if !self.require_trusted_source_control_mutation("discarding changes") {
            self.pending_source_control_discard = None;
            return;
        }
        if !self.normalize_pending_source_control_discard_paths() {
            return;
        }
        let Some(target) = self.pending_source_control_discard.as_ref() else {
            return;
        };
        let mut discard = false;
        let mut cancel = false;
        let prompt = discard_dialog_prompt(&target.paths);
        let title = prompt.title();

        egui::Window::new(title)
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 24.0,
            ))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .fixed_size([520.0, 150.0])
            .show(ctx, |ui| {
                match prompt {
                    DiscardDialogPrompt::SingleFile { display_label } => {
                        ui.label(RichText::new(display_label).strong());
                        ui.label("Discard source control changes in this file?");
                    }
                    DiscardDialogPrompt::MultipleFiles { count } => {
                        ui.label(RichText::new(format!("{count} files")).strong());
                        ui.label("Discard all source control changes?");
                    }
                }
                ui.label("This cannot be undone.");

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                }

                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if popup_button(ui, "Cancel", PopupButtonKind::Secondary).clicked() {
                        cancel = true;
                    }
                    if popup_button(ui, "Discard", PopupButtonKind::Danger).clicked() {
                        discard = true;
                    }
                });
            });

        if cancel {
            self.pending_source_control_discard = None;
            self.status = "Discard canceled".to_owned();
        } else if discard {
            self.confirm_source_control_discard();
        }
    }
}

fn normalize_source_control_discard_paths(
    operation_root: &Path,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    let mut paths = paths
        .into_iter()
        .filter_map(|path| normalize_source_control_discard_path(operation_root, &path))
        .collect::<Vec<_>>();
    paths.sort_unstable();
    paths.dedup();
    paths
}

fn normalize_source_control_discard_path(operation_root: &Path, path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        return normalize_child_path(operation_root, path);
    }
    normalize_child_path(Path::new("."), path)?;
    if source_control_discard_path_starts_with_root(path, operation_root) {
        return normalize_root_prefixed_source_control_discard_path(operation_root, path);
    }
    normalize_child_path(operation_root, path)
}

fn normalize_root_prefixed_source_control_discard_path(
    operation_root: &Path,
    path: &Path,
) -> Option<PathBuf> {
    let mut path_components = path.components();
    let mut normalized = PathBuf::new();
    for root_component in source_control_discard_root_components(operation_root) {
        let path_component = next_non_current_dir_component(&mut path_components)?;
        if !source_control_discard_components_match(path_component, root_component) {
            return None;
        }
        normalized.push(root_component.as_os_str());
    }

    let mut child_depth = 0usize;
    for component in path_components {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => {
                normalized.push(part);
                child_depth += 1;
            }
            Component::ParentDir if child_depth == 0 => return None,
            Component::ParentDir => {
                normalized.pop();
                child_depth -= 1;
            }
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    if normalized.as_os_str().is_empty() {
        Some(PathBuf::from("."))
    } else {
        Some(normalized)
    }
}

fn source_control_discard_path_starts_with_root(path: &Path, operation_root: &Path) -> bool {
    let mut path_components = path.components();
    for root_component in source_control_discard_root_components(operation_root) {
        let Some(path_component) = next_non_current_dir_component(&mut path_components) else {
            return false;
        };
        if !source_control_discard_components_match(path_component, root_component) {
            return false;
        }
    }
    true
}

fn source_control_discard_root_components(path: &Path) -> impl Iterator<Item = Component<'_>> {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
}

fn next_non_current_dir_component<'a>(
    components: &mut impl Iterator<Item = Component<'a>>,
) -> Option<Component<'a>> {
    loop {
        match components.next()? {
            Component::CurDir => {}
            component => return Some(component),
        }
    }
}

fn source_control_discard_components_match(left: Component<'_>, right: Component<'_>) -> bool {
    match (left, right) {
        (Component::Prefix(left), Component::Prefix(right)) => {
            source_control_discard_os_str_matches(left.as_os_str(), right.as_os_str())
        }
        (Component::RootDir, Component::RootDir) => true,
        (Component::ParentDir, Component::ParentDir) => true,
        (Component::Normal(left), Component::Normal(right)) => {
            source_control_discard_os_str_matches(left, right)
        }
        _ => false,
    }
}

#[cfg(windows)]
fn source_control_discard_os_str_matches(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> bool {
    left == right
        || left
            .to_string_lossy()
            .chars()
            .flat_map(char::to_lowercase)
            .eq(right.to_string_lossy().chars().flat_map(char::to_lowercase))
}

#[cfg(not(windows))]
fn source_control_discard_os_str_matches(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> bool {
    left == right
}

enum DiscardDialogPrompt {
    SingleFile { display_label: String },
    MultipleFiles { count: usize },
}

impl DiscardDialogPrompt {
    fn title(&self) -> &'static str {
        match self {
            Self::SingleFile { .. } => "Discard Changes",
            Self::MultipleFiles { .. } => "Discard All Changes",
        }
    }
}

fn discard_dialog_prompt(paths: &[PathBuf]) -> DiscardDialogPrompt {
    match paths {
        [path] => DiscardDialogPrompt::SingleFile {
            display_label: discard_file_label(path),
        },
        paths => DiscardDialogPrompt::MultipleFiles { count: paths.len() },
    }
}

fn discard_file_label(path: &Path) -> String {
    discard_file_label_cow(path).into_owned()
}

fn discard_file_label_cow(path: &Path) -> Cow<'_, str> {
    display_path_label_cow(path)
}

#[cfg(test)]
mod tests {
    use crate::{
        path_display::DISPLAY_PATH_LABEL_MAX_CHARS,
        source_control_runtime::{
            source_control_app_for_test, source_control_mutation_restricted_status,
        },
        transient_state::PendingSourceControlDiscard,
    };
    use eframe::egui::Context;
    use std::path::PathBuf;

    use super::{
        DiscardDialogPrompt, NO_SOURCE_CONTROL_DISCARD_TARGETS_STATUS, discard_dialog_prompt,
        discard_file_label, normalize_source_control_discard_paths,
    };

    #[test]
    fn discard_file_label_sanitizes_newline_control_and_bidi_characters() {
        let path = PathBuf::from("workspace").join("line\nname\u{0007}\u{202e}hidden.rs");

        let label = discard_file_label(&path);

        assert_eq!(label, "line name hidden.rs");
    }

    #[test]
    fn discard_dialog_prompt_sanitizes_single_file_label_without_mutating_pending_path() {
        let path = PathBuf::from("workspace").join("line\nname\u{0007}\u{202e}hidden.rs");
        let pending = PendingSourceControlDiscard {
            paths: vec![path.clone()],
        };

        let prompt = discard_dialog_prompt(&pending.paths);

        match prompt {
            DiscardDialogPrompt::SingleFile { display_label } => {
                assert_eq!(display_label, "line name hidden.rs");
            }
            DiscardDialogPrompt::MultipleFiles { count } => {
                panic!("expected single file prompt, got {count} files");
            }
        }
        assert_eq!(pending.paths, vec![path]);
    }

    #[test]
    fn discard_dialog_prompt_uses_count_without_building_file_labels_for_multi_file_discard() {
        let prompt = discard_dialog_prompt(&[
            PathBuf::from("workspace").join("line\nname\u{0007}\u{202e}hidden.rs"),
            PathBuf::from("workspace").join("other.rs"),
        ]);

        match prompt {
            DiscardDialogPrompt::SingleFile { display_label } => {
                panic!("expected multi-file prompt, got {display_label}");
            }
            DiscardDialogPrompt::MultipleFiles { count } => {
                assert_eq!(count, 2);
            }
        }
    }

    #[test]
    fn discard_file_label_bounds_long_labels() {
        let path = PathBuf::from("workspace")
            .join(format!("changed-{}-file.rs", "very-long-name-".repeat(16)));

        let label = discard_file_label(&path);

        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS);
    }

    #[test]
    fn discard_paths_normalize_aliases_dedup_and_drop_out_of_root_targets() {
        let root = PathBuf::from("workspace");
        let main = root.join("src").join("main.rs");
        let readme = root.join("README.md");

        let paths = normalize_source_control_discard_paths(
            &root,
            vec![
                root.join("src").join(".").join("main.rs"),
                root.join("src")
                    .join("generated")
                    .join("..")
                    .join("main.rs"),
                readme.clone(),
                root.join("src").join("..").join("..").join("outside.rs"),
                root.join("bad\nname.rs"),
            ],
        );

        assert_eq!(paths, vec![readme, main]);
    }

    #[test]
    fn pending_discard_paths_are_normalized_before_prompting() {
        let root = PathBuf::from("workspace");
        let main = root.join("src").join("main.rs");
        let readme = root.join("README.md");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.pending_source_control_discard = Some(PendingSourceControlDiscard {
            paths: vec![
                root.join("src").join(".").join("main.rs"),
                root.join("src")
                    .join("generated")
                    .join("..")
                    .join("main.rs"),
                readme.clone(),
            ],
        });

        assert!(app.normalize_pending_source_control_discard_paths());

        let pending = app
            .pending_source_control_discard
            .as_ref()
            .expect("pending discard should remain");
        assert_eq!(pending.paths, vec![readme, main]);
        match discard_dialog_prompt(&pending.paths) {
            DiscardDialogPrompt::SingleFile { display_label } => {
                panic!("expected multi-file prompt, got {display_label}");
            }
            DiscardDialogPrompt::MultipleFiles { count } => {
                assert_eq!(count, 2);
            }
        }
    }

    #[test]
    fn missing_pending_discard_targets_are_cleared_before_prompting() {
        let root = PathBuf::from("workspace");
        let mut app = source_control_app_for_test(root.clone(), true);
        app.pending_source_control_discard = Some(PendingSourceControlDiscard {
            paths: vec![root.join("src").join("..").join("..").join("outside.rs")],
        });

        assert!(!app.normalize_pending_source_control_discard_paths());

        assert!(app.pending_source_control_discard.is_none());
        assert_eq!(app.status, NO_SOURCE_CONTROL_DISCARD_TARGETS_STATUS);
    }

    #[test]
    fn untrusted_workspace_closes_pending_discard_prompt_on_render() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, false);
        app.pending_source_control_discard =
            Some(PendingSourceControlDiscard { paths: vec![path] });

        app.render_source_control_discard(&Context::default());

        assert_eq!(
            app.status,
            source_control_mutation_restricted_status("discarding changes")
        );
        assert!(app.pending_source_control_discard.is_none());
    }

    #[test]
    fn untrusted_workspace_does_not_open_or_confirm_discard_prompts() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, false);

        app.begin_discard_file_changes(path.clone());
        assert_eq!(
            app.status,
            source_control_mutation_restricted_status("discarding changes")
        );
        assert!(app.pending_source_control_discard.is_none());

        app.begin_discard_all_changes();
        assert_eq!(
            app.status,
            source_control_mutation_restricted_status("discarding changes")
        );
        assert!(app.pending_source_control_discard.is_none());

        app.pending_source_control_discard =
            Some(PendingSourceControlDiscard { paths: vec![path] });
        app.confirm_source_control_discard();
        assert_eq!(
            app.status,
            source_control_mutation_restricted_status("discarding changes")
        );
        assert!(app.pending_source_control_discard.is_none());
    }

    #[test]
    fn stale_single_file_discard_does_not_open_prompt() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root.clone(), true);

        app.begin_discard_file_changes(path.clone());

        assert!(app.pending_source_control_discard.is_none());
        assert_eq!(app.status, "No source control changes in main.rs");
        assert_eq!(path, root.join("src/main.rs"));
    }

    #[test]
    fn stale_pending_discard_is_rechecked_before_confirming() {
        let root = PathBuf::from("workspace");
        let path = root.join("src/main.rs");
        let mut app = source_control_app_for_test(root, true);
        app.pending_source_control_discard =
            Some(PendingSourceControlDiscard { paths: vec![path] });

        app.confirm_source_control_discard();

        assert!(app.pending_source_control_discard.is_none());
        assert_eq!(app.status, "No source control changes in main.rs");
    }
}

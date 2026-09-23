use super::*;
use crate::git::{
    MAX_GIT_COMMIT_SHORT_HASH_LENGTH, MAX_GIT_DETECT_SUBMODULES_LIMIT,
    MAX_GIT_SIMILARITY_THRESHOLD, MAX_GIT_STATUS_LIMIT, MIN_GIT_COMMIT_SHORT_HASH_LENGTH,
    MIN_GIT_DETECT_SUBMODULES_LIMIT, MIN_GIT_SIMILARITY_THRESHOLD, MIN_GIT_STATUS_LIMIT,
    clamp_git_commit_short_hash_length, clamp_git_detect_submodules_limit,
    clamp_git_similarity_threshold, clamp_git_status_limit,
};
use crate::{command::Command, keymap::KeyBinding};
use std::fs;

fn temp_settings_path(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "kuroya-settings-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .join(".kuroya")
        .join("settings.toml")
}

fn assert_no_setting_temps(path: &Path) {
    let parent = path.parent().unwrap();
    let temp_count = fs::read_dir(parent)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.contains(".tmp."))
        })
        .count();
    assert_eq!(temp_count, 0);
}

mod discord;
mod editor;
mod git_scm;
mod minimap;
mod parse_load_save_recovery;
mod plugins;
mod terminal;

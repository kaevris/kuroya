use crate::large_file_mode::{
    buffer_uses_large_file_mode, buffer_uses_large_file_performance_mode,
};
use kuroya_core::{
    BufferId, LanguageId, LspServerConfig, PluginLanguageRegistry, TextBuffer,
    lsp_language_id_for_path, server_config_for_language,
};
use std::borrow::Cow;
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) const LSP_DISK_EDIT_MAX_BYTES: u64 = 3 * 1024 * 1024;
pub(crate) const LANGUAGE_SYNC_DEBOUNCE: Duration = Duration::from_millis(180);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundLanguageBlockReason {
    BinaryPreview,
    LossyDecoded,
    LargeFileMode,
    LargeBuffer,
}

impl BackgroundLanguageBlockReason {
    pub(crate) fn folding_status(self) -> &'static str {
        match self {
            Self::BinaryPreview => "Folding is disabled for binary previews",
            Self::LossyDecoded => {
                "Folding is disabled for files decoded with replacement characters"
            }
            Self::LargeFileMode => "Folding is disabled for very large buffers",
            Self::LargeBuffer => "Language folding is paused for this large buffer",
        }
    }

    pub(crate) fn formatting_status(self) -> &'static str {
        match self {
            Self::BinaryPreview => "Formatting is disabled for binary previews",
            Self::LossyDecoded => {
                "Formatting is disabled for files decoded with replacement characters"
            }
            Self::LargeFileMode => "Formatting is disabled in large file mode",
            Self::LargeBuffer => "Formatting is paused for this large buffer",
        }
    }
}

pub(crate) fn due_language_sync_ids(
    pending: &HashMap<BufferId, Instant>,
    now: Instant,
    debounce: Duration,
) -> Vec<BufferId> {
    let mut ids = pending
        .iter()
        .filter_map(|(id, scheduled)| {
            (now.saturating_duration_since(*scheduled) >= debounce).then_some(*id)
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

pub(crate) fn lsp_lifecycle_target_for_buffer(
    buffer: &TextBuffer,
    configs: &[LspServerConfig],
    plugin_languages: &PluginLanguageRegistry,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> Option<(String, PathBuf)> {
    let id = buffer.id();
    if background_language_block_reason(id, buffer, lossy_buffers, binary_buffers).is_some() {
        return None;
    }
    let path = buffer.path()?.clone();
    let (config, _) = lsp_server_config_for_buffer(configs, plugin_languages, buffer)?;
    let key = config.language.clone();
    Some((key, path))
}

pub(crate) fn lsp_lifecycle_targets_for_buffers(
    buffers: &[TextBuffer],
    configs: &[LspServerConfig],
    plugin_languages: &PluginLanguageRegistry,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> Vec<(String, PathBuf)> {
    buffers
        .iter()
        .filter_map(|buffer| {
            lsp_lifecycle_target_for_buffer(
                buffer,
                configs,
                plugin_languages,
                lossy_buffers,
                binary_buffers,
            )
        })
        .collect()
}

/// Resolves the PRIMARY server config for a buffer: the first entry of
/// [`lsp_server_configs_for_buffer`].
pub(crate) fn lsp_server_config_for_buffer<'a>(
    configs: &'a [LspServerConfig],
    plugin_languages: &'a PluginLanguageRegistry,
    buffer: &'a TextBuffer,
) -> Option<(&'a LspServerConfig, Cow<'a, str>)> {
    lsp_server_configs_for_buffer(configs, plugin_languages, buffer)
        .into_iter()
        .next()
}

/// Resolves EVERY configured server matching this buffer, in settings order.
/// A config matches when its language equals the buffer's resolved language
/// id (custom extension matches first, then plugin languages, then the
/// built-in language id); when none match, the buffer's core language id is
/// used. Entries with an identical language + command + args (which would
/// otherwise produce the same client) are deduplicated, keeping the first.
pub(crate) fn lsp_server_configs_for_buffer<'a>(
    configs: &'a [LspServerConfig],
    plugin_languages: &'a PluginLanguageRegistry,
    buffer: &'a TextBuffer,
) -> Vec<(&'a LspServerConfig, Cow<'a, str>)> {
    let language = lsp_language_id_for_buffer(configs, plugin_languages, buffer);
    let mut matches: Vec<(&'a LspServerConfig, Cow<'a, str>)> = configs
        .iter()
        .filter(|config| config.language == language.as_ref())
        .map(|config| (config, language.clone()))
        .collect();
    if matches.is_empty()
        && let Some(primary) = server_config_for_language(configs, buffer.language())
    {
        matches.extend(
            configs
                .iter()
                .filter(|config| config.language == primary.language)
                .map(|config| (config, language.clone())),
        );
    }
    let mut seen_identities: Vec<(&str, &str, &[String])> = Vec::with_capacity(matches.len());
    matches.retain(|(config, _)| {
        let identity = (
            config.language.as_str(),
            config.command.as_str(),
            config.args.as_slice(),
        );
        if seen_identities.contains(&identity) {
            false
        } else {
            seen_identities.push(identity);
            true
        }
    });
    matches
}

pub(crate) fn lsp_language_id_for_buffer<'a>(
    configs: &'a [LspServerConfig],
    plugin_languages: &'a PluginLanguageRegistry,
    buffer: &'a TextBuffer,
) -> Cow<'a, str> {
    if let Some(path) = buffer.path() {
        if let Some(config) = custom_lsp_server_config_for_path(configs, path) {
            return Cow::Borrowed(config.language.as_str());
        }
        if buffer.language() == LanguageId::PlainText
            && let Some(language) = plugin_languages.language_for_path(path)
        {
            return Cow::Borrowed(language.language_id.as_str());
        }
    }

    Cow::Borrowed(lsp_language_id_for_path(
        buffer.language(),
        buffer.path().map(PathBuf::as_path),
    ))
}

fn custom_lsp_server_config_for_path<'a>(
    configs: &'a [LspServerConfig],
    path: &Path,
) -> Option<&'a LspServerConfig> {
    let extension = path.extension()?.to_str()?.trim_start_matches('.');
    if extension.is_empty() {
        return None;
    }
    configs.iter().find(|config| {
        config.extensions.iter().any(|configured| {
            let configured = configured.trim_start_matches('.');
            !configured.is_empty() && configured.eq_ignore_ascii_case(extension)
        })
    })
}

pub(crate) fn open_lsp_workspace_edit_block_reason(
    id: BufferId,
    changed_on_disk: &HashSet<BufferId>,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
    buffers: &[TextBuffer],
) -> Option<&'static str> {
    let buffer = buffers.iter().find(|buffer| buffer.id() == id);
    if binary_buffers.contains(&id) {
        Some("binary preview")
    } else if lossy_buffers.contains(&id) {
        Some("UTF-8 replacement preview")
    } else if buffer.is_some_and(buffer_uses_large_file_mode) {
        Some("large file mode")
    } else if buffer.is_some_and(buffer_uses_large_file_performance_mode) {
        Some("large buffer")
    } else if changed_on_disk.contains(&id)
        && buffer.is_some_and(|buffer| buffer.is_dirty() && buffer.path().is_some())
    {
        Some("changed on disk")
    } else {
        None
    }
}

pub(crate) fn read_lsp_disk_edit_text(path: &Path, max_bytes: u64) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if metadata.len() > max_bytes {
        return Err(format!(
            "file too large for LSP disk edit ({} bytes)",
            metadata.len()
        ));
    }
    let capacity =
        usize::try_from(metadata.len().min(max_bytes.saturating_add(1))).unwrap_or(usize::MAX);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if byte_len > max_bytes {
        return Err(format!(
            "file too large for LSP disk edit ({byte_len} bytes)"
        ));
    }
    if bytes.contains(&0) {
        return Err("binary file skipped".to_owned());
    }
    String::from_utf8(bytes).map_err(|_| "invalid UTF-8 file skipped".to_owned())
}

pub(crate) fn buffer_allows_background_language(buffer: &TextBuffer) -> bool {
    !buffer_uses_large_file_performance_mode(buffer)
}

pub(crate) fn background_language_block_reason(
    id: BufferId,
    buffer: &TextBuffer,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> Option<BackgroundLanguageBlockReason> {
    if binary_buffers.contains(&id) {
        Some(BackgroundLanguageBlockReason::BinaryPreview)
    } else if lossy_buffers.contains(&id) {
        Some(BackgroundLanguageBlockReason::LossyDecoded)
    } else if buffer_uses_large_file_mode(buffer) {
        Some(BackgroundLanguageBlockReason::LargeFileMode)
    } else if !buffer_allows_background_language(buffer) {
        Some(BackgroundLanguageBlockReason::LargeBuffer)
    } else {
        None
    }
}

use crate::{
    KuroyaApp,
    lsp_client::{LspClientHandle, can_use_server_for_path},
    lsp_lifecycle::{background_language_block_reason, lsp_server_configs_for_buffer},
    lsp_text_positions::buffer_position_to_lsp_utf16_column,
    path_display::{display_error_label_cow, display_path_label_cow, sanitized_display_label_cow},
};
use kuroya_core::{
    BufferId, EditorSettings, LanguageId, LspServerConfig, PluginLanguageRegistry, TextBuffer,
    server_config_for_language as core_server_config_for_language,
};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

mod document_sync;
mod watched_files;

pub(crate) const LSP_MAX_RESTART_ATTEMPTS: u8 = 3;
pub(crate) const LSP_RESTART_BASE_DELAY: Duration = Duration::from_millis(250);
pub(crate) const LSP_SYMBOL_REFRESH_DEBOUNCE: Duration = Duration::from_millis(240);
pub(crate) const PENDING_LSP_RESYNC_LIMIT: usize = 256;
pub(crate) const LSP_LANGUAGE_LABEL_MAX_CHARS: usize = 64;
pub(crate) const LSP_STATUS_MESSAGE_MAX_CHARS: usize = 160;
const LSP_METHOD_LABEL_MAX_CHARS: usize = 96;

pub(crate) fn lsp_command_queue_failed_status(method: &str) -> String {
    format!(
        "Could not queue LSP request: {}",
        lsp_method_display_label_cow(method)
    )
}

/// Records a resync request in insertion order. Re-queueing an existing path
/// moves it to the back (it is the newest request); once the limit is
/// reached the OLDEST entry (front) is evicted.
pub(crate) fn record_pending_lsp_resync_path(
    pending: &mut VecDeque<PathBuf>,
    path: PathBuf,
) -> bool {
    if let Some(existing) = pending.iter().position(|existing| *existing == path) {
        pending.remove(existing);
        pending.push_back(path);
        return false;
    }
    while pending.len() >= PENDING_LSP_RESYNC_LIMIT {
        pending.pop_front();
    }
    pending.push_back(path);
    true
}

pub(crate) fn lsp_buffer_synced_status(path: &Path, version: u64) -> String {
    let path = display_path_label_cow(path);
    format!("{} synced with LSP at v{version}", path.as_ref())
}

pub(crate) fn lsp_language_display_label(language: &str) -> String {
    lsp_language_display_label_cow(language).into_owned()
}

pub(crate) fn lsp_status_display_message(message: &str) -> String {
    lsp_status_display_message_cow(message).into_owned()
}

fn lsp_method_display_label_cow(method: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(method, LSP_METHOD_LABEL_MAX_CHARS, "unknown method")
}

fn lsp_language_display_label_cow(language: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(language, LSP_LANGUAGE_LABEL_MAX_CHARS, "Unknown")
}

fn lsp_status_display_message_cow(message: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(message, LSP_STATUS_MESSAGE_MAX_CHARS, "LSP status")
}

pub(crate) fn lsp_read_error_status_message(language: &str, error: &anyhow::Error) -> String {
    let language = lsp_language_display_label_cow(language);
    let error_text = error.to_string();
    let error = display_error_label_cow(&error_text);
    let message = format!("{language} LSP read error: {error}");
    lsp_status_display_message_cow(&message).into_owned()
}

pub(crate) fn lsp_stopped_status_message(language: &str) -> String {
    let language = lsp_language_display_label_cow(language);
    let message = format!("{language} LSP stopped");
    lsp_status_display_message_cow(&message).into_owned()
}

/// Stopped status carrying an optional machine-derived detail (exit code,
/// captured stderr tail). The detail is sanitized and bounded like every
/// other status fragment so hostile servers cannot inject control or bidi
/// formatting characters.
pub(crate) fn lsp_stopped_status_message_with_detail(
    language: &str,
    detail: Option<&str>,
) -> String {
    match detail.map(str::trim).filter(|detail| !detail.is_empty()) {
        Some(detail) => {
            let detail = display_error_label_cow(detail);
            let message = format!(
                "{} LSP stopped ({detail})",
                lsp_language_display_label_cow(language)
            );
            lsp_status_display_message_cow(&message).into_owned()
        }
        None => lsp_stopped_status_message(language),
    }
}

pub(crate) fn lsp_server_ready_status(language: &str) -> String {
    format!("{} LSP ready", lsp_language_display_label_cow(language))
}

pub(crate) fn lsp_stopped_no_buffers_status(language: &str) -> String {
    format!(
        "{} LSP stopped; no open buffers to restart",
        lsp_language_display_label_cow(language)
    )
}

pub(crate) fn lsp_stopped_disabled_status(language: &str) -> String {
    format!(
        "{} LSP stopped repeatedly; restart disabled",
        lsp_language_display_label_cow(language)
    )
}

pub(crate) fn lsp_stopped_restart_scheduled_status(language: &str, reopened: usize) -> String {
    format!(
        "{} LSP stopped; restart scheduled for {reopened} open buffer(s)",
        lsp_language_display_label_cow(language)
    )
}

pub(crate) fn lsp_restart_skipped_restricted_status(language: &str) -> String {
    format!(
        "{} LSP restart skipped; workspace is restricted",
        lsp_language_display_label_cow(language)
    )
}

pub(crate) fn lsp_restart_skipped_no_buffers_status(language: &str) -> String {
    format!(
        "{} LSP restart skipped; no eligible open buffers",
        lsp_language_display_label_cow(language)
    )
}

pub(crate) fn lsp_restart_requested_status(language: &str, reopened: usize) -> String {
    format!(
        "{} LSP restart requested for {reopened} open buffer(s)",
        lsp_language_display_label_cow(language)
    )
}

pub(crate) fn lsp_stopped_workspace_symbol_reason(language: &str) -> String {
    format!("{} LSP stopped", lsp_language_display_label_cow(language))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LspRestartDecision {
    NoEligibleBuffers,
    Restart { attempt: u8 },
    Disable,
}

pub(crate) fn lsp_server_configs_for_settings(settings: &EditorSettings) -> Vec<LspServerConfig> {
    settings.lsp_server_configs()
}

pub(crate) fn lsp_server_config_for_language(
    configs: &[LspServerConfig],
    language: LanguageId,
) -> Option<&LspServerConfig> {
    core_server_config_for_language(configs, language)
}

/// Stable registry key for a resolved server config. When a language has
/// exactly one configured server the key is the language id itself (all
/// restart/unavailable state keeps today's shape); when several servers
/// serve one language the command and args are appended (NUL-separated) so
/// every config gets its own client, restart ladder, and unavailable flag.
pub(crate) fn lsp_client_key(config: &LspServerConfig, configs: &[LspServerConfig]) -> String {
    let shared_language = configs
        .iter()
        .filter(|existing| existing.language == config.language)
        .count()
        > 1;
    if shared_language {
        format!(
            "{}\u{0}{}\u{0}{:?}",
            config.language, config.command, config.args
        )
    } else {
        config.language.clone()
    }
}

/// The language id portion of a client key (plain language keys pass through).
pub(crate) fn lsp_client_key_language(client_key: &str) -> &str {
    client_key.split('\u{0}').next().unwrap_or(client_key)
}

/// Stable bucket key for the diagnostics published by one server instance.
/// The client generation is globally unique per spawned client, so it alone
/// pins the instance (the language keeps the key readable); stored
/// diagnostics can therefore be replaced or purged per server without
/// touching co-attached servers or later restart generations.
pub(crate) fn lsp_diagnostics_source_key(language: &str, generation: u64) -> String {
    format!("{language}\u{0}{generation}")
}

/// The command portion of a client key; empty for plain language keys.
pub(crate) fn lsp_client_key_command(client_key: &str) -> &str {
    client_key.split('\u{0}').nth(1).unwrap_or_default()
}

/// Finds the configured server a client key was derived from. Keys without a
/// command segment (plain language keys, e.g. from long-lived restart state)
/// fall back to the first config for that language.
pub(crate) fn lsp_config_for_client_key<'a>(
    client_key: &str,
    configs: &'a [LspServerConfig],
) -> Option<&'a LspServerConfig> {
    if let Some(config) = configs
        .iter()
        .find(|config| lsp_client_key(config, configs) == client_key)
    {
        return Some(config);
    }
    if lsp_client_key_command(client_key).is_empty() {
        return configs
            .iter()
            .find(|config| config.language == lsp_client_key_language(client_key));
    }
    None
}

/// Display label for status messages naming a client. When multiple servers
/// are configured for the language, the command is appended so the two
/// ladders can be told apart ("rust (rust-analyzer) LSP stopped"); otherwise
/// the label is exactly today's language label.
pub(crate) fn lsp_client_display_label(client_key: &str, configs: &[LspServerConfig]) -> String {
    let language = lsp_client_key_language(client_key);
    let command = lsp_client_key_command(client_key);
    let shared_language = configs
        .iter()
        .filter(|config| config.language == language)
        .count()
        > 1;
    let label = if shared_language && !command.is_empty() {
        format!("{language} ({command})")
    } else {
        language.to_owned()
    };
    lsp_language_display_label_cow(&label).into_owned()
}

impl KuroyaApp {
    /// Resolves the PRIMARY LSP client for a buffer: the first live (or
    /// spawnable) client among the servers matching the buffer, in settings
    /// order. Every interactive request (hover, completion, definition,
    /// formatting, ...) intentionally uses only this primary client so UI
    /// features never merge results across servers; document-sync
    /// notifications fan out to every client instead (see
    /// [`KuroyaApp::ensure_lsp_clients_for_buffer`]).
    pub(crate) fn ensure_lsp_for_buffer(&mut self, id: BufferId) -> Option<LspClientHandle> {
        self.ensure_lsp_clients_for_buffer(id).into_iter().next()
    }

    /// Ensures every configured server matching this buffer has a client and
    /// returns the handles in settings order (primary first). Each config gets
    /// its own client keyed by [`lsp_client_key`]; configs whose client is
    /// unavailable, dead, or not eligible for the buffer path are skipped so
    /// one broken server never blocks the others.
    ///
    /// Spawning a client while its restart ladder is pending supersedes that
    /// ladder: the spawn clears the pending restart and replays the ladder's
    /// didOpen pass for every open buffer the client serves, so the fresh
    /// server process learns about all of them (see the spawn site below).
    pub(crate) fn ensure_lsp_clients_for_buffer(&mut self, id: BufferId) -> Vec<LspClientHandle> {
        if !self.workspace_trusted {
            return Vec::new();
        }

        let lsp_configs = lsp_server_configs_for_settings(&self.settings);
        let Some((path, matched_configs)) = self.buffer(id).and_then(|buffer| {
            if background_language_block_reason(
                id,
                buffer,
                &self.lossy_decoded_buffers,
                &self.binary_preview_buffers,
            )
            .is_some()
            {
                return None;
            }
            let path = buffer.path()?.clone();
            let matched_configs =
                lsp_server_configs_for_buffer(&lsp_configs, &self.plugin_languages, buffer)
                    .into_iter()
                    .map(|(config, _)| config.clone())
                    .collect::<Vec<_>>();
            Some((path, matched_configs))
        }) else {
            return Vec::new();
        };

        let mut handles = Vec::with_capacity(matched_configs.len());
        for config in matched_configs {
            let key = lsp_client_key(&config, &lsp_configs);
            if self.lsp_unavailable.contains(&key) {
                continue;
            }
            if let Some(client) = self.lsp_clients.get(&key) {
                handles.push(client.clone());
                continue;
            }
            if !can_use_server_for_path(&config, &self.workspace.root, &path) {
                continue;
            }
            let handle = LspClientHandle::spawn_on(
                &self.runtime,
                config,
                self.workspace.root.clone(),
                self.tx.clone(),
            );
            let superseded_restart =
                clear_pending_lsp_restart_for_started_client(&mut self.pending_lsp_restarts, &key);
            self.lsp_clients.insert(key.clone(), handle.clone());
            if superseded_restart {
                // This client replaces one that died under a scheduled
                // restart, so the restart ladder's reopen pass runs here
                // instead: a fresh server process starts with no document
                // state and needs didOpen for every open buffer it serves,
                // not just the buffer that triggered the spawn. Without the
                // replay, the keystroke that spawned the client sends only
                // didChange, which servers ignore until didOpen re-opens
                // the document (every buffer of the language stays
                // feature-dead until closed and reopened).
                self.reopen_lsp_buffers_for_client_keys(
                    std::iter::once(key.as_str()),
                    &lsp_configs,
                );
            }
            handles.push(handle);
        }
        handles
    }

    /// The live clients previously spawned for a language (any client key
    /// sharing that language), used to fan out document notifications.
    pub(crate) fn live_lsp_clients_for_language(&self, language: &str) -> Vec<LspClientHandle> {
        self.lsp_clients
            .iter()
            .filter(|(client_key, _)| lsp_client_key_language(client_key) == language)
            .map(|(_, handle)| handle.clone())
            .collect()
    }

    /// Locates the live client (and its registry key) a lifecycle event
    /// belongs to. Events carry the client generation, which uniquely
    /// identifies it; the client key is recovered from the registry so the
    /// restart ladder stays per client.
    pub(crate) fn lsp_client_entry_for_event(
        &self,
        language: &str,
        generation: u64,
    ) -> Option<(String, LspClientHandle)> {
        self.lsp_clients
            .iter()
            .find(|(client_key, handle)| {
                lsp_client_key_language(client_key) == language && handle.generation() == generation
            })
            .map(|(client_key, handle)| (client_key.clone(), handle.clone()))
    }

    pub(crate) fn active_lsp_position(&self) -> Option<(BufferId, PathBuf, u64, usize, usize)> {
        let id = self.active?;
        self.lsp_position_for_buffer(id)
    }

    pub(crate) fn lsp_position_for_buffer(
        &self,
        id: BufferId,
    ) -> Option<(BufferId, PathBuf, u64, usize, usize)> {
        let cursor = self.buffer(id)?.cursor();
        self.lsp_position_for_buffer_char(id, cursor)
    }

    pub(crate) fn lsp_position_for_buffer_char(
        &self,
        id: BufferId,
        char_idx: usize,
    ) -> Option<(BufferId, PathBuf, u64, usize, usize)> {
        let buffer = self.buffer(id)?;
        if background_language_block_reason(
            id,
            buffer,
            &self.lossy_decoded_buffers,
            &self.binary_preview_buffers,
        )
        .is_some()
        {
            return None;
        }
        let path = buffer.path()?.clone();
        let version = buffer.version();
        let position = buffer.char_position(char_idx.min(buffer.len_chars()));
        let character =
            buffer_position_to_lsp_utf16_column(buffer, position.line, position.column)?;
        Some((id, path, version, position.line, character))
    }

    pub(crate) fn flush_pending_lsp_restarts(&mut self) -> usize {
        let client_keys =
            take_due_lsp_restart_languages(&mut self.pending_lsp_restarts, Instant::now());
        let mut restarted = 0usize;
        for client_key in client_keys {
            let client_active = self.lsp_clients.contains_key(&client_key);
            let unavailable = self.lsp_unavailable.contains(&client_key);
            if !pending_lsp_restart_should_run(self.workspace_trusted, client_active, unavailable) {
                if unavailable || !self.workspace_trusted {
                    self.lsp_restart_attempts.remove(&client_key);
                }
                if !self.workspace_trusted {
                    let lsp_configs = lsp_server_configs_for_settings(&self.settings);
                    let label = lsp_client_display_label(&client_key, &lsp_configs);
                    self.status = lsp_restart_skipped_restricted_status(&label);
                }
                continue;
            }

            let lsp_configs = lsp_server_configs_for_settings(&self.settings);
            let label = lsp_client_display_label(&client_key, &lsp_configs);
            let restart_targets = lsp_restart_buffer_ids(
                &client_key,
                &self.buffers,
                &lsp_configs,
                &self.plugin_languages,
                &self.workspace.root,
                &self.lossy_decoded_buffers,
                &self.binary_preview_buffers,
            );
            if restart_targets.is_empty() {
                self.lsp_restart_attempts.remove(&client_key);
                self.status = lsp_restart_skipped_no_buffers_status(&label);
                continue;
            }

            for id in &restart_targets {
                self.notify_lsp_open(*id);
            }
            restarted = restarted.saturating_add(1);
            self.status = lsp_restart_requested_status(&label, restart_targets.len());
        }
        restarted
    }

    pub(crate) fn sync_lsp_server_settings_after_reload(
        &mut self,
        previous_settings: &EditorSettings,
    ) -> usize {
        let previous_configs = lsp_server_configs_for_settings(previous_settings);
        let current_configs = lsp_server_configs_for_settings(&self.settings);
        if previous_configs != current_configs {
            return self.restart_lsp_clients_for_server_config_change(&current_configs);
        }

        if self.lsp_unavailable.is_empty() {
            return 0;
        }

        let unavailable = std::mem::take(&mut self.lsp_unavailable);
        self.lsp_restart_attempts
            .retain(|client_key, _| !unavailable.contains(client_key));
        self.pending_lsp_restarts
            .retain(|client_key, _| !unavailable.contains(client_key));
        self.reopen_lsp_buffers_for_client_keys(
            unavailable.iter().map(String::as_str),
            &current_configs,
        )
    }

    fn restart_lsp_clients_for_server_config_change(
        &mut self,
        configs: &[LspServerConfig],
    ) -> usize {
        for (_, client) in self.lsp_clients.drain() {
            client.shutdown();
        }
        self.lsp_unavailable.clear();
        self.lsp_restart_attempts.clear();
        self.pending_lsp_restarts.clear();
        let client_keys = configs
            .iter()
            .map(|config| lsp_client_key(config, configs))
            .collect::<Vec<_>>();
        self.reopen_lsp_buffers_for_client_keys(client_keys.iter().map(String::as_str), configs)
    }

    fn reopen_lsp_buffers_for_client_keys<'a>(
        &mut self,
        client_keys: impl IntoIterator<Item = &'a str>,
        configs: &[LspServerConfig],
    ) -> usize {
        let mut buffer_ids = Vec::new();
        for client_key in client_keys {
            buffer_ids.extend(lsp_restart_buffer_ids(
                client_key,
                &self.buffers,
                configs,
                &self.plugin_languages,
                &self.workspace.root,
                &self.lossy_decoded_buffers,
                &self.binary_preview_buffers,
            ));
        }
        buffer_ids.sort_unstable();
        buffer_ids.dedup();

        let reopened = buffer_ids.len();
        for id in buffer_ids {
            self.notify_lsp_open(id);
        }
        reopened
    }
}

pub(crate) fn pending_lsp_restart_should_run(
    workspace_trusted: bool,
    client_active: bool,
    unavailable: bool,
) -> bool {
    workspace_trusted && !client_active && !unavailable
}

pub(crate) fn clear_pending_lsp_restart_for_started_client(
    pending: &mut HashMap<String, Instant>,
    language: &str,
) -> bool {
    pending.remove(language).is_some()
}

pub(crate) fn lsp_restart_decision(
    current_attempts: Option<u8>,
    eligible_buffer_count: usize,
    max_attempts: u8,
) -> LspRestartDecision {
    if eligible_buffer_count == 0 {
        return LspRestartDecision::NoEligibleBuffers;
    }

    let attempt = current_attempts.unwrap_or_default().saturating_add(1);
    if attempt > max_attempts {
        LspRestartDecision::Disable
    } else {
        LspRestartDecision::Restart { attempt }
    }
}

/// Buffers eligible for a restart of the server identified by `client_key`:
/// buffers whose resolved server set contains that key's config and whose
/// path the server accepts. Plain language keys (legacy restart state)
/// restart any server for the language.
pub(crate) fn lsp_restart_buffer_ids(
    client_key: &str,
    buffers: &[TextBuffer],
    configs: &[LspServerConfig],
    plugin_languages: &PluginLanguageRegistry,
    root: &Path,
    lossy_buffers: &HashSet<BufferId>,
    binary_buffers: &HashSet<BufferId>,
) -> Vec<BufferId> {
    let key_config = lsp_config_for_client_key(client_key, configs);
    let key_language = lsp_client_key_language(client_key);
    buffers
        .iter()
        .filter_map(|buffer| {
            let id = buffer.id();
            if background_language_block_reason(id, buffer, lossy_buffers, binary_buffers).is_some()
            {
                return None;
            }

            let matched = lsp_server_configs_for_buffer(configs, plugin_languages, buffer);
            let config = match key_config {
                Some(key_config) if matched.iter().any(|(config, _)| *config == key_config) => {
                    key_config
                }
                Some(_) => return None,
                None => matched
                    .iter()
                    .find(|(config, _)| config.language == key_language)
                    .map(|(config, _)| *config)?,
            };

            let path = buffer.path()?;
            can_use_server_for_path(config, root, path).then_some(id)
        })
        .collect()
}

pub(crate) fn schedule_lsp_restart_at(now: Instant, attempt: u8) -> Instant {
    now + lsp_restart_delay(attempt, LSP_RESTART_BASE_DELAY)
}

pub(crate) fn lsp_restart_delay(attempt: u8, base: Duration) -> Duration {
    let exponent = attempt.saturating_sub(1).min(4);
    let multiplier = 1u32 << exponent;
    base.checked_mul(multiplier).unwrap_or(base)
}

#[cfg(test)]
pub(crate) fn due_lsp_restart_languages(
    pending: &HashMap<String, Instant>,
    now: Instant,
) -> Vec<String> {
    let mut languages = Vec::with_capacity(pending.len());
    languages.extend(
        pending
            .iter()
            .filter_map(|(language, due)| (*due <= now).then_some(language.clone())),
    );
    languages.sort();
    languages
}

pub(crate) fn take_due_lsp_restart_languages(
    pending: &mut HashMap<String, Instant>,
    now: Instant,
) -> Vec<String> {
    let mut languages = Vec::with_capacity(pending.len());
    pending.retain(|language, due| {
        if *due <= now {
            languages.push(language.clone());
            false
        } else {
            true
        }
    });
    languages.sort();
    languages
}

#[cfg(test)]
pub(crate) fn due_lsp_symbol_refresh_ids(
    pending: &HashMap<BufferId, Instant>,
    now: Instant,
    debounce: Duration,
) -> Vec<BufferId> {
    let mut ids = Vec::with_capacity(pending.len());
    ids.extend(pending.iter().filter_map(|(id, scheduled)| {
        (now.saturating_duration_since(*scheduled) >= debounce).then_some(*id)
    }));
    ids.sort_unstable();
    ids
}

pub(crate) fn take_due_lsp_symbol_refresh_ids(
    pending: &mut HashMap<BufferId, Instant>,
    now: Instant,
    debounce: Duration,
) -> Vec<BufferId> {
    let mut ids = Vec::with_capacity(pending.len());
    pending.retain(|id, scheduled| {
        if now.saturating_duration_since(*scheduled) >= debounce {
            ids.push(*id);
            false
        } else {
            true
        }
    });
    ids.sort_unstable();
    ids
}

#[cfg(test)]
mod tests {
    use super::{
        LSP_LANGUAGE_LABEL_MAX_CHARS, LSP_METHOD_LABEL_MAX_CHARS, LSP_STATUS_MESSAGE_MAX_CHARS,
        PENDING_LSP_RESYNC_LIMIT, buffer_position_to_lsp_utf16_column, due_lsp_restart_languages,
        lsp_client_display_label, lsp_client_key, lsp_client_key_command, lsp_client_key_language,
        lsp_command_queue_failed_status, lsp_config_for_client_key, lsp_language_display_label,
        lsp_language_display_label_cow, lsp_method_display_label_cow,
        lsp_read_error_status_message, lsp_restart_requested_status,
        lsp_restart_skipped_no_buffers_status, lsp_restart_skipped_restricted_status,
        lsp_server_config_for_language, lsp_status_display_message, lsp_status_display_message_cow,
        lsp_stopped_disabled_status, lsp_stopped_no_buffers_status,
        lsp_stopped_restart_scheduled_status, lsp_stopped_status_message,
        lsp_stopped_status_message_with_detail, record_pending_lsp_resync_path,
        take_due_lsp_restart_languages, take_due_lsp_symbol_refresh_ids,
    };
    use crate::path_display::sanitized_display_label;
    use kuroya_core::{EditorSettings, LanguageId, LspServerConfig, TextBuffer};
    use std::{
        borrow::Cow,
        collections::{HashMap, HashSet, VecDeque},
        path::PathBuf,
        time::{Duration, Instant},
    };

    #[test]
    fn lsp_cursor_positions_use_utf16_columns() {
        let mut buffer = TextBuffer::from_text(1, None, "😀alpha".to_owned());
        buffer.set_single_cursor(buffer.line_column_to_char(0, 1));
        let position = buffer.cursor_position();

        assert_eq!(
            buffer_position_to_lsp_utf16_column(&buffer, position.line, position.column),
            Some(2)
        );
    }

    #[test]
    fn lsp_status_messages_sanitize_language_error_and_method_labels() {
        let language = format!(
            "rust\n{}\u{202e}",
            "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS)
        );
        let error = anyhow::anyhow!(
            "first line\nsecond line \u{2066}{}",
            "error-fragment-".repeat(LSP_STATUS_MESSAGE_MAX_CHARS)
        );
        let method = format!(
            "textDocument\n{}\u{202e}",
            "method-fragment-".repeat(LSP_METHOD_LABEL_MAX_CHARS)
        );

        let read_error = lsp_read_error_status_message(&language, &error);
        let stopped = lsp_stopped_status_message(&language);
        let queue_failed = lsp_command_queue_failed_status(&method);

        assert_display_safe(&read_error);
        assert_display_safe(&stopped);
        assert_display_safe(&queue_failed);
        assert!(read_error.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
        assert!(stopped.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
        assert!(
            queue_failed.chars().count()
                <= "Could not queue LSP request: ".chars().count() + LSP_METHOD_LABEL_MAX_CHARS
        );
        assert!(read_error.contains("..."));
        assert!(queue_failed.contains("..."));
    }

    #[test]
    fn lsp_display_label_cow_helpers_borrow_clean_ascii_and_unicode() {
        assert!(matches!(
            lsp_method_display_label_cow("textDocument/hover"),
            Cow::Borrowed("textDocument/hover")
        ));
        assert!(matches!(
            lsp_language_display_label_cow("rust"),
            Cow::Borrowed("rust")
        ));
        assert!(matches!(
            lsp_status_display_message_cow("Rust LSP ready"),
            Cow::Borrowed("Rust LSP ready")
        ));

        let method = "workspace/\u{03bb}";
        let language = "rust-\u{03bb}";
        let status = "Rust \u{03bb} LSP ready";

        match lsp_method_display_label_cow(method) {
            Cow::Borrowed(label) => assert_eq!(label, method),
            Cow::Owned(label) => panic!("expected borrowed method label, got {label:?}"),
        }
        match lsp_language_display_label_cow(language) {
            Cow::Borrowed(label) => assert_eq!(label, language),
            Cow::Owned(label) => panic!("expected borrowed language label, got {label:?}"),
        }
        match lsp_status_display_message_cow(status) {
            Cow::Borrowed(label) => assert_eq!(label, status),
            Cow::Owned(label) => panic!("expected borrowed status label, got {label:?}"),
        }
    }

    #[test]
    fn lsp_display_label_cow_helpers_own_dirty_truncated_and_fallback_labels() {
        let dirty_method = lsp_method_display_label_cow("textDocument\n\u{202e}hover");
        let blank_method = lsp_method_display_label_cow("\n\u{202e}\t");
        let blank_language = lsp_language_display_label_cow("\n\u{202e}\t");
        let blank_status = lsp_status_display_message_cow("\n\u{202e}\t");
        let long_language = "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS);
        let truncated_language = lsp_language_display_label_cow(&long_language);

        assert_eq!(dirty_method.as_ref(), "textDocument hover");
        assert_eq!(blank_method.as_ref(), "unknown method");
        assert_eq!(blank_language.as_ref(), "Unknown");
        assert_eq!(blank_status.as_ref(), "LSP status");
        assert_display_safe(&dirty_method);
        assert!(truncated_language.contains("..."), "{truncated_language}");
        assert!(truncated_language.chars().count() <= LSP_LANGUAGE_LABEL_MAX_CHARS);

        assert!(matches!(dirty_method, Cow::Owned(_)));
        assert!(matches!(blank_method, Cow::Owned(_)));
        assert!(matches!(blank_language, Cow::Owned(_)));
        assert!(matches!(blank_status, Cow::Owned(_)));
        assert!(matches!(truncated_language, Cow::Owned(_)));
    }

    #[test]
    fn lsp_display_label_cow_helpers_match_wrappers_and_status_output() {
        for language in ["rust", "rust\n\u{202e}", "\n\u{202e}\t"] {
            let expected =
                sanitized_display_label(language, LSP_LANGUAGE_LABEL_MAX_CHARS, "Unknown");

            assert_eq!(lsp_language_display_label_cow(language).as_ref(), expected);
            assert_eq!(lsp_language_display_label(language), expected);
        }

        let long_language = "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS);
        let expected_language =
            sanitized_display_label(&long_language, LSP_LANGUAGE_LABEL_MAX_CHARS, "Unknown");
        assert_eq!(
            lsp_language_display_label_cow(&long_language).as_ref(),
            expected_language
        );
        assert_eq!(
            lsp_language_display_label(&long_language),
            expected_language
        );

        for message in ["Rust LSP ready", "Rust\n\u{202e}LSP ready", "\n\u{202e}\t"] {
            let expected =
                sanitized_display_label(message, LSP_STATUS_MESSAGE_MAX_CHARS, "LSP status");

            assert_eq!(lsp_status_display_message_cow(message).as_ref(), expected);
            assert_eq!(lsp_status_display_message(message), expected);
        }

        for method in [
            "textDocument/hover",
            "textDocument\n\u{202e}hover",
            "\n\u{202e}\t",
        ] {
            assert_eq!(
                lsp_method_display_label_cow(method).as_ref(),
                sanitized_display_label(method, LSP_METHOD_LABEL_MAX_CHARS, "unknown method")
            );
            assert_eq!(
                lsp_command_queue_failed_status(method),
                format!(
                    "Could not queue LSP request: {}",
                    sanitized_display_label(method, LSP_METHOD_LABEL_MAX_CHARS, "unknown method")
                )
            );
        }

        let language = "rust\n\u{202e}";
        let language_label =
            sanitized_display_label(language, LSP_LANGUAGE_LABEL_MAX_CHARS, "Unknown");
        assert_eq!(
            lsp_stopped_status_message(language),
            sanitized_display_label(
                &format!("{language_label} LSP stopped"),
                LSP_STATUS_MESSAGE_MAX_CHARS,
                "LSP status"
            )
        );
    }

    #[test]
    fn lsp_restart_statuses_sanitize_overlong_language_labels() {
        let language = format!(
            "rust\n{}\u{202e}",
            "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS)
        );
        let statuses = [
            lsp_stopped_no_buffers_status(&language),
            lsp_stopped_disabled_status(&language),
            lsp_stopped_restart_scheduled_status(&language, 3),
            lsp_restart_skipped_restricted_status(&language),
            lsp_restart_skipped_no_buffers_status(&language),
            lsp_restart_requested_status(&language, 3),
        ];

        for status in statuses {
            assert_display_safe(&status);
            assert!(status.contains("..."));
            assert!(status.chars().count() <= LSP_LANGUAGE_LABEL_MAX_CHARS + 58);
        }
    }

    #[test]
    fn stopped_status_with_detail_sanitizes_and_bounds_the_detail() {
        let language = format!(
            "rust\n{}\u{202e}",
            "language-fragment-".repeat(LSP_LANGUAGE_LABEL_MAX_CHARS)
        );
        let dirty_detail = format!(
            "exit code 1\nsecond line \u{2066}{}",
            "detail-fragment-".repeat(40)
        );

        let with_detail = lsp_stopped_status_message_with_detail(&language, Some(&dirty_detail));
        let blank_detail = lsp_stopped_status_message_with_detail(&language, Some("  \n "));
        let no_detail = lsp_stopped_status_message_with_detail(&language, None);

        assert!(with_detail.contains("LSP stopped ("), "{with_detail}");
        assert_display_safe(&with_detail);
        assert!(with_detail.chars().count() <= LSP_STATUS_MESSAGE_MAX_CHARS);
        assert_eq!(blank_detail, no_detail);
        assert!(no_detail.ends_with("LSP stopped"));
        assert_display_safe(&no_detail);
    }

    #[test]
    fn due_lsp_restart_languages_preserves_raw_restart_keys() {
        let now = Instant::now();
        let raw_language = "rust\n\u{202e}".to_owned();
        let pending = HashMap::from([(raw_language.clone(), now)]);

        assert_eq!(due_lsp_restart_languages(&pending, now), vec![raw_language]);
    }

    #[test]
    fn take_due_lsp_restart_languages_removes_only_ready_entries() {
        let now = Instant::now();
        let later = now + Duration::from_millis(50);
        let mut pending = HashMap::from([
            ("python".to_owned(), later),
            ("rust".to_owned(), now),
            ("go".to_owned(), now - Duration::from_millis(1)),
        ]);

        assert_eq!(
            take_due_lsp_restart_languages(&mut pending, now),
            vec!["go".to_owned(), "rust".to_owned()]
        );
        assert_eq!(
            pending.keys().cloned().collect::<HashSet<_>>(),
            HashSet::from(["python".to_owned()])
        );
    }

    #[test]
    fn lsp_server_config_lookup_uses_effective_settings_configs() {
        let mut settings = EditorSettings::default();
        settings.lsp_servers.push(LspServerConfig {
            language: "go".to_owned(),
            command: "gopls".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: vec!["go.mod".to_owned()],
            enabled: true,
        });
        let configs = settings.lsp_server_configs();
        let rust = lsp_server_config_for_language(&configs, LanguageId::Rust).expect("rust config");
        let go = lsp_server_config_for_language(&configs, LanguageId::Go).expect("go config");

        assert_eq!(rust.language, "rust");
        assert_eq!(go.command, "gopls");
        assert!(lsp_server_config_for_language(&configs, LanguageId::PlainText).is_none());
        assert!(lsp_server_config_for_language(&configs, LanguageId::Diff).is_none());
    }

    #[test]
    fn lsp_client_keys_stay_plain_for_single_server_and_disambiguate_multiples() {
        let mut configs = EditorSettings::default().lsp_server_configs();
        let rust = configs
            .iter()
            .find(|config| config.language == "rust")
            .expect("default rust config")
            .clone();

        // Single server per language keeps today's plain language key.
        assert_eq!(lsp_client_key(&rust, &configs), "rust");
        assert_eq!(lsp_client_key_language("rust"), "rust");
        assert_eq!(lsp_client_key_command("rust"), "");

        configs.push(LspServerConfig {
            language: "rust".to_owned(),
            command: "rust-analyzer-obsidian".to_owned(),
            args: vec!["--stdio".to_owned()],
            extensions: Vec::new(),
            root_markers: Vec::new(),
            enabled: true,
        });
        let primary_key = lsp_client_key(&rust, &configs);
        let secondary_key = lsp_client_key(configs.last().expect("appended config"), &configs);

        assert_eq!(primary_key, "rust\u{0}rust-analyzer\u{0}[]");
        assert_eq!(
            secondary_key,
            "rust\u{0}rust-analyzer-obsidian\u{0}[\"--stdio\"]"
        );
        assert_eq!(lsp_client_key_language(&primary_key), "rust");
        assert_eq!(lsp_client_key_command(&primary_key), "rust-analyzer");

        // Config lookup round-trips, and plain language keys fall back to the
        // first config for the language.
        assert_eq!(
            lsp_config_for_client_key(&secondary_key, &configs)
                .map(|config| config.command.as_str()),
            Some("rust-analyzer-obsidian")
        );
        assert_eq!(
            lsp_config_for_client_key("rust", &configs).map(|config| config.command.as_str()),
            Some("rust-analyzer")
        );
    }

    #[test]
    fn lsp_client_display_labels_append_command_only_for_shared_languages() {
        let mut configs = EditorSettings::default().lsp_server_configs();
        let rust = configs
            .iter()
            .find(|config| config.language == "rust")
            .expect("default rust config")
            .clone();

        assert_eq!(lsp_client_display_label("rust", &configs), "rust");
        assert_eq!(
            lsp_client_display_label("rust LSP ready", &configs),
            "rust LSP ready"
        );

        configs.push(LspServerConfig {
            language: "rust".to_owned(),
            command: "rust-analyzer-obsidian".to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
            enabled: true,
        });
        let key = lsp_client_key(&rust, &configs);

        assert_eq!(
            lsp_client_display_label(&key, &configs),
            "rust (rust-analyzer)"
        );
        // Plain keys keep the plain label even when siblings exist.
        assert_eq!(lsp_client_display_label("rust", &configs), "rust");
    }

    #[test]
    fn take_due_lsp_symbol_refresh_ids_drains_ready_ids() {
        let now = Instant::now();
        let debounce = Duration::from_millis(25);
        let mut pending = HashMap::from([
            (9, now),
            (3, now - debounce),
            (7, now - debounce - Duration::from_millis(1)),
        ]);

        assert_eq!(
            take_due_lsp_symbol_refresh_ids(&mut pending, now, debounce),
            vec![3, 7]
        );
        assert_eq!(
            pending.keys().copied().collect::<HashSet<_>>(),
            HashSet::from([9])
        );
    }

    #[test]
    fn pending_lsp_resync_recording_stays_bounded() {
        let mut pending = VecDeque::new();
        for index in 0..PENDING_LSP_RESYNC_LIMIT * 2 {
            record_pending_lsp_resync_path(&mut pending, PathBuf::from(format!("src/{index}.rs")));
        }

        assert_eq!(pending.len(), PENDING_LSP_RESYNC_LIMIT);
        assert!(pending.contains(&PathBuf::from(format!(
            "src/{}.rs",
            PENDING_LSP_RESYNC_LIMIT * 2 - 1
        ))));

        assert!(record_pending_lsp_resync_path(
            &mut pending,
            PathBuf::from("src/newest.rs")
        ));
        assert_eq!(pending.len(), PENDING_LSP_RESYNC_LIMIT);
        assert!(pending.contains(&PathBuf::from("src/newest.rs")));
    }

    #[test]
    fn pending_lsp_resync_evicts_the_oldest_paths_first() {
        let mut pending = VecDeque::new();
        let inserted = 300usize;
        for index in 0..inserted {
            record_pending_lsp_resync_path(&mut pending, PathBuf::from(format!("src/{index}.rs")));
        }

        assert_eq!(pending.len(), PENDING_LSP_RESYNC_LIMIT);
        let evicted = inserted - PENDING_LSP_RESYNC_LIMIT;
        for index in 0..evicted {
            assert!(
                !pending.contains(&PathBuf::from(format!("src/{index}.rs"))),
                "src/{index}.rs should have been evicted"
            );
        }
        for index in evicted..inserted {
            assert!(pending.contains(&PathBuf::from(format!("src/{index}.rs"))));
        }
        // Oldest survivor is at the front, newest at the back.
        assert_eq!(
            pending.front(),
            Some(&PathBuf::from(format!("src/{evicted}.rs")))
        );
        assert_eq!(
            pending.back(),
            Some(&PathBuf::from(format!("src/{}.rs", inserted - 1)))
        );
    }

    #[test]
    fn pending_lsp_resync_requeue_moves_existing_path_to_the_back() {
        let mut pending = VecDeque::new();
        for index in 0..PENDING_LSP_RESYNC_LIMIT {
            record_pending_lsp_resync_path(&mut pending, PathBuf::from(format!("src/{index}.rs")));
        }
        let oldest = PathBuf::from("src/0.rs");

        assert!(!record_pending_lsp_resync_path(
            &mut pending,
            oldest.clone()
        ));

        assert_eq!(pending.len(), PENDING_LSP_RESYNC_LIMIT);
        assert_eq!(pending.back(), Some(&oldest));
        assert_ne!(pending.front(), Some(&oldest));
    }

    fn assert_display_safe(value: &str) {
        assert!(!value.chars().any(char::is_control), "{value:?}");
        assert!(!value.chars().any(is_bidi_format_control), "{value:?}");
    }

    fn is_bidi_format_control(ch: char) -> bool {
        matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
    }
}

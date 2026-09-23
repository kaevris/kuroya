use crate::{
    syntax_cache::{
        CHECKPOINT_INTERVAL, HighlightCache, HighlightCacheKey, MAX_HIGHLIGHT_CACHES,
        MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE,
    },
    syntax_layout::{
        SyntaxThemeColors, advance_highlight_state, highlighted_job, normalize_layout_inputs,
        plain_job,
    },
};
use anyhow::Context;
use egui::{Color32, text::LayoutJob};
use kuroya_core::{
    LanguageId, MAX_PLUGIN_SYNTAX_BYTES, PluginDescriptor, PluginDiscoveryError,
    PluginSyntaxRegistration, PluginSyntaxRegistry, TextBuffer,
    editor_stop_rendering_line_after_limit, read_plugin_text_file_with_limit,
};
use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    ops::Range,
    path::Path,
};
use syntect::{
    highlighting::{Highlighter, Theme, ThemeSet},
    parsing::{SyntaxDefinition, SyntaxReference, SyntaxSet},
};

pub(crate) const MAX_HIGHLIGHT_REPLAY_LINES_PER_LAYOUT: usize = CHECKPOINT_INTERVAL * 8;

#[derive(Debug)]
pub(crate) struct PluginSyntaxLoad {
    pub(crate) syntax_set: SyntaxSet,
    pub(crate) registry: PluginSyntaxRegistry,
    pub(crate) errors: Vec<PluginDiscoveryError>,
}

impl PluginSyntaxLoad {
    pub(crate) fn from_plugins(plugins: &[PluginDescriptor]) -> Self {
        let declared = PluginSyntaxRegistry::from_plugins(plugins);
        let mut builder = default_syntax_set().into_builder();
        let mut loaded_registrations = Vec::new();
        let mut errors = Vec::new();

        for registration in declared.syntaxes() {
            match load_plugin_syntax_definition(registration) {
                Ok(syntax) => {
                    builder.add(syntax);
                    loaded_registrations.push(registration.clone());
                }
                Err(error) => errors.push(PluginDiscoveryError {
                    root: registration.path.clone(),
                    error: format!(
                        "could not load syntax {} from {}: {error}",
                        registration.language_id,
                        registration.path.display()
                    ),
                }),
            }
        }

        Self {
            syntax_set: builder.build(),
            registry: PluginSyntaxRegistry::from_registrations(loaded_registrations),
            errors,
        }
    }

    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            syntax_set: default_syntax_set(),
            registry: PluginSyntaxRegistry::default(),
            errors: Vec::new(),
        }
    }
}

pub struct SyntaxHighlighter {
    syntaxes: SyntaxSet,
    theme: Theme,
    caches: HashMap<HighlightCacheKey, HighlightCache>,
    cache_order: VecDeque<HighlightCacheKey>,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        let themes = ThemeSet::load_defaults();
        let theme = themes
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .or_else(|| themes.themes.values().next().cloned())
            .unwrap_or_default();
        Self {
            syntaxes: default_syntax_set(),
            theme,
            caches: HashMap::new(),
            cache_order: VecDeque::new(),
        }
    }

    pub(crate) fn install_plugin_syntaxes(&mut self, syntax_load: PluginSyntaxLoad) {
        self.syntaxes = syntax_load.syntax_set;
        self.clear_caches();
    }

    pub(crate) fn reset_plugin_syntaxes(&mut self) {
        self.syntaxes = default_syntax_set();
        self.clear_caches();
    }

    pub(crate) fn layout_visible(
        &mut self,
        buffer: &TextBuffer,
        font_size: f32,
        tab_width: usize,
        rows: Range<usize>,
        syntax_highlighting: bool,
        text_color: Color32,
        stop_rendering_line_after: i64,
    ) -> Vec<LayoutJob> {
        let rows = visible_rows(buffer, rows);
        if rows.start >= rows.end {
            return Vec::new();
        }

        let line_char_limit = editor_stop_rendering_line_after_limit(stop_rendering_line_after);

        if !syntax_highlighting {
            self.clear_caches_for_buffer(buffer.id());
            return plain_visible(
                buffer,
                font_size,
                tab_width,
                rows,
                text_color,
                line_char_limit,
            );
        }

        if rows.end.saturating_sub(rows.start) > MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE {
            return plain_visible(
                buffer,
                font_size,
                tab_width,
                rows,
                text_color,
                line_char_limit,
            );
        }

        let (font_size, tab_width) = normalize_layout_inputs(font_size, tab_width);
        let theme_colors = SyntaxThemeColors::from_text_color(text_color);
        let syntax_extension = syntax_extension_for_buffer(buffer);
        let key = HighlightCacheKey::for_buffer_with_extension(
            buffer,
            font_size,
            tab_width,
            syntax_extension.as_ref(),
            line_char_limit,
            text_color,
        );

        let end = rows.end.min(buffer.len_lines());
        let start = rows.start.min(end);
        let cached_jobs = self
            .caches
            .get_mut(&key)
            .and_then(|cache| cache.visible_layout_jobs(start..end));
        if let Some(jobs) = cached_jobs {
            self.prepare_highlight_cache_key(&key);
            return jobs;
        }

        let is_plain_text_syntax =
            syntax_for_extension(&self.syntaxes, syntax_extension.as_ref()).name == "Plain Text";
        if is_plain_text_syntax {
            self.clear_caches_for_buffer(buffer.id());
            return plain_visible(
                buffer,
                font_size,
                tab_width,
                rows,
                text_color,
                line_char_limit,
            );
        }

        self.prepare_highlight_cache_key(&key);

        let syntax = syntax_for_extension(&self.syntaxes, syntax_extension.as_ref());
        let highlighter = Highlighter::new(&self.theme);
        let cache = self
            .caches
            .entry(key.clone())
            .or_insert_with(|| HighlightCache::new(syntax, &highlighter));

        let (checkpoint_line, checkpoint) =
            cache.checkpoint_before_or_at(start, syntax, &highlighter);
        if start.saturating_sub(checkpoint_line) > MAX_HIGHLIGHT_REPLAY_LINES_PER_LAYOUT {
            warm_highlight_cache(
                cache,
                buffer,
                checkpoint_line,
                checkpoint,
                start,
                line_char_limit,
                &highlighter,
                &self.syntaxes,
            );
            return plain_visible(
                buffer,
                font_size,
                tab_width,
                start..end,
                text_color,
                line_char_limit,
            );
        }

        let mut parse_state = checkpoint.parse_state;
        let mut highlight_state = checkpoint.highlight_state;
        let mut jobs = Vec::with_capacity(end.saturating_sub(start));

        for line_idx in checkpoint_line..end {
            let text = visible_line_text(buffer, line_idx, line_char_limit);
            let job = match parse_state.parse_line(&text, &self.syntaxes) {
                Ok(ops) => {
                    if line_idx < start {
                        advance_highlight_state(&text, &ops, &mut highlight_state, &highlighter);
                        None
                    } else {
                        Some(highlighted_job(
                            &text,
                            &ops,
                            &mut highlight_state,
                            &highlighter,
                            font_size,
                            tab_width,
                            theme_colors,
                        ))
                    }
                }
                Err(_) => {
                    return plain_visible(
                        buffer,
                        font_size,
                        tab_width,
                        start..end,
                        text_color,
                        line_char_limit,
                    );
                }
            };

            if line_idx + 1 < end && (line_idx + 1) % CHECKPOINT_INTERVAL == 0 {
                cache.insert_checkpoint(line_idx + 1, &parse_state, &highlight_state);
            }

            if let Some(job) = job {
                jobs.push(job);
            }
        }

        cache.insert_visible_layout_jobs(start..end, jobs.clone());
        jobs
    }

    fn prepare_highlight_cache_key(&mut self, key: &HighlightCacheKey) {
        if MAX_HIGHLIGHT_CACHES == 0 {
            self.clear_caches();
            return;
        }

        self.remove_stale_cache_entries_for_buffer(key);

        let existing = self.caches.contains_key(key);
        if existing && self.cache_order.back() == Some(key) {
            return;
        }
        self.cache_order.retain(|cached| cached != key);

        if !existing {
            while self.caches.len() >= MAX_HIGHLIGHT_CACHES {
                let Some(evicted) = self.cache_order.pop_front() else {
                    self.clear_caches();
                    break;
                };
                self.caches.remove(&evicted);
            }
        }

        self.cache_order.push_back(key.clone());
    }

    fn remove_stale_cache_entries_for_buffer(&mut self, key: &HighlightCacheKey) {
        let mut stale_keys = Vec::new();
        self.cache_order.retain(|cached| {
            let stale = cached.is_stale_parse_state_for_same_buffer(key);
            if stale {
                stale_keys.push(cached.clone());
            }
            !stale
        });
        for stale_key in stale_keys {
            self.caches.remove(&stale_key);
        }
    }

    fn clear_caches(&mut self) {
        self.caches.clear();
        self.cache_order.clear();
    }

    fn clear_caches_for_buffer(&mut self, buffer_id: u64) {
        self.caches
            .retain(|key, _| !key.is_for_buffer_id(buffer_id));
        self.cache_order
            .retain(|key| !key.is_for_buffer_id(buffer_id));
    }

    #[cfg(test)]
    fn syntax_for_buffer(&self, buffer: &TextBuffer) -> &SyntaxReference {
        let syntax_extension = syntax_extension_for_buffer(buffer);
        syntax_for_extension(&self.syntaxes, syntax_extension.as_ref())
    }

    #[cfg(test)]
    pub(crate) fn syntax_name_for_buffer(&self, buffer: &TextBuffer) -> &str {
        &self.syntax_for_buffer(buffer).name
    }
}

fn warm_highlight_cache(
    cache: &mut HighlightCache,
    buffer: &TextBuffer,
    checkpoint_line: usize,
    checkpoint: crate::syntax_cache::HighlightCheckpoint,
    target_line: usize,
    line_char_limit: Option<usize>,
    highlighter: &Highlighter<'_>,
    syntaxes: &SyntaxSet,
) {
    let warm_end = checkpoint_line
        .saturating_add(MAX_HIGHLIGHT_REPLAY_LINES_PER_LAYOUT)
        .min(target_line)
        .min(buffer.len_lines());
    if warm_end <= checkpoint_line {
        return;
    }

    let mut parse_state = checkpoint.parse_state;
    let mut highlight_state = checkpoint.highlight_state;
    let mut warmed_until = checkpoint_line;
    for line_idx in checkpoint_line..warm_end {
        let text = visible_line_text(buffer, line_idx, line_char_limit);
        if let Ok(ops) = parse_state.parse_line(&text, syntaxes) {
            advance_highlight_state(&text, &ops, &mut highlight_state, highlighter);
        } else {
            break;
        }

        let next_line = line_idx + 1;
        warmed_until = next_line;
        if next_line % CHECKPOINT_INTERVAL == 0 {
            cache.insert_checkpoint(next_line, &parse_state, &highlight_state);
        }
    }

    if warmed_until > checkpoint_line {
        cache.insert_checkpoint(warmed_until, &parse_state, &highlight_state);
    }
}

fn default_syntax_set() -> SyntaxSet {
    SyntaxSet::load_defaults_newlines()
}

fn syntax_for_extension<'a>(
    syntaxes: &'a SyntaxSet,
    syntax_extension: &str,
) -> &'a SyntaxReference {
    syntaxes
        .find_syntax_by_extension(syntax_extension)
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text())
}

fn load_plugin_syntax_definition(
    registration: &PluginSyntaxRegistration,
) -> anyhow::Result<SyntaxDefinition> {
    let text = read_plugin_text_file_with_limit(&registration.path, MAX_PLUGIN_SYNTAX_BYTES)?;
    let mut syntax = SyntaxDefinition::load_from_str(
        &text,
        true,
        registration.path.file_stem().and_then(|stem| stem.to_str()),
    )
    .with_context(|| format!("could not parse {}", registration.path.display()))?;

    for extension in &registration.extensions {
        if !syntax
            .file_extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        {
            syntax.file_extensions.push(extension.clone());
        }
    }

    Ok(syntax)
}

fn syntax_extension_for_buffer(buffer: &TextBuffer) -> Cow<'static, str> {
    if buffer.language() != LanguageId::PlainText {
        return Cow::Borrowed(buffer.language().syntect_extension());
    }

    buffer
        .path()
        .and_then(|path| path_extension(path))
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed("txt"))
}

fn path_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.trim_start_matches('.').to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
}

pub(crate) fn plain_visible(
    buffer: &TextBuffer,
    font_size: f32,
    tab_width: usize,
    rows: Range<usize>,
    text_color: Color32,
    line_char_limit: Option<usize>,
) -> Vec<LayoutJob> {
    let (font_size, tab_width) = normalize_layout_inputs(font_size, tab_width);
    let rows = visible_rows(buffer, rows);
    let start = rows.start;
    let end = rows.end;
    let mut jobs = Vec::with_capacity(end.saturating_sub(start));
    for line_idx in start..end {
        let text = visible_line_text(buffer, line_idx, line_char_limit);
        jobs.push(plain_job(&text, font_size, tab_width, text_color));
    }
    jobs
}

fn visible_rows(buffer: &TextBuffer, rows: Range<usize>) -> Range<usize> {
    let end = rows.end.min(buffer.len_lines());
    let start = rows.start.min(end);
    start..end
}

fn visible_line_text(
    buffer: &TextBuffer,
    line_idx: usize,
    line_char_limit: Option<usize>,
) -> String {
    buffer
        .line_content_prefix(line_idx, line_char_limit.unwrap_or(usize::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod hardening_tests {
    use super::*;
    use crate::syntax_cache::{HighlightCacheKey, MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE};
    use std::path::PathBuf;

    #[test]
    fn visible_highlighting_prunes_same_id_version_text_shape_changes() {
        let first = TextBuffer::from_text(
            1,
            Some(PathBuf::from("src/main.rs")),
            "let value = 1;\n".to_owned(),
        );
        let second = TextBuffer::from_text(
            1,
            Some(PathBuf::from("src/main.rs")),
            "let value = 1;\nlet next = 2;\n".to_owned(),
        );
        let mut highlighter = SyntaxHighlighter::new();

        highlighter.layout_visible(&first, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
        let first_key = HighlightCacheKey::for_buffer(&first, 13.0, 4, egui::Color32::WHITE);
        assert!(highlighter.caches.contains_key(&first_key));

        highlighter.layout_visible(&second, 13.0, 4, 0..2, true, egui::Color32::WHITE, -1);
        let second_key = HighlightCacheKey::for_buffer(&second, 13.0, 4, egui::Color32::WHITE);

        assert_ne!(first_key, second_key);
        assert!(!highlighter.caches.contains_key(&first_key));
        assert!(highlighter.caches.contains_key(&second_key));
    }

    #[test]
    fn oversized_visible_highlighting_falls_back_to_plain_without_cache() {
        let row_count = MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE + 1;
        let text = (0..row_count)
            .map(|line| format!("let value_{line} = {line};"))
            .collect::<Vec<_>>()
            .join("\n");
        let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), text);
        let mut highlighter = SyntaxHighlighter::new();

        let jobs = highlighter.layout_visible(
            &buffer,
            13.0,
            4,
            0..row_count,
            true,
            egui::Color32::WHITE,
            -1,
        );

        assert_eq!(jobs.len(), row_count);
        assert_eq!(jobs[0].text, "let value_0 = 0;");
        assert!(highlighter.caches.is_empty());
        assert!(highlighter.cache_order.is_empty());
    }

    #[test]
    fn plain_text_highlighting_uses_requested_theme_text_color() {
        let text_color = egui::Color32::from_rgb(22, 44, 66);
        let buffer = TextBuffer::from_text(1, None, "plain text".to_owned());
        let mut highlighter = SyntaxHighlighter::new();

        let jobs = highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, text_color, -1);

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].sections[0].format.color, text_color);
        assert!(highlighter.caches.is_empty());
        assert!(highlighter.cache_order.is_empty());
    }
}

#[cfg(test)]
mod tests;

use egui::{Color32, text::LayoutJob};
use kuroya_core::{LanguageId, TextBuffer};
use std::{
    collections::{BTreeMap, VecDeque},
    ops::Range,
};
use syntect::{
    highlighting::{HighlightState, Highlighter},
    parsing::{ParseState, ScopeStack, SyntaxReference},
};

pub(crate) const CHECKPOINT_INTERVAL: usize = 96;
pub(crate) const MAX_HIGHLIGHT_CACHES: usize = 8;
pub(crate) const MAX_VISIBLE_LAYOUT_RANGES_PER_CACHE: usize = 8;
pub(crate) const MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE: usize = CHECKPOINT_INTERVAL * 2;
// Keep the final usize tail reserved for overflow/sentinel row values from viewport math.
const MAX_CACHEABLE_VISIBLE_LAYOUT_ROW: usize = usize::MAX - MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct HighlightCacheKey {
    buffer_id: u64,
    version: u64,
    len_chars: usize,
    len_bytes: usize,
    len_lines: usize,
    language: LanguageId,
    syntax_extension: String,
    font_bits: u32,
    tab_width: usize,
    line_char_limit: Option<usize>,
    /// Active app theme identity for token color mapping. The theme text color
    /// drives the estimated background polarity and the low-contrast fallback,
    /// so a theme change produces a different key and stale highlight colors are
    /// never served.
    theme_text: Color32,
}

impl HighlightCacheKey {
    #[cfg(test)]
    pub(crate) fn for_buffer(
        buffer: &TextBuffer,
        font_size: f32,
        tab_width: usize,
        theme_text: Color32,
    ) -> Self {
        Self::for_buffer_with_extension(
            buffer,
            font_size,
            tab_width,
            buffer.language().syntect_extension(),
            None,
            theme_text,
        )
    }

    pub(crate) fn for_buffer_with_extension(
        buffer: &TextBuffer,
        font_size: f32,
        tab_width: usize,
        syntax_extension: &str,
        line_char_limit: Option<usize>,
        theme_text: Color32,
    ) -> Self {
        Self {
            buffer_id: buffer.id(),
            version: buffer.version(),
            len_chars: buffer.len_chars(),
            len_bytes: buffer.len_bytes(),
            len_lines: buffer.len_lines(),
            language: buffer.language(),
            syntax_extension: syntax_extension.to_owned(),
            font_bits: font_size.to_bits(),
            tab_width: tab_width.max(1),
            line_char_limit,
            theme_text,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_for_same_buffer(&self, other: &Self) -> bool {
        self.buffer_id == other.buffer_id
    }

    pub(crate) fn is_stale_parse_state_for_same_buffer(&self, other: &Self) -> bool {
        self.buffer_id == other.buffer_id
            && (self.version != other.version
                || self.len_chars != other.len_chars
                || self.len_bytes != other.len_bytes
                || self.len_lines != other.len_lines
                || self.language != other.language
                || self.syntax_extension != other.syntax_extension)
    }

    pub(crate) fn is_for_buffer_id(&self, buffer_id: u64) -> bool {
        self.buffer_id == buffer_id
    }
}

pub(crate) struct HighlightCheckpoint {
    pub(crate) parse_state: ParseState,
    pub(crate) highlight_state: HighlightState,
}

impl HighlightCheckpoint {
    fn initial(syntax: &SyntaxReference, highlighter: &Highlighter<'_>) -> Self {
        Self {
            parse_state: ParseState::new(syntax),
            highlight_state: HighlightState::new(highlighter, ScopeStack::new()),
        }
    }
}

impl Clone for HighlightCheckpoint {
    fn clone(&self) -> Self {
        Self {
            parse_state: self.parse_state.clone(),
            highlight_state: self.highlight_state.clone(),
        }
    }
}

pub(crate) struct HighlightCache {
    checkpoints: BTreeMap<usize, HighlightCheckpoint>,
    visible_layouts: BTreeMap<VisibleLayoutRange, Vec<LayoutJob>>,
    visible_layout_order: VecDeque<VisibleLayoutRange>,
    #[cfg(test)]
    visible_layout_hits: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct VisibleLayoutRange {
    start: usize,
    end: usize,
}

impl VisibleLayoutRange {
    fn from_rows(rows: Range<usize>) -> Option<Self> {
        let len = rows.end.checked_sub(rows.start)?;
        if rows.end > MAX_CACHEABLE_VISIBLE_LAYOUT_ROW {
            return None;
        }
        (len > 0 && len <= MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE).then_some(Self {
            start: rows.start,
            end: rows.end,
        })
    }

    fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }
}

impl HighlightCache {
    pub(crate) fn new(syntax: &SyntaxReference, highlighter: &Highlighter<'_>) -> Self {
        let mut checkpoints = BTreeMap::new();
        checkpoints.insert(0, HighlightCheckpoint::initial(syntax, highlighter));
        Self {
            checkpoints,
            visible_layouts: BTreeMap::new(),
            visible_layout_order: VecDeque::with_capacity(MAX_VISIBLE_LAYOUT_RANGES_PER_CACHE),
            #[cfg(test)]
            visible_layout_hits: 0,
        }
    }

    pub(crate) fn checkpoint_before_or_at(
        &self,
        line: usize,
        syntax: &SyntaxReference,
        highlighter: &Highlighter<'_>,
    ) -> (usize, HighlightCheckpoint) {
        self.checkpoints
            .range(..=line)
            .next_back()
            .map(|(line, checkpoint)| (*line, checkpoint.clone()))
            .unwrap_or_else(|| (0, HighlightCheckpoint::initial(syntax, highlighter)))
    }

    pub(crate) fn insert_checkpoint(
        &mut self,
        line: usize,
        parse_state: &ParseState,
        highlight_state: &HighlightState,
    ) {
        self.checkpoints
            .entry(line)
            .or_insert_with(|| HighlightCheckpoint {
                parse_state: parse_state.clone(),
                highlight_state: highlight_state.clone(),
            });
    }

    pub(crate) fn visible_layout_jobs(&mut self, rows: Range<usize>) -> Option<Vec<LayoutJob>> {
        let range = VisibleLayoutRange::from_rows(rows)?;
        let (cached_range, jobs) = if let Some(jobs) = self.visible_layouts.get(&range) {
            (range, jobs.clone())
        } else {
            self.containing_visible_layout_jobs(range)?
        };
        self.refresh_visible_layout_range(cached_range);
        #[cfg(test)]
        {
            self.visible_layout_hits = self.visible_layout_hits.saturating_add(1);
        }
        Some(jobs)
    }

    pub(crate) fn insert_visible_layout_jobs(&mut self, rows: Range<usize>, jobs: Vec<LayoutJob>) {
        let Some(range) = VisibleLayoutRange::from_rows(rows) else {
            return;
        };
        if jobs.len() != range.len() {
            return;
        }

        self.refresh_visible_layout_range(range);
        self.visible_layouts.insert(range, jobs);

        while self.visible_layout_order.len() > MAX_VISIBLE_LAYOUT_RANGES_PER_CACHE {
            if let Some(evicted) = self.visible_layout_order.pop_front() {
                self.visible_layouts.remove(&evicted);
            } else {
                self.visible_layouts.clear();
                break;
            }
        }
    }

    fn containing_visible_layout_jobs(
        &self,
        range: VisibleLayoutRange,
    ) -> Option<(VisibleLayoutRange, Vec<LayoutJob>)> {
        self.visible_layout_order
            .iter()
            .rev()
            .filter_map(|cached_range| {
                let jobs = self.visible_layouts.get(cached_range)?;
                Some((*cached_range, jobs))
            })
            .find_map(|(cached_range, jobs)| {
                if cached_range.start > range.start || cached_range.end < range.end {
                    return None;
                }

                let start = range.start.saturating_sub(cached_range.start);
                let end = range.end.saturating_sub(cached_range.start);
                jobs.get(start..end)
                    .map(|jobs| (cached_range, jobs.to_vec()))
            })
    }

    fn refresh_visible_layout_range(&mut self, range: VisibleLayoutRange) {
        if self.visible_layout_order.back() == Some(&range) {
            return;
        }
        self.visible_layout_order
            .retain(|existing| *existing != range);
        self.visible_layout_order.push_back(range);
    }

    #[cfg(test)]
    pub(crate) fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    #[cfg(test)]
    pub(crate) fn max_checkpoint_line(&self) -> Option<usize> {
        self.checkpoints.keys().next_back().copied()
    }

    #[cfg(test)]
    pub(crate) fn visible_layout_count(&self) -> usize {
        self.visible_layouts.len()
    }

    #[cfg(test)]
    pub(crate) fn visible_layout_hits(&self) -> usize {
        self.visible_layout_hits
    }
}

#[cfg(test)]
mod tests {
    use super::{HighlightCache, HighlightCacheKey, MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE};
    use egui::{Color32, text::LayoutJob};
    use kuroya_core::TextBuffer;
    use syntect::{
        highlighting::{Highlighter, ThemeSet},
        parsing::SyntaxSet,
    };

    fn test_cache() -> HighlightCache {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let syntax = syntaxes.find_syntax_plain_text();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get("base16-ocean.dark")
            .or_else(|| theme_set.themes.values().next())
            .unwrap();
        let highlighter = Highlighter::new(theme);
        HighlightCache::new(syntax, &highlighter)
    }

    fn layout_job(text: impl Into<String>) -> LayoutJob {
        LayoutJob {
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn visible_layout_cache_rejects_oversized_ranges() {
        let mut cache = test_cache();
        let oversized = MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE + 1;
        let jobs = (0..oversized)
            .map(|line| layout_job(format!("line {line}")))
            .collect::<Vec<_>>();

        cache.insert_visible_layout_jobs(0..oversized, jobs);

        assert_eq!(cache.visible_layout_count(), 0);
        assert!(cache.visible_layout_jobs(0..oversized).is_none());
        assert!(
            cache
                .visible_layout_jobs(0..MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE)
                .is_none()
        );
    }

    #[test]
    fn visible_layout_cache_rejects_reversed_and_extreme_ranges() {
        let mut cache = test_cache();

        let reversed_start = 10usize;
        let reversed_end = 3usize;
        cache
            .insert_visible_layout_jobs(reversed_start..reversed_end, vec![layout_job("reversed")]);
        assert_eq!(cache.visible_layout_count(), 0);
        assert!(
            cache
                .visible_layout_jobs(reversed_start..reversed_end)
                .is_none()
        );

        let extreme = usize::MAX - 1..usize::MAX;
        cache.insert_visible_layout_jobs(extreme.clone(), vec![layout_job("extreme")]);
        assert_eq!(cache.visible_layout_count(), 0);
        assert!(cache.visible_layout_jobs(extreme).is_none());

        cache.insert_visible_layout_jobs(1_000_000..1_000_001, vec![layout_job("large")]);
        assert_eq!(cache.visible_layout_count(), 1);
        assert_eq!(
            cache.visible_layout_jobs(1_000_000..1_000_001),
            Some(vec![layout_job("large")])
        );
    }

    #[test]
    fn visible_layout_cache_reuses_bounded_subranges() {
        let mut cache = test_cache();
        let jobs = (0..MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE)
            .map(|line| layout_job(format!("line {line}")))
            .collect::<Vec<_>>();

        cache.insert_visible_layout_jobs(0..MAX_VISIBLE_LAYOUT_ROWS_PER_RANGE, jobs.clone());
        let subrange = cache
            .visible_layout_jobs(3..7)
            .expect("bounded cached ranges should serve contained requests");

        assert_eq!(subrange, jobs[3..7]);
        assert_eq!(cache.visible_layout_count(), 1);
        assert_eq!(cache.visible_layout_hits(), 1);
    }

    #[test]
    fn highlight_cache_key_distinguishes_theme_text_color() {
        let buffer = TextBuffer::from_text(1, None, "let value = 1;\n".to_owned());
        let dark_theme = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, Color32::WHITE);
        let light_theme =
            HighlightCacheKey::for_buffer(&buffer, 13.0, 4, Color32::from_rgb(36, 41, 49));
        let dark_theme_again = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, Color32::WHITE);

        assert_ne!(dark_theme, light_theme);
        assert_eq!(dark_theme, dark_theme_again);
    }
}

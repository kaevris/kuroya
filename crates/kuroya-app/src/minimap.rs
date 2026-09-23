use crate::{
    large_file_mode::{LARGE_FILE_MODE_MAX_BYTES, LARGE_FILE_MODE_MAX_LINES},
    theme::diagnostic_color,
};
use egui::{
    self, Align2, Color32, FontFamily, FontId, Rect, Sense, TextFormat, pos2, text::LayoutJob, vec2,
};
use kuroya_core::{
    BufferId, DiagnosticSeverity, EditorMinimapShowSlider, GitLineChangeKind,
    MAX_EDITOR_MINIMAP_MAX_COLUMN, TextBuffer, minimap_section_header_lines,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

mod geometry;

#[cfg(test)]
pub(crate) use geometry::minimap_line_from_y;
pub(crate) use geometry::{
    minimap_content_fits_viewport, minimap_sample_line, minimap_target_line_from_y,
    minimap_viewport_rect,
};

const MAX_MINIMAP_LINE_LENGTH_CACHES: usize = 8;
const MAX_MINIMAP_SAMPLE_LINE_CACHES: usize = 8;
const MAX_MINIMAP_SECTION_HEADER_CACHES: usize = 8;
const MAX_MINIMAP_LINE_SAMPLES: usize = 4096;
const MAX_MINIMAP_SECTION_HEADER_LABEL_CHARS: usize = 120;
const MINIMAP_SECTION_HEADER_RESCAN_DEBOUNCE: Duration = Duration::from_secs(1);

#[derive(Debug, Default)]
pub(crate) struct MinimapLineLengthCache {
    entries: VecDeque<MinimapLineLengthCacheEntry>,
    sample_line_entries: VecDeque<MinimapSampleLineCacheEntry>,
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    sample_reuses: usize,
    #[cfg(test)]
    sample_line_cache_hits: usize,
}

#[derive(Debug, Clone)]
struct MinimapLineLengthCacheEntry {
    key: MinimapLineLengthCacheKey,
    samples: Vec<MinimapLineSample>,
}

#[derive(Debug, Clone)]
struct MinimapSampleLineCacheEntry {
    line_count: usize,
    sample_count: usize,
    line_indices: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MinimapLineSample {
    line_idx: usize,
    line_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MinimapLineLengthCacheKey {
    buffer_id: BufferId,
    buffer_version: u64,
    line_count: usize,
    sample_count: usize,
    max_column: usize,
    render_characters: bool,
}

impl MinimapLineLengthCache {
    pub(crate) fn sampled_lengths_for(
        &mut self,
        buffer: &TextBuffer,
        sample_count: usize,
        max_column: usize,
        render_characters: bool,
    ) -> &[MinimapLineSample] {
        let buffer_id = buffer.id();
        let buffer_version = buffer.version();
        let line_count = buffer.len_lines().max(1);
        let sample_count = minimap_clamped_sample_count(line_count, sample_count);
        let max_column = minimap_line_length_max_column(max_column, render_characters);

        let key = MinimapLineLengthCacheKey {
            buffer_id,
            buffer_version,
            line_count,
            sample_count,
            max_column,
            render_characters,
        };

        if let Some(index) = self.entries.iter().rposition(|entry| entry.key == key) {
            #[cfg(test)]
            {
                self.hits = self.hits.saturating_add(1);
            }
            if index + 1 == self.entries.len() {
                return self
                    .entries
                    .get(index)
                    .map(|entry| entry.samples.as_slice())
                    .unwrap_or(&[]);
            }
            let Some(entry) = self.entries.remove(index) else {
                return &[];
            };
            self.entries.push_back(entry);
            return self
                .entries
                .back()
                .map(|entry| entry.samples.as_slice())
                .unwrap_or(&[]);
        }

        let reusable_samples = self.retain_current_buffer_version(buffer_id, buffer_version);
        let mut source_index = self.reusable_sample_source_index(&key);
        let mut samples =
            self.reusable_sample_buffer(sample_count, reusable_samples, &mut source_index);
        let reused_sample_count = {
            let (sample_lines, sample_line_cache_hit) =
                Self::sample_lines_for(&mut self.sample_line_entries, line_count, sample_count);
            #[cfg(test)]
            {
                if sample_line_cache_hit {
                    self.sample_line_cache_hits = self.sample_line_cache_hits.saturating_add(1);
                }
            }
            #[cfg(not(test))]
            let _ = sample_line_cache_hit;

            if let Some(source_index) = source_index {
                let source_entry = &self.entries[source_index];
                Self::populate_sampled_lengths(
                    buffer,
                    &mut samples,
                    sample_lines,
                    Some((&source_entry.key, source_entry.samples.as_slice())),
                    &key,
                )
            } else {
                Self::populate_sampled_lengths(buffer, &mut samples, sample_lines, None, &key)
            }
        };
        if let Some(source_index) = source_index {
            self.promote_entry(source_index);
        }
        if reused_sample_count > 0 {
            #[cfg(test)]
            {
                self.sample_reuses = self.sample_reuses.saturating_add(reused_sample_count);
            }
        }
        self.entries
            .push_back(MinimapLineLengthCacheEntry { key, samples });
        self.entries
            .back()
            .map(|entry| entry.samples.as_slice())
            .unwrap_or(&[])
    }

    fn reusable_sample_buffer(
        &mut self,
        sample_count: usize,
        reusable_samples: Option<Vec<MinimapLineSample>>,
        protected_index: &mut Option<usize>,
    ) -> Vec<MinimapLineSample> {
        if let Some(samples) = reusable_samples {
            return Self::prepare_reusable_sample_buffer(samples, sample_count);
        }

        if self.entries.len() >= MAX_MINIMAP_LINE_LENGTH_CACHES
            && let Some(evict_index) =
                Self::evictable_entry_index(self.entries.len(), *protected_index)
            && let Some(entry) = self.entries.remove(evict_index)
        {
            if let Some(index) = protected_index.as_mut()
                && evict_index < *index
            {
                *index -= 1;
            }
            return Self::prepare_reusable_sample_buffer(entry.samples, sample_count);
        }

        Vec::with_capacity(sample_count)
    }

    fn evictable_entry_index(len: usize, protected_index: Option<usize>) -> Option<usize> {
        if len < MAX_MINIMAP_LINE_LENGTH_CACHES {
            return None;
        }

        match protected_index {
            Some(0) if len > 1 => Some(1),
            _ => Some(0),
        }
    }

    fn prepare_reusable_sample_buffer(
        mut samples: Vec<MinimapLineSample>,
        sample_count: usize,
    ) -> Vec<MinimapLineSample> {
        samples.clear();
        let capacity = samples.capacity();
        if capacity < sample_count {
            samples.reserve(sample_count - capacity);
        }
        samples
    }

    fn reusable_sample_source_index(&self, key: &MinimapLineLengthCacheKey) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.key.can_reuse_line_lengths_for(key))
            .max_by_key(|(_, entry)| entry.key.sample_count)
            .map(|(index, _)| index)
    }

    fn promote_entry(&mut self, index: usize) {
        if index + 1 == self.entries.len() {
            return;
        }

        if let Some(entry) = self.entries.remove(index) {
            self.entries.push_back(entry);
        }
    }

    fn sample_lines_for(
        sample_line_entries: &mut VecDeque<MinimapSampleLineCacheEntry>,
        line_count: usize,
        sample_count: usize,
    ) -> (&[usize], bool) {
        let mut cache_hit = false;
        if let Some(index) = sample_line_entries
            .iter()
            .rposition(|entry| entry.line_count == line_count && entry.sample_count == sample_count)
        {
            cache_hit = true;
            if index + 1 != sample_line_entries.len()
                && let Some(entry) = sample_line_entries.remove(index)
            {
                sample_line_entries.push_back(entry);
            }
        } else {
            if sample_line_entries.len() >= MAX_MINIMAP_SAMPLE_LINE_CACHES {
                sample_line_entries.pop_front();
            }
            sample_line_entries.push_back(MinimapSampleLineCacheEntry {
                line_count,
                sample_count,
                line_indices: Self::collect_sample_lines(line_count, sample_count),
            });
        }

        (
            sample_line_entries
                .back()
                .map(|entry| entry.line_indices.as_slice())
                .unwrap_or(&[]),
            cache_hit,
        )
    }

    fn collect_sample_lines(line_count: usize, sample_count: usize) -> Vec<usize> {
        let line_count = line_count.max(1);
        let sample_count = minimap_clamped_sample_count(line_count, sample_count);
        let mut line_indices = Vec::with_capacity(sample_count);
        if sample_count == line_count {
            line_indices.extend(0..sample_count);
        } else {
            line_indices.extend(
                (0..sample_count)
                    .map(|sample_idx| minimap_sample_line(sample_idx, sample_count, line_count)),
            );
        }
        line_indices
    }

    fn populate_sampled_lengths(
        buffer: &TextBuffer,
        samples: &mut Vec<MinimapLineSample>,
        sample_lines: &[usize],
        source: Option<(&MinimapLineLengthCacheKey, &[MinimapLineSample])>,
        key: &MinimapLineLengthCacheKey,
    ) -> usize {
        let (source_key, source_samples) = source.unwrap_or((key, &[]));
        let mut source_index = 0;
        let mut reused_sample_count = 0;
        for &line_idx in sample_lines {
            let line_len = if let Some(line_len) = Self::reusable_sample_line_len(
                source_key,
                source_samples,
                &mut source_index,
                line_idx,
                key,
            ) {
                reused_sample_count += 1;
                line_len
            } else {
                minimap_line_len(buffer, line_idx, key.max_column, key.render_characters)
            };
            samples.push(MinimapLineSample { line_idx, line_len });
        }
        reused_sample_count
    }

    fn reusable_sample_line_len(
        source_key: &MinimapLineLengthCacheKey,
        source_samples: &[MinimapLineSample],
        source_index: &mut usize,
        line_idx: usize,
        key: &MinimapLineLengthCacheKey,
    ) -> Option<usize> {
        while let Some(sample) = source_samples.get(*source_index) {
            if sample.line_idx >= line_idx {
                break;
            }
            *source_index += 1;
        }
        source_samples
            .get(*source_index)
            .filter(|sample| sample.line_idx == line_idx)
            .and_then(|sample| Self::reusable_line_len(source_key, sample.line_len, key))
    }

    fn reusable_line_len(
        source_key: &MinimapLineLengthCacheKey,
        source_line_len: usize,
        key: &MinimapLineLengthCacheKey,
    ) -> Option<usize> {
        if !key.render_characters {
            return Some(usize::from(source_line_len > 0));
        }
        if !source_key.render_characters {
            return None;
        }
        if source_key.max_column == key.max_column {
            return Some(source_line_len);
        }
        if source_key.max_column >= key.max_column {
            return Some(source_line_len.min(key.max_column));
        }
        (source_line_len < source_key.max_column).then_some(source_line_len)
    }

    fn retain_current_buffer_version(
        &mut self,
        buffer_id: BufferId,
        buffer_version: u64,
    ) -> Option<Vec<MinimapLineSample>> {
        let mut reusable_samples = None;
        let mut index = 0;
        while index < self.entries.len() {
            let is_stale = {
                let key = &self.entries[index].key;
                key.buffer_id == buffer_id && key.buffer_version != buffer_version
            };
            if is_stale {
                if let Some(entry) = self.entries.remove(index) {
                    reusable_samples =
                        Self::larger_reusable_sample_buffer(reusable_samples, entry.samples);
                }
            } else {
                index += 1;
            }
        }
        reusable_samples
    }

    fn larger_reusable_sample_buffer(
        reusable_samples: Option<Vec<MinimapLineSample>>,
        samples: Vec<MinimapLineSample>,
    ) -> Option<Vec<MinimapLineSample>> {
        match reusable_samples {
            Some(existing) if existing.capacity() >= samples.capacity() => Some(existing),
            _ => Some(samples),
        }
    }

    pub(crate) fn clear_for_buffer(&mut self, buffer_id: BufferId) {
        self.entries
            .retain(|entry| entry.key.buffer_id != buffer_id);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.sample_line_entries.clear();
        #[cfg(test)]
        {
            self.hits = 0;
            self.sample_reuses = 0;
            self.sample_line_cache_hits = 0;
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    fn hits(&self) -> usize {
        self.hits
    }

    #[cfg(test)]
    fn sample_reuses(&self) -> usize {
        self.sample_reuses
    }

    #[cfg(test)]
    fn sample_line_cache_hits(&self) -> usize {
        self.sample_line_cache_hits
    }

    #[cfg(test)]
    fn sample_line_cache_len(&self) -> usize {
        self.sample_line_entries.len()
    }

    #[cfg(test)]
    fn newest_sample_capacity(&self) -> usize {
        self.entries
            .back()
            .map(|entry| entry.samples.capacity())
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn contains_buffer_for_test(&self, buffer_id: BufferId) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.key.buffer_id == buffer_id)
    }
}

impl MinimapLineLengthCacheKey {
    fn can_reuse_line_lengths_for(&self, key: &Self) -> bool {
        self.buffer_id == key.buffer_id
            && self.buffer_version == key.buffer_version
            && self.line_count == key.line_count
            && (self.render_characters == key.render_characters
                || (self.render_characters && !key.render_characters))
    }
}

#[derive(Debug)]
pub(crate) struct MinimapSectionHeaderCache {
    entries: VecDeque<MinimapSectionHeaderCacheEntry>,
    rescan_debounce: Duration,
    #[cfg(test)]
    hits: usize,
}

impl Default for MinimapSectionHeaderCache {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            rescan_debounce: MINIMAP_SECTION_HEADER_RESCAN_DEBOUNCE,
            #[cfg(test)]
            hits: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct MinimapSectionHeaderCacheEntry {
    key: MinimapSectionHeaderCacheKey,
    headers: Arc<BTreeMap<usize, String>>,
    last_rescan: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MinimapSectionHeaderCacheKey {
    buffer_id: BufferId,
    buffer_version: u64,
    show_region_headers: bool,
    show_mark_headers: bool,
    mark_section_header_regex: String,
}

impl MinimapSectionHeaderCache {
    pub(crate) fn headers_for(
        &mut self,
        buffer: &TextBuffer,
        show_region_headers: bool,
        show_mark_headers: bool,
        mark_section_header_regex: &str,
    ) -> Arc<BTreeMap<usize, String>> {
        let buffer_id = buffer.id();
        let buffer_version = buffer.version();

        if !show_region_headers && !show_mark_headers {
            self.clear_for_buffer(buffer_id);
            return minimap_empty_section_headers();
        }

        if !minimap_section_header_scan_allowed(buffer.len_lines(), buffer.len_bytes()) {
            self.clear_for_buffer(buffer_id);
            return minimap_empty_section_headers();
        }

        let mark_section_header_regex = if show_mark_headers {
            mark_section_header_regex
        } else {
            ""
        };

        if let Some(index) = self.entries.iter().rposition(|entry| {
            entry.key.buffer_id == buffer_id
                && entry.key.buffer_version == buffer_version
                && entry.key.show_region_headers == show_region_headers
                && entry.key.show_mark_headers == show_mark_headers
                && entry.key.mark_section_header_regex.as_str() == mark_section_header_regex
        }) {
            #[cfg(test)]
            {
                self.hits = self.hits.saturating_add(1);
            }
            if index + 1 == self.entries.len() {
                return self
                    .entries
                    .get(index)
                    .map(|entry| entry.headers.clone())
                    .unwrap_or_else(minimap_empty_section_headers);
            }
            let Some(entry) = self.entries.remove(index) else {
                return minimap_empty_section_headers();
            };
            self.entries.push_back(entry);
            return self
                .entries
                .back()
                .map(|entry| entry.headers.clone())
                .unwrap_or_else(minimap_empty_section_headers);
        }

        // The buffer changed since the last scan. Debounce the full-buffer
        // rescan so typing does not rescan every keystroke: reuse the previous
        // headers until the debounce window elapses since the last rescan.
        let now = Instant::now();
        if self.rescan_debounce > Duration::ZERO
            && let Some(entry) = self
                .entries
                .iter_mut()
                .find(|entry| entry.key.buffer_id == buffer_id)
            && now.duration_since(entry.last_rescan) < self.rescan_debounce
        {
            return entry.headers.clone();
        }

        self.retain_current_buffer_version(buffer_id, buffer_version);
        let headers = Arc::new(minimap_section_header_lines(
            buffer,
            show_region_headers,
            show_mark_headers,
            mark_section_header_regex,
        ));
        let key = MinimapSectionHeaderCacheKey {
            buffer_id,
            buffer_version,
            show_region_headers,
            show_mark_headers,
            mark_section_header_regex: mark_section_header_regex.to_owned(),
        };
        self.entries.push_back(MinimapSectionHeaderCacheEntry {
            key,
            headers: headers.clone(),
            last_rescan: now,
        });
        while self.entries.len() > MAX_MINIMAP_SECTION_HEADER_CACHES {
            self.entries.pop_front();
        }
        headers
    }

    fn retain_current_buffer_version(&mut self, buffer_id: BufferId, buffer_version: u64) {
        self.entries.retain(|entry| {
            entry.key.buffer_id != buffer_id || entry.key.buffer_version == buffer_version
        });
    }

    pub(crate) fn clear_for_buffer(&mut self, buffer_id: BufferId) {
        self.entries
            .retain(|entry| entry.key.buffer_id != buffer_id);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        #[cfg(test)]
        {
            self.hits = 0;
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    fn hits(&self) -> usize {
        self.hits
    }

    #[cfg(test)]
    pub(crate) fn contains_buffer_for_test(&self, buffer_id: BufferId) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.key.buffer_id == buffer_id)
    }
}

fn minimap_section_header_scan_allowed(line_count: usize, byte_count: usize) -> bool {
    line_count <= LARGE_FILE_MODE_MAX_LINES && byte_count <= LARGE_FILE_MODE_MAX_BYTES
}

fn minimap_empty_section_headers() -> Arc<BTreeMap<usize, String>> {
    static EMPTY: OnceLock<Arc<BTreeMap<usize, String>>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::new(BTreeMap::new())).clone()
}

#[derive(Debug, Clone, Copy)]
struct MinimapMarkerLines<'a> {
    lines: &'a HashSet<usize>,
    bounds: Option<(usize, usize)>,
}

impl<'a> MinimapMarkerLines<'a> {
    fn new(lines: &'a HashSet<usize>, bounds_scan_limit: usize) -> Self {
        let bounds = if !lines.is_empty() && lines.len() <= bounds_scan_limit {
            minimap_marker_line_bounds(lines)
        } else {
            None
        };

        Self { lines, bounds }
    }

    fn contains(&self, line_number: usize) -> bool {
        if self.lines.is_empty() {
            return false;
        }

        if let Some((first_line, last_line)) = self.bounds
            && (line_number < first_line || line_number > last_line)
        {
            return false;
        }

        self.lines.contains(&line_number)
    }
}

fn minimap_marker_line_bounds(lines: &HashSet<usize>) -> Option<(usize, usize)> {
    let mut iter = lines.iter().copied();
    let first = iter.next()?;
    let mut first_line = first;
    let mut last_line = first;
    for line in iter {
        first_line = first_line.min(line);
        last_line = last_line.max(line);
    }
    Some((first_line, last_line))
}

pub(crate) fn render_minimap(
    ui: &mut egui::Ui,
    buffer: &TextBuffer,
    line_length_cache: &mut MinimapLineLengthCache,
    scroll_offset_y: f32,
    viewport_height: f32,
    row_height: f32,
    max_column: usize,
    show_slider: EditorMinimapShowSlider,
    scale: usize,
    render_characters: bool,
    background_image_active: bool,
    section_headers: &BTreeMap<usize, String>,
    section_header_font_size: f32,
    section_header_letter_spacing: f32,
    diff_lines: &BTreeMap<usize, GitLineChangeKind>,
    show_diff_lines: bool,
    diagnostics_by_line: &HashMap<usize, DiagnosticSeverity>,
    find_match_lines: &HashSet<usize>,
    cursor_lines: &HashSet<usize>,
    visible_line_indices: &[usize],
    visible_row_count: usize,
) -> Option<usize> {
    let size = minimap_render_size(ui.available_width(), ui.available_height())?;
    let line_count = buffer.len_lines().max(1);
    // The editor scrolls in fold-filtered row space, so the thumb and click
    // mapping must use the same row count the ScrollArea renders, not the raw
    // buffer line count.
    let row_count = minimap_row_count(visible_line_indices, visible_row_count, line_count);
    let visible_lines = minimap_visible_line_count(viewport_height, row_height, row_count);
    let first_visible_row = minimap_first_visible_line(scroll_offset_y, row_height, row_count);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();

    painter.rect_filled(
        rect,
        0.0,
        minimap_background_color(visuals, background_image_active),
    );
    let line_span = minimap_content_line_span(rect);
    let max_column = minimap_line_length_max_column(max_column, render_characters);
    let stroke_width = minimap_stroke_width(scale);
    let requested_sample_count = minimap_sample_count(line_count, rect.height(), stroke_width);
    let sampled_lines = if line_span.is_some() {
        line_length_cache.sampled_lengths_for(
            buffer,
            requested_sample_count,
            max_column,
            render_characters,
        )
    } else {
        &[]
    };
    let sample_count = sampled_lines.len();
    let sample_denominator = sample_count.saturating_sub(1);
    let sample_y_step = if sample_denominator == 0 {
        0.0
    } else {
        rect.height() / sample_denominator as f32
    };
    let cursor_line_lookup = MinimapMarkerLines::new(cursor_lines, sample_count);
    let has_diagnostics = !diagnostics_by_line.is_empty();
    let find_match_line_lookup = MinimapMarkerLines::new(find_match_lines, sample_count);
    let has_diff_lines = show_diff_lines && !diff_lines.is_empty();
    let section_header_style = (!section_headers.is_empty())
        .then(|| {
            MinimapSectionHeaderPaintStyle::new(
                rect.width(),
                section_header_font_size,
                section_header_letter_spacing,
            )
        })
        .filter(|style| style.char_capacity > 0);
    let mut diff_line_iter = diff_lines.iter().peekable();
    let mut section_header_iter = section_headers.iter().peekable();
    if let Some((x1, content_width)) = line_span {
        for (sample_idx, sample) in sampled_lines.iter().enumerate() {
            let line_idx = sample.line_idx;
            let line_number = line_idx.saturating_add(1);
            let line_len = sample.line_len;
            let y = rect.top() + sample_idx as f32 * sample_y_step;
            let line_width =
                minimap_line_width(line_len, max_column, content_width, render_characters);
            let x2 = x1 + line_width;
            let color = if cursor_line_lookup.contains(line_number) {
                minimap_cursor_line_color(visuals)
            } else if has_diagnostics
                && let Some(severity) = diagnostics_by_line.get(&line_number).copied()
            {
                diagnostic_color(severity)
            } else if find_match_line_lookup.contains(line_number) {
                minimap_find_match_line_color(visuals)
            } else if has_diff_lines
                && let Some(kind) = minimap_line_change_for_sample(&mut diff_line_iter, line_number)
            {
                minimap_line_change_color(kind)
            } else {
                minimap_default_line_color(visuals)
            };
            painter.line_segment(
                [pos2(x1, y), pos2(x2, y)],
                egui::Stroke::new(stroke_width, color),
            );
            if let Some(section_header_style) = section_header_style
                && let Some(label) =
                    minimap_section_header_for_sample(&mut section_header_iter, line_number)
            {
                paint_minimap_section_header(
                    &painter,
                    rect,
                    y,
                    label,
                    section_header_style,
                    visuals,
                );
            }
        }
    }

    let viewport_rect = minimap_viewport_rect(rect, first_visible_row, visible_lines, row_count);
    if !minimap_content_fits_viewport(visible_lines, row_count)
        && minimap_slider_visible(
            show_slider,
            response.hovered(),
            response.dragged() || response.is_pointer_button_down_on(),
        )
    {
        let slider_color = minimap_slider_color(
            visuals,
            response.hovered(),
            response.is_pointer_button_down_on() || response.dragged(),
        );
        painter.rect_filled(viewport_rect, 0.0, slider_color);
    }

    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        return Some(minimap_jump_line_from_y(
            pos.y,
            rect,
            line_count,
            row_count,
            visible_lines,
            visible_line_indices,
        ));
    }

    None
}

fn minimap_row_count(
    visible_line_indices: &[usize],
    visible_row_count: usize,
    line_count: usize,
) -> usize {
    let row_count = if visible_line_indices.is_empty() {
        visible_row_count
    } else {
        visible_line_indices.len()
    };
    row_count.max(1).min(line_count.max(1))
}

fn minimap_jump_line_from_y(
    y: f32,
    rect: Rect,
    line_count: usize,
    row_count: usize,
    visible_lines: usize,
    visible_line_indices: &[usize],
) -> usize {
    let target_row = minimap_target_line_from_y(y, rect, row_count, visible_lines);
    let line_idx = match visible_line_indices.get(target_row) {
        Some(&line_idx) => line_idx,
        None => visible_line_indices.last().copied().unwrap_or(target_row),
    };
    line_idx.min(line_count.saturating_sub(1))
}

fn minimap_background_color(visuals: &egui::Visuals, background_image_active: bool) -> Color32 {
    minimap_background_fill(visuals.code_bg_color, background_image_active)
}

// The minimap base slab is opaque chrome over the editor background, so while
// a background image is active it must go away entirely and let the image show
// through, exactly like the editor rows and their overlays do.
fn minimap_background_fill(bg_color: Color32, background_image_active: bool) -> Color32 {
    if background_image_active {
        Color32::TRANSPARENT
    } else {
        bg_color
    }
}

fn minimap_cursor_line_color(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.active.bg_fill
}

fn minimap_find_match_line_color(visuals: &egui::Visuals) -> Color32 {
    visuals.warn_fg_color
}

fn minimap_default_line_color(visuals: &egui::Visuals) -> Color32 {
    visuals.widgets.inactive.bg_stroke.color
}

fn minimap_slider_color(visuals: &egui::Visuals, hovered: bool, interacting: bool) -> Color32 {
    let (base, alpha) = if interacting {
        (visuals.selection.bg_fill, 150)
    } else if hovered {
        (visuals.widgets.hovered.bg_fill, 180)
    } else {
        (visuals.widgets.inactive.bg_fill, 100)
    };
    color_with_alpha(base, alpha)
}

fn color_with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn minimap_render_size(width: f32, height: f32) -> Option<egui::Vec2> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some(vec2(width, height))
}

fn minimap_content_line_span(rect: Rect) -> Option<(f32, f32)> {
    let left = rect.left();
    let right = rect.right();
    if !left.is_finite() || !right.is_finite() {
        return None;
    }

    let rect_width = rect.width();
    if !rect_width.is_finite() || rect_width <= 0.0 {
        return None;
    }

    let side_padding = if rect_width >= 12.0 {
        6.0
    } else {
        rect_width * 0.25
    };
    let x = left + side_padding;
    let content_width = (rect_width - side_padding * 2.0).max(0.0);
    if !x.is_finite() || !content_width.is_finite() || content_width <= 0.0 {
        return None;
    }

    let drawable_width = content_width.min((right - x).max(0.0));
    if !drawable_width.is_finite() || drawable_width <= 0.0 {
        return None;
    }

    Some((x, drawable_width))
}

fn minimap_line_len(
    buffer: &TextBuffer,
    line_idx: usize,
    max_column: usize,
    render_characters: bool,
) -> usize {
    buffer.line_content_char_count_capped(
        line_idx,
        minimap_line_length_max_column(max_column, render_characters),
    )
}

fn minimap_line_length_max_column(max_column: usize, render_characters: bool) -> usize {
    if render_characters {
        max_column.clamp(1, MAX_EDITOR_MINIMAP_MAX_COLUMN)
    } else {
        1
    }
}

fn minimap_line_width(
    line_len: usize,
    max_column: usize,
    content_width: f32,
    render_characters: bool,
) -> f32 {
    if !content_width.is_finite() || content_width <= 0.0 {
        return 0.0;
    }

    if render_characters {
        (line_len.max(1) as f32 / max_column.max(1) as f32).min(1.0) * content_width
    } else if line_len == 0 {
        content_width.min(1.0)
    } else {
        content_width
    }
}

fn minimap_stroke_width(scale: usize) -> f32 {
    scale.clamp(1, 3) as f32
}

fn minimap_sample_count(line_count: usize, height: f32, stroke_width: f32) -> usize {
    let height = if height.is_finite() {
        height.max(1.0)
    } else {
        1.0
    };
    let stroke_width = if stroke_width.is_finite() {
        stroke_width.max(1.0)
    } else {
        1.0
    };
    minimap_clamped_sample_count(line_count, (height / stroke_width).ceil().max(1.0) as usize)
}

fn minimap_clamped_sample_count(line_count: usize, sample_count: usize) -> usize {
    line_count
        .max(1)
        .min(sample_count.max(1))
        .min(MAX_MINIMAP_LINE_SAMPLES)
}

fn minimap_visible_line_count(viewport_height: f32, row_height: f32, line_count: usize) -> usize {
    let line_count = line_count.max(1);
    if !viewport_height.is_finite()
        || viewport_height <= 0.0
        || !row_height.is_finite()
        || row_height <= 0.0
    {
        return 1;
    }

    let visible_lines = (viewport_height as f64 / row_height as f64).ceil();
    if !visible_lines.is_finite() {
        return line_count;
    }

    minimap_bounded_usize(visible_lines, 1, line_count)
}

fn minimap_first_visible_line(scroll_offset_y: f32, row_height: f32, line_count: usize) -> usize {
    let line_count = line_count.max(1);
    if !scroll_offset_y.is_finite() || !row_height.is_finite() || row_height <= 0.0 {
        return 0;
    }

    let first_visible_line = (scroll_offset_y.max(0.0) as f64 / row_height as f64).floor();
    minimap_bounded_usize(first_visible_line, 0, line_count.saturating_sub(1))
}

fn minimap_bounded_usize(value: f64, min: usize, max: usize) -> usize {
    if max <= min || !value.is_finite() || value <= min as f64 {
        return min;
    }
    if value >= max as f64 {
        return max;
    }
    value as usize
}

fn paint_minimap_section_header(
    painter: &egui::Painter,
    rect: Rect,
    y: f32,
    label: &str,
    style: MinimapSectionHeaderPaintStyle,
    visuals: &egui::Visuals,
) {
    let text = minimap_section_header_display_text_for_capacity(label, style.char_capacity);
    if text.is_empty() {
        return;
    }

    let top = (y - style.font_size * 0.5).clamp(
        rect.top(),
        (rect.bottom() - style.font_size).max(rect.top()),
    );
    let background = Rect::from_min_max(
        pos2(rect.left() + 2.0, top - 1.0),
        pos2(rect.right() - 2.0, top + style.font_size + 1.0),
    );
    painter.rect_filled(
        background,
        1.0,
        color_with_alpha(visuals.widgets.active.bg_fill, 220),
    );

    let font = FontId::new(style.font_size, FontFamily::Monospace);
    let color = visuals.text_color();
    let y = top + style.font_size * 0.5;
    let galley = painter.layout_job(LayoutJob::single_section(
        text,
        TextFormat {
            font_id: font,
            extra_letter_spacing: style.letter_spacing,
            color,
            ..Default::default()
        },
    ));
    let text_rect = Align2::LEFT_CENTER.anchor_size(pos2(rect.left() + 6.0, y), galley.size());
    painter.galley(text_rect.min, galley, color);
}

#[derive(Debug, Clone, Copy)]
struct MinimapSectionHeaderPaintStyle {
    font_size: f32,
    letter_spacing: f32,
    char_capacity: usize,
}

impl MinimapSectionHeaderPaintStyle {
    fn new(width: f32, font_size: f32, letter_spacing: f32) -> Self {
        let font_size = minimap_section_header_font_size(font_size);
        let letter_spacing = minimap_section_header_letter_spacing(letter_spacing);
        let char_capacity = minimap_section_header_char_capacity(width, font_size, letter_spacing)
            .min(MAX_MINIMAP_SECTION_HEADER_LABEL_CHARS);

        Self {
            font_size,
            letter_spacing,
            char_capacity,
        }
    }
}

#[cfg(test)]
fn minimap_section_header_display_text(
    label: &str,
    width: f32,
    font_size: f32,
    letter_spacing: f32,
) -> String {
    let capacity =
        MinimapSectionHeaderPaintStyle::new(width, font_size, letter_spacing).char_capacity;
    minimap_section_header_display_text_for_capacity(label, capacity)
}

fn minimap_section_header_display_text_for_capacity(label: &str, capacity: usize) -> String {
    if capacity == 0 {
        return String::new();
    }

    let capacity = capacity.min(MAX_MINIMAP_SECTION_HEADER_LABEL_CHARS);
    let mut text = String::with_capacity(label.len().min(capacity.saturating_mul(4)));
    let mut char_count = 0;
    let mut pending_whitespace = String::new();
    let mut pending_whitespace_count = 0usize;
    for ch in label.chars() {
        if !minimap_section_header_char_is_visible(ch) {
            continue;
        }

        if ch.is_whitespace() {
            if char_count > 0 && char_count + pending_whitespace_count + 1 < capacity {
                pending_whitespace.push(ch);
                pending_whitespace_count += 1;
            }
            continue;
        }

        if pending_whitespace_count > 0 {
            text.push_str(&pending_whitespace);
            char_count += pending_whitespace_count;
            pending_whitespace.clear();
            pending_whitespace_count = 0;
        }

        text.push(ch);
        char_count += 1;
        if char_count == capacity {
            break;
        }
    }
    text
}

fn minimap_section_header_char_is_visible(ch: char) -> bool {
    !ch.is_control()
        && !matches!(
            ch,
            '\u{061c}'
                | '\u{200e}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

fn minimap_section_header_char_capacity(width: f32, font_size: f32, letter_spacing: f32) -> usize {
    if !width.is_finite() || width <= 12.0 {
        return 0;
    }

    let available = width - 12.0;
    let advance = minimap_section_header_char_advance(font_size, letter_spacing);
    (available / advance).floor().max(0.0) as usize
}

fn minimap_section_header_char_advance(font_size: f32, letter_spacing: f32) -> f32 {
    minimap_section_header_font_size(font_size) * 0.58
        + minimap_section_header_letter_spacing(letter_spacing)
}

fn minimap_section_header_font_size(font_size: f32) -> f32 {
    if font_size.is_finite() {
        font_size.clamp(4.0, 32.0)
    } else {
        9.0
    }
}

fn minimap_section_header_letter_spacing(letter_spacing: f32) -> f32 {
    if letter_spacing.is_finite() {
        letter_spacing.clamp(0.0, 5.0)
    } else {
        1.0
    }
}

fn minimap_slider_visible(
    setting: EditorMinimapShowSlider,
    hovered: bool,
    interacting: bool,
) -> bool {
    match setting {
        EditorMinimapShowSlider::Always => true,
        EditorMinimapShowSlider::Mouseover => hovered || interacting,
    }
}

fn minimap_line_change_color(kind: GitLineChangeKind) -> Color32 {
    match kind {
        GitLineChangeKind::Added => Color32::from_rgb(54, 92, 58),
        GitLineChangeKind::Modified => Color32::from_rgb(58, 72, 104),
        GitLineChangeKind::Deleted => Color32::from_rgb(106, 58, 58),
    }
}

fn minimap_line_change_for_sample<'a, I>(
    diff_line_iter: &mut std::iter::Peekable<I>,
    line_number: usize,
) -> Option<GitLineChangeKind>
where
    I: Iterator<Item = (&'a usize, &'a GitLineChangeKind)>,
{
    while let Some((diff_line, kind)) = diff_line_iter.peek().copied() {
        if *diff_line == 0 || *diff_line < line_number {
            diff_line_iter.next();
            continue;
        }
        if *diff_line == line_number {
            return Some(*kind);
        }
        break;
    }
    None
}

fn minimap_section_header_for_sample<'a, I>(
    section_header_iter: &mut std::iter::Peekable<I>,
    line_number: usize,
) -> Option<&'a str>
where
    I: Iterator<Item = (&'a usize, &'a String)>,
{
    let mut latest = None;
    while let Some((header_line, label)) = section_header_iter.peek().copied() {
        if *header_line == 0 {
            section_header_iter.next();
            continue;
        }
        if *header_line > line_number {
            break;
        }
        latest = Some(label.as_str());
        section_header_iter.next();
    }
    latest
}

#[cfg(test)]
mod tests;

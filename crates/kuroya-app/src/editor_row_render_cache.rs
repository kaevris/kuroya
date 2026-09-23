use eframe::egui::{Color32, FontId, Galley};
use kuroya_core::{BufferId, EditorExperimentalGpuAcceleration};
use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const EDITOR_ROW_RENDER_CACHE_CAPACITY: usize = 4096;

static EDITOR_ROW_RENDER_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
static EDITOR_ROW_RENDER_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub(crate) struct EditorRowRenderCacheEntry {
    key: EditorRowRenderKey,
    value: Arc<Galley>,
    stamp: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EditorRowRenderKey {
    buffer_id: BufferId,
    buffer_version: u64,
    row_index: usize,
    wrap_max_width_bits: u32,
    font_id_hash: u64,
    theme_revision: u64,
    text_color: [u8; 4],
    tab_width: usize,
    stop_rendering_line_after: i64,
}

impl EditorRowRenderKey {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        buffer_id: BufferId,
        buffer_version: u64,
        row_index: usize,
        wrap_max_width_bits: u32,
        font_id: &FontId,
        theme_revision: u64,
        text_color: Color32,
        tab_width: usize,
        stop_rendering_line_after: i64,
    ) -> Self {
        Self {
            buffer_id,
            buffer_version,
            row_index,
            wrap_max_width_bits,
            font_id_hash: font_id_hash(font_id),
            theme_revision,
            text_color: [
                text_color.r(),
                text_color.g(),
                text_color.b(),
                text_color.a(),
            ],
            tab_width,
            stop_rendering_line_after,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct EditorRowRenderCache {
    entries: HashMap<EditorRowRenderKey, EditorRowRenderCacheEntry>,

    order: VecDeque<(EditorRowRenderKey, u64)>,
    next_stamp: u64,
}

pub(crate) struct GpuRowRenderScope<'a> {
    pub(crate) cache: &'a mut EditorRowRenderCache,
    pub(crate) theme_revision: u64,
}

impl EditorRowRenderCache {
    pub(crate) fn get_or_compute(
        &mut self,
        key: EditorRowRenderKey,
        compute: impl FnOnce() -> Arc<Galley>,
    ) -> Arc<Galley> {
        if let Some(value) = self.lookup(&key) {
            return value;
        }

        let value = compute();
        self.insert(key, value.clone());
        value
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    pub(crate) fn clear_for_buffer(&mut self, buffer_id: BufferId) {
        self.entries
            .retain(|_, entry| entry.key.buffer_id != buffer_id);
        self.order.retain(|(key, _)| key.buffer_id != buffer_id);
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    fn lookup(&mut self, key: &EditorRowRenderKey) -> Option<Arc<Galley>> {
        let stamp = self.next_stamp();
        let entry = self.entries.get_mut(key)?;
        let value = entry.value.clone();
        entry.stamp = stamp;
        EDITOR_ROW_RENDER_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
        Some(value)
    }

    fn insert(&mut self, key: EditorRowRenderKey, value: Arc<Galley>) {
        if self.entries.contains_key(&key) {
            let stamp = self.next_stamp();
            self.entries
                .insert(key, EditorRowRenderCacheEntry { key, value, stamp });
            EDITOR_ROW_RENDER_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
            return;
        }
        while self.entries.len() >= EDITOR_ROW_RENDER_CACHE_CAPACITY {
            if !self.evict_least_recently_used() {
                self.clear();
                break;
            }
        }
        let stamp = self.next_stamp();
        self.order.push_back((key, stamp));
        self.entries
            .insert(key, EditorRowRenderCacheEntry { key, value, stamp });
        EDITOR_ROW_RENDER_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    }

    fn next_stamp(&mut self) -> u64 {
        self.next_stamp = self.next_stamp.wrapping_add(1);
        self.next_stamp
    }

    fn evict_least_recently_used(&mut self) -> bool {
        while let Some((candidate, queued_stamp)) = self.order.pop_front() {
            let current_stamp = self.entries.get(&candidate).map(|entry| entry.stamp);
            match current_stamp {
                Some(stamp) if stamp != queued_stamp => {
                    self.order.push_back((candidate, stamp));
                }
                Some(_) => {
                    self.entries.remove(&candidate);
                    return true;
                }

                None => {}
            }
        }
        false
    }
}

pub(crate) fn editor_row_render_cache_enabled(mode: EditorExperimentalGpuAcceleration) -> bool {
    matches!(mode, EditorExperimentalGpuAcceleration::On)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditorRowRenderCacheStats {
    pub(crate) enabled: bool,
    pub(crate) hits: u64,
    pub(crate) misses: u64,
    pub(crate) entries: usize,
    pub(crate) capacity: usize,
}

pub(crate) fn editor_row_render_cache_stats(
    mode: EditorExperimentalGpuAcceleration,
    cache: Option<&EditorRowRenderCache>,
) -> EditorRowRenderCacheStats {
    EditorRowRenderCacheStats {
        enabled: editor_row_render_cache_enabled(mode),
        hits: EDITOR_ROW_RENDER_CACHE_HITS.load(Ordering::Relaxed),
        misses: EDITOR_ROW_RENDER_CACHE_MISSES.load(Ordering::Relaxed),
        entries: cache.map_or(0, EditorRowRenderCache::len),
        capacity: EDITOR_ROW_RENDER_CACHE_CAPACITY,
    }
}

pub(crate) fn editor_row_theme_revision(theme: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    theme.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn editor_render_cache_hits() -> u64 {
    EDITOR_ROW_RENDER_CACHE_HITS.load(Ordering::Relaxed)
}

pub(crate) fn editor_render_cache_misses() -> u64 {
    EDITOR_ROW_RENDER_CACHE_MISSES.load(Ordering::Relaxed)
}

fn font_id_hash(font_id: &FontId) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    font_id.size.to_bits().hash(&mut hasher);
    font_id.family.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::{
        EDITOR_ROW_RENDER_CACHE_CAPACITY, EditorRowRenderCache, EditorRowRenderKey,
        editor_row_render_cache_enabled, editor_row_render_cache_stats, font_id_hash,
    };
    use eframe::egui::{self, Color32, FontFamily, FontId, Galley};
    use kuroya_core::EditorExperimentalGpuAcceleration;
    use std::{cell::Cell, sync::Arc};

    fn sample_galley() -> Arc<Galley> {
        let ctx = egui::Context::default();
        let job = egui::text::LayoutJob::simple(
            "row".to_owned(),
            FontId::new(13.0, FontFamily::Monospace),
            Color32::WHITE,
            f32::INFINITY,
        );
        let galley = std::cell::RefCell::new(None);
        let _ = ctx.run(Default::default(), |ctx| {
            *galley.borrow_mut() = Some(ctx.fonts_mut(|fonts| fonts.layout_job(job.clone())));
        });
        galley.into_inner().expect("galley should be laid out")
    }

    fn test_key(
        row_index: usize,
        wrap_max_width_bits: u32,
        buffer_version: u64,
    ) -> EditorRowRenderKey {
        EditorRowRenderKey::new(
            1,
            buffer_version,
            row_index,
            wrap_max_width_bits,
            &FontId::new(13.0, FontFamily::Monospace),
            7,
            Color32::WHITE,
            4,
            -1,
        )
    }

    #[test]
    fn editor_row_render_cache_reuses_entry_for_identical_key() {
        let mut cache = EditorRowRenderCache::default();
        let computes = Cell::new(0usize);
        let key = test_key(0, 800.0_f32.to_bits(), 3);

        let first = cache.get_or_compute(key, || {
            computes.set(computes.get() + 1);
            sample_galley()
        });
        let second = cache.get_or_compute(key, || {
            computes.set(computes.get() + 1);
            sample_galley()
        });

        assert_eq!(computes.get(), 1);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn editor_row_render_cache_keys_track_version_width_and_font() {
        let base = test_key(0, 800.0_f32.to_bits(), 3);

        assert_ne!(base, test_key(0, 800.0_f32.to_bits(), 4));
        assert_ne!(base, test_key(0, 801.0_f32.to_bits(), 3));

        let monospace = FontId::new(13.0, FontFamily::Monospace);
        let proportional = FontId::new(13.0, FontFamily::Proportional);
        let larger = FontId::new(14.0, FontFamily::Monospace);

        assert_ne!(font_id_hash(&monospace), font_id_hash(&larger));
        assert_ne!(font_id_hash(&monospace), font_id_hash(&proportional));
        assert_eq!(font_id_hash(&monospace), font_id_hash(&monospace));
    }

    #[test]
    fn editor_row_render_cache_distinct_keys_compute_separately() {
        let mut cache = EditorRowRenderCache::default();
        let computes = Cell::new(0usize);

        for key in [
            test_key(0, 800.0_f32.to_bits(), 3),
            test_key(0, 800.0_f32.to_bits(), 4),
            test_key(0, 801.0_f32.to_bits(), 3),
        ] {
            cache.get_or_compute(key, || {
                computes.set(computes.get() + 1);
                sample_galley()
            });
        }

        assert_eq!(computes.get(), 3);
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn editor_row_render_cache_disabled_for_off_setting() {
        assert!(!editor_row_render_cache_enabled(
            EditorExperimentalGpuAcceleration::Off
        ));
        assert!(editor_row_render_cache_enabled(
            EditorExperimentalGpuAcceleration::On
        ));
    }

    #[test]
    fn editor_row_render_cache_evicts_oldest_at_capacity() {
        let mut cache = EditorRowRenderCache::default();
        for row_index in 0..EDITOR_ROW_RENDER_CACHE_CAPACITY {
            let _ =
                cache.get_or_compute(test_key(row_index, 800.0_f32.to_bits(), 3), sample_galley);
        }
        assert_eq!(cache.len(), EDITOR_ROW_RENDER_CACHE_CAPACITY);

        let _ = cache.get_or_compute(
            test_key(EDITOR_ROW_RENDER_CACHE_CAPACITY, 800.0_f32.to_bits(), 3),
            sample_galley,
        );
        assert_eq!(cache.len(), EDITOR_ROW_RENDER_CACHE_CAPACITY);

        let oldest = test_key(0, 800.0_f32.to_bits(), 3);
        let recomputes = Cell::new(0usize);
        let _ = cache.get_or_compute(oldest, || {
            recomputes.set(recomputes.get() + 1);
            sample_galley()
        });

        assert_eq!(recomputes.get(), 1, "oldest entry was evicted at capacity");
    }

    #[test]
    fn editor_row_render_cache_promotion_survives_capacity_eviction() {
        let mut cache = EditorRowRenderCache::default();
        for row_index in 0..EDITOR_ROW_RENDER_CACHE_CAPACITY {
            let _ =
                cache.get_or_compute(test_key(row_index, 800.0_f32.to_bits(), 3), sample_galley);
        }

        let promoted = test_key(0, 800.0_f32.to_bits(), 3);
        let recomputes = Cell::new(0usize);
        let _ = cache.get_or_compute(promoted, || {
            recomputes.set(recomputes.get() + 1);
            sample_galley()
        });
        assert_eq!(
            recomputes.get(),
            0,
            "promotion is a lookup, not a recompute"
        );

        let _ = cache.get_or_compute(
            test_key(EDITOR_ROW_RENDER_CACHE_CAPACITY, 800.0_f32.to_bits(), 3),
            sample_galley,
        );

        let _ = cache.get_or_compute(promoted, || {
            recomputes.set(recomputes.get() + 1);
            sample_galley()
        });
        assert_eq!(recomputes.get(), 0, "promoted entry survives eviction");

        let evicted_neighbor = test_key(1, 800.0_f32.to_bits(), 3);
        let _ = cache.get_or_compute(evicted_neighbor, || {
            recomputes.set(recomputes.get() + 1);
            sample_galley()
        });
        assert_eq!(
            recomputes.get(),
            1,
            "least recently used entry was evicted instead of the promoted one"
        );
        assert_eq!(cache.len(), EDITOR_ROW_RENDER_CACHE_CAPACITY);
    }

    #[test]
    fn editor_row_render_cache_clear_for_buffer_keeps_other_buffers_cached() {
        let mut cache = EditorRowRenderCache::default();
        let computes = Cell::new(0usize);
        let other_buffer_key = EditorRowRenderKey::new(
            2,
            3,
            0,
            800.0_f32.to_bits(),
            &FontId::new(13.0, FontFamily::Monospace),
            7,
            Color32::WHITE,
            4,
            -1,
        );

        let _ = cache.get_or_compute(test_key(0, 800.0_f32.to_bits(), 3), sample_galley);
        let first = cache.get_or_compute(other_buffer_key, sample_galley);

        cache.clear_for_buffer(1);

        let _ = cache.get_or_compute(test_key(0, 800.0_f32.to_bits(), 3), || {
            computes.set(computes.get() + 1);
            sample_galley()
        });
        assert_eq!(computes.get(), 1, "cleared buffer rows must recompute");

        let second = cache.get_or_compute(other_buffer_key, || {
            computes.set(computes.get() + 1);
            sample_galley()
        });
        assert_eq!(
            computes.get(),
            1,
            "other buffers keep their cached rows after a targeted clear"
        );
        assert!(Arc::ptr_eq(&first, &second));

        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn editor_row_render_cache_stats_reflect_mode_and_entries() {
        let mut cache = EditorRowRenderCache::default();

        let stats =
            editor_row_render_cache_stats(EditorExperimentalGpuAcceleration::Off, Some(&cache));
        assert!(!stats.enabled);

        let stats =
            editor_row_render_cache_stats(EditorExperimentalGpuAcceleration::On, Some(&cache));
        assert!(stats.enabled);
        assert_eq!(stats.entries, 0);
        assert_eq!(stats.capacity, EDITOR_ROW_RENDER_CACHE_CAPACITY);

        let _ = cache.get_or_compute(test_key(0, 800.0_f32.to_bits(), 3), sample_galley);
        let stats =
            editor_row_render_cache_stats(EditorExperimentalGpuAcceleration::On, Some(&cache));
        assert_eq!(stats.entries, 1);
        assert!(
            stats.hits + stats.misses >= 1,
            "get_or_compute records a hit or miss"
        );

        let stats = editor_row_render_cache_stats(EditorExperimentalGpuAcceleration::On, None);
        assert_eq!(stats.entries, 0, "no cache means no entries to report");
    }
}

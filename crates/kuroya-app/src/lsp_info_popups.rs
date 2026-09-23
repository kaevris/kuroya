use crate::{
    KuroyaApp,
    lsp_markdown_render::{LspMarkdownTextSize, render_lsp_markdown},
    path_display::display_path_label_cow,
    popup_buttons::{PopupButtonKind, popup_button},
    workspace_state::paths_match_lexically,
};
use eframe::egui::{self, Align, Color32, Context, Id, Key, RichText, ScrollArea};
use kuroya_core::{
    BufferId, TextBuffer, clamp_hover_hiding_delay_ms, editor_stop_rendering_line_after_limit,
};
use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    fmt::Write,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

mod signature;

const MAX_HOVER_MARKDOWN_CHARS: usize = 10_000;
const HOVER_MARKDOWN_TRUNCATED_NOTICE: &str = "\n\n[Hover truncated]";
const LSP_HOVER_DISPLAY_CACHE_ID: &str = "lsp_hover_display_cache";

impl KuroyaApp {
    pub(crate) fn render_lsp_hover(&mut self, ctx: &Context) {
        if self
            .lsp_hover
            .as_ref()
            .is_some_and(|hover| !hover_popup_target_matches_buffer(self.active_buffer(), hover))
        {
            self.lsp_hover = None;
            return;
        }
        let Some(hover) = self.lsp_hover.as_ref() else {
            return;
        };
        let mut close = false;
        let hiding_delay = Duration::from_millis(clamp_hover_hiding_delay_ms(
            self.settings.hover_hiding_delay_ms,
        ) as u64);
        if let Some(remaining) = hover_hide_remaining(
            hover.opened_at,
            Instant::now(),
            hiding_delay,
            self.settings.hover_sticky,
        ) {
            if remaining.is_zero() {
                self.lsp_hover = None;
                self.status = "Closed hover after hiding delay".to_owned();
                return;
            }
            ctx.request_repaint_after(remaining);
        }
        let (anchor, offset) = hover_window_anchor(self.settings.hover_above);
        let contents = cached_bounded_hover_markdown(ctx, hover);

        egui::Window::new("LSP Hover")
            .max_size(crate::layout::popup_window_max_size(ctx))
            .collapsible(false)
            .resizable(true)
            .anchor(anchor, offset)
            .default_size([420.0, 220.0])
            .show(ctx, |ui| {
                let target_label = cached_hover_target_label(
                    ctx,
                    &hover.path,
                    hover.line,
                    hover.column,
                    ui.visuals().weak_text_color(),
                );
                ui.horizontal(|ui| {
                    ui.label(target_label.as_ref().clone());
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if popup_button(ui, "Close", PopupButtonKind::Secondary).clicked() {
                            close = true;
                        }
                    });
                });

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }

                if let Some(warning) = hover_long_line_warning(
                    self.active_buffer(),
                    hover,
                    self.settings.stop_rendering_line_after,
                    self.settings.hover_show_long_line_warning,
                ) {
                    ui.label(
                        RichText::new(warning)
                            .small()
                            .color(ui.visuals().warn_fg_color),
                    );
                }

                ui.separator();
                ScrollArea::vertical().show(ui, |ui| {
                    render_lsp_markdown(ui, contents.as_ref(), LspMarkdownTextSize::Normal);
                });
                let _ = hover.id;
            });

        if close {
            self.lsp_hover = None;
            self.status = "Closed hover".to_owned();
        }
    }
}

pub(crate) fn hover_window_anchor(above: bool) -> (egui::Align2, [f32; 2]) {
    if above {
        (egui::Align2::RIGHT_TOP, [-24.0, 252.0])
    } else {
        (egui::Align2::RIGHT_BOTTOM, [-24.0, -252.0])
    }
}

pub(crate) fn hover_hide_remaining(
    opened_at: Instant,
    now: Instant,
    hiding_delay: Duration,
    sticky: bool,
) -> Option<Duration> {
    if sticky {
        return None;
    }

    Some(hiding_delay.saturating_sub(now.saturating_duration_since(opened_at)))
}

fn cached_hover_target_label(
    ctx: &Context,
    path: &Path,
    line: usize,
    column: usize,
    text_color: Color32,
) -> Arc<RichText> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<LspHoverDisplayCache>(Id::new(LSP_HOVER_DISPLAY_CACHE_ID))
            .target_label(path, line, column, text_color)
    })
}

fn cached_bounded_hover_markdown(
    ctx: &Context,
    hover: &crate::transient_state::LspHoverPopup,
) -> Arc<str> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<LspHoverDisplayCache>(Id::new(LSP_HOVER_DISPLAY_CACHE_ID))
            .contents(hover)
    })
}

#[derive(Clone, Default)]
struct LspHoverDisplayCache {
    target_label: LspHoverTargetLabelCache,
    contents: LspHoverContentsCache,
}

impl LspHoverDisplayCache {
    fn target_label(
        &mut self,
        path: &Path,
        line: usize,
        column: usize,
        text_color: Color32,
    ) -> Arc<RichText> {
        self.target_label.label(path, line, column, text_color)
    }

    fn contents(&mut self, hover: &crate::transient_state::LspHoverPopup) -> Arc<str> {
        self.contents.contents(hover)
    }
}

#[derive(Clone, Default)]
struct LspHoverTargetLabelCache {
    key: Option<LspHoverTargetLabelKey>,
    label: Option<Arc<RichText>>,
}

impl LspHoverTargetLabelCache {
    fn label(
        &mut self,
        path: &Path,
        line: usize,
        column: usize,
        text_color: Color32,
    ) -> Arc<RichText> {
        let target_matches = self
            .key
            .as_ref()
            .is_some_and(|key| key.matches(path, line, column, text_color));
        if target_matches {
            if let Some(label) = &self.label {
                return Arc::clone(label);
            }
        }

        let label = Arc::new(hover_target_rich_text(path, line, column, text_color));
        self.key = Some(LspHoverTargetLabelKey {
            path: path.to_path_buf(),
            line,
            column,
            text_color: color_cache_key(text_color),
        });
        self.label = Some(Arc::clone(&label));
        label
    }
}

#[derive(Clone, PartialEq, Eq)]
struct LspHoverTargetLabelKey {
    path: PathBuf,
    line: usize,
    column: usize,
    text_color: [u8; 4],
}

impl LspHoverTargetLabelKey {
    fn matches(&self, path: &Path, line: usize, column: usize, text_color: Color32) -> bool {
        self.path.as_path() == path
            && self.line == line
            && self.column == column
            && self.text_color == color_cache_key(text_color)
    }
}

#[derive(Clone, Default)]
struct LspHoverContentsCache {
    key: Option<LspHoverContentsCacheKey>,
    contents: Option<Arc<str>>,
}

impl LspHoverContentsCache {
    fn contents(&mut self, hover: &crate::transient_state::LspHoverPopup) -> Arc<str> {
        if self
            .key
            .as_ref()
            .is_some_and(|cached| cached.matches_hover(hover))
        {
            if let Some(contents) = &self.contents {
                return Arc::clone(contents);
            }
        }

        let contents = Arc::<str>::from(bounded_hover_markdown(&hover.contents).into_owned());
        self.key = Some(LspHoverContentsCacheKey::new(hover));
        self.contents = Some(Arc::clone(&contents));
        contents
    }
}

#[derive(Clone, PartialEq, Eq)]
struct LspHoverContentsCacheKey {
    id: BufferId,
    path: PathBuf,
    line: usize,
    column: usize,
    opened_at: Instant,
    source_ptr: usize,
    source_len: usize,
    source_hash: u64,
}

impl LspHoverContentsCacheKey {
    fn new(hover: &crate::transient_state::LspHoverPopup) -> Self {
        Self {
            id: hover.id,
            path: hover.path.clone(),
            line: hover.line,
            column: hover.column,
            opened_at: hover.opened_at,
            source_ptr: hover.contents.as_ptr() as usize,
            source_len: hover.contents.len(),
            source_hash: hover_contents_hash(&hover.contents),
        }
    }

    fn matches_hover(&self, hover: &crate::transient_state::LspHoverPopup) -> bool {
        self.id == hover.id
            && self.path == hover.path
            && self.line == hover.line
            && self.column == hover.column
            && self.opened_at == hover.opened_at
            && self.source_ptr == hover.contents.as_ptr() as usize
            && self.source_len == hover.contents.len()
            && self.source_hash == hover_contents_hash(&hover.contents)
    }
}

fn hover_contents_hash(contents: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn hover_long_line_warning(
    buffer: Option<&TextBuffer>,
    hover: &crate::transient_state::LspHoverPopup,
    stop_rendering_line_after: i64,
    enabled: bool,
) -> Option<String> {
    if !enabled {
        return None;
    }
    let limit = editor_stop_rendering_line_after_limit(stop_rendering_line_after)?;
    let buffer = buffer?;
    if buffer
        .path()
        .is_none_or(|path| !paths_match_lexically(path, &hover.path))
    {
        return None;
    }
    let line_idx = hover.line.checked_sub(1)?;
    let line = buffer.line(line_idx)?;
    let line_length = line.trim_end_matches(['\r', '\n']).chars().count();
    (line_length > limit)
        .then(|| format!("Line has {line_length} characters; rendering is capped at {limit}."))
}

fn hover_popup_target_matches_buffer(
    buffer: Option<&TextBuffer>,
    hover: &crate::transient_state::LspHoverPopup,
) -> bool {
    let Some(buffer) = buffer else {
        return false;
    };
    if buffer.id() != hover.id || hover.line == 0 || hover.column == 0 {
        return false;
    }
    let Some(path) = buffer.path() else {
        return false;
    };
    if !paths_match_lexically(path, &hover.path) {
        return false;
    }
    hover.line - 1 < buffer.len_lines()
}

pub(crate) fn bounded_hover_markdown(contents: &str) -> Cow<'_, str> {
    let contents = contents.trim();
    if contents.len() <= MAX_HOVER_MARKDOWN_CHARS {
        return Cow::Borrowed(contents);
    }

    let Some((cut, _)) = contents.char_indices().nth(MAX_HOVER_MARKDOWN_CHARS) else {
        return Cow::Borrowed(contents);
    };

    let mut bounded = String::with_capacity(cut + HOVER_MARKDOWN_TRUNCATED_NOTICE.len());
    bounded.push_str(contents[..cut].trim_end());
    bounded.push_str(HOVER_MARKDOWN_TRUNCATED_NOTICE);
    Cow::Owned(bounded)
}

fn hover_target_rich_text(
    path: &Path,
    line: usize,
    column: usize,
    text_color: Color32,
) -> RichText {
    RichText::new(hover_target_label(path, line, column))
        .small()
        .color(text_color)
}

fn color_cache_key(color: Color32) -> [u8; 4] {
    [color.r(), color.g(), color.b(), color.a()]
}

fn hover_target_label(path: &Path, line: usize, column: usize) -> String {
    let path = display_path_label_cow(path);
    let mut label = String::with_capacity(path.len() + 24);
    label.push_str(&path);
    let _ = write!(label, ":{line}:{column}");
    label
}

#[cfg(test)]
mod tests {
    use super::{
        LspHoverContentsCache, LspHoverContentsCacheKey, LspHoverTargetLabelCache,
        MAX_HOVER_MARKDOWN_CHARS, bounded_hover_markdown, hover_hide_remaining,
        hover_long_line_warning, hover_popup_target_matches_buffer, hover_target_label,
        hover_window_anchor,
    };
    use crate::{path_display::DISPLAY_PATH_LABEL_MAX_CHARS, transient_state::LspHoverPopup};
    use eframe::egui::{Align2, Color32};
    use kuroya_core::TextBuffer;
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    #[test]
    fn hover_window_anchor_follows_above_setting() {
        assert_eq!(
            hover_window_anchor(true),
            (Align2::RIGHT_TOP, [-24.0, 252.0])
        );
        assert_eq!(
            hover_window_anchor(false),
            (Align2::RIGHT_BOTTOM, [-24.0, -252.0])
        );
    }

    #[test]
    fn hover_hide_remaining_only_counts_down_when_not_sticky() {
        let opened = Instant::now();
        assert_eq!(
            hover_hide_remaining(
                opened,
                opened + Duration::from_millis(50),
                Duration::from_millis(100),
                true
            ),
            None
        );
        assert_eq!(
            hover_hide_remaining(
                opened,
                opened + Duration::from_millis(50),
                Duration::from_millis(100),
                false
            ),
            Some(Duration::from_millis(50))
        );
        assert_eq!(
            hover_hide_remaining(
                opened,
                opened + Duration::from_millis(150),
                Duration::from_millis(100),
                false
            ),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn hover_long_line_warning_follows_setting_path_and_limit() {
        let path = PathBuf::from("src/main.rs");
        let buffer = TextBuffer::from_text(1, Some(path.clone()), "abcdef\n".to_owned());
        let popup = LspHoverPopup {
            id: 1,
            path,
            line: 1,
            column: 1,
            contents: "hover".to_owned(),
            opened_at: Instant::now(),
        };

        assert_eq!(
            hover_long_line_warning(Some(&buffer), &popup, 3, true),
            Some("Line has 6 characters; rendering is capped at 3.".to_owned())
        );
        assert_eq!(
            hover_long_line_warning(Some(&buffer), &popup, 3, false),
            None
        );
        assert_eq!(
            hover_long_line_warning(Some(&buffer), &popup, -1, true),
            None
        );
    }

    #[test]
    fn hover_popup_target_must_match_active_buffer_path_and_line() {
        let path = PathBuf::from("workspace/src/main.rs");
        let equivalent_path = PathBuf::from("workspace/src/../src/main.rs");
        let buffer = TextBuffer::from_text(7, Some(path), "line one\nline two\n".to_owned());
        let popup = LspHoverPopup {
            id: 7,
            path: equivalent_path,
            line: 2,
            column: 1,
            contents: "hover".to_owned(),
            opened_at: Instant::now(),
        };

        assert!(hover_popup_target_matches_buffer(Some(&buffer), &popup));
        assert!(!hover_popup_target_matches_buffer(
            Some(&buffer),
            &LspHoverPopup {
                id: 8,
                ..popup.clone()
            }
        ));
        assert!(!hover_popup_target_matches_buffer(
            Some(&buffer),
            &LspHoverPopup {
                path: PathBuf::from("workspace/other/main.rs"),
                ..popup.clone()
            }
        ));
        assert!(!hover_popup_target_matches_buffer(
            Some(&buffer),
            &LspHoverPopup {
                line: 4,
                ..popup.clone()
            }
        ));
        assert!(!hover_popup_target_matches_buffer(None, &popup));
    }

    #[test]
    fn hover_target_label_sanitizes_and_bounds_path_text() {
        let path = PathBuf::from("workspace").join(format!(
            "hover\n{}\u{202e}.rs",
            "target-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));

        let label = hover_target_label(&path, 8, 13);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.ends_with(":8:13"));
        assert!(
            label.trim_end_matches(":8:13").chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS,
            "hover target path should be bounded: {label:?}"
        );
    }

    #[test]
    fn hover_target_label_cache_reuses_styled_label_until_target_changes() {
        let path = PathBuf::from("workspace/src/main.rs");
        let mut cache = LspHoverTargetLabelCache::default();

        let first = cache.label(&path, 8, 13, Color32::WHITE);
        let second = cache.label(&path, 8, 13, Color32::WHITE);

        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(first.text(), "main.rs:8:13");

        let third = cache.label(&path, 8, 14, Color32::WHITE);

        assert!(!std::sync::Arc::ptr_eq(&first, &third));
        assert_eq!(third.text(), "main.rs:8:14");
    }

    #[test]
    fn hover_contents_cache_reuses_bounded_display_without_mutating_payload() {
        let contents = format!("  {}tail  ", "a".repeat(MAX_HOVER_MARKDOWN_CHARS + 24));
        let popup = LspHoverPopup {
            id: 7,
            path: PathBuf::from("workspace/src/main.rs"),
            line: 8,
            column: 13,
            contents,
            opened_at: Instant::now(),
        };
        let mut cache = LspHoverContentsCache::default();

        let first = cache.contents(&popup);
        let second = cache.contents(&popup);

        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert!(first.ends_with("[Hover truncated]"));
        assert!(!first.contains("tail"));
        assert!(popup.contents.contains("tail"));
    }

    #[test]
    fn hover_contents_cache_refreshes_for_new_popup_payload() {
        let now = Instant::now();
        let first = LspHoverPopup {
            id: 7,
            path: PathBuf::from("workspace/src/main.rs"),
            line: 8,
            column: 13,
            contents: "first".to_owned(),
            opened_at: now,
        };
        let second = LspHoverPopup {
            contents: "second".to_owned(),
            opened_at: now + Duration::from_millis(1),
            ..first.clone()
        };
        let mut cache = LspHoverContentsCache::default();

        let first_contents = cache.contents(&first);
        let second_contents = cache.contents(&second);

        assert!(!std::sync::Arc::ptr_eq(&first_contents, &second_contents));
        assert_eq!(first_contents.as_ref(), "first");
        assert_eq!(second_contents.as_ref(), "second");
    }

    #[test]
    fn hover_contents_cache_refreshes_when_same_string_slot_changes() {
        let now = Instant::now();
        let mut popup = LspHoverPopup {
            id: 7,
            path: PathBuf::from("workspace/src/main.rs"),
            line: 8,
            column: 13,
            contents: "first".to_owned(),
            opened_at: now,
        };
        let mut cache = LspHoverContentsCache::default();
        let first_key = LspHoverContentsCacheKey::new(&popup);
        let first_contents = cache.contents(&popup);

        popup.contents.replace_range(.., "other");
        let second_key = LspHoverContentsCacheKey::new(&popup);
        let second_contents = cache.contents(&popup);

        assert_eq!(first_key.source_ptr, second_key.source_ptr);
        assert_eq!(first_key.source_len, second_key.source_len);
        assert_ne!(first_key.source_hash, second_key.source_hash);
        assert!(!std::sync::Arc::ptr_eq(&first_contents, &second_contents));
        assert_eq!(first_contents.as_ref(), "first");
        assert_eq!(second_contents.as_ref(), "other");
    }

    #[test]
    fn bounded_hover_markdown_trims_and_leaves_small_hovers_borrowed() {
        let hover = bounded_hover_markdown("  `Vec::new`  ");

        assert!(matches!(hover, std::borrow::Cow::Borrowed("`Vec::new`")));
    }

    #[test]
    fn bounded_hover_markdown_caps_pathological_payloads() {
        let contents = format!("{}tail", "a".repeat(MAX_HOVER_MARKDOWN_CHARS + 24));
        let bounded = bounded_hover_markdown(&contents);

        assert!(matches!(bounded, std::borrow::Cow::Owned(_)));
        assert!(bounded.ends_with("[Hover truncated]"));
        assert!(!bounded.contains("tail"));
    }

    #[test]
    fn bounded_hover_markdown_preserves_utf8_boundaries() {
        let contents = format!("{}tail", "\u{03b1}".repeat(MAX_HOVER_MARKDOWN_CHARS + 4));
        let bounded = bounded_hover_markdown(&contents);

        assert!(bounded.starts_with('\u{03b1}'));
        assert!(bounded.ends_with("[Hover truncated]"));
        assert!(!bounded.contains("tail"));
    }
}

use crate::editor_text_geometry::visual_width_for_char;
use egui::{Color32, FontFamily, FontId, TextFormat, text::LayoutJob};
use syntect::{
    highlighting::{HighlightIterator, HighlightState, Highlighter, Style},
    parsing::ScopeStackOp,
};

const DEFAULT_FONT_SIZE: f32 = 13.0;
const MAX_FONT_SIZE: f32 = 128.0;
const DEFAULT_TAB_WIDTH: usize = 4;
const MAX_TAB_WIDTH: usize = 32;
// Below this WCAG contrast ratio a syntect token foreground is unreadable
// against the active theme and falls back to the theme text color.
const MIN_TOKEN_CONTRAST_RATIO: f32 = 3.0;
// The editor background is not threaded into the highlighter, so it is
// estimated from the theme family picked by the text color polarity (the app
// guarantees its text color is readable against its background). The values
// mirror the app's default dark and light theme backgrounds.
const ESTIMATED_DARK_BACKGROUND: Color32 = Color32::from_rgb(18, 20, 24);
const ESTIMATED_LIGHT_BACKGROUND: Color32 = Color32::from_rgb(244, 246, 248);

/// Render-relevant identity of the active app theme for syntect token colors.
///
/// The bundled syntect theme is fixed, so token colors are contrast-checked
/// against the active app theme at paint time: pale token colors designed for
/// dark backgrounds fall back to the theme text color instead of washing out on
/// light themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyntaxThemeColors {
    background: Color32,
    text: Color32,
}

impl SyntaxThemeColors {
    pub(crate) fn from_text_color(text: Color32) -> Self {
        let background = if relative_luminance(text) >= 0.5 {
            ESTIMATED_DARK_BACKGROUND
        } else {
            ESTIMATED_LIGHT_BACKGROUND
        };
        Self { background, text }
    }

    pub(crate) fn readable_token_color(self, token: Color32) -> Color32 {
        if contrast_ratio(token, self.background) >= MIN_TOKEN_CONTRAST_RATIO {
            token
        } else {
            self.text
        }
    }
}

fn contrast_ratio(first: Color32, second: Color32) -> f32 {
    let bright = relative_luminance(first).max(relative_luminance(second));
    let dark = relative_luminance(first).min(relative_luminance(second));
    (bright + 0.05) / (dark + 0.05)
}

fn relative_luminance(color: Color32) -> f32 {
    let red = srgb_channel_to_linear(color.r());
    let green = srgb_channel_to_linear(color.g());
    let blue = srgb_channel_to_linear(color.b());
    (0.2126 * red) + (0.7152 * green) + (0.0722 * blue)
}

fn srgb_channel_to_linear(value: u8) -> f32 {
    let channel = value as f32 / 255.0;
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) fn normalize_layout_inputs(font_size: f32, tab_width: usize) -> (f32, usize) {
    let font_size = if font_size.is_finite() && font_size > 0.0 {
        font_size.min(MAX_FONT_SIZE)
    } else {
        DEFAULT_FONT_SIZE
    };
    let tab_width = if tab_width == 0 {
        DEFAULT_TAB_WIDTH
    } else {
        tab_width.min(MAX_TAB_WIDTH)
    };
    (font_size, tab_width)
}

pub(crate) fn highlighted_job(
    text: &str,
    ops: &[(usize, ScopeStackOp)],
    highlight_state: &mut HighlightState,
    highlighter: &Highlighter<'_>,
    font_size: f32,
    tab_width: usize,
    theme: SyntaxThemeColors,
) -> LayoutJob {
    let (font_size, tab_width) = normalize_layout_inputs(font_size, tab_width);
    let mut job = layout_job_with_text_capacity(expanded_text_capacity(text, tab_width));
    let mut visual_column = 0usize;
    for (style, slice) in HighlightIterator::new(highlight_state, ops, text, highlighter) {
        append_text_with_expanded_tabs(
            &mut job,
            slice,
            format_from_style(style, font_size, theme),
            &mut visual_column,
            tab_width,
        );
    }
    job
}

pub(crate) fn advance_highlight_state(
    text: &str,
    ops: &[(usize, ScopeStackOp)],
    highlight_state: &mut HighlightState,
    highlighter: &Highlighter<'_>,
) {
    for _ in HighlightIterator::new(highlight_state, ops, text, highlighter) {}
}

fn format_from_style(style: Style, font_size: f32, theme: SyntaxThemeColors) -> TextFormat {
    TextFormat {
        font_id: FontId::new(font_size, FontFamily::Monospace),
        color: theme.readable_token_color(Color32::from_rgb(
            style.foreground.r,
            style.foreground.g,
            style.foreground.b,
        )),
        ..Default::default()
    }
}

pub(crate) fn plain_job(
    text: &str,
    font_size: f32,
    tab_width: usize,
    text_color: Color32,
) -> LayoutJob {
    let (font_size, tab_width) = normalize_layout_inputs(font_size, tab_width);
    let mut job = layout_job_with_text_capacity(expanded_text_capacity(text, tab_width));
    let mut visual_column = 0usize;
    append_text_with_expanded_tabs(
        &mut job,
        text,
        plain_format(font_size, text_color),
        &mut visual_column,
        tab_width,
    );
    job
}

fn layout_job_with_text_capacity(capacity: usize) -> LayoutJob {
    LayoutJob {
        text: String::with_capacity(capacity),
        ..Default::default()
    }
}

fn plain_format(font_size: f32, text_color: Color32) -> TextFormat {
    TextFormat {
        font_id: FontId::new(font_size, FontFamily::Monospace),
        color: text_color,
        ..Default::default()
    }
}

fn append_text_with_expanded_tabs(
    job: &mut LayoutJob,
    text: &str,
    format: TextFormat,
    visual_column: &mut usize,
    tab_width: usize,
) {
    if !text.as_bytes().contains(&b'\t') {
        *visual_column += text_columns_without_tabs(text);
        job.append(text, 0.0, format);
        return;
    }

    let tab_width = tab_width.max(1);
    let mut run_start = 0usize;
    for (byte_idx, byte) in text.as_bytes().iter().enumerate() {
        if *byte != b'\t' {
            continue;
        }

        if run_start < byte_idx {
            let run = &text[run_start..byte_idx];
            *visual_column += text_columns_without_tabs(run);
            job.append(run, 0.0, format.clone());
        }

        let spaces = visual_width_for_char('\t', *visual_column, tab_width);
        append_spaces(job, spaces, format.clone());
        *visual_column += spaces;
        run_start = byte_idx + 1;
    }

    if run_start < text.len() {
        let run = &text[run_start..];
        *visual_column += text_columns_without_tabs(run);
        job.append(run, 0.0, format);
    }
}

fn text_columns_without_tabs(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().count()
    }
}

fn expanded_text_capacity(text: &str, tab_width: usize) -> usize {
    if !text.as_bytes().contains(&b'\t') {
        return text.len();
    }

    let tab_width = tab_width.max(1);
    let mut capacity = 0usize;
    let mut visual_column = 0usize;
    let mut run_start = 0usize;

    for (byte_idx, byte) in text.as_bytes().iter().enumerate() {
        if *byte != b'\t' {
            continue;
        }

        if run_start < byte_idx {
            let run = &text[run_start..byte_idx];
            capacity += run.len();
            visual_column += text_columns_without_tabs(run);
        }

        let spaces = visual_width_for_char('\t', visual_column, tab_width);
        capacity += spaces;
        visual_column += spaces;
        run_start = byte_idx + 1;
    }

    if run_start < text.len() {
        capacity += text[run_start..].len();
    }

    capacity
}

fn append_spaces(job: &mut LayoutJob, spaces: usize, format: TextFormat) {
    const SPACES: &str = "                                ";

    if spaces <= SPACES.len() {
        job.append(&SPACES[..spaces], 0.0, format);
    } else {
        let repeated = " ".repeat(spaces);
        job.append(&repeated, 0.0, format);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ESTIMATED_DARK_BACKGROUND, ESTIMATED_LIGHT_BACKGROUND, MAX_TAB_WIDTH, SyntaxThemeColors,
        advance_highlight_state, append_spaces, expanded_text_capacity, format_from_style,
        highlighted_job, plain_job as build_plain_job, text_columns_without_tabs,
    };
    use egui::{TextFormat, text::LayoutJob};
    use syntect::{
        highlighting::{Color, FontStyle, HighlightState, Highlighter, Style, ThemeSet},
        parsing::{ParseState, ScopeStack, SyntaxSet},
    };

    fn plain_job(text: &str, font_size: f32, tab_width: usize) -> LayoutJob {
        build_plain_job(text, font_size, tab_width, egui::Color32::WHITE)
    }

    fn style_with_foreground(foreground: Color) -> Style {
        Style {
            foreground,
            background: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            font_style: FontStyle::default(),
        }
    }

    #[test]
    fn plain_layout_uses_supplied_theme_text_color() {
        let text_color = egui::Color32::from_rgb(17, 29, 43);
        let job = build_plain_job("text", 13.0, 4, text_color);

        assert_eq!(job.sections[0].format.color, text_color);
    }

    #[test]
    fn plain_layout_expands_tabs_to_configured_tab_stops() {
        assert_eq!(plain_job("\ta", 13.0, 4).text, "    a");
        assert_eq!(plain_job("ab\tc", 13.0, 4).text, "ab  c");
    }

    #[test]
    fn tab_expansion_appends_contiguous_non_tab_runs() {
        let job = plain_job("a\tbc\t🙂", 13.0, 4);

        assert_eq!(job.text, "a   bc  🙂");
        assert_eq!(job.sections.len(), 5);
        assert_eq!(job.sections[0].byte_range, 0..1);
        assert_eq!(job.sections[1].byte_range, 1..4);
        assert_eq!(job.sections[2].byte_range, 4..6);
        assert_eq!(job.sections[3].byte_range, 6..8);
        assert_eq!(job.sections[4].byte_range, 8..12);
    }

    #[test]
    fn text_columns_without_tabs_uses_byte_len_only_for_ascii() {
        assert_eq!(text_columns_without_tabs("abc"), 3);
        assert_eq!(text_columns_without_tabs("🙂bc"), 3);
    }

    #[test]
    fn append_spaces_handles_common_and_large_tab_widths() {
        let mut common = LayoutJob::default();
        append_spaces(&mut common, 4, TextFormat::default());
        assert_eq!(common.text, "    ");

        let mut large = LayoutJob::default();
        append_spaces(&mut large, 40, TextFormat::default());
        assert_eq!(large.text.len(), 40);
        assert!(large.text.chars().all(|ch| ch == ' '));
    }

    #[test]
    fn expanded_text_capacity_accounts_for_tab_expansion() {
        assert_eq!(expanded_text_capacity("abc", 4), 3);
        assert_eq!(expanded_text_capacity("\ta", 4), 5);
        assert_eq!(expanded_text_capacity("ab\tc", 4), 5);
        assert_eq!(expanded_text_capacity("a\tbc\t\u{1f642}", 4), 12);
    }

    #[test]
    fn tabbed_plain_jobs_preallocate_expanded_text_capacity() {
        let job = plain_job("a\tbc\t\u{1f642}", 13.0, 4);

        assert_eq!(job.text, "a   bc  \u{1f642}");
        assert_eq!(job.text.capacity(), job.text.len());
    }

    #[test]
    fn layout_jobs_use_safe_defaults_for_invalid_inputs() {
        assert_eq!(plain_job("\ta", f32::NAN, 0).text, "    a");
        assert_eq!(
            plain_job("\ta", 13.0, usize::MAX).text.len(),
            MAX_TAB_WIDTH + 1
        );
    }

    #[test]
    fn advance_highlight_state_matches_highlighted_job_for_next_line() {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let syntax = syntaxes.find_syntax_by_extension("rs").unwrap();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get("base16-ocean.dark")
            .or_else(|| theme_set.themes.values().next())
            .unwrap();
        let highlighter = Highlighter::new(theme);
        let first_line = "/* comment";
        let next_line = "still comment */";

        let mut job_parse_state = ParseState::new(syntax);
        let mut job_highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
        let theme = SyntaxThemeColors::from_text_color(egui::Color32::WHITE);
        let first_ops = job_parse_state.parse_line(first_line, &syntaxes).unwrap();
        let _ = highlighted_job(
            first_line,
            &first_ops,
            &mut job_highlight_state,
            &highlighter,
            13.0,
            4,
            theme,
        );
        let next_ops = job_parse_state.parse_line(next_line, &syntaxes).unwrap();
        let after_job_replay = highlighted_job(
            next_line,
            &next_ops,
            &mut job_highlight_state,
            &highlighter,
            13.0,
            4,
            theme,
        );

        let mut advance_parse_state = ParseState::new(syntax);
        let mut advance_highlight_state_value =
            HighlightState::new(&highlighter, ScopeStack::new());
        let first_ops = advance_parse_state
            .parse_line(first_line, &syntaxes)
            .unwrap();
        advance_highlight_state(
            first_line,
            &first_ops,
            &mut advance_highlight_state_value,
            &highlighter,
        );
        let next_ops = advance_parse_state
            .parse_line(next_line, &syntaxes)
            .unwrap();
        let after_state_advance = highlighted_job(
            next_line,
            &next_ops,
            &mut advance_highlight_state_value,
            &highlighter,
            13.0,
            4,
            theme,
        );

        let job_replay_sections = after_job_replay
            .sections
            .iter()
            .map(|section| (section.byte_range.clone(), section.format.color))
            .collect::<Vec<_>>();
        let state_advance_sections = after_state_advance
            .sections
            .iter()
            .map(|section| (section.byte_range.clone(), section.format.color))
            .collect::<Vec<_>>();

        assert_eq!(after_job_replay.text, after_state_advance.text);
        assert_eq!(job_replay_sections, state_advance_sections);
    }

    #[test]
    fn syntax_theme_colors_estimate_background_from_text_polarity() {
        let dark_theme = SyntaxThemeColors::from_text_color(egui::Color32::WHITE);
        let light_theme = SyntaxThemeColors::from_text_color(egui::Color32::from_rgb(36, 41, 49));

        assert_eq!(dark_theme.background, ESTIMATED_DARK_BACKGROUND);
        assert_eq!(light_theme.background, ESTIMATED_LIGHT_BACKGROUND);
        assert_eq!(dark_theme.text, egui::Color32::WHITE);
        assert_eq!(light_theme.text, egui::Color32::from_rgb(36, 41, 49));
    }

    #[test]
    fn format_from_style_falls_back_to_theme_text_for_pale_tokens_on_light_themes() {
        // The base16-ocean.dark default foreground: far too pale for light apps.
        let pale_token = style_with_foreground(Color {
            r: 192,
            g: 197,
            b: 206,
            a: 255,
        });
        // The ocean comment color is dark enough to survive on a light theme.
        let readable_token = style_with_foreground(Color {
            r: 101,
            g: 115,
            b: 126,
            a: 255,
        });
        let light_theme_text = egui::Color32::from_rgb(36, 41, 49);
        let light_theme = SyntaxThemeColors::from_text_color(light_theme_text);
        let dark_theme = SyntaxThemeColors::from_text_color(egui::Color32::WHITE);

        let pale_on_light = format_from_style(pale_token, 13.0, light_theme);
        let readable_on_light = format_from_style(readable_token, 13.0, light_theme);
        let pale_on_dark = format_from_style(pale_token, 13.0, dark_theme);

        assert_eq!(pale_on_light.color, light_theme_text);
        assert_eq!(
            readable_on_light.color,
            egui::Color32::from_rgb(101, 115, 126)
        );
        // Dark themes keep the original syntect token color.
        assert_eq!(pale_on_dark.color, egui::Color32::from_rgb(192, 197, 206));
    }

    #[test]
    fn readable_token_color_is_idempotent_for_highlighted_output() {
        let light_theme = SyntaxThemeColors::from_text_color(egui::Color32::from_rgb(36, 41, 49));

        for token in [
            egui::Color32::from_rgb(192, 197, 206),
            egui::Color32::from_rgb(101, 115, 126),
            egui::Color32::from_rgb(36, 41, 49),
        ] {
            let readable = light_theme.readable_token_color(token);
            assert_eq!(light_theme.readable_token_color(readable), readable);
        }
    }
}

use crate::{
    KuroyaApp,
    lsp_markdown_render::{LspMarkdownTextSize, render_lsp_inline_markdown, render_lsp_markdown},
    path_display::display_path_label_cow,
    popup_buttons::{PopupButtonKind, popup_button},
};
use eframe::egui::{
    self, Align, Color32, Context, FontFamily, FontId, Id, Key, RichText, ScrollArea, TextFormat,
    text::LayoutJob,
};
use kuroya_core::{BufferId, LspParameterInformation, LspSignatureHelp, LspSignatureInformation};
use std::{
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    fmt::Write,
    hash::{Hash, Hasher},
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_SIGNATURE_MARKDOWN_CHARS: usize = 8_000;
const MAX_SIGNATURE_PARAMETER_MARKDOWN_CHARS: usize = 2_000;
const MAX_SIGNATURE_LABEL_DISPLAY_CHARS: usize = 400;
const MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS: usize = 160;
const SIGNATURE_MARKDOWN_TRUNCATED_NOTICE: &str = "\n\n[Signature documentation truncated]";
const SIGNATURE_PARAMETER_MARKDOWN_TRUNCATED_NOTICE: &str = " [Parameter documentation truncated]";
const SIGNATURE_LABEL_TRUNCATED_NOTICE: &str = "...";
const LSP_SIGNATURE_DISPLAY_CACHE_ID: &str = "kuroya.lsp_signature.display_cache";

impl KuroyaApp {
    pub(crate) fn render_signature_help(&mut self, ctx: &Context) {
        let Some(popup) = self.signature_help.as_ref() else {
            return;
        };
        let mut close = false;
        let mut selected_signature = None;

        egui::Window::new("Signature Help")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 172.0,
            ))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 148.0])
            .default_size([620.0, 240.0])
            .show(ctx, |ui| {
                let display = cached_signature_popup_display(
                    ui.ctx(),
                    popup,
                    ui.visuals().text_color(),
                    ui.visuals().weak_text_color(),
                    ui.visuals().selection.bg_fill,
                );
                ui.horizontal(|ui| {
                    ui.label(Arc::clone(&display.target_label));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if popup_button(ui, "Close", PopupButtonKind::Secondary).clicked() {
                            close = true;
                        }
                    });
                });

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }

                ui.separator();
                if let Some(signature) = &display.signature {
                    ui.label(signature.label_job.clone());
                    if let Some(documentation) = &signature.documentation {
                        ui.add_space(4.0);
                        render_lsp_markdown(ui, documentation, LspMarkdownTextSize::Small);
                    }
                    if !signature.parameters.is_empty() {
                        ui.separator();
                        ScrollArea::vertical().max_height(104.0).show(ui, |ui| {
                            for (idx, parameter) in signature.parameters.iter().enumerate() {
                                let active = popup.help.active_parameter == Some(idx);
                                let label = RichText::new(parameter.label.as_str()).monospace();
                                let label = if active {
                                    label.strong().color(ui.visuals().text_color())
                                } else {
                                    label
                                };
                                ui.horizontal(|ui| {
                                    ui.label(label);
                                    if let Some(documentation) = &parameter.documentation {
                                        render_lsp_inline_markdown(
                                            ui,
                                            documentation,
                                            LspMarkdownTextSize::Small,
                                            ui.visuals().weak_text_color(),
                                        );
                                    }
                                });
                            }
                        });
                    }
                    if let Some(footer) = &display.footer {
                        ui.label(Arc::clone(footer));
                    }
                    if popup.help.signatures.len() > 1 {
                        ui.separator();
                        ui.horizontal(|ui| {
                            let previous = signature_help_step_index(
                                popup.help.active_signature,
                                popup.help.signatures.len(),
                                -1,
                                self.settings.parameter_hints_cycle,
                            );
                            let next = signature_help_step_index(
                                popup.help.active_signature,
                                popup.help.signatures.len(),
                                1,
                                self.settings.parameter_hints_cycle,
                            );
                            let can_previous = self.settings.parameter_hints_cycle
                                || previous < popup.help.active_signature;
                            let can_next = self.settings.parameter_hints_cycle
                                || next > popup.help.active_signature;
                            if ui
                                .add_enabled(can_previous, egui::Button::new("Previous"))
                                .clicked()
                            {
                                selected_signature = Some(previous);
                            }
                            if ui
                                .add_enabled(can_next, egui::Button::new("Next"))
                                .clicked()
                            {
                                selected_signature = Some(next);
                            }
                        });
                    }
                } else {
                    ui.add_space(24.0);
                    ui.centered_and_justified(|ui| {
                        ui.label("No signature help");
                    });
                }
                let _ = popup.id;
            });

        if close {
            self.signature_help = None;
            self.status = "Closed signature help".to_owned();
        } else if let Some(active_signature) = selected_signature
            && let Some(current) = &mut self.signature_help
        {
            current.help.active_signature = active_signature;
        }
    }
}

fn cached_signature_popup_display(
    ctx: &Context,
    popup: &crate::transient_state::LspSignatureHelpPopup,
    text_color: Color32,
    weak_text_color: Color32,
    active_background: Color32,
) -> Arc<SignaturePopupDisplay> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<LspSignatureDisplayCache>(Id::new(
            LSP_SIGNATURE_DISPLAY_CACHE_ID,
        ))
        .display_for_popup(popup, text_color, weak_text_color, active_background)
    })
}

#[derive(Clone, Default)]
struct LspSignatureDisplayCache {
    display: Option<SignaturePopupDisplayCacheEntry>,
}

impl LspSignatureDisplayCache {
    fn display_for_popup(
        &mut self,
        popup: &crate::transient_state::LspSignatureHelpPopup,
        text_color: Color32,
        weak_text_color: Color32,
        active_background: Color32,
    ) -> Arc<SignaturePopupDisplay> {
        if let Some(entry) = &self.display
            && entry
                .key
                .matches_popup(popup, text_color, weak_text_color, active_background)
        {
            return Arc::clone(&entry.display);
        }

        let key = SignaturePopupDisplayCacheKey::new(
            popup,
            text_color,
            weak_text_color,
            active_background,
        );
        let display = Arc::new(SignaturePopupDisplay::new(
            popup,
            text_color,
            weak_text_color,
            active_background,
        ));
        self.display = Some(SignaturePopupDisplayCacheEntry {
            key,
            display: Arc::clone(&display),
        });
        display
    }
}

#[derive(Clone)]
struct SignaturePopupDisplayCacheEntry {
    key: SignaturePopupDisplayCacheKey,
    display: Arc<SignaturePopupDisplay>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SignaturePopupDisplayCacheKey {
    id: BufferId,
    path: PathBuf,
    line: usize,
    column: usize,
    active_signature: usize,
    active_parameter: Option<usize>,
    signature_count: usize,
    text_color: [u8; 4],
    weak_text_color: [u8; 4],
    active_background: [u8; 4],
    source_fingerprint: u64,
}

impl SignaturePopupDisplayCacheKey {
    fn new(
        popup: &crate::transient_state::LspSignatureHelpPopup,
        text_color: Color32,
        weak_text_color: Color32,
        active_background: Color32,
    ) -> Self {
        Self {
            id: popup.id,
            path: popup.path.clone(),
            line: popup.line,
            column: popup.column,
            active_signature: popup.help.active_signature,
            active_parameter: popup.help.active_parameter,
            signature_count: popup.help.signatures.len(),
            text_color: signature_color_key(text_color),
            weak_text_color: signature_color_key(weak_text_color),
            active_background: signature_color_key(active_background),
            source_fingerprint: signature_help_source_fingerprint(&popup.help),
        }
    }

    fn matches_popup(
        &self,
        popup: &crate::transient_state::LspSignatureHelpPopup,
        text_color: Color32,
        weak_text_color: Color32,
        active_background: Color32,
    ) -> bool {
        self.id == popup.id
            && self.line == popup.line
            && self.column == popup.column
            && self.active_signature == popup.help.active_signature
            && self.active_parameter == popup.help.active_parameter
            && self.signature_count == popup.help.signatures.len()
            && self.text_color == signature_color_key(text_color)
            && self.weak_text_color == signature_color_key(weak_text_color)
            && self.active_background == signature_color_key(active_background)
            && self.path.as_path() == popup.path.as_path()
            && self.source_fingerprint == signature_help_source_fingerprint(&popup.help)
    }
}

struct SignaturePopupDisplay {
    target_label: Arc<RichText>,
    signature: Option<SignatureDisplay>,
    footer: Option<Arc<RichText>>,
}

impl SignaturePopupDisplay {
    fn new(
        popup: &crate::transient_state::LspSignatureHelpPopup,
        text_color: Color32,
        weak_text_color: Color32,
        active_background: Color32,
    ) -> Self {
        let signature = signature_help_visible_signature(&popup.help).map(|signature| {
            SignatureDisplay::new(
                signature,
                popup.help.active_parameter,
                text_color,
                active_background,
            )
        });
        let footer = signature.as_ref().map(|_| {
            Arc::new(
                RichText::new(format!(
                    "Signature {} of {}",
                    popup.help.active_signature + 1,
                    popup.help.signatures.len()
                ))
                .small()
                .color(weak_text_color),
            )
        });

        Self {
            target_label: Arc::new(signature_target_rich_text(
                &popup.path,
                popup.line,
                popup.column,
                weak_text_color,
            )),
            signature,
            footer,
        }
    }
}

struct SignatureDisplay {
    label_job: LayoutJob,
    documentation: Option<String>,
    parameters: Vec<SignatureParameterDisplay>,
}

impl SignatureDisplay {
    fn new(
        signature: &LspSignatureInformation,
        active_parameter: Option<usize>,
        text_color: Color32,
        active_background: Color32,
    ) -> Self {
        Self {
            label_job: signature_label_job(
                &signature.label,
                &signature.parameters,
                active_parameter,
                text_color,
                active_background,
            ),
            documentation: signature
                .documentation
                .as_deref()
                .map(|documentation| bounded_signature_markdown(documentation).into_owned()),
            parameters: signature
                .parameters
                .iter()
                .map(SignatureParameterDisplay::new)
                .collect(),
        }
    }
}

struct SignatureParameterDisplay {
    label: String,
    documentation: Option<String>,
}

impl SignatureParameterDisplay {
    fn new(parameter: &LspParameterInformation) -> Self {
        Self {
            label: bounded_signature_display_text(
                &parameter.label,
                MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS,
            )
            .into_owned(),
            documentation: parameter.documentation.as_deref().map(|documentation| {
                bounded_signature_parameter_markdown(documentation).into_owned()
            }),
        }
    }
}

fn bounded_signature_markdown(contents: &str) -> Cow<'_, str> {
    bounded_signature_documentation(
        contents,
        MAX_SIGNATURE_MARKDOWN_CHARS,
        SIGNATURE_MARKDOWN_TRUNCATED_NOTICE,
    )
}

fn bounded_signature_parameter_markdown(contents: &str) -> Cow<'_, str> {
    bounded_signature_documentation(
        contents,
        MAX_SIGNATURE_PARAMETER_MARKDOWN_CHARS,
        SIGNATURE_PARAMETER_MARKDOWN_TRUNCATED_NOTICE,
    )
}

fn signature_target_rich_text(
    path: &Path,
    line: usize,
    column: usize,
    text_color: Color32,
) -> RichText {
    RichText::new(signature_target_label(path, line, column))
        .small()
        .color(text_color)
}

fn signature_target_label(path: &Path, line: usize, column: usize) -> String {
    let path = display_path_label_cow(path);
    let mut label = String::with_capacity(path.len() + 24);
    label.push_str(&path);
    let _ = write!(label, ":{line}:{column}");
    label
}

fn signature_help_visible_signature(help: &LspSignatureHelp) -> Option<&LspSignatureInformation> {
    help.signatures
        .get(help.active_signature)
        .or_else(|| help.signatures.first())
}

fn signature_color_key(color: Color32) -> [u8; 4] {
    [color.r(), color.g(), color.b(), color.a()]
}

fn signature_help_source_fingerprint(help: &LspSignatureHelp) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Some(signature) = signature_help_visible_signature(help) {
        signature_source_fingerprint(signature, &mut hasher);
    }
    hasher.finish()
}

fn signature_source_fingerprint(signature: &LspSignatureInformation, hasher: &mut impl Hasher) {
    signature_source_str_fingerprint(&signature.label, hasher);
    signature_source_option_str_fingerprint(signature.documentation.as_deref(), hasher);
    signature.parameters.len().hash(hasher);
    for parameter in &signature.parameters {
        signature_source_str_fingerprint(&parameter.label, hasher);
        signature_source_option_str_fingerprint(parameter.documentation.as_deref(), hasher);
    }
}

fn signature_source_option_str_fingerprint(value: Option<&str>, hasher: &mut impl Hasher) {
    value.is_some().hash(hasher);
    if let Some(value) = value {
        signature_source_str_fingerprint(value, hasher);
    }
}

fn signature_source_str_fingerprint(value: &str, hasher: &mut impl Hasher) {
    (value.as_ptr() as usize).hash(hasher);
    value.len().hash(hasher);
}

fn bounded_signature_documentation<'a>(
    contents: &'a str,
    max_chars: usize,
    notice: &str,
) -> Cow<'a, str> {
    let contents = contents.trim();
    if contents.len() <= max_chars {
        return Cow::Borrowed(contents);
    }

    let Some((cut, _)) = contents.char_indices().nth(max_chars) else {
        return Cow::Borrowed(contents);
    };

    let mut bounded = String::with_capacity(cut + notice.len());
    bounded.push_str(contents[..cut].trim_end());
    bounded.push_str(notice);
    Cow::Owned(bounded)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignatureDisplayLabel<'a> {
    text: Cow<'a, str>,
    raw_spans: Vec<SignatureDisplaySpan>,
    raw_is_display: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SignatureDisplaySpan {
    raw: Range<usize>,
    display: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SignatureDisplayBounds {
    cut_byte: usize,
    truncated: bool,
    needs_owned: bool,
}

#[cfg(test)]
fn bounded_signature_display_label(label: &str, max_chars: usize) -> SignatureDisplayLabel<'_> {
    let bounds = signature_display_bounds(label, max_chars);
    bounded_signature_display_label_with_bounds(label, max_chars, bounds)
}

fn bounded_signature_display_label_with_bounds(
    label: &str,
    max_chars: usize,
    bounds: SignatureDisplayBounds,
) -> SignatureDisplayLabel<'_> {
    if !bounds.needs_owned {
        return SignatureDisplayLabel {
            text: Cow::Borrowed(label),
            raw_spans: Vec::new(),
            raw_is_display: true,
        };
    }

    let mut text = String::with_capacity(signature_display_text_capacity(
        label,
        max_chars,
        bounds.truncated,
    ));
    let mut raw_spans = Vec::with_capacity(max_chars.min(bounds.cut_byte));
    let mut displayed_chars = 0;
    for (raw_start, ch) in label[..bounds.cut_byte].char_indices() {
        if displayed_chars == max_chars {
            break;
        }
        if is_signature_label_bidi_format_control(ch) {
            continue;
        }

        let display_ch = if is_signature_label_line_or_control(ch) {
            ' '
        } else {
            ch
        };
        let raw_end = raw_start + ch.len_utf8();
        let display_start = text.len();
        text.push(display_ch);
        let display_end = text.len();
        raw_spans.push(SignatureDisplaySpan {
            raw: raw_start..raw_end,
            display: display_start..display_end,
        });
        displayed_chars += 1;
    }

    if bounds.truncated {
        text.push_str(SIGNATURE_LABEL_TRUNCATED_NOTICE);
    }

    SignatureDisplayLabel {
        text: Cow::Owned(text),
        raw_spans,
        raw_is_display: false,
    }
}

fn bounded_signature_display_text(label: &str, max_chars: usize) -> Cow<'_, str> {
    let bounds = signature_display_bounds(label, max_chars);
    bounded_signature_display_text_with_bounds(label, max_chars, bounds)
}

fn bounded_signature_display_text_with_bounds(
    label: &str,
    max_chars: usize,
    bounds: SignatureDisplayBounds,
) -> Cow<'_, str> {
    if !bounds.needs_owned {
        return Cow::Borrowed(label);
    }

    Cow::Owned(owned_signature_display_text(label, max_chars, bounds))
}

fn signature_display_bounds(label: &str, max_chars: usize) -> SignatureDisplayBounds {
    if max_chars == 0 {
        return SignatureDisplayBounds {
            cut_byte: 0,
            truncated: false,
            needs_owned: !label.is_empty(),
        };
    }

    let mut displayed_chars = 0;
    let mut needs_owned = false;
    for (raw_start, ch) in label.char_indices() {
        if displayed_chars == max_chars {
            return SignatureDisplayBounds {
                cut_byte: raw_start,
                truncated: true,
                needs_owned: true,
            };
        }
        if is_signature_label_bidi_format_control(ch) {
            needs_owned = true;
            continue;
        }

        needs_owned |= is_signature_label_line_or_control(ch);
        displayed_chars += 1;
    }

    SignatureDisplayBounds {
        cut_byte: label.len(),
        truncated: false,
        needs_owned,
    }
}

fn owned_signature_display_text(
    label: &str,
    max_chars: usize,
    bounds: SignatureDisplayBounds,
) -> String {
    let mut text = String::with_capacity(signature_display_text_capacity(
        label,
        max_chars,
        bounds.truncated,
    ));
    let mut displayed_chars = 0;
    for ch in label[..bounds.cut_byte].chars() {
        if displayed_chars == max_chars {
            break;
        }
        if is_signature_label_bidi_format_control(ch) {
            continue;
        }

        text.push(signature_display_char(ch));
        displayed_chars += 1;
    }

    if bounds.truncated {
        text.push_str(SIGNATURE_LABEL_TRUNCATED_NOTICE);
    }

    text
}

fn signature_display_text_capacity(label: &str, max_chars: usize, truncated: bool) -> usize {
    let notice_len = if truncated {
        SIGNATURE_LABEL_TRUNCATED_NOTICE.len()
    } else {
        0
    };
    max_chars
        .saturating_mul(4)
        .saturating_add(notice_len)
        .min(label.len().saturating_add(notice_len))
}

fn signature_display_range(
    label: &SignatureDisplayLabel<'_>,
    raw_range: Range<usize>,
) -> Option<Range<usize>> {
    if label.raw_is_display {
        return (raw_range.start < raw_range.end && raw_range.end <= label.text.len())
            .then_some(raw_range);
    }

    let start = label
        .raw_spans
        .iter()
        .find(|span| span.raw.end > raw_range.start && span.raw.start < raw_range.end)?
        .display
        .start;
    let end = label
        .raw_spans
        .iter()
        .rev()
        .find(|span| span.raw.start < raw_range.end && span.raw.end > raw_range.start)?
        .display
        .end;
    (start < end).then_some(start..end)
}

fn signature_display_char(ch: char) -> char {
    if is_signature_label_line_or_control(ch) {
        ' '
    } else {
        ch
    }
}

fn is_signature_label_line_or_control(ch: char) -> bool {
    ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}')
}

fn is_signature_label_bidi_format_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn signature_label_job(
    label: &str,
    parameters: &[LspParameterInformation],
    active_parameter: Option<usize>,
    text_color: Color32,
    active_background: Color32,
) -> LayoutJob {
    let normal = signature_label_format(text_color, Color32::TRANSPARENT);

    let Some(raw_range) =
        signature_active_parameter_byte_range(label, parameters, active_parameter)
    else {
        let display_label =
            bounded_signature_display_text(label, MAX_SIGNATURE_LABEL_DISPLAY_CHARS);
        let mut job = signature_label_job_with_capacity(display_label.len(), 1);
        job.append(display_label.as_ref(), 0.0, normal);
        return job;
    };
    let bounds = signature_display_bounds(label, MAX_SIGNATURE_LABEL_DISPLAY_CHARS);
    if raw_range.start >= bounds.cut_byte {
        let display_label = bounded_signature_display_text_with_bounds(
            label,
            MAX_SIGNATURE_LABEL_DISPLAY_CHARS,
            bounds,
        );
        let mut job = signature_label_job_with_capacity(display_label.len(), 1);
        job.append(display_label.as_ref(), 0.0, normal);
        return job;
    }

    let display_label = bounded_signature_display_label_with_bounds(
        label,
        MAX_SIGNATURE_LABEL_DISPLAY_CHARS,
        bounds,
    );
    let Some(range) = signature_display_range(&display_label, raw_range) else {
        let mut job = signature_label_job_with_capacity(display_label.text.len(), 1);
        job.append(display_label.text.as_ref(), 0.0, normal);
        return job;
    };

    let display_text = display_label.text.as_ref();
    let active = signature_label_format(text_color, active_background);
    let mut job = signature_label_job_with_capacity(display_text.len(), 3);
    job.append(&display_text[..range.start], 0.0, normal.clone());
    job.append(&display_text[range.clone()], 0.0, active);
    job.append(&display_text[range.end..], 0.0, normal);
    job
}

fn signature_label_job_with_capacity(text_capacity: usize, section_capacity: usize) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.text.reserve(text_capacity);
    job.sections.reserve(section_capacity);
    job
}

fn signature_label_format(color: Color32, background: Color32) -> TextFormat {
    TextFormat {
        font_id: FontId::new(14.0, FontFamily::Monospace),
        color,
        background,
        ..Default::default()
    }
}

fn signature_active_parameter_byte_range(
    label: &str,
    parameters: &[LspParameterInformation],
    active_parameter: Option<usize>,
) -> Option<Range<usize>> {
    let active_parameter = active_parameter?;
    let active_label = parameters.get(active_parameter)?.label.as_str();
    if active_label.is_empty() {
        return None;
    }

    let mut cursor = 0;
    for (index, parameter) in parameters.iter().enumerate().take(active_parameter + 1) {
        let parameter_label = parameter.label.as_str();
        if parameter_label.is_empty() {
            continue;
        }

        let Some(relative_start) = label[cursor..].find(parameter_label) else {
            if index == active_parameter {
                return label
                    .find(active_label)
                    .map(|start| start..start + active_label.len());
            }
            continue;
        };
        let start = cursor + relative_start;
        let end = start + parameter_label.len();
        if index == active_parameter {
            return Some(start..end);
        }
        cursor = end;
    }

    None
}

pub(crate) fn signature_help_step_index(
    active_signature: usize,
    signature_count: usize,
    step: isize,
    cycle: bool,
) -> usize {
    if signature_count == 0 {
        return 0;
    }

    let active = active_signature.min(signature_count - 1);
    if step < 0 {
        if active == 0 {
            if cycle { signature_count - 1 } else { 0 }
        } else {
            active - 1
        }
    } else if step > 0 {
        if active + 1 >= signature_count {
            if cycle { 0 } else { signature_count - 1 }
        } else {
            active + 1
        }
    } else {
        active
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LspSignatureDisplayCache, MAX_SIGNATURE_LABEL_DISPLAY_CHARS, MAX_SIGNATURE_MARKDOWN_CHARS,
        MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS, MAX_SIGNATURE_PARAMETER_MARKDOWN_CHARS,
        SIGNATURE_LABEL_TRUNCATED_NOTICE, SIGNATURE_MARKDOWN_TRUNCATED_NOTICE,
        SIGNATURE_PARAMETER_MARKDOWN_TRUNCATED_NOTICE, SignaturePopupDisplay,
        bounded_signature_display_label, bounded_signature_display_text,
        bounded_signature_markdown, bounded_signature_parameter_markdown,
        signature_active_parameter_byte_range, signature_display_range, signature_help_step_index,
        signature_label_job, signature_target_label,
    };
    use crate::{
        path_display::DISPLAY_PATH_LABEL_MAX_CHARS, transient_state::LspSignatureHelpPopup,
    };
    use eframe::egui::Color32;
    use std::{borrow::Cow, path::PathBuf, sync::Arc};

    use kuroya_core::{LspParameterInformation, LspSignatureHelp, LspSignatureInformation};

    fn parameter(label: &str) -> LspParameterInformation {
        LspParameterInformation {
            label: label.to_owned(),
            documentation: None,
        }
    }

    fn documented_parameter(label: &str, documentation: &str) -> LspParameterInformation {
        LspParameterInformation {
            label: label.to_owned(),
            documentation: Some(documentation.to_owned()),
        }
    }

    fn signature(label: &str, parameters: Vec<LspParameterInformation>) -> LspSignatureInformation {
        LspSignatureInformation {
            label: label.to_owned(),
            documentation: None,
            parameters,
        }
    }

    fn signature_popup(signatures: Vec<LspSignatureInformation>) -> LspSignatureHelpPopup {
        LspSignatureHelpPopup {
            id: 7,
            path: PathBuf::from("workspace/src/main.rs"),
            line: 3,
            column: 21,
            help: LspSignatureHelp {
                signatures,
                active_signature: 0,
                active_parameter: Some(0),
            },
        }
    }

    #[test]
    fn signature_help_step_index_respects_cycle_setting() {
        assert_eq!(signature_help_step_index(0, 3, -1, true), 2);
        assert_eq!(signature_help_step_index(2, 3, 1, true), 0);
        assert_eq!(signature_help_step_index(0, 3, -1, false), 0);
        assert_eq!(signature_help_step_index(2, 3, 1, false), 2);
        assert_eq!(signature_help_step_index(1, 3, 1, false), 2);
        assert_eq!(signature_help_step_index(8, 3, 0, true), 2);
        assert_eq!(signature_help_step_index(0, 0, 1, true), 0);
    }

    #[test]
    fn signature_target_label_sanitizes_and_bounds_path_text() {
        let path = PathBuf::from("workspace").join(format!(
            "signature\n{}\u{202e}.rs",
            "target-".repeat(DISPLAY_PATH_LABEL_MAX_CHARS)
        ));

        let label = signature_target_label(&path, 3, 21);

        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.ends_with(":3:21"));
        assert!(
            label.trim_end_matches(":3:21").chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS,
            "signature target path should be bounded: {label:?}"
        );
    }

    #[test]
    fn signature_display_cache_reuses_display_until_popup_state_changes() {
        let mut popup = signature_popup(vec![signature(
            "replace(value, replacement)",
            vec![parameter("value"), parameter("replacement")],
        )]);
        let mut cache = LspSignatureDisplayCache::default();
        let first = cache.display_for_popup(
            &popup,
            Color32::WHITE,
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(78, 120, 255, 72),
        );
        let second = cache.display_for_popup(
            &popup,
            Color32::WHITE,
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(78, 120, 255, 72),
        );

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.target_label.text(), "main.rs:3:21");

        popup.help.active_parameter = Some(1);
        let third = cache.display_for_popup(
            &popup,
            Color32::WHITE,
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(78, 120, 255, 72),
        );

        assert!(!Arc::ptr_eq(&first, &third));
        let signature = third.signature.as_ref().expect("signature");
        assert_eq!(signature.label_job.text, "replace(value, replacement)");
        assert_eq!(
            &signature.label_job.text[signature.label_job.sections[1].byte_range.clone()],
            "replacement"
        );
    }

    #[test]
    fn signature_popup_display_preformats_bounded_docs_and_parameter_labels() {
        let parameter_label = format!(
            "param\n{}tail",
            "x".repeat(MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS + 8)
        );
        let parameter_doc = format!(
            "{}tail",
            "d".repeat(MAX_SIGNATURE_PARAMETER_MARKDOWN_CHARS + 4)
        );
        let mut signature = signature(
            "call(param)",
            vec![documented_parameter(&parameter_label, &parameter_doc)],
        );
        signature.documentation = Some("  **Signature** docs  ".to_owned());
        let popup = signature_popup(vec![signature]);

        let display = SignaturePopupDisplay::new(
            &popup,
            Color32::WHITE,
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(78, 120, 255, 72),
        );
        let signature = display.signature.as_ref().expect("signature display");
        let parameter = signature.parameters.first().expect("parameter display");

        assert_eq!(
            signature.documentation.as_deref(),
            Some("**Signature** docs")
        );
        assert_eq!(
            parameter.label.chars().count(),
            MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS
                + SIGNATURE_LABEL_TRUNCATED_NOTICE.chars().count()
        );
        assert!(parameter.label.starts_with("param "));
        assert!(!parameter.label.contains('\n'));
        assert!(!parameter.label.contains("tail"));
        assert!(
            parameter
                .documentation
                .as_deref()
                .expect("parameter documentation")
                .ends_with(SIGNATURE_PARAMETER_MARKDOWN_TRUNCATED_NOTICE)
        );
    }

    #[test]
    fn signature_active_parameter_range_follows_parameter_order() {
        let parameters = [parameter("value"), parameter("value")];

        assert_eq!(
            signature_active_parameter_byte_range("replace(value, value)", &parameters, Some(1)),
            Some(15..20)
        );
    }

    #[test]
    fn signature_active_parameter_range_uses_unicode_byte_boundaries() {
        let parameters = [parameter("\u{00e9}clair: usize")];

        assert_eq!(
            signature_active_parameter_byte_range(
                "show(\u{00e9}clair: usize)",
                &parameters,
                Some(0)
            ),
            Some(5..19)
        );
    }

    #[test]
    fn signature_active_parameter_range_ignores_missing_active_parameter() {
        let parameters = [parameter("value")];

        assert_eq!(
            signature_active_parameter_byte_range("replace(value)", &parameters, Some(2)),
            None
        );
        assert_eq!(
            signature_active_parameter_byte_range("replace(value)", &parameters, None),
            None
        );
    }

    #[test]
    fn signature_display_label_sanitizes_control_bidi_and_line_separators() {
        let label = bounded_signature_display_label("call(\nalpha\t\u{202e}beta\u{2028}gamma)", 80);

        assert_eq!(label.text, "call( alpha beta gamma)");
        assert!(!label.text.chars().any(char::is_control));
        assert!(!label.text.contains('\u{202e}'));
        assert!(!label.text.contains('\u{2028}'));
    }

    #[test]
    fn signature_display_text_borrows_safe_short_labels() {
        let label = bounded_signature_display_text(
            "call(value: usize)",
            MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS,
        );

        assert!(matches!(label, Cow::Borrowed("call(value: usize)")));
    }

    #[test]
    fn signature_display_text_sanitizes_and_bounds_huge_control_strings() {
        let raw = format!(
            "a\u{202e}\n{}tail",
            "b".repeat(MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS + 16)
        );
        let label =
            bounded_signature_display_text(&raw, MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS);

        assert!(matches!(label, Cow::Owned(_)));
        assert_eq!(
            label.chars().count(),
            MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS
                + SIGNATURE_LABEL_TRUNCATED_NOTICE.chars().count()
        );
        assert!(label.starts_with("a "));
        assert!(label.ends_with(SIGNATURE_LABEL_TRUNCATED_NOTICE));
        assert!(!label.chars().any(char::is_control));
        assert!(!label.contains('\u{202e}'));
        assert!(!label.contains("tail"));
    }

    #[test]
    fn signature_display_label_bounds_pathological_signature_labels() {
        let raw = format!("{}tail", "x".repeat(MAX_SIGNATURE_LABEL_DISPLAY_CHARS + 32));
        let label = bounded_signature_display_label(&raw, MAX_SIGNATURE_LABEL_DISPLAY_CHARS);

        assert_eq!(
            label.text.chars().count(),
            MAX_SIGNATURE_LABEL_DISPLAY_CHARS + SIGNATURE_LABEL_TRUNCATED_NOTICE.chars().count()
        );
        assert!(label.text.ends_with(SIGNATURE_LABEL_TRUNCATED_NOTICE));
        assert!(!label.text.contains("tail"));
    }

    #[test]
    fn signature_parameter_display_label_uses_smaller_cap() {
        let raw = format!(
            "{}tail",
            "p".repeat(MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS + 8)
        );
        let label =
            bounded_signature_display_label(&raw, MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS);

        assert_eq!(
            label.text.chars().count(),
            MAX_SIGNATURE_PARAMETER_LABEL_DISPLAY_CHARS
                + SIGNATURE_LABEL_TRUNCATED_NOTICE.chars().count()
        );
        assert!(label.text.ends_with(SIGNATURE_LABEL_TRUNCATED_NOTICE));
        assert!(!label.text.contains("tail"));
    }

    #[test]
    fn signature_display_range_maps_raw_active_parameter_to_sanitized_text() {
        let raw = "call(\nvalue\u{202e}: usize)";
        let display = bounded_signature_display_label(raw, MAX_SIGNATURE_LABEL_DISPLAY_CHARS);
        let raw_range = raw.find("value\u{202e}: usize").map(|start| {
            let end = start + "value\u{202e}: usize".len();
            start..end
        });

        assert_eq!(display.text, "call( value: usize)");
        assert_eq!(
            raw_range.and_then(|range| signature_display_range(&display, range)),
            Some(6..18)
        );
    }

    #[test]
    fn signature_display_range_uses_raw_range_for_borrowed_safe_labels() {
        let raw = "call(value: usize)";
        let display = bounded_signature_display_label(raw, MAX_SIGNATURE_LABEL_DISPLAY_CHARS);

        assert_eq!(signature_display_range(&display, 5..17), Some(5..17));
        assert!(matches!(display.text, Cow::Borrowed("call(value: usize)")));
    }

    #[test]
    fn signature_label_job_sanitizes_text_and_preserves_active_highlight_when_visible() {
        let parameters = [parameter("value\u{202e}: usize")];
        let job = signature_label_job(
            "call(\nvalue\u{202e}: usize)",
            &parameters,
            Some(0),
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(78, 120, 255, 72),
        );

        assert_eq!(job.text, "call( value: usize)");
        assert!(!job.text.chars().any(char::is_control));
        assert!(!job.text.contains('\u{202e}'));
        assert_eq!(
            &job.text[job.sections[1].byte_range.clone()],
            "value: usize"
        );
    }

    #[test]
    fn signature_label_job_preserves_active_highlight_on_borrowed_safe_label() {
        let parameters = [parameter("value: usize")];
        let job = signature_label_job(
            "call(value: usize)",
            &parameters,
            Some(0),
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(78, 120, 255, 72),
        );

        assert_eq!(job.text, "call(value: usize)");
        assert_eq!(
            &job.text[job.sections[1].byte_range.clone()],
            "value: usize"
        );
    }

    #[test]
    fn signature_label_job_drops_highlight_for_truncated_active_parameter() {
        let prefix = "x".repeat(MAX_SIGNATURE_LABEL_DISPLAY_CHARS + 4);
        let raw = format!("call({prefix}, active)");
        let parameters = [parameter("active")];
        let job = signature_label_job(
            &raw,
            &parameters,
            Some(0),
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(78, 120, 255, 72),
        );

        assert!(job.text.ends_with(SIGNATURE_LABEL_TRUNCATED_NOTICE));
        assert_eq!(job.sections.len(), 1);
        assert!(!job.text.contains("active"));
    }

    #[test]
    fn bounded_signature_markdown_trims_and_leaves_small_docs_borrowed() {
        let documentation = bounded_signature_markdown("  **Insert** a value.  ");

        assert_eq!(documentation, Cow::Borrowed("**Insert** a value."));
    }

    #[test]
    fn bounded_signature_markdown_caps_pathological_docs() {
        let documentation = format!("{}tail", "a".repeat(MAX_SIGNATURE_MARKDOWN_CHARS + 32));
        let bounded = bounded_signature_markdown(&documentation);

        assert_eq!(
            bounded.chars().count(),
            MAX_SIGNATURE_MARKDOWN_CHARS + SIGNATURE_MARKDOWN_TRUNCATED_NOTICE.chars().count()
        );
        assert!(bounded.ends_with(SIGNATURE_MARKDOWN_TRUNCATED_NOTICE));
        assert!(!bounded.contains("tail"));
    }

    #[test]
    fn bounded_signature_parameter_markdown_uses_smaller_inline_cap() {
        let documentation = format!(
            "{}tail",
            "b".repeat(MAX_SIGNATURE_PARAMETER_MARKDOWN_CHARS + 16)
        );
        let bounded = bounded_signature_parameter_markdown(&documentation);

        assert_eq!(
            bounded.chars().count(),
            MAX_SIGNATURE_PARAMETER_MARKDOWN_CHARS
                + SIGNATURE_PARAMETER_MARKDOWN_TRUNCATED_NOTICE
                    .chars()
                    .count()
        );
        assert!(bounded.ends_with(SIGNATURE_PARAMETER_MARKDOWN_TRUNCATED_NOTICE));
        assert!(!bounded.contains("tail"));
    }

    #[test]
    fn bounded_signature_markdown_preserves_utf8_boundaries() {
        let documentation = format!(
            "{}tail",
            "\u{03b1}".repeat(MAX_SIGNATURE_MARKDOWN_CHARS + 4)
        );
        let bounded = bounded_signature_markdown(&documentation);

        assert!(bounded.starts_with('\u{03b1}'));
        assert_eq!(
            bounded.matches('\u{03b1}').count(),
            MAX_SIGNATURE_MARKDOWN_CHARS
        );
        assert!(bounded.ends_with(SIGNATURE_MARKDOWN_TRUNCATED_NOTICE));
    }
}

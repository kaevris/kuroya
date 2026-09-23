use crate::{
    path_display::sanitized_display_label_cow,
    ui_icons::{IconKind, icon_label},
};
use egui::{Color32, RichText, Ui};
use kuroya_core::{
    GitCountBadge, GitSnapshot, GitStatusCounts, ScmCountBadge, ScmProviderCountBadge,
};
use std::borrow::Cow;

const STATUS_ITEM_TEXT_MAX_CHARS: usize = 96;
const STATUS_ITEM_TOOLTIP_MAX_CHARS: usize = 240;
const STATUS_ITEM_CHROME_WIDTH: f32 = 42.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedStatusItem<'a> {
    text: Cow<'a, str>,
    tooltip: Cow<'a, str>,
}

pub(crate) fn status_item_width(ui: &Ui, item: &PreparedStatusItem<'_>) -> f32 {
    STATUS_ITEM_CHROME_WIDTH + small_text_width(ui, item.text.as_ref())
}

pub(crate) fn small_text_width(ui: &Ui, text: &str) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let font = egui::TextStyle::Small.resolve(ui.style());
    ui.painter()
        .layout_no_wrap(text.to_owned(), font, Color32::WHITE)
        .size()
        .x
}

pub(crate) fn prepare_status_item<'a>(
    text: impl Into<Cow<'a, str>>,
    tooltip: impl Into<Cow<'a, str>>,
) -> PreparedStatusItem<'a> {
    let text = normalize_status_bar_display_text(
        text.into(),
        STATUS_ITEM_TEXT_MAX_CHARS,
        Cow::Borrowed("unknown"),
    );
    let tooltip = normalize_status_bar_display_text(
        tooltip.into(),
        STATUS_ITEM_TOOLTIP_MAX_CHARS,
        Cow::Borrowed(""),
    );
    let tooltip = if tooltip.is_empty() {
        status_bar_tooltip_fallback(&text)
    } else {
        tooltip
    };

    PreparedStatusItem { text, tooltip }
}

fn status_bar_tooltip_fallback<'a>(text: &Cow<'a, str>) -> Cow<'a, str> {
    match text {
        Cow::Borrowed(label) => Cow::Borrowed(label),
        Cow::Owned(label) => Cow::Owned(label.clone()),
    }
}

pub(crate) fn git_status_label(snapshot: &GitSnapshot) -> String {
    let branch = snapshot
        .branch()
        .and_then(|branch| normalize_status_bar_text(branch, STATUS_ITEM_TEXT_MAX_CHARS))
        .unwrap_or(Cow::Borrowed("no git"));
    let counts = snapshot.counts();
    if git_status_counts_total(counts) == 0 {
        let mut label = String::with_capacity(branch.len() + 6);
        label.push_str(branch.as_ref());
        label.push_str(" clean");
        return label;
    }

    let summary = git_status_counts_label(counts);
    let mut label = String::with_capacity(branch.len() + 1 + summary.len());
    label.push_str(branch.as_ref());
    label.push(' ');
    label.push_str(&summary);
    label
}

pub(crate) fn git_status_counts_label(counts: GitStatusCounts) -> String {
    if git_status_counts_total(counts) == 0 {
        return String::new();
    }

    let mut label = String::with_capacity(git_status_counts_label_capacity(counts));
    push_git_status_count(&mut label, 'M', counts.modified);
    push_git_status_count(&mut label, 'A', counts.added);
    push_git_status_count(&mut label, 'D', counts.deleted);
    push_git_status_count(&mut label, 'R', counts.renamed);
    push_git_status_count(&mut label, '?', counts.untracked);
    push_git_status_count(&mut label, '!', counts.conflicted);
    label
}

fn push_git_status_count(label: &mut String, prefix: char, count: usize) {
    if count == 0 {
        return;
    }
    if !label.is_empty() {
        label.push(' ');
    }
    label.push(prefix);
    push_usize(label, count);
}

fn push_usize(label: &mut String, value: usize) {
    let mut digits = [0u8; 20];
    let mut value = value;
    let mut len = 0;
    loop {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
        if value == 0 {
            break;
        }
    }
    for digit in digits[..len].iter().rev() {
        label.push(char::from(*digit));
    }
}

fn git_status_counts_label_capacity(counts: GitStatusCounts) -> usize {
    let mut capacity = 0;
    add_git_status_count_capacity(&mut capacity, counts.modified);
    add_git_status_count_capacity(&mut capacity, counts.added);
    add_git_status_count_capacity(&mut capacity, counts.deleted);
    add_git_status_count_capacity(&mut capacity, counts.renamed);
    add_git_status_count_capacity(&mut capacity, counts.untracked);
    add_git_status_count_capacity(&mut capacity, counts.conflicted);
    capacity
}

fn add_git_status_count_capacity(capacity: &mut usize, count: usize) {
    if count == 0 {
        return;
    }
    if *capacity > 0 {
        *capacity += 1;
    }
    *capacity += 1 + decimal_digit_count(count);
}

fn decimal_digit_count(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

pub(crate) fn git_status_count_badge_label(
    counts: GitStatusCounts,
    scm_mode: ScmCountBadge,
    git_mode: GitCountBadge,
) -> Option<String> {
    if matches!(scm_mode, ScmCountBadge::Off) {
        return None;
    }
    let total = match git_mode {
        GitCountBadge::All => git_status_counts_total(counts),
        GitCountBadge::Tracked => git_status_counts_tracked_total(counts),
        GitCountBadge::Off => 0,
    };
    (total > 0).then(|| usize_label(total))
}

pub(crate) fn source_control_provider_count_badge_label(
    counts: GitStatusCounts,
    mode: ScmProviderCountBadge,
) -> Option<String> {
    if matches!(mode, ScmProviderCountBadge::Hidden) {
        return None;
    }
    let total = git_status_counts_total(counts);
    match mode {
        ScmProviderCountBadge::Hidden => None,
        ScmProviderCountBadge::Auto => (total > 0).then(|| usize_label(total)),
        ScmProviderCountBadge::Visible => Some(usize_label(total)),
    }
}

pub(crate) fn status_item(ui: &mut Ui, icon: IconKind, item: PreparedStatusItem<'_>) {
    render_status_item(ui, icon, item.text, item.tooltip);
}

fn render_status_item(ui: &mut Ui, icon: IconKind, text: Cow<'_, str>, tooltip: Cow<'_, str>) {
    let tooltip_text = tooltip.as_ref();
    ui.horizontal(|ui| {
        icon_label(
            ui,
            icon,
            ui.visuals().widgets.inactive.fg_stroke.color,
            tooltip_text,
        );
        ui.label(RichText::new(text.into_owned()).small());
    })
    .response
    .on_hover_text(tooltip_text);
    ui.separator();
}

pub(super) fn normalize_status_bar_text(text: &str, max_chars: usize) -> Option<Cow<'_, str>> {
    if max_chars == 0 {
        return None;
    }

    let text = sanitized_display_label_cow(text, max_chars, "");
    (!text.is_empty()).then_some(text)
}

fn normalize_status_bar_display_text<'a>(
    text: Cow<'a, str>,
    max_chars: usize,
    fallback: Cow<'a, str>,
) -> Cow<'a, str> {
    if max_chars == 0 {
        return fallback;
    }

    if is_simple_status_bar_text(text.as_ref(), max_chars) {
        return text;
    }

    match text {
        Cow::Borrowed(text) => {
            normalize_borrowed_status_bar_display_text(text, max_chars, fallback)
        }
        Cow::Owned(text) => normalize_owned_status_bar_display_text(text, max_chars, fallback),
    }
}

fn normalize_borrowed_status_bar_display_text<'a>(
    text: &'a str,
    max_chars: usize,
    fallback: Cow<'a, str>,
) -> Cow<'a, str> {
    let text = sanitized_display_label_cow(text, max_chars, fallback.as_ref());
    if text.is_empty() { fallback } else { text }
}

fn normalize_owned_status_bar_display_text<'a>(
    text: String,
    max_chars: usize,
    fallback: Cow<'a, str>,
) -> Cow<'a, str> {
    let normalized = {
        let raw = text.as_str();
        match sanitized_display_label_cow(raw, max_chars, fallback.as_ref()) {
            Cow::Borrowed(label) => {
                let borrowed_original =
                    !raw.is_empty() && label.as_ptr() == raw.as_ptr() && label.len() == raw.len();
                if borrowed_original {
                    None
                } else {
                    Some(label.to_owned())
                }
            }
            Cow::Owned(label) => Some(label),
        }
    };

    match normalized {
        Some(label) if label.is_empty() => fallback,
        Some(label) => Cow::Owned(label),
        None => Cow::Owned(text),
    }
}

fn is_simple_status_bar_text(text: &str, max_chars: usize) -> bool {
    if text.is_empty() || text.len() > max_chars {
        return false;
    }

    let bytes = text.as_bytes();
    !matches!(bytes.first(), Some(b' '))
        && !matches!(bytes.last(), Some(b' '))
        && bytes.iter().all(|byte| (b' '..=b'~').contains(byte))
}

fn usize_label(value: usize) -> String {
    let mut label = String::with_capacity(decimal_digit_count(value));
    push_usize(&mut label, value);
    label
}

fn git_status_counts_total(counts: GitStatusCounts) -> usize {
    counts
        .modified
        .saturating_add(counts.added)
        .saturating_add(counts.deleted)
        .saturating_add(counts.renamed)
        .saturating_add(counts.untracked)
        .saturating_add(counts.conflicted)
}

fn git_status_counts_tracked_total(counts: GitStatusCounts) -> usize {
    counts
        .modified
        .saturating_add(counts.added)
        .saturating_add(counts.deleted)
        .saturating_add(counts.renamed)
        .saturating_add(counts.conflicted)
}

#[cfg(test)]
mod tests {
    use super::{
        STATUS_ITEM_TEXT_MAX_CHARS, git_status_count_badge_label, git_status_counts_label,
        normalize_status_bar_text, prepare_status_item, source_control_provider_count_badge_label,
    };
    use kuroya_core::{GitCountBadge, GitStatusCounts, ScmCountBadge, ScmProviderCountBadge};
    use std::borrow::Cow;

    #[test]
    fn status_bar_text_is_single_line_and_bounded() {
        assert_eq!(
            normalize_status_bar_text("  main\n\tbranch\u{0}\u{202e}dirty  ", 64).as_deref(),
            Some("main branch dirty")
        );
        assert_eq!(
            normalize_status_bar_text("abcdefghijklmnopqrstuvwxyz", 12).as_deref(),
            Some("abcd...vwxyz")
        );
        assert_eq!(normalize_status_bar_text("\n\t\u{0}", 64), None);
        assert_eq!(normalize_status_bar_text("abc", 0), None);
    }

    #[test]
    fn status_bar_text_borrows_clean_and_owns_dirty_paths() {
        assert!(matches!(
            normalize_status_bar_text("main", 64),
            Some(Cow::Borrowed("main"))
        ));

        let unicode = "mode-\u{03bb}";
        match normalize_status_bar_text(unicode, 64).expect("clean unicode status text") {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }

        let dirty = normalize_status_bar_text("  main\n\tbranch\u{0}\u{202e}dirty  ", 64)
            .expect("dirty status text");
        assert_eq!(dirty.as_ref(), "main branch dirty");
        assert!(matches!(dirty, Cow::Owned(_)));

        let bounded =
            normalize_status_bar_text("abcdefghijklmnopqrstuvwxyz", 12).expect("bounded text");
        assert_eq!(bounded.as_ref(), "abcd...vwxyz");
        assert!(matches!(bounded, Cow::Owned(_)));

        assert_eq!(normalize_status_bar_text("\n\t\u{0}", 64), None);
        assert_eq!(normalize_status_bar_text("abc", 0), None);
    }

    #[test]
    fn prepared_status_item_bounds_label_and_falls_back_for_blank_tooltip() {
        let item = prepare_status_item("  main\n\tbranch\u{0}\u{202e}dirty  ", "\n\t\u{0}");

        assert_eq!(item.text.as_ref(), "main branch dirty");
        assert_eq!(item.tooltip.as_ref(), "main branch dirty");

        let bounded = prepare_status_item("a".repeat(STATUS_ITEM_TEXT_MAX_CHARS + 24), "tooltip");

        assert_eq!(bounded.text.chars().count(), STATUS_ITEM_TEXT_MAX_CHARS);
        assert!(bounded.text.contains("..."));
        assert_eq!(bounded.tooltip.as_ref(), "tooltip");
    }

    #[test]
    fn prepared_status_item_reuses_borrowed_label_for_blank_tooltip() {
        let item = prepare_status_item("main", "\n\t\u{0}");

        assert!(matches!(item.text, Cow::Borrowed("main")));
        assert!(matches!(item.tooltip, Cow::Borrowed("main")));
    }

    #[test]
    fn git_status_counts_label_preserves_order_without_empty_segments() {
        assert_eq!(git_status_counts_label(GitStatusCounts::default()), "");
        assert_eq!(
            git_status_counts_label(GitStatusCounts {
                modified: 2,
                added: 0,
                deleted: 1,
                renamed: 3,
                untracked: 5,
                conflicted: 8,
            }),
            "M2 D1 R3 ?5 !8"
        );
    }

    #[test]
    fn git_status_labels_pre_size_large_counts_without_changing_text() {
        let counts = GitStatusCounts {
            modified: 1_000_000_000,
            added: 0,
            deleted: 12_345,
            renamed: 0,
            untracked: 9,
            conflicted: 0,
        };

        assert_eq!(git_status_counts_label(counts), "M1000000000 D12345 ?9");
        assert_eq!(
            git_status_count_badge_label(counts, ScmCountBadge::All, GitCountBadge::Tracked),
            Some("1000012345".to_owned())
        );
        assert_eq!(
            source_control_provider_count_badge_label(counts, ScmProviderCountBadge::Visible),
            Some(counts.total().to_string())
        );
    }

    #[test]
    fn git_status_badges_saturate_overflowing_count_summaries() {
        let counts = GitStatusCounts {
            modified: usize::MAX,
            added: 1,
            deleted: 2,
            renamed: 3,
            untracked: 4,
            conflicted: 5,
        };
        let saturated = usize::MAX.to_string();

        assert_eq!(
            git_status_counts_label(counts),
            format!("M{} A1 D2 R3 ?4 !5", usize::MAX)
        );
        assert_eq!(
            git_status_count_badge_label(counts, ScmCountBadge::All, GitCountBadge::All),
            Some(saturated.clone())
        );
        assert_eq!(
            git_status_count_badge_label(counts, ScmCountBadge::All, GitCountBadge::Tracked),
            Some(saturated.clone())
        );
        assert_eq!(
            source_control_provider_count_badge_label(counts, ScmProviderCountBadge::Visible),
            Some(saturated)
        );
    }
}

use egui::{RichText, Ui};
use kuroya_core::EditorWordWrap;
use std::borrow::Cow;

pub(crate) fn render_status_warnings(
    ui: &mut Ui,
    active_changed_on_disk: bool,
    active_binary_preview: bool,
    active_image_preview: bool,
    active_lossy_decoded: bool,
    active_read_only: bool,
    active_large_file_mode: bool,
    external_change_count: usize,
    word_wrap: EditorWordWrap,
) {
    if external_change_count > 0 {
        warning_label(
            ui,
            disk_change_warning_label(active_changed_on_disk, external_change_count),
            disk_change_warning_tooltip(active_changed_on_disk, external_change_count),
        );
    }
    if active_image_preview {
        warning_label(
            ui,
            "image preview",
            "Active file is shown as an image preview",
        );
    } else if active_binary_preview {
        warning_label(
            ui,
            "binary preview",
            "Active file is shown as a binary preview",
        );
    } else if active_lossy_decoded {
        warning_label(
            ui,
            "UTF-8 replacement",
            "Active file contains bytes replaced during UTF-8 decoding",
        );
    }
    if active_read_only {
        warning_label(ui, "read-only", "Active file is read-only");
    }
    if active_large_file_mode {
        warning_label(
            ui,
            "large file mode",
            "Active file is using large file safeguards",
        );
    }
    if word_wrap_warning_shown(word_wrap) {
        warning_label(ui, "wrap", word_wrap_warning_tooltip());
    }
}

fn word_wrap_warning_shown(word_wrap: EditorWordWrap) -> bool {
    word_wrap != EditorWordWrap::Off
}

fn word_wrap_warning_tooltip() -> &'static str {
    "Word wrap: long lines render over multiple rows; some click positions may be approximate"
}

fn disk_change_warning_label(
    active_changed_on_disk: bool,
    external_change_count: usize,
) -> Cow<'static, str> {
    if active_changed_on_disk {
        Cow::Borrowed("changed on disk")
    } else if external_change_count == 1 {
        Cow::Borrowed("1 disk change")
    } else {
        let mut label =
            String::with_capacity(count_label_capacity(external_change_count, "disk changes"));
        push_count_label(
            &mut label,
            external_change_count,
            "disk change",
            "disk changes",
        );
        Cow::Owned(label)
    }
}

fn disk_change_warning_tooltip(
    active_changed_on_disk: bool,
    external_change_count: usize,
) -> Cow<'static, str> {
    if active_changed_on_disk {
        if external_change_count > 1 {
            if external_change_count == 2 {
                return Cow::Borrowed("Active file and 1 other open file changed on disk");
            }
            let other_count = external_change_count - 1;
            let mut tooltip = String::with_capacity(
                "Active file and ".len()
                    + count_label_capacity(other_count, "other open files")
                    + " changed on disk".len(),
            );
            tooltip.push_str("Active file and ");
            push_count_label(
                &mut tooltip,
                other_count,
                "other open file",
                "other open files",
            );
            tooltip.push_str(" changed on disk");
            Cow::Owned(tooltip)
        } else {
            Cow::Borrowed("Active file changed on disk")
        }
    } else if external_change_count == 1 {
        Cow::Borrowed("1 open file changed on disk")
    } else {
        let mut tooltip = String::with_capacity(
            count_label_capacity(external_change_count, "open files") + " changed on disk".len(),
        );
        push_count_label(
            &mut tooltip,
            external_change_count,
            "open file",
            "open files",
        );
        tooltip.push_str(" changed on disk");
        Cow::Owned(tooltip)
    }
}

fn push_count_label(label: &mut String, count: usize, singular: &str, plural: &str) {
    let noun = if count == 1 { singular } else { plural };
    push_usize_decimal(label, count);
    label.push(' ');
    label.push_str(noun);
}

fn count_label_capacity(count: usize, noun: &str) -> usize {
    decimal_digit_count(count) + 1 + noun.len()
}

fn decimal_digit_count(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn push_usize_decimal(output: &mut String, mut value: usize) {
    const MAX_USIZE_DECIMAL_DIGITS: usize = 39;

    let mut digits = [0_u8; MAX_USIZE_DECIMAL_DIGITS];
    let mut digit_count = 0;
    loop {
        digits[digit_count] = b'0' + (value % 10) as u8;
        digit_count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    for digit in digits[..digit_count].iter().rev() {
        output.push(*digit as char);
    }
}

fn warning_label(ui: &mut Ui, text: impl AsRef<str>, tooltip: impl AsRef<str>) {
    ui.label(
        RichText::new(text.as_ref())
            .small()
            .color(ui.visuals().warn_fg_color),
    )
    .on_hover_text(tooltip.as_ref());
    ui.separator();
}

#[cfg(test)]
mod tests {
    use super::{
        disk_change_warning_label, disk_change_warning_tooltip, word_wrap_warning_shown,
        word_wrap_warning_tooltip,
    };
    use kuroya_core::EditorWordWrap;

    #[test]
    fn word_wrap_warning_shows_only_when_wrap_is_not_off() {
        assert!(!word_wrap_warning_shown(EditorWordWrap::Off));
        assert!(word_wrap_warning_shown(EditorWordWrap::On));
        assert!(word_wrap_warning_shown(EditorWordWrap::WordWrapColumn));
        assert!(word_wrap_warning_shown(EditorWordWrap::Bounded));
    }

    #[test]
    fn word_wrap_warning_tooltip_explains_approximate_clicks() {
        assert_eq!(
            word_wrap_warning_tooltip(),
            "Word wrap: long lines render over multiple rows; some click positions may be approximate"
        );
    }

    #[test]
    fn disk_change_warning_label_uses_singular_and_active_context() {
        assert_eq!(disk_change_warning_label(false, 1), "1 disk change");
        assert_eq!(disk_change_warning_label(false, 2), "2 disk changes");
        assert_eq!(
            disk_change_warning_label(false, 12_345),
            "12345 disk changes"
        );
        assert_eq!(disk_change_warning_label(true, 1), "changed on disk");
    }

    #[test]
    fn disk_change_warning_tooltip_names_scope() {
        assert_eq!(
            disk_change_warning_tooltip(true, 1),
            "Active file changed on disk"
        );
        assert_eq!(
            disk_change_warning_tooltip(true, 2),
            "Active file and 1 other open file changed on disk"
        );
        assert_eq!(
            disk_change_warning_tooltip(true, 3),
            "Active file and 2 other open files changed on disk"
        );
        assert_eq!(
            disk_change_warning_tooltip(false, 1),
            "1 open file changed on disk"
        );
        assert_eq!(
            disk_change_warning_tooltip(false, 3),
            "3 open files changed on disk"
        );
        assert_eq!(
            disk_change_warning_tooltip(false, 12_345),
            "12345 open files changed on disk"
        );
    }

    #[test]
    fn disk_change_warning_text_handles_max_counts() {
        assert_eq!(
            disk_change_warning_label(false, usize::MAX),
            format!("{} disk changes", usize::MAX)
        );
        assert_eq!(
            disk_change_warning_tooltip(false, usize::MAX),
            format!("{} open files changed on disk", usize::MAX)
        );
        assert_eq!(
            disk_change_warning_tooltip(true, usize::MAX),
            format!(
                "Active file and {} other open files changed on disk",
                usize::MAX - 1
            )
        );
    }
}

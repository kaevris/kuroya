use crate::settings_form::{optional_setting_path_from_input, optional_setting_path_input_matches};
use kuroya_core::{EditorLineDecorationsWidth, EditorSettings};

use super::draft::apply_settings_panel_draft_with_font_paths;

#[derive(Debug, Clone)]
pub(in crate::preference_panels) struct SettingsPanelDraftValidation {
    candidate: EditorSettings,
    has_pending_inputs: bool,
    has_effective_changes: bool,
    invalid_numeric_count: usize,
    invalid_font_path_count: usize,
}

impl SettingsPanelDraftValidation {
    pub(in crate::preference_panels) fn into_candidate(self) -> EditorSettings {
        self.candidate
    }

    pub(in crate::preference_panels) fn has_pending_inputs(&self) -> bool {
        self.has_pending_inputs
    }

    pub(in crate::preference_panels) fn has_warnings(&self) -> bool {
        self.invalid_numeric_count > 0 || self.invalid_font_path_count > 0
    }

    pub(in crate::preference_panels) fn footer_message(&self) -> String {
        if self.invalid_numeric_count > 0 || self.invalid_font_path_count > 0 {
            return match (self.invalid_numeric_count, self.invalid_font_path_count) {
                (numbers, 0) => format!(
                    "{numbers} invalid numeric {} will reset on apply",
                    plural(numbers, "draft", "drafts")
                ),
                (0, paths) => format!(
                    "{paths} invalid font {} will clear on apply",
                    plural(paths, "path", "paths")
                ),
                (numbers, paths) => format!(
                    "{numbers} invalid numeric {} and {paths} invalid font {} will reset on apply",
                    plural(numbers, "draft", "drafts"),
                    plural(paths, "path", "paths")
                ),
            };
        }

        if self.has_effective_changes {
            "Unsaved settings changes".to_owned()
        } else if self.has_pending_inputs {
            "Draft will be normalized on apply".to_owned()
        } else {
            "Settings are up to date".to_owned()
        }
    }

    pub(in crate::preference_panels) fn apply_note(&self) -> Option<&'static str> {
        self.has_warnings()
            .then_some("normalized invalid draft values")
            .or_else(|| {
                (self.has_pending_inputs && !self.has_effective_changes)
                    .then_some("normalized draft values")
            })
    }
}

pub(super) fn validate_settings_panel_draft(
    current: &EditorSettings,
    draft: &EditorSettings,
    editor_font_path: &str,
    ui_font_path: &str,
) -> SettingsPanelDraftValidation {
    let editor_font_path = OptionalSettingPathInput::new(editor_font_path);
    let ui_font_path = OptionalSettingPathInput::new(ui_font_path);
    let mut candidate = current.clone();
    apply_settings_panel_draft_with_font_paths(
        &mut candidate,
        draft,
        editor_font_path.value(),
        ui_font_path.value(),
    );

    SettingsPanelDraftValidation {
        has_pending_inputs: draft != current
            || !editor_font_path.matches_current(&current.editor_font_path)
            || !ui_font_path.matches_current(&current.ui_font_path),
        has_effective_changes: candidate != *current,
        invalid_numeric_count: invalid_numeric_count(draft),
        invalid_font_path_count: invalid_font_path_count(&editor_font_path, &ui_font_path),
        candidate,
    }
}

struct OptionalSettingPathInput<'a> {
    raw: &'a str,
    value: Option<String>,
    invalid_non_empty: bool,
}

impl<'a> OptionalSettingPathInput<'a> {
    fn new(raw: &'a str) -> Self {
        let value = optional_setting_path_from_input(raw);
        let invalid_non_empty = value.is_none() && !raw.trim().is_empty();
        OptionalSettingPathInput {
            raw,
            value,
            invalid_non_empty,
        }
    }

    fn value(&self) -> Option<String> {
        self.value.clone()
    }

    fn matches_current(&self, current: &Option<String>) -> bool {
        optional_setting_path_input_matches(self.raw, current)
    }

    fn is_invalid_non_empty(&self) -> bool {
        self.invalid_non_empty
    }
}

fn invalid_numeric_count(draft: &EditorSettings) -> usize {
    let mut count = 0;
    for value in [
        draft.font_size,
        draft.ui_font_size,
        draft.background_image_dim,
        draft.letter_spacing,
        draft.line_height,
        draft.mouse_wheel_scroll_sensitivity,
        draft.fast_scroll_sensitivity,
        draft.minimap_section_header_font_size,
        draft.minimap_section_header_letter_spacing,
        draft.diff_split_view_default_ratio,
        draft.scm_input_font_size,
        draft.window_zoom_level,
        draft.cursor_width,
        draft.terminal_font_size,
        draft.terminal_line_height,
        draft.terminal_letter_spacing,
        draft.terminal_cursor_width,
        draft.terminal_minimum_contrast_ratio,
        draft.terminal_mouse_wheel_scroll_sensitivity,
        draft.terminal_fast_scroll_sensitivity,
    ] {
        count += usize::from(!value.is_finite());
    }

    count
        + usize::from(line_decorations_width_is_invalid(
            draft.line_decorations_width,
        ))
}

fn line_decorations_width_is_invalid(width: EditorLineDecorationsWidth) -> bool {
    match width {
        EditorLineDecorationsWidth::Pixels(width) | EditorLineDecorationsWidth::Ch(width) => {
            !width.is_finite()
        }
    }
}

fn invalid_font_path_count(
    editor_font_path: &OptionalSettingPathInput<'_>,
    ui_font_path: &OptionalSettingPathInput<'_>,
) -> usize {
    [editor_font_path, ui_font_path]
        .into_iter()
        .filter(|input| input.is_invalid_non_empty())
        .count()
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

#[cfg(test)]
mod tests {
    use super::validate_settings_panel_draft;
    use kuroya_core::EditorSettings;

    #[test]
    fn validation_reports_non_finite_numeric_drafts_without_effective_change() {
        let current = EditorSettings::default();
        let mut draft = current.clone();
        draft.font_size = f32::NAN;
        draft.terminal_line_height = f32::INFINITY;
        draft.cursor_width = f32::NEG_INFINITY;

        let validation = validate_settings_panel_draft(&current, &draft, "", "");

        assert!(validation.has_pending_inputs());
        assert!(!validation.has_effective_changes);
        assert!(validation.has_warnings());
        assert!(validation.footer_message().contains("3 invalid numeric"));
    }

    #[test]
    fn validation_reports_invalid_font_path_inputs() {
        let current = EditorSettings::default();

        let validation =
            validate_settings_panel_draft(&current, &current, "fonts/Editor.ttf\nbad", "");

        assert!(validation.has_pending_inputs());
        assert!(validation.has_warnings());
        assert!(validation.footer_message().contains("invalid font path"));
    }

    #[test]
    fn validation_keeps_untrimmed_font_path_pending_without_effective_change() {
        let current = EditorSettings {
            editor_font_path: Some("fonts/editor.ttf".to_owned()),
            ..EditorSettings::default()
        };

        let validation =
            validate_settings_panel_draft(&current, &current, " fonts/editor.ttf ", "");

        assert!(validation.has_pending_inputs());
        assert!(!validation.has_effective_changes);
        assert!(!validation.has_warnings());
        assert_eq!(
            validation.candidate.editor_font_path.as_deref(),
            Some("fonts/editor.ttf")
        );
        assert_eq!(
            validation.footer_message(),
            "Draft will be normalized on apply"
        );
    }
}

pub(crate) fn completion_request_for_typed_text(
    text: &str,
    quick_suggestions: bool,
    suggest_on_trigger_characters: bool,
    quick_suggestions_blocked: bool,
) -> bool {
    let Some(ch) = typed_text_request_character(text) else {
        return false;
    };

    (quick_suggestions && !quick_suggestions_blocked && is_quick_suggestion_character(ch))
        || (suggest_on_trigger_characters && is_suggestion_trigger_character(ch))
}

pub(crate) fn completion_request_after_text_edit(
    text: &str,
    changed: bool,
    quick_suggestions: bool,
    suggest_on_trigger_characters: bool,
    quick_suggestions_blocked: bool,
) -> bool {
    changed
        && completion_request_for_typed_text(
            text,
            quick_suggestions,
            suggest_on_trigger_characters,
            quick_suggestions_blocked,
        )
}

pub(crate) fn signature_help_request_for_typed_text(
    text: &str,
    parameter_hints_enabled: bool,
    parameter_hints_on_trigger_characters: bool,
) -> bool {
    let Some(ch) = typed_text_request_character(text) else {
        return false;
    };

    parameter_hints_enabled
        && parameter_hints_on_trigger_characters
        && is_signature_help_trigger_character(ch)
}

pub(crate) fn signature_help_request_after_text_edit(
    text: &str,
    changed: bool,
    parameter_hints_enabled: bool,
    parameter_hints_on_trigger_characters: bool,
) -> bool {
    changed
        && signature_help_request_for_typed_text(
            text,
            parameter_hints_enabled,
            parameter_hints_on_trigger_characters,
        )
}

pub(crate) fn format_on_type_request_after_text_edit(
    text: &str,
    changed: bool,
    format_on_type: bool,
) -> bool {
    let Some(ch) = typed_text_request_character(text) else {
        return false;
    };

    changed && format_on_type && is_format_on_type_trigger_character(ch)
}

fn typed_text_request_character(text: &str) -> Option<char> {
    text.chars().next_back()
}

fn is_quick_suggestion_character(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

fn is_suggestion_trigger_character(ch: char) -> bool {
    matches!(ch, '.' | ':' | '>' | '/' | '"' | '\'' | '`')
}

fn is_signature_help_trigger_character(ch: char) -> bool {
    matches!(ch, '(' | ',' | '<')
}

fn is_format_on_type_trigger_character(ch: char) -> bool {
    matches!(ch, '\n' | ';' | '}')
}

#[cfg(test)]
mod tests {
    use super::{
        completion_request_after_text_edit, completion_request_for_typed_text,
        format_on_type_request_after_text_edit, signature_help_request_after_text_edit,
        signature_help_request_for_typed_text,
    };

    #[test]
    fn completion_request_follows_quick_suggestions_setting() {
        assert!(completion_request_for_typed_text("a", true, false, false));
        assert!(!completion_request_for_typed_text("a", false, false, false));
    }

    #[test]
    fn completion_request_follows_trigger_character_setting() {
        assert!(completion_request_for_typed_text(".", false, true, false));
        assert!(!completion_request_for_typed_text(".", false, false, false));
        assert!(!completion_request_for_typed_text(" ", true, true, false));
    }

    #[test]
    fn completion_request_can_block_quick_suggestions_without_blocking_triggers() {
        assert!(!completion_request_for_typed_text("a", true, true, true));
        assert!(completion_request_for_typed_text(".", true, true, true));
    }

    #[test]
    fn completion_request_uses_trailing_character_for_coalesced_text() {
        assert!(completion_request_for_typed_text(
            "object.", false, true, false
        ));
        assert!(!completion_request_for_typed_text(
            "object.member",
            false,
            true,
            false
        ));
        assert!(completion_request_for_typed_text(
            "object.member",
            true,
            false,
            false
        ));
        assert!(!completion_request_for_typed_text(
            "value ", true, true, false
        ));
    }

    #[test]
    fn completion_request_requires_successful_text_edit() {
        assert!(completion_request_after_text_edit(
            "a", true, true, false, false
        ));
        assert!(!completion_request_after_text_edit(
            "a", false, true, false, false
        ));
        assert!(!completion_request_after_text_edit(
            ".", false, false, true, false
        ));
    }

    #[test]
    fn signature_help_request_follows_parameter_hint_settings() {
        assert!(signature_help_request_for_typed_text("(", true, true));
        assert!(!signature_help_request_for_typed_text("(", false, true));
        assert!(!signature_help_request_for_typed_text("(", true, false));
        assert!(!signature_help_request_for_typed_text("a", true, true));
    }

    #[test]
    fn signature_help_request_uses_common_trigger_characters() {
        assert!(signature_help_request_for_typed_text(",", true, true));
        assert!(signature_help_request_for_typed_text("<", true, true));
    }

    #[test]
    fn signature_help_request_uses_trailing_trigger_character() {
        assert!(signature_help_request_for_typed_text("call(", true, true));
        assert!(!signature_help_request_for_typed_text(
            "call(arg", true, true
        ));
    }

    #[test]
    fn signature_help_request_requires_successful_text_edit() {
        assert!(signature_help_request_after_text_edit(
            "(", true, true, true
        ));
        assert!(!signature_help_request_after_text_edit(
            "(", false, true, true
        ));
    }

    #[test]
    fn format_on_type_request_follows_setting_and_trigger_characters() {
        assert!(format_on_type_request_after_text_edit(";", true, true));
        assert!(format_on_type_request_after_text_edit("}", true, true));
        assert!(format_on_type_request_after_text_edit("\n", true, true));
        assert!(!format_on_type_request_after_text_edit("a", true, true));
        assert!(!format_on_type_request_after_text_edit(";", false, true));
        assert!(!format_on_type_request_after_text_edit(";", true, false));
    }

    #[test]
    fn format_on_type_request_uses_trailing_trigger_character() {
        assert!(format_on_type_request_after_text_edit(
            "statement;",
            true,
            true
        ));
        assert!(!format_on_type_request_after_text_edit(
            "statement;x",
            true,
            true
        ));
    }
}

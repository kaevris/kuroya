use super::*;

#[test]
fn minimap_section_header_lines_find_region_and_mark_labels() {
    let buffer = TextBuffer::from_text(
        1,
        None,
        "// #region API\nfn main() {}\n// MARK: - Helpers\n// MARK:\n".to_owned(),
    );

    let headers = minimap_section_header_lines(
        &buffer,
        true,
        true,
        DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX,
    );

    assert_eq!(headers.get(&1).map(String::as_str), Some("API"));
    assert_eq!(headers.get(&3).map(String::as_str), Some("Helpers"));
    assert_eq!(headers.get(&4).map(String::as_str), Some("MARK"));
}

#[test]
fn minimap_section_header_lines_respect_visibility_and_invalid_regex() {
    let buffer = TextBuffer::from_text(1, None, "// #region API\n// MARK: Helpers\n".to_owned());

    let only_marks = minimap_section_header_lines(
        &buffer,
        false,
        true,
        DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX,
    );
    assert_eq!(only_marks.keys().copied().collect::<Vec<_>>(), vec![2]);

    let invalid_regex = minimap_section_header_lines(&buffer, false, true, "(");
    assert!(invalid_regex.is_empty());

    let disabled = minimap_section_header_lines(
        &buffer,
        false,
        false,
        DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX,
    );
    assert!(disabled.is_empty());
}

#[test]
fn minimap_section_header_lines_bound_long_line_scan() {
    let long_label = "header-fragment-".repeat(400);
    let late_marker = format!(
        "{}// MARK: Late",
        "x".repeat(MINIMAP_SECTION_HEADER_SCAN_CHAR_LIMIT + 8)
    );
    let buffer = TextBuffer::from_text(1, None, format!("// MARK: {long_label}\n{late_marker}\n"));

    let headers = minimap_section_header_lines(
        &buffer,
        true,
        true,
        DEFAULT_EDITOR_MINIMAP_MARK_SECTION_HEADER_REGEX,
    );

    let label = headers.get(&1).expect("leading marker should be found");
    assert!(label.starts_with("header-fragment-"));
    assert_eq!(label.chars().count(), 80);
    assert!(
        !headers.contains_key(&2),
        "markers beyond the bounded scan prefix should be ignored"
    );
}

#[test]
fn minimap_section_header_lines_reuse_cached_compiled_mark_regexes() {
    let pattern = r"MARK: (?<label>\w+)";
    assert!(
        !crate::settings::minimap::minimap_cached_mark_regexes_for_test()
            .iter()
            .any(|(cached, _)| cached.as_str() == pattern),
        "test pattern should not be cached before this test runs"
    );

    let first = TextBuffer::from_text(1, None, "// MARK: One\n".to_owned());
    let headers = minimap_section_header_lines(&first, false, true, pattern);
    assert_eq!(headers.get(&1).map(String::as_str), Some("One"));

    let second = TextBuffer::from_text(1, None, "// MARK: One\n// MARK: Two\n".to_owned());
    let rescanned = minimap_section_header_lines(&second, false, true, pattern);
    assert_eq!(rescanned.get(&2).map(String::as_str), Some("Two"));

    let entries = crate::settings::minimap::minimap_cached_mark_regexes_for_test();
    let matching = entries
        .iter()
        .filter(|(cached, _)| cached.as_str() == pattern)
        .count();
    assert_eq!(matching, 1);
    assert!(
        entries
            .iter()
            .any(|(cached, compiled)| cached.as_str() == pattern && *compiled)
    );

    let invalid = minimap_section_header_lines(&second, false, true, "(");
    assert!(invalid.is_empty());
    let entries = crate::settings::minimap::minimap_cached_mark_regexes_for_test();
    assert_eq!(
        entries
            .iter()
            .find(|(cached, _)| cached.as_str() == "(")
            .map(|(_, compiled)| *compiled),
        Some(false)
    );
}

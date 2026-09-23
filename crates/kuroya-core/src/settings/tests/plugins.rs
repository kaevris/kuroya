use super::*;

#[test]
fn plugin_settings_default_is_enabled_with_no_disabled_ids() {
    let plugins = PluginSettings::default();
    assert!(plugins.enabled);
    assert!(plugins.disabled_ids.is_empty());
    assert_eq!(EditorSettings::default().plugins, plugins);
}

#[test]
fn plugins_section_round_trips_through_toml() {
    let text =
        "schema_version = 3\n\n[plugins]\nenabled = false\ndisabled_ids = [\"format\", \"lint\"]\n";
    let settings = parse_settings_text(text).unwrap().0;
    assert!(!settings.plugins.enabled);
    assert_eq!(settings.plugins.disabled_ids, ["format", "lint"]);

    let serialized = toml::to_string_pretty(&settings).unwrap();
    assert!(serialized.contains("[plugins]"));
    assert!(serialized.contains("enabled = false"));
    assert!(serialized.contains("disabled_ids"));

    let reparsed: EditorSettings = toml::from_str(&serialized).unwrap();
    assert_eq!(reparsed.plugins, settings.plugins);
}

#[test]
fn missing_plugins_section_defaults_to_enabled_plugins() {
    let settings: EditorSettings =
        toml::from_str("schema_version = 3\nfont_size = 14.0\n").unwrap();
    assert!(settings.plugins.enabled);
    assert!(settings.plugins.disabled_ids.is_empty());

    let empty_section: EditorSettings =
        toml::from_str("schema_version = 3\n\n[plugins]\n").unwrap();
    assert!(empty_section.plugins.enabled);
    assert!(empty_section.plugins.disabled_ids.is_empty());
}

#[test]
fn plugins_disabled_ids_are_trimmed_deduped_and_capped_per_id() {
    let text = format!(
        "schema_version = 3\n\n[plugins]\ndisabled_ids = [\"  alpha \", \"\", \"alpha\", \"beta\", \"{}\"]\n",
        "x".repeat(200)
    );
    let settings = parse_settings_text(&text).unwrap().0;

    let expected = vec![
        "alpha".to_owned(),
        "beta".to_owned(),
        "x".repeat(MAX_PLUGIN_SETTINGS_DISABLED_ID_CHARS),
    ];
    assert_eq!(settings.plugins.disabled_ids, expected);
}

#[test]
fn plugins_disabled_ids_keep_first_max_entries() {
    let entries: Vec<String> = (0..(MAX_PLUGIN_SETTINGS_DISABLED_IDS + 2))
        .map(|index| format!("\"plugin-{index}\""))
        .collect();
    let text = format!(
        "schema_version = 3\n\n[plugins]\ndisabled_ids = [{}]\n",
        entries.join(", ")
    );
    let settings = parse_settings_text(&text).unwrap().0;

    assert_eq!(
        settings.plugins.disabled_ids.len(),
        MAX_PLUGIN_SETTINGS_DISABLED_IDS
    );
    assert_eq!(settings.plugins.disabled_ids[0], "plugin-0");
    assert_eq!(
        settings.plugins.disabled_ids[MAX_PLUGIN_SETTINGS_DISABLED_IDS - 1],
        "plugin-127"
    );
}

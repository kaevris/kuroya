use super::*;

#[test]
fn discord_settings_default_to_disabled_presence_with_empty_client_id() {
    let discord = DiscordSettings::default();
    assert!(!discord.presence_enabled);
    assert_eq!(discord.client_id, "");
    assert!(!discord.presence_is_configurable());
    assert_eq!(EditorSettings::default().discord, discord);
}

#[test]
fn discord_section_round_trips_through_toml() {
    let text = "schema_version = 5\n\n[discord]\npresence_enabled = true\nclient_id = \"1234567890123456789\"\n";
    let settings = parse_settings_text(text).unwrap().0;
    assert!(settings.discord.presence_enabled);
    assert_eq!(settings.discord.client_id, "1234567890123456789");
    assert!(settings.discord.presence_is_configurable());

    let serialized = toml::to_string_pretty(&settings).unwrap();
    assert!(serialized.contains("[discord]"));
    assert!(serialized.contains("presence_enabled = true"));
    assert!(serialized.contains("client_id"));

    let reparsed: EditorSettings = toml::from_str(&serialized).unwrap();
    assert_eq!(reparsed.discord, settings.discord);
}

#[test]
fn missing_discord_section_defaults_to_disabled_presence() {
    let settings: EditorSettings =
        toml::from_str("schema_version = 5\nfont_size = 14.0\n").unwrap();
    assert!(!settings.discord.presence_enabled);
    assert_eq!(settings.discord.client_id, "");

    let empty_section: EditorSettings =
        toml::from_str("schema_version = 5\n\n[discord]\n").unwrap();
    assert!(!empty_section.discord.presence_enabled);
    assert_eq!(empty_section.discord.client_id, "");
}

#[test]
fn discord_presence_without_client_id_is_not_configurable() {
    let text = "schema_version = 5\n\n[discord]\npresence_enabled = true\n";
    let settings = parse_settings_text(text).unwrap().0;
    assert!(settings.discord.presence_enabled);
    assert_eq!(settings.discord.client_id, "");
    assert!(!settings.discord.presence_is_configurable());
}

#[test]
fn discord_client_id_is_sanitized_to_a_plain_string() {
    let mut settings = EditorSettings::default();
    settings.discord.client_id = " 1234567890\n".to_owned();

    assert!(settings.sanitize());
    assert_eq!(settings.discord.client_id, "1234567890");
}

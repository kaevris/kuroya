use super::*;

#[test]
fn load_or_create_migrates_legacy_settings_schema_atomically() {
    let path = temp_settings_path("migrate");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "schema_version = 1\nfont_size = 15.0\n").unwrap();

    let settings = EditorSettings::load_or_create(&path).unwrap();

    assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    assert_eq!(settings.font_size, 15.0);
    let migrated = fs::read_to_string(&path).unwrap();
    assert!(migrated.contains(&format!("schema_version = {SETTINGS_SCHEMA_VERSION}")));
    assert!(migrated.contains("font_size = 15.0"));
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn settings_schema_version_preflight_preserves_validation() {
    // An absent schema_version means the file was written by an unknown or
    // current tool: it counts as the current version so migrations never run.
    assert_eq!(
        settings_schema_version_from_toml("font_size = 15.0\n").unwrap(),
        SETTINGS_SCHEMA_VERSION
    );

    let text = format!(
        "schema_version = {SETTINGS_SCHEMA_VERSION}\nfont_family = \"{}\"\n[theme]\nname = \"Graphite\"\n",
        "Cascadia Code, ".repeat(128)
    );
    assert_eq!(
        settings_schema_version_from_toml(&text).unwrap(),
        SETTINGS_SCHEMA_VERSION
    );

    let negative_error = settings_schema_version_from_toml("schema_version = -1\n")
        .unwrap_err()
        .to_string();
    assert!(
        negative_error.contains("settings schema_version must be between 0"),
        "{negative_error}"
    );

    let future_version = SETTINGS_SCHEMA_VERSION + 1;
    let future_error =
        settings_schema_version_from_toml(&format!("schema_version = {future_version}\n"))
            .unwrap_err()
            .to_string();
    assert!(
        future_error.contains("newer than supported version"),
        "{future_error}"
    );

    // A non-integer schema_version is unreadable, so it counts as absent:
    // current version, no migrations.
    assert_eq!(
        settings_schema_version_from_toml("schema_version = \"1\"\n").unwrap(),
        SETTINGS_SCHEMA_VERSION
    );
}

#[test]
fn read_settings_text_with_limit_preserves_size_boundary() {
    let path = temp_settings_path("read-size-boundary");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let exact_limit = usize::try_from(SETTINGS_FILE_MAX_BYTES).unwrap();

    fs::write(&path, vec![b'a'; exact_limit]).unwrap();
    let text = read_settings_text_with_limit(&path).unwrap();
    assert_eq!(text.len(), exact_limit);

    fs::write(&path, vec![b'a'; exact_limit + 1]).unwrap();
    let error = read_settings_text_with_limit(&path)
        .unwrap_err()
        .to_string();
    assert!(error.contains("exceeds settings file limit"), "{error}");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_settings_text_with_limit_errors_for_directory_paths() {
    let path = temp_settings_path("read-directory");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(&path).unwrap();

    assert!(read_settings_text_with_limit(&path).is_err());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_writes_default_settings_file() {
    let path = temp_settings_path("create");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();

    let settings = EditorSettings::load_or_create(&path).unwrap();

    assert_eq!(settings, EditorSettings::default());
    assert_eq!(settings.schema_version, SETTINGS_SCHEMA_VERSION);
    assert_eq!(EditorSettings::load_or_create(&path).unwrap(), settings);
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_reports_directory_settings_path_without_default_overwrite() {
    let path = temp_settings_path("create-directory");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(&path).unwrap();

    assert!(EditorSettings::load_or_create(&path).is_err());
    assert!(path.is_dir());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_writes_default_settings_file_when_missing() {
    let path = temp_settings_path("recover-missing");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    assert_eq!(loaded.quarantined_path, None);
    assert_eq!(
        EditorSettings::load_or_create(&path).unwrap(),
        EditorSettings::default()
    );
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_does_not_quarantine_missing_settings_file() {
    let path = temp_settings_path("recover-missing-no-quarantine");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let mut quarantine_called = false;

    let loaded = EditorSettings::load_or_create_with_recovery_and_quarantine(&path, |_| {
        quarantine_called = true;
        Err(anyhow::anyhow!(
            "missing settings should not be quarantined"
        ))
    })
    .unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    assert_eq!(loaded.quarantined_path, None);
    assert!(!quarantine_called);
    assert_eq!(
        EditorSettings::load_or_create(&path).unwrap(),
        EditorSettings::default()
    );
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_quarantines_corrupt_settings() {
    let path = temp_settings_path("recover-corrupt");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "font_size = ").unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    let quarantined = loaded.quarantined_path.expect("corrupt file is moved");
    assert!(
        quarantined
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("settings.toml.corrupt."))
    );
    assert_eq!(fs::read_to_string(&quarantined).unwrap(), "font_size = ");
    assert_eq!(
        EditorSettings::load_or_create(&path).unwrap(),
        EditorSettings::default()
    );
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_uses_defaults_when_corrupt_settings_quarantine_fails() {
    let path = temp_settings_path("recover-corrupt-quarantine-fails");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "font_size = ").unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery_and_quarantine(&path, |_| {
        Err(anyhow::anyhow!("quarantine unavailable"))
    })
    .unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    assert_eq!(loaded.quarantined_path, None);
    assert_eq!(
        EditorSettings::load_or_create(&path).unwrap(),
        EditorSettings::default()
    );
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_reports_failure_when_quarantine_and_default_write_fail() {
    let root = std::env::temp_dir().join(format!(
        "kuroya-settings-recover-double-failure-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let kuroya_file = root.join(".kuroya");
    fs::write(&kuroya_file, "not a directory").unwrap();
    let path = kuroya_file.join("settings.toml");

    let error = EditorSettings::recover_default_settings_with(&path, |_| {
        Err(anyhow::anyhow!("quarantine unavailable"))
    })
    .unwrap_err()
    .to_string();

    assert!(error.contains("quarantine failed"), "{error}");
    assert!(error.contains("writing defaults failed"), "{error}");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_quarantines_invalid_setting_values() {
    let path = temp_settings_path("recover-invalid-value");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "terminal_confirm_on_exit = \"sometimes\"\n").unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    let quarantined = loaded.quarantined_path.expect("invalid file is moved");
    assert_eq!(
        fs::read_to_string(&quarantined).unwrap(),
        "terminal_confirm_on_exit = \"sometimes\"\n"
    );
    assert_eq!(
        EditorSettings::load_or_create(&path).unwrap(),
        EditorSettings::default()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_defaults_invalid_line_numbers_without_quarantine() {
    let path = temp_settings_path("recover-invalid-line-numbers");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    // An explicit legacy schema_version keeps the rewrite-to-clean-file
    // behavior under test; a missing schema_version would no longer migrate.
    fs::write(
        &path,
        "schema_version = 1\nfont_size = 15.0\nline_numbers = \"visible\"\n[theme]\nname = \"Graphite\"\n",
    )
    .unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    assert_eq!(loaded.quarantined_path, None);
    assert_eq!(loaded.settings.font_size, 15.0);
    assert_eq!(loaded.settings.line_numbers, EditorLineNumbers::On);
    assert_eq!(loaded.settings.theme.name, "Graphite");
    assert_eq!(
        EditorSettings::load_or_create(&path).unwrap(),
        loaded.settings
    );
    assert!(!fs::read_to_string(&path).unwrap().contains("\"visible\""));
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_quarantines_invalid_schema_versions() {
    let path = temp_settings_path("recover-invalid-schema-version");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "schema_version = -1\nfont_size = 15.0\n").unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    let quarantined = loaded
        .quarantined_path
        .expect("invalid schema file is moved");
    assert_eq!(
        fs::read_to_string(&quarantined).unwrap(),
        "schema_version = -1\nfont_size = 15.0\n"
    );
    assert_eq!(
        EditorSettings::load_or_create(&path).unwrap(),
        EditorSettings::default()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_keeps_future_schema_versions_for_newer_builds() {
    let path = temp_settings_path("recover-future-schema-version");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let future_version = SETTINGS_SCHEMA_VERSION + 1;
    let text = format!(
        "schema_version = {future_version}\nfont_size = 15.0\nfuture_setting = \"preserved\"\n"
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &text).unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    assert_eq!(loaded.quarantined_path, None);
    assert_eq!(loaded.future_schema_version, Some(future_version));
    let status = loaded
        .future_schema_version_status()
        .expect("future schema status is set");
    assert!(
        status.contains("newer version of Kuroya")
            && status.contains(&format!("schema {future_version}"))
            && status.contains("uses them as defaults"),
        "{status}"
    );
    // The file written by the newer build stays in place: nothing is
    // quarantined and nothing is rewritten against it.
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    let sibling_count = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.contains(".corrupt."))
        })
        .count();
    assert_eq!(
        sibling_count, 0,
        "future schema file must not be quarantined"
    );
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_keeps_future_schema_before_known_recovery() {
    let path = temp_settings_path("recover-future-schema-with-invalid-line-numbers");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let future_version = SETTINGS_SCHEMA_VERSION + 1;
    let text = format!(
        "schema_version = {future_version}\nline_numbers = \"visible\"\nfuture_setting = \"preserved\"\n"
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &text).unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    assert_eq!(loaded.quarantined_path, None);
    assert_eq!(loaded.future_schema_version, Some(future_version));
    // The known line_numbers recovery must not rewrite a future-version file.
    assert_eq!(fs::read_to_string(&path).unwrap(), text);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_keeps_missing_schema_version_settings_unmigrated() {
    let path = temp_settings_path("missing-schema-version-no-migration");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    let text = "terminal_tabs_location = \"right\"\nterminal_copy_on_selection = false\n";
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, text).unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    // A hand-written file without schema_version counts as current-era: the
    // schema 1/2 migrations must not run against user intent.
    assert_eq!(loaded.quarantined_path, None);
    assert_eq!(loaded.future_schema_version, None);
    assert_eq!(
        loaded.settings.terminal_tabs_location,
        TerminalTabsLocation::Right
    );
    assert!(!loaded.settings.terminal_copy_on_selection);
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert_no_setting_temps(&path);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn load_or_create_with_recovery_quarantines_oversized_settings() {
    let path = temp_settings_path("recover-oversized");
    let root = path.parent().unwrap().parent().unwrap().to_path_buf();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        vec![b'a'; usize::try_from(SETTINGS_FILE_MAX_BYTES + 1).unwrap()],
    )
    .unwrap();

    let loaded = EditorSettings::load_or_create_with_recovery(&path).unwrap();

    assert_eq!(loaded.settings, EditorSettings::default());
    let quarantined = loaded.quarantined_path.expect("oversized file is moved");
    assert_eq!(
        fs::metadata(&quarantined).unwrap().len(),
        SETTINGS_FILE_MAX_BYTES + 1
    );
    assert_eq!(
        EditorSettings::load_or_create(&path).unwrap(),
        EditorSettings::default()
    );

    fs::remove_dir_all(root).unwrap();
}

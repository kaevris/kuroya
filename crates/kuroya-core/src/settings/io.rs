use super::{EditorSettings, SETTINGS_FILE_MAX_BYTES, SETTINGS_SCHEMA_VERSION};
use serde::Deserialize;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn parse_settings_text_with_known_recovery(
    text: &str,
) -> anyhow::Result<(EditorSettings, bool)> {
    match parse_settings_text(text) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            if let Ok(Some(recovered_text)) = recover_invalid_line_numbers_setting(text)
                && let Ok((settings, _)) = parse_settings_text(&recovered_text)
            {
                return Ok((settings, true));
            }

            Err(error)
        }
    }
}

pub(super) fn parse_settings_text(text: &str) -> anyhow::Result<(EditorSettings, bool)> {
    let source_version = settings_schema_version_from_toml(text)?;
    let mut settings: EditorSettings = toml::from_str(text)?;
    let settings_changed = settings.sanitize();
    let should_save_migration = settings.apply_migrations(source_version) || settings_changed;
    Ok((settings, should_save_migration))
}

pub(super) fn recover_invalid_line_numbers_setting(text: &str) -> anyhow::Result<Option<String>> {
    let mut value: toml::Value = toml::from_str(text)?;
    let Some(table) = value.as_table_mut() else {
        return Ok(None);
    };
    let Some(line_numbers) = table.get("line_numbers") else {
        return Ok(None);
    };
    if line_numbers
        .as_str()
        .is_some_and(|value| matches!(value, "on" | "off" | "relative" | "interval"))
    {
        return Ok(None);
    }

    table.remove("line_numbers");
    Ok(Some(toml::to_string_pretty(&value)?))
}

pub(super) fn read_settings_text_with_limit(path: &Path) -> anyhow::Result<String> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let is_file = metadata.is_file();
    let file_len = metadata.len();
    if is_file && file_len > SETTINGS_FILE_MAX_BYTES {
        anyhow::bail!(
            "{} exceeds settings file limit of {SETTINGS_FILE_MAX_BYTES} bytes",
            path.display()
        );
    }

    let read_limit = SETTINGS_FILE_MAX_BYTES.saturating_add(1);
    let mut reader = file.take(read_limit);
    let initial_capacity = if is_file && file_len > 0 {
        usize::try_from(file_len.min(SETTINGS_FILE_MAX_BYTES))
            .unwrap_or(0)
            .saturating_add(1)
    } else {
        0
    };
    let mut bytes = Vec::with_capacity(initial_capacity);
    reader.read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > SETTINGS_FILE_MAX_BYTES {
        anyhow::bail!(
            "{} exceeds settings file limit of {SETTINGS_FILE_MAX_BYTES} bytes",
            path.display()
        );
    }

    String::from_utf8(bytes).map_err(Into::into)
}

pub(super) fn settings_read_error_is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

#[derive(Default, Deserialize)]
struct SettingsSchemaVersionToml {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_settings_schema_version"
    )]
    schema_version: Option<i64>,
}

fn deserialize_optional_settings_schema_version<'de, D>(
    deserializer: D,
) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct SchemaVersionVisitor;

    impl<'de> serde::de::Visitor<'de> for SchemaVersionVisitor {
        type Value = Option<i64>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an integer schema version")
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(i64::try_from(value).ok())
        }

        fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(self)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
            Ok(None)
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            while map
                .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                .is_some()
            {}
            Ok(None)
        }
    }

    deserializer.deserialize_any(SchemaVersionVisitor)
}

pub(super) fn settings_schema_version_from_toml(text: &str) -> anyhow::Result<u32> {
    let version = settings_schema_version_raw_from_toml(text)?;
    anyhow::ensure!(
        version <= SETTINGS_SCHEMA_VERSION,
        "settings schema_version {version} is newer than supported version {SETTINGS_SCHEMA_VERSION}"
    );
    Ok(version)
}

fn settings_schema_version_raw_from_toml(text: &str) -> anyhow::Result<u32> {
    let schema = toml::from_str::<SettingsSchemaVersionToml>(text)?;
    let Some(version) = schema.schema_version else {
        return Ok(SETTINGS_SCHEMA_VERSION);
    };
    u32::try_from(version)
        .map_err(|_| anyhow::anyhow!("settings schema_version must be between 0 and {}", u32::MAX))
}

pub(super) fn settings_schema_version_newer_than_supported(text: &str) -> Option<u32> {
    let version = settings_schema_version_raw_from_toml(text).ok()?;
    (version > SETTINGS_SCHEMA_VERSION).then_some(version)
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let temp = temporary_path(path);
    let result = (|| -> anyhow::Result<()> {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }

    result
}

pub(super) fn quarantine_corrupt_settings(path: &Path) -> anyhow::Result<PathBuf> {
    let quarantine = corrupt_settings_path(path);
    fs::rename(path, &quarantine)?;
    Ok(quarantine)
}

pub(super) fn corrupt_settings_path(path: &Path) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.toml");
    path.with_file_name(format!(
        "{file_name}.corrupt.{}.{}",
        std::process::id(),
        unique
    ))
}

pub(super) fn temporary_path(path: &Path) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.toml");
    path.with_file_name(format!(
        ".{file_name}.tmp.{}.{}",
        std::process::id(),
        unique
    ))
}

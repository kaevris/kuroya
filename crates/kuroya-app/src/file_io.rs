pub(crate) use crate::file_decode::{DecodedText, decode_text_bytes};

use kuroya_core::TextSnapshot;
use std::{io::Read, path::Path};
use tokio::io::AsyncReadExt;

pub(crate) const FILE_OPEN_MAX_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const FILE_WRITE_MAX_BYTES: u64 = FILE_OPEN_MAX_BYTES;
const READ_BUFFER_PREALLOC_MAX_BYTES: u64 = FILE_OPEN_MAX_BYTES;

pub(crate) async fn read_text_file(path: &Path) -> Result<DecodedText, String> {
    read_text_file_with_limit(path, FILE_OPEN_MAX_BYTES).await
}

pub(crate) async fn read_text_file_with_limit(
    path: &Path,
    max_bytes: u64,
) -> Result<DecodedText, String> {
    let bytes = read_file_bytes_with_limit(path, max_bytes).await?;

    Ok(decode_text_bytes(bytes))
}

pub(crate) async fn read_file_bytes_with_limit(
    path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let file = open_read_target(path).await?;
    let metadata = file.metadata().await.map_err(|error| error.to_string())?;
    ensure_read_target_is_file(path, &metadata)?;
    if file_size_exceeds_limit(metadata.len(), max_bytes) {
        return Err(file_too_large_message(metadata.len(), max_bytes));
    }
    let capacity = read_buffer_capacity(metadata.len(), max_bytes);

    if max_bytes == 0 {
        let mut reader = file;
        let mut bytes = Vec::with_capacity(capacity);
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(bytes);
    }

    let mut reader = file.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    ensure_read_bytes_within_limit(bytes.len(), max_bytes)?;

    Ok(bytes)
}

async fn open_read_target(path: &Path) -> Result<tokio::fs::File, String> {
    match tokio::fs::File::open(path).await {
        Ok(file) => Ok(file),
        Err(open_error) => {
            if open_error_needs_non_file_probe(&open_error) {
                reject_non_file_open_error(path).await?;
            }
            Err(open_error.to_string())
        }
    }
}

async fn reject_non_file_open_error(path: &Path) -> Result<(), String> {
    let Ok(metadata) = tokio::fs::metadata(path).await else {
        return Ok(());
    };
    ensure_read_target_is_file(path, &metadata)
}

pub(crate) fn read_utf8_text_file_with_limit(
    path: &Path,
    max_bytes: usize,
) -> Result<String, String> {
    let max_bytes = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    let bytes = read_file_bytes_with_limit_sync(path, max_bytes)?;

    String::from_utf8(bytes).map_err(|error| error.to_string())
}

pub(crate) async fn write_text_snapshot_atomic_async(
    path: &Path,
    text: TextSnapshot,
) -> anyhow::Result<()> {
    ensure_write_bytes_within_limit(text.len_bytes())?;
    crate::file_atomic_write::write_text_snapshot_atomic_async(path, text).await
}

pub(crate) fn write_text_atomic(path: &Path, text: &str) -> anyhow::Result<()> {
    ensure_write_bytes_within_limit(text.len())?;
    crate::file_atomic_write::write_text_atomic(path, text)
}

fn read_file_bytes_with_limit_sync(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    let (file, byte_len) = open_read_file_with_limit_sync(path, max_bytes)?;
    let capacity = read_buffer_capacity(byte_len, max_bytes);

    if max_bytes == 0 {
        let mut reader = file;
        let mut bytes = Vec::with_capacity(capacity);
        reader
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        return Ok(bytes);
    }

    let mut reader = file.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(capacity);
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    ensure_read_bytes_within_limit(bytes.len(), max_bytes)?;

    Ok(bytes)
}

pub(crate) fn open_read_file_with_limit_sync(
    path: &Path,
    max_bytes: u64,
) -> Result<(std::fs::File, u64), String> {
    let file = open_read_target_sync(path)?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    ensure_read_target_is_file(path, &metadata)?;
    if file_size_exceeds_limit(metadata.len(), max_bytes) {
        return Err(file_too_large_message(metadata.len(), max_bytes));
    }
    Ok((file, metadata.len()))
}

fn open_read_target_sync(path: &Path) -> Result<std::fs::File, String> {
    match std::fs::File::open(path) {
        Ok(file) => Ok(file),
        Err(open_error) => {
            if open_error_needs_non_file_probe(&open_error) {
                reject_non_file_open_error_sync(path)?;
            }
            Err(open_error.to_string())
        }
    }
}

fn reject_non_file_open_error_sync(path: &Path) -> Result<(), String> {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(());
    };
    ensure_read_target_is_file(path, &metadata)
}

pub(crate) fn file_size_exceeds_limit(bytes: u64, max_bytes: u64) -> bool {
    max_bytes > 0 && bytes > max_bytes
}

pub(crate) fn file_too_large_message(bytes: u64, max_bytes: u64) -> String {
    format!(
        "file is too large to open ({}; limit {})",
        format_byte_size(bytes),
        format_byte_size(max_bytes)
    )
}

fn ensure_read_bytes_within_limit(byte_len: usize, max_bytes: u64) -> Result<(), String> {
    let byte_len = u64::try_from(byte_len).unwrap_or(u64::MAX);
    if file_size_exceeds_limit(byte_len, max_bytes) {
        Err(file_too_large_message(byte_len, max_bytes))
    } else {
        Ok(())
    }
}

fn ensure_write_bytes_within_limit(byte_len: usize) -> anyhow::Result<()> {
    let byte_len = u64::try_from(byte_len).unwrap_or(u64::MAX);
    if file_size_exceeds_limit(byte_len, FILE_WRITE_MAX_BYTES) {
        anyhow::bail!("{}", file_too_large_to_save_message(byte_len));
    }
    Ok(())
}

fn file_too_large_to_save_message(bytes: u64) -> String {
    format!(
        "file is too large to save ({}; limit {})",
        format_byte_size(bytes),
        format_byte_size(FILE_WRITE_MAX_BYTES)
    )
}

fn read_buffer_capacity(metadata_len: u64, max_bytes: u64) -> usize {
    let prealloc_limit = if max_bytes == 0 {
        READ_BUFFER_PREALLOC_MAX_BYTES
    } else {
        max_bytes
            .saturating_add(1)
            .min(READ_BUFFER_PREALLOC_MAX_BYTES)
    };
    let capacity = if max_bytes > 0 && metadata_len >= max_bytes {
        max_bytes.saturating_add(1).min(prealloc_limit)
    } else {
        metadata_len.min(prealloc_limit)
    };
    usize::try_from(capacity).unwrap_or(usize::MAX)
}

fn ensure_read_target_is_file(path: &Path, metadata: &std::fs::Metadata) -> Result<(), String> {
    if metadata.is_file() {
        Ok(())
    } else {
        Err(format!("open target is not a file: {}", path.display()))
    }
}

fn open_error_needs_non_file_probe(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::IsADirectory | std::io::ErrorKind::PermissionDenied
    )
}

pub(crate) fn format_byte_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / MIB)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod read_capacity_tests {
    use super::{
        FILE_OPEN_MAX_BYTES, FILE_WRITE_MAX_BYTES, ensure_write_bytes_within_limit,
        read_buffer_capacity,
    };

    #[test]
    fn read_buffer_capacity_uses_limit_plus_overflow_byte_for_bounded_reads() {
        assert_eq!(read_buffer_capacity(3, 4), 3);
        assert_eq!(read_buffer_capacity(4, 4), 5);
        assert_eq!(read_buffer_capacity(100, 4), 5);
    }

    #[test]
    fn read_buffer_capacity_caps_unlimited_preallocation() {
        assert_eq!(
            read_buffer_capacity(FILE_OPEN_MAX_BYTES + 1024, 0),
            usize::try_from(FILE_OPEN_MAX_BYTES).unwrap()
        );
    }

    #[test]
    fn write_size_guard_rejects_only_bytes_over_limit() {
        assert!(ensure_write_bytes_within_limit(FILE_WRITE_MAX_BYTES as usize).is_ok());
        let error = ensure_write_bytes_within_limit(FILE_WRITE_MAX_BYTES as usize + 1)
            .unwrap_err()
            .to_string();

        assert!(error.contains("file is too large to save"));
        assert!(error.contains("limit 16.0 MiB"));
    }
}

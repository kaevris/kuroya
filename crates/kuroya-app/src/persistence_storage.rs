use std::{
    fs::{File, Metadata, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::io::AsyncReadExt;

mod paths;

#[cfg(not(test))]
pub(crate) use paths::{app_settings_path, app_state_path};
pub(crate) use paths::{
    app_state_dir, legacy_project_index_cache_path, legacy_session_path,
    legacy_session_snapshots_dir, legacy_state_dir, legacy_workspace_snapshots_dir,
    project_index_cache_path, session_path, session_snapshots_dir, state_dir,
    workspace_snapshots_dir,
};

static WRITE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const WRITE_TEMP_CREATE_ATTEMPTS: u32 = 32;
const READ_BUFFER_PREALLOC_MAX_BYTES: u64 = 32 * 1024 * 1024;

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    atomic_write_with_temp_candidates(
        path,
        bytes,
        (0..WRITE_TEMP_CREATE_ATTEMPTS).map(|_| temporary_path(path)),
    )
}

fn atomic_write_with_temp_candidates<I>(
    path: &Path,
    bytes: &[u8],
    candidates: I,
) -> anyhow::Result<()>
where
    I: IntoIterator<Item = PathBuf>,
{
    if let Some(parent) = parent_dir(path) {
        std::fs::create_dir_all(parent)?;
    }

    let (temp, mut file) = create_unique_write_temp_file(path, candidates)?;
    let result = (|| -> anyhow::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)?;
        sync_parent_dir_best_effort(path);
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }

    result
}

pub(crate) async fn atomic_write_async(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    atomic_write_async_with_temp_candidates(
        path,
        bytes,
        (0..WRITE_TEMP_CREATE_ATTEMPTS).map(|_| temporary_path(path)),
    )
    .await
}

async fn atomic_write_async_with_temp_candidates<I>(
    path: &Path,
    bytes: &[u8],
    candidates: I,
) -> anyhow::Result<()>
where
    I: IntoIterator<Item = PathBuf>,
{
    if let Some(parent) = parent_dir(path) {
        tokio::fs::create_dir_all(parent).await?;
    }

    let (temp, mut file) = create_unique_write_temp_file_async(path, candidates).await?;
    let result = async {
        tokio::io::AsyncWriteExt::write_all(&mut file, bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&temp, path).await?;
        sync_parent_dir_best_effort_async(path).await;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }

    result
}

fn create_unique_write_temp_file<I>(path: &Path, candidates: I) -> anyhow::Result<(PathBuf, File)>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut collisions = 0usize;
    for temp in candidates {
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                collisions = collisions.saturating_add(1);
            }
            Err(error) => return Err(error.into()),
        }
    }

    anyhow::bail!(
        "failed to create unique temporary persistence file near {} after {} collisions",
        path.display(),
        collisions
    )
}

async fn create_unique_write_temp_file_async<I>(
    path: &Path,
    candidates: I,
) -> anyhow::Result<(PathBuf, tokio::fs::File)>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut collisions = 0usize;
    for temp in candidates {
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .await
        {
            Ok(file) => return Ok((temp, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                collisions = collisions.saturating_add(1);
            }
            Err(error) => return Err(error.into()),
        }
    }

    anyhow::bail!(
        "failed to create unique temporary persistence file near {} after {} collisions",
        path.display(),
        collisions
    )
}

pub(crate) fn read_file_bytes_with_limit(path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
    let (file, metadata) = open_file_with_metadata(path)?;
    if max_bytes > 0 && metadata.len() > max_bytes {
        return Err(file_size_error(path, max_bytes));
    }

    read_bytes_with_known_len(path, file, metadata.len(), max_bytes)
}

fn open_file_with_metadata(path: &Path) -> io::Result<(File, Metadata)> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => return Err(normalize_file_open_error(path, error)),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(file_type_error(path));
    }

    Ok((file, metadata))
}

fn normalize_file_open_error(path: &Path, error: io::Error) -> io::Error {
    match std::fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => file_type_error(path),
        _ => error,
    }
}

fn read_bytes_with_known_len<R: Read>(
    path: &Path,
    reader: R,
    metadata_len: u64,
    max_bytes: u64,
) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(read_buffer_capacity(metadata_len, max_bytes));
    if max_bytes == 0 {
        let mut reader = reader;
        reader.read_to_end(&mut bytes)?;
        return Ok(bytes);
    }

    let mut reader = reader.take(max_bytes.saturating_add(1));
    reader.read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(file_size_error(path, max_bytes));
    }

    Ok(bytes)
}

pub(crate) async fn read_file_bytes_with_limit_async(
    path: &Path,
    max_bytes: u64,
) -> io::Result<Vec<u8>> {
    let (file, metadata) = open_file_with_metadata_async(path).await?;
    if max_bytes > 0 && metadata.len() > max_bytes {
        return Err(file_size_error(path, max_bytes));
    }

    read_bytes_with_known_len_async(path, file, metadata.len(), max_bytes).await
}

async fn open_file_with_metadata_async(path: &Path) -> io::Result<(tokio::fs::File, Metadata)> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) => return Err(normalize_file_open_error_async(path, error).await),
    };
    let metadata = file.metadata().await?;
    if !metadata.is_file() {
        return Err(file_type_error(path));
    }

    Ok((file, metadata))
}

async fn normalize_file_open_error_async(path: &Path, error: io::Error) -> io::Error {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if !metadata.is_file() => file_type_error(path),
        _ => error,
    }
}

async fn read_bytes_with_known_len_async<R>(
    path: &Path,
    reader: R,
    metadata_len: u64,
    max_bytes: u64,
) -> io::Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::with_capacity(read_buffer_capacity(metadata_len, max_bytes));
    if max_bytes == 0 {
        let mut reader = reader;
        reader.read_to_end(&mut bytes).await?;
        return Ok(bytes);
    }

    let mut reader = reader.take(max_bytes.saturating_add(1));
    reader.read_to_end(&mut bytes).await?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return Err(file_size_error(path, max_bytes));
    }

    Ok(bytes)
}

fn read_buffer_capacity(metadata_len: u64, max_bytes: u64) -> usize {
    let capacity = if max_bytes == 0 {
        metadata_len
    } else {
        metadata_len.min(max_bytes)
    };
    let capacity = capacity.min(READ_BUFFER_PREALLOC_MAX_BYTES);
    usize::try_from(capacity).unwrap_or(0)
}

fn file_size_error(path: &Path, max_bytes: u64) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "{} exceeds persistence file limit of {max_bytes} bytes",
            path.display()
        ),
    )
}

fn file_type_error(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{} is not a regular persistence file", path.display()),
    )
}

fn sync_parent_dir_best_effort(path: &Path) {
    if let Some(parent) = parent_dir(path) {
        let _ = sync_dir(parent);
    }
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> anyhow::Result<()> {
    let dir = File::open(path)?;
    dir.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

async fn sync_parent_dir_best_effort_async(path: &Path) {
    if let Some(parent) = parent_dir(path) {
        let _ = sync_dir_async(parent).await;
    }
}

#[cfg(unix)]
async fn sync_dir_async(path: &Path) -> anyhow::Result<()> {
    let dir = tokio::fs::File::open(path).await?;
    dir.sync_all().await?;
    Ok(())
}

#[cfg(not(unix))]
async fn sync_dir_async(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn parent_dir(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

fn temporary_path(path: &Path) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let counter = WRITE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("session.json");
    path.with_file_name(format!(
        ".{file_name}.tmp.{}.{unique}.{counter:016}",
        std::process::id(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "kuroya-persistence-storage-{name}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join("state")
            .join("session.json")
    }

    fn assert_no_write_temps(path: &Path) {
        let parent = path.parent().unwrap();
        let temp_count = fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.contains(".tmp."))
            })
            .count();
        assert_eq!(temp_count, 0);
    }

    #[test]
    fn temporary_paths_include_monotonic_counter() {
        let path = PathBuf::from("state/session.json");
        let first = temporary_path(&path);
        let second = temporary_path(&path);

        assert_ne!(first, second);
        let first_name = first.file_name().unwrap().to_str().unwrap();
        let second_name = second.file_name().unwrap().to_str().unwrap();
        assert!(first_name.starts_with(".session.json.tmp."));
        assert!(second_name.starts_with(".session.json.tmp."));
        assert_eq!(first_name.rsplit('.').next().unwrap().len(), 16);
        assert_eq!(second_name.rsplit('.').next().unwrap().len(), 16);
    }

    #[test]
    fn read_buffer_capacity_uses_known_file_size_without_oversizing() {
        assert_eq!(read_buffer_capacity(5, 1024), 5);
        assert_eq!(read_buffer_capacity(1025, 1024), 1024);
        assert_eq!(read_buffer_capacity(5, 0), 5);
        assert_eq!(read_buffer_capacity(u64::MAX, 1024), 1024);
        assert_eq!(
            read_buffer_capacity(u64::MAX, 0),
            usize::try_from(READ_BUFFER_PREALLOC_MAX_BYTES).unwrap()
        );
    }

    #[test]
    fn read_bytes_with_known_len_still_rejects_growth_past_limit() {
        let error = read_bytes_with_known_len(
            Path::new("session.json"),
            std::io::Cursor::new(b"growth".to_vec()),
            4,
            4,
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn read_bytes_with_known_len_async_still_rejects_growth_past_limit() {
        let error = read_bytes_with_known_len_async(
            Path::new("session.json"),
            std::io::Cursor::new(b"growth".to_vec()),
            4,
            4,
        )
        .await
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_file_bytes_with_limit_preserves_missing_not_found_errors() {
        let path = temp_file("read-missing");

        let error = read_file_bytes_with_limit(&path, 1024).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn read_file_bytes_with_limit_rejects_oversized_files_as_invalid_data() {
        let path = temp_file("read-oversized");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"oversized").unwrap();

        let error = read_file_bytes_with_limit(&path, 4).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_file_bytes_with_limit_zero_limit_reads_entire_file() {
        let path = temp_file("read-zero-limit");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"unbounded").unwrap();

        let bytes = read_file_bytes_with_limit(&path, 0).unwrap();

        assert_eq!(bytes, b"unbounded");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_file_bytes_with_limit_rejects_directories_as_invalid_data() {
        let path = temp_file("read-directory");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();
        fs::create_dir_all(&path).unwrap();

        let error = read_file_bytes_with_limit(&path, 1024).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_write_creates_parent_dirs_and_cleans_temps() {
        let path = temp_file("sync-parent");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();

        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_no_write_temps(&path);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_write_temp_creation_skips_existing_candidate_without_truncating_it() {
        let path = temp_file("sync-temp-collision");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        let first_temp = parent.join(".session.json.tmp.collision.first");
        let second_temp = parent.join(".session.json.tmp.collision.second");
        fs::write(&first_temp, b"keep existing temp").unwrap();

        atomic_write_with_temp_candidates(
            &path,
            b"replacement",
            [first_temp.clone(), second_temp.clone()],
        )
        .unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        assert_eq!(fs::read(&first_temp).unwrap(), b"keep existing temp");
        assert!(!second_temp.exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn atomic_write_async_creates_parent_dirs_and_cleans_temps() {
        let path = temp_file("async-parent");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();

        atomic_write_async(&path, b"async").await.unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"async");
        assert_no_write_temps(&path);

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn read_file_bytes_with_limit_async_preserves_missing_not_found_errors() {
        let path = temp_file("async-read-missing");

        let error = read_file_bytes_with_limit_async(&path, 1024)
            .await
            .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn read_file_bytes_with_limit_async_rejects_oversized_files_as_invalid_data() {
        let path = temp_file("async-read-oversized");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"oversized").unwrap();

        let error = read_file_bytes_with_limit_async(&path, 4)
            .await
            .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn read_file_bytes_with_limit_async_zero_limit_reads_entire_file() {
        let path = temp_file("async-read-zero-limit");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"unbounded").unwrap();

        let bytes = read_file_bytes_with_limit_async(&path, 0).await.unwrap();

        assert_eq!(bytes, b"unbounded");

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn read_file_bytes_with_limit_async_rejects_directories_as_invalid_data() {
        let path = temp_file("async-read-directory");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();
        fs::create_dir_all(&path).unwrap();

        let error = read_file_bytes_with_limit_async(&path, 1024)
            .await
            .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn atomic_write_async_temp_creation_skips_existing_candidate_without_truncating_it() {
        let path = temp_file("async-temp-collision");
        let root = path.parent().and_then(Path::parent).unwrap().to_path_buf();
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        let first_temp = parent.join(".session.json.tmp.async-collision.first");
        let second_temp = parent.join(".session.json.tmp.async-collision.second");
        fs::write(&first_temp, b"keep existing async temp").unwrap();

        atomic_write_async_with_temp_candidates(
            &path,
            b"async replacement",
            [first_temp.clone(), second_temp.clone()],
        )
        .await
        .unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"async replacement");
        assert_eq!(fs::read(&first_temp).unwrap(), b"keep existing async temp");
        assert!(!second_temp.exists());

        fs::remove_dir_all(root).unwrap();
    }
}

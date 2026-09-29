use crate::{
    persistence_storage::app_state_dir,
    ui_event_channel::{Sender, send_ui_event},
    ui_events::UiEvent,
};
use anyhow::Context;
use base64::Engine as _;
use kuroya_core::{LspServerConfig, lsp_registry::registry_entry_for};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::io::AsyncWriteExt;

pub(crate) const LSP_RELEASE_DOWNLOAD_BASE: &str =
    "https://github.com/kaevris/kuroya/releases/download/lsps";
const LSP_RELEASE_DOWNLOAD_PATH_PREFIX: &str = "/kaevris/kuroya/releases/download/lsps/";
const LSP_DOWNLOAD_USER_AGENT: &str = concat!("Kuroya/", env!("CARGO_PKG_VERSION"));
const LSP_DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const LSP_DOWNLOAD_CHUNK_CHANNEL_DEPTH: usize = 16;
pub(crate) const LSP_DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const LSP_BUNDLES_DIR_NAME: &str = "lsps";
const LSP_BUNDLE_PART_FILE_NAME: &str = "download.part";
const LSP_BUNDLE_INSTALLED_MARKER_FILE_NAME: &str = "installed.json";
#[cfg(any(unix, test))]
const LSP_ARCHIVE_ENTRY_PATH_MAX_CHARS: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LspInstallFailure {
    Shell { detail: String },
    Verify { detail: String },
    Download { detail: String },
    Checksum { detail: String },
    MissingAsset { detail: String },
    Extract { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepoBundleInstall {
    pub(crate) id: String,
    pub(crate) asset: String,
    pub(crate) launch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct InstalledLspBundle {
    pub(crate) asset: String,
    pub(crate) launch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RustInstallDecision {
    Rustup,
    RepoDownload,
}

pub(crate) fn lsp_release_asset_url(asset: &str) -> String {
    format!("{LSP_RELEASE_DOWNLOAD_BASE}/{asset}")
}

pub(crate) fn lsp_release_checksum_url(asset: &str) -> String {
    format!("{}/{asset}.sha256", LSP_RELEASE_DOWNLOAD_BASE)
}

pub(crate) fn lsp_download_url_is_pinned(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host = match authority.rsplit_once('@') {
        Some((_userinfo, host)) => host,
        None => authority,
    };
    let host = host.split(':').next().unwrap_or(host);
    if host.eq_ignore_ascii_case("objects.githubusercontent.com") {
        return true;
    }
    if !host.eq_ignore_ascii_case("github.com") {
        return false;
    }
    let Some(path) = rest.get(authority_end..) else {
        return false;
    };
    path.starts_with(LSP_RELEASE_DOWNLOAD_PATH_PREFIX)
}

pub(crate) fn lsp_bundles_dir() -> PathBuf {
    app_state_dir().join(LSP_BUNDLES_DIR_NAME)
}

pub(crate) fn lsp_bundle_dir(id: &str) -> PathBuf {
    lsp_bundles_dir().join(id)
}

fn lsp_bundle_part_path(bundle_dir: &Path) -> PathBuf {
    bundle_dir.join(LSP_BUNDLE_PART_FILE_NAME)
}

fn lsp_bundle_marker_path(bundle_dir: &Path) -> PathBuf {
    bundle_dir.join(LSP_BUNDLE_INSTALLED_MARKER_FILE_NAME)
}

pub(crate) fn lsp_bundle_binary_path(bundle_dir: &Path, launch: &str) -> PathBuf {
    #[cfg(windows)]
    {
        bundle_dir.join(format!("{launch}.exe"))
    }
    #[cfg(not(windows))]
    {
        bundle_dir.join(launch)
    }
}

pub(crate) fn read_installed_lsp_bundle(bundle_dir: &Path) -> Option<InstalledLspBundle> {
    let text = std::fs::read_to_string(lsp_bundle_marker_path(bundle_dir)).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_installed_lsp_bundle(
    bundle_dir: &Path,
    bundle: &InstalledLspBundle,
) -> anyhow::Result<()> {
    let text = serde_json::to_string_pretty(bundle)?;
    std::fs::write(lsp_bundle_marker_path(bundle_dir), text).with_context(|| {
        format!(
            "could not write {}",
            lsp_bundle_marker_path(bundle_dir).display()
        )
    })
}

pub(crate) fn lsp_bundle_command_override(config: &LspServerConfig) -> Option<PathBuf> {
    let definition = registry_entry_for(&config.language)?;
    if !definition.supports_repo_download() || definition.launch != config.command {
        return None;
    }
    let bundle_dir = lsp_bundle_dir(&definition.id);
    let bundle = read_installed_lsp_bundle(&bundle_dir)?;
    let binary = lsp_bundle_binary_path(&bundle_dir, &bundle.launch);
    binary.is_file().then_some(binary)
}

pub(crate) fn resolved_lsp_server_command(config: &LspServerConfig) -> String {
    lsp_bundle_command_override(config)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| config.command.clone())
}

pub(crate) fn rust_install_decision(rustup_available: bool) -> RustInstallDecision {
    if rustup_available {
        RustInstallDecision::Rustup
    } else {
        RustInstallDecision::RepoDownload
    }
}

pub(crate) async fn detect_rustup() -> bool {
    matches!(
        tokio::process::Command::new("rustup")
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .output()
            .await,
        Ok(output) if output.status.success()
    )
}

pub(crate) async fn download_and_install_lsp_bundle(
    install: &RepoBundleInstall,
    bytes_downloaded: Arc<AtomicU64>,
) -> Result<PathBuf, LspInstallFailure> {
    let asset_url = lsp_release_asset_url(&install.asset);
    let checksum_url = lsp_release_checksum_url(&install.asset);
    if !lsp_download_url_is_pinned(&asset_url) || !lsp_download_url_is_pinned(&checksum_url) {
        return Err(LspInstallFailure::Download {
            detail: format!(
                "{} is not hosted on the pinned Kuroya lsps release",
                install.asset
            ),
        });
    }
    let client = reqwest::Client::builder()
        .user_agent(LSP_DOWNLOAD_USER_AGENT)
        .connect_timeout(LSP_DOWNLOAD_CONNECT_TIMEOUT)
        .build()
        .map_err(|error| LspInstallFailure::Download {
            detail: format!("could not create download client: {error}"),
        })?;
    let expected_checksum =
        fetch_lsp_bundle_checksum(&client, &checksum_url, &install.asset).await?;
    let bundle_dir = lsp_bundle_dir(&install.id);
    tokio::fs::create_dir_all(&bundle_dir)
        .await
        .map_err(|error| LspInstallFailure::Download {
            detail: format!("could not create {}: {error}", bundle_dir.display()),
        })?;
    let response =
        client
            .get(&asset_url)
            .send()
            .await
            .map_err(|error| LspInstallFailure::Download {
                detail: format!("could not download {}: {error}", install.asset),
            })?;
    let mut response = lsp_download_response(response, &install.asset)?;
    let (chunk_tx, chunk_rx) = tokio::sync::mpsc::channel(LSP_DOWNLOAD_CHUNK_CHANNEL_DEPTH);
    let chunk_error_context = install.asset.clone();
    tokio::spawn(async move {
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if chunk_tx.send(Ok(chunk.to_vec())).await.is_err() {
                        return;
                    }
                }
                Ok(None) => return,
                Err(error) => {
                    let _ = chunk_tx
                        .send(Err(anyhow::Error::new(error)
                            .context(format!("could not read {chunk_error_context}"))))
                        .await;
                    return;
                }
            }
        }
    });
    let archive_path = finalize_lsp_download(
        &bundle_dir,
        &install.asset,
        chunk_rx,
        &expected_checksum,
        &bytes_downloaded,
    )
    .await?;
    match activate_lsp_archive(&bundle_dir, &archive_path, &install.launch).await {
        Ok(binary_path) => Ok(binary_path),
        Err(failure) => {
            let _ = std::fs::remove_file(&archive_path);
            cleanup_failed_lsp_bundle_install(&bundle_dir, &install.launch);
            Err(failure)
        }
    }
}

fn cleanup_failed_lsp_bundle_install(bundle_dir: &Path, launch: &str) {
    let _ = std::fs::remove_file(lsp_bundle_binary_path(bundle_dir, launch));
    let _ = std::fs::remove_file(lsp_bundle_marker_path(bundle_dir));
    let _ = std::fs::remove_file(lsp_bundle_part_path(bundle_dir));
}

async fn fetch_lsp_bundle_checksum(
    client: &reqwest::Client,
    sidecar_url: &str,
    asset_name: &str,
) -> Result<String, LspInstallFailure> {
    let response =
        client
            .get(sidecar_url)
            .send()
            .await
            .map_err(|error| LspInstallFailure::Download {
                detail: format!("could not download the checksum for {asset_name}: {error}"),
            })?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(LspInstallFailure::MissingAsset {
            detail: format!("the checksum sidecar for {asset_name} was not found"),
        });
    }
    let response = response
        .error_for_status()
        .map_err(|error| LspInstallFailure::Download {
            detail: format!("checksum download failed for {asset_name}: {error}"),
        })?;
    let text = response
        .text()
        .await
        .map_err(|error| LspInstallFailure::Download {
            detail: format!("could not read the checksum for {asset_name}: {error}"),
        })?;
    crate::update_checker::checksum_from_sidecar_text(&text).ok_or_else(|| {
        LspInstallFailure::Checksum {
            detail: format!("could not parse the checksum sidecar for {asset_name}"),
        }
    })
}

fn lsp_download_response(
    response: reqwest::Response,
    asset_name: &str,
) -> Result<reqwest::Response, LspInstallFailure> {
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(LspInstallFailure::MissingAsset {
            detail: format!("{asset_name} was not found on the lsps release"),
        });
    }
    response
        .error_for_status()
        .map_err(|error| LspInstallFailure::Download {
            detail: format!("download failed for {asset_name}: {error}"),
        })
}

async fn finalize_lsp_download(
    bundle_dir: &Path,
    asset_name: &str,
    mut chunks: tokio::sync::mpsc::Receiver<anyhow::Result<Vec<u8>>>,
    expected_checksum: &str,
    bytes_downloaded: &AtomicU64,
) -> Result<PathBuf, LspInstallFailure> {
    let part_path = lsp_bundle_part_path(bundle_dir);
    let archive_path = bundle_dir.join(asset_name);
    let result = async {
        stream_lsp_download_chunks(&mut chunks, &part_path, bytes_downloaded).await?;
        let actual_checksum =
            crate::update_checker::file_sha256_hex(&part_path).map_err(|error| {
                LspInstallFailure::Download {
                    detail: error.to_string(),
                }
            })?;
        if !actual_checksum.eq_ignore_ascii_case(expected_checksum) {
            return Err(LspInstallFailure::Checksum {
                detail: format!("checksum mismatch for {asset_name}"),
            });
        }
        let _ = std::fs::remove_file(&archive_path);
        std::fs::rename(&part_path, &archive_path).map_err(|error| {
            LspInstallFailure::Download {
                detail: format!("could not finalize {}: {error}", archive_path.display()),
            }
        })?;
        Ok(archive_path)
    }
    .await;
    if result.is_err() {
        let _ = std::fs::remove_file(&part_path);
    }
    result
}

async fn stream_lsp_download_chunks(
    chunks: &mut tokio::sync::mpsc::Receiver<anyhow::Result<Vec<u8>>>,
    part_path: &Path,
    bytes_downloaded: &AtomicU64,
) -> Result<(), LspInstallFailure> {
    let mut file =
        tokio::fs::File::create(part_path)
            .await
            .map_err(|error| LspInstallFailure::Download {
                detail: format!("could not create {}: {error}", part_path.display()),
            })?;
    let mut total_bytes = 0u64;
    while let Some(chunk) = chunks.recv().await {
        let chunk = chunk.map_err(|error| LspInstallFailure::Download {
            detail: format!("could not stream {}: {error}", part_path.display()),
        })?;
        file.write_all(&chunk)
            .await
            .map_err(|error| LspInstallFailure::Download {
                detail: format!("could not write {}: {error}", part_path.display()),
            })?;
        total_bytes += chunk.len() as u64;
        bytes_downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    file.flush()
        .await
        .map_err(|error| LspInstallFailure::Download {
            detail: format!("could not flush {}: {error}", part_path.display()),
        })?;
    if total_bytes == 0 {
        return Err(LspInstallFailure::Download {
            detail: format!("{} downloaded no data", part_path.display()),
        });
    }
    Ok(())
}

async fn activate_lsp_archive(
    bundle_dir: &Path,
    archive_path: &Path,
    launch: &str,
) -> Result<PathBuf, LspInstallFailure> {
    extract_lsp_archive_checked(archive_path, bundle_dir).await?;
    let binary_path = lsp_bundle_binary_path(bundle_dir, launch);
    if !binary_path.is_file() {
        return Err(LspInstallFailure::Extract {
            detail: format!("the archive did not contain {launch} at its root"),
        });
    }
    write_installed_lsp_bundle(
        bundle_dir,
        &InstalledLspBundle {
            asset: archive_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            launch: launch.to_owned(),
        },
    )
    .map_err(|error| LspInstallFailure::Extract {
        detail: error.to_string(),
    })?;
    let _ = std::fs::remove_file(archive_path);
    Ok(binary_path)
}

#[cfg(any(unix, test))]
fn lsp_archive_entry_escapes_target(entry: &str) -> bool {
    if entry.chars().count() > LSP_ARCHIVE_ENTRY_PATH_MAX_CHARS
        || entry.trim().is_empty()
        || entry.chars().any(char::is_control)
    {
        return true;
    }
    let normalized = entry.replace('\\', "/");
    if normalized.starts_with('/') {
        return true;
    }
    let bytes = normalized.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return true;
    }
    normalized.split('/').any(|component| component == "..")
}

fn extracted_path_escapes_bundle(bundle_dir: &Path, extracted_path: &Path) -> bool {
    if !extracted_path.starts_with(bundle_dir) {
        return true;
    }
    let Ok(bundle_root) = std::fs::canonicalize(bundle_dir) else {
        return false;
    };
    match std::fs::canonicalize(extracted_path) {
        Ok(resolved) => !resolved.starts_with(&bundle_root),
        Err(_) => false,
    }
}

fn find_escaping_extracted_path(bundle_dir: &Path) -> Option<PathBuf> {
    fn walk(directory: &Path, bundle_dir: &Path, offender: &mut Option<PathBuf>) {
        if offender.is_some() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if extracted_path_escapes_bundle(bundle_dir, &path) {
                *offender = Some(path);
                return;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                walk(&path, bundle_dir, offender);
                if offender.is_some() {
                    return;
                }
            }
        }
    }
    let mut offender = None;
    walk(bundle_dir, bundle_dir, &mut offender);
    offender
}

#[cfg(windows)]
const LSP_ZIP_ENTRY_UNSAFE_FILTER: &str = "$n = $_.FullName.Replace('/', '\\'); \
     ($n.Length -gt 4096) -or ($n.Trim().Length -eq 0) -or ($n -match '[\\x00-\\x1f]') \
     -or ($n.StartsWith('\\')) -or ($n -match '^[A-Za-z]:') \
     -or ($n.Split('\\') -contains '..')";

async fn extract_lsp_archive_checked(
    archive_path: &Path,
    target_dir: &Path,
) -> Result<(), LspInstallFailure> {
    #[cfg(windows)]
    {
        let script = "Add-Type -AssemblyName System.IO.Compression.FileSystem; ".to_owned()
            + "$zip = [System.IO.Compression.ZipFile]::OpenRead(%ARCHIVE%); "
            + "$unsafe = @($zip.Entries | Where-Object { "
            + LSP_ZIP_ENTRY_UNSAFE_FILTER
            + " }); "
            + "if ($unsafe.Count -gt 0) { throw ('unsafe archive entry: ' + $unsafe[0].FullName) }; "
            + "[System.IO.Compression.ZipFile]::ExtractToDirectory(%ARCHIVE%, %TARGET%); "
            + "$zip.Dispose()";
        let script = script
            .replace("%ARCHIVE%", &powershell_single_quoted(archive_path))
            .replace("%TARGET%", &powershell_single_quoted(target_dir));
        let output = run_windows_powershell(&script).await?;
        if !output.status.success() {
            return Err(LspInstallFailure::Extract {
                detail: format!(
                    "could not extract {}: {}",
                    archive_path.display(),
                    windows_stderr_tail(&output)
                ),
            });
        }
    }
    #[cfg(not(windows))]
    {
        let listing = tokio::process::Command::new("tar")
            .args(["-tzf"])
            .arg(archive_path)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|error| LspInstallFailure::Extract {
                detail: format!("could not run tar: {error}"),
            })?;
        if !listing.status.success() {
            return Err(LspInstallFailure::Extract {
                detail: format!("could not list {}", archive_path.display()),
            });
        }
        let listing_text = String::from_utf8_lossy(&listing.stdout);
        if let Some(entry) = listing_text
            .lines()
            .find(|entry| lsp_archive_entry_escapes_target(entry))
        {
            return Err(LspInstallFailure::Extract {
                detail: format!("archive entry escapes the install directory: {entry}"),
            });
        }
        let output = tokio::process::Command::new("tar")
            .args(["-xzf"])
            .arg(archive_path)
            .arg("-C")
            .arg(target_dir)
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|error| LspInstallFailure::Extract {
                detail: format!("could not run tar: {error}"),
            })?;
        if !output.status.success() {
            return Err(LspInstallFailure::Extract {
                detail: format!("could not extract {}", archive_path.display()),
            });
        }
    }
    if let Some(offending_path) = find_escaping_extracted_path(target_dir) {
        let _ = std::fs::remove_dir_all(target_dir);
        return Err(LspInstallFailure::Extract {
            detail: format!(
                "extracted path escaped the install directory: {}",
                offending_path.display()
            ),
        });
    }
    Ok(())
}

#[cfg(windows)]
async fn run_windows_powershell(script: &str) -> Result<std::process::Output, LspInstallFailure> {
    let utf16le: Vec<u8> = script
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16le);
    tokio::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| LspInstallFailure::Extract {
            detail: format!("could not run PowerShell: {error}"),
        })
}

#[cfg(windows)]
fn powershell_single_quoted(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

#[cfg(windows)]
fn windows_stderr_tail(output: &std::process::Output) -> String {
    windows_stderr_text_tail(&String::from_utf8_lossy(&output.stderr))
}

#[cfg(windows)]
fn windows_stderr_text_tail(stderr: &str) -> String {
    let flat = if stderr.trim_start().starts_with("#< CLIXML") {
        stderr
            .split("<S S=\"Error\">")
            .skip(1)
            .filter_map(|fragment| {
                let end = fragment.find("</S>")?;
                Some(decode_clixml_escapes(&fragment[..end]))
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        stderr.trim().to_owned()
    };
    let tail = flat
        .chars()
        .filter(|character| !character.is_control())
        .take(160)
        .collect::<String>();
    if tail.is_empty() {
        "unknown PowerShell error".to_owned()
    } else {
        tail
    }
}

#[cfg(windows)]
fn decode_clixml_escapes(raw: &str) -> String {
    let mut decoded = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(position) = rest.find("_x") {
        decoded.push_str(&rest[..position]);
        let tail = &rest[position + 2..];
        let bytes = tail.as_bytes();
        if bytes.len() >= 5 && bytes[4] == b'_' && bytes[..4].iter().all(u8::is_ascii_hexdigit) {
            let code = u32::from_str_radix(std::str::from_utf8(&bytes[..4]).unwrap_or(""), 16)
                .unwrap_or(0xFFFD);
            if let Some(character) = char::from_u32(code) {
                decoded.push(character);
            }
            rest = &tail[5..];
        } else {
            decoded.push('_');
            rest = tail;
        }
    }
    decoded.push_str(rest);
    decoded
}

pub(crate) fn spawn_lsp_download_progress_task(
    tx: Sender<UiEvent>,
    bytes_downloaded: Arc<AtomicU64>,
    language: String,
    display_name: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(LSP_DOWNLOAD_PROGRESS_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;
        let mut last_reported = 0u64;
        loop {
            interval.tick().await;
            let bytes = bytes_downloaded.load(Ordering::Relaxed);
            if bytes == last_reported {
                continue;
            }
            last_reported = bytes;
            if !send_ui_event(
                &tx,
                UiEvent::LspInstallProgress {
                    language: language.clone(),
                    display_name: display_name.clone(),
                    bytes_downloaded: bytes,
                },
            ) {
                return;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        InstalledLspBundle, LSP_ARCHIVE_ENTRY_PATH_MAX_CHARS, LspInstallFailure, RepoBundleInstall,
        RustInstallDecision, cleanup_failed_lsp_bundle_install, extracted_path_escapes_bundle,
        finalize_lsp_download, find_escaping_extracted_path, lsp_archive_entry_escapes_target,
        lsp_bundle_binary_path, lsp_bundle_command_override, lsp_bundle_dir, lsp_bundles_dir,
        lsp_download_url_is_pinned, lsp_release_asset_url, lsp_release_checksum_url,
        read_installed_lsp_bundle, resolved_lsp_server_command, rust_install_decision,
        write_installed_lsp_bundle,
    };
    #[cfg(windows)]
    use super::{
        LSP_ZIP_ENTRY_UNSAFE_FILTER, extract_lsp_archive_checked, powershell_single_quoted,
        run_windows_powershell, windows_stderr_tail, windows_stderr_text_tail,
    };
    use kuroya_core::{LspServerConfig, lsp_registry::registry_entry_for};
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };
    use tokio::runtime::Runtime;

    fn lsp_server_config(language: &str, command: &str) -> LspServerConfig {
        LspServerConfig {
            language: language.to_owned(),
            command: command.to_owned(),
            args: Vec::new(),
            extensions: Vec::new(),
            root_markers: Vec::new(),
            enabled: false,
        }
    }

    fn unique_bundle_sandbox(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-lsp-installer-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    async fn test_chunk_receiver(
        chunks: Vec<anyhow::Result<Vec<u8>>>,
    ) -> tokio::sync::mpsc::Receiver<anyhow::Result<Vec<u8>>> {
        let (tx, rx) = tokio::sync::mpsc::channel(chunks.len().max(1));
        for chunk in chunks {
            tx.send(chunk).await.expect("chunk should send");
        }
        rx
    }

    #[test]
    fn download_url_pin_accepts_only_the_pinned_lsps_release_urls() {
        assert!(lsp_download_url_is_pinned(&lsp_release_asset_url(
            "lsp-gopls-windows-x64.zip"
        )));
        assert!(lsp_download_url_is_pinned(&lsp_release_checksum_url(
            "lsp-gopls-windows-x64.zip"
        )));
        assert!(lsp_download_url_is_pinned(
            "https://objects.githubusercontent.com/signed-asset-path"
        ));
        assert!(lsp_download_url_is_pinned(
            "https://GITHUB.com/kaevris/kuroya/releases/download/lsps/lsp-gopls-windows-x64.zip"
        ));
        assert!(lsp_download_url_is_pinned(
            "https://github.com:443/kaevris/kuroya/releases/download/lsps/lsp-gopls-linux-x64.tar.gz"
        ));
    }

    #[test]
    fn download_url_pin_rejects_foreign_hosts_schemes_and_paths() {
        assert!(!lsp_download_url_is_pinned(
            "http://github.com/kaevris/kuroya/releases/download/lsps/lsp-gopls-windows-x64.zip"
        ));
        assert!(!lsp_download_url_is_pinned(
            "https://evil.test/kaevris/kuroya/releases/download/lsps/lsp-gopls-windows-x64.zip"
        ));
        assert!(!lsp_download_url_is_pinned(
            "https://github.com/kaevris/other/releases/download/lsps/lsp-gopls-windows-x64.zip"
        ));
        assert!(!lsp_download_url_is_pinned(
            "https://github.com/kaevris/kuroya/releases/download/v0.2.0/Kuroya-Setup.exe"
        ));
        assert!(!lsp_download_url_is_pinned(
            "https://github.com/kaevris/kuroya/releases/tag/lsps"
        ));
        assert!(!lsp_download_url_is_pinned("https://github.com"));
        assert!(!lsp_download_url_is_pinned("not a url"));
    }

    #[test]
    fn release_urls_are_built_from_the_pinned_base() {
        assert_eq!(
            lsp_release_asset_url("lsp-marksman-macos-x64.tar.gz"),
            "https://github.com/kaevris/kuroya/releases/download/lsps/lsp-marksman-macos-x64.tar.gz"
        );
        assert_eq!(
            lsp_release_checksum_url("lsp-clangd-linux-x64.tar.gz"),
            "https://github.com/kaevris/kuroya/releases/download/lsps/lsp-clangd-linux-x64.tar.gz.sha256"
        );
    }

    #[test]
    fn archive_entries_escaping_the_target_are_rejected() {
        assert!(lsp_archive_entry_escapes_target("../evil.exe"));
        assert!(lsp_archive_entry_escapes_target("..\\evil.exe"));
        assert!(lsp_archive_entry_escapes_target("bin/../../evil.exe"));
        assert!(lsp_archive_entry_escapes_target("..\\../evil.exe"));
        assert!(lsp_archive_entry_escapes_target("/absolute/evil.exe"));
        assert!(lsp_archive_entry_escapes_target("\\absolute\\evil.exe"));
        assert!(lsp_archive_entry_escapes_target("C:\\absolute\\evil.exe"));
        assert!(lsp_archive_entry_escapes_target("C:/absolute/evil.exe"));
        assert!(lsp_archive_entry_escapes_target("C:relative/evil.exe"));
        assert!(lsp_archive_entry_escapes_target("//server/share/evil.exe"));
        assert!(lsp_archive_entry_escapes_target(".."));
        assert!(lsp_archive_entry_escapes_target("bin/.."));
        assert!(lsp_archive_entry_escapes_target("./../evil.exe"));
        assert!(lsp_archive_entry_escapes_target("\u{0001}evil.exe"));
        assert!(lsp_archive_entry_escapes_target("evil\u{007f}.exe"));
        assert!(lsp_archive_entry_escapes_target(""));
        assert!(lsp_archive_entry_escapes_target("   "));
        assert!(lsp_archive_entry_escapes_target(&format!(
            "deep/{}",
            "a".repeat(LSP_ARCHIVE_ENTRY_PATH_MAX_CHARS + 1)
        )));

        assert!(!lsp_archive_entry_escapes_target("gopls"));
        assert!(!lsp_archive_entry_escapes_target("./gopls"));
        assert!(!lsp_archive_entry_escapes_target("./"));
        assert!(!lsp_archive_entry_escapes_target("."));
        assert!(!lsp_archive_entry_escapes_target("bin/gopls.exe"));
        assert!(!lsp_archive_entry_escapes_target("clangd/clangd"));
        assert!(!lsp_archive_entry_escapes_target("rust-analyzer"));
        assert!(!lsp_archive_entry_escapes_target("..evil.txt"));
        assert!(!lsp_archive_entry_escapes_target("evil..txt"));
        assert!(!lsp_archive_entry_escapes_target("C"));
        assert!(!lsp_archive_entry_escapes_target(&format!(
            "deep/{}",
            "a".repeat(LSP_ARCHIVE_ENTRY_PATH_MAX_CHARS - 5)
        )));
    }

    #[test]
    fn containment_check_flags_only_paths_outside_the_bundle_directory() {
        let sandbox = unique_bundle_sandbox("containment-decision");
        let bundle_dir = sandbox.join("bundle");
        fs::create_dir_all(bundle_dir.join("nested")).expect("bundle dir");

        assert!(!extracted_path_escapes_bundle(
            &bundle_dir,
            &bundle_dir.join("marksman.exe")
        ));
        assert!(!extracted_path_escapes_bundle(
            &bundle_dir,
            &bundle_dir.join("nested").join("benign.txt")
        ));
        assert!(extracted_path_escapes_bundle(
            &bundle_dir,
            &sandbox.join("escaped.txt")
        ));
        assert!(extracted_path_escapes_bundle(
            &bundle_dir,
            &sandbox.join("bundle-sibling").join("escaped.txt")
        ));

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn containment_walk_accepts_a_fully_contained_bundle() {
        let sandbox = unique_bundle_sandbox("containment-walk-clean");
        let bundle_dir = sandbox.join("bundle");
        fs::create_dir_all(bundle_dir.join("nested").join("deeper")).expect("bundle dir");
        fs::write(bundle_dir.join("marksman.exe"), b"binary").unwrap();
        fs::write(bundle_dir.join("nested").join("benign.txt"), b"benign").unwrap();
        fs::write(
            bundle_dir.join("nested").join("deeper").join("benign.txt"),
            b"benign",
        )
        .unwrap();

        assert!(find_escaping_extracted_path(&bundle_dir).is_none());

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn containment_walk_catches_a_symlinked_escape() {
        let sandbox = unique_bundle_sandbox("containment-walk-symlink");
        let bundle_dir = sandbox.join("bundle");
        fs::create_dir_all(&bundle_dir).expect("bundle dir");
        fs::write(sandbox.join("outside.txt"), b"outside").unwrap();
        std::os::unix::fs::symlink(sandbox.join("outside.txt"), bundle_dir.join("link.txt"))
            .unwrap();

        let offender = find_escaping_extracted_path(&bundle_dir).expect("symlink escape");
        assert_eq!(offender, bundle_dir.join("link.txt"));

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[cfg(windows)]
    const HOSTILE_ZIP_ENTRY_NAMES: &[&str] = &[
        "../evil.txt",
        "..\\evil.txt",
        "a/../../evil.txt",
        "./../evil.txt",
        "/absolute.txt",
        "C:\\absolute.txt",
        "C:absolute.txt",
        "\\\\server\\share.txt",
        "a\\..\\/..\\evil.txt",
        "..",
    ];

    #[cfg(windows)]
    async fn write_test_zip_with_entries(zip_path: &Path, entry_names: &[&str]) {
        let mut script = "Add-Type -AssemblyName System.IO.Compression; ".to_owned()
            + "Add-Type -AssemblyName System.IO.Compression.FileSystem; "
            + "$zip = [System.IO.Compression.ZipFile]::Open(%ZIP%, 'Create'); ";
        for entry_name in entry_names {
            let escaped_name = entry_name.replace('\'', "''");
            script += &format!(
                "$entry = $zip.CreateEntry('{escaped_name}'); \
                 if ($null -eq $entry) {{ throw ('entry creation failed for {escaped_name}') }}; \
                 $stream = $entry.Open(); \
                 $bytes = [System.Text.Encoding]::UTF8.GetBytes('benign content'); \
                 $stream.Write($bytes, 0, $bytes.Length); $stream.Dispose(); "
            );
        }
        script += "$zip.Dispose(); ";
        script += &format!(
            "$check = [System.IO.Compression.ZipFile]::OpenRead(%ZIP%); \
             if ($check.Entries.Count -ne {}) {{ throw ('test zip entry count mismatch: ' + \
             $check.Entries.Count) }}; $check.Dispose();",
            entry_names.len()
        );
        let script = script.replace("%ZIP%", &powershell_single_quoted(zip_path));
        let output = run_windows_powershell(&script)
            .await
            .expect("test PowerShell should run");
        assert!(
            output.status.success(),
            "test zip creation failed: {}",
            windows_stderr_tail(&output)
        );
    }

    #[cfg(windows)]
    fn patch_zip_entry_name(zip_path: &Path, from: &[u8], to: &[u8]) {
        assert_eq!(
            from.len(),
            to.len(),
            "patched entry names must keep their byte length"
        );
        let mut bytes = fs::read(zip_path).expect("test zip should be readable");
        let mut name_positions = Vec::new();
        let mut start = 0;
        while let Some(position) = bytes[start..]
            .windows(from.len())
            .position(|window| window == from)
        {
            let at = start + position;
            name_positions.push(at);
            start = at + from.len();
        }
        assert_eq!(
            name_positions.len(),
            2,
            "the entry name must appear exactly in the local header and the central directory"
        );
        for (index, &position) in name_positions.iter().enumerate() {
            let (header_offset, flag_offset, signature): (usize, usize, &[u8]) = if index == 0 {
                (30, 6, b"PK\x03\x04")
            } else {
                (46, 8, b"PK\x01\x02")
            };
            assert_eq!(
                &bytes[position - header_offset..position - header_offset + 4],
                signature,
                "zip header layout changed"
            );
            bytes[position - header_offset + flag_offset + 1] |= 0x08;
            bytes[position..position + to.len()].copy_from_slice(to);
        }
        fs::write(zip_path, bytes).expect("test zip should be writable");
    }

    #[cfg(windows)]
    #[test]
    fn hostile_zip_entry_names_are_rejected_without_extracting_anything() {
        let runtime = Runtime::new().expect("test runtime");
        let sandbox = unique_bundle_sandbox("hostile-zip-entries");
        let target_dir = sandbox.join("bundle");
        fs::create_dir_all(&target_dir).expect("target dir");

        runtime.block_on(async {
            for (index, entry_name) in HOSTILE_ZIP_ENTRY_NAMES.iter().enumerate() {
                let archive_path = sandbox.join(format!("hostile-{index}.zip"));
                write_test_zip_with_entries(&archive_path, &[*entry_name, "benign.txt"]).await;
                let failure = extract_lsp_archive_checked(&archive_path, &target_dir)
                    .await
                    .err()
                    .unwrap_or_else(|| panic!("entry {entry_name:?} must be rejected"));
                let LspInstallFailure::Extract { detail } = failure else {
                    panic!("entry {entry_name:?} must fail as Extract, got {failure:?}");
                };
                assert!(
                    detail.contains("unsafe archive entry"),
                    "detail should name the hostile check for {entry_name:?}: {detail}"
                );
                assert!(
                    detail.contains(*entry_name),
                    "detail should contain the offending entry {entry_name:?}: {detail}"
                );
                assert!(
                    fs::read_dir(&target_dir)
                        .expect("target dir")
                        .next()
                        .is_none(),
                    "target dir must stay empty for {entry_name:?}"
                );
            }
        });

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn safe_zip_entries_extract_and_pass_the_containment_walk() {
        let runtime = Runtime::new().expect("test runtime");
        let sandbox = unique_bundle_sandbox("safe-zip-entries");
        let target_dir = sandbox.join("bundle");
        fs::create_dir_all(&target_dir).expect("target dir");

        runtime.block_on(async {
            let archive_path = sandbox.join("safe.zip");
            write_test_zip_with_entries(
                &archive_path,
                &["marksman.exe", "benign.txt", "nested/benign.txt"],
            )
            .await;
            let result = extract_lsp_archive_checked(&archive_path, &target_dir).await;
            assert!(result.is_ok(), "safe entries must extract: {result:?}");
        });

        assert!(target_dir.join("marksman.exe").is_file());
        assert!(target_dir.join("benign.txt").is_file());
        assert!(target_dir.join("nested").join("benign.txt").is_file());
        assert!(find_escaping_extracted_path(&target_dir).is_none());

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn control_char_zip_entry_names_never_reach_the_bundle_directory() {
        let runtime = Runtime::new().expect("test runtime");
        let sandbox = unique_bundle_sandbox("control-char-zip-entry");
        let target_dir = sandbox.join("bundle");
        fs::create_dir_all(&target_dir).expect("target dir");

        runtime.block_on(async {
            let archive_path = sandbox.join("control-char.zip");
            write_test_zip_with_entries(&archive_path, &["evil.txt", "benign.txt"]).await;
            patch_zip_entry_name(&archive_path, b"evil.txt", b"\x01evil.tx");
            let result = extract_lsp_archive_checked(&archive_path, &target_dir).await;
            assert!(
                result.is_ok(),
                "the extraction engine silently drops control-char entries, so the \
                 extraction must report success: {result:?}"
            );
        });

        assert!(
            fs::read_dir(&target_dir)
                .expect("target dir")
                .next()
                .is_none(),
            "no entry whose name contains a control character may reach the bundle directory"
        );

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn powershell_entry_filter_flags_hostile_variants_and_passes_safe_ones() {
        let hostile_names: &[&str] = &[
            "../evil.txt",
            "..\\evil.txt",
            "a/../../evil.txt",
            "./../evil.txt",
            "/absolute.txt",
            "\\absolute.txt",
            "C:\\absolute.txt",
            "C:absolute.txt",
            "\\\\server\\share.txt",
            "\u{0001}evil.txt",
            "\u{001f}",
            "",
            "   ",
            "..",
            "bin/..",
        ];
        let safe_names: &[&str] = &[
            "gopls.exe",
            "./gopls.exe",
            "./",
            ".",
            "bin/gopls.exe",
            "nested/dir/benign.txt",
            "..evil.txt",
            "evil..txt",
            "C",
        ];
        let mut script = "Add-Type -AssemblyName System.IO.Compression.FileSystem; ".to_owned()
            + "$mismatch = @(); ";
        for (kind, names) in [("hostile", hostile_names), ("safe", safe_names)] {
            let expected_flag = if kind == "hostile" { "$true" } else { "$false" };
            for name in names {
                let escaped = name.replace('\'', "''");
                script += &format!(
                    "$flagged = [bool]([pscustomobject] @{{ FullName = '{escaped}' }} | \
                     Where-Object {{ {LSP_ZIP_ENTRY_UNSAFE_FILTER} }}); \
                     $expected = {expected_flag}; \
                     if ($flagged -ne $expected) {{ \
                     $mismatch += ('{kind} entry flagged=' + $flagged + ': ' + '{escaped}') }}; "
                );
            }
        }
        script += "if ($mismatch.Count -gt 0) { throw ($mismatch -join '; ') }";
        let runtime = Runtime::new().expect("test runtime");
        let outcome = runtime.block_on(async {
            run_windows_powershell(&script)
                .await
                .expect("test PowerShell should run")
        });
        assert!(
            outcome.status.success(),
            "the PowerShell filter must agree with the Rust decision: {}",
            windows_stderr_tail(&outcome)
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_stderr_tail_decodes_clixml_error_streams() {
        let plain = windows_stderr_text_tail("unsafe archive entry: ..\r\nAt line:1\r\n");
        assert!(plain.starts_with("unsafe archive entry: .."));

        let clixml = concat!(
            "#< CLIXML\r\n",
            "<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\">",
            "<Obj S=\"progress\" RefId=\"0\"><TN RefId=\"0\"/></Obj>",
            "<S S=\"Error\">unsafe archive entry: ../evil.txt_x000D__x000A_</S>",
            "<S S=\"Error\">At line:1 char:1_x000D__x000A_</S>",
            "</Objs>"
        );
        let tail = windows_stderr_text_tail(clixml);
        assert!(tail.starts_with("unsafe archive entry: ../evil.txt At line:1 char:1"));
        assert!(!tail.contains("_x000D_"));
        assert!(!tail.contains("<S "));

        assert_eq!(windows_stderr_text_tail(""), "unknown PowerShell error");
    }

    #[test]
    fn finalize_download_streams_bytes_and_renames_the_part_file_into_the_archive() {
        let runtime = Runtime::new().expect("test runtime");
        let sandbox = unique_bundle_sandbox("finalize-success");
        let bundle_dir = sandbox.join("go");
        fs::create_dir_all(&bundle_dir).unwrap();
        let bytes_downloaded = Arc::new(AtomicU64::new(0));
        let body = b"gopls archive body";
        let expected_checksum = format!("{:x}", Sha256::digest(body));

        let archive_path = runtime.block_on(async {
            let chunks =
                test_chunk_receiver(vec![Ok(body[..7].to_vec()), Ok(body[7..].to_vec())]).await;
            finalize_lsp_download(
                &bundle_dir,
                "lsp-gopls-windows-x64.zip",
                chunks,
                &expected_checksum,
                &bytes_downloaded,
            )
            .await
        });

        let archive_path = archive_path.expect("finalize should succeed");
        assert_eq!(archive_path, bundle_dir.join("lsp-gopls-windows-x64.zip"));
        assert_eq!(fs::read(&archive_path).unwrap(), body);
        assert!(!bundle_dir.join("download.part").exists());
        assert_eq!(bytes_downloaded.load(Ordering::Relaxed), body.len() as u64);

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn finalize_download_rejects_a_checksum_mismatch_and_deletes_the_part_file() {
        let runtime = Runtime::new().expect("test runtime");
        let sandbox = unique_bundle_sandbox("finalize-checksum-fail");
        let bundle_dir = sandbox.join("clangd");
        fs::create_dir_all(&bundle_dir).unwrap();
        let bytes_downloaded = Arc::new(AtomicU64::new(0));

        let result = runtime.block_on(async {
            let chunks = test_chunk_receiver(vec![Ok(b"tampered archive".to_vec())]).await;
            finalize_lsp_download(
                &bundle_dir,
                "lsp-clangd-windows-x64.zip",
                chunks,
                &"0".repeat(64),
                &bytes_downloaded,
            )
            .await
        });

        assert!(matches!(result, Err(LspInstallFailure::Checksum { .. })));
        assert!(!bundle_dir.join("lsp-clangd-windows-x64.zip").exists());
        assert!(!bundle_dir.join("download.part").exists());

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn finalize_download_fails_when_the_stream_ends_without_bytes() {
        let runtime = Runtime::new().expect("test runtime");
        let sandbox = unique_bundle_sandbox("finalize-empty");
        let bundle_dir = sandbox.join("marksman");
        fs::create_dir_all(&bundle_dir).unwrap();
        let bytes_downloaded = Arc::new(AtomicU64::new(0));

        let result = runtime.block_on(async {
            let chunks = test_chunk_receiver(Vec::new()).await;
            finalize_lsp_download(
                &bundle_dir,
                "lsp-marksman-windows-x64.zip",
                chunks,
                &"a".repeat(64),
                &bytes_downloaded,
            )
            .await
        });

        assert!(matches!(result, Err(LspInstallFailure::Download { .. })));
        assert!(!bundle_dir.join("download.part").exists());
        assert_eq!(bytes_downloaded.load(Ordering::Relaxed), 0);

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn finalize_download_replaces_a_stale_archive_left_by_a_crashed_run() {
        let runtime = Runtime::new().expect("test runtime");
        let sandbox = unique_bundle_sandbox("finalize-stale-archive");
        let bundle_dir = sandbox.join("go");
        fs::create_dir_all(&bundle_dir).unwrap();
        let bytes_downloaded = Arc::new(AtomicU64::new(0));
        let body = b"fresh gopls archive";
        let expected_checksum = format!("{:x}", Sha256::digest(body));
        let archive_path = bundle_dir.join("lsp-gopls-windows-x64.zip");
        fs::write(&archive_path, b"stale archive from a crashed run").unwrap();

        let result = runtime.block_on(async {
            let chunks = test_chunk_receiver(vec![Ok(body.to_vec())]).await;
            finalize_lsp_download(
                &bundle_dir,
                "lsp-gopls-windows-x64.zip",
                chunks,
                &expected_checksum,
                &bytes_downloaded,
            )
            .await
        });

        assert_eq!(result.expect("finalize should succeed"), archive_path);
        assert_eq!(fs::read(&archive_path).unwrap(), body);

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn failed_activation_cleanup_removes_partial_binaries_and_markers() {
        let sandbox = unique_bundle_sandbox("activate-cleanup");
        let bundle_dir = sandbox.join("markdown");
        fs::create_dir_all(&bundle_dir).unwrap();
        let binary_path = lsp_bundle_binary_path(&bundle_dir, "marksman");
        fs::write(&binary_path, b"truncated marksman.exe").unwrap();
        write_installed_lsp_bundle(
            &bundle_dir,
            &InstalledLspBundle {
                asset: "lsp-marksman-windows-x64.zip".to_owned(),
                launch: "marksman".to_owned(),
            },
        )
        .expect("marker write should succeed");
        fs::write(bundle_dir.join("download.part"), b"stale part").unwrap();

        cleanup_failed_lsp_bundle_install(&bundle_dir, "marksman");

        assert!(
            !binary_path.exists(),
            "a partial binary must not shadow the PATH resolution after a failed install"
        );
        assert!(read_installed_lsp_bundle(&bundle_dir).is_none());
        assert!(!bundle_dir.join("download.part").exists());

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn installed_marker_round_trips_and_reports_the_launch_name() {
        let sandbox = unique_bundle_sandbox("marker-round-trip");
        let bundle_dir = sandbox.join("lua");
        fs::create_dir_all(&bundle_dir).unwrap();

        assert!(read_installed_lsp_bundle(&bundle_dir).is_none());

        write_installed_lsp_bundle(
            &bundle_dir,
            &InstalledLspBundle {
                asset: "lsp-lua-language-server-windows-x64.zip".to_owned(),
                launch: "lua-language-server".to_owned(),
            },
        )
        .expect("marker write should succeed");

        assert_eq!(
            read_installed_lsp_bundle(&bundle_dir),
            Some(InstalledLspBundle {
                asset: "lsp-lua-language-server-windows-x64.zip".to_owned(),
                launch: "lua-language-server".to_owned(),
            })
        );
        assert!(
            lsp_bundle_binary_path(&bundle_dir, "lua-language-server")
                .to_string_lossy()
                .ends_with(if cfg!(windows) {
                    "lua-language-server.exe"
                } else {
                    "lua-language-server"
                })
        );

        fs::remove_dir_all(sandbox).unwrap();
    }

    #[test]
    fn command_override_prefers_an_installed_marker_only_for_repo_servers() {
        for id in ["go", "c", "cpp"] {
            let bundle_dir = lsp_bundle_dir(id);
            fs::create_dir_all(&bundle_dir).expect("bundle dir");
            let launch = registry_entry_for(id).expect("entry").launch.clone();
            write_installed_lsp_bundle(
                &bundle_dir,
                &InstalledLspBundle {
                    asset: "lsp-sample-windows-x64.zip".to_owned(),
                    launch: launch.clone(),
                },
            )
            .expect("marker write");
            fs::write(lsp_bundle_binary_path(&bundle_dir, &launch), b"binary").unwrap();
        }

        let go = lsp_server_config("go", "gopls");
        let override_path = lsp_bundle_command_override(&go).expect("go override");
        assert!(override_path.starts_with(lsp_bundles_dir().join("go")));
        assert_eq!(
            resolved_lsp_server_command(&go),
            override_path.to_string_lossy()
        );

        let cpp = lsp_server_config("cpp", "clangd");
        let cpp_override = lsp_bundle_command_override(&cpp).expect("cpp override");
        assert!(cpp_override.starts_with(lsp_bundles_dir().join("cpp")));

        assert_eq!(
            resolved_lsp_server_command(&lsp_server_config("cpp", "clangd-custom")),
            "clangd-custom",
            "a customized command keeps the user's value"
        );

        let python = lsp_server_config("python", "pyright-langserver");
        assert!(
            lsp_bundle_command_override(&python).is_none(),
            "npm-only servers never resolve through the marker"
        );

        let rust = lsp_server_config("rust", "rust-analyzer");
        assert!(
            lsp_bundle_command_override(&rust).is_none(),
            "rust without an installed marker resolves through PATH"
        );

        let uninstalled_go = lsp_server_config("go", "gopls");
        let go_bundle_dir = lsp_bundle_dir("go");
        std::fs::remove_dir_all(go_bundle_dir).ok();
        assert!(
            lsp_bundle_command_override(&uninstalled_go).is_none(),
            "a missing marker falls back to PATH resolution"
        );

        for id in ["go", "c", "cpp"] {
            std::fs::remove_dir_all(lsp_bundle_dir(id)).ok();
        }
    }

    #[test]
    fn rust_install_decision_prefers_rustup_and_falls_back_to_the_repo() {
        assert_eq!(rust_install_decision(true), RustInstallDecision::Rustup);
        assert_eq!(
            rust_install_decision(false),
            RustInstallDecision::RepoDownload
        );
    }

    #[test]
    fn repo_bundle_installs_carry_the_id_asset_and_launch() {
        let install = RepoBundleInstall {
            id: "go".to_owned(),
            asset: "lsp-gopls-windows-x64.zip".to_owned(),
            launch: "gopls".to_owned(),
        };
        assert_eq!(install.id, "go");
        assert_eq!(install.asset, "lsp-gopls-windows-x64.zip");
        assert_eq!(install.launch, "gopls");
    }

    #[test]
    fn download_failures_stay_debuggable() {
        let failure = LspInstallFailure::MissingAsset {
            detail: "lsp-gopls-windows-x64.zip was not found".to_owned(),
        };
        assert!(format!("{failure:?}").contains("MissingAsset"));
        let failure = LspInstallFailure::Checksum {
            detail: "checksum mismatch for lsp-gopls-windows-x64.zip".to_owned(),
        };
        assert!(format!("{failure:?}").contains("checksum mismatch"));
    }
}

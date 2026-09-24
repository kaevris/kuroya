use crate::{
    KuroyaApp,
    ui_event_channel::{Sender, send_ui_event},
    ui_events::UiEvent,
    update_checker::{
        checksum_from_sidecar_text, download_url_is_pinned_to_repository_path, format_byte_size,
    },
};
use anyhow::Context;
use kuroya_core::lsp_registry::{
    LspInstallDefinition, LspInstallPlatform, lsp_launch_binary_name, lsp_release_asset_name,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt;

pub(crate) const LSP_BUNDLE_REPOSITORY: &str = "kaevris/kuroya";
pub(crate) const LSP_BUNDLE_RELEASE_TAG: &str = "lsps";
pub(crate) const LSP_BUNDLE_RELEASE_PAGE_URL: &str =
    "https://github.com/kaevris/kuroya/releases/tag/lsps";
const LSP_BUNDLE_USER_AGENT: &str = concat!("Kuroya/", env!("CARGO_PKG_VERSION"));
const LSP_BUNDLE_DOWNLOAD_DIR_NAME: &str = "lsps";
const LSP_BUNDLE_PART_FILE_NAME: &str = "download.part";
const LSP_BUNDLE_MARKER_FILE_NAME: &str = "installed.json";
const LSP_BUNDLE_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const LSP_BUNDLE_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const LSP_BUNDLE_LAUNCH_SEARCH_MAX_DEPTH: usize = 6;
const LSP_BUNDLE_DOWNLOAD_PATH_PREFIX: &str = "/releases/download/lsps/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LspInstallMarker {
    pub(crate) version: String,
    pub(crate) asset: String,
    pub(crate) launch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LspInstallFailure {
    Shell { detail: String },
    Verify { detail: String },
    Download { detail: String },
    Checksum { detail: String },
    Unavailable { release_url: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LspInstallOutcome {
    pub(crate) command_override: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LspInstallStage {
    Downloading { bytes_downloaded: u64 },
    Installing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LspBundleInstallPlan {
    pub(crate) server_id: String,
    pub(crate) display_name: String,
    pub(crate) asset: String,
    pub(crate) download_url: String,
    pub(crate) checksum_url: String,
    pub(crate) install_dir: PathBuf,
    pub(crate) launch_binary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LspBundleArchive {
    Zip,
    TarGz,
}

pub(crate) fn lsp_bundle_install_plan(
    definition: &LspInstallDefinition,
    platform: LspInstallPlatform,
) -> Option<LspBundleInstallPlan> {
    let asset = lsp_release_asset_name(definition, platform)?;
    let launch_binary = lsp_launch_binary_name(definition, platform)?;
    let download_url = lsp_bundle_download_url(&asset);
    if !lsp_bundle_url_is_pinned_to_release(&download_url) {
        return None;
    }
    Some(LspBundleInstallPlan {
        server_id: definition.id.clone(),
        display_name: definition.display_name.clone(),
        checksum_url: format!("{download_url}.sha256"),
        download_url,
        asset,
        install_dir: lsp_install_dir(&definition.id),
        launch_binary,
    })
}

pub(crate) fn lsp_bundle_download_url(asset: &str) -> String {
    format!(
        "https://github.com/{LSP_BUNDLE_REPOSITORY}/releases/download/{LSP_BUNDLE_RELEASE_TAG}/{asset}"
    )
}

pub(crate) fn lsp_bundle_url_is_pinned_to_release(url: &str) -> bool {
    download_url_is_pinned_to_repository_path(
        url,
        LSP_BUNDLE_REPOSITORY,
        LSP_BUNDLE_DOWNLOAD_PATH_PREFIX,
    )
}

pub(crate) fn lsp_install_root_dir() -> PathBuf {
    crate::persistence_storage::app_state_dir().join(LSP_BUNDLE_DOWNLOAD_DIR_NAME)
}

pub(crate) fn lsp_install_dir(server_id: &str) -> PathBuf {
    lsp_install_root_dir().join(server_id)
}

pub(crate) fn lsp_bundle_archive_kind(platform: LspInstallPlatform) -> LspBundleArchive {
    match platform {
        LspInstallPlatform::Windows => LspBundleArchive::Zip,
        LspInstallPlatform::Linux | LspInstallPlatform::MacOS => LspBundleArchive::TarGz,
    }
}

pub(crate) fn archive_entry_path_is_safe(entry: &str) -> bool {
    if entry.is_empty() || entry.chars().any(char::is_control) {
        return false;
    }
    let normalized = entry.replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains(':') {
        return false;
    }
    let components: Vec<&str> = normalized.split('/').collect();
    let mut depth = 0usize;
    for (index, component) in components.iter().enumerate() {
        if component.is_empty() {
            if index + 1 != components.len() {
                return false;
            }
            continue;
        }
        if *component == "." {
            continue;
        }
        if *component == ".." {
            return false;
        }
        depth += 1;
    }
    depth > 0
}

pub(crate) fn tar_archive_listing_is_safe(listing: &str) -> bool {
    listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .all(archive_entry_path_is_safe)
}

pub(crate) fn read_lsp_install_marker(install_dir: &Path) -> Option<LspInstallMarker> {
    let bytes = std::fs::read(install_dir.join(LSP_BUNDLE_MARKER_FILE_NAME)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_lsp_install_marker(install_dir: &Path, marker: &LspInstallMarker) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(marker).context("could not encode the install marker")?;
    std::fs::write(install_dir.join(LSP_BUNDLE_MARKER_FILE_NAME), bytes)
        .with_context(|| format!("could not write the marker in {}", install_dir.display()))
}

pub(crate) fn find_installed_launch_binary(
    install_dir: &Path,
    launch_binary: &str,
) -> Option<PathBuf> {
    let mut shallowest: Option<(usize, PathBuf)> = None;
    visit_launch_candidates(install_dir, 0, launch_binary, &mut |candidate, depth| {
        if shallowest.as_ref().is_none_or(|(best, _)| depth < *best) {
            shallowest = Some((depth, candidate));
        }
    });
    shallowest.map(|(_, path)| path)
}

fn visit_launch_candidates(
    directory: &Path,
    depth: usize,
    launch_binary: &str,
    visit: &mut dyn FnMut(PathBuf, usize),
) {
    if depth > LSP_BUNDLE_LAUNCH_SEARCH_MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_directory = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        if is_directory {
            visit_launch_candidates(&path, depth + 1, launch_binary, visit);
        } else if entry.file_name().to_string_lossy() == launch_binary {
            visit(path, depth);
        }
    }
}

pub(crate) fn installed_lsp_command_override(
    server_id: &str,
    launch_binary: &str,
) -> Option<String> {
    let install_dir = lsp_install_dir(server_id);
    read_lsp_install_marker(&install_dir)
        .filter(|marker| marker.launch == launch_binary)
        .and_then(|_| find_installed_launch_binary(&install_dir, launch_binary))
        .map(|path| path.to_string_lossy().into_owned())
}

pub(crate) async fn run_lsp_bundle_install(
    plan: LspBundleInstallPlan,
    language: String,
    tx: Sender<UiEvent>,
    bytes_downloaded: &AtomicU64,
) -> Result<LspInstallOutcome, LspInstallFailure> {
    download_and_install_lsp_bundle(&plan, &language, &tx, bytes_downloaded)
        .await
        .map(|launch_path| LspInstallOutcome {
            command_override: Some(launch_path),
        })
}

async fn download_and_install_lsp_bundle(
    plan: &LspBundleInstallPlan,
    language: &str,
    tx: &Sender<UiEvent>,
    bytes_downloaded: &AtomicU64,
) -> Result<String, LspInstallFailure> {
    let client = reqwest::Client::builder()
        .user_agent(LSP_BUNDLE_USER_AGENT)
        .connect_timeout(LSP_BUNDLE_CONNECT_TIMEOUT)
        .build()
        .map_err(|error| LspInstallFailure::Download {
            detail: format!("could not create the download client: {error}"),
        })?;
    let expected_checksum = fetch_bundle_checksum(&client, &plan.checksum_url, &plan.asset).await?;
    let archive_path = plan.install_dir.join(&plan.asset);
    let part_path = plan.install_dir.join(LSP_BUNDLE_PART_FILE_NAME);
    stream_bundle_download(
        &client,
        plan,
        language,
        tx,
        bytes_downloaded,
        &part_path,
        &expected_checksum,
    )
    .await?;

    let result = verify_and_extract_bundle(plan, &part_path, &archive_path, &expected_checksum);
    if result.is_err() {
        let _ = std::fs::remove_file(&part_path);
        let _ = std::fs::remove_file(&archive_path);
    }
    result
}

async fn fetch_bundle_checksum(
    client: &reqwest::Client,
    checksum_url: &str,
    asset: &str,
) -> Result<String, LspInstallFailure> {
    if !lsp_bundle_url_is_pinned_to_release(checksum_url) {
        return Err(LspInstallFailure::Download {
            detail: format!("the checksum url for {asset} is not pinned to the Kuroya releases"),
        });
    }
    let response = client
        .get(checksum_url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| lsp_bundle_request_failure(error, asset))?;
    let text = response
        .text()
        .await
        .map_err(|error| LspInstallFailure::Download {
            detail: format!("could not read the checksum for {asset}: {error}"),
        })?;
    checksum_from_sidecar_text(&text).ok_or_else(|| LspInstallFailure::Download {
        detail: format!("could not parse the checksum sidecar for {asset}"),
    })
}

fn lsp_bundle_request_failure(error: reqwest::Error, asset: &str) -> LspInstallFailure {
    if error.status() == Some(reqwest::StatusCode::NOT_FOUND) {
        return LspInstallFailure::Unavailable {
            release_url: LSP_BUNDLE_RELEASE_PAGE_URL.to_owned(),
        };
    }
    LspInstallFailure::Download {
        detail: format!("{asset} could not be downloaded: {error}"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn stream_bundle_download(
    client: &reqwest::Client,
    plan: &LspBundleInstallPlan,
    language: &str,
    tx: &Sender<UiEvent>,
    bytes_downloaded: &AtomicU64,
    part_path: &Path,
    expected_checksum: &str,
) -> Result<(), LspInstallFailure> {
    if !lsp_bundle_url_is_pinned_to_release(&plan.download_url) {
        return Err(LspInstallFailure::Download {
            detail: format!("{} is not pinned to the Kuroya releases", plan.asset),
        });
    }
    let mut response = client
        .get(&plan.download_url)
        .send()
        .await
        .and_then(|response| response.error_for_status())
        .map_err(|error| lsp_bundle_request_failure(error, &plan.asset))?;

    std::fs::create_dir_all(&plan.install_dir).map_err(|error| LspInstallFailure::Download {
        detail: format!("could not create {}: {error}", plan.install_dir.display()),
    })?;
    let mut file = tokio::fs::File::create(part_path).await.map_err(|error| {
        LspInstallFailure::Download {
            detail: format!("could not create {}: {error}", part_path.display()),
        }
    })?;
    let mut hasher = Sha256::new();
    let mut total_bytes = 0u64;
    let mut last_progress = Instant::now() - LSP_BUNDLE_PROGRESS_INTERVAL;
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                file.write_all(&chunk).await.map_err(|error| {
                    LspInstallFailure::Download {
                        detail: format!("could not write {}: {error}", part_path.display()),
                    }
                })?;
                hasher.update(&chunk);
                total_bytes += chunk.len() as u64;
                bytes_downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                if last_progress.elapsed() >= LSP_BUNDLE_PROGRESS_INTERVAL {
                    last_progress = Instant::now();
                    send_ui_event(
                        tx,
                        UiEvent::LspInstallProgress {
                            language: language.to_owned(),
                            display_name: plan.display_name.clone(),
                            stage: LspInstallStage::Downloading {
                                bytes_downloaded: total_bytes,
                            },
                        },
                    );
                }
            }
            Ok(None) => break,
            Err(error) => {
                let _ = std::fs::remove_file(part_path);
                return Err(LspInstallFailure::Download {
                    detail: format!("{} could not be downloaded: {error}", plan.asset),
                });
            }
        }
    }
    file.flush().await.map_err(|error| LspInstallFailure::Download {
        detail: format!("could not flush {}: {error}", part_path.display()),
    })?;
    if total_bytes == 0 {
        let _ = std::fs::remove_file(part_path);
        return Err(LspInstallFailure::Download {
            detail: format!("{} downloaded no data", plan.asset),
        });
    }
    let actual_checksum = format!("{:x}", hasher.finalize());
    if !expected_checksum.eq_ignore_ascii_case(&actual_checksum) {
        let _ = std::fs::remove_file(part_path);
        return Err(LspInstallFailure::Checksum {
            detail: format!("{} failed checksum verification", plan.asset),
        });
    }
    send_ui_event(
        tx,
        UiEvent::LspInstallProgress {
            language: language.to_owned(),
            display_name: plan.display_name.clone(),
            stage: LspInstallStage::Installing,
        },
    );
    Ok(())
}

fn verify_and_extract_bundle(
    plan: &LspBundleInstallPlan,
    part_path: &Path,
    archive_path: &Path,
    expected_checksum: &str,
) -> Result<String, LspInstallFailure> {
    verify_bundle_checksum_against_disk(part_path, expected_checksum)?;
    std::fs::rename(part_path, archive_path).map_err(|error| LspInstallFailure::Download {
        detail: format!("could not finalize {}: {error}", archive_path.display()),
    })?;
    let archive = lsp_bundle_archive_kind(current_platform_for_archive());
    let listing = list_archive_entries(&archive, archive_path)?;
    let unsafe_entry = listing
        .lines()
        .map(str::trim)
        .find(|entry| !entry.is_empty() && !archive_entry_path_is_safe(entry));
    if let Some(entry) = unsafe_entry {
        return Err(LspInstallFailure::Download {
            detail: format!(
                "{} contains an unsafe archive entry {:?}; the download was discarded",
                plan.asset, entry
            ),
        });
    }
    extract_bundle_archive(&archive, archive_path, &plan.install_dir)?;
    let _ = std::fs::remove_file(archive_path);
    let launch_path = find_installed_launch_binary(&plan.install_dir, &plan.launch_binary)
        .ok_or_else(|| LspInstallFailure::Download {
            detail: format!(
                "{} did not contain the {} binary",
                plan.asset, plan.launch_binary
            ),
        })?;
    let launch_relative = launch_path
        .strip_prefix(&plan.install_dir)
        .unwrap_or(&launch_path)
        .to_string_lossy()
        .into_owned();
    write_lsp_install_marker(
        &plan.install_dir,
        &LspInstallMarker {
            version: LSP_BUNDLE_RELEASE_TAG.to_owned(),
            asset: plan.asset.clone(),
            launch: launch_relative,
        },
    )
    .map_err(|error| LspInstallFailure::Download {
        detail: error.to_string(),
    })?;
    Ok(launch_path.to_string_lossy().into_owned())
}

fn verify_bundle_checksum_against_disk(
    part_path: &Path,
    expected_checksum: &str,
) -> Result<(), LspInstallFailure> {
    let bytes = std::fs::read(part_path).map_err(|error| LspInstallFailure::Download {
        detail: format!("could not read {}: {error}", part_path.display()),
    })?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if expected_checksum.eq_ignore_ascii_case(&actual) {
        return Ok(());
    }
    Err(LspInstallFailure::Checksum {
        detail: format!("{} failed checksum verification", part_path.display()),
    })
}

fn current_platform_for_archive() -> LspInstallPlatform {
    if cfg!(windows) {
        LspInstallPlatform::Windows
    } else if cfg!(target_os = "macos") {
        LspInstallPlatform::MacOS
    } else {
        LspInstallPlatform::Linux
    }
}

fn list_archive_entries(
    archive: &LspBundleArchive,
    archive_path: &Path,
) -> Result<String, LspInstallFailure> {
    let archive_text = archive_path.to_string_lossy().into_owned();
    let output = match archive {
        LspBundleArchive::Zip => std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "Add-Type -AssemblyName System.IO.Compression.FileSystem; [System.IO.Compression.ZipFile]::OpenRead('{archive_text}').Entries | ForEach-Object {{ $_.FullName }}"
                ),
            ])
            .output(),
        LspBundleArchive::TarGz => std::process::Command::new("tar")
            .args(["-tzf", &archive_text])
            .output(),
    }
    .map_err(|error| LspInstallFailure::Download {
        detail: format!("could not inspect {}: {error}", archive_path.display()),
    })?;
    if !output.status.success() {
        return Err(LspInstallFailure::Download {
            detail: format!(
                "could not list the entries of {}: {}",
                archive_path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn extract_bundle_archive(
    archive: &LspBundleArchive,
    archive_path: &Path,
    install_dir: &Path,
) -> Result<(), LspInstallFailure> {
    let archive_text = archive_path.to_string_lossy().into_owned();
    let target_text = install_dir.to_string_lossy().into_owned();
    let status = match archive {
        LspBundleArchive::Zip => std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "Expand-Archive -LiteralPath '{archive_text}' -DestinationPath '{target_text}' -Force"
                ),
            ])
            .status(),
        LspBundleArchive::TarGz => std::process::Command::new("tar")
            .args(["-xzf", &archive_text, "-C", &target_text])
            .status(),
    }
    .map_err(|error| LspInstallFailure::Download {
        detail: format!("could not extract {}: {error}", archive_path.display()),
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(LspInstallFailure::Download {
            detail: format!("could not extract {}", archive_path.display()),
        })
    }
}

pub(crate) fn lsp_downloading_status(display_name: &str, bytes_downloaded: u64) -> String {
    format!(
        "Downloading {display_name}… {}",
        format_byte_size(bytes_downloaded)
    )
}

pub(crate) fn lsp_download_failed_status(display_name: &str, detail: &str) -> String {
    format!("Could not download {display_name}: {detail}")
}

pub(crate) fn lsp_checksum_failed_status(display_name: &str) -> String {
    format!("{display_name} failed checksum verification and the download was discarded")
}

pub(crate) fn lsp_bundle_unavailable_status(display_name: &str) -> String {
    format!(
        "{display_name} is not yet available on the release page: {LSP_BUNDLE_RELEASE_PAGE_URL}"
    )
}

impl KuroyaApp {
    pub(crate) fn apply_lsp_install_progress(
        &mut self,
        language: &str,
        display_name: &str,
        stage: LspInstallStage,
    ) {
        let Some(prompt) = self.lsp_enable_prompt.as_ref() else {
            return;
        };
        if prompt.language != language || !prompt.install_in_flight {
            return;
        }
        self.status = match stage {
            LspInstallStage::Downloading { bytes_downloaded } => {
                lsp_downloading_status(display_name, bytes_downloaded)
            }
            LspInstallStage::Installing => {
                crate::lsp_enable_prompt::lsp_installing_status(display_name)
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LspInstallMarker, archive_entry_path_is_safe, find_installed_launch_binary,
        installed_lsp_command_override, lsp_bundle_archive_kind, lsp_bundle_download_url,
        lsp_bundle_install_plan, lsp_bundle_url_is_pinned_to_release, lsp_checksum_failed_status,
        lsp_download_failed_status, lsp_install_dir, lsp_install_root_dir, read_lsp_install_marker,
        tar_archive_listing_is_safe, write_lsp_install_marker, LspBundleArchive,
    };
    use kuroya_core::lsp_registry::{LspInstallPlatform, registry_entry_for};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    #[test]
    fn bundle_download_urls_are_pinned_to_the_owner_release() {
        assert!(lsp_bundle_url_is_pinned_to_release(
            "https://github.com/kaevris/kuroya/releases/download/lsps/lsp-rust-analyzer-windows-x64.zip"
        ));
        assert!(lsp_bundle_url_is_pinned_to_release(
            "https://objects.githubusercontent.com/signed-asset-path"
        ));
        assert!(lsp_bundle_url_is_pinned_to_release(
            "https://GITHUB.com/kaevris/kuroya/releases/download/lsps/lsp-rust-analyzer-windows-x64.zip"
        ));
    }

    #[test]
    fn bundle_download_urls_reject_foreign_hosts_paths_and_schemes() {
        assert!(!lsp_bundle_url_is_pinned_to_release(
            "https://evil.test/kaevris/kuroya/releases/download/lsps/lsp-rust-analyzer-windows-x64.zip"
        ));
        assert!(!lsp_bundle_url_is_pinned_to_release(
            "http://github.com/kaevris/kuroya/releases/download/lsps/lsp-rust-analyzer-windows-x64.zip"
        ));
        assert!(!lsp_bundle_url_is_pinned_to_release(
            "https://github.com/kaevris/kuroya/releases/download/v0.2.0/Kuroya-Setup.exe"
        ));
        assert!(!lsp_bundle_url_is_pinned_to_release(
            "https://github.com/kaevris/other/releases/download/lsps/lsp-rust-analyzer-windows-x64.zip"
        ));
        assert!(!lsp_bundle_url_is_pinned_to_release(
            "https://github.com/kaevris/kuroya/releases/tag/lsps"
        ));
        assert!(!lsp_bundle_url_is_pinned_to_release("not a url"));
        assert!(!lsp_bundle_url_is_pinned_to_release("https://github.com"));
    }

    #[test]
    fn install_plans_resolve_pinned_urls_and_install_paths() {
        let rust = registry_entry_for("rust").expect("rust registry entry");
        let plan =
            lsp_bundle_install_plan(rust, LspInstallPlatform::Windows).expect("rust download plan");
        assert_eq!(plan.server_id, "rust");
        assert_eq!(plan.display_name, "rust-analyzer");
        assert_eq!(plan.asset, "lsp-rust-analyzer-windows-x64.zip");
        assert_eq!(
            plan.download_url,
            lsp_bundle_download_url("lsp-rust-analyzer-windows-x64.zip")
        );
        assert_eq!(plan.checksum_url, format!("{}.sha256", plan.download_url));
        assert!(lsp_bundle_url_is_pinned_to_release(&plan.download_url));
        assert!(lsp_bundle_url_is_pinned_to_release(&plan.checksum_url));
        assert_eq!(plan.launch_binary, "rust-analyzer.exe");
        assert_eq!(plan.install_dir, lsp_install_root_dir().join("rust"));

        let python = registry_entry_for("python").expect("python registry entry");
        assert!(lsp_bundle_install_plan(python, LspInstallPlatform::Windows).is_none());
    }

    #[test]
    fn archive_entry_validation_rejects_zip_slip_paths() {
        assert!(archive_entry_path_is_safe("rust-analyzer.exe"));
        assert!(archive_entry_path_is_safe("bin/lua-language-server"));
        assert!(archive_entry_path_is_safe("clangd_22.1.6/bin/clangd"));
        assert!(archive_entry_path_is_safe("bundle/"));

        assert!(!archive_entry_path_is_safe("../evil.exe"));
        assert!(!archive_entry_path_is_safe("bundle/../../evil.exe"));
        assert!(!archive_entry_path_is_safe("..\\evil.exe"));
        assert!(!archive_entry_path_is_safe("/absolute/path"));
        assert!(!archive_entry_path_is_safe("C:\\evil.exe"));
        assert!(!archive_entry_path_is_safe("\\\\server\\share"));
        assert!(!archive_entry_path_is_safe(""));
        assert!(!archive_entry_path_is_safe("bad\nname"));
        assert!(!archive_entry_path_is_safe("a//b"));
    }

    #[test]
    fn tar_listing_validation_rejects_entries_escaping_the_target_dir() {
        assert!(tar_archive_listing_is_safe(
            "rust-analyzer\nbin/lua-language-server\n"
        ));
        assert!(tar_archive_listing_is_safe(""));
        assert!(!tar_archive_listing_is_safe("rust-analyzer\n../evil\n"));
        assert!(!tar_archive_listing_is_safe("/etc/passwd\n"));
    }

    #[test]
    fn install_markers_round_trip_through_the_install_dir() {
        let install_dir = temp_dir("marker-roundtrip");
        fs::create_dir_all(&install_dir).unwrap();
        assert!(read_lsp_install_marker(&install_dir).is_none());

        let marker = LspInstallMarker {
            version: "lsps".to_owned(),
            asset: "lsp-rust-analyzer-windows-x64.zip".to_owned(),
            launch: "rust-analyzer.exe".to_owned(),
        };
        write_lsp_install_marker(&install_dir, &marker).expect("marker write");

        assert_eq!(read_lsp_install_marker(&install_dir), Some(marker));
        assert!(install_dir.join("installed.json").is_file());

        fs::remove_dir_all(install_dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn launch_binary_resolution_prefers_the_shallowest_match() {
        let install_dir = temp_dir("launch-resolution");
        let nested = install_dir.join("clangd_22.1.6").join("bin");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join(launch_fixture_name()), b"clangd").unwrap();
        let deep = install_dir.join("extra").join("deeper");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join(launch_fixture_name()), b"decoy").unwrap();

        let resolved = find_installed_launch_binary(&install_dir, launch_fixture_name())
            .expect("launch binary should resolve");
        assert_eq!(resolved, nested.join(launch_fixture_name()));

        assert!(
            find_installed_launch_binary(&install_dir, "missing-binary").is_none(),
            "a missing launch binary must not resolve"
        );

        fs::remove_dir_all(install_dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn command_overrides_resolve_only_for_matching_installed_markers() {
        let install_dir = lsp_install_dir("marker-server");
        fs::create_dir_all(&install_dir).unwrap();
        fs::write(install_dir.join(launch_fixture_name()), b"server").unwrap();
        write_lsp_install_marker(
            &install_dir,
            &LspInstallMarker {
                version: "lsps".to_owned(),
                asset: "lsp-marker-server-windows-x64.zip".to_owned(),
                launch: launch_fixture_name().to_owned(),
            },
        )
        .unwrap();

        let command = installed_lsp_command_override("marker-server", launch_fixture_name())
            .expect("installed marker should resolve a command override");
        assert!(
            Path::new(&command).is_file(),
            "the override must point at the installed binary: {command}"
        );

        assert!(
            installed_lsp_command_override("marker-server", "other-binary").is_none(),
            "a marker for a different launch binary must not resolve"
        );
        assert!(
            installed_lsp_command_override("missing-server", launch_fixture_name()).is_none(),
            "a server without an install marker must not resolve"
        );

        fs::remove_dir_all(install_dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn download_statuses_report_progress_checksums_and_unavailable_assets() {
        assert_eq!(
            lsp_downloading_status("rust-analyzer", 4_404_019),
            "Downloading rust-analyzer… 4.2 MB"
        );
        assert_eq!(
            lsp_checksum_failed_status("rust-analyzer"),
            "rust-analyzer failed checksum verification and the download was discarded"
        );
        let unavailable = super::lsp_bundle_unavailable_status("clangd");
        assert!(
            unavailable.contains("clangd")
                && unavailable.contains(super::LSP_BUNDLE_RELEASE_PAGE_URL)
        );
        assert!(super::lsp_download_failed_status("gopls", "timeout").contains("gopls"));
    }

    #[test]
    fn archive_kinds_split_by_platform_target() {
        assert!(matches!(
            lsp_bundle_archive_kind(LspInstallPlatform::Windows),
            LspBundleArchive::Zip
        ));
        assert!(matches!(
            lsp_bundle_archive_kind(LspInstallPlatform::Linux),
            LspBundleArchive::TarGz
        ));
        assert!(matches!(
            lsp_bundle_archive_kind(LspInstallPlatform::MacOS),
            LspBundleArchive::TarGz
        ));
    }

    fn launch_fixture_name() -> &'static str {
        if cfg!(windows) {
            "sample-server.exe"
        } else {
            "sample-server"
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-lsp-install-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}

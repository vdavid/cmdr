//! Custom macOS updater that preserves TCC/Full Disk Access permissions across updates.
//!
//! Instead of replacing the entire `.app` bundle (which changes its inode and causes macOS
//! to lose track of FDA grants), this updater syncs files *into* the existing bundle,
//! preserving the directory inode and `com.apple.macl` xattr.
//!
//! Three Tauri commands:
//! - `check_for_update` — fetches `latest.json`, compares versions
//! - `download_update` — downloads tarball, verifies minisign signature
//! - `install_update` — extracts and syncs into the running `.app` bundle

mod installer;
mod manifest;
mod signature;

use manifest::UpdateInfo;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

/// Shared state between `download_update` and `install_update`.
/// Holds the path to the downloaded (and verified) tarball.
pub struct UpdateState {
    downloaded_tarball: Mutex<Option<PathBuf>>,
}

impl UpdateState {
    pub fn new() -> Self {
        Self {
            downloaded_tarball: Mutex::new(None),
        }
    }
}

/// Fetches `latest.json` (via the update check proxy for analytics) and returns update info
/// if a newer version is available.
///
/// Returns `None` when:
/// - The `CI` env var is set (CI guard — avoids network calls in tests)
/// - The remote version is not newer than the current version
/// - The manifest doesn't contain an entry for this platform
#[tauri::command]
pub async fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    if std::env::var("CI").is_ok() {
        log::debug!("Skipping update check in CI");
        return Ok(None);
    }

    let current_version = env!("CARGO_PKG_VERSION");
    log::info!("Checking for updates (current version: {current_version})");

    let arch = manifest::platform_key().strip_prefix("darwin-").unwrap_or("unknown");
    let url = format!("https://api.getcmdr.com/update-check/{current_version}?arch={arch}");

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Couldn't fetch update manifest: {e}"))?;

    let manifest: manifest::UpdateManifest = response
        .json()
        .await
        .map_err(|e| format!("Couldn't parse update manifest: {e}"))?;

    Ok(manifest::check_manifest(&manifest, current_version))
}

/// Downloads the update tarball and verifies its minisign signature.
///
/// On success, stores the tarball path in `UpdateState` for `install_update` to consume.
#[tauri::command]
pub async fn download_update(url: String, signature: String, state: State<'_, UpdateState>) -> Result<(), String> {
    log::info!("Downloading update from {url}");

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Couldn't download update: {e}"))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Couldn't read update response: {e}"))?;

    log::info!("Downloaded {} bytes, verifying signature", bytes.len());
    signature::verify(&bytes, &signature)?;
    log::info!("Signature verified");

    let temp_dir = std::env::temp_dir().join("cmdr-update");
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("Couldn't create temp dir: {e}"))?;

    let tarball_path = temp_dir.join("Cmdr.app.tar.gz");
    std::fs::write(&tarball_path, &bytes).map_err(|e| format!("Couldn't write tarball: {e}"))?;

    let mut guard = state
        .downloaded_tarball
        .lock()
        .map_err(|e| format!("Couldn't lock update state: {e}"))?;
    *guard = Some(tarball_path);

    Ok(())
}

/// Installs a previously downloaded update by syncing files into the running `.app` bundle.
///
/// Reads (and clears) the tarball path stored by `download_update`.
#[tauri::command]
pub async fn install_update(state: State<'_, UpdateState>) -> Result<(), String> {
    let tarball_path = {
        let mut guard = state
            .downloaded_tarball
            .lock()
            .map_err(|e| format!("Couldn't lock update state: {e}"))?;
        guard.take().ok_or_else(|| "No update downloaded".to_string())?
    };

    log::info!("Installing update from {}", tarball_path.display());

    // Run the install on a blocking thread since it does filesystem I/O

    tokio::task::spawn_blocking(move || installer::install(&tarball_path))
        .await
        .map_err(|e| format!("Install task panicked: {e}"))?
}

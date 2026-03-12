//! Parses `latest.json` from the update server and determines whether an update is available.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The full `latest.json` manifest served by the update server.
#[derive(Debug, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub platforms: HashMap<String, PlatformEntry>,
}

/// Per-platform entry in the manifest: download URL and minisign signature.
#[derive(Debug, Deserialize)]
pub struct PlatformEntry {
    pub url: String,
    pub signature: String,
}

/// Update metadata returned to the frontend when a newer version is available.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub url: String,
    pub signature: String,
}

/// Returns the platform key for this binary's target architecture.
/// Matches Tauri's built-in updater key format: `darwin-aarch64` or `darwin-x86_64`.
pub fn platform_key() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "darwin-aarch64"
    } else {
        "darwin-x86_64"
    }
}

/// Checks whether the remote version is newer than the current app version.
/// Returns `Some(UpdateInfo)` if an update is available, `None` otherwise.
pub fn check_manifest(manifest: &UpdateManifest, current_version: &str) -> Option<UpdateInfo> {
    let current = semver::Version::parse(current_version).ok()?;
    let remote = semver::Version::parse(&manifest.version).ok()?;

    if remote <= current {
        log::debug!("No update available (current={current}, remote={remote})");
        return None;
    }

    let key = platform_key();
    let entry = manifest.platforms.get(key)?;

    log::info!("Update available: {current} -> {remote} (platform={key})");
    Some(UpdateInfo {
        version: manifest.version.clone(),
        url: entry.url.clone(),
        signature: entry.signature.clone(),
    })
}

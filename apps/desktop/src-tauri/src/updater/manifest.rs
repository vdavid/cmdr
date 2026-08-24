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
#[derive(Debug, Clone, Serialize, specta::Type)]
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
///
/// Three of the four `None` paths mean something is wrong with the manifest rather than "you're up
/// to date", and the caller can't tell them apart: it gets a bare `None` either way, which the
/// frontend renders as "Cmdr is up to date". So each one warns. An install that quietly never
/// updates because the manifest lost its platform entry is otherwise invisible from both ends,
/// and only the "up to date" path is routine enough to stay at debug.
pub fn check_manifest(manifest: &UpdateManifest, current_version: &str) -> Option<UpdateInfo> {
    let current = match semver::Version::parse(current_version) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Can't compare versions: this build's own version {current_version} isn't semver: {e}");
            return None;
        }
    };
    let remote = match semver::Version::parse(&manifest.version) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "Can't compare versions: manifest version {} isn't semver: {e}",
                manifest.version
            );
            return None;
        }
    };

    if remote <= current {
        log::debug!("No update available (current={current}, remote={remote})");
        return None;
    }

    let key = platform_key();
    let Some(entry) = manifest.platforms.get(key) else {
        let mut offered: Vec<&str> = manifest.platforms.keys().map(String::as_str).collect();
        offered.sort_unstable();
        log::warn!(
            "Update {remote} is out but has no {key} build, so this install stays on {current} (manifest offers: {})",
            offered.join(", ")
        );
        return None;
    };

    log::info!("Update available: {current} -> {remote} (platform={key})");
    Some(UpdateInfo {
        version: manifest.version.clone(),
        url: entry.url.clone(),
        signature: entry.signature.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str, platforms: &[&str]) -> UpdateManifest {
        UpdateManifest {
            version: version.to_string(),
            platforms: platforms
                .iter()
                .map(|key| {
                    (
                        (*key).to_string(),
                        PlatformEntry {
                            url: format!("https://example.invalid/{key}.tar.gz"),
                            signature: format!("sig-{key}"),
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn newer_version_with_this_platform_is_an_update() {
        let m = manifest("0.41.0", &["darwin-aarch64", "darwin-x86_64", "darwin-universal"]);
        let info = check_manifest(&m, "0.40.0").expect("0.41.0 is newer than 0.40.0");
        assert_eq!(info.version, "0.41.0");
        assert_eq!(info.url, format!("https://example.invalid/{}.tar.gz", platform_key()));
        assert_eq!(info.signature, format!("sig-{}", platform_key()));
    }

    #[test]
    fn same_version_is_not_an_update() {
        let m = manifest("0.40.0", &["darwin-aarch64", "darwin-x86_64"]);
        assert!(check_manifest(&m, "0.40.0").is_none());
    }

    #[test]
    fn older_remote_version_is_not_an_update() {
        let m = manifest("0.39.0", &["darwin-aarch64", "darwin-x86_64"]);
        assert!(check_manifest(&m, "0.40.0").is_none());
    }

    /// A manifest that lost this build's platform entry reads to the caller exactly like "you're
    /// up to date", so the install would sit on an old version indefinitely with nothing said.
    /// The `None` is the contract; the warn beside it is what makes the case diagnosable.
    #[test]
    fn a_newer_version_with_no_entry_for_this_platform_is_not_an_update() {
        let m = manifest("0.41.0", &["darwin-not-a-real-arch"]);
        assert!(check_manifest(&m, "0.40.0").is_none());
    }

    #[test]
    fn a_non_semver_manifest_version_is_not_an_update() {
        let m = manifest("latest", &["darwin-aarch64", "darwin-x86_64"]);
        assert!(check_manifest(&m, "0.40.0").is_none());
    }

    #[test]
    fn a_prerelease_is_older_than_its_release() {
        let m = manifest("0.41.0-rc1", &["darwin-aarch64", "darwin-x86_64"]);
        assert!(
            check_manifest(&m, "0.41.0").is_none(),
            "semver orders a prerelease below its release, so 0.41.0 must not downgrade"
        );
    }

    #[test]
    fn platform_key_is_one_of_the_keys_the_manifest_ships() {
        assert!(
            ["darwin-aarch64", "darwin-x86_64"].contains(&platform_key()),
            "platform_key() must match a key `latest.json` actually carries"
        );
    }
}

//! Which cloud provider owns a path, as a type rather than a string sniff.
//!
//! One source of truth for "is this path in a cloud folder, and whose?", shared
//! by the volume switcher (which needs a display name and a stable volume ID)
//! and the file context menu (which needs to know what each provider can
//! actually do).
//!
//! ## Why a provider is a type, not a string
//!
//! The eviction pair ("Make available offline" / "Remove download") works for
//! iCloud Drive and nothing else, and that limit is easy to lose track of at a
//! call site. [`CloudProvider::supports_eviction`] states it once, so a new
//! provider arriving here can't silently inherit actions it can't perform.
//!
//! ## Gotcha: this finds Drive's stream mode only
//!
//! Google Drive for desktop has two modes. In **stream** mode it lives under
//! `~/Library/CloudStorage/GoogleDrive-<account>/`, which is what this module
//! matches. In **mirror** mode the files are ordinary local files in a folder
//! the user picks (`~/My Drive` by default), indistinguishable from any other
//! directory by path alone. So a `None` from [`locate`] does NOT prove a path
//! is outside Google Drive. The Drive menu items deliberately don't rely on
//! this: they key off a resolved item ID instead (`google_drive.rs`).

use std::path::{Path, PathBuf};

/// Subdirectory under `$HOME` holding iCloud Drive's documents.
pub const ICLOUD_DRIVE_SUBPATH: &str = "Library/Mobile Documents/com~apple~CloudDocs";

/// Subdirectory under `$HOME` where macOS mounts third-party File Provider
/// cloud folders (Dropbox, Google Drive, OneDrive, Box, …).
pub const CLOUD_STORAGE_SUBPATH: &str = "Library/CloudStorage";

/// A cloud storage provider Cmdr recognizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudProvider {
    ICloudDrive,
    GoogleDrive,
    Dropbox,
    OneDrive,
    OneDriveForBusiness,
    Box,
    PCloud,
    /// A provider we don't know by name, carrying the label parsed from its
    /// `CloudStorage` directory. Keeps an unrecognized provider a first-class
    /// cloud drive in the switcher instead of dropping it.
    Other(String),
}

impl CloudProvider {
    /// The name shown in the volume switcher.
    pub fn display_name(&self) -> &str {
        match self {
            Self::ICloudDrive => "iCloud Drive",
            Self::GoogleDrive => "Google Drive",
            Self::Dropbox => "Dropbox",
            Self::OneDrive => "OneDrive",
            Self::OneDriveForBusiness => "OneDrive for Business",
            Self::Box => "Box",
            Self::PCloud => "pCloud",
            Self::Other(name) => name,
        }
    }

    /// The stable volume ID the switcher matches its checkmark on. Persisted in
    /// user state, so ❌ don't renumber these.
    pub fn volume_id(&self) -> String {
        match self {
            Self::ICloudDrive => "cloud-icloud".to_string(),
            Self::GoogleDrive => "cloud-google-drive".to_string(),
            Self::Dropbox => "cloud-dropbox".to_string(),
            Self::OneDrive => "cloud-onedrive".to_string(),
            Self::OneDriveForBusiness => "cloud-onedrive-business".to_string(),
            Self::Box => "cloud-box".to_string(),
            Self::PCloud => "cloud-pcloud".to_string(),
            Self::Other(name) => format!("cloud-{}", name.to_lowercase()),
        }
    }

    /// Whether "Make available offline" / "Remove download" can work here.
    ///
    /// True for iCloud Drive alone. Those two route through
    /// `FileManager.evictUbiquitousItem` / `startDownloadingUbiquitousItem`,
    /// which accept URLs inside an iCloud container and reject everything else.
    /// A third-party provider's pin/unpin is a File Provider custom action
    /// reserved for the app that BUNDLES the extension, so Cmdr gets
    /// `NSFileProviderErrorProviderNotFound` if it tries. ❌ Don't widen this;
    /// `cloud_actions.rs` has the full story.
    pub fn supports_eviction(&self) -> bool {
        matches!(self, Self::ICloudDrive)
    }

    /// Identifies a provider from its `~/Library/CloudStorage` directory name,
    /// which macOS shapes as `<Provider>` or `<Provider>-<account>`.
    pub fn from_cloud_storage_dir(dir_name: &str) -> Option<Self> {
        if dir_name.is_empty() {
            return None;
        }
        if dir_name.starts_with("Dropbox") {
            return Some(Self::Dropbox);
        }
        if dir_name.starts_with("GoogleDrive") {
            return Some(Self::GoogleDrive);
        }
        if dir_name.starts_with("OneDrive") {
            // OneDrive-Personal, OneDrive-Business, OneDrive-<tenant>, …
            return Some(if dir_name.contains("Business") {
                Self::OneDriveForBusiness
            } else {
                Self::OneDrive
            });
        }
        if dir_name.starts_with("Box") {
            return Some(Self::Box);
        }
        if dir_name.starts_with("pCloud") {
            return Some(Self::PCloud);
        }
        // Unknown provider: the segment before the account suffix is its name.
        let label = dir_name.split('-').next().unwrap_or(dir_name);
        Some(Self::Other(label.to_string()))
    }
}

/// A path's cloud provider plus the root folder of that provider's drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudLocation {
    pub provider: CloudProvider,
    /// The drive's root, NOT the path that was looked up.
    pub root: PathBuf,
}

/// Resolves `path` to the cloud drive containing it, if any.
///
/// Pure (compares paths, touches no disk) so it's cheap enough to call on every
/// navigation and unit-testable without a real cloud folder. `home` is passed in
/// rather than read from the environment for the same reason.
///
/// Matching is component-wise, so `com~apple~CloudDocsBackup` is not iCloud.
pub fn locate(home: &Path, path: &Path) -> Option<CloudLocation> {
    let icloud_root = home.join(ICLOUD_DRIVE_SUBPATH);
    if path.starts_with(&icloud_root) {
        return Some(CloudLocation {
            provider: CloudProvider::ICloudDrive,
            root: icloud_root,
        });
    }

    // Third-party providers: ~/Library/CloudStorage/<provider-dir>/… The first
    // component under CloudStorage names the drive; everything deeper belongs to
    // that same drive. The container itself is not a cloud drive.
    let cloud_storage_root = home.join(CLOUD_STORAGE_SUBPATH);
    let relative = path.strip_prefix(&cloud_storage_root).ok()?;
    let provider_dir = relative.components().next()?.as_os_str().to_str()?;
    let provider = CloudProvider::from_cloud_storage_dir(provider_dir)?;
    Some(CloudLocation {
        provider,
        root: cloud_storage_root.join(provider_dir),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("/Users/test")
    }

    fn provider_of(path: &str) -> Option<CloudProvider> {
        locate(&home(), Path::new(path)).map(|found| found.provider)
    }

    #[test]
    fn icloud_matches_its_container_and_everything_under_it() {
        let icloud = "/Users/test/Library/Mobile Documents/com~apple~CloudDocs";
        assert_eq!(provider_of(icloud), Some(CloudProvider::ICloudDrive));
        assert_eq!(
            provider_of(&format!("{icloud}/Projects/notes.md")),
            Some(CloudProvider::ICloudDrive)
        );
    }

    #[test]
    fn third_party_providers_resolve_from_their_cloudstorage_directory() {
        assert_eq!(
            provider_of("/Users/test/Library/CloudStorage/GoogleDrive-me@gmail.com/My Drive/x"),
            Some(CloudProvider::GoogleDrive)
        );
        assert_eq!(
            provider_of("/Users/test/Library/CloudStorage/Dropbox/Work/report.pdf"),
            Some(CloudProvider::Dropbox)
        );
        assert_eq!(
            provider_of("/Users/test/Library/CloudStorage/OneDrive-Personal/x"),
            Some(CloudProvider::OneDrive)
        );
        assert_eq!(
            provider_of("/Users/test/Library/CloudStorage/OneDrive-Business/x"),
            Some(CloudProvider::OneDriveForBusiness)
        );
    }

    #[test]
    fn an_unknown_provider_keeps_its_directory_label() {
        assert_eq!(
            provider_of("/Users/test/Library/CloudStorage/Fastmail-me@fastmail.com/x"),
            Some(CloudProvider::Other("Fastmail".to_string()))
        );
    }

    #[test]
    fn non_cloud_paths_and_the_bare_container_resolve_to_nothing() {
        assert_eq!(provider_of("/"), None);
        assert_eq!(provider_of("/Users/test/Documents"), None);
        assert_eq!(provider_of("/Volumes/External/photos"), None);
        assert_eq!(provider_of("/Users/test/Library/CloudStorage"), None);
        // A sibling sharing a name prefix must not match: comparison is component-wise.
        assert_eq!(
            provider_of("/Users/test/Library/Mobile Documents/com~apple~CloudDocsBackup"),
            None
        );
    }

    /// Drive's mirror mode puts real files outside `CloudStorage`, so a path
    /// lookup can't see them. Pinned so nobody "fixes" the Drive menu items to
    /// gate on this.
    #[test]
    fn mirror_mode_google_drive_is_invisible_to_a_path_lookup() {
        assert_eq!(provider_of("/Users/test/My Drive/report.pdf"), None);
    }

    #[test]
    fn the_root_is_the_drive_root_not_the_looked_up_path() {
        let found = locate(
            &home(),
            Path::new("/Users/test/Library/CloudStorage/Dropbox/Work/2026/report.pdf"),
        )
        .expect("Dropbox match");
        assert_eq!(found.root, PathBuf::from("/Users/test/Library/CloudStorage/Dropbox"));
        assert_eq!(found.provider.display_name(), "Dropbox");
        assert_eq!(found.provider.volume_id(), "cloud-dropbox");
    }

    /// The invariant the whole enum exists to hold: eviction is iCloud's alone.
    #[test]
    fn only_icloud_supports_eviction() {
        assert!(CloudProvider::ICloudDrive.supports_eviction());
        for provider in [
            CloudProvider::GoogleDrive,
            CloudProvider::Dropbox,
            CloudProvider::OneDrive,
            CloudProvider::OneDriveForBusiness,
            CloudProvider::Box,
            CloudProvider::PCloud,
            CloudProvider::Other("Whatever".to_string()),
        ] {
            assert!(
                !provider.supports_eviction(),
                "{provider:?} must not offer the eviction pair"
            );
        }
    }
}

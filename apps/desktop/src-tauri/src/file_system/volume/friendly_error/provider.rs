//! Provider detection and enrichment: detects which cloud/mount provider manages a path
//! and SETS the typed `provider` field on the classification.
//!
//! Detection stays in Rust (it needs path patterns + `statfs`); the words live on
//! the frontend (`src/lib/errors/provider-error-messages.ts`), which overlays the
//! provider-specific suggestion when `provider` is present.

use std::path::Path;

use super::ListingError;

// ============================================================================
// Provider detection
// ============================================================================

/// Known cloud/mount provider. The variant identity crosses IPC (serialized
/// camelCase); the FE maps it to display names, app names, and the
/// (provider, category) suggestion table. Variant names match the TS `Provider`
/// union member-for-member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    Dropbox,
    GoogleDrive,
    OneDrive,
    Box,
    PCloud,
    Nextcloud,
    SynologyDrive,
    Tresorit,
    ProtonDrive,
    Sync,
    Egnyte,
    MacDroid,
    ICloud,
    PCloudFuse,
    MacFuse,
    VeraCrypt,
    CmVolumes,
    /// Any unrecognized dir under `~/Library/CloudStorage/`.
    GenericCloudStorage,
}

/// Detects the cloud/mount provider from the path and SETS the typed `provider`
/// field. Leaves everything else unchanged. The FE overlays the
/// provider-specific suggestion when `provider` is present.
pub fn enrich_with_provider(error: &mut ListingError, path: &Path) {
    if let Some(provider) = detect_provider(path) {
        error.provider = Some(provider);
    }
}

/// Reads the filesystem type for a path via `libc::statfs`.
///
/// Returns `None` if the `statfs` call fails (for example, the path doesn't exist).
#[cfg(target_os = "macos")]
fn get_fs_type_for_path(path: &Path) -> Option<String> {
    use std::ffi::CString;

    let c_path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
    let mut stat: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();

    // SAFETY: `c_path` is a valid NUL-terminated C string from `path`, and `stat` is an
    // uninitialized but correctly-typed `libc::statfs` out-buffer the kernel fills on success.
    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

    // SAFETY: `statfs` returned 0, so the kernel fully initialized `stat`.
    let stat = unsafe { stat.assume_init() };
    let name_bytes: Vec<u8> = stat
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8(name_bytes).ok()
}

/// Detects the provider from the path.
fn detect_provider(path: &Path) -> Option<Provider> {
    let path_str = path.to_string_lossy();

    // Expand ~ to the home directory for matching.
    let home = dirs::home_dir().unwrap_or_default();
    let cloud_storage_prefix = home.join("Library/CloudStorage");
    let mobile_docs_prefix = home.join("Library/Mobile Documents");
    let cm_volumes_prefix = home.join(".CMVolumes");

    let cloud_storage_str = cloud_storage_prefix.to_string_lossy();
    let mobile_docs_str = mobile_docs_prefix.to_string_lossy();
    let cm_volumes_str = cm_volumes_prefix.to_string_lossy();

    // 1. CloudStorage prefix providers
    if path_str.starts_with(cloud_storage_str.as_ref()) {
        // Get the directory name right after CloudStorage/
        let remainder = &path_str[cloud_storage_str.len()..];
        let remainder = remainder.strip_prefix('/').unwrap_or(remainder);
        let dir_name = remainder.split('/').next().unwrap_or("");

        return Some(if dir_name.starts_with("Dropbox") {
            Provider::Dropbox
        } else if dir_name.starts_with("GoogleDrive") {
            Provider::GoogleDrive
        } else if dir_name.starts_with("OneDrive") {
            Provider::OneDrive
        } else if dir_name.starts_with("Box") {
            Provider::Box
        } else if dir_name.starts_with("pCloud") {
            Provider::PCloud
        } else if dir_name.starts_with("Nextcloud") {
            Provider::Nextcloud
        } else if dir_name.starts_with("SynologyDrive") {
            Provider::SynologyDrive
        } else if dir_name.starts_with("Tresorit") {
            Provider::Tresorit
        } else if dir_name.starts_with("ProtonDrive") {
            Provider::ProtonDrive
        } else if dir_name.starts_with("Sync") {
            Provider::Sync
        } else if dir_name.starts_with("Egnyte") {
            Provider::Egnyte
        } else if dir_name.starts_with("MacDroid") {
            Provider::MacDroid
        } else {
            Provider::GenericCloudStorage
        });
    }

    // 2. iCloud: ~/Library/Mobile Documents/
    if path_str.starts_with(mobile_docs_str.as_ref()) {
        return Some(Provider::ICloud);
    }

    // 3. Specific paths
    if path_str.starts_with("/Volumes/pCloudDrive") {
        return Some(Provider::PCloudFuse);
    }
    if path_str.starts_with("/Volumes/veracrypt") {
        return Some(Provider::VeraCrypt);
    }
    if path_str.starts_with(cm_volumes_str.as_ref()) {
        return Some(Provider::CmVolumes);
    }

    // 4. statfs-based FUSE detection for mounts not covered by known path patterns.
    #[cfg(target_os = "macos")]
    if let Some(fs_type) = get_fs_type_for_path(path) {
        match fs_type.as_str() {
            "macfuse" | "osxfuse" => return Some(Provider::MacFuse),
            "pcloudfs" => return Some(Provider::PCloudFuse),
            _ => {}
        }
    }

    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::file_system::volume::VolumeError;
    use crate::file_system::volume::friendly_error::listing_error_from_volume_error;

    // ── Provider detection tests ────────────────────────────────────────

    fn home_path(suffix: &str) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/Users/test"))
            .join(suffix)
    }

    #[test]
    fn detect_cloud_storage_providers() {
        let cases = [
            ("Library/CloudStorage/Dropbox/docs/file.txt", Provider::Dropbox),
            (
                "Library/CloudStorage/GoogleDrive-me@gmail.com/My Drive/file.txt",
                Provider::GoogleDrive,
            ),
            ("Library/CloudStorage/OneDrive-Personal/file.txt", Provider::OneDrive),
            ("Library/CloudStorage/Box-Enterprise/file.txt", Provider::Box),
            ("Library/CloudStorage/pCloud/file.txt", Provider::PCloud),
            ("Library/CloudStorage/Nextcloud-myserver/file.txt", Provider::Nextcloud),
            (
                "Library/CloudStorage/SynologyDrive-NAS/file.txt",
                Provider::SynologyDrive,
            ),
            ("Library/CloudStorage/Tresorit/file.txt", Provider::Tresorit),
            ("Library/CloudStorage/ProtonDrive-me/file.txt", Provider::ProtonDrive),
            ("Library/CloudStorage/Sync-myaccount/file.txt", Provider::Sync),
            ("Library/CloudStorage/Egnyte-Corp/file.txt", Provider::Egnyte),
            ("Library/CloudStorage/MacDroid-Phone/DCIM/photo.jpg", Provider::MacDroid),
            (
                "Library/CloudStorage/ExpanDrive-S3/file.txt",
                Provider::GenericCloudStorage,
            ),
        ];

        for (suffix, expected) in cases {
            let path = home_path(suffix);
            let detected = detect_provider(&path);
            assert_eq!(
                detected,
                Some(expected),
                "Path suffix '{}' should detect {:?}, got {:?}",
                suffix,
                expected,
                detected
            );
        }
    }

    #[test]
    fn detect_icloud() {
        let path = home_path("Library/Mobile Documents/com~apple~CloudDocs/file.txt");
        assert_eq!(detect_provider(&path), Some(Provider::ICloud));
    }

    #[test]
    fn detect_pcloud_fuse() {
        let path = Path::new("/Volumes/pCloudDrive/folder/file.txt");
        assert_eq!(detect_provider(path), Some(Provider::PCloudFuse));
    }

    #[test]
    fn detect_veracrypt() {
        let path = Path::new("/Volumes/veracrypt1/secret/file.txt");
        assert_eq!(detect_provider(path), Some(Provider::VeraCrypt));
    }

    #[test]
    fn detect_cm_volumes() {
        let path = home_path(".CMVolumes/MyMount/file.txt");
        assert_eq!(detect_provider(&path), Some(Provider::CmVolumes));
    }

    #[test]
    fn detect_generic_cloud_storage_fallback() {
        let path = home_path("Library/CloudStorage/MountainDuck-S3/file.txt");
        assert_eq!(detect_provider(&path), Some(Provider::GenericCloudStorage));
    }

    #[test]
    fn no_provider_for_regular_path() {
        let path = Path::new("/Users/test/Documents/file.txt");
        assert_eq!(detect_provider(path), None);
    }

    // ── Enrichment behavior tests ───────────────────────────────────────

    #[test]
    fn enrichment_sets_provider_but_not_category_or_retry() {
        let err = VolumeError::ConnectionTimeout("test".into());
        let path = home_path("Library/CloudStorage/Dropbox/some/folder");

        let mut listing = listing_error_from_volume_error(&err, &path);
        let original_category = listing.category;
        let original_retry = listing.retry_hint;
        assert_eq!(listing.provider, None, "provider starts unset");

        enrich_with_provider(&mut listing, &path);

        assert_eq!(listing.category, original_category, "category should not change");
        assert_eq!(listing.retry_hint, original_retry, "retry_hint should not change");
        assert_eq!(
            listing.provider,
            Some(Provider::Dropbox),
            "provider should be set by enrichment"
        );
    }

    #[test]
    fn enrichment_is_noop_for_unknown_path() {
        let err = VolumeError::ConnectionTimeout("test".into());
        let path = Path::new("/Users/test/Documents/folder");

        let mut listing = listing_error_from_volume_error(&err, path);
        enrich_with_provider(&mut listing, path);

        assert_eq!(listing.provider, None, "provider should stay unset for unknown paths");
    }

    #[test]
    fn enrichment_sets_specific_path_providers() {
        for (path_str, expected) in [
            ("/Volumes/pCloudDrive/f", Provider::PCloudFuse),
            ("/Volumes/veracrypt1/f", Provider::VeraCrypt),
        ] {
            let path = Path::new(path_str);
            let err = VolumeError::ConnectionTimeout("test".into());
            let mut listing = listing_error_from_volume_error(&err, path);
            enrich_with_provider(&mut listing, path);
            assert_eq!(
                listing.provider,
                Some(expected),
                "path {path_str} should set {expected:?}"
            );
        }

        let cm_path = home_path(".CMVolumes/MyMount/f");
        let err = VolumeError::ConnectionTimeout("test".into());
        let mut listing = listing_error_from_volume_error(&err, &cm_path);
        enrich_with_provider(&mut listing, &cm_path);
        assert_eq!(listing.provider, Some(Provider::CmVolumes));
    }
}

//! Provider detection and enrichment: detects which cloud/mount provider manages a path
//! and tailors error suggestions accordingly.
//!
//! Used by `friendly_error.rs` to overwrite the generic suggestion with provider-specific advice.

use std::path::Path;

use super::friendly_error::{ErrorCategory, FriendlyError};

// ============================================================================
// Provider enrichment
// ============================================================================

/// Detects the cloud/mount provider from the path and overwrites `suggestion`
/// (and sometimes `explanation`) with provider-specific advice.
///
/// Leaves `title`, `category`, and `retry_hint` unchanged.
pub fn enrich_with_provider(error: &mut FriendlyError, path: &Path) {
    let Some(provider) = detect_provider(path) else {
        return;
    };

    // Build provider-specific suggestion based on the error category and provider.
    let suggestion = provider_suggestion(&provider, error);
    error.suggestion = suggestion;
}

// ── Provider detection ──────────────────────────────────────────────────

/// Known cloud/mount provider.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Provider {
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

impl Provider {
    fn display_name(&self) -> &'static str {
        match self {
            Self::Dropbox => "Dropbox",
            Self::GoogleDrive => "Google Drive",
            Self::OneDrive => "OneDrive",
            Self::Box => "Box",
            Self::PCloud => "pCloud",
            Self::Nextcloud => "Nextcloud",
            Self::SynologyDrive => "Synology Drive",
            Self::Tresorit => "Tresorit",
            Self::ProtonDrive => "Proton Drive",
            Self::Sync => "Sync.com",
            Self::Egnyte => "Egnyte",
            Self::MacDroid => "MacDroid",
            Self::ICloud => "iCloud Drive",
            Self::PCloudFuse => "pCloud",
            Self::MacFuse => "macFUSE",
            Self::VeraCrypt => "VeraCrypt",
            Self::CmVolumes => "Cloud mount",
            Self::GenericCloudStorage => "your cloud provider",
        }
    }

    fn app_name(&self) -> Option<&'static str> {
        match self {
            Self::Dropbox => Some("Dropbox"),
            Self::GoogleDrive => Some("Google Drive"),
            Self::OneDrive => Some("OneDrive"),
            Self::Box => Some("Box Drive"),
            Self::PCloud | Self::PCloudFuse => Some("pCloud Drive"),
            Self::MacFuse => None, // macFUSE is a framework, not a single app
            Self::Nextcloud => Some("Nextcloud"),
            Self::SynologyDrive => Some("Synology Drive"),
            Self::Tresorit => Some("Tresorit"),
            Self::ProtonDrive => Some("Proton Drive"),
            Self::Sync => Some("Sync.com"),
            Self::Egnyte => Some("Egnyte Connect"),
            Self::MacDroid => Some("MacDroid"),
            Self::ICloud => None, // Built into macOS
            Self::VeraCrypt => Some("VeraCrypt"),
            Self::CmVolumes => None,
            Self::GenericCloudStorage => None,
        }
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

    let result = unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if result != 0 {
        return None;
    }

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

/// Builds a provider-specific suggestion string.
fn provider_suggestion(provider: &Provider, error: &FriendlyError) -> String {
    let name = provider.display_name();

    match provider {
        Provider::MacDroid => match error.category {
            ErrorCategory::Transient => "This folder is managed by **MacDroid**. Here's what to try:\n\
                    - Open MacDroid and check that your phone is connected\n\
                    - Make sure your phone is unlocked and set to file transfer mode\n\
                    - Unplug and replug the USB cable, then navigate here again"
                .to_string(),
            ErrorCategory::NeedsAction => "This folder is managed by **MacDroid**. Here's what to try:\n\
                    - Open MacDroid and check that your phone is connected\n\
                    - Make sure your phone is unlocked with the screen on\n\
                    - Check that USB file transfer mode is enabled on your phone"
                .to_string(),
            ErrorCategory::Serious => "This folder is managed by **MacDroid**. Here's what to try:\n\
                    - Unplug and replug the USB cable\n\
                    - Restart MacDroid\n\
                    - Try a different USB port or cable"
                .to_string(),
        },

        Provider::ICloud => match error.category {
            ErrorCategory::Transient => format!(
                "This folder is managed by **{name}**. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Make sure you're signed in to iCloud in System Settings\n\
                    - Navigate here again to retry"
            ),
            ErrorCategory::NeedsAction => format!(
                "This folder is managed by **{name}**. Here's what to try:\n\
                    - Check that iCloud Drive is enabled in **System Settings > Apple Account > iCloud**\n\
                    - Make sure you're signed in to the right Apple account\n\
                    - Check your iCloud storage isn't full"
            ),
            ErrorCategory::Serious => format!(
                "This folder is managed by **{name}**. Here's what to try:\n\
                    - Sign out and back in to iCloud in System Settings\n\
                    - Check Apple's [system status page](https://www.apple.com/support/systemstatus/)"
            ),
        },

        Provider::MacFuse => match error.category {
            ErrorCategory::Transient => "This is a **macFUSE** mount. The remote server may be slow or unreachable. \
                Here's what to try:\n\
                    - Check your network connection\n\
                    - Check that the remote server is running\n\
                    - Navigate here again to retry"
                .to_string(),
            ErrorCategory::Serious => "This is a **macFUSE** mount. The FUSE process backing it has likely \
                crashed or disconnected. Here's what to try:\n\
                    - Force-unmount the volume: run `umount -f /Volumes/<name>` in Terminal\n\
                    - Remount using the original mount command\n\
                    - If this keeps happening, check that macFUSE is up to date"
                .to_string(),
            ErrorCategory::NeedsAction => "This is a **macFUSE** mount. Here's what to try:\n\
                    - Check that the FUSE process backing this mount is still running\n\
                    - Force-unmount and remount the volume if needed\n\
                    - Make sure macFUSE is up to date in **System Settings > General > Login Items & Extensions**"
                .to_string(),
        },

        Provider::PCloudFuse => match error.category {
            ErrorCategory::Transient => "This folder is on **pCloud**'s virtual drive. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Make sure the pCloud app is running\n\
                    - Navigate here again to retry"
                .to_string(),
            ErrorCategory::Serious => "This folder is on **pCloud**'s virtual drive. The pCloud FUSE process may have \
                crashed. Here's what to try:\n\
                    - Quit and reopen the pCloud app\n\
                    - If the drive doesn't reappear, force-unmount it: run `umount -f /Volumes/pCloudDrive` in Terminal\n\
                    - After a macOS update, re-approve pCloud's system extension in \
                      **System Settings > General > Login Items & Extensions**"
                .to_string(),
            ErrorCategory::NeedsAction => "This folder is on **pCloud**'s virtual drive. Here's what to try:\n\
                    - Make sure the pCloud app is running and you're signed in\n\
                    - Check your internet connection\n\
                    - After a macOS update, re-approve pCloud's system extension in \
                      **System Settings > General > Login Items & Extensions**"
                .to_string(),
        },

        Provider::VeraCrypt => match error.category {
            ErrorCategory::Transient => format!(
                "This is a **{name}** encrypted volume. Here's what to try:\n\
                    - Check that the VeraCrypt volume is still mounted\n\
                    - Navigate here again to retry"
            ),
            ErrorCategory::NeedsAction => format!(
                "This is a **{name}** encrypted volume. Here's what to try:\n\
                    - Open VeraCrypt and check that this volume is mounted\n\
                    - Dismount and remount the volume if needed"
            ),
            ErrorCategory::Serious => format!(
                "This is a **{name}** encrypted volume. Here's what to try:\n\
                    - Dismount and remount the volume in VeraCrypt\n\
                    - If the volume keeps having issues, check it with VeraCrypt's repair tools"
            ),
        },

        Provider::CmVolumes => match error.category {
            ErrorCategory::Transient => "This is a cloud mount. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Check that the mount software (CloudMounter, Mountain Duck, etc.) is running\n\
                    - Navigate here again to retry"
                .to_string(),
            _ => "This is a cloud mount. Here's what to try:\n\
                    - Check that the mount software (CloudMounter, Mountain Duck, etc.) is running\n\
                    - Disconnect and reconnect the mount\n\
                    - Check your credentials haven't expired"
                .to_string(),
        },

        Provider::GenericCloudStorage => match error.category {
            ErrorCategory::Transient => "This folder is managed by a cloud provider. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Check that the sync app is running\n\
                    - Navigate here again to retry"
                .to_string(),
            _ => "This folder is managed by a cloud provider. Here's what to try:\n\
                    - Check that the sync app is running\n\
                    - Sign out and back in to the cloud app\n\
                    - Check your internet connection"
                .to_string(),
        },

        // Cloud providers with an app name: Dropbox, Google Drive, OneDrive, Box,
        // pCloud, Nextcloud, SynologyDrive, Tresorit, ProtonDrive, Sync, Egnyte
        _ => {
            let app = provider.app_name().unwrap_or(name);
            match error.category {
                ErrorCategory::Transient => format!(
                    "This folder is managed by **{name}**. Here's what to try:\n\
                    - Check your internet connection\n\
                    - Open {app} and make sure it's running and synced\n\
                    - Navigate here again to retry"
                ),
                ErrorCategory::NeedsAction => format!(
                    "This folder is managed by **{name}**. Here's what to try:\n\
                    - Open {app} and check your sync status\n\
                    - Make sure you're signed in to {app}\n\
                    - Check that you have access to this folder in {name}"
                ),
                ErrorCategory::Serious => format!(
                    "This folder is managed by **{name}**. Here's what to try:\n\
                    - Quit and reopen {app}\n\
                    - Sign out and back in to {app}\n\
                    - Check {name}'s status page for outages"
                ),
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::file_system::volume::VolumeError;
    use crate::file_system::volume::friendly_error::friendly_error_from_volume_error;

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
                detected.as_ref(),
                Some(&expected),
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
    fn enrichment_overwrites_suggestion_but_not_title_or_category() {
        let err = VolumeError::ConnectionTimeout("test".into());
        let path = home_path("Library/CloudStorage/Dropbox/some/folder");

        let mut friendly = friendly_error_from_volume_error(&err, &path);
        let original_title = friendly.title.clone();
        let original_category = friendly.category;
        let original_retry = friendly.retry_hint;
        let original_suggestion = friendly.suggestion.clone();

        enrich_with_provider(&mut friendly, &path);

        assert_eq!(friendly.title, original_title, "title should not change");
        assert_eq!(friendly.category, original_category, "category should not change");
        assert_eq!(friendly.retry_hint, original_retry, "retry_hint should not change");
        assert_ne!(
            friendly.suggestion, original_suggestion,
            "suggestion should be overwritten by provider enrichment"
        );
        assert!(
            friendly.suggestion.contains("Dropbox"),
            "enriched suggestion should mention Dropbox"
        );
    }

    #[test]
    fn enrichment_is_noop_for_unknown_path() {
        let err = VolumeError::ConnectionTimeout("test".into());
        let path = Path::new("/Users/test/Documents/folder");

        let mut friendly = friendly_error_from_volume_error(&err, path);
        let original_suggestion = friendly.suggestion.clone();

        enrich_with_provider(&mut friendly, path);

        assert_eq!(
            friendly.suggestion, original_suggestion,
            "suggestion should not change for unknown paths"
        );
    }

    // ── Provider suggestion tests ───────────────────────────────────────

    #[test]
    fn all_providers_produce_specific_suggestions() {
        let providers_and_paths: Vec<(&str, Provider)> = vec![
            ("Library/CloudStorage/Dropbox/f", Provider::Dropbox),
            ("Library/CloudStorage/GoogleDrive-x/f", Provider::GoogleDrive),
            ("Library/CloudStorage/OneDrive-x/f", Provider::OneDrive),
            ("Library/CloudStorage/Box-x/f", Provider::Box),
            ("Library/CloudStorage/pCloud/f", Provider::PCloud),
            ("Library/CloudStorage/Nextcloud-x/f", Provider::Nextcloud),
            ("Library/CloudStorage/SynologyDrive-x/f", Provider::SynologyDrive),
            ("Library/CloudStorage/Tresorit/f", Provider::Tresorit),
            ("Library/CloudStorage/ProtonDrive-x/f", Provider::ProtonDrive),
            ("Library/CloudStorage/Sync-x/f", Provider::Sync),
            ("Library/CloudStorage/Egnyte-x/f", Provider::Egnyte),
            ("Library/CloudStorage/MacDroid-x/f", Provider::MacDroid),
            ("Library/CloudStorage/Unknown-x/f", Provider::GenericCloudStorage),
            ("Library/Mobile Documents/com~apple~CloudDocs/f", Provider::ICloud),
        ];

        for (suffix, expected_provider) in &providers_and_paths {
            let path = home_path(suffix);
            let err = VolumeError::ConnectionTimeout("test".into());
            let mut friendly = friendly_error_from_volume_error(&err, &path);
            enrich_with_provider(&mut friendly, &path);

            assert!(
                friendly.suggestion.contains(expected_provider.display_name())
                    || *expected_provider == Provider::GenericCloudStorage
                    || *expected_provider == Provider::CmVolumes,
                "Suggestion for {:?} should mention provider name. Got: {}",
                expected_provider,
                friendly.suggestion
            );
        }

        // Specific-path providers
        let specific_paths: Vec<(&str, Provider)> = vec![
            ("/Volumes/pCloudDrive/f", Provider::PCloudFuse),
            ("/Volumes/veracrypt1/f", Provider::VeraCrypt),
        ];

        for (path_str, expected_provider) in &specific_paths {
            let path = Path::new(path_str);
            let err = VolumeError::ConnectionTimeout("test".into());
            let mut friendly = friendly_error_from_volume_error(&err, path);
            enrich_with_provider(&mut friendly, path);

            assert!(
                friendly.suggestion.contains(expected_provider.display_name()),
                "Suggestion for {:?} should mention provider name. Got: {}",
                expected_provider,
                friendly.suggestion
            );
        }

        // CmVolumes
        let cm_path = home_path(".CMVolumes/MyMount/f");
        let err = VolumeError::ConnectionTimeout("test".into());
        let mut friendly = friendly_error_from_volume_error(&err, &cm_path);
        enrich_with_provider(&mut friendly, &cm_path);
        assert!(
            friendly.suggestion.contains("cloud mount"),
            "CmVolumes suggestion should mention cloud mount"
        );
    }

    // ── MacFuse and PCloudFuse suggestion tests ────────────────────────

    #[test]
    fn macfuse_suggestions_mention_macfuse() {
        let categories = [
            ErrorCategory::Transient,
            ErrorCategory::NeedsAction,
            ErrorCategory::Serious,
        ];
        for category in categories {
            let error = FriendlyError {
                category,
                title: "test".into(),
                explanation: "test".into(),
                suggestion: "placeholder".into(),
                raw_detail: "test".into(),
                retry_hint: false,
            };
            let suggestion = provider_suggestion(&Provider::MacFuse, &error);
            assert!(
                suggestion.contains("macFUSE"),
                "MacFuse {:?} suggestion should mention macFUSE. Got: {}",
                category,
                suggestion
            );
        }
    }

    #[test]
    fn pcloud_fuse_suggestions_mention_pcloud() {
        let categories = [
            ErrorCategory::Transient,
            ErrorCategory::NeedsAction,
            ErrorCategory::Serious,
        ];
        for category in categories {
            let error = FriendlyError {
                category,
                title: "test".into(),
                explanation: "test".into(),
                suggestion: "placeholder".into(),
                raw_detail: "test".into(),
                retry_hint: false,
            };
            let suggestion = provider_suggestion(&Provider::PCloudFuse, &error);
            assert!(
                suggestion.contains("pCloud"),
                "PCloudFuse {:?} suggestion should mention pCloud. Got: {}",
                category,
                suggestion
            );
        }
    }

    #[test]
    fn fuse_provider_suggestions_follow_style_guide() {
        let providers = [Provider::MacFuse, Provider::PCloudFuse];
        let categories = [
            ErrorCategory::Transient,
            ErrorCategory::NeedsAction,
            ErrorCategory::Serious,
        ];

        for provider in &providers {
            for category in &categories {
                let error = FriendlyError {
                    category: *category,
                    title: "test".into(),
                    explanation: "test".into(),
                    suggestion: "placeholder".into(),
                    raw_detail: "test".into(),
                    retry_hint: false,
                };
                let suggestion = provider_suggestion(provider, &error);
                let lower = suggestion.to_lowercase();

                assert!(
                    !lower.contains("error") && !lower.contains("failed"),
                    "{:?} {:?} suggestion contains 'error' or 'failed': {}",
                    provider,
                    category,
                    suggestion
                );
            }
        }
    }

    #[test]
    fn macfuse_serious_suggests_force_unmount() {
        let error = FriendlyError {
            category: ErrorCategory::Serious,
            title: "test".into(),
            explanation: "test".into(),
            suggestion: "placeholder".into(),
            raw_detail: "test".into(),
            retry_hint: false,
        };
        let suggestion = provider_suggestion(&Provider::MacFuse, &error);
        assert!(
            suggestion.contains("umount -f"),
            "MacFuse Serious suggestion should mention force-unmount. Got: {}",
            suggestion
        );
    }

    #[test]
    fn pcloud_fuse_serious_suggests_system_extension_reapproval() {
        let error = FriendlyError {
            category: ErrorCategory::Serious,
            title: "test".into(),
            explanation: "test".into(),
            suggestion: "placeholder".into(),
            raw_detail: "test".into(),
            retry_hint: false,
        };
        let suggestion = provider_suggestion(&Provider::PCloudFuse, &error);
        assert!(
            suggestion.contains("System Settings"),
            "PCloudFuse Serious suggestion should mention System Settings. Got: {}",
            suggestion
        );
    }
}

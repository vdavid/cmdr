//! "Make available offline" / "Remove download" wrappers (iCloud Drive only).
//!
//! Implementation notes (important context for future agents):
//!
//! `NSFileProviderManager` looked like the universal cross-provider API (Dropbox,
//! Google Drive, OneDrive, Box, iCloud all show up as File Provider domains since
//! macOS 12.3). It isn't: its host-side methods (`getDomainsWithCompletionHandler`,
//! `evictItemWithIdentifier`, `requestDownloadForItemWithIdentifier`) are reserved
//! for the app that *bundles* the File Provider extension. From a third-party app
//! like Cmdr, the system rejects the call with `NSFileProviderErrorProviderNotFound`
//! ("The application cannot be used right now"). Finder gets around this with
//! private XPC; there's no public path for non-host apps.
//!
//! What does work for any app: `FileManager.evictUbiquitousItem(at:)` and
//! `startDownloadingUbiquitousItem(at:)`. They route through the **iCloud (NSUbiquity)
//! infrastructure**, separate from the File Provider host APIs above, and accept
//! any URL inside an iCloud ubiquity container. So we offer the eviction / download
//! menu items only for files under `~/Library/Mobile Documents/com~apple~CloudDocs/`
//! (iCloud Drive). For Dropbox/GDrive/OneDrive items the menu items don't appear;
//! the user has to use the provider's own client (or Finder).
use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

/// Subdirectory under `$HOME` for iCloud Drive items.
#[cfg(target_os = "macos")]
pub const ICLOUD_DRIVE_SUBPATH: &str = "Library/Mobile Documents/com~apple~CloudDocs";

#[cfg(target_os = "macos")]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Returns true only for paths inside iCloud Drive, the location where
/// `FileManager.evictUbiquitousItem` / `startDownloadingUbiquitousItem` work.
#[cfg(target_os = "macos")]
pub fn is_in_icloud_drive(path: &Path) -> bool {
    let Some(home) = home_dir() else {
        return false;
    };
    path.starts_with(home.join(ICLOUD_DRIVE_SUBPATH))
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use log::{debug, warn};
    use objc2::rc::Retained;
    use objc2_foundation::{NSError, NSFileManager, NSString, NSURL};

    /// Reads `code`, `domain`, and `localizedDescription` off an `NSError` so we can
    /// log the structured error fields the system actually returned. The plain
    /// `localizedDescription` (which is what we surface to the user) often loses the
    /// useful bits (the domain tells us whether it's `NSCocoaErrorDomain`,
    /// `NSURLErrorDomain`, etc.; the code tells us which error within that domain).
    fn describe_ns_error(err: &NSError) -> String {
        let code = err.code();
        let domain = err.domain().to_string();
        let desc = err.localizedDescription().to_string();
        format!("domain={domain} code={code}: {desc}")
    }

    fn nsurl_from_path(path: &Path) -> Result<Retained<NSURL>, String> {
        let path_str = path
            .to_str()
            .ok_or_else(|| "Couldn't convert path to UTF-8".to_string())?;
        let ns_path = NSString::from_str(path_str);
        Ok(NSURL::fileURLWithPath(&ns_path))
    }

    /// Evicts a downloaded iCloud Drive file: keeps the placeholder, removes the local
    /// copy. The path's sync status flips to `OnlineOnly` once macOS lands the change.
    pub fn evict_item(path: &Path) -> Result<(), String> {
        debug!(target: "cloud_actions", "evict_item: path={path:?}");
        if !is_in_icloud_drive(path) {
            return Err("Removing the download is only supported for iCloud Drive files".to_string());
        }
        let url = nsurl_from_path(path)?;
        let fm = NSFileManager::defaultManager();
        let result = fm.evictUbiquitousItemAtURL_error(&url);
        match result {
            Ok(()) => {
                // Eviction only flips `SF_DATALESS`, which doesn't reliably reach
                // FSEvents, so the badge would otherwise show the old answer until
                // its TTL ran out.
                crate::file_system::sync_status::invalidate_path(path);
                Ok(())
            }
            Err(err) => {
                let detail = describe_ns_error(&err);
                warn!(target: "cloud_actions", "evictUbiquitousItem failed: {detail}");
                Err(err.localizedDescription().to_string())
            }
        }
    }

    /// Requests that an iCloud Drive file be downloaded (made available offline). The
    /// call returns once the request is queued, not once the download completes;
    /// `sync_status.rs` tracks the `Downloading → Synced` transition for the UI.
    pub fn request_download(path: &Path) -> Result<(), String> {
        debug!(target: "cloud_actions", "request_download: path={path:?}");
        if !is_in_icloud_drive(path) {
            return Err("Making files available offline is only supported for iCloud Drive".to_string());
        }
        let url = nsurl_from_path(path)?;
        let fm = NSFileManager::defaultManager();
        let result = fm.startDownloadingUbiquitousItemAtURL_error(&url);
        match result {
            Ok(()) => {
                // The download starts behind this call, so the cached `OnlineOnly`
                // has to go now for the badge to pick up `Downloading`.
                crate::file_system::sync_status::invalidate_path(path);
                Ok(())
            }
            Err(err) => {
                let detail = describe_ns_error(&err);
                warn!(target: "cloud_actions", "startDownloadingUbiquitousItem failed: {detail}");
                Err(err.localizedDescription().to_string())
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::{evict_item, request_download};

#[cfg(not(target_os = "macos"))]
pub fn evict_item(_path: &Path) -> Result<(), String> {
    Err("Cloud actions are only available on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn request_download(_path: &Path) -> Result<(), String> {
    Err("Cloud actions are only available on macOS".to_string())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// The real home directory, which is what `is_in_icloud_drive` resolves.
    ///
    /// ❌ Don't reintroduce `env::set_var("HOME", ...)` to fake this. The
    /// environment is process-wide, so under a plain `cargo test` (threads, one
    /// process) it poisons every concurrently-running test that resolves a home
    /// path — `favorites::store`'s defaults and `go_to_path` among them — and
    /// mutating the environment while other threads read it is UB besides.
    /// Building the fixtures from the real home tests the same comparison.
    fn home() -> PathBuf {
        dirs::home_dir().expect("a home directory")
    }

    #[test]
    fn is_in_icloud_drive_matches_icloud() {
        let icloud = home().join(ICLOUD_DRIVE_SUBPATH);
        assert!(is_in_icloud_drive(&icloud.join("foo.txt")));
        assert!(is_in_icloud_drive(&icloud.join("sub/folder/x")));
    }

    #[test]
    fn is_in_icloud_drive_excludes_third_party() {
        // Third-party clouds aren't iCloud, so eviction APIs don't apply.
        let cloud_storage = home().join("Library/CloudStorage");
        assert!(!is_in_icloud_drive(&cloud_storage.join("Dropbox/foo.txt")));
        assert!(!is_in_icloud_drive(
            &cloud_storage.join("GoogleDrive-me@example.com/sub/file")
        ));
        assert!(!is_in_icloud_drive(&cloud_storage.join("OneDrive-Personal/x")));
    }

    #[test]
    fn is_in_icloud_drive_negative() {
        assert!(!is_in_icloud_drive(&home().join("Documents/foo.txt")));
        assert!(!is_in_icloud_drive(Path::new("/tmp/foo.txt")));
        // Contains the subpath but isn't actually under it.
        assert!(!is_in_icloud_drive(
            &home().join("Other").join(ICLOUD_DRIVE_SUBPATH).join("foo")
        ));
    }
}

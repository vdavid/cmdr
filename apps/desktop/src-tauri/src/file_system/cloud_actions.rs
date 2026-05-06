//! Cross-provider cloud action wrappers around `NSFileProviderManager`.
//!
//! Provides "Make available offline" (download) and "Remove download" (evict) for files
//! managed by any File Provider extension on macOS — iCloud Drive, Dropbox, Google Drive,
//! OneDrive, Box, and others. Uses public AppKit APIs only.
//!
//! Detection of "is this a cloud file" is a fast path-prefix check
//! (`is_in_cloud_storage`); the actual evict/download chain calls async
//! `NSFileProviderManager` APIs synchronously via completion handlers + sync_channel.

use std::path::{Path, PathBuf};

/// Subdirectory under `$HOME` for iCloud Drive items.
pub const ICLOUD_DRIVE_SUBPATH: &str = "Library/Mobile Documents/com~apple~CloudDocs";

/// Subdirectory under `$HOME` for File-Provider-backed clouds (Dropbox, Google Drive, OneDrive,
/// Box, etc.). Apple migrated all third-party providers under here in macOS 12.3.
pub const CLOUD_STORAGE_SUBPATH: &str = "Library/CloudStorage";

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Returns true if the path lies within any macOS File Provider domain — iCloud Drive
/// or any third-party cloud under `~/Library/CloudStorage/`. Fast path-prefix check
/// with no system calls.
pub fn is_in_cloud_storage(path: &Path) -> bool {
    let Some(home) = home_dir() else {
        return false;
    };
    path.starts_with(home.join(ICLOUD_DRIVE_SUBPATH)) || path.starts_with(home.join(CLOUD_STORAGE_SUBPATH))
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use block2::RcBlock;
    use log::{debug, warn};
    use objc2::rc::Retained;
    use objc2_file_provider::{NSFileProviderDomain, NSFileProviderManager};
    use objc2_foundation::{NSArray, NSError, NSRange, NSString, NSURL};
    use std::ptr::NonNull;
    use std::sync::mpsc;
    use std::time::Duration;

    /// Reads `code`, `domain`, and `localizedDescription` off an `NSError` so we can
    /// log the structured error fields the system actually returned. The plain
    /// `localizedDescription` (which is what we surface to the user) often loses the
    /// useful bits (the domain tells us whether it's `NSFileProviderErrorDomain`,
    /// `NSCocoaErrorDomain`, etc.; the code tells us which error within that domain).
    fn describe_ns_error(err: &NSError) -> String {
        let code = err.code();
        let domain = err.domain().to_string();
        let desc = err.localizedDescription().to_string();
        format!("domain={domain} code={code}: {desc}")
    }

    /// Timeout for the path → identifier and domains-list lookups (XPC round-trips).
    const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);
    /// Timeout for evict/download requests. Downloads can be slow on large files; the
    /// completion handler fires when the download starts, not when it finishes.
    const ACTION_TIMEOUT: Duration = Duration::from_secs(30);

    fn nsurl_from_path(path: &Path) -> Result<Retained<NSURL>, String> {
        let path_str = path
            .to_str()
            .ok_or_else(|| "Couldn't convert path to UTF-8".to_string())?;
        let ns_path = NSString::from_str(path_str);
        Ok(NSURL::fileURLWithPath(&ns_path))
    }

    /// Wraps `getIdentifierForUserVisibleFileAtURL:completionHandler:`. Returns the
    /// item identifier and domain identifier for the file. Returns an error if the
    /// file is not managed by any File Provider extension.
    fn fetch_item_and_domain_id(url: &NSURL) -> Result<(Retained<NSString>, Retained<NSString>), String> {
        let (tx, rx) = mpsc::sync_channel::<Result<(Retained<NSString>, Retained<NSString>), String>>(1);
        let block = RcBlock::new(move |item: *mut NSString, domain: *mut NSString, error: *mut NSError| {
            if !error.is_null() {
                let err = unsafe { &*error };
                let detail = describe_ns_error(err);
                warn!(target: "cloud_actions", "getIdentifierForUserVisibleFileAtURL failed: {detail}");
                let _ = tx.send(Err(err.localizedDescription().to_string()));
                return;
            }
            let (Some(item), Some(domain)) = (unsafe { Retained::retain(item) }, unsafe { Retained::retain(domain) })
            else {
                let _ = tx.send(Err("This file isn't managed by a cloud provider".to_string()));
                return;
            };
            debug!(
                target: "cloud_actions",
                "getIdentifierForUserVisibleFileAtURL ok: item={} domain={}",
                item,
                domain
            );
            let _ = tx.send(Ok((item, domain)));
        });
        unsafe {
            NSFileProviderManager::getIdentifierForUserVisibleFileAtURL_completionHandler(url, &block);
        }
        rx.recv_timeout(LOOKUP_TIMEOUT)
            .map_err(|_| "Timed out asking the system for the cloud item".to_string())?
    }

    /// Wraps `getDomainsWithCompletionHandler:`. Returns all currently registered
    /// File Provider domains.
    fn fetch_domains() -> Result<Retained<NSArray<NSFileProviderDomain>>, String> {
        let (tx, rx) = mpsc::sync_channel::<Result<Retained<NSArray<NSFileProviderDomain>>, String>>(1);
        let block = RcBlock::new(
            move |domains: NonNull<NSArray<NSFileProviderDomain>>, error: *mut NSError| {
                if !error.is_null() {
                    let err = unsafe { &*error };
                    let detail = describe_ns_error(err);
                    warn!(target: "cloud_actions", "getDomainsWithCompletionHandler failed: {detail}");
                    let _ = tx.send(Err(err.localizedDescription().to_string()));
                    return;
                }
                let domains = unsafe { Retained::retain(domains.as_ptr()) };
                match domains {
                    Some(d) => {
                        debug!(target: "cloud_actions", "getDomains ok: {} domains", d.count());
                        let _ = tx.send(Ok(d));
                    }
                    None => {
                        let _ = tx.send(Err("System returned no cloud domains".to_string()));
                    }
                }
            },
        );
        unsafe {
            NSFileProviderManager::getDomainsWithCompletionHandler(&block);
        }
        rx.recv_timeout(LOOKUP_TIMEOUT)
            .map_err(|_| "Timed out fetching cloud domains".to_string())?
    }

    /// Finds the manager for the domain that owns this file, plus the file's item identifier.
    fn manager_and_item_id(path: &Path) -> Result<(Retained<NSFileProviderManager>, Retained<NSString>), String> {
        let url = nsurl_from_path(path)?;
        let (item_id, domain_id) = fetch_item_and_domain_id(&url)?;
        let domains = fetch_domains()?;
        // Find the matching domain by identifier. `identifier()` is unsafe because the
        // Apple selector returns `+0` and the binding can't statically verify the
        // domain object hasn't been freed; we hold `domains` for the iteration's lifetime.
        let domain = domains
            .iter()
            .find(|d| {
                let id = unsafe { d.identifier() };
                id.isEqualToString(&domain_id)
            })
            .ok_or_else(|| "Cloud domain not found for this file".to_string())?;
        let manager = unsafe { NSFileProviderManager::managerForDomain(&domain) }
            .ok_or_else(|| "Cloud manager unavailable for this domain".to_string())?;
        Ok((manager, item_id))
    }

    /// Drains a fresh sync_channel that receives a single optional error from a completion block.
    fn run_with_error_completion<F>(stage: &'static str, timeout: Duration, run: F) -> Result<(), String>
    where
        F: FnOnce(&block2::DynBlock<dyn Fn(*mut NSError)>),
    {
        let (tx, rx) = mpsc::sync_channel::<Result<(), String>>(1);
        let block = RcBlock::new(move |error: *mut NSError| {
            if error.is_null() {
                let _ = tx.send(Ok(()));
            } else {
                let err = unsafe { &*error };
                let detail = describe_ns_error(err);
                warn!(target: "cloud_actions", "{stage} failed: {detail}");
                let _ = tx.send(Err(err.localizedDescription().to_string()));
            }
        });
        run(&block);
        rx.recv_timeout(timeout)
            .map_err(|_| "Timed out waiting for cloud action".to_string())?
    }

    /// Evicts a downloaded cloud file: keeps the placeholder, removes the local copy.
    /// The path's sync status will flip to `OnlineOnly` once the eviction lands.
    pub fn evict_item(path: &Path) -> Result<(), String> {
        debug!(target: "cloud_actions", "evict_item: path={path:?}");
        let (manager, item_id) = manager_and_item_id(path)?;
        run_with_error_completion("evictItemWithIdentifier", ACTION_TIMEOUT, |block| unsafe {
            manager.evictItemWithIdentifier_completionHandler(&item_id, block);
        })
    }

    /// Requests download of a cloud file (whole file). The completion fires when the
    /// download is initiated, not when it finishes; `sync_status.rs` polling tracks
    /// the `Downloading → Synced` transition for the UI.
    pub fn request_download(path: &Path) -> Result<(), String> {
        debug!(target: "cloud_actions", "request_download: path={path:?}");
        let (manager, item_id) = manager_and_item_id(path)?;
        // `NSRange{ location: NSNotFound, length: 0 }` means "the whole file" per Apple's docs.
        // `NSNotFound` is `NSIntegerMax` (`isize::MAX`) cast to `NSUInteger`.
        let whole_file = NSRange {
            location: isize::MAX as usize,
            length: 0,
        };
        run_with_error_completion("requestDownloadForItemWithIdentifier", ACTION_TIMEOUT, |block| unsafe {
            manager.requestDownloadForItemWithIdentifier_requestedRange_completionHandler(&item_id, whole_file, block);
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_in_cloud_storage_icloud() {
        // SAFETY: tests run sequentially in the same process; setting HOME during
        // the test is fine because we only read it inside is_in_cloud_storage.
        unsafe {
            std::env::set_var("HOME", "/Users/test");
        }
        assert!(is_in_cloud_storage(Path::new(
            "/Users/test/Library/Mobile Documents/com~apple~CloudDocs/foo.txt"
        )));
        assert!(is_in_cloud_storage(Path::new(
            "/Users/test/Library/Mobile Documents/com~apple~CloudDocs/sub/folder/x"
        )));
    }

    #[test]
    fn test_is_in_cloud_storage_third_party() {
        unsafe {
            std::env::set_var("HOME", "/Users/test");
        }
        assert!(is_in_cloud_storage(Path::new(
            "/Users/test/Library/CloudStorage/Dropbox/foo.txt"
        )));
        assert!(is_in_cloud_storage(Path::new(
            "/Users/test/Library/CloudStorage/GoogleDrive-me@example.com/sub/file"
        )));
        assert!(is_in_cloud_storage(Path::new(
            "/Users/test/Library/CloudStorage/OneDrive-Personal/x"
        )));
    }

    #[test]
    fn test_is_in_cloud_storage_negative() {
        unsafe {
            std::env::set_var("HOME", "/Users/test");
        }
        assert!(!is_in_cloud_storage(Path::new("/Users/test/Documents/foo.txt")));
        assert!(!is_in_cloud_storage(Path::new("/tmp/foo.txt")));
        // Path that contains the substring but isn't actually under it
        assert!(!is_in_cloud_storage(Path::new(
            "/Users/test/Other/Library/CloudStorage/foo"
        )));
    }
}

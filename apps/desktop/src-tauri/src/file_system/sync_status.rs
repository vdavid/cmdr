//! Dropbox sync status detection for macOS File Provider.
//!
//! Detects file sync states:
//! - Synced: Local content matches cloud
//! - OnlineOnly: Stub file, content in cloud only
//! - Uploading: Local changes being uploaded
//! - Downloading: Cloud content being fetched
//!
//! Detection uses stat() for fast online-only detection.
//! For uploading/downloading states, we use NSURL resource values.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// macOS SF_DATALESS flag indicating a stub/online-only file.
const SF_DATALESS: u32 = 0x40000000;

/// Sync status for a file in a cloud-synced folder (Dropbox, iCloud, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    /// File is fully synced - local content matches cloud
    Synced,
    /// File is online-only - stub file, content in cloud
    OnlineOnly,
    /// File is being uploaded to cloud
    Uploading,
    /// File is being downloaded from cloud
    Downloading,
    /// Not a cloud file or status cannot be determined
    Unknown,
}

/// Gets sync status for a single file.
///
/// Uses stat() for fast online-only detection, then NSURL for upload/download state.
fn get_sync_status(path: &Path) -> SyncStatus {
    use std::os::macos::fs::MetadataExt;

    // Get file metadata
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return SyncStatus::Unknown,
    };

    // Check if file is a stub (online-only) via SF_DATALESS flag
    let flags = metadata.st_flags();
    let is_dataless = (flags & SF_DATALESS) != 0;

    if is_dataless {
        // File is a stub - could be online-only or downloading
        // Try to detect downloading state via NSURL
        if is_downloading(path) {
            SyncStatus::Downloading
        } else {
            SyncStatus::OnlineOnly
        }
    } else {
        // File has local content - could be synced or uploading
        // Use is_cloud_file() to check if this is actually a cloud file
        match is_uploading_cloud_file(path) {
            Some(true) => SyncStatus::Uploading,
            Some(false) => SyncStatus::Synced,
            None => SyncStatus::Unknown, // Not a cloud file
        }
    }
}

/// Checks if file is currently uploading via NSURL resource values.
/// Returns None if file is not a cloud file.
fn is_uploading_cloud_file(path: &Path) -> Option<bool> {
    get_ubiquitous_bool(path, "NSURLUbiquitousItemIsUploadingKey")
}

/// Checks if file is currently downloading via NSURL resource values.
fn is_downloading(path: &Path) -> bool {
    get_ubiquitous_bool(path, "NSURLUbiquitousItemIsDownloadingKey").unwrap_or(false)
}

/// Gets a boolean ubiquitous item property from NSURL.
fn get_ubiquitous_bool(path: &Path, key: &str) -> Option<bool> {
    use objc2::rc::Retained;
    use objc2_foundation::{NSNumber, NSString, NSURL};

    let path_str = path.to_str()?;
    let ns_path = NSString::from_str(path_str);
    let url = NSURL::fileURLWithPath(&ns_path);

    let key = NSString::from_str(key);
    let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
    let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };

    if success.is_ok() {
        value.and_then(|obj| obj.downcast::<NSNumber>().ok().map(|n| n.boolValue()))
    } else {
        None
    }
}

/// Gets sync status for multiple paths in parallel.
///
/// Uses Rayon's default thread pool (auto-detects CPU cores).
pub fn get_sync_statuses(paths: Vec<String>) -> HashMap<String, SyncStatus> {
    paths
        .par_iter()
        .map(|path| {
            let status = get_sync_status(Path::new(path));
            (path.clone(), status)
        })
        .collect()
}

/// Gets sync status for multiple paths with configurable parallelism.
///
/// Uses a Rayon thread pool with the specified number of threads.
#[allow(dead_code)] // Used for benchmarking
pub fn get_sync_statuses_with_threads(paths: Vec<String>, num_threads: usize) -> HashMap<String, SyncStatus> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

    pool.install(|| {
        paths
            .par_iter()
            .map(|path| {
                let status = get_sync_status(Path::new(path));
                (path.clone(), status)
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_serialization() {
        assert_eq!(serde_json::to_string(&SyncStatus::Synced).unwrap(), "\"synced\"");
        assert_eq!(
            serde_json::to_string(&SyncStatus::OnlineOnly).unwrap(),
            "\"online_only\""
        );
        assert_eq!(serde_json::to_string(&SyncStatus::Uploading).unwrap(), "\"uploading\"");
        assert_eq!(
            serde_json::to_string(&SyncStatus::Downloading).unwrap(),
            "\"downloading\""
        );
    }
}

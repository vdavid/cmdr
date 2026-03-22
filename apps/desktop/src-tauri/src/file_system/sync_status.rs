//! Cloud sync status detection for macOS File Provider.
//!
//! Detects file sync states:
//! - Synced: Local content matches cloud
//! - OnlineOnly: Stub file, content in cloud only
//! - Uploading: Local changes being uploaded
//! - Downloading: Cloud content being fetched
//!
//! Detection uses stat() for fast online-only detection.
//! For uploading/downloading states, we use NSURL resource values.
//!
//! Parallelism uses dedicated OS threads (not rayon) because the NSURL calls
//! make synchronous XPC round-trips to FileProvider daemons. These are I/O-bound
//! and can consume deep stack frames (FileProvider override chains), so they need
//! a larger stack than rayon's default 2 MB. Using dedicated threads also avoids
//! starving rayon's pool, which is reserved for CPU-bound work.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// macOS SF_DATALESS flag indicating a stub/online-only file.
const SF_DATALESS: u32 = 0x40000000;

/// Sync status for a file in a cloud-synced folder (Dropbox, iCloud, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    Synced,
    /// Stub file, content in cloud only.
    OnlineOnly,
    Uploading,
    Downloading,
    /// Not a cloud file or status cannot be determined.
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
    use objc2::rc::{Retained, autoreleasepool};
    use objc2_foundation::{NSNumber, NSString, NSURL};

    // Drain autoreleased ObjC objects (NSURL, NSString) created per call.
    // Called from spawned threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
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
    })
}

/// 8 MB stack per thread — enough for deep FileProvider XPC call chains.
const THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Gets sync status for multiple paths in parallel using dedicated OS threads.
///
/// Each NSURL resource-value lookup makes a synchronous XPC call into the
/// FileProvider daemon, which can build deep stack frames through override
/// chains. Dedicated threads with an explicit 8 MB stack prevent the stack
/// overflows that occur on rayon's default 2 MB worker threads.
pub fn get_sync_statuses(paths: Vec<String>) -> HashMap<String, SyncStatus> {
    if paths.is_empty() {
        return HashMap::new();
    }

    let num_threads = paths
        .len()
        .min(std::thread::available_parallelism().map_or(4, |n| n.get()));

    std::thread::scope(|scope| {
        let chunk_size = paths.len().div_ceil(num_threads);
        let handles: Vec<_> = paths
            .chunks(chunk_size)
            .map(|chunk| {
                let chunk = chunk.to_vec();
                std::thread::Builder::new()
                    .stack_size(THREAD_STACK_SIZE)
                    .spawn_scoped(scope, move || {
                        chunk
                            .into_iter()
                            .map(|path| {
                                let status = get_sync_status(Path::new(&path));
                                (path, status)
                            })
                            .collect::<Vec<_>>()
                    })
                    .expect("failed to spawn sync-status thread")
            })
            .collect();

        let mut result = HashMap::with_capacity(paths.len());
        for handle in handles {
            result.extend(handle.join().expect("sync-status thread panicked"));
        }
        result
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

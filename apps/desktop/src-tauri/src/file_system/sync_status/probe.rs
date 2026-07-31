//! Asking macOS what a single file's cloud sync state is.
//!
//! Two tiers, cheapest first:
//!
//! 1. `stat`'s `SF_DATALESS` flag answers "is this a stub?" for free, off the
//!    inode. No provider process is involved.
//! 2. An `NSURL` ubiquitous-item resource value answers "is it moving right now?".
//!    This one is a **synchronous XPC round-trip into the provider's daemon**
//!    (`fileproviderd` and the provider's `.appex`), so it has no deadline of its
//!    own and can block for as long as the provider is unwell. Everything in the
//!    parent module exists to keep that fact from reaching the user.

use super::SyncStatus;
use std::path::Path;

/// macOS `SF_DATALESS` flag indicating a stub/online-only file.
const SF_DATALESS: u32 = 0x40000000;

/// The sync status of one file. Blocks for as long as the file's provider takes.
pub(super) fn sync_status_for(path: &Path) -> SyncStatus {
    use std::os::macos::fs::MetadataExt;

    let Ok(metadata) = std::fs::metadata(path) else {
        return SyncStatus::Unknown;
    };

    if metadata.st_flags() & SF_DATALESS != 0 {
        // A stub: either purely online, or being fetched right now.
        if ubiquitous_bool(path, "NSURLUbiquitousItemIsDownloadingKey").unwrap_or(false) {
            SyncStatus::Downloading
        } else {
            SyncStatus::OnlineOnly
        }
    } else {
        // Local content exists: either settled, or being pushed up. `None` means
        // the file isn't a cloud file at all.
        match ubiquitous_bool(path, "NSURLUbiquitousItemIsUploadingKey") {
            Some(true) => SyncStatus::Uploading,
            Some(false) => SyncStatus::Synced,
            None => SyncStatus::Unknown,
        }
    }
}

/// Reads a boolean ubiquitous-item property off `NSURL`. `None` when the property
/// doesn't apply (not a cloud file) or the read failed.
fn ubiquitous_bool(path: &Path, key: &str) -> Option<bool> {
    use objc2::rc::{Retained, autoreleasepool};
    use objc2_foundation::{NSNumber, NSString, NSURL};

    // Drain autoreleased ObjC objects (NSURL, NSString) created per call. Pool
    // workers are plain OS threads and have no AppKit autorelease pool of their own.
    autoreleasepool(|_| {
        let path_str = path.to_str()?;
        let ns_path = NSString::from_str(path_str);
        let url = NSURL::fileURLWithPath(&ns_path);

        let key = NSString::from_str(key);
        let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
        // SAFETY: `url` is a valid NSURL, `key` a valid NSString, and `&mut value` a valid
        // `&mut Option<Retained<_>>` out-param. On success objc2 stores an already-retained
        // object there per its out-param convention, so the `Retained` owns one reference.
        let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };

        if success.is_ok() {
            value.and_then(|obj| obj.downcast::<NSNumber>().ok().map(|n| n.boolValue()))
        } else {
            None
        }
    })
}

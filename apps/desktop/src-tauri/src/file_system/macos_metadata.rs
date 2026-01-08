//! macOS-specific file metadata retrieval using NSURL resource values.
//!
//! Provides access to metadata not available through standard `std::fs`:
//! - `added_at`: When the file was added to its current directory (moved/copied)
//! - `opened_at`: When the file was last opened

use std::path::Path;

use objc2::rc::Retained;
use objc2_foundation::{NSDate, NSString, NSURL};

/// Extended macOS metadata for a file.
pub struct MacOSMetadata {
    /// Unix timestamp: when the file was added to its current directory
    pub added_at: Option<u64>,
    /// Unix timestamp: when the file was last opened
    pub opened_at: Option<u64>,
}

/// Retrieves macOS-specific metadata for a file using NSURL resource values.
///
/// Returns `None` values for individual fields if they are unavailable on the volume
/// or if any error occurs during retrieval.
pub fn get_macos_metadata(path: &Path) -> MacOSMetadata {
    // Helper to convert NSDate to Unix timestamp
    fn nsdate_to_unix(date: Option<Retained<NSDate>>) -> Option<u64> {
        date.and_then(|d| {
            // NSDate timeIntervalSince1970 returns seconds since Unix epoch as f64
            let interval = d.timeIntervalSince1970();
            if interval >= 0.0 { Some(interval as u64) } else { None }
        })
    }

    // Convert path to NSString
    let path_str = match path.to_str() {
        Some(s) => s,
        None => {
            return MacOSMetadata {
                added_at: None,
                opened_at: None,
            };
        }
    };

    let ns_path = NSString::from_str(path_str);

    // Create NSURL from file path
    let url = NSURL::fileURLWithPath(&ns_path);

    // Fetch added_at (NSURLAddedToDirectoryDateKey)
    let added_at = {
        let key = NSString::from_str("NSURLAddedToDirectoryDateKey");
        let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
        let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };
        if success.is_ok() {
            // Cast AnyObject to NSDate if it's a date
            value.and_then(|obj| {
                // Downcast to NSDate - this is safe because we know the key returns NSDate
                let retained = obj.downcast::<NSDate>().ok();
                nsdate_to_unix(retained)
            })
        } else {
            None
        }
    };

    // Fetch opened_at (NSURLContentAccessDateKey)
    let opened_at = {
        let key = NSString::from_str("NSURLContentAccessDateKey");
        let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
        let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };
        if success.is_ok() {
            value.and_then(|obj| {
                let retained = obj.downcast::<NSDate>().ok();
                nsdate_to_unix(retained)
            })
        } else {
            None
        }
    };

    MacOSMetadata { added_at, opened_at }
}

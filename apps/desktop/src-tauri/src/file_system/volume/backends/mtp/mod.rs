//! MTP (Media Transfer Protocol) volume implementation.
//!
//! Wraps MTP device storage as a Volume, enabling MTP browsing through
//! the standard file listing pipeline (same icons, sorting, view modes as local files).
//!
//! Split by concern like its SMB sibling: this module holds `MtpVolume` itself
//! and its path conversions, with the `impl Volume` block in `volume_impl`, byte
//! movement in `streams`, error classification in `mapping`, cancellation
//! bridging in `cancel`, and copy scanning in `scan`.

mod cancel;
mod mapping;
mod scan;
mod streams;
mod volume_impl;

// Both re-exports exist for the sibling `mtp_test` module, which reaches them at
// `mtp::…` rather than through the private `streams` submodule. Each carries the
// gate its consumer there does, or `-D unused` fails the narrower build.
#[cfg(test)]
pub(super) use streams::volume_read_stream_to_chunk_stream;

#[cfg(all(test, feature = "virtual-mtp"))]
pub(super) use streams::test_window;

use super::{
    BatchScanResult, CopyScanResult, LaneKey, MutationEvent, ScanConflict, SourceItemInfo, SpaceInfo, Volume,
    VolumeError, VolumeReadStream, WatchCoverage,
};
use std::path::{Path, PathBuf};

/// A volume backed by an MTP device storage.
///
/// This implementation wraps the MTP connection manager to provide file system
/// abstraction. All methods are natively async: MTP operations go through the
/// connection manager which uses async USB bulk transfers.
pub struct MtpVolume {
    /// Display name (typically the storage description like "Internal storage")
    name: String,
    /// MTP device ID (for example, "mtp-20-5")
    pub(super) device_id: String,
    /// Storage ID within the device
    pub(super) storage_id: u32,
    /// Virtual root path for this volume (for example, "/mtp-20-5/65537")
    root: PathBuf,
    /// Volume ID for listing cache lookups (format: "{device_id}:{storage_id}").
    volume_id: String,
}

impl MtpVolume {
    /// Creates a new MTP volume for a specific device storage.
    ///
    /// # Arguments
    /// * `device_id` - The MTP device ID (format: "mtp-{bus}-{address}")
    /// * `storage_id` - The storage ID within the device
    /// * `name` - Display name for the storage (for example, "Internal shared storage")
    pub fn new(device_id: &str, storage_id: u32, name: &str) -> Self {
        let volume_id = format!("{}:{}", device_id, storage_id);
        Self {
            name: name.to_string(),
            device_id: device_id.to_string(),
            storage_id,
            root: PathBuf::from(format!("mtp://{}/{}", device_id, storage_id)),
            volume_id,
        }
    }

    /// Converts a Volume path to an MTP inner path.
    ///
    /// The path can be in several formats:
    /// - MTP URL: `mtp://mtp-0-1/65537` or `mtp://mtp-0-1/65537/DCIM/Camera`
    /// - Absolute path: `/DCIM/Camera`
    /// - Relative path: `DCIM/Camera`
    ///
    /// The MTP API expects paths relative to the storage root (for example, `DCIM/Camera`).
    pub(super) fn to_mtp_path(&self, path: &Path) -> String {
        let path_str = path.to_string_lossy();

        // Handle MTP URLs (mtp://device-id/storage-id/optional/path)
        if path_str.starts_with("mtp://") {
            // Parse: mtp://mtp-0-1/65537/DCIM/Camera -> DCIM/Camera
            // The format is: mtp://{device_id}/{storage_id}/{path}
            let without_scheme = path_str.strip_prefix("mtp://").unwrap_or(&path_str);

            // Find the device_id/storage_id prefix and skip it
            // Device ID format: mtp-{bus}-{address} (like mtp-0-1)
            // So we need to skip: device_id/storage_id/
            let parts: Vec<&str> = without_scheme.splitn(3, '/').collect();
            // parts[0] = device_id (like "mtp-0-1")
            // parts[1] = storage_id (like "65537")
            // parts[2] = inner path (like "DCIM/Camera") or absent for root

            return if parts.len() >= 3 {
                parts[2].to_string()
            } else {
                String::new() // Root of storage
            };
        }

        // Handle empty or root paths
        if path_str.is_empty() || path_str == "/" || path_str == "." {
            return String::new();
        }

        // Strip leading slash if present
        path_str.strip_prefix('/').unwrap_or(&path_str).to_string()
    }

    /// Normalizes any caller-supplied path on this volume to the canonical
    /// absolute MTP URL (`mtp://{device_id}/{storage_id}[/inner/path]`).
    ///
    /// `notify_mutation` passes this as the PARENT path to
    /// `notify_directory_changed`, which finds the target `LISTING_CACHE` entry by
    /// exact path equality against `CachedListing.path` — and that IS the absolute
    /// URL (pane navigation feeds the URL into the listing pipeline). Write/delete
    /// callers, however, may hand us a volume-relative path (e.g. `/file-a.txt`
    /// after the cross-volume copy orchestrator does `dest_path.join(name)` with
    /// `dest_path = "/"`); without this conversion the listing lookup misses and
    /// the cache patch is silently dropped, leaving the pane stale.
    ///
    /// Note the per-ENTRY paths INSIDE a listing are the storage-relative inner
    /// form (`/Documents/notes.txt`), NOT the URL — so the `Removed` patch matches
    /// entries by NAME, not full path (see `caching::remove_entry_by_name`).
    fn to_url_path(&self, path: &Path) -> PathBuf {
        let path_str = path.to_string_lossy();
        if path_str.starts_with("mtp://") {
            return path.to_path_buf();
        }
        let inner = self.to_mtp_path(path);
        if inner.is_empty() {
            self.root.clone()
        } else {
            self.root.join(inner)
        }
    }
}

/// Test-only call counter for `MtpVolume::list_directory`. The
/// `scan_for_copy_batch_with_boundary` integration tests assert "exactly 2
/// `list_directory` calls for 2 unique parents" without having to wrap the
/// volume (the override calls `self.list_directory` via static dispatch on
/// `MtpVolume`, so a wrapper Volume can't intercept it).
#[cfg(test)]
// Visible at `crate::file_system::volume::mtp_scan_oracle_tests`: those oracle
// tests live one level up (in `volume`), so they need this wider scope rather
// than a `pub(super)` that would only reach `backends`.
pub(in crate::file_system::volume) mod test_hooks {
    // The two readers below are called from the oracle tests one level up, which a
    // partial test build may not compile; `deny(unused)` flags them either way.
    #![allow(dead_code, reason = "read by the `mtp_scan_oracle_tests` module one level up")]

    use std::sync::atomic::{AtomicUsize, Ordering};

    static LIST_DIRECTORY_CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

    pub(super) fn bump_list_directory_call_count() {
        LIST_DIRECTORY_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    pub fn reset_list_directory_call_count() {
        LIST_DIRECTORY_CALL_COUNT.store(0, Ordering::Relaxed);
    }

    pub fn list_directory_call_count() -> usize {
        LIST_DIRECTORY_CALL_COUNT.load(Ordering::Relaxed)
    }
}

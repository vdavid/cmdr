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
/// The two instruments a test outside this backend reads it with.
#[cfg(any(test, feature = "testing"))]
pub mod testing;
mod volume_impl;

/// `MtpVolume`'s identity and the path conversions every operation starts with.
#[cfg(test)]
mod path_test;
/// The bounded-window read path, against a virtual device.
#[cfg(all(test, feature = "virtual-device"))]
mod read_range_test;

/// The `VolumeReadStream` → chunk-stream adapter the upload path feeds `mtp-rs`.
///
/// Published under the test gate for the cells that assert on its per-chunk
/// progress and its cancellation, which sit beside the app's transfer pipeline.
/// ❌ Not `cfg(test)` alone: that's set only while this crate builds its OWN test
/// target, so a consumer's test build would see it vanish.
#[cfg(any(test, feature = "testing"))]
pub use streams::volume_read_stream_to_chunk_stream;

use crate::connection::MtpConnectionManager;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A volume backed by an MTP device storage.
///
/// Every operation goes through the connection manager that attached this
/// storage, which is why the volume holds one rather than looking one up: the
/// manager owns the PTP session, the caches, and the per-device priority gate.
/// All methods are natively async, over async USB bulk transfers.
pub struct MtpVolume {
    /// Display name (typically the storage description like "Internal storage")
    name: String,
    /// The session layer this storage is served by.
    manager: Arc<MtpConnectionManager>,
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
    /// * `manager` - The session layer that holds this device's PTP session
    /// * `device_id` - The MTP device ID (format: "mtp-{bus}-{address}")
    /// * `storage_id` - The storage ID within the device
    /// * `name` - Display name for the storage (for example, "Internal shared storage")
    pub fn new(manager: Arc<MtpConnectionManager>, device_id: &str, storage_id: u32, name: &str) -> Self {
        let volume_id = format!("{}:{}", device_id, storage_id);
        Self {
            name: name.to_string(),
            manager,
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

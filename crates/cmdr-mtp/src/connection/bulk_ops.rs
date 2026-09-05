//! Bulk and recursive MTP operations (scan, recursive download/upload).

use log::debug;

use super::errors::MtpConnectionError;
use super::{MtpConnectionManager, normalize_mtp_path};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{CopyScanResult, ScanStop};

/// A scan the user stopped. ❗ The typed `Cancelled` variant, so `map_mtp_error`
/// turns it into `VolumeError::Cancelled` and every caller classifies on the
/// variant rather than on this sentence.
fn scan_cancelled(device_id: &str) -> MtpConnectionError {
    MtpConnectionError::Cancelled {
        device_id: device_id.to_string(),
        message: "Operation cancelled by user".to_string(),
    }
}

impl MtpConnectionManager {
    /// Scans an MTP path recursively to get statistics for a copy operation.
    ///
    /// # Arguments
    ///
    /// * `device_id` - The connected device ID
    /// * `storage_id` - The storage ID within the device
    /// * `path` - Virtual path on the device to scan
    ///
    /// # Returns
    ///
    /// Statistics including file count, directory count, and total bytes.
    pub async fn scan_for_copy(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
    ) -> Result<CopyScanResult, MtpConnectionError> {
        self.scan_for_copy_with_stop(device_id, storage_id, path, &ScanStop::none())
            .await
    }

    /// [`scan_for_copy`](Self::scan_for_copy) that a person can stop.
    ///
    /// ❗ The seam is one PTP listing wide: this walk's whole cost is one
    /// `list_directory` per directory over a USB pipe (~17 s for a 1,000-entry
    /// folder), so `stop` is consulted before each of them and before each entry.
    /// It cannot be finer — a listing in flight is inside `mtp-rs` and this layer
    /// has nothing to interrupt it with.
    pub(crate) async fn scan_for_copy_with_stop(
        &self,
        device_id: &str,
        storage_id: u32,
        path: &str,
        stop: &ScanStop,
    ) -> Result<CopyScanResult, MtpConnectionError> {
        debug!(
            "MTP scan_for_copy: device={}, storage={}, path={}",
            device_id, storage_id, path
        );

        if stop.should_stop().await {
            return Err(scan_cancelled(device_id));
        }

        // Try to list the path as a directory
        match self.list_directory(device_id, storage_id, path).await {
            Ok(entries) if !entries.is_empty() => {
                // Directory with contents: recurse using entries directly
                let mut result = self
                    .scan_entries_recursive(device_id, storage_id, entries, stop)
                    .await?;
                result.top_level_is_directory = true;
                Ok(result)
            }
            Ok(_) => {
                // Empty result: either an empty directory or a file (some MTP devices
                // return empty for files instead of an error). Check parent to disambiguate.
                if let Some(result) = self.try_scan_as_file(device_id, storage_id, path).await {
                    return Ok(result);
                }
                // Empty directory
                Ok(CopyScanResult {
                    file_count: 0,
                    dir_count: 1,
                    total_bytes: 0,
                    // MTP has no hardlinks: source footprint == write footprint.
                    dedup_bytes: 0,
                    top_level_is_directory: true,
                })
            }
            Err(e) => {
                // list_directory failed, likely because path is a file, not a directory.
                debug!(
                    "MTP scan_for_copy: list_directory failed for '{}', checking if it's a file: {:?}",
                    path, e
                );
                if let Some(result) = self.try_scan_as_file(device_id, storage_id, path).await {
                    return Ok(result);
                }
                Err(e)
            }
        }
    }

    /// Recursively accumulates scan stats from a list of directory entries.
    ///
    /// Unlike `scan_for_copy`, this function takes entries that are already known
    /// from a parent listing. For file entries, it counts them directly without
    /// any USB calls. For directory entries, it lists their contents (one USB call
    /// per directory) and recurses.
    ///
    /// This ensures exactly one `list_directory` call per directory in the tree,
    /// with zero calls for files.
    async fn scan_entries_recursive(
        &self,
        device_id: &str,
        storage_id: u32,
        entries: Vec<FileEntry>,
        stop: &ScanStop,
    ) -> Result<CopyScanResult, MtpConnectionError> {
        let mut file_count = 0usize;
        let mut dir_count = 0usize;
        let mut total_bytes = 0u64;

        for entry in &entries {
            // Per entry, and so BEFORE the subdirectory listing below: that
            // listing is the round trip, and asking after it is a Cancel the
            // person waits out.
            if stop.should_stop().await {
                return Err(scan_cancelled(device_id));
            }
            if entry.is_directory {
                dir_count += 1;
                // One list_directory call per subdirectory
                let children = self.list_directory(device_id, storage_id, &entry.path).await?;
                if !children.is_empty() {
                    let child_result =
                        Box::pin(self.scan_entries_recursive(device_id, storage_id, children, stop)).await?;
                    file_count += child_result.file_count;
                    dir_count += child_result.dir_count;
                    total_bytes += child_result.total_bytes;
                }
            } else {
                file_count += 1;
                total_bytes += entry.size.unwrap_or(0);
            }
        }

        debug!(
            "MTP scan_entries_recursive: {} files, {} dirs, {} bytes",
            file_count, dir_count, total_bytes
        );

        Ok(CopyScanResult {
            file_count,
            dir_count,
            total_bytes,
            // MTP has no hardlinks: source footprint == write footprint.
            dedup_bytes: total_bytes,
            // `scan_entries_recursive` is only called on known-directory input
            // (caller already listed the path). Setting `true` keeps downstream
            // callers from re-issuing a type probe.
            top_level_is_directory: true,
        })
    }

    /// Helper to check if a path is a file by listing its parent directory.
    /// Returns Some(CopyScanResult) if path is a file, None otherwise.
    async fn try_scan_as_file(&self, device_id: &str, storage_id: u32, path: &str) -> Option<CopyScanResult> {
        let path_buf = normalize_mtp_path(path);
        let parent = path_buf.parent()?;
        let name = path_buf.file_name()?.to_str()?;

        let parent_entries = self
            .list_directory(device_id, storage_id, &parent.to_string_lossy())
            .await
            .ok()?;

        let entry = parent_entries.iter().find(|e| e.name == name)?;

        if entry.is_directory {
            // It's a directory, not a file. Let caller handle it.
            return None;
        }

        debug!(
            "MTP scan_for_copy: path '{}' is a file with size {}",
            path,
            entry.size.unwrap_or(0)
        );

        Some(CopyScanResult {
            file_count: 1,
            dir_count: 0,
            total_bytes: entry.size.unwrap_or(0),
            // MTP has no hardlinks: source footprint == write footprint.
            dedup_bytes: entry.size.unwrap_or(0),
            top_level_is_directory: false,
        })
    }
}

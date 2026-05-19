//! MTP (Media Transfer Protocol) volume implementation.
//!
//! Wraps MTP device storage as a Volume, enabling MTP browsing through
//! the standard file listing pipeline (same icons, sorting, view modes as local files).

use super::{
    BatchScanResult, CopyScanResult, MutationEvent, ScanConflict, SourceItemInfo, SpaceInfo, Volume, VolumeError,
    VolumeReadStream,
};
use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::try_get_watched_listing;
use crate::mtp::connection::{MtpConnectionError, connection_manager};
use log::debug;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

/// A volume backed by an MTP device storage.
///
/// This implementation wraps the MTP connection manager to provide file system
/// abstraction. All methods are natively async: MTP operations go through the
/// connection manager which uses async USB bulk transfers.
pub struct MtpVolume {
    /// Display name (typically the storage description like "Internal storage")
    name: String,
    /// MTP device ID (for example, "mtp-20-5")
    device_id: String,
    /// Storage ID within the device
    storage_id: u32,
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
    fn to_mtp_path(&self, path: &Path) -> String {
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
}

impl Volume for MtpVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_directory_with_cancel(path, on_progress, None)
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
        cancel: Option<&'a std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            #[cfg(test)]
            test_hooks::bump_list_directory_call_count();

            let mtp_path = self.to_mtp_path(path);

            // Build a mtp_rs CancelToken that shares the caller's Arc<AtomicBool>.
            // No second polling task: flipping the original atomic flips the token.
            let cancel_token = cancel.map(|c| mtp_rs::CancelToken::from_arc(std::sync::Arc::clone(c)));
            let cancel_ref = cancel_token.as_ref();

            debug!(
                "MtpVolume::list_directory: device={}, storage={}, input_path={}, mtp_path={}, cancel={}",
                self.device_id,
                self.storage_id,
                path.display(),
                mtp_path,
                cancel_ref.is_some()
            );

            let start = std::time::Instant::now();
            let result = if let Some(on_progress) = on_progress {
                connection_manager()
                    .list_directory_with_progress_and_cancel(
                        &self.device_id,
                        self.storage_id,
                        &mtp_path,
                        on_progress,
                        cancel_ref,
                    )
                    .await
            } else {
                connection_manager()
                    .list_directory_with_cancel(&self.device_id, self.storage_id, &mtp_path, cancel_ref)
                    .await
            };

            match &result {
                Ok(entries) => debug!(
                    "MtpVolume::list_directory: completed in {:?}, {} entries",
                    start.elapsed(),
                    entries.len()
                ),
                Err(e) => debug!(
                    "MtpVolume::list_directory: failed in {:?}, error={:?}",
                    start.elapsed(),
                    e
                ),
            }

            result.map_err(map_mtp_error)
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // MTP has no single-file stat: list the parent directory and find the entry.
            let path_str = path.to_string_lossy();
            if path_str.is_empty() || path_str == "/" || path_str == "." {
                // Root: synthesize a directory entry
                return Ok(FileEntry::new(
                    self.name.clone(),
                    self.root.display().to_string(),
                    true,
                    false,
                ));
            }

            let Some(parent) = path.parent() else {
                return Ok(FileEntry::new(
                    self.name.clone(),
                    self.root.display().to_string(),
                    true,
                    false,
                ));
            };

            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return Err(VolumeError::NotFound(path.display().to_string()));
            };

            let entries = self.list_directory(parent, None).await?;
            entries
                .into_iter()
                .find(|e| e.name == name)
                .ok_or_else(|| VolumeError::NotFound(path.display().to_string()))
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.is_ok() })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.map(|e| e.is_directory) })
    }

    fn supports_watching(&self) -> bool {
        // Return false because MTP has its OWN file watching mechanism that is
        // independent of the listing pipeline. The MtpConnectionManager starts an
        // event loop when a device connects (see start_event_loop) that polls for
        // USB interrupt endpoint events (ObjectAdded/ObjectRemoved/ObjectInfoChanged).
        // These events emit `mtp-directory-changed` directly to the frontend.
        //
        // The `supports_watching()` check in operations.rs is used to decide whether
        // to start the local notify-based file watcher, which only works for POSIX
        // paths. MTP paths like "/DCIM/Camera" don't exist on the local filesystem,
        // so we must return false to prevent the notify watcher from failing.
        false
    }

    fn supports_local_fs_access(&self) -> bool {
        false
    }

    fn listing_is_watched(&self, _path: &Path) -> bool {
        // MTP "watching" is volume-level, not path-level. The MTP event loop is
        // per-device and would report any changes the device emits to any path.
        // So as long as the device is connected, treat every cached listing on
        // this volume as oracle-eligible. Caveat: many MTP devices (cameras
        // especially) never emit per-object events, so `true` means only "the
        // device is reachable and would forward changes if it sent any".
        connection_manager().is_connected(&self.device_id)
    }

    fn notify_mutation<'a>(
        &'a self,
        _volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};

            // MTP's get_metadata lists the parent dir to find the entry, which is expensive
            // but correct. The MTP event loop (connection/event_loop.rs) also handles
            // change notifications, so this is belt-and-suspenders for self-mutations.
            match mutation {
                MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                    let entry_path = parent_path.join(name);
                    match self.get_metadata(&entry_path).await {
                        Ok(entry) => {
                            let change = if matches!(mutation, MutationEvent::Created(_)) {
                                DirectoryChange::Added(entry)
                            } else {
                                DirectoryChange::Modified(entry)
                            };
                            notify_directory_changed(&self.volume_id, parent_path, change);
                        }
                        Err(e) => {
                            debug!(
                                "MtpVolume::notify_mutation: couldn't stat {}: {}",
                                entry_path.display(),
                                e
                            );
                        }
                    }
                }
                MutationEvent::Deleted(name) => {
                    notify_directory_changed(&self.volume_id, parent_path, DirectoryChange::Removed(name));
                }
                MutationEvent::Renamed { from, to } => {
                    let new_path = parent_path.join(&to);
                    match self.get_metadata(&new_path).await {
                        Ok(entry) => {
                            notify_directory_changed(
                                &self.volume_id,
                                parent_path,
                                DirectoryChange::Renamed {
                                    old_name: from,
                                    new_entry: entry,
                                },
                            );
                        }
                        Err(e) => {
                            debug!(
                                "MtpVolume::notify_mutation: couldn't stat renamed entry {}: {}",
                                new_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        })
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let Some(parent) = path.parent() else {
                return Err(VolumeError::IoError {
                    message: "Cannot create root directory".into(),
                    raw_os_error: None,
                });
            };
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                return Err(VolumeError::IoError {
                    message: "Invalid directory name".into(),
                    raw_os_error: None,
                });
            };

            let parent_mtp_path = self.to_mtp_path(parent);
            let folder_name = name.to_string();

            connection_manager()
                .create_folder(&self.device_id, self.storage_id, &parent_mtp_path, &folder_name)
                .await
                .map(|_| ())
                .map_err(map_mtp_error)?;

            self.notify_mutation(&self.volume_id, parent, MutationEvent::Created(name.to_string()))
                .await;
            Ok(())
        })
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.delete_with_cancel(path, None)
    }

    fn delete_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            let cancel_token = cancel.map(|c| mtp_rs::CancelToken::from_arc(std::sync::Arc::clone(c)));
            let cancel_ref = cancel_token.as_ref();

            connection_manager()
                .delete_object_with_cancel(&self.device_id, self.storage_id, &mtp_path, cancel_ref)
                .await
                .map_err(map_mtp_error)?;

            if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                self.notify_mutation(
                    &self.volume_id,
                    parent,
                    MutationEvent::Deleted(name.to_string_lossy().to_string()),
                )
                .await;
            }
            Ok(())
        })
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // MTP doesn't support atomic overwrite, so check for conflicts when not forced.
            if !force && self.exists(to).await {
                return Err(VolumeError::AlreadyExists(to.display().to_string()));
            }

            let from_mtp = self.to_mtp_path(from);
            let to_mtp = self.to_mtp_path(to);

            let from_parent = Path::new(&from_mtp).parent().unwrap_or(Path::new(""));
            let to_parent = Path::new(&to_mtp).parent().unwrap_or(Path::new(""));
            let same_parent = from_parent == to_parent;

            let from_name = Path::new(&from_mtp)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| VolumeError::IoError {
                    message: "Invalid source path".into(),
                    raw_os_error: None,
                })?;
            let to_name =
                Path::new(&to_mtp)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| VolumeError::IoError {
                        message: "Invalid destination path".into(),
                        raw_os_error: None,
                    })?;
            let same_name = from_name == to_name;

            if same_parent {
                // Same directory: just rename
                let new_name = to_name.to_string();
                connection_manager()
                    .rename_object(&self.device_id, self.storage_id, &from_mtp, &new_name)
                    .await
                    .map(|_| ())
                    .map_err(map_mtp_error)?;

                // Notify listing cache about same-directory rename
                if let Some(from_parent_path) = from.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        from_parent_path,
                        MutationEvent::Renamed {
                            from: from_name.to_string(),
                            to: to_name.to_string(),
                        },
                    )
                    .await;
                }
            } else {
                // Different directory: use MTP MoveObject
                let to_parent_str = to_parent.to_string_lossy().to_string();
                connection_manager()
                    .move_object(&self.device_id, self.storage_id, &from_mtp, &to_parent_str)
                    .await
                    .map(|_| ())
                    .map_err(map_mtp_error)?;

                // If the name also changed, rename after moving
                if !same_name {
                    let moved_path = format!(
                        "{}{}{}",
                        to_parent_str,
                        if to_parent_str.is_empty() || to_parent_str.ends_with('/') {
                            ""
                        } else {
                            "/"
                        },
                        from_name
                    );
                    let new_name = to_name.to_string();
                    connection_manager()
                        .rename_object(&self.device_id, self.storage_id, &moved_path, &new_name)
                        .await
                        .map(|_| ())
                        .map_err(map_mtp_error)?;
                }

                // Cross-directory move: remove from source dir, add in dest dir
                if let Some(from_parent_path) = from.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        from_parent_path,
                        MutationEvent::Deleted(from_name.to_string()),
                    )
                    .await;
                }
                if let Some(to_parent_path) = to.parent() {
                    self.notify_mutation(
                        &self.volume_id,
                        to_parent_path,
                        MutationEvent::Created(to_name.to_string()),
                    )
                    .await;
                }
            }
            Ok(())
        })
    }

    fn supports_export(&self) -> bool {
        true
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            debug!(
                "MtpVolume::scan_for_copy: device={}, storage={}, path={}",
                self.device_id, self.storage_id, mtp_path
            );

            connection_manager()
                .scan_for_copy(&self.device_id, self.storage_id, &mtp_path)
                .await
                .map_err(map_mtp_error)
        })
    }

    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_with_progress(paths, None)
    }

    /// Batch scan with parent-grouping + fresh-listing oracle.
    ///
    /// Decision flow:
    /// 1. Group selected paths by their parent directory (one MTP listing per parent on the cold
    ///    path is the load-bearing optimization: selecting 135 photos in `/DCIM/Camera` should
    ///    produce ONE `list_directory` call, not 135 `get_metadata` calls each of which lists the
    ///    parent).
    /// 2. For each unique parent, ask `try_get_watched_listing(volume_id, parent)` first. On hit,
    ///    every child entry's size + `is_directory` comes from the cached `FileEntry`, no MTP I/O.
    ///    On miss, fall through to the existing single `list_directory(parent)` per group.
    ///
    /// The oracle decision is per-parent: different parents in the same call
    /// can resolve different ways (one watched, one cold). On oracle hit no
    /// `list_directory_with_progress` callbacks fire for that parent, so the
    /// FE's scan-preview counter doesn't tick for those entries; the final
    /// `BatchScanResult.aggregate` still reflects them.
    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if paths.is_empty() {
                return Ok(BatchScanResult {
                    aggregate: CopyScanResult {
                        file_count: 0,
                        dir_count: 0,
                        total_bytes: 0,
                        top_level_is_directory: false,
                    },
                    per_path: Vec::new(),
                });
            }

            // Group paths by parent. Two keys per group:
            //   - `original_parent`: the path the FE/cache uses as a listing key (typically
            //     `/DCIM/Camera`-style absolute). This is what the oracle is looked up against.
            //   - `mtp_parent`: the MTP-relative form used by `list_directory` on the cold-cache fallthrough.
            //     Stored so we don't call `to_mtp_path` twice per group.
            #[derive(Default)]
            struct ParentGroup<'p> {
                original_parent: PathBuf,
                mtp_parent: String,
                children: Vec<&'p PathBuf>,
            }
            let mut groups: std::collections::HashMap<PathBuf, ParentGroup<'a>> = std::collections::HashMap::new();
            for path in paths {
                let original_parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
                let entry = groups.entry(original_parent.clone()).or_insert_with(|| {
                    let mtp_path = self.to_mtp_path(path);
                    let mtp_path_buf = PathBuf::from(&mtp_path);
                    let mtp_parent = mtp_path_buf
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ParentGroup {
                        original_parent,
                        mtp_parent,
                        children: Vec::new(),
                    }
                });
                entry.children.push(path);
            }

            debug!(
                "MtpVolume::scan_for_copy_batch: {} paths across {} unique parent dirs",
                paths.len(),
                groups.len()
            );

            // Stage per-path results in a map so the final per_path vec
            // preserves the caller's input order.
            let mut per_path_results: std::collections::HashMap<PathBuf, CopyScanResult> =
                std::collections::HashMap::with_capacity(paths.len());

            let mut aggregate = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                // Aggregate over multiple paths: not meaningful for a batch.
                top_level_is_directory: false,
            };

            for group in groups.values() {
                // Oracle short-circuit: if the parent is watcher-fresh, use
                // the cached listing instead of touching the device. The
                // freshness contract for MTP is volume-level: when this
                // returns `Some`, the device is connected and would forward
                // any change events it sends.
                let cached = try_get_watched_listing(&self.volume_id, &group.original_parent);

                // List the parent directory once on cold cache (goes through
                // the listing cache). The MTP listing is what dominates
                // wall-clock on a cold cache (17 s for 1047 entries via USB),
                // so forward `on_progress` to `list_directory_with_progress`
                // (via the trait method) so the scan-preview dialog sees a
                // climbing file count instead of a frozen 0/0/0 spinner. On
                // an oracle hit there's no list, so no progress ticks fire
                // for this parent's children — the final aggregate still
                // includes them.
                let entries = match cached {
                    Some(entries) => {
                        debug!(
                            "MtpVolume::scan_for_copy_batch: oracle hit for parent {} ({} cached entries, {} selected children)",
                            group.original_parent.display(),
                            entries.len(),
                            group.children.len()
                        );
                        entries
                    }
                    None => self.list_directory(Path::new(&group.mtp_parent), on_progress).await?,
                };

                // Index entries by name so each child lookup is O(1). A naive
                // `entries.iter().find(...)` per child is O(n) and the outer
                // loop is also O(n), so 15k photos in /DCIM/Camera turned a
                // single parent listing into ~225M string comparisons (~10 s
                // stall in the scan preview).
                let entries_by_name: std::collections::HashMap<&str, &FileEntry> =
                    entries.iter().map(|e| (e.name.as_str(), e)).collect();

                for child_path in &group.children {
                    let mtp_path = self.to_mtp_path(child_path);
                    let name = Path::new(&mtp_path).file_name().and_then(|n| n.to_str()).unwrap_or("");

                    if let Some(entry) = entries_by_name.get(name).copied() {
                        if entry.is_directory {
                            let scan = self.scan_for_copy(child_path).await?;
                            aggregate.file_count += scan.file_count;
                            aggregate.dir_count += scan.dir_count;
                            aggregate.total_bytes += scan.total_bytes;
                            per_path_results.insert((*child_path).clone(), scan);
                        } else {
                            let size = entry.size.unwrap_or(0);
                            aggregate.file_count += 1;
                            aggregate.total_bytes += size;
                            per_path_results.insert(
                                (*child_path).clone(),
                                CopyScanResult {
                                    file_count: 1,
                                    dir_count: 0,
                                    total_bytes: size,
                                    top_level_is_directory: false,
                                },
                            );
                        }
                    }
                }
            }

            let per_path = paths
                .iter()
                .filter_map(|p| per_path_results.remove(p).map(|r| (p.clone(), r)))
                .collect();

            Ok(BatchScanResult { aggregate, per_path })
        })
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // List destination directory to check for conflicts
            let entries = self.list_directory(dest_path, None).await?;
            let mut conflicts = Vec::new();

            for item in source_items {
                // Check if a file with the same name exists at destination
                if let Some(existing) = entries.iter().find(|e| e.name == item.name) {
                    let dest_modified = existing.modified_at.map(|s| s as i64);
                    conflicts.push(ScanConflict {
                        source_path: item.name.clone(),
                        dest_path: existing.path.clone(),
                        source_size: item.size,
                        dest_size: existing.size.unwrap_or(0),
                        source_modified: item.modified,
                        dest_modified,
                    });
                }
            }

            Ok(conflicts)
        })
    }

    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs(5))
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let info = connection_manager()
                .get_device_info(&self.device_id)
                .await
                .ok_or_else(|| {
                    map_mtp_error(MtpConnectionError::NotConnected {
                        device_id: self.device_id.clone(),
                    })
                })?;

            // Find this storage in the device info
            let storage = info.storages.iter().find(|s| s.id == self.storage_id).ok_or_else(|| {
                map_mtp_error(MtpConnectionError::Other {
                    device_id: self.device_id.clone(),
                    message: format!("Storage {} not found", self.storage_id),
                })
            })?;

            Ok(SpaceInfo {
                total_bytes: storage.total_bytes,
                available_bytes: storage.available_bytes,
                used_bytes: storage.total_bytes.saturating_sub(storage.available_bytes),
            })
        })
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn max_concurrent_ops(&self) -> usize {
        // MTP is a single USB bulk transport, so parallel ops would just
        // serialize on the wire with extra overhead.
        1
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            let (download, total_size) = connection_manager()
                .open_download_stream(&self.device_id, self.storage_id, &mtp_path)
                .await
                .map_err(map_mtp_error)?;

            Ok(Box::new(MtpReadStream {
                download: Some(download),
                total_size,
                bytes_read: 0,
            }) as Box<dyn VolumeReadStream>)
        })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        mut stream: Box<dyn VolumeReadStream>,
        _on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let dest_folder = dest.parent().map(|p| self.to_mtp_path(p)).unwrap_or_default();
            let filename = dest
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| VolumeError::IoError {
                    message: "Invalid filename".into(),
                    raw_os_error: None,
                })?
                .to_string();

            // Stream chunks directly with .await (no need to pre-collect; we're
            // fully async now, no nested block_on risk).
            let mut chunks: Vec<bytes::Bytes> = Vec::new();
            while let Some(result) = stream.next_chunk().await {
                let data = result?;
                chunks.push(bytes::Bytes::from(data));
            }

            let bytes_written = connection_manager()
                .upload_from_chunks(&self.device_id, self.storage_id, &dest_folder, &filename, size, chunks)
                .await
                .map_err(map_mtp_error)?;

            // Patch the listing cache from local knowledge so the destination
            // pane sees the new file immediately. The MTP USB event loop is
            // unreliable for self-mutations (many devices emit no events at
            // all), so without this the cache would only catch up on the
            // next manual refresh.
            if let Some(parent) = dest.parent() {
                self.notify_mutation(&self.volume_id, parent, MutationEvent::Created(filename))
                    .await;
            }
            Ok(bytes_written)
        })
    }
}

/// Direct async streaming reader for MTP files.
///
/// Calls `FileDownload::next_chunk().await` directly (`VolumeReadStream::next_chunk()`
/// is async, so no background task or channel needed).
struct MtpReadStream {
    download: Option<mtp_rs::FileDownload>,
    total_size: u64,
    bytes_read: u64,
}

impl Drop for MtpReadStream {
    fn drop(&mut self) {
        if let Some(mut download) = self.download.take() {
            // Not fully consumed: cancel the USB transfer to prevent
            // ReceiveStream's Drop from panicking (and corrupting the session).
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let timeout = mtp_rs::DEFAULT_CANCEL_TIMEOUT;
                    if let Err(e) = download.cancel(timeout).await {
                        log::warn!("MTP download cancel on drop: {:?}", e);
                    }
                });
            }
        }
    }
}

impl VolumeReadStream for MtpReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            let download = self.download.as_mut()?;
            match download.next_chunk().await {
                Some(Ok(bytes)) => {
                    self.bytes_read += bytes.len() as u64;
                    Some(Ok(bytes.to_vec()))
                }
                Some(Err(e)) => Some(Err(VolumeError::IoError {
                    message: e.to_string(),
                    raw_os_error: None,
                })),
                None => None,
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.total_size
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}

/// Maps MTP connection errors to Volume errors.
fn map_mtp_error(e: MtpConnectionError) -> VolumeError {
    match e {
        MtpConnectionError::DeviceNotFound { .. } | MtpConnectionError::NotConnected { .. } => {
            VolumeError::NotFound(e.to_string())
        }
        MtpConnectionError::ObjectNotFound { path, .. } => VolumeError::NotFound(path),
        MtpConnectionError::ExclusiveAccess { .. } | MtpConnectionError::PermissionDenied { .. } => {
            VolumeError::PermissionDenied(e.to_string())
        }
        MtpConnectionError::Cancelled { .. } => VolumeError::Cancelled(e.to_string()),
        MtpConnectionError::Disconnected { .. } => VolumeError::DeviceDisconnected(e.to_string()),
        MtpConnectionError::Timeout { .. } => VolumeError::ConnectionTimeout(e.to_string()),
        MtpConnectionError::StorageFull { .. } => VolumeError::StorageFull { message: e.to_string() },
        MtpConnectionError::StoreReadOnly { .. } => VolumeError::ReadOnly(e.to_string()),
        _ => VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: None,
        },
    }
}

/// Test-only call counter for `MtpVolume::list_directory`. The
/// `scan_for_copy_batch_with_progress` integration tests assert "exactly 2
/// `list_directory` calls for 2 unique parents" without having to wrap the
/// volume (the override calls `self.list_directory` via static dispatch on
/// `MtpVolume`, so a wrapper Volume can't intercept it).
#[cfg(test)]
pub(super) mod test_hooks {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_volume() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Internal storage");
        assert_eq!(vol.name(), "Internal storage");
        assert_eq!(vol.device_id, "mtp-20-5");
        assert_eq!(vol.storage_id, 65537);
    }

    #[test]
    fn test_root_path() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Internal storage");
        assert_eq!(vol.root().to_string_lossy(), "mtp://mtp-20-5/65537");
    }

    #[test]
    fn test_to_mtp_path_empty() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert_eq!(vol.to_mtp_path(Path::new("")), "");
        assert_eq!(vol.to_mtp_path(Path::new("/")), "");
        assert_eq!(vol.to_mtp_path(Path::new(".")), "");
    }

    #[test]
    fn test_to_mtp_path_relative() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert_eq!(vol.to_mtp_path(Path::new("DCIM")), "DCIM");
        assert_eq!(vol.to_mtp_path(Path::new("DCIM/Camera")), "DCIM/Camera");
    }

    #[test]
    fn test_to_mtp_path_absolute() {
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert_eq!(vol.to_mtp_path(Path::new("/DCIM")), "DCIM");
        assert_eq!(vol.to_mtp_path(Path::new("/DCIM/Camera")), "DCIM/Camera");
    }

    #[test]
    fn test_to_mtp_path_mtp_url_root() {
        let vol = MtpVolume::new("mtp-0-1", 65537, "Test");
        // MTP URL for storage root
        assert_eq!(vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537")), "");
    }

    #[test]
    fn test_to_mtp_path_mtp_url_with_path() {
        let vol = MtpVolume::new("mtp-0-1", 65537, "Test");
        // MTP URL with nested path
        assert_eq!(vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537/DCIM")), "DCIM");
        assert_eq!(
            vol.to_mtp_path(Path::new("mtp://mtp-0-1/65537/DCIM/Camera")),
            "DCIM/Camera"
        );
    }

    #[test]
    fn test_supports_watching_returns_false() {
        // MTP volumes return false for supports_watching because they have their
        // own event loop (in MtpConnectionManager) that handles file watching
        // independently. The supports_watching check in operations.rs is only
        // for the local notify-based watcher, which doesn't work for MTP paths.
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert!(!vol.supports_watching());
    }

    #[test]
    fn test_supports_streaming_returns_true() {
        // MTP volumes support streaming for direct MTP-to-MTP transfers.
        let vol = MtpVolume::new("mtp-20-5", 65537, "Test");
        assert!(vol.supports_streaming());
    }

    #[test]
    fn test_listing_is_watched_false_when_device_not_connected() {
        // Without `virtual-mtp`, we can still assert the negative case: a freshly
        // created `MtpVolume` whose device_id was never connected returns false.
        let vol = MtpVolume::new("mtp-never-connected-9999", 65537, "Test");
        assert!(!vol.listing_is_watched(Path::new("/DCIM")));
    }

    /// Connects to a virtual MTP device, asserts the oracle gate flips true, then
    /// disconnects and asserts it flips false. Requires the `virtual-mtp` feature.
    #[cfg(feature = "virtual-mtp")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_listing_is_watched_flips_with_connection() {
        use crate::mtp::virtual_device::setup_virtual_mtp_device;

        // Register a virtual device backed by a tmp dir.
        let location_id = setup_virtual_mtp_device();
        let device_id = format!("mtp-{}", location_id);

        // Before connect: false.
        let vol = MtpVolume::new(&device_id, 65537, "Test");
        assert!(!vol.listing_is_watched(Path::new("/")), "expected false before connect");

        // Connect, then assert true.
        let info = connection_manager()
            .connect(&device_id, None)
            .await
            .expect("virtual-mtp connect should succeed");
        // Use whatever storage_id the virtual device reported (we don't care
        // which storage; the gate is volume-level).
        let storage_id = info.storages.first().expect("virtual device should have storages").id;
        let vol = MtpVolume::new(&device_id, storage_id, "Test");
        assert!(vol.listing_is_watched(Path::new("/")), "expected true once connected");

        // Disconnect, then assert false again.
        connection_manager()
            .disconnect(&device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
            .await
            .expect("virtual-mtp disconnect should succeed");
        assert!(
            !vol.listing_is_watched(Path::new("/")),
            "expected false after disconnect"
        );
    }
}

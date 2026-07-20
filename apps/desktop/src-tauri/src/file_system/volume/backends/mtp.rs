//! MTP (Media Transfer Protocol) volume implementation.
//!
//! Wraps MTP device storage as a Volume, enabling MTP browsing through
//! the standard file listing pipeline (same icons, sorting, view modes as local files).

use super::{
    BatchScanResult, CopyScanResult, LaneKey, MutationEvent, ScanConflict, SourceItemInfo, SpaceInfo, Volume,
    VolumeError, VolumeReadStream,
};
use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::try_get_watched_listing;
use crate::mtp::connection::{MtpConnectionError, MtpReadSession, connection_manager};
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

impl Volume for MtpVolume {
    fn name(&self) -> &str {
        &self.name
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn lane_key(&self) -> LaneKey {
        // One USB pipe per device: every storage on a device shares its lane,
        // so two transfers to the same phone serialize. Key by `device_id`
        // (not `volume_id`, which is per-storage) so they collapse to one lane.
        LaneKey::new(self.device_id.clone())
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_directory_with_cancel(path, on_progress, None)
    }

    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
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

    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);
            let cancel_token = cancel.map(|c| mtp_rs::CancelToken::from_arc(std::sync::Arc::clone(c)));

            // The per-unit, foreground-yielding scan listing: never holds the USB
            // pipe across the whole folder, so a background scan can't starve
            // foreground nav/copy/delete. See `mtp/connection/directory_ops.rs`.
            connection_manager()
                .list_directory_for_scan(&self.device_id, self.storage_id, &mtp_path, cancel_token.as_ref())
                .await
                .map_err(map_mtp_error)
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

            // Normalize once so every branch (incl. get_metadata's parent
            // lookup) sees the canonical absolute MTP URL. Callers happily
            // hand us either form depending on which layer they came from.
            let parent_url = self.to_url_path(parent_path);
            let parent_ref = parent_url.as_path();
            // MTP's get_metadata lists the parent dir to find the entry, which is expensive
            // but correct. The MTP event loop (connection/event_loop.rs) also handles
            // change notifications, so this is belt-and-suspenders for self-mutations.
            match mutation {
                MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                    let entry_path = parent_ref.join(name);
                    match self.get_metadata(&entry_path).await {
                        Ok(entry) => {
                            let change = if matches!(mutation, MutationEvent::Created(_)) {
                                DirectoryChange::Added(entry)
                            } else {
                                DirectoryChange::Modified(entry)
                            };
                            notify_directory_changed(&self.volume_id, parent_ref, change);
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
                    notify_directory_changed(&self.volume_id, parent_ref, DirectoryChange::Removed(name));
                }
                MutationEvent::Renamed { from, to } => {
                    let new_path = parent_ref.join(&to);
                    match self.get_metadata(&new_path).await {
                        Ok(entry) => {
                            notify_directory_changed(
                                &self.volume_id,
                                parent_ref,
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

    /// MTP `create_directory` does NOT error on an existing same-name dir — the
    /// protocol allows same-name sibling objects, so `create_folder` would make a
    /// duplicate. The folder-merge walker pre-checks existence on MTP instead of
    /// trusting `create_directory` to surface `AlreadyExists`.
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        false
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
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if paths.is_empty() {
                return Ok(BatchScanResult {
                    aggregate: CopyScanResult {
                        file_count: 0,
                        dir_count: 0,
                        total_bytes: 0,
                        dedup_bytes: 0,
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
                // MTP has no hardlinks: source footprint == write footprint.
                dedup_bytes: 0,
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
                            aggregate.dedup_bytes += scan.dedup_bytes;
                            per_path_results.insert((*child_path).clone(), scan);
                        } else {
                            let size = entry.size.unwrap_or(0);
                            aggregate.file_count += 1;
                            aggregate.total_bytes += size;
                            aggregate.dedup_bytes += size;
                            per_path_results.insert(
                                (*child_path).clone(),
                                CopyScanResult {
                                    file_count: 1,
                                    dir_count: 0,
                                    total_bytes: size,
                                    dedup_bytes: size,
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
                        source_is_directory: item.is_directory,
                        dest_is_directory: existing.is_directory,
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
        self.open_read_stream_at_offset(path, 0)
    }

    // Reads in bounded windows from `offset` forward. Since the copy path
    // (`CheckpointStream`) parks in place between windows rather than releasing +
    // reopening, NOTHING calls this with a non-zero offset anymore; it's reached
    // only via `open_read_stream`'s `offset == 0`. Keep the offset parameter
    // (the resumable primitive is correct and cheap) — don't "clean it up" as
    // dead just because the non-zero path currently has no caller.
    fn open_read_stream_at_offset<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mtp_path = self.to_mtp_path(path);

            // The window size (`mtp_read_window()`, shrinkable in tests) and the
            // start `offset` (non-zero resumes a reopened read; see
            // `CheckpointStream`) are baked into the `WindowedDownload` here.
            let session = connection_manager()
                .open_read_session(&self.device_id, self.storage_id, &mtp_path, offset, mtp_read_window())
                .await
                .map_err(map_mtp_error)?;

            Ok(Box::new(MtpReadStream {
                session,
                device_id: self.device_id.clone(),
            }) as Box<dyn VolumeReadStream>)
        })
    }

    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if len == 0 {
                return Ok(Vec::new());
            }
            let mtp_path = self.to_mtp_path(path);
            // ONE `GetPartialObject64` per call, straight to the device: no
            // session, no `GetStorageInfo`, no `GetObjectInfo`. A bounded read
            // discards everything those two round trips would produce, and the
            // archive extraction loop issues one of these per 256 KiB, so the
            // saving is the whole extraction. (`open_read_session` stays the
            // right shape for a streaming copy, where one `GetObjectInfo`
            // amortizes over hundreds of windows and anchors progress.)
            let window = u32::try_from(len).unwrap_or(u32::MAX);
            let mut out = Vec::with_capacity(len.min(window as usize));
            while out.len() < len {
                let remaining = u32::try_from(len - out.len()).unwrap_or(u32::MAX);
                let chunk = connection_manager()
                    .read_range_direct(
                        &self.device_id,
                        self.storage_id,
                        &mtp_path,
                        offset + out.len() as u64,
                        remaining,
                    )
                    .await
                    .map_err(map_mtp_error)?;
                // A short read is legal mid-file, so keep asking for the rest.
                // An EMPTY read is the terminator (EOF, or a device with nothing
                // more to give): stop instead of spinning.
                if chunk.is_empty() {
                    break;
                }
                out.extend_from_slice(&chunk);
            }
            Ok(out)
        })
    }

    fn pause_releases_read_stream(&self) -> bool {
        // MTP reads in bounded windows (~8 MiB) and holds the one-per-device PTP
        // session ONLY during a window — between windows the session is free. So a
        // pause has nothing scarce to release: it just stops starting the next
        // window (park-in-place, like every other backend). The phone stays
        // navigable while paused because the copy isn't holding the session.
        false
    }

    fn supports_foreground_yield(&self) -> bool {
        // A running MTP copy reads in bounded windows, so a foreground listing/nav
        // already slips in between windows; this opt-in tells `CheckpointStream`
        // to ALSO not start the next window while foreground work is pending (so
        // the copy doesn't immediately re-grab the device lock and starve it). The
        // yield is "don't start the next window," not a session release. See
        // `CheckpointStream`'s auto-yield arm.
        true
    }

    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { connection_manager().foreground_pending(&self.device_id).await })
    }

    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move { connection_manager().background_yield_point(&self.device_id).await })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
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

            let chunk_stream = volume_read_stream_to_chunk_stream(stream, size, on_progress);
            let chunk_stream = Box::pin(chunk_stream);

            let bytes_written = connection_manager()
                .upload_from_stream(
                    &self.device_id,
                    self.storage_id,
                    &dest_folder,
                    &filename,
                    size,
                    chunk_stream,
                )
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

/// Adapts a `VolumeReadStream` into a `futures::Stream` that mtp-rs can
/// consume lazily, calling `on_progress` after each chunk and surfacing
/// `ControlFlow::Break` as an `io::Error` so the upload unwinds promptly.
///
/// Pre-fix this loop was missing entirely: `write_from_stream` collected
/// every chunk into a `Vec<Bytes>` before any USB write began (OOM risk for
/// large files) and never invoked the transfer progress / cancel callback.
pub(super) fn volume_read_stream_to_chunk_stream<'a>(
    stream: Box<dyn VolumeReadStream>,
    total: u64,
    on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
) -> impl futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + 'a {
    futures_util::stream::unfold(
        (stream, 0u64, on_progress, total),
        |(mut stream, bytes_written, on_progress, total)| async move {
            match stream.next_chunk().await {
                Some(Ok(chunk)) => {
                    let new_total = bytes_written + chunk.len() as u64;
                    if on_progress(new_total, total) == std::ops::ControlFlow::Break(()) {
                        let err = std::io::Error::new(std::io::ErrorKind::Interrupted, "Operation cancelled");
                        return Some((Err(err), (stream, new_total, on_progress, total)));
                    }
                    Some((Ok(bytes::Bytes::from(chunk)), (stream, new_total, on_progress, total)))
                }
                Some(Err(e)) => {
                    let err = std::io::Error::other(e.to_string());
                    Some((Err(err), (stream, bytes_written, on_progress, total)))
                }
                None => None,
            }
        },
    )
}

/// Bytes-per-window for a [`MtpReadStream`]. Production uses
/// [`crate::mtp::connection::MTP_READ_WINDOW`]; tests shrink it via
/// [`test_window`] so a small fixture spans multiple windows.
fn mtp_read_window() -> u32 {
    #[cfg(test)]
    {
        let o = test_window::get();
        if o != 0 {
            return o;
        }
    }
    crate::mtp::connection::MTP_READ_WINDOW
}

/// Test-only override for the read window size (see [`mtp_read_window`]). A
/// global is harmless here: every read test wants a small window, and the
/// production default is never asserted, so a value set by one test never
/// breaks another. Unit tests construct [`MtpReadStream`] with an explicit
/// `window` instead and don't touch this.
// `pub(super)` so the sibling `mtp_test` module (a child of `backends`) can
// reach it; `set` is widened to the same scope, while `get` stays `pub(super)`
// to `test_window` because only the in-file `mtp_read_window` reads it.
#[cfg(test)]
pub(super) mod test_window {
    use std::sync::atomic::{AtomicU32, Ordering};

    static OVERRIDE: AtomicU32 = AtomicU32::new(0);

    pub(super) fn get() -> u32 {
        OVERRIDE.load(Ordering::Relaxed)
    }

    #[cfg(feature = "virtual-mtp")]
    pub(in crate::file_system::volume::backends) fn set(window: u32) {
        OVERRIDE.store(window, Ordering::Relaxed);
    }
}

/// Bounded-window MTP read stream.
///
/// Reads a file as a sequence of bounded `GetPartialObject64` windows instead of
/// one held-open `GetObject`. Between windows nothing is in flight and the
/// one-per-device PTP session is free, so a foreground listing slips in at window
/// granularity (the whole point — navigate the phone during a copy).
///
/// `next_chunk` delegates to the connection layer's `read_next_window`, which
/// takes the per-device lock for each `GetPartialObject64`. The window
/// bookkeeping (total size, offset, clamp-to-remaining, EOF, advance-by-returned-
/// length, the 0-byte-before-EOF stall guard) lives in mtp-rs's `WindowedDownload`
/// inside the cached [`MtpReadSession`]; this struct just relays windows and
/// reports progress.
struct MtpReadStream {
    session: MtpReadSession,
    device_id: String,
}

impl VolumeReadStream for MtpReadStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            match connection_manager()
                .read_next_window(&mut self.session, &self.device_id)
                .await
            {
                Ok(Some(bytes)) => Some(Ok(bytes)),
                Ok(None) => None,
                Err(e) => Some(Err(map_mtp_error(e))),
            }
        })
    }

    fn total_size(&self) -> u64 {
        self.session.total_size()
    }

    fn bytes_read(&self) -> u64 {
        self.session.bytes_read()
    }

    // `cancel_and_release` uses the trait default (no-op): bounded windows hold
    // nothing between reads, so there's no in-flight transaction to abort. A
    // window read in flight when the stream is dropped self-heals via mtp-rs's
    // `TransactionScope` (see the connection layer's `read_next_window`).
}

/// Maps MTP connection errors to Volume errors.
fn map_mtp_error(e: MtpConnectionError) -> VolumeError {
    match e {
        MtpConnectionError::DeviceNotFound { .. } | MtpConnectionError::NotConnected { .. } => {
            VolumeError::NotFound(e.to_string())
        }
        MtpConnectionError::ObjectNotFound { path, .. } => VolumeError::NotFound(path),
        MtpConnectionError::StaleParentHandle { dest_folder, .. } => VolumeError::StaleDestinationHandle(dest_folder),
        MtpConnectionError::ExclusiveAccess { .. } | MtpConnectionError::PermissionDenied { .. } => {
            VolumeError::PermissionDenied(e.to_string())
        }
        MtpConnectionError::Cancelled { .. } => VolumeError::Cancelled(e.to_string()),
        MtpConnectionError::Disconnected { .. } => VolumeError::DeviceDisconnected(e.to_string()),
        // ❌ NOT `DeviceDisconnected`: a session reset leaves the device plugged
        // in and reopenable, so tearing down the volume would throw away a live
        // device. It stays a plain I/O failure of the one operation.
        MtpConnectionError::SessionReset { .. } => VolumeError::IoError {
            message: e.to_string(),
            raw_os_error: None,
        },
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
// Visible at `crate::file_system::volume::mtp_scan_oracle_tests`: those oracle
// tests live one level up (in `volume`), so they need this wider scope rather
// than a `pub(super)` that would only reach `backends`.
pub(in crate::file_system::volume) mod test_hooks {
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

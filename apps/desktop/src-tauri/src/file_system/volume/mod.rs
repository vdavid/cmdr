//! Volume trait for abstracting file system access.
//!
//! This module provides the `Volume` trait which abstracts file system operations,
//! enabling different storage backends (local filesystem, in-memory for testing, etc.).
//!
//! The data types the trait exchanges live in [`types`] (`VolumeError`, `SpaceInfo`,
//! `CopyScanResult`, `ScanConflict`, `MutationEvent`, …) and the volume ID helpers in
//! [`ids`] (`path_to_id`, `smb_volume_id`); both are re-exported here so callers keep
//! importing `volume::VolumeError`, `volume::smb_volume_id`, etc. unchanged.

// The Volume trait surface defines optional capability methods and helper types
// (VolumeScanner, VolumeWatcher, MutationEvent variants, etc.) that are part of
// the public API for future backends but aren't all called from production code
// paths today. InMemoryVolume + its helpers are test-only scaffolding.
// `#![deny(unused)]` at the crate root would flag these against a non-test build,
// so we relax dead-code checking for the whole submodule.
#![allow(dead_code, reason = "Trait API surface and test-only scaffolding")]

use crate::file_system::listing::FileEntry;
use crate::indexing::scanner::{ScanConfig, ScanError, ScanHandle, ScanSummary};
use crate::indexing::watcher::{DriveWatcher, FsChangeEvent, WatcherError};
use crate::indexing::writer::IndexWriter;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// A stream of bytes read from a volume.
///
/// This is an async interface for reading file data in chunks. Used for
/// streaming transfers between volumes. `next_chunk` is async (returns a
/// pinned boxed future) so that network-backed volumes (MTP, SMB) can
/// yield to the runtime instead of blocking. `total_size` and `bytes_read`
/// stay sync because they return cached values.
pub trait VolumeReadStream: Send {
    /// Returns the next chunk of data, or None if complete.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>>;

    /// Total size of the file in bytes.
    fn total_size(&self) -> u64;

    /// Bytes read so far (for progress tracking).
    fn bytes_read(&self) -> u64;

    /// Promptly release any scarce backend resource this stream holds across
    /// chunks, before the stream is dropped. After this call the stream is spent;
    /// `next_chunk` must not be called again on it.
    ///
    /// Default is a no-op, and that's what every current backend uses: reads that
    /// could otherwise pin a scarce resource (MTP's one-per-device PTP session)
    /// are bounded windows that hold nothing between chunks, so the copy wrapper
    /// (`CheckpointStream`) parks in place rather than releasing anything. This
    /// stays a trait hook for a hypothetical future backend whose stream genuinely
    /// holds a resource across chunks; nothing in the copy path calls it today.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}

/// Bulk enumeration for drive indexing. Each volume type implements its optimal strategy.
///
/// `LocalPosixVolume` uses jwalk for fast parallel traversal. Future volume types
/// (SMB, MTP, S3, etc.) will implement their own scanning strategies.
pub trait VolumeScanner: Send + Sync {
    /// Start a full-volume scan on a background thread.
    ///
    /// Returns a [`ScanHandle`] for progress tracking and cancellation, plus a
    /// [`std::thread::JoinHandle`] for the scan result.
    fn scan_volume(
        &self,
        config: ScanConfig,
        writer: &IndexWriter,
    ) -> Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError>;

    /// Synchronous subtree scan. Runs in the caller's thread.
    ///
    /// Used by post-replay background verification.
    fn scan_subtree(&self, root: &Path, writer: &IndexWriter, cancelled: &AtomicBool)
    -> Result<ScanSummary, ScanError>;
}

/// Real-time filesystem change notification for drive indexing.
///
/// Each volume type implements its own mechanism: FSEvents for local POSIX,
/// kqueue for SMB, polling for NFS/AFP, etc.
pub trait VolumeWatcher: Send + Sync {
    /// Start watching the volume root for filesystem changes.
    ///
    /// - `root`: path to watch (typically the volume root).
    /// - `since_when`: FSEvents event ID to replay from. Use `0` for "since now".
    /// - `event_sender`: channel to receive parsed change events.
    ///
    /// Returns a [`DriveWatcher`] handle for stopping and querying state.
    fn watch(
        &self,
        root: &Path,
        since_when: u64,
        event_sender: mpsc::Sender<FsChangeEvent>,
    ) -> Result<DriveWatcher, WatcherError>;
}

/// Async trait for volume file system operations.
///
/// Implementations provide access to different storage backends:
/// - `LocalPosixVolume`: Real local file system (async via `spawn_blocking`)
/// - `InMemoryVolume`: In-memory file system for testing
/// - `MtpVolume`: MTP device storage (natively async)
/// - `SmbVolume`: SMB share storage (natively async via smb2)
///
/// All path parameters are relative to the volume root. The volume handles
/// translating these to actual storage locations.
///
/// Methods are split into two categories:
/// - **Sync**: Identity accessors and capability flags that return struct fields. No I/O.
/// - **Async**: Methods that perform I/O. Return `Pin<Box<dyn Future<Output = T> + Send + '_>>` for
///   object safety (`dyn Volume`). Implementors wrap bodies in `Box::pin(async { ... })`.
pub trait Volume: Send + Sync {
    /// Returns the display name for this volume (like "Macintosh HD", "Dropbox").
    fn name(&self) -> &str;

    /// Returns the root path of this volume.
    fn root(&self) -> &Path;

    /// Returns this volume as `&dyn Any` for downcasting to a concrete
    /// backend type. Used by debug/IPC paths (for example, the SMB
    /// diagnostics dashboard) that need backend-specific state. Most
    /// implementations are one line: `fn as_any(&self) -> &dyn std::any::Any { self }`.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Identifies the shared physical resource this volume contends for, so the
    /// operation manager can serialize transfers that would thrash the same
    /// device or saturate the same single transport. See [`LaneKey`].
    ///
    /// Default: the volume root. Backends override with a per-resource id so
    /// two volumes on the SAME device share a lane: `LocalPosixVolume` →
    /// mount root, `MtpVolume` → device serial (one USB pipe), `SmbVolume` →
    /// `server+port+share` id. Never parse a `volume_id` string here.
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(self.root().to_string_lossy().into_owned())
    }

    // ========================================
    // Required: All volumes must implement
    // ========================================

    /// Lists directory contents at the given path (relative to volume root).
    ///
    /// Returns entries sorted with directories first, then files, both alphabetically.
    /// Pass `on_progress` to receive incremental `ListingProgress` updates during the stat
    /// loop (used by the streaming listing UI and by the scan-preview/scan-for-copy paths
    /// to surface running bytes + dirs in the dialog). Pass `None` when progress isn't
    /// needed. Backends should call `on_progress` periodically, not per-entry, to avoid
    /// flooding the IPC layer.
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>>;

    /// Cancel-aware version of [`list_directory`](Self::list_directory).
    ///
    /// `cancel`, when `Some`, is consulted by backends that issue many small
    /// USB or network roundtrips inside one listing (currently MTP — a 950-entry
    /// folder is 950 `GetObjectInfo` calls). When the flag flips to `true`,
    /// the backend bails between roundtrips with `VolumeError::Cancelled`
    /// instead of running to completion.
    ///
    /// Local and in-memory backends ignore the flag (their listings are
    /// effectively atomic from the caller's perspective). SMB ignores it
    /// today — adding SMB cancel propagation is a follow-up.
    ///
    /// Default impl delegates to `list_directory`, dropping the flag.
    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a std::sync::Arc<AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        let _ = cancel;
        self.list_directory(path, on_progress)
    }

    /// List a directory for the BACKGROUND index scan.
    ///
    /// Same result as `list_directory_with_cancel` with no progress callback, but
    /// backends that hold a scarce serialized resource across a listing (currently
    /// MTP — one USB pipe shared with foreground nav/copy/delete) override this to
    /// release that resource between bounded units and YIELD to any pending
    /// foreground op, so a long scan of a huge folder can't starve interactive use.
    /// Backends with no such contention (local, SMB, in-memory) use the default,
    /// which is just `list_directory_with_cancel`. The scanner
    /// (`indexing::volume_scanner`) calls THIS, not `list_directory`, for every
    /// directory it walks.
    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a std::sync::Arc<AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_directory_with_cancel(path, None, cancel)
    }

    /// Gets metadata for a single path (relative to volume root).
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>>;

    /// Checks if a path exists (relative to volume root).
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

    /// Checks if a path is a directory.
    /// Returns Ok(true) if directory, Ok(false) if file, Err if path doesn't exist.
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>>;

    // ========================================
    // E2E test support (feature-gated)
    // ========================================

    /// Injects an error that will be returned by the next `list_directory` call.
    /// After the error is returned once, subsequent calls work normally (enables testing retry).
    /// Only available in E2E builds. Default is no-op.
    #[cfg(feature = "playwright-e2e")]
    fn inject_error(&self, _errno: i32) {
        // No-op for volumes that don't support error injection
    }

    // ========================================
    // Optional: Default to NotSupported
    // ========================================

    /// Creates a file with the given content.
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = (path, content);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Creates a directory.
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Recursively creates `path` and any missing ancestor directories, like
    /// `mkdir -p`. Idempotent: a path (or ancestor) that already exists is a
    /// no-op, so re-running against an existing destination succeeds.
    ///
    /// This is the volume-aware transfer pipelines' destination gate: a copy or
    /// move into a not-yet-existing folder creates it on EVERY backend (local,
    /// SMB, MTP, in-memory), matching the local-FS `ensure_destination_dir`.
    ///
    /// The default walks `path`'s ancestors leaf→root, stopping at the first one
    /// that already `exists()` (or at the volume root), then creates the missing
    /// ones shallowest-first via `create_directory`. Probing existence per
    /// ancestor before creating means it never calls `create_directory` on a dir
    /// that's already there, so backends whose `create_directory` can't signal a
    /// collision (`MtpVolume`, `create_directory_errors_on_existing_dir() ==
    /// false`) never make a duplicate sibling. An `AlreadyExists` from
    /// `create_directory` (a concurrent op won a race) is also treated as
    /// success. These are network/IPC round-trips, so the leaf-first walk keeps
    /// them minimal: when the parent already exists (the common "new folder name
    /// under an existing dir" case), it's one `exists()` plus one
    /// `create_directory`.
    ///
    /// Backends override only if they have a cheaper native recursive mkdir;
    /// SMB and MTP don't, so the per-component loop is the right shape there.
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // Collect the missing ancestors, walking leaf→root until we reach
            // one that already exists (or run out). A component with no file
            // name (the volume root `/`, an empty path, or `.`) has nothing to
            // create above it — the root always exists — so it stops the walk.
            let mut missing: Vec<PathBuf> = Vec::new();
            for ancestor in path.ancestors() {
                if ancestor.file_name().is_none() {
                    break;
                }
                if self.exists(ancestor).await {
                    break;
                }
                missing.push(ancestor.to_path_buf());
            }

            // `ancestors()` yields leaf→root, so create shallowest-first: a
            // child can't be created before its parent.
            for dir in missing.iter().rev() {
                match self.create_directory(dir).await {
                    Ok(()) => {}
                    // A concurrent op created it between our `exists()` check and
                    // this call. Treat as success to keep the create idempotent.
                    Err(VolumeError::AlreadyExists(_)) => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        })
    }

    /// Deletes a single file or **empty** directory.
    ///
    /// **Strict contract: must NOT recurse.** If `path` is a non-empty directory,
    /// the implementation must return an error (typically `VolumeError::IoError`
    /// with errno `ENOTEMPTY` or equivalent), not silently delete the contents.
    /// The conflict resolver and several callers rely on this: `apply_volume_conflict_resolution`
    /// uses `is_directory` + skip-delete to enforce "Overwrite means merge for dirs"
    /// architecturally, but other call sites (rollback, partial-file cleanup) assume
    /// they only ever delete one node at a time and would over-delete if this contract
    /// loosened.
    ///
    /// For recursive deletes, callers should walk the tree themselves and call
    /// `delete` per leaf. See `delete_volume_path_recursive` in `volume_copy.rs`.
    ///
    /// Default: `NotSupported`.
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Cancel-aware version of [`delete`](Self::delete).
    ///
    /// MTP overrides this to thread the cancel flag through to mtp-rs's
    /// `delete_with_cancel`, which bails before issuing the `DeleteObject` PTP
    /// request when the flag is set. For non-empty directories the MTP
    /// implementation also checks the flag between recursive child deletes.
    ///
    /// Default impl delegates to `delete`, dropping the flag.
    fn delete_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a std::sync::Arc<AtomicBool>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = cancel;
        self.delete(path)
    }

    /// Renames/moves a file or directory within this volume.
    ///
    /// Both source and destination paths are relative to the volume root.
    /// When `force` is false, returns `AlreadyExists` if the destination exists.
    /// When `force` is true, proceeds even if the destination exists (POSIX rename
    /// silently overwrites).
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = (from, to, force);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    // ========================================
    // Mutation notification
    // ========================================

    /// Called after a successful mutation to update any active listings showing the parent
    /// directory.
    ///
    /// Default implementation uses `std::fs` to stat the entry and calls
    /// `notify_directory_changed`. Non-local volumes (SMB, MTP) override to use their own
    /// protocol for metadata.
    fn notify_mutation<'a>(
        &'a self,
        volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};
            use crate::file_system::listing::reading::get_single_entry;

            match mutation {
                MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                    let entry_path = parent_path.join(name);
                    match get_single_entry(&entry_path) {
                        Ok(entry) => {
                            let change = if matches!(mutation, MutationEvent::Created(_)) {
                                DirectoryChange::Added(entry)
                            } else {
                                DirectoryChange::Modified(entry)
                            };
                            notify_directory_changed(volume_id, parent_path, change);
                        }
                        Err(e) => {
                            log::warn!("notify_mutation: couldn't stat {}: {}", entry_path.display(), e);
                        }
                    }
                }
                MutationEvent::Deleted(name) => {
                    notify_directory_changed(volume_id, parent_path, DirectoryChange::Removed(name));
                }
                MutationEvent::Renamed { from, to } => {
                    let new_path = parent_path.join(&to);
                    match get_single_entry(&new_path) {
                        Ok(entry) => {
                            notify_directory_changed(
                                volume_id,
                                parent_path,
                                DirectoryChange::Renamed {
                                    old_name: from,
                                    new_entry: entry,
                                },
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "notify_mutation: couldn't stat renamed entry {}: {}",
                                new_path.display(),
                                e
                            );
                        }
                    }
                }
            }
        })
    }

    // ========================================
    // Lifecycle: Optional, default no-op
    // ========================================

    /// Returns the SMB connection state if this is an SMB volume.
    ///
    /// Only `SmbVolume` returns `Some`. Used by the frontend to show a connection
    /// quality indicator (green = direct smb2, yellow = OS mount fallback).
    fn smb_connection_state(&self) -> Option<SmbConnectionState> {
        None
    }

    /// Called when the volume is about to be unmounted/unregistered.
    ///
    /// Implementations can use this to clean up resources (disconnect network
    /// sessions, cancel background tasks, etc.). Default is a no-op.
    fn on_unmount(&self) {}

    /// Tries to rebuild this volume's underlying session in place after a
    /// transient connection loss. Idempotent and expected to be single-flight.
    ///
    /// Default returns `Err(NotSupported)`. Only `SmbVolume` overrides today;
    /// it's invoked by the FE reconnect manager on each backoff tick and on the
    /// "Retry now" button. Future network/cloud volumes should override this
    /// when they have a story for in-place reconnect.
    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Reconnect using freshly-entered credentials, replacing whatever was cached.
    ///
    /// Invoked by the "Sign in" affordance the frontend shows when an in-place reconnect
    /// gave up on an auth failure (a password changed on the server). The implementation
    /// persists the new credentials so the next reconnect is silent, then runs the normal
    /// reconnect. Default `Err(NotSupported)`; only `SmbVolume` overrides today.
    fn reconnect_with_credentials<'a>(
        &'a self,
        _username: String,
        _password: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    // ========================================
    // Watching: Optional, default no-op
    // ========================================

    /// Returns true if this volume supports file watching.
    fn supports_watching(&self) -> bool {
        false
    }

    /// Whether this volume's paths can be accessed via `std::fs` operations
    /// (stat, read_dir, metadata, etc.). True for local filesystems and
    /// OS-mounted network shares. False for protocol-only volumes like MTP.
    fn supports_local_fs_access(&self) -> bool {
        true
    }

    /// Returns `true` when the listing at `path` is currently being kept in sync
    /// by a live watcher on this volume. Used by
    /// `file_system::listing::caching::try_get_watched_listing` to decide whether
    /// a cached listing can replace a real read in write-op pre-flight.
    ///
    /// "Live watcher" is intentionally coarse for non-local backends; the
    /// returned `true` does NOT mean the cache is byte-perfect with the device
    /// right now. Every backend has a debounce or settling window between a real
    /// change and the cache reflecting it. See the freshness contract on
    /// `try_get_watched_listing` for the per-backend windows callers must tolerate.
    ///
    /// Default `false`: new backends without an active watcher opt in explicitly.
    fn listing_is_watched(&self, _path: &Path) -> bool {
        false
    }

    // ========================================
    // Indexing: Optional, default None
    // ========================================

    /// Returns a scanner for bulk enumeration during drive indexing.
    ///
    /// Only volume types that support efficient bulk traversal return `Some`.
    /// Currently: `LocalPosixVolume` (via jwalk). Returns `None` by default.
    fn scanner(&self) -> Option<Box<dyn VolumeScanner>> {
        None
    }

    /// Returns a watcher for real-time change notification during drive indexing.
    ///
    /// Only volume types with native change notification return `Some`.
    /// Currently: `LocalPosixVolume` (via FSEvents). Returns `None` by default.
    fn watcher(&self) -> Option<Box<dyn VolumeWatcher>> {
        None
    }

    // ========================================
    // Copy/Export: Optional, default no-op
    // ========================================

    /// Returns whether this volume can stream its bytes via `open_read_stream`
    /// (that is, it can act as a source in a cross-volume copy). Gates the copy
    /// dialog's "copy from this volume" UI.
    fn supports_export(&self) -> bool {
        false
    }

    /// How many streaming copy operations can be driven concurrently on this
    /// volume.
    ///
    /// Volumes serialized by a single underlying transport (MTP over USB,
    /// single SMB session without pipelining) return `1`; volumes that support
    /// parallel I/O (local disk, SMB with Phase 3 concurrent `execute`, S3)
    /// return higher. The copy engine takes `min(src, dst, 32)` to decide how
    /// many `FuturesUnordered` tasks to keep in flight. Default `1` preserves
    /// current sequential behavior for any new backend that doesn't override.
    fn max_concurrent_ops(&self) -> usize {
        1
    }

    /// Scans a path recursively to get statistics for a copy operation.
    /// Returns file count, directory count, and total bytes.
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Scans multiple paths to get aggregate + per-path copy statistics.
    ///
    /// The default iterates over `scan_for_copy` per path, which is correct for
    /// volumes where per-path I/O is cheap (local FS, in-memory). Volume types
    /// with expensive per-path I/O (MTP, SMB, FTP, S3) should override this to
    /// batch, typically by pipelining per-path stats over a shared session
    /// (SMB) or grouping paths by parent directory and listing each parent
    /// once (MTP).
    ///
    /// The returned `BatchScanResult` carries both the rolled-up `aggregate`
    /// (what the scan-preview / pre-flight checks want) and a `per_path` vec
    /// (what the copy engine uses to seed its per-source hints, so it doesn't
    /// have to re-probe each source's type and size with a separate stat).
    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_with_progress(paths, None)
    }

    /// Same as `scan_for_copy_batch`, but emits running progress as the scan
    /// walks. `on_progress(files_found)` is called repeatedly as entries are
    /// discovered, letting the scan-preview dialog show a climbing count
    /// instead of a frozen "0 files" spinner during a slow enumeration (the
    /// MTP listing of /DCIM/Camera with 1k+ entries takes ~17 s of USB
    /// round-trips, and there's nothing for the user to look at during it).
    ///
    /// The default implementation ignores `on_progress` and delegates to the
    /// existing `scan_for_copy_batch`. Volumes with expensive per-path I/O
    /// (currently MTP) override this to thread the callback through to their
    /// underlying streaming listing primitive (`list_directory_with_progress`).
    ///
    /// The callback receives a `ListingProgress` carrying running files / dirs
    /// / bytes. Backends accumulate from the entries they've enumerated and
    /// report the cumulative totals for the current scan call. The FE renders
    /// all three counters climbing live during the scan dialog.
    #[allow(unused_variables, reason = "Default impl intentionally ignores `on_progress`")]
    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut aggregate = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                dedup_bytes: 0,
                // Aggregate over multiple paths: meaningless for a batch.
                // Callers that need per-path type should read `per_path`.
                top_level_is_directory: false,
            };
            let mut per_path = Vec::with_capacity(paths.len());
            for path in paths {
                let scan = self.scan_for_copy(path).await?;
                aggregate.file_count += scan.file_count;
                aggregate.dir_count += scan.dir_count;
                aggregate.total_bytes += scan.total_bytes;
                aggregate.dedup_bytes += scan.dedup_bytes;
                per_path.push((path.clone(), scan));
                if let Some(cb) = on_progress {
                    cb(ListingProgress {
                        files: aggregate.file_count,
                        dirs: aggregate.dir_count,
                        bytes: aggregate.total_bytes,
                    });
                }
            }
            Ok(BatchScanResult { aggregate, per_path })
        })
    }

    /// Checks destination for conflicts with source items.
    /// Returns list of files that already exist at destination.
    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        let _ = (source_items, dest_path);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Gets space information for this volume.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Recommended poll interval for live disk-space monitoring.
    ///
    /// Local volumes use a short interval (2 s) because `statvfs`/NSURL is
    /// microsecond-cheap. Network and MTP volumes use a longer interval (5 s)
    /// to avoid unnecessary traffic. Returns `None` if space polling is not
    /// meaningful for this volume type (for example, in-memory test volumes).
    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs(2))
    }

    // ========================================
    // Capability hints for copy optimization
    // ========================================

    /// Returns the local filesystem path if this volume is backed by one.
    /// Used to optimize local-to-local copies using native OS APIs (such as copyfile on macOS).
    /// Returns None for non-local volumes (MTP, S3, FTP, etc.).
    fn local_path(&self) -> Option<PathBuf> {
        None
    }

    /// Returns true if this volume supports streaming read/write operations.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Whether `create_directory` reliably returns `VolumeError::AlreadyExists`
    /// when a directory of the same name already exists at the path.
    ///
    /// The scan-as-you-merge folder-merge walker
    /// (`write_operations/transfer/volume_strategy.rs`) uses the `AlreadyExists`
    /// result as the signal that a destination level PRE-EXISTED and must be
    /// merged into (list it once, resolve clashing children) rather than created
    /// fresh. Default `true` covers LocalPosix (`std::fs::create_dir` →
    /// `ErrorKind::AlreadyExists`), SMB (smb2 typed STATUS_OBJECT_NAME_COLLISION),
    /// and InMemory. `MtpVolume` overrides to `false`: the MTP protocol allows
    /// same-name sibling objects and `create_folder` silently makes a duplicate
    /// `photos` instead of erroring, so the walker must pre-check existence on
    /// MTP before creating — a blindly-created duplicate would make the merge
    /// target the wrong directory.
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        true
    }

    /// Opens a streaming reader for the given path.
    ///
    /// Returns a VolumeReadStream that yields chunks of data.
    /// The stream must be fully consumed or dropped before other operations.
    ///
    /// # Streaming requirement
    ///
    /// **Must stream.** Don't read the whole file into a `Vec<u8>` inside
    /// this method and hand chunks of it back. That's just pre-buffering
    /// with extra steps. A user streaming an 8 GB file would allocate 8 GB
    /// of RAM before the consumer sees a single byte. Drive the backend on
    /// demand from `next_chunk` (smb2: an `smb2::FileDownload`; MTP: bounded
    /// `GetPartialObject64` windows). If the backend gives you a borrowed
    /// handle, use a bounded producer/consumer channel (see `SmbReadStream`
    /// for the pattern).
    ///
    /// Peak memory per transfer should be bounded by a small chunk buffer
    /// (~1 MiB) regardless of file size.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Opens a streaming reader with an optional size hint from the caller.
    ///
    /// Network-backed volumes can use the hint to pick a faster compound
    /// request path for small files (e.g., SMB's CREATE+READ+CLOSE compound)
    /// instead of the 3-RTT streaming open. Backends that can't use the hint
    /// fall through to `open_read_stream`.
    ///
    /// The hint is best-effort. Callers pass `None` when they don't know
    /// the size ahead of time, and the backend must work correctly either
    /// way.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let _ = size_hint;
        self.open_read_stream(path)
    }

    /// Opens a streaming reader that starts at a byte offset (resumable read).
    ///
    /// Streams `[offset, size)` of `path`. `offset == 0` is equivalent to
    /// [`open_read_stream`](Self::open_read_stream) (the whole file). The
    /// returned stream's `total_size()` reports the FULL file size (not the
    /// remaining tail), so a resumed transfer's progress stays anchored to the
    /// whole file; `bytes_read()` counts only this segment.
    ///
    /// A resumable-read primitive. The copy path no longer reopens at an offset
    /// (pause and foreground yield park in place between bounded windows in the
    /// transfer wrapper `CheckpointStream`, so nothing calls this with a non-zero
    /// offset today), but MTP keeps it correct: a non-zero `offset` streams
    /// exactly `[offset, size)` with no gap or overlap.
    ///
    /// Default is `NotSupported`; only MTP implements it. `MtpVolume`'s
    /// `open_read_stream` routes through it with `offset == 0`.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_at_offset<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let _ = (path, offset);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Reads a bounded byte range `[offset, offset + len)` of `path`, returning
    /// the bytes actually read.
    ///
    /// The positioned, `pread`-shaped primitive that backs **remote-archive
    /// browsing**: the archive byte source calls this to feed `rc-zip`'s sans-IO
    /// reader a few ranges (the tail for the central directory, then each entry's
    /// compressed span) without downloading the whole `.zip`. Unlike
    /// [`open_read_stream_at_offset`](Self::open_read_stream_at_offset) — a
    /// stream-to-EOF — this returns exactly the requested window.
    ///
    /// Returns fewer than `len` bytes ONLY at end of file (a read wholly past the
    /// end yields an empty `Vec`); the backend fills the window from as few
    /// backend round-trips as it can, so a caller never has to loop for a network
    /// short read. `len` is caller-bounded (the archive source uses ≤ a tail-sized
    /// window), so buffering the range is safe — this is NOT a whole-file read.
    ///
    /// Default is `NotSupported`. Implemented by the backends that can back a
    /// remote archive: `LocalPosixVolume` (`pread`), `SmbVolume` (a positioned
    /// SMB READ), and `MtpVolume` (a `GetPartialObject64` window). A backend that
    /// can't do positioned reads leaves the default, and the archive layer treats
    /// its archives as unreadable rather than misbehaving.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        let _ = (path, offset, len);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Whether pausing a streaming read from this volume needs to release a
    /// scarce backend resource (rather than park the open stream in place).
    ///
    /// `false` for every backend now — including MTP, whose reads are bounded
    /// windows that hold the one-per-device PTP session only DURING a window, not
    /// between them, so a pause has nothing to release and just stops starting the
    /// next window. The predicate is kept as the trait extension point for a
    /// hypothetical future backend whose stream genuinely pins a resource across
    /// the whole read; no backend currently overrides it and the copy wrapper no
    /// longer reads it.
    fn pause_releases_read_stream(&self) -> bool {
        false
    }

    /// Whether a running transfer reading from this volume should AUTO-YIELD to
    /// foreground device work mid-copy (don't start the next read window while
    /// foreground work is pending; resume from the current offset once it drains),
    /// without the user pausing.
    ///
    /// `true` only for MTP. Its reads are bounded windows, so a foreground
    /// listing/nav already slips in between windows; this opt-in additionally
    /// keeps the copy from immediately re-grabbing the device lock and starving
    /// foreground — the copy's per-window checkpoint behaves like the index scan's
    /// `background_yield_point`, parking until foreground drains. No session is
    /// released; "yield" means "don't start the next window."
    ///
    /// `false` (default): the auto-yield arm in `CheckpointStream` is a no-op, so
    /// local FS, SMB, and in-memory transfers behave exactly as before.
    fn supports_foreground_yield(&self) -> bool {
        false
    }

    /// Whether a foreground op is currently pending on this volume's device.
    ///
    /// Polled once per chunk by `CheckpointStream` (cheap — an atomic load behind
    /// the device's priority gate). `MtpVolume` delegates to the connection
    /// manager's per-device gate; every other backend uses the default `false`,
    /// so they never trigger an auto-yield. See [`supports_foreground_yield`](Self::supports_foreground_yield).
    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }

    /// Park until this volume's device is clear of foreground work.
    ///
    /// Called by `CheckpointStream`'s auto-yield arm to hold off the next read
    /// window: it waits here so the foreground listing/nav owns the device, then
    /// the checkpoint lets the next window proceed from the current offset.
    /// `MtpVolume` delegates to the per-device gate's `background_yield_point`
    /// (returns the instant the last foreground guard drops); every other backend
    /// uses the default no-op. See [`supports_foreground_yield`](Self::supports_foreground_yield).
    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    /// Writes data from a stream to the given path.
    ///
    /// `on_progress(bytes_written, total_size)` is called after each chunk is
    /// written. Return `ControlFlow::Break(())` to cancel the transfer.
    ///
    /// # Arguments
    /// * `dest` - Destination path (file will be created/overwritten)
    /// * `size` - Total size in bytes (required for protocols like MTP)
    /// * `stream` - Source data stream
    /// * `on_progress` - Progress callback; return `ControlFlow::Break(())` to cancel
    ///
    /// # Streaming requirement
    ///
    /// **Must stream.** Don't drain `stream` into a `Vec<u8>` before writing
    /// to the backend. A user copying an 8 GB file through this path would
    /// allocate 8 GB of RAM. Pull each chunk from `stream.next_chunk().await`
    /// and push it straight into the backend's streaming writer (smb2:
    /// `FileWriter`, mtp-rs: `upload_stream`) in the same loop. Holding the
    /// backend's session mutex across the source `next_chunk` awaits is
    /// fine. Different volumes use different mutexes, so there's no
    /// deadlock risk.
    ///
    /// Peak memory per transfer should be bounded by a small chunk buffer
    /// (~1 MiB) regardless of file size.
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let _ = (dest, size, stream, on_progress);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }
}

// Shared data types (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `ScanConflict`,
// `MutationEvent`, …) live in `types`; the volume ID helpers (`path_to_id`,
// `smb_volume_id`) live in `ids`. Both are re-exported below so external callers
// keep importing `volume::VolumeError`, `volume::smb_volume_id`, etc. unchanged.
mod ids;
mod types;
pub use ids::*;
pub use types::*;

// Per-backend `Volume` implementations live in `backends/`. The trait surface
// stays here; submodule names are re-exported below so external callers keep
// importing `volume::LocalPosixVolume`, `volume::MtpVolume`, etc. without
// caring about the `backends/` split.
pub mod backends;
// Volume teardown (USB/SD/DMG/SMB/MTP), used only by the macOS+Linux eject
// command. The macOS-vs-Linux difference (diskutil vs umount, NSURL vs
// `/sys/block`) lives inside via per-fn `#[cfg]`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod eject;
pub mod friendly_error;
pub(crate) mod manager;

pub use backends::{InMemoryVolume, LocalPosixVolume};
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backends::{MtpVolume, SmbVolume};

// `smb` is re-exported as a module path because callers reach into it for
// `SmbConnectionParams` / `connect_smb_volume` / `set_app_handle`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use backends::smb;

#[cfg(test)]
mod inmemory_test;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod mtp_scan_oracle_tests;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod smb_scan_oracle_tests;

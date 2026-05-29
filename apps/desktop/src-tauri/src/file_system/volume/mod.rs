//! Volume trait for abstracting file system access.
//!
//! This module provides the `Volume` trait which abstracts file system operations,
//! enabling different storage backends (local filesystem, in-memory for testing, etc.).

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
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;

/// SMB connection state for the frontend indicator and the reconnect UI.
///
/// `Direct` means Cmdr's smb2 session is active (fast path).
/// `OsMount` means only the OS mount is alive (fallback path).
/// `Disconnected` means an SmbVolume exists but its smb2 session is broken. The
/// frontend reconnect manager owns the recovery cycle.
///
/// Non-SMB volumes return `None` from `Volume::smb_connection_state()` (trait
/// default). The frontend uses this to distinguish "this isn't an SMB volume"
/// (no value) from "this is an SMB volume in trouble" (Some(Disconnected)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SmbConnectionState {
    /// smb2 session active: fast path (green indicator).
    Direct,
    /// Using OS mount only: slower fallback (yellow indicator).
    OsMount,
    /// Cmdr's smb2 session has dropped. The frontend swaps to `SmbReconnectingView`
    /// and the per-volume reconnect manager runs the backoff cycle.
    Disconnected,
}

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Running tally a `Volume`'s directory walk reports through its progress
/// callback. Replaces the old `Fn(usize)` callback shape so backends can
/// stream the bytes-and-dirs UI numbers alongside the file count.
///
/// Semantics: every field is the *cumulative* count for the current listing
/// scope (a single `list_directory` call, or a single `scan_for_copy_batch`
/// invocation). `files` excludes directories and `bytes` is the sum of file
/// sizes only (directories contribute 0). Consumers that want the total
/// entry count for "Loading N entries…" displays read `files + dirs`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListingProgress {
    pub files: usize,
    pub dirs: usize,
    pub bytes: u64,
}

impl ListingProgress {
    /// Total entries enumerated so far (files + dirs). Convenience for the
    /// streaming listing UI, which renders one "Loaded N entries…" line.
    pub fn entries(&self) -> usize {
        self.files + self.dirs
    }
}

/// Convert a mount path to a safe ID string.
///
/// **Don't use this for SMB mounts.** Use [`smb_volume_id`] instead: it keys by
/// the underlying mount (server, port, share) rather than the path-shape, so two
/// SMB shares with the same case-folded name on different servers don't collide.
pub(crate) fn path_to_id(path: &str) -> String {
    if path == "/" {
        return DEFAULT_VOLUME_ID.to_string();
    }
    path.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase()
}

/// Build a stable, collision-free volume ID for an SMB mount.
///
/// Format: `smb-{server}-{port}-{share}`, with dots in the server (IPs) replaced
/// by `-`, everything else stripped down to `[a-z0-9-]`, and both server and share
/// lowercased.
///
/// # Why not [`path_to_id`]?
///
/// Path-based IDs lowercase the mount path, so two SMB shares with the same
/// case-folded name on different servers (a NAS sharing `Public`, a Docker
/// container sharing `public`) collide on `volumespublic`. The collision
/// cross-contaminates `lastUsedPaths`, tab `volumeId` fields, and any other
/// per-volume state, which surfaces as wrong-cased paths flowing into
/// `SmbVolume::list_directory` and the server returning
/// `STATUS_OBJECT_PATH_NOT_FOUND`. Keying by (server, port, share) instead of
/// path-shape prevents the collision at the root.
///
/// # Case folding
///
/// - Server: DNS hostnames are case-insensitive, so `Naspolya` and `naspolya` are the same host.
///   Lowercased.
/// - Share: SMB is case-insensitive for share names per the protocol (Windows and Samba default),
///   so `Public` and `public` on the same server are the same share. Lowercased.
/// - Port: literal, no folding.
pub fn smb_volume_id(server: &str, port: u16, share: &str) -> String {
    fn sanitize(s: &str) -> String {
        s.chars()
            .map(|c| if c == '.' { '-' } else { c })
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect::<String>()
            .to_lowercase()
    }
    format!("smb-{}-{}-{}", sanitize(server), port, sanitize(share))
}

/// Describes what mutation occurred, so `notify_mutation` can update the listing cache.
pub enum MutationEvent {
    /// A file or directory was created. Contains the entry name.
    Created(String),
    /// A file or directory was deleted. Contains the entry name.
    Deleted(String),
    /// A file or directory was modified. Contains the entry name.
    Modified(String),
    /// A file or directory was renamed within the same parent. Contains old and new names.
    Renamed { from: String, to: String },
}

/// Result of scanning a path for copy operation.
#[derive(Debug, Clone)]
pub struct CopyScanResult {
    pub file_count: usize,
    pub dir_count: usize,
    /// Total size in bytes — the **write footprint**. Counts every file at
    /// full size, including each hardlink, because hardlinks don't survive a
    /// cross-volume copy (every link materializes as an independent file at
    /// the destination). This is the number the progress bar fills against
    /// and the disk-space check requires.
    pub total_bytes: u64,
    /// Source on-disk footprint, `du`-equivalent: each inode counted once.
    /// Equals `total_bytes` on backends without hardlinks (MTP, SMB,
    /// InMemory) or trees with none. `LocalPosixVolume` dedupes by inode so
    /// the Copy dialog can show "X will be written (source is Y)" when the
    /// two differ. **Informational only** — never feeds the progress bar or
    /// the space check. Dedup is scan-scoped per top-level source; a hardlink
    /// pair spanning two separately-selected sources counts twice (rare;
    /// over-counts the source size slightly, which is the safe direction for
    /// an informational hint).
    pub dedup_bytes: u64,
    /// Whether the scanned top-level path is a directory (vs a single file).
    ///
    /// Populated by each volume's `scan_for_copy` using the stat it already does
    /// for the top-level path. Callers (the copy pipeline) reuse this instead of
    /// issuing a separate `is_directory` probe per source, saving one round-trip
    /// per file on network-backed volumes (SMB, MTP).
    pub top_level_is_directory: bool,
}

/// Result of a batch scan over multiple source paths.
///
/// Returned by `Volume::scan_for_copy_batch`. Bundles the aggregate stats that
/// the pre-flight / scan-preview callers want with a per-path breakdown that
/// the copy engine uses to seed its `source_hints` map (without re-issuing N
/// stat probes). `per_path[i].0` is the caller's input path verbatim; `.1`
/// carries `top_level_is_directory` and `total_bytes` (for top-level files,
/// that's the file size, used by the SMB compound fast-path).
#[derive(Debug, Clone)]
pub struct BatchScanResult {
    /// Aggregate stats across all input paths.
    pub aggregate: CopyScanResult,
    /// Per-input-path result, in the same order as the `paths` slice the
    /// caller passed in. Paths that failed to scan won't appear. On a
    /// per-path failure the method returns `Err` without partial data.
    pub per_path: Vec<(PathBuf, CopyScanResult)>,
}

/// A conflict detected during pre-copy scanning: a source item that already exists at the
/// destination.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanConflict {
    /// Relative to volume root.
    pub source_path: String,
    /// Relative to volume root.
    pub dest_path: String,
    /// In bytes.
    pub source_size: u64,
    /// In bytes.
    pub dest_size: u64,
    /// Unix timestamp in seconds.
    pub source_modified: Option<i64>,
    /// Unix timestamp in seconds.
    pub dest_modified: Option<i64>,
}

/// Space information for a volume.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpaceInfo {
    /// In bytes.
    pub total_bytes: u64,
    /// In bytes.
    pub available_bytes: u64,
    /// In bytes.
    pub used_bytes: u64,
}

/// Information about a source item for conflict scanning.
#[derive(Debug, Clone)]
pub struct SourceItemInfo {
    pub name: String,
    /// In bytes.
    pub size: u64,
    /// Unix timestamp in seconds.
    pub modified: Option<i64>,
}

/// Error type for volume operations.
#[derive(Debug, Clone)]
pub enum VolumeError {
    NotFound(String),
    PermissionDenied(String),
    AlreadyExists(String),
    /// Not supported by this volume type.
    NotSupported,
    /// Device went away mid-operation.
    DeviceDisconnected(String),
    /// Device or volume is read-only.
    ReadOnly(String),
    /// Device storage is full.
    StorageFull {
        message: String,
    },
    /// Connection timed out.
    ConnectionTimeout(String),
    /// Operation was cancelled by the user (progress callback returned Break).
    Cancelled(String),
    /// The path is a directory, not a file (for example, SMB STATUS_FILE_IS_A_DIRECTORY).
    IsADirectory(String),
    /// The file is in `STATUS_DELETE_PENDING`: a delete has been requested on the server
    /// but at least one open handle is keeping the file alive. The file will disappear
    /// once the last handle closes; any new `Create` (stat, open, write) on the path
    /// fails with this status in the meantime. SMB-only today.
    DeletePending(String),
    IoError {
        message: String,
        raw_os_error: Option<i32>,
    },
    /// Structured git-layer failure.
    ///
    /// Carries the full `FriendlyGitError` (kind + path + optional raw detail)
    /// so the listing pipeline's `friendly_error_from_volume_error` can hand
    /// `ErrorPane` a fully-shaped `FriendlyError` (title, explanation,
    /// suggestion, category) without parsing strings. Built by the volume
    /// hooks in `file_system::git::mod` (`try_route_listing`,
    /// `try_route_metadata`, `try_open_blob_stream`).
    FriendlyGit(crate::file_system::git::friendly::FriendlyGitError),
}

impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Path not found: {}", path),
            Self::PermissionDenied(path) => write!(f, "Permission denied: {}", path),
            Self::AlreadyExists(path) => write!(f, "Already exists: {}", path),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::DeviceDisconnected(msg) => write!(f, "Device disconnected: {}", msg),
            Self::ReadOnly(msg) => write!(f, "Read-only: {}", msg),
            Self::StorageFull { message } => write!(f, "Storage full: {}", message),
            Self::ConnectionTimeout(msg) => write!(f, "Connection timed out: {}", msg),
            Self::Cancelled(msg) => write!(f, "Cancelled: {}", msg),
            Self::IsADirectory(path) => write!(f, "Is a directory: {}", path),
            Self::DeletePending(path) => write!(f, "Delete pending: {}", path),
            Self::IoError { message, .. } => write!(f, "I/O error: {}", message),
            Self::FriendlyGit(err) => write!(f, "git: {}", err),
        }
    }
}

impl std::error::Error for VolumeError {}

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

impl From<std::io::Error> for VolumeError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied(err.to_string()),
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists(err.to_string()),
            _ => Self::IoError {
                message: err.to_string(),
                raw_os_error: err.raw_os_error(),
            },
        }
    }
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
    /// of RAM before the consumer sees a single byte. Drive the backend's
    /// streaming reader (smb2: `FileDownload`, mtp-rs: `FileDownload`) on
    /// demand from `next_chunk`. If the backend gives you a borrowed
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

// Per-backend `Volume` implementations live in `backends/`. The trait surface
// stays here; submodule names are re-exported below so external callers keep
// importing `volume::LocalPosixVolume`, `volume::MtpVolume`, etc. without
// caring about the `backends/` split.
pub mod backends;
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

#[cfg(test)]
mod id_tests {
    use super::*;

    #[test]
    fn smb_volume_id_distinguishes_servers_with_same_share_name() {
        // The exact bug that motivated per-mount IDs: QNAP's `Public` share and a
        // Docker container's `public` share would both collide on `volumespublic`
        // under the old path-shape ID scheme, cross-contaminating `lastUsedPaths`,
        // tabs, and per-volume state. They must produce distinct IDs.
        let qnap = smb_volume_id("Naspolya", 445, "Public");
        let docker = smb_volume_id("localhost", 10494, "public");
        assert_ne!(qnap, docker);
    }

    #[test]
    fn smb_volume_id_is_stable_for_identical_inputs() {
        // Same logical mount → same ID across calls (required for `lastUsedPaths`
        // and tab state to roundtrip).
        let a = smb_volume_id("naspolya", 445, "naspi");
        let b = smb_volume_id("naspolya", 445, "naspi");
        assert_eq!(a, b);
    }

    #[test]
    fn smb_volume_id_treats_server_case_insensitively() {
        // DNS hostnames are case-insensitive; mounting `smb://Naspolya/...` and
        // `smb://naspolya/...` is the same mount, so the IDs must match.
        assert_eq!(
            smb_volume_id("Naspolya", 445, "naspi"),
            smb_volume_id("naspolya", 445, "naspi")
        );
    }

    #[test]
    fn smb_volume_id_treats_share_case_insensitively() {
        // The SMB protocol treats share names case-insensitively (Windows/Samba
        // default). Two mounts of the same server with case-only-different shares
        // are the same share.
        assert_eq!(
            smb_volume_id("naspolya", 445, "Public"),
            smb_volume_id("naspolya", 445, "public")
        );
    }

    #[test]
    fn smb_volume_id_distinguishes_ports() {
        // Same host, same share name, different port = different server in
        // practice (typical with reverse proxies and dev fixtures on localhost).
        assert_ne!(
            smb_volume_id("localhost", 10480, "public"),
            smb_volume_id("localhost", 10494, "public")
        );
    }

    #[test]
    fn smb_volume_id_handles_ip_addresses_without_collision() {
        // IPs with dots must not be silently squashed in a way that lets two
        // different IPs collide.
        assert_ne!(
            smb_volume_id("192.168.1.111", 445, "naspi"),
            smb_volume_id("192.168.1.112", 445, "naspi")
        );
    }

    #[test]
    fn smb_volume_id_does_not_collide_with_path_based_ids() {
        // No realistic local volume path should ever produce the same ID as an
        // SMB mount. The `smb-` prefix is the contract.
        let smb = smb_volume_id("localhost", 10494, "public");
        let local = path_to_id("/Volumes/Smb");
        assert_ne!(smb, local);
        assert!(smb.starts_with("smb-"), "got: {smb}");
    }

    #[test]
    fn path_to_id_still_works_for_non_smb_paths() {
        // Sanity: the path-based helper is unchanged for local volumes.
        assert_eq!(path_to_id("/"), "root");
        assert_eq!(path_to_id("/Volumes/External"), "volumesexternal");
    }
}

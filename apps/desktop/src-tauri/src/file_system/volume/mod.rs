//! Volume trait for abstracting file system access.
//!
//! This module provides the `Volume` trait which abstracts file system operations,
//! enabling different storage backends (local filesystem, in-memory for testing, etc.).

// TODO: Remove this once Volume is integrated into operations.rs (Phase 2)
#![allow(dead_code, reason = "Volume abstraction not yet integrated into operations.rs")]

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

/// SMB connection state for the frontend indicator.
///
/// `Direct` means Cmdr's smb2 session is active (fast path).
/// `OsMount` means only the OS mount is alive (fallback path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmbConnectionState {
    /// smb2 session active — fast path (green indicator).
    Direct,
    /// Using OS mount only — slower fallback (yellow indicator).
    OsMount,
}

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Convert a mount path to a safe ID string.
pub(crate) fn path_to_id(path: &str) -> String {
    if path == "/" {
        return DEFAULT_VOLUME_ID.to_string();
    }
    path.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .to_lowercase()
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
    /// Total size in bytes.
    pub total_bytes: u64,
}

/// A conflict detected during pre-copy scanning: a source item that already exists at the destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    IoError {
        message: String,
        raw_os_error: Option<i32>,
    },
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
            Self::IoError { message, .. } => write!(f, "I/O error: {}", message),
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
/// - **Async**: Methods that perform I/O. Return `Pin<Box<dyn Future<Output = T> + Send + '_>>`
///   for object safety (`dyn Volume`). Implementors wrap bodies in `Box::pin(async { ... })`.
pub trait Volume: Send + Sync {
    /// Returns the display name for this volume (like "Macintosh HD", "Dropbox").
    fn name(&self) -> &str;

    /// Returns the root path of this volume.
    fn root(&self) -> &Path;

    // ========================================
    // Required: All volumes must implement
    // ========================================

    /// Lists directory contents at the given path (relative to volume root).
    ///
    /// Returns entries sorted with directories first, then files, both alphabetically.
    /// Pass `on_progress` to receive incremental `loaded_count` updates during the stat loop
    /// (used by the streaming listing UI). Pass `None` when progress isn't needed.
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(usize) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>>;

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

    /// Deletes a file or empty directory.
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
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

    /// Called after a successful mutation to update any active listings showing the parent directory.
    ///
    /// Default implementation uses `std::fs` to stat the entry and calls `notify_directory_changed`.
    /// Non-local volumes (SMB, MTP) override to use their own protocol for metadata.
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

    /// Returns whether this volume supports copy/export operations.
    fn supports_export(&self) -> bool {
        false
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

    /// Scans multiple paths to get aggregate copy statistics.
    ///
    /// The default iterates over `scan_for_copy` per path, which is correct for
    /// volumes where per-path I/O is cheap (local FS, in-memory). Volume types
    /// with expensive per-path I/O (MTP, SMB, FTP, S3) should override this to
    /// batch — typically by grouping paths by parent directory, listing each
    /// parent once, and resolving files from the listing.
    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut result = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
            };
            for path in paths {
                let scan = self.scan_for_copy(path).await?;
                result.file_count += scan.file_count;
                result.dir_count += scan.dir_count;
                result.total_bytes += scan.total_bytes;
            }
            Ok(result)
        })
    }

    /// Downloads/exports a file or directory from this volume to a local path.
    /// For local volumes, this is a file copy. For MTP, this downloads.
    ///
    /// `on_progress(bytes_done, bytes_total)` is called periodically during the transfer.
    /// Return `ControlFlow::Break(())` from the callback to cancel the transfer.
    /// Returns bytes transferred.
    fn export_to_local<'a>(
        &'a self,
        source: &'a Path,
        local_dest: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let _ = (source, local_dest, on_progress);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Imports/uploads a file or directory from a local path to this volume.
    /// For local volumes, this is a file copy. For MTP, this uploads.
    ///
    /// `on_progress(bytes_done, bytes_total)` is called periodically during the transfer.
    /// Return `ControlFlow::Break(())` from the callback to cancel the transfer.
    /// Returns bytes transferred.
    fn import_from_local<'a>(
        &'a self,
        local_source: &'a Path,
        dest: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let _ = (local_source, dest, on_progress);
        Box::pin(async { Err(VolumeError::NotSupported) })
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

// Implementations
pub mod friendly_error;
mod in_memory;
mod local_posix;
pub(crate) mod manager;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod mtp;
mod provider;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod smb;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_watcher;

pub use in_memory::InMemoryVolume;
pub use local_posix::LocalPosixVolume;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use mtp::MtpVolume;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use smb::SmbVolume;

// Re-export types defined in this module for convenience
// (they're already public since defined in mod.rs)

#[cfg(test)]
mod in_memory_test;
#[cfg(test)]
mod inmemory_test;
#[cfg(test)]
mod local_posix_test;

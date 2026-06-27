//! Shared data types for the `Volume` abstraction.
//!
//! The `Volume` trait and its sub-traits live in [`super`] (`mod.rs`); the plain
//! data types they exchange (errors, scan results, conflict records, progress
//! tallies, space info, mutation events) live here. `mod.rs` re-exports
//! everything in this module, so callers keep importing `volume::VolumeError`,
//! `volume::CopyScanResult`, etc. unchanged.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

/// Identifies the shared physical resource a volume contends for, so the
/// operation manager can serialize transfers that would thrash the same device
/// or saturate the same single transport.
///
/// Two volumes share a lane when they resolve to the same physical resource:
/// the same local mount, the same MTP device (one USB pipe), or the same SMB
/// server+share. An operation acquires a slot in EVERY lane it touches (source
/// and destination), and runs only when all those lanes are free (budget 1 per
/// lane in v1). A newtype over `String` (not a bare `String`) so it can't be
/// confused with a `volume_id` or a path at a call site — the two are derived
/// differently and must never be cross-assigned.
///
/// Derived from [`Volume::lane_key`](super::Volume::lane_key), NOT from parsing
/// a `volume_id` string (that would violate the `no-string-matching` rule).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LaneKey(String);

impl LaneKey {
    /// Builds a lane key from any stable per-resource identifier (a mount root,
    /// a device serial, an SMB `server+port+share` id).
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// The underlying key string (for logging / map keys).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for LaneKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

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
    /// `true` when the source item is a directory (from the caller-supplied
    /// `SourceItemInfo`). Lets the FE classify a dir-vs-dir collision as a
    /// silent merge ("will merge") instead of a conflict.
    pub source_is_directory: bool,
    /// `true` when the destination item is a directory (from the dest listing
    /// entry). See `source_is_directory`.
    pub dest_is_directory: bool,
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
    /// `true` when the source item is a directory. The caller knows this from
    /// the `FileEntry` it already has in hand; backends copy it straight onto
    /// the resulting `ScanConflict::source_is_directory`.
    pub is_directory: bool,
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
    /// The destination folder's cached handle was stale and the backend rejected
    /// a write into it (MTP: the device re-keyed its object handles since the
    /// folder was last listed). The backend has already refreshed its cache, so
    /// the transfer engine retries the write once with a fresh source stream.
    /// Carries the destination folder path for a destination-correct message if
    /// the retry also fails. MTP-only today.
    StaleDestinationHandle(String),
    IoError {
        message: String,
        raw_os_error: Option<i32>,
    },
    /// Structured git-layer failure.
    ///
    /// Carries the full `FriendlyGitError` (kind + path + optional raw detail)
    /// so the listing pipeline's `listing_error_from_volume_error` ships the
    /// typed git kind to `ErrorPane` as the `Git` reason (category from the
    /// kind, no baked prose) without parsing strings; the FE renders the
    /// git-specific copy. Built by the volume hooks in `file_system::git::mod`
    /// (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`).
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
            Self::StaleDestinationHandle(path) => write!(f, "Destination folder handle was stale: {}", path),
            Self::IoError { message, .. } => write!(f, "I/O error: {}", message),
            Self::FriendlyGit(err) => write!(f, "git: {}", err),
        }
    }
}

impl std::error::Error for VolumeError {}

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

#[cfg(test)]
mod scan_conflict_serde_tests {
    use super::*;

    #[test]
    fn scan_conflict_round_trips_directory_flags() {
        let conflict = ScanConflict {
            source_path: "photos".to_string(),
            dest_path: "/dst/photos".to_string(),
            source_size: 0,
            dest_size: 4_096,
            source_modified: Some(1_700_000_000),
            dest_modified: Some(1_700_000_001),
            source_is_directory: true,
            dest_is_directory: true,
        };

        let json = serde_json::to_string(&conflict).unwrap();
        // camelCase on the wire (matches the FE binding).
        assert!(json.contains("\"sourceIsDirectory\":true"), "json was: {json}");
        assert!(json.contains("\"destIsDirectory\":true"), "json was: {json}");

        let back: ScanConflict = serde_json::from_str(&json).unwrap();
        assert!(back.source_is_directory);
        assert!(back.dest_is_directory);
    }

    #[test]
    fn scan_conflict_round_trips_type_mismatch_flags() {
        let conflict = ScanConflict {
            source_path: "data".to_string(),
            dest_path: "/dst/data".to_string(),
            source_size: 10,
            dest_size: 20,
            source_modified: None,
            dest_modified: None,
            source_is_directory: true,
            dest_is_directory: false,
        };

        let back: ScanConflict = serde_json::from_str(&serde_json::to_string(&conflict).unwrap()).unwrap();
        assert!(back.source_is_directory);
        assert!(!back.dest_is_directory);
    }
}

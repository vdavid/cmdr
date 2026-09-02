//! Shared data types for the `Volume` abstraction.
//!
//! The `Volume` trait and its sub-traits live in [`super`] (`mod.rs`); the plain
//! data types they exchange (errors, scan results, conflict records, progress
//! tallies, space info, mutation events) live here. `mod.rs` re-exports
//! everything in this module, so callers keep importing `volume::VolumeError`,
//! `volume::CopyScanResult`, etc. unchanged.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::entry::FileEntry;

/// Whether [`Volume::create_directory_all`](super::Volume::create_directory_all)
/// had to create the directory it was asked for, or found one already there.
///
/// The distinction is what lets a transfer know the destination is a folder
/// nothing else has ever written into. ❌ It is NOT "the directory is empty":
/// an empty directory that already existed can gain an entry from another
/// process at any moment, and a directory we created a second ago cannot have
/// held anything before that. Only the second claim is safe to skip a conflict
/// check on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryCreation {
    /// This call created the leaf directory. It was empty at that instant.
    Created,
    /// The leaf directory was already there. Anything may be inside it.
    AlreadyExisted,
}

/// What a live watch on a listing actually observes.
///
/// A boolean can't answer this. "Is this listing watched?" has a third answer on
/// an OS-mounted network share: yes, and the watch is blind to the writers that
/// matter most. Naming that state is the point of this enum, so a caller has to
/// decide which side of it belongs on rather than defaulting to the comfortable
/// `true`.
///
/// The rule for a new backend: pick the variant that matches what your
/// notification channel is wired to, not what you wish it covered. Under-claiming
/// costs a re-read; over-claiming hands stale entries to a delete walker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchCoverage {
    /// No live watch for this listing. Nothing is keeping it fresh.
    ///
    /// Also the honest answer while a watch is being established: a listing that
    /// exists but isn't wired up yet is not being kept fresh yet.
    None,
    /// A live watch that reports only what THIS machine wrote.
    ///
    /// An OS-mounted network share (SMB, NFS, AFP, WebDAV) watched by FSEvents:
    /// it's a local-VFS notifier, not a share notifier, so a write by another
    /// client on the share produces no event at all. Good enough to keep a pane
    /// current with the user's own work, never good enough to skip a read before
    /// a destructive operation. (Verified on macOS 26.5.2 against a live `smbfs`
    /// mount, 2026-08-08: a write from another client produced no event in 30 s,
    /// while a write through the mount delivered immediately. See
    /// `docs/notes/silent-inertness-hunt-2026-08-08.md`.)
    ThisMachineOnly,
    /// A live watch that reports every change to the directory, whoever made it.
    ///
    /// A local disk under FSEvents, an SMB share under `CHANGE_NOTIFY`, an MTP
    /// device forwarding its own object events. The only variant that lets a
    /// caller substitute a cached listing for a real read.
    EveryWriter,
}

/// Describes a change to a directory's contents on a specific volume.
///
/// Used by `file_system::listing::caching::notify_directory_changed` to apply targeted cache updates
/// and emit `directory-diff` events to the frontend.
///
/// `Clone` so the SMB watch→index translator (`indexing::transports::smb::watch`) can stash a
/// change in its mid-scan replay buffer without taking ownership away from the
/// pane-update path.
#[derive(Clone)]
pub enum DirectoryChange {
    /// A single entry was added. Includes the full `FileEntry` to insert.
    Added(FileEntry),
    /// A single entry was removed by name.
    Removed(String),
    /// A single entry was modified. Includes the updated `FileEntry`.
    Modified(FileEntry),
    /// An entry was renamed within the same directory.
    Renamed {
        /// The name the entry had before the rename.
        old_name: String,
        /// The entry under its new name, with fresh metadata.
        new_entry: FileEntry,
    },
    /// Unknown or bulk change: trigger a full re-read via the Volume trait.
    FullRefresh,
}

/// One file the one-pass sequential extractor yields (see
/// [`Volume::open_sequential_extract`](super::Volume::open_sequential_extract)):
/// its full source path (in the source volume's namespace, matching what
/// `list_directory` reports) and uncompressed size. Directories are not yielded —
/// the copy engine creates the destination folders from the tree, and reserves the
/// single decode pass for byte-carrying files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedFile {
    /// Full source path, in the source volume's namespace.
    pub source_path: PathBuf,
    /// Uncompressed size in bytes.
    pub size: u64,
}

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

/// What a "Sign in" affordance on a volume may ask a person for.
///
/// ❗ **Read from the live volume at the moment the affordance renders**, ❌
/// never captured when the volume was opened. A backend that authenticates per
/// connection can prove itself with a different credential each time it dials,
/// so a value stored earlier describes a session that may no longer exist, and it
/// goes wrong in both directions: a stale [`Nothing`](Self::Nothing) leaves a
/// volume that now wants a password with no way in, and a stale
/// [`KeyPassphrase`](Self::KeyPassphrase) asks for a secret the session doesn't
/// use. [`Volume::sign_in_prompt`](super::Volume::sign_in_prompt) is the read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SignInPrompt {
    /// ❌ Nothing to ask, so ❌ no sign-in button. The session comes back on its
    /// own (an ssh-agent identity, an unencrypted key file), and there is no
    /// secret a person could type that would help.
    Nothing,
    /// The account's password. Persisted on a successful sign-in, so the next
    /// reconnect is silent.
    Password,
    /// The passphrase on a key file. ❗ Used for that session and ❌ never saved:
    /// persisting it would undo what encrypting the key asked for.
    KeyPassphrase,
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
/// a `volume_id` string.
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
// DEFAULT-OK: zero really is "nothing enumerated yet", the state a listing is in before
// its walk starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListingProgress {
    /// Files enumerated so far, directories excluded.
    pub files: usize,
    /// Directories enumerated so far.
    pub dirs: usize,
    /// Sum of file sizes so far; directories contribute 0.
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
    /// A file or directory was renamed within the same parent.
    Renamed {
        /// The name before the rename.
        from: String,
        /// The name after it.
        to: String,
    },
}

/// Result of scanning a path for copy operation.
#[derive(Debug, Clone)]
pub struct CopyScanResult {
    /// Files found in the scanned subtree.
    pub file_count: usize,
    /// Directories found in it.
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

/// What a volume can say about its room.
///
/// Two shapes, because two situations are genuinely different. A disk, a share,
/// or a quota'd account has a TOTAL, so "free" and "how full" both mean
/// something. Storage with no quota at all has no total, and the only honest
/// number is what's already stored: a stock Nextcloud account is the live case,
/// answering RFC 4331 `quota-available-bytes: -3` (sabre/dav's unlimited
/// sentinel) next to a real `quota-used-bytes`.
///
/// ❌ Three `Option`s would let a caller build an `available` with no `total`,
/// which is a percentage with no denominator: a fill bar at an invented figure,
/// and the 80% / 95% warning bands firing on a volume that can't run out. Here
/// that value can't be constructed, so no caller has to remember not to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SpaceInfo {
    /// The volume has a ceiling, so it can be full and a percentage means
    /// something.
    Bounded {
        /// Capacity, in bytes.
        total_bytes: u64,
        /// Room left, in bytes.
        available_bytes: u64,
        /// Already stored, in bytes.
        used_bytes: u64,
    },
    /// No ceiling. Only what's stored is known, so there's nothing to fill a bar
    /// against and no band to warn in.
    Unbounded {
        /// Already stored, in bytes.
        used_bytes: u64,
    },
}

impl SpaceInfo {
    /// A bounded volume whose used figure is simply what isn't free.
    ///
    /// Most backends are here: SMB, MTP, ADB, and a quota'd WebDAV account each
    /// report two numbers that add up. `statvfs` does NOT (its reserved blocks
    /// are neither free nor stored), so `local_posix` builds [`Self::Bounded`]
    /// itself with all three.
    pub fn bounded(total_bytes: u64, available_bytes: u64) -> Self {
        Self::Bounded {
            total_bytes,
            available_bytes,
            used_bytes: total_bytes.saturating_sub(available_bytes),
        }
    }

    /// Bytes already stored. Every volume that answers at all knows this one.
    pub fn used_bytes(&self) -> u64 {
        match *self {
            Self::Bounded { used_bytes, .. } | Self::Unbounded { used_bytes } => used_bytes,
        }
    }

    /// Room left, or `None` when there's no total to subtract from.
    ///
    /// ❗ The transfer pre-flight reads `None` as "can't tell, go ahead", ❌
    /// never as "no room": an unbounded destination is the one place a copy can
    /// always fit.
    pub fn available_bytes(&self) -> Option<u64> {
        match *self {
            Self::Bounded { available_bytes, .. } => Some(available_bytes),
            Self::Unbounded { .. } => None,
        }
    }

    /// Capacity, or `None` when the volume has no ceiling.
    pub fn total_bytes(&self) -> Option<u64> {
        match *self {
            Self::Bounded { total_bytes, .. } => Some(total_bytes),
            Self::Unbounded { .. } => None,
        }
    }
}

/// Information about a source item for conflict scanning.
#[derive(Debug, Clone)]
pub struct SourceItemInfo {
    /// The item's own file name.
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

/// What is known about whether a volume's connection is still answering.
///
/// Always carried in an `Option`, because the interesting state is the THIRD one:
/// `None`, "we have no evidence either way", which is what a client with no
/// keepalive can honestly say about a server that has gone quiet. A bare `bool`
/// would force every caller to guess, and the guess that elapsed silence means
/// death is exactly the one that kills healthy slow transfers.
///
/// Produced by [`Volume::connection_liveness`](super::Volume::connection_liveness).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionLiveness {
    /// The connection answered inside its keepalive window.
    Alive,
    /// The connection failed its keepalive: it is gone, not slow.
    Dead,
}

/// Error type for volume operations, and the value the frontend renders words
/// from.
///
/// ❗ **This crosses IPC as a discriminated union and the frontend owns 100% of
/// the prose.** Adjacently tagged (`{ "type": "notFound", "data": "/path" }`)
/// rather than the internally-tagged shape the rest of the app uses, because
/// most variants carry a positional payload; converting all of them to named
/// fields would touch ~380 call sites and buy only a flatter JSON object.
/// Rename a variant and the TS union member has to move with it, the same
/// contract `ListingErrorReason` and `FriendlyGitErrorKind` carry.
///
/// The `String`s in here are LOG AND TECHNICAL-DETAIL text, never a sentence
/// shown on its own: the path-carrying variants carry a path (see
/// [`from_io_at`](Self::from_io_at)), and the diagnostic-carrying ones carry
/// whatever the backend said, for the technical-details disclosure.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VolumeError {
    /// No such path. Carries the path.
    NotFound(String),
    /// The OS refused access. Carries the path.
    PermissionDenied(String),
    /// The destination already exists. Carries the path.
    AlreadyExists(String),
    /// Not supported by this volume type.
    NotSupported,
    /// Device went away mid-operation.
    DeviceDisconnected(String),
    /// The device's session died mid-operation but the device itself is still
    /// attached, and a reopen is already running in the background (MTP: a PTP
    /// `DeviceReset`, typically after a cancelled or timed-out transfer). The
    /// operation that tripped it is lost, but retrying in a few seconds works —
    /// ❌ never map this to `DeviceDisconnected`, which would tear a live device
    /// out of the sidebar. MTP-only today. See `mtp/connection/DETAILS.md`
    /// § "Session reset is not a disconnect".
    DeviceSessionReset(String),
    /// Device or volume is read-only.
    ReadOnly(String),
    /// Device storage is full.
    StorageFull {
        /// What the backend reported, for the technical-details panel.
        message: String,
    },
    /// Connection timed out.
    ConnectionTimeout(String),
    /// Operation was cancelled by the user (progress callback returned Break).
    Cancelled(String),
    /// The path is a directory, not a file (for example, SMB STATUS_FILE_IS_A_DIRECTORY).
    IsADirectory(String),
    /// The destination can't hold this name, whatever it's asked to do with it.
    ///
    /// Distinct from [`NotFound`](Self::NotFound): the backend never got as far as
    /// looking, so retrying the same name can only fail the same way. The only fix
    /// is a different name, which is why it can't ride as a generic
    /// [`IoError`](Self::IoError) — that one offers a retry.
    ///
    /// SMB raises it from `STATUS_OBJECT_NAME_INVALID`. smb2 maps the characters
    /// SMB2 forbids outright (`"`, `*`, `:`, `<`, `>`, `?`, `\`, `|`, the control
    /// characters, and a trailing space or period) into the Unicode private-use
    /// area, so those copy through fine and what reaches here is a reserved Windows
    /// device name (`CON`, `NUL`, `LPT1`), a name past the server's own length
    /// limit, or a character the server's filesystem can't store. Carries what the
    /// backend reported, for the technical-details panel.
    InvalidName(String),
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
    /// Anything the backend couldn't classify further. The classifier
    /// re-dispatches on `raw_os_error` when one is present.
    IoError {
        /// What the OS or backend reported, for the technical-details panel.
        message: String,
        /// The errno behind it, when there was one.
        raw_os_error: Option<i32>,
    },
    /// A password-protected archive needs a password to browse (header-encrypted
    /// 7z) or extract (any encrypted entry). `wrong_attempt` is `false` when no
    /// password has been tried and `true` when the supplied one was rejected, so
    /// the frontend can prompt afresh vs. say "that password didn't work". The
    /// archive backend raises this; the frontend supplies a per-archive password
    /// via `set_archive_password` and retries. Carries no path — the failing path
    /// is the one the caller was reading.
    NeedsPassword {
        /// `false` when no password has been tried yet, `true` when the supplied
        /// one was rejected.
        wrong_attempt: bool,
    },
    /// Structured git-layer failure.
    ///
    /// Carries the full `FriendlyGitError` (kind + path + optional raw detail)
    /// so the listing pipeline's `listing_error_from_volume_error` ships the
    /// typed git kind to `ErrorPane` as the `Git` reason (category from the
    /// kind, no baked prose) without parsing strings; the FE renders the
    /// git-specific copy. Built by the volume hooks in `file_system::git::mod`
    /// (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`).
    FriendlyGit(crate::volume::friendly_error::git::FriendlyGitError),
}

/// ❗ **For logs and debugging only.** Nothing a user reads comes from here: the
/// frontend renders every word from the typed variant (`src/lib/error-messages/`
/// and `src/lib/file-operations/…`), through the message catalog, in ten locales.
/// A `to_string()` that reaches a UI surface is a bug, not a shortcut.
impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Path not found: {}", path),
            Self::PermissionDenied(path) => write!(f, "Permission denied: {}", path),
            Self::AlreadyExists(path) => write!(f, "Already exists: {}", path),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::DeviceDisconnected(msg) => write!(f, "Device disconnected: {}", msg),
            Self::DeviceSessionReset(msg) => write!(f, "Device session restarted: {}", msg),
            Self::ReadOnly(msg) => write!(f, "Read-only: {}", msg),
            Self::StorageFull { message } => write!(f, "Storage full: {}", message),
            Self::ConnectionTimeout(msg) => write!(f, "Connection timed out: {}", msg),
            Self::Cancelled(msg) => write!(f, "Cancelled: {}", msg),
            Self::IsADirectory(path) => write!(f, "Is a directory: {}", path),
            Self::InvalidName(msg) => write!(f, "Name not usable at the destination: {}", msg),
            Self::DeletePending(path) => write!(f, "Delete pending: {}", path),
            Self::StaleDestinationHandle(path) => write!(f, "Destination folder handle was stale: {}", path),
            Self::IoError { message, .. } => write!(f, "I/O error: {}", message),
            Self::NeedsPassword { wrong_attempt } => {
                if *wrong_attempt {
                    f.write_str("Archive password is incorrect")
                } else {
                    f.write_str("Archive is password-protected")
                }
            }
            Self::FriendlyGit(err) => write!(f, "git: {}", err),
        }
    }
}

impl std::error::Error for VolumeError {}

impl VolumeError {
    /// Classifies a [`std::io::Error`] that happened at a KNOWN path.
    ///
    /// ❗ **The path is the payload, not context.** [`NotFound`](Self::NotFound),
    /// [`PermissionDenied`](Self::PermissionDenied), and
    /// [`AlreadyExists`](Self::AlreadyExists) are defined to carry the path, and
    /// the transfer layer takes that literally: `map_volume_error` forwards the
    /// string straight into `SourceNotFound { path }`, which the frontend renders
    /// as the name of the file the user is missing. A bare `io::Error` has no path
    /// inside it, so there is deliberately no `From<io::Error>` to reach for; use
    /// this at every site that knows which path it was touching, and
    /// [`from_io_without_path`](Self::from_io_without_path) only where none exists.
    ///
    /// `assert_not_found_carries_the_path` holds every backend to it.
    pub fn from_io_at(err: &std::io::Error, path: impl AsRef<std::path::Path>) -> Self {
        let located = || path.as_ref().to_string_lossy().into_owned();
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(located()),
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied(located()),
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists(located()),
            _ => Self::from_io_without_path(err),
        }
    }

    /// Classifies a [`std::io::Error`] with no path behind it: a pipe, a socket, a
    /// channel, a subprocess.
    ///
    /// Always [`IoError`](Self::IoError), because the three path-carrying variants
    /// would have nothing honest to carry. Prefer
    /// [`from_io_at`](Self::from_io_at) wherever a path is in scope.
    pub fn from_io_without_path(err: &std::io::Error) -> Self {
        Self::IoError {
            message: err.to_string(),
            raw_os_error: err.raw_os_error(),
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

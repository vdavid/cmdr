//! Shared data types for the `Volume` abstraction.
//!
//! The `Volume` trait and its sub-traits live in [`super`] (`mod.rs`); the plain
//! data types they exchange (scan results, conflict records, progress tallies,
//! space info, mutation events, stream and write modes) live here, and the error
//! union they fail with lives in `error.rs`. `mod.rs` re-exports both, so callers
//! keep importing `volume::VolumeError`, `volume::CopyScanResult`, etc.
//! unchanged.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::entry::FileEntry;
use crate::metadata::{CarriedMetadata, MetadataClasses, MetadataReport};

/// Whether a background index walk descends into one directory it met in a
/// listing ([`Volume::index_walk`](super::Volume::index_walk)).
///
/// A walk keeps the directory's own row either way; this decides only whether it
/// lists what's inside. The answer is the backend's, because only the backend
/// knows which of its trees hold files and which are views onto the kernel, or a
/// second path onto files the walk already covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexWalk {
    /// Walk the directory's contents. A link answered this way is walked, and
    /// indexed, as the folder it points at.
    Descend,
    /// Keep the directory's row and leave its contents unwalked, so its size reads
    /// as unknown rather than as zero.
    RowOnly,
}

impl IndexWalk {
    /// The rule a walk follows where nothing else decides: a real directory is
    /// walked, and a link is the one row it is. Walking a link would count its
    /// target's files a second time, and a link loop would keep the walk going for
    /// as long as its paths grew.
    pub fn unless_link(is_symlink: bool) -> Self {
        if is_symlink { Self::RowOnly } else { Self::Descend }
    }
}

/// What [`Volume::write_from_stream`](super::Volume::write_from_stream) may do
/// to a file that already holds the destination name.
///
/// Only the caller knows which it is, because only the caller knows whether it
/// checked the name or claimed it: the backend sees the same occupied path
/// either way. A required argument, so no call site can leave it to a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// The name was free when the caller looked, and nobody decided what should
    /// happen to a file there. Something at it now (another writer got there
    /// mid-upload) is refused with [`VolumeError::AlreadyExists`](super::VolumeError::AlreadyExists) and left
    /// untouched, the same contract as [`Volume::create_file`](super::Volume::create_file).
    /// Backends use their atomic primitive where they have one (`O_EXCL`,
    /// SMB's `FileCreate`, `SSH_FXF_EXCL`, WebDAV's `If-None-Match: *`) and
    /// check just before writing where they don't (MTP, ADB).
    CreateNew,
    /// [`CreateNew`](Self::CreateNew) for a name inside a folder THIS operation
    /// created, which the creation proved empty at that moment (the
    /// `create_directory` that made it succeeded rather than finding it).
    ///
    /// It refuses an occupied name exactly as `CreateNew` does wherever that
    /// costs nothing extra (an atomic create, a conditional header). A backend
    /// whose no-overwrite check is a request of its own (an object store's HEAD
    /// before each write) may skip that request: a file another writer puts in
    /// the brand-new folder during the operation is then overwritten, the
    /// window the caller accepted by handing down this fact. ❌ Only from the
    /// caller that made the folder; never a guess at write time.
    CreateNewInFreshFolder,
    /// Whatever holds the name is the caller's to replace: a temp it minted, a
    /// name it claimed with a placeholder, a file the user chose to overwrite.
    ///
    /// ❗ A backend may write it IN PLACE (local disk, SFTP, SMB, WebDAV, and
    /// ADB do), so the name can hold a partial until the write ends, and after a
    /// crash. A caller that must keep a user's filename whole stages first (the
    /// transfer engine always does); a backend ❌ never stages this mode again on
    /// top of the caller's temp, which costs a rename per file.
    CreateOrReplace,
}

impl WriteMode {
    /// Whether a write in this mode must not replace what holds the name:
    /// everything but [`CreateOrReplace`](Self::CreateOrReplace).
    pub fn refuses_occupied(self) -> bool {
        !matches!(self, Self::CreateOrReplace)
    }
}

/// Whether a stream's final byte length is known before writing starts.
///
/// Real files carry [`Known`](Self::Known). Generated streams may carry
/// [`Unknown`](Self::Unknown), so callers never disguise an absent length as
/// zero. A backend must explicitly opt into unknown-length writes before it can
/// receive one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamLength {
    /// The stream must contain exactly this many bytes.
    Known(u64),
    /// The final byte count is unavailable until the stream reaches EOF.
    Unknown,
}

impl StreamLength {
    /// Returns the exact byte count when one was declared.
    pub const fn known(self) -> Option<u64> {
        match self {
            Self::Known(bytes) => Some(bytes),
            Self::Unknown => None,
        }
    }
}

/// One progress update from a streaming write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamWriteProgress {
    /// Bytes accepted by the destination so far.
    pub bytes_written: u64,
    /// The declared final length, or unknown for generated output.
    pub expected_length: StreamLength,
}

/// The modified time [`Volume::get_metadata`](super::Volume::get_metadata)
/// answers for a file a write just landed, when the write knows it without
/// asking.
///
/// The operation log records each copied file's landed time so an undo can tell
/// an untouched copy from one edited since, and the undo rechecks it with
/// `get_metadata`. A backend whose listing reports a DIFFERENT time than
/// `get_metadata` (S3: a listing's `LastModified` is the upload, a HEAD's
/// `x-amz-meta-mtime` is the source's own date) would have every folder copy
/// read as edited, so it reports the time here and nothing is read back.
/// The contract, both halves: `conformance::assert_landed_time_agrees_with_get_metadata`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandedModified {
    /// The write doesn't know. A caller that needs the time reads it back, and
    /// a listing of the file's folder must then agree with `get_metadata`.
    Unknown,
    /// `get_metadata` of the written path answers exactly this `modified_at`
    /// (`None` included) until something else changes the file.
    Known(Option<u64>),
}

/// What a [`Volume::write_from_stream`](super::Volume::write_from_stream) or a
/// [`Volume::copy_on_server`](super::Volume::copy_on_server) that succeeded
/// did: the bytes it wrote, what of the source's metadata the destination
/// refused on the way, and the landed file's modified time when the write knows
/// it ([`LandedModified`]).
///
/// The refusals are already narrowed to what the source had worth keeping
/// ([`CarriedMetadata::worth_keeping`]), so a caller counts them as they are:
/// the only constructor that takes a report narrows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamWritten {
    /// Bytes written.
    pub bytes: u64,
    refused: MetadataClasses,
    landed_modified: LandedModified,
}

impl StreamWritten {
    /// `bytes` written, and nothing the destination was handed refused: every
    /// class kept, or one it can't hold at all (which `MetadataCaps` already
    /// says).
    pub const fn nothing_refused(bytes: u64) -> Self {
        Self {
            bytes,
            refused: MetadataClasses::NONE,
            landed_modified: LandedModified::Unknown,
        }
    }

    /// The same write, reporting the landed file's modified time. Only a time
    /// `get_metadata` will answer exactly: a guess is worse than
    /// [`LandedModified::Unknown`], because the undo would see drift on a file
    /// nobody touched and keep it.
    #[must_use]
    pub const fn landed_at(mut self, landed: LandedModified) -> Self {
        self.landed_modified = landed;
        self
    }

    /// The landed file's modified time as `get_metadata` answers it, when the
    /// write knows ([`LandedModified`]).
    pub const fn landed_modified(&self) -> LandedModified {
        self.landed_modified
    }

    /// `bytes` written, and `report` saying what became of `carried` (and the
    /// source's `modified` date). A refused class counts only when `carried`
    /// had something worth keeping in it, against a fresh file's default mode.
    pub fn with_report(
        bytes: u64,
        report: &MetadataReport,
        carried: &CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> Self {
        Self {
            bytes,
            refused: report
                .refused_classes()
                .intersection(carried.worth_keeping(modified, crate::metadata::default_file_mode())),
            landed_modified: LandedModified::Unknown,
        }
    }

    /// The classes the destination refused of what the source had worth
    /// keeping.
    pub const fn refused(&self) -> MetadataClasses {
        self.refused
    }
}

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

/// Whatever a backend keeps alive while a pane shows one of its directories,
/// from [`Volume::watch_listing`](super::Volume::watch_listing). Dropping it
/// releases that.
///
/// Opaque on purpose: the listing engine only holds it for the listing's life,
/// and the backend decides what it keeps (the SMB backend: a watch on the DFS
/// link the directory sits behind).
pub struct ListingWatch(#[allow(dead_code, reason = "held for its Drop alone")] Box<dyn std::any::Any + Send + Sync>);

impl ListingWatch {
    /// Wraps `hold`, whose `Drop` releases what the backend keeps for the listing.
    pub fn new(hold: impl std::any::Any + Send + Sync) -> Self {
        Self(Box::new(hold))
    }
}

/// Describes a change to a directory's contents on a specific volume.
///
/// Used by `cmdr_listing::Listings::directory_changed` to apply targeted cache updates
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
    /// The backend re-read the directory and these are its contents now.
    ///
    /// For a backend that learns "something under here moved" without learning
    /// what: it re-lists once and hands the result over, and the HOST diffs
    /// against what the panes hold and patches them. One call however many
    /// entries came back, which is what keeps a device event out of a per-entry
    /// loop.
    ///
    /// Different from [`FullRefresh`](Self::FullRefresh), which asks the host to
    /// do the re-read through `Volume::list_directory`. Report `Replaced` when
    /// you already have the entries in hand (an MTP event loop that must
    /// invalidate its own path cache before re-listing anyway); report
    /// `FullRefresh` when the host should go and get them.
    Replaced(Vec<FileEntry>),
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
///
/// Build one with a struct literal ending in `..ListingProgress::default()`, so a
/// new field lands here and nowhere else.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListingProgress {
    /// Files enumerated so far, directories excluded.
    pub files: usize,
    /// Directories enumerated so far.
    pub dirs: usize,
    /// Sum of file sizes so far; directories contribute 0.
    pub bytes: u64,
    /// What the listing is doing before its first entry, when that takes a while
    /// and has its own measure (a backup's index load). `None` while it's only
    /// enumerating, which is nearly every listing.
    pub phase: Option<ListingPhase>,
}

/// The work a listing does before it can enumerate, with its own progress.
///
/// Typed and word-free: the frontend words each phase. Movement in a phase counts
/// as progress for the listing's stall watch, since no entry arrives meanwhile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ListingPhase {
    /// Loading a backup repository's index, which every folder inside a snapshot
    /// needs first. Counts the repo's index files and their bytes, read so far
    /// and in all.
    LoadingIndex {
        /// Bytes of index files read so far.
        done_bytes: u64,
        /// The index files' sizes, summed.
        total_bytes: u64,
        /// Index files read so far.
        done_files: u32,
        /// Index files in the repo.
        total_files: u32,
    },
}

impl ListingProgress {
    /// A tick that reports only `phase`, with nothing enumerated yet.
    pub fn in_phase(phase: ListingPhase) -> Self {
        Self {
            phase: Some(phase),
            ..Self::default()
        }
    }

    /// Total entries enumerated so far (files + dirs). Convenience for the
    /// streaming listing UI, which renders one "Loaded N entries…" line.
    pub fn entries(&self) -> usize {
        self.files + self.dirs
    }

    /// The whole of `entries` folded into one tick, for a backend whose listing
    /// arrives in a single piece and so has nothing incremental to report.
    ///
    /// The trait asks every backend to call `on_progress` at least once, and an
    /// atomic listing (an archive's central directory, a git snapshot's tree)
    /// has exactly one honest thing to say. Shared so the two agree on what a
    /// directory contributes: a count, and no bytes.
    pub fn of(entries: &[FileEntry]) -> Self {
        let mut progress = Self::default();
        for entry in entries {
            if entry.is_directory {
                progress.dirs += 1;
            } else {
                progress.files += 1;
                progress.bytes += entry.size.unwrap_or(0);
            }
        }
        progress
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
    /// The scanned top-level path's own modification date (Unix seconds), from
    /// the same stat, or `None` when that stat carried none (an S3 prefix) or
    /// the result is an aggregate over several paths.
    ///
    /// The copy pipeline dates a copied top-level FOLDER with it, once the
    /// folder's contents have landed, so that date costs no round trip of its
    /// own. `crates/cmdr-operations/src/transfer/volume/DETAILS.md`
    /// § "Copies keep the source's metadata".
    pub top_level_modified_at: Option<u64>,
    /// The scanned top-level path's own creation date (Unix seconds), from the
    /// same stat, or `None` when that stat carried none (most backends but
    /// local, SMB, and MTP) or the result is an aggregate over several paths.
    /// What a copied top-level FOLDER is born with, for the same reason.
    pub top_level_created_at: Option<u64>,
}

impl CopyScanResult {
    /// A local top-level path's `(modified, created)` dates in the whole Unix
    /// seconds [`Self::top_level_modified_at`] and [`Self::top_level_created_at`]
    /// carry, off the `lstat` a local scan already took. `None` for a date the
    /// stat lacks (or no stat at all).
    pub fn local_top_level_dates(meta: Option<&std::fs::Metadata>) -> (Option<u64>, Option<u64>) {
        let secs = |date: std::io::Result<std::time::SystemTime>| {
            date.ok()
                .and_then(|date| date.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|since| since.as_secs())
        };
        match meta {
            Some(meta) => (secs(meta.modified()), secs(meta.created())),
            None => (None, None),
        }
    }
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
    /// Every file's size and date, kept only by a backend whose operations are
    /// billed per object (S3, through `ScanSource::keeps_files`), for a cost
    /// estimate; `None` everywhere else, and wherever any part of the scan
    /// answered without walking (a cached listing).
    pub files: Option<Vec<ScannedFile>>,
}

/// One file a scan walked past, as a cost estimate needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScannedFile {
    /// Logical size in bytes.
    pub size: u64,
    /// Last-modified time, Unix seconds; on an object store, the upload time.
    pub modified_at: Option<u64>,
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

/// Whether a write into a folder would be taken, as far as the backend can tell
/// without writing anything ([`Volume::write_access_at`](super::Volume::write_access_at)).
///
/// Three answers, because "can't tell" is its own truth: a backend with no way to
/// ask answers [`Unknown`](Self::Unknown), ❌ never a guess in either direction.
/// The transfer pre-flight asks this BEFORE it measures space, so a folder nothing
/// can be written to never reads as a full one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WriteAccess {
    /// This user can write here.
    Writable,
    /// Nothing this user sends can land here.
    Unwritable {
        /// Why, as far as the backend can tell.
        reason: UnwritableReason,
    },
    /// The backend has no way to tell without writing.
    Unknown,
}

/// Why a folder takes no writes ([`WriteAccess::Unwritable`]).
///
/// Read-only and no-permission are different truths with different fixes, so a
/// backend that can tell them apart says which. ❌ Never pick one when the backend
/// can't tell: that's [`Unexplained`](Self::Unexplained).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UnwritableReason {
    /// The filesystem holding the folder is mounted read-only, so nobody can write there.
    ReadOnlyFilesystem,
    /// The filesystem takes writes, but this user may not write into the folder.
    NoPermission,
    /// This user can't write here, and the backend can't tell whether the
    /// filesystem or a permission is the reason.
    Unexplained,
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
    /// The server put a byte on the wire inside the liveness window, the bytes
    /// of a response still arriving included.
    Alive,
    /// The keepalive is armed, a request is outstanding, and the server has put
    /// nothing at all on the wire for a whole liveness window, probe replies
    /// included: gone, not slow. Still evidence rather than a licence to act;
    /// see [`Volume::connection_liveness`](super::Volume::connection_liveness).
    Dead,
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

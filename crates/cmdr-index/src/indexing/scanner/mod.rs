//! Parallel directory walker for drive indexing.
//!
//! Drives the hang-tolerant [`walker`] engine with an [`InsertVisitor`] to run
//! three walks over one body: a full-volume scan ([`scan_volume`]), a targeted
//! subtree REBUILD ([`scan_subtree`]), and the search-driven walk over ground the
//! index has no claim on ([`cover_subtree`]). [`ScanRoot`] is what separates them,
//! and the difference that matters is whether the walk may delete.
//!
//! Discovered entries are sent in batches to the [`IndexWriter`] for insertion
//! into the SQLite index, and — when a search is listening — to that search over a
//! second channel, as [`CoveredEntry`] batches.
//!
//! What a walk refuses to descend into is one value, [`WalkPolicy`], resolved
//! before the walk starts and applied per child in the visitor: the structural
//! exclusions (macOS system directories, virtual filesystems, junk) with an
//! on/off switch, and the device a search walk must not leave. Either verdict
//! means no row at all, not just no descent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

use crate::indexing::IndexPathSpace;
use crate::indexing::store::{IndexStore, UnreadableCause, resolve_scan_root};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};
use cmdr_fs::pluralize::{pluralize, pluralize_with};

mod exclusions;
pub use exclusions::SYSTEM_DIR_EXCLUDES;
pub(in crate::indexing) use exclusions::*;

mod heartbeat;
pub(crate) use heartbeat::WalkHeartbeat;

mod live_emit;
pub(in crate::indexing) use live_emit::{EMIT_INTERVAL, EmitPacer};

mod insert_visitor;
use insert_visitor::{InsertVisitor, UnreadableIds};

mod walker;
use walker::{
    DEFAULT_GIVE_UP_AFTER, DEFAULT_PER_ENTRY_ALLOWANCE, DirTask, ReadDirFn, WalkConfig, default_reader, walk,
};

// The batched `getattrlistbulk` read and the child file-type vocabulary that goes
// with it, re-exported for the serial reconcile walk
// (`reconcile::reconciler::read_fs_children`), which reads directories exactly the
// way the fresh scan does but on its own guarded worker thread. macOS-only, because
// that reader is: no other platform has `getattrlistbulk`, and nothing else outside
// the scanner names either type. The walker engine stays private, and `RawDirEntry`
// with it: only the visitor ever sees one.
#[cfg(target_os = "macos")]
pub(in crate::indexing) use walker::RawFileType;
#[cfg(target_os = "macos")]
pub(in crate::indexing) use walker::bulk_read::{BulkDirRead, bulk_read_dir_unwatched};

/// How long one LOCAL directory read may go without producing anything before
/// it's abandoned. It measures a STALL, never total duration: a disconnected File
/// Provider mount blocks and delivers nothing (abandoned in 15 s, its subtree
/// pruned, so a dead mount costs a handful of frontier dirs), while a big healthy
/// directory keeps delivering and is read to completion however long it takes.
///
/// The serial reconcile's `GuardedReader` reuses this constant on a reader with no
/// progress signal, where it acts as a plain total cap — the same 15 s verdict for
/// a read that has produced nothing. (The network scanner's 120 s is tuned for
/// SMB-over-WAN.)
pub(in crate::indexing) const LOCAL_LIST_TIMEOUT: Duration = Duration::from_secs(15);

/// How often the walker watchdog checks for over-timeout reads (also the ceiling
/// on caller-cancel latency).
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(test)]
mod convergence_tests;
#[cfg(test)]
mod heartbeat_tests;
#[cfg(test)]
mod live_emit_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod test_fixtures;
#[cfg(test)]
mod tests;

/// Number of dir ids per `MarkDirsListed` / `MarkDirsUnreadable` message
/// (mirrors `network_scanner`).
const MARK_CHUNK: usize = 10_000;

/// Stamp the directories this walk couldn't read, so the coverage frontier stops
/// offering them to every later search. A no-op when empty.
///
/// One message per cause: a refusal is the user's to fix and stands until they do,
/// while abandoned ground is Cmdr's to retry on the backoff in
/// `writer/abandoned_retry.rs`. Which ids land in which is decided in
/// `insert_visitor.rs`.
fn send_unreadable_marks(ids: &UnreadableIds, writer: &IndexWriter) {
    let batches = [
        (&ids.denied, UnreadableCause::Denied),
        (&ids.abandoned, UnreadableCause::Abandoned),
    ];
    for (ids, cause) in batches {
        for chunk in ids.chunks(MARK_CHUNK) {
            if let Err(e) = writer.send(WriteMessage::MarkDirsUnreadable {
                ids: chunk.to_vec(),
                cause,
            }) {
                log::warn!("Scanner: failed to send MarkDirsUnreadable: {e}");
            }
        }
    }
}

// ── Types ────────────────────────────────────────────────────────────

/// What the scan root IS, which decides what the scan may do to the ground under
/// it. Replaces an `is_volume_root` boolean, because the third case is exactly
/// the one a boolean couldn't say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum ScanRoot {
    /// The whole volume. The root maps to `ROOT_ID` whatever its path is, the
    /// epoch is seeded rather than read, and the per-child exclusion policy
    /// applies.
    Volume,
    /// A subtree the index already holds, rebuilt from scratch: descendants are
    /// deleted first and the walk re-inserts them under fresh ids.
    Rebuild,
    /// Ground the index has no claim on — a coverage frontier node. ❌ NOTHING is
    /// deleted: a search picked this directory precisely because nothing had
    /// listed it, and deleting under it would throw away whatever an earlier
    /// interrupted walk or a verification pass had already learned. The walk may
    /// only ADD.
    ///
    /// It rests on the root being genuinely empty in the index, which
    /// [`cover_subtree`] checks before it walks: the walk allocates fresh ids for
    /// every name it finds, and `INSERT OR IGNORE` would silently drop a fresh row
    /// that collided with a pre-existing sibling, orphaning everything the walk
    /// then attributed to the id it dropped.
    Virgin,
}

impl ScanRoot {
    /// Whether this is the volume-root scan, which is what selects `ROOT_ID` and
    /// the seeded epoch.
    fn is_volume(self) -> bool {
        self == ScanRoot::Volume
    }

    /// Whether this walk must stay on the device it started on.
    ///
    /// Only the search walk does. A search targets ONE volume (Decision 4), so
    /// crossing into another one has left its scope, and the other volume's rows
    /// belong to the other volume's index. A full scan keeps today's behavior: it
    /// bounds itself by path prefix (`/Volumes/` on the boot disk) rather than by
    /// device, and pinning it would silently change what a boot index contains.
    fn stays_on_one_device(self) -> bool {
        self == ScanRoot::Virgin
    }
}

/// What one walk refuses to descend into, resolved before the walk starts.
///
/// Two rules, both about staying on the ground this walk owns, and neither one a
/// second source of scope:
///
/// - the **structural exclusion policy**, whose rules come from the volume KIND
///   ([`ExclusionScope`], never from `is_volume_root`) and which EVERY walk runs:
///   a walk writes the rows a full scan would write, or the index stops matching
///   itself;
/// - the **device** the walk started on, so a filesystem mounted inside the tree
///   is left to its own volume's index ([`ScanRoot::stays_on_one_device`]).
///
/// ⚠️ File Provider domains (Dropbox, iCloud Drive, Google Drive) are NOT a
/// boundary and must never become one: they report the same device as `$HOME`
/// and belong to the boot volume's scope. The guarded walker's stall detection is
/// what makes descending into a disconnected one safe, which is why this needs no
/// help from `cmdr_fs::file_provider::domain_id_for_dir` — that probe answers a
/// different question (where a volume ROOT sits, for the pseudo-filesystem rule).
#[derive(Debug, Clone)]
struct WalkPolicy {
    /// What the scan root is, which decides what the walk may do to the ground
    /// under it.
    root: ScanRoot,
    /// Which structural rules this volume kind gets.
    scope: ExclusionScope,
    /// The device the walk's own root sits on, or `None` when this walk doesn't
    /// pin one (and so descends wherever the tree leads).
    ///
    /// The WALK's root, deliberately, not the volume's: a walk whose root is
    /// itself on a foreign device would otherwise cut away every one of its own
    /// children, list the root, and read as fully covered while holding nothing —
    /// the silent false-complete [`ExclusionTier`] exists to prevent.
    device: Option<u64>,
    /// How a path's device is read. Injected so a test can simulate a mount
    /// without one, the same way [`RootProbes`] injects its two.
    device_of: fn(&Path) -> Option<u64>,
}

impl WalkPolicy {
    /// The policy a walk of `mode` over `root` runs under.
    fn for_walk(mode: ScanRoot, space: &IndexPathSpace, root: &Path) -> Self {
        Self::with_device_probe(mode, space, root, device_of)
    }

    fn with_device_probe(
        mode: ScanRoot,
        space: &IndexPathSpace,
        root: &Path,
        device_of: fn(&Path) -> Option<u64>,
    ) -> Self {
        Self {
            root: mode,
            scope: space.exclusion_scope().clone(),
            // A root whose device can't be read pins nothing: the walk is about to
            // fail to list it anyway, and a `None` pin that cut every child would
            // turn an unreadable root into a false-complete.
            device: mode.stays_on_one_device().then(|| device_of(root)).flatten(),
            device_of,
        }
    }

    /// Whether the structural policy excludes this child. Pure string work plus,
    /// for a directory actually named `proc` / `sys` / `dev`, the root probes.
    fn excludes(&self, path_str: &str) -> bool {
        should_exclude(path_str, &self.scope)
    }

    /// Whether descending into this DIRECTORY would leave the volume the walk is
    /// covering: it sits on a different device, so something is mounted there.
    ///
    /// One `lstat` per directory the walk discovers, and only while a device is
    /// pinned — a full scan pays nothing. A device that can't be read is NOT a
    /// boundary: the walk descends, its read fails, and `visit_read_error` reports
    /// that honestly instead of this guessing.
    fn leaves_the_volume(&self, path: &Path) -> bool {
        let Some(pinned) = self.device else { return false };
        (self.device_of)(path).is_some_and(|device| device != pinned)
    }
}

/// The device a path sits on. On a mount point this is the MOUNTED filesystem's,
/// which is what makes the difference from its parent's the boundary signal.
#[cfg(unix)]
fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).ok().map(|meta| meta.dev())
}

/// No device identity to compare, so no walk ever cuts at a boundary here. The
/// LOCAL guarded walker is macOS / Linux only, so nothing reaches this in
/// practice.
#[cfg(not(unix))]
fn device_of(_path: &Path) -> Option<u64> {
    None
}

/// One entry a walk discovered, in the shape a live consumer needs it.
///
/// Search matches on these while the walk is still running, which is the whole
/// point of Decision 3: the scan is owned by `indexing/` and the matching stays
/// in `search/`, connected by a batched channel, so no matcher ever has to live
/// inside this crate.
///
/// Sizes are the entry's OWN, before hardlink dedup: dedup exists to keep the
/// stored recursive sums honest, and a result row showing a hardlinked file as
/// 0 bytes would just be wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoveredEntry {
    /// Absolute path, in the same space the walk was asked about.
    pub path: PathBuf,
    /// Whether it's a directory.
    pub is_directory: bool,
    /// Whether it's a symlink (never followed).
    pub is_symlink: bool,
    /// Apparent size in bytes, or `None` when it couldn't be read.
    pub logical_size: Option<u64>,
    /// Bytes actually occupied on disk, or `None` when it couldn't be read.
    pub physical_size: Option<u64>,
    /// Last-modified time, seconds since the Unix epoch.
    pub modified_at: Option<u64>,
}

/// Where a walk sends the entries it discovers, one crossing per batch.
///
/// Bounded on purpose: a consumer that falls behind slows the walk down instead
/// of letting an unbounded queue grow to the size of the subtree.
pub(in crate::indexing) type EntrySender = std::sync::mpsc::SyncSender<Vec<CoveredEntry>>;

/// Configuration for a scan operation.
pub struct ScanConfig {
    /// Root path to scan from.
    pub root: PathBuf,
    /// Batch size for sending entries to the writer.
    pub batch_size: usize,
    /// Number of walker worker threads (0 = auto-detect).
    pub num_threads: usize,
    /// The scanned volume's path space: where it's rooted (which selects the
    /// per-child exclusion tier) and whether its inodes are a trustworthy identity.
    /// `boot_disk` for the `/`-rooted scan (keeps it off `/Volumes/`, system trees);
    /// `mount_rooted` for an external drive scan rooted at `/Volumes/X` (skips only
    /// junk basenames, else it would exclude its own subtree and falsely complete
    /// empty). `pub(crate)` because `IndexPathSpace` is a crate-internal type and
    /// `ScanConfig` is only built in-crate.
    pub(crate) space: IndexPathSpace,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/"),
            batch_size: 2000,
            num_threads: 0,
            space: IndexPathSpace::root(),
        }
    }
}

/// Progress counters for an active scan. Atomically updated by the scan thread.
pub struct ScanProgress {
    /// Files and directories recorded so far.
    pub entries_scanned: Arc<AtomicU64>,
    /// Directories among them, the tier-1 progress numerator.
    pub dirs_found: Arc<AtomicU64>,
    /// Resolved post-dedup physical bytes seen so far. Each entry contributes its
    /// `physical_size.unwrap_or(0)` after hardlink dedup, so the live numerator
    /// follows the exact same rules as the stored physical-size sums (directories,
    /// symlinks, and second+ hardlinks contribute 0).
    pub bytes_scanned: Arc<AtomicU64>,
}

/// A point-in-time read of an active scan's progress counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanProgressSnapshot {
    pub entries_scanned: u64,
    pub dirs_found: u64,
    pub bytes_scanned: u64,
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanProgress {
    /// A fresh set of counters for one scan.
    pub fn new() -> Self {
        Self {
            entries_scanned: Arc::new(AtomicU64::new(0)),
            dirs_found: Arc::new(AtomicU64::new(0)),
            bytes_scanned: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Read current progress snapshot.
    pub fn snapshot(&self) -> ScanProgressSnapshot {
        ScanProgressSnapshot {
            entries_scanned: self.entries_scanned.load(Ordering::Relaxed),
            dirs_found: self.dirs_found.load(Ordering::Relaxed),
            bytes_scanned: self.bytes_scanned.load(Ordering::Relaxed),
        }
    }
}

/// Handle returned by `scan_volume` for progress tracking and cancellation.
pub struct ScanHandle {
    pub progress: Arc<ScanProgress>,
    cancel: CancellationToken,
}

impl ScanHandle {
    /// Build a handle around an existing progress + cancel pair. Used by the
    /// `Volume`-trait scanner (`network_scanner`), which owns the walk itself and
    /// just needs the manager-facing progress/cancel surface.
    pub(crate) fn new(progress: Arc<ScanProgress>, cancel: CancellationToken) -> Self {
        Self { progress, cancel }
    }

    /// Signal the scan to stop. Already-written data remains in the DB.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}

/// What a walk covered. Reaching a caller as `Ok` means the walk ran to the END:
/// a cancelled walk's partial totals arrive inside
/// [`ScanError::Cancelled`] instead, so no caller can treat
/// a partial as a completion by forgetting to check a flag.
#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub total_entries: u64,
    pub total_dirs: u64,
    /// Resolved post-dedup physical bytes the scan summed (the final value of the
    /// `bytes_scanned` counter). Apples-to-apples with the stored physical-size sums.
    pub total_physical_bytes: u64,
    pub duration_ms: u64,
}

/// Everything [`run_scan`] produced, INCLUDING whether the walk ran to the end.
///
/// Internal on purpose: the write sequences that follow a walk (stamping listed
/// dirs, the subtree aggregate) need the ids and the epoch on the cancel path
/// too, so the cancelled/completed split only becomes a `Result` at the public
/// entry points, via [`ScanOutcome::into_result`].
#[derive(Debug)]
struct ScanOutcome {
    summary: ScanSummary,
    /// True when the scan's token fired before the walk ended.
    cancelled: bool,
    root_id: i64,
}

impl ScanOutcome {
    /// The caller-facing answer: totals for a finished walk, the typed
    /// cancellation for a stopped one.
    fn into_result(self) -> Result<ScanSummary, ScanError> {
        if self.cancelled {
            Err(ScanError::Cancelled(self.summary))
        } else {
            Ok(self.summary)
        }
    }
}

/// Errors that can occur during scanning.
#[derive(Debug)]
pub enum ScanError {
    Io(std::io::Error),
    WriterSend(String),
    /// The volume ROOT listing SUCCEEDED but returned zero children, so a
    /// reconcile rescan would see an empty live tree and delete every existing
    /// child (blanking the index). Surfaced by the LOCAL reconcile walker
    /// (`local_reconcile`) before it diffs the root, so the completion handler
    /// takes its `Err` arm and writes NO `scan_completed_at`: the prior
    /// stale-but-real index is kept and heals on the next launch. Mirrors the
    /// network path's `VolumeScanError::EmptyRoot`; see
    /// `indexing/DETAILS.md` § "No completion marker on an empty root".
    EmptyRoot,
    /// The volume ROOT itself couldn't be listed (its read errored or timed out),
    /// which for a mount-rooted external drive means the mount VANISHED mid-scan (a
    /// yanked USB stick / SD card). Distinct from [`EmptyRoot`](ScanError::EmptyRoot):
    /// an empty-but-readable root lists successfully (zero children), whereas a
    /// vanished root can't be read at all. The completion handler treats this as an
    /// aborted scan — it writes NO `scan_completed_at` and emits `index-scan-aborted`
    /// so the frontend clears the stuck "scanning" row — mirroring the network path's
    /// disconnect arm. Surfaced by the fresh guarded-walker scan (`run_scan`, when the root is
    /// the only dir and never read) and the LOCAL reconcile walk (`local_reconcile`,
    /// when its root read returns `None`).
    RootUnlistable,
    /// The walk stopped because its cancellation token fired, carrying the
    /// partial totals it had reached. A distinct variant rather than a flag on
    /// the summary: `scan_completed_at` is written only on the `Ok` path, so a
    /// caller cannot mark a half-built index complete by forgetting to check.
    /// It is NOT a failure — the completion handlers route it to its own arm,
    /// which keeps the prior freshness and fires no abort. See
    /// `lifecycle/scan_completion.rs`.
    Cancelled(ScanSummary),
    /// A [`ScanRoot::Virgin`] walk was pointed at a directory the index already
    /// holds children for, so the add-only walk it wanted to run isn't safe: its
    /// fresh ids would collide with the existing rows, `INSERT OR IGNORE` would
    /// drop them silently, and everything below a dropped id would be orphaned.
    /// NOT a failure — the caller repairs the directory with the serial
    /// reconcile, which compares by name and writes only differences.
    NotVirgin,
    /// The reconcile walk panicked. `local_reconcile::start_local_reconcile`
    /// wraps the walk in `catch_unwind` and converts the panic payload into this
    /// typed variant (carrying the panic message), so the thread's `JoinHandle`
    /// resolves to `Ok(Err(ScanError::Panicked(_)))` instead of a raw thread
    /// panic. That routes it through the completion handler's `Ok(Err(_))` arm,
    /// which logs cleanly and fires `FreshnessEvent::ScanFailed` ⇒ Stale.
    Panicked(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Io(e) => write!(f, "I/O error: {e}"),
            ScanError::WriterSend(msg) => write!(f, "Writer send failed: {msg}"),
            ScanError::EmptyRoot => write!(f, "root listing returned no children (treating as a failed rescan)"),
            ScanError::RootUnlistable => write!(f, "volume root became unlistable (mount vanished mid-scan)"),
            ScanError::Cancelled(s) => write!(
                f,
                "scan cancelled after {} entries in {} dirs",
                s.total_entries, s.total_dirs
            ),
            ScanError::NotVirgin => write!(f, "the scan root already has children, so an add-only walk isn't safe"),
            ScanError::Panicked(msg) => write!(f, "reconcile walk panicked: {msg}"),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<std::io::Error> for ScanError {
    fn from(err: std::io::Error) -> Self {
        ScanError::Io(err)
    }
}

/// How long a COVER walk pauses before each directory read, from
/// `CMDR_E2E_WALK_THROTTLE_MS`.
///
/// A soft test hook, in the style of the app's `CMDR_E2E_COPY_THROTTLE_MS`: a
/// search that walks a fixture tree finishes in milliseconds, which leaves an E2E
/// no window to observe a pane growing mid-walk in. Unset in every real build, so
/// the read costs one `LazyLock` deref per walk and nothing else. ❌ Only cover
/// walks honor it (they're the only ones a test drives from the UI); a background
/// scan is never throttled.
fn cover_walk_throttle() -> Option<Duration> {
    static THROTTLE: std::sync::LazyLock<Option<Duration>> = std::sync::LazyLock::new(|| {
        std::env::var("CMDR_E2E_WALK_THROTTLE_MS")
            .ok()
            .and_then(|raw| raw.trim().parse::<u64>().ok())
            .filter(|ms| *ms > 0)
            .map(Duration::from_millis)
    });
    *THROTTLE
}

// ── Public API ───────────────────────────────────────────────────────

/// Start a full-volume scan on a background thread.
///
/// Spawns a `std::thread` that walks the directory tree via the guarded [`walker`],
/// sends batches of [`EntryRow`](crate::indexing::store::EntryRow) to the writer, and triggers `ComputeAllAggregates`
/// on completion.
///
/// Returns a [`ScanHandle`] for progress/cancellation and a [`std::thread::JoinHandle`]
/// for the scan result.
pub fn scan_volume(
    config: ScanConfig,
    writer: &IndexWriter,
    cancel: CancellationToken,
) -> Result<(ScanHandle, std::thread::JoinHandle<Result<ScanSummary, ScanError>>), ScanError> {
    let progress = Arc::new(ScanProgress::new());

    let handle = ScanHandle {
        progress: Arc::clone(&progress),
        cancel: cancel.clone(),
    };

    let writer = writer.clone();
    let thread_handle = std::thread::Builder::new()
        .name("index-scanner".into())
        .spawn(move || {
            // Yield CPU to the UI: the whole scan runs on this thread and its walker pool.
            cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
            let reader: ReadDirFn = default_reader();
            let policy = WalkPolicy::for_walk(ScanRoot::Volume, &config.space, &config.root);
            let result = run_scan(
                &config.root,
                &cancel,
                &progress,
                &writer,
                config.batch_size,
                config.num_threads,
                policy,
                &config.space,
                reader,
                LOCAL_LIST_TIMEOUT,
                None,
                None,
            );

            // Every listed dir was already stamped, in step with the rows that
            // made it stampable, so the ordering invariant (mark before the
            // final aggregate) holds by construction — including on the cancel
            // path, where the coverage a stopped walk earned stays durable
            // instead of being thrown away. What's left here is the CLEAN
            // finish's aggregate, plus a WAL checkpoint that trims the GB-scale
            // post-scan spike now instead of waiting for the ticker.
            if let Ok(outcome) = &result
                && !outcome.cancelled
            {
                if let Err(e) = writer.send(WriteMessage::ComputeAllAggregates {
                    source: AggSource::Maps,
                }) {
                    log::warn!("Scanner: failed to send ComputeAllAggregates: {e}");
                } else if let Err(e) = writer.send(WriteMessage::WalCheckpoint) {
                    log::warn!("Scanner: failed to send post-scan WalCheckpoint: {e}");
                }
            }

            result.and_then(ScanOutcome::into_result)
        })
        .map_err(ScanError::Io)?;

    Ok((handle, thread_handle))
}

/// Synchronous subtree scan. Runs in the caller's thread.
///
/// Used by post-replay background verification and by the per-navigation verifier.
/// After scanning, sends `ComputeSubtreeAggregates` to the writer.
///
/// `space` is the OWNING VOLUME's path space, and both halves of it matter here:
/// `root` is an absolute FS path, so a mount-rooted volume's subtree resolves to an
/// entry id only after the mount root is stripped, and a `BootDisk` scope would
/// exclude every `/Volumes/X/...` child and scan the subtree to nothing.
pub fn scan_subtree(
    root: &Path,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    cancel: &CancellationToken,
) -> Result<ScanSummary, ScanError> {
    walk_subtree(
        root,
        space,
        writer,
        cancel,
        WalkPolicy::for_walk(ScanRoot::Rebuild, space, root),
        None,
        default_reader(),
        0,
        None,
    )
}

/// Walk a coverage frontier node: ground the index has no claim on.
///
/// The search-driven half of the coverage concept. It differs from
/// [`scan_subtree`] in exactly one way that matters, and it's a data-safety one:
/// ❌ it DELETES NOTHING. A frontier node was chosen because nothing had listed
/// it, but "nothing listed it" does not mean "nothing is known below it" — an
/// FSEvents verification pass upserts children under a directory without marking
/// that directory listed, so a frontier node can sit above rows the index
/// genuinely knows (`convergence_tests::a_frontier_node_can_hold_a_listed_descendant`).
///
/// That safety rests on the root being empty in the index, which is checked here
/// rather than assumed: the walk allocates fresh ids for every name it finds, and
/// `INSERT OR IGNORE` would silently drop a fresh row colliding with a
/// pre-existing sibling, orphaning everything below the id it dropped. A root
/// that ISN'T empty is [`ScanError::NotVirgin`], and the caller repairs it with
/// the serial reconcile instead, which compares by name and only writes
/// differences.
///
/// `emit` is where a live consumer receives the entries as they're found; pass
/// `None` to fill the index and nothing else. `heartbeat` is how that same
/// consumer sees the walk moving between batches, and how many times it gave up
/// on ground it started.
pub(in crate::indexing) fn cover_subtree(
    root: &Path,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    emit: Option<EntrySender>,
    cancel: &CancellationToken,
    heartbeat: &WalkHeartbeat,
) -> Result<ScanSummary, ScanError> {
    walk_subtree(
        root,
        space,
        writer,
        cancel,
        WalkPolicy::for_walk(ScanRoot::Virgin, space, root),
        emit,
        default_reader(),
        0,
        Some(heartbeat),
    )
}

/// The shared body of [`scan_subtree`] and [`cover_subtree`]: walk, then stamp
/// what the walk learned and repair the ancestors it changed.
///
/// ❌ The post-walk sequence runs on the CANCEL path too, which is why the
/// outcome only becomes a `Result` afterwards. A `Rebuild` has already sent the
/// destructive `DeleteDescendantsById`, so bailing early would strand a
/// half-rebuilt subtree with stale ancestors; a `Virgin` walk has partial
/// coverage worth keeping, and an aggregate is what turns it into the honest
/// "≥" sizes a listing shows.
#[allow(
    clippy::too_many_arguments,
    reason = "the two entry points above are the API; this is their shared body"
)]
fn walk_subtree(
    root: &Path,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    cancel: &CancellationToken,
    policy: WalkPolicy,
    emit: Option<EntrySender>,
    reader: ReadDirFn,
    num_threads: usize,
    heartbeat: Option<&WalkHeartbeat>,
) -> Result<ScanSummary, ScanError> {
    let progress = Arc::new(ScanProgress::new());
    let outcome = run_scan(
        root,
        cancel,
        &progress,
        writer,
        2000,
        num_threads,
        policy,
        space,
        reader,
        LOCAL_LIST_TIMEOUT,
        emit,
        heartbeat,
    )?;

    if let Err(e) = writer.send(WriteMessage::ComputeSubtreeAggregates {
        root_id: outcome.root_id,
    }) {
        log::warn!("Scanner: failed to send ComputeSubtreeAggregates: {e}");
    }

    outcome.into_result()
}

// ── Core scan logic ──────────────────────────────────────────────────

/// Walk the local tree from `root` and insert every discovered entry, guarded so
/// a hung directory read can't stall the scan (see [`walker`]).
///
/// Parent attribution needs no path→id map: [`walk`] carries each directory's id
/// to its own read, so children take their parent's id directly. Ids come from the
/// shared `IndexWriter` counter. The scan root maps to `ROOT_ID` (volume scans) or
/// its existing entry id (subtree scans). `stall_timeout` is how long one read may
/// go without delivering an entry (production passes `LOCAL_LIST_TIMEOUT`; tests
/// pass a short one).
#[allow(
    clippy::too_many_arguments,
    reason = "internal scan entry point threading writer/progress/config; a param struct would add indirection without clarity"
)]
fn run_scan(
    root: &Path,
    cancel: &CancellationToken,
    progress: &ScanProgress,
    writer: &IndexWriter,
    batch_size: usize,
    num_threads: usize,
    policy: WalkPolicy,
    space: &IndexPathSpace,
    reader: ReadDirFn,
    stall_timeout: Duration,
    emit: Option<EntrySender>,
    heartbeat: Option<&WalkHeartbeat>,
) -> Result<ScanOutcome, ScanError> {
    let start = Instant::now();
    let mode = policy.root;
    let is_volume_root = mode.is_volume();

    // Resolve the scan root id and read the epoch every listed dir is stamped with
    // (a first scan seeds epoch 1). Volume-root scans need a write connection (to
    // create the root sentinel / seed the epoch); subtree scans read on a read
    // connection after the full scan already seeded both.
    let (root_id, epoch) = {
        let db_path = writer.db_path();
        let conn = if is_volume_root {
            IndexStore::open_write_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?
        } else {
            IndexStore::open_read_connection(&db_path).map_err(|e| ScanError::WriterSend(e.to_string()))?
        };
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| ScanError::WriterSend(e.to_string()))?;
        let epoch = if is_volume_root {
            IndexStore::seed_current_epoch(&conn).map_err(|e| ScanError::WriterSend(e.to_string()))?
        } else {
            IndexStore::read_current_epoch(&conn).map_err(|e| ScanError::WriterSend(e.to_string()))?
        };
        // A volume-root scan maps to `ROOT_ID` whatever its path is, so it hands
        // `root` over untouched. A SUBTREE scan resolves an EXISTING entry, so its
        // absolute root must first cross into the volume's index path space
        // (identity on the boot disk, mount-root-stripped elsewhere): an absolute
        // walk from `ROOT_ID` misses at the very first component on a mount-rooted
        // volume, and a subtree outside the mount root has no entry here at all.
        let root_id = if is_volume_root {
            resolve_scan_root(&conn, root, true).map_err(|e| ScanError::WriterSend(e.to_string()))?
        } else {
            let index_root = space
                .index_relative(&root.to_string_lossy())
                .ok_or_else(|| ScanError::WriterSend(format!("{} is outside the volume's index", root.display())))?;
            resolve_scan_root(&conn, Path::new(&index_root), false).map_err(|e| ScanError::WriterSend(e.to_string()))?
        };
        // An add-only walk is safe only over ground the index holds nothing for.
        // Asked in the same breath as the resolve, on the same connection, because
        // the answer decides whether the walk may run at all.
        if mode == ScanRoot::Virgin
            && IndexStore::count_children_capped(root_id, &conn, 1).map_err(|e| ScanError::WriterSend(e.to_string()))?
                > 0
        {
            return Err(ScanError::NotVirgin);
        }
        (root_id, epoch)
    };

    // A REBUILD deletes existing descendants first (it re-inserts fresh children);
    // the subtree root entry itself is preserved. ❌ A `Virgin` walk deletes
    // nothing — see [`ScanRoot::Virgin`], it may only add.
    if mode == ScanRoot::Rebuild {
        writer
            .send(WriteMessage::DeleteDescendantsById(root_id))
            .map_err(|e| ScanError::WriterSend(e.to_string()))?;
    }

    // A CHILD of the scan's token: cancelling the parent stops the walk, and the
    // visitor can stop the walk on a writer-send failure WITHOUT that reading as
    // a user cancel (`was_cancelled` below asks the parent).
    let walk_cancel = cancel.child_token();
    let emitting = emit.is_some();
    let visitor = Arc::new(InsertVisitor::new(
        writer.clone(),
        policy,
        space.inodes_trustworthy(),
        batch_size,
        progress,
        walk_cancel.clone(),
        epoch,
        emit,
    ));

    // Watchdog ticks faster than the timeout (production 15s → 1s; a short test
    // timeout scales down, floored at 5ms).
    let watchdog_interval = (stall_timeout / 15).clamp(Duration::from_millis(5), WATCHDOG_INTERVAL);
    // A walk somebody is watching gives the watchdog a second duty: handing over
    // a partial batch of found entries that has waited long enough. So its tick
    // can't be slower than that wait, or a walk parked on one directory would sit
    // on rows it has already found (`live_emit.rs`).
    let watchdog_interval = if emitting {
        watchdog_interval.min(EMIT_INTERVAL)
    } else {
        watchdog_interval
    };
    let cfg = WalkConfig {
        num_threads,
        stall_timeout,
        per_entry_allowance: DEFAULT_PER_ENTRY_ALLOWANCE,
        watchdog_interval,
        give_up_after: DEFAULT_GIVE_UP_AFTER,
        heartbeat: heartbeat.cloned(),
        per_dir_delay: heartbeat.and(cover_walk_throttle()),
    };
    let root_task = DirTask {
        path: root.to_path_buf(),
        id: root_id,
    };

    let walk_stats = walk(root_task, cfg, reader, Arc::clone(&visitor), walk_cancel);

    // Flush the final batch and surface any writer-send failure.
    visitor.finish()?;

    if walk_stats.timed_out > 0 {
        log::warn!(
            "Scanner: {} skipped after producing nothing for {}s each (hung / disconnected dirs)",
            pluralize(walk_stats.timed_out, "dir"),
            stall_timeout.as_secs(),
        );
    }
    if walk_stats.subtrees_abandoned > 0 {
        log::warn!(
            "Scanner: gave up on {} after {DEFAULT_GIVE_UP_AFTER} consecutive failed reads each \
             (dead mounts / providers); their subtrees are left honestly unindexed",
            pluralize(walk_stats.subtrees_abandoned, "subtree"),
        );
    }
    // Both numbers mean the same thing to a search: this walk read less than the
    // tree holds, so its result list is a lower bound even if the walk ran to the
    // end (Accepted difference 9). They're separate log lines because the causes
    // differ; they're one signal because the sentence the user reads doesn't.
    if let Some(heartbeat) = heartbeat {
        heartbeat.abandoned(walk_stats.timed_out + walk_stats.subtrees_abandoned);
    }

    let was_cancelled = cancel.is_cancelled();

    // A volume-root scan whose ROOT never listed (`dirs_read == 0`) means the mount
    // itself couldn't be read — it vanished or went unreadable mid-scan (a yanked
    // external drive). This is distinct from an empty-but-readable root, which reads
    // successfully (so `dirs_read == 1`). Surface it as the typed `RootUnlistable`
    // abort so the completion handler writes no `scan_completed_at` (no
    // false-complete of an empty index) and clears the stuck "scanning" row, instead
    // of silently "completing" with zero entries. Only for a volume-root scan (`/`
    // and mount roots); subtree scans have their own root handling.
    if is_volume_root && !was_cancelled && walk_stats.dirs_read == 0 {
        return Err(ScanError::RootUnlistable);
    }

    // Directories the walk found it can't read stop re-entering the frontier.
    // After `visitor.finish()` above, so every `MarkDirsListed` this walk earned is
    // already ahead of these on the writer channel: a directory that failed once
    // and then succeeded on a retry within the same walk ends up listed rather than
    // pinned unreadable — and `mark_dirs_listed` clears the cause anyway, whichever
    // order they land in.
    send_unreadable_marks(&visitor.take_unreadable_ids(), writer);

    let snap = progress.snapshot();

    log::debug!(
        "Scanner: walk complete: {}, {} in {}ms",
        pluralize_with(snap.entries_scanned, "entry", "entries"),
        pluralize(snap.dirs_found, "dir"),
        start.elapsed().as_millis()
    );

    Ok(ScanOutcome {
        summary: ScanSummary {
            total_entries: snap.entries_scanned,
            total_dirs: snap.dirs_found,
            total_physical_bytes: snap.bytes_scanned,
            duration_ms: start.elapsed().as_millis() as u64,
        },
        cancelled: was_cancelled,
        root_id,
    })
}

/// [`cover_subtree`] with the reader and the worker count injected, for tests
/// that need to decide exactly which directory reads succeed and when the walk
/// gets cancelled.
#[cfg(test)]
pub(in crate::indexing) fn cover_subtree_with_reader(
    root: &Path,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    emit: Option<EntrySender>,
    cancel: &CancellationToken,
    reader: ReadDirFn,
    heartbeat: Option<&WalkHeartbeat>,
) -> Result<ScanSummary, ScanError> {
    let policy = WalkPolicy::for_walk(ScanRoot::Virgin, space, root);
    walk_subtree(root, space, writer, cancel, policy, emit, reader, 1, heartbeat)
}

/// [`cover_subtree_with_reader`] with the device probe injected too, so a test
/// can put a "mount" anywhere in a temp tree without one.
#[cfg(test)]
pub(in crate::indexing) fn cover_subtree_with_device_probe(
    root: &Path,
    space: &IndexPathSpace,
    writer: &IndexWriter,
    cancel: &CancellationToken,
    device_of: fn(&Path) -> Option<u64>,
) -> Result<ScanSummary, ScanError> {
    let policy = WalkPolicy::with_device_probe(ScanRoot::Virgin, space, root, device_of);
    walk_subtree(root, space, writer, cancel, policy, None, default_reader(), 1, None)
}

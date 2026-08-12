//! A `Volume`-trait recursive scanner for indexing network/USB volumes.
//!
//! The local guarded walker (the [`scanner`](super::scanner) module) is local-FS-only (`getattrlistbulk`)
//! and its `should_exclude` deliberately blocks `/Volumes/`. SMB (and, later,
//! MTP) shares are walked here instead, over the SAME `Volume::list_directory`
//! API the live pane uses, pulling sizes from the backend's stat. EVERYTHING
//! downstream of [`EntryRow`](super::store::EntryRow) is reused unchanged: the
//! shared `Arc<AtomicI64>` id counter via `ScanContext`, the single writer
//! thread (`InsertEntriesV2` batches), the aggregator (`ComputeAllAggregates`),
//! and `dir_stats`. Only the *front* of the pipeline (how entries are
//! discovered and stat'd) differs.
//!
//! ## Discipline for network round trips (plan rabbit hole #3)
//!
//! Every `list_directory` is a network syscall that can block 30–120 s on a
//! slow or hung mount, so the walk:
//!
//! - **is cancelable at every round trip**: the cancel flag is checked before
//!   each directory listing and the BFS bails immediately when set;
//! - **wraps each listing in a timeout** (`LIST_TIMEOUT`): a wedged mount yields
//!   a typed `VolumeScanError::Timeout` rather than parking forever;
//! - **wraps each round trip in `objc2::rc::autoreleasepool` on macOS**: the SMB
//!   listing path touches NSURL/`NSString`-adjacent code, and unpooled ObjC
//!   autoreleases leak multi-GB over a long walk (the same rule the index writer
//!   thread follows — see `indexing/CLAUDE.md`).
//!
//! ## Terminal disconnect ⇒ keep an honest partial; cancel ⇒ discard
//!
//! A mid-walk **disconnect** (the typed `DeviceDisconnected`/`Disconnected`, or
//! the consecutive-failure backstop for a disconnect-shaped untyped error) is
//! TERMINAL: the walk stops immediately rather than churning the still-queued
//! dirs into silently-empty rows (the reported prod bug). Before returning the
//! typed error, it runs the partial-preserving write sequence
//! (`finish_partial_scan`: flush + `MarkDirsListed` + `ComputeAllAggregates`)
//! so the kept partial is self-describing — scanned subtrees roll up to
//! `min_subtree_epoch > 0` (exact, stale once the epoch is bumped), unscanned
//! ones stay `0` (`—`/`≥`). The completion handler (`lifecycle/manager.rs`) then keeps the
//! instance + DB and marks the volume Stale.
//!
//! A **user cancel** still discards: cancelling the token returns `Cancelled` with no
//! marks/aggregate, and the completion handler resets the volume to gray.
//!
//! This scanner NEVER writes the `scan_completed_at` meta marker (on any path);
//! the caller's completion handler does, only on a clean finish — the same
//! `scan_completed_at`-absent ⇒ no-Fresh / heal-to-rescan mechanism the local
//! scanner relies on (see `lifecycle/manager.rs`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

pub(crate) mod scan_pace;
mod system_dirs;

/// The scoped, add-only walk a search drives.
mod cover_scan;
mod full_scan;
/// The non-destructive diff-and-patch.
mod reconcile_scan;

pub(crate) use cover_scan::cover_volume_subtree;
pub use full_scan::scan_volume_via_trait;
pub(crate) use reconcile_scan::reconcile_volume_via_trait;

pub(crate) use system_dirs::{exclusion_stamp_message, index_predates_exclusion_list, is_recursion_excluded_dir};

use cmdr_fs::volume::Volume;

use super::scanner::ScanSummary;
use super::writer::IndexWriter;

/// Batch size for `InsertEntriesV2` sends — matches the local guarded walker's default.
const BATCH_SIZE: usize = 2000;

/// How often a walk here commits its open insert transaction, so the single
/// writer fsyncs once per interval instead of once per 2000-entry batch (the
/// lever that keeps the writer up with the connection pool's ~4x listing
/// throughput). Short on purpose: mid-scan "growing sizes" stay visible within
/// one interval, and a crash loses at most one interval (heals to a rescan).
/// Rationale + crash-safety: `DETAILS.md` § "Two levers past 64 in-flight:
/// connections, and the writer".
const SCAN_COMMIT_INTERVAL: Duration = Duration::from_secs(2);

/// Hand the accumulated rows to the writer, leaving the batch empty. A no-op on
/// an empty batch, so every exit path can call it unconditionally.
fn flush_batch(batch: &mut Vec<crate::indexing::store::EntryRow>, writer: &IndexWriter) -> Result<(), VolumeScanError> {
    if batch.is_empty() {
        return Ok(());
    }
    let entries = std::mem::take(batch);
    writer
        .send(crate::indexing::writer::WriteMessage::InsertEntriesV2(entries))
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))
}

/// Open a walk's explicit insert transaction (see [`SCAN_COMMIT_INTERVAL`]).
fn begin_scan_tx(writer: &IndexWriter, tx_open: &mut bool) -> Result<(), VolumeScanError> {
    writer
        .send(crate::indexing::writer::WriteMessage::BeginTransaction)
        .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
    *tx_open = true;
    Ok(())
}

/// Commit a walk's open insert transaction if one is open (idempotent). Called at
/// each commit interval AND before EVERY exit (clean finish, cancel, root-fatal,
/// empty-root, disconnect, consecutive-failure) so the writer connection never
/// returns mid-transaction, and so the marks + aggregate that follow run in
/// autocommit.
fn commit_scan_tx(writer: &IndexWriter, tx_open: &mut bool) -> Result<(), VolumeScanError> {
    if *tx_open {
        writer
            .send(crate::indexing::writer::WriteMessage::CommitTransaction)
            .map_err(|e| VolumeScanError::WriterSend(e.to_string()))?;
        *tx_open = false;
    }
    Ok(())
}

/// Per-directory listing timeout. Network/USB `list_directory` blocks 30–120 s
/// on a hung mount; we cap a single round trip so a wedged share fails the walk
/// instead of parking it. Generous enough for a slow-but-alive NAS directory.
const LIST_TIMEOUT: Duration = Duration::from_secs(120);

/// Consecutive-failure backstop. A whole-volume disconnect that doesn't map to
/// the typed `DeviceDisconnected`/`Disconnected` variant (e.g. a generic
/// `IoError` "connection reset") would otherwise make every remaining queued
/// listing fail instantly — the exact prod bug, where ~6,475 dirs churned into
/// empty rows in ~1 s. So after this many CONSECUTIVE listing failures we abort
/// the walk (terminal), keeping the honest partial, rather than fabricating
/// empties. The counter resets on every success, so an isolated bad dir is still
/// skip-and-continue. 32 is generous enough that a sparse cluster of genuinely
/// unlistable dirs (a permission-walled tree) doesn't trip it, but small enough
/// that a real disconnect aborts in milliseconds.
const CONSECUTIVE_FAILURE_ABORT: usize = 32;

/// Why a `Volume`-trait scan ended other than cleanly.
#[derive(Debug)]
pub enum VolumeScanError {
    /// A directory listing exceeded `LIST_TIMEOUT` (wedged/hung mount).
    Timeout(PathBuf),
    /// The backend returned an error (disconnect mid-walk, permission, etc.).
    /// A `DeviceDisconnected`/`Disconnected` value here is a TERMINAL disconnect
    /// (see [`VolumeScanError::is_terminal_disconnect`]); other variants are the
    /// root-fatal case (failing to list the root itself).
    Volume(cmdr_fs::volume::VolumeError),
    /// The consecutive-failure backstop tripped: `count` listings in a row
    /// failed with a non-typed (disconnect-shaped) error, so the walk aborted
    /// rather than churning every queued dir into a silently-empty row. `last`
    /// is the most recent failing error's display. Treated as a terminal
    /// disconnect by the completion handler — see `is_terminal_disconnect`.
    ConsecutiveFailures { count: usize, last: String },
    /// A writer send failed (the writer thread is gone).
    WriterSend(String),
    /// Setting up the scan context (root sentinel, id counter) failed.
    Context(String),
    /// The walk stopped because its cancellation token fired, carrying the
    /// partial totals it had reached. A distinct variant rather than a flag on
    /// the summary, so `Ok` means "this walk finished" and no caller can mark a
    /// partial index complete by forgetting to check. Deliberately NOT a
    /// terminal disconnect (see [`is_terminal_disconnect`](Self::is_terminal_disconnect)):
    /// the partial is discardable, so the completion handler resets the volume
    /// rather than keeping it Stale.
    Cancelled(ScanSummary),
    /// The ROOT listing SUCCEEDED but returned zero children, so the walk
    /// produced an empty index. Distinct from a root listing that FAILED
    /// (`Volume`) — here the backend answered, it just answered "nothing". For a
    /// NAS share that's almost always a transient glitch or a wrong scan root,
    /// not a genuinely empty share, so we treat it as a failed scan: surfacing
    /// this makes the completion handler NOT persist `scan_completed_at`, which
    /// would otherwise strand the index as falsely "complete" and refuse all
    /// future rescans (the real-hardware bug). A genuinely empty share is
    /// vanishingly rare and self-heals on the next rescan, so the safe rule
    /// wins. See `indexing/DETAILS.md` § "No completion marker on an empty root".
    EmptyRoot,
}

impl VolumeScanError {
    /// Whether this error means the volume went away mid-walk (a continuity
    /// break), so the completion handler should KEEP the honest partial and mark
    /// the volume Stale rather than discard it. True for a typed
    /// `DeviceDisconnected`/`Disconnected` and for the consecutive-failure
    /// backstop; false for a timeout / context / writer-send failure (those are
    /// genuine aborts with no honest partial to keep).
    ///
    /// Classifies by the TYPED variant, never a message substring.
    pub(crate) fn is_terminal_disconnect(&self) -> bool {
        use cmdr_fs::volume::VolumeError;
        matches!(
            self,
            Self::Volume(VolumeError::DeviceDisconnected(_)) | Self::ConsecutiveFailures { .. }
        )
    }
}

impl std::fmt::Display for VolumeScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout(p) => write!(f, "listing timed out: {}", p.display()),
            Self::Volume(e) => write!(f, "volume error: {e}"),
            Self::ConsecutiveFailures { count, last } => {
                write!(f, "{count} consecutive listing failures (last: {last})")
            }
            Self::WriterSend(m) => write!(f, "writer send failed: {m}"),
            Self::Context(m) => write!(f, "scan context setup failed: {m}"),
            Self::Cancelled(s) => write!(
                f,
                "scan cancelled after {} entries in {} dirs",
                s.total_entries, s.total_dirs
            ),
            Self::EmptyRoot => write!(f, "root listing returned no children (treating as a failed scan)"),
        }
    }
}

impl std::error::Error for VolumeScanError {}

/// List one directory over the `Volume` trait, giving up on it after
/// [`LIST_TIMEOUT`] and (macOS) draining the autoreleasepool. The pool is drained
/// per round trip so autoreleased ObjC objects from the SMB listing path don't
/// accumulate across a long walk.
///
/// Uses `list_directory_for_scan` so a foreground-priority backend (MTP) walks
/// the folder in yielding units; `cancel` threads in so an in-flight listing
/// bails within one round trip (the MTP path checks it at each unit and per
/// `GetObjectInfo`), not just between directories.
///
/// ❌ The listing runs in its OWN task and the timeout races that task's join
/// handle, never the listing future itself. Timing out drops the handle, which
/// DETACHES the task; it does not cancel it. That distinction is load-bearing:
/// wrapping the listing future directly would drop it mid-round-trip, and on MTP
/// that abandons an in-flight PTP transaction and wedges the phone
/// (`mtp/connection/CLAUDE.md`). The walk gives up on the directory either way;
/// the difference is whether the device survives it. A background MTP scan hits
/// this routinely: it parks at `background_yield_point` while the user is
/// active, so a big folder easily outlives `LIST_TIMEOUT`.
async fn list_one_directory(
    volume: Arc<dyn Volume>,
    dir_path: PathBuf,
    cancel: CancellationToken,
) -> Result<Vec<cmdr_fs::entry::FileEntry>, VolumeScanError> {
    let listing_path = dir_path.clone();
    let listing = tokio::spawn(async move {
        let result = volume.list_directory_for_scan(&listing_path, Some(&cancel)).await;
        // Drain the autoreleased ObjC objects this listing created before the
        // future resolves. Cheap no-op on non-macOS.
        drain_autorelease_pool();
        result
    });

    match tokio::time::timeout(LIST_TIMEOUT, listing).await {
        Ok(Ok(Ok(entries))) => Ok(entries),
        Ok(Ok(Err(e))) => Err(VolumeScanError::Volume(e)),
        Ok(Err(join_err)) => Err(VolumeScanError::Volume(cmdr_fs::volume::VolumeError::IoError {
            message: format!("Directory listing task failed: {join_err}"),
            raw_os_error: None,
        })),
        Err(_elapsed) => Err(VolumeScanError::Timeout(dir_path)),
    }
}

/// Stat ONE path over the `Volume` trait under the same disciplines as a listing,
/// for the chain a scoped walk has to materialize before it can start
/// (`lifecycle/cover/bootstrap.rs`).
///
/// `None` means "not a directory this walk may descend into": the path is gone,
/// the backend wouldn't answer in time, or it's a symlink — the index stores
/// symlinks without descending into them, so a walk rooted below one would
/// attribute another directory's contents to this path.
///
/// ❌ Same detach rule as [`list_one_directory`]: the timeout races the task's JOIN
/// HANDLE, never the future itself. Dropping a `get_metadata` future mid-round-trip
/// abandons an in-flight PTP transaction and wedges the phone.
pub(crate) async fn stat_one_directory(volume: Arc<dyn Volume>, path: PathBuf) -> Option<cmdr_fs::entry::FileEntry> {
    let stat_path = path.clone();
    let stat = tokio::spawn(async move {
        let result = volume.get_metadata(&stat_path).await;
        drain_autorelease_pool();
        result
    });
    match tokio::time::timeout(LIST_TIMEOUT, stat).await {
        Ok(Ok(Ok(entry))) if entry.is_directory && !entry.is_symlink => Some(entry),
        Ok(Ok(Ok(_))) => None,
        Ok(Ok(Err(e))) => {
            log::debug!("network_scanner: can't stat {}: {e}", path.display());
            None
        }
        Ok(Err(join_err)) => {
            log::debug!("network_scanner: stat task for {} failed: {join_err}", path.display());
            None
        }
        Err(_elapsed) => {
            log::warn!("network_scanner: stat of {} timed out", path.display());
            None
        }
    }
}

/// Drain the current thread's ObjC autorelease pool. On macOS this wraps a
/// no-op closure in `objc2::rc::autoreleasepool`, which drains on scope exit; on
/// other platforms it's a no-op. We can't hold an `autoreleasepool` guard across
/// an `.await` (it isn't `Send`), so we drain after the await resolves instead.
#[inline]
fn drain_autorelease_pool() {
    #[cfg(target_os = "macos")]
    objc2::rc::autoreleasepool(|_| {});
}

/// Whether a `VolumeError` means the whole volume went away mid-walk (terminal
/// disconnect), classified by the TYPED variant — never a message substring.
///
/// `DeviceDisconnected` is the one `VolumeError` variant that means "the volume
/// is gone": a dropped MTP device AND a broken SMB smb2 session both surface as
/// `DeviceDisconnected` from `list_directory` (the SMB-connection-state
/// `Disconnected` is a separate enum used by the FE-facing `smb_connection_state`
/// probe, not returned from a listing call). A `ConnectionTimeout` is handled by
/// the `Timeout`/consecutive-failure path, not here.
fn is_typed_disconnect(e: &cmdr_fs::volume::VolumeError) -> bool {
    use cmdr_fs::volume::VolumeError;
    matches!(e, VolumeError::DeviceDisconnected(_))
}

/// Minimum gap between scan-progress heartbeat log lines.
const PROGRESS_LOG_INTERVAL: Duration = Duration::from_secs(1);

/// Throttled scan-progress heartbeat (~1/s) at DEBUG. The per-listing
/// `SmbVolume::list_directory` line is at TRACE (off by default), so on a long network
/// scan this is what tells a triager reading an error report that the walk is ALIVE,
/// WHERE it is, and how far along — without the per-directory flood. `phase` is
/// `"scanning"` (fresh) or `"reconciling"`.
fn log_scan_progress(last_log: &mut Instant, phase: &str, dir_path: &Path, total_dirs: u64, total_entries: u64) {
    if last_log.elapsed() < PROGRESS_LOG_INTERVAL {
        return;
    }
    *last_log = Instant::now();
    log::debug!(
        "network_scanner: {phase}… {}, {}, current: {}",
        cmdr_fs::pluralize::pluralize(total_dirs, "dir"),
        cmdr_fs::pluralize::pluralize_with(total_entries, "entry", "entries"),
        dir_path.display()
    );
}

fn summary(entries: u64, dirs: u64, physical_bytes: u64, start: Instant) -> ScanSummary {
    ScanSummary {
        total_entries: entries,
        total_dirs: dirs,
        total_physical_bytes: physical_bytes,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod pace_tests;
#[cfg(test)]
mod tests;

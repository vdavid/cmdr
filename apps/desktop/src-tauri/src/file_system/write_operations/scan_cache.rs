//! Scan-preview state: one map from `preview_id` to everything the app knows
//! about that preview, the per-file `FileInfo` / `ScanResult` carriers, and the
//! TTL safety-net over settled entries.
//!
//! These types are owned here but re-exported from `lifecycle/state.rs`, so existing
//! `state::FileInfo` / `state::ScanResult` / `state::CachedScanResult` paths
//! keep resolving.
//!
//! `PREVIEWS` itself is private to this module: every insert, read, and take
//! goes through a function here, so the coherence canary and the request
//! binding can't be bypassed by a new call site writing the map.
//!
//! ## One map, because "the preview is gone" is ambiguous
//!
//! A preview is either in flight or settled, and a settled one carries WHY it
//! settled: complete (with its `CachedScanResult`), errored (with its message),
//! or cancelled. Splitting in-flight state from results would leave a window
//! where a worker has dropped its in-flight entry but not yet published its
//! result, and an operation waiting on that preview cannot tell that window
//! apart from "evicted", "errored", or "never existed". One entry, replaced in
//! place, makes the publication atomic.
//!
//! ## Claims
//!
//! An operation that intends to consume a preview CLAIMS it, exactly one owner
//! per preview (`claim_preview`). The claim is what lets the progress bridge
//! forward the walk's counts under the operation's id, what exempts the result
//! from TTL eviction while its owner waits behind a busy lane, and what stops a
//! dialog teardown from freeing a result an operation is about to read.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

use crate::file_system::volume::CopyScanResult;

// ============================================================================
// Scan preview state
// ============================================================================

/// State for a scan preview operation.
pub(super) struct ScanPreviewState {
    pub cancelled: AtomicBool,
    pub progress_interval: Duration,
}

/// How a scan preview ended. Published atomically with the in-flight state's
/// removal, and readable afterwards for as long as the entry lives, so an
/// operation whose task spawns minutes later still learns what happened.
///
/// `Cancelled` comes from the worker's own cancel flag at its exit, never from
/// which event fired: a genuinely cancelled walk returns an error carrying the
/// word "cancelled", and classifying on that would both misreport a user's
/// cancel as a failure and put string-matching on the control path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScanOutcome {
    /// The walk finished. A `CachedScanResult` is in the map for its owner.
    Complete,
    /// The walk stopped on an I/O or protocol error, with its message.
    Error(String),
    /// The walk stopped because someone cancelled the preview.
    Cancelled,
}

/// Whether a preview is still walking or has published its outcome.
enum PreviewPhase {
    InFlight(Arc<ScanPreviewState>),
    Settled {
        outcome: ScanOutcome,
        /// The walk's result, present for `Complete` until someone takes it.
        result: Option<Box<CachedScanResult>>,
        /// Drives the TTL safety net (`prune_expired`). See `SCAN_RESULT_TTL`.
        settled_at: Instant,
    },
}

/// One preview: its phase, plus the operation that claimed it, if any.
struct PreviewEntry {
    phase: PreviewPhase,
    /// The single operation allowed to wait on and consume this preview. A
    /// second operation naming the same `preview_id` is refused and falls back
    /// to its own walk, because `take_cached_scan_result` REMOVES what it reads
    /// and two claimants would race for one consumable result.
    claim: Option<String>,
}

/// Woken every time a preview settles. Waiters re-read their own entry rather
/// than trusting the wakeup, so a spurious or coalesced notification is
/// harmless.
pub(super) static PREVIEW_SETTLED: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Cached result from a completed scan preview.
///
/// ❌ The fields are PRIVATE and stay that way: build one through
/// `from_local_walk` or `from_volume_batch`. The two production walks emit
/// genuinely different shapes, and a hand-written literal is how a test invents
/// a third one that production has never once produced — which is exactly how
/// the empty-`per_path` bug got certified by a suite full of fully-populated
/// fixtures.
pub(super) struct CachedScanResult {
    /// The top-level paths the preview was asked to walk. A `preview_id` proves
    /// the frontend once asked for a scan; it proves nothing about WHICH scan,
    /// so `take_cached_scan_result` refuses an entry whose sources don't match
    /// the operation's own. Without this, an operation on `/b` would happily
    /// act on a cached preview of `/a`, and on delete that has no rollback.
    sources: Vec<PathBuf>,
    files: Vec<FileInfo>,
    dirs: Vec<PathBuf>,
    file_count: usize,
    /// Write footprint (un-dedup'd). See `CopyScanResult::total_bytes`.
    total_bytes: u64,
    /// `du`-equivalent source footprint (hardlinks counted once). See
    /// `CopyScanResult::dedup_bytes`.
    dedup_bytes: u64,
    /// Per-source-path scan results: one `CopyScanResult` per top-level source,
    /// carrying its type and totals. The copy pipeline's cached branch rebuilds
    /// `source_hints` from it rather than probing `is_directory` per path
    /// (which on MTP each list the parent dir), so a source missing from here
    /// costs a round trip and a source wrongly absent costs a wrong answer.
    per_path: Vec<(PathBuf, CopyScanResult)>,
    /// Compressed-size estimate for a compress-mode local scan, carried so the
    /// recovery path (`get_scan_preview_totals`) can hand it back when the FE
    /// missed the complete event. `None` for copy/move and remote scans.
    estimated_compressed_bytes: Option<super::types::CompressedSizeEstimate>,
}

impl CachedScanResult {
    /// The LOCAL `std::fs` walk's shape (`run_scan_preview`): a per-file
    /// `FileInfo` list, the directories it found, one `CopyScanResult` per
    /// top-level source, and possibly a compressed-size estimate.
    ///
    /// `file_count` is derived from `files`: a caller passing it is one more
    /// thing to get wrong.
    pub(super) fn from_local_walk(
        sources: Vec<PathBuf>,
        files: Vec<FileInfo>,
        dirs: Vec<PathBuf>,
        total_bytes: u64,
        dedup_bytes: u64,
        per_path: Vec<(PathBuf, CopyScanResult)>,
        estimated_compressed_bytes: Option<super::types::CompressedSizeEstimate>,
    ) -> Self {
        // Stronger than `insert_scan_result`'s generic canary, because this
        // constructor knows which shape it's building: a local walk that found
        // files walked at least one source, so it recorded at least one.
        debug_assert!(
            files.is_empty() || !per_path.is_empty(),
            "a local walk that found {} files must record a per-source result",
            files.len()
        );
        Self {
            sources,
            file_count: files.len(),
            files,
            dirs,
            total_bytes,
            dedup_bytes,
            per_path,
            estimated_compressed_bytes,
        }
    }

    /// The VOLUME batch scan's shape (`run_volume_scan_preview`, SMB / MTP): no
    /// per-file list at all (consumers read `per_path`, and a delete that needs
    /// paths recurses itself), aggregate counters, and never an estimate
    /// (remote sources don't sample).
    pub(super) fn from_volume_batch(
        sources: Vec<PathBuf>,
        file_count: usize,
        total_bytes: u64,
        dedup_bytes: u64,
        per_path: Vec<(PathBuf, CopyScanResult)>,
    ) -> Self {
        Self {
            sources,
            files: Vec::new(),
            dirs: Vec::new(),
            file_count,
            total_bytes,
            dedup_bytes,
            per_path,
            estimated_compressed_bytes: None,
        }
    }
}

/// How long a settled preview entry lives before the TTL safety net evicts it.
/// The normal lifecycle frees results far sooner (`take_cached_scan_result` at
/// op start, or `release_preview` on dialog teardown); this only catches the
/// case where neither fires.
///
/// ⚠️ A CLAIMED entry is exempt (`prune_expired`). With `LANE_BUDGET = 1` an
/// operation can sit Queued well past five minutes, and evicting the very
/// result its owner is waiting for would silently downgrade it to a re-walk.
pub(super) const SCAN_RESULT_TTL: Duration = Duration::from_secs(300);

/// Returns the preview ids in `entries` whose `inserted_at` is older than
/// `ttl` relative to `now`. Pure so it's unit-testable without touching the
/// global cache. Callers remove the returned ids under the write lock.
pub(super) fn expired_scan_result_ids<'a>(
    entries: impl IntoIterator<Item = (&'a String, Instant)>,
    now: Instant,
    ttl: Duration,
) -> Vec<String> {
    entries
        .into_iter()
        .filter(|(_, inserted_at)| now.duration_since(*inserted_at) > ttl)
        .map(|(id, _)| id.clone())
        .collect()
}

/// Registers a preview as in flight. Called by `start_scan_preview` before it
/// spawns the walk, so a cancel or a claim arriving immediately after finds an
/// entry.
pub(super) fn register_preview(preview_id: String, state: Arc<ScanPreviewState>) {
    if let Ok(mut previews) = PREVIEWS.write() {
        previews.insert(
            preview_id,
            PreviewEntry {
                phase: PreviewPhase::InFlight(state),
                claim: None,
            },
        );
    }
}

/// The in-flight state for `preview_id` (its cancel flag and progress
/// interval), or `None` once the preview has settled.
pub(super) fn in_flight_state(preview_id: &str) -> Option<Arc<ScanPreviewState>> {
    let previews = PREVIEWS.read().ok()?;
    match &previews.get(preview_id)?.phase {
        PreviewPhase::InFlight(state) => Some(Arc::clone(state)),
        PreviewPhase::Settled { .. } => None,
    }
}

/// Publishes how a preview ended, replacing its in-flight state in one write,
/// then wakes every waiter. `result` is the walk's output for `Complete` and
/// `None` otherwise.
///
/// The single choke point for a settled entry, so the TTL sweep can't be
/// forgotten by a new call site and the coherence canary below sees every
/// completed walk. A preview nobody registered still settles (an entry is
/// created), which keeps a late worker from silently dropping its result.
pub(super) fn settle_preview(preview_id: &str, outcome: ScanOutcome, result: Option<CachedScanResult>) {
    // The incoherent shape: a completed walk that counted files but recorded no
    // per-source result. Downstream that reads as "no information", and a copy
    // driver turns "no information" into a confident `is_directory: false` — the
    // exact lie that streamed a directory as a file and let a failed copy
    // recursively delete a merged destination. One-directional on purpose: a
    // volume batch legitimately caches an empty `files` list with a populated
    // `per_path`, never the reverse.
    if let Some(result) = &result
        && result.file_count > 0
        && result.per_path.is_empty()
    {
        log::warn!(
            target: "cmdr_lib::write_operations",
            "scan preview {} completed a walk of {} files with no per-source results; downstream hints will be empty",
            preview_id,
            result.file_count
        );
        debug_assert!(
            false,
            "a completed walk with file_count > 0 must carry per_path entries (preview {preview_id})"
        );
    }
    if let Ok(mut previews) = PREVIEWS.write() {
        prune_expired(&mut previews);
        let claim = previews.remove(preview_id).and_then(|entry| entry.claim);
        previews.insert(
            preview_id.to_string(),
            PreviewEntry {
                phase: PreviewPhase::Settled {
                    outcome,
                    result: result.map(Box::new),
                    settled_at: Instant::now(),
                },
                claim,
            },
        );
    }
    PREVIEW_SETTLED.notify_waiters();
}

/// Drops every settled, UNCLAIMED entry older than `SCAN_RESULT_TTL`. In-flight
/// entries have no age to judge and claimed ones belong to an operation that
/// may still be waiting on a lane, so both stay.
fn prune_expired(previews: &mut HashMap<String, PreviewEntry>) {
    let now = Instant::now();
    let aged = previews.iter().filter_map(|(id, entry)| match &entry.phase {
        PreviewPhase::Settled { settled_at, .. } if entry.claim.is_none() => Some((id, *settled_at)),
        _ => None,
    });
    for id in expired_scan_result_ids(aged, now, SCAN_RESULT_TTL) {
        previews.remove(&id);
    }
}

/// What a claim attempt found.
pub(super) enum PreviewClaim {
    /// Claimed; the preview is still walking. Wait for `poll_claim`.
    Waiting,
    /// Claimed; the preview had already settled. The outcome itself is read
    /// back when the owner's task runs, which may be minutes later.
    AlreadySettled,
    /// No such preview (evicted, or a stale id from a reloaded window). The
    /// caller falls back to its own walk — the foolproof re-scan, never a hang.
    Unknown,
    /// Another operation already owns this preview. Same fallback as `Unknown`;
    /// ❌ never share, since the result is consumable exactly once.
    Refused,
}

/// Claims `preview_id` for `operation_id`, at most one owner ever.
pub(super) fn claim_preview(preview_id: &str, operation_id: &str) -> PreviewClaim {
    let Ok(mut previews) = PREVIEWS.write() else {
        return PreviewClaim::Unknown;
    };
    let Some(entry) = previews.get_mut(preview_id) else {
        return PreviewClaim::Unknown;
    };
    match &entry.claim {
        Some(owner) if owner != operation_id => return PreviewClaim::Refused,
        _ => entry.claim = Some(operation_id.to_string()),
    }
    match &entry.phase {
        PreviewPhase::InFlight(_) => PreviewClaim::Waiting,
        PreviewPhase::Settled { .. } => PreviewClaim::AlreadySettled,
    }
}

/// The settled outcome for a claimed preview, or `None` while it still walks.
/// An entry that vanished under its owner reads as `Cancelled`: the only ways
/// to remove one are an explicit release and the TTL sweep, and neither leaves
/// a result to consume.
pub(super) fn poll_claim(preview_id: &str) -> Option<ScanOutcome> {
    let Ok(previews) = PREVIEWS.read() else {
        return Some(ScanOutcome::Cancelled);
    };
    match previews.get(preview_id) {
        None => Some(ScanOutcome::Cancelled),
        Some(entry) => match &entry.phase {
            PreviewPhase::InFlight(_) => None,
            PreviewPhase::Settled { outcome, .. } => Some(outcome.clone()),
        },
    }
}

/// The operation that owns `preview_id`, if any. The progress bridge reads it
/// to decide whether a walk's counts also belong under an operation's id.
pub(super) fn claimed_operation(preview_id: &str) -> Option<String> {
    PREVIEWS.read().ok()?.get(preview_id)?.claim.clone()
}

/// Ends a claim without touching the result: the owner is done waiting and is
/// about to consume it. Re-arms the TTL, which is right — from here the entry
/// is an ordinary unconsumed result.
pub(super) fn finish_claim(preview_id: &str) {
    if let Ok(mut previews) = PREVIEWS.write()
        && let Some(entry) = previews.get_mut(preview_id)
    {
        entry.claim = None;
    }
}

/// Ends a claim AND frees the preview: cancels a still-running walk so it stops
/// working for nobody, and drops any result. For every path where the owner
/// stops without consuming (cancel, error, a queued op cancelled before
/// admission, the panic net). Idempotent, and a no-op for a preview nobody
/// claimed.
pub(super) fn abandon_claim(preview_id: &str) {
    let Ok(mut previews) = PREVIEWS.write() else {
        return;
    };
    let Some(entry) = previews.get(preview_id) else {
        return;
    };
    if entry.claim.is_none() {
        return;
    }
    if let PreviewPhase::InFlight(state) = &entry.phase {
        state.cancelled.store(true, Ordering::Relaxed);
    }
    previews.remove(preview_id);
}

/// Tries to get cached scan results for a preview, removing them from cache.
///
/// `requested_sources` is the selection the OPERATION was asked to act on. A
/// cached entry that describes a different set is not a cache hit: the entry is
/// dropped, a warn names both lists, and the caller falls through to its own
/// fresh scan. Order doesn't matter (it's a frontend detail, and `per_path` is
/// already order-rebuilt), so the comparison is set-wise — and deliberately
/// literal: if the frontend can hand back a path that differs only by a
/// trailing separator, that belongs fixed at the IPC edge, not softened here
/// into another belief.
pub(super) fn take_cached_scan_result(preview_id: &str, requested_sources: &[PathBuf]) -> Option<ScanResult> {
    let Ok(mut previews) = PREVIEWS.write() else {
        return None;
    };
    let entry = previews.remove(preview_id)?;
    let PreviewPhase::Settled {
        result: Some(cached), ..
    } = entry.phase
    else {
        // Still walking, or settled without a result (error / cancel). Neither
        // is a cache hit, and putting an in-flight entry back is what keeps a
        // premature take from cancelling a walk its owner is waiting on.
        if let PreviewPhase::InFlight(_) = entry.phase {
            previews.insert(preview_id.to_string(), entry);
        }
        return None;
    };
    let cached = *cached;
    if !same_path_set(&cached.sources, requested_sources) {
        log::warn!(
            target: "cmdr_lib::write_operations",
            "scan preview {} describes {:?}, but the operation asked for {:?}; ignoring the cache and rescanning",
            preview_id,
            cached.sources,
            requested_sources
        );
        return None;
    }
    Some(ScanResult {
        files: cached.files,
        dirs: cached.dirs,
        file_count: cached.file_count,
        total_bytes: cached.total_bytes,
        dedup_bytes: cached.dedup_bytes,
        per_path: cached.per_path,
    })
}

/// Whether two path lists hold the same paths, ignoring order and duplicates.
fn same_path_set(a: &[PathBuf], b: &[PathBuf]) -> bool {
    let left: HashSet<&Path> = a.iter().map(PathBuf::as_path).collect();
    let right: HashSet<&Path> = b.iter().map(PathBuf::as_path).collect();
    left == right
}

/// Reads the cached totals for `preview_id` without consuming the entry. Keeps
/// the lock and the map behind the module wall for `get_scan_preview_totals`,
/// the compress dialog's size estimate.
pub(super) fn cached_scan_totals(preview_id: &str) -> Option<super::types::ScanPreviewTotals> {
    let previews = PREVIEWS.read().ok()?;
    let PreviewPhase::Settled {
        result: Some(cached), ..
    } = &previews.get(preview_id)?.phase
    else {
        return None;
    };
    Some(super::types::ScanPreviewTotals {
        files_total: cached.file_count,
        dirs_total: cached.dirs.len(),
        bytes_total: cached.total_bytes,
        dedup_bytes_total: cached.dedup_bytes,
        estimated_compressed_bytes: cached.estimated_compressed_bytes.clone(),
    })
}

/// Seeds an entry that deliberately fails `settle_preview`'s coherence canary,
/// for the fixtures that pin what downstream does when a half-empty preview
/// reaches it anyway. The canary is a `debug_assert!`, so in a release build an
/// incoherent entry still lands in the cache and the drivers still have to
/// handle it without lying; these fixtures are that defense's only proof.
/// ❌ Not a general-purpose seeder: everything else goes through
/// `settle_preview`.
#[cfg(test)]
pub(crate) fn seed_incoherent_scan_result_for_test(
    preview_id: String,
    sources: Vec<PathBuf>,
    file_count: usize,
    total_bytes: u64,
) {
    let result = CachedScanResult {
        sources,
        files: Vec::new(),
        dirs: Vec::new(),
        file_count,
        total_bytes,
        dedup_bytes: total_bytes,
        per_path: Vec::new(),
        estimated_compressed_bytes: None,
    };
    if let Ok(mut previews) = PREVIEWS.write() {
        previews.insert(
            preview_id,
            PreviewEntry {
                phase: PreviewPhase::Settled {
                    outcome: ScanOutcome::Complete,
                    result: Some(Box::new(result)),
                    settled_at: Instant::now(),
                },
                claim: None,
            },
        );
    }
}

/// Test-only: backdates a settled entry so the next sweep sees it as stale.
/// Lets a test prove the TTL exemption without waiting out five real minutes,
/// and without making `SCAN_RESULT_TTL` injectable everywhere it's read.
#[cfg(test)]
pub(super) fn age_settled_entry_for_test(preview_id: &str, by: Duration) {
    if let Ok(mut previews) = PREVIEWS.write()
        && let Some(entry) = previews.get_mut(preview_id)
        && let PreviewPhase::Settled { settled_at, .. } = &mut entry.phase
    {
        *settled_at -= by;
    }
}

/// Test-only: publish a completed preview in one call, the shape
/// `run_scan_preview` produces. Keeps the fixtures that seed a consumable
/// result off the two-step register/settle dance.
#[cfg(test)]
pub(super) fn insert_scan_result(preview_id: String, result: CachedScanResult) {
    settle_preview(&preview_id, ScanOutcome::Complete, Some(result));
}

/// Drops everything known about `preview_id`. Called on dialog teardown
/// (`cancel_scan_preview`) so a result that finished scanning but was never
/// consumed by a started op doesn't linger until quit.
pub(super) fn release_preview(preview_id: &str) {
    if let Ok(mut previews) = PREVIEWS.write() {
        previews.remove(preview_id);
    }
}

/// Everything the app knows about each scan preview, in flight or settled.
/// ❌ Private on purpose, and it stays that way: every completed walk has to
/// pass `settle_preview`'s coherence canary, and every read has to go through
/// `take_cached_scan_result`'s request binding. A `pub(super)` static is a
/// choke point anyone can walk around.
static PREVIEWS: LazyLock<RwLock<HashMap<String, PreviewEntry>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

// ============================================================================
// FileInfo (used for scanning and sorting)
// ============================================================================

/// File info collected during scan (used for sorting).
#[derive(Debug, Clone)]
pub(super) struct FileInfo {
    pub path: PathBuf,
    /// Parent of the original source (used to compute relative path for destination)
    pub source_root: PathBuf,
    pub size: u64,
    /// Bytes this entry contributes to operation progress. Equals `size` for
    /// the first occurrence of an inode in the scan; `0` for subsequent
    /// hardlink pairs to the same inode. Active-phase counters (delete,
    /// trash, copy, move) sum this so the bar denominator (`total_bytes`,
    /// also dedup'd at scan time) and the numerator (`bytes_done`) stay in
    /// agreement. Without this split, a hardlink-heavy tree like cargo's
    /// `target/` overshoots — 81.6 GB delete numerator against a 59.84 GB
    /// scan denominator on a real-world repro.
    ///
    /// Set per call site: scan sets it from inode tracking; sites that build
    /// `FileInfo` without inode info (the oracle path in `walk_cached_entries`,
    /// MTP synthesis) fall back to `size` and accept the documented
    /// cross-boundary overshoot (see write_operations CLAUDE.md gotcha).
    pub progress_bytes: u64,
    pub modified: u64, // Unix timestamp in seconds
    pub created: u64,  // Unix timestamp in seconds
    pub is_symlink: bool,
}

impl FileInfo {
    /// Construct a `FileInfo` from filesystem metadata, treating it as the
    /// first observation of its inode (`progress_bytes == size`). Use
    /// `with_progress_bytes` to override when the scan-side inode tracker
    /// has already seen this inode.
    pub fn new(path: PathBuf, source_root: PathBuf, metadata: &std::fs::Metadata) -> Self {
        use std::time::UNIX_EPOCH;
        let size = metadata.len();
        Self {
            path,
            source_root,
            size,
            progress_bytes: size,
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            created: metadata
                .created()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            is_symlink: metadata.is_symlink(),
        }
    }

    /// Override `progress_bytes` (typically with `0`) when the scan-side
    /// inode tracker reports this file shares an inode with a previously-seen
    /// `FileInfo`. Keeps `size` (the actual file size) intact for sites that
    /// need it (sorting, conflict checks).
    #[must_use]
    pub fn with_progress_bytes(mut self, progress_bytes: u64) -> Self {
        self.progress_bytes = progress_bytes;
        self
    }

    /// Get extension for sorting (lowercase, empty string if none).
    pub fn extension(&self) -> String {
        self.path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    /// Get filename for sorting (lowercase).
    pub fn name_lower(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    }

    /// Compute the destination path for this file given the destination root.
    pub fn dest_path(&self, destination: &Path) -> PathBuf {
        // Strip source_root from path to get relative path, then join with destination
        if let Ok(relative) = self.path.strip_prefix(&self.source_root) {
            destination.join(relative)
        } else {
            // Fallback: just use the filename
            destination.join(self.path.file_name().unwrap_or_default())
        }
    }
}

/// Information about files to be processed.
pub(super) struct ScanResult {
    pub files: Vec<FileInfo>,
    /// For deletion: in reverse order, deepest first.
    pub dirs: Vec<PathBuf>,
    /// Not including directories.
    pub file_count: usize,
    /// Write footprint (un-dedup'd): every file at full size. Copy's
    /// disk-space check and active-phase bar use this. See
    /// `CopyScanResult::total_bytes`.
    pub total_bytes: u64,
    /// `du`-equivalent source footprint (hardlinks counted once). Delete's
    /// active phase uses this; the Copy dialog shows it as context. See
    /// `CopyScanResult::dedup_bytes`.
    pub dedup_bytes: u64,
    /// Per-source-path scan results, populated by volume scan previews so the
    /// copy pipeline can seed `source_hints` without re-statting. Empty for
    /// local-FS scans.
    pub per_path: Vec<(PathBuf, CopyScanResult)>,
}

#[cfg(test)]
#[path = "scan_cache_tests.rs"]
mod cache_binding_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    // ---- expired_scan_result_ids (TTL safety net) ----

    #[test]
    fn expired_scan_result_ids_returns_only_stale_entries() {
        let now = Instant::now();
        let ttl = Duration::from_secs(300);
        let fresh = now - Duration::from_secs(10);
        let stale = now - Duration::from_secs(400);
        let fresh_id = String::from("fresh");
        let stale_id = String::from("stale");
        let entries = vec![(&fresh_id, fresh), (&stale_id, stale)];

        let expired = expired_scan_result_ids(entries, now, ttl);

        assert_eq!(expired, vec![String::from("stale")]);
    }

    #[test]
    fn expired_scan_result_ids_empty_when_all_fresh() {
        let now = Instant::now();
        let ttl = Duration::from_secs(300);
        let a = String::from("a");
        let b = String::from("b");
        let entries = vec![(&a, now), (&b, now - Duration::from_secs(299))];

        let expired = expired_scan_result_ids(entries, now, ttl);

        assert!(expired.is_empty());
    }

    #[test]
    fn expired_scan_result_ids_boundary_is_strictly_greater_than_ttl() {
        // Exactly at the TTL is NOT expired; one tick past it is.
        let now = Instant::now();
        let ttl = Duration::from_secs(300);
        let at_ttl = String::from("at-ttl");
        let past_ttl = String::from("past-ttl");
        let entries = vec![
            (&at_ttl, now - Duration::from_secs(300)),
            (&past_ttl, now - Duration::from_secs(301)),
        ];

        let expired = expired_scan_result_ids(entries, now, ttl);

        assert_eq!(expired, vec![String::from("past-ttl")]);
    }

    // ---- FileInfo derived sort keys ----

    fn make_file_info(path: &str, source_root: &str) -> FileInfo {
        FileInfo {
            path: PathBuf::from(path),
            source_root: PathBuf::from(source_root),
            size: 0,
            progress_bytes: 0,
            modified: 0,
            created: 0,
            is_symlink: false,
        }
    }

    #[test]
    fn extension_is_lowercased() {
        // Kills: replace extension → String::new() / → "xyzzy".
        assert_eq!(make_file_info("/x/Photo.JPG", "/x").extension(), "jpg");
        assert_eq!(make_file_info("/x/archive.TAR.GZ", "/x").extension(), "gz");
    }

    #[test]
    fn extension_is_empty_for_no_extension() {
        assert_eq!(make_file_info("/x/README", "/x").extension(), "");
    }

    #[test]
    fn name_lower_is_lowercased_filename_only() {
        // Kills: replace name_lower → String::new() / → "xyzzy".
        assert_eq!(make_file_info("/x/y/Foo.Bar", "/x").name_lower(), "foo.bar");
    }

    #[test]
    fn dest_path_preserves_relative_layout_under_destination_root() {
        // Kills: replace dest_path → Default::default().
        let info = make_file_info("/src/dir/sub/leaf.txt", "/src");
        assert_eq!(
            info.dest_path(Path::new("/dst")),
            PathBuf::from("/dst/dir/sub/leaf.txt")
        );
    }

    #[test]
    fn dest_path_falls_back_to_filename_when_prefix_does_not_match() {
        // The fallback branch: when strip_prefix fails, just place the file
        // by name at the destination root.
        let info = make_file_info("/elsewhere/file.bin", "/different-root");
        assert_eq!(info.dest_path(Path::new("/dst")), PathBuf::from("/dst/file.bin"));
    }
}

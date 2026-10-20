//! The scan preview store: one map from `preview_id` to everything the engine
//! knows about that preview, in flight or settled, with the claims, the TTL
//! safety net, and the request binding over it. One [`ScanPreviews`] per
//! engine.
//!
//! A child of `scan_cache`, so it builds and reads a `CachedScanResult`
//! through its private fields while nothing else can.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

use super::super::types::{ScanPreviewPurpose, WriteOperationError};
use super::{CachedScanResult, ScanCostFacts, ScanResult};
use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::ScannedFile;
use cmdr_operation_log::types::OperationId;

// ============================================================================
// Scan preview state
// ============================================================================

/// State for a scan preview operation.
pub(crate) struct ScanPreviewState {
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
#[derive(Debug, Clone)]
pub(crate) enum ScanOutcome {
    /// The walk finished. A `CachedScanResult` is in the map for its owner.
    Complete,
    /// The walk stopped on an I/O or protocol error. Carries the failure a
    /// waiting operation reports as-is, typed and naming the item the walk
    /// stopped on, so a missing file reads as "couldn't find X" rather than a
    /// bare errno sentence with an empty path.
    Error(WriteOperationError),
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
    claim: Option<OperationId>,
}

/// How long a settled preview entry lives before the TTL safety net evicts it.
/// The normal lifecycle frees results far sooner (`take_cached_scan_result` at
/// op start, or `release_preview` on dialog teardown); this only catches the
/// case where neither fires.
///
/// ⚠️ A CLAIMED entry is exempt (`prune_expired`). With `LANE_BUDGET = 1` an
/// operation can sit Queued well past five minutes, and evicting the very
/// result its owner is waiting for would silently downgrade it to a re-walk.
pub(crate) const SCAN_RESULT_TTL: Duration = Duration::from_secs(300);

/// Returns the preview ids in `entries` whose `inserted_at` is older than
/// `ttl` relative to `now`. Pure so it's unit-testable without touching the
/// global cache. Callers remove the returned ids under the write lock.
pub(crate) fn expired_scan_result_ids<'a>(
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

/// Everything one engine knows about each scan preview, in flight or settled,
/// and the wakeup its waiters park on.
pub(crate) struct ScanPreviews {
    /// ❌ Private on purpose, and it stays that way: every completed walk has
    /// to pass `settle_preview`'s coherence canary, and every read has to go
    /// through `take_cached_scan_result`'s request binding. A reachable map is
    /// a choke point anyone can walk around.
    entries: RwLock<HashMap<String, PreviewEntry>>,
    /// Woken every time a preview settles. Waiters re-read their own entry
    /// rather than trusting the wakeup, so a spurious or coalesced notification
    /// is harmless.
    settled: Notify,
}

impl ScanPreviews {
    /// No previews.
    pub(crate) fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            settled: Notify::new(),
        }
    }

    /// The wakeup every settle fires.
    pub(crate) fn settled(&self) -> &Notify {
        &self.settled
    }

    /// The map, for writing. On poison it recovers AND FORGETS every preview,
    /// ❌ never keeps one: a panic may have left an entry half-edited, and
    /// `take_cached_scan_result` is what binds a delete's path set.
    /// A forgotten preview is a miss, and a miss rescans, which is the
    /// fail-closed answer (`delete/CLAUDE.md`). An operation waiting on a
    /// forgotten walk reads it as `Cancelled`, as `poll_claim` documents.
    fn write(&self) -> RwLockWriteGuard<'_, HashMap<String, PreviewEntry>> {
        let mut previews = self.entries.write_ignore_poison();
        if !self.entries.is_poisoned() {
            return previews;
        }
        previews.clear();
        self.entries.clear_poison();
        drop(previews);
        log::warn!(
            target: "cmdr_operations",
            "a panic poisoned the scan preview store; every preview is forgotten, so each operation scans its own sources"
        );
        self.write()
    }

    /// The map, for reading, forgotten first if a panic poisoned it (see
    /// [`write`](Self::write)). Only a writer can poison it, and none runs
    /// while this guard is held, so a clean check here stays clean.
    fn read(&self) -> RwLockReadGuard<'_, HashMap<String, PreviewEntry>> {
        let previews = self.entries.read_ignore_poison();
        if !self.entries.is_poisoned() {
            return previews;
        }
        drop(previews);
        drop(self.write());
        self.read()
    }
}

impl ScanPreviews {
    /// Registers a preview as in flight. Called by `start_scan_preview` before it
    /// spawns the walk, so a cancel or a claim arriving immediately after finds an
    /// entry.
    pub(crate) fn register_preview(&self, preview_id: String, state: Arc<ScanPreviewState>) {
        self.write().insert(
            preview_id,
            PreviewEntry {
                phase: PreviewPhase::InFlight(state),
                claim: None,
            },
        );
    }

    /// The in-flight state for `preview_id` (its cancel flag and progress
    /// interval), or `None` once the preview has settled.
    pub(crate) fn in_flight_state(&self, preview_id: &str) -> Option<Arc<ScanPreviewState>> {
        let previews = self.read();
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
    pub(crate) fn settle_preview(&self, preview_id: &str, outcome: ScanOutcome, result: Option<CachedScanResult>) {
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
                target: "cmdr_operations",
                "scan preview {} completed a walk of {} files with no per-source results; downstream hints will be empty",
                preview_id,
                result.file_count
            );
            debug_assert!(
                false,
                "a completed walk with file_count > 0 must carry per_path entries (preview {preview_id})"
            );
        }
        {
            let mut previews = self.write();
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
        self.settled.notify_waiters();
    }
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
pub(crate) enum PreviewClaim {
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

impl ScanPreviews {
    /// Claims `preview_id` for `operation_id`, at most one owner ever.
    pub(crate) fn claim_preview(&self, preview_id: &str, operation_id: &OperationId) -> PreviewClaim {
        let mut previews = self.write();
        let Some(entry) = previews.get_mut(preview_id) else {
            return PreviewClaim::Unknown;
        };
        match &entry.claim {
            Some(owner) if owner != operation_id => return PreviewClaim::Refused,
            _ => entry.claim = Some(operation_id.clone()),
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
    pub(crate) fn poll_claim(&self, preview_id: &str) -> Option<ScanOutcome> {
        match self.read().get(preview_id) {
            None => Some(ScanOutcome::Cancelled),
            Some(entry) => match &entry.phase {
                PreviewPhase::InFlight(_) => None,
                PreviewPhase::Settled { outcome, .. } => Some(outcome.clone()),
            },
        }
    }

    /// The operation that owns `preview_id`, if any. The progress bridge reads it
    /// to decide whether a walk's counts also belong under an operation's id.
    pub(crate) fn claimed_operation(&self, preview_id: &str) -> Option<OperationId> {
        self.read().get(preview_id)?.claim.clone()
    }

    /// Ends a claim without touching the result: the owner is done waiting and is
    /// about to consume it. Re-arms the TTL, which is right — from here the entry
    /// is an ordinary unconsumed result.
    pub(crate) fn finish_claim(&self, preview_id: &str) {
        if let Some(entry) = self.write().get_mut(preview_id) {
            entry.claim = None;
        }
    }

    /// Ends a claim AND frees the preview: cancels a still-running walk so it stops
    /// working for nobody, and drops any result. For every path where the owner
    /// stops without consuming (cancel, error, a queued op cancelled before
    /// admission, the panic net). Idempotent, and a no-op for a preview nobody
    /// claimed.
    pub(crate) fn abandon_claim(&self, preview_id: &str) {
        let mut previews = self.write();
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
    pub(crate) fn take_cached_scan_result(
        &self,
        preview_id: &str,
        requested_sources: &[PathBuf],
    ) -> Option<ScanResult> {
        self.take_settled(preview_id, requested_sources, None)
    }

    /// [`Self::take_cached_scan_result`] for a caller that DELETES the cached
    /// file list as is (the local delete walker): a preview walked for anything
    /// but a delete isn't a hit. A copy's walk lists what a copy reads, and
    /// only a delete's walk stops at a mount and counts a selected link as the
    /// link, so another purpose's list could name files a delete must not
    /// touch. The entry is dropped either way.
    pub(crate) fn take_cached_delete_scan(
        &self,
        preview_id: &str,
        requested_sources: &[PathBuf],
    ) -> Option<ScanResult> {
        self.take_settled(preview_id, requested_sources, Some(ScanPreviewPurpose::Delete))
    }

    fn take_settled(
        &self,
        preview_id: &str,
        requested_sources: &[PathBuf],
        walked_for: Option<ScanPreviewPurpose>,
    ) -> Option<ScanResult> {
        let mut previews = self.write();
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
        // Released before the warn below: a log line is file I/O, and every
        // claim, poll, and settle waits on this lock.
        drop(previews);
        let cached = *cached;
        if !same_path_set(&cached.sources, requested_sources) {
            log::warn!(
                target: "cmdr_operations",
                "scan preview {} describes {:?}, but the operation asked for {:?}; ignoring the cache and rescanning",
                preview_id,
                cached.sources.iter().map(LogPrivate::path).collect::<Vec<_>>(),
                requested_sources.iter().map(LogPrivate::path).collect::<Vec<_>>()
            );
            return None;
        }
        if let Some(wanted) = walked_for
            && cached.purpose != wanted
        {
            log::warn!(
                target: "cmdr_operations",
                "scan preview {} was walked for {:?}, but the operation needs one walked for {:?}; ignoring the cache and rescanning",
                preview_id,
                cached.purpose,
                wanted
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
            tree: cached.tree,
        })
    }
}

/// Whether two path lists hold the same paths, ignoring order and duplicates.
fn same_path_set(a: &[PathBuf], b: &[PathBuf]) -> bool {
    let left: HashSet<&Path> = a.iter().map(PathBuf::as_path).collect();
    let right: HashSet<&Path> = b.iter().map(PathBuf::as_path).collect();
    left == right
}

impl ScanPreviews {
    /// The cost facts of a settled preview, without consuming the entry, or `None`
    /// while it's in flight, after it failed, or once it's gone.
    pub(crate) fn cached_cost_facts(&self, preview_id: &str) -> Option<ScanCostFacts> {
        let previews = self.read();
        let PreviewPhase::Settled {
            result: Some(cached), ..
        } = &previews.get(preview_id)?.phase
        else {
            return None;
        };
        let per_file = match &cached.scanned_files {
            Some(kept) => Some(kept.clone()),
            None if !cached.files.is_empty() => Some(
                cached
                    .files
                    .iter()
                    .map(|file| ScannedFile {
                        size: file.size,
                        modified_at: None,
                    })
                    .collect(),
            ),
            None => None,
        };
        Some(ScanCostFacts {
            files: cached.file_count,
            dirs: cached.per_path.iter().map(|(_, scan)| scan.dir_count).sum(),
            bytes: cached.total_bytes,
            per_file,
            selected_folders: cached
                .per_path
                .iter()
                .filter(|(_, scan)| scan.top_level_is_directory)
                .count(),
            selected_file_sizes: cached
                .per_path
                .iter()
                .filter(|(_, scan)| !scan.top_level_is_directory)
                .map(|(_, scan)| scan.total_bytes)
                .collect(),
        })
    }

    /// Reads the cached totals for `preview_id` without consuming the entry. Keeps
    /// the lock and the map behind the module wall for `get_scan_preview_totals`,
    /// the compress dialog's size estimate.
    pub(crate) fn cached_scan_totals(&self, preview_id: &str) -> Option<super::super::types::ScanPreviewTotals> {
        let previews = self.read();
        let PreviewPhase::Settled {
            result: Some(cached), ..
        } = &previews.get(preview_id)?.phase
        else {
            return None;
        };
        Some(super::super::types::ScanPreviewTotals {
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
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn seed_incoherent_scan_result_for_test(
        &self,
        preview_id: String,
        sources: Vec<PathBuf>,
        file_count: usize,
        total_bytes: u64,
    ) {
        let result = CachedScanResult {
            purpose: ScanPreviewPurpose::Transfer,
            sources,
            files: Vec::new(),
            dirs: Vec::new(),
            file_count,
            total_bytes,
            dedup_bytes: total_bytes,
            per_path: Vec::new(),
            estimated_compressed_bytes: None,
            scanned_files: None,
            tree: None,
        };
        self.write().insert(
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

    /// Test-only: backdates a settled entry so the next sweep sees it as stale.
    /// Lets a test prove the TTL exemption without waiting out five real minutes,
    /// and without making `SCAN_RESULT_TTL` injectable everywhere it's read.
    #[cfg(test)]
    pub(crate) fn age_settled_entry_for_test(&self, preview_id: &str, by: Duration) {
        if let Some(entry) = self.write().get_mut(preview_id)
            && let PreviewPhase::Settled { settled_at, .. } = &mut entry.phase
        {
            *settled_at -= by;
        }
    }

    /// Test-only: a panic in the middle of mutating `preview_id`'s settled
    /// result in place, the shape a bug in any in-place edit leaves behind. The
    /// entry now claims `torn_sources` while still holding the files it walked
    /// for its old ones, and the store's lock is poisoned.
    #[cfg(test)]
    pub(crate) fn tear_mid_mutation_for_test(&self, preview_id: &str, torn_sources: Vec<PathBuf>) {
        let torn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut previews = self.entries.write_ignore_poison();
            if let Some(PreviewEntry {
                phase: PreviewPhase::Settled {
                    result: Some(cached), ..
                },
                ..
            }) = previews.get_mut(preview_id)
            {
                cached.sources = torn_sources;
            }
            panic!("a preview edit panicked halfway, on purpose");
        }));
        assert!(torn.is_err() && self.entries.is_poisoned());
    }

    /// Test-only: publish a completed preview in one call, the shape
    /// `run_scan_preview` produces. Keeps the fixtures that seed a consumable
    /// result off the two-step register/settle dance.
    #[cfg(test)]
    pub(crate) fn insert_scan_result(&self, preview_id: String, result: CachedScanResult) {
        self.settle_preview(&preview_id, ScanOutcome::Complete, Some(result));
    }

    /// Drops everything known about `preview_id`. Called on dialog teardown
    /// (`cancel_scan_preview`) so a result that finished scanning but was never
    /// consumed by a started op doesn't linger until quit.
    pub(crate) fn release_preview(&self, preview_id: &str) {
        self.write().remove(preview_id);
    }
}

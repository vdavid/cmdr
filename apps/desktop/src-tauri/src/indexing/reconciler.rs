//! Event reconciler: buffers FSEvents during scan, replays after scan completes.
//!
//! During the initial full scan, the watcher runs concurrently and buffers events.
//! Once the scan finishes, the reconciler replays only events that arrived *after*
//! the scanner read their affected path (using monotonically increasing event IDs).
//! Events with `event_id <= scan_start_event_id` are discarded because the scan data
//! is newer.
//!
//! After replay, the reconciler switches to live mode where events are processed
//! immediately.
//!
//! ## Integer-keyed resolution (milestone 4)
//!
//! All path resolution uses `store::resolve_path(conn, path)` to convert filesystem
//! paths to integer entry IDs. Write messages use integer-keyed variants:
//! `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`, `PropagateDeltaById`.
//! The reconciler holds a read connection (`rusqlite::Connection`) for path resolution.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rusqlite::Connection;
use tauri::AppHandle;

use crate::ignore_poison::IgnorePoison;
use crate::indexing::firmlinks;
use crate::indexing::metadata::extract_metadata;
use crate::indexing::scanner;
use crate::indexing::store::{self, IndexStore, IndexStoreError};
use crate::indexing::watcher::FsChangeEvent;
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};
use crate::indexing::{DEBUG_STATS, IndexPathSpace};
// Only the test-only `new()` / `new_with_throttle_window` and the rescan tests
// name the root volume id; production sites thread the real id through `new_for`.
#[cfg(test)]
use crate::indexing::ROOT_VOLUME_ID;
use crate::pluralize::pluralize;

mod escalation;
mod rescan;
mod rescan_route;
mod rescan_throttle;
mod throttle;
use escalation::resolve_escalation_anchor;
/// The shallow-anchor sweep window, re-exported for its out-of-module users:
/// `manager::resume_or_scan` reseeds it from `meta` at index start,
/// `scan_completion` restarts it whenever a full walk finishes, and `queries`
/// reads the record for the volume status surface.
pub(in crate::indexing) use rescan_route::{
    SHALLOW_COALESCED_KEY, SHALLOW_RESCAN_MIN_INTERVAL, SHALLOW_SWEEP_AT_KEY, now_unix, record_sweep_completed,
    seed_from_meta, sweep_record,
};
use rescan_throttle::RescanThrottle;
use throttle::{Throttle, ThrottleOutcome};

/// How the reconciler asks for a VISIBLE scanner rescan when a shallow/root-scale
/// `MustScanSubDirs` anchor should take the `start_scan` path instead of the
/// invisible reconcile hold (see [`rescan_route`]). Defaults to [`Self::Registry`]
/// on every production reconciler, so the two live-loop construction sites
/// (`scan_completion`, `event_loop::replay`) need no extra wiring.
pub(in crate::indexing) enum ScanTrigger {
    /// Production: spawn [`crate::indexing::manager::perform_registry_rescan`],
    /// which re-resolves this volume's manager in the registry and runs a fresh
    /// single-flight `start_scan`.
    Registry,
    /// Test: don't touch the registry (no Tauri runtime under a unit test).
    #[cfg(test)]
    Disabled,
    /// Test: record the trigger labels so a test can assert routing happened.
    #[cfg(test)]
    Recording(Arc<Mutex<Vec<String>>>),
}

/// The live-path throttle, carrying the exact upsert to replay on the trailing
/// flush (see [`PendingUpsert`]). Only the live reconciler holds one; the replay
/// path threads `None` so journal catch-up stays unthrottled. Visible to
/// `indexing` because it rides `process_fs_event`'s signature.
pub(super) type LiveThrottle = Throttle<PendingUpsert>;

/// The suppressed upsert a throttled file's trailing flush replays. Built from
/// the already-stat'd suppressed event, so the sweep never re-stats (which would
/// risk blocking the live loop on a dead mount, and add a phantom-apply-on-
/// deleted-file case). Only regular files are throttled, so `is_directory` /
/// `is_symlink` are always false at flush time.
pub(super) struct PendingUpsert {
    parent_id: i64,
    name: String,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    nlink: Option<u64>,
}

// ── EventReconciler ──────────────────────────────────────────────────

/// Maximum number of events the reconciler will buffer during a scan.
/// Beyond this, buffering stops and a full rescan is forced after the
/// current scan completes. The index is a disposable cache, so dropping
/// events is always safe.
const MAX_BUFFER_CAPACITY: usize = 500_000;

/// Aggregator for high-volume reconciler skip/escalation events. Each per-event line
/// is at TRACE (off by default; file chain captures Debug+ only); this aggregator emits
/// a single DEBUG summary every ~5 s, so error report bundles still carry the
/// existence-of-drift signal without the per-event noise. Two instances cover the two
/// classes that dominate normal log volume: [`UNKNOWN_PATH_SKIPS`] (removal for a path
/// not in the DB) and [`ESCALATED_MISSING_PARENTS`] (create/modify whose parent dir
/// isn't in the DB — now escalated to a subtree rescan rather than dropped).
///
/// Most are harmless build-output churn, but a sustained rate (or a sample path in an
/// unexpected tree) can flag real reconciler/index drift. Triagers: if you need the
/// per-event detail, run with `RUST_LOG=cmdr_lib::indexing::reconciler=trace,debug`.
mod skip_aggregator {
    use std::sync::Mutex;
    use std::time::Instant;

    use crate::pluralize::pluralize;

    const FLUSH_INTERVAL_SECS: u64 = 5;
    const SAMPLE_LEN: usize = 80;

    struct State {
        count_since_last_flush: u64,
        total: u64,
        last_flush: Instant,
        /// One representative path from the current window. Truncated to SAMPLE_LEN
        /// chars so a verbose log dir doesn't blow up the line. Cleared on flush.
        sample: Option<String>,
    }

    /// One skip category: its own rolling state plus the words for the summary line
    /// (`"skipped {unit}s {reason} in …"`).
    pub(super) struct SkipAggregator {
        state: Mutex<Option<State>>,
        /// Counted noun, e.g. `"removal"` → "skipped 3 removals".
        unit: &'static str,
        /// Reason phrase, e.g. `"for unknown paths"`.
        reason: &'static str,
    }

    impl SkipAggregator {
        const fn new(unit: &'static str, reason: &'static str) -> Self {
            Self {
                state: Mutex::new(None),
                unit,
                reason,
            }
        }

        /// Increment the counter and emit a summary if the flush interval has elapsed.
        /// Called on every skip (cheap: one mutex acquisition, one branch).
        pub(super) fn record(&self, path: &str) {
            let mut guard = match self.state.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            let state = guard.get_or_insert_with(|| State {
                count_since_last_flush: 0,
                total: 0,
                last_flush: Instant::now(),
                sample: None,
            });
            state.count_since_last_flush += 1;
            state.total += 1;
            if state.sample.is_none() {
                // Truncate on a CHAR boundary, not a byte index: NAS paths carry accented
                // names (e.g. "Külkeres síelés"), and `&path[..SAMPLE_LEN]` would panic
                // when byte SAMPLE_LEN lands mid-codepoint.
                let s = if path.chars().count() > SAMPLE_LEN {
                    format!("{}…", path.chars().take(SAMPLE_LEN).collect::<String>())
                } else {
                    path.to_string()
                };
                state.sample = Some(s);
            }
            if state.last_flush.elapsed().as_secs() >= FLUSH_INTERVAL_SECS {
                let count = state.count_since_last_flush;
                let total = state.total;
                let sample = state.sample.clone().unwrap_or_default();
                let secs = state.last_flush.elapsed().as_secs_f64();
                state.count_since_last_flush = 0;
                state.last_flush = Instant::now();
                state.sample = None;
                let (unit, reason) = (self.unit, self.reason);
                // Drop the lock before logging so the message format won't reenter under it.
                drop(guard);
                log::debug!(
                    "Reconciler: skipped {} {reason} in {secs:.1}s [{total} total], sample: {sample}",
                    pluralize(count, unit)
                );
            }
        }
    }

    /// Removals for a path that isn't in the DB (mostly harmless build-output churn).
    pub(super) static UNKNOWN_PATH_SKIPS: SkipAggregator = SkipAggregator::new("removal", "for unknown paths");
    /// Create/modify events whose parent dir isn't in the DB: escalated to a subtree
    /// rescan of the highest missing dir (Leak B) rather than dropped.
    pub(super) static ESCALATED_MISSING_PARENTS: SkipAggregator =
        SkipAggregator::new("event", "escalated for missing parents");
}

/// Buffers FSEvents during the initial scan and replays them after the scan completes.
pub struct EventReconciler {
    /// Events buffered during scan, in arrival order.
    buffer: Vec<FsChangeEvent>,
    /// Whether we're in buffering mode (scan in progress).
    buffering: bool,
    /// Set when the buffer cap is hit. Forces a full rescan after the
    /// current scan completes instead of replaying individual events.
    pub(super) buffer_overflow: bool,
    /// Paths pending MustScanSubDirs rescans, deduplicated. Shared with
    /// spawned rescan tasks so they can start the next rescan on completion.
    pending_rescans: Arc<Mutex<HashSet<PathBuf>>>,
    /// Whether a MustScanSubDirs rescan is currently running.
    rescan_active: Arc<AtomicBool>,
    /// The path of the CURRENTLY-running rescan (set at spawn, cleared on
    /// completion). `start_next_rescan` pops the path out of `pending_rescans`
    /// before spawning, so without this slot the removal-storm drop rule would see
    /// an empty set and drop nothing while a rescan is in flight. Also the seam
    /// the held-hourglass tier reads. `None` when no rescan runs.
    active_rescan_path: Arc<Mutex<Option<PathBuf>>>,
    /// Per-subtree rescan throttle (leading + trailing, 60 s window). Caps a
    /// hard-churning subtree to at most one reconcile per window, so a folder's
    /// size stays bounded-fresh without re-walking its subtree continuously.
    /// Shared with spawned rescan tasks (which record each completion) and
    /// consulted at pick time; its trailing re-kick rides the same sweep tick as
    /// `throttle`, via [`EventReconciler::sweep_rescan_throttle`].
    rescan_throttle: Arc<Mutex<RescanThrottle>>,
    /// Per-file throttle for live upserts (leading + trailing, 60 s window). Only
    /// consulted on the live path; the trailing flush runs off the event loop's
    /// sweep tick via [`EventReconciler::sweep_throttle`].
    throttle: LiveThrottle,
    /// The volume's path space: pass-through for the boot disk, mount-relative strip
    /// for a mount-rooted external drive. Threaded to `process_fs_event` and the
    /// `MustScanSubDirs` `reconcile_subtree` so both speak the right space.
    space: IndexPathSpace,
    /// The volume this reconciler serves. Routes the rescan hourglass hold/release
    /// (and the completion emit) to THIS volume's `PendingSizes` tracker via
    /// `get_pending_sizes_for` — defaulting to the root-only handle would recreate
    /// the cross-volume bug the held tier fixes for clears.
    volume_id: String,
    /// How a shallow/root-scale `MustScanSubDirs` anchor requests the visible
    /// scanner path. `Registry` in production (spawns `perform_registry_rescan`);
    /// tests inject `Disabled`/`Recording`. See [`ScanTrigger`] and [`rescan_route`].
    scan_trigger: ScanTrigger,
}

impl EventReconciler {
    /// Create a new reconciler in buffering mode for the boot disk (`root` space,
    /// `root` volume id). Test-only convenience; production sites carry the real
    /// volume id + space through [`new_for`](Self::new_for). The scan trigger is
    /// `Disabled` so a shallow-anchor route doesn't touch the registry under a
    /// unit test; a test that wants to observe routing calls
    /// [`set_recording_scan_trigger`](Self::set_recording_scan_trigger).
    #[cfg(test)]
    pub fn new() -> Self {
        let mut reconciler = Self::new_for(ROOT_VOLUME_ID.to_string(), IndexPathSpace::root());
        reconciler.scan_trigger = ScanTrigger::Disabled;
        reconciler
    }

    /// Create a reconciler bound to a volume's id + path space. A mount-rooted
    /// external drive passes its space so live/replay resolution strips the mount
    /// root, and its id so the rescan hourglass routes to its own tracker.
    pub(super) fn new_for(volume_id: String, space: IndexPathSpace) -> Self {
        Self::with_space_and_throttle(volume_id, space, Throttle::new(resolve_downloads_prefix()))
    }

    /// Construct with a caller-supplied id + space + throttle (tests inject a short window).
    fn with_space_and_throttle(volume_id: String, space: IndexPathSpace, throttle: LiveThrottle) -> Self {
        Self {
            buffer: Vec::new(),
            buffering: true,
            buffer_overflow: false,
            pending_rescans: Arc::new(Mutex::new(HashSet::new())),
            rescan_active: Arc::new(AtomicBool::new(false)),
            active_rescan_path: Arc::new(Mutex::new(None)),
            rescan_throttle: Arc::new(Mutex::new(RescanThrottle::new())),
            throttle,
            space,
            volume_id,
            scan_trigger: ScanTrigger::Registry,
        }
    }

    /// Test constructor with an explicit throttle window, so the trailing flush is
    /// exercised without sleeping a real [`THROTTLE_WINDOW`].
    #[cfg(test)]
    pub(super) fn new_with_throttle_window(window: std::time::Duration) -> Self {
        let mut reconciler = Self::with_space_and_throttle(
            ROOT_VOLUME_ID.to_string(),
            IndexPathSpace::root(),
            Throttle::with_window(window, None),
        );
        reconciler.scan_trigger = ScanTrigger::Disabled;
        reconciler
    }

    /// Test-only: route shallow `MustScanSubDirs` anchors to a recording trigger so
    /// a test can assert the scanner path was taken (instead of the reconcile hold).
    #[cfg(test)]
    pub(in crate::indexing) fn set_recording_scan_trigger(&mut self, sink: Arc<Mutex<Vec<String>>>) {
        self.scan_trigger = ScanTrigger::Recording(sink);
    }

    /// Buffer an event during scan. If the buffer cap is reached, stops
    /// buffering and sets `buffer_overflow` to force a full rescan.
    pub fn buffer_event(&mut self, event: FsChangeEvent) {
        if !self.buffering || self.buffer_overflow {
            return;
        }
        if self.buffer.len() >= MAX_BUFFER_CAPACITY {
            log::warn!(
                // allowed-pluralize-noun: MAX_BUFFER_CAPACITY is the const 500_000.
                "Reconciler: buffer cap reached ({MAX_BUFFER_CAPACITY} events). \
                 Dropping further events; a full rescan will run after the current scan."
            );
            self.buffer_overflow = true;
            self.buffer.clear();
            self.buffer.shrink_to_fit();
            return;
        }
        self.buffer.push(event);
    }

    /// Replay buffered events after scan completes.
    ///
    /// - Events with `event_id <= scan_start_event_id` are skipped (scan data is newer).
    /// - Events with `event_id > scan_start_event_id` are processed (filesystem changed after
    ///   scan).
    /// - Returns the last processed event ID.
    pub fn replay(
        &mut self,
        scan_start_event_id: u64,
        conn: &Connection,
        writer: &IndexWriter,
        on_dirs_affected: &mut dyn FnMut(Vec<String>),
    ) -> Result<u64, String> {
        // Sort by event_id to process in order
        self.buffer.sort_by_key(|e| e.event_id);

        let total = self.buffer.len();
        let mut processed = 0u64;
        let mut last_event_id = scan_start_event_id;
        let mut affected_paths: Vec<String> = Vec::new();

        log::info!(
            "Reconciler: replaying {} (scan_start_event_id={scan_start_event_id})",
            pluralize(total as u64, "buffered event")
        );

        for event in &self.buffer {
            // Skip events that the scan already covered
            if event.event_id <= scan_start_event_id {
                continue;
            }

            // Replay stays unthrottled (None): journal catch-up must converge
            // fully and fast; throttling is a live-steady-state concern.
            // Missing-parent escalations DEFER into the pending set without starting
            // a rescan (no live queueing during replay); the live loop that follows
            // drains them via `kick_pending_rescans`.
            let mut escalation: Option<PathBuf> = None;
            if let Some(paths) = process_fs_event(event, &self.space, conn, writer, None, &mut escalation) {
                affected_paths.extend(paths);
            }
            if let Some(anchor) = escalation {
                self.pending_rescans.lock_ignore_poison().insert(anchor);
            }

            last_event_id = event.event_id;
            processed += 1;
        }

        // Notify caller of all affected paths
        if !affected_paths.is_empty() {
            on_dirs_affected(affected_paths);
        }

        // Store last event ID
        if last_event_id > scan_start_event_id {
            let _ = writer.send(WriteMessage::UpdateLastEventId(last_event_id));
        }

        log::info!(
            "Reconciler: replayed {processed}/{} (last_event_id={last_event_id})",
            pluralize(total as u64, "event")
        );
        Ok(last_event_id)
    }

    /// Switch from buffering to live mode. Clears the buffer.
    pub fn switch_to_live(&mut self) {
        self.buffering = false;
        self.buffer_overflow = false;
        self.buffer.clear();
        self.buffer.shrink_to_fit();
        log::info!("Reconciler: switched to live mode");
    }

    /// Process a single event in live mode.
    ///
    /// Collects affected directory paths into `pending_paths` for batched
    /// emission by the caller (1s flush interval). Returns the event ID
    /// on success, or `None` if still buffering.
    pub fn process_live_event(
        &mut self,
        event: &FsChangeEvent,
        conn: &Connection,
        writer: &IndexWriter,
        pending_paths: &mut HashSet<String>,
    ) -> Option<u64> {
        if self.buffering {
            self.buffer_event(event.clone());
            return None;
        }

        // Handle MustScanSubDirs
        if event.flags.must_scan_sub_dirs {
            // Keep the path absolute (the reconcile walks the FS from it); the
            // mount-relative strip happens inside `reconcile_subtree`'s resolve.
            let absolute = self.space.absolute(&event.path);
            // Depth-split routing: a shallow/root-scale anchor takes the visible
            // scanner path; a deep/narrow one keeps the throttled reconcile drain.
            self.route_must_scan_sub_dirs(PathBuf::from(&absolute), writer);
            return Some(event.event_id);
        }

        // Missing-parent escalation (Leak B): if the event's parent chain isn't in
        // the index, `process_fs_event` sets `escalation` to the rescan anchor
        // instead of dropping the credit. Live mode queues it right away.
        let mut escalation: Option<PathBuf> = None;
        if let Some(affected_paths) = process_fs_event(
            event,
            &self.space,
            conn,
            writer,
            Some(&mut self.throttle),
            &mut escalation,
        ) {
            pending_paths.extend(affected_paths);
        }
        if let Some(anchor) = escalation {
            self.queue_must_scan_sub_dirs(anchor, writer);
        }

        // UpdateLastEventId is sent once per batch by the caller (process_live_batch)
        // instead of per-event, to reduce writer channel pressure during event storms.

        Some(event.event_id)
    }

    /// Flush every throttled key whose 60 s window has elapsed, applying its
    /// last-seen size (never re-statting). Called on the event loop's ~1 s
    /// throttle-sweep tick. Returns the ancestor paths whose `dir_stats` the
    /// flushes changed, for the caller's batched `index-dir-updated` emit.
    pub(super) fn sweep_throttle(&mut self, writer: &IndexWriter, now: Instant) -> Vec<String> {
        let flushes = self.throttle.sweep(now);
        let mut affected: Vec<String> = Vec::new();
        for (path, upsert) in flushes {
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: upsert.parent_id,
                name: upsert.name,
                is_directory: false,
                is_symlink: false,
                logical_size: upsert.logical_size,
                physical_size: upsert.physical_size,
                modified_at: upsert.modified_at,
                inode: upsert.inode,
                nlink: upsert.nlink,
            });
            affected.extend(collect_ancestor_paths(&path));
        }
        affected
    }

    /// Whether the reconciler's event buffer overflowed during the scan.
    pub(super) fn did_buffer_overflow(&self) -> bool {
        self.buffer_overflow
    }

    /// Number of buffered events (for diagnostics).
    #[cfg(test)]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the reconciler is in buffering mode.
    #[cfg(test)]
    pub fn is_buffering(&self) -> bool {
        self.buffering
    }
}

// ── Subtree reconciliation ───────────────────────────────────────────

/// Summary of a subtree reconciliation.
pub(super) struct ReconcileSummary {
    pub added: u64,
    pub removed: u64,
    pub updated: u64,
    pub duration: std::time::Duration,
    /// Set when the reconcile couldn't anchor because the subtree root's chain is
    /// still (partly) missing from the index (the skip branch, or a parent that
    /// resolves to a file). Carries the escalation anchor — a rescan root strictly
    /// closer to the volume root — for the caller to re-queue. `None` on success.
    pub escalation: Option<PathBuf>,
}

/// A live directory child, normalized to the fields the per-dir diff needs.
/// The two walk sources build this identically: the local `read_dir` path from a
/// `std::fs::Metadata`, the network path from a `Volume` listing's `FileEntry`.
/// The diff is then source-agnostic — the same add/remove/modify/type-change
/// logic for both.
pub(super) struct LiveChild {
    pub name: String,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub snap: crate::indexing::metadata::MetadataSnapshot,
}

/// Outcome of diffing ONE directory's live children against its DB rows.
pub(super) struct DirDiff {
    pub added: u64,
    pub removed: u64,
    pub updated: u64,
    /// `(child_dir_id, child_name)` for every EXISTING child dir that matched a
    /// live dir — the caller recurses into these (their id is already known).
    /// This is UNCONDITIONAL of whether the dir changed: an unchanged dir still
    /// recurses, because "unchanged at the parent's level" says nothing about
    /// whether its own subtree was ever listed. Gating this on `changed` is the
    /// reconcile-stops-at-the-root bug (see the fn doc).
    pub matched_child_dirs: Vec<(i64, String)>,
    /// Names of NEW child dirs created this pass — the caller flushes the writer,
    /// resolves their ids, then recurses (the id isn't known until the insert
    /// commits).
    pub new_child_dir_names: Vec<String>,
}

/// Diff one directory's live listing against its DB children and emit only the
/// differences (`UpsertEntryV2` for adds + changes, `DeleteEntryById` /
/// `DeleteSubtreeById` for vanished rows). Shared by the local `read_dir` walk
/// and the network `Volume`-trait walk so the diff logic lives in ONE place.
///
/// Writes NOTHING for an unchanged row (the no-op-cheap property the perf bench
/// relied on): a matched row is re-UPSERTed only when its size/mtime (file) or
/// mtime (dir/symlink) actually differs, so a rescan over an unchanged tree
/// issues zero entry-row writes and never touches the catastrophic
/// `INSERT OR REPLACE`/`platform_case` path.
///
/// The recursion set (`matched_child_dirs`) is DECOUPLED from that write
/// decision: every matched child dir is returned for the caller to descend into,
/// changed or not. The walk must re-list each existing child dir's subtree on a
/// reconcile — a child being unchanged at THIS dir's level proves nothing about
/// whether its subtree was ever scanned. (Re-gating recursion on `changed` is the
/// reconcile-stops-at-the-root prod bug: a share with only its top dirs indexed
/// would match them, write nothing, recurse nowhere, and "complete" instantly
/// over an unscanned tree.)
pub(super) fn diff_dir_against_db(
    dir_id: i64,
    live_children: &[LiveChild],
    db_children: &[store::EntryRow],
    writer: &IndexWriter,
) -> DirDiff {
    let mut added: u64 = 0;
    let mut removed: u64 = 0;
    let mut updated: u64 = 0;
    let mut matched_child_dirs: Vec<(i64, String)> = Vec::new();
    let mut new_child_dir_names: Vec<String> = Vec::new();

    let mut db_by_name: std::collections::HashMap<String, &store::EntryRow> =
        std::collections::HashMap::with_capacity(db_children.len());
    for row in db_children {
        db_by_name.insert(store::normalize_for_comparison(&row.name), row);
    }

    let mut matched_db_keys: HashSet<String> = HashSet::with_capacity(live_children.len());

    for child in live_children {
        let norm_name = store::normalize_for_comparison(&child.name);
        let is_dir = child.is_directory;
        let is_symlink = child.is_symlink;
        let snap = &child.snap;

        if let Some(db_row) = db_by_name.get(&norm_name) {
            matched_db_keys.insert(norm_name);

            let changed = if is_dir || is_symlink {
                snap.modified_at != db_row.modified_at
            } else {
                snap.logical_size != db_row.logical_size || snap.modified_at != db_row.modified_at
            };

            if changed {
                // Type change (file↔dir): delete first so counts propagate correctly.
                // Dir→file: DeleteSubtreeById removes children + propagates negative deltas.
                // File→dir: DeleteEntryById propagates negative file_count delta.
                // Then UpsertEntryV2 inserts fresh with the correct type and positive deltas.
                if db_row.is_directory != is_dir {
                    if db_row.is_directory {
                        let _ = writer.send(WriteMessage::DeleteSubtreeById(db_row.id));
                    } else {
                        let _ = writer.send(WriteMessage::DeleteEntryById(db_row.id));
                    }
                }

                let _ = writer.send(WriteMessage::UpsertEntryV2 {
                    parent_id: dir_id,
                    name: child.name.clone(),
                    is_directory: is_dir,
                    is_symlink,
                    logical_size: snap.logical_size,
                    physical_size: snap.physical_size,
                    modified_at: snap.modified_at,
                    inode: snap.inode,
                    nlink: snap.nlink,
                });
                updated += 1;
            }

            // Recurse into this child if it's a (non-symlink) dir on disk:
            // - was already a dir in the DB → recurse the existing id;
            // - was a file, now a dir (type change) → the old row was deleted and
            //   a fresh dir inserted above, so treat it like a new dir: resolve the
            //   new id after a flush, then recurse (its children must be walked).
            if is_dir && !is_symlink {
                if db_row.is_directory {
                    matched_child_dirs.push((db_row.id, child.name.clone()));
                } else {
                    new_child_dir_names.push(child.name.clone());
                }
            }
        } else {
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id: dir_id,
                name: child.name.clone(),
                is_directory: is_dir,
                is_symlink,
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            });
            // UpsertEntryV2 auto-propagates deltas in the writer.
            added += 1;

            if is_dir && !is_symlink {
                new_child_dir_names.push(child.name.clone());
            }
        }
    }

    for row in db_children {
        let norm_name = store::normalize_for_comparison(&row.name);
        if !matched_db_keys.contains(&norm_name) {
            if row.is_directory {
                let _ = writer.send(WriteMessage::DeleteSubtreeById(row.id));
            } else {
                let _ = writer.send(WriteMessage::DeleteEntryById(row.id));
            }
            removed += 1;
        }
    }

    DirDiff {
        added,
        removed,
        updated,
        matched_child_dirs,
        new_child_dir_names,
    }
}

// ── Full-rescan finish (shared) ──────────────────────────────────────

/// Number of dir ids per `MarkDirsListed` message. Bounds each message's size;
/// the writer-side `IndexStore::mark_dirs_listed` chunks the SQL `UPDATE` further
/// (at 900, under SQLite's bound-parameter ceiling) inside a savepoint, so this is
/// only message-level batching — never load-bearing for SQL correctness.
const MARK_CHUNK: usize = 10_000;

/// Emit `MarkDirsListed` for every successfully-listed dir id, chunked. A no-op
/// when empty. The only failure is a writer send (the writer thread is gone); the
/// caller maps that into its own error type.
pub(super) fn send_marks(listed_ids: &[i64], epoch: u64, writer: &IndexWriter) -> Result<(), IndexStoreError> {
    for chunk in listed_ids.chunks(MARK_CHUNK) {
        writer.send(WriteMessage::MarkDirsListed {
            ids: chunk.to_vec(),
            epoch,
        })?;
    }
    Ok(())
}

/// The full-rescan FINISH, in ONE place so the network and (future) local
/// full-tree reconcile can't drift on the ordering invariant: stamp every
/// successfully-listed dir's `listed_epoch` FIRST, then run a SINGLE
/// `ComputeAllAggregates`.
///
/// The mark-before-aggregate order is load-bearing: aggregating
/// before the marks rolls the whole tree to `min_subtree_epoch = 0` (incomplete);
/// a mark queued after the aggregate drags that dir's ancestors to incomplete. The
/// single in-order writer guarantees the order once sequenced here.
///
/// This is the single-aggregate coverage refresh the full-rescan path uses, NOT
/// the per-dir `PropagateMinSubtreeEpoch` propagation `reconcile_subtree` runs (a
/// ~2.4x regression at full scale; that stays the small-scope live path). A no-op
/// reconcile still runs the aggregate (cheap O(dirs) bulk SQL since no
/// `InsertEntriesV2` ran) so coverage re-stamps to the new epoch; it writes no
/// entry rows. The only failure is a writer send; the caller maps it.
pub(super) fn finish_reconcile(listed_ids: &[i64], epoch: u64, writer: &IndexWriter) -> Result<(), IndexStoreError> {
    send_marks(listed_ids, epoch, writer)?;
    // `Sql`, not `Maps`: a reconcile writes via `UpsertEntryV2` (maps empty in the
    // happy case), but a verification subtree scan's `InsertEntriesV2` can leave
    // the shared writer's accumulator polluted with subtree-only data. Declaring
    // `Sql` recomputes from committed rows and can't be poisoned by that (Leak D).
    writer.send(WriteMessage::ComputeAllAggregates { source: AggSource::Sql })?;
    Ok(())
}

/// RAII bracket for a FULL reconcile's bulk walk: tells the writer to STOP
/// per-entry ancestor `dir_stats` propagation for the walk's duration, then
/// RESTORES it on EVERY scope exit (clean finish, cancel, empty-root, error, or
/// panic) so the shared, long-lived writer is never left non-propagating for the
/// LIVE event loop that runs afterwards.
///
/// Why suppress: the full reconcile emits thousands of `UpsertEntryV2` / `Delete*`;
/// letting each one walk the ancestor chain
/// (`propagate_delta_by_id` / `propagate_min_subtree_epoch` /
/// `propagate_recursive_has_symlinks`) is O(entries × tree-depth) and wedges the
/// writer for hours on a large delta. It's also pure waste: `finish_reconcile`'s
/// single `ComputeAllAggregates` recomputes every dir's stats from the entries
/// table, overwriting whatever per-entry propagation produced. The LIVE path
/// (`reconcile_subtree`, FSEvents) has NO final aggregate, so it MUST keep
/// propagating — which is exactly why this guard restores the default on exit.
///
/// Suppression is a DEBT, so the bracket also records it: `begin` marks the
/// `dir_stats` ledger unpaid (durably, via `MarkLedgerUnpaid`) and the exit pays
/// it (`PayLedgerIfUnpaid`, a no-op once the walk's own `ComputeAllAggregates`
/// disarmed the latch). Without that, a walk that never reaches its terminal
/// aggregate leaves every ancestor of a mid-walk-discovered directory claiming an
/// exact size over a descendant at `listed_epoch = 0` — measured in production as
/// 249 lying directories after a rescan the user quit 5 seconds in. The durable
/// half is what covers process death, where no `Drop` runs at all.
///
/// All sends are best-effort: on a hard writer-gone error the send fails and is
/// ignored, matching how the surrounding walk already treats writer sends.
pub(super) struct BulkReconcileGuard {
    writer: IndexWriter,
}

impl BulkReconcileGuard {
    /// Begin the bracket: record the debt, then disable per-entry propagation.
    ///
    /// Order matters: the marker must be committed BEFORE the first suppressed
    /// write, or a death between the two would leave drift with a paid ledger.
    pub(super) fn begin(writer: &IndexWriter) -> Self {
        let _ = writer.send(WriteMessage::MarkLedgerUnpaid);
        let _ = writer.send(WriteMessage::SetDeltaPropagation(false));
        Self { writer: writer.clone() }
    }
}

impl Drop for BulkReconcileGuard {
    fn drop(&mut self) {
        // Re-enable per-entry propagation for the subsequent live path, then pay
        // the ledger if this walk never ran its own aggregate.
        let _ = self.writer.send(WriteMessage::SetDeltaPropagation(true));
        let _ = self.writer.send(WriteMessage::PayLedgerIfUnpaid);
    }
}

/// Reconcile a subtree by diffing the filesystem against the DB directory-by-directory.
///
/// Unlike `scanner::scan_subtree` which deletes all descendants then re-inserts,
/// this function walks each directory, compares children by name, and only writes
/// the differences. Safe to interrupt at any point: the DB is never in a
/// partially-deleted state.
///
/// This is the LIVE small-scope fill path (per-navigation verifier,
/// `MustScanSubDirs`, SMB-overflow `FullRefresh`): it propagates coverage per
/// listed dir. The full-rescan path does NOT use this — the network rescan walks
/// via `volume_scanner::reconcile_volume_via_trait`, which reuses the shared
/// [`diff_dir_against_db`] but stamps + runs ONE `ComputeAllAggregates` (the
/// single-aggregate constraint the perf bench measured), never per-dir propagation.
pub(super) fn reconcile_subtree(
    root: &Path,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    cancelled: &AtomicBool,
) -> Result<ReconcileSummary, String> {
    let start = Instant::now();
    let mut added: u64 = 0;
    let mut removed: u64 = 0;
    let mut updated: u64 = 0;

    // The epoch every dir we successfully list this pass is stamped with. A
    // reconcile *stamps* with the current epoch; it never bumps it. Read once.
    let epoch = IndexStore::read_current_epoch(conn).unwrap_or(1);
    // Every dir whose direct contents we successfully list (incl. empty), so we
    // can `MarkDirsListed` them after the walk and lift ancestor coverage.
    // Without this, a reconcile-discovered subtree stays `listed_epoch = 0`
    // forever and drags every ancestor to incomplete — the exact local-live-path
    // regression this milestone guards against.
    let mut listed_dir_ids: Vec<i64> = Vec::new();

    // The absolute path in this volume's world (firmlink-normalized for the boot
    // disk, raw for a mount-rooted drive); the mount-relative strip is applied only
    // at the `resolve_abs` argument, so `root_str` stays absolute for the FS reads.
    let root_str = space.absolute(&root.to_string_lossy());
    let root_id = match space.resolve_abs(conn, &root_str) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Root not in DB. This happens when must_scan_sub_dirs fires for a
            // newly created/copied directory. Try to create it: resolve the parent,
            // stat the root, and upsert it via the writer.
            let parent_path = compute_parent_path(&root_str);
            let parent_id = match space.resolve_abs(conn, &parent_path) {
                Ok(Some(id)) => {
                    // Harden against the type-change orphan class: the parent must be
                    // a DIRECTORY row before we parent new entries under it. A parent
                    // that resolves to a FILE (a stale file→dir type change) means the
                    // chain is broken; escalate to a rescan that re-lists the deepest
                    // existing dir, healing it — never upsert under a file id.
                    let parent_is_dir = matches!(
                        IndexStore::get_entry_by_id(conn, id),
                        Ok(Some(e)) if e.is_directory
                    );
                    if parent_is_dir {
                        id
                    } else {
                        log::debug!("reconcile_subtree: parent of {root_str} is not a directory row, escalating");
                        return Ok(ReconcileSummary {
                            added: 0,
                            removed: 0,
                            updated: 0,
                            duration: start.elapsed(),
                            escalation: resolve_escalation_anchor(space, conn, &root_str),
                        });
                    }
                }
                Ok(None) => {
                    // Neither root nor parent in DB: the chain above is (partly)
                    // missing (Leak B, subtree-scan variant). Escalate to a rescan
                    // anchored at the highest missing dir (strictly closer to the
                    // volume root, so the caller's re-queue converges by depth)
                    // rather than dropping the whole subtree's credit.
                    log::debug!("reconcile_subtree: neither root nor parent in DB, escalating: {root_str}");
                    return Ok(ReconcileSummary {
                        added: 0,
                        removed: 0,
                        updated: 0,
                        duration: start.elapsed(),
                        escalation: resolve_escalation_anchor(space, conn, &root_str),
                    });
                }
                Err(e) => return Err(format!("resolve_path for parent: {e}")),
            };

            // Stat the root directory and upsert it
            let metadata = match std::fs::symlink_metadata(root) {
                Ok(m) => m,
                Err(e) => {
                    // The root vanished between the event and now: nothing to index
                    // and no chain to heal (a real delete will arrive as its own
                    // event), so no escalation.
                    log::debug!("reconcile_subtree: can't stat root {root_str}: {e}");
                    return Ok(ReconcileSummary {
                        added: 0,
                        removed: 0,
                        updated: 0,
                        duration: start.elapsed(),
                        escalation: None,
                    });
                }
            };

            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let snap = extract_metadata(&metadata, metadata.is_dir(), metadata.is_symlink());
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id,
                name,
                is_directory: metadata.is_dir(),
                is_symlink: metadata.is_symlink(),
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                // Null the inode on FAT/exFAT (unstable derived inode).
                inode: space.trust_inode(snap.inode),
                nlink: snap.nlink,
            });

            // Flush so the read connection can see the new entry
            if let Err(e) = writer.flush_blocking() {
                log::warn!("reconcile_subtree: flush after root upsert failed: {e}");
            }
            added += 1;

            match space.resolve_abs(conn, &root_str) {
                Ok(Some(id)) => id,
                Ok(None) => {
                    // We just upserted and flushed, yet the row is absent: a write
                    // race or a concurrent delete. Re-queuing the same root would
                    // spin, so don't escalate; the next real event heals it.
                    log::warn!("reconcile_subtree: root still not in DB after upsert, skipping: {root_str}");
                    return Ok(ReconcileSummary {
                        added,
                        removed: 0,
                        updated: 0,
                        duration: start.elapsed(),
                        escalation: None,
                    });
                }
                Err(e) => return Err(format!("resolve_path for root after upsert: {e}")),
            }
        }
        Err(e) => return Err(format!("resolve_path for root: {e}")),
    };

    let mut queue: VecDeque<(PathBuf, i64)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), root_id));

    // Collect newly-created directories so we can flush the writer, resolve their IDs,
    // and then queue them for recursive processing.
    let mut new_dir_paths: Vec<PathBuf> = Vec::new();

    while let Some((dir_path, dir_id)) = queue.pop_front() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let fs_children = match read_fs_children(&dir_path, space) {
            Some(c) => c,
            None => continue,
        };
        // We successfully listed this dir's direct contents (an empty listing
        // still counts). Stamp it at the current epoch after the walk.
        listed_dir_ids.push(dir_id);

        let db_children =
            IndexStore::list_children_on(dir_id, conn).map_err(|e| format!("list_children_on({dir_id}): {e}"))?;

        // Normalize the local listing into source-agnostic `LiveChild`s and run
        // the shared per-dir diff (same logic the network walk uses).
        let live_children: Vec<LiveChild> = fs_children
            .iter()
            .map(|(name, meta, is_symlink)| {
                let is_dir = meta.is_dir();
                let mut snap = extract_metadata(meta, is_dir, *is_symlink);
                // Null the inode on FAT/exFAT so the value `diff_dir_against_db`
                // stores can never feed a false rename match.
                snap.inode = space.trust_inode(snap.inode);
                LiveChild {
                    name: name.clone(),
                    is_directory: is_dir,
                    is_symlink: *is_symlink,
                    snap,
                }
            })
            .collect();

        let diff = diff_dir_against_db(dir_id, &live_children, &db_children, writer);
        added += diff.added;
        removed += diff.removed;
        updated += diff.updated;
        for (child_id, child_name) in diff.matched_child_dirs {
            queue.push_back((dir_path.join(child_name), child_id));
        }
        for child_name in diff.new_child_dir_names {
            new_dir_paths.push(dir_path.join(child_name));
        }

        // If we found new directories and the queue is empty (current level done),
        // flush the writer so the read connection can resolve the new IDs.
        if !new_dir_paths.is_empty() && queue.is_empty() {
            if let Err(e) = writer.flush_blocking() {
                log::warn!("reconcile_subtree: flush failed: {e}");
            }
            for new_dir in new_dir_paths.drain(..) {
                let path_str = space.absolute(&new_dir.to_string_lossy());
                if let Ok(Some(id)) = space.resolve_abs(conn, &path_str) {
                    queue.push_back((new_dir, id));
                }
            }
        }
    }

    // Stamp every dir we listed at the current epoch, then lift ancestor
    // coverage. The walk collected ids shallow→deep (BFS), so recompute
    // deepest-first: a parent's `min_subtree_epoch` reads its children's stored
    // values, which must already reflect this pass. `propagate_min_subtree_epoch`
    // short-circuits once a value stabilizes, so the repeated up-walks are cheap.
    // (This is the SMALL-SCOPE live path; a full rescan uses the single-aggregate
    // path in `volume_scanner`, not per-dir propagation — see the fn doc.)
    if !listed_dir_ids.is_empty() {
        // Chunk under SQLite's bound-parameter ceiling (+1 for the epoch param).
        const MARK_CHUNK: usize = 900;
        for chunk in listed_dir_ids.chunks(MARK_CHUNK) {
            if let Err(e) = writer.send(WriteMessage::MarkDirsListed {
                ids: chunk.to_vec(),
                epoch,
            }) {
                log::warn!("reconcile_subtree: failed to send MarkDirsListed: {e}");
            }
        }
        for dir_id in listed_dir_ids.iter().rev() {
            let _ = writer.send(WriteMessage::PropagateMinSubtreeEpoch(*dir_id));
        }
    }

    Ok(ReconcileSummary {
        added,
        removed,
        updated,
        duration: start.elapsed(),
        escalation: None,
    })
}

/// Read and filter filesystem children of a directory.
///
/// Shared by both local reconcile walks — the small-scope live `reconcile_subtree`
/// and the full-tree [`local_reconcile`](crate::indexing::local_reconcile) rescan.
/// Returns `None` when the directory itself can't be listed (a permission wall or a
/// vanished dir), distinct from `Some(vec![])` for an empty-but-readable dir.
///
/// Applies the SAME two filters the guarded-walker fresh scan uses, so a reconcile converges
/// to the identical DB the fresh scan would build:
/// - `scanner::should_exclude` (system/virtual prefixes, `/Volumes/`, the E2E
///   allowlist), and
/// - `scanner::is_canonicalization_alias` — the macOS root symlinks `/tmp`, `/var`,
///   `/etc` normalize to `/private/...`, so they're aliases of a real directory the
///   fresh scan stored under the canonical path. The fresh scan skips the alias
///   (`scanner::run_scan`); a reconcile that DIDN'T would find them absent from the
///   DB and re-add them every pass, diverging from the fresh-scan tree.
///   Skipping here keeps fresh and reconcile in lock-step.
pub(super) fn read_fs_children(
    dir_path: &Path,
    space: &IndexPathSpace,
) -> Option<Vec<(String, std::fs::Metadata, bool)>> {
    let read_dir = match std::fs::read_dir(dir_path) {
        Ok(rd) => rd,
        Err(e) => {
            log::debug!("reconcile: can't read {}: {e}", dir_path.display());
            return None;
        }
    };

    let mut children = Vec::new();
    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let child_path = dir_path.join(&name);
        let child_path_str = child_path.to_string_lossy();
        // The canonical absolute child path (firmlink-normalized only for the boot
        // disk); the scope comes from the volume kind so a mount-rooted drive skips
        // only junk basenames, not its own `/Volumes/X` subtree.
        let normalized_child = space.absolute(&child_path_str);
        if scanner::should_exclude(&normalized_child, space.exclusion_scope()) {
            continue;
        }
        // Skip the canonicalization-alias symlinks (/tmp, /var, /etc) so we don't
        // re-add what the fresh scan deliberately stored under the canonical
        // /private/... path.
        if scanner::is_canonicalization_alias(&child_path_str, &normalized_child) {
            continue;
        }
        if let Ok(meta) = std::fs::symlink_metadata(&child_path) {
            let is_symlink = meta.is_symlink();
            children.push((name, meta, is_symlink));
        }
    }
    Some(children)
}

// ── Event processing ─────────────────────────────────────────────────

/// Process a single filesystem event. Returns affected parent paths for UI notification.
///
/// Shared between replay and live mode. Normalizes paths, checks exclusions,
/// stats the file, resolves paths to integer entry IDs, and sends appropriate
/// integer-keyed write messages (`UpsertEntryV2`, `DeleteEntryById`, etc.).
///
/// `throttle` is `Some` ONLY on the live path (`process_live_event`). Replay and
/// cold-start pass `None` so journal catch-up applies every event immediately.
/// When present, a regular file's in-place rewrite may be suppressed here (its
/// last-seen size flushed later by [`EventReconciler::sweep_throttle`]).
///
/// `escalation` is an out-param for Leak B: when a create/modify event's parent
/// chain is (partly) missing from the index, this sets it to the rescan anchor
/// (the highest missing dir) instead of dropping the credit. The caller
/// (`process_live_event` live, buffered replay) owns the reconciler state and
/// queues or defers it. `None` means nothing to escalate. A typed `PathBuf`
/// out-param, never a string signal — no string-matching classification.
pub(super) fn process_fs_event(
    event: &FsChangeEvent,
    space: &IndexPathSpace,
    conn: &Connection,
    writer: &IndexWriter,
    throttle: Option<&mut LiveThrottle>,
    escalation: &mut Option<PathBuf>,
) -> Option<Vec<String>> {
    // The canonical ABSOLUTE path in this volume's world. It stays absolute through
    // the whole function (FS stat, exclusion, ancestor/affected paths, the FE emit);
    // the mount-relative strip is applied ONLY at each `resolve_abs` argument. For
    // the boot disk this firmlink-normalizes; for a mount-rooted drive it's the raw
    // path (firmlink semantics don't apply under `/Volumes`).
    let normalized = space.absolute(&event.path);

    // Skip excluded paths, scoped by the volume kind: `BootDisk` keeps the `/`-rooted
    // boot disk off `/Volumes/`/system trees; `MountRooted` skips only junk basenames
    // so an external drive still indexes its own subtree.
    if scanner::should_exclude(&normalized, space.exclusion_scope()) {
        return None;
    }

    // Skip HistoryDone marker events
    if event.flags.history_done {
        return None;
    }

    let parent_path = compute_parent_path(&normalized);
    let mut affected = collect_ancestor_paths(&normalized);

    if event.flags.item_removed {
        return handle_removal(&normalized, space, conn, event, writer, affected, throttle, escalation);
    }

    if event.flags.item_created || event.flags.item_modified || event.flags.item_renamed {
        return handle_creation_or_modification(
            &normalized,
            &parent_path,
            space,
            conn,
            event,
            writer,
            &mut affected,
            throttle,
            escalation,
        );
    }

    // For other flag combinations (xattr, owner change, etc.), just stat and update
    if event.flags.item_is_file || event.flags.item_is_dir {
        return handle_creation_or_modification(
            &normalized,
            &parent_path,
            space,
            conn,
            event,
            writer,
            &mut affected,
            throttle,
            escalation,
        );
    }

    None
}

/// Handle a file/directory removal event.
///
/// FSEvents can deliver `item_removed` for paths that still exist on disk
/// (e.g., atomic file swaps, coalesced events with OR'd flags). To avoid
/// deleting live entries, we stat the path first: if it exists, delegate to
/// `handle_creation_or_modification` (which upserts). Only delete from the DB
/// when the path is truly gone from the filesystem.
#[allow(
    clippy::too_many_arguments,
    reason = "shares the event-processing param set; a struct would add indirection without clarity"
)]
fn handle_removal(
    normalized: &str,
    space: &IndexPathSpace,
    conn: &Connection,
    event: &FsChangeEvent,
    writer: &IndexWriter,
    mut affected: Vec<String>,
    throttle: Option<&mut LiveThrottle>,
    escalation: &mut Option<PathBuf>,
) -> Option<Vec<String>> {
    // Check if the path actually exists on disk before deleting from the DB.
    // `normalized` is the absolute FS path, so this stat is correct on any volume.
    if Path::new(normalized).symlink_metadata().is_ok() {
        // Path still exists, so treat as a modification, not a removal (throttled
        // like any other in-place rewrite). Deletes themselves are never throttled.
        let parent_path = compute_parent_path(normalized);
        return handle_creation_or_modification(
            normalized,
            &parent_path,
            space,
            conn,
            event,
            writer,
            &mut affected,
            throttle,
            escalation,
        );
    }

    // Path is truly gone; resolve (mount-strip for a mount-rooted drive) and delete.
    let entry_id = match space.resolve_abs(conn, normalized) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Per-event line at TRACE: useful when actively debugging reconciler/index
            // drift, but ~90% of normal log volume comes from build-output churn that's
            // genuinely harmless. The aggregate at DEBUG (below) gives the existence-of-
            // drift signal without flooding the file.
            log::trace!("Reconciler: removal for unknown path, skipping: {normalized}");
            skip_aggregator::UNKNOWN_PATH_SKIPS.record(normalized);
            return Some(affected);
        }
        Err(e) => {
            log::warn!("Reconciler: resolve_path failed for removal {normalized}: {e}");
            return Some(affected);
        }
    };

    if event.flags.item_is_dir {
        let _ = writer.send(WriteMessage::DeleteSubtreeById(entry_id));
    } else {
        let _ = writer.send(WriteMessage::DeleteEntryById(entry_id));
    }

    Some(affected)
}

/// Handle file/directory creation, modification, or rename.
///
/// Resolves the parent path to an integer ID and sends `UpsertEntryV2`.
/// For new entries (create), also sends `PropagateDeltaById` starting
/// from the parent so dir_stats are updated along the ancestor chain.
#[allow(
    clippy::too_many_arguments,
    reason = "shares the event-processing param set (path/parent/space/conn/event/writer/affected/throttle/escalation); a struct would add indirection without clarity, matching run_scan"
)]
fn handle_creation_or_modification(
    normalized: &str,
    parent_path: &str,
    space: &IndexPathSpace,
    conn: &Connection,
    event: &FsChangeEvent,
    writer: &IndexWriter,
    affected: &mut Vec<String>,
    throttle: Option<&mut LiveThrottle>,
    escalation: &mut Option<PathBuf>,
) -> Option<Vec<String>> {
    // Stat the file to get current metadata. `normalized` is the absolute FS path.
    let path = Path::new(normalized);
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            // Path doesn't exist (deleted since event was generated).
            // Treat as a removal: resolve to entry ID and send integer-keyed delete.
            // Use DeleteSubtreeById for directories to also remove child entries;
            // journal replay may coalesce child events into a parent dir event,
            // leaving orphaned children without a subtree delete.
            match space.resolve_abs(conn, normalized) {
                Ok(Some(id)) => {
                    if event.flags.item_is_dir {
                        let _ = writer.send(WriteMessage::DeleteSubtreeById(id));
                    } else {
                        let _ = writer.send(WriteMessage::DeleteEntryById(id));
                    }
                }
                Ok(None) => {
                    // Not in DB either -- nothing to do
                }
                Err(e) => {
                    log::warn!("Reconciler: resolve_path failed for gone path {normalized}: {e}");
                }
            }
            return Some(affected.clone());
        }
    };

    // Resolve parent path to integer ID (mount-strip for a mount-rooted drive).
    let parent_id = match space.resolve_abs(conn, parent_path) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Parent not in DB (Leak B): the intermediate dir chain is missing.
            // Instead of dropping the credit, escalate to a subtree rescan anchored
            // at the highest missing dir, so `reconcile_subtree` discovers and
            // credits the whole chain. The caller queues (live) or defers (replay).
            // Per-event at TRACE; a DEBUG aggregate every ~5 s keeps the drift
            // signal in error reports without the per-event flood.
            if let Some(anchor) = resolve_escalation_anchor(space, conn, normalized) {
                *escalation = Some(anchor);
            }
            log::trace!("Reconciler: parent path not in DB, escalating event for {normalized} (parent: {parent_path})");
            skip_aggregator::ESCALATED_MISSING_PARENTS.record(normalized);
            return Some(affected.clone());
        }
        Err(e) => {
            log::warn!("Reconciler: resolve_path failed for parent {parent_path}: {e}");
            return Some(affected.clone());
        }
    };

    let is_dir = metadata.is_dir();
    let is_symlink = metadata.is_symlink();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let snap = extract_metadata(&metadata, is_dir, is_symlink);
    // On a volume without stable inodes (FAT/exFAT) store `inode: None`, so a
    // reused inode can never let the live rename pre-pass false-match a move.
    let inode = space.trust_inode(snap.inode);

    // Live-path throttle: a regular file rewritten in place may be suppressed so
    // rapid rewrites collapse to ≤1 index write per THROTTLE_WINDOW. Only files
    // (dirs/symlinks carry no size), never on replay (`throttle` is None), and
    // never under the user's Downloads (active downloads want a live size). The
    // trailing flush that applies the suppressed size runs from the sweep tick.
    let is_regular_file = !is_dir && !is_symlink;
    let suppress = match throttle {
        Some(t) if is_regular_file && !t.is_exempt(normalized) => {
            let payload = PendingUpsert {
                parent_id,
                name: name.clone(),
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode,
                nlink: snap.nlink,
            };
            matches!(
                t.on_change(normalized, snap.logical_size.unwrap_or(0), payload, Instant::now()),
                ThrottleOutcome::Suppress
            )
        }
        _ => false,
    };

    if suppress {
        // Nothing written, so no dir_stats changed: don't notify ancestors. The
        // last-seen size is applied by the trailing-flush sweep.
        return Some(Vec::new());
    }

    let _ = writer.send(WriteMessage::UpsertEntryV2 {
        parent_id,
        name,
        is_directory: is_dir,
        is_symlink,
        logical_size: snap.logical_size,
        physical_size: snap.physical_size,
        modified_at: snap.modified_at,
        inode,
        nlink: snap.nlink,
    });

    // UpsertEntryV2 auto-propagates deltas in the writer, so no separate
    // PropagateDeltaById needed here.

    // For new directories, add them to affected paths for downstream processing
    if event.flags.item_created && is_dir {
        affected.push(normalized.to_string());
    }

    Some(affected.clone())
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Resolve the user's Downloads directory to a normalized prefix, so the live
/// throttle can exempt it (active downloads want a live size). Resolved once at
/// reconciler construction via the OS dir API, not a hardcoded string; `None`
/// when the OS reports no Downloads dir (rare). Normalized the same way live
/// event paths are, so the prefix comparison lines up. This is purely a
/// "don't throttle" flag — it reads no new metadata, so adds no TCC surface.
fn resolve_downloads_prefix() -> Option<String> {
    dirs::download_dir().map(|p| firmlinks::normalize_path(&p.to_string_lossy()))
}

/// Compute parent path from a normalized path.
fn compute_parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => String::new(),
    }
}

/// Collect all ancestor paths from the immediate parent up to "/".
/// Used to notify the frontend that dir_stats changed along the entire ancestor chain
/// (since propagate_delta updates all ancestors, not just the direct parent).
fn collect_ancestor_paths(path: &str) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut current = path.to_string();
    loop {
        let parent = compute_parent_path(&current);
        if parent.is_empty() || parent == current {
            break;
        }
        ancestors.push(parent.clone());
        if parent == "/" {
            break;
        }
        current = parent;
    }
    ancestors
}

/// Emit an `index-dir-updated` event to the frontend.
pub(super) fn emit_dir_updated(app: &AppHandle, paths: Vec<String>) {
    use tauri_specta::Event;
    let _ = crate::indexing::IndexDirUpdatedEvent { paths }.emit(app);
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

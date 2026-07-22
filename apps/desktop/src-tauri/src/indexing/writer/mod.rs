//! Single-writer thread for all SQLite index writes.
//!
//! All writes go through a dedicated `std::thread` that owns the write connection.
//! This eliminates contention between the full scan, subtree scans, and watcher updates.
//! Reads happen on separate connections (WAL mode allows concurrent reads).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::sync::oneshot;

#[cfg(test)]
use crate::ignore_poison::IgnorePoison;
use crate::indexing::IndexFailureSignal;
use crate::indexing::aggregator::AggregationPhase;
use crate::indexing::lifecycle::state::ROOT_VOLUME_ID;
use crate::indexing::store::{EntryRow, IndexStore, IndexStoreError};
use crate::pluralize::{pluralize, pluralize_with};

mod aggregation;
mod deferred_repair;
mod delta;
mod entries;
mod maintenance;
mod repair;
pub(crate) mod wait_probe;

use crate::indexing::read::pending_sizes;
use crate::indexing::reconcile::reconciler;
use aggregation::{
    handle_backfill_missing_dir_stats, handle_compute_all_aggregates, handle_compute_partial_aggregates,
    handle_compute_subtree_aggregates,
};
use deferred_repair::DeferredRepairs;
use delta::{propagate_delta_by_id, propagate_min_subtree_epoch};
use entries::{
    handle_delete_entry_by_id, handle_delete_subtree_by_id, handle_insert_entries_v2, handle_move_entry_v2,
    handle_truncate_data, handle_upsert_entry_v2,
};
use maintenance::{handle_incremental_vacuum, request_wal_checkpoint, run_deferred_wal_checkpoint};

// ── Aggregation progress events ──────────────────────────────────────

/// Tauri event payload for aggregation progress updates.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[tauri_specta(event_name = "index-aggregation-progress")]
#[serde(rename_all = "camelCase")]
pub struct AggregationProgressEvent {
    /// The volume whose writer is aggregating. The writer is spawned per volume,
    /// so this is known at spawn time and threaded down to every emit site. Lets
    /// the FE attribute aggregation progress to the right drive even when two
    /// volumes aggregate concurrently (no more `lastCompletedScanVolumeId` guess).
    pub volume_id: String,
    /// One of `phase_to_str`'s outputs: `saving_entries` | `loading` | `sorting` | `computing` | `writing`.
    pub phase: String,
    pub current: u64,
    pub total: u64,
}

pub(super) fn phase_to_str(phase: AggregationPhase) -> &'static str {
    match phase {
        AggregationPhase::SavingEntries => "saving_entries",
        AggregationPhase::LoadingDirectories => "loading",
        AggregationPhase::Sorting => "sorting",
        AggregationPhase::Computing => "computing",
        AggregationPhase::Writing => "writing",
    }
}

// ── Writer generation (for search index staleness detection) ─────────

/// Monotonically increasing generation counter, bumped on every mutation
/// (`InsertEntriesV2`, `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`,
/// `TruncateData`) of the **search-feeding** index only. The search index stores
/// the generation it was loaded at; a mismatch triggers a background reload.
/// Initialized to 1 (not 0) to avoid ambiguity with a freshly constructed
/// search index.
///
/// Search is single-volume by construction (D7): `search/index.rs` loads exactly
/// one in-memory `SearchIndex` off `root`'s DB. So ONLY `root`'s writer ticks this
/// — an SMB/MTP writer mutating its own DB must not invalidate the root search
/// index it doesn't feed (that would thrash a reload of the whole root index on
/// every NAS/phone change-notify event). The gate lives in
/// [`MutationTracker::bump`], the single point of policy; see `lifecycle/state.rs`'s
/// `feeds_search` plumbing and `indexing/DETAILS.md` § "Search stays single-volume".
pub(crate) static WRITER_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Per-writer mutation bookkeeping passed down through the handler functions.
///
/// Bundles the per-writer `counter` (test-only "did THIS writer mutate?" probe,
/// immune to other concurrent writers in the same test binary) with the
/// `feeds_search` flag that decides whether a mutation also bumps the global
/// [`WRITER_GENERATION`]. Only the search-feeding (root) writer sets
/// `feeds_search`, so a non-root mutation ticks just its own counter and leaves
/// the root search index's generation untouched.
pub(super) struct MutationTracker {
    counter: AtomicU64,
    feeds_search: bool,
    /// Per-writer count of `DeleteEntryById` messages processed. Test-only probe
    /// for the removal-storm coalescing tests, whose pre- and post-fix DB END
    /// states are identical (rows end up gone either way) — the behavior only
    /// shows at the MESSAGE level (per-file deletes vs one coalesced rescan). A
    /// per-writer counter (like [`counter`](Self::counter)) is immune to other
    /// concurrent writers in the same test binary; a global `DEBUG_STATS` counter
    /// would be racy under `cargo test`'s threaded execution.
    delete_entry_count: AtomicU64,
    /// Per-writer count of `DeleteSubtreeById` messages processed (see
    /// [`delete_entry_count`](Self::delete_entry_count)).
    delete_subtree_count: AtomicU64,
    /// Per-writer count of times THIS writer bumped the global [`WRITER_GENERATION`]
    /// (i.e. processed a search-feeding mutation). Test-only probe: the
    /// search-isolation tests assert on this instead of a before/after read of the
    /// process-global `WRITER_GENERATION`, which flakes under `cargo test` (every
    /// other spawned ROOT writer bumps the shared generation). See DETAILS §
    /// "Test isolation".
    #[cfg(test)]
    global_generation_bumps: AtomicU64,
    /// Test-only capture of every `EmitDirUpdated` message's paths, in send
    /// order. Lets the rescan completion-emit test assert the rescan-completion
    /// refresh rides the writer AFTER the reconcile's writes, carrying the root
    /// plus its ancestor chain — without a real `AppHandle` (the production emit
    /// is `AppHandle`-gated, a no-op in tests). Gated off entirely in release
    /// builds so the emit path stays allocation-free there.
    #[cfg(test)]
    emitted_paths: std::sync::Mutex<Vec<Vec<String>>>,
}

impl MutationTracker {
    pub(super) fn new(feeds_search: bool) -> Self {
        Self {
            counter: AtomicU64::new(0),
            feeds_search,
            delete_entry_count: AtomicU64::new(0),
            delete_subtree_count: AtomicU64::new(0),
            #[cfg(test)]
            global_generation_bumps: AtomicU64::new(0),
            #[cfg(test)]
            emitted_paths: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Record a mutation: always tick the per-writer counter; tick the global
    /// search generation only when this writer feeds the search index (root).
    #[inline]
    pub(super) fn bump(&self) {
        self.counter.fetch_add(1, Ordering::Relaxed);
        if self.feeds_search {
            WRITER_GENERATION.fetch_add(1, Ordering::Relaxed);
            #[cfg(test)]
            self.global_generation_bumps.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Tick the per-writer `DeleteEntryById` probe (test observability).
    #[inline]
    pub(super) fn record_delete_entry(&self) {
        self.delete_entry_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Tick the per-writer `DeleteSubtreeById` probe (test observability).
    #[inline]
    pub(super) fn record_delete_subtree(&self) {
        self.delete_subtree_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an `EmitDirUpdated` message's paths (test observability).
    #[cfg(test)]
    pub(super) fn record_emit(&self, paths: &[String]) {
        self.emitted_paths.lock_ignore_poison().push(paths.to_vec());
    }

    /// Every `EmitDirUpdated` message's paths, in send order (test-only).
    #[cfg(test)]
    pub(super) fn emitted_paths(&self) -> Vec<Vec<String>> {
        self.emitted_paths.lock_ignore_poison().clone()
    }

    /// The per-writer mutation count (test-only observable).
    #[cfg(test)]
    pub(super) fn count(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// How many times THIS writer bumped the global `WRITER_GENERATION` (see the
    /// [`global_generation_bumps`](Self::global_generation_bumps) field; test-only).
    #[cfg(test)]
    pub(super) fn global_generation_bumps(&self) -> u64 {
        self.global_generation_bumps.load(Ordering::Relaxed)
    }

    /// The per-writer `DeleteEntryById` / `DeleteSubtreeById` counts (test-only).
    #[cfg(test)]
    pub(super) fn delete_counts(&self) -> (u64, u64) {
        (
            self.delete_entry_count.load(Ordering::Relaxed),
            self.delete_subtree_count.load(Ordering::Relaxed),
        )
    }
}

// ── Messages ─────────────────────────────────────────────────────────

/// Capacity of the bounded writer channel. When full, senders block,
/// providing natural backpressure instead of unbounded memory growth.
const WRITER_CHANNEL_CAPACITY: usize = 20_000;

/// Which source a `Compute*Aggregates` pass computes its sizes from.
///
/// Carried on both `ComputeAllAggregates` and `ComputePartialAggregates` (NOT
/// sniffed from `propagate_deltas` or map emptiness) so the handler routes
/// deterministically — the key defense against Leak D, where a verification
/// subtree scan's `InsertEntriesV2` leaves the accumulator maps holding
/// subtree-only data and a `Maps`-sniffing full aggregate would then roll every
/// out-of-subtree dir up from zero.
///
/// - `Maps`: the writer's in-memory accumulator maps, populated only by
///   `InsertEntriesV2`. Correct for fresh guarded-walker scans (the full scan's
///   completion). On `ComputeAllAggregates`, empty maps fall back to the SQL path
///   (an explicitly-`Maps` sender whose maps got consumed must not treat "empty"
///   as "everything is zero"); on `ComputePartialAggregates`, empty maps are a
///   deliberate no-op (see that handler).
/// - `Sql`: the committed `entries` / `dir_stats` rows. Ignores the accumulator
///   entirely, so it works for ALL write paths (the guarded walker, `UpsertEntryV2`
///   reconcile, network) AND stays correct even when the maps are polluted by an
///   interleaving subtree scan. The reconcile finish and the one-shot heal send this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggSource {
    Maps,
    Sql,
}

/// Messages sent to the writer thread via a bounded mpsc channel.
pub enum WriteMessage {
    /// Full scan: batch of entries with pre-assigned integer IDs.
    InsertEntriesV2(Vec<EntryRow>),
    /// Watcher/reconciler: upsert a single entry by parent_id + name.
    /// The writer resolves or inserts using integer keys.
    UpsertEntryV2 {
        parent_id: i64,
        name: String,
        is_directory: bool,
        is_symlink: bool,
        logical_size: Option<u64>,
        physical_size: Option<u64>,
        modified_at: Option<u64>,
        inode: Option<u64>,
        nlink: Option<u64>,
    },
    /// Live event loop's rename pre-pass: move an existing entry to a new
    /// `(parent_id, name)`, preserving its `entry_id` (and therefore any
    /// `dir_stats` for directories). Detected by inode match against the
    /// post-rename path. Cross-parent moves propagate the entry's contribution
    /// down the old ancestor chain and up the new one. Same-parent renames
    /// don't change ancestor totals so no propagation is needed.
    MoveEntryV2 {
        entry_id: i64,
        new_parent_id: i64,
        new_name: String,
    },
    /// Watcher: delete a single entry and its dir_stats by entry ID.
    DeleteEntryById(i64),
    /// Watcher: delete a subtree (directory removed with all children) by entry ID.
    DeleteSubtreeById(i64),
    /// Scanner: delete all descendants of an entry before a subtree rescan.
    /// Prevents orphaned entries when re-scanning an already-indexed subtree.
    DeleteDescendantsById(i64),
    /// Watcher: incremental delta propagation walking the parent_id chain.
    PropagateDeltaById {
        entry_id: i64,
        logical_size_delta: i64,
        physical_size_delta: i64,
        file_count_delta: i32,
        dir_count_delta: i32,
    },
    /// Recompute `min_subtree_epoch` up the parent chain starting at `start_id`.
    /// Fired by the off-writer-thread fill paths (`reconcile_subtree`, the
    /// verifier's `scan_subtree`) AFTER they have marked the dirs they listed,
    /// so ancestors lift their coverage. The in-writer shape-change handlers
    /// (`UpsertEntryV2`/delete/move) call `propagate_min_subtree_epoch` directly
    /// and don't need this message. Coverage-only, so no generation bump.
    PropagateMinSubtreeEpoch(i64),
    /// Full scan / reconcile complete: trigger bottom-up aggregation for all
    /// directories.
    ///
    /// `source` selects where the sizes come from (see [`AggSource`]). `Maps`
    /// (fresh full-scan completions) uses the in-memory accumulator, falling back
    /// to SQL when the maps are empty; `Sql` (the reconcile finish, the one-shot
    /// heal) recomputes from committed rows and ignores the accumulator — the
    /// Leak-D defense, since a verification subtree scan can leave the maps holding
    /// subtree-only data. The subtree handler deliberately does NOT clear the
    /// accumulator, so declaring the source is what keeps a polluted map from
    /// poisoning this aggregate.
    ComputeAllAggregates { source: AggSource },
    /// Mid-scan: compute partial recursive sizes and write a bounded subset of
    /// dir_stats rows so visible listings can show growing sizes during the scan.
    ///
    /// `source` selects where the sizes come from (see [`AggSource`]):
    /// `Maps` borrows the in-memory accumulator maps read-only (MUST NOT clear or
    /// mutate them — the final ComputeAllAggregates depends on them; no SQL
    /// fallback on empty maps), `Sql` recomputes from committed rows scoped to the
    /// hot dirs (works on reconcile/network where the maps are empty).
    ComputePartialAggregates {
        /// Directories whose `dir_stats` should be written, because a pane is
        /// currently showing them ("hot" paths). Already firmlink-normalized by
        /// the sender; for the `Sql` source they're index-relative
        /// (volume-root-stripped).
        hot_paths: Vec<String>,
        /// Which source to compute from. `Maps` preserves today's behavior
        /// byte-for-byte; `Sql` is the unified path.
        source: AggSource,
    },
    /// Subtree scan complete: trigger aggregation for a subtree only, keyed by
    /// the subtree root's entry id. The id (not a path) is carried so a rename or
    /// delete landing between send and process can't make the aggregate — and the
    /// ancestor repair that follows it — silently no-op after the destructive
    /// `DeleteDescendantsById` already ran.
    ComputeSubtreeAggregates { root_id: i64 },
    /// Store the last processed FSEvents event ID.
    UpdateLastEventId(u64),
    /// Update a meta key.
    UpdateMeta { key: String, value: String },
    /// Delete a meta key (no-op if absent). Used at scan start to clear the
    /// previous `scan_completed_at` so a killed rescan heals to a fresh scan
    /// instead of replaying on top of a gutted index. Not search-relevant, so
    /// (like `UpdateMeta`) it does NOT bump the writer generation.
    DeleteMeta(String),
    /// Stamp the given directories' `listed_epoch` (their direct contents were
    /// successfully listed at `epoch`). The scanner ACCUMULATES the ids of every
    /// successfully-listed dir and sends this ONCE after the final
    /// `flush_batch` and BEFORE `ComputeAllAggregates`, so every entry row is
    /// already committed-in-order when the PK-keyed `UPDATE` runs (a per-dir
    /// emit could update a row still pending in an unflushed batch, leaving it
    /// `listed_epoch=0` forever). Like `UpdateMeta`/`DeleteMeta`, it does NOT
    /// bump the writer generation: it changes nothing search cares about, so it
    /// must not thrash a root-search reload each scan. See the "Honest sizes"
    /// model in `indexing/DETAILS.md`.
    MarkDirsListed { ids: Vec<i64>, epoch: u64 },
    /// Bump the volume's `current_epoch` by one and persist it (a continuity
    /// break: reconnect/rescan, watcher death, overflow, disconnect, or a
    /// launch-loading-Stale). A scan/reconcile only STAMPS `listed_epoch` with
    /// the value, never bumps. Routed through the writer to honor the
    /// single-writer-per-DB invariant. Like `UpdateMeta`/`MarkDirsListed`, it
    /// does NOT bump the writer generation: it touches only `meta`, nothing
    /// search indexes. At a scan-start funnel, the caller `flush`es after this so
    /// the bump is committed BEFORE the scan thread reads `current_epoch` on its
    /// own connection. See the "Honest sizes" epoch model in `indexing/DETAILS.md`.
    BumpCurrentEpoch,
    /// Request current entry count (for progress reporting).
    #[cfg(test)]
    GetEntryCount(oneshot::Sender<Result<u64, IndexStoreError>>),
    /// Flush: confirms all prior messages have been committed.
    /// The writer responds through the channel after processing this message.
    Flush(oneshot::Sender<()>),
    /// Truncate `entries` and `dir_stats` tables, preserving `meta`.
    /// Used before a full rescan so the scan starts from a clean slate.
    TruncateData,
    /// Toggle per-entry ancestor `dir_stats` propagation on the writer thread.
    ///
    /// The FULL reconcile (local `run_local_reconcile` + network
    /// `reconcile_volume_via_trait`) sets this `false` before its BFS walk so the
    /// thousands of `UpsertEntryV2` / `Delete*` it sends DON'T each walk the
    /// ancestor chain (`propagate_delta_by_id` / `propagate_min_subtree_epoch` /
    /// `propagate_recursive_has_symlinks`). That per-entry propagation is
    /// O(entries × tree-depth) and, on a large delta, wedges the writer for hours.
    /// It's also pure wasted work here: the reconcile's `finish_reconcile` runs
    /// ONE `ComputeAllAggregates` that recomputes EVERY dir's `dir_stats` from the
    /// entries table, overwriting whatever the per-entry propagation produced.
    ///
    /// The reconcile re-enables it (`true`) on EVERY exit path (success, cancel,
    /// empty-root, error) so the subsequent LIVE event loop — which has NO final
    /// aggregate and relies on per-entry propagation — works normally again.
    /// Default is `true`; only the full-reconcile brackets flip it, and the live
    /// `reconcile_subtree` / FSEvents path never touches it.
    ///
    /// Only the ancestor PROPAGATION is suppressed. Entry insert/update/delete,
    /// hardlink dedup, and the new-directory zero-valued `dir_stats` row init all
    /// still run (enrichment needs that row mid-walk; the final aggregate fills it).
    SetDeltaPropagation(bool),
    /// Begin an explicit SQLite transaction.
    /// All subsequent writes are batched until `CommitTransaction`.
    /// Dramatically reduces fsync overhead for bulk operations (replay).
    BeginTransaction,
    /// Commit the current explicit transaction.
    CommitTransaction,
    /// Backfill dir_stats for directories that have entries but no stats row.
    /// Happens after reconciler replay or cold-start replay to catch dirs
    /// created by events that ran after the last full aggregation.
    BackfillMissingDirStats,
    /// Arm the one-shot `dir_stats` ledger-heal latch. Sent once at launch (from
    /// the `resume_or_scan` heal decision) for a DB whose `LEDGER_HEAL_KEY` is
    /// absent. The NEXT successful `ComputeAllAggregates` on this writer — from
    /// whichever flow runs it — then persists the key and disarms the latch, so
    /// every existing install self-heals its drifted aggregates exactly once. A
    /// failed aggregate leaves the latch armed (re-heals next launch). Like
    /// `SetDeltaPropagation`, a control message: no `MutationTracker::bump()`.
    ArmLedgerHealLatch,
    /// Declare the `dir_stats` ledger UNPAID for the duration of a bulk walk:
    /// clear `LEDGER_HEAL_KEY` on disk and arm the heal latch. Sent by
    /// `reconciler::BulkReconcileGuard::begin`, BEFORE any suppressed write, so
    /// the debt is recorded durably from the first one.
    ///
    /// Under suppression every `UpsertEntryV2` / `Delete*` skips its ancestor
    /// walk, and the ONLY thing that makes those ancestors true again is the
    /// walk's terminal `ComputeAllAggregates`. A walk that never gets there
    /// (process death, cancel, error) would otherwise leave ancestors silently
    /// claiming exact sizes over a descendant at `listed_epoch = 0`. The cleared
    /// marker makes the next launch heal; the armed latch lets the terminal
    /// aggregate (or `PayLedgerIfUnpaid`) re-mark it paid in this session.
    MarkLedgerUnpaid,
    /// Pay the ledger IF it's still unpaid: run a full `ComputeAllAggregates`
    /// when the heal latch is armed, and no-op when it isn't. Sent by
    /// `BulkReconcileGuard`'s `Drop`, so an in-process interruption heals right
    /// away instead of waiting for the next launch, while a walk that already
    /// finished (its aggregate disarmed the latch) pays nothing twice.
    PayLedgerIfUnpaid,
    /// Periodic housekeeping: reclaim free pages from deletes/rescans.
    /// Sent by a background timer, not counted in WriterStats.
    IncrementalVacuum,
    /// Periodic housekeeping: TRUNCATE the WAL file once readers permit, so the
    /// post-scan high-water mark doesn't sit on disk indefinitely. Sent by the
    /// same background timer as `IncrementalVacuum`, and also fired explicitly
    /// after a full scan's `ComputeAllAggregates` so the scan-time spike doesn't
    /// wait up to 30 s before being trimmed. Not counted in WriterStats.
    WalCheckpoint,
    /// Emit `index-dir-updated` for the given paths. Enqueued after a batch
    /// of writes so the UI notification fires only after all prior messages
    /// (deletes, upserts, deltas) are committed.
    EmitDirUpdated(Vec<String>),
    /// Shut down the writer thread.
    Shutdown,
}

// ── IndexWriter handle ───────────────────────────────────────────────

/// Handle for sending messages to the writer thread.
///
/// Cloneable; all clones share the same underlying channel.
#[derive(Clone)]
pub struct IndexWriter {
    sender: mpsc::SyncSender<WriteMessage>,
    /// Handle for the writer thread, shared so shutdown() can join it.
    thread_handle: Arc<std::sync::Mutex<Option<thread::JoinHandle<()>>>>,
    /// Path to the database file (needed by scanner for ScanContext init).
    db_path: PathBuf,
    /// Expected total entries from the scan, set by the caller when the scan
    /// completes. The writer thread reads this to report flushing progress as
    /// it processes remaining `InsertEntriesV2` batches.
    expected_total_entries: Arc<AtomicU64>,
    /// Shared ID counter for entry allocation. The scanner atomically increments
    /// this to get unique IDs, and the writer bumps it after `UpsertEntryV2` inserts
    /// (which let SQLite auto-assign). Reset to 2 on `TruncateData`.
    next_id: Arc<AtomicI64>,
    /// Per-writer mutation bookkeeping (`counter` + `feeds_search`). The counter
    /// ticks on every mutating message; tests use it instead of the global so an
    /// assertion of "did this writer mutate?" isn't disturbed by other concurrent
    /// writers (cargo test runs tests as threads in one process). `feeds_search`
    /// gates whether a mutation also bumps the global `WRITER_GENERATION` (only the
    /// root, search-feeding writer does). Production code reads `WRITER_GENERATION`;
    /// this handle is read only by the test-only `mutation_count`, but the writer
    /// thread holds its own `Arc` clone, so the bookkeeping is live either way.
    #[cfg_attr(not(test), allow(dead_code, reason = "test-only observable"))]
    mutation_tracker: Arc<MutationTracker>,
    /// Phase 1 instrumentation: best-effort estimate of channel depth.
    /// Incremented on each `send()`; the writer thread decrements it after each `recv()`.
    /// Used by the heartbeat (writer thread) to log queue pressure.
    queue_depth: Arc<AtomicUsize>,
    /// The per-volume fatal-storage-failure signal. The writer thread trips this on
    /// the first fatal DB error (`SQLITE_IOERR` / corruption / full / read-only) and
    /// then exits, rather than logging and retrying forever. Shared with the live
    /// event loop (which polls it to stop) and the supervisor (which awaits it to
    /// fail the volume). See `indexing::failure`.
    failure_signal: Arc<IndexFailureSignal>,
}

impl IndexWriter {
    /// Spawn the writer thread with its own write connection.
    ///
    /// Opens a WAL-mode write connection to the DB at `db_path`, spawns a
    /// `std::thread` (blocking I/O, not tokio), and returns a handle.
    /// If `app_handle` is provided, the writer emits `index-aggregation-progress`
    /// events during `ComputeAllAggregates`.
    ///
    /// `feeds_search` is `true` only for the volume whose DB backs the in-memory
    /// search index (root): its mutations bump the global `WRITER_GENERATION` so
    /// search reloads. A non-root (SMB/MTP) writer passes `false`, so its writes
    /// never invalidate the root search index it doesn't feed. Tests that don't
    /// care about search isolation use [`spawn`](Self::spawn) (defaults to
    /// search-feeding, preserving prior behavior).
    pub fn spawn(db_path: &Path, app_handle: Option<AppHandle>) -> Result<Self, IndexStoreError> {
        Self::spawn_for(db_path, app_handle, true, ROOT_VOLUME_ID.to_string())
    }

    /// Spawn a writer, explicitly choosing whether it feeds the search index.
    /// See [`spawn`](Self::spawn) for the `feeds_search` contract.
    ///
    /// `volume_id` is stamped onto every `index-aggregation-progress` event this
    /// writer emits, so the FE can attribute aggregation to the right drive.
    pub fn spawn_for(
        db_path: &Path,
        app_handle: Option<AppHandle>,
        feeds_search: bool,
        volume_id: String,
    ) -> Result<Self, IndexStoreError> {
        let conn = IndexStore::open_write_connection(db_path)?;
        // SQLite busy retry accounting. The handler only tallies the attempt; the ONE
        // summary line ("writer waited 340 ms over 27 attempts for the write lock") is
        // emitted when the episode closes, from `flush_busy_episode` in the writer loop.
        // A per-attempt line here is a log flood by construction, and the varying
        // `attempt=` payload defeats the log writer's identical-line coalescing.
        conn.busy_handler(Some(|attempt: i32| {
            maintenance::note_busy_attempt(attempt);
            // Same back-off behaviour as default busy timeout (sleep up to ~250ms).
            if attempt > maintenance::BUSY_GIVE_UP_ATTEMPT {
                false
            } else {
                thread::sleep(Duration::from_millis(5));
                true
            }
        }))?;

        let initial_next_id = IndexStore::get_next_id(&conn)?;
        // This read ran on the SPAWNING thread with the handler already installed, so
        // close any episode it opened here — the writer loop's flush only ever sees its
        // own thread's.
        maintenance::flush_busy_episode();
        let (sender, receiver) = mpsc::sync_channel::<WriteMessage>(WRITER_CHANNEL_CAPACITY);
        let expected_total_entries = Arc::new(AtomicU64::new(0));
        let expected_total_clone = Arc::clone(&expected_total_entries);
        let next_id = Arc::new(AtomicI64::new(initial_next_id));
        let next_id_clone = Arc::clone(&next_id);
        let mutation_tracker = Arc::new(MutationTracker::new(feeds_search));
        let mutation_tracker_clone = Arc::clone(&mutation_tracker);
        let queue_depth = Arc::new(AtomicUsize::new(0));
        let queue_depth_clone = Arc::clone(&queue_depth);
        let failure_signal = Arc::new(IndexFailureSignal::new());
        let failure_signal_clone = Arc::clone(&failure_signal);

        let handle = thread::Builder::new()
            .name("index-writer".into())
            .spawn(move || {
                // Yield CPU to the UI: this thread writes the index DB in the background.
                crate::thread_qos::set_current_thread_qos(crate::thread_qos::QosClass::Utility);
                writer_loop(
                    conn,
                    receiver,
                    app_handle,
                    volume_id,
                    expected_total_clone,
                    next_id_clone,
                    mutation_tracker_clone,
                    queue_depth_clone,
                    failure_signal_clone,
                )
            })
            .map_err(IndexStoreError::Io)?;

        Ok(Self {
            sender,
            thread_handle: Arc::new(std::sync::Mutex::new(Some(handle))),
            db_path: db_path.to_path_buf(),
            expected_total_entries,
            next_id,
            mutation_tracker,
            queue_depth,
            failure_signal,
        })
    }

    /// The per-volume fatal-storage-failure signal. The manager clones this to hand
    /// to the live event loop (which polls it to stop) and to the supervisor task
    /// (which awaits it to fail the volume). Trips exactly once on the first fatal
    /// DB error; see `indexing::failure`.
    pub(crate) fn failure_signal(&self) -> Arc<IndexFailureSignal> {
        Arc::clone(&self.failure_signal)
    }

    /// Return the path to the DB file. Used by the scanner to open a
    /// temporary connection for `ScanContext` initialization.
    pub fn db_path(&self) -> PathBuf {
        self.db_path.clone()
    }

    /// Shared ID counter for entry allocation. The scanner uses this to
    /// allocate unique IDs without reading from the DB (which can be stale).
    pub fn next_id(&self) -> &Arc<AtomicI64> {
        &self.next_id
    }

    /// Set the expected total entries from a completed scan. The writer thread
    /// reads this to report flushing progress as it drains `InsertEntriesV2`.
    pub fn set_expected_total_entries(&self, total: u64) {
        self.expected_total_entries.store(total, Ordering::Relaxed);
    }

    /// Per-writer mutation counter. Bumped alongside the global `WRITER_GENERATION`
    /// every time the writer thread processes a mutating message. Tests rely on
    /// this to assert "did THIS writer mutate?" without flaking under concurrent
    /// other-writer activity in the same test binary.
    #[cfg(test)]
    pub(crate) fn mutation_count(&self) -> u64 {
        self.mutation_tracker.count()
    }

    /// How many times THIS writer bumped the global `WRITER_GENERATION` (search-feeding
    /// mutations). Per-writer, so it's immune to concurrent other-writer bumps — see
    /// the `MutationTracker::global_generation_bumps` field.
    #[cfg(test)]
    pub(crate) fn global_generation_bumps(&self) -> u64 {
        self.mutation_tracker.global_generation_bumps()
    }

    /// Per-writer `(DeleteEntryById, DeleteSubtreeById)` message counts. The
    /// removal-storm coalescing tests assert the MESSAGE shape (one coalesced
    /// rescan vs N per-file deletes) because the DB END state is identical either
    /// way. Per-writer, so it's immune to concurrent writers in the same binary.
    #[cfg(test)]
    pub(crate) fn delete_counts(&self) -> (u64, u64) {
        self.mutation_tracker.delete_counts()
    }

    /// Every `EmitDirUpdated` message's paths, in send order. The rescan
    /// completion-emit test asserts the rescan-completion refresh rides the
    /// writer after the reconcile's writes with the root + its ancestor chain.
    #[cfg(test)]
    pub(crate) fn emitted_paths(&self) -> Vec<Vec<String>> {
        self.mutation_tracker.emitted_paths()
    }

    /// Send a message to the writer thread. Blocks if the channel is full
    /// (backpressure), which slows down event processing rather than
    /// consuming unlimited memory. Any time spent parked is recorded in
    /// [`wait_probe`], so a caller timing its own work can attribute it.
    pub fn send(&self, msg: WriteMessage) -> Result<(), IndexStoreError> {
        send_blocking_with_depth(&self.sender, &self.queue_depth, msg)
    }

    /// Best-effort estimate of the writer channel depth: messages sent but not
    /// yet processed. Read by the scan progress loop to skip partial-aggregation
    /// passes while the writer is catching up on an insert backlog.
    pub fn queue_depth(&self) -> usize {
        self.queue_depth.load(Ordering::Relaxed)
    }

    /// Non-blocking send. Unlike `send`, never parks the caller when the channel
    /// is full — the message is dropped and `Ok(false)` is returned. This is what
    /// lets the partial-aggregation sender live on a tokio task without risking a
    /// parked worker: a full channel means the writer is busy with the real scan
    /// work, and a dropped partial pass is harmless (the next tick retries).
    ///
    /// Returns:
    /// - `Ok(true)`  — message enqueued.
    /// - `Ok(false)` — channel full, message dropped (not an error).
    /// - `Err(..)`   — writer thread gone (channel disconnected).
    pub fn try_send(&self, msg: WriteMessage) -> Result<bool, IndexStoreError> {
        try_send_with_depth(&self.sender, &self.queue_depth, msg)
    }

    /// Send a `Flush` and await the response, confirming all prior messages have been committed.
    pub async fn flush(&self) -> Result<(), IndexStoreError> {
        let (tx, rx) = oneshot::channel();
        self.send(WriteMessage::Flush(tx))?;
        rx.await.map_err(|_| {
            IndexStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Writer thread dropped flush reply",
            ))
        })
    }

    /// Send a `Flush` and block until all prior messages have been committed.
    /// Safe to call from synchronous code (no async runtime needed). The wait for
    /// the writer to catch up is recorded in [`wait_probe`], same as a parked send:
    /// from the caller's side both are time spent waiting on the writer queue.
    pub fn flush_blocking(&self) -> Result<(), IndexStoreError> {
        let (tx, rx) = oneshot::channel();
        self.send(WriteMessage::Flush(tx))?;
        let parked = Instant::now();
        let result = rx.blocking_recv();
        wait_probe::note(parked.elapsed());
        result.map_err(|_| {
            IndexStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Writer thread dropped flush reply",
            ))
        })
    }

    /// Send a `Shutdown` message and wait for the writer thread to finish.
    ///
    /// Joins the thread to ensure all buffered writes are flushed.
    /// After this call further sends will fail.
    pub fn shutdown(&self) {
        let _ = self.sender.send(WriteMessage::Shutdown);
        if let Ok(mut guard) = self.thread_handle.lock()
            && let Some(handle) = guard.take()
            && let Err(e) = handle.join()
        {
            log::warn!("Index writer thread panicked on shutdown: {e:?}");
        }
    }
}

/// Blocking send with depth accounting and writer-wait attribution. A free
/// function over the raw channel + atomic (same reason as `try_send_with_depth`):
/// the parking behaviour is testable against a bare `sync_channel`, without a
/// draining writer thread.
///
/// The non-blocking attempt comes first so the common case costs nothing to
/// measure: only a FULL channel parks the caller, and only that wait is timed and
/// recorded. On `Full` the message comes back to us, so nothing is lost and the
/// thread's own send order is preserved.
fn send_blocking_with_depth(
    sender: &mpsc::SyncSender<WriteMessage>,
    queue_depth: &AtomicUsize,
    msg: WriteMessage,
) -> Result<(), IndexStoreError> {
    fn gone() -> IndexStoreError {
        IndexStoreError::Io(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "Writer thread has shut down",
        ))
    }

    // Phase 1 instrumentation: track best-effort channel depth.
    queue_depth.fetch_add(1, Ordering::Relaxed);
    let msg = match sender.try_send(msg) {
        Ok(()) => return Ok(()),
        Err(mpsc::TrySendError::Full(msg)) => msg,
        Err(mpsc::TrySendError::Disconnected(_)) => {
            // Send failed. Undo the depth bump so the heartbeat doesn't drift.
            queue_depth.fetch_sub(1, Ordering::Relaxed);
            return Err(gone());
        }
    };

    let parked = Instant::now();
    let result = sender.send(msg);
    wait_probe::note(parked.elapsed());
    result.map_err(|_| {
        queue_depth.fetch_sub(1, Ordering::Relaxed);
        gone()
    })
}

/// Bump the depth counter, attempt a non-blocking `try_send`, and undo the bump
/// on any failure. Extracted as a free function (taking the raw channel + atomic)
/// so the bump/undo accounting can be tested against a bare `sync_channel`
/// without standing up a draining writer thread.
///
/// The undo on **both** `Full` and `Disconnected` is load-bearing: `queue_depth`
/// is only ever incremented on a successful enqueue, so a failed `try_send` that
/// left the bump in place would drift the depth upward forever — breaking both
/// the `PARTIAL_AGG_MAX_QUEUE_DEPTH` backpressure skip and the `queue_depth == 0`
/// pending-sizes wholesale clear in `writer_loop`. This mirrors `send`'s
/// undo-on-error pattern.
fn try_send_with_depth(
    sender: &mpsc::SyncSender<WriteMessage>,
    queue_depth: &AtomicUsize,
    msg: WriteMessage,
) -> Result<bool, IndexStoreError> {
    queue_depth.fetch_add(1, Ordering::Relaxed);
    match sender.try_send(msg) {
        Ok(()) => Ok(true),
        Err(mpsc::TrySendError::Full(_)) => {
            queue_depth.fetch_sub(1, Ordering::Relaxed);
            Ok(false)
        }
        Err(mpsc::TrySendError::Disconnected(_)) => {
            queue_depth.fetch_sub(1, Ordering::Relaxed);
            Err(IndexStoreError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Writer thread has shut down",
            )))
        }
    }
}

// ── Writer thread loop ───────────────────────────────────────────────

/// Snapshot of cumulative counters, used to compute per-interval deltas.
#[derive(Clone, Default)]
struct StatsSnapshot {
    total: u64,
    insert_entries: u64,
    upsert_entry: u64,
    move_entry: u64,
    delete_entry: u64,
    delete_subtree: u64,
    propagate_delta: u64,
    compute_aggregates: u64,
    compute_partial: u64,
    flush: u64,
    other: u64,
}

/// Diagnostic counters for writer thread logging.
struct WriterStats {
    current: StatsSnapshot,
    previous: StatsSnapshot,
    last_summary: Instant,
}

impl WriterStats {
    fn new() -> Self {
        Self {
            current: StatsSnapshot::default(),
            previous: StatsSnapshot::default(),
            last_summary: Instant::now(),
        }
    }

    fn record(&mut self, msg: &WriteMessage) {
        self.current.total += 1;
        match msg {
            WriteMessage::InsertEntriesV2(_) => self.current.insert_entries += 1,
            WriteMessage::UpsertEntryV2 { .. } => self.current.upsert_entry += 1,
            WriteMessage::MoveEntryV2 { .. } => self.current.move_entry += 1,
            WriteMessage::DeleteEntryById(_) => self.current.delete_entry += 1,
            WriteMessage::DeleteSubtreeById(_) | WriteMessage::DeleteDescendantsById(_) => {
                self.current.delete_subtree += 1;
            }
            WriteMessage::PropagateDeltaById { .. } | WriteMessage::PropagateMinSubtreeEpoch(_) => {
                self.current.propagate_delta += 1;
            }
            WriteMessage::ComputeAllAggregates { .. } | WriteMessage::ComputeSubtreeAggregates { .. } => {
                self.current.compute_aggregates += 1;
            }
            WriteMessage::ComputePartialAggregates { .. } => self.current.compute_partial += 1,
            WriteMessage::Flush(_) => self.current.flush += 1,
            _ => self.current.other += 1,
        }
    }

    /// Log a summary if at least 5 seconds have passed since the last one.
    ///
    /// Shows per-interval deltas as the primary info, with cumulative total in brackets.
    /// Only non-zero delta categories are included to keep the message concise.
    fn maybe_log_summary(&mut self) {
        let elapsed = self.last_summary.elapsed();
        if elapsed.as_secs() < 5 || self.current.total == 0 {
            return;
        }

        let delta_total = self.current.total - self.previous.total;
        if delta_total == 0 {
            self.last_summary = Instant::now();
            return;
        }

        // (singular, plural, count). Pluralizing per row keeps the "+1 insert"
        // / "+5 inserts" form natural; baking `+s` everywhere reads as "+1 inserts".
        let deltas: &[(&str, &str, u64)] = &[
            (
                "insert",
                "inserts",
                self.current.insert_entries - self.previous.insert_entries,
            ),
            (
                "upsert",
                "upserts",
                self.current.upsert_entry - self.previous.upsert_entry,
            ),
            ("move", "moves", self.current.move_entry - self.previous.move_entry),
            (
                "delete",
                "deletes",
                self.current.delete_entry - self.previous.delete_entry,
            ),
            (
                "delete_subtree",
                "delete_subtrees",
                self.current.delete_subtree - self.previous.delete_subtree,
            ),
            (
                "propagation",
                "propagations",
                self.current.propagate_delta - self.previous.propagate_delta,
            ),
            (
                "aggregate",
                "aggregates",
                self.current.compute_aggregates - self.previous.compute_aggregates,
            ),
            (
                "partial aggregate",
                "partial aggregates",
                self.current.compute_partial - self.previous.compute_partial,
            ),
            ("flush", "flushes", self.current.flush - self.previous.flush),
            ("other", "others", self.current.other - self.previous.other),
        ];

        let parts: Vec<String> = deltas
            .iter()
            .filter(|(_, _, count)| *count > 0)
            .map(|(singular, plural, count)| pluralize_with(*count, singular, plural))
            .collect();

        let breakdown = if parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", parts.join(", "))
        };

        log::debug!(
            "Writer: +{}{breakdown} in {:.1}s [{} total]",
            pluralize(delta_total, "msg"),
            elapsed.as_secs_f64(),
            self.current.total,
        );

        self.previous = self.current.clone();
        self.last_summary = Instant::now();
    }
}

/// In-memory accumulation of direct children stats, built during InsertEntriesV2.
///
/// Eliminates the two expensive full-table-scan SQL queries in the aggregator
/// (`bulk_get_children_stats_by_id` and `bulk_get_child_dir_ids`) by tracking
/// the same information incrementally as entries are inserted.
pub(super) struct AccumulatorMaps {
    /// `parent_id -> (logical_size_sum, physical_size_sum, file_count, dir_count,
    /// has_symlinks_direct)`: direct children only. `has_symlinks_direct` is `true` if any
    /// direct child of `parent_id` is a symlink.
    pub(super) direct_stats: HashMap<i64, (u64, u64, u64, u64, bool)>,
    /// `parent_id -> Vec<child_dir_id>`: direct child directories only.
    pub(super) child_dirs: HashMap<i64, Vec<i64>>,
    /// Running count of entries inserted so far (for flushing progress).
    pub(super) entries_inserted: u64,
    /// Running count of rows the scan skipped on a UNIQUE `(parent_id,
    /// name_folded)` conflict (`INSERT OR IGNORE`). Summarized once per scan at
    /// `ComputeAllAggregates`; see `classify_skip_severity`.
    pub(super) entries_skipped: u64,
}

impl AccumulatorMaps {
    pub(super) fn new() -> Self {
        Self {
            direct_stats: HashMap::new(),
            child_dirs: HashMap::new(),
            entries_inserted: 0,
            entries_skipped: 0,
        }
    }

    /// Accumulate stats from a set of inserted entries. Accepts any iterator
    /// of `&EntryRow` so callers can pre-filter (skipping rows that lost a
    /// UNIQUE conflict during `INSERT OR IGNORE`) without an extra clone.
    pub(super) fn accumulate<'a>(&mut self, entries: impl IntoIterator<Item = &'a EntryRow>) {
        for entry in entries {
            self.entries_inserted += 1;
            let stats = self.direct_stats.entry(entry.parent_id).or_insert((0, 0, 0, 0, false));
            if entry.is_symlink {
                stats.4 = true;
            }
            if entry.is_directory {
                stats.3 += 1;
                self.child_dirs.entry(entry.parent_id).or_default().push(entry.id);
            } else {
                stats.0 += entry.logical_size.unwrap_or(0);
                stats.1 += entry.physical_size.unwrap_or(0);
                stats.2 += 1;
            }
        }
    }

    pub(super) fn clear(&mut self) {
        self.direct_stats.clear();
        self.child_dirs.clear();
        self.entries_inserted = 0;
        self.entries_skipped = 0;
    }
}

/// Main loop for the writer thread.
///
/// Processes messages sequentially from the mpsc channel. Each message is
/// handled in order, ensuring all writes are serialized. Maintains in-memory
/// accumulator maps during InsertEntriesV2 to skip expensive SQL queries
/// when ComputeAllAggregates arrives.
#[allow(clippy::too_many_arguments, reason = "writer-loop ambient state")]
fn writer_loop(
    conn: rusqlite::Connection,
    receiver: mpsc::Receiver<WriteMessage>,
    app_handle: Option<AppHandle>,
    volume_id: String,
    expected_total_entries: Arc<AtomicU64>,
    next_id: Arc<AtomicI64>,
    mutation_tracker: Arc<MutationTracker>,
    queue_depth: Arc<AtomicUsize>,
    failure_signal: Arc<IndexFailureSignal>,
) {
    log::debug!("Writer: thread started");
    let mut stats = WriterStats::new();
    let mut accumulator = AccumulatorMaps::new();
    // Whether per-entry mutations propagate size/count/coverage deltas up the
    // ancestor `dir_stats` chain. Default `true` (the live path needs it); the
    // FULL reconcile flips it off around its bulk walk via `SetDeltaPropagation`.
    let mut propagate_deltas = true;
    // One-shot ledger-heal latch. Armed by `ArmLedgerHealLatch` (once at launch
    // when this DB has never healed), consumed by the first SUCCESSFUL
    // `ComputeAllAggregates` — which then persists `LEDGER_HEAL_KEY`. Default
    // `false`: a DB that already healed never re-arms, so routine aggregates
    // don't rewrite the key.
    let mut heal_latch = false;
    // A maintenance tick that landed inside an open batch transaction, waiting for
    // the commit to run its checkpoint. See `maintenance::request_wal_checkpoint`.
    let mut deferred_checkpoint = false;
    // Chains a failed `dir_stats` read/write left drifted, drained below once the
    // writer is idle again. See `deferred_repair.rs`.
    let repairs = DeferredRepairs::new();

    // Phase 1 instrumentation: time split between recv() (idle waiting),
    // processing (handlers), and commit (txn commits, tracked via wrapper).
    let mut probe = ProbeStats::new();
    // Heartbeat cadence
    const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

    loop {
        let recv_start = Instant::now();
        // Use recv_timeout so we can emit heartbeats even when the channel
        // is idle (the 163s smoking gun should make this visible).
        let recv_result = receiver.recv_timeout(HEARTBEAT_INTERVAL);
        let recv_elapsed = recv_start.elapsed();
        probe.time_in_recv += recv_elapsed;

        let msg = match recv_result {
            Ok(m) => {
                // Decrement queue depth: the message has left the channel.
                queue_depth.fetch_sub(1, Ordering::Relaxed);
                m
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // No message in this window. Emit heartbeat and loop.
                probe.maybe_emit_heartbeat(queue_depth.load(Ordering::Relaxed));
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        if !matches!(msg, WriteMessage::IncrementalVacuum | WriteMessage::WalCheckpoint) {
            stats.record(&msg);
        }

        let proc_start = Instant::now();
        // macOS: drain autoreleased ObjC objects each iteration.
        #[cfg(target_os = "macos")]
        let should_exit = objc2::rc::autoreleasepool(|_| {
            process_message(
                &conn,
                msg,
                &stats,
                &mut accumulator,
                &app_handle,
                &volume_id,
                &expected_total_entries,
                &next_id,
                &mutation_tracker,
                &mut probe,
                &mut propagate_deltas,
                &mut heal_latch,
                &mut deferred_checkpoint,
                &repairs,
                &failure_signal,
            )
        });
        #[cfg(not(target_os = "macos"))]
        let should_exit = process_message(
            &conn,
            msg,
            &stats,
            &mut accumulator,
            &app_handle,
            &volume_id,
            &expected_total_entries,
            &next_id,
            &mutation_tracker,
            &mut probe,
            &mut propagate_deltas,
            &mut heal_latch,
            &mut deferred_checkpoint,
            &repairs,
            &failure_signal,
        );
        probe.time_in_processing += proc_start.elapsed();
        probe.messages_processed += 1;
        // The busy handler has no "you got the lock" callback, so a contention
        // episode is closed here, once the message that contended is done. One
        // summary line per episode instead of one per retry.
        maintenance::flush_busy_episode();

        if should_exit {
            log::debug!("Writer: shutdown after processing {} messages", stats.current.total);
            return;
        }

        // A fatal storage error (dead disk, corruption, full/read-only volume) means
        // the DB is unusable and every later write fails identically. Stop the writer
        // thread instead of logging-and-retrying forever (the 12,700-warning
        // livelock). The supervisor, woken by the same signal, transitions the volume
        // to the `Failed` phase and tears the rest down. See `indexing::failure`.
        if failure_signal.is_tripped() {
            log::warn!(
                "Writer: stopping for '{volume_id}' after a fatal storage error ({} messages processed)",
                stats.current.total,
            );
            return;
        }
        stats.maybe_log_summary();
        probe.maybe_emit_heartbeat(queue_depth.load(Ordering::Relaxed));

        // Pending-size hourglass: once the writer has fully caught up (no more
        // queued work), every directory's `dir_stats` reflects all known
        // changes, so the transient "size updating" marks are correct to clear
        // wholesale (held rescan roots survive — see `pending_sizes::clear`).
        // Done here (end of iteration, after the message's DB effect is applied)
        // rather than at recv time — at recv the depth hits 0 *before* the
        // delete/propagate runs, which would briefly show a settled flag against
        // a not-yet-updated size. Route to THIS volume's tracker: a root-only
        // `get_pending_sizes()` from a non-root writer would wipe root's hourglass
        // and never clear its own. See `indexing/read/pending_sizes.rs`.
        if queue_depth.load(Ordering::Relaxed) == 0
            && let Some(tracker) = pending_sizes::get_pending_sizes_for(&volume_id)
        {
            tracker.clear();
        }

        // Drain deferred `dir_stats` repairs at the same caught-up point, and for
        // the same reason: with nothing queued behind us every committed row is
        // final, so a recompute-from-children sees the whole truth, and whatever
        // contention failed the original write (a checkpoint, a long reader) has
        // had its chance to clear. `is_autocommit()` keeps the drain out of an
        // open `BeginTransaction` batch, where the tree is only half written: a
        // repair there would roll ancestors up from a partial state and then
        // dequeue the id, baking that half-state in.
        if queue_depth.load(Ordering::Relaxed) == 0 && conn.is_autocommit() && !repairs.is_empty() {
            repairs.drain(&conn);
        }
    }

    log::debug!(
        "Writer: channel closed, thread exiting after processing {} messages",
        stats.current.total,
    );
}

/// Phase 1 instrumentation: rolling diagnostics for the writer thread.
struct ProbeStats {
    last_heartbeat: Instant,
    time_in_recv: Duration,
    time_in_processing: Duration,
    time_in_commit: Duration,
    messages_processed: u64,
    transaction_commits: u64,
}

impl ProbeStats {
    fn new() -> Self {
        Self {
            last_heartbeat: Instant::now(),
            time_in_recv: Duration::ZERO,
            time_in_processing: Duration::ZERO,
            time_in_commit: Duration::ZERO,
            messages_processed: 0,
            transaction_commits: 0,
        }
    }

    fn maybe_emit_heartbeat(&mut self, queue_depth: usize) {
        if self.last_heartbeat.elapsed() < Duration::from_secs(5) {
            return;
        }
        log::debug!(
            target: "stall_probe::writer",
            "heartbeat queue_depth={} messages_processed_since_last_heartbeat={} transaction_commits_since_last_heartbeat={} time_in_recv_ms={} time_in_processing_ms={} time_in_commit_ms={}",
            queue_depth,
            self.messages_processed,
            self.transaction_commits,
            self.time_in_recv.as_millis(),
            self.time_in_processing.as_millis(),
            self.time_in_commit.as_millis(),
        );
        self.last_heartbeat = Instant::now();
        self.time_in_recv = Duration::ZERO;
        self.time_in_processing = Duration::ZERO;
        self.time_in_commit = Duration::ZERO;
        self.messages_processed = 0;
        self.transaction_commits = 0;
    }
}

/// Process a single message. Returns `true` if the thread should exit.
#[allow(clippy::too_many_arguments, reason = "writer-loop ambient state")]
fn process_message(
    conn: &rusqlite::Connection,
    msg: WriteMessage,
    stats: &WriterStats,
    accumulator: &mut AccumulatorMaps,
    app_handle: &Option<AppHandle>,
    volume_id: &str,
    expected_total_entries: &AtomicU64,
    next_id: &AtomicI64,
    mutation_tracker: &MutationTracker,
    probe: &mut ProbeStats,
    propagate_deltas: &mut bool,
    heal_latch: &mut bool,
    deferred_checkpoint: &mut bool,
    repairs: &DeferredRepairs,
    signal: &IndexFailureSignal,
) -> bool {
    match msg {
        // ── Integer-keyed variants ───────────────────────────────────
        WriteMessage::InsertEntriesV2(entries) => {
            handle_insert_entries_v2(
                conn,
                entries,
                accumulator,
                app_handle,
                volume_id,
                expected_total_entries,
                mutation_tracker,
                signal,
            );
        }
        WriteMessage::UpsertEntryV2 {
            parent_id,
            name,
            is_directory,
            is_symlink,
            logical_size,
            physical_size,
            modified_at,
            inode,
            nlink,
        } => {
            handle_upsert_entry_v2(
                conn,
                parent_id,
                name,
                is_directory,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode,
                nlink,
                next_id,
                mutation_tracker,
                *propagate_deltas,
                repairs,
                signal,
            );
        }
        WriteMessage::MoveEntryV2 {
            entry_id,
            new_parent_id,
            new_name,
        } => {
            handle_move_entry_v2(
                conn,
                entry_id,
                new_parent_id,
                new_name,
                mutation_tracker,
                repairs,
                signal,
            );
        }
        WriteMessage::DeleteEntryById(entry_id) => {
            mutation_tracker.record_delete_entry();
            handle_delete_entry_by_id(conn, entry_id, *propagate_deltas, mutation_tracker, repairs, signal);
        }
        WriteMessage::DeleteSubtreeById(root_id) => {
            mutation_tracker.record_delete_subtree();
            handle_delete_subtree_by_id(conn, root_id, *propagate_deltas, mutation_tracker, repairs, signal);
        }
        WriteMessage::DeleteDescendantsById(root_id) => {
            // No delta propagation: the subtree will be immediately re-scanned and
            // ComputeSubtreeAggregates will recompute stats for the subtree root.
            if let Err(e) = IndexStore::delete_descendants_by_id(conn, root_id) {
                signal.note(&e, &format!("delete_descendants_by_id id={root_id}"));
            }
        }
        WriteMessage::PropagateDeltaById {
            entry_id,
            logical_size_delta,
            physical_size_delta,
            file_count_delta,
            dir_count_delta,
        } => {
            propagate_delta_by_id(
                conn,
                entry_id,
                logical_size_delta,
                physical_size_delta,
                file_count_delta,
                dir_count_delta,
                repairs,
            );
        }
        WriteMessage::PropagateMinSubtreeEpoch(start_id) => {
            propagate_min_subtree_epoch(conn, start_id, repairs);
        }
        WriteMessage::TruncateData => {
            handle_truncate_data(
                conn,
                accumulator,
                expected_total_entries,
                next_id,
                mutation_tracker,
                signal,
            );
            // The tables are gone; queued ids name rows that no longer exist.
            repairs.clear();
        }
        WriteMessage::ComputeAllAggregates { source } => {
            handle_compute_all_aggregates(
                conn,
                accumulator,
                app_handle,
                volume_id,
                expected_total_entries,
                source,
                heal_latch,
                signal,
            );
        }
        WriteMessage::ComputePartialAggregates { hot_paths, source } => {
            handle_compute_partial_aggregates(conn, accumulator, app_handle, hot_paths, source, signal);
        }
        WriteMessage::ComputeSubtreeAggregates { root_id } => {
            handle_compute_subtree_aggregates(conn, root_id, repairs, signal);
        }
        WriteMessage::UpdateLastEventId(id) => {
            if let Err(e) = IndexStore::update_meta(conn, "last_event_id", &id.to_string()) {
                signal.note(&e, "update last_event_id");
            }
        }
        WriteMessage::UpdateMeta { key, value } => {
            if let Err(e) = IndexStore::update_meta(conn, &key, &value) {
                signal.note(&e, &format!("update_meta({key})"));
            }
        }
        WriteMessage::DeleteMeta(key) => {
            if let Err(e) = IndexStore::delete_meta(conn, &key) {
                signal.note(&e, &format!("delete_meta({key})"));
            }
        }
        WriteMessage::MarkDirsListed { ids, epoch } => {
            // No MutationTracker::bump(): stamping coverage changes nothing
            // search indexes, so it must not trigger a root-search reload.
            if let Err(e) = IndexStore::mark_dirs_listed(conn, &ids, epoch) {
                signal.note(&e, &format!("mark_dirs_listed (count={}, epoch={epoch})", ids.len()));
            }
        }
        WriteMessage::BumpCurrentEpoch => {
            // No MutationTracker::bump(): a meta-only write, nothing search cares
            // about (same policy as MarkDirsListed/UpdateMeta).
            match IndexStore::bump_current_epoch(conn) {
                Ok(epoch) => log::debug!("Index writer: bumped current_epoch to {epoch}"),
                Err(e) => {
                    signal.note(&e, "bump_current_epoch");
                }
            }
        }
        #[cfg(test)]
        WriteMessage::GetEntryCount(reply) => {
            let result = IndexStore::get_entry_count(conn);
            // If the receiver dropped, that's fine; ignore the send error
            let _ = reply.send(result);
        }
        WriteMessage::Flush(reply) => {
            log::debug!(
                "Writer: processing flush (total msgs processed so far: {})",
                stats.current.total,
            );
            // All prior messages have been processed; signal the caller
            let _ = reply.send(());
        }
        WriteMessage::SetDeltaPropagation(enabled) => {
            // A control message, not a mutation: it only flips ambient writer
            // state, so no `MutationTracker::bump()` and nothing search cares about.
            log::debug!("Writer: SetDeltaPropagation({enabled})");
            *propagate_deltas = enabled;
        }
        WriteMessage::BeginTransaction => {
            log::debug!("Writer: BEGIN IMMEDIATE transaction");
            if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE") {
                log::warn!("Index writer: BEGIN TRANSACTION failed: {e}");
            }
        }
        WriteMessage::CommitTransaction => {
            let t = Instant::now();
            if let Err(e) = conn.execute_batch("COMMIT") {
                log::warn!("Index writer: COMMIT failed: {e}");
            }
            let elapsed = t.elapsed();
            probe.time_in_commit += elapsed;
            probe.transaction_commits += 1;
            let elapsed_ms = elapsed.as_millis();
            log::debug!("Writer: COMMIT transaction ({elapsed_ms}ms)");
            if elapsed_ms > 50 {
                log::info!(
                    target: "stall_probe::writer",
                    "commit_slow ms={elapsed_ms}",
                );
            }
            // The batch is closed, so a maintenance tick parked during it can run
            // now. A long batch (a journal replay) is exactly when the WAL grew
            // most, so this is the truncate that matters.
            run_deferred_wal_checkpoint(conn, signal, deferred_checkpoint);
        }
        WriteMessage::BackfillMissingDirStats => {
            handle_backfill_missing_dir_stats(conn, repairs, signal);
        }
        WriteMessage::ArmLedgerHealLatch => {
            // A control message, not a mutation (no `MutationTracker::bump()`):
            // it only arms ambient writer state. The next successful full
            // aggregate consumes it. Idempotent — re-arming an already-armed
            // latch is a no-op.
            log::debug!("Writer: ArmLedgerHealLatch");
            *heal_latch = true;
        }
        WriteMessage::MarkLedgerUnpaid => {
            // Durable first, latch second: if the process dies between them the
            // DB still says "unpaid" and the next launch heals. The reverse order
            // could lose the debt entirely.
            if let Err(e) = IndexStore::clear_ledger_heal_done(conn) {
                signal.note(&e, "clear_ledger_heal_done");
            }
            *heal_latch = true;
        }
        WriteMessage::PayLedgerIfUnpaid => {
            if *heal_latch {
                log::debug!("Writer: bulk window closed unpaid, rebuilding aggregates");
                handle_compute_all_aggregates(
                    conn,
                    accumulator,
                    app_handle,
                    volume_id,
                    expected_total_entries,
                    AggSource::Sql,
                    heal_latch,
                    signal,
                );
            }
        }
        WriteMessage::IncrementalVacuum => {
            handle_incremental_vacuum(conn, signal);
        }
        WriteMessage::WalCheckpoint => {
            request_wal_checkpoint(conn, signal, deferred_checkpoint);
        }
        WriteMessage::EmitDirUpdated(paths) => {
            #[cfg(test)]
            mutation_tracker.record_emit(&paths);
            if let Some(app) = app_handle {
                reconciler::emit_dir_updated(app, paths);
            }
        }
        WriteMessage::Shutdown => return true,
    }
    false
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
pub(super) mod tests;

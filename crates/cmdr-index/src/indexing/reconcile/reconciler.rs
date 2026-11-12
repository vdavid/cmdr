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
//!
//! ## What lives where
//!
//! This file holds [`EventReconciler`] — the buffer, the live/replay switch, and the
//! state a rescan drain rides on. The work itself is split by responsibility:
//! [`dir_read`] reads one directory off disk, [`diff`] compares it against the DB,
//! [`subtree`] walks a subtree through those two, [`events`] turns one FSEvent into
//! writes, and [`finish`] ends a full-rescan walk.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
// `Ordering` isn't used directly in this file; the `tests/` files reach it through
// their `use super::*`. (The `rescan*` submodules import it themselves.)
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;
// Only the test-only `new_with_throttle_window` names it here.
#[cfg(test)]
use std::time::Duration;

use rusqlite::Connection;

use crate::indexing::IndexPathSpace;
use crate::indexing::deletes;
use crate::indexing::hold::VolumeWork;
use crate::indexing::watch::watcher::FsChangeEvent;
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::firmlinks;
use cmdr_fs::ignore_poison::IgnorePoison;
// Only the test-only `new()` / `new_with_throttle_window` and the rescan tests
// name the root volume id; production sites thread the real id through `new_for`.
#[cfg(test)]
use crate::ROOT_VOLUME_ID;
use cmdr_fs::pluralize::pluralize;

mod diff;
mod dir_read;
mod escalation;
mod events;
mod finish;
mod rescan;
mod subtree;
mod throttle;

pub(crate) use diff::{LiveChild, MissingRows, diff_dir_against_db};
pub(crate) use dir_read::{FsChild, Listing, child_is_absent, read_fs_children};
/// The immediate-send form, reached only by the per-event tests (see its doc).
#[cfg(test)]
pub(crate) use events::process_fs_event;
pub(crate) use events::process_fs_event_into;
pub(crate) use finish::{BulkReconcileGuard, finish_reconcile, send_marks};
pub(in crate::indexing) use subtree::{ReconcileSummary, reconcile_subtree};

/// The portable read, reached only by the tests that pin it against the batched one.
#[cfg(test)]
use dir_read::read_fs_children_via_read_dir;
use events::origin_dir;

/// The shallow-anchor sweep window, re-exported for its out-of-module users:
/// `manager::resume_or_scan` reseeds it from `meta` at index start,
/// `scan_completion` restarts it whenever a full walk finishes, and `queries`
/// reads the record for the volume status surface.
pub(in crate::indexing) use rescan::route::{
    SHALLOW_COALESCED_KEY, SHALLOW_RESCAN_MIN_INTERVAL, SHALLOW_SWEEP_AT_KEY, now_unix, record_sweep_completed,
    seed_from_meta, sweep_record,
};
use rescan::throttle::RescanThrottle;
use throttle::Throttle;

/// How the reconciler asks for a VISIBLE scanner rescan when a shallow/root-scale
/// `MustScanSubDirs` anchor should take the `start_scan` path instead of the
/// invisible reconcile hold (see [`rescan::route`]). Defaults to [`Self::Registry`]
/// on every production reconciler, so the two live-loop construction sites
/// (`scan_completion`, `event_loop::replay`) need no extra wiring.
pub(in crate::indexing) enum ScanTrigger {
    /// Production: spawn [`crate::indexing::lifecycle::manager::perform_registry_rescan`],
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
pub(crate) type LiveThrottle = Throttle<PendingUpsert>;

/// The suppressed upsert a throttled file's trailing flush replays. Built from
/// the already-stat'd suppressed event, so the sweep never re-stats (which would
/// risk blocking the live loop on a dead mount, and add a phantom-apply-on-
/// deleted-file case). Only regular files are throttled, so `is_directory` /
/// `is_symlink` are always false at flush time.
pub(crate) struct PendingUpsert {
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

/// Buffers FSEvents during the initial scan and replays them after the scan completes.
pub struct EventReconciler {
    /// Events buffered during scan, in arrival order.
    buffer: Vec<FsChangeEvent>,
    /// Whether we're in buffering mode (scan in progress).
    buffering: bool,
    /// Set when the buffer cap is hit. Forces a full rescan after the
    /// current scan completes instead of replaying individual events.
    pub(crate) buffer_overflow: bool,
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
    /// Per-subtree rescan throttle (leading + trailing, cost-proportional window).
    /// Caps a hard-churning subtree to at most one reconcile per window, so a
    /// folder's size stays bounded-fresh without re-walking continuously. Shared with
    /// spawned rescan tasks (which record each completion); consulted at pick time,
    /// at enqueue time, and on the sweep tick — the last two derive the hourglass
    /// hold from it (`rescan/hold.rs`). Its trailing re-kick rides the same sweep tick
    /// as `throttle`, via [`EventReconciler::sweep_rescan_throttle`].
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
    /// tests inject `Disabled`/`Recording`. See [`ScanTrigger`] and [`rescan::route`].
    scan_trigger: ScanTrigger,
    /// The work of the loop this reconciler serves (`LiveLoop`): its stop signal,
    /// and the share of the volume's hold that loop carries, handed down by whoever
    /// built this reconciler. Every detached subtree walk is taken under it, so
    /// tearing the volume down stops them and each holds the drive while it reads.
    /// Pushed in rather than looked up, because a walk that starts after teardown
    /// would look up nothing and never stop.
    work: VolumeWork,
    /// How much of the volume the loop this reconciler serves answers for, which
    /// is what decides the ground it may walk (`WatchScope::may_walk`).
    ///
    /// `None` for a reconciler with no live loop behind it (a test, a one-shot
    /// subtree reconcile), where nothing is walking and everything is in scope.
    scope: Option<crate::indexing::watch::branches::WatchScope>,
    /// Deletes this batch decided on, held until [`flush_deletes`](Self::flush_deletes)
    /// can prove the drive was still there.
    ///
    /// An event whose stat failed because the drive went away looks exactly like a
    /// removal, so sending per event would let a vanishing drive delete its own
    /// index one event at a time. Gathering is what makes ONE presence read cover a
    /// whole batch.
    pending_deletes: Vec<WriteMessage>,
    /// `MustScanSubDirs` anchors the post-scan replay found in the buffer, held
    /// until the loop is live ([`kick_pending_rescans`](Self::kick_pending_rescans)).
    ///
    /// The scan may have read an anchor's subtree BEFORE the events it lost, so the
    /// scan's rows can't answer for it. They wait here rather than in
    /// `pending_rescans` because routing isn't decided yet: a shallow one belongs to
    /// the visible sweep, which the drain must never walk. A set, so a tree that
    /// overflowed many times during one scan is one anchor.
    deferred_must_scans: std::collections::BTreeSet<PathBuf>,
}

/// Everything a rescan walk needs, cloned out of the [`EventReconciler`] so the
/// spawned thread — and the self-drain call it makes on completion — can own it.
///
/// It travels as one value because the drain re-enters itself: a seven-argument
/// call repeated at four sites is where a mismatched pair sneaks in. It lives
/// HERE rather than beside the drain in `rescan/` because `EventReconciler`'s
/// own method returns it: naming a `rescan` type in this module's signatures
/// would import the child back into the parent, and that direction closes a
/// cycle through `lifecycle::manager`.
#[derive(Clone)]
struct RescanDrain {
    /// Anchors waiting to walk. Shared with the spawned walk so it can drain the
    /// queue on completion.
    pending: Arc<Mutex<HashSet<PathBuf>>>,
    /// Whether a walk is in flight (the drain is single-flight).
    active: Arc<AtomicBool>,
    /// The path of the in-flight walk, for the removal-storm drop rule.
    active_path: Arc<Mutex<Option<PathBuf>>>,
    /// The per-subtree throttle consulted at pick time and recorded on completion.
    throttle: Arc<Mutex<RescanThrottle>>,
    /// The volume's path space, so the walk resolves in the right one.
    space: IndexPathSpace,
    /// The volume this drain belongs to (routes the hourglass hold/release).
    volume_id: String,
    /// The work the next walk is taken under: the reconciler's for the first walk,
    /// then each walk's own for the walk it hands the drain on to, so a walk thread
    /// never carries the live loop's share. Each walk takes a child, so tearing the
    /// volume down stops a long walk instead of letting it write into a draining
    /// writer, and the drive stays held until the walk's last read. ❌ Don't
    /// resolve this from the registry inside the walk: once the volume is gone the
    /// lookup answers with a token that never fires, which is exactly the case that
    /// needs to stop.
    work: VolumeWork,
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
        let mut reconciler = Self::new_for(
            ROOT_VOLUME_ID.to_string(),
            IndexPathSpace::root(),
            VolumeWork::for_test(ROOT_VOLUME_ID),
        );
        reconciler.scan_trigger = ScanTrigger::Disabled;
        reconciler
    }

    /// Create a reconciler bound to a volume's id + path space. A mount-rooted
    /// external drive passes its space so live/replay resolution strips the mount
    /// root, and its id so the rescan hourglass routes to its own tracker. `work` is
    /// the work of the loop it serves (`LiveLoop`), which it carries from here on.
    pub(crate) fn new_for(volume_id: String, space: IndexPathSpace, work: VolumeWork) -> Self {
        Self::with_space_and_throttle(volume_id, space, Throttle::new(resolve_downloads_prefix()), work)
    }

    /// Construct with a caller-supplied id + space + throttle (tests inject a short window).
    fn with_space_and_throttle(
        volume_id: String,
        space: IndexPathSpace,
        throttle: LiveThrottle,
        work: VolumeWork,
    ) -> Self {
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
            work,
            scope: None,
            pending_deletes: Vec::new(),
            deferred_must_scans: std::collections::BTreeSet::new(),
        }
    }

    /// Send the deletes this batch gathered, but only if the drive is still listed.
    ///
    /// ⚠️ **The presence read comes AFTER the events were stat'd**, which is the whole
    /// point: a drive that went away mid-batch makes every stat fail, and those
    /// failures are indistinguishable from real removals. One read per batch, ❌ never
    /// per event — a storm is tens of thousands of events and would be that many
    /// mount-table reads.
    ///
    /// A batch it drops is not lost work: nothing was deleted, so the rows stay and
    /// the index is marked for a rebuild through the delete generation instead.
    pub(crate) fn flush_deletes(&mut self, writer: &IndexWriter) {
        if self.pending_deletes.is_empty() {
            return;
        }
        if !self.work.drive_is_listed() {
            log::info!(
                "Reconciler: '{}' stopped being listed, so {} went unsent rather than deleting rows against a drive that left",
                self.volume_id,
                pluralize(self.pending_deletes.len() as u64, "gathered delete"),
            );
            self.pending_deletes.clear();
            // The batches already sent are the ones nothing can take back.
            deletes::note_the_drive_left(self.work.volume_id(), writer);
            return;
        }
        // A `Some(true)` here is also the presence half of the delete generation's reset.
        deletes::drive_seen(self.work.volume_id());
        for message in self.pending_deletes.drain(..) {
            let _ = writer.send(message);
        }
        deletes::batch_sent(self.work.volume_id());
    }

    /// Tell this reconciler how much of its volume the loop behind it answers for.
    ///
    /// It decides the ground the reconciler may walk: never into a cover walk's
    /// in-flight ground (on any volume), and on a branch-watched one never outside
    /// the covered branches. See `WatchScope::may_walk`.
    pub(crate) fn within(&mut self, scope: crate::indexing::watch::branches::WatchScope) -> &mut Self {
        self.scope = Some(scope);
        self
    }

    /// Whether this reconciler serves a volume watched branch by branch.
    fn is_branch_confined(&self) -> bool {
        matches!(
            self.scope,
            Some(crate::indexing::watch::branches::WatchScope::Branches(_))
        )
    }

    /// Whether a rescan anchor is ground this reconciler may walk right now.
    fn may_walk(&self, anchor: &Path) -> bool {
        self.scope.as_ref().is_none_or(|scope| scope.may_walk(anchor))
    }

    /// Test constructor with an explicit throttle window, so the trailing flush is
    /// exercised without sleeping a real [`THROTTLE_WINDOW`].
    #[cfg(test)]
    pub(crate) fn new_with_throttle_window(window: Duration) -> Self {
        let mut reconciler = Self::with_space_and_throttle(
            ROOT_VOLUME_ID.to_string(),
            IndexPathSpace::root(),
            Throttle::with_window(window, None),
            VolumeWork::for_test(ROOT_VOLUME_ID),
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
        on_dirs_changed: &mut dyn FnMut(Vec<String>),
    ) -> Result<u64, String> {
        // Sort by event_id to process in order
        self.buffer.sort_by_key(|e| e.event_id);

        let total = self.buffer.len();
        let mut processed = 0u64;
        let mut last_event_id = scan_start_event_id;
        let mut origin_dirs: Vec<String> = Vec::new();

        log::info!(
            "Reconciler: replaying {} (scan_start_event_id={scan_start_event_id})",
            pluralize(total as u64, "buffered event")
        );

        for event in &self.buffer {
            // Skip events that the scan already covered
            if event.event_id <= scan_start_event_id {
                continue;
            }

            // A lost-events signal that arrived mid-scan. ❌ Never the per-event
            // path below: it stats the anchor and re-lists nothing, so the signal
            // would vanish. Nothing walks during replay, so it waits for the live
            // loop, which routes it exactly like a live one.
            if event.flags.must_scan_sub_dirs {
                self.deferred_must_scans
                    .insert(PathBuf::from(self.space.absolute(&event.path)));
                last_event_id = event.event_id;
                processed += 1;
                continue;
            }

            // Replay stays unthrottled (None): journal catch-up must converge
            // fully and fast; throttling is a live-steady-state concern.
            // Missing-parent escalations DEFER into the pending set without starting
            // a rescan (no live queueing during replay); the live loop that follows
            // drains them via `kick_pending_rescans`.
            let mut escalation: Option<PathBuf> = None;
            if let Some(paths) = process_fs_event_into(
                event,
                &self.space,
                conn,
                writer,
                None,
                &mut escalation,
                &mut self.pending_deletes,
            ) {
                origin_dirs.extend(paths);
            }
            if let Some(anchor) = escalation {
                self.pending_rescans.lock_ignore_poison().insert(anchor);
            }

            last_event_id = event.event_id;
            processed += 1;
        }

        // The replay is one batch, so its deletes take one presence read.
        self.flush_deletes(writer);

        // Hand the caller every ORIGIN dir the replay touched (the dirs whose own
        // listings changed). The caller expands to the ancestor closure for the
        // facts that need it (the FE emit, the post-replay verification set).
        if !origin_dirs.is_empty() {
            on_dirs_changed(origin_dirs);
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

    /// Switch from buffering to live mode. Clears the buffer, and keeps the
    /// `MustScanSubDirs` anchors the replay set aside for the live loop.
    pub fn switch_to_live(&mut self) {
        self.buffering = false;
        self.buffer_overflow = false;
        self.buffer.clear();
        self.buffer.shrink_to_fit();
        log::info!("Reconciler: switched to live mode");
    }

    /// Process a single event in live mode.
    ///
    /// Collects the ORIGIN dirs (the dirs whose own listing changed) into
    /// `pending_origins` for batched emission by the caller (1s flush interval).
    /// Returns the event ID on success, or `None` if still buffering.
    pub fn process_live_event(
        &mut self,
        event: &FsChangeEvent,
        conn: &Connection,
        writer: &IndexWriter,
        pending_origins: &mut HashSet<String>,
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
            self.admit_must_scan(PathBuf::from(&absolute), writer);
            return Some(event.event_id);
        }

        // Missing-parent escalation (Leak B): if the event's parent chain isn't in
        // the index, `process_fs_event` sets `escalation` to the rescan anchor
        // instead of dropping the credit. Live mode queues it right away.
        let mut escalation: Option<PathBuf> = None;
        if let Some(origins) = process_fs_event_into(
            event,
            &self.space,
            conn,
            writer,
            Some(&mut self.throttle),
            &mut escalation,
            &mut self.pending_deletes,
        ) {
            pending_origins.extend(origins);
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
    /// throttle-sweep tick. Returns the ORIGIN dirs whose listings the flushes
    /// changed, for the caller's batched `DirsUpdated` emit (which expands
    /// them to the ancestor closure).
    pub(crate) fn sweep_throttle(&mut self, writer: &IndexWriter, now: Instant) -> Vec<String> {
        let flushes = self.throttle.sweep(now);
        let mut origins: Vec<String> = Vec::new();
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
            origins.extend(origin_dir(&path));
        }
        origins
    }

    /// Whether the reconciler's event buffer overflowed during the scan.
    pub(crate) fn did_buffer_overflow(&self) -> bool {
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

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;

//! `IndexManager`: central coordinator for the drive indexing system.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use super::phases;
use super::state::{INDEX_REGISTRY, IndexPhase, start_pending_phases};
use crate::indexing::IndexPathSpace;
use crate::indexing::events::{
    ActivityPhase, DEBUG_STATS, EventSink, IndexDebugStatusResponse, IndexEvent, IndexStatusResponse, PhaseRecord,
    RescanReason, ScanRunKind, emit_rescan_notification, set_phase_for,
};
use crate::indexing::lifecycle::progress_reporter::ScanProgressReporter;
use crate::indexing::lifecycle::rescan_request::ScanStartError;
use crate::indexing::reconcile::local_reconcile;
use crate::indexing::reconcile::reconciler;
use crate::indexing::scanner::{self, ScanConfig};
use crate::indexing::store::IndexStore;
use crate::indexing::volume::IndexVolumeKind;
use crate::indexing::watch::branches;
use crate::indexing::watch::event_loop::{JOURNAL_GAP_THRESHOLD, ReplayConfig, run_replay_event_loop};
use crate::indexing::watch::watcher::{self, DriveWatcher};
use crate::indexing::writer::{AggSource, IndexWriter, WriteMessage};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::pluralize::pluralize;

// ── IndexManager ─────────────────────────────────────────────────────

/// Central coordinator for the drive indexing system.
///
/// Owns the SQLite store (reads), the writer thread (writes), and the scanner handle.
/// Accessed by module-level functions that lock the `INDEXING` static.
pub(crate) struct IndexManager {
    /// Volume ID (for example, "root" for /)
    pub(super) volume_id: String,
    /// What kind of volume this is, which selects the scan strategy (the guarded
    /// walker + FSEvents for `Local`, the `Volume`-trait scanner with no journal for
    /// `Smb`) and the launch-time freshness. See `IndexVolumeKind`.
    pub(super) kind: IndexVolumeKind,
    /// Volume root path
    pub(super) volume_root: PathBuf,
    /// Whether this volume's filesystem inode is a trustworthy identity, resolved
    /// once at construction (from the volume's `FilesystemKind` for a local
    /// external drive; `true` for the boot disk and trait-scanned volumes). Feeds
    /// the per-scan [`IndexPathSpace`] so a FAT/exFAT drive stores `inode: None`
    /// and its rename pre-pass stays inert. See `filesystem_kind::has_stable_inodes`.
    pub(super) inodes_trustworthy: bool,
    /// SQLite store for reads
    pub(super) store: IndexStore,
    /// Writer handle for sending writes
    pub(super) writer: IndexWriter,
    /// Handle to the active full scan (if running)
    pub(super) scan_handle: Option<scanner::ScanHandle>,
    /// This VOLUME's stop signal, the root of every cancellation under it — the
    /// SAME token the registry `IndexInstance` holds, so the two can't disagree.
    /// Each scan, reconcile, and subtree walk runs on a `child_token()`, so
    /// stopping one scan (`stop_scan`) leaves the volume able to start another,
    /// while `shutdown` cancels this and everything below it at once.
    pub(super) volume_cancel: CancellationToken,
    /// FSEvents watcher (started alongside scan, persists after scan completes)
    drive_watcher: Option<DriveWatcher>,
    /// Whether the watcher above covers only what a search walk covered, rather
    /// than the whole volume.
    ///
    /// The two are mutually exclusive by construction — `ensure_branch_watch`
    /// declines when a watcher is already running, and `start_scan` retires the
    /// branch set — so this says which of them is up, never both.
    pub(super) branch_watched: bool,
    /// Live event processing task (runs after reconciliation completes).
    /// Shared with spawned async tasks so they can store the handle.
    live_event_task: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Where this volume's scan and phase reports go.
    pub(super) events: Arc<dyn EventSink>,
    /// Whether a full scan is currently running. Shared with the completion handler.
    pub(super) scanning: Arc<AtomicBool>,
    /// This volume's freshness signal — the SAME `Arc` the registry `IndexInstance`
    /// holds. The manager fires its scan transitions (`ScanStarted`,
    /// `ScanCompleted`, `WatcherDied`) through this handle via
    /// `state::apply_freshness_event_on`, which never locks `INDEX_REGISTRY`. That
    /// is what lets a held-registry caller (`force_scan`, the journal-gap fallback)
    /// drive a scan without self-deadlocking on a registry re-lock. External
    /// (volume-id-only) callers still use `state::apply_freshness_event`.
    pub(super) freshness: Arc<std::sync::Mutex<Option<super::freshness::Freshness>>>,
    /// The running phase machine, when this volume is being covered in pieces
    /// rather than walked whole. Its flags are what every scan entry refuses
    /// against and what `get_status` reports; see `phases::PhaseHandle`.
    pub(super) phases: Option<phases::PhaseHandle>,
    /// Where this volume sits between "the launch route handed it to the phase
    /// machine" and "the machine is running". See [`phased::PendingPhases`]: every
    /// state but `No` counts as WORK, the window off the registry lock included.
    pub(super) pending_phases: PendingPhases,
    /// Calibration for the in-flight scan, captured in `start_scan`: the prior
    /// completed scan's totals (read from meta before truncating) plus the
    /// scanned volume's used bytes (fetched once). A plain field is enough —
    /// `start_scan` is `&mut self` and `get_status` is `&self`. `None` until the
    /// first scan starts; refreshed at the start of every scan.
    pub(super) scan_calibration: Option<ScanCalibration>,
}

/// The static, per-scan inputs the frontend needs to pick and drive a scan
/// progress tier. Captured once at scan start (`get_status` reads it back for
/// late-join), so the moving 500 ms progress events carry only live counters.
#[derive(Debug, Clone, Copy)]
pub(super) struct ScanCalibration {
    /// The prior completed scan's persisted totals (tier-1 denominator + ETA
    /// seed), picked for THIS run's kind: same-kind first, then the last scan of
    /// any kind. The two walks differ ~5x, so mixing them ships a wrong ETA.
    pub(super) prior: crate::indexing::store::ScanCalibration,
    /// The scanned volume's used bytes at scan start (tier-2 denominator). `None`
    /// when the space-info fetch failed; never blocks or delays the scan.
    pub(super) volume_used_bytes: Option<u64>,
    /// What kind of run this is. Rides the started event so the frontend states
    /// it, and picks the calibration bucket the completion handler writes into.
    pub(super) run_kind: ScanRunKind,
}

/// The live scan-progress fields `get_status` surfaces on `IndexStatusResponse`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LiveScanCounters {
    entries_scanned: u64,
    dirs_found: u64,
    bytes_scanned: u64,
    volume_used_bytes: Option<u64>,
    scan_run_kind: Option<ScanRunKind>,
    prior_total_entries: Option<u64>,
    prior_scan_duration_ms: Option<u64>,
}

/// Derive the live scan counters for `get_status` from the active scan's progress
/// snapshot and the stashed per-scan calibration. Extracted as a pure function so
/// the snapshot-and-calibration combining is unit-testable without an `AppHandle`
/// (`get_status` itself needs a full `IndexManager`, which the module's testing
/// bar keeps under integration coverage). No active scan → all-zero counters; the
/// `volume_used_bytes` denominator rides the stashed calibration so a mid-scan
/// window reload can still backfill tier-2 progress after missing the started event.
fn live_scan_counters(
    snapshot: Option<scanner::ScanProgressSnapshot>,
    calibration: Option<ScanCalibration>,
) -> LiveScanCounters {
    LiveScanCounters {
        entries_scanned: snapshot.map(|s| s.entries_scanned).unwrap_or(0),
        dirs_found: snapshot.map(|s| s.dirs_found).unwrap_or(0),
        bytes_scanned: snapshot.map(|s| s.bytes_scanned).unwrap_or(0),
        volume_used_bytes: calibration.and_then(|c| c.volume_used_bytes),
        scan_run_kind: calibration.map(|c| c.run_kind),
        prior_total_entries: calibration.and_then(|c| c.prior.total_entries),
        prior_scan_duration_ms: calibration.and_then(|c| c.prior.scan_duration_ms),
    }
}

/// Which scanner a forced (re)scan must use for a volume of a given kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RescanScanner {
    /// The `Volume`-trait walk from the share/storage root (`start_volume_scan`).
    /// SMB and MTP — there is no local filesystem to walk and no FSEvents journal.
    VolumeTrait,
    /// The guarded walker + FSEvents walk from the volume root (`start_scan`). Local disk only.
    LocalWalker,
}

/// Pure routing for `force_rescan`: pick the scanner by the TYPED volume kind.
///
/// This is the regression anchor for the real-hardware bug where a NAS "Rescan
/// now" ran the local guarded walker over the SMB mount (walking nothing, then
/// falsely marking the index complete). A local-scanner kind (boot disk or a
/// local external drive) maps to `LocalWalker`; a trait-scanned kind (SMB/MTP)
/// maps to `VolumeTrait`. Kept as a separate pure function so the dispatch is
/// unit-testable without an `AppHandle`.
fn rescan_scanner_for_kind(kind: IndexVolumeKind) -> RescanScanner {
    if kind.uses_local_scanner() {
        RescanScanner::LocalWalker
    } else {
        RescanScanner::VolumeTrait
    }
}

/// Whether a LOCAL (re)scan should reconcile in place rather than truncate +
/// rebuild. True only when the index holds rows BEYOND the ROOT sentinel AND the
/// prior scan actually COMPLETED.
///
/// `entry_count > 1`, not `> 0`: `create_tables` → `ensure_root_sentinel` always
/// inserts the ROOT row (id=1), and `TruncateData` re-inserts it, so a
/// never-scanned DB has `entry_count == 1`. A `> 0` predicate would route a
/// brand-new user's FIRST `/` scan to the serial reconcile instead of the fast
/// parallel guarded-walker bulk build.
///
/// `prior_scan_completed` (the completeness gate): reconcile ONLY a previously
/// COMPLETED index (`scan_completed_at` was present at scan start). A partial that
/// never finished (first scan interrupted, or repeated mid-scan quits) takes the
/// fast truncate + parallel guarded-walker rebuild instead. Reason: reconcile's per-dir
/// serial walk plus its add-everything delta is far slower than a parallel bulk
/// rebuild when the existing index is only a small fraction complete — a 4%-complete
/// partial made the app look hung for ~15 min on a real `/`. Reconcile is the right
/// call only when the index is substantially complete (a rescan with a small delta:
/// sizes stay visible, no freelist). A tiny partial is mostly `<dir>` anyway, so
/// keeping it "visible" buys little, and the guarded walker is fast. (LOCAL-only; the network
/// predicate stays unchanged — a NAS rescan is slow, so keeping the partial visible
/// is worth more there, and network partials are small.)
///
/// Pure so the boundary is unit-testable without an `AppHandle`.
fn local_rescan_reconciles(entry_count: u64, prior_scan_completed: bool) -> bool {
    entry_count > 1 && prior_scan_completed
}

/// Whether `resume_or_scan`'s local branch should replay the FSEvents journal on
/// launch, rather than (re)scanning.
///
/// **Gate on `has_event_journal()`, NEVER on `stored_event_id.is_some()`** (plan
/// Decision 2). The shared local event loop and scan-completion handler persist
/// `last_event_id` for ANY local-scanner volume, so a completed `LocalExternal`
/// index carries BOTH a stored event id AND `scan_completed_at` — yet an external
/// volume has no `.fseventsd` journal to replay. Gating on the id would route it
/// into an empty/garbage replay of a journal it doesn't have; gating on the kind
/// sends it to a fresh scan (empty DB) or reconcile-in-place (populated DB), the
/// path that (re)starts the live `DriveWatcher`. Only the boot disk (`Local`) has
/// a journal, so only it replays. A future cleanup that collapses this back to an
/// id-based gate silently breaks external drives — keep it kind-based.
///
/// The remaining conditions match the original: replay needs platform support
/// (macOS FSEvents; false on Linux), a completed prior scan, and a positive
/// stored event id. Pure so the gate is unit-testable without an `AppHandle`.
fn should_replay_journal(
    kind: IndexVolumeKind,
    supports_event_replay: bool,
    scan_completed: bool,
    stored_event_id: Option<u64>,
) -> bool {
    kind.has_event_journal() && supports_event_replay && scan_completed && stored_event_id.is_some_and(|id| id > 0)
}

/// Take a volume's manager OUT of the registry (transient `ShuttingDown`), stop
/// its current watcher + live loop, run a fresh `start_scan` OFF the registry
/// lock, then reinsert it as `Running`. Re-resolves the manager by volume id, so
/// it can be spawned fire-and-forget with no captured manager.
///
/// Shared by two triggers that both mean "roll forward from the visible scanner,
/// not the invisible reconcile": the cold-start replay full-scan fallback
/// (`start_replay`) and the shallow-`MustScanSubDirs` scanner routing
/// (`reconcile/reconciler/rescan/mod.rs`). Single-flight: `start_scan` no-ops if a scan is
/// already running, so overlapping triggers coalesce.
///
/// Runs the blocking `start_scan` prelude off the lock (holding it across
/// `flush_blocking` + the space-info query would freeze every concurrent registry
/// user; the freshness firing inside `start_scan` would also re-lock the registry,
/// now fired through the manager's own `Arc`). Mirrors `state::force_scan`'s
/// extract-drop-run-reinsert flow.
pub(in crate::indexing) async fn perform_registry_rescan(volume_id: &str, trigger: &str) {
    let mut mgr = {
        let mut reg = match INDEX_REGISTRY.lock() {
            Ok(g) => g,
            Err(e) => {
                log::warn!("Failed to lock registry for a scanner rescan: {e}");
                return;
            }
        };
        let Some(instance) = reg.get_mut(volume_id) else {
            return;
        };
        // `mgr` is the `IndexManager` taken out of `Running`.
        match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
            IndexPhase::Running(mut mgr) => {
                // Stop the current watcher + live loop (the fresh scan starts its
                // own) while still under the lock — these are non-blocking.
                if let Some(ref mut watcher) = mgr.drive_watcher {
                    watcher.stop();
                }
                mgr.drive_watcher = None;
                mgr.branch_watched = false;
                {
                    let mut task_guard = mgr.live_event_task.lock_ignore_poison();
                    if let Some(task) = task_guard.take() {
                        task.abort();
                    }
                }
                mgr
            }
            other => {
                instance.phase = other;
                return;
            }
        }
    };

    // Guard released: run the blocking-prelude scan start off the lock. The same
    // door "Rescan now" goes through, so a volume the machine is still building
    // has its phases restarted rather than its half-built index truncated.
    if let Err(ref e) = mgr.cover_or_scan(trigger) {
        log::warn!("Scanner rescan for '{volume_id}' failed to start: {e}");
    }

    // Re-lock to restore the manager as `Running`. If the volume was torn down
    // while we were detached, shut the orphaned manager down instead of
    // resurrecting a removed volume.
    let mut reg = match INDEX_REGISTRY.lock() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("Failed to re-lock registry after a scanner rescan: {e}");
            mgr.shutdown();
            return;
        }
    };
    match reg.get_mut(volume_id) {
        Some(instance) if matches!(instance.phase, IndexPhase::ShuttingDown) => {
            instance.phase = IndexPhase::Running(mgr);
            drop(reg);
            // On the far side of the restore, for the reason that function names.
            start_pending_phases(volume_id);
        }
        _ => {
            drop(reg);
            log::info!("scanner rescan: '{volume_id}' was torn down during scan start; shutting down the manager");
            mgr.shutdown();
        }
    }
}

impl IndexManager {
    /// Create a new IndexManager for a volume of the given kind.
    ///
    /// Opens (or creates) the SQLite database at `db_path`, spawns the writer
    /// thread, and records the volume kind so `resume_or_scan` picks the right scan
    /// strategy. Takes the resolved path rather than resolving it, so nothing here
    /// needs to reach app-owned configuration.
    pub fn new_for_kind(
        volume_id: String,
        volume_root: PathBuf,
        db_path: PathBuf,
        kind: IndexVolumeKind,
        inodes_trustworthy: bool,
        signals: super::state::VolumeSignals,
    ) -> Result<Self, String> {
        let store = IndexStore::open(&db_path).map_err(|e| format!("Failed to open index store: {e}"))?;

        // Only the search-feeding volume's writer bumps the global
        // `WRITER_GENERATION`. Search is single-volume by construction (D7): it
        // loads exactly one in-memory index off `root`'s (local-disk) DB. An
        // SMB/MTP writer must not invalidate the root search index it doesn't
        // feed, or every NAS/phone change-notify event would thrash a full root
        // search reload. See `writer::WRITER_GENERATION` and `indexing/DETAILS.md`.
        let super::state::VolumeSignals {
            freshness,
            events,
            cancel: volume_cancel,
        } = signals;

        let feeds_search = kind.feeds_search();
        let writer = IndexWriter::spawn_for(&db_path, Arc::clone(&events), feeds_search, volume_id.clone())
            .map_err(|e| format!("Failed to spawn index writer: {e}"))?;

        log::debug!(
            "IndexManager created for volume '{volume_id}' ({kind:?}) at {}",
            volume_root.display()
        );

        Ok(Self {
            volume_id,
            kind,
            volume_root,
            inodes_trustworthy,
            store,
            writer,
            scan_handle: None,
            volume_cancel,
            drive_watcher: None,
            branch_watched: false,
            live_event_task: Arc::new(std::sync::Mutex::new(None)),
            events,
            scanning: Arc::new(AtomicBool::new(false)),
            freshness,
            phases: None,
            pending_phases: PendingPhases::No,
            scan_calibration: None,
        })
    }

    /// This volume's path space: pass-through for the `/`-rooted boot disk,
    /// mount-relative strip for a mount-rooted drive, carrying its inode-trust fact.
    ///
    /// The one place it's derived, so the scan, the replay + live loops, and the
    /// per-navigation verifier can't drift on where this volume is rooted.
    pub(super) fn path_space(&self) -> IndexPathSpace {
        IndexPathSpace::for_volume(self.kind, &self.volume_root, self.inodes_trustworthy)
    }

    /// Whether a filesystem watcher is up for this volume, over the whole volume
    /// or over the branches a search walked.
    #[cfg(test)]
    pub(super) fn is_watching(&self) -> bool {
        self.drive_watcher.is_some()
    }

    /// Do whatever this volume's own index says it needs at launch: replay its
    /// journal, walk it whole, or hand it to the phase machine.
    ///
    /// This reads the facts and pays for the answer; the answer itself is
    /// [`launch_route`](launch_route::launch_route), where the whole routing table
    /// lives as a table. Everything before the routing call is preparation every
    /// arm needs (the sweep-window seed, the ledger-heal latch), and its PLACEMENT
    /// is constrained: it sits below the `is_trait_scanned` early return, or a
    /// share and a phone get routed into a local phase machine.
    pub fn resume_or_scan(&mut self) -> Result<(), String> {
        // SMB and MTP volumes have no event journal, so there's nothing to
        // replay: a persisted index loaded Stale on launch (already seeded by
        // `start_indexing_for`) and stays browsable until the user rescans; a
        // never-scanned volume gets a fresh `Volume`-trait scan.
        if self.kind.is_trait_scanned() {
            return self.resume_or_scan_network();
        }

        let status = self
            .store
            .get_index_status()
            .map_err(|e| format!("Failed to get index status: {e}"))?;
        let stored_event_id = status.last_event_id.as_deref().and_then(|s| s.parse::<u64>().ok());

        // Reload the shallow-`MustScanSubDirs` sweep window AND its coalesced count
        // from disk. Both live in a process-global ledger, so without this a
        // relaunch would hand the next shallow anchor a free full sweep and reset
        // the count to zero — and a once-a-day policy that resets on every launch
        // is not a once-a-day policy. See `reconcile/reconciler/rescan/route.rs`.
        //
        // The window takes the LATER of two facts, which is what makes an
        // interrupted sweep safe: `scan_completed_at` (any full walk FINISHED, and
        // `start_scan` deletes it before walking) and `shallow_sweep_at` (a sweep
        // was TRIGGERED, which survives a walk that never finished).
        let read_conn = self.store.read_conn();
        let sweep_at = IndexStore::get_meta(read_conn, reconciler::SHALLOW_SWEEP_AT_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok());
        let completed_at = status.scan_completed_at.as_deref().and_then(|s| s.parse::<u64>().ok());
        let coalesced = IndexStore::get_meta(read_conn, reconciler::SHALLOW_COALESCED_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        reconciler::seed_from_meta(&self.volume_id, sweep_at.max(completed_at), coalesced);

        // One-shot ledger heal (see `indexing/DETAILS.md` § "The dir_stats ledger").
        // A DB that has never healed still carries pre-ledger drift; arm the
        // writer latch so the next full aggregate rebuilds and marks it. EVERY
        // branch below arms (any flow's own aggregate then consumes it); the
        // replay branch, which runs no full aggregate of its own, ALSO enqueues
        // the heal's own `Sql` aggregate (below, `heal_pending` into `start_replay`).
        let heal_pending = IndexStore::ledger_heal_done(self.store.read_conn()).is_ok_and(|done| !done);
        if heal_pending {
            let _ = self.writer.send(WriteMessage::ArmLedgerHealLatch);
        }

        // Journal replay is gated on the KIND, never on a stored event id (see
        // `should_replay_journal` for the load-bearing why). The gap pre-check
        // rides along: replaying tens of millions of events is slower than a fresh
        // walk, so a stored id the journal has run far past is no longer a
        // replayable one. (The watcher channel's overflow detection is the
        // secondary net.)
        let last_event_id = stored_event_id.unwrap_or(0);
        let journal_replayable = should_replay_journal(
            self.kind,
            watcher::supports_event_replay(),
            status.scan_completed_at.is_some(),
            stored_event_id,
        );
        let current_id = if journal_replayable {
            watcher::current_event_id()
        } else {
            0
        };
        let journal_gap_too_wide = current_id > 0 && current_id > last_event_id + JOURNAL_GAP_THRESHOLD;

        let route = launch_route::launch_route(&launch_route::IndexOnDisk {
            scan_completed: status.scan_completed_at.is_some(),
            has_rows: IndexStore::get_entry_count(read_conn).is_ok_and(|count| count > 1),
            has_covered_branches: branches::any_persisted(read_conn),
            journal_replayable,
            journal_gap_too_wide,
            phased_first_index: phases::phased_first_index(),
        });

        match route {
            launch_route::LaunchRoute::ReplayTheJournal => {
                let gap = current_id.saturating_sub(last_event_id);
                log::info!(
                    "Startup: cold-start replay (last_event_id={last_event_id}, current={current_id}, gap={gap})"
                );
                self.start_replay(last_event_id, heal_pending)
            }
            launch_route::LaunchRoute::ScanTheVolume => {
                let trigger = if journal_gap_too_wide {
                    let gap = current_id.saturating_sub(last_event_id);
                    emit_rescan_notification(
                        self.events.as_ref(),
                        &self.volume_id,
                        RescanReason::StaleIndex,
                        format!(
                            "Stored last_event_id={last_event_id}, current system \
                             event_id={current_id}, gap={gap} \
                             (threshold={JOURNAL_GAP_THRESHOLD}). \
                             The app likely hasn't run for a long time."
                        ),
                    );
                    "stale index: journal gap too large"
                } else if status.scan_completed_at.is_some() {
                    // A COMPLETED index with no journal to replay: the walk brings
                    // it current in place (and (re)starts the `DriveWatcher`). This
                    // is the path a `LocalExternal` volume ALWAYS takes, plus every
                    // non-journaled case for the boot disk.
                    "rescan of existing index"
                } else {
                    // Only reachable with the phased-first-index switch off, which
                    // is what "restore the bulk-build path" means.
                    "incomplete previous scan"
                };
                log::info!("Startup: walking '{}' whole ({trigger})", self.volume_id);
                self.start_scan(trigger).map_err(|e| e.to_string())
            }
            launch_route::LaunchRoute::CoverInPhases | launch_route::LaunchRoute::RebuildThenCoverInPhases => {
                // Nothing has ever completed here, so the phase machine builds it:
                // the folders this user cares about first, then home, then the rest
                // of the drive, add-only the whole way. ❌ It only REGISTERS the
                // intent — the first walk has to wait for `resume_branch_watch`
                // (`state/startup.rs`), or last session's covered ground comes back
                // watched and never epoch-bumped, rendering as current when nothing
                // verified it.
                log::info!(
                    "Startup: covering '{}' in phases (no completed scan on record)",
                    self.volume_id
                );
                self.register_a_phased_start(match route {
                    launch_route::LaunchRoute::RebuildThenCoverInPhases => PhasedStart::RebuildFirst,
                    _ => PhasedStart::KeepTheRows,
                });
                Ok(())
            }
        }
    }

    /// Force a (re)scan of this volume, routed to the RIGHT scanner by the typed
    /// volume kind — exactly as `resume_or_scan` routes the startup scan.
    ///
    /// A trait-scanned volume (SMB/MTP) goes to `start_volume_scan` (the
    /// `Volume`-trait walk from the share/storage root); a `Local` volume goes to
    /// `start_scan` (the guarded walker + FSEvents from `/`). This is the manual-rescan entry
    /// point behind `state::force_scan` / the "Rescan now" menu. Routing by kind
    /// HERE (not unconditionally calling `start_scan`) is what keeps an SMB/MTP
    /// rescan from running the local guarded walker over a network mount — which
    /// walked nothing in ~2 ms and falsely marked the index complete (the
    /// real-hardware "rescan does nothing to the NAS" bug). Classifies by the
    /// typed `kind`, never a volume-id substring.
    pub fn force_rescan(&mut self, scan_trigger: &str) -> Result<(), ScanStartError> {
        match rescan_scanner_for_kind(self.kind) {
            RescanScanner::VolumeTrait => {
                self.start_volume_scan(super::network_scan::NetworkScanMode::Auto, scan_trigger)
            }
            RescanScanner::LocalWalker => self.cover_or_scan(scan_trigger),
        }
    }

    /// Stop the active full scan and watcher.
    pub fn stop_scan(&mut self) {
        set_phase_for(self.events.as_ref(), &self.volume_id, ActivityPhase::Idle, "stopped");

        // A volume covered in phases has no `ScanHandle` to cancel; stopping it is
        // stopping the machine. Covered ground stays covered and watched.
        self.stop_phases();

        if let Some(ref handle) = self.scan_handle {
            handle.cancel();
        }
        self.scan_handle = None;
        self.scanning.store(false, Ordering::Relaxed);

        // Stop the FSEvents watcher
        if let Some(ref mut watcher) = self.drive_watcher {
            watcher.stop();
        }
        self.drive_watcher = None;
        self.branch_watched = false;

        DEBUG_STATS.reset();

        // Abort the live event processing task
        {
            let mut guard = self.live_event_task.lock_ignore_poison();
            if let Some(task) = guard.take() {
                task.abort();
            }
        }

        // Stopping a SCAN must not silently retire a branch watch that was never
        // part of it. A volume with walk-covered branches gets its watcher back
        // here; one whose scan just stopped has no branches (the scan retired
        // them), so this is a no-op there.
        self.ensure_branch_watch(false);
    }

    /// Get the current index status.
    pub fn get_status(&self) -> Result<IndexStatusResponse, String> {
        let index_status = self
            .store
            .get_index_status()
            .map_err(|e| format!("Failed to get index status: {e}"))?;

        let db_file_size = self.store.db_file_size().ok();

        // A phased run has no `ScanHandle`; its counters live on the machine, and
        // its `scanning` is "the machine has work", never "a walk is running right
        // now" (which goes false between frontier roots, 50-150 times a phase).
        let snap = self
            .scan_handle
            .as_ref()
            .map(|h| h.progress.snapshot())
            .or_else(|| self.phases.as_ref().map(|phases| phases.progress().snapshot()));
        let counters = live_scan_counters(snap, self.scan_calibration);
        let scanning = self.scanning.load(Ordering::Relaxed) || self.phases_have_work();

        // What the walker holds right now. A whole-volume run holds the volume
        // root (the same ground it announced at start), a phased one holds the
        // frontier root it is on, and neither holds anything between walks. The
        // events are what a live frontend follows; this answers the same question
        // for one that just reloaded.
        let walked_roots = match self.phases.as_ref() {
            Some(phases) if phases.has_work() => phases.walked_roots(),
            _ if self.scanning.load(Ordering::Relaxed) => vec![self.volume_root.to_string_lossy().into_owned()],
            _ => Vec::new(),
        };

        Ok(IndexStatusResponse {
            initialized: true,
            scanning,
            covered_in_phases: self.phases_have_work(),
            walked_roots,
            // Same question again, for the header rather than the hourglass:
            // which phase is running. Read only while the machine has work, so a
            // finished run reports nothing rather than the phase it ended on.
            coverage_phase: self
                .phases
                .as_ref()
                .filter(|phases| phases.has_work())
                .and_then(|phases| phases.coverage_phase()),
            entries_scanned: counters.entries_scanned,
            dirs_found: counters.dirs_found,
            bytes_scanned: counters.bytes_scanned,
            index_status: Some(index_status),
            db_file_size,
            volume_used_bytes: counters.volume_used_bytes,
            scan_run_kind: counters.scan_run_kind,
            prior_total_entries: counters.prior_total_entries,
            prior_scan_duration_ms: counters.prior_scan_duration_ms,
        })
    }

    /// Get extended debug status including live DB counts and event stats.
    pub fn get_debug_status(&self) -> Result<IndexDebugStatusResponse, String> {
        let base = self.get_status()?;
        let conn = self.store.read_conn();

        let live_entry_count = IndexStore::get_entry_count(conn).ok();
        let live_dir_count = IndexStore::get_dir_count(conn).ok();
        let dirs_with_stats = IndexStore::get_dirs_with_stats_count(conn).ok();

        let recent_must_scan_paths = DEBUG_STATS
            .recent_must_scan_paths
            .lock()
            .map(|p| p.clone())
            .unwrap_or_default();

        let (activity_phase, phase_started_at, phase_duration_ms, phase_history) = Self::read_phase_timeline();

        let db_main_size = self.store.db_main_size().ok();
        let db_wal_size = self.store.db_wal_size().ok();
        let (db_page_count, db_freelist_count) = IndexStore::db_page_stats(conn)
            .map(|(p, f)| (Some(p), Some(f)))
            .unwrap_or((None, None));

        Ok(IndexDebugStatusResponse {
            base,
            watcher_active: DEBUG_STATS.watcher_active.load(Ordering::Relaxed),
            live_event_count: DEBUG_STATS.live_event_count.load(Ordering::Relaxed),
            must_scan_count: DEBUG_STATS.must_scan_sub_dirs_count.load(Ordering::Relaxed),
            must_scan_rescans_completed: DEBUG_STATS.must_scan_rescans_completed.load(Ordering::Relaxed),
            live_entry_count,
            live_dir_count,
            dirs_with_stats,
            recent_must_scan_paths,
            activity_phase,
            phase_started_at,
            phase_duration_ms,
            phase_history,
            verifying: DEBUG_STATS.verifying.load(Ordering::Relaxed),
            verify_declined_dirs: DEBUG_STATS.verify_declined_dirs.load(Ordering::Relaxed),
            verify_truncated_dirs: DEBUG_STATS.verify_truncated_dirs.load(Ordering::Relaxed),
            reconcile_budget_subtrees: DEBUG_STATS.reconcile_budget_subtrees.load(Ordering::Relaxed),
            reconcile_budget_skipped_dirs: DEBUG_STATS.reconcile_budget_skipped_dirs.load(Ordering::Relaxed),
            db_main_size,
            db_wal_size,
            db_page_count,
            db_freelist_count,
        })
    }

    /// Read the current phase timeline from DebugStats.
    pub(crate) fn read_phase_timeline() -> (ActivityPhase, String, u64, Vec<PhaseRecord>) {
        let history = DEBUG_STATS.phase_history.lock().map(|h| h.clone()).unwrap_or_default();

        let (activity_phase, phase_started_at) = history
            .last()
            .map(|r| (r.phase.clone(), r.started_at.clone()))
            .unwrap_or((ActivityPhase::Idle, String::new()));

        let phase_duration_ms = DEBUG_STATS
            .phase_started
            .lock()
            .ok()
            .and_then(|s| s.map(|i| i.elapsed().as_millis() as u64))
            .unwrap_or(0);

        (activity_phase, phase_started_at, phase_duration_ms, history)
    }

    /// Return the DB file path for this index.
    pub fn db_path(&self) -> &Path {
        self.store.db_path()
    }

    /// Shut down the indexing system gracefully.
    ///
    /// Sequence: stop watcher (closes the channel sender) → wait for the event
    /// loop to drain its final batch and send `UpdateLastEventId` → shut down
    /// the writer. This ensures `last_event_id` is up-to-date on next restart.
    pub fn shutdown(&mut self) {
        set_phase_for(self.events.as_ref(), &self.volume_id, ActivityPhase::Idle, "shutdown");

        // 1. Cancel everything running for this volume — the active scan and every
        //    child operation under it (subtree verifications, a reconcile walk).
        //    Unlike `stop_scan`, this is terminal: no later scan starts here.
        self.volume_cancel.cancel();
        self.stop_phases();
        self.scan_handle = None;
        self.scanning.store(false, Ordering::Relaxed);

        // 2. Stop the watcher. Dropping the sender closes the channel, which causes event_rx.recv() to
        //    return None in the event loop.
        if let Some(ref mut watcher) = self.drive_watcher {
            watcher.stop();
        }
        self.drive_watcher = None;
        self.branch_watched = false;

        // 3. Wait for the event loop to drain (process final batch + UpdateLastEventId). Use block_in_place
        //    so we can .await the join handle without blocking the tokio runtime thread pool.
        let task = self.live_event_task.lock_ignore_poison().take();
        if let Some(task) = task {
            tokio::task::block_in_place(|| {
                crate::indexing::host::runtime::block_on(async {
                    match tokio::time::timeout(Duration::from_secs(5), task).await {
                        Ok(Ok(())) => log::debug!("Live event loop drained successfully"),
                        Ok(Err(e)) => log::debug!("Live event loop task error: {e}"),
                        Err(_) => log::warn!("Live event loop drain timed out after 5s"),
                    }
                });
            });
        }

        // 4. Now shut down the writer (all final writes have been queued)
        self.writer.shutdown();

        log::info!("IndexManager: shut down for volume '{}'", self.volume_id);
    }
}

mod launch_route;
mod phased;
mod start;

pub(in crate::indexing::lifecycle) use phased::{PendingPhases, PhaseResume, PhasedStart};

#[cfg(test)]
mod tests;

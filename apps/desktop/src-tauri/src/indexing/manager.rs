//! `IndexManager`: central coordinator for the drive indexing system.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::AppHandle;
use tauri_specta::Event;

use super::event_loop::{
    JOURNAL_GAP_THRESHOLD, ReplayConfig, WATCHER_CHANNEL_CAPACITY, run_live_event_loop, run_replay_event_loop,
};
use super::events::{
    ActivityPhase, DEBUG_STATS, IndexAggregationCompleteEvent, IndexDebugStatusResponse, IndexDirUpdatedEvent,
    IndexScanCompleteEvent, IndexScanStartedEvent, IndexStatusResponse, PhaseRecord, RescanReason,
    emit_rescan_notification, set_phase_for,
};
use super::local_reconcile;
use super::progress_reporter::ScanProgressReporter;
use super::reconciler::{self, EventReconciler};
use super::scanner::{self, ScanConfig};
use super::state::{INDEX_REGISTRY, IndexPhase, IndexVolumeKind};
use super::store::IndexStore;
use super::watcher::{self, DriveWatcher};
use super::writer::{IndexWriter, PartialAggSource, WriteMessage};
use crate::ignore_poison::IgnorePoison;
use crate::pluralize::pluralize;

// ── IndexManager ─────────────────────────────────────────────────────

/// Central coordinator for the drive indexing system.
///
/// Owns the SQLite store (reads), the writer thread (writes), and the scanner handle.
/// Accessed by module-level functions that lock the `INDEXING` static.
pub(crate) struct IndexManager {
    /// Volume ID (for example, "root" for /)
    pub(super) volume_id: String,
    /// What kind of volume this is, which selects the scan strategy (jwalk +
    /// FSEvents for `Local`, the `Volume`-trait scanner with no journal for
    /// `Smb`) and the launch-time freshness. See `IndexVolumeKind`.
    pub(super) kind: IndexVolumeKind,
    /// Volume root path
    pub(super) volume_root: PathBuf,
    /// SQLite store for reads
    pub(super) store: IndexStore,
    /// Writer handle for sending writes
    pub(super) writer: IndexWriter,
    /// Handle to the active full scan (if running)
    pub(super) scan_handle: Option<scanner::ScanHandle>,
    /// FSEvents watcher (started alongside scan, persists after scan completes)
    drive_watcher: Option<DriveWatcher>,
    /// Live event processing task (runs after reconciliation completes).
    /// Shared with spawned async tasks so they can store the handle.
    live_event_task: Arc<std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    /// Tauri app handle for emitting events
    pub(super) app: AppHandle,
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
    /// The prior completed scan's persisted totals (tier-1 denominator + ETA seed).
    pub(super) prior: super::store::ScanCalibration,
    /// The scanned volume's used bytes at scan start (tier-2 denominator). `None`
    /// when the space-info fetch failed; never blocks or delays the scan.
    pub(super) volume_used_bytes: Option<u64>,
}

/// The live scan-progress fields `get_status` surfaces on `IndexStatusResponse`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LiveScanCounters {
    entries_scanned: u64,
    dirs_found: u64,
    bytes_scanned: u64,
    volume_used_bytes: Option<u64>,
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
    }
}

/// Which scanner a forced (re)scan must use for a volume of a given kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RescanScanner {
    /// The `Volume`-trait walk from the share/storage root (`start_volume_scan`).
    /// SMB and MTP — there is no local filesystem to jwalk and no FSEvents journal.
    VolumeTrait,
    /// The jwalk + FSEvents walk from the volume root (`start_scan`). Local disk only.
    LocalJwalk,
}

/// Pure routing for `force_rescan`: pick the scanner by the TYPED volume kind.
///
/// This is the regression anchor for the real-hardware bug where a NAS "Rescan
/// now" ran the local jwalk scanner over the SMB mount (walking nothing, then
/// falsely marking the index complete). A trait-scanned kind (SMB/MTP) must map
/// to `VolumeTrait`; only `Local` maps to `LocalJwalk`. Mirrors
/// `resume_or_scan`'s `is_trait_scanned` routing, kept as a separate pure
/// function so the dispatch is unit-testable without an `AppHandle`.
fn rescan_scanner_for_kind(kind: IndexVolumeKind) -> RescanScanner {
    if kind.is_trait_scanned() {
        RescanScanner::VolumeTrait
    } else {
        RescanScanner::LocalJwalk
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
/// parallel jwalk bulk build.
///
/// `prior_scan_completed` (the completeness gate): reconcile ONLY a previously
/// COMPLETED index (`scan_completed_at` was present at scan start). A partial that
/// never finished (first scan interrupted, or repeated mid-scan quits) takes the
/// fast truncate + parallel-jwalk rebuild instead. Reason: reconcile's per-dir
/// serial walk plus its add-everything delta is far slower than a parallel bulk
/// rebuild when the existing index is only a small fraction complete — a 4%-complete
/// partial made the app look hung for ~15 min on a real `/`. Reconcile is the right
/// call only when the index is substantially complete (a rescan with a small delta:
/// sizes stay visible, no freelist). A tiny partial is mostly `<dir>` anyway, so
/// keeping it "visible" buys little, and jwalk is fast. (LOCAL-only; the network
/// predicate stays unchanged — a NAS rescan is slow, so keeping the partial visible
/// is worth more there, and network partials are small.)
///
/// Pure so the boundary is unit-testable without an `AppHandle`.
fn local_rescan_reconciles(entry_count: u64, prior_scan_completed: bool) -> bool {
    entry_count > 1 && prior_scan_completed
}

impl IndexManager {
    /// Create a new IndexManager for a volume of the given kind.
    ///
    /// Opens (or creates) the SQLite database, spawns the writer thread, and
    /// records the volume kind so `resume_or_scan` picks the right scan strategy.
    pub fn new_for_kind(
        volume_id: String,
        volume_root: PathBuf,
        app: AppHandle,
        kind: IndexVolumeKind,
        freshness: Arc<std::sync::Mutex<Option<super::freshness::Freshness>>>,
    ) -> Result<Self, String> {
        let data_dir = crate::config::resolved_app_data_dir(&app)?;

        let db_path = data_dir.join(format!("index-{volume_id}.db"));

        let store = IndexStore::open(&db_path).map_err(|e| format!("Failed to open index store: {e}"))?;

        // Only the search-feeding volume's writer bumps the global
        // `WRITER_GENERATION`. Search is single-volume by construction (D7): it
        // loads exactly one in-memory index off `root`'s (local-disk) DB. An
        // SMB/MTP writer must not invalidate the root search index it doesn't
        // feed, or every NAS/phone change-notify event would thrash a full root
        // search reload. See `writer::WRITER_GENERATION` and `indexing/DETAILS.md`.
        let feeds_search = kind == IndexVolumeKind::Local;
        let writer = IndexWriter::spawn_for(&db_path, Some(app.clone()), feeds_search, volume_id.clone())
            .map_err(|e| format!("Failed to spawn index writer: {e}"))?;

        log::debug!(
            "IndexManager created for volume '{volume_id}' ({kind:?}) at {}",
            volume_root.display()
        );

        Ok(Self {
            volume_id,
            kind,
            volume_root,
            store,
            writer,
            scan_handle: None,
            drive_watcher: None,
            live_event_task: Arc::new(std::sync::Mutex::new(None)),
            app,
            scanning: Arc::new(AtomicBool::new(false)),
            freshness,
            scan_calibration: None,
        })
    }

    /// Resume from an existing index or start a fresh full scan.
    ///
    /// **macOS (with event replay support):**
    /// If an existing index exists (`scan_completed_at` is set in meta) and we have a
    /// stored `last_event_id`, start the FSEvents watcher with `sinceWhen = last_event_id`
    /// to replay the journal. If the journal is unavailable, fall back to a full scan.
    ///
    /// **Linux (no event replay):**
    /// Always does a full scan on startup. The existing index DB is kept as-is for
    /// instant enrichment; the scan overwrites stale entries. The watcher starts
    /// alongside the scan for live events.
    ///
    /// **No existing index:** Full scan via `start_scan()`.
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

        // Event ID replay is only available on macOS (FSEvents journal).
        // On Linux (inotify), always rescan -- there's no journal to replay.
        if watcher::supports_event_replay() && status.scan_completed_at.is_some() {
            if let Some(ref last_event_id_str) = status.last_event_id {
                let last_event_id: u64 = last_event_id_str.parse().unwrap_or(0);
                if last_event_id > 0 {
                    // Pre-check: compare stored event ID with current system event ID.
                    // If the gap is too large, skip replay entirely. Replaying tens of
                    // millions of events is slower than a fresh scan. The watcher channel
                    // (32K capacity) has overflow detection as a secondary safety net.
                    let current_id = watcher::current_event_id();
                    if current_id > 0 && current_id > last_event_id + JOURNAL_GAP_THRESHOLD {
                        let gap = current_id - last_event_id;
                        emit_rescan_notification(
                            &self.app,
                            &self.volume_id,
                            RescanReason::StaleIndex,
                            format!(
                                "Stored last_event_id={last_event_id}, current system \
                                 event_id={current_id}, gap={gap} \
                                 (threshold={JOURNAL_GAP_THRESHOLD}). \
                                 The app likely hasn't run for a long time."
                            ),
                        );
                        return self.start_scan("stale index: journal gap too large");
                    }

                    let current_id = watcher::current_event_id();
                    let gap = current_id.saturating_sub(last_event_id);
                    log::info!(
                        "Startup: cold-start replay (last_event_id={last_event_id}, current={current_id}, gap={gap})",
                    );
                    return self.start_replay(last_event_id);
                }
            }
            log::info!("Startup: fresh scan (existing index has no last_event_id)");
        } else if status.scan_completed_at.is_some() {
            log::info!("Startup: full rescan (no event replay on this platform)");
        } else if status.last_event_id.is_some() {
            emit_rescan_notification(
                &self.app,
                &self.volume_id,
                RescanReason::IncompletePreviousScan,
                "Index DB exists but scan_completed_at is not set. Previous scan likely didn't \
                 finish."
                    .to_string(),
            );
        } else {
            log::info!("Startup: fresh scan (no existing index)");
        }

        // Determine the trigger string for the scan phase
        let trigger = if status.last_event_id.is_some() && status.scan_completed_at.is_none() {
            "incomplete previous scan"
        } else if status.scan_completed_at.is_some() {
            "full rescan (no event replay on this platform)"
        } else {
            "fresh scan"
        };
        self.start_scan(trigger)
    }

    /// Force a (re)scan of this volume, routed to the RIGHT scanner by the typed
    /// volume kind — exactly as `resume_or_scan` routes the startup scan.
    ///
    /// A trait-scanned volume (SMB/MTP) goes to `start_volume_scan` (the
    /// `Volume`-trait walk from the share/storage root); a `Local` volume goes to
    /// `start_scan` (jwalk + FSEvents from `/`). This is the manual-rescan entry
    /// point behind `state::force_scan` / the "Rescan now" menu. Routing by kind
    /// HERE (not unconditionally calling `start_scan`) is what keeps an SMB/MTP
    /// rescan from running the local jwalk scanner over a network mount — which
    /// walked nothing in ~2 ms and falsely marked the index complete (the
    /// real-hardware "rescan does nothing to the NAS" bug). Classifies by the
    /// typed `kind`, never a volume-id substring.
    pub fn force_rescan(&mut self, scan_trigger: &str) -> Result<(), String> {
        match rescan_scanner_for_kind(self.kind) {
            RescanScanner::VolumeTrait => self.start_volume_scan(scan_trigger),
            RescanScanner::LocalJwalk => self.start_scan(scan_trigger),
        }
    }

    /// Resume from an existing index by replaying FSEvents journal since `since_event_id`.
    ///
    /// Starts the watcher with `sinceWhen = since_event_id`. The watcher replays
    /// journal events which are processed as live events. If the journal is
    /// unavailable (gap detected), falls back to a full scan.
    fn start_replay(&mut self, since_event_id: u64) -> Result<(), String> {
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(WATCHER_CHANNEL_CAPACITY);
        let current_id = watcher::current_event_id();

        let watcher_overflow: Option<Arc<AtomicBool>>;
        match DriveWatcher::start(&self.volume_root, since_event_id, event_tx) {
            Ok(watcher) => {
                watcher_overflow = Some(watcher.overflow_flag());
                self.drive_watcher = Some(watcher);
                DEBUG_STATS.watcher_active.store(true, Ordering::Relaxed);
                let gap = current_id.saturating_sub(since_event_id);
                set_phase_for(
                    &self.app,
                    &self.volume_id,
                    ActivityPhase::Replaying,
                    &format!("app launch, ~{}", pluralize(gap, "pending FSEvent")),
                );
                log::info!("Replay: watcher started (since_event_id={since_event_id}, current={current_id})");
            }
            Err(e) => {
                emit_rescan_notification(
                    &self.app,
                    &self.volume_id,
                    RescanReason::WatcherStartFailed,
                    format!("DriveWatcher failed to start for replay: {e}"),
                );
                return self.start_scan("watcher failed to start for replay");
            }
        }

        // Estimated total events for progress reporting (approximate: not all IDs
        // in the range belong to our volume)
        let estimated_total = if current_id > since_event_id {
            Some(current_id - since_event_id)
        } else {
            None
        };

        // Suppress verifier until replay completes. The spawned task resets
        // this to false when replay is done (or on fallback to full scan).
        self.scanning.store(true, Ordering::Relaxed);

        // Spawn the replay event processing loop
        let writer = self.writer.clone();
        let app = self.app.clone();
        let volume_id = self.volume_id.clone();
        let live_event_task_slot = Arc::clone(&self.live_event_task);
        let scanning = Arc::clone(&self.scanning);

        // The fallback task (below) re-resolves this manager in the registry by
        // volume id, so keep a clone for it before `volume_id` is moved into the
        // replay loop task.
        let fallback_volume_id = self.volume_id.clone();

        // We need a way for the replay loop to signal "journal unavailable, need full scan".
        // Use a oneshot channel: if the replay detects a gap, it sends a signal.
        let (fallback_tx, fallback_rx) = tokio::sync::oneshot::channel::<()>();

        // Use tauri::async_runtime::spawn because indexing can start from the
        // synchronous Tauri setup() hook where no Tokio runtime context exists.
        // Store the handle so shutdown() can wait for it to drain.
        let handle = tauri::async_runtime::spawn(async move {
            let result = run_replay_event_loop(
                event_rx,
                writer.clone(),
                app.clone(),
                ReplayConfig {
                    volume_id: volume_id.clone(),
                    since_event_id,
                    estimated_total,
                },
                fallback_tx,
                watcher_overflow,
                Arc::clone(&scanning),
            )
            .await;

            // Live event loop ended (shutdown). Clear scanning as a safety net
            // (normally cleared inside run_replay_event_loop after replay phase).
            scanning.store(false, Ordering::Relaxed);

            if let Err(e) = result {
                log::warn!("Replay event loop error: {e}");
            }
        });
        {
            let mut guard = live_event_task_slot.lock_ignore_poison();
            *guard = Some(handle);
        }

        // Spawn a task that watches for the fallback signal and triggers a full scan if needed.
        tauri::async_runtime::spawn(async move {
            if fallback_rx.await.is_ok() {
                log::warn!("Journal replay detected gap, initiating full scan fallback");

                // Take the manager OUT of the registry (transient `ShuttingDown`)
                // so the blocking `start_scan` prelude runs OFF the registry lock.
                // Holding the lock across `start_scan`'s `flush_blocking` +
                // space-info query would freeze every concurrent registry user;
                // the freshness firing inside `start_scan` would also re-lock the
                // registry (now fired through the manager's own freshness `Arc`).
                // Mirrors `state::force_scan`'s extract-drop-run-reinsert flow.
                let mut mgr = {
                    let mut reg = match INDEX_REGISTRY.lock() {
                        Ok(g) => g,
                        Err(e) => {
                            log::warn!("Failed to lock registry for fallback scan: {e}");
                            return;
                        }
                    };
                    let Some(instance) = reg.get_mut(&fallback_volume_id) else {
                        return;
                    };
                    // `mgr` is the `Box<IndexManager>` taken out of `Running`.
                    match std::mem::replace(&mut instance.phase, IndexPhase::ShuttingDown) {
                        IndexPhase::Running(mut mgr) => {
                            // Stop the current watcher (replay detected it's useless)
                            // while still under the lock — these are non-blocking.
                            if let Some(ref mut watcher) = mgr.drive_watcher {
                                watcher.stop();
                            }
                            mgr.drive_watcher = None;
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

                // Guard released: run the blocking-prelude scan start off the lock.
                let result = mgr.start_scan("journal replay detected gap");
                if let Err(ref e) = result {
                    log::warn!("Fallback full scan failed: {e}");
                }

                // Re-lock to restore the manager as `Running`. If the volume was
                // torn down while we were detached, shut the orphaned manager down
                // instead of resurrecting a removed volume.
                let mut reg = match INDEX_REGISTRY.lock() {
                    Ok(g) => g,
                    Err(e) => {
                        log::warn!("Failed to re-lock registry after fallback scan: {e}");
                        mgr.shutdown();
                        return;
                    }
                };
                match reg.get_mut(&fallback_volume_id) {
                    Some(instance) if matches!(instance.phase, IndexPhase::ShuttingDown) => {
                        instance.phase = IndexPhase::Running(mgr);
                    }
                    _ => {
                        drop(reg);
                        log::info!(
                            "fallback scan: '{fallback_volume_id}' was torn down during scan start; shutting down the manager"
                        );
                        mgr.shutdown();
                    }
                }
            }
        });

        Ok(())
    }

    /// Start a full volume scan with concurrent FSEvents watching.
    ///
    /// Flow:
    /// 1. Start DriveWatcher (sinceWhen=0) to buffer events during the scan
    /// 2. Record scan-start event ID
    /// 3. Start the full scan
    /// 4. On scan completion: replay buffered events, switch to live mode
    /// 5. Live events processed continuously until shutdown
    pub fn start_scan(&mut self, scan_trigger: &str) -> Result<(), String> {
        if self.scanning.load(Ordering::Relaxed) {
            return Err("Scan already running".to_string());
        }

        // Step 0: Capture this scan's calibration BEFORE truncating.
        //
        // The prior completed scan's totals are read straight off the live read
        // connection: the calibration keys survive `TruncateData` (it preserves
        // `meta`), but reading first keeps the data flow obviously correct — we
        // snapshot the previous scan's numbers before the truncate touches anything.
        let prior = IndexStore::read_scan_calibration(self.store.read_conn()).unwrap_or_else(|e| {
            log::warn!("Failed to read prior scan calibration (tier-1 will degrade): {e}");
            super::store::ScanCalibration::default()
        });

        // The completeness gate for reconcile-vs-truncate (see `local_rescan_reconciles`):
        // snapshot whether the prior scan COMPLETED, read BEFORE `DeleteMeta` clears
        // `scan_completed_at` below. A partial that never finished must NOT reconcile
        // (its add-everything delta wedges the serial walk); it takes the fast jwalk
        // rebuild instead.
        let prior_scan_completed = self
            .store
            .get_index_status()
            .map(|s| s.scan_completed_at.is_some())
            .unwrap_or(false);

        // Fetch the scanned volume's used bytes ONCE (tier-2 denominator). The call
        // does disk I/O — an NSURL XPC round-trip on macOS, `statvfs` on Linux — and
        // `start_scan` runs in async contexts (the auto-start spawn, async Tauri
        // commands), so wrap it in `block_in_place`, matching the `flush_blocking`
        // call below. A bare blocking call on a tokio worker can stall on a wedged
        // mount. Failure → `None`; never block or delay the scan for the denominator.
        let volume_root = self.volume_root.clone();
        let volume_used_bytes = tokio::task::block_in_place(|| {
            crate::file_system::volume::backends::get_space_info_for_path(&volume_root)
                .map(|info| info.used_bytes)
                .map_err(|e| log::warn!("Failed to read volume used bytes (tier-2 will degrade): {e}"))
                .ok()
        });

        let calibration = ScanCalibration {
            prior,
            volume_used_bytes,
        };
        self.scan_calibration = Some(calibration);

        // Step 0a: Clear the previous scan's completion marker BEFORE truncating.
        // Without this, a rescan killed mid-way (power loss, `kill -9`) leaves the
        // PREVIOUS scan's `scan_completed_at` in meta on top of a truncated/partial
        // `entries` table, so the next startup takes the journal-replay path over a
        // gutted index instead of the `IncompletePreviousScan` fresh rescan. The
        // calibration keys (`total_entries`, `total_physical_bytes`, `scan_duration_ms`)
        // are intentionally left intact so they keep describing the last COMPLETED
        // scan throughout this one. The same flush below covers both sends.
        if let Err(e) = self
            .writer
            .send(WriteMessage::DeleteMeta("scan_completed_at".to_string()))
        {
            log::warn!("Failed to send DeleteMeta(scan_completed_at): {e}");
        }

        // Step 0a': Bump `current_epoch` at the scan-start funnel. Every full
        // (re)scan funnels through here regardless of trigger (journal-gap, stale,
        // overflow, force_scan), so bumping once covers them all without
        // enumerating `RescanReason` (those are FE-toast notifications, not
        // control-flow points). The first-ever scan bumps 1→2 (benign). The
        // flush below (Step 0b) commits it BEFORE the scan thread reads
        // `current_epoch` on its own connection — else the walk stamps the stale
        // epoch. (Local is journaled, so a Fresh-on-launch load skips this funnel
        // entirely and doesn't bump; only an actual rescan does.)
        if let Err(e) = self.writer.send(WriteMessage::BumpCurrentEpoch) {
            log::warn!("Failed to send BumpCurrentEpoch: {e}");
        }

        // Step 0a'': Reconcile vs truncate. A previously-COMPLETED, populated index
        // (rows beyond the ROOT sentinel) is RESCANNED in place by `local_reconcile`
        // (diff each dir, write only changes) so the last-good directory sizes stay
        // visible (stale) throughout and no large freelist is minted. A first/empty
        // scan OR a never-completed partial keeps the fast parallel jwalk bulk build
        // (see `local_rescan_reconciles` for the completeness gate). Read the entry
        // count from the live read connection BEFORE any truncate. (NOTE: the network
        // predicate in `network_scan.rs` is intentionally left unchanged.)
        let reconcile = IndexStore::get_entry_count(self.store.read_conn())
            .map(|n| local_rescan_reconciles(n, prior_scan_completed))
            .unwrap_or(false);

        // Step 0b: Truncate entries + dir_stats so a FRESH scan inserts into an empty
        // DB. Without this, INSERT OR REPLACE on a populated table with the
        // `platform_case` collation is ~12x slower (30 min vs 2.5 min), and old rows
        // with stale IDs accumulate as orphaned subtrees, bloating the DB 3-4x per
        // scan cycle. A RECONCILE skips ONLY the truncate (the whole point is to never
        // blank the index); the `BumpCurrentEpoch` above and the flush below stay
        // unconditional, so the walker reads the bumped `current_epoch` on its own
        // read connection (else it stamps the stale epoch).
        if !reconcile && let Err(e) = self.writer.send(WriteMessage::TruncateData) {
            log::warn!("Failed to send TruncateData: {e}");
        }
        if let Err(e) = tokio::task::block_in_place(|| self.writer.flush_blocking()) {
            log::warn!("Failed to flush before scan: {e}");
        }

        // Step 1: Start the FSEvents watcher BEFORE the scan so we don't miss events
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(WATCHER_CHANNEL_CAPACITY);
        let scan_start_event_id = watcher::current_event_id();

        // In E2E mode, scope the watcher to the fixture directory instead of `/`.
        // On Linux, inotify's RecursiveMode::Recursive adds a watch per subdirectory,
        // so watching `/` blocks for minutes on a container with thousands of dirs.
        let watcher_root = std::env::var("CMDR_E2E_START_PATH")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.volume_root.clone());

        // watcher_overflow is None if the watcher failed to start (non-fatal).
        let watcher_overflow: Option<Arc<AtomicBool>>;
        match DriveWatcher::start(&watcher_root, 0, event_tx) {
            Ok(watcher) => {
                watcher_overflow = Some(watcher.overflow_flag());
                self.drive_watcher = Some(watcher);
                DEBUG_STATS.watcher_active.store(true, Ordering::Relaxed);
                log::info!("Scan: watcher started (scan_start_event_id={scan_start_event_id})");
            }
            Err(e) => {
                watcher_overflow = None;
                // Watcher failure is non-fatal: scan works without it, just no live updates
                log::warn!("Failed to start DriveWatcher (scan will proceed without watcher): {e}");
            }
        }

        // Emit started event with the static, per-scan calibration. Static values
        // ride this event once; the 500 ms progress event carries only the moving
        // counters, so the FE never re-receives constants. The tier decision
        // (calibrated vs rough) is then a pure function of this one event.
        let _ = IndexScanStartedEvent {
            volume_id: self.volume_id.clone(),
            prior_total_entries: calibration.prior.total_entries,
            prior_scan_duration_ms: calibration.prior.scan_duration_ms,
            volume_used_bytes: calibration.volume_used_bytes,
        }
        .emit(&self.app);

        set_phase_for(&self.app, &self.volume_id, ActivityPhase::Scanning, scan_trigger);

        // Freshness ⇒ Scanning (blue). For local `root` this also drives the
        // per-drive badge; the clean-completion handler flips it back
        // to Fresh. (Root is journaled, so a restart re-seeds Fresh; this keeps
        // the badge honest DURING a scan/rescan.) Fire through the manager's OWN
        // freshness handle (`apply_freshness_event_on`), NOT the volume-id lookup:
        // `force_scan` (and the journal-gap fallback) call `start_scan` while
        // holding the registry lock, so a registry re-lock here deadlocks.
        super::state::apply_freshness_event_on(
            &self.freshness,
            &self.volume_id,
            super::freshness::FreshnessEvent::ScanStarted,
        );

        // Step 2: Start the walk. A reconcile rescan runs the serial full-tree
        // `local_reconcile` walker (returns the SAME `(ScanHandle, JoinHandle)` shape
        // as `scan_volume`, runs on a `std::thread`, does its marks + single aggregate
        // in-thread), so the completion handler below is reused literally unchanged. A
        // fresh scan runs the fast parallel jwalk `scan_volume`.
        let (scan_handle, join_handle) = if reconcile {
            log::info!("local scan: reconcile rescan for '{}' ({scan_trigger})", self.volume_id);
            local_reconcile::start_local_reconcile(self.volume_root.clone(), &self.writer)
                .map_err(|e| format!("Failed to start reconcile rescan: {e}"))?
        } else {
            log::info!(
                "local scan: fresh scan (truncate) for '{}' ({scan_trigger})",
                self.volume_id
            );
            let config = ScanConfig {
                root: self.volume_root.clone(),
                ..ScanConfig::default()
            };
            scanner::scan_volume(config, &self.writer).map_err(|e| format!("Failed to start scan: {e}"))?
        };

        self.scanning.store(true, Ordering::Relaxed);

        // Shared flag: set to true when the scan finishes (or fails/panics), so the
        // progress reporter loop exits. The completion handler below sets it.
        let scan_done = Arc::new(AtomicBool::new(false));

        // Spawn the 500 ms progress reporter: it emits `index-scan-progress` events
        // and drives mid-scan partial aggregation, running until `scan_done` is set
        // by the completion handler. The tick loop lives in `progress_reporter`.
        // Source by scan kind: a RECONCILE rescan leaves the accumulator maps empty
        // (it's all `UpsertEntryV2`), so it must recompute partial sizes from
        // committed rows (`Sql`); a FRESH jwalk scan populates the maps (`Maps`).
        let partial_agg_source = if reconcile {
            PartialAggSource::Sql
        } else {
            PartialAggSource::Maps
        };
        ScanProgressReporter::new(
            Arc::clone(&scan_handle.progress),
            self.writer.clone(),
            self.app.clone(),
            self.volume_id.clone(),
            partial_agg_source,
        )
        .spawn(Arc::clone(&scan_done));

        // Step 3: Spawn completion handler that also does reconciliation.
        // Use tauri::async_runtime::spawn because indexing can start from the
        // synchronous Tauri setup() hook where no Tokio runtime context exists.
        let volume_id = self.volume_id.clone();
        let app = self.app.clone();
        let writer = self.writer.clone();
        let scanning = Arc::clone(&self.scanning);
        // Clone the freshness handle into the completion task so it fires
        // `ScanCompleted` through the `Arc` directly, never re-locking the registry.
        let freshness = Arc::clone(&self.freshness);
        let live_event_task_slot = Arc::clone(&self.live_event_task);
        let watcher_overflow_flag = watcher_overflow;
        tauri::async_runtime::spawn(async move {
            // Wait for scan to complete
            let join_result = tokio::task::spawn_blocking(move || join_handle.join()).await;

            // Signal the progress reporter to stop regardless of outcome
            scan_done.store(true, Ordering::Relaxed);
            // Reset scanning flag so get_status() reports correctly and new scans can start
            scanning.store(false, Ordering::Relaxed);

            // Flatten the outer Result (from spawn_blocking) and inner Result (from thread join)
            let result = match join_result {
                Ok(thread_result) => thread_result,
                Err(e) => {
                    log::warn!("Completion handler task failed: {e}");
                    return;
                }
            };

            match result {
                Ok(Ok(summary)) => {
                    log::info!(
                        "Scan: complete ({} entries, {} dirs, {:.1}s)",
                        summary.total_entries,
                        summary.total_dirs,
                        summary.duration_ms as f64 / 1000.0,
                    );

                    DEBUG_STATS.close_phase_with_stats(vec![
                        ("entries", summary.total_entries.to_string()),
                        ("dirs", summary.total_dirs.to_string()),
                        ("duration_s", format!("{:.1}", summary.duration_ms as f64 / 1000.0)),
                    ]);
                    set_phase_for(&app, &volume_id, ActivityPhase::Aggregating, "post-scan");

                    // Step 4: Reconcile buffered watcher events
                    let mut reconciler = EventReconciler::new();

                    // Drain all buffered events from the channel into the reconciler
                    let mut event_rx = event_rx;
                    let mut buffered_count = 0u64;
                    while let Ok(event) = event_rx.try_recv() {
                        reconciler.buffer_event(event);
                        buffered_count += 1;
                    }
                    log::info!(
                        "Reconciler: {} buffered during scan",
                        pluralize(buffered_count, "event")
                    );

                    if reconciler.did_buffer_overflow() {
                        emit_rescan_notification(
                            &app,
                            &volume_id,
                            RescanReason::ReconcilerBufferOverflow,
                            "The filesystem watcher buffered over 500,000 events during the \
                             scan, exceeding the reconciler's capacity. A lot of filesystem \
                             activity was happening during the scan."
                                .to_string(),
                        );
                    }

                    // Check if the FSEvents channel overflowed (events dropped
                    // before reaching the forward task). If so, our buffered events
                    // are incomplete. The reconciler replay will miss changes.
                    // We still proceed (the scan data itself is fine), but log a
                    // warning. The live event loop will detect the overflow flag
                    // and trigger a rescan at that point, since a fresh scan is
                    // the only way to recover from dropped events.
                    if let Some(ref flag) = watcher_overflow_flag
                        && flag.load(Ordering::Relaxed)
                    {
                        log::info!(
                            "FSEvents channel overflowed during scan. Some watcher \
                                 events were dropped. Live event loop will trigger a rescan."
                        );
                    }

                    // Emit scan-complete first, then start the flushing phase.
                    // Order matters: the frontend's scan-complete handler calls
                    // resetAggregation(), so the saving_entries event must come
                    // after to avoid being immediately cleared.
                    let _ = IndexScanCompleteEvent {
                        volume_id: volume_id.clone(),
                        total_entries: summary.total_entries,
                        total_dirs: summary.total_dirs,
                        duration_ms: summary.duration_ms,
                    }
                    .emit(&app);

                    // Tell the writer how many entries the scan produced, so it
                    // can report flushing progress as it drains remaining
                    // InsertEntriesV2 batches from the channel.
                    writer.set_expected_total_entries(summary.total_entries);

                    // Flush the writer to ensure all scan batches are committed
                    // before opening the read connection. Without this, the WAL
                    // snapshot may not include the latest InsertEntriesV2 batches,
                    // causing resolve_path to fail for recently-scanned parents.
                    if let Err(e) = writer.flush().await {
                        log::warn!("Reconciler: writer flush before replay failed: {e}");
                    }

                    // Signal that aggregation (and entry flushing) is complete.
                    // The flush above drains all queued writes including
                    // ComputeAllAggregates, so by this point the UI can dismiss
                    // the progress overlay.
                    let _ = IndexAggregationCompleteEvent {
                        volume_id: volume_id.clone(),
                    }
                    .emit(&app);

                    DEBUG_STATS.close_phase_with_stats(vec![]);
                    set_phase_for(&app, &volume_id, ActivityPhase::Reconciling, "post-scan");

                    // Tell the frontend to refresh all visible listings. Directory
                    // sizes are now available for the first time after a full scan.
                    let _ = IndexDirUpdatedEvent {
                        paths: vec!["/".to_string()],
                    }
                    .emit(&app);

                    // Store scan metadata now, before the reconciler replay which
                    // can fail (e.g. "database is locked") and cause an early return.
                    // Without this, scan_completed_at is never persisted and the next
                    // startup triggers a full rescan of the entire volume.
                    //
                    // Gate ALL meta writes behind `!was_cancelled`: a user-stopped scan
                    // holds only partial totals, and writing `scan_completed_at` for it
                    // would mark a partial index as complete — the next startup would skip
                    // the `IncompletePreviousScan` fresh rescan. With the clear-at-start
                    // above, a cancelled scan leaves NO completion marker, so it heals on
                    // restart. The reconcile/live transition below is intentionally NOT
                    // gated; only the meta writes are.
                    if !summary.was_cancelled {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs().to_string())
                            .unwrap_or_default();
                        let _ = writer.send(WriteMessage::UpdateMeta {
                            key: "scan_completed_at".to_string(),
                            value: now,
                        });
                        let _ = writer.send(WriteMessage::UpdateMeta {
                            key: "scan_duration_ms".to_string(),
                            value: summary.duration_ms.to_string(),
                        });
                        let _ = writer.send(WriteMessage::UpdateMeta {
                            key: "total_entries".to_string(),
                            value: summary.total_entries.to_string(),
                        });
                        let _ = writer.send(WriteMessage::UpdateMeta {
                            key: "total_physical_bytes".to_string(),
                            value: summary.total_physical_bytes.to_string(),
                        });
                        let _ = writer.send(WriteMessage::UpdateMeta {
                            key: "volume_path".to_string(),
                            value: "/".to_string(),
                        });
                    }

                    // Open a read connection for path resolution during replay
                    let replay_conn = match IndexStore::open_read_connection(&writer.db_path()) {
                        Ok(c) => c,
                        Err(e) => {
                            log::warn!("Reconciler: failed to open read connection for replay: {e}");
                            return;
                        }
                    };

                    // Set a baseline last_event_id so there's always a valid
                    // event ID even if no live events were buffered during the scan.
                    // The reconciler will overwrite this with a higher ID if any
                    // post-scan events exist.
                    if scan_start_event_id > 0 {
                        let _ = writer.send(WriteMessage::UpdateLastEventId(scan_start_event_id));
                    }

                    // Replay events that arrived after the scan read their paths
                    match reconciler.replay(scan_start_event_id, &replay_conn, &writer, &mut |paths| {
                        reconciler::emit_dir_updated(&app, paths)
                    }) {
                        Ok(last_id) => {
                            log::info!("Reconciler: post-scan replay complete (last_event_id={last_id})");
                        }
                        Err(e) => {
                            log::warn!("Reconciler: replay failed: {e}");
                        }
                    }

                    // Backfill dir_stats for any directories created by the replay
                    // that didn't go through the full aggregation pass.
                    let _ = writer.send(WriteMessage::BackfillMissingDirStats);

                    // Switch to live mode
                    reconciler.switch_to_live();

                    // Freshness ⇒ Fresh (green) on a clean completion. A cancelled
                    // local scan keeps its prior freshness (root stays browsable);
                    // it isn't reset to gray the way an interrupted SMB scan is,
                    // because local data isn't tied to a connection that vanished.
                    if !summary.was_cancelled {
                        super::state::apply_freshness_event_on(
                            &freshness,
                            &volume_id,
                            super::freshness::FreshnessEvent::ScanCompleted,
                        );
                    }

                    DEBUG_STATS.close_phase_with_stats(vec![("buffered_events", buffered_count.to_string())]);
                    set_phase_for(
                        &app,
                        &volume_id,
                        ActivityPhase::Live,
                        "post-scan reconciliation complete",
                    );

                    // Step 5: Start live event processing loop
                    let writer_live = writer.clone();
                    let app_live = app.clone();
                    let volume_id_live = volume_id.clone();
                    let overflow_live = watcher_overflow_flag.clone();
                    let handle = tauri::async_runtime::spawn(async move {
                        run_live_event_loop(
                            event_rx,
                            reconciler,
                            writer_live,
                            app_live,
                            volume_id_live,
                            overflow_live,
                        )
                        .await;
                    });

                    // Store the handle so shutdown() can wait for it to drain
                    {
                        let mut guard = live_event_task_slot.lock_ignore_poison();
                        *guard = Some(handle);
                    }
                }
                Ok(Err(e)) => {
                    log::warn!("Volume scan failed: {e}");
                    // The scan/reconcile bailed (e.g. `EmptyRoot`, or a
                    // `catch_unwind`-converted reconcile-walk `Panicked`). The
                    // prior index is untouched and stays visible, but `ScanStarted`
                    // already moved freshness to Scanning, so reset it to Stale —
                    // honest "rescan available" instead of a stuck spinner. Fire
                    // through the cloned handle, never the registry (no re-lock).
                    super::state::apply_freshness_event_on(
                        &freshness,
                        &volume_id,
                        super::freshness::FreshnessEvent::ScanFailed,
                    );
                }
                Err(_) => {
                    log::warn!("Volume scan thread panicked");
                    // The walker thread itself panicked (the reconcile walk is
                    // `catch_unwind`-wrapped, so this is the residual jwalk/thread
                    // case). Same honest reset as the `Ok(Err(_))` arm above.
                    super::state::apply_freshness_event_on(
                        &freshness,
                        &volume_id,
                        super::freshness::FreshnessEvent::ScanFailed,
                    );
                }
            }
        });

        self.scan_handle = Some(scan_handle);
        Ok(())
    }

    /// Stop the active full scan and watcher.
    pub fn stop_scan(&mut self) {
        set_phase_for(&self.app, &self.volume_id, ActivityPhase::Idle, "stopped");

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

        DEBUG_STATS.reset();

        // Abort the live event processing task
        {
            let mut guard = self.live_event_task.lock_ignore_poison();
            if let Some(task) = guard.take() {
                task.abort();
            }
        }
    }

    /// Get the current index status.
    pub fn get_status(&self) -> Result<IndexStatusResponse, String> {
        let index_status = self
            .store
            .get_index_status()
            .map_err(|e| format!("Failed to get index status: {e}"))?;

        let db_file_size = self.store.db_file_size().ok();

        let snap = self.scan_handle.as_ref().map(|h| h.progress.snapshot());
        let counters = live_scan_counters(snap, self.scan_calibration);

        Ok(IndexStatusResponse {
            initialized: true,
            scanning: self.scanning.load(Ordering::Relaxed),
            entries_scanned: counters.entries_scanned,
            dirs_found: counters.dirs_found,
            bytes_scanned: counters.bytes_scanned,
            index_status: Some(index_status),
            db_file_size,
            volume_used_bytes: counters.volume_used_bytes,
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
            db_main_size,
            db_wal_size,
            db_page_count,
            db_freelist_count,
        })
    }

    /// Read the current phase timeline from DebugStats.
    pub(super) fn read_phase_timeline() -> (ActivityPhase, String, u64, Vec<PhaseRecord>) {
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
        set_phase_for(&self.app, &self.volume_id, ActivityPhase::Idle, "shutdown");

        // 1. Cancel active scan (but don't abort event loop)
        if let Some(ref handle) = self.scan_handle {
            handle.cancel();
        }
        self.scan_handle = None;
        self.scanning.store(false, Ordering::Relaxed);

        // 2. Stop the watcher. Dropping the sender closes the channel, which causes event_rx.recv() to
        //    return None in the event loop.
        if let Some(ref mut watcher) = self.drive_watcher {
            watcher.stop();
        }
        self.drive_watcher = None;

        // 3. Wait for the event loop to drain (process final batch + UpdateLastEventId). Use block_in_place
        //    so we can .await the join handle without blocking the tokio runtime thread pool.
        let task = self.live_event_task.lock_ignore_poison().take();
        if let Some(task) = task {
            tokio::task::block_in_place(|| {
                tauri::async_runtime::block_on(async {
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

#[cfg(test)]
mod tests {
    //! Unit tests for the pure `get_status` helper.
    //!
    //! `IndexManager::get_status` itself needs a full manager (and thus an
    //! `AppHandle`), which the module's testing bar keeps under integration
    //! coverage. `live_scan_counters` is the snapshot-and-calibration combining
    //! it delegates to; pinning that here exercises every field `get_status`
    //! surfaces — live bytes from the scan snapshot and the tier-2 used-bytes
    //! denominator from the stashed calibration — without an `AppHandle`.
    use super::*;
    use crate::indexing::scanner::ScanProgressSnapshot;

    fn snapshot(entries: u64, dirs: u64, bytes: u64) -> ScanProgressSnapshot {
        ScanProgressSnapshot {
            entries_scanned: entries,
            dirs_found: dirs,
            bytes_scanned: bytes,
        }
    }

    fn calibration(used_bytes: Option<u64>) -> ScanCalibration {
        ScanCalibration {
            prior: super::super::store::ScanCalibration::default(),
            volume_used_bytes: used_bytes,
        }
    }

    #[test]
    fn live_counters_reflect_snapshot_bytes_and_calibration_used_bytes() {
        // Mid-scan: an active snapshot plus a calibration carrying the tier-2
        // denominator. get_status must surface both, apples-to-apples with what
        // the 500 ms progress event emits.
        let counters = live_scan_counters(
            Some(snapshot(42_000, 1_200, 905_000_000)),
            Some(calibration(Some(746_000_000))),
        );
        assert_eq!(counters.entries_scanned, 42_000);
        assert_eq!(counters.dirs_found, 1_200);
        assert_eq!(counters.bytes_scanned, 905_000_000);
        assert_eq!(counters.volume_used_bytes, Some(746_000_000));
    }

    #[test]
    fn live_counters_are_zero_with_no_active_scan() {
        // No scan handle and no calibration (the idle / between-scans state):
        // every live counter reads 0 and the tier-2 denominator is absent.
        let counters = live_scan_counters(None, None);
        assert_eq!(counters, LiveScanCounters::default());
        assert_eq!(counters.bytes_scanned, 0);
        assert_eq!(counters.volume_used_bytes, None);
    }

    #[test]
    fn live_counters_omit_used_bytes_when_space_info_failed() {
        // First scan where the space-info fetch failed: a live snapshot exists,
        // but the tier-2 denominator is `None`, so the FE falls back to tier 1 /
        // counter-only. The live bytes still flow through.
        let counters = live_scan_counters(Some(snapshot(10, 3, 4_096)), Some(calibration(None)));
        assert_eq!(counters.bytes_scanned, 4_096);
        assert_eq!(counters.volume_used_bytes, None);
    }

    /// Regression anchor for the real-hardware "SMB Rescan indexes nothing" bug:
    /// `force_rescan` routes by the TYPED volume kind, so an SMB/MTP rescan hits
    /// the `Volume`-trait scanner — NOT the local jwalk `start_scan`, which ran
    /// over the network mount, walked nothing, and falsely marked the index
    /// complete. Pre-fix `force_scan` called `start_scan` unconditionally, so an
    /// SMB id wrongly mapped to `LocalJwalk`; this pins the correct mapping.
    /// The reconcile-vs-truncate boundary: reconcile ONLY a previously-completed,
    /// populated index. A sentinel-only DB (`entry_count == 1`, never scanned) takes
    /// FRESH/truncate jwalk. `> 1` not `> 0` — the latter would send a brand-new
    /// user's first `/` scan down the serial reconcile (the onboarding bug). AND the
    /// completeness gate: a populated-but-never-completed partial (`scan_completed_at`
    /// absent) also takes the fast jwalk rebuild, because reconciling its
    /// add-everything delta wedges the serial walk (the ~15-min "looks hung" bug on a
    /// real `/`). The sentinel-makes-it-1 fact is verified against a fresh store below.
    #[test]
    fn local_rescan_reconciles_only_beyond_the_root_sentinel() {
        // Completeness gate: even a populated DB does NOT reconcile if the prior scan
        // never completed.
        assert!(!local_rescan_reconciles(0, true), "empty DB ⇒ fresh/truncate path");
        assert!(
            !local_rescan_reconciles(1, true),
            "sentinel-only DB (never scanned) ⇒ fresh/truncate path, NOT reconcile"
        );
        assert!(
            local_rescan_reconciles(2, true),
            "populated AND prior-completed ⇒ reconcile path"
        );
        assert!(
            !local_rescan_reconciles(2, false),
            "populated but never-completed partial ⇒ fast jwalk rebuild, NOT reconcile"
        );

        // A fresh store has exactly the ROOT sentinel, so its entry_count is 1 and
        // the predicate routes it to the fresh path — the onboarding guarantee.
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("sentinel.db");
        let store = IndexStore::open(&db_path).expect("open store");
        let count = IndexStore::get_entry_count(store.read_conn()).expect("count");
        assert_eq!(count, 1, "a fresh DB holds only the ROOT sentinel");
        assert!(
            !local_rescan_reconciles(count, true),
            "so a fresh DB takes the truncate path"
        );
    }

    #[test]
    fn force_rescan_routes_smb_and_mtp_to_the_trait_scanner_not_jwalk() {
        assert_eq!(
            rescan_scanner_for_kind(IndexVolumeKind::Smb),
            RescanScanner::VolumeTrait,
            "an SMB rescan must walk the Volume trait from the share root, not jwalk the mount",
        );
        assert_eq!(
            rescan_scanner_for_kind(IndexVolumeKind::Mtp),
            RescanScanner::VolumeTrait,
            "an MTP rescan must walk the Volume trait, not jwalk",
        );
        assert_eq!(
            rescan_scanner_for_kind(IndexVolumeKind::Local),
            RescanScanner::LocalJwalk,
            "only a local disk uses the jwalk + FSEvents scanner",
        );
    }
}

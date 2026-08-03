//! Starting a walk: the two ways an `IndexManager` puts a volume back in sync.
//!
//! `start_scan` rebuilds (or reconciles) from a full walk with a watcher
//! buffering events underneath it; `start_replay` skips the walk entirely and
//! replays the FSEvents journal from a stored event id, falling back to a scan
//! when the journal has a gap. `resume_or_scan` in `manager.rs` picks between
//! them; everything after either one starts is the same live-event machinery.

use super::*;

impl IndexManager {
    /// Resume from an existing index by replaying FSEvents journal since `since_event_id`.
    ///
    /// Starts the watcher with `sinceWhen = since_event_id`. The watcher replays
    /// journal events which are processed as live events. If the journal is
    /// unavailable (gap detected), falls back to a full scan.
    pub(super) fn start_replay(&mut self, since_event_id: u64, heal_after_replay: bool) -> Result<(), String> {
        // Unbounded: a slow replay drain must never backpressure the FSEvents
        // forward task into dropping events (Fix 2). Memory is bounded by the
        // ingestion hard cap in `run_replay_event_loop`, not by the channel.
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let current_id = watcher::current_event_id();

        let watcher_overflow: Option<Arc<AtomicBool>>;
        match DriveWatcher::start(&self.volume_root, since_event_id, event_tx) {
            Ok(watcher) => {
                watcher_overflow = Some(watcher.overflow_flag());
                self.drive_watcher = Some(watcher);
                DEBUG_STATS.watcher_active.store(true, Ordering::Relaxed);
                let gap = current_id.saturating_sub(since_event_id);
                set_phase_for(
                    self.events.as_ref(),
                    &self.volume_id,
                    ActivityPhase::Replaying,
                    &format!("app launch, ~{}", pluralize(gap, "pending FSEvent")),
                );
                log::info!("Replay: watcher started (since_event_id={since_event_id}, current={current_id})");
            }
            Err(e) => {
                emit_rescan_notification(
                    self.events.as_ref(),
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
        let events = Arc::clone(&self.events);
        let volume_id = self.volume_id.clone();
        // Journal replay only runs for a journaled volume (the boot disk), so this
        // is `root` today; it's derived rather than hardcoded so replay resolves in
        // the same space as the live loop that follows.
        let space = self.path_space();
        let live_event_task_slot = Arc::clone(&self.live_event_task);
        let scanning = Arc::clone(&self.scanning);

        // The fallback task (below) re-resolves this manager in the registry by
        // volume id, so keep a clone for it before `volume_id` is moved into the
        // replay loop task.
        let fallback_volume_id = self.volume_id.clone();

        // A way for the replay loop to signal "can't roll forward from the journal,
        // need a full scan", carrying WHY. Several distinct causes trip this (journal
        // purged, >10M events, watcher channel overflowed); the reason rides the
        // channel so the fallback logs the real cause instead of guessing "gap".
        let (fallback_tx, fallback_rx) = tokio::sync::oneshot::channel::<RescanReason>();

        // The loop's own branch of this volume's stop signal. Taken here, where the
        // manager owns it, so nothing below has to reach back into the registry for
        // it (see `ReplayConfig::cancel`).
        let replay_cancel = self.volume_cancel.child_token();

        // Spawn through the host runtime seam, which resolves a handle instead of
        // inheriting one: indexing can start from the app's synchronous setup() hook,
        // where there's no ambient Tokio runtime for `tokio::spawn` to find.
        // Store the handle so shutdown() can wait for it to drain.
        let handle = crate::indexing::host::runtime::spawn(async move {
            let result = run_replay_event_loop(
                event_rx,
                writer.clone(),
                Arc::clone(&events),
                ReplayConfig {
                    volume_id: volume_id.clone(),
                    space,
                    since_event_id,
                    estimated_total,
                    heal_after_replay,
                    cancel: replay_cancel,
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
        crate::indexing::host::runtime::spawn(async move {
            if let Ok(reason) = fallback_rx.await {
                log::warn!("Replay signaled a full-scan fallback ({reason:?}); rescanning the volume");
                perform_registry_rescan(&fallback_volume_id, &format!("replay fallback ({reason:?})")).await;
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

        // The completeness gate for reconcile-vs-truncate (see `local_rescan_reconciles`):
        // snapshot whether the prior scan COMPLETED, read BEFORE `DeleteMeta` clears
        // `scan_completed_at` below. A partial that never finished must NOT reconcile
        // (its add-everything delta wedges the serial walk); it takes the fast
        // guarded-walker rebuild instead.
        let prior_scan_completed = self
            .store
            .get_index_status()
            .map(|s| s.scan_completed_at.is_some())
            .unwrap_or(false);

        // Reconcile vs truncate. A previously-COMPLETED, populated index (rows beyond
        // the ROOT sentinel) is RESCANNED in place by `local_reconcile` (diff each dir,
        // write only changes) so the last-good directory sizes stay visible (stale)
        // throughout and no large freelist is minted. A first/empty scan OR a
        // never-completed partial keeps the fast parallel guarded-walker bulk build
        // (see `local_rescan_reconciles` for the completeness gate). Read the entry
        // count from the live read connection BEFORE any truncate. (NOTE: the network
        // predicate in `lifecycle/network_scan.rs` is intentionally left unchanged.)
        let reconcile = IndexStore::get_entry_count(self.store.read_conn())
            .map(|n| local_rescan_reconciles(n, prior_scan_completed))
            .unwrap_or(false);

        // Step 0: Capture this scan's calibration BEFORE truncating.
        //
        // The prior completed scans' totals are read straight off the live read
        // connection: the calibration keys survive `TruncateData` (it preserves
        // `meta`), but reading first keeps the data flow obviously correct — we
        // snapshot the previous scans' numbers before the truncate touches anything.
        //
        // PER KIND: the two walks differ ~5x in wall clock, so a change check's
        // duration would predict a wildly wrong ETA for a full walk and vice versa.
        // `for_kind` prefers this run's own kind and falls back to the last scan of
        // any kind (better a stale-ish number than none).
        let calibration_set = IndexStore::read_scan_calibration_set(self.store.read_conn()).unwrap_or_else(|e| {
            log::warn!("Failed to read prior scan calibration (tier-1 will degrade): {e}");
            crate::indexing::store::ScanCalibrationSet::default()
        });
        let run_kind = ScanRunKind::classify(reconcile, calibration_set.any.total_entries);
        let prior = calibration_set.for_kind(run_kind.calibration_kind());

        // Fetch the scanned volume's used bytes ONCE (tier-2 denominator). The call
        // does disk I/O — an NSURL XPC round-trip on macOS, `statvfs` on Linux — and
        // `start_scan` runs in async contexts (the auto-start spawn, async Tauri
        // commands), so wrap it in `block_in_place`, matching the `flush_blocking`
        // call below. A bare blocking call on a tokio worker can stall on a wedged
        // mount. Failure → `None`; never block or delay the scan for the denominator.
        let volume_root = self.volume_root.clone();
        let volume_used_bytes =
            tokio::task::block_in_place(|| crate::indexing::host::volumes::current().volume_used_bytes(&volume_root));

        let calibration = ScanCalibration {
            prior,
            volume_used_bytes,
            run_kind,
        };
        self.scan_calibration = Some(calibration);

        // Step 0a: Clear the previous scan's completion marker BEFORE truncating.
        // Without this, a rescan killed mid-way (power loss, `kill -9`) leaves the
        // PREVIOUS scan's `scan_completed_at` in meta on top of a truncated/partial
        // `entries` table, so the next startup takes the journal-replay path over a
        // gutted index instead of the `IncompletePreviousScan` fresh rescan. The
        // calibration keys (`total_entries`, `total_physical_bytes`, `scan_duration_ms`,
        // and their per-walk-kind twins) are intentionally left intact so they keep
        // describing the last COMPLETED scan throughout this one. The same flush below
        // covers both sends.
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

        // The volume's path space: pass-through for the boot disk, mount-relative
        // strip for a mount-rooted external drive. Threaded to the scanner (exclusion
        // scope), the reconcile walk, and the completion handler's replay + live loop.
        let space = self.path_space();

        // Step 1: Start the FSEvents watcher BEFORE the scan so we don't miss events.
        // Unbounded so a slow buffered-event drain never backpressures the forward
        // task into dropping events (Fix 2); memory is capped by the ingestion hard
        // cap in the live loop, not the channel.
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
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
        self.events.emit(IndexEvent::ScanStarted {
            volume_id: self.volume_id.clone(),
            run_kind: calibration.run_kind,
            prior_total_entries: calibration.prior.total_entries,
            prior_scan_duration_ms: calibration.prior.scan_duration_ms,
            volume_used_bytes: calibration.volume_used_bytes,
        });

        set_phase_for(
            self.events.as_ref(),
            &self.volume_id,
            ActivityPhase::Scanning,
            scan_trigger,
        );

        // Freshness ⇒ Scanning (blue). For local `root` this also drives the
        // per-drive badge; the clean-completion handler flips it back
        // to Fresh. (Root is journaled, so a restart re-seeds Fresh; this keeps
        // the badge honest DURING a scan/rescan.) Fire through the manager's OWN
        // freshness handle (`apply_freshness_event_on`), NOT the volume-id lookup:
        // `force_scan` (and the journal-gap fallback) call `start_scan` while
        // holding the registry lock, so a registry re-lock here deadlocks.
        super::super::state::apply_freshness_event_on(
            &self.freshness,
            self.events.as_ref(),
            &self.volume_id,
            super::super::freshness::FreshnessEvent::ScanStarted,
        );

        // Step 2: Start the walk. A reconcile rescan runs the serial full-tree
        // `local_reconcile` walker (returns the SAME `(ScanHandle, JoinHandle)` shape
        // as `scan_volume`, runs on a `std::thread`, does its marks + single aggregate
        // in-thread), so the completion handler below is reused literally unchanged. A
        // fresh scan runs the fast parallel guarded-walker `scan_volume`.
        let (scan_handle, join_handle) = if reconcile {
            log::info!("local scan: reconcile rescan for '{}' ({scan_trigger})", self.volume_id);
            local_reconcile::start_local_reconcile(
                self.volume_root.clone(),
                space.clone(),
                &self.writer,
                self.volume_cancel.child_token(),
            )
            .map_err(|e| format!("Failed to start reconcile rescan: {e}"))?
        } else {
            log::info!(
                "local scan: fresh scan (truncate) for '{}' ({scan_trigger})",
                self.volume_id
            );
            let config = ScanConfig {
                root: self.volume_root.clone(),
                // Carries both volume facts the walk needs: a mount-rooted drive
                // gates children with `MountRooted` (the boot tier would exclude its
                // own `/Volumes/X` subtree and falsely complete the scan empty), and
                // a FAT/exFAT drive's derived inodes are untrusted, so the scanner
                // stores `inode: None` (keeping the rename pre-pass inert).
                space: space.clone(),
                ..ScanConfig::default()
            };
            scanner::scan_volume(config, &self.writer, self.volume_cancel.child_token())
                .map_err(|e| format!("Failed to start scan: {e}"))?
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
        // committed rows (`Sql`); a FRESH guarded-walker scan populates the maps (`Maps`).
        let partial_agg_source = if reconcile { AggSource::Sql } else { AggSource::Maps };
        ScanProgressReporter::new(
            Arc::clone(&scan_handle.progress),
            self.writer.clone(),
            Arc::clone(&self.events),
            self.volume_id.clone(),
            partial_agg_source,
        )
        .spawn(Arc::clone(&scan_done));

        // Step 3: Spawn completion handler that also does reconciliation.
        // Spawn through the host runtime seam, which resolves a handle instead of
        // inheriting one: indexing can start from the app's synchronous setup() hook,
        // where there's no ambient Tokio runtime for `tokio::spawn` to find.
        let volume_id = self.volume_id.clone();
        let events = Arc::clone(&self.events);
        let writer = self.writer.clone();
        let scanning = Arc::clone(&self.scanning);
        // Clone the freshness handle into the completion task so it fires
        // `ScanCompleted` through the `Arc` directly, never re-locking the registry.
        let freshness = Arc::clone(&self.freshness);
        let live_event_task_slot = Arc::clone(&self.live_event_task);
        let watcher_overflow_flag = watcher_overflow;
        crate::indexing::host::runtime::spawn(super::super::scan_completion::run_scan_completion(
            super::super::scan_completion::ScanCompletion {
                join_handle,
                scan_done,
                scanning,
                event_rx,
                watcher_overflow_flag,
                volume_id,
                space,
                events,
                writer,
                freshness,
                live_event_task_slot,
                scan_start_event_id,
                calibration_kind: run_kind.calibration_kind(),
                cancel: self.volume_cancel.child_token(),
            },
        ));

        self.scan_handle = Some(scan_handle);
        Ok(())
    }
}

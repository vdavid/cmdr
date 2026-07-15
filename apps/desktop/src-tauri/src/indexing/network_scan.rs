//! The SMB/MTP `Volume`-trait scan path for [`IndexManager`].
//!
//! Network volumes (SMB shares, MTP storages) have no local filesystem to walk
//! and no FSEvents journal, so they scan through the async `Volume` trait
//! (`volume_scanner`) instead of the local guarded-walker path in [`super::manager`]. This
//! module owns that family: the startup dispatch for a journal-less volume
//! (`resume_or_scan_network`), the scan/rescan entry (`start_volume_scan`), and
//! its bespoke completion handling — partial-aggregation-free progress loop,
//! buffered-change replay, and freshness transitions that differ from the local
//! path. The dispatcher (`resume_or_scan`, `force_rescan`) and everything shared
//! stay in [`super::manager`]; these methods are split out as a sibling `impl
//! IndexManager` block (Rust allows split impls) and called from there.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use tauri_specta::Event;

use super::events::{
    ActivityPhase, DEBUG_STATS, IndexAggregationCompleteEvent, IndexDirUpdatedEvent, IndexScanAbortedEvent,
    IndexScanCompleteEvent, IndexScanStartedEvent, set_phase_for,
};
use super::manager::{IndexManager, ScanCalibration};
use super::progress_reporter::ScanProgressReporter;
use super::state::IndexVolumeKind;
use super::store::IndexStore;
use super::writer::{PartialAggSource, WriteMessage};

/// Replay the changes the live watcher buffered during a `Volume`-trait scan,
/// dispatching to the right per-backend buffer (SMB `CHANGE_NOTIFY` vs. MTP PTP
/// events). Returns whether the volume stays Fresh (false ⇒ overflow forced
/// Stale). `Local` never reaches here (the guarded-walker path), so it's a trivially-Fresh
/// no-op. The buffers are macOS/Linux-only (the only `Volume`-trait backends).
fn replay_buffered_changes_for_kind(kind: IndexVolumeKind, volume_id: &str) -> bool {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    match kind {
        IndexVolumeKind::Smb => return super::replay_buffered_changes(volume_id),
        IndexVolumeKind::Mtp => return super::replay_buffered_mtp_changes(volume_id),
        // Local-scanner kinds take the guarded-walker path and never buffer network changes.
        IndexVolumeKind::Local | IndexVolumeKind::LocalExternal => {}
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (kind, volume_id);
    true
}

/// Discard the live-watcher buffer for an interrupted scan (D-interrupted),
/// dispatching by backend. Mirrors `replay_buffered_changes_for_kind`.
fn discard_buffered_changes_for_kind(kind: IndexVolumeKind, volume_id: &str) {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    match kind {
        IndexVolumeKind::Smb => super::discard_buffered_changes(volume_id),
        IndexVolumeKind::Mtp => super::discard_buffered_mtp_changes(volume_id),
        // Local-scanner kinds take the guarded-walker path and never buffer network changes.
        IndexVolumeKind::Local | IndexVolumeKind::LocalExternal => {}
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = (kind, volume_id);
}

impl IndexManager {
    /// `resume_or_scan` for journal-less network volumes (SMB and MTP).
    ///
    /// A completed prior scan loaded **Stale** (no journal to roll forward — see
    /// `freshness`), so we DON'T rescan automatically: the index stays browsable
    /// and the user rescans to refresh. A never-completed index (first connect,
    /// or an interrupted prior scan) triggers a fresh `Volume`-trait scan.
    ///
    /// Note: no `DriveWatcher` here. FSEvents doesn't cover network mounts; the
    /// live SMB watcher that keeps the index Fresh hooks in through
    /// `state::apply_freshness_event` instead. This path handles the scan and
    /// freshness seeding only.
    pub(super) fn resume_or_scan_network(&mut self) -> Result<(), String> {
        let kind = self.kind_label();
        let status = self
            .store
            .get_index_status()
            .map_err(|e| format!("Failed to get index status: {e}"))?;

        if status.scan_completed_at.is_some() {
            log::info!(
                "Startup: {kind} volume '{}' has a completed index, loading as Stale (no journal to replay)",
                self.volume_id
            );
            // Already Stale (seeded at reservation). Nothing to scan; reads serve
            // the persisted index until the user rescans. The live watcher (SMB
            // CHANGE_NOTIFY / MTP PTP event loop) runs connection-scoped and is
            // what keeps a re-enabled/re-scanned index Fresh.
            return Ok(());
        }

        // No completion marker. Either a never-scanned volume (empty DB → first
        // scan truncates + builds) or a persisted PARTIAL from a prior mid-scan
        // disconnect (non-empty DB → `start_volume_scan` reconciles in place, so
        // the partial stays visible stale rather than being blanked). The mode is
        // chosen inside `start_volume_scan` by whether the DB already has rows.
        log::info!(
            "Startup: {kind} volume '{}' scan (no completion marker; reconcile if a partial persists)",
            self.volume_id
        );
        self.start_volume_scan("startup scan (no completion marker)")
    }

    /// A short label for this volume kind, for diagnostics. Only `Smb`/`Mtp`
    /// reach the network scan path; `Local` is handled by the guarded-walker path.
    fn kind_label(&self) -> &'static str {
        match self.kind {
            IndexVolumeKind::Mtp => "MTP",
            IndexVolumeKind::Smb => "SMB",
            IndexVolumeKind::Local => "local",
            IndexVolumeKind::LocalExternal => "local-external",
        }
    }

    /// Start a `Volume`-trait scan/rescan for a network volume (SMB or MTP).
    ///
    /// Mirrors `start_scan`'s shape (bump epoch → walk → aggregate → meta on clean
    /// completion) but walks via `volume_scanner` instead of the guarded walker, and starts NO
    /// `DriveWatcher` (the live-watch layer owns that). Picks the WALK by whether
    /// the index already has data: an empty DB does a fresh `scan_volume_via_trait`
    /// (truncate + bulk build); a populated DB does a non-destructive
    /// `reconcile_volume_via_trait` (diff each dir, write only changes, never blank
    /// the index). See `indexing/DETAILS.md` § "Non-destructive rescan".
    /// Freshness transitions: `ScanStarted` ⇒ Scanning now; on clean completion the
    /// completion task fires `ScanCompleted` ⇒ Fresh and writes the meta marker;
    /// on cancel/error the partial is discarded by RESETTING the volume to gray
    /// (removing the registry instance), per D-interrupted.
    pub(super) fn start_volume_scan(&mut self, scan_trigger: &str) -> Result<(), String> {
        use super::scanner::{ScanHandle, ScanProgress};
        use std::sync::atomic::AtomicBool;

        if self.scanning.load(Ordering::Relaxed) {
            return Err("Scan already running".to_string());
        }

        // Resolve the live volume handle by id. Gone ⇒ the share unmounted; bail
        // so the caller resets to gray rather than scanning nothing.
        let volume = crate::file_system::get_volume_manager()
            .get(&self.volume_id)
            .ok_or_else(|| format!("Volume '{}' is not registered (unmounted?)", self.volume_id))?;

        // Capture tier-2 calibration before truncating (same flow as start_scan).
        let prior = IndexStore::read_scan_calibration(self.store.read_conn()).unwrap_or_default();
        let volume_root = self.volume_root.clone();
        let volume_used_bytes = tokio::task::block_in_place(|| {
            crate::file_system::volume::backends::get_space_info_for_path(&volume_root)
                .map(|info| info.used_bytes)
                .ok()
        });
        self.scan_calibration = Some(ScanCalibration {
            prior,
            volume_used_bytes,
        });

        // Pre-arm-before-snapshot: flip `scanning` BEFORE truncating, so any live
        // SMB change racing in during/after the truncate is BUFFERED by
        // `apply_smb_change` (which reads this flag) instead of being applied
        // against the gutted, half-rebuilt index and lost. The smb2 watcher has
        // been running continuously since connect, so its events are already on
        // the wire; this is the moment we start stashing them for post-scan
        // replay. The ordering survives a mid-scan watcher respawn: a respawned
        // watcher feeds the same buffer while this flag stays set.
        self.scanning.store(true, Ordering::Relaxed);

        // Reconcile vs truncate: an already-populated index is RESCANNED in place
        // (diff each dir, write only changes) so the last-good data stays visible
        // (stale) throughout and a mid-rescan disconnect leaves it intact. A first
        // scan (DB holds only the ROOT sentinel) truncates and bulk-builds (faster
        // on empty). The predicate is "the entries table has rows BEYOND the ROOT
        // sentinel" — true for both a prior COMPLETED index and a persisted PARTIAL
        // (from a prior mid-scan disconnect), so a persisted partial survives
        // relaunch shown stale instead of being truncated. See `indexing/DETAILS.md`
        // § "Non-destructive rescan".
        //
        // MUST be `> 1`, not `> 0`: `ensure_root_sentinel` always inserts the ROOT
        // row (id=1) and `TruncateData` re-inserts it, so a never-scanned DB has
        // `entry_count == 1`. With `> 0`, a first connect would run the per-entry
        // reconcile against the 1-row sentinel DB instead of the faster bulk build.
        // (Same `> 1` rule as the LOCAL path's `local_rescan_reconciles`.)
        let reconcile = IndexStore::get_entry_count(self.store.read_conn())
            .map(|n| n > 1)
            .unwrap_or(false);

        // Clear the prior completion marker (so an interrupted rescan heals — no
        // stale `scan_completed_at` over a now-stale/partly-rewritten table) and
        // bump `current_epoch` at the scan-start funnel (a continuity break:
        // reconnect/journal-gap/stale/overflow/force rescans all funnel here, so
        // bumping once covers them without enumerating the trigger). The first-ever
        // scan also bumps (1→2 with nothing yet at epoch 1) — benign. For a FIRST
        // scan we also truncate so the bulk insert lands in an empty DB; a RECONCILE
        // rescan does NOT truncate — the whole point is to never blank the index.
        // The flush below commits all of this BEFORE the walk thread reads
        // `current_epoch` on its own connection (else it would stamp the stale
        // epoch). Freshness is reset to Scanning below.
        let _ = self
            .writer
            .send(WriteMessage::DeleteMeta("scan_completed_at".to_string()));
        let _ = self.writer.send(WriteMessage::BumpCurrentEpoch);
        if !reconcile {
            let _ = self.writer.send(WriteMessage::TruncateData);
        }
        if let Err(e) = tokio::task::block_in_place(|| self.writer.flush_blocking()) {
            log::warn!("network scan: flush after scan-start meta/truncate failed: {e}");
        }
        log::info!(
            "network scan: {} for '{}' ({scan_trigger})",
            if reconcile {
                "reconcile rescan"
            } else {
                "fresh scan (truncate)"
            },
            self.volume_id,
        );

        // Freshness ⇒ Scanning (blue), via the state machine. Fire through the
        // manager's OWN freshness handle (`apply_freshness_event_on`), NOT the
        // volume-id lookup, so a held-registry caller can't self-deadlock on a
        // registry re-lock here.
        super::state::apply_freshness_event_on(
            &self.freshness,
            &self.volume_id,
            super::freshness::FreshnessEvent::ScanStarted,
        );

        let _ = IndexScanStartedEvent {
            volume_id: self.volume_id.clone(),
            prior_total_entries: prior.total_entries,
            prior_scan_duration_ms: prior.scan_duration_ms,
            volume_used_bytes,
        }
        .emit(&self.app);
        set_phase_for(&self.app, &self.volume_id, ActivityPhase::Scanning, scan_trigger);

        let progress = Arc::new(ScanProgress::new());
        let cancelled = Arc::new(AtomicBool::new(false));
        self.scan_handle = Some(ScanHandle::new(Arc::clone(&progress), Arc::clone(&cancelled)));
        // `scanning` was already set true above (pre-arm before truncate).

        // Progress + mid-scan partial-aggregation reporter (500 ms), stops when the
        // scan signals done. The SAME generalized `ScanProgressReporter` the local
        // guarded-walker path uses: it emits the identical `index-scan-progress` event AND
        // drives mid-scan partial aggregation (which the bespoke inline loop never
        // did), so network fresh/reconcile and MTP fresh/reconcile all get growing
        // sizes through one path. Source by scan kind: a RECONCILE rescan leaves the
        // accumulator maps empty, so it recomputes from committed rows (`Sql`); a
        // FRESH `scan_volume_via_trait` populates the maps via `InsertEntriesV2`
        // (`Maps`).
        let scan_done = Arc::new(AtomicBool::new(false));
        let partial_agg_source = if reconcile {
            PartialAggSource::Sql
        } else {
            PartialAggSource::Maps
        };
        ScanProgressReporter::new(
            Arc::clone(&progress),
            self.writer.clone(),
            self.app.clone(),
            self.volume_id.clone(),
            partial_agg_source,
        )
        .spawn(Arc::clone(&scan_done));

        // The walk + completion handler. Runs as a tokio task because the
        // `Volume` API is async. The writer is `Send` and shared by `Arc`.
        let writer = self.writer.clone();
        let app = self.app.clone();
        let volume_id = self.volume_id.clone();
        let scanning = Arc::clone(&self.scanning);
        // Clone the freshness handle into the completion task so it fires the
        // `ScanCompleted` / `WatcherDied` transition through the `Arc` directly,
        // never re-locking the registry.
        let freshness = Arc::clone(&self.freshness);
        let root = self.volume_root.clone();
        let kind = self.kind;
        tauri::async_runtime::spawn(async move {
            let result = if reconcile {
                super::volume_scanner::reconcile_volume_via_trait(volume, root, writer.clone(), progress, cancelled)
                    .await
            } else {
                super::volume_scanner::scan_volume_via_trait(volume, root, writer.clone(), progress, cancelled).await
            };

            scan_done.store(true, Ordering::Relaxed);
            scanning.store(false, Ordering::Relaxed);

            match result {
                Ok(summary) if !summary.was_cancelled => {
                    log::info!(
                        "network scan: complete ({} entries, {} dirs, {:.1}s)",
                        summary.total_entries,
                        summary.total_dirs,
                        summary.duration_ms as f64 / 1000.0,
                    );
                    DEBUG_STATS.close_phase_with_stats(vec![
                        ("entries", summary.total_entries.to_string()),
                        ("dirs", summary.total_dirs.to_string()),
                    ]);

                    // Persist the completion marker so reads see Fresh and a
                    // future restart knows a scan finished (loads Stale then).
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
                    let _ = writer.flush().await;

                    let _ = IndexScanCompleteEvent {
                        volume_id: volume_id.clone(),
                        total_entries: summary.total_entries,
                        total_dirs: summary.total_dirs,
                        duration_ms: summary.duration_ms,
                    }
                    .emit(&app);
                    let _ = IndexAggregationCompleteEvent {
                        volume_id: volume_id.clone(),
                    }
                    .emit(&app);

                    // Replay changes the live watcher buffered DURING the scan
                    // (pre-arm-before-snapshot): the smb2 watcher ran throughout,
                    // and any change to an already-walked dir was stashed rather
                    // than lost against the rebuilding index. Replay now that the
                    // full tree (and dir_stats) is in place. Returns false if the
                    // buffer overflowed mid-scan — then it already signaled
                    // OverflowUnrecoverable ⇒ Stale, so we must NOT claim Fresh.
                    let stayed_fresh = replay_buffered_changes_for_kind(kind, &volume_id);

                    if stayed_fresh {
                        // Freshness ⇒ Fresh (green). The volume is now authoritative
                        // until the live watcher observes a continuity break. Fire
                        // through the cloned `Arc` (no registry re-lock).
                        super::state::apply_freshness_event_on(
                            &freshness,
                            &volume_id,
                            super::freshness::FreshnessEvent::ScanCompleted,
                        );
                    }
                    set_phase_for(&app, &volume_id, ActivityPhase::Live, "network scan complete");

                    // Tell the FE sizes are ready for this share's listings.
                    let _ = IndexDirUpdatedEvent {
                        paths: vec![volume_id.clone()],
                    }
                    .emit(&app);
                }
                // A mid-walk DISCONNECT: keep the honest partial. The scanner
                // already ran its partial-preserving write sequence (flush +
                // MarkDirsListed + ComputeAllAggregates) before returning the
                // typed error, so `dir_stats`/`min_subtree_epoch` exist for what
                // was scanned: scanned subtrees read exact-but-stale, unscanned
                // ones `—`/`≥`. So DON'T discard — keep the instance + DB, leave
                // `scan_completed_at` UNwritten (it heals to a rescan on relaunch,
                // the accepted session-scoped limitation until the reconcile rescan
                // lands), bump `current_epoch` (the continuity break that makes the
                // kept rows stale), and mark the volume Stale. The buffered live
                // changes are meaningless now
                // (we can't trust the partial tree), so drop them.
                Err(ref e) if e.is_terminal_disconnect() => {
                    log::warn!(
                        "network scan: disconnected for '{volume_id}' ({e}); keeping honest partial, marking Stale"
                    );
                    discard_buffered_changes_for_kind(kind, &volume_id);
                    // Bump the epoch via the captured `writer` directly, NOT
                    // `state::bump_current_epoch_for` (which needs the phase to be
                    // `Running`): this completion task can fire while the volume is
                    // still `Initializing` for a first scan, before the manager is
                    // promoted, so the registry lookup would no-op. The scanner
                    // stamped the partial's listed dirs at the scan-start epoch;
                    // bumping past it makes those rows read exact-but-stale, the
                    // honest state for a connection that vanished.
                    let _ = writer.send(WriteMessage::BumpCurrentEpoch);
                    super::state::apply_freshness_event_on(
                        &freshness,
                        &volume_id,
                        super::freshness::FreshnessEvent::WatcherDied,
                    );
                    set_phase_for(
                        &app,
                        &volume_id,
                        ActivityPhase::Idle,
                        "network scan disconnected (honest partial kept)",
                    );
                    // Clear the FE's live activity: the scan ended without a
                    // completion event, so without this the corner indicator and
                    // the breadcrumb badge tooltip would keep a stuck "scanning"
                    // row for this volume. The dot still flips to yellow (Stale)
                    // via the freshness change above.
                    let _ = IndexScanAbortedEvent {
                        volume_id: volume_id.clone(),
                    }
                    .emit(&app);
                }
                other => {
                    // User cancel, timeout, or another genuine abort: the partial
                    // is discardable. Reset the volume to gray / not-indexed and
                    // drop the changes buffered during the aborted scan.
                    match &other {
                        Ok(_) => log::info!("network scan: cancelled for '{volume_id}', discarding partial"),
                        Err(e) => log::warn!("network scan: failed for '{volume_id}' ({e}), discarding partial"),
                    }
                    discard_buffered_changes_for_kind(kind, &volume_id);
                    super::state::reset_to_not_indexed(&volume_id);
                    // Clear the FE's live activity (no completion event fired for
                    // an aborted scan), so the corner indicator and badge tooltip
                    // don't keep a stuck "scanning" row. The dot reverts to gray
                    // (not-indexed) via the freshness reset above.
                    let _ = IndexScanAbortedEvent {
                        volume_id: volume_id.clone(),
                    }
                    .emit(&app);
                }
            }
        });

        Ok(())
    }
}

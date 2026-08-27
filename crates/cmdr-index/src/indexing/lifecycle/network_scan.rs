//! The SMB/MTP `Volume`-trait scan path for [`IndexManager`].
//!
//! Network volumes (SMB shares, MTP storages) have no local filesystem to walk
//! and no FSEvents journal, so they scan through the async `Volume` trait
//! (`network_scanner`) instead of the local guarded-walker path in [`super::manager`]. This
//! module owns that family: the startup dispatch for a journal-less volume
//! (`resume_or_scan_network`), the scan/rescan entry (`start_volume_scan`), and
//! its bespoke completion handling — partial-aggregation-free progress loop,
//! buffered-change replay, and freshness transitions that differ from the local
//! path. The dispatcher (`resume_or_scan`, `force_rescan`) and everything shared
//! stay in [`super::manager`]; these methods are split out as a sibling `impl
//! IndexManager` block (Rust allows split impls) and called from there.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::manager::{IndexManager, ScanCalibration};
use crate::indexing::events::{
    ActivityPhase, DEBUG_STATS, IndexEvent, ScanRunKind, announce_whole_volume_walk, set_phase_for,
};
use crate::indexing::lifecycle::progress_reporter::ScanProgressReporter;
use crate::indexing::lifecycle::rescan_request::ScanStartError;
use crate::indexing::network_scanner::VolumeScanError;
use crate::indexing::store::IndexStore;
use crate::indexing::volume::IndexVolumeKind;
use crate::indexing::writer::{AggSource, WriteMessage};

/// How a `Volume`-trait scan treats whatever the index already holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NetworkScanMode {
    /// Pick by what's in the DB: reconcile a populated index in place (so the
    /// last-good data stays visible), bulk-build an empty one. The normal mode for
    /// startup scans and manual rescans.
    Auto,
    /// Truncate first, whatever the DB holds. For invalidation: the indexed rows
    /// are known-unusable, so rebuilding is cheaper and more honest than any
    /// in-place repair. Reconcile CAN'T do this job — it only diffs the dirs it
    /// lists, so rows under a dir the scanner no longer walks would survive it.
    Rebuild,
}

/// Replay the changes the live watcher buffered during a `Volume`-trait scan,
/// dispatching to the right per-backend buffer (SMB `CHANGE_NOTIFY` vs. MTP PTP
/// events). Returns whether the volume stays Fresh (false ⇒ overflow forced
/// Stale). `Local` never reaches here (the guarded-walker path), so it's a trivially-Fresh
/// no-op. The buffers are macOS/Linux-only (the only `Volume`-trait backends).
fn replay_buffered_changes_for_kind(kind: IndexVolumeKind, volume_id: &str) -> bool {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    match kind {
        IndexVolumeKind::Smb => return crate::indexing::transports::smb::watch::replay_buffered_changes(volume_id),
        IndexVolumeKind::Mtp => return crate::indexing::transports::mtp::watch::replay_buffered_mtp_changes(volume_id),
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
        IndexVolumeKind::Smb => crate::indexing::transports::smb::watch::discard_buffered_changes(volume_id),
        IndexVolumeKind::Mtp => crate::indexing::transports::mtp::watch::discard_buffered_mtp_changes(volume_id),
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

        // One-shot ledger heal (see `indexing/DETAILS.md` § "The dir_stats
        // ledger"). Arm the writer latch on a never-healed DB so the next full
        // aggregate rebuilds and marks it. The scan branch below lets its own
        // aggregate consume the latch; the completed-index branch runs no scan,
        // so it ALSO enqueues the heal's own `Sql` aggregate.
        let heal_pending = IndexStore::ledger_heal_done(self.store.read_conn()).is_ok_and(|done| !done);
        if heal_pending {
            let _ = self.writer.send(WriteMessage::ArmLedgerHealLatch);
        }

        // One-time rebuild for an index built under an older NAS system-dir
        // exclusion list. Such an index still holds rows beneath dirs today's
        // scanner never walks (10 898 710 of them on the author's QNAP, 80% of the
        // index), and no reconcile can shed them: it only diffs the dirs it LISTS,
        // and it never lists these. The drive index is a disposable cache, so we
        // rebuild rather than migrate. The stamp is written when a scan truncates,
        // so this fires once per list version, not once per launch.
        if crate::indexing::network_scanner::index_predates_exclusion_list(self.store.read_conn()) {
            log::info!(
                "Startup: {kind} volume '{}' isn't built against the current NAS system-dir exclusion list; rebuilding it from scratch",
                self.volume_id
            );
            match self.start_volume_scan(NetworkScanMode::Rebuild, "NAS system-dir exclusion list changed") {
                Ok(()) => return Ok(()),
                // Can't scan right now (share unmounted, or a scan is already
                // running). Keep the existing index browsable and re-arm next load:
                // nothing stamps the DB until a rebuild actually truncates it.
                Err(e) => log::warn!(
                    "Startup: {kind} volume '{}' couldn't start its exclusion-list rebuild ({e}); keeping the existing index for now",
                    self.volume_id
                ),
            }
        }

        if status.scan_completed_at.is_some() {
            log::info!(
                "Startup: {kind} volume '{}' has a completed index, loading as Stale (no journal to replay)",
                self.volume_id
            );
            // Already Stale (seeded at reservation). Nothing to scan; reads serve
            // the persisted index until the user rescans. The live watcher (SMB
            // CHANGE_NOTIFY / MTP PTP event loop) runs connection-scoped and is
            // what keeps a re-enabled/re-scanned index Fresh.
            //
            // This branch does NOT rescan on connect, so nothing would consume the
            // armed heal latch — enqueue the heal's own aggregate directly. It
            // recomputes `dir_stats` from the committed `entries` (Sql), so the
            // stale-but-browsable index gets its drifted sizes healed in place.
            if heal_pending {
                let _ = self
                    .writer
                    .send(WriteMessage::ComputeAllAggregates { source: AggSource::Sql });
            }
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
        self.start_volume_scan(NetworkScanMode::Auto, "startup scan (no completion marker)")
            .map_err(|e| e.to_string())
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
    /// completion) but walks via `network_scanner` instead of the guarded walker, and starts NO
    /// `DriveWatcher` (the live-watch layer owns that). In `Auto` mode it picks the
    /// WALK by whether the index already has data: an empty DB does a fresh
    /// `scan_volume_via_trait` (truncate + bulk build); a populated DB does a
    /// non-destructive `reconcile_volume_via_trait` (diff each dir, write only
    /// changes, never blank the index). `Rebuild` forces the truncate + bulk build.
    /// See `indexing/DETAILS.md` § "Non-destructive rescan".
    /// Freshness transitions: `ScanStarted` ⇒ Scanning now; on clean completion the
    /// completion task fires `ScanCompleted` ⇒ Fresh and writes the meta marker;
    /// on cancel/error the partial is discarded by RESETTING the volume to gray
    /// (removing the registry instance), per D-interrupted.
    pub(super) fn start_volume_scan(
        &mut self,
        mode: NetworkScanMode,
        scan_trigger: &str,
    ) -> Result<(), ScanStartError> {
        use crate::indexing::scanner::{ScanHandle, ScanProgress};
        use std::sync::atomic::AtomicBool;

        // The same question of the phase machine, for the same reason `start_scan`
        // asks it: a volume the machine still owes work to is being walked whole,
        // in pieces, and this call would rebuild the index from the share root over
        // the top of one.
        //
        // ⚠️ No `IndexVolumeKind` is both trait-scanned and phase-covered today —
        // `first_index_is_the_machines` requires `uses_local_scanner()` — so this
        // guard refuses nothing that can currently reach it. It is here so a fifth
        // kind that was both doesn't silently start a second whole-volume walk,
        // which is precisely the shape no test would catch by accident.
        if self.phases_have_work() {
            return Err(ScanStartError::AlreadyScanning);
        }

        // And the same claim over the whole share, for the same reason: over the
        // wire the cover walk is the slowest we have, so the window where a
        // truncate could land under one is the widest here.
        let ground = self.claim_the_volume()?;

        // Resolve the live volume handle by id. Gone ⇒ the share unmounted; bail
        // so the caller resets to gray rather than scanning nothing.
        let volume = crate::indexing::host::volumes::current()
            .get(&self.volume_id)
            .ok_or_else(|| format!("Volume '{}' is not registered (unmounted?)", self.volume_id))?;

        // Capture tier-2 calibration before truncating (same flow as start_scan).
        // The per-kind bucket is picked below, once the walk kind is known.
        let calibration_set = IndexStore::read_scan_calibration_set(self.store.read_conn()).unwrap_or_default();
        let volume_root = self.volume_root.clone();
        let volume_used_bytes =
            tokio::task::block_in_place(|| crate::indexing::host::volumes::current().volume_used_bytes(&volume_root));

        // Pre-arm-before-snapshot: flip `ground_in_flux` BEFORE truncating, so any live
        // SMB change racing in during/after the truncate is BUFFERED by
        // `apply_smb_change` (which reads this flag) instead of being applied
        // against the gutted, half-rebuilt index and lost. The smb2 watcher has
        // been running continuously since connect, so its events are already on
        // the wire; this is the moment we start stashing them for post-scan
        // replay. The ordering survives a mid-scan watcher respawn: a respawned
        // watcher feeds the same buffer while this flag stays set.
        self.ground_in_flux.store(true, Ordering::Relaxed);

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
        let reconcile = mode == NetworkScanMode::Auto
            && IndexStore::get_entry_count(self.store.read_conn())
                .map(|n| n > 1)
                .unwrap_or(false);

        // The run's kind: what the frontend states, and which calibration bucket
        // this run reads from and writes back into. A trait reconcile and a trait
        // bulk build differ the same way the local pair does, so they keep
        // separate timings too.
        let run_kind = ScanRunKind::classify(reconcile, calibration_set.any.total_entries);
        let prior = calibration_set.for_kind(run_kind.calibration_kind());
        let calibration_kind = run_kind.calibration_kind();
        self.scan_calibration = Some(ScanCalibration {
            prior,
            volume_used_bytes,
            run_kind,
        });

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
            // Right after the truncate is the ONE moment this DB provably holds no
            // row beneath a dir the scanner refuses to walk, so it's where the index
            // records the NAS system-dir exclusion list it's built against. A
            // reconcile must never claim it: it doesn't list those dirs, so it can't
            // clear what an older list let in.
            let _ = self
                .writer
                .send(crate::indexing::network_scanner::exclusion_stamp_message());
            // Same moment, same reasoning, for the LOCAL exclusion policy. A network
            // walk doesn't run `should_exclude` at all (its own skip list is the NAS
            // one above), so the stamp is conservative rather than exact: it can only
            // over-report a policy change, which costs a re-walk, never claim a
            // coverage the rows don't support. Without any stamp, coverage answers
            // over this volume are worthless — see `store::EXCLUSION_POLICY_KEY`.
            let _ = self
                .writer
                .send(crate::indexing::scanner::exclusion_policy_stamp_message());
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
            self.events.as_ref(),
            &self.volume_id,
            super::freshness::FreshnessEvent::ScanStarted,
        );

        self.events.emit(IndexEvent::ScanStarted {
            volume_id: self.volume_id.clone(),
            run_kind,
            prior_total_entries: prior.total_entries,
            prior_scan_duration_ms: prior.scan_duration_ms,
            volume_used_bytes,
            // A trait scan takes the share whole, so the checklist shows the
            // network family of steps rather than the phased one.
            covered_in_phases: false,
        });
        // The ground: the whole share, reported the same way a phase reports its
        // branch.
        announce_whole_volume_walk(
            self.events.as_ref(),
            &self.volume_id,
            self.volume_root.to_string_lossy().into_owned(),
        );
        set_phase_for(
            self.events.as_ref(),
            &self.volume_id,
            ActivityPhase::Scanning,
            scan_trigger,
        );

        let progress = Arc::new(ScanProgress::new());
        // A CHILD of the volume's stop signal: stopping this scan leaves the
        // volume able to start another, while tearing the volume down stops it.
        let cancel = self.volume_cancel.child_token();
        self.scan_handle = Some(ScanHandle::new(Arc::clone(&progress), cancel.clone()));
        // `ground_in_flux` was already set true above (pre-arm before truncate).

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
        let partial_agg_source = if reconcile { AggSource::Sql } else { AggSource::Maps };
        ScanProgressReporter::new(
            Arc::clone(&progress),
            self.writer.clone(),
            Arc::clone(&self.events),
            self.volume_id.clone(),
            partial_agg_source,
        )
        .spawn(Arc::clone(&scan_done));

        // The walk + completion handler. Runs as a tokio task because the
        // `Volume` API is async. The writer is `Send` and shared by `Arc`.
        let writer = self.writer.clone();
        let events = Arc::clone(&self.events);
        let volume_id = self.volume_id.clone();
        let ground_in_flux = Arc::clone(&self.ground_in_flux);
        // Clone the freshness handle into the completion task so it fires the
        // `ScanCompleted` / `WatcherDied` transition through the `Arc` directly,
        // never re-locking the registry.
        let freshness = Arc::clone(&self.freshness);
        let root = self.volume_root.clone();
        // Kept alongside `root` (which the scan consumes): the completion handler
        // persists it as `volume_path` so search can strip the mount root off scope
        // paths (symmetry with the local scan-completion path).
        let volume_root_str = self.volume_root.to_string_lossy().into_owned();
        let kind = self.kind;
        // Pace the walk against foreground activity on THIS volume: while the user
        // browses the share, the walk drops to one listing in flight so a navigation
        // isn't queued behind the scan's backlog. See `indexing/network_scanner/scan_pace.rs`.
        let pacer = crate::indexing::network_scanner::scan_pace::ScanPacer::for_volume(self.volume_id.clone());
        // Kept alive across the walk to open/close the backend's scan-scoped
        // resources (SMB spreads the walk across a small pool of extra
        // connections; see `file_system/.../smb/scan_pool.rs`). Invisible to the
        // scanner, which keeps calling `list_directory_for_scan`. Default no-op on
        // backends without such resources (MTP, local).
        let scan_session_volume = Arc::clone(&volume);
        crate::indexing::host::runtime::spawn(async move {
            scan_session_volume.begin_scan_session().await;
            let result = if reconcile {
                crate::indexing::network_scanner::reconcile_volume_via_trait(
                    volume,
                    root,
                    writer.clone(),
                    progress,
                    cancel,
                    pacer,
                )
                .await
            } else {
                crate::indexing::network_scanner::scan_volume_via_trait(
                    volume,
                    root,
                    writer.clone(),
                    progress,
                    cancel,
                    pacer,
                )
                .await
            };
            // Tear the pool down on EVERY outcome (clean, cancel, disconnect,
            // error): this line runs before any completion-arm below.
            scan_session_volume.end_scan_session().await;

            scan_done.store(true, Ordering::Relaxed);
            ground_in_flux.store(false, Ordering::Relaxed);
            // And the share's ground goes back, now that nothing is walking it. The
            // claim rode into this task because the scan outlives `start_volume_scan`;
            // owned here, it is released on every arm below, on a cancel, and on a
            // panic alike.
            //
            // ⚠️ A rescan queued behind this scan runs at the END of this task, not
            // here: the arms below still write (the completion marker above all), and
            // a truncating rescan landing between the two would stamp this scan's
            // marker onto its own half-built index.
            drop(ground);

            // Three outcomes, three arms. `Ok` now means the walk FINISHED (a
            // cancel arrives as `VolumeScanError::Cancelled`, which is
            // deliberately not a terminal disconnect), so the completion marker
            // below can only be written for a whole scan.
            match result {
                Ok(summary) => {
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
                    // Both buckets: this walk kind's own keys (so the next run of the
                    // same kind gets a comparable ETA) and the unsuffixed
                    // last-completed-scan keys. Same split as the local path.
                    for (key, value) in [
                        ("scan_duration_ms", summary.duration_ms.to_string()),
                        ("total_entries", summary.total_entries.to_string()),
                        ("total_physical_bytes", summary.total_physical_bytes.to_string()),
                    ] {
                        let _ = writer.send(WriteMessage::UpdateMeta {
                            key: calibration_kind.meta_key(key),
                            value: value.clone(),
                        });
                        let _ = writer.send(WriteMessage::UpdateMeta {
                            key: key.to_string(),
                            value,
                        });
                    }
                    let _ = writer.send(WriteMessage::UpdateMeta {
                        key: "volume_path".to_string(),
                        value: volume_root_str.clone(),
                    });
                    let _ = writer.flush().await;

                    events.emit(IndexEvent::ScanComplete {
                        volume_id: volume_id.clone(),
                        total_entries: summary.total_entries,
                        total_dirs: summary.total_dirs,
                        duration_ms: summary.duration_ms,
                    });
                    events.emit(IndexEvent::AggregationComplete {
                        volume_id: volume_id.clone(),
                    });

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
                            events.as_ref(),
                            &volume_id,
                            super::freshness::FreshnessEvent::ScanCompleted,
                        );
                    }
                    set_phase_for(
                        events.as_ref(),
                        &volume_id,
                        ActivityPhase::Live,
                        "network scan complete",
                    );

                    // Tell the FE sizes are ready for this share's listings.
                    events.emit(IndexEvent::DirsUpdated {
                        paths: vec![volume_id.clone()],
                    });
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
                        events.as_ref(),
                        &volume_id,
                        super::freshness::FreshnessEvent::WatcherDied,
                    );
                    set_phase_for(
                        events.as_ref(),
                        &volume_id,
                        ActivityPhase::Idle,
                        "network scan disconnected (honest partial kept)",
                    );
                    // Clear the FE's live activity: the scan ended without a
                    // completion event, so without this the corner indicator and
                    // the breadcrumb badge tooltip would keep a stuck "scanning"
                    // row for this volume. The dot still flips to yellow (Stale)
                    // via the freshness change above.
                    events.emit(IndexEvent::ScanAborted {
                        volume_id: volume_id.clone(),
                    });
                }
                Err(e) => {
                    // User cancel, timeout, or another genuine abort: the partial
                    // is discardable. Reset the volume to gray / not-indexed and
                    // drop the changes buffered during the aborted scan.
                    match &e {
                        VolumeScanError::Cancelled(partial) => log::info!(
                            "network scan: cancelled for '{volume_id}' after {} entries, discarding partial",
                            partial.total_entries
                        ),
                        e => log::warn!("network scan: failed for '{volume_id}' ({e}), discarding partial"),
                    }
                    discard_buffered_changes_for_kind(kind, &volume_id);
                    super::state::reset_to_not_indexed(&volume_id);
                    // Clear the FE's live activity (no completion event fired for
                    // an aborted scan), so the corner indicator and badge tooltip
                    // don't keep a stuck "scanning" row. The dot reverts to gray
                    // (not-indexed) via the freshness reset above.
                    events.emit(IndexEvent::ScanAborted {
                        volume_id: volume_id.clone(),
                    });
                }
            }

            // This run is over on every arm, so the share can have the walk somebody
            // queued behind it. Refuses itself while anything still holds the ground.
            crate::indexing::lifecycle::rescan_request::run_if_owed(&volume_id);
        });

        Ok(())
    }
}

//! MustScanSubDirs rescan orchestration for the reconciler.
//!
//! One rescan runs at a time (`rescan_active`), on a dedicated `Utility`-QoS
//! thread; anchors queue in `pending_rescans` and drain automatically on
//! completion. Three behaviors the drain leans on: ancestor-collapse at pick time
//! (a queued descendant is redundant once its ancestor's reconcile re-lists the
//! whole subtree); the shared `active_rescan_path` slot the removal-storm drop
//! rule reads to see the in-flight rescan (the path is popped out of
//! `pending_rescans` at spawn); and the per-subtree [`RescanThrottle`], which caps
//! a churning anchor to ≤1 walk per window by picking only ELIGIBLE anchors and
//! leaving throttled ones queued for the sweep tick's re-kick.
//!
//! The "size updating" hourglass a rescan holds is decided in
//! [`hold`], which tracks the same eligibility: a queued-but-resting
//! anchor holds nothing.

mod churn;
mod hold;
// `route` and `throttle` are `pub(super)`: `reconciler.rs` re-exports the sweep
// record from one and holds a `RescanThrottle` field of the other. The rest of
// the scheduler is private to this module.
pub(super) mod route;
mod settle;
pub(super) mod throttle;

use self::hold::{
    adopt_picked_holds, hold_if_eligible, reconcile_with_eligibility, release_and_emit_completion, release_rescan_hold,
};
use self::route::{RescanRoute, SHALLOW_COALESCED_KEY, SHALLOW_SWEEP_AT_KEY, now_unix};
use self::throttle::RescanThrottle;
use super::{
    DEBUG_STATS, EventReconciler, IndexStore, IndexWriter, ReconcileSummary, RescanDrain, ScanTrigger, WriteMessage,
    reconcile_subtree,
};
use crate::indexing::lifecycle::manager;
use crate::indexing::paths::path_prefix;
use cmdr_fs::ignore_poison::IgnorePoison;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

impl EventReconciler {
    /// This reconciler's drain handles, cloned for a spawned walk.
    pub(super) fn rescan_drain(&self) -> RescanDrain {
        RescanDrain {
            pending: Arc::clone(&self.pending_rescans),
            active: Arc::clone(&self.rescan_active),
            active_path: Arc::clone(&self.active_rescan_path),
            throttle: Arc::clone(&self.rescan_throttle),
            space: self.space.clone(),
            volume_id: self.volume_id.clone(),
            cancel: self.cancel.clone(),
        }
    }

    /// Route a `MustScanSubDirs` anchor by depth (see [`route`]). The single
    /// entry point for the two feeders the churn-resilience fix targets — the live
    /// path (`process_live_event`) and the post-replay handoff (`event_loop::replay`):
    ///
    /// - **Shallow/root-scale** anchor: take the VISIBLE scanner path
    ///   ([`route_shallow_to_scanner`](Self::route_shallow_to_scanner)) — single-
    ///   flight, updates freshness, and (critically) NO per-dir hourglass hold, so a
    ///   continuously re-churning `/` can't leave the hold stuck for a ~20-min walk.
    /// - **Deep/narrow** anchor: keep the throttled `reconcile_subtree` drain, which
    ///   is exactly what it's good at.
    pub(in crate::indexing) fn route_must_scan_sub_dirs(&mut self, path: PathBuf, writer: &IndexWriter) {
        match route::classify(path_prefix::depth(&path.to_string_lossy())) {
            RescanRoute::Scanner => self.route_shallow_to_scanner(path, writer),
            RescanRoute::Reconcile => self.queue_must_scan_sub_dirs(path, writer),
        }
    }

    /// Request a VISIBLE full (re)scan for a shallow/root-scale anchor, gated by the
    /// per-volume once-a-day sweep window. Deliberately takes NO hourglass hold and
    /// never enters `pending_rescans`: the scanner path is visible and single-flight,
    /// and holding the per-dir hourglass for a root-scale reconcile is the stuck-
    /// hourglass bug this replaces.
    ///
    /// Inside the window we do NOT sweep, and the skipped signal is not forgotten:
    /// it's COUNTED and persisted, so the volume tooltip can say how many change
    /// signals macOS lost and when the next sweep is due. The badge deliberately
    /// stays green — once-a-day sweeping is the DESIGNED operating state, not a
    /// fault, and a fault colour shown all day trains people to ignore it.
    ///
    /// The window is boot-disk-only ([`route::min_interval_for`]); a
    /// mount-rooted external drive keeps the short cooldown. See
    /// `route::SHALLOW_RESCAN_MIN_INTERVAL` for the measurements.
    fn route_shallow_to_scanner(&mut self, anchor: PathBuf, writer: &IndexWriter) {
        DEBUG_STATS.record_must_scan(&anchor.to_string_lossy());
        let (action, record) = route::decide_shallow_anchor(
            &self.volume_id,
            now_unix(),
            route::min_interval_for(self.space.is_boot_disk()),
        );
        if action == route::ShallowAnchorAction::Coalesce {
            log::info!(
                "MustScanSubDirs: shallow anchor {} inside the sweep window; coalescing ({} since the last sweep)",
                anchor.display(),
                record.coalesced_since_sweep,
            );
            // Mirror the count into `meta` so it survives relaunch: the window spans
            // many restarts, and a count that reset on launch would under-report.
            let _ = writer.send(WriteMessage::UpdateMeta {
                key: SHALLOW_COALESCED_KEY.to_string(),
                value: record.coalesced_since_sweep.to_string(),
            });
            return;
        }
        // Stamp the TRIGGER time, not only the completion: `start_scan` deletes
        // `scan_completed_at` before walking, so without this an interrupted sweep
        // would leave the window looking permanently expired and we'd sweep on every
        // launch. See `route::SweepRecord::last_sweep_unix`.
        if let Some(at) = record.last_sweep_unix {
            let _ = writer.send(WriteMessage::UpdateMeta {
                key: SHALLOW_SWEEP_AT_KEY.to_string(),
                value: at.to_string(),
            });
        }
        let label = format!("shallow MustScanSubDirs ({})", anchor.display());
        log::info!(
            "MustScanSubDirs: routing shallow anchor {} to the visible scanner",
            anchor.display()
        );
        match &self.scan_trigger {
            ScanTrigger::Registry => {
                let volume_id = self.volume_id.clone();
                // Fire-and-forget: `perform_registry_rescan` re-resolves the manager
                // in the registry and runs a fresh single-flight `start_scan`. Spawn
                // (not inline) because we hold a read `Connection` on the live loop.
                crate::indexing::host::runtime::spawn(async move {
                    manager::perform_registry_rescan(&volume_id, &label).await;
                });
            }
            #[cfg(test)]
            ScanTrigger::Disabled => {}
            #[cfg(test)]
            ScanTrigger::Recording(sink) => sink.lock_ignore_poison().push(label),
        }
    }

    /// Queue a MustScanSubDirs rescan on the throttled reconcile drain, capped to
    /// max 1 concurrent. This is the DEEP-anchor path; shallow anchors route to the
    /// scanner via [`route_must_scan_sub_dirs`](Self::route_must_scan_sub_dirs).
    pub(in crate::indexing) fn queue_must_scan_sub_dirs(&mut self, path: PathBuf, writer: &IndexWriter) {
        // On a branch-watched volume the walk owns coverage growth: an anchor
        // outside the covered branches would have the watcher indexing ground
        // nobody asked for. That ground stays frontier, so the next search over it
        // walks it — which is where growing coverage belongs.
        if !self.may_walk(&path) {
            log::trace!(
                "Reconciler: leaving {} to the next search; it's outside this volume's walked branches",
                path.display()
            );
            return;
        }
        DEBUG_STATS.record_must_scan(&path.to_string_lossy());
        // Stat the anchor for its birthtime BEFORE it's queued or held: a subtree
        // created seconds ago is still being written (an updater unpacking a
        // bundle), and walking it indexes rows for data that's usually deleted
        // before we finish. See `settle`.
        settle::note_settle_deadline(&self.rescan_throttle, &path, Instant::now());
        // A signal for an anchor that may not walk yet is one walk the throttle or
        // the settle delay just absorbed. Counted HERE, on the real signal path, and
        // deliberately not in `requeue_rescan`: a removal storm re-queues thousands
        // of times for one scope and would drown the number. See `churn`.
        if !self
            .rescan_throttle
            .lock_ignore_poison()
            .is_eligible(&path, Instant::now())
        {
            churn::record_held_back();
        }
        // A signal that lands mid-walk waits for the single-flight drain. Counted
        // here and not in `enqueue_rescan` for the same reason as `held_back`: a
        // removal storm re-queues one scope thousands of times, and folding those
        // in would turn a queue-pressure number into a storm detector.
        if self.rescan_active.load(Ordering::Relaxed) {
            churn::record_queued_while_active();
        }
        self.enqueue_rescan(path, writer);
    }

    /// Re-queue a rescan anchor without the `DEBUG_STATS` bookkeeping or the
    /// settle stat. Used by the removal-storm drop rule, which fires once per
    /// dropped event (thousands in a storm) — the debug ring buffer, the counter,
    /// and a syscall per dropped event would all just churn, and the scope being
    /// re-queued is already queued or walking, so its settle verdict is already
    /// recorded. Set-dedup makes
    /// re-inserting the already-queued (or active) anchor idempotent; if it's the
    /// ACTIVE rescan's path (popped out of `pending_rescans`), re-inserting
    /// schedules the follow-up pass the tail events need.
    pub(in crate::indexing) fn requeue_rescan(&mut self, path: PathBuf, writer: &IndexWriter) {
        self.enqueue_rescan(path, writer);
    }

    /// Insert an anchor into `pending_rescans` and start a rescan if none runs.
    fn enqueue_rescan(&mut self, path: PathBuf, writer: &IndexWriter) {
        self.pending_rescans.lock_ignore_poison().insert(path.clone());
        // Hold the rescan-root hourglass on THIS volume's tracker (it survives the
        // writer-drain clear) only while a walk is in flight or imminent — a
        // throttled anchor stays quiet. Set-insert, so a re-queue of the already-held
        // active path is a no-op. See `hold`'s invariant for the full lifecycle.
        hold_if_eligible(&self.volume_id, &path, &self.rescan_throttle, Instant::now());

        if self.rescan_active.load(Ordering::Relaxed) {
            // TRACE, not DEBUG: the paths are unique (a compiler's fingerprint
            // dirs), so consecutive-line dedup never fires and this was ~4,000
            // lines an hour on an ordinary build machine, a quarter of the whole
            // log. What a reader needs from it is the RATE, and that rides
            // `churn`'s 15-minute line as `queued behind a running rescan`,
            // which is in the bundle. The paths themselves are still one
            // `RUST_LOG=cmdr_lib::indexing::reconcile=trace` away, and
            // `DEBUG_STATS.record_must_scan` keeps the recent ones in memory.
            log::trace!(
                "Reconciler: MustScanSubDirs for {} queued (rescan already active)",
                path.display()
            );
            return;
        }

        start_next_rescan(self.rescan_drain(), writer);
    }

    /// Start a rescan if any are pending and none is running. Drains rescans that
    /// were DEFERRED into `pending_rescans` during buffered replay (no live queueing
    /// then); the live loop calls this once at startup so those anchors run.
    pub(in crate::indexing) fn kick_pending_rescans(&mut self, writer: &IndexWriter) {
        if self.rescan_active.load(Ordering::Relaxed) {
            return;
        }
        if self.pending_rescans.lock_ignore_poison().is_empty() {
            return;
        }
        start_next_rescan(self.rescan_drain(), writer);
    }

    /// Trailing edge of the per-subtree throttle, driven by the event loop's
    /// ~1 s sweep tick (the same tick as [`Self::sweep_throttle`]). Re-kicks the
    /// drain so an anchor that was held back because its window hadn't elapsed
    /// reconciles once it has: this is what guarantees a hard-churning subtree
    /// re-walks every window and never starves. Also re-derives each queued
    /// anchor's hourglass hold from its current eligibility (see `hold`), and
    /// garbage-collects throttle records for anchors no longer pending, so the map
    /// stays bounded by the count of actively-churning subtrees.
    pub(in crate::indexing) fn sweep_rescan_throttle(&mut self, writer: &IndexWriter) {
        {
            let pending = self.pending_rescans.lock_ignore_poison();
            let mut throttle = self.rescan_throttle.lock_ignore_poison();
            let now = Instant::now();
            throttle.gc(&pending, now);
            // The in-flight walk is out of `pending`, but a storm can re-queue it;
            // pass it so its hold survives (`hold`'s no-unheld-write rule).
            let active = self.active_rescan_path.lock_ignore_poison().clone();
            reconcile_with_eligibility(&self.volume_id, &pending, active.as_ref(), &throttle, now);
        }
        // Close the churn window when it's due. Without a tick, a burst followed by
        // silence would sit unreported until the next reconcile, hours later.
        churn::poll_window();
        self.kick_pending_rescans(writer);
    }

    /// Snapshot the set of queued-or-active rescan scopes: every path in
    /// `pending_rescans` plus the currently-running rescan's path. The
    /// removal-storm drop rule tests each removal event against these prefixes.
    pub(in crate::indexing) fn rescan_scopes(&self) -> Vec<PathBuf> {
        let mut scopes: Vec<PathBuf> = self.pending_rescans.lock_ignore_poison().iter().cloned().collect();
        if let Some(active) = self.active_rescan_path.lock_ignore_poison().clone() {
            scopes.push(active);
        }
        scopes
    }

    /// Test-only: force the `rescan_active` flag so a queued anchor stays in
    /// `pending_rescans` (no spawn) for deterministic assertions.
    #[cfg(test)]
    pub(in crate::indexing) fn set_rescan_active_for_test(&self, active: bool) {
        self.rescan_active.store(active, Ordering::Relaxed);
    }

    /// Test-only: snapshot the queued rescan paths (order-independent).
    #[cfg(test)]
    pub(in crate::indexing) fn pending_rescans_snapshot(&self) -> Vec<PathBuf> {
        self.pending_rescans.lock_ignore_poison().iter().cloned().collect()
    }

    /// Test-only: whether a rescan task is currently running. Used by the stress
    /// test's fixed-point quiescence loop.
    #[cfg(test)]
    pub(in crate::indexing) fn is_rescan_active_for_test(&self) -> bool {
        self.rescan_active.load(Ordering::Relaxed)
    }

    /// Test-only: seed a queued rescan scope (simulates a rescan already covering
    /// this path, so the removal-storm drop rule can see it).
    #[cfg(test)]
    pub(in crate::indexing) fn insert_pending_rescan_for_test(&self, path: PathBuf) {
        self.pending_rescans.lock_ignore_poison().insert(path);
    }

    /// Test-only: record a rescan completion for `path`, putting it inside the
    /// throttle window its `walk_cost` earns. The "queued but resting" state the
    /// hourglass-hold tests need, without running a real walk.
    #[cfg(test)]
    pub(in crate::indexing) fn record_rescan_completion_for_test(&self, path: &Path, walk_cost: Duration) {
        self.rescan_throttle
            .lock_ignore_poison()
            .record_completion(path, Instant::now(), walk_cost);
    }

    /// Test-only: name the in-flight rescan, so the sweep tick sees a walk it must
    /// not disturb (production sets this at spawn).
    #[cfg(test)]
    pub(in crate::indexing) fn set_active_rescan_path_for_test(&self, path: Option<PathBuf>) {
        *self.active_rescan_path.lock_ignore_poison() = path;
    }

    /// Test-only: zero both throttle bounds AND the settle delay, so every anchor
    /// is always eligible. The storm/stress fixed-point tests use this so a
    /// re-queued anchor drains immediately instead of lingering in
    /// `pending_rescans` — they queue brand-new temp dirs, which the settle delay
    /// would otherwise hold back past the test's budget. Cadence itself is covered
    /// by `throttle`'s unit tests.
    #[cfg(test)]
    pub(in crate::indexing) fn disable_rescan_throttle_for_test(&self) {
        let mut throttle = self.rescan_throttle.lock_ignore_poison();
        *throttle = RescanThrottle::with_bounds(Duration::ZERO, Duration::ZERO);
        throttle.set_settle_delay(Duration::ZERO);
    }

    /// Test-only: shorten the settle delay. Zero means "every directory reads as
    /// established", the pre-settle-delay behavior.
    #[cfg(test)]
    pub(in crate::indexing) fn set_settle_delay_for_test(&self, delay: Duration) {
        self.rescan_throttle.lock_ignore_poison().set_settle_delay(delay);
    }
}

/// Pick the next rescan anchor from the pending set: the SHALLOWEST ELIGIBLE
/// queued path (fewest components), then drop it AND every queued STRICT
/// descendant of it. An ancestor's reconcile re-lists the whole subtree, so a
/// queued descendant is redundant — collapsing bounds an escalation or removal
/// storm to ONE subtree walk instead of one per level. Returns the picked anchor
/// plus the dropped descendants (so the caller can release their held-hourglass
/// roots — the picked ancestor's hold now covers them), or `None` when nothing is
/// eligible (empty set, or every queued anchor is still inside its throttle
/// window — the sweep tick retries once a window elapses).
///
/// Eligibility is the per-subtree throttle: an anchor reconciled less than the
/// window ago is skipped (left pending), so a hard-churning subtree re-walks at
/// most once per window. A never-walked anchor is always eligible (the leading
/// edge), so a freshly-dirty subtree still reconciles promptly. Strict
/// descendants are dropped whether or not THEY are eligible: the picked ancestor's
/// walk re-lists them regardless.
pub(super) fn pick_and_collapse_rescan(
    pending: &mut HashSet<PathBuf>,
    throttle: &RescanThrottle,
    now: Instant,
) -> Option<(PathBuf, Vec<PathBuf>)> {
    let picked = pending
        .iter()
        .filter(|p| throttle.is_eligible(p, now))
        .min_by_key(|p| path_prefix::depth(&p.to_string_lossy()))
        .cloned()?;
    let picked_str = picked.to_string_lossy().to_string();
    let dropped: Vec<PathBuf> = pending
        .iter()
        .filter(|q| **q != picked && path_prefix::is_strict_descendant(&q.to_string_lossy(), &picked_str))
        .cloned()
        .collect();
    pending.retain(|q| *q != picked && !path_prefix::is_strict_descendant(&q.to_string_lossy(), &picked_str));
    Some((picked, dropped))
}

/// Start the next pending MustScanSubDirs rescan, if any.
///
/// Standalone function (not a method) so the spawned task can call it after
/// completion to drain the pending queue automatically.
pub(super) fn start_next_rescan(drain: RescanDrain, writer: &IndexWriter) {
    let RescanDrain {
        pending: pending_rescans,
        active: rescan_active,
        active_path: active_rescan_path,
        throttle: rescan_throttle,
        space,
        volume_id,
        cancel: volume_cancel,
    } = drain;
    let path = {
        let mut pending = pending_rescans.lock_ignore_poison();
        let throttle = rescan_throttle.lock_ignore_poison();
        // Lock order is always pending → throttle → active-path where more than one
        // is held (here, and in `sweep_rescan_throttle`); every other site takes one
        // alone or in that order, so there's no inverse.
        match pick_and_collapse_rescan(&mut pending, &throttle, Instant::now()) {
            Some((picked, dropped)) => {
                // Take the hourglass for the walk that's about to start, and free the
                // collapsed descendants (now covered by `picked`'s hold). Under the
                // `pending` lock, and `picked` is already out of the set, so a
                // concurrent sweep can't disagree about either.
                adopt_picked_holds(&volume_id, &picked, &dropped);
                picked
            }
            None => return,
        }
    };
    rescan_active.store(true, Ordering::Relaxed);
    // Retain the active path in a shared slot so the removal-storm drop rule can
    // see the in-flight rescan (it's no longer in `pending_rescans`).
    *active_rescan_path.lock_ignore_poison() = Some(path.clone());

    let writer = writer.clone();
    let pending_for_task = Arc::clone(&pending_rescans);
    let active_for_task = Arc::clone(&rescan_active);
    let active_path_for_task = Arc::clone(&active_rescan_path);
    let throttle_for_task = Arc::clone(&rescan_throttle);
    let space_for_task = space.clone();
    let volume_id_for_task = volume_id.clone();
    // A child of THIS volume's stop signal, taken BEFORE the walk starts, so
    // tearing the index down stops a long subtree walk instead of letting it write
    // into a writer that's shutting down.
    let cancel = volume_cancel.child_token();
    // The same handles again, as one value, for the self-drain call this walk makes
    // when it finishes (on either exit path — the borrow checker sees the early
    // return, so one binding covers both).
    let drain_for_next = RescanDrain {
        pending: Arc::clone(&pending_rescans),
        active: Arc::clone(&rescan_active),
        active_path: Arc::clone(&active_rescan_path),
        throttle: Arc::clone(&rescan_throttle),
        space: space.clone(),
        volume_id: volume_id.clone(),
        cancel: volume_cancel,
    };

    // Debug, not info: this is one line per walk, thousands a day, and it's paired
    // with a completion line that carries the duration. The info-level signal for
    // the drain as a whole is [`churn`]'s 15-minute aggregate.
    log::debug!("MustScanSubDirs: reconcile starting for {}", path.display());

    // Kept for the rare spawn-failure handler below (the closure moves `path`).
    let path_for_spawn_failure = path.clone();

    // A DEDICATED thread (not the tokio blocking pool) so we can lower it to
    // `Utility` QoS: this background subtree walk must never outrank the webview
    // for CPU, matching the scanner and local-reconcile threads. QoS on a pooled
    // thread would leak onto later unrelated tasks, so `thread_qos` forbids it.
    // One thread per rescan is fine: the drain is single-flight and per-subtree
    // throttled, so spawns are infrequent. Panics unwind this thread only
    // (`panic=unwind`), same as the pool task it replaces.
    let spawn_result = std::thread::Builder::new()
        .name("rescan-subtree".into())
        .spawn(move || {
            cmdr_fs::thread_qos::set_current_thread_qos(cmdr_fs::thread_qos::QosClass::Utility);
            // The reconciler holds a READ connection (invariant: reconciler/event
            // loops never open a write connection — a write conn contends with the
            // writer thread and `SQLITE_BUSY` silently kills live indexing). Every
            // reconcile_subtree DB access is a read; writes ride the writer channel.
            let conn = match IndexStore::open_read_connection(&writer.db_path()) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!(
                        "MustScanSubDirs: couldn't open read connection for {}: {e}",
                        path.display()
                    );
                    // Release this root's hourglass before recursing to the next
                    // rescan. No completion was recorded, so a re-queued anchor is
                    // still eligible and keeps the hold for its imminent retry.
                    release_rescan_hold(
                        &volume_id_for_task,
                        &path,
                        &pending_for_task,
                        &throttle_for_task,
                        Instant::now(),
                    );
                    active_for_task.store(false, Ordering::Relaxed);
                    *active_path_for_task.lock_ignore_poison() = None;
                    // Try the next pending rescan even if this one failed
                    start_next_rescan(drain_for_next, &writer);
                    return;
                }
            };

            let (escalation, walk_cost) = match reconcile_subtree(&path, &space_for_task, &conn, &writer, &cancel) {
                Ok(summary) => {
                    let (level, message) = reconcile_report(&path, &summary);
                    log::log!(level, "{message}");
                    let walk_cost = summary.walk_cost();
                    // Feed the 15-minute aggregate that replaces this line at info.
                    // Only a walk that finished is counted: a failed one measured
                    // nothing, so it would report as free churn.
                    churn::record_reconcile(&path, walk_cost, summary.added + summary.removed + summary.updated);
                    (summary.escalation, walk_cost)
                }
                Err(e) => {
                    log::warn!("MustScanSubDirs: reconcile failed for {}: {e}", path.display());
                    // No measured walk, so the throttle falls back to its floor.
                    (None, Duration::ZERO)
                }
            };

            // The subtree's chain was still (partly) missing: re-queue the anchor the
            // skip branch resolved (strictly closer to the volume root, so this
            // converges by depth). Hold its hourglass if it may walk now, so the
            // follow-up rescan is covered from the moment it's queued. The anchor is a
            // proper ancestor of `path` (never equal), so it doesn't affect `path`'s
            // own release decision. The drain below picks it up.
            if let Some(anchor) = escalation {
                // Same settle question as any other enqueue: the missing chain is
                // often missing precisely BECAUSE it was created seconds ago, and
                // that is the subtree we don't want to walk yet.
                settle::note_settle_deadline(&throttle_for_task, &anchor, Instant::now());
                hold_if_eligible(&volume_id_for_task, &anchor, &throttle_for_task, Instant::now());
                pending_for_task.lock_ignore_poison().insert(anchor);
            }

            // Record this subtree's reconcile so the per-subtree throttle holds the
            // anchor back until the window elapses. The window scales with what THIS
            // walk cost, so an expensive anchor backs off further. A hard-churning
            // subtree that re-queues immediately stays pending but won't re-walk until
            // then; the sweep tick's re-kick fires it at the window boundary (the
            // trailing edge).
            throttle_for_task
                .lock_ignore_poison()
                .record_completion(&path, Instant::now(), walk_cost);

            // Release this root's hourglass and emit the in-place refresh for the root
            // + its ancestor chain. The completion above is already recorded, so a
            // churning re-queue reads THROTTLED here and releases: a resting anchor
            // must not hold `~` and `/` in the hourglass for its whole back-off.
            // Release precedes the emit so the triggered refetch reads
            // `pending == false`; the emit rides the writer so it lands after the
            // reconcile's writes.
            release_and_emit_completion(
                &volume_id_for_task,
                &path,
                &pending_for_task,
                &throttle_for_task,
                Instant::now(),
                &writer,
            );

            DEBUG_STATS.record_rescan_completed();
            active_for_task.store(false, Ordering::Relaxed);
            *active_path_for_task.lock_ignore_poison() = None;

            // Automatically start the next queued rescan
            start_next_rescan(drain_for_next, &writer);
        });

    if let Err(e) = spawn_result {
        // Spawning the rescan thread failed (a rare resource limit). Undo the
        // in-flight flags set just above so the single-flight drain isn't wedged,
        // and drop this anchor's hourglass; the next enqueue or sweep re-kicks.
        log::warn!(
            "MustScanSubDirs: couldn't spawn rescan thread for {}: {e}",
            path_for_spawn_failure.display()
        );
        rescan_active.store(false, Ordering::Relaxed);
        *active_rescan_path.lock_ignore_poison() = None;
        release_rescan_hold(
            &volume_id,
            &path_for_spawn_failure,
            &pending_rescans,
            &rescan_throttle,
            Instant::now(),
        );
    }
}

/// How long a reconcile has to run before it's worth a line above `debug`.
const RECONCILE_SLOW_SECS: u64 = 10;

/// The line one finished subtree reconcile emits: `(level, message)`. Pure, so the
/// wording and the level policy are unit-testable without a logger.
///
/// An ordinary reconcile is DEBUG. There are thousands a day and most of them
/// change nothing, so at info they buried the two lines that mattered. The
/// info-level answer to "are we reconciling too much?" is [`churn`]'s
/// 15-minute aggregate, which one line can actually carry.
///
/// A long reconcile is only newsworthy if the walk itself was slow. Time parked on
/// the writer queue lands inside the same duration with nothing to attribute it to,
/// which is how "reconcile slow … (+7 -0 ~0, 21s)" came to mean "the writer was
/// saturated for 19 of those seconds". So the wait is named in the line, and when
/// it DOMINATES the line stays at `debug`: writer saturation already has its own
/// signal (the writer heartbeat), and repeating it under the reconciler's name is
/// worse than not repeating it at all.
fn reconcile_report(path: &Path, summary: &ReconcileSummary) -> (log::Level, String) {
    let changes = format!("+{} -{} ~{}", summary.added, summary.removed, summary.updated);
    // The one aggregate that replaced a per-path DEBUG line. Printed only when it
    // happened: on a healthy walk it's zero, and "0 unreadable dirs" on every line
    // is the noise this change removed.
    let unreadable = if summary.unreadable_dirs == 0 {
        String::new()
    } else {
        format!(
            ", {}",
            cmdr_fs::pluralize::pluralize_grouped(summary.unreadable_dirs, "unreadable dir")
        )
    };
    if summary.duration.as_secs() <= RECONCILE_SLOW_SECS {
        return (
            log::Level::Debug,
            format!(
                "MustScanSubDirs: reconcile complete for {} ({changes}{unreadable}, {}ms)",
                path.display(),
                summary.duration.as_millis(),
            ),
        );
    }

    let waited = summary.writer_wait.min(summary.duration);
    let wait_dominated = waited * 2 > summary.duration;
    let attribution = if waited.as_secs() > 0 {
        format!(", {}s waiting on the writer", waited.as_secs())
    } else {
        String::new()
    };
    let level = if wait_dominated {
        log::Level::Debug
    } else {
        log::Level::Warn
    };
    let what = if wait_dominated {
        "reconcile waited"
    } else {
        "reconcile slow"
    };
    (
        level,
        format!(
            "MustScanSubDirs: {what} for {} ({changes}{unreadable}, {}s{attribution})",
            path.display(),
            summary.duration.as_secs(),
        ),
    )
}

#[cfg(test)]
mod tests;

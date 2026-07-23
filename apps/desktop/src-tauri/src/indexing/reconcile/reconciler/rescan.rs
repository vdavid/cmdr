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
//! [`super::rescan_hold`], which tracks the same eligibility: a queued-but-resting
//! anchor holds nothing.

use super::rescan_churn;
use super::rescan_hold::{
    adopt_picked_holds, hold_if_eligible, reconcile_with_eligibility, release_and_emit_completion, release_rescan_hold,
};
use super::rescan_route::{self, RescanRoute};
use super::rescan_settle;
use super::rescan_throttle::RescanThrottle;
use super::*;
use crate::indexing::lifecycle::manager;
use crate::indexing::paths::path_prefix;

impl EventReconciler {
    /// Route a `MustScanSubDirs` anchor by depth (see [`rescan_route`]). The single
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
        match rescan_route::classify(path_prefix::depth(&path.to_string_lossy())) {
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
    /// The window is boot-disk-only ([`rescan_route::min_interval_for`]); a
    /// mount-rooted external drive keeps the short cooldown. See
    /// `rescan_route::SHALLOW_RESCAN_MIN_INTERVAL` for the measurements.
    fn route_shallow_to_scanner(&mut self, anchor: PathBuf, writer: &IndexWriter) {
        DEBUG_STATS.record_must_scan(&anchor.to_string_lossy());
        let (action, record) = rescan_route::decide_shallow_anchor(
            &self.volume_id,
            now_unix(),
            rescan_route::min_interval_for(self.space.is_boot_disk()),
        );
        if action == rescan_route::ShallowAnchorAction::Coalesce {
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
        // launch. See `rescan_route::SweepRecord::last_sweep_unix`.
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
                tauri::async_runtime::spawn(async move {
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
        DEBUG_STATS.record_must_scan(&path.to_string_lossy());
        // Stat the anchor for its birthtime BEFORE it's queued or held: a subtree
        // created seconds ago is still being written (an updater unpacking a
        // bundle), and walking it indexes rows for data that's usually deleted
        // before we finish. See `rescan_settle`.
        rescan_settle::note_settle_deadline(&self.rescan_throttle, &path, Instant::now());
        // A signal for an anchor that may not walk yet is one walk the throttle or
        // the settle delay just absorbed. Counted HERE, on the real signal path, and
        // deliberately not in `requeue_rescan`: a removal storm re-queues thousands
        // of times for one scope and would drown the number. See `rescan_churn`.
        if !self
            .rescan_throttle
            .lock_ignore_poison()
            .is_eligible(&path, Instant::now())
        {
            rescan_churn::record_held_back();
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
        // active path is a no-op. See `rescan_hold`'s invariant for the full lifecycle.
        hold_if_eligible(&self.volume_id, &path, &self.rescan_throttle, Instant::now());

        if self.rescan_active.load(Ordering::Relaxed) {
            log::debug!(
                "Reconciler: MustScanSubDirs for {} queued (rescan already active)",
                path.display()
            );
            return;
        }

        start_next_rescan(
            Arc::clone(&self.pending_rescans),
            Arc::clone(&self.rescan_active),
            Arc::clone(&self.active_rescan_path),
            Arc::clone(&self.rescan_throttle),
            self.space.clone(),
            self.volume_id.clone(),
            writer,
        );
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
        start_next_rescan(
            Arc::clone(&self.pending_rescans),
            Arc::clone(&self.rescan_active),
            Arc::clone(&self.active_rescan_path),
            Arc::clone(&self.rescan_throttle),
            self.space.clone(),
            self.volume_id.clone(),
            writer,
        );
    }

    /// Trailing edge of the per-subtree throttle, driven by the event loop's
    /// ~1 s sweep tick (the same tick as [`Self::sweep_throttle`]). Re-kicks the
    /// drain so an anchor that was held back because its window hadn't elapsed
    /// reconciles once it has: this is what guarantees a hard-churning subtree
    /// re-walks every window and never starves. Also re-derives each queued
    /// anchor's hourglass hold from its current eligibility (see `rescan_hold`), and
    /// garbage-collects throttle records for anchors no longer pending, so the map
    /// stays bounded by the count of actively-churning subtrees.
    pub(in crate::indexing) fn sweep_rescan_throttle(&mut self, writer: &IndexWriter) {
        {
            let pending = self.pending_rescans.lock_ignore_poison();
            let mut throttle = self.rescan_throttle.lock_ignore_poison();
            let now = Instant::now();
            throttle.gc(&pending, now);
            // The in-flight walk is out of `pending`, but a storm can re-queue it;
            // pass it so its hold survives (`rescan_hold`'s no-unheld-write rule).
            let active = self.active_rescan_path.lock_ignore_poison().clone();
            reconcile_with_eligibility(&self.volume_id, &pending, active.as_ref(), &throttle, now);
        }
        // Close the churn window when it's due. Without a tick, a burst followed by
        // silence would sit unreported until the next reconcile, hours later.
        rescan_churn::poll_window();
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
    /// by `rescan_throttle`'s unit tests.
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
pub(super) fn start_next_rescan(
    pending_rescans: Arc<Mutex<HashSet<PathBuf>>>,
    rescan_active: Arc<AtomicBool>,
    active_rescan_path: Arc<Mutex<Option<PathBuf>>>,
    rescan_throttle: Arc<Mutex<RescanThrottle>>,
    space: IndexPathSpace,
    volume_id: String,
    writer: &IndexWriter,
) {
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

    // Debug, not info: this is one line per walk, thousands a day, and it's paired
    // with a completion line that carries the duration. The info-level signal for
    // the drain as a whole is [`rescan_churn`]'s 15-minute aggregate.
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
            crate::thread_qos::set_current_thread_qos(crate::thread_qos::QosClass::Utility);
            let cancelled = AtomicBool::new(false);
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
                    start_next_rescan(
                        pending_for_task,
                        active_for_task,
                        active_path_for_task,
                        throttle_for_task,
                        space_for_task,
                        volume_id_for_task,
                        &writer,
                    );
                    return;
                }
            };

            let (escalation, walk_cost) = match reconcile_subtree(&path, &space_for_task, &conn, &writer, &cancelled) {
                Ok(summary) => {
                    let (level, message) = reconcile_report(&path, &summary);
                    log::log!(level, "{message}");
                    let walk_cost = summary.walk_cost();
                    // Feed the 15-minute aggregate that replaces this line at info.
                    // Only a walk that finished is counted: a failed one measured
                    // nothing, so it would report as free churn.
                    rescan_churn::record_reconcile(&path, walk_cost, summary.added + summary.removed + summary.updated);
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
                rescan_settle::note_settle_deadline(&throttle_for_task, &anchor, Instant::now());
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
            start_next_rescan(
                pending_for_task,
                active_for_task,
                active_path_for_task,
                throttle_for_task,
                space_for_task,
                volume_id_for_task,
                &writer,
            );
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
/// info-level answer to "are we reconciling too much?" is [`rescan_churn`]'s
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
    if summary.duration.as_secs() <= RECONCILE_SLOW_SECS {
        return (
            log::Level::Debug,
            format!(
                "MustScanSubDirs: reconcile complete for {} ({changes}, {}ms)",
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
            "MustScanSubDirs: {what} for {} ({changes}, {}s{attribution})",
            path.display(),
            summary.duration.as_secs(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::super::rescan_settle::NEW_SUBTREE_SETTLE_DELAY;
    use super::super::rescan_throttle::RESCAN_THROTTLE_WINDOW;
    use super::*;

    fn summary(duration: Duration, writer_wait: Duration) -> ReconcileSummary {
        ReconcileSummary {
            added: 7,
            removed: 0,
            updated: 0,
            duration,
            writer_wait,
            escalation: None,
        }
    }

    /// A long reconcile that was mostly WAITING is not a slow walk, and saying
    /// "reconcile slow" sends a reader hunting in the reconciler when the whole
    /// story is in the writer. The wait belongs in the line.
    #[test]
    fn a_reconcile_dominated_by_the_writer_wait_says_so_and_stays_quiet() {
        let (level, message) = reconcile_report(
            Path::new("/tmp/site-data"),
            &summary(Duration::from_secs(21), Duration::from_secs(19)),
        );
        assert_eq!(
            level,
            log::Level::Debug,
            "writer saturation is already reported by the writer heartbeat, so this line is a duplicate signal"
        );
        assert_eq!(
            message,
            "MustScanSubDirs: reconcile waited for /tmp/site-data (+7 -0 ~0, 21s, 19s waiting on the writer)"
        );
    }

    /// A genuinely slow WALK (the reconcile really was doing the work) still warns,
    /// which is what the line was for.
    #[test]
    fn a_slow_walk_that_was_not_waiting_still_warns() {
        let (level, message) = reconcile_report(
            Path::new("/tmp/deep-tree"),
            &summary(Duration::from_secs(21), Duration::from_millis(300)),
        );
        assert_eq!(level, log::Level::Warn);
        assert_eq!(
            message,
            "MustScanSubDirs: reconcile slow for /tmp/deep-tree (+7 -0 ~0, 21s)"
        );
    }

    /// The ordinary case is DEBUG: one line per walk, thousands a day, and most of
    /// them `+0 -0 ~0`. The signal that a reader needs at info is the 15-minute
    /// aggregate ([`super::rescan_churn`]), not the per-walk line. Content is
    /// unchanged, so `RUST_LOG` still gives the full picture.
    #[test]
    fn a_quick_reconcile_stays_out_of_the_way() {
        let (level, message) = reconcile_report(
            Path::new("/tmp/quick"),
            &summary(Duration::from_millis(120), Duration::ZERO),
        );
        assert_eq!(level, log::Level::Debug);
        assert_eq!(
            message,
            "MustScanSubDirs: reconcile complete for /tmp/quick (+7 -0 ~0, 120ms)"
        );
    }

    /// The throttle charges an anchor for its WALK, not for the reconcile's wall
    /// clock: time parked on a saturated writer queue is the writer's, not the
    /// anchor's. Charging it would let one transient global saturation (an initial
    /// scan, say) inflate every anchor's measured cost at once and back the whole
    /// volume off for half an hour.
    #[test]
    fn walk_cost_charges_the_walk_not_the_writer_wait() {
        let waited = summary(Duration::from_secs(20), Duration::from_secs(19));
        assert_eq!(
            waited.walk_cost(),
            Duration::from_secs(1),
            "a 20 s reconcile with 19 s on the writer queue is a 1 s walk"
        );

        let t0 = Instant::now();
        let mut throttle = RescanThrottle::new();
        throttle.record_completion(Path::new("/waited"), t0, waited.walk_cost());
        assert!(
            throttle.is_eligible(Path::new("/waited"), t0 + RESCAN_THROTTLE_WINDOW),
            "a 1 s walk earns the floor window"
        );

        // What charging the full duration would have done, for contrast.
        let mut naive = RescanThrottle::new();
        naive.record_completion(Path::new("/waited"), t0, waited.duration);
        assert!(
            !naive.is_eligible(Path::new("/waited"), t0 + RESCAN_THROTTLE_WINDOW),
            "20 s charged in full would throttle the anchor for 10 minutes"
        );
    }

    /// Picks the SHALLOWEST queued path and drops every queued strict descendant of
    /// it — the ancestor's reconcile re-lists the whole subtree, so a deeper queued
    /// path is redundant. Bounds an escalation/removal storm to one subtree walk.
    #[test]
    fn pick_and_collapse_takes_shallowest_and_drops_descendants() {
        let mut pending: HashSet<PathBuf> = [
            PathBuf::from("/a/b"),
            PathBuf::from("/a/b/c"),
            PathBuf::from("/a/b/c/d"),
        ]
        .into_iter()
        .collect();
        let (picked, dropped) =
            pick_and_collapse_rescan(&mut pending, &RescanThrottle::new(), Instant::now()).expect("a path is picked");
        assert_eq!(picked, PathBuf::from("/a/b"));
        assert!(
            pending.is_empty(),
            "all queued descendants collapse into the ancestor's walk"
        );
        // Both descendants are reported dropped so their held hourglasses release.
        let mut dropped_sorted = dropped;
        dropped_sorted.sort();
        assert_eq!(dropped_sorted, vec![PathBuf::from("/a/b/c"), PathBuf::from("/a/b/c/d")]);
    }

    /// Unrelated queued subtrees both survive (only strict descendants collapse).
    #[test]
    fn pick_and_collapse_keeps_unrelated_siblings() {
        let mut pending: HashSet<PathBuf> = [PathBuf::from("/a/b/c"), PathBuf::from("/x/y")].into_iter().collect();
        let (picked, dropped) =
            pick_and_collapse_rescan(&mut pending, &RescanThrottle::new(), Instant::now()).expect("a path is picked");
        assert_eq!(picked, PathBuf::from("/x/y"), "shallowest picked first");
        assert!(dropped.is_empty(), "an unrelated sibling is not a collapsed descendant");
        assert_eq!(
            pending.iter().cloned().collect::<Vec<_>>(),
            vec![PathBuf::from("/a/b/c")],
            "the unrelated deeper subtree stays queued"
        );
    }

    /// A throttled anchor (reconciled within the window) is skipped at pick time,
    /// so a still-eligible sibling is chosen even though the throttled one is
    /// shallower. This is the per-subtree throttle gating the drain: a hard-churning
    /// subtree can't monopolize the single-flight drain by re-queueing.
    #[test]
    fn pick_skips_throttled_anchor_for_eligible_sibling() {
        let window = Duration::from_millis(100);
        let mut throttle = RescanThrottle::with_bounds(window, window);
        let t0 = Instant::now();
        throttle.record_completion(&PathBuf::from("/a"), t0, Duration::ZERO); // /a just walked -> throttled
        let mut pending: HashSet<PathBuf> = [PathBuf::from("/a"), PathBuf::from("/x/y")].into_iter().collect();
        let (picked, _dropped) =
            pick_and_collapse_rescan(&mut pending, &throttle, t0).expect("an eligible anchor is picked");
        assert_eq!(
            picked,
            PathBuf::from("/x/y"),
            "shallower /a is throttled, so eligible /x/y wins"
        );
        assert_eq!(
            pending.iter().cloned().collect::<Vec<_>>(),
            vec![PathBuf::from("/a")],
            "the throttled anchor stays queued for a later sweep, not dropped"
        );
    }

    /// A brand-new subtree is left QUEUED while it settles, not dropped, and it is
    /// picked the moment it has settled. This is what keeps an updater's ephemeral
    /// bundle out of the index while still honoring the signal for a directory a
    /// person actually created.
    #[test]
    fn a_settling_anchor_is_left_queued_then_picked_once_it_settles() {
        let mut throttle = RescanThrottle::new();
        let t0 = Instant::now();
        let anchor = PathBuf::from("/aaa/Caches/update.a1b2c3/App.app/Contents");
        throttle.note_settle_deadline(&anchor, t0 + NEW_SUBTREE_SETTLE_DELAY);
        let mut pending: HashSet<PathBuf> = [anchor.clone()].into_iter().collect();

        assert!(
            pick_and_collapse_rescan(&mut pending, &throttle, t0).is_none(),
            "a subtree created a moment ago is not walked yet"
        );
        assert_eq!(pending.len(), 1, "and it stays queued: nothing is dropped or forgotten");

        let (picked, _dropped) = pick_and_collapse_rescan(&mut pending, &throttle, t0 + NEW_SUBTREE_SETTLE_DELAY)
            .expect("eligible once it has settled");
        assert_eq!(picked, anchor, "the settled anchor walks");
    }

    /// When every queued anchor is inside its throttle window nothing is picked (the
    /// drain goes idle; the sweep tick retries). Once the window elapses the anchor
    /// is eligible again: the trailing edge that stops a busy subtree from starving.
    #[test]
    fn pick_none_when_all_throttled_then_eligible_after_window() {
        let window = Duration::from_millis(100);
        let mut throttle = RescanThrottle::with_bounds(window, window);
        let t0 = Instant::now();
        throttle.record_completion(&PathBuf::from("/a"), t0, Duration::ZERO);
        let mut pending: HashSet<PathBuf> = [PathBuf::from("/a")].into_iter().collect();
        assert!(
            pick_and_collapse_rescan(&mut pending, &throttle, t0).is_none(),
            "the only anchor is throttled, so nothing is picked"
        );
        assert_eq!(pending.len(), 1, "the throttled anchor is left queued, not dropped");
        let (picked, _dropped) =
            pick_and_collapse_rescan(&mut pending, &throttle, t0 + window).expect("eligible once the window elapses");
        assert_eq!(picked, PathBuf::from("/a"));
    }
}

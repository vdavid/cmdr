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

use super::rescan_route::{self, RescanRoute};
use super::rescan_throttle::RescanThrottle;
use super::*;
use crate::indexing::lifecycle::manager;
use crate::indexing::paths::path_prefix;
use crate::indexing::read::pending_sizes;

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
        self.enqueue_rescan(path, writer);
    }

    /// Re-queue a rescan anchor without the `DEBUG_STATS` bookkeeping. Used by the
    /// removal-storm drop rule, which fires once per dropped event (thousands in a
    /// storm) — the debug ring buffer and counter would just churn. Set-dedup makes
    /// re-inserting the already-queued (or active) anchor idempotent; if it's the
    /// ACTIVE rescan's path (popped out of `pending_rescans`), re-inserting
    /// schedules the follow-up pass the tail events need.
    pub(in crate::indexing) fn requeue_rescan(&mut self, path: PathBuf, writer: &IndexWriter) {
        self.enqueue_rescan(path, writer);
    }

    /// Insert an anchor into `pending_rescans` and start a rescan if none runs.
    fn enqueue_rescan(&mut self, path: PathBuf, writer: &IndexWriter) {
        self.pending_rescans.lock_ignore_poison().insert(path.clone());
        // Hold the rescan-root hourglass on THIS volume's tracker for the whole
        // detached reconcile (it survives the writer-drain clear). Set-insert, so
        // a re-queue of the already-held active path is a no-op. Released at every
        // rescan exit (completion, failure, conn-open failure, ancestor-collapse).
        hold_rescan(&self.volume_id, &path);

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
    /// re-walks every window and never starves. Also garbage-collects throttle
    /// records for anchors no longer pending, so the map stays bounded by the
    /// count of actively-churning subtrees.
    pub(in crate::indexing) fn sweep_rescan_throttle(&mut self, writer: &IndexWriter) {
        {
            let pending = self.pending_rescans.lock_ignore_poison();
            self.rescan_throttle.lock_ignore_poison().gc(&pending, Instant::now());
        }
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

    /// Test-only: replace the per-subtree rescan throttle with one using `window`.
    /// A zero window disables throttling (every anchor is always eligible), which
    /// the storm/stress fixed-point tests use so a re-queued anchor drains
    /// immediately instead of lingering in `pending_rescans` for the production
    /// 60 s window. Cadence itself is covered by `rescan_throttle`'s unit tests.
    #[cfg(test)]
    pub(in crate::indexing) fn set_rescan_throttle_window_for_test(&self, window: Duration) {
        *self.rescan_throttle.lock_ignore_poison() = RescanThrottle::with_window(window);
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

/// Hold a rescan root's hourglass on `volume_id`'s tracker. No-op if the volume
/// has no tracker (indexing stopped).
fn hold_rescan(volume_id: &str, root: &Path) {
    if let Some(tracker) = pending_sizes::get_pending_sizes_for(volume_id) {
        tracker.hold(&root.to_string_lossy());
    }
}

/// Release a rescan root's held hourglass, UNLESS the same root is back in
/// `pending_rescans` (a storm re-queue of the active path, or an escalation
/// targeting it): the follow-up rescan needs the hold to persist, else it runs
/// unheld. Membership is checked under the lock to close the re-queue race.
fn release_rescan_hold(volume_id: &str, root: &Path, pending_rescans: &Mutex<HashSet<PathBuf>>) {
    if pending_rescans.lock_ignore_poison().contains(root) {
        return;
    }
    if let Some(tracker) = pending_sizes::get_pending_sizes_for(volume_id) {
        tracker.release(&root.to_string_lossy());
    }
}

/// Release the held hourglasses of descendants dropped by ancestor-collapse.
/// Unconditional: a dropped descendant is out of `pending_rescans` and its
/// pendingness now rides the picked ancestor's still-held hold.
fn release_dropped_holds(volume_id: &str, dropped: &[PathBuf]) {
    if dropped.is_empty() {
        return;
    }
    if let Some(tracker) = pending_sizes::get_pending_sizes_for(volume_id) {
        for d in dropped {
            tracker.release(&d.to_string_lossy());
        }
    }
}

/// Release the rescan root's hourglass (skip if re-queued), then emit
/// `index-dir-updated` for the root plus its ancestor chain via the writer
/// channel so the refresh sequences AFTER the rescan's writes land. Release
/// precedes the emit so the triggered refetch reads `pending == false`.
fn release_and_emit_completion(
    volume_id: &str,
    root: &Path,
    pending_rescans: &Mutex<HashSet<PathBuf>>,
    writer: &IndexWriter,
) {
    release_rescan_hold(volume_id, root, pending_rescans);
    let root_str = root.to_string_lossy().to_string();
    let mut paths = vec![root_str.clone()];
    paths.extend(collect_ancestor_paths(&root_str));
    let _ = writer.send(WriteMessage::EmitDirUpdated(paths));
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
        // Lock order is always pending → throttle where both are held; the task
        // records completions under the throttle lock alone, so there's no inverse.
        match pick_and_collapse_rescan(&mut pending, &throttle, Instant::now()) {
            Some((picked, dropped)) => {
                // The collapsed descendants are now covered by `picked`'s hold;
                // release their own so the held set doesn't leak them forever.
                release_dropped_holds(&volume_id, &dropped);
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

    log::info!("MustScanSubDirs: reconcile starting for {}", path.display());

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
                    // rescan (skip if it's been re-queued meanwhile).
                    release_rescan_hold(&volume_id_for_task, &path, &pending_for_task);
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

            let escalation = match reconcile_subtree(&path, &space_for_task, &conn, &writer, &cancelled) {
                Ok(summary) => {
                    let (level, message) = reconcile_report(&path, &summary);
                    log::log!(level, "{message}");
                    summary.escalation
                }
                Err(e) => {
                    log::warn!("MustScanSubDirs: reconcile failed for {}: {e}", path.display());
                    None
                }
            };

            // The subtree's chain was still (partly) missing: re-queue the anchor the
            // skip branch resolved (strictly closer to the volume root, so this
            // converges by depth). Hold its hourglass before inserting so the follow-up
            // rescan is covered, and so the completion release below can't strand it.
            // The anchor is a proper ancestor of `path` (never equal), so it doesn't
            // affect `path`'s own release decision. The drain below picks it up.
            if let Some(anchor) = escalation {
                hold_rescan(&volume_id_for_task, &anchor);
                pending_for_task.lock_ignore_poison().insert(anchor);
            }

            // Record this subtree's reconcile so the per-subtree throttle holds the
            // anchor back until the window elapses. A hard-churning subtree that
            // re-queues immediately stays pending but won't re-walk until then; the
            // sweep tick's re-kick fires it at the window boundary (the trailing edge).
            throttle_for_task
                .lock_ignore_poison()
                .record_completion(&path, Instant::now());

            // Release this root's hourglass (unless a storm re-queued it) and emit the
            // in-place refresh for the root + its ancestor chain. Release precedes the
            // emit so the triggered refetch reads `pending == false`; the emit rides
            // the writer so it lands after the reconcile's writes.
            release_and_emit_completion(&volume_id_for_task, &path, &pending_for_task, &writer);

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
        release_rescan_hold(&volume_id, &path_for_spawn_failure, &pending_rescans);
    }
}

/// How long a reconcile has to run before it's worth a line above `info`.
const RECONCILE_SLOW_SECS: u64 = 10;

/// The line one finished subtree reconcile emits: `(level, message)`. Pure, so the
/// wording and the level policy are unit-testable without a logger.
///
/// A long reconcile is only newsworthy if the walk itself was slow. Time parked on
/// the writer queue lands inside the same duration with nothing to attribute it to,
/// which is how "reconcile slow … (+7 -0 ~0, 21s)" came to mean "the writer was
/// saturated for 19 of those seconds". So the wait is named in the line, and when
/// it DOMINATES the line drops to `debug`: writer saturation already has its own
/// signal (the writer heartbeat), and repeating it under the reconciler's name is
/// worse than not repeating it at all.
fn reconcile_report(path: &Path, summary: &ReconcileSummary) -> (log::Level, String) {
    let changes = format!("+{} -{} ~{}", summary.added, summary.removed, summary.updated);
    if summary.duration.as_secs() <= RECONCILE_SLOW_SECS {
        return (
            log::Level::Info,
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

    /// The ordinary case is unchanged: an info line with millisecond precision.
    #[test]
    fn a_quick_reconcile_reports_at_info() {
        let (level, message) = reconcile_report(
            Path::new("/tmp/quick"),
            &summary(Duration::from_millis(120), Duration::ZERO),
        );
        assert_eq!(level, log::Level::Info);
        assert_eq!(
            message,
            "MustScanSubDirs: reconcile complete for /tmp/quick (+7 -0 ~0, 120ms)"
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
        let mut throttle = RescanThrottle::with_window(window);
        let t0 = Instant::now();
        throttle.record_completion(&PathBuf::from("/a"), t0); // /a just walked -> throttled
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

    /// When every queued anchor is inside its throttle window nothing is picked (the
    /// drain goes idle; the sweep tick retries). Once the window elapses the anchor
    /// is eligible again: the trailing edge that stops a busy subtree from starving.
    #[test]
    fn pick_none_when_all_throttled_then_eligible_after_window() {
        let window = Duration::from_millis(100);
        let mut throttle = RescanThrottle::with_window(window);
        let t0 = Instant::now();
        throttle.record_completion(&PathBuf::from("/a"), t0);
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

    use crate::indexing::lifecycle::state::IndexVolumeKind;
    use crate::indexing::stress_test_helpers::TestInstanceGuard;

    /// Spawn a real NON-root writer over a throwaway DB and register a PRIVATE
    /// per-volume instance for `volume_id`, so the completion's hold-release
    /// routes to a private tracker (`get_pending_sizes_for(volume_id)`) immune to
    /// foreign root writers clearing the process-global root `PENDING_SIZES`
    /// mid-assertion (the isolation flake; its panic used to poison
    /// `PENDING_SIZES_TEST_MUTEX` and cascade). The completion emit rides this
    /// writer's channel, and `None` app handle makes the emit an observable-only
    /// no-op captured by the writer's test probe.
    fn spawn_probe_writer_for(volume_id: &str) -> (IndexWriter, tempfile::TempDir, TestInstanceGuard) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("rescan-emit.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn_for(&db_path, None, false, volume_id.to_string()).expect("spawn writer");
        let instance = TestInstanceGuard::register(volume_id, &db_path, IndexVolumeKind::Smb);
        (writer, dir, instance)
    }

    /// On completion, `release_and_emit_completion` drops the root's hold, then
    /// emits the root + its full ancestor chain via the writer so the FE refetch
    /// (which reads `is_pending`) lands after the release and after the writes.
    #[test]
    fn completion_releases_then_emits_root_and_ancestors() {
        let volume_id = "smb://rescan-test-release-emit";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        instance.tracker.hold("/aaa/bbb/ccc");
        assert!(instance.tracker.is_pending("/aaa/bbb/ccc"), "held before completion");

        let pending: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());
        release_and_emit_completion(volume_id, Path::new("/aaa/bbb/ccc"), &pending, &writer);

        // Release happened (the FE refetch will read pending == false).
        assert!(
            !instance.tracker.is_pending("/aaa/bbb/ccc"),
            "hold released on completion"
        );
        assert!(
            !instance.tracker.is_pending("/aaa"),
            "ancestor no longer pending via this root"
        );

        // The emit rode the writer with the root + its ancestor chain.
        writer.flush_blocking().expect("flush");
        assert_eq!(
            writer.emitted_paths(),
            vec![vec![
                "/aaa/bbb/ccc".to_string(),
                "/aaa/bbb".to_string(),
                "/aaa".to_string(),
                "/".to_string(),
            ]],
            "one EmitDirUpdated carrying root + ancestor chain"
        );
    }

    /// A storm re-queue of the active path leaves it in `pending_rescans` when the
    /// rescan completes: the release SKIPS (the follow-up rescan needs the hold),
    /// but the completion still emits so the in-place refresh fires.
    #[test]
    fn completion_skips_release_when_requeued() {
        let volume_id = "smb://rescan-test-skip-release";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        instance.tracker.hold("/aaa/bbb/ccc");

        let mut set = HashSet::new();
        set.insert(PathBuf::from("/aaa/bbb/ccc"));
        let pending = Mutex::new(set);
        release_and_emit_completion(volume_id, Path::new("/aaa/bbb/ccc"), &pending, &writer);

        assert!(
            instance.tracker.is_pending("/aaa/bbb/ccc"),
            "hold persists while the root is re-queued for a follow-up rescan"
        );
        writer.flush_blocking().expect("flush");
        assert_eq!(
            writer.emitted_paths().len(),
            1,
            "the completion still emits the refresh"
        );
    }
}

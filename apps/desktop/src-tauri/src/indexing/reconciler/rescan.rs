//! MustScanSubDirs rescan orchestration for the reconciler.
//!
//! One rescan runs at a time (`rescan_active`); anchors queue in `pending_rescans`
//! and drain automatically on completion. Two behaviors this milestone leans on:
//! ancestor-collapse at pick time (a queued descendant is redundant once its
//! ancestor's reconcile re-lists the whole subtree) and the shared
//! `active_rescan_path` slot the removal-storm drop rule reads to see the
//! in-flight rescan (the path is popped out of `pending_rescans` at spawn).

use super::*;
use crate::indexing::path_prefix;

impl EventReconciler {
    /// Queue a MustScanSubDirs rescan, throttled to max 1 concurrent.
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
            self.space.clone(),
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
            self.space.clone(),
            writer,
        );
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
}

/// Pick the next rescan anchor from the pending set: the SHALLOWEST queued path
/// (fewest components), then drop it AND every queued STRICT descendant of it.
/// An ancestor's reconcile re-lists the whole subtree, so a queued descendant is
/// redundant — collapsing bounds an escalation or removal storm to ONE subtree
/// walk instead of one per level. Returns `None` when the set is empty.
pub(super) fn pick_and_collapse_rescan(pending: &mut HashSet<PathBuf>) -> Option<PathBuf> {
    let picked = pending
        .iter()
        .min_by_key(|p| path_prefix::depth(&p.to_string_lossy()))
        .cloned()?;
    let picked_str = picked.to_string_lossy().to_string();
    pending.retain(|q| *q != picked && !path_prefix::is_strict_descendant(&q.to_string_lossy(), &picked_str));
    Some(picked)
}

/// Start the next pending MustScanSubDirs rescan, if any.
///
/// Standalone function (not a method) so the spawned task can call it after
/// completion to drain the pending queue automatically.
pub(super) fn start_next_rescan(
    pending_rescans: Arc<Mutex<HashSet<PathBuf>>>,
    rescan_active: Arc<AtomicBool>,
    active_rescan_path: Arc<Mutex<Option<PathBuf>>>,
    space: IndexPathSpace,
    writer: &IndexWriter,
) {
    let path = {
        let mut pending = pending_rescans.lock_ignore_poison();
        match pick_and_collapse_rescan(&mut pending) {
            Some(p) => p,
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
    let space_for_task = space.clone();

    log::info!("MustScanSubDirs: reconcile starting for {}", path.display());

    tokio::task::spawn_blocking(move || {
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
                active_for_task.store(false, Ordering::Relaxed);
                *active_path_for_task.lock_ignore_poison() = None;
                // Try the next pending rescan even if this one failed
                start_next_rescan(
                    pending_for_task,
                    active_for_task,
                    active_path_for_task,
                    space_for_task,
                    &writer,
                );
                return;
            }
        };

        let escalation = match reconcile_subtree(&path, &space_for_task, &conn, &writer, &cancelled) {
            Ok(summary) => {
                if summary.duration.as_secs() > 10 {
                    log::warn!(
                        "MustScanSubDirs: reconcile slow for {} (+{} -{} ~{}, {}s)",
                        path.display(),
                        summary.added,
                        summary.removed,
                        summary.updated,
                        summary.duration.as_secs(),
                    );
                } else {
                    log::info!(
                        "MustScanSubDirs: reconcile complete for {} (+{} -{} ~{}, {}ms)",
                        path.display(),
                        summary.added,
                        summary.removed,
                        summary.updated,
                        summary.duration.as_millis(),
                    );
                }
                summary.escalation
            }
            Err(e) => {
                log::warn!("MustScanSubDirs: reconcile failed for {}: {e}", path.display());
                None
            }
        };

        // The subtree's chain was still (partly) missing: re-queue the anchor the
        // skip branch resolved (strictly closer to the volume root, so this
        // converges by depth). The drain below picks it up.
        if let Some(anchor) = escalation {
            pending_for_task.lock_ignore_poison().insert(anchor);
        }

        DEBUG_STATS.record_rescan_completed();
        active_for_task.store(false, Ordering::Relaxed);
        *active_path_for_task.lock_ignore_poison() = None;

        // Automatically start the next queued rescan
        start_next_rescan(
            pending_for_task,
            active_for_task,
            active_path_for_task,
            space_for_task,
            &writer,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let picked = pick_and_collapse_rescan(&mut pending);
        assert_eq!(picked, Some(PathBuf::from("/a/b")));
        assert!(
            pending.is_empty(),
            "all queued descendants collapse into the ancestor's walk"
        );
    }

    /// Unrelated queued subtrees both survive (only strict descendants collapse).
    #[test]
    fn pick_and_collapse_keeps_unrelated_siblings() {
        let mut pending: HashSet<PathBuf> = [PathBuf::from("/a/b/c"), PathBuf::from("/x/y")].into_iter().collect();
        let picked = pick_and_collapse_rescan(&mut pending).expect("a path is picked");
        assert_eq!(picked, PathBuf::from("/x/y"), "shallowest picked first");
        assert_eq!(
            pending.iter().cloned().collect::<Vec<_>>(),
            vec![PathBuf::from("/a/b/c")],
            "the unrelated deeper subtree stays queued"
        );
    }
}

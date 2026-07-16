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
            self.space.clone(),
            self.volume_id.clone(),
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
/// walk instead of one per level. Returns the picked anchor plus the dropped
/// descendants (so the caller can release their held-hourglass roots — the
/// picked ancestor's hold now covers them), or `None` when the set is empty.
pub(super) fn pick_and_collapse_rescan(pending: &mut HashSet<PathBuf>) -> Option<(PathBuf, Vec<PathBuf>)> {
    let picked = pending
        .iter()
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
    if let Some(tracker) = crate::indexing::pending_sizes::get_pending_sizes_for(volume_id) {
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
    if let Some(tracker) = crate::indexing::pending_sizes::get_pending_sizes_for(volume_id) {
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
    if let Some(tracker) = crate::indexing::pending_sizes::get_pending_sizes_for(volume_id) {
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
    space: IndexPathSpace,
    volume_id: String,
    writer: &IndexWriter,
) {
    let path = {
        let mut pending = pending_rescans.lock_ignore_poison();
        match pick_and_collapse_rescan(&mut pending) {
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
    let space_for_task = space.clone();
    let volume_id_for_task = volume_id.clone();

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
                    space_for_task,
                    volume_id_for_task,
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
        // converges by depth). Hold its hourglass before inserting so the follow-up
        // rescan is covered, and so the completion release below can't strand it.
        // The anchor is a proper ancestor of `path` (never equal), so it doesn't
        // affect `path`'s own release decision. The drain below picks it up.
        if let Some(anchor) = escalation {
            hold_rescan(&volume_id_for_task, &anchor);
            pending_for_task.lock_ignore_poison().insert(anchor);
        }

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
            space_for_task,
            volume_id_for_task,
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
        let (picked, dropped) = pick_and_collapse_rescan(&mut pending).expect("a path is picked");
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
        let (picked, dropped) = pick_and_collapse_rescan(&mut pending).expect("a path is picked");
        assert_eq!(picked, PathBuf::from("/x/y"), "shallowest picked first");
        assert!(dropped.is_empty(), "an unrelated sibling is not a collapsed descendant");
        assert_eq!(
            pending.iter().cloned().collect::<Vec<_>>(),
            vec![PathBuf::from("/a/b/c")],
            "the unrelated deeper subtree stays queued"
        );
    }

    use crate::indexing::pending_sizes::{PENDING_SIZES, PENDING_SIZES_TEST_MUTEX, PendingSizes};

    /// Spawn a real writer over a throwaway DB. The completion emit rides this
    /// writer's channel, and `None` app handle makes the emit an observable-only
    /// no-op captured by the writer's test probe.
    fn spawn_probe_writer() -> (IndexWriter, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("rescan-emit.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        (writer, dir)
    }

    /// On completion, `release_and_emit_completion` drops the root's hold, then
    /// emits the root + its full ancestor chain via the writer so the FE refetch
    /// (which reads `is_pending`) lands after the release and after the writes.
    #[test]
    fn completion_releases_then_emits_root_and_ancestors() {
        let _guard = PENDING_SIZES_TEST_MUTEX.lock().expect("test mutex");
        *PENDING_SIZES.lock().expect("install tracker") = Some(Arc::new(PendingSizes::new()));
        let tracker = crate::indexing::pending_sizes::get_pending_sizes_for(ROOT_VOLUME_ID).expect("tracker");
        tracker.hold("/aaa/bbb/ccc");
        assert!(tracker.is_pending("/aaa/bbb/ccc"), "held before completion");

        let (writer, _dir) = spawn_probe_writer();
        let pending: Mutex<HashSet<PathBuf>> = Mutex::new(HashSet::new());
        release_and_emit_completion(ROOT_VOLUME_ID, Path::new("/aaa/bbb/ccc"), &pending, &writer);

        // Release happened (the FE refetch will read pending == false).
        assert!(!tracker.is_pending("/aaa/bbb/ccc"), "hold released on completion");
        assert!(!tracker.is_pending("/aaa"), "ancestor no longer pending via this root");

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
        *PENDING_SIZES.lock().expect("uninstall") = None;
    }

    /// A storm re-queue of the active path leaves it in `pending_rescans` when the
    /// rescan completes: the release SKIPS (the follow-up rescan needs the hold),
    /// but the completion still emits so the in-place refresh fires.
    #[test]
    fn completion_skips_release_when_requeued() {
        let _guard = PENDING_SIZES_TEST_MUTEX.lock().expect("test mutex");
        *PENDING_SIZES.lock().expect("install tracker") = Some(Arc::new(PendingSizes::new()));
        let tracker = crate::indexing::pending_sizes::get_pending_sizes_for(ROOT_VOLUME_ID).expect("tracker");
        tracker.hold("/aaa/bbb/ccc");

        let (writer, _dir) = spawn_probe_writer();
        let mut set = HashSet::new();
        set.insert(PathBuf::from("/aaa/bbb/ccc"));
        let pending = Mutex::new(set);
        release_and_emit_completion(ROOT_VOLUME_ID, Path::new("/aaa/bbb/ccc"), &pending, &writer);

        assert!(
            tracker.is_pending("/aaa/bbb/ccc"),
            "hold persists while the root is re-queued for a follow-up rescan"
        );
        writer.flush_blocking().expect("flush");
        assert_eq!(
            writer.emitted_paths().len(),
            1,
            "the completion still emits the refresh"
        );
        *PENDING_SIZES.lock().expect("uninstall") = None;
    }
}

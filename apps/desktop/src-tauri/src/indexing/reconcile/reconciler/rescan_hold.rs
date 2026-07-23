//! The "size updating" hourglass hold for `MustScanSubDirs` rescan anchors.
//!
//! A held root marks its whole chain pending in BOTH directions
//! (`read/pending_sizes.rs`), so an anchor at `~/Library/Caches/…/Resource`
//! holding drags `~/Library`, `~`, and `/` into the hourglass with it. That reach
//! is correct while the subtree is being rewritten and wrong the rest of the time,
//! which is what this module decides.
//!
//! ## The invariant
//!
//! **An anchor holds iff it is walking right now, or it is queued AND eligible to
//! walk now.** The hold means "unprocessed index writes in flight or imminent" —
//! nothing weaker.
//!
//! An anchor resting out its [`super::rescan_throttle`] window is neither: its last
//! walk completed, its aggregate is consistent, and the next walk is up to
//! `RESCAN_THROTTLE_MAX_WINDOW` away. It stays quiet. An anchor that is eligible and
//! merely waiting behind the single-flight active walk DOES hold: its walk is
//! imminent, so the hourglass is honest.
//!
//! Four sites keep that true, and their overlap is deliberate:
//!
//! - **Enqueue** ([`hold_if_eligible`]): an eligible anchor holds the moment it's
//!   queued, so the honest signal doesn't wait for the next sweep tick.
//! - **Pick** ([`adopt_picked_holds`]): the anchor about to walk holds
//!   unconditionally, under the `pending_rescans` lock. That's what makes
//!   "walking ⇒ held" structural rather than inferred, and it's why every release
//!   path below may release freely: a follow-up walk takes its own hold.
//! - **Sweep tick** (~1 s, [`reconcile_with_eligibility`]): re-derives each queued
//!   anchor's hold from its current eligibility, turning a throttled anchor quiet
//!   and re-arming it when its window elapses. Skips the active walk.
//! - **Every rescan exit** ([`release_rescan_hold`], [`release_and_emit_completion`]):
//!   releases unless the anchor is queued and eligible again.
//!
//! **There is no window where a walk is writing while its anchor is unheld.** The
//! active walk is popped out of `pending_rescans`, so the sweep never iterates it;
//! a storm that re-queues the active path can only put it back while its throttle
//! record still predates the walk, which reads eligible and therefore holds. Once
//! the walk records its completion the anchor is ineligible, but `active_rescan_path`
//! names it until the task itself releases, so the sweep keeps skipping it.

use super::rescan_throttle::RescanThrottle;
use super::*;
use crate::indexing::read::pending_sizes;

/// Hold a rescan root's hourglass on `volume_id`'s tracker. No-op if the volume
/// has no tracker (indexing stopped).
pub(super) fn hold_rescan(volume_id: &str, root: &Path) {
    if let Some(tracker) = pending_sizes::get_pending_sizes_for(volume_id) {
        tracker.hold(&root.to_string_lossy());
    }
}

/// Hold `root` only if it may walk NOW. A throttled anchor is queued but resting,
/// with no writes in flight and none imminent, so it stays quiet until the sweep
/// tick sees its window elapse.
pub(super) fn hold_if_eligible(volume_id: &str, root: &Path, throttle: &Mutex<RescanThrottle>, now: Instant) {
    if throttle.lock_ignore_poison().is_eligible(root, now) {
        hold_rescan(volume_id, root);
    }
}

/// Hourglass bookkeeping for a freshly-picked anchor: hold the picked root (it is
/// about to walk, whatever its enqueue-time eligibility was) and release the
/// descendants ancestor-collapse dropped. Releasing a dropped descendant is exact,
/// not a guess: it's out of `pending_rescans` and its pendingness now rides the
/// picked ancestor's hold.
pub(super) fn adopt_picked_holds(volume_id: &str, picked: &Path, dropped: &[PathBuf]) {
    let Some(tracker) = pending_sizes::get_pending_sizes_for(volume_id) else {
        return;
    };
    tracker.hold(&picked.to_string_lossy());
    for d in dropped {
        tracker.release(&d.to_string_lossy());
    }
}

/// Re-derive every QUEUED anchor's hold from its current eligibility — the sweep
/// tick's half of the invariant. An anchor whose window elapsed starts holding
/// (its walk is imminent); one still resting stops.
///
/// `active` (the in-flight walk, popped out of `pending`) is skipped: a storm can
/// re-queue it mid-walk, and the walk's own hold must survive that.
pub(super) fn reconcile_with_eligibility(
    volume_id: &str,
    pending: &HashSet<PathBuf>,
    active: Option<&PathBuf>,
    throttle: &RescanThrottle,
    now: Instant,
) {
    let Some(tracker) = pending_sizes::get_pending_sizes_for(volume_id) else {
        return;
    };
    for root in pending {
        if active == Some(root) {
            continue;
        }
        let root_str = root.to_string_lossy();
        if throttle.is_eligible(root, now) {
            tracker.hold(&root_str);
        } else {
            tracker.release(&root_str);
        }
    }
}

/// Release a previously-held rescan root, UNLESS it's back in `pending_rescans`
/// AND eligible to walk now (a storm re-queue of the active path, or an escalation
/// targeting it): that follow-up walk is imminent, so the hold stays unbroken
/// rather than flickering off and straight back on. A re-queued but THROTTLED root
/// releases: it's resting, not working. Membership is read under the lock, and the
/// pick-time hold covers the rest — a re-queue landing after this check gets its
/// own hold when it's picked.
pub(super) fn release_rescan_hold(
    volume_id: &str,
    root: &Path,
    pending_rescans: &Mutex<HashSet<PathBuf>>,
    throttle: &Mutex<RescanThrottle>,
    now: Instant,
) {
    if pending_rescans.lock_ignore_poison().contains(root) && throttle.lock_ignore_poison().is_eligible(root, now) {
        return;
    }
    if let Some(tracker) = pending_sizes::get_pending_sizes_for(volume_id) {
        tracker.release(&root.to_string_lossy());
    }
}

/// Release the rescan root's hourglass (skip if a follow-up walk is imminent),
/// then emit `index-dir-updated` for the root plus its ancestor chain via the
/// writer channel so the refresh sequences AFTER the rescan's writes land. Release
/// precedes the emit so the triggered refetch reads `pending == false`.
///
/// The caller records the completion FIRST, so a re-queued churning anchor is by
/// then inside its own fresh window and this releases it: the hourglass goes out
/// the moment the walk is done, instead of holding every ancestor up to `/` for
/// the whole back-off.
pub(super) fn release_and_emit_completion(
    volume_id: &str,
    root: &Path,
    pending_rescans: &Mutex<HashSet<PathBuf>>,
    throttle: &Mutex<RescanThrottle>,
    now: Instant,
    writer: &IndexWriter,
) {
    release_rescan_hold(volume_id, root, pending_rescans, throttle, now);
    let root_str = root.to_string_lossy().to_string();
    let mut paths = vec![root_str.clone()];
    paths.extend(collect_ancestor_paths(&root_str));
    let _ = writer.send(WriteMessage::EmitDirUpdated(paths));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::lifecycle::state::IndexVolumeKind;
    use crate::indexing::stress_test_helpers::TestInstanceGuard;

    /// Spawn a real NON-root writer over a throwaway DB and register a PRIVATE
    /// per-volume instance for `volume_id`, so the hold/release routes to a private
    /// tracker (`get_pending_sizes_for(volume_id)`) immune to foreign root writers
    /// clearing the process-global root `PENDING_SIZES` mid-assertion (the isolation
    /// flake; its panic used to poison `PENDING_SIZES_TEST_MUTEX` and cascade). The
    /// completion emit rides this writer's channel, and `None` app handle makes the
    /// emit an observable-only no-op captured by the writer's test probe.
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
        let throttle = Mutex::new(RescanThrottle::new());
        release_and_emit_completion(
            volume_id,
            Path::new("/aaa/bbb/ccc"),
            &pending,
            &throttle,
            Instant::now(),
            &writer,
        );

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

    /// A deep anchor at `~/Library/Caches/…/Resource` churns continuously, so it is
    /// re-queued while it is still resting out the back-off its last (expensive)
    /// walk earned. Nothing is in flight and nothing is imminent, so it must hold
    /// nothing — and above all not drag `~/Library`, `~`, and `/` into the "size
    /// updating" hourglass for the whole window, which is what a user sees.
    #[test]
    fn a_throttled_anchor_does_not_hold_its_ancestors_hourglass() {
        let volume_id = "smb://rescan-test-throttled-quiet";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let mut reconciler = EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root());
        // Keep the anchor queued (no spawn), so we observe the pending state itself.
        reconciler.set_rescan_active_for_test(true);
        let anchor = PathBuf::from("/aaa/Library/Caches/cmdr/WebKit/NetworkCache/Resource");
        // Its last walk cost 10 s, so it's backing off for five minutes.
        reconciler.record_rescan_completion_for_test(&anchor, Duration::from_secs(10));

        reconciler.queue_must_scan_sub_dirs(anchor.clone(), &writer);

        assert_eq!(
            reconciler.pending_rescans_snapshot(),
            vec![anchor.clone()],
            "the anchor is still queued: it walks again when its window elapses"
        );
        assert!(
            !instance.tracker.is_pending(&anchor.to_string_lossy()),
            "a resting anchor has no writes in flight, so it holds no hourglass"
        );
        assert!(
            !instance.tracker.is_pending("/aaa/Library"),
            "and no ancestor reads as 'size updating' either"
        );
        writer.shutdown();
    }

    /// A brand-new subtree (an updater unpacking a bundle) is queued but SETTLING:
    /// nothing is in flight and nothing is imminent, so it must hold nothing — and
    /// above all must not drag its ancestors up to `/` into the "size updating"
    /// hourglass, which is the half a user sees.
    #[test]
    fn a_settling_anchor_does_not_hold_its_ancestors_hourglass() {
        let volume_id = "smb://rescan-test-settling-quiet";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let mut reconciler = EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root());
        // Keep the anchor queued (no spawn), so we observe the pending state itself.
        reconciler.set_rescan_active_for_test(true);
        // A REAL directory created a moment ago: the enqueue path stats it.
        let staging = tempfile::tempdir().expect("temp dir");
        let anchor = staging.path().join("update.a1b2c3/App.app/Contents/Resources");
        std::fs::create_dir_all(&anchor).expect("create the fresh bundle");

        reconciler.queue_must_scan_sub_dirs(anchor.clone(), &writer);

        assert_eq!(
            reconciler.pending_rescans_snapshot(),
            vec![anchor.clone()],
            "the anchor is still queued: it walks once it has settled"
        );
        assert!(
            !instance.tracker.is_pending(&anchor.to_string_lossy()),
            "a settling anchor has no writes in flight, so it holds no hourglass"
        );
        assert!(
            !instance.tracker.is_pending(&staging.path().to_string_lossy()),
            "and no ancestor reads as 'size updating' either"
        );
        writer.shutdown();
    }

    /// The guard: an ESTABLISHED directory behaves exactly as before the settle
    /// delay existed — its walk is imminent, so it holds from the moment it's
    /// queued. (A zero delay is the same question as an old birthtime.)
    #[test]
    fn an_established_anchor_holds_from_the_moment_it_is_queued() {
        let volume_id = "smb://rescan-test-established-holds";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let mut reconciler = EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root());
        reconciler.set_rescan_active_for_test(true);
        reconciler.set_settle_delay_for_test(Duration::ZERO);
        let established = tempfile::tempdir().expect("temp dir");
        let anchor = established.path().join("projects/site/build");
        std::fs::create_dir_all(&anchor).expect("create");

        reconciler.queue_must_scan_sub_dirs(anchor.clone(), &writer);

        assert!(
            instance.tracker.is_pending(&anchor.to_string_lossy()),
            "an established anchor's walk is imminent, so it holds"
        );
        assert!(
            instance.tracker.is_pending(&established.path().to_string_lossy()),
            "and its ancestors read as 'size updating', which is honest here"
        );
        writer.shutdown();
    }

    /// Fail open: an anchor whose directory has no readable birthtime (it already
    /// vanished, or the filesystem records none) is never stalled. It holds and
    /// walks exactly as it does today.
    #[test]
    fn an_anchor_with_no_readable_birthtime_is_not_delayed() {
        let volume_id = "smb://rescan-test-no-birthtime";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let mut reconciler = EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root());
        reconciler.set_rescan_active_for_test(true);
        let staging = tempfile::tempdir().expect("temp dir");
        // Never created on disk: there is no birthtime to read.
        let anchor = staging.path().join("already-gone/App.app");

        reconciler.queue_must_scan_sub_dirs(anchor.clone(), &writer);

        assert!(
            instance.tracker.is_pending(&anchor.to_string_lossy()),
            "no birthtime must not stall the anchor: it holds and walks as before"
        );
        writer.shutdown();
    }

    /// The sweep tick carries settling the same way it carries the throttle: the
    /// anchor is quiet while it settles, and re-arms once it has (one tick before
    /// the re-kick walks it).
    #[test]
    fn the_sweep_rearms_the_hold_when_the_subtree_settles() {
        let volume_id = "smb://rescan-test-sweep-settle";
        let (_writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let root = PathBuf::from("/aaa/Caches/update.a1b2c3");
        instance.tracker.hold("/aaa/Caches/update.a1b2c3");
        let now = Instant::now();
        let mut throttle = RescanThrottle::new();
        throttle.note_settle_deadline(&root, now + Duration::from_secs(30));
        let pending: HashSet<PathBuf> = [root].into_iter().collect();

        reconcile_with_eligibility(volume_id, &pending, None, &throttle, now);
        assert!(
            !instance.tracker.is_pending("/aaa"),
            "a settling anchor stops holding its ancestors"
        );

        reconcile_with_eligibility(volume_id, &pending, None, &throttle, now + Duration::from_secs(30));
        assert!(
            instance.tracker.is_pending("/aaa/Caches/update.a1b2c3"),
            "once it has settled the walk is imminent, so it holds again"
        );
    }

    /// The honest signal stays: an anchor that MAY walk now and is only waiting
    /// behind the single-flight active walk still holds. Its walk is imminent.
    #[test]
    fn an_eligible_queued_anchor_still_holds() {
        let volume_id = "smb://rescan-test-eligible-holds";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let mut reconciler = EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root());
        reconciler.set_rescan_active_for_test(true);
        let anchor = PathBuf::from("/aaa/bbb/ccc/ddd/target");

        reconciler.queue_must_scan_sub_dirs(anchor.clone(), &writer);

        assert!(
            instance.tracker.is_pending(&anchor.to_string_lossy()),
            "a never-walked anchor is eligible, so its imminent walk holds the hourglass"
        );
        assert!(
            instance.tracker.is_pending("/aaa/bbb"),
            "and its ancestors read as 'size updating', which is honest here"
        );
        writer.shutdown();
    }

    /// The sweep tick re-derives holds from eligibility: an anchor that held while
    /// eligible goes quiet once it has walked and is resting.
    #[test]
    fn the_sweep_tick_drops_the_hold_of_a_now_throttled_anchor() {
        let volume_id = "smb://rescan-test-sweep-drops";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let mut reconciler = EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root());
        reconciler.set_rescan_active_for_test(true);
        let anchor = PathBuf::from("/aaa/bbb/ccc/ddd/churny");

        reconciler.queue_must_scan_sub_dirs(anchor.clone(), &writer);
        assert!(
            instance.tracker.is_pending("/aaa/bbb"),
            "held while eligible (precondition)"
        );

        // It walks, and the 10 s walk earns a five-minute back-off.
        reconciler.record_rescan_completion_for_test(&anchor, Duration::from_secs(10));
        reconciler.sweep_rescan_throttle(&writer);

        assert!(
            !instance.tracker.is_pending("/aaa/bbb"),
            "the sweep frees the ancestors of an anchor that is only resting"
        );
        assert_eq!(
            reconciler.pending_rescans_snapshot(),
            vec![anchor],
            "still queued, just quiet: it re-walks when its window elapses"
        );
        writer.shutdown();
    }

    /// No unheld-write window: the sweep must never strip the hold of the walk
    /// that's running right now, even though that anchor is throttled from its
    /// previous walk and a storm has re-queued it.
    #[test]
    fn the_sweep_tick_never_strips_the_active_walks_hold() {
        let volume_id = "smb://rescan-test-sweep-active";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let mut reconciler = EventReconciler::new_for(volume_id.to_string(), IndexPathSpace::root());
        reconciler.set_rescan_active_for_test(true);
        let anchor = PathBuf::from("/aaa/bbb/ccc/ddd/walking");
        // The walk is in flight and holding; its record is from the PREVIOUS walk.
        reconciler.set_active_rescan_path_for_test(Some(anchor.clone()));
        hold_rescan(volume_id, &anchor);
        reconciler.record_rescan_completion_for_test(&anchor, Duration::from_secs(10));
        // A storm re-queues the path that is walking.
        reconciler.insert_pending_rescan_for_test(anchor.clone());

        reconciler.sweep_rescan_throttle(&writer);

        assert!(
            instance.tracker.is_pending(&anchor.to_string_lossy()),
            "the walk is writing; its hold must survive a re-queue plus a sweep"
        );
        writer.shutdown();
    }

    /// A re-queued root that may walk again NOW (a rescan that exited without
    /// recording a completion, so no window ever started) keeps the hold: the
    /// follow-up walk is imminent, and dropping the hourglass in between would
    /// flicker it off and straight back on. The exit still emits the refresh.
    #[test]
    fn exit_keeps_the_hold_when_the_requeued_root_may_walk_now() {
        let volume_id = "smb://rescan-test-skip-release";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        instance.tracker.hold("/aaa/bbb/ccc");

        let mut set = HashSet::new();
        set.insert(PathBuf::from("/aaa/bbb/ccc"));
        let pending = Mutex::new(set);
        let throttle = Mutex::new(RescanThrottle::new());
        release_and_emit_completion(
            volume_id,
            Path::new("/aaa/bbb/ccc"),
            &pending,
            &throttle,
            Instant::now(),
            &writer,
        );

        assert!(
            instance.tracker.is_pending("/aaa/bbb/ccc"),
            "hold persists while an eligible follow-up rescan is queued"
        );
        writer.flush_blocking().expect("flush");
        assert_eq!(
            writer.emitted_paths().len(),
            1,
            "the completion still emits the refresh"
        );
    }

    /// The churn case at the completion seam: the walk finished, the storm re-queued
    /// the anchor, and the recorded completion has it resting for five minutes.
    /// Nothing is in flight, so the hourglass goes out on the anchor AND on every
    /// ancestor up to `/`, which is where a user actually sees it.
    #[test]
    fn completion_releases_a_requeued_but_throttled_root() {
        let volume_id = "smb://rescan-test-requeued-throttled";
        let (writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let root = "/aaa/Library/Caches/WebKit/Resource";
        instance.tracker.hold(root);

        let mut set = HashSet::new();
        set.insert(PathBuf::from(root));
        let pending = Mutex::new(set);
        let now = Instant::now();
        // The completion the caller records just before this call: a 10 s walk earns
        // a five-minute window, so the re-queued anchor is resting, not working.
        let mut recorded = RescanThrottle::new();
        recorded.record_completion(Path::new(root), now, Duration::from_secs(10));
        let throttle = Mutex::new(recorded);

        release_and_emit_completion(volume_id, Path::new(root), &pending, &throttle, now, &writer);

        assert!(
            !instance.tracker.is_pending(root),
            "a resting anchor holds nothing: its walk completed and its aggregate is consistent"
        );
        assert!(
            !instance.tracker.is_pending("/aaa/Library"),
            "and no ancestor reads as 'size updating' either"
        );
        writer.shutdown();
    }

    /// Pick time is where "walking ⇒ held" becomes structural: the picked anchor
    /// holds whatever its enqueue-time state was, and the descendants collapsed into
    /// its walk hand their holds over to it.
    #[test]
    fn picking_holds_the_anchor_and_frees_the_collapsed_descendants() {
        let volume_id = "smb://rescan-test-adopt-picked";
        let (_writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        instance.tracker.hold("/aaa/bbb/ccc");
        instance.tracker.hold("/aaa/bbb/ccc/ddd");

        adopt_picked_holds(
            volume_id,
            Path::new("/aaa/bbb"),
            &[PathBuf::from("/aaa/bbb/ccc"), PathBuf::from("/aaa/bbb/ccc/ddd")],
        );

        assert_eq!(instance.tracker.held_len(), 1, "only the picked ancestor is held");
        assert!(
            instance.tracker.is_pending("/aaa/bbb/ccc/ddd"),
            "a collapsed descendant is still pending, now via the picked ancestor's hold"
        );
    }

    /// The sweep's rule with the clock injected: a queued anchor inside its window
    /// holds nothing; the same anchor holds again once that window has elapsed.
    #[test]
    fn the_sweep_rearms_the_hold_when_the_window_elapses() {
        let volume_id = "smb://rescan-test-sweep-rearm";
        let (_writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let root = PathBuf::from("/aaa/churny");
        instance.tracker.hold("/aaa/churny");
        let now = Instant::now();
        let mut throttle = RescanThrottle::new();
        throttle.record_completion(&root, now, Duration::from_secs(10)); // a five-minute window
        let pending: HashSet<PathBuf> = [root].into_iter().collect();

        reconcile_with_eligibility(volume_id, &pending, None, &throttle, now);
        assert!(
            !instance.tracker.is_pending("/aaa"),
            "a resting anchor stops holding its ancestors"
        );

        reconcile_with_eligibility(volume_id, &pending, None, &throttle, now + Duration::from_secs(300));
        assert!(
            instance.tracker.is_pending("/aaa/churny"),
            "once its window elapses the walk is imminent again, so it holds again"
        );
    }

    /// Holds are per-root and `is_pending` is a prefix test in both directions, so a
    /// stray release would silently un-flag a live subtree. One anchor's sweep
    /// outcome must not touch another's.
    #[test]
    fn the_sweep_leaves_unrelated_anchors_alone() {
        let volume_id = "smb://rescan-test-sweep-unrelated";
        let (_writer, _dir, instance) = spawn_probe_writer_for(volume_id);
        let resting = PathBuf::from("/aaa/resting");
        let fresh = PathBuf::from("/zzz/fresh");
        instance.tracker.hold("/aaa/resting");
        instance.tracker.hold("/zzz/fresh");
        let now = Instant::now();
        let mut throttle = RescanThrottle::new();
        throttle.record_completion(&resting, now, Duration::from_secs(10));
        let pending: HashSet<PathBuf> = [resting, fresh].into_iter().collect();

        reconcile_with_eligibility(volume_id, &pending, None, &throttle, now);

        assert!(!instance.tracker.is_pending("/aaa/resting"), "the resting one released");
        assert!(
            instance.tracker.is_pending("/zzz/fresh"),
            "the never-walked one still holds"
        );
    }
}

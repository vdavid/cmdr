use super::settle::NEW_SUBTREE_SETTLE_DELAY;
use super::throttle::RESCAN_THROTTLE_WINDOW;
use super::*;

fn summary(duration: Duration, writer_wait: Duration) -> ReconcileSummary {
    ReconcileSummary {
        added: 7,
        added_dirs: 0,
        removed: 0,
        updated: 0,
        unreadable_dirs: 0,
        duration,
        writer_wait,
        escalation: None,
        cancelled: false,
    }
}

/// Directories that vanished mid-walk are the EXPECTED race with a compiler, not
/// a diagnosis, so they don't get a line each (that was ~750 an hour on a build
/// machine). The count still has to reach the bundle, because "half this subtree
/// was unreadable" is what explains a reconcile that found nothing.
#[test]
fn dirs_that_vanished_mid_walk_are_counted_on_the_summary_line() {
    let mut vanished = summary(Duration::from_millis(120), Duration::ZERO);
    vanished.unreadable_dirs = 143;
    let (level, message) = reconcile_report(Path::new("/tmp/target"), &vanished);
    assert_eq!(level, log::Level::Debug, "a vanishing build dir is not a problem");
    assert_eq!(
        message,
        "MustScanSubDirs: reconcile complete for /tmp/target (+7 -0 ~0, 143 unreadable dirs, 120ms)"
    );
}

/// The usual walk reads every directory it visits, and a line that said
/// "0 unreadable" every time would be noise about nothing.
#[test]
fn a_walk_that_read_everything_says_nothing_about_unreadable_dirs() {
    let (_, message) = reconcile_report(
        Path::new("/tmp/quiet"),
        &summary(Duration::from_millis(120), Duration::ZERO),
    );
    assert!(!message.contains("unreadable"), "{message}");
}

/// A slow walk carries the count too: a subtree churning hard enough to be slow
/// is exactly where "and most of it was unreadable" changes the diagnosis.
#[test]
fn a_slow_walk_carries_the_unreadable_count_too() {
    let mut vanished = summary(Duration::from_secs(21), Duration::from_millis(300));
    vanished.unreadable_dirs = 9;
    let (level, message) = reconcile_report(Path::new("/tmp/deep-tree"), &vanished);
    assert_eq!(level, log::Level::Warn);
    assert_eq!(
        message,
        "MustScanSubDirs: reconcile slow for /tmp/deep-tree (+7 -0 ~0, 9 unreadable dirs, 21s)"
    );
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
/// aggregate ([`super::churn`]), not the per-walk line. Content is
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

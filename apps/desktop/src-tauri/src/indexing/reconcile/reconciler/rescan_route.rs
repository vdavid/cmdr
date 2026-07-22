//! Depth-split routing for `MustScanSubDirs` anchors, plus the per-volume
//! once-a-day sweep window.
//!
//! macOS drops fine-grained FSEvents under heavy churn and raises
//! `MustScanSubDirs` on ever-higher paths, up to `/`. A deep/narrow anchor (a
//! single `target/`) is exactly what the throttled `reconcile_subtree` drain is
//! good at. A shallow/root-scale anchor is NOT: reconciling `/` is a ~20-min walk
//! that holds the per-dir hourglass the whole time (`reconcile/reconciler/rescan.rs`), and
//! under continuous root churn the anchor never leaves `pending_rescans`, so the
//! hold never clears. A channel overflow — the SAME "we lost events" signal —
//! already takes the VISIBLE scanner path (single-flight, updates freshness,
//! self-clearing). This routes shallow anchors there too, so the two equivalent
//! signals stop diverging.
//!
//! Everything here is pure + clock-injected so the split and the cooldown are
//! deterministically unit-tested.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ignore_poison::IgnorePoison;

/// Anchor-depth threshold: an anchor at or below this depth routes to the visible
/// scanner; a deeper one keeps the throttled reconcile. `path_prefix::depth`
/// counts components, so `depth("/") = 0`, `depth("/Users") = 1`,
/// `depth("/Users/<me>") = 2`. A threshold of 2 therefore catches the top ~3
/// levels a root-scale `MustScanSubDirs` collapses to, while a project's `target/`
/// (three levels deep or more, e.g. `/Users/me/projects/foo/target`) still
/// reconciles in place. Depth stands in for walk-the-world cost: two-to-three
/// levels is the band where a reconcile stops being cheap and starts holding the
/// hourglass for the better part of a full scan.
pub(in crate::indexing) const SHALLOW_RESCAN_MAX_DEPTH: usize = 2;

/// Minimum interval between VISIBLE scanner sweeps triggered by shallow routing,
/// PER volume: at most one real sweep per day.
///
/// **The measurement behind the number** (David's machine, 2026-07-18..20, from
/// the recorded scan log): 14 of 28 scans were triggered by a shallow anchor,
/// roughly one every 2.5 hours INCLUDING OVERNIGHT while the machine sat idle
/// (01:17, 03:44, 06:39, 08:46, 11:16 — gaps of 2h27m, 2h55m, 2h07m, 2h30m).
/// **Thirteen of those 14 anchors were `/` itself; the fourteenth was `/System`,
/// a sealed read-only volume where nothing writes.** So the anchor path carries
/// no diagnostic information: macOS is not telling us WHERE churn happened, it
/// is telling us it gave up and coalesced to the watch root. Each trigger runs
/// the serial reconcile walk, measured at 1,309 s on this volume (one earlier run
/// went 2h03m without finishing) — about ten multi-minute-to-multi-hour full
/// walks a day for a signal that says nothing about what changed.
///
/// So a shallow anchor means "this index is now SUSPECT", not "rescan right now".
/// The coalesced anchors are COUNTED (see [`SweepRecord`]) so the volume tooltip
/// can say how many signals we skipped and when the next sweep is due; the badge
/// deliberately stays green, because once-a-day sweeping is the designed operating
/// state, not a fault.
///
/// **Boot disk only** — see [`EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL`].
pub(in crate::indexing) const SHALLOW_RESCAN_MIN_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// The window for a MOUNT-ROOTED volume (an external drive). Deliberately left at
/// the short original cooldown, NOT unified with the boot disk's 24 hours.
///
/// Don't "simplify" this into one constant. Two independent reasons:
/// 1. **No evidence.** The storm was measured on `/`; we have none for external
///    volumes, so widening their window buys no measured benefit.
/// 2. **No safety net there.** The per-navigation verifier (`reconcile/verifier.rs`) is
///    root-scoped: it reads the ROOT `ReadPool` and bails inert on a mount-rooted
///    volume. Meanwhile `/Volumes/<name>` is depth 2, so external anchors DO
///    classify as shallow. A 24-hour blind window there would be a pure
///    correctness regression on the one volume kind with zero verifier cover.
pub(in crate::indexing) const EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL: Duration = Duration::from_secs(45);

/// The sweep window for a volume, by whether it's the `/`-rooted boot disk.
/// A named function, not an inline ternary, so the boot-disk-only scope of the
/// once-a-day policy is greppable and testable.
pub(in crate::indexing) fn min_interval_for(is_boot_disk: bool) -> Duration {
    if is_boot_disk {
        SHALLOW_RESCAN_MIN_INTERVAL
    } else {
        EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL
    }
}

/// Where a `MustScanSubDirs` anchor should go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum RescanRoute {
    /// Shallow/root-scale: take the visible `start_scan` path (like a channel
    /// overflow) — single-flight, updates freshness, no per-dir hourglass hold.
    Scanner,
    /// Deep/narrow: keep the throttled `reconcile_subtree` drain.
    Reconcile,
}

/// Classify a `MustScanSubDirs` anchor by its component depth. Pure so the split
/// is one testable decision, independent of the trigger, the reconciler state, and
/// the cooldown.
pub(in crate::indexing) fn classify(anchor_depth: usize) -> RescanRoute {
    if anchor_depth <= SHALLOW_RESCAN_MAX_DEPTH {
        RescanRoute::Scanner
    } else {
        RescanRoute::Reconcile
    }
}

/// What the caller must do with a shallow anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum ShallowAnchorAction {
    /// The window has elapsed: run the visible full sweep now.
    Sweep,
    /// Inside the window: skip the sweep. The anchor is COUNTED, not forgotten,
    /// so the volume tooltip can tell the user how many change signals macOS lost
    /// since the last sweep, and when the next one is due.
    Coalesce,
}

/// One volume's sweep bookkeeping.
///
/// Wall-clock (unix seconds), NOT `Instant`, for two reasons. (1) `Instant` on
/// macOS is `mach_absolute_time`, which does not tick while the machine is
/// asleep, so an `Instant`-based "day" on a laptop that sleeps 8 hours a night is
/// really 32 hours of wall time; a policy expressed in days should measure days.
/// (2) `Instant` cannot be restored from disk, and both fields MUST survive
/// relaunch (David restarts often, and a 24-hour window spans many restarts).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::indexing) struct SweepRecord {
    /// When the last full walk was TRIGGERED or completed (unix secs).
    ///
    /// Trigger time counts, not only completion, and that is load-bearing:
    /// `start_scan` DELETES `meta.scan_completed_at` before walking (`lifecycle/manager.rs`,
    /// so a killed scan heals to a fresh one). If the window read only
    /// `scan_completed_at`, an interrupted sweep would leave it absent, the window
    /// would look permanently expired, and we would sweep on every launch — the
    /// exact bug this policy fixes, inverted. So a triggered sweep stamps
    /// [`SHALLOW_SWEEP_AT_KEY`] immediately.
    pub last_sweep_unix: Option<u64>,
    /// Shallow anchors coalesced SINCE THE LAST COMPLETED SWEEP — never a lifetime
    /// total, which would only measure how long the app has been installed. Reset
    /// by [`record_sweep_completed`].
    pub coalesced_since_sweep: u32,
}

/// Per-volume sweep bookkeeping. Module-global, NOT per-reconciler: a reconciler
/// is recreated on every scan cycle, so a per-instance field would reset the
/// window each time and let a churning root re-scan back-to-back across cycles.
/// Seeded from `meta` at index start ([`seed_from_meta`]).
static SHALLOW_SWEEPS: LazyLock<Mutex<HashMap<String, SweepRecord>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// `meta` key mirroring [`SweepRecord::last_sweep_unix`].
pub(in crate::indexing) const SHALLOW_SWEEP_AT_KEY: &str = "shallow_sweep_at";
/// `meta` key mirroring [`SweepRecord::coalesced_since_sweep`].
pub(in crate::indexing) const SHALLOW_COALESCED_KEY: &str = "shallow_coalesced_since_sweep";

/// Now, in UNIX seconds. A pre-1970 clock reads as 0 (which only makes the next
/// shallow anchor sweep, never starve).
pub(in crate::indexing) fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Whether the min-interval has elapsed. Pure, so the window is one testable
/// decision. A `last` in the FUTURE (a backwards clock jump, or an index DB
/// carried over from a machine with a skewed clock) counts as elapsed: a bogus
/// record must not wedge sweeps shut for years, and the cost of being wrong here
/// is one extra sweep.
fn window_elapsed(last: Option<u64>, now: u64, min_interval: Duration) -> bool {
    match last {
        None => true,
        Some(last) if now < last => true,
        Some(last) => now - last >= min_interval.as_secs(),
    }
}

/// Decide what to do with a shallow anchor on `volume_id` at `now` (unix secs),
/// under `min_interval` (see [`min_interval_for`]). Returns the decision plus the
/// resulting record, which the caller mirrors into `meta`. Clock-injected.
pub(in crate::indexing) fn decide_shallow_anchor(
    volume_id: &str,
    now: u64,
    min_interval: Duration,
) -> (ShallowAnchorAction, SweepRecord) {
    decide_shallow_anchor_in(&mut SHALLOW_SWEEPS.lock_ignore_poison(), volume_id, now, min_interval)
}

/// The decision itself, over an EXPLICIT ledger. Split out so the policy is
/// testable against a local `HashMap` instead of the process-global one: tests
/// run in parallel threads, and a shared global plus a `clear()`-style reset is a
/// race, not a fixture (it flaked exactly that way while being written).
fn decide_shallow_anchor_in(
    ledger: &mut HashMap<String, SweepRecord>,
    volume_id: &str,
    now: u64,
    min_interval: Duration,
) -> (ShallowAnchorAction, SweepRecord) {
    let record = ledger.entry(volume_id.to_string()).or_default();
    let action = if window_elapsed(record.last_sweep_unix, now, min_interval) {
        record.last_sweep_unix = Some(now);
        ShallowAnchorAction::Sweep
    } else {
        // Saturating: a pathological storm must not wrap the counter back to 0 and
        // silently under-report in the tooltip.
        record.coalesced_since_sweep = record.coalesced_since_sweep.saturating_add(1);
        ShallowAnchorAction::Coalesce
    };
    (action, *record)
}

/// Record that a full walk COMPLETED: restart the window and clear the coalesced
/// count, since the drift those signals stood for has now been repaired.
///
/// Fires for EVERY completed full walk, not only a shallow-triggered one: the
/// window means "a full walk happened recently", so the user's own "Rescan now"
/// counts too.
pub(in crate::indexing) fn record_sweep_completed(volume_id: &str, now: u64) -> SweepRecord {
    record_sweep_completed_in(&mut SHALLOW_SWEEPS.lock_ignore_poison(), volume_id, now)
}

fn record_sweep_completed_in(ledger: &mut HashMap<String, SweepRecord>, volume_id: &str, now: u64) -> SweepRecord {
    let record = ledger.entry(volume_id.to_string()).or_default();
    record.last_sweep_unix = Some(now);
    record.coalesced_since_sweep = 0;
    *record
}

/// This volume's current record, for the status surface the badge and tooltip read.
pub(in crate::indexing) fn sweep_record(volume_id: &str) -> SweepRecord {
    SHALLOW_SWEEPS
        .lock_ignore_poison()
        .get(volume_id)
        .copied()
        .unwrap_or_default()
}

/// Seed both fields from persisted `meta` at index start, so the window AND the
/// count survive relaunch.
///
/// `last_sweep` is the LATER of [`SHALLOW_SWEEP_AT_KEY`] (stamped when a sweep is
/// triggered) and `meta.scan_completed_at` (stamped when any full walk finishes).
/// Reading both is what makes an interrupted sweep safe (see
/// [`SweepRecord::last_sweep_unix`]) while still letting a manual rescan restart
/// the window.
///
/// Never moves the window BACKWARDS (`max`): a stale on-disk timestamp must not
/// undo a sweep this process already ran.
pub(in crate::indexing) fn seed_from_meta(volume_id: &str, last_sweep: Option<u64>, coalesced: u32) {
    seed_from_meta_in(
        &mut SHALLOW_SWEEPS.lock_ignore_poison(),
        volume_id,
        last_sweep,
        coalesced,
    );
}

fn seed_from_meta_in(
    ledger: &mut HashMap<String, SweepRecord>,
    volume_id: &str,
    last_sweep: Option<u64>,
    coalesced: u32,
) {
    let record = ledger.entry(volume_id.to_string()).or_default();
    record.last_sweep_unix = match (record.last_sweep_unix, last_sweep) {
        (Some(existing), Some(seeded)) => Some(existing.max(seeded)),
        (existing, seeded) => existing.or(seeded),
    };
    record.coalesced_since_sweep = record.coalesced_since_sweep.max(coalesced);
}

/// Test-only: clear the PROCESS-GLOBAL ledger. Only for the `reconcile/reconciler/tests.rs`
/// cases that drive a real event through `process_live_event` and therefore can't
/// inject a ledger; those already serialize on `PENDING_SIZES_TEST_MUTEX`. The
/// policy's own tests use a local ledger via [`decide_shallow_anchor_in`] instead,
/// because clearing a shared global from parallel tests is a race.
#[cfg(test)]
pub(in crate::indexing) fn reset_cooldown_for_test() {
    SHALLOW_SWEEPS.lock_ignore_poison().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shallow_anchors_route_to_the_scanner() {
        // The top ~3 levels a root-scale MustScanSubDirs collapses to.
        assert_eq!(classify(0), RescanRoute::Scanner, "/ routes to the scanner");
        assert_eq!(classify(1), RescanRoute::Scanner, "/Users routes to the scanner");
        assert_eq!(classify(2), RescanRoute::Scanner, "/Users/<me> routes to the scanner");
    }

    #[test]
    fn deep_anchors_keep_the_reconcile_drain() {
        // A project's target/ and anything deeper: the throttled reconcile is good
        // at these, so they must NOT take the visible scanner path.
        assert_eq!(classify(3), RescanRoute::Reconcile, "a depth-3 anchor reconciles");
        assert_eq!(classify(6), RescanRoute::Reconcile, "a deep anchor reconciles");
    }

    /// A fixed, readable wall-clock base so the arithmetic in these tests is
    /// obvious (2026-07-20T00:00:00Z). Nothing depends on the exact value.
    const T0: u64 = 1_784_505_600;
    const HOUR: u64 = 3600;

    /// A fresh, test-local ledger. Never the process-global one: these tests run
    /// in parallel threads.
    fn new_ledger() -> HashMap<String, SweepRecord> {
        HashMap::new()
    }

    /// The boot disk's window. Shorthand so each test reads as policy, not plumbing.
    fn boot(ledger: &mut HashMap<String, SweepRecord>, vol: &str, now: u64) -> (ShallowAnchorAction, SweepRecord) {
        decide_shallow_anchor_in(ledger, vol, now, min_interval_for(true))
    }

    #[test]
    fn first_shallow_anchor_sweeps_then_the_day_is_coalesced() {
        let mut ledger = new_ledger();
        let vol = "sweep-vol-a";
        assert_eq!(
            boot(&mut ledger, vol, T0).0,
            ShallowAnchorAction::Sweep,
            "the first shallow trigger is the leading edge"
        );
        assert_eq!(
            boot(&mut ledger, vol, T0 + 1).0,
            ShallowAnchorAction::Coalesce,
            "a second trigger 1 s later is coalesced"
        );
        // The measured production cadence: shallow anchors arrive roughly every
        // 2.5 hours, all day and all night, and all 13-14 of them named `/` or
        // `/System`. Every one after the first must be coalesced, not turned into
        // another multi-minute serial reconcile walk.
        for (elapsed, label) in [
            (2 * HOUR + 27 * 60, "2h27m"),
            (5 * HOUR + 22 * 60, "5h22m"),
            (7 * HOUR + 29 * 60, "7h29m"),
            (9 * HOUR + 59 * 60, "9h59m"),
            (23 * HOUR + 59 * 60, "23h59m"),
        ] {
            assert_eq!(
                boot(&mut ledger, vol, T0 + elapsed).0,
                ShallowAnchorAction::Coalesce,
                "the real-world anchor {label} in is coalesced, not swept"
            );
        }
    }

    #[test]
    fn a_shallow_anchor_after_the_window_sweeps_again() {
        let mut ledger = new_ledger();
        let vol = "sweep-vol-b";
        assert_eq!(boot(&mut ledger, vol, T0).0, ShallowAnchorAction::Sweep);
        assert_eq!(
            boot(&mut ledger, vol, T0 + SHALLOW_RESCAN_MIN_INTERVAL.as_secs()).0,
            ShallowAnchorAction::Sweep,
            "exactly one window later is allowed again (never starves)"
        );
    }

    #[test]
    fn the_window_is_tracked_independently_per_volume() {
        let mut ledger = new_ledger();
        assert_eq!(boot(&mut ledger, "sweep-vol-c", T0).0, ShallowAnchorAction::Sweep);
        assert_eq!(
            boot(&mut ledger, "sweep-vol-d", T0).0,
            ShallowAnchorAction::Sweep,
            "a different volume has its own leading edge"
        );
        assert_eq!(
            boot(&mut ledger, "sweep-vol-c", T0 + HOUR).0,
            ShallowAnchorAction::Coalesce,
            "the first volume is still inside its window"
        );
    }

    /// An EXTERNAL (mount-rooted) volume keeps the short cooldown. The once-a-day
    /// window is boot-disk-only on purpose: the storm was measured on `/`, and the
    /// per-navigation verifier is root-scoped, so an external drive has zero cover
    /// between sweeps. Don't unify these two constants.
    #[test]
    fn an_external_volume_keeps_the_short_cooldown() {
        let mut ledger = new_ledger();
        let vol = "mtp-phone";
        let external = min_interval_for(false);
        assert_eq!(
            decide_shallow_anchor_in(&mut ledger, vol, T0, external).0,
            ShallowAnchorAction::Sweep
        );
        assert_eq!(
            decide_shallow_anchor_in(&mut ledger, vol, T0 + 10, external).0,
            ShallowAnchorAction::Coalesce,
            "10 s in is still inside the short cooldown"
        );
        // An hour later an external volume sweeps again, where the boot disk would
        // still be coalescing. This is the whole point of keeping them separate.
        assert_eq!(
            decide_shallow_anchor_in(&mut ledger, vol, T0 + HOUR, external).0,
            ShallowAnchorAction::Sweep,
            "the external window is the short one, not the day"
        );
        let mut boot_ledger = new_ledger();
        assert_eq!(boot(&mut boot_ledger, "root", T0).0, ShallowAnchorAction::Sweep);
        assert_eq!(
            boot(&mut boot_ledger, "root", T0 + HOUR).0,
            ShallowAnchorAction::Coalesce,
            "the boot disk at the same +1 h is still coalescing"
        );
    }

    #[test]
    fn min_interval_is_a_day_for_the_boot_disk_and_short_elsewhere() {
        assert_eq!(min_interval_for(true), SHALLOW_RESCAN_MIN_INTERVAL);
        assert_eq!(min_interval_for(false), EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL);
        assert!(
            EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL < SHALLOW_RESCAN_MIN_INTERVAL,
            "an external volume must never get a LONGER blind window than the boot disk"
        );
    }

    /// The persistence contract. A process-global ledger is exactly what a
    /// relaunch loses, and David restarts often: without seeding, every launch
    /// would hand the next shallow anchor a free full sweep, and a 24-hour policy
    /// that resets on launch is not a 24-hour policy.
    #[test]
    fn the_window_survives_a_restart_via_persisted_meta() {
        let vol = "sweep-vol-restart";
        let mut before_restart = new_ledger();
        assert_eq!(boot(&mut before_restart, vol, T0).0, ShallowAnchorAction::Sweep);

        // Restart: the process-global ledger is exactly what a relaunch loses, so
        // the new process starts from an EMPTY one and reseeds from `meta`.
        let mut after_restart = new_ledger();
        seed_from_meta_in(&mut after_restart, vol, Some(T0), 0);

        assert_eq!(
            boot(&mut after_restart, vol, T0 + 3 * HOUR).0,
            ShallowAnchorAction::Coalesce,
            "3 h after the last sweep, a fresh process must still coalesce"
        );
        assert_eq!(
            boot(&mut after_restart, vol, T0 + 25 * HOUR).0,
            ShallowAnchorAction::Sweep,
            "once a real day has passed, the fresh process sweeps"
        );
    }

    /// The coalesce COUNT survives a restart too. A 24-hour window spans many
    /// restarts, so a count that reset on launch would under-report in the tooltip.
    #[test]
    fn the_coalesced_count_survives_a_restart() {
        let vol = "sweep-vol-count-restart";
        let mut after_restart = new_ledger();
        seed_from_meta_in(&mut after_restart, vol, Some(T0), 4);

        let (action, record) = boot(&mut after_restart, vol, T0 + HOUR);
        assert_eq!(action, ShallowAnchorAction::Coalesce);
        assert_eq!(
            record.coalesced_since_sweep, 5,
            "the 5th coalesce continues the persisted count, it doesn't restart at 1"
        );
    }

    /// The interrupted-sweep hazard, pinned. `start_scan` DELETES
    /// `meta.scan_completed_at` before walking, so a sweep that never finishes
    /// leaves that key absent. If the window were derived from `scan_completed_at`
    /// alone, it would read as "never swept" forever and we'd sweep on every single
    /// launch — the bug this policy fixes, inverted. The separately-stamped
    /// trigger time is what prevents that.
    #[test]
    fn an_interrupted_sweep_does_not_reopen_the_window_on_relaunch() {
        let vol = "sweep-vol-interrupted";
        let mut during_sweep = new_ledger();
        let (action, record) = boot(&mut during_sweep, vol, T0);
        assert_eq!(action, ShallowAnchorAction::Sweep);
        let stamped_at_trigger = record.last_sweep_unix.expect("a triggered sweep stamps its time");

        // The sweep is killed mid-walk: `scan_completed_at` was deleted at start and
        // never rewritten, so the ONLY surviving fact is the trigger stamp.
        let mut after_relaunch = new_ledger();
        seed_from_meta_in(&mut after_relaunch, vol, Some(stamped_at_trigger), 0);

        assert_eq!(
            boot(&mut after_relaunch, vol, T0 + 2 * HOUR).0,
            ShallowAnchorAction::Coalesce,
            "an interrupted sweep still holds the window shut; we must not sweep every launch"
        );
    }

    /// A completed sweep restarts the window AND zeroes the count, because the
    /// drift those coalesced signals stood for has been repaired. The count is
    /// "since the last completed sweep", never a lifetime total.
    #[test]
    fn a_completed_sweep_restarts_the_window_and_clears_the_count() {
        let mut ledger = new_ledger();
        let vol = "sweep-vol-completed";
        assert_eq!(boot(&mut ledger, vol, T0).0, ShallowAnchorAction::Sweep);
        for i in 1..=3 {
            let (_, record) = boot(&mut ledger, vol, T0 + i * HOUR);
            assert_eq!(record.coalesced_since_sweep, i as u32);
        }

        let record = record_sweep_completed_in(&mut ledger, vol, T0 + 4 * HOUR);
        assert_eq!(record.coalesced_since_sweep, 0, "a completed sweep repaired the drift");
        assert_eq!(record.last_sweep_unix, Some(T0 + 4 * HOUR));

        assert_eq!(
            boot(&mut ledger, vol, T0 + 27 * HOUR).0,
            ShallowAnchorAction::Coalesce,
            "the window restarted at completion time, so 27 h after T0 is only 23 h in"
        );
    }

    /// Seeding must never move the window BACKWARDS: a sweep this process already
    /// ran outranks an older on-disk timestamp arriving late.
    #[test]
    fn seeding_never_undoes_a_sweep_this_process_already_ran() {
        let mut ledger = new_ledger();
        let vol = "sweep-vol-seed-order";
        assert_eq!(boot(&mut ledger, vol, T0 + 10 * HOUR).0, ShallowAnchorAction::Sweep);
        seed_from_meta_in(&mut ledger, vol, Some(T0), 0); // older on-disk value
        assert_eq!(
            boot(&mut ledger, vol, T0 + 20 * HOUR).0,
            ShallowAnchorAction::Coalesce,
            "the in-process sweep at +10 h still governs, so +20 h is inside the window"
        );
    }

    /// A backwards clock jump (or a DB carried from a skewed machine) must not
    /// wedge sweeps shut for years. Failing toward one extra sweep is the safe
    /// direction; failing toward "never sweep again" is not.
    #[test]
    fn a_future_timestamp_does_not_wedge_the_window_shut() {
        let day = SHALLOW_RESCAN_MIN_INTERVAL;
        assert!(window_elapsed(None, T0, day), "no record yet ⇒ sweep");
        assert!(
            window_elapsed(Some(T0 + 400 * 24 * HOUR), T0, day),
            "a last-sweep time in the future is treated as elapsed"
        );
        assert!(
            !window_elapsed(Some(T0), T0 + 23 * HOUR, day),
            "23 h in is still inside"
        );
        assert!(window_elapsed(Some(T0), T0 + 24 * HOUR, day), "24 h in is elapsed");
    }
}

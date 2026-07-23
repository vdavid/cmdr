//! The one INFO line the rescan drain owes a reader: "we are reconciling far
//! more than this machine should need to".
//!
//! Per-walk lines are DEBUG, because there are thousands of them a day and most
//! say `+0 -0 ~0`. What a reader actually needs is the aggregate, so this rolls
//! every completed reconcile into a [`CHURN_WINDOW`] and emits at most ONE line
//! per window, only when the window crossed a budget. A quiet machine stays
//! silent forever; a churning one names the totals and the top anchors by cost,
//! because "which folder" is the whole diagnostic value.
//!
//! Shape follows [`super::rescan_throttle`]: the accumulate/threshold/format
//! engine is pure and clock-injected, so every rule is unit-tested without a
//! logger, a clock, or a filesystem. The impure part is the three thin fns at the
//! bottom, which own the global, the clock, and the `log::info!`.

use crate::ignore_poison::IgnorePoison;
use crate::pluralize::pluralize_grouped;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// How much reconciling gets rolled into one line. Long enough that a burst of
/// activity (a build, an update, a big copy) lands inside a single window instead
/// of ringing three times; short enough to still point at what is happening now.
pub(in crate::indexing) const CHURN_WINDOW: Duration = Duration::from_secs(15 * 60);

/// Cumulative walk time in one window that counts as too much. A window's worth
/// of background walking past this is a machine spending real CPU on staying in
/// sync, which is exactly what the reader wants to hear about.
const WALK_BUDGET: Duration = Duration::from_secs(60);

/// Cumulative row changes in one window that count as too much. Catches the churn
/// a cheap walk can still cause: a subtree that rewrites its rows over and over
/// costs the writer and the aggregates, however fast the listing was.
const ROW_BUDGET: u64 = 100_000;

/// How many anchors keep per-anchor tallies within a window. The machine can
/// produce thousands of distinct anchors a day (most of them one-shot), so the map
/// is capped: past it, only an anchor that outspends the cheapest one tracked gets
/// in. Totals stay exact whatever the cap does.
const MAX_TRACKED_ANCHORS: usize = 64;

/// Anchors named in the line. Three is enough to point at a culprit without
/// turning one line into a list.
const TOP_ANCHORS: usize = 3;

/// One anchor's reconciles within the current window.
#[derive(Default)]
struct AnchorTally {
    walks: u64,
    cost: Duration,
}

/// One anchor's line in a report, ranked by cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::indexing) struct TopAnchor {
    pub(in crate::indexing) path: String,
    pub(in crate::indexing) walks: u64,
    pub(in crate::indexing) cost: Duration,
}

/// One window's aggregate, built only when a budget was crossed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::indexing) struct ChurnReport {
    /// The MEASURED window length, not the configured one: the poll that closes a
    /// window rides the ~1 s sweep tick, and a busy machine can stretch it.
    pub(in crate::indexing) elapsed: Duration,
    pub(in crate::indexing) reconciles: u64,
    pub(in crate::indexing) walked: Duration,
    pub(in crate::indexing) rows: u64,
    /// Anchors tracked (never more than [`MAX_TRACKED_ANCHORS`]).
    pub(in crate::indexing) anchors: usize,
    /// Whether the anchor map hit its cap, so `anchors` is a floor.
    pub(in crate::indexing) anchors_capped: bool,
    /// `MustScanSubDirs` signals the throttle or the settle delay held back. Zero
    /// while the window churns is the first sign one of them regressed.
    pub(in crate::indexing) held_back: u64,
    pub(in crate::indexing) top: Vec<TopAnchor>,
}

/// Rolling window over completed subtree reconciles. Pure + clock-injected: it
/// accumulates, decides, and formats, and does nothing else.
pub(in crate::indexing) struct RescanChurnWindow {
    window: Duration,
    walk_budget: Duration,
    row_budget: u64,
    max_anchors: usize,
    started: Instant,
    reconciles: u64,
    walked: Duration,
    rows: u64,
    held_back: u64,
    anchors: HashMap<PathBuf, AnchorTally>,
    anchors_capped: bool,
}

impl RescanChurnWindow {
    /// A window with the production budgets, starting at `now`.
    pub(in crate::indexing) fn new(now: Instant) -> Self {
        Self {
            window: CHURN_WINDOW,
            walk_budget: WALK_BUDGET,
            row_budget: ROW_BUDGET,
            max_anchors: MAX_TRACKED_ANCHORS,
            started: now,
            reconciles: 0,
            walked: Duration::ZERO,
            rows: 0,
            held_back: 0,
            anchors: HashMap::new(),
            anchors_capped: false,
        }
    }

    /// Fold one completed reconcile in. Totals always take it; the per-anchor
    /// tally takes it if the anchor is tracked or earns a slot (see
    /// [`MAX_TRACKED_ANCHORS`]). Two map operations and no allocation in the
    /// common case, so a quiet machine pays essentially nothing.
    pub(in crate::indexing) fn record_reconcile(&mut self, anchor: &Path, walk_cost: Duration, rows: u64) {
        self.reconciles = self.reconciles.saturating_add(1);
        self.walked = self.walked.saturating_add(walk_cost);
        self.rows = self.rows.saturating_add(rows);

        if let Some(tally) = self.anchors.get_mut(anchor) {
            tally.walks = tally.walks.saturating_add(1);
            tally.cost = tally.cost.saturating_add(walk_cost);
            return;
        }
        if self.anchors.len() >= self.max_anchors {
            self.anchors_capped = true;
            // The cap is what keeps this safe on a machine that produces thousands
            // of one-shot anchors a day. Refusing every newcomer would be worse than
            // no feature at all: the cheap one-shots would fill the map in minutes,
            // and the expensive anchor that shows up later (the ONE the reader
            // needs) would never be named. So the cheapest tracked anchor gives way
            // to anyone who outspent it, and the ranking converges on what matters.
            // One clone, on the loser, not one per candidate: this runs per new
            // anchor once the map is full, and a churning window has thousands.
            let Some((cheapest, cheapest_cost)) = self
                .anchors
                .iter()
                .min_by(|a, b| a.1.cost.cmp(&b.1.cost).then_with(|| a.0.cmp(b.0)))
                .map(|(path, tally)| (path.clone(), tally.cost))
            else {
                return;
            };
            if walk_cost <= cheapest_cost {
                return;
            }
            self.anchors.remove(&cheapest);
        }
        self.anchors.insert(
            anchor.to_path_buf(),
            AnchorTally {
                walks: 1,
                cost: walk_cost,
            },
        );
    }

    /// Note one `MustScanSubDirs` signal that arrived for an anchor which may not
    /// walk yet.
    pub(in crate::indexing) fn record_held_back(&mut self) {
        self.held_back = self.held_back.saturating_add(1);
    }

    /// Close the window if `now` reached its end, returning a report only when a
    /// budget was crossed. Resets either way, so a window reports at most once and
    /// a quiet stretch can't accumulate its way to a line hours later.
    pub(in crate::indexing) fn poll(&mut self, now: Instant) -> Option<ChurnReport> {
        let elapsed = now.saturating_duration_since(self.started);
        if elapsed < self.window {
            return None;
        }
        // Build nothing unless there's something to say: the ranking sort and every
        // allocation below live on the emit path only.
        let report = (self.walked > self.walk_budget || self.rows > self.row_budget).then(|| ChurnReport {
            elapsed,
            reconciles: self.reconciles,
            walked: self.walked,
            rows: self.rows,
            anchors: self.anchors.len(),
            anchors_capped: self.anchors_capped,
            held_back: self.held_back,
            top: self.rank_anchors(),
        });
        self.reset(now);
        report
    }

    /// The costliest anchors, cost-desc then path-asc so the order is stable.
    fn rank_anchors(&self) -> Vec<TopAnchor> {
        let mut ranked: Vec<TopAnchor> = self
            .anchors
            .iter()
            .map(|(path, tally)| TopAnchor {
                path: path.to_string_lossy().into_owned(),
                walks: tally.walks,
                cost: tally.cost,
            })
            .collect();
        ranked.sort_by(|a, b| b.cost.cmp(&a.cost).then_with(|| a.path.cmp(&b.path)));
        ranked.truncate(TOP_ANCHORS);
        ranked
    }

    /// Start a fresh window at `now`. Nothing survives it, which is the other half
    /// of the memory bound.
    fn reset(&mut self, now: Instant) {
        self.started = now;
        self.reconciles = 0;
        self.walked = Duration::ZERO;
        self.rows = 0;
        self.held_back = 0;
        self.anchors.clear();
        self.anchors.shrink_to_fit();
        self.anchors_capped = false;
    }
}

impl ChurnReport {
    /// The line, ready to log. The window length is the MEASURED one, so a line
    /// closed late by a busy machine says so instead of claiming 15 minutes.
    pub(in crate::indexing) fn message(&self) -> String {
        let minutes = self.elapsed.as_secs() / 60;
        let reconciles = pluralize_grouped(self.reconciles, "subtree reconcile");
        let walked = format_duration(self.walked);
        let rows = pluralize_grouped(self.rows, "row change");
        let anchors = if self.anchors_capped {
            // A floor, not a count: the map stopped tracking new anchors here.
            format!("{}+ anchors", self.anchors)
        } else {
            pluralize_grouped(self.anchors as u64, "anchor")
        };
        let held_back = pluralize_grouped(self.held_back, "signal");
        let mut line = format!(
            "Reconciler: heavy churn in the last {minutes} min: {reconciles}, {walked} of walking, {rows}, {anchors}, {held_back} held back."
        );
        if !self.top.is_empty() {
            let top: Vec<String> = self
                .top
                .iter()
                .map(|anchor| {
                    format!(
                        "{} ({}, {})",
                        anchor.path,
                        pluralize_grouped(anchor.walks, "walk"),
                        format_duration(anchor.cost)
                    )
                })
                .collect();
            line.push_str(&format!(" Top: {}", top.join(", ")));
        }
        line
    }
}

/// A walk's cost, at the precision that reads honestly: whole seconds once it's
/// worth a second, milliseconds below that.
fn format_duration(duration: Duration) -> String {
    if duration < Duration::from_secs(1) {
        format!("{}ms", duration.as_millis())
    } else {
        format!("{}s", duration.as_secs())
    }
}

/// The process-wide window. Reconcile churn is a MACHINE-level question (two
/// volumes each walking 40 s is 80 s of this machine's CPU), so the accumulator
/// sums across volumes rather than living per-reconciler. Same reasoning that
/// makes `DEBUG_STATS` app-wide.
static WINDOW: LazyLock<Mutex<RescanChurnWindow>> =
    LazyLock::new(|| Mutex::new(RescanChurnWindow::new(Instant::now())));

/// Record a completed reconcile, and emit if that closed a busy window.
pub(super) fn record_reconcile(anchor: &Path, walk_cost: Duration, rows: u64) {
    let now = Instant::now();
    let report = {
        let mut window = WINDOW.lock_ignore_poison();
        window.record_reconcile(anchor, walk_cost, rows);
        window.poll(now)
    };
    emit(report);
}

/// Record one held-back `MustScanSubDirs` signal.
pub(super) fn record_held_back() {
    WINDOW.lock_ignore_poison().record_held_back();
}

/// Close the window if it's due, from the ~1 s sweep tick. Without this a burst
/// followed by silence would never close its window.
pub(super) fn poll_window() {
    let now = Instant::now();
    let report = WINDOW.lock_ignore_poison().poll(now);
    emit(report);
}

/// Log a report, if there is one. Outside the lock: formatting must never run
/// under it.
fn emit(report: Option<ChurnReport>) {
    if let Some(report) = report {
        log::info!("{}", report.message());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An ordinary day: plenty of cheap reconciles, nothing worth a line.
    #[test]
    fn a_quiet_window_says_nothing() {
        let t0 = Instant::now();
        let mut window = RescanChurnWindow::new(t0);
        for _ in 0..20 {
            window.record_reconcile(Path::new("/Users/me/Documents"), Duration::from_millis(200), 5);
        }
        assert!(
            window.poll(t0 + CHURN_WINDOW).is_none(),
            "4s of walking and 100 row changes is a normal machine, so the log stays quiet"
        );
    }

    /// The walk-time budget: a subtree eating real CPU is named, once.
    #[test]
    fn a_window_over_the_walk_budget_reports_once() {
        let t0 = Instant::now();
        let mut window = RescanChurnWindow::new(t0);
        for _ in 0..30 {
            window.record_reconcile(Path::new("/Users/me/build"), Duration::from_secs(3), 40);
        }
        assert!(
            window.poll(t0 + CHURN_WINDOW - Duration::from_millis(1)).is_none(),
            "the window hasn't closed yet"
        );
        let report = window
            .poll(t0 + CHURN_WINDOW)
            .expect("90s of walking crosses the budget");
        assert_eq!(report.walked, Duration::from_secs(90));
        assert_eq!(
            report.message(),
            "Reconciler: heavy churn in the last 15 min: 30 subtree reconciles, 90s of walking, \
             1,200 row changes, 1 anchor, 0 signals held back. Top: /Users/me/build (30 walks, 90s)"
        );
    }

    /// The row budget catches what a cheap walk can still cost: a subtree that
    /// rewrites its rows over and over keeps the writer and the aggregates busy.
    #[test]
    fn a_window_over_the_row_budget_reports_once() {
        let t0 = Instant::now();
        let mut at_budget = RescanChurnWindow::new(t0);
        at_budget.record_reconcile(Path::new("/Users/me/cache"), Duration::from_millis(400), ROW_BUDGET);
        assert!(
            at_budget.poll(t0 + CHURN_WINDOW).is_none(),
            "exactly at the budget is not over it"
        );

        let mut over_budget = RescanChurnWindow::new(t0);
        over_budget.record_reconcile(Path::new("/Users/me/cache"), Duration::from_millis(400), ROW_BUDGET + 1);
        let report = over_budget
            .poll(t0 + CHURN_WINDOW)
            .expect("100,001 row changes crosses the budget");
        assert_eq!(report.rows, 100_001);
        assert!(
            report.message().contains("100,001 row changes"),
            "counts carry thousands separators: {}",
            report.message()
        );
        assert!(
            report.message().contains("/Users/me/cache (1 walk, 400ms)"),
            "a sub-second walk still reads honestly: {}",
            report.message()
        );
    }

    /// At most one line per window, and a window that reported starts empty.
    #[test]
    fn a_window_reports_at_most_once() {
        let t0 = Instant::now();
        let mut window = RescanChurnWindow::new(t0);
        window.record_reconcile(Path::new("/a"), Duration::from_secs(90), 10);
        assert!(window.poll(t0 + CHURN_WINDOW).is_some(), "the busy window reports");
        assert!(
            window.poll(t0 + CHURN_WINDOW).is_none(),
            "the same window can't report twice"
        );
        assert!(
            window.poll(t0 + CHURN_WINDOW * 2).is_none(),
            "and the next window starts from nothing"
        );
    }

    /// A window UNDER the budgets still resets, so a quiet stretch can't
    /// accumulate its way to a line hours later.
    #[test]
    fn a_quiet_window_does_not_carry_its_total_forward() {
        let t0 = Instant::now();
        let mut window = RescanChurnWindow::new(t0);
        window.record_reconcile(Path::new("/a"), Duration::from_secs(50), 10);
        assert!(window.poll(t0 + CHURN_WINDOW).is_none(), "50s is under the budget");
        window.record_reconcile(Path::new("/a"), Duration::from_secs(50), 10);
        assert!(
            window.poll(t0 + CHURN_WINDOW * 2).is_none(),
            "the second window is judged on its own 50s, not on 100s carried over"
        );
    }

    /// "Which folder" is the diagnostic value, so the line names the most
    /// EXPENSIVE anchors, not the most frequent ones.
    #[test]
    fn the_top_anchors_are_ranked_by_cost() {
        let t0 = Instant::now();
        let mut window = RescanChurnWindow::new(t0);
        // The cheap anchor walks most often, and still doesn't lead.
        for _ in 0..40 {
            window.record_reconcile(Path::new("/frequent"), Duration::from_millis(100), 1);
        }
        window.record_reconcile(Path::new("/middling"), Duration::from_secs(20), 1);
        window.record_reconcile(Path::new("/priciest"), Duration::from_secs(30), 1);
        window.record_reconcile(Path::new("/priciest"), Duration::from_secs(5), 1);
        window.record_reconcile(Path::new("/quietest"), Duration::from_secs(2), 1);

        let report = window
            .poll(t0 + CHURN_WINDOW)
            .expect("61s of walking crosses the budget");
        assert_eq!(
            report.top,
            vec![
                TopAnchor {
                    path: "/priciest".to_string(),
                    walks: 2,
                    cost: Duration::from_secs(35),
                },
                TopAnchor {
                    path: "/middling".to_string(),
                    walks: 1,
                    cost: Duration::from_secs(20),
                },
                TopAnchor {
                    path: "/frequent".to_string(),
                    walks: 40,
                    cost: Duration::from_secs(4),
                },
            ],
            "top three by cost, and /quietest doesn't make the cut"
        );
    }

    /// The bound that makes this safe to ship: thousands of one-shot anchors a
    /// day can't grow the map. The cap keeps the anchors worth naming, and the
    /// totals stay exact whatever it drops.
    #[test]
    fn the_anchor_map_is_capped_and_keeps_the_expensive_anchors() {
        let t0 = Instant::now();
        let mut window = RescanChurnWindow::new(t0);
        // A day's worth of one-shot anchors, each too cheap to matter...
        for i in 0..2_300 {
            window.record_reconcile(
                &PathBuf::from(format!("/tmp/one-shot-{i}")),
                Duration::from_millis(10),
                3,
            );
        }
        // ...and the anchor that actually costs something, arriving LAST, so a
        // first-come cap would have shut it out entirely.
        window.record_reconcile(Path::new("/Users/me/Library/Caches/hot"), Duration::from_secs(70), 9);

        let report = window
            .poll(t0 + CHURN_WINDOW)
            .expect("93s of walking crosses the budget");
        assert_eq!(report.anchors, MAX_TRACKED_ANCHORS, "the map never grows past its cap");
        assert!(report.anchors_capped, "and the line says the count is a floor");
        assert_eq!(
            report.top.first().map(|a| a.path.as_str()),
            Some("/Users/me/Library/Caches/hot"),
            "a late, expensive anchor displaces a cheap one"
        );
        assert_eq!(report.reconciles, 2_301, "every reconcile counts, capped or not");
        assert_eq!(
            report.walked,
            Duration::from_secs(70) + Duration::from_millis(23_000),
            "and so does every second of walking"
        );
        assert_eq!(report.rows, 2_300 * 3 + 9, "and every row change");
        assert!(
            report.message().contains("64+ anchors"),
            "a capped count reads as a floor: {}",
            report.message()
        );
    }

    /// The number that shows the throttle and the settle delay working. Its
    /// absence during heavy churn is the first sign one of them regressed.
    #[test]
    fn held_back_signals_are_counted_and_reset_per_window() {
        let t0 = Instant::now();
        let mut window = RescanChurnWindow::new(t0);
        window.record_reconcile(Path::new("/a"), Duration::from_secs(90), 10);
        for _ in 0..37 {
            window.record_held_back();
        }
        let report = window.poll(t0 + CHURN_WINDOW).expect("a busy window");
        assert_eq!(report.held_back, 37);
        assert!(
            report.message().contains("37 signals held back"),
            "{}",
            report.message()
        );

        window.record_reconcile(Path::new("/a"), Duration::from_secs(90), 10);
        let next = window.poll(t0 + CHURN_WINDOW * 2).expect("another busy window");
        assert_eq!(next.held_back, 0, "the count is per window, not cumulative");
    }

    /// The window closes on a ~1 s sweep tick that a busy machine can starve, so
    /// the line reports the MEASURED length rather than claiming 15 minutes.
    #[test]
    fn the_line_reports_the_measured_window_length() {
        let t0 = Instant::now();
        let mut window = RescanChurnWindow::new(t0);
        window.record_reconcile(Path::new("/a"), Duration::from_secs(90), 10);
        let report = window
            .poll(t0 + Duration::from_secs(40 * 60))
            .expect("a busy window, closed late");
        assert_eq!(report.elapsed, Duration::from_secs(40 * 60));
        assert!(
            report
                .message()
                .starts_with("Reconciler: heavy churn in the last 40 min:"),
            "{}",
            report.message()
        );
    }
}

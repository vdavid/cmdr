//! Rate-limit a repetitive log line without losing the fact that it happened.
//!
//! Some lines are worth having in an error-report bundle but not worth having a
//! thousand times: a per-tick poller, a per-batch heartbeat, a "nothing changed"
//! summary. Demoting those to TRACE removes them from the bundle entirely (the
//! file sink is Debug, see `docs/tooling/logging.md`), which trades away the
//! diagnostic. This trades away the REPETITION instead: the first occurrence
//! logs immediately, and sustained repetition collapses into one line per
//! [`interval`](LogRollup::new) that says how many it stands for.
//!
//! Keyed, because every caller so far rolls up per volume or per share and a
//! busy volume must not swallow a quiet one's first line.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::ignore_poison::IgnorePoison;

/// What one emitted rollup line stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollupBatch {
    /// Occurrences since the last emitted line, including the one that
    /// triggered this batch. `1` means nothing was suppressed.
    pub count: u64,
    /// Wall time those occurrences span. Zero for a key's first line.
    pub elapsed: Duration,
}

impl RollupBatch {
    /// Whether this batch stands for more than the occurrence that triggered it,
    /// so the line is worth a "×N in Ys" suffix.
    #[must_use]
    pub fn is_rolled_up(&self) -> bool {
        self.count > 1
    }
}

/// One rollup point in the code, tracking each key independently.
///
/// Cheap enough to call on every occurrence (one mutex, one map lookup) and
/// `const`-constructible, so the usual shape is a `static` beside the log call.
pub struct LogRollup {
    interval: Duration,
    keys: Mutex<Option<HashMap<String, KeyState>>>,
}

/// One key's window: when its last line went out, and how many occurrences have
/// been folded into the next one.
struct KeyState {
    last_emit: Instant,
    suppressed: u64,
}

impl LogRollup {
    /// A rollup that emits at most one line per `interval` per key.
    #[must_use]
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            keys: Mutex::new(None),
        }
    }

    /// Record one occurrence for `key`, and say whether to log now.
    ///
    /// `Some` on a key's FIRST occurrence (so a rare event is never delayed or
    /// lost) and once per interval after that; `None` in between, where the
    /// occurrence is counted into the next batch.
    pub fn record(&self, key: &str) -> Option<RollupBatch> {
        self.record_at(key, Instant::now())
    }

    /// [`record`](Self::record) with the clock injected, so the rules are
    /// testable without sleeping.
    pub fn record_at(&self, key: &str, now: Instant) -> Option<RollupBatch> {
        let mut guard = self.keys.lock_ignore_poison();
        let keys = guard.get_or_insert_with(HashMap::new);
        let Some(state) = keys.get_mut(key) else {
            keys.insert(
                key.to_string(),
                KeyState {
                    last_emit: now,
                    suppressed: 0,
                },
            );
            return Some(RollupBatch {
                count: 1,
                elapsed: Duration::ZERO,
            });
        };

        state.suppressed += 1;
        let elapsed = now.saturating_duration_since(state.last_emit);
        if elapsed < self.interval {
            return None;
        }
        let count = state.suppressed;
        state.suppressed = 0;
        state.last_emit = now;
        Some(RollupBatch { count, elapsed })
    }

    /// Drop a key's state, so a volume that goes away stops costing a map entry
    /// and its next first line is immediate again.
    pub fn forget(&self, key: &str) {
        if let Some(keys) = self.keys.lock_ignore_poison().as_mut() {
            keys.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(60);

    /// The first occurrence is never delayed: a rare event still lands in the
    /// bundle at the moment it happened.
    #[test]
    fn the_first_occurrence_logs_immediately() {
        let rollup = LogRollup::new(WINDOW);
        let batch = rollup.record_at("root", Instant::now()).expect("the first one logs");
        assert_eq!(batch.count, 1);
        assert!(!batch.is_rolled_up(), "nothing was suppressed yet");
    }

    /// The whole point: a per-tick line becomes one line per window, carrying
    /// how many ticks it stands for.
    #[test]
    fn repetition_collapses_into_one_line_per_window() {
        let t0 = Instant::now();
        let rollup = LogRollup::new(WINDOW);
        rollup.record_at("root", t0).expect("the first one logs");

        for i in 1..30 {
            assert!(
                rollup.record_at("root", t0 + Duration::from_secs(i)).is_none(),
                "second {i} is inside the window, so it only counts"
            );
        }
        let batch = rollup
            .record_at("root", t0 + WINDOW)
            .expect("the window closed, so this one carries the batch");
        assert_eq!(batch.count, 30, "29 suppressed plus the one that emitted");
        assert_eq!(batch.elapsed, WINDOW);
        assert!(batch.is_rolled_up());
    }

    /// A busy key must not swallow a quiet one's first line, which is why this
    /// is keyed at all.
    #[test]
    fn keys_roll_up_independently() {
        let t0 = Instant::now();
        let rollup = LogRollup::new(WINDOW);
        rollup.record_at("root", t0).expect("root's first line");
        assert!(rollup.record_at("root", t0).is_none(), "root is now inside its window");
        let batch = rollup
            .record_at("naspi", t0)
            .expect("a different share is on its own clock");
        assert_eq!(batch.count, 1);
    }

    /// The count is per batch, not cumulative: two windows of ten each read as
    /// ten and ten, so the rate in the log is the real rate.
    #[test]
    fn each_window_counts_only_its_own() {
        let t0 = Instant::now();
        let rollup = LogRollup::new(WINDOW);
        rollup.record_at("root", t0).expect("the first line");
        for i in 1..10 {
            rollup.record_at("root", t0 + Duration::from_secs(i));
        }
        assert_eq!(rollup.record_at("root", t0 + WINDOW).expect("first batch").count, 10);

        for i in 1..5 {
            rollup.record_at("root", t0 + WINDOW + Duration::from_secs(i));
        }
        assert_eq!(
            rollup.record_at("root", t0 + WINDOW * 2).expect("second batch").count,
            5,
            "the second window is counted on its own, not on top of the first"
        );
    }

    /// A forgotten key starts over, so a remounted volume's next line is
    /// immediate rather than swallowed by a stale window.
    #[test]
    fn forgetting_a_key_restores_its_immediate_first_line() {
        let t0 = Instant::now();
        let rollup = LogRollup::new(WINDOW);
        rollup.record_at("naspi", t0).expect("the first line");
        assert!(rollup.record_at("naspi", t0).is_none());
        rollup.forget("naspi");
        let batch = rollup.record_at("naspi", t0).expect("a forgotten key logs again");
        assert_eq!(batch.count, 1);
    }
}

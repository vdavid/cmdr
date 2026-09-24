//! The `warn` for a metadata round trip (a stat, a listing) that took the
//! server more than a second.
//!
//! A busy QNAP holds 0.5–3% of single metadata requests for 0.3–6 s while its
//! session stays healthy, and nothing else in a log shows it: smb2 rightly
//! treats it as slow rather than dead, so no error, reconnect, or timeout fires.
//! One line per stall would flood the log on a bad NAS, so the lines roll up
//! per share (`cmdr_fs::log_rollup`) and each rolled-up line names the slowest
//! call it stands for. ❌ No path in the line: the share and the call are what a
//! reader needs to place it, and the path is what a bundle shouldn't carry.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_rollup::LogRollup;

/// A metadata call that takes longer than this is the server holding it: a
/// healthy one answers in milliseconds.
const SLOW_CALL: Duration = Duration::from_secs(1);

/// At most one line per share per this window once stalls keep coming.
const ROLLUP_WINDOW: Duration = Duration::from_secs(60);

/// Decides whether a metadata call is worth a line, and words it.
pub(super) struct SlowCallLog {
    rollup: LogRollup,
    /// Per share, the slowest call folded into the next line.
    slowest: Mutex<Option<HashMap<String, Duration>>>,
}

impl SlowCallLog {
    pub(super) const fn new() -> Self {
        Self {
            rollup: LogRollup::new(ROLLUP_WINDOW),
            slowest: Mutex::new(None),
        }
    }

    /// The line to log for `op` on `share` having taken `elapsed`, or `None`
    /// when it was quick or folds into a later line.
    pub(super) fn line_at(&self, share: &str, op: &str, elapsed: Duration, now: Instant) -> Option<String> {
        if elapsed < SLOW_CALL {
            return None;
        }
        // One guard across the rollup's decision, so a stall on another thread
        // can't land between reading the slowest and resetting it.
        let (batch, slowest) = {
            let mut guard = self.slowest.lock_ignore_poison();
            let per_share = guard.get_or_insert_with(HashMap::new);
            let slowest = per_share.entry(share.to_string()).or_default();
            *slowest = (*slowest).max(elapsed);
            let slowest = *slowest;
            let batch = self.rollup.record_at(share, now)?;
            per_share.remove(share);
            (batch, slowest)
        };

        let took = elapsed.as_millis();
        Some(if batch.is_rolled_up() {
            format!(
                "SMB metadata calls were slow: share={share} ×{} over {} s in {}s, slowest {} ms, latest {op} {took} ms. \
                 The server held them; check its load (a busy NAS does this under heavy writes)",
                batch.count,
                SLOW_CALL.as_secs(),
                batch.elapsed.as_secs(),
                slowest.as_millis(),
            )
        } else {
            format!(
                "SMB metadata call was slow: share={share} {op} took {took} ms. \
                 The server held it; check its load (a busy NAS does this under heavy writes)"
            )
        })
    }
}

static SLOW_CALLS: SlowCallLog = SlowCallLog::new();

/// Logs `op` on `share` at `warn` when the server held it, rolled up per share.
pub(super) fn note(share: &str, op: &str, elapsed: Duration) {
    if let Some(line) = SLOW_CALLS.line_at(share, op, elapsed, Instant::now()) {
        log::warn!("{line}");
    }
}

#[cfg(test)]
#[path = "slow_calls_test.rs"]
mod slow_calls_test;

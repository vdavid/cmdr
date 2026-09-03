//! The daily ceiling on what the agent may spend on its own initiative.
//!
//! The fifth gate, beside the three in `readiness.rs` and the `proactive` toggle, and the only
//! one counted in money rather than permission. ⚠️ **It is a backstop, ❌ never a substitute for
//! calibration**: an agent that reaches this ceiling on an ordinary day is miscalibrated, and the
//! fix is upstream in `importance.rs` and `interest.rs`. This is what stops a miscalibration from
//! costing a day's quota before anybody notices.

use rusqlite::Connection;

use crate::agent::store;

const LOG_TARGET: &str = "agent::wake";

/// How many tokens (prompt plus completion) proactive work may spend in one local day.
///
/// The arithmetic: a wake's cheapest turn is roughly 7,000 prompt tokens, so 200,000 buys about
/// 28 of them; one long tool loop cost about 180,000 on 2026-09-03, so a single deep dive plus
/// change also fits. It sits well under that day's 374,127 tokens in seven minutes, which is the
/// number it exists to have refused.
///
/// ⚠️ **Proactive work only.** What the user asks for is never capped and never counted; the two
/// are different money, and one number for both would let a chatty afternoon on the rail starve
/// the wake loop, or a runaway wake loop eat the user's own budget.
pub(super) const DAILY_PROACTIVE_TOKEN_BUDGET: u64 = 200_000;

/// Whether a wake may spend anything, given what proactive work has already cost today.
///
/// A force is a developer asking for a wake and skips it, the same way it skips the timer and the
/// `proactive` toggle.
pub(super) fn may_spend(forced: bool, spent_today: u64) -> bool {
    forced || spent_today < DAILY_PROACTIVE_TOKEN_BUDGET
}

/// What proactive work has spent on `day`, or `0` if the meter could not be read.
///
/// ⚠️ **A read failure fails OPEN**, which is the deliberate half. A broken read means the store
/// is broken, and silencing the agent for a whole day over one SQLite hiccup would be a worse
/// answer than the spacing and the refusal backoff already give. Logged so it is not silent.
pub(super) fn spent_today(conn: &Connection, day: &str) -> u64 {
    match store::proactive_tokens_for_day(conn, day) {
        Ok(tokens) => tokens,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "the agent's daily spend could not be read, so the ceiling is not enforced this pass: {e}");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ **The number this exists to have refused.** On 2026-09-03 the wake loop spent 374,127
    /// tokens in seven minutes and exhausted the user's provider quota for the rest of the day.
    #[test]
    fn the_day_that_exhausted_the_quota_would_have_been_cut_off() {
        assert!(!may_spend(false, 374_127));
        assert!(!may_spend(false, 200_000), "and the ceiling itself is the last word");
        assert!(may_spend(false, 199_999), "a token under it still goes");
    }

    /// An ordinary day is untouched: the ceiling has to sit far enough above real use that
    /// reaching it means something is wrong, or it becomes a cadence knob by accident.
    #[test]
    fn an_ordinary_days_worth_of_wakes_is_nowhere_near_it() {
        // Ten wakes at the cheapest a wake gets.
        assert!(may_spend(false, 10 * 7_000));
        // And one long tool loop, the deepest single wake seen so far.
        assert!(may_spend(false, 180_000));
    }

    /// A force is a developer asking for a wake, and it already skips the timer and the
    /// `proactive` toggle. It skips the ceiling for the same reason: a spent day would otherwise
    /// make the E2E hook silently do nothing.
    #[test]
    fn a_forced_wake_spends_past_the_ceiling() {
        assert!(may_spend(true, 374_127));
        assert!(may_spend(true, u64::MAX));
    }
}

//! How long a session lasts, measured WITHOUT a quit event.
//!
//! ## Why there's no `app_quit`
//!
//! A quit event is unreliable exactly when it matters. A crash, a force-quit, a
//! power cut, and a `SIGKILL` all end a session with no moment left to report in,
//! so a session length counted at the end systematically drops its shortest and
//! its most interesting cases and reads back longer than the truth. That's the
//! same trap `first_index.rs` documents for the interruption rate, and the answer
//! is the same one: count the milestones a session REACHES, and let the absence
//! of the next one be the ending.
//!
//! So this is a ladder. A launch fires `session_reached` at one minute, then five,
//! fifteen, an hour, four, twelve, and twenty-four. Each rung is monotone: once
//! sent, nothing can retract it. The distribution of the top rung per launch is
//! the session-length survival curve, and `app_launched` is its denominator (the
//! zeroth rung, already emitted from setup).
//!
//! ## What it does and doesn't measure
//!
//! It measures **the app being open**, not the person being at the keyboard. A
//! Cmdr left running overnight climbs the whole ladder. That's a deliberate
//! limit: telling "using" from "open" needs input or focus tracking, and watching
//! when someone touches their keyboard is a bigger intrusion than the question is
//! worth. Read the top rungs as "leaves it running", not "worked for 12 hours".

use std::time::Duration;

/// One rung: how long into the session it fires, and the token it reports.
///
/// The tokens are the rung's own name rather than a range, because a rung means
/// "reached at least this", not "ended in this window" — the ranges are what you
/// get by subtracting adjacent rungs at read time.
type Rung = (Duration, &'static str);

/// The ladder, in order. Dense early (a file manager opened to move one file is a
/// real and common session) and sparse late (past a few hours the only question
/// left is whether it's parked).
///
/// ❌ Don't add a rung in the middle without saying so in the catalog: a rung that
/// appears mid-history looks like a behavior change in the survival curve rather
/// than a schema one.
fn ladder() -> [Rung; 7] {
    [
        (Duration::from_secs(60), "1m"),
        (Duration::from_secs(5 * 60), "5m"),
        (Duration::from_secs(15 * 60), "15m"),
        (Duration::from_secs(60 * 60), "1h"),
        (Duration::from_secs(4 * 60 * 60), "4h"),
        (Duration::from_secs(12 * 60 * 60), "12h"),
        (Duration::from_secs(24 * 60 * 60), "24h"),
    ]
}

/// The gap to wait before each rung, given the one before it. Pure, so the
/// schedule is testable without waiting out a day.
fn gaps() -> Vec<(Duration, &'static str)> {
    let mut previous = Duration::ZERO;
    ladder()
        .into_iter()
        .map(|(at, token)| {
            let gap = at.saturating_sub(previous);
            previous = at;
            (gap, token)
        })
        .collect()
}

/// Starts the ladder for this launch. Call once from setup, after
/// [`super::init`].
///
/// One task that sleeps between rungs and exits at the top, so a session parked
/// for a week costs seven events and then nothing. The task dies with the process,
/// which is the whole mechanism: an ending needs no code.
pub fn start() {
    tauri::async_runtime::spawn(async {
        for (gap, token) in gaps() {
            tokio::time::sleep(gap).await;
            super::posthog::capture("session_reached", serde_json::json!({ "milestone": token }));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{gaps, ladder};

    #[test]
    fn the_ladder_only_climbs() {
        let rungs = ladder();
        for pair in rungs.windows(2) {
            assert!(
                pair[1].0 > pair[0].0,
                "rung {} must come after {}",
                pair[1].1,
                pair[0].1
            );
        }
    }

    #[test]
    fn every_rung_has_its_own_token() {
        let rungs = ladder();
        let mut tokens: Vec<&str> = rungs.iter().map(|(_, t)| *t).collect();
        tokens.sort_unstable();
        let count = tokens.len();
        tokens.dedup();
        assert_eq!(tokens.len(), count, "two rungs would be indistinguishable in the data");
    }

    #[test]
    fn gaps_sum_back_to_the_rungs() {
        // The task sleeps gap-by-gap, so a wrong gap would fire a rung at the
        // wrong time and quietly bend the survival curve.
        let mut elapsed = std::time::Duration::ZERO;
        for ((gap, gap_token), (at, rung_token)) in gaps().into_iter().zip(ladder()) {
            elapsed += gap;
            assert_eq!(gap_token, rung_token);
            assert_eq!(elapsed, at, "rung {rung_token} would fire at the wrong time");
        }
    }
}

//! Shared test-only helpers for the whole crate.
//!
//! Today that's one thing: waiting for background work to land. [`wait_until`] serves sync
//! `#[test]`s, [`wait_until_async`] serves `#[tokio::test]`s. Both poll a condition to a deadline
//! and panic when it never holds, so a wait can't silently pass. Don't hand-roll a poll loop, and
//! don't sleep a fixed span hoping the work landed: the sleep inside these two helpers is the only
//! sanctioned one in Rust test code.

use std::future::Future;
use std::panic::Location;
use std::time::{Duration, Instant};

/// How often we re-check the condition: short enough that a satisfied wait returns promptly, long
/// enough that a cheap predicate doesn't spin a core.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Polls `condition` until it holds, panicking after `timeout` if it never does.
///
/// `description` finishes the sentence "timed out after 2s waiting for …", so phrase it as a noun
/// phrase: `"the ByteSeek to LineIndex upgrade to finish"`.
///
/// ❌ Don't call this from an `async` test: `std::thread::sleep` blocks the runtime worker and
/// deadlocks a current-thread scheduler. Use [`wait_until_async`] there.
#[track_caller]
pub(crate) fn wait_until(timeout: Duration, description: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return;
        }
        assert!(Instant::now() < deadline, "{}", timed_out(timeout, description));
        // allowed-test-sleep: the sanctioned poll interval; every sync test wait routes through here
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The async twin of [`wait_until`], for `#[tokio::test]`s.
///
/// Deadline and poll both run on tokio's clock, so a `start_paused` runtime auto-advances through
/// the waiting instead of burning wall-clock.
///
/// This is a plain `fn` returning a future rather than an `async fn` on purpose: `#[track_caller]`
/// doesn't reach through the future an `async fn` generates, so we capture the call site eagerly
/// and put it in the panic message instead.
#[track_caller]
pub(crate) fn wait_until_async<'a>(
    timeout: Duration,
    description: &'a str,
    mut condition: impl FnMut() -> bool + 'a,
) -> impl Future<Output = ()> + 'a {
    let caller = Location::caller();
    async move {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if condition() {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{} (at {caller})",
                timed_out(timeout, description)
            );
            // allowed-test-sleep: the sanctioned poll interval; every async test wait routes through here
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

fn timed_out(timeout: Duration, description: &str) -> String {
    format!("timed out after {timeout:.1?} waiting for {description}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_already_true_condition_returns_without_waiting() {
        let started = Instant::now();
        wait_until(Duration::from_secs(30), "an always-true condition", || true);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_condition_that_turns_true_later_is_picked_up() {
        let mut polls = 0;
        wait_until(Duration::from_secs(5), "the third poll", || {
            polls += 1;
            polls >= 3
        });
        assert_eq!(polls, 3);
    }

    #[test]
    #[should_panic(expected = "timed out after 20.0ms waiting for a condition that never holds")]
    fn a_condition_that_never_holds_panics_with_the_description() {
        wait_until(Duration::from_millis(20), "a condition that never holds", || false);
    }

    #[tokio::test]
    async fn an_already_true_condition_returns_without_waiting_async() {
        let started = Instant::now();
        wait_until_async(Duration::from_secs(30), "an always-true condition", || true).await;
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn a_condition_that_turns_true_later_is_picked_up_async() {
        let mut polls = 0;
        wait_until_async(Duration::from_secs(5), "the third poll", || {
            polls += 1;
            polls >= 3
        })
        .await;
        assert_eq!(polls, 3);
    }

    #[tokio::test]
    #[should_panic(expected = "timed out after 20.0ms waiting for a condition that never holds")]
    async fn a_condition_that_never_holds_panics_with_the_description_async() {
        wait_until_async(Duration::from_millis(20), "a condition that never holds", || false).await;
    }

    #[test]
    fn the_timeout_message_names_the_budget_and_the_condition() {
        assert_eq!(
            timed_out(Duration::from_secs(2), "the upgrade to finish"),
            "timed out after 2.0s waiting for the upgrade to finish"
        );
    }
}

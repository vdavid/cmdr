//! What bounds the verdict: one in flight, at most [`MAX_STARTS`] starts in any
//! [`WINDOW`], and a finished verdict reused from memory when the same node is asked
//! about again.
//!
//! Why: a held-down shortcut opens and closes the card every frame, and each start
//! would bill a call's input even when the card is gone a frame later; a local model
//! takes seconds per answer. The cache lives in memory only (❌ nothing on disk: a
//! verdict is never stored), holds at most [`CACHE_ENTRIES`] answers, and keys on the
//! node, so a file saved over since asks again.

use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::volume::NodeIdentity;
use tokio_util::sync::CancellationToken;

use super::super::event::VerdictContents;
use crate::agent::llm::types::ProviderTag;
use crate::ignore_poison::IgnorePoison;

/// The most verdicts that may start in any [`WINDOW`].
pub(crate) const MAX_STARTS: usize = 12;
/// The window [`MAX_STARTS`] counts over.
pub(crate) const WINDOW: Duration = Duration::from_secs(60);
/// The most finished verdicts kept for a reopen.
pub(crate) const CACHE_ENTRIES: usize = 32;

/// Which item, in which state, by which model: a reopen with every part equal reuses
/// the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheKey {
    pub(crate) volume_id: String,
    pub(crate) path: String,
    pub(crate) node: Option<NodeIdentity>,
    pub(crate) modified: Option<i64>,
    pub(crate) size: Option<u64>,
    pub(crate) provider: ProviderTag,
    pub(crate) model: String,
}

/// A finished verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finished {
    pub(crate) contents: VerdictContents,
    pub(crate) text: String,
}

/// Why a start was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooMany;

struct InFlight {
    request_id: String,
    token: CancellationToken,
    run: u64,
}

#[derive(Default)]
struct State {
    starts: VecDeque<Instant>,
    in_flight: Option<InFlight>,
    cache: VecDeque<(CacheKey, Finished)>,
    next_run: u64,
    /// How many times [`Limits::stop_all`] ran, whether or not a verdict was in flight.
    stops: u64,
}

/// Which [`Limits::stop_all`] a verdict's gate resolved after: read before the gate
/// starts, handed to [`Limits::claim`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StopsSeen(u64);

/// The verdict's limits. One for the app ([`LIMITS`]); a test builds its own.
#[derive(Default)]
pub(crate) struct Limits(Mutex<State>);

/// The one verdict in flight, released when dropped (a newer one may already hold the
/// slot, and keeps it).
pub(crate) struct Claim<'a> {
    limits: &'a Limits,
    run: u64,
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        let mut state = self.limits.0.lock_ignore_poison();
        if state.in_flight.as_ref().is_some_and(|held| held.run == self.run) {
            state.in_flight = None;
        }
    }
}

impl Limits {
    /// Counts a start at `now`, or refuses it when [`MAX_STARTS`] already started in
    /// the last [`WINDOW`].
    pub(crate) fn admit(&self, now: Instant) -> Result<(), TooMany> {
        let mut state = self.0.lock_ignore_poison();
        while state
            .starts
            .front()
            .is_some_and(|started| now.duration_since(*started) >= WINDOW)
        {
            state.starts.pop_front();
        }
        if state.starts.len() >= MAX_STARTS {
            return Err(TooMany);
        }
        state.starts.push_back(now);
        Ok(())
    }

    /// The stops so far, read before a verdict's gate starts resolving.
    pub(crate) fn stops_seen(&self) -> StopsSeen {
        StopsSeen(self.0.lock_ignore_poison().stops)
    }

    /// Makes `token` the one verdict in flight, stopping whichever held the slot.
    ///
    /// ❗ A [`Self::stop_all`] since `seen` (a consent revoke, a policy narrowing,
    /// quitting) landed while the gate resolved and the facts gathered, when there
    /// was no claim yet for it to stop: `token` is stopped at once. Checked under the
    /// same lock `stop_all` takes, so no stop falls between the two.
    pub(crate) fn claim(&self, request_id: &str, token: CancellationToken, seen: StopsSeen) -> Claim<'_> {
        let mut state = self.0.lock_ignore_poison();
        if state.stops != seen.0 {
            token.cancel();
        }
        let run = state.next_run;
        state.next_run += 1;
        let held = state.in_flight.replace(InFlight {
            request_id: request_id.to_string(),
            token,
            run,
        });
        if let Some(older) = held {
            older.token.cancel();
        }
        Claim { limits: self, run }
    }

    /// Stops the verdict of `request_id`, if it's the one in flight. The card stays.
    pub(crate) fn stop(&self, request_id: &str) -> bool {
        let state = self.0.lock_ignore_poison();
        match &state.in_flight {
            Some(held) if held.request_id == request_id => {
                held.token.cancel();
                true
            }
            _ => false,
        }
    }

    /// Stops the verdict in flight, whichever it is: cloud AI turned off, a policy
    /// narrowed, the app is quitting. Returns how many it stopped.
    pub(crate) fn stop_all(&self) -> usize {
        let mut state = self.0.lock_ignore_poison();
        state.stops += 1;
        match &state.in_flight {
            Some(held) if !held.token.is_cancelled() => {
                held.token.cancel();
                1
            }
            _ => 0,
        }
    }

    /// A finished verdict for `key`, if one is kept.
    pub(crate) fn recall(&self, key: &CacheKey) -> Option<Finished> {
        let state = self.0.lock_ignore_poison();
        state
            .cache
            .iter()
            .find(|(kept, _)| kept == key)
            .map(|(_, finished)| finished.clone())
    }

    /// Keeps `finished` for a reopen, dropping the oldest past [`CACHE_ENTRIES`].
    pub(crate) fn remember(&self, key: CacheKey, finished: Finished) {
        let mut state = self.0.lock_ignore_poison();
        state.cache.retain(|(kept, _)| *kept != key);
        state.cache.push_back((key, finished));
        while state.cache.len() > CACHE_ENTRIES {
            state.cache.pop_front();
        }
    }
}

/// The app's limits.
pub(crate) static LIMITS: LazyLock<Limits> = LazyLock::new(Limits::default);

#[cfg(test)]
mod tests {
    use super::*;

    fn key(path: &str) -> CacheKey {
        CacheKey {
            volume_id: "root".to_string(),
            path: path.to_string(),
            node: None,
            modified: Some(1),
            size: Some(2),
            provider: ProviderTag::Anthropic,
            model: "m".to_string(),
        }
    }

    fn finished(text: &str) -> Finished {
        Finished {
            contents: VerdictContents::None,
            text: text.to_string(),
        }
    }

    /// The 13th start inside a minute is refused; a minute after the first, one more fits.
    #[test]
    fn the_thirteenth_start_in_a_minute_is_refused() {
        let limits = Limits::default();
        let t0 = Instant::now();
        for i in 0..MAX_STARTS {
            limits
                .admit(t0 + Duration::from_millis(i as u64))
                .expect("inside the ceiling");
        }
        assert_eq!(limits.admit(t0 + Duration::from_secs(30)), Err(TooMany));
        assert_eq!(limits.admit(t0 + WINDOW), Ok(()), "the first start aged out");
    }

    /// A second claim stops the first; the first's release leaves the second in place.
    #[test]
    fn one_verdict_in_flight() {
        let limits = Limits::default();
        let first = CancellationToken::new();
        let second = CancellationToken::new();
        let first_claim = limits.claim("a", first.clone(), limits.stops_seen());
        let second_claim = limits.claim("b", second.clone(), limits.stops_seen());
        assert!(first.is_cancelled(), "starting a verdict stops the other");
        drop(first_claim);
        assert!(!limits.stop("a"), "a is no longer the one in flight");
        assert!(limits.stop("b"));
        assert!(second.is_cancelled());
        drop(second_claim);
        assert_eq!(limits.stop_all(), 0);
    }

    /// ❗ A consent revoke that lands while the gate resolves finds no claim to stop;
    /// the claim made after it is stopped at once, so the call never goes out.
    #[test]
    fn a_stop_before_the_claim_stops_the_claim() {
        let limits = Limits::default();
        let seen = limits.stops_seen();
        assert_eq!(limits.stop_all(), 0, "nothing in flight yet");
        let late = CancellationToken::new();
        let _claim = limits.claim("a", late.clone(), seen);
        assert!(late.is_cancelled());

        let fresh = CancellationToken::new();
        let _claim = limits.claim("b", fresh.clone(), limits.stops_seen());
        assert!(!fresh.is_cancelled(), "a gate that resolved after the stop is fine");
    }

    /// A reopen with the same key reuses; a changed node, time, or model asks again; the
    /// cache stays bounded.
    #[test]
    fn a_finished_verdict_is_kept_for_the_same_node_only() {
        let limits = Limits::default();
        limits.remember(key("/a"), finished("first"));
        assert_eq!(limits.recall(&key("/a")).map(|f| f.text), Some("first".to_string()));
        let saved_over = CacheKey {
            modified: Some(9),
            ..key("/a")
        };
        assert_eq!(limits.recall(&saved_over), None);
        let other_model = CacheKey {
            model: "n".to_string(),
            ..key("/a")
        };
        assert_eq!(limits.recall(&other_model), None);
        for i in 0..(CACHE_ENTRIES * 2) {
            limits.remember(key(&format!("/{i}")), finished("x"));
        }
        assert_eq!(limits.recall(&key("/a")), None, "the oldest went first");
        assert!(limits.recall(&key(&format!("/{}", CACHE_ENTRIES * 2 - 1))).is_some());
    }
}

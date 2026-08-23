//! Shared utilities for Tauri command modules.

#[cfg(test)]
mod budget_tests;

use serde::{Deserialize, Serialize};
use std::future::Future;
use tokio::time::Duration;

/// Wraps a value with a flag indicating whether the operation timed out.
/// Used by commands returning collections or Option to let the frontend
/// distinguish "genuinely empty/none" from "timed out before completing."
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimedOut<T: Serialize + specta::Type> {
    pub data: T,
    pub timed_out: bool,
}

/// The only two ways a command can fail when the work it wraps can't itself
/// refuse: the deadline passed, or the task never came back.
///
/// ❗ It is NOT a general-purpose IPC error. Reach for it only where the
/// underlying call is genuinely infallible (a store write that swallows its own
/// errors, a pure resolve); a command whose work has real refusals owes the
/// frontend its own vocabulary, the way `MutationError` and `EjectError` do.
///
/// ❌ Nothing here is prose a user reads. These commands are logged, not
/// worded, today; a surface that grows words for them renders from the variant.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DeadlineError {
    /// The work didn't finish inside the command's wait. ❗ It was NOT
    /// cancelled: the deadline bounds the frontend's wait, not the work.
    TimedOut,
    /// The task panicked, so no answer is coming.
    Unexpected {
        /// What the runtime reported, for the log.
        detail: String,
    },
}

impl std::fmt::Display for DeadlineError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut => f.write_str("timed out"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl std::error::Error for DeadlineError {}

/// Runs a blocking closure on the blocking thread pool with a timeout.
/// Returns the fallback value if the closure doesn't complete in time.
pub async fn blocking_with_timeout<T: Send + 'static>(
    timeout_duration: Duration,
    fallback: T,
    f: impl FnOnce() -> T + Send + 'static,
) -> T {
    match tokio::time::timeout(timeout_duration, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => result,
        _ => fallback, // Timeout or JoinError
    }
}

/// Like `blocking_with_timeout`, but returns `TimedOut<T>` so the caller
/// knows whether the fallback was returned due to a timeout.
pub async fn blocking_with_timeout_flag<T: Send + Serialize + specta::Type + 'static>(
    timeout_duration: Duration,
    fallback: T,
    f: impl FnOnce() -> T + Send + 'static,
) -> TimedOut<T> {
    match tokio::time::timeout(timeout_duration, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => TimedOut {
            data: result,
            timed_out: false,
        },
        _ => TimedOut {
            data: fallback,
            timed_out: true,
        },
    }
}

/// Runs a blocking closure that can refuse, under a deadline, in the command's
/// OWN typed error vocabulary.
///
/// The closure's error crosses the wire unchanged, and `on_timeout` mints the same
/// type for the deadline case, so the frontend keeps one exhaustive thing to match on
/// rather than a typed error plus a stringly-typed timeout beside it.
pub async fn blocking_typed_result_with_timeout<T, E>(
    timeout_duration: Duration,
    on_timeout: impl FnOnce() -> E,
    on_join_failure: impl FnOnce(String) -> E,
    f: impl FnOnce() -> Result<T, E> + Send + 'static,
) -> Result<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    match tokio::time::timeout(timeout_duration, tokio::task::spawn_blocking(f)).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(on_join_failure(join_err.to_string())),
        Err(_) => Err(on_timeout()),
    }
}

/// A cap on how many of ONE command family's blocking tasks may occupy the shared
/// blocking pool at once.
///
/// **Why any command needs one.** `spawn_blocking` draws from a pool with a hard
/// upper bound (tokio's default is 512 threads). A command the frontend can re-issue
/// faster than it completes will take every one of them, and then EVERY other
/// `spawn_blocking` in the app — directory listings, the volume list, sync status —
/// queues behind it forever. That is not a slow feature; it's a frozen app, and it has
/// happened: the image-index badge query saturated the pool during a burst of
/// watcher-driven pane refreshes, and the panes and the volume dropdown wedged until
/// restart.
///
/// A budget bounds the damage to the feature that overruns. Callers past the cap wait
/// as async futures (hundreds of bytes each), not as threads, and they keep their
/// place in line, so a bounded command degrades to "slower" instead of taking the app
/// with it.
///
/// **Sizing.** Pick the concurrency the underlying resource can actually use, not the
/// most the pool could give. Queries that serialize on one SQLite connection or one
/// global mutex gain nothing past a handful and lose throughput to contention.
///
/// Declare one per family as a `static`, and share it across the commands that
/// contend for the same resource so the cap covers their SUM:
///
/// ```ignore
/// static BADGE_QUERIES: BlockingBudget = BlockingBudget::new(4);
///
/// BADGE_QUERIES.run(move || classify(&paths)).await
/// ```
pub struct BlockingBudget {
    permits: tokio::sync::Semaphore,
}

impl BlockingBudget {
    /// A budget allowing `permits` concurrent blocking tasks.
    pub const fn new(permits: usize) -> Self {
        Self {
            permits: tokio::sync::Semaphore::const_new(permits),
        }
    }

    /// Run `f` on the blocking pool once a permit is free, releasing it when `f`
    /// returns. `Err` only when the blocking task panicked, matching `spawn_blocking`.
    ///
    /// The permit is taken BEFORE the task is spawned, which is the whole point: a
    /// task that never spawns never holds a pool thread.
    pub async fn run<T: Send + 'static>(
        &'static self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, tokio::task::JoinError> {
        // The semaphore is never closed (it lives for the process), so acquiring
        // cannot fail; treating a closed one as "no budget left" would deadlock the
        // command rather than degrade it, so the error maps to running unbounded.
        let permit = self.permits.acquire().await.ok();
        let result = tokio::task::spawn_blocking(f).await;
        drop(permit);
        result
    }
}

/// One wall-clock budget shared by a command's several legs.
///
/// **Why a command with more than one leg needs one.** Give each leg its own
/// `timeout_detached(30 s)` and the command's promise becomes "30 s times however
/// many legs it happens to run today" — a number nobody can state, that grows
/// silently when a leg is added, and that a frontend can't size a spinner
/// against. A deadline turns it back into one number the user can be told: this
/// answers, or says it couldn't, within `total`.
///
/// Hand each leg [`Deadline::remaining`]; a leg that starts with nothing left
/// doesn't start at all ([`timeout_detached_within`]).
pub struct Deadline {
    started: tokio::time::Instant,
    total: Duration,
}

impl Deadline {
    pub fn new(total: Duration) -> Self {
        Self {
            started: tokio::time::Instant::now(),
            total,
        }
    }

    /// How long the command has run so far.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// What's left of the budget: `ZERO` once it's spent, never negative.
    pub fn remaining(&self) -> Duration {
        self.total.saturating_sub(self.elapsed())
    }
}

/// [`timeout_detached_typed`] against what's LEFT of `deadline`.
///
/// A spent deadline mints `on_timeout` without spawning: the work would only be
/// abandoned a moment later, and the caller has already been kept as long as it
/// agreed to wait.
pub async fn timeout_detached_within<T, E>(
    deadline: &Deadline,
    on_timeout: impl FnOnce() -> E,
    on_join_failure: impl FnOnce(String) -> E,
    fut: impl Future<Output = Result<T, E>> + Send + 'static,
) -> Result<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    let remaining = deadline.remaining();
    if remaining.is_zero() {
        return Err(on_timeout());
    }
    timeout_detached_typed(remaining, on_timeout, on_join_failure, fut).await
}

/// Bounds how long the FRONTEND waits, never the work itself.
///
/// `fut` runs in its own task and the timeout races that task's join handle. On
/// expiry the handle is dropped, which DETACHES the task: it keeps running to
/// its own end. The caller gets `on_timeout()` promptly, the work finishes
/// safely behind it.
///
/// The future's error crosses the wire unchanged and `on_timeout` mints the same
/// type for the deadline, so the frontend keeps ONE exhaustive union to match on
/// rather than a typed error plus a stringly-typed timeout beside it.
///
/// ❌ Use this, not a bare `tokio::time::timeout(d, fut)`, for anything that can
/// reach a device backend. A bare timeout DROPS the future wherever it happens
/// to be, and on MTP that abandons an in-flight PTP transaction and wedges the
/// user's phone (`mtp/connection/CLAUDE.md`). An IPC deadline is a promise about
/// the reply, not permission to abandon a half-written transaction.
pub async fn timeout_detached_typed<T, E>(
    timeout_duration: Duration,
    on_timeout: impl FnOnce() -> E,
    on_join_failure: impl FnOnce(String) -> E,
    fut: impl Future<Output = Result<T, E>> + Send + 'static,
) -> Result<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
{
    match tokio::time::timeout(timeout_duration, tokio::spawn(fut)).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(on_join_failure(join_err.to_string())),
        Err(_) => Err(on_timeout()),
    }
}

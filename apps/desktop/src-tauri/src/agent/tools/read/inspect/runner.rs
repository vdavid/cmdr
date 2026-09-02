//! Running many paths under one call: a blocking thread per path, bounded concurrency, a
//! per-path deadline that first asks nicely and then abandons, and a call deadline past
//! which nothing new starts.
//!
//! ## What "unreachable" means here
//!
//! A thread stuck in a kernel call (a `read` on a dead NFS or SMB mount) cannot be
//! cancelled. When a path runs out its budget the tool ABANDONS it: the `spawn_blocking`
//! task keeps running until the syscall returns, holding a blocking-pool thread, and the
//! row reports `unreachable` anyway. Same posture as `commands/file_viewer.rs`'s
//! `blocking_viewer_op`. Two hundred paths on a dead mount can park up to `concurrency`
//! threads for as long as the mount is dead.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream;
use tokio::task::JoinError;
use tokio::time::{Instant, timeout_at};

use super::{FileRow, UnreadableReason};

/// The timeout policy, as values so the tests can shrink them.
#[derive(Debug, Clone, Copy)]
pub(super) struct RunnerConfig {
    /// A path's whole budget, cooperative window plus grace.
    pub path_timeout: Duration,
    /// How long after flipping the cancel flag a path may still hand back partial,
    /// flagged results before it is abandoned. Must be shorter than `path_timeout`.
    pub cancel_grace: Duration,
    /// After this no new path is launched; in-flight ones get their grace.
    pub call_timeout: Duration,
    /// Paths in flight at once.
    pub concurrency: usize,
}

/// The per-path blocking work, injectable so the timeout policy is testable without a
/// hung mount. Gets the path and the cancel flag the deadline flips.
pub(super) type InspectFn = Arc<dyn Fn(&str, &AtomicBool) -> FileRow + Send + Sync>;

/// Inspect every path, `cfg.concurrency` at a time, and return one slot per path in
/// request order. `None` is a path that was never launched (the call deadline had
/// passed); a launched path always gets a row, `Unreachable` when abandoned.
pub(super) async fn run_paths(paths: &[String], cfg: &RunnerConfig, inspect: InspectFn) -> Vec<Option<FileRow>> {
    let call_deadline = Instant::now() + cfg.call_timeout;
    let results: Vec<(usize, Option<FileRow>)> = stream::iter(paths.iter().cloned().enumerate())
        .map(|(index, path)| {
            let inspect = inspect.clone();
            async move { (index, run_one(path, call_deadline, cfg, inspect).await) }
        })
        .buffer_unordered(cfg.concurrency.max(1))
        .collect()
        .await;
    let mut rows: Vec<Option<FileRow>> = (0..paths.len()).map(|_| None).collect();
    for (index, row) in results {
        rows[index] = row;
    }
    rows
}

/// One path, two phases: wait the cooperative window (bounded by the call deadline),
/// then flip `cancel` and wait the grace for a partial answer, then abandon.
async fn run_one(path: String, call_deadline: Instant, cfg: &RunnerConfig, inspect: InspectFn) -> Option<FileRow> {
    let started = Instant::now();
    if started >= call_deadline {
        return None;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let job_cancel = cancel.clone();
    let job_path = path.clone();
    let mut job = tokio::task::spawn_blocking(move || inspect(&job_path, &job_cancel));

    let cooperative_until = (started + cfg.path_timeout.saturating_sub(cfg.cancel_grace)).min(call_deadline);
    if let Ok(done) = timeout_at(cooperative_until, &mut job).await {
        return Some(finish(path, done));
    }

    // A slow-but-alive read stops on the flag with partial, flagged results (an
    // approximate line window). A wedged one never sees it.
    cancel.store(true, Ordering::Relaxed);
    match timeout_at(Instant::now() + cfg.cancel_grace, &mut job).await {
        Ok(done) => Some(finish(path, done)),
        // Dropping `job` detaches the task: abandoned, not stopped (module docs).
        Err(_) => Some(FileRow::Unreachable { path }),
    }
}

/// A finished job's row. A panic inside one path's read is that path's problem, not the
/// call's: the panic hook has already reported it, and the other rows still answer.
fn finish(path: String, done: Result<FileRow, JoinError>) -> FileRow {
    match done {
        Ok(row) => row,
        Err(e) => {
            log::warn!(target: "agent::tools::inspect", "inspect_file: reading one path panicked: {e}");
            FileRow::Unreadable {
                path,
                reason: UnreadableReason::Io,
            }
        }
    }
}

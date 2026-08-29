//! What the rollback engine needs from the operation it runs inside: hands that
//! perform a decided act, two answers that stop or park the loop, and a voice
//! that says where it stands.
//!
//! The engine here is a **planner**: it pages the journal, verifies each item
//! against its snapshot, decides the inverse act, and keeps the journal's books.
//! PERFORMING an act needs the cross-volume primitives (staged writes, mid-file
//! cancel, retry, stall detection), and those live in `write_operations`, which
//! `operation_log` must never import. So the whole executor arrives injected —
//! the same move the `spawn` hook in [`rollback_operation`](super::rollback_operation)
//! already makes, for the same reason.
//!
//! The implementation lives in `write_operations::rollback`.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use super::super::store::RollbackUnit;
use super::removal_target;
use crate::file_system::volume::{Volume, VolumeError};

/// One decided inverse act. The planner has already resolved both volumes,
/// verified the item against its recorded snapshot, and established that the
/// target is clear; the executor only acts, and reports the raw
/// [`VolumeError`] back so the planner keeps owning the typed skip reasons.
pub enum InverseAct<'a> {
    /// Delete the file the operation created at `path` — verified unchanged
    /// since it was written.
    RemoveFile {
        volume: &'a Arc<dyn Volume>,
        path: &'a Path,
    },
    /// Delete the directory the operation created at `path` — verified still
    /// empty, so this never needs to recurse.
    RemoveDir {
        volume: &'a Arc<dyn Volume>,
        path: &'a Path,
    },
    /// Move one entry back where it started (move / trash / rename undo). The
    /// target was verified clear, so this never overwrites.
    Restore {
        /// Where the item sits now.
        from: &'a Arc<dyn Volume>,
        from_path: &'a Path,
        /// Where it belongs.
        to: &'a Arc<dyn Volume>,
        to_path: &'a Path,
        /// One volume ⇒ a rename; two ⇒ a staged per-file transfer, which is what
        /// buys mid-file cancel and byte progress.
        same_volume: bool,
        /// Land the rename even though the target "exists": it IS this entry,
        /// folded onto one name by a case-insensitive volume. Only ever set for a
        /// same-volume restore (a cross-volume target can never be self).
        force: bool,
    },
}

/// Where a reversal stands, reported once per item so the bar moves without a
/// scanning phase: the totals come off the journal before the first act.
pub struct RollbackProgress<'a> {
    /// Items the reversal has finished considering — reversed AND skipped, since
    /// both are done being decided.
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The name of the item about to be reversed, for the queue row's readout.
    pub current_name: Option<&'a str>,
}

/// The engine's executor, injected by whoever runs the reversal.
///
/// Every method is called from the planner's item loop, so an implementation is
/// shared across `.await` points: `Sync` rather than `&mut self`.
pub trait RollbackRunner: Sync {
    /// Perform one decided act.
    fn perform<'a>(&'a self, act: InverseAct<'a>)
    -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>>;

    /// Should the reversal stop where it is?
    ///
    /// **Not the same question as "is this operation cancelled?"**, and the
    /// difference is a data-safety one. `write_operations::state::is_cancelled`
    /// answers "intent isn't `Running`", and `RollingBack` is one of the values it
    /// calls cancelled — right for a forward transfer that must stop and undo
    /// itself, wrong for a reversal running UNDER an operation whose intent is
    /// already `RollingBack`, where that value is the instruction to reverse
    /// rather than an order to stop. An implementation that conflated the two
    /// would bail on its first item and report a clean stop having reversed
    /// nothing. So the reversal's owner names which reading it means
    /// (`write_operations::rollback::StopMeans`) instead of inheriting the
    /// transfer's.
    fn should_stop(&self) -> bool;

    /// Park while the reversal is paused, returning the moment it resumes OR
    /// [`should_stop`](Self::should_stop) turns true — a stop always wins over a
    /// pause. Called at the item boundary, BEFORE the item's snapshot is
    /// verified: parking between "verified unchanged" and "delete" would let a
    /// ten-minute-stale verification authorize a destructive act.
    fn wait_while_paused(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    /// Report where the reversal stands. Called once per item; throttling to the
    /// operation's progress interval is the implementation's business, as is
    /// making sure the frame that reaches the total goes out.
    fn report_progress(&self, progress: RollbackProgress<'_>);
}

/// Where the reversal stands, and the one place a progress frame is built from
/// it. The totals arrive from the journal before the first act, so the bar is
/// honest from the first frame and there's nothing to scan.
///
/// An item counts as done once it has been DECIDED — reversed or skipped — since
/// both are equally over, and a bar that stalled on skips would misreport a run
/// that legitimately left files alone.
pub(super) struct ProgressStand {
    files_done: u64,
    files_total: u64,
    bytes_done: u64,
    bytes_total: u64,
}

impl ProgressStand {
    pub(super) fn over((files_total, bytes_total): (u64, u64)) -> Self {
        ProgressStand {
            files_done: 0,
            files_total,
            bytes_done: 0,
            bytes_total,
        }
    }

    /// Tell the runner where things stand. `next` is the item about to be
    /// reversed (`None` for the closing frame, which reports the position the run
    /// finished at).
    pub(super) fn announce(&self, runner: &dyn RollbackRunner, next: Option<&RollbackUnit>) {
        let acting_on = next.map(|unit| removal_target(unit).1);
        let current_name = acting_on
            .as_deref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy());
        runner.report_progress(RollbackProgress {
            files_done: self.files_done,
            files_total: self.files_total,
            bytes_done: self.bytes_done,
            bytes_total: self.bytes_total,
            current_name: current_name.as_deref(),
        });
    }

    pub(super) fn credit(&mut self, unit: &RollbackUnit) {
        self.files_done += 1;
        self.bytes_done += unit.size.unwrap_or(0).max(0) as u64;
    }
}

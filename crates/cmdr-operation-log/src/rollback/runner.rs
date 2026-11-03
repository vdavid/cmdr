//! What the rollback engine needs from whoever runs it: the volumes an operation
//! touched, hands that perform a decided act, answers that pace, stop, or park
//! the loop, and a voice that says where it stands.
//!
//! The engine here is a **planner**: it pages the journal, verifies each item
//! against its snapshot, decides the inverse act, and keeps the journal's books.
//! PERFORMING an act needs the cross-volume primitives (staged writes, mid-file
//! cancel, retry, stall detection), and those live in `cmdr-operations`, which
//! `operation_log` must never import. So the whole executor arrives injected —
//! the same move the `spawn` hook in [`rollback_operation`](super::rollback_operation)
//! already makes, for the same reason.
//!
//! The implementation lives in `cmdr_operations::rollback`.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::super::store::RollbackUnit;
use super::removal_target;
use cmdr_fs::volume::{Volume, VolumeError};

/// Which volume serves an id, asked when the gate checks that every volume an
/// operation touched is connected, and once per item for the volumes it acts on.
pub trait VolumeLookup: Send + Sync {
    /// The volume registered under `volume_id`, or `None` when it isn't connected.
    fn get(&self, volume_id: &str) -> Option<Arc<dyn Volume>>;
}

/// One decided inverse act. The planner has already resolved both volumes,
/// verified the item against its recorded snapshot, and established that the
/// target is clear; the executor only acts, and reports the raw
/// [`VolumeError`] back (inside an [`ActRefusal`]) so the planner keeps owning
/// the typed skip reasons.
pub enum InverseAct<'a> {
    /// Delete the file the operation created at `path` — verified unchanged
    /// since it was written.
    RemoveFile {
        /// The volume the file is on.
        volume: &'a Arc<dyn Volume>,
        /// The file.
        path: &'a Path,
    },
    /// Delete the directory the operation created at `path` — verified still
    /// empty, so this never needs to recurse.
    RemoveDir {
        /// The volume the directory is on.
        volume: &'a Arc<dyn Volume>,
        /// The directory.
        path: &'a Path,
    },
    /// Move one entry back where it started (move / trash / rename undo). The
    /// target was verified clear, so this never overwrites.
    Restore {
        /// Where the item sits now.
        from: &'a Arc<dyn Volume>,
        /// Its path there.
        from_path: &'a Path,
        /// Where it belongs.
        to: &'a Arc<dyn Volume>,
        /// Its path there.
        to_path: &'a Path,
        /// One volume ⇒ a rename; two ⇒ a staged per-file transfer, which is what
        /// buys mid-file cancel and byte progress.
        same_volume: bool,
        /// Land the rename even though the target "exists": it IS this entry,
        /// folded onto one name by a case-insensitive volume. Only ever set for a
        /// same-volume restore (a cross-volume target can never be self).
        force: bool,
    },
    /// Replace extended attribute `name` on `path` with `value` (a tag write's undo,
    /// verified to still carry what the write left). Never creates the file.
    WriteXattr {
        /// The id `volume` is registered under, so the panes showing the file can
        /// read its tags again.
        volume_id: &'a str,
        /// The volume the file is on.
        volume: &'a Arc<dyn Volume>,
        /// The file.
        path: &'a Path,
        /// The attribute.
        name: &'a str,
        /// The value to put back.
        value: &'a [u8],
    },
}

/// Why a decided act didn't happen.
#[derive(Debug)]
pub enum ActRefusal {
    /// The volume refused it; the planner turns the error into a typed skip.
    Volume(VolumeError),
    /// The item is the operation's own, verified, but it wears a lock this
    /// process can't clear (`schg`), so it can't be removed.
    Locked,
}

impl From<VolumeError> for ActRefusal {
    fn from(e: VolumeError) -> Self {
        ActRefusal::Volume(e)
    }
}

/// Where a reversal stands, reported once per item so the bar moves without a
/// scanning phase: the totals come off the journal before the first act.
pub struct RollbackProgress<'a> {
    /// Items the reversal has finished considering — reversed AND skipped, since
    /// both are done being decided.
    pub files_done: u64,
    /// Items the reversal will consider, from the journal.
    pub files_total: u64,
    /// Bytes of the items finished so far.
    pub bytes_done: u64,
    /// Bytes of every item it will consider, from the journal.
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
    fn perform<'a>(&'a self, act: InverseAct<'a>) -> Pin<Box<dyn Future<Output = Result<(), ActRefusal>> + Send + 'a>>;

    /// Should the reversal stop where it is?
    ///
    /// **Not the same question as "is this operation cancelled?"**, and the
    /// difference is a data-safety one. the engine's cancelled reading (`IntentCell`)
    /// answers "intent isn't `Running`", and `RollingBack` is one of the values it
    /// calls cancelled — right for a forward transfer that must stop and undo
    /// itself, wrong for a reversal running UNDER an operation whose intent is
    /// already `RollingBack`, where that value is the instruction to reverse
    /// rather than an order to stop. An implementation that conflated the two
    /// would bail on its first item and report a clean stop having reversed
    /// nothing. So the reversal's owner names which reading it means
    /// (`StopMeans`, in `cmdr-operations`' `operation_intent.rs`) instead of inheriting the
    /// transfer's.
    fn should_stop(&self) -> bool;

    /// Park while the reversal is paused, returning the moment it resumes OR
    /// [`should_stop`](Self::should_stop) turns true — a stop always wins over a
    /// pause. Called at the item boundary, BEFORE the item's snapshot is
    /// verified: parking between "verified unchanged" and "delete" would let a
    /// ten-minute-stale verification authorize a destructive act.
    fn wait_while_paused(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;

    /// How long to wait before each file item, for an E2E spec that has to act
    /// inside a running reversal. `None`, the default, waits not at all.
    fn pace_between_items(&self) -> Option<Duration> {
        None
    }

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

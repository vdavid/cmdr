//! The creation snapshot for sources the agent named one by one.
//!
//! A selector's ops carry the index row they matched. A hand-listed path has nothing behind it,
//! so without this every row of the agent's usual kind of group read "not in the index", even
//! on a freshly indexed drive. So each named source gets ONE bounded stat through its group's
//! source volume, before anything is written: a path that answers freezes its size, date, and
//! inode, and one that doesn't (missing, refused, too slow) stays `NULL`. Never a zero.
//!
//! The ONE path for every producer that names sources by hand: `propose_suggestions` (paths and
//! renames) and `propose_rename_plan` both freeze through [`freeze_paths`]. A call shares ONE
//! deadline, so a sweep onto a dead mount costs the agent one budget, not one per file; the
//! stats themselves are capped in `suggested_ops::facts`.

use std::sync::Arc;

use cmdr_fs::deadline::{Deadline, SESSION_IO_TIMEOUT};

use super::input::{PlannedOps, PlannedSources, PlannedSweep};
use crate::agent::store::proposals::OpSnapshot;
use crate::agent::suggested_ops::facts::stat_sources;
use crate::file_system::volume::Volume;

/// One deadline for a whole proposal call's snapshot.
pub(crate) fn snapshot_deadline() -> Deadline {
    Deadline::new(SESSION_IO_TIMEOUT)
}

/// The creation snapshot of each of `paths` on `volume`, in order. A volume nobody registered
/// (`None`) answers `None` for every path, which the review words as "no record".
pub(crate) async fn freeze_paths(
    volume: Option<Arc<dyn Volume>>,
    paths: Vec<String>,
    deadline: &Deadline,
) -> Vec<Option<OpSnapshot>> {
    match volume {
        Some(volume) => stat_sources(volume, paths, deadline)
            .await
            .into_iter()
            .map(|stat| stat.snapshot())
            .collect(),
        None => vec![None; paths.len()],
    }
}

/// Fill the creation snapshot of every hand-listed source in `planned`, asking `volume_for`
/// for each group's source volume.
pub(super) async fn freeze_named_sources(
    planned: &mut PlannedSweep,
    volume_for: impl Fn(&str) -> Option<Arc<dyn Volume>>,
) {
    let deadline = snapshot_deadline();
    for group in &mut planned.groups {
        match &mut group.ops {
            PlannedOps::Sources {
                sources: PlannedSources::Paths { ops, naming },
                ..
            } => {
                let paths = ops.iter().map(|op| op.source_path.clone()).collect();
                let snapshots = freeze_paths(volume_for(&naming.source_volume_id), paths, &deadline).await;
                for (op, snapshot) in ops.iter_mut().zip(snapshots) {
                    op.snapshot = snapshot;
                }
            }
            PlannedOps::Named { ops, naming, .. } => {
                let paths = ops.iter().map(|op| op.source_path.clone()).collect();
                let snapshots = freeze_paths(volume_for(&naming.source_volume_id), paths, &deadline).await;
                for (op, snapshot) in ops.iter_mut().zip(snapshots) {
                    op.snapshot = snapshot;
                }
            }
            PlannedOps::Rename { renames, naming, .. } => {
                let paths = renames.iter().map(|op| op.source_path.clone()).collect();
                let snapshots = freeze_paths(volume_for(&naming.source_volume_id), paths, &deadline).await;
                for (op, snapshot) in renames.iter_mut().zip(snapshots) {
                    op.snapshot = snapshot;
                }
            }
            // A selector's ops carry the index row they matched; resolution fills them.
            PlannedOps::Sources {
                sources: PlannedSources::Selector(_),
                ..
            } => {}
        }
    }
}

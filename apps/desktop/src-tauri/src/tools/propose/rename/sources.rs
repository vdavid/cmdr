//! The one live check a plan gets before it's staged: is each source there, and is it a file?
//!
//! Every source is stat'ed ONCE through the plan's volume, local disk and remote alike, under
//! one shared deadline (`agent::suggested_ops::facts::stat_sources`: bounded, detached, capped
//! in flight). That one answer does two jobs:
//!
//! - **Existence**: a missing file stays in the plan and preflight shows it as "(doesn't
//!   exist)", so one invented name costs that row and not the plan. A folder refuses its row. A
//!   source the volume didn't answer for in time refuses its row as `unreachable`, which is the
//!   dead-mount guarantee: a hung share costs the turn the deadline, then says so.
//! - **The creation snapshot**: a present file's size, date, and inode freeze into the row, so
//!   the review shows what the file was when it was suggested.

use std::sync::Arc;

use cmdr_fs::deadline::Deadline;

use super::rows::{RowProblem, RowRejection};
use super::store::RenameDraftRow;
use crate::agent::suggested_ops::facts::{SourceStat, stat_sources};
use crate::file_system::volume::Volume;

/// Stat every row's source on `volume` within `deadline`, freezing each present file's snapshot
/// in place, or refuse with every row that can't be staged.
///
/// A plan where EVERY source is missing refuses too, each row as `missing`: nothing the user
/// could approve would rename anything, so the plan names the wrong folder or invented names.
pub(super) async fn check_sources(
    volume: Arc<dyn Volume>,
    rows: &mut [RenameDraftRow],
    deadline: &Deadline,
) -> Result<(), Vec<RowRejection>> {
    let paths = rows.iter().map(|row| row.source_path.clone()).collect();
    let stats = stat_sources(volume, paths, deadline).await;
    let mut rejections = Vec::new();
    let mut missing = 0;
    for (row, stat) in rows.iter_mut().zip(stats) {
        match stat {
            SourceStat::Present { is_directory: true, .. } => {
                rejections.push(RowRejection::new(&row.source_path, RowProblem::Folder));
            }
            SourceStat::Unknown => rejections.push(RowRejection::new(&row.source_path, RowProblem::Unreachable)),
            SourceStat::Missing => missing += 1,
            SourceStat::Present { .. } => row.snapshot = stat.snapshot(),
        }
    }
    if rejections.is_empty() && missing == rows.len() {
        rejections = rows
            .iter()
            .map(|row| RowRejection::new(&row.source_path, RowProblem::Missing))
            .collect();
    }
    if rejections.is_empty() { Ok(()) } else { Err(rejections) }
}

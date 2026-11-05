//! The one live check a plan gets before it's staged: is each source there, and is it a file?
//!
//! Every source is stat'ed ONCE through the plan's volume, local disk and remote alike, under
//! one shared deadline (`agent::suggested_ops::facts::stat_sources`: bounded, detached, capped
//! in flight PER VOLUME, so a dead share's stragglers never starve a plan on another volume).
//! That one answer does two jobs:
//!
//! - **Existence**: a missing file stays in the plan and preflight shows it as "(doesn't
//!   exist)", so one invented name costs that row and not the plan. A folder refuses its row. A
//!   source the volume didn't answer for in time refuses its row as `unreachable`, which is the
//!   dead-mount guarantee: a hung share costs the turn the deadline, then says so.
//! - **The creation snapshot**: a present file's size, date, and inode freeze into the row, so
//!   the review shows what the file was when it was suggested.

use std::sync::Arc;

use cmdr_fs::deadline::Deadline;

use super::rows::{RowProblem, RowRejection, looks_like_a_snapshot_view};
use super::store::RenameDraftRow;
use crate::agent::suggested_ops::facts::{SourceStat, stat_sources};
use crate::file_system::volume::Volume;

/// Stat every row's source on `volume` within `deadline`, freezing each present file's snapshot
/// in place, and answer every row that can't be staged (empty: all of them can).
///
/// When no row is a file the volume vouched for (each is missing, or a folder, and none went
/// unanswered), the missing ones refuse too: nothing the user could approve would rename
/// anything, so the plan names the wrong folder or invented names. Reported in the same answer
/// as the folders, so the model doesn't learn it one retry later.
pub(super) async fn check_sources(
    volume_id: &str,
    volume: Arc<dyn Volume>,
    rows: &mut [RenameDraftRow],
    deadline: &Deadline,
) -> Vec<RowRejection> {
    let paths = rows.iter().map(|row| row.source_path.clone()).collect();
    let stats = stat_sources(volume_id, volume, paths, deadline).await;
    let mut rejections = Vec::new();
    let mut missing = Vec::new();
    let mut files = 0;
    let mut unanswered = 0;
    for (row, stat) in rows.iter_mut().zip(stats) {
        match stat {
            SourceStat::Present { is_directory: true, .. } => {
                rejections.push(RowRejection::new(&row.source_path, RowProblem::Folder));
            }
            SourceStat::Unknown => {
                unanswered += 1;
                rejections.push(RowRejection::new(&row.source_path, RowProblem::Unreachable));
            }
            SourceStat::Missing => missing.push(row.source_path.clone()),
            SourceStat::Present { .. } => {
                files += 1;
                row.snapshot = stat.snapshot();
            }
        }
    }
    if files == 0 && unanswered == 0 {
        rejections.extend(missing.into_iter().map(|path| {
            let problem = if looks_like_a_snapshot_view(&path) {
                RowProblem::MissingUnderSnapshots
            } else {
                RowProblem::Missing
            };
            RowRejection::new(&path, problem)
        }));
    }
    rejections
}

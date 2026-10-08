//! A batch of renames on a volume where some rename copies (an object store's
//! folders and big files, `Volume::rename_work`): the whole batch runs as ONE
//! background move with the new names (David's decision), so it gets the
//! transfer engine's progress, pause, cancel, and journaling.
//!
//! - **Order**: the executor's own dependency order (`plan.rs`), so a chain
//!   `a → b, b → c` moves `b` out of the way first.
//! - **Conflicts skip**: a name something outside the batch holds keeps its
//!   owner, the executor's answer too.
//! - ❗ **A cycle stays where it is**: `a ↔ b` would need a temporary name, and
//!   a move onto a FOLDER that's still there merges into it. Its rows are left
//!   out, ❌ never moved onto each other, and counted in
//!   [`RenamesStarted::swaps_left_out`] so the batch's result line says so.
//! - **The sources are bound** to the fingerprints preflight captured, the way
//!   every other approved operation is (`source_binding.rs`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::plan::{RenamePlanStep, build_execution_plan, spelled_destinations};
use super::{BulkRenameRow, RenameStartError, start_bulk_rename};
use crate::file_system::volume::{RenameWork, Volume};
use crate::file_system::write_operations::event_sinks::OperationEventSink;
use crate::file_system::write_operations::routing::start_rename_by_move;
use crate::file_system::write_operations::source_binding::ExpectedSources;
use crate::file_system::write_operations::transfer::volume::unregistered_source_error;
use crate::file_system::write_operations::types::{ConflictResolution, VolumeCopyConfig, WriteOperationStartResult};
use crate::operation_log::types::Initiator;

/// How many entries a batch asks `Volume::rename_work` about at once.
const RENAME_WORK_CONCURRENCY: usize = 8;

/// A started batch, and what it left out.
#[derive(Debug, Clone)]
pub(crate) struct RenamesStarted {
    pub operation: WriteOperationStartResult,
    /// Renames that swap names with each other (`a ↔ b`, or a longer cycle),
    /// kept out of a batch that runs as a move. Always `0` from the batch
    /// executor, which swaps through a temporary name.
    pub swaps_left_out: usize,
}

/// Starts a reviewed batch of renames on one volume, routed by what a rename
/// costs there. Where every row renames in one call, the batch executor
/// ([`start_bulk_rename`]) runs it; where any row's rename copies (an object
/// store's folder or big file, `Volume::rename_work`), the whole batch runs as
/// one background move with the new names. The production entry
/// for Ask Cmdr's approved renames and the bulk-rename command.
pub(crate) async fn start_renames(
    events: Arc<dyn OperationEventSink>,
    volume_id: String,
    rows: Vec<BulkRenameRow>,
    initiator: Initiator,
) -> Result<RenamesStarted, RenameStartError> {
    if volume_id != "root" {
        let Some(volume) = crate::file_system::volume::manager::get_volume_manager().get(&volume_id) else {
            // Asked before anything runs, so a phone or server nobody connected is worded the way
            // a clicked copy off it would be, ❌ never as a volume that vanished.
            let first = rows
                .first()
                .map(|row| row.source.display().to_string())
                .unwrap_or_default();
            return Err(RenameStartError::Engine {
                error: unregistered_source_error(&volume_id, &first).await,
            });
        };
        if any_rename_copies(volume.as_ref(), &rows).await {
            return start_batch_as_move(events, volume_id, volume.as_ref(), rows, initiator).await;
        }
    }
    start_bulk_rename(events, volume_id, rows, initiator).map(|operation| RenamesStarted {
        operation,
        swaps_left_out: 0,
    })
}

/// Whether any row's rename copies on `volume`. A volume that renames
/// everything in one call answers with no I/O; an entry that can't be asked
/// about is left to the executor, which refuses it on its own terms.
async fn any_rename_copies(volume: &dyn Volume, rows: &[BulkRenameRow]) -> bool {
    for chunk in rows.chunks(RENAME_WORK_CONCURRENCY) {
        let answers = futures_util::future::join_all(chunk.iter().map(|row| volume.rename_work(&row.source))).await;
        if answers
            .iter()
            .any(|work| matches!(work, Ok(RenameWork::CopyThenDelete)))
        {
            return true;
        }
    }
    false
}

/// The batch as one move with new names, in the executor's dependency order,
/// conflicts skipped, its sources bound to what preflight saw.
async fn start_batch_as_move(
    events: Arc<dyn OperationEventSink>,
    volume_id: String,
    volume: &dyn Volume,
    rows: Vec<BulkRenameRow>,
    initiator: Initiator,
) -> Result<RenamesStarted, RenameStartError> {
    if rows.iter().any(|row| row.source.parent() != row.destination.parent()) {
        return Err(RenameStartError::NotInOneFolder);
    }
    let rows = spelled_destinations(volume, rows);
    let Some(parent) = rows.first().and_then(|row| row.source.parent()).map(Path::to_path_buf) else {
        return Err(RenameStartError::NothingToRename);
    };
    let (renames, left_out) = move_order(&rows);
    let swaps_left_out = left_out.len();
    for index in left_out {
        log::warn!(
            target: "volume",
            "bulk rename on '{volume_id}': {} is part of a swap, which a move can't do without merging; it keeps its name",
            rows[index].source.display()
        );
    }
    let expected = ExpectedSources::new(
        rows.iter()
            .map(|row| (row.source.clone(), row.expected_fingerprint.clone())),
    );
    let config = VolumeCopyConfig {
        // A name something outside the batch holds keeps its owner.
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };
    start_rename_by_move(
        events,
        volume_id,
        renames,
        parent.display().to_string(),
        config,
        initiator,
        Some(expected),
    )
    .await
    .map(|operation| RenamesStarted {
        operation,
        swaps_left_out,
    })
    .map_err(|error| RenameStartError::Engine { error })
}

/// What a batch moves, in order, as `(source, new name)`, and which rows a
/// cycle kept out. No-op rows (a name that doesn't change) move nowhere.
fn move_order(rows: &[BulkRenameRow]) -> (Vec<(PathBuf, String)>, Vec<usize>) {
    let active = vec![true; rows.len()];
    let mut moves = Vec::with_capacity(rows.len());
    let mut left_out = Vec::new();
    for step in build_execution_plan(rows, &active) {
        match step {
            RenamePlanStep::Direct(index) | RenamePlanStep::CaseOnly(index) => {
                let row = &rows[index];
                let Some(name) = row.destination.file_name() else {
                    left_out.push(index);
                    continue;
                };
                moves.push((row.source.clone(), name.to_string_lossy().into_owned()));
            }
            RenamePlanStep::Cycle(indices) => left_out.extend(indices),
        }
    }
    (moves, left_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::source_binding::{RemoteContent, SourceFingerprint};

    fn row(id: &str, from: &str, to: &str) -> BulkRenameRow {
        BulkRenameRow {
            row_id: id.to_string(),
            source: PathBuf::from(from),
            destination: PathBuf::from(to),
            expected_fingerprint: SourceFingerprint::Remote {
                normalized_path: from.to_string(),
                content: RemoteContent::Directory,
            },
        }
    }

    #[test]
    fn a_chain_moves_its_last_link_first() {
        let rows = vec![row("1", "/d/a", "/d/b"), row("2", "/d/b", "/d/c")];
        let (moves, left_out) = move_order(&rows);
        assert_eq!(
            moves,
            vec![
                (PathBuf::from("/d/b"), "c".to_string()),
                (PathBuf::from("/d/a"), "b".to_string()),
            ]
        );
        assert!(left_out.is_empty());
    }

    /// ❗ A swap would merge one folder into the other: it stays out.
    #[test]
    fn a_cycle_is_left_out_and_a_noop_moves_nowhere() {
        let rows = vec![
            row("1", "/d/x", "/d/y"),
            row("2", "/d/y", "/d/x"),
            row("3", "/d/same", "/d/same"),
            row("4", "/d/old", "/d/new"),
        ];
        let (moves, mut left_out) = move_order(&rows);
        assert_eq!(moves, vec![(PathBuf::from("/d/old"), "new".to_string())]);
        left_out.sort_unstable();
        assert_eq!(left_out, vec![0, 1]);
    }
}

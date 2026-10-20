//! Managed batch rename for Ask Cmdr's reviewed rename proposals.
//!
//! This module owns the server-side rows and collision-safe driver. Independent
//! rows and chains rename directly in dependency order. Each cycle and each
//! case-only rename uses one same-directory temporary name. Which rows run and in
//! what order (spelling, conflicts, look-alikes, the step plan) is `bulk/plan.rs`.

use super::super::state::IntentCell;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::super::event_sinks::OperationEventSink;
use super::super::host::EngineHost;
use super::super::manager::{self, OperationDescriptor, OperationSummaryText};
use super::super::source_binding::SourceFingerprint;
use super::super::state::{WriteOperationState, WriteSettledGuard};
use super::super::types::{
    CancelRollback, SourceItemOutcome, WriteCancelledEvent, WriteCompleteEvent, WriteOperationError,
    WriteOperationStartResult, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use cmdr_fs::volume::{LaneKey, Volume};
use cmdr_local::rename_local_exclusive;
use cmdr_operation_log::types::{ExecutionStatus, Initiator, OpKind, OperationId};

mod by_move;
mod plan;
mod recorder;
use recorder::BulkRenameRecorder;

pub use by_move::RenamesStarted;

use plan::{
    RenamePlanStep, build_execution_plan, settle_local_conflicts, settle_remote_conflicts, spelled_destinations,
};

/// One immutable row that the user allowed and preflight accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkRenameRow {
    /// The caller's id for the row, which its outcome names.
    pub row_id: String,
    /// The file to rename.
    pub source: PathBuf,
    /// Where it lands.
    pub destination: PathBuf,
    /// What the source looked like when the user reviewed it.
    pub expected_fingerprint: SourceFingerprint,
}

/// Why a reviewed batch of renames wouldn't start. Nothing was renamed.
///
/// ❌ Not prose: each surface words its own variant, and a volume refusal rides as the
/// `WriteOperationError` the transfer dialogs already word.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[allow(
    missing_docs,
    reason = "a wire type: every `///` here reaches `bindings.ts` as JSDoc, so its fields and variants stay as they ship"
)]
pub enum RenameStartError {
    /// No row is left to run: every source was dropped before the batch got here (unreadable
    /// at preflight, or turned off in the review).
    NothingToRename,
    /// A row would move its file to another folder. Unreachable through a rename group, which
    /// binds one shared parent, and refused rather than run as a move.
    NotInOneFolder,
    /// The engine refused before anything ran: the volume isn't connected, or the batch's
    /// move couldn't start.
    Engine { error: WriteOperationError },
}

impl std::fmt::Display for RenameStartError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NothingToRename => f.write_str("no rename left to run"),
            Self::NotInOneFolder => f.write_str("a row would change folders"),
            Self::Engine { error } => write!(f, "the engine refused: {error:?}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BulkRenameOutcome {
    Done,
    Skipped,
    Failed,
}

impl BulkRenameOutcome {
    #[cfg(test)]
    fn is_done(self) -> bool {
        self == Self::Done
    }
}

impl crate::Operations {
    /// Starts one queued, same-volume batch rename. The caller has already resolved
    /// the proposal id and exact accepted-preflight subset; this layer receives only
    /// immutable backend rows, never frontend paths or names.
    pub(crate) fn start_bulk_rename(
        &self,
        events: Arc<dyn OperationEventSink>,
        volume_id: String,
        rows: Vec<BulkRenameRow>,
        initiator: Initiator,
    ) -> Result<WriteOperationStartResult, RenameStartError> {
        if rows.is_empty() {
            return Err(RenameStartError::NothingToRename);
        }
        if rows.iter().any(|row| row.source.parent() != row.destination.parent()) {
            return Err(RenameStartError::NotInOneFolder);
        }

        // `root` is the only backend that owns raw local paths here. Every mounted
        // volume, including a locally mounted removable drive, stays on its Volume
        // route so its listing and connection semantics remain authoritative.
        let uses_local_paths = volume_id == "root";
        let (lanes, volume_ids, settled_volume, rows) = if uses_local_paths {
            (vec![LaneKey::new("root")], Vec::new(), None, rows)
        } else {
            // `start_renames` classified an unregistered id before this; one missing NOW left the
            // registry in between (an unmount race). Plain `get`: `start_renames` refused a
            // routed row.
            let volume = self
                .host()
                .volumes()
                .get(&volume_id)
                .ok_or_else(|| RenameStartError::Engine {
                    error: WriteOperationError::SourceNoLongerConnected {
                        path: rows[0].source.display().to_string(),
                    },
                })?;
            let rows = spelled_destinations(volume.as_ref(), rows);
            (
                vec![volume.lane_key()],
                vec![volume_id.clone()],
                Some(volume.name().to_string()),
                rows,
            )
        };

        let operation_id = OperationId::from(cmdr_operation_log::new_operation_id());
        let summary = OperationSummaryText {
            source: Some(format!("{} files", rows.len())),
            destination: None,
            items: None,
        };
        let descriptor = OperationDescriptor {
            operation_id: operation_id.clone(),
            operation_type: WriteOperationType::Rename,
            lanes,
            volume_ids,
            summary,
            // A rename moves no bytes: cancelling stops before the next name, and
            // the ones already renamed stay renamed.
            rollback_support: crate::types::RollbackSupport::InstantChange,
            // No scan preview: nothing walked a tree to plan this op.
            preview_id: None,
            reverses: None,
        };
        let state = Arc::new(self.new_operation_state(Duration::from_millis(200)));
        let events_for_task = Arc::clone(&events);
        let operation_id_for_task = operation_id.clone();
        let rows_for_task = rows.clone();
        let volume_id_for_task = volume_id.clone();
        let state_for_task = Arc::clone(&state);
        let deferred = move |ops: crate::Operations| -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async move {
                let task_guard = manager::ManagedTaskGuard::new(ops.manager(), operation_id_for_task.clone());
                let _settled_guard = WriteSettledGuard::new(
                    Arc::clone(&events_for_task),
                    operation_id_for_task.to_string(),
                    WriteOperationType::Rename,
                    settled_volume,
                );
                super::super::journal::open_volume_op(
                    state_for_task.scope(),
                    operation_id_for_task.as_str(),
                    OpKind::Rename,
                    initiator,
                    &volume_id_for_task,
                    Some(&volume_id_for_task),
                    rows_for_task.len() as u64,
                );

                let recorder = BulkRenameRecorder::new(state_for_task.shared_scope(), volume_id_for_task.clone());
                let run = if uses_local_paths {
                    let rows = rows_for_task.clone();
                    let intent = state_for_task.intent().clone();
                    let renaming = ops
                        .host()
                        .runtime()
                        .spawn_blocking(move || bulk_rename_local(&rows, &intent, &recorder));
                    match renaming.await {
                        Ok(result) => result,
                        Err(join_error) => BulkRenameRun::failed(rows_for_task.len(), join_error.to_string()),
                    }
                } else {
                    bulk_rename_remote(
                        ops.host(),
                        &rows_for_task,
                        &volume_id_for_task,
                        state_for_task.intent(),
                        &recorder,
                    )
                    .await
                };

                if uses_local_paths {
                    for (row, outcome) in rows_for_task.iter().zip(run.outcomes.iter()) {
                        if *outcome == BulkRenameOutcome::Done && row.source != row.destination {
                            super::notify_rename_in_listing(
                                ops.host(),
                                &volume_id_for_task,
                                &row.source,
                                &row.destination,
                            )
                            .await;
                        }
                    }
                }

                emit_bulk_rename_progress(
                    events_for_task.as_ref(),
                    &state_for_task,
                    operation_id_for_task.as_str(),
                    &rows_for_task,
                    &run.outcomes,
                );
                if run.cancelled {
                    events_for_task.emit_cancelled(WriteCancelledEvent {
                        operation_id: operation_id_for_task.to_string(),
                        operation_type: WriteOperationType::Rename,
                        files_processed: run.processed(),
                        rollback: CancelRollback::none(),
                    });
                    super::super::journal::finalize_op(
                        state_for_task.scope(),
                        OpKind::Rename,
                        ExecutionStatus::Canceled,
                    );
                } else {
                    events_for_task.emit_complete(WriteCompleteEvent {
                        operation_id: operation_id_for_task.to_string(),
                        operation_type: WriteOperationType::Rename,
                        files_processed: rows_for_task.len(),
                        files_skipped: run.skipped(),
                        bytes_processed: 0,
                        appeared_during_move: None,
                        top_level_skipped: None,
                        refused: None,
                        metadata_left_behind: None,
                        durability_unconfirmed: None,
                        locked_kept: None,
                    });
                    super::super::journal::finalize_op(state_for_task.scope(), OpKind::Rename, ExecutionStatus::Done);
                }

                task_guard.disarm();
                ops.manager().on_settled(&operation_id_for_task);
            })
        };

        self.manager().spawn_managed(descriptor, state, Box::new(deferred));
        Ok(WriteOperationStartResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Rename,
        })
    }
}

#[derive(Debug)]
struct BulkRenameRun {
    outcomes: Vec<BulkRenameOutcome>,
    cancelled: bool,
}

impl BulkRenameRun {
    fn failed(row_count: usize, _detail: String) -> Self {
        Self {
            outcomes: vec![BulkRenameOutcome::Failed; row_count],
            cancelled: false,
        }
    }

    /// The verdict once no further hop will land: journals the rows that never moved,
    /// then hands the outcomes up.
    fn settled(
        rows: &[BulkRenameRow],
        outcomes: Vec<BulkRenameOutcome>,
        cancelled: bool,
        recorder: &BulkRenameRecorder,
    ) -> Self {
        recorder.record_unmoved(rows, &outcomes);
        Self { outcomes, cancelled }
    }

    fn skipped(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| **outcome != BulkRenameOutcome::Done)
            .count()
    }

    fn processed(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| **outcome == BulkRenameOutcome::Done)
            .count()
    }
}

/// Local batch engine used on the blocking pool. Acyclic rows move directly in
/// dependency order. Only cycles and case-only changes use a sibling temporary.
fn bulk_rename_local(rows: &[BulkRenameRow], intent: &IntentCell, recorder: &BulkRenameRecorder) -> BulkRenameRun {
    let mut outcomes = vec![BulkRenameOutcome::Skipped; rows.len()];
    let mut active: Vec<bool> = rows
        .iter()
        .map(|row| {
            SourceFingerprint::capture_local(&row.source).is_some_and(|actual| actual == row.expected_fingerprint)
        })
        .collect();
    settle_local_conflicts(rows, &mut active);
    complete_noop_rows(rows, &active, &mut outcomes, recorder);
    for step in build_execution_plan(rows, &active) {
        if intent.is_cancelled() {
            return BulkRenameRun::settled(rows, outcomes, true, recorder);
        }
        match step {
            RenamePlanStep::Direct(index) => rename_local_direct(&rows[index], &mut outcomes[index], recorder),
            RenamePlanStep::CaseOnly(index) => rename_local_case_only(&rows[index], &mut outcomes[index], recorder),
            RenamePlanStep::Cycle(indices) => rename_local_cycle(rows, &indices, &mut outcomes, recorder),
        }
    }
    BulkRenameRun::settled(rows, outcomes, false, recorder)
}

async fn bulk_rename_remote(
    host: &EngineHost,
    rows: &[BulkRenameRow],
    volume_id: &str,
    intent: &IntentCell,
    recorder: &BulkRenameRecorder,
) -> BulkRenameRun {
    // Plain `get`: a bulk rename acts on the drive itself; it has no archive or `.git` routing.
    let Some(volume) = host.volumes().get(volume_id) else {
        return BulkRenameRun::failed(rows.len(), "volume unavailable".to_string());
    };
    let mut outcomes = vec![BulkRenameOutcome::Skipped; rows.len()];
    let mut active = Vec::with_capacity(rows.len());
    for row in rows {
        active.push(remote_fingerprint_matches(volume.as_ref(), &row.source, &row.expected_fingerprint).await);
    }
    settle_remote_conflicts(
        rows,
        &mut active,
        &mut outcomes,
        host.listings(),
        volume.as_ref(),
        volume_id,
    )
    .await;
    complete_noop_rows(rows, &active, &mut outcomes, recorder);
    for step in build_execution_plan(rows, &active) {
        if intent.is_cancelled() {
            return BulkRenameRun::settled(rows, outcomes, true, recorder);
        }
        match step {
            RenamePlanStep::Direct(index) => {
                rename_remote_direct(volume.as_ref(), &rows[index], &mut outcomes[index], recorder).await;
            }
            RenamePlanStep::CaseOnly(index) => {
                rename_remote_case_only(volume.as_ref(), &rows[index], &mut outcomes[index], recorder).await;
            }
            RenamePlanStep::Cycle(indices) => {
                rename_remote_cycle(volume.as_ref(), rows, &indices, &mut outcomes, recorder).await;
            }
        }
    }
    BulkRenameRun::settled(rows, outcomes, false, recorder)
}

/// Completes every active row whose destination is its source without touching the
/// filesystem. A no-op rename still belongs in the log as a `Done` row.
fn complete_noop_rows(
    rows: &[BulkRenameRow],
    active: &[bool],
    outcomes: &mut [BulkRenameOutcome],
    recorder: &BulkRenameRecorder,
) {
    for (index, row) in rows.iter().enumerate().filter(|(index, _)| active[*index]) {
        if row.source == row.destination {
            outcomes[index] = BulkRenameOutcome::Done;
            recorder.record_hop(row, &row.source, &row.destination);
        }
    }
}

fn rename_local_direct(row: &BulkRenameRow, outcome: &mut BulkRenameOutcome, recorder: &BulkRenameRecorder) {
    if !SourceFingerprint::capture_local(&row.source).is_some_and(|actual| actual == row.expected_fingerprint) {
        return;
    }
    recorder.note_write(&row.source, &row.destination);
    *outcome = match rename_local_exclusive(&row.source, &row.destination) {
        Ok(()) => {
            recorder.record_hop(row, &row.source, &row.destination);
            BulkRenameOutcome::Done
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => BulkRenameOutcome::Skipped,
        Err(_) => BulkRenameOutcome::Failed,
    };
}

fn rename_local_case_only(row: &BulkRenameRow, outcome: &mut BulkRenameOutcome, recorder: &BulkRenameRecorder) {
    if !SourceFingerprint::capture_local(&row.source).is_some_and(|actual| actual == row.expected_fingerprint) {
        return;
    }
    let Some(temporary) = unique_temporary_path(&row.source, &row.row_id) else {
        *outcome = BulkRenameOutcome::Failed;
        return;
    };
    recorder.note_write(&row.source, &temporary);
    if rename_local_exclusive(&row.source, &temporary).is_err() {
        *outcome = BulkRenameOutcome::Failed;
        return;
    }
    recorder.record_hop(row, &row.source, &temporary);
    recorder.note_write(&temporary, &row.destination);
    *outcome = match rename_local_exclusive(&temporary, &row.destination) {
        Ok(()) => {
            recorder.record_hop(row, &temporary, &row.destination);
            BulkRenameOutcome::Done
        }
        Err(error) => {
            recorder.note_write(&temporary, &row.source);
            if rename_local_exclusive(&temporary, &row.source).is_ok() {
                recorder.record_hop(row, &temporary, &row.source);
            }
            if error.kind() == io::ErrorKind::AlreadyExists {
                BulkRenameOutcome::Skipped
            } else {
                BulkRenameOutcome::Failed
            }
        }
    };
}

/// Rotates a closed dependency component with one temporary. Once staging has
/// started, the bounded component finishes or rolls back before cancellation is
/// observed again, so Cmdr never intentionally strands a private temp name.
fn rename_local_cycle(
    rows: &[BulkRenameRow],
    indices: &[usize],
    outcomes: &mut [BulkRenameOutcome],
    recorder: &BulkRenameRecorder,
) {
    if indices.iter().any(|index| {
        !SourceFingerprint::capture_local(&rows[*index].source)
            .is_some_and(|actual| actual == rows[*index].expected_fingerprint)
    }) {
        return;
    }
    let first = indices[0];
    let Some(temporary) = unique_temporary_path(&rows[first].source, &rows[first].row_id) else {
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    };
    recorder.note_write(&rows[first].source, &temporary);
    if rename_local_exclusive(&rows[first].source, &temporary).is_err() {
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    }
    recorder.record_hop(&rows[first], &rows[first].source, &temporary);

    let mut moved = Vec::with_capacity(indices.len() - 1);
    for index in indices.iter().skip(1).rev().copied() {
        let row = &rows[index];
        recorder.note_write(&row.source, &row.destination);
        if rename_local_exclusive(&row.source, &row.destination).is_err() {
            restore_local_cycle(rows, first, &temporary, &moved, recorder);
            outcomes[index] = BulkRenameOutcome::Failed;
            return;
        }
        recorder.record_hop(row, &row.source, &row.destination);
        moved.push(index);
    }
    recorder.note_write(&temporary, &rows[first].destination);
    if rename_local_exclusive(&temporary, &rows[first].destination).is_err() {
        restore_local_cycle(rows, first, &temporary, &moved, recorder);
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    }
    recorder.record_hop(&rows[first], &temporary, &rows[first].destination);
    for index in indices {
        outcomes[*index] = BulkRenameOutcome::Done;
    }
}

/// Puts a half-rotated cycle back. Each reversal is itself a hop the filesystem
/// really made, so it gets its own row: the log stays a faithful list of what
/// happened, and a reverse-order replay of source→temp then temp→source is a net
/// no-op, which is what a restored cycle is.
fn restore_local_cycle(
    rows: &[BulkRenameRow],
    first: usize,
    temporary: &Path,
    moved: &[usize],
    recorder: &BulkRenameRecorder,
) {
    for index in moved.iter().rev().copied() {
        recorder.note_write(&rows[index].destination, &rows[index].source);
        if rename_local_exclusive(&rows[index].destination, &rows[index].source).is_ok() {
            recorder.record_hop(&rows[index], &rows[index].destination, &rows[index].source);
        }
    }
    recorder.note_write(temporary, &rows[first].source);
    if rename_local_exclusive(temporary, &rows[first].source).is_ok() {
        recorder.record_hop(&rows[first], temporary, &rows[first].source);
    }
}

async fn rename_remote_direct(
    volume: &dyn Volume,
    row: &BulkRenameRow,
    outcome: &mut BulkRenameOutcome,
    recorder: &BulkRenameRecorder,
) {
    if !remote_fingerprint_matches(volume, &row.source, &row.expected_fingerprint).await {
        return;
    }
    recorder.note_write(&row.source, &row.destination);
    *outcome = if volume.rename(&row.source, &row.destination, false).await.is_ok() {
        recorder.record_hop(row, &row.source, &row.destination);
        BulkRenameOutcome::Done
    } else if volume.get_metadata(&row.destination).await.is_ok() {
        BulkRenameOutcome::Skipped
    } else {
        BulkRenameOutcome::Failed
    };
}

async fn rename_remote_case_only(
    volume: &dyn Volume,
    row: &BulkRenameRow,
    outcome: &mut BulkRenameOutcome,
    recorder: &BulkRenameRecorder,
) {
    if !remote_fingerprint_matches(volume, &row.source, &row.expected_fingerprint).await {
        return;
    }
    let Some(temporary) = unique_remote_temporary_path(volume, &row.source, &row.row_id).await else {
        *outcome = BulkRenameOutcome::Failed;
        return;
    };
    recorder.note_write(&row.source, &temporary);
    if volume.rename(&row.source, &temporary, false).await.is_err() {
        *outcome = BulkRenameOutcome::Failed;
        return;
    }
    recorder.record_hop(row, &row.source, &temporary);
    recorder.note_write(&temporary, &row.destination);
    if volume.rename(&temporary, &row.destination, false).await.is_ok() {
        recorder.record_hop(row, &temporary, &row.destination);
        *outcome = BulkRenameOutcome::Done;
    } else {
        recorder.note_write(&temporary, &row.source);
        let destination_exists = volume.get_metadata(&row.destination).await.is_ok();
        if volume.rename(&temporary, &row.source, false).await.is_ok() {
            recorder.record_hop(row, &temporary, &row.source);
        }
        *outcome = if destination_exists {
            BulkRenameOutcome::Skipped
        } else {
            BulkRenameOutcome::Failed
        };
    }
}

async fn rename_remote_cycle(
    volume: &dyn Volume,
    rows: &[BulkRenameRow],
    indices: &[usize],
    outcomes: &mut [BulkRenameOutcome],
    recorder: &BulkRenameRecorder,
) {
    for index in indices {
        if !remote_fingerprint_matches(volume, &rows[*index].source, &rows[*index].expected_fingerprint).await {
            return;
        }
    }
    let first = indices[0];
    let Some(temporary) = unique_remote_temporary_path(volume, &rows[first].source, &rows[first].row_id).await else {
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    };
    recorder.note_write(&rows[first].source, &temporary);
    if volume.rename(&rows[first].source, &temporary, false).await.is_err() {
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    }
    recorder.record_hop(&rows[first], &rows[first].source, &temporary);
    let mut moved = Vec::with_capacity(indices.len() - 1);
    for index in indices.iter().skip(1).rev().copied() {
        let row = &rows[index];
        recorder.note_write(&row.source, &row.destination);
        if volume.rename(&row.source, &row.destination, false).await.is_err() {
            restore_remote_cycle(volume, rows, first, &temporary, &moved, recorder).await;
            outcomes[index] = BulkRenameOutcome::Failed;
            return;
        }
        recorder.record_hop(row, &row.source, &row.destination);
        moved.push(index);
    }
    recorder.note_write(&temporary, &rows[first].destination);
    if volume
        .rename(&temporary, &rows[first].destination, false)
        .await
        .is_err()
    {
        restore_remote_cycle(volume, rows, first, &temporary, &moved, recorder).await;
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    }
    recorder.record_hop(&rows[first], &temporary, &rows[first].destination);
    for index in indices {
        outcomes[*index] = BulkRenameOutcome::Done;
    }
}

async fn restore_remote_cycle(
    volume: &dyn Volume,
    rows: &[BulkRenameRow],
    first: usize,
    temporary: &Path,
    moved: &[usize],
    recorder: &BulkRenameRecorder,
) {
    for index in moved.iter().rev().copied() {
        recorder.note_write(&rows[index].destination, &rows[index].source);
        if volume
            .rename(&rows[index].destination, &rows[index].source, false)
            .await
            .is_ok()
        {
            recorder.record_hop(&rows[index], &rows[index].destination, &rows[index].source);
        }
    }
    recorder.note_write(temporary, &rows[first].source);
    if volume.rename(temporary, &rows[first].source, false).await.is_ok() {
        recorder.record_hop(&rows[first], temporary, &rows[first].source);
    }
}

fn unique_temporary_path(source: &Path, row_id: &str) -> Option<PathBuf> {
    let parent = source.parent()?;
    for _ in 0..16 {
        let candidate = parent.join(format!(".cmdr-bulk-rename-{row_id}-{}", Uuid::new_v4()));
        if std::fs::symlink_metadata(&candidate).is_err() {
            return Some(candidate);
        }
    }
    None
}

async fn unique_remote_temporary_path(volume: &dyn Volume, source: &Path, row_id: &str) -> Option<PathBuf> {
    let parent = source.parent()?;
    for _ in 0..16 {
        let candidate = parent.join(format!(".cmdr-bulk-rename-{row_id}-{}", Uuid::new_v4()));
        if volume.get_metadata(&candidate).await.is_err() {
            return Some(candidate);
        }
    }
    None
}

/// Whether the row's source on `volume` is still the file the preflight recorded.
/// The recorded normalized path is part of the fingerprint, so a row pointing at a
/// different path than the one it was accepted for can never match.
async fn remote_fingerprint_matches(volume: &dyn Volume, path: &Path, expected: &SourceFingerprint) -> bool {
    SourceFingerprint::capture_remote(volume, path).await.as_ref() == Some(expected)
}

fn emit_bulk_rename_progress(
    events: &dyn OperationEventSink,
    state: &WriteOperationState,
    operation_id: &str,
    rows: &[BulkRenameRow],
    outcomes: &[BulkRenameOutcome],
) {
    for (index, (row, outcome)) in rows.iter().zip(outcomes.iter()).enumerate() {
        state.scope().status().update(
            operation_id,
            super::super::types::WriteOperationPhase::Copying,
            Some(
                row.destination
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            ),
            index + 1,
            rows.len(),
            0,
            0,
        );
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Rename,
                super::super::types::WriteOperationPhase::Copying,
                Some(
                    row.destination
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                ),
                index + 1,
                rows.len(),
                0,
                0,
            ),
        );
        if *outcome == BulkRenameOutcome::Done {
            events.emit_source_item_done(WriteSourceItemDoneEvent {
                operation_id: operation_id.to_string(),
                source_path: row.source.to_string_lossy().to_string(),
                // `Done` means the row landed at its new name, so nothing answers
                // to the old path any more.
                source_removed: true,
                outcome: SourceItemOutcome::Done,
            });
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod look_alike_tests;

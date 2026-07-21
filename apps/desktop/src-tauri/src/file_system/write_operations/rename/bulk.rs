//! Managed batch rename for Ask Cmdr's reviewed rename proposals.
//!
//! This module owns the server-side rows and collision-safe driver. Independent
//! rows and chains rename directly in dependency order. Each cycle and each
//! case-only rename uses one same-directory temporary name.

use std::collections::HashSet;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicU8;
use std::time::Duration;

use uuid::Uuid;

use super::super::event_sinks::OperationEventSink;
use super::super::manager::{self, OperationDescriptor, OperationSummaryText};
use super::super::state::{WriteOperationState, WriteSettledGuard, is_cancelled, update_operation_status};
use super::super::types::{
    WriteCancelledEvent, WriteCompleteEvent, WriteOperationStartResult, WriteOperationType, WriteProgressEvent,
    WriteSourceItemDoneEvent,
};
use crate::file_system::volume::{LaneKey, Volume};
use crate::operation_log::types::{EntryType, ExecutionStatus, Initiator, ItemOutcome, OpKind};

/// Atomically renames a local file only when `destination` is unoccupied.
///
/// The review-time existence check is advisory. This syscall-level exclusion
/// is the write boundary that prevents a destination created after review from
/// being silently replaced.
#[cfg(target_os = "macos")]
pub(super) fn rename_local_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains a null byte"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination path contains a null byte"))?;
    // SAFETY: Both pointers come from live `CString`s and remain valid for the
    // duration of the call. `RENAME_EXCL` asks the kernel to combine the
    // destination-absence check and rename into one operation.
    let result = unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
pub(super) fn rename_local_exclusive(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains a null byte"))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "destination path contains a null byte"))?;
    // SAFETY: Both pointers come from live `CString`s and remain valid for the
    // duration of the call. `RENAME_NOREPLACE` provides Linux's equivalent
    // atomic no-overwrite contract.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Server-owned source identity captured by the rename-review preflight. The
/// frontend never creates this data; the Ask Cmdr command maps its accepted
/// preflight directly into this write-engine input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BulkRenameFingerprint {
    Local {
        device: u64,
        inode: u64,
        size: u64,
        modified_nanos: Option<u128>,
    },
    Remote {
        normalized_path: String,
        size: Option<u64>,
        modified: Option<i64>,
    },
}

/// One immutable row that the user allowed and preflight accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BulkRenameRow {
    pub row_id: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub expected_fingerprint: BulkRenameFingerprint,
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

    fn journal_outcome(self) -> ItemOutcome {
        match self {
            Self::Done => ItemOutcome::Done,
            Self::Skipped => ItemOutcome::Skipped,
            Self::Failed => ItemOutcome::Failed,
        }
    }
}

/// Starts one queued, same-volume batch rename. The caller has already resolved
/// the proposal id and exact accepted-preflight subset; this layer receives only
/// immutable backend rows, never frontend paths or names.
pub(crate) fn start_bulk_rename(
    events: Arc<dyn OperationEventSink>,
    volume_id: String,
    rows: Vec<BulkRenameRow>,
    initiator: Initiator,
) -> Result<WriteOperationStartResult, String> {
    if rows.is_empty() {
        return Err("Choose at least one rename to apply.".to_string());
    }
    if rows.iter().any(|row| row.source.parent() != row.destination.parent()) {
        return Err("A rename plan can only change names in the same folder.".to_string());
    }

    // `root` is the only backend that owns raw local paths here. Every mounted
    // volume, including a locally mounted removable drive, stays on its Volume
    // route so its listing and connection semantics remain authoritative.
    let uses_local_paths = volume_id == "root";
    let (lanes, volume_ids, settled_volume) = if uses_local_paths {
        (vec![LaneKey::new("root")], Vec::new(), None)
    } else {
        let volume = crate::file_system::get_volume_manager()
            .get(&volume_id)
            .ok_or_else(|| "The rename volume is no longer available.".to_string())?;
        (
            vec![volume.lane_key()],
            vec![volume_id.clone()],
            Some(volume.name().to_string()),
        )
    };

    let operation_id = Uuid::new_v4().to_string();
    let summary = OperationSummaryText {
        source: Some(format!("{} files", rows.len())),
        destination: None,
    };
    let descriptor = OperationDescriptor {
        operation_id: operation_id.clone(),
        operation_type: WriteOperationType::Rename,
        lanes,
        volume_ids,
        summary,
    };
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(200)));
    let events_for_task = Arc::clone(&events);
    let operation_id_for_task = operation_id.clone();
    let rows_for_task = rows.clone();
    let volume_id_for_task = volume_id.clone();
    let state_for_task = Arc::clone(&state);
    let deferred = move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let task_guard = manager::ManagedTaskGuard::new(operation_id_for_task.clone());
            let _settled_guard = WriteSettledGuard::new(
                Arc::clone(&events_for_task),
                operation_id_for_task.clone(),
                WriteOperationType::Rename,
                settled_volume,
            );
            super::super::journal::open_volume_op(
                &operation_id_for_task,
                OpKind::Rename,
                initiator,
                &volume_id_for_task,
                Some(&volume_id_for_task),
                rows_for_task.len() as u64,
            );

            let run = if uses_local_paths {
                let rows = rows_for_task.clone();
                let intent = Arc::clone(&state_for_task.intent);
                match tokio::task::spawn_blocking(move || bulk_rename_local(&rows, &intent)).await {
                    Ok(result) => result,
                    Err(join_error) => BulkRenameRun::failed(rows_for_task.len(), join_error.to_string()),
                }
            } else {
                bulk_rename_remote(&rows_for_task, &volume_id_for_task, &state_for_task.intent).await
            };

            if uses_local_paths {
                for (row, outcome) in rows_for_task.iter().zip(run.outcomes.iter()) {
                    if *outcome == BulkRenameOutcome::Done && row.source != row.destination {
                        super::notify_rename_in_listing(&volume_id_for_task, &row.source, &row.destination).await;
                    }
                }
            }

            record_bulk_rename_outcomes(
                &operation_id_for_task,
                &volume_id_for_task,
                &rows_for_task,
                &run.outcomes,
            );
            emit_bulk_rename_progress(
                events_for_task.as_ref(),
                &state_for_task,
                &operation_id_for_task,
                &rows_for_task,
                &run.outcomes,
            );
            if run.cancelled {
                events_for_task.emit_cancelled(WriteCancelledEvent {
                    operation_id: operation_id_for_task.clone(),
                    operation_type: WriteOperationType::Rename,
                    files_processed: run.processed(),
                    rolled_back: false,
                });
                super::super::journal::finalize_op(&operation_id_for_task, OpKind::Rename, ExecutionStatus::Canceled);
            } else {
                events_for_task.emit_complete(WriteCompleteEvent {
                    operation_id: operation_id_for_task.clone(),
                    operation_type: WriteOperationType::Rename,
                    files_processed: rows_for_task.len(),
                    files_skipped: run.skipped(),
                    bytes_processed: 0,
                });
                super::super::journal::finalize_op(&operation_id_for_task, OpKind::Rename, ExecutionStatus::Done);
            }

            task_guard.disarm();
            manager::manager().on_settled(&operation_id_for_task);
        })
    };

    manager::manager().spawn_managed(descriptor, state, Box::new(deferred));
    Ok(WriteOperationStartResult {
        operation_id,
        operation_type: WriteOperationType::Rename,
    })
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

/// One collision-safe unit in a batch rename. Direct steps consume a free
/// destination. A cycle rotates through one temporary name, while a case-only
/// change uses one because the volume may treat both spellings as the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RenamePlanStep {
    Direct(usize),
    Cycle(Vec<usize>),
    CaseOnly(usize),
}

/// Orders active rows without filesystem access. The rename graph is
/// functional after preflight: every source and destination has at most one
/// owner. Removing rows whose destination is currently free peels all acyclic
/// chains in execution order; the remaining components are cycles.
fn build_execution_plan(rows: &[BulkRenameRow], active: &[bool]) -> Vec<RenamePlanStep> {
    let mut remaining: HashSet<usize> = rows
        .iter()
        .enumerate()
        .filter(|(index, row)| active[*index] && row.source != row.destination)
        .map(|(index, _)| index)
        .collect();
    let mut plan = Vec::with_capacity(remaining.len());

    loop {
        let source_to_index: std::collections::HashMap<String, usize> = remaining
            .iter()
            .map(|index| (normalized_path(&rows[*index].source), *index))
            .collect();
        let mut ready: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|index| {
                let source = normalized_path(&rows[*index].source);
                let destination = normalized_path(&rows[*index].destination);
                source == destination || !source_to_index.contains_key(&destination)
            })
            .collect();
        ready.sort_unstable();
        if ready.is_empty() {
            break;
        }
        for index in ready {
            if !remaining.remove(&index) {
                continue;
            }
            if normalized_path(&rows[index].source) == normalized_path(&rows[index].destination) {
                plan.push(RenamePlanStep::CaseOnly(index));
            } else {
                plan.push(RenamePlanStep::Direct(index));
            }
        }
    }

    while let Some(start) = remaining.iter().min().copied() {
        let source_to_index: std::collections::HashMap<String, usize> = remaining
            .iter()
            .map(|index| (normalized_path(&rows[*index].source), *index))
            .collect();
        let mut cycle = vec![start];
        let mut current = start;
        loop {
            let destination = normalized_path(&rows[current].destination);
            let next = source_to_index[&destination];
            if next == start {
                break;
            }
            cycle.push(next);
            current = next;
        }
        for index in &cycle {
            remaining.remove(index);
        }
        plan.push(RenamePlanStep::Cycle(cycle));
    }
    plan
}

/// Local batch engine used on the blocking pool. Acyclic rows move directly in
/// dependency order. Only cycles and case-only changes use a sibling temporary.
fn bulk_rename_local(rows: &[BulkRenameRow], intent: &AtomicU8) -> BulkRenameRun {
    let mut outcomes = vec![BulkRenameOutcome::Skipped; rows.len()];
    let mut active: Vec<bool> = rows
        .iter()
        .map(|row| local_fingerprint(&row.source).is_some_and(|actual| actual == row.expected_fingerprint))
        .collect();
    settle_local_conflicts(rows, &mut active);
    for (index, row) in rows.iter().enumerate().filter(|(index, _)| active[*index]) {
        if row.source == row.destination {
            outcomes[index] = BulkRenameOutcome::Done;
        }
    }
    for step in build_execution_plan(rows, &active) {
        if is_cancelled(intent) {
            return BulkRenameRun {
                outcomes,
                cancelled: true,
            };
        }
        match step {
            RenamePlanStep::Direct(index) => rename_local_direct(&rows[index], &mut outcomes[index]),
            RenamePlanStep::CaseOnly(index) => rename_local_case_only(&rows[index], &mut outcomes[index]),
            RenamePlanStep::Cycle(indices) => rename_local_cycle(rows, &indices, &mut outcomes),
        }
    }
    BulkRenameRun {
        outcomes,
        cancelled: false,
    }
}

async fn bulk_rename_remote(rows: &[BulkRenameRow], volume_id: &str, intent: &AtomicU8) -> BulkRenameRun {
    let Some(volume) = crate::file_system::get_volume_manager().get(volume_id) else {
        return BulkRenameRun::failed(rows.len(), "volume unavailable".to_string());
    };
    let mut outcomes = vec![BulkRenameOutcome::Skipped; rows.len()];
    let mut active = Vec::with_capacity(rows.len());
    for row in rows {
        active.push(remote_fingerprint_matches(volume.as_ref(), &row.source, &row.expected_fingerprint).await);
    }
    settle_remote_conflicts(rows, &mut active, volume.as_ref()).await;
    for (index, row) in rows.iter().enumerate().filter(|(index, _)| active[*index]) {
        if row.source == row.destination {
            outcomes[index] = BulkRenameOutcome::Done;
        }
    }
    for step in build_execution_plan(rows, &active) {
        if is_cancelled(intent) {
            return BulkRenameRun {
                outcomes,
                cancelled: true,
            };
        }
        match step {
            RenamePlanStep::Direct(index) => {
                rename_remote_direct(volume.as_ref(), &rows[index], &mut outcomes[index]).await;
            }
            RenamePlanStep::CaseOnly(index) => {
                rename_remote_case_only(volume.as_ref(), &rows[index], &mut outcomes[index]).await;
            }
            RenamePlanStep::Cycle(indices) => {
                rename_remote_cycle(volume.as_ref(), rows, &indices, &mut outcomes).await;
            }
        }
    }
    BulkRenameRun {
        outcomes,
        cancelled: false,
    }
}

fn rename_local_direct(row: &BulkRenameRow, outcome: &mut BulkRenameOutcome) {
    if !local_fingerprint(&row.source).is_some_and(|actual| actual == row.expected_fingerprint) {
        return;
    }
    note_rename_write(&row.source, &row.destination);
    *outcome = match rename_local_exclusive(&row.source, &row.destination) {
        Ok(()) => BulkRenameOutcome::Done,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => BulkRenameOutcome::Skipped,
        Err(_) => BulkRenameOutcome::Failed,
    };
}

fn rename_local_case_only(row: &BulkRenameRow, outcome: &mut BulkRenameOutcome) {
    if !local_fingerprint(&row.source).is_some_and(|actual| actual == row.expected_fingerprint) {
        return;
    }
    let Some(temporary) = unique_temporary_path(&row.source, &row.row_id) else {
        *outcome = BulkRenameOutcome::Failed;
        return;
    };
    note_rename_write(&row.source, &temporary);
    if rename_local_exclusive(&row.source, &temporary).is_err() {
        *outcome = BulkRenameOutcome::Failed;
        return;
    }
    note_rename_write(&temporary, &row.destination);
    *outcome = match rename_local_exclusive(&temporary, &row.destination) {
        Ok(()) => BulkRenameOutcome::Done,
        Err(error) => {
            note_rename_write(&temporary, &row.source);
            let _ = rename_local_exclusive(&temporary, &row.source);
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
fn rename_local_cycle(rows: &[BulkRenameRow], indices: &[usize], outcomes: &mut [BulkRenameOutcome]) {
    if indices.iter().any(|index| {
        !local_fingerprint(&rows[*index].source).is_some_and(|actual| actual == rows[*index].expected_fingerprint)
    }) {
        return;
    }
    let first = indices[0];
    let Some(temporary) = unique_temporary_path(&rows[first].source, &rows[first].row_id) else {
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    };
    note_rename_write(&rows[first].source, &temporary);
    if rename_local_exclusive(&rows[first].source, &temporary).is_err() {
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    }

    let mut moved = Vec::with_capacity(indices.len() - 1);
    for index in indices.iter().skip(1).rev().copied() {
        let row = &rows[index];
        note_rename_write(&row.source, &row.destination);
        if rename_local_exclusive(&row.source, &row.destination).is_err() {
            restore_local_cycle(rows, first, &temporary, &moved);
            outcomes[index] = BulkRenameOutcome::Failed;
            return;
        }
        moved.push(index);
    }
    note_rename_write(&temporary, &rows[first].destination);
    if rename_local_exclusive(&temporary, &rows[first].destination).is_err() {
        restore_local_cycle(rows, first, &temporary, &moved);
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    }
    for index in indices {
        outcomes[*index] = BulkRenameOutcome::Done;
    }
}

fn restore_local_cycle(rows: &[BulkRenameRow], first: usize, temporary: &Path, moved: &[usize]) {
    for index in moved.iter().rev().copied() {
        note_rename_write(&rows[index].destination, &rows[index].source);
        let _ = rename_local_exclusive(&rows[index].destination, &rows[index].source);
    }
    note_rename_write(temporary, &rows[first].source);
    let _ = rename_local_exclusive(temporary, &rows[first].source);
}

async fn rename_remote_direct(volume: &dyn Volume, row: &BulkRenameRow, outcome: &mut BulkRenameOutcome) {
    if !remote_fingerprint_matches(volume, &row.source, &row.expected_fingerprint).await {
        return;
    }
    note_rename_write(&row.source, &row.destination);
    *outcome = if volume.rename(&row.source, &row.destination, false).await.is_ok() {
        BulkRenameOutcome::Done
    } else if volume.get_metadata(&row.destination).await.is_ok() {
        BulkRenameOutcome::Skipped
    } else {
        BulkRenameOutcome::Failed
    };
}

async fn rename_remote_case_only(volume: &dyn Volume, row: &BulkRenameRow, outcome: &mut BulkRenameOutcome) {
    if !remote_fingerprint_matches(volume, &row.source, &row.expected_fingerprint).await {
        return;
    }
    let Some(temporary) = unique_remote_temporary_path(volume, &row.source, &row.row_id).await else {
        *outcome = BulkRenameOutcome::Failed;
        return;
    };
    note_rename_write(&row.source, &temporary);
    if volume.rename(&row.source, &temporary, false).await.is_err() {
        *outcome = BulkRenameOutcome::Failed;
        return;
    }
    note_rename_write(&temporary, &row.destination);
    if volume.rename(&temporary, &row.destination, false).await.is_ok() {
        *outcome = BulkRenameOutcome::Done;
    } else {
        note_rename_write(&temporary, &row.source);
        let destination_exists = volume.get_metadata(&row.destination).await.is_ok();
        let _ = volume.rename(&temporary, &row.source, false).await;
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
    note_rename_write(&rows[first].source, &temporary);
    if volume.rename(&rows[first].source, &temporary, false).await.is_err() {
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    }
    let mut moved = Vec::with_capacity(indices.len() - 1);
    for index in indices.iter().skip(1).rev().copied() {
        let row = &rows[index];
        note_rename_write(&row.source, &row.destination);
        if volume.rename(&row.source, &row.destination, false).await.is_err() {
            restore_remote_cycle(volume, rows, first, &temporary, &moved).await;
            outcomes[index] = BulkRenameOutcome::Failed;
            return;
        }
        moved.push(index);
    }
    note_rename_write(&temporary, &rows[first].destination);
    if volume
        .rename(&temporary, &rows[first].destination, false)
        .await
        .is_err()
    {
        restore_remote_cycle(volume, rows, first, &temporary, &moved).await;
        outcomes[first] = BulkRenameOutcome::Failed;
        return;
    }
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
) {
    for index in moved.iter().rev().copied() {
        note_rename_write(&rows[index].destination, &rows[index].source);
        let _ = volume
            .rename(&rows[index].destination, &rows[index].source, false)
            .await;
    }
    note_rename_write(temporary, &rows[first].source);
    let _ = volume.rename(temporary, &rows[first].source, false).await;
}

fn settle_local_conflicts(rows: &[BulkRenameRow], active: &mut [bool]) {
    loop {
        let sources: HashSet<String> = rows
            .iter()
            .zip(active.iter())
            .filter(|(_, active)| **active)
            .map(|(row, _)| normalized_path(&row.source))
            .collect();
        let mut changed = false;
        for (row, active) in rows.iter().zip(active.iter_mut()) {
            if !*active || row.source == row.destination {
                continue;
            }
            if !sources.contains(&normalized_path(&row.destination))
                && let Ok(destination_meta) = std::fs::symlink_metadata(&row.destination)
            {
                let destination_is_source = std::fs::symlink_metadata(&row.source)
                    .is_ok_and(|source_meta| same_local_file(&source_meta, &destination_meta));
                if !destination_is_source {
                    *active = false;
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
}

async fn settle_remote_conflicts(rows: &[BulkRenameRow], active: &mut [bool], volume: &dyn Volume) {
    loop {
        let sources: HashSet<String> = rows
            .iter()
            .zip(active.iter())
            .filter(|(_, active)| **active)
            .map(|(row, _)| normalized_path(&row.source))
            .collect();
        let mut changed = false;
        for (row, active) in rows.iter().zip(active.iter_mut()) {
            if !*active || row.source == row.destination {
                continue;
            }
            if !sources.contains(&normalized_path(&row.destination))
                && volume.get_metadata(&row.destination).await.is_ok()
            {
                *active = false;
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

fn normalized_path(path: &Path) -> String {
    crate::indexing::store::normalize_for_comparison(&path.to_string_lossy())
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

fn note_rename_write(from: &Path, to: &Path) {
    crate::downloads::note_pending_write_for_cmdr(from);
    crate::downloads::note_pending_write_for_cmdr(to);
}

fn local_fingerprint(path: &Path) -> Option<BulkRenameFingerprint> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_dir() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(BulkRenameFingerprint::Local {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified_nanos: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|time| time.as_nanos()),
        })
    }
    #[cfg(not(unix))]
    {
        Some(BulkRenameFingerprint::Local {
            device: 0,
            inode: 0,
            size: metadata.len(),
            modified_nanos: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|time| time.as_nanos()),
        })
    }
}

#[cfg(unix)]
fn same_local_file(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_local_file(_left: &std::fs::Metadata, _right: &std::fs::Metadata) -> bool {
    false
}

async fn remote_fingerprint_matches(volume: &dyn Volume, path: &Path, expected: &BulkRenameFingerprint) -> bool {
    let BulkRenameFingerprint::Remote {
        normalized_path,
        size,
        modified,
    } = expected
    else {
        return false;
    };
    if normalized_path != &crate::indexing::store::normalize_for_comparison(&path.to_string_lossy()) {
        return false;
    }
    let Ok(metadata) = volume.get_metadata(path).await else {
        return false;
    };
    !metadata.is_directory && metadata.size == *size && metadata.modified_at.map(|value| value as i64) == *modified
}

fn record_bulk_rename_outcomes(
    operation_id: &str,
    volume_id: &str,
    rows: &[BulkRenameRow],
    outcomes: &[BulkRenameOutcome],
) {
    for (row, outcome) in rows.iter().zip(outcomes.iter().copied()) {
        let size = match &row.expected_fingerprint {
            BulkRenameFingerprint::Local { size, .. } => Some(*size as i64),
            BulkRenameFingerprint::Remote { size, .. } => size.map(|size| size as i64),
        };
        super::super::journal::record_volume_leaf(
            operation_id,
            EntryType::File,
            volume_id,
            &row.source,
            Some((volume_id, &row.destination)),
            size,
            None,
            false,
            outcome.journal_outcome(),
        );
    }
}

fn emit_bulk_rename_progress(
    events: &dyn OperationEventSink,
    state: &WriteOperationState,
    operation_id: &str,
    rows: &[BulkRenameRow],
    outcomes: &[BulkRenameOutcome],
) {
    for (index, (row, outcome)) in rows.iter().zip(outcomes.iter()).enumerate() {
        update_operation_status(
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
            });
        }
    }
}

#[cfg(test)]
mod tests;

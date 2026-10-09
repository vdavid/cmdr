//! Apply (Start): renames the session's rows that are `Ready` NOW, if they're
//! exactly the ones the user's preview showed (`session::prepare`), through the
//! bulk-rename executor Ask Cmdr uses (`start_renames`): chains, swaps, and
//! case-only renames, a fingerprint recheck before each step, every step
//! journaled, so the operation log's undo reverses it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

use crate::deadline::{Deadline, blocking_typed_result_with_timeout, io_budget_for_volume, timeout_detached_within};
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::{BulkRenameRow, SourceFingerprint, start_renames};
use crate::operation_log::types::Initiator;

use super::error::MultiRenameError;
use super::plan::PreviewRow;
use super::session::{Prepared, prepare};

/// A started rename.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameStarted {
    /// The operation, for its progress, the queue, and Undo.
    pub operation_id: String,
    /// How many rows it renames.
    pub renaming: usize,
    /// Rows that swap names with each other, which a batch that runs as a move
    /// (a rename that copies, on S3) leaves out: they keep their names.
    pub swaps_left_out: usize,
}

/// How long apply waits for the remote fingerprints, at least (a volume with a
/// live session gets `SESSION_IO_TIMEOUT`, see `io_budget`).
const REMOTE_FINGERPRINT_TIMEOUT: Duration = Duration::from_secs(30);

/// How many remote fingerprints apply reads at once, like `start_renames` asks
/// `Volume::rename_work` (`RENAME_WORK_CONCURRENCY`).
const FINGERPRINT_CONCURRENCY: usize = 8;

fn bulk_row(dir: &Path, row: &PreviewRow, expected_fingerprint: SourceFingerprint) -> BulkRenameRow {
    BulkRenameRow {
        row_id: row.row.to_string(),
        source: dir.join(&row.old_name),
        destination: dir.join(&row.new_name),
        expected_fingerprint,
    }
}

/// Each ready row's source, fingerprinted through `volume`, several at once
/// under one deadline. A row whose file vanished since the preview is left out,
/// not failed.
async fn remote_rows(
    volume: Arc<dyn Volume>,
    volume_id: &str,
    dir: PathBuf,
    ready: Vec<PreviewRow>,
) -> Result<Vec<BulkRenameRow>, MultiRenameError> {
    let deadline = Deadline::new(io_budget_for_volume(volume_id, REMOTE_FINGERPRINT_TIMEOUT));
    timeout_detached_within(
        &deadline,
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        async move {
            let captured: Vec<Option<BulkRenameRow>> = stream::iter(ready)
                .map(|row| {
                    let volume = Arc::clone(&volume);
                    let dir = dir.clone();
                    async move {
                        SourceFingerprint::capture_remote(volume.as_ref(), &dir.join(&row.old_name))
                            .await
                            .map(|fingerprint| bulk_row(&dir, &row, fingerprint))
                    }
                })
                .buffered(FINGERPRINT_CONCURRENCY)
                .collect()
                .await;
            Ok(captured.into_iter().flatten().collect())
        },
    )
    .await
}

/// Renames the rows preview `preview_id` showed as ready, if they're still
/// exactly that. Returns once the operation has started; progress, the queue,
/// and Undo follow it by its id.
pub(crate) async fn apply(
    events: Arc<dyn crate::file_system::write_operations::OperationEventSink>,
    session_id: String,
    preview_id: u64,
) -> Result<MultiRenameStarted, MultiRenameError> {
    // Off the IPC thread: the listing lookup and a mask and regex per row.
    let prepared = blocking_typed_result_with_timeout(
        Duration::from_secs(5),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || prepare(&session_id, preview_id),
    )
    .await?;

    // An archive or a `.git` portal serves its folder read-only.
    let resolved = crate::file_system::volume::manager::get_volume_manager()
        .resolve(&prepared.volume_id, &prepared.dir)
        .await;
    if resolved.routed.is_some() {
        return Err(MultiRenameError::ReadOnly);
    }

    let Prepared { volume_id, dir, ready } = prepared;
    let bulk: Vec<BulkRenameRow> = if volume_id == "root" {
        blocking_typed_result_with_timeout(
            Duration::from_secs(5),
            || MultiRenameError::TimedOut,
            |detail| MultiRenameError::Internal { detail },
            move || {
                // A row whose file vanished since the preview is left out, not failed.
                Ok(ready
                    .iter()
                    .filter_map(|row| {
                        SourceFingerprint::capture_local(&dir.join(&row.old_name))
                            .map(|fingerprint| bulk_row(&dir, row, fingerprint))
                    })
                    .collect())
            },
        )
        .await?
    } else {
        let volume = resolved.volume.ok_or_else(|| MultiRenameError::NotConnected {
            volume_id: volume_id.clone(),
        })?;
        remote_rows(volume, &volume_id, dir, ready).await?
    };
    if bulk.is_empty() {
        return Err(MultiRenameError::NothingToRename);
    }
    let rows = bulk.len();
    let started = start_renames(events, volume_id, bulk, Initiator::User)
        .await
        .map_err(|reason| MultiRenameError::CouldntStart { reason })?;
    Ok(MultiRenameStarted {
        operation_id: started.operation.operation_id,
        renaming: rows.saturating_sub(started.swaps_left_out),
        swaps_left_out: started.swaps_left_out,
    })
}

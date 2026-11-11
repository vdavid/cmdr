//! Apply (Start): renames the session's rows that are `Ready` NOW, if they're
//! exactly the ones the user's preview showed (`MultiRename::prepare`), through the
//! bulk-rename executor Ask Cmdr uses (`start_renames`): chains, swaps, and
//! case-only renames, a fingerprint recheck before each step, every step
//! journaled, so the operation log's undo reverses it. Where the rename lands
//! (read-only route, local disk, a volume) is the host's answer.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};

use cmdr_fs::deadline::{Deadline, blocking_typed_result_with_timeout_on, timeout_detached_within};
use cmdr_fs::volume::Volume;
use cmdr_operation_log::types::Initiator;
use cmdr_operations::{BulkRenameRow, OperationEventSink, SourceFingerprint};

use crate::error::MultiRenameError;
use crate::host::RenamePlace;
use crate::plan::PreviewRow;
use crate::rule_kinds::rule_tokens;
use crate::session::{MultiRename, Prepared};

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
/// live session gets more, see `MultiRenameHost::io_budget`).
const REMOTE_FINGERPRINT_TIMEOUT: Duration = Duration::from_secs(30);

/// How many remote fingerprints apply reads at once, like `start_renames` asks
/// `Volume::rename_work` (`RENAME_WORK_CONCURRENCY`).
const FINGERPRINT_CONCURRENCY: usize = 8;

/// The deadline of each of apply's blocking steps: the proof, and the local fingerprints.
const BLOCKING_STEP_TIMEOUT: Duration = Duration::from_secs(5);

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
    budget: Duration,
    dir: PathBuf,
    ready: Vec<PreviewRow>,
) -> Result<Vec<BulkRenameRow>, MultiRenameError> {
    let deadline = Deadline::new(budget);
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

impl MultiRename {
    /// Renames the rows preview `preview_id` showed as ready, if they're still
    /// exactly that. Returns once the operation has started; progress, the queue,
    /// and Undo follow it by its id.
    ///
    /// Every apply, refused or started, reports once to the host
    /// ([`MultiRenameHost::rename_applied`](crate::MultiRenameHost::rename_applied)),
    /// from here: the one exit every caller (the sheet, the MCP tool) passes.
    pub async fn apply(
        &self,
        events: Arc<dyn OperationEventSink>,
        session_id: String,
        preview_id: u64,
    ) -> Result<MultiRenameStarted, MultiRenameError> {
        let rules = self
            .spec_of(&session_id, preview_id)
            .map(|spec| rule_tokens(&spec))
            .unwrap_or_default();
        let result = self.apply_inner(events, session_id, preview_id).await;
        self.host().rename_applied(&rules, &result);
        result
    }

    async fn apply_inner(
        &self,
        events: Arc<dyn OperationEventSink>,
        session_id: String,
        preview_id: u64,
    ) -> Result<MultiRenameStarted, MultiRenameError> {
        // Off the caller's thread: the listing lookup and a mask and regex per row.
        let engine = self.clone();
        let prepared = blocking_typed_result_with_timeout_on(
            self.runtime(),
            BLOCKING_STEP_TIMEOUT,
            || MultiRenameError::TimedOut,
            |detail| MultiRenameError::Internal { detail },
            move || engine.prepare(&session_id, preview_id),
        )
        .await?;

        // An archive or a `.git` portal serves its folder read-only. A ROW of a repo's
        // `.git/` listing can be the portal's too: its `worktrees` and `branches` rows
        // stand in for git's real folders at the same path, which the batch would rename.
        let place = self
            .host()
            .rename_place(&prepared.volume_id, &prepared.dir, &prepared.ready)
            .await;

        let Prepared { volume_id, dir, ready } = prepared;
        let bulk: Vec<BulkRenameRow> = match place {
            RenamePlace::ReadOnly(place) => return Err(MultiRenameError::ReadOnly { place }),
            RenamePlace::NotConnected => return Err(MultiRenameError::NotConnected { volume_id }),
            RenamePlace::LocalDisk => {
                blocking_typed_result_with_timeout_on(
                    self.runtime(),
                    BLOCKING_STEP_TIMEOUT,
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
            }
            RenamePlace::Volume(volume) => {
                let budget = self.host().io_budget(&volume_id, REMOTE_FINGERPRINT_TIMEOUT);
                remote_rows(volume, budget, dir, ready).await?
            }
        };
        if bulk.is_empty() {
            return Err(MultiRenameError::NothingToRename);
        }
        let rows = bulk.len();
        let engine = self.host().operations()?;
        let started = engine
            .start_renames(events, volume_id, bulk, Initiator::User)
            .await
            .map_err(|reason| MultiRenameError::CouldntStart { reason })?;
        Ok(MultiRenameStarted {
            operation_id: started.operation.operation_id,
            renaming: rows.saturating_sub(started.swaps_left_out),
            swaps_left_out: started.swaps_left_out,
        })
    }
}

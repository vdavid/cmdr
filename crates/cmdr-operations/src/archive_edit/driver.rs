//! The generic archive-edit driver: runs a pre-resolved [`Changeset`] as a
//! managed op (via [`super::engine::run_managed_edit`]), emits the terminal event,
//! and — for an into-archive move — deletes the local sources after the commit.
//! Also the thin in-archive delete route that builds a `{ delete }` changeset and
//! hands it to that driver. The create/rename instant-op forks and
//! `Operations::route_archive_delete` both feed this path.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use cmdr_operation_log::types::OperationId;

use super::super::OperationEventSink;
use super::super::manager::{ManagedTaskGuard, OperationDescriptor, OperationSummaryText};
use super::super::state::WriteSettledGuard;
use super::super::types::ReadOnlySide;
use super::super::types::{WriteOperationError, WriteOperationStartResult, WriteOperationType};
use super::edit_error::EditError;
use super::engine::{EditLeftovers, MutatorHooks, emit_archive_terminal, run_managed_edit, to_write_error};
use super::routing::{ensure_zip_writable, normalize_inner_path, read_only_error};
use cmdr_archive::mutator::{self, Changeset, MutationError};
use cmdr_fs::volume::LaneKey;

/// Everything the driver needs to run one archive edit.
pub struct ArchiveEditRequest {
    /// Absolute path of the `.zip` being edited (on its parent drive).
    pub archive_path: PathBuf,
    /// The parent drive's volume id: the op's lane key comes from it (archive
    /// work shares the drive's serialization lane), and it's the id marked busy
    /// so the drive can't be ejected mid-edit. `"root"` for a local disk.
    pub parent_volume_id: String,
    /// The resolved batch of changes (conflicts already folded in).
    pub changeset: Changeset,
    /// Queue-window summary (e.g. the added item's name, "Delete note.txt").
    pub summary: OperationSummaryText,
    /// How many source entries the changeset couldn't represent and skipped (a
    /// conflict resolved to Skip, or a symlink / special file a zip can't hold).
    /// Reported as `files_skipped` on the terminal event so the user isn't
    /// silently surprised, and (for a move) any skip suppresses the source
    /// deletion. Zero for delete / mkdir / mkfile / rename edits.
    pub skipped_count: usize,
    /// The confirming dialog's scan preview, when one fed this edit (an
    /// in-archive delete counts its entries first). This changeset is already
    /// planned, so the wait buys no reuse — it buys the preview an OWNER, so an
    /// abandoned walk stops instead of finishing for nobody.
    pub preview_id: Option<String>,
}

impl crate::Operations {
    /// Routes an in-archive delete (one or more entries inside the SAME zip) to the
    /// managed edit driver as a single `{ delete }` changeset. All sources must
    /// resolve to one archive (a multi-select can't span archives in one pane).
    /// `parent_volume_id` is the display drive id (`"root"` for a local zip).
    pub(crate) async fn route_archive_delete(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: &[PathBuf],
        parent_volume_id: &str,
        progress_interval_ms: u64,
        preview_id: Option<String>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        let first = sources.first().ok_or_else(|| WriteOperationError::IoError {
            path: String::new(),
            message: "no entries to delete".to_string(),
        })?;
        let (archive_path, _) = cmdr_fs::archive_format::archive_boundary_candidate(first)
            .ok_or_else(|| read_only_error(first, ReadOnlySide::Destination))?;
        ensure_zip_writable(&archive_path, ReadOnlySide::Destination)?;

        let mut deletes = Vec::with_capacity(sources.len());
        for source in sources {
            let (source_archive, inner) = cmdr_fs::archive_format::archive_boundary_candidate(source)
                .ok_or_else(|| read_only_error(source, ReadOnlySide::Destination))?;
            if source_archive != archive_path {
                return Err(WriteOperationError::IoError {
                    path: source.display().to_string(),
                    message: "can't delete entries from more than one archive at once".to_string(),
                });
            }
            deletes.push(normalize_inner_path(&inner));
        }

        let summary_source = deletes.first().map(|d| d.rsplit('/').next().unwrap_or(d).to_string());
        let request = ArchiveEditRequest {
            archive_path,
            parent_volume_id: parent_volume_id.to_string(),
            changeset: Changeset {
                deletes,
                ..Default::default()
            },
            summary: OperationSummaryText {
                source: summary_source,
                destination: None,
                items: None,
            },
            skipped_count: 0,
            preview_id,
        };
        self.archive_edit_start(events, request, progress_interval_ms).await
    }

    /// Starts an archive edit as a managed operation, returning its id immediately
    /// (the queue row shows at once; the mutator runs when the parent lane is free).
    pub(crate) async fn archive_edit_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        request: ArchiveEditRequest,
        progress_interval_ms: u64,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        let operation_id = OperationId::from(cmdr_operation_log::new_operation_id());
        let state = Arc::new(self.new_operation_state(Duration::from_millis(progress_interval_ms)));

        // Share the parent drive's lane so the op serializes against other work on
        // that device (falls back to the id itself if the volume isn't registered —
        // the mutator's own open will surface a real error on a truly-gone drive).
        let lane = state
            .scope()
            .host()
            .volumes()
            .get(&request.parent_volume_id)
            .map(|v| v.lane_key())
            .unwrap_or_else(|| LaneKey::new(request.parent_volume_id.clone()));

        let descriptor = OperationDescriptor {
            operation_id: operation_id.clone(),
            operation_type: WriteOperationType::ArchiveEdit,
            lanes: vec![lane],
            // Mark the parent drive busy while editing (the manager drops `root`).
            volume_ids: vec![request.parent_volume_id.clone()],
            summary: request.summary,
            // A zip edit is a whole-archive temp+rename rewrite: it either lands or
            // it doesn't, so there's no half-written state to reverse.
            rollback_support: crate::types::RollbackSupport::ArchiveRewrite,
            preview_id: request.preview_id.clone(),
            reverses: None,
        };

        let events_for_op = Arc::clone(&events);
        let op_id_outer = operation_id.clone();
        let state_for_op = Arc::clone(&state);
        let progress_interval = Duration::from_millis(progress_interval_ms);
        let ArchiveEditRequest {
            archive_path,
            parent_volume_id,
            changeset,
            skipped_count,
            ..
        } = request;

        let deferred = move |ops: crate::Operations| -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async move {
                let events = events_for_op;
                let op_id = op_id_outer;
                let state = state_for_op;
                let task_guard = ManagedTaskGuard::new(ops.manager(), op_id.clone());
                // `write-settled` fires at end of scope (after the terminal event and
                // `on_settled`). `root` local edits carry no volume id (matches the
                // local-op convention).
                let settle_volume = (parent_volume_id != "root").then(|| parent_volume_id.clone());
                let _settled = WriteSettledGuard::new(
                    Arc::clone(&events),
                    op_id.to_string(),
                    WriteOperationType::ArchiveEdit,
                    settle_volume,
                );

                // Wait out the confirming dialog's scan; see
                // `cmdr_operations::start_write_operation`.
                if crate::scan_bridge::await_claimed_preview(
                    &ops,
                    &*events,
                    &op_id,
                    WriteOperationType::ArchiveEdit,
                    &state,
                )
                .await
                .stopped()
                {
                    task_guard.disarm();
                    ops.manager().on_settled(&op_id);
                    return;
                }

                let hooks = Arc::new(MutatorHooks::new(
                    Arc::clone(&state),
                    Arc::clone(&events),
                    op_id.to_string(),
                    WriteOperationType::ArchiveEdit,
                    progress_interval,
                ));

                let hooks_for_blocking = Arc::clone(&hooks);
                let result = run_managed_edit(
                    &parent_volume_id,
                    archive_path.clone(),
                    Arc::clone(&state),
                    hooks.remote_progress_observer(),
                    move |working: &Path| {
                        mutator::apply(working, &changeset, &*hooks_for_blocking).map_err(|e| match e {
                            MutationError::Cancelled => EditError::Cancelled,
                            other => EditError::Op(to_write_error(working, other)),
                        })
                    },
                )
                .await;

                let final_progress = hooks.latest_progress();
                emit_archive_terminal(
                    events.as_ref(),
                    op_id.as_str(),
                    WriteOperationType::ArchiveEdit,
                    result,
                    skipped_count,
                    EditLeftovers::of(&state, &parent_volume_id, None),
                    &final_progress,
                );

                task_guard.disarm();
                ops.manager().on_settled(&op_id);
            })
        };

        self.manager().spawn_managed(descriptor, state, Box::new(deferred));

        Ok(WriteOperationStartResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::ArchiveEdit,
        })
    }
}

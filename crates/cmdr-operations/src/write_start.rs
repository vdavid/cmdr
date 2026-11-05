//! The same-`root` move, delete, and trash starts every host calls: each asks
//! the host whether `root` is its local disk and hands the operation to the
//! local engine (`local_start.rs`) when it is, else to the volume engine. A
//! host with no local disk (the browser demo, where `local_start.rs` isn't
//! compiled) takes the volume path every time. Each is an [`Operations`] method.

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::LaneKey;
use cmdr_operation_log::types::Initiator;

#[cfg(not(target_family = "wasm"))]
use super::leaf_validation::{LeafIdentity, validate_transfer_leaves};
use super::routed_writes::{first_routed, routed_transfer_refusal};
use super::source_binding::ExpectedSources;
use super::types::{
    ReadOnlySide, VolumeCopyConfig, WriteOperationConfig, WriteOperationError, WriteOperationStartResult,
};
use super::{OperationEventSink, Operations, transfer_sides};

impl Operations {
    /// Starts a move operation in the background: the same-`root` start. On the
    /// local disk it validates the config's leaves before anything is admitted
    /// (`leaf_validation::validate_transfer_leaves`), then runs the local move.
    ///
    /// `expected_sources` is the caller's source binding, or `None` for the ordinary
    /// user-started case. `source_binding.rs`.
    #[allow(
        clippy::too_many_arguments,
        reason = "the manager's context alongside the op's own inputs"
    )]
    pub async fn move_files_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: Vec<PathBuf>,
        destination: PathBuf,
        config: WriteOperationConfig,
        volume_ids: Vec<String>,
        lanes: Option<Vec<LaneKey>>,
        initiator: Initiator,
        expected_sources: Option<ExpectedSources>,
        sides: Option<transfer_sides::TransferSides>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        let root = cmdr_fs::volume::DEFAULT_VOLUME_ID;
        #[cfg(not(target_family = "wasm"))]
        if self.host().volumes().local_disk(root).is_some() {
            validate_transfer_leaves(&config.leaves, &sources, &destination, LeafIdentity::NamesAndInodes)?;
            return self
                .local_move_start(
                    events,
                    sources,
                    destination,
                    config,
                    volume_ids,
                    lanes,
                    initiator,
                    expected_sources,
                    sides,
                )
                .await;
        }
        // A host with no local disk (the browser demo) has no `std::fs` engine to
        // run this on: its `root` is a volume, so the move is a volume move.
        let _ = (volume_ids, lanes, sides);
        self.start_volume_move(
            events,
            root.into(),
            sources,
            root.into(),
            destination.display().to_string(),
            VolumeCopyConfig::from(&config),
            initiator,
            expected_sources,
        )
        .await
    }

    /// Starts a delete operation in the background.
    ///
    /// Recursively deletes files and directories. When `volume_id` is provided and
    /// is not the default volume, routes through `delete_volume_files_with_progress`
    /// which uses the Volume trait (needed for MTP and other non-local volumes).
    /// `expected_sources` is the caller's source binding, or `None` for the ordinary
    /// user-started case. `source_binding.rs`.
    pub async fn delete_files_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: Vec<PathBuf>,
        config: WriteOperationConfig,
        volume_id: Option<String>,
        initiator: Initiator,
        expected_sources: Option<ExpectedSources>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        let volume_id_str = volume_id.unwrap_or_else(|| "root".to_string());

        log::info!(
            "delete_files_start: sources={:?}, volume={}, dry_run={}",
            sources.iter().map(LogPrivate::path).collect::<Vec<_>>(),
            volume_id_str,
            config.dry_run
        );

        // Never the person's own home folder, nor a folder holding it (`home_folder.rs`).
        if let Some(hit) = self.home_folder_hit(&volume_id_str, &sources) {
            return Err(WriteOperationError::HomeFolder {
                path: hit.path,
                relation: hit.relation,
            });
        }

        // Deleting entries INSIDE a zip is a mutation: route to the managed archive-edit
        // driver as a single `{ delete }` changeset (a rewrite, not per-entry). The
        // `.zip` file itself is a regular file — deleting it stays on the normal path.
        // Parent-aware detection (not the `std::fs`-only sync predicate) so a delete
        // inside a REMOTE zip (direct SMB / MTP) also reaches the driver instead of
        // falling through to a confusing parent-volume delete.
        let first_is_archive_inner = match sources.first() {
            Some(s) => self.host().volumes().path_is_inside_archive(&volume_id_str, s).await,
            None => false,
        };
        if first_is_archive_inner {
            return self
                .route_archive_delete(
                    events,
                    &sources,
                    &volume_id_str,
                    config.progress_interval_ms,
                    config.preview_id.clone(),
                )
                .await;
        }

        // Anything else a route serves (a repo's `.git/<category>/` trees, or an in-zip
        // path behind a first source that isn't one) is read-only, and the drive
        // underneath would delete a real entry (`routed_writes.rs`).
        if let Some((routed, _)) = first_routed(self.host(), &volume_id_str, sources.iter().map(PathBuf::as_path)).await
        {
            return Err(routed_transfer_refusal(&routed, ReadOnlySide::Source));
        }

        #[cfg(not(target_family = "wasm"))]
        if self.host().volumes().local_disk(&volume_id_str).is_some() {
            return self
                .local_delete_start(events, sources, config, initiator, expected_sources)
                .await;
        }
        // Volume-aware delete: its body is async (the `Volume` trait's I/O is), so
        // it owns its own deferred start rather than riding `start_write_operation`.
        // `delete/volume_start.rs`.
        Ok(self.start_volume_delete(events, sources, config, volume_id_str, initiator, expected_sources))
    }

    /// Starts a trash operation in the background.
    ///
    /// Moves top-level items to the macOS Trash via `NSFileManager.trashItemAtURL`.
    /// Supports cancellation between items and partial failure (some items may fail
    /// while others succeed).
    /// `expected_sources` is the caller's source binding, or `None` for the ordinary
    /// user-started case. `source_binding.rs`.
    ///
    /// ⚠️ `item_sizes` is positional, so the binding filters both halves together.
    pub async fn trash_files_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: Vec<PathBuf>,
        item_sizes: Option<Vec<u64>>,
        config: WriteOperationConfig,
        initiator: Initiator,
        expected_sources: Option<ExpectedSources>,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        log::info!(
            "trash_files_start: sources={:?}",
            sources.iter().map(LogPrivate::path).collect::<Vec<_>>()
        );

        // Never the person's own home folder, nor a folder holding it
        // (`home_folder.rs`). Before the preview is freed: the dialog that asked
        // is still up and owns it.
        if let Some(hit) = self.home_folder_hit(cmdr_fs::volume::DEFAULT_VOLUME_ID, &sources) {
            return Err(WriteOperationError::HomeFolder {
                path: hit.path,
                relation: hit.relation,
            });
        }

        // ❌ Trash does NOT wait for the confirming dialog's scan preview, and this
        // is the one operation that doesn't. `trashItemAtURL` is atomic per
        // top-level item: trash walks nothing, so there is no second walk to
        // serialize against and no cached result to consume — waiting would be pure
        // delay, and on a big tree a long one. Nothing downstream will consume the
        // preview either, so free it here rather than leaving an ownerless walk to
        // finish for nobody and its result to sit until a TTL sweep. (The dialog
        // can't free it: it sets `confirmed` and deliberately skips its own
        // cleanup, because on the DELETE path the operation does consume it.)
        if let Some(preview_id) = &config.preview_id {
            self.cancel_scan_preview(preview_id);
        }

        // A routed path is read-only, as for delete (`routed_writes.rs`).
        if let Some((routed, _)) = first_routed(
            self.host(),
            cmdr_fs::volume::DEFAULT_VOLUME_ID,
            sources.iter().map(PathBuf::as_path),
        )
        .await
        {
            return Err(routed_transfer_refusal(&routed, ReadOnlySide::Source));
        }

        #[cfg(not(target_family = "wasm"))]
        if self
            .host()
            .volumes()
            .local_disk(cmdr_fs::volume::DEFAULT_VOLUME_ID)
            .is_some()
        {
            return self
                .local_trash_start(events, sources, item_sizes, config, initiator, expected_sources)
                .await;
        }
        // The OS trash is the local disk's; a host with none (the browser demo) has
        // no Trash, which is exactly what a share without one answers.
        let _ = (events, item_sizes, initiator, expected_sources);
        Err(WriteOperationError::TrashNotSupported {
            path: sources.first().map(|p| p.display().to_string()).unwrap_or_default(),
        })
    }
}

//! Fresh ZIP routing. The managed seedless creator owns creation; existing ZIP
//! mutation continues through the archive mutator's separate routes.

use std::path::PathBuf;
use std::sync::Arc;

use super::super::OperationEventSink;
use super::super::look_alike::{LookAlike, look_alike_in, spelled_new_path};
use super::super::transfer::volume::{PathRole, map_volume_error};
use super::super::types::{ConflictResolution, ReadOnlySide, WriteOperationError, WriteOperationStartResult};
use cmdr_fs::volume::Volume;

/// Picks the concrete remote spelling that the overwrite warning referred to.
/// One equivalent spelling is the target; several are ambiguous and refused.
async fn archive_landing(parent: &dyn Volume, target: PathBuf) -> Result<(PathBuf, bool), WriteOperationError> {
    if parent.exists(&target).await {
        return Ok((target, true));
    }
    let (Some(dir), Some(name)) = (target.parent(), target.file_name().and_then(|name| name.to_str())) else {
        return Ok((target, false));
    };
    match look_alike_in(parent, dir, name).await {
        Ok(LookAlike::None) => Ok((spelled_new_path(parent, &target), false)),
        Ok(LookAlike::One(entry)) => Ok((dir.join(entry.name), true)),
        Ok(LookAlike::Several) => Err(WriteOperationError::DestinationExists {
            path: target.display().to_string(),
        }),
        Err(error) => Err(map_volume_error(
            &target.display().to_string(),
            PathRole::Destination,
            error,
        )),
    }
}

impl crate::Operations {
    /// Starts one managed fresh compression. Nothing at the destination changes
    /// until the producer has closed, byte counts agree, and the staged ZIP parses.
    #[allow(
        clippy::too_many_arguments,
        reason = "command seam carries both endpoints, operation settings, preview ownership, and provenance"
    )]
    pub(crate) async fn compress_start(
        &self,
        events: Arc<dyn OperationEventSink>,
        source_volume: Arc<dyn Volume>,
        source_paths: Vec<PathBuf>,
        dest_zip_full_path: PathBuf,
        parent_volume_id: String,
        conflict: ConflictResolution,
        progress_interval_ms: u64,
        compression_level: Option<i64>,
        preview_id: Option<String>,
        initiator: cmdr_operation_log::types::Initiator,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        super::routing::ensure_zip_writable(&dest_zip_full_path, ReadOnlySide::Destination)?;
        // Plain `get`: the `.zip` file itself is a plain file on its parent drive.
        let (dest_zip_full_path, existed) = match self.host().volumes().get(&parent_volume_id) {
            Some(parent) if !parent.supports_local_fs_access() => {
                archive_landing(parent.as_ref(), dest_zip_full_path).await?
            }
            Some(parent) => {
                let existed = parent.exists(&dest_zip_full_path).await;
                (dest_zip_full_path, existed)
            }
            None => {
                let probe_path = dest_zip_full_path.clone();
                let existed = self
                    .host()
                    .runtime()
                    .spawn_blocking(move || std::fs::symlink_metadata(probe_path).is_ok())
                    .await
                    .map_err(|error| WriteOperationError::IoError {
                        path: dest_zip_full_path.display().to_string(),
                        message: error.to_string().into(),
                    })?;
                (dest_zip_full_path, existed)
            }
        };
        super::fresh_compress::start(
            self,
            events,
            source_volume,
            source_paths,
            dest_zip_full_path,
            parent_volume_id,
            conflict,
            progress_interval_ms,
            compression_level,
            preview_id,
            !existed,
            initiator,
        )
        .await
    }
}

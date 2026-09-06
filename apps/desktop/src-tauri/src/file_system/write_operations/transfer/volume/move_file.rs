//! Moving ONE file across two volumes, with no conflict resolution and no
//! operation driver above it.
//!
//! The volume move (`r#move.rs`) is an operation-level driver: it scans sources,
//! resolves conflicts, walks trees, and journals. A caller that has already
//! decided about exactly one leaf — the operation-log rollback restoring one
//! journaled file to where it came from — needs the layer under that, and this is
//! it: the same staged, retrying, mid-file-cancelable streaming every cross-volume
//! copy uses, plus the source-side removal that makes it a move.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;

use super::super::super::state::WriteOperationState;
use super::preflight::SourceFileFacts;
use super::strategy::{WriteStaging, stream_pipe_file};
use crate::file_system::volume::{Volume, VolumeError};

/// Move one file from `source_volume` to `dest_volume`, streaming its bytes and
/// then removing the source side. Returns the bytes transferred.
///
/// **The order is the data-safety property.** The bytes land whole at the
/// destination (staged on a `.cmdr-tmp-*` sibling and renamed into place) before
/// anything is removed from the source, so a stop mid-file leaves the source
/// exactly as it was and no partial at the destination — the caller's `Break` on
/// `on_file_progress` reaches the backend, which drops its own handle, and the
/// staging layer abandons the temp. Whichever side holds the file holds all of
/// it.
///
/// The destination is assumed CLEAR: nothing here merges, resolves, or
/// overwrites. The caller establishes that (the rollback engine's pinned
/// never-overwrite recheck does).
///
/// Cancel and pause both ride the op's `state`: cancel through
/// `on_file_progress`, pause between chunks via the `CheckpointStream` inside
/// [`stream_pipe_file`], so a paused reversal parks mid-file instead of streaming
/// a large file to completion first.
pub(in crate::file_system::write_operations) async fn move_file_across_volumes(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
) -> Result<u64, VolumeError> {
    let bytes = stream_pipe_file(
        source_volume,
        source_path,
        // No size hint: the stream reports the REAL length, so a source whose
        // listed metadata size lies still moves its true bytes. No mode either,
        // so a local destination spends one stat on it after the bytes land —
        // the file this restores is the user's, and it has to come back wearing
        // what it wore.
        SourceFileFacts::default(),
        dest_volume,
        dest_path,
        state,
        on_file_progress,
        // Nothing was pre-staged for us, so the temp-and-land is ours to do.
        WriteStaging::Stage,
    )
    .await
    // `WriteStaging::Stage` means `LandingName::ExpectedFree`, and a landing that
    // expected a free name never clears anything, so it never rescues anything
    // either: `new_data_at` is `None` on every failure this call can produce.
    .map_err(|f| f.error)?;
    source_volume.delete(source_path).await?;
    Ok(bytes)
}

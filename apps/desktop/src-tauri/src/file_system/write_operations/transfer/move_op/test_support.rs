//! The rig both local-move suites drive the engines through: an operation state
//! with a real progress throttle, and one call-through per engine so a test can
//! name the move kind it means.

use super::cross_fs::move_with_staging;
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::ConflictResolution;

pub(super) fn make_state(progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(std::time::Duration::from_millis(
        progress_interval_ms,
    )))
}

/// Drives a same-FS local move (`move_with_rename`) of `sources` into `dst_dir`
/// under the given resolution. Within one tempdir, source and dest share a
/// filesystem, so `move_files_with_progress_inner` routes to the rename path.
pub(super) fn run_same_fs_move(
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig {
        conflict_resolution: resolution,
        ..WriteOperationConfig::default()
    };
    move_files_with_progress_inner(&*events, op_id, &state, sources, dst_dir, &config)?;
    Ok(events)
}

/// Drives a cross-FS local move (`move_with_staging`) of `sources` into
/// `dst_dir` under the given resolution.
pub(super) fn run_cross_fs_move(
    sources: &[PathBuf],
    dst_dir: &Path,
    resolution: ConflictResolution,
    op_id: &str,
) -> Result<Arc<CollectorEventSink>, WriteOperationError> {
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig {
        conflict_resolution: resolution,
        ..WriteOperationConfig::default()
    };
    move_with_staging(&*events, op_id, &state, sources, dst_dir, &config, 0)?;
    Ok(events)
}

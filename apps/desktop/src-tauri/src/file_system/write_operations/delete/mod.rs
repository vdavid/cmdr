//! Delete and trash operations, both local-FS and volume-aware.
//!
//! The local-FS walker uses `walkdir` + `fs::remove_file`. The volume-aware
//! variant uses the `Volume` trait so MTP / SMB / future remote backends
//! work the same way. Trash routes to the OS-native trash (macOS
//! `trashItemAtURL`, Linux `trash` crate).
//!
//! See `CLAUDE.md` in this directory for delete walker semantics, the
//! oracle-aware fast path, trash, and the volume-delete preview-reuse path.

pub(crate) mod trash;
mod walker;

pub(in crate::file_system::write_operations) use walker::{
    delete_files_with_progress_inner, delete_volume_files_with_progress_inner,
};

/// The volume delete driver, reachable from the SMB integration suite, which
/// sits outside `write_operations` and drives the real walker rather than the
/// IPC command.
///
/// A thin `pub(crate)` wrapper rather than a re-export: the walker function
/// itself stays `pub(in write_operations)`, so nothing in production gains a
/// wider path to it.
#[cfg(test)]
pub(crate) async fn delete_volume_files_for_test(
    volume: std::sync::Arc<dyn crate::file_system::volume::Volume>,
    volume_id: &str,
    events: &dyn super::types::OperationEventSink,
    operation_id: &str,
    state: &std::sync::Arc<super::state::WriteOperationState>,
    sources: &[std::path::PathBuf],
    config: &super::types::WriteOperationConfig,
) -> Result<(), super::types::WriteOperationError> {
    delete_volume_files_with_progress_inner(volume, volume_id, events, operation_id, state, sources, config).await
}

#[cfg(test)]
mod delete_integration_test;
#[cfg(test)]
mod delete_volume_reuse_tests;
#[cfg(test)]
mod hardlink_progress_tests;
#[cfg(test)]
mod preview_binding_tests;
#[cfg(test)]
mod volume_cancel_tests;
#[cfg(test)]
mod volume_hardlink_progress_tests;

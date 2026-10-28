//! Delete and trash operations, both local-FS and volume-aware.
//!
//! The local-FS walker uses `walkdir` + `fs::remove_file`. The volume-aware
//! variant uses the `Volume` trait so MTP / SMB / future remote backends
//! work the same way. Trash routes to the OS-native trash (macOS
//! `trashItemAtURL`, Linux `trash` crate).
//!
//! See `CLAUDE.md` in this directory for delete walker semantics, the
//! oracle-aware fast path, trash, and the volume-delete preview-reuse path.

pub(crate) mod cloud_trash;
mod outage;
pub(crate) mod trash;
mod volume_scan;
mod volume_start;
mod volume_walker;
mod walker;

pub(crate) use volume_walker::delete_volume_files_with_progress_inner;
pub(crate) use walker::delete_files_with_progress_inner;

/// The volume delete driver, reachable from the SMB integration suite, which
/// sits outside `cmdr-operations` and drives the real walker rather than the
/// IPC command.
///
/// A thin `pub(crate)` wrapper rather than a re-export: the walker function
/// itself stays `pub(crate)`, so nothing in production gains a
/// wider path to it.
#[cfg(any(test, feature = "testing"))]
pub async fn delete_volume_files_for_test(
    volume: std::sync::Arc<dyn cmdr_fs::volume::Volume>,
    volume_id: &str,
    events: &dyn super::event_sinks::OperationEventSink,
    operation_id: &str,
    state: &std::sync::Arc<super::state::WriteOperationState>,
    sources: &[std::path::PathBuf],
    config: &super::types::WriteOperationConfig,
) -> Result<(), super::types::WriteOperationError> {
    delete_volume_files_with_progress_inner(volume, volume_id, events, operation_id, state, sources, config).await
}

/// What a cancel does to a delete, and what the cancelled event admits.
#[cfg(test)]
mod delete_cancel_tests;
#[cfg(test)]
mod delete_integration_test;
/// A delete on a share that drops mid-delete waits for it, re-checking each entry.
#[cfg(test)]
mod delete_outage_tests;
#[cfg(test)]
mod delete_volume_reuse_tests;
#[cfg(test)]
mod hardlink_progress_tests;
/// A volume delete removes a symbolic link and never walks into its target (#505).
#[cfg(test)]
mod link_delete_tests;
#[cfg(test)]
mod preview_binding_tests;
// The nested-mount pins (macOS, `#[ignore]`d, run by `pnpm check disk-images`). The
// `macos` gate is load-bearing: the harness is macOS-only.
#[cfg(all(test, target_os = "macos"))]
mod real_image;
#[cfg(test)]
mod trash_pause_tests;
#[cfg(test)]
mod volume_cancel_tests;
#[cfg(test)]
mod volume_hardlink_progress_tests;
/// A volume delete reports each top-level source as it finishes.
#[cfg(test)]
mod volume_source_done_tests;
#[cfg(test)]
mod volume_symlink_tests;

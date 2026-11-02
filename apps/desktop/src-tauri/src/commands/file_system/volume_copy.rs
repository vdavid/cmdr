//! Tauri commands for cross-volume copy/move operations.
//!
//! These are pass-throughs: they build the `TauriEventSink` at the edge and hand
//! the whole routed transfer to `cmdr_operations::routing`, which owns the volume
//! and destination-path resolution and the archive forks. That split is what lets
//! a backend caller start the same transfer with its own injected sink.

use crate::file_system::ScanConflict;
use crate::operations_host::TauriEventSink;
use crate::operations_host::{NotInstalled, operations, refuse_not_installed};
use cmdr_operations::{
    CONFLICT_CHECK_BUDGET, OperationEventSink, Operations, SourceItemInput, VolumeCopyConfig, VolumeScanError,
    WriteOperationError, WriteOperationStartResult, resolve_dest_path,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::time::Duration;

use crate::file_system::volume::manager::get_volume_manager;
use cmdr_fs::deadline::{Deadline, timeout_detached_typed};
use cmdr_operation_log::types::Initiator;

/// Unified copy across volume types (local, MTP, extract out of a `.zip`).
/// Emits write-progress, write-complete, write-error, write-cancelled.
#[tauri::command]
#[specta::specta]
pub async fn copy_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Initiator,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    copy_between_volumes_on(
        operations(),
        events,
        source_volume_id,
        source_paths,
        dest_volume_id,
        dest_path,
        config,
        initiator,
    )
    .await
}

/// [`copy_between_volumes`] on the engine `engine` answers, with the sink passed in.
#[allow(
    clippy::too_many_arguments,
    reason = "the command's own arguments plus its engine and sink"
)]
async fn copy_between_volumes_on(
    engine: Result<&'static Operations, NotInstalled>,
    events: Arc<dyn OperationEventSink>,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Initiator,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Before `install` there is no engine to start on: refuse, never write.
    let engine = engine.map_err(refuse_not_installed)?;
    engine
        .start_volume_copy(
            events,
            source_volume_id,
            source_paths.iter().map(PathBuf::from).collect(),
            dest_volume_id,
            dest_path,
            config.unwrap_or_default(),
            initiator,
            // No source binding: the user picked these in the pane they are looking at.
            None,
        )
        .await
}

/// Unified move across volume types. Handles same-volume (native rename/move),
/// both-local (native move), cross-volume (copy+delete), and both directions
/// across a `.zip` boundary.
#[tauri::command]
#[specta::specta]
pub async fn move_between_volumes(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Initiator,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Before `install` there is no engine to start on: refuse, never write.
    let engine = operations().map_err(refuse_not_installed)?;
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    engine
        .start_volume_move(
            events,
            source_volume_id,
            source_paths.iter().map(PathBuf::from).collect(),
            dest_volume_id,
            dest_path,
            config.unwrap_or_default(),
            initiator,
            // No source binding: the user picked these in the pane they are looking at.
            None,
        )
        .await
}

/// A rename that runs as a move on one volume: `source_path` moves into
/// `dest_path` under `new_name`. What the Move dialog confirms when F2 opens
/// it for a rename that copies (an S3 folder past the small-rename count,
/// `RenameValidityResult::by_move`). Same events as `move_between_volumes`.
#[tauri::command]
#[specta::specta]
pub async fn rename_by_move(
    app: tauri::AppHandle,
    volume_id: String,
    source_path: String,
    dest_path: String,
    new_name: String,
    config: Option<VolumeCopyConfig>,
    initiator: Initiator,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Before `install` there is no engine to start on: refuse, never write.
    let engine = operations().map_err(refuse_not_installed)?;
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    engine
        .start_rename_by_move(
            events,
            volume_id,
            vec![(PathBuf::from(source_path), new_name)],
            dest_path,
            config.unwrap_or_default(),
            initiator,
            // No source binding: the user picked it in the pane they are looking at.
            None,
        )
        .await
}

/// Compresses `source_paths` into a NEW zip at `dest_zip_path` on `dest_volume_id`.
/// Same events as `copy_between_volumes`. The destination may be LOCAL or REMOTE
/// (SMB/MTP).
#[tauri::command]
#[specta::specta]
pub async fn compress_files(
    app: tauri::AppHandle,
    source_volume_id: String,
    source_paths: Vec<String>,
    dest_volume_id: String,
    dest_zip_path: String,
    config: Option<VolumeCopyConfig>,
    initiator: Initiator,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Before `install` there is no engine to start on: refuse, never write.
    let engine = operations().map_err(refuse_not_installed)?;
    let events: Arc<dyn OperationEventSink> = Arc::new(TauriEventSink::new(app));
    engine
        .start_volume_compress(
            events,
            source_volume_id,
            source_paths.iter().map(PathBuf::from).collect(),
            dest_volume_id,
            dest_zip_path,
            config.unwrap_or_default(),
            initiator,
        )
        .await
}

/// Whether the transfer dialog's destination folder takes writes, for the notice
/// under its path box. Resolved and anchored the way the copy op does it
/// (`resolve_dest_path` expands a local `~`), and bounded: a volume that isn't
/// registered (a phone nobody dialed) or doesn't answer in 2 s is `Unknown`, which
/// shows nothing. ❌ Never a refusal of its own: the transfer asks again before it
/// writes, and that answer is the one that refuses.
#[tauri::command]
#[specta::specta]
pub async fn destination_write_access(dest_volume_id: String, dest_path: String) -> cmdr_fs::volume::WriteAccess {
    use cmdr_fs::volume::WriteAccess;

    let Some(dest_volume) = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_path))
        .await
        .volume
    else {
        return WriteAccess::Unknown;
    };
    let dest_path = resolve_dest_path(&dest_volume, dest_path);
    // Detached, like the scan: dropping a probe mid-round-trip wedges an MTP phone.
    timeout_detached_typed(
        Duration::from_secs(2),
        || WriteAccess::Unknown,
        |_| WriteAccess::Unknown,
        async move { Ok::<_, WriteAccess>(dest_volume.write_access_at(&dest_path).await) },
    )
    .await
    .unwrap_or_else(|unknown| unknown)
}

/// The transfer dialog's destination when it starts with the place's own root
/// folder: both readings, for the warning under the path box. Paths are
/// server-side (`resolved`, `rootFolder`) or volume-relative (`stripped`, what
/// the box would hold instead). The rule: `cmdr_fs::volume::root_echo`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DestinationRootEcho {
    /// The server-side folder the place is rooted at (`/srv/data`).
    pub root_folder: String,
    /// Where the transfer goes as typed (`/srv/data/srv/data/photos`).
    pub resolved: String,
    /// The box's text with the repeated root folder taken off (`/photos`).
    pub stripped: String,
}

/// Whether the destination box's path repeats the place's own root folder, so
/// the dialog can warn. ❌ Never rewrites anything: the transfer still anchors
/// the path as typed (`resolve_dest_path`), because the doubled folder can be
/// real. `None` for an unregistered volume and for any path that reads one way.
#[tauri::command]
#[specta::specta]
pub async fn destination_root_echo(dest_volume_id: String, dest_path: String) -> Option<DestinationRootEcho> {
    let dest_volume = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_path))
        .await
        .volume?;
    let echo = cmdr_fs::volume::root_echo(dest_volume.root(), Path::new(&dest_path))?;
    Some(DestinationRootEcho {
        root_folder: echo.root_folder.to_string_lossy().into_owned(),
        resolved: echo.resolved.to_string_lossy().into_owned(),
        stripped: echo.stripped.to_string_lossy().into_owned(),
    })
}

/// Checks which source items already exist at the destination. Returns conflict details for UI.
///
/// When `source_volume_id` and `source_paths` are both provided, each item's
/// `is_directory` and `size` are resolved authoritatively on the source volume
/// via one `get_metadata` per top-level path (see `stat_source_paths`: strictly
/// O(top-level items), never a subtree walk), overriding whatever the caller
/// passed in `source_items`. This lets the dialog classify dir-vs-dir collisions
/// as silent merges without the FE having to plumb per-item types. Callers that
/// don't pass the source volume keep the legacy name-only behavior.
///
/// A thin wrapper: the budgeted scan itself is
/// `Operations::scan_volume_for_conflicts_within`,
/// which is what a test hands a budget it can wait out.
#[tauri::command]
#[specta::specta]
pub async fn scan_volume_for_conflicts(
    volume_id: String,
    source_items: Vec<SourceItemInput>,
    dest_path: String,
    source_volume_id: Option<String>,
    source_paths: Option<Vec<String>>,
) -> Result<Vec<ScanConflict>, VolumeScanError> {
    // Before `install` no volume is registered, so the destination isn't found.
    let Ok(engine) = operations() else {
        return Err(VolumeScanError::DestinationVolumeNotFound { volume_id });
    };
    engine
        .scan_volume_for_conflicts_within(
            Deadline::new(CONFLICT_CHECK_BUDGET),
            volume_id,
            source_items,
            dest_path,
            source_volume_id,
            source_paths,
        )
        .await
}

#[cfg(test)]
#[path = "destination_root_echo_test.rs"]
mod destination_root_echo_test;

#[cfg(test)]
mod tests {
    use std::sync::{Arc, OnceLock};

    use crate::test_support::TestDir;
    use cmdr_operations::testing::CollectorEventSink;
    use cmdr_operations::{OperationEventSink, Operations, WriteOperationError};

    /// A copy started before the engine is installed is refused with the
    /// generic I/O failure the frontend already renders, and writes nothing.
    #[tokio::test]
    async fn a_copy_before_install_is_refused_and_writes_nothing() {
        static NEVER_INSTALLED: OnceLock<Operations> = OnceLock::new();
        let dir = TestDir::new("copy-before-install");
        let source = dir.join("a.txt");
        std::fs::write(&source, b"a").expect("write the source");
        let destination = dir.join("dest");
        std::fs::create_dir(&destination).expect("make the destination");
        let events = Arc::new(CollectorEventSink::new());

        let refused = super::copy_between_volumes_on(
            crate::operations_host::operations_in(&NEVER_INSTALLED),
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            "root".to_string(),
            vec![source.display().to_string()],
            "root".to_string(),
            destination.display().to_string(),
            None,
            cmdr_operation_log::types::Initiator::User,
        )
        .await;

        assert!(
            matches!(refused, Err(WriteOperationError::IoError { .. })),
            "refused with the generic I/O failure, got {refused:?}"
        );
        assert!(!destination.join("a.txt").exists(), "nothing was written");
        use cmdr_fs::ignore_poison::IgnorePoison;
        assert!(
            events.settled.lock_ignore_poison().is_empty() && events.errors.lock_ignore_poison().is_empty(),
            "no operation started, so nothing was emitted"
        );
    }
}

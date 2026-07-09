//! Shared detection and path primitives every archive-edit route builds on: the
//! archive-boundary inner-path helpers, the zip-only write guard (the single
//! write-side chokepoint that refuses tar/7z), the duplicate-existence oracle the
//! create/rename pre-check consults, and the instant-op event-sink builder. The
//! per-flow routes (`copy_into`, `move_out`, `driver`) layer their planning and
//! drivers on top of these.

use std::path::Path;
use std::sync::Arc;

use super::super::OperationEventSink;
use super::super::manager;
use super::super::types::WriteOperationError;
use crate::file_system::get_volume_manager;
use crate::file_system::volume::backends::archive;
use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveIndex, LocalFileSource};

/// Builds a Tauri-backed event sink from the startup-wired app handle, for
/// routing an archive-target instant op (mkdir / mkfile / rename) to the managed
/// edit driver. `None` before the app handle is wired (unit tests), so callers
/// fall back to a plain refusal rather than silently dropping the op.
pub(crate) fn global_tauri_sink() -> Option<Arc<dyn OperationEventSink>> {
    manager::operations_app_handle()
        .map(|app| Arc::new(super::super::TauriEventSink::new(app)) as Arc<dyn OperationEventSink>)
}

/// Joins an archive-inner parent path and a new child name into a single
/// `/`-separated inner path (root-relative, no surrounding slashes).
pub(crate) fn join_inner_path(inner_parent: &Path, name: &str) -> String {
    let parent = inner_parent.to_string_lossy().replace('\\', "/");
    let parent = parent.trim_matches('/');
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// Normalizes an archive-inner path (from `archive_boundary_candidate`) to the
/// `/`-separated, surrounding-slash-free form the changeset uses.
pub(crate) fn normalize_inner_path(inner: &Path) -> String {
    inner.to_string_lossy().replace('\\', "/").trim_matches('/').to_string()
}

/// The read-only refusal for a path that should have crossed an archive boundary
/// but didn't confirm (a mislabeled or vanished `.zip`).
pub(super) fn read_only_error(path: &Path) -> WriteOperationError {
    WriteOperationError::ReadOnlyDevice {
        path: path.display().to_string(),
        device_name: None,
    }
}

/// Refuses a mutation on a NON-ZIP archive. tar / 7z (and every other format) are
/// browse + extract only — only zip is writable — so every archive-edit route
/// funnels through this guard before touching the (zip-only) mutator. A tar/7z
/// target returns the typed read-only refusal, leaving the archive untouched. The
/// routing predicates stay format-agnostic (they also gate reads and navigation);
/// this is the single write-side chokepoint.
pub(crate) fn ensure_zip_writable(archive_path: &Path) -> Result<(), WriteOperationError> {
    match archive::format_for_path(archive_path) {
        Some(ArchiveFormat::Zip) => Ok(()),
        _ => Err(read_only_error(archive_path)),
    }
}

/// Whether `inner_path` already exists in the archive at `archive_path` (on its
/// parent drive `parent_volume_id`). The create/rename routing calls this to
/// reject a duplicate up front with the app's typed already-exists error, so the
/// mutator never builds a temp for an edit `zip` would only reject at write time
/// (`Duplicate filename`).
///
/// Dispatches on the parent like `run_managed_edit`: a LOCAL (or unregistered —
/// always a local edit) parent parses the central directory straight off the real
/// `.zip` file (zero network, no volume registration needed); a REMOTE (direct SMB
/// / MTP) parent has no local file to open, so it reads the central directory
/// through the parent volume (a ranged tail read via `resolve`, NOT a full pull).
///
/// An unresolvable or unreadable archive resolves to `false`: it isn't a known
/// duplicate, so routing proceeds and the managed op surfaces the real fault
/// through its normal error path rather than being masked here.
pub(crate) async fn archive_inner_exists(parent_volume_id: &str, archive_path: &Path, inner_path: &str) -> bool {
    let parent = get_volume_manager().get(parent_volume_id);
    let is_remote = parent.as_ref().is_some_and(|p| !p.supports_local_fs_access());

    if is_remote {
        let full_path = if inner_path.is_empty() {
            archive_path.to_path_buf()
        } else {
            archive_path.join(inner_path)
        };
        let resolved = get_volume_manager().resolve(parent_volume_id, &full_path).await;
        return match resolved.volume {
            Some(volume) if resolved.is_archive => volume.exists(&full_path).await,
            _ => false,
        };
    }

    let archive_path = archive_path.to_path_buf();
    let inner_path = inner_path.to_string();
    tokio::task::spawn_blocking(move || {
        let Ok(source) = LocalFileSource::open(&archive_path) else {
            return false;
        };
        // Mutation is zip-only, so the oracle always parses as zip (no password:
        // a zip's central directory is plaintext even when its entries aren't).
        let Ok(index) = ArchiveIndex::parse(Arc::new(source), ArchiveFormat::Zip, None) else {
            return false;
        };
        index.exists(&inner_path)
    })
    .await
    .unwrap_or(false)
}

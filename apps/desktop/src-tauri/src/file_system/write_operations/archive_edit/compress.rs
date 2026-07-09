//! Compress: create a NEW zip at a target and pack the sources into it.
//!
//! Mechanically this IS an archive edit — seed a valid empty zip at the
//! destination, then reuse [`route_archive_copy_into`] (`is_move = false`) to add
//! the sources as one changeset. So compress inherits everything the copy-into
//! flow already earns: the scan, plan-inside-the-op, the mutator's temp+rename
//! durability, progress/ETA, cancel, and lane admission.
//!
//! The seed is LOAD-BEARING. `route_archive_copy_into` (and the mutator it drives)
//! opens the target with `ZipArchive::new`, which rejects a 0-byte file with
//! `ZipError::InvalidArchive`. A brand-new compress target has no bytes, so it
//! MUST be seeded with a valid empty archive first — otherwise the copy-into fails
//! before adding anything.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use super::super::OperationEventSink;
use super::super::archive_remote_edit::{self, RemoteEditError};
use super::super::scratch_dir::ScratchDir;
use super::super::state::WriteOperationState;
use super::super::types::{ConflictResolution, WriteOperationError, WriteOperationStartResult};
use super::route_archive_copy_into;
use crate::file_system::get_volume_manager;
use crate::file_system::volume::Volume;

/// Same-directory temp infix, mirroring the mutator's: `foo.zip` seeds through
/// `foo.zip.cmdr-tmp-<uuid>` before the atomic rename onto `foo.zip`.
const TEMP_INFIX: &str = ".cmdr-tmp-";

/// The bytes of a valid empty zip: a bare end-of-central-directory record. The
/// four-byte EOCD signature `PK\x05\x06` followed by 18 zero bytes (disk numbers,
/// entry counts, central-directory size and offset, and comment length all zero).
/// `bytes_start_with_zip_signature` accepts the signature, and `ZipArchive::new`
/// opens it as a zero-entry archive.
fn empty_zip_bytes() -> [u8; 22] {
    let mut bytes = [0u8; 22];
    bytes[..4].copy_from_slice(b"PK\x05\x06");
    bytes
}

/// Writes a valid empty zip at `path` via temp+rename safe-overwrite, mirroring
/// the mutator's discipline so a crash mid-seed never leaves a torn file under
/// `path`: build into a same-directory temp, fsync it, atomically rename over
/// `path`, then fsync the parent dir. Any early return removes the temp and
/// leaves `path` untouched.
pub(crate) fn seed_empty_zip(path: &Path) -> Result<(), WriteOperationError> {
    let write_err = |e: std::io::Error| WriteOperationError::WriteError {
        path: path.display().to_string(),
        message: e.to_string(),
    };

    // Build into a same-directory temp, then atomically rename over `path`. A crash
    // mid-write leaves only the temp (removed by the guard on any early return), so
    // `path` is never a torn half-seed — the rename is the single instant it appears.
    let temp_path = temp_sibling_path(path);
    let mut guard = SeedTempGuard {
        path: temp_path.clone(),
        armed: true,
    };

    let mut file = File::create(&temp_path).map_err(write_err)?;
    file.write_all(&empty_zip_bytes()).map_err(write_err)?;
    // fsync the bytes before the swap so a power loss can't surface an empty temp.
    file.sync_all().map_err(write_err)?;
    drop(file);

    std::fs::rename(&temp_path, path).map_err(write_err)?;
    guard.disarm();

    // Best-effort: fsync the parent dir so the rename itself survives a power loss.
    fsync_parent_dir(path);
    Ok(())
}

/// Seeds a valid empty zip at a REMOTE target THROUGH the parent volume. The
/// remote copy-into path (`route_archive_copy_into` -> `pull_apply_upload_swap`)
/// PULLS the target before editing, so a local-FS seed would be invisible to it —
/// the seed must be a real file on the remote. Writes the 22-byte empty zip to a
/// local scratch file, then places it at `dest_zip_full_path` via the remote
/// edit's own durable upload+swap (temp sibling -> atomic swap), so a crash never
/// leaves a torn seed at the user's destination and an overwrite is atomic.
async fn seed_empty_zip_remote(parent: &dyn Volume, dest_zip_full_path: &Path) -> Result<(), WriteOperationError> {
    // Stage the 22 bytes in a private scratch dir; its `Drop` removes the file.
    let scratch = ScratchDir::new("cmdr-compress-seed").map_err(|e| WriteOperationError::WriteError {
        path: dest_zip_full_path.display().to_string(),
        message: e.to_string(),
    })?;
    let local_seed = scratch.path().join("seed.zip");
    std::fs::write(&local_seed, empty_zip_bytes()).map_err(|e| WriteOperationError::WriteError {
        path: local_seed.display().to_string(),
        message: e.to_string(),
    })?;

    // A fresh, never-cancelled state: the seed is a 22-byte write that runs BEFORE
    // the managed op exists, so there is no live cancel to thread through it.
    let state = WriteOperationState::new(Duration::from_millis(0));
    archive_remote_edit::place_local_file(parent, &local_seed, dest_zip_full_path, &state)
        .await
        .map_err(|e| match e {
            RemoteEditError::Cancelled => WriteOperationError::Cancelled {
                message: "the compress seed was cancelled".to_string(),
            },
            RemoteEditError::Op(w) => w,
        })
}

/// A fresh same-directory temp path: `foo.zip` -> `foo.zip.cmdr-tmp-<uuid>`,
/// matching the mutator so the reaper cleans an abandoned seed the same way.
fn temp_sibling_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|s| s.to_os_string()).unwrap_or_default();
    name.push(TEMP_INFIX);
    name.push(Uuid::new_v4().to_string());
    path.with_file_name(name)
}

/// fsyncs the target's parent directory so a just-completed rename is durable.
/// Best-effort (opening a dir read-only can fail on some filesystems).
fn fsync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent()
        && let Ok(dir) = File::open(parent)
    {
        let _ = dir.sync_all();
    }
}

/// Removes the in-progress temp on any early return, so a failed seed never leaves
/// a half-built sibling. The happy path disarms it right after the atomic rename.
struct SeedTempGuard {
    path: PathBuf,
    armed: bool,
}

impl SeedTempGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SeedTempGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Compresses `source_paths` (relative to `source_volume`'s root) into a NEW zip
/// at `dest_zip_full_path`: seed a valid empty archive, then delegate to
/// [`route_archive_copy_into`] to add the sources as one changeset. Reuses
/// `WriteOperationType::ArchiveEdit` — compress has no distinct backend op type;
/// its identity lives in the frontend.
#[allow(
    clippy::too_many_arguments,
    reason = "mirrors route_archive_copy_into's cross-volume→archive seam (source handle, paths, dest, parent id, policy)"
)]
pub(crate) async fn compress_start(
    events: Arc<dyn OperationEventSink>,
    source_volume: Arc<dyn Volume>,
    source_paths: Vec<PathBuf>,
    dest_zip_full_path: PathBuf,
    parent_volume_id: String,
    conflict: ConflictResolution,
    progress_interval_ms: u64,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Seed a valid empty zip at the target so the copy-into has a real archive to
    // open. The seed must be visible to `route_archive_copy_into`'s parent-aware
    // path: a LOCAL parent edits the file in place, so a local-FS seed works; a
    // REMOTE parent PULLS the target before editing (`pull_apply_upload_swap`), so
    // its seed must be a real file ON the remote, written THROUGH the parent volume.
    match get_volume_manager().get(&parent_volume_id) {
        Some(parent) if !parent.supports_local_fs_access() => {
            seed_empty_zip_remote(parent.as_ref(), &dest_zip_full_path).await?;
        }
        // Local parent, or an unregistered id (`route_archive_copy_into` falls back
        // to a local in-place edit for it) — seed the local filesystem.
        _ => seed_empty_zip(&dest_zip_full_path)?,
    }

    route_archive_copy_into(
        events,
        source_volume,
        source_paths,
        dest_zip_full_path,
        parent_volume_id,
        conflict,
        progress_interval_ms,
        false,
    )
    .await
}

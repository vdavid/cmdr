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

use uuid::Uuid;

use super::super::OperationEventSink;
use super::super::types::{ConflictResolution, WriteOperationError, WriteOperationStartResult};
use super::route_archive_copy_into;
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
    // Local-only seed. A REMOTE parent would need the seed written THROUGH the
    // parent volume (the remote edit PULLS the existing `.zip` before editing, so a
    // local-FS seed is invisible to it). v1 doesn't do that — the command layer
    // (M2) refuses a remote destination. Remote dest: see
    // compress-feature-plan.md § Open questions (M8).
    seed_empty_zip(&dest_zip_full_path)?;

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

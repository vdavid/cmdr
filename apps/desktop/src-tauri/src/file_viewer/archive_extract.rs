//! Preview-in-zip for the file viewer: bounded temp-extract of an archive entry.
//!
//! The viewer core is 100% `std::fs::File`-based (byte-seek, line-index, encoding,
//! media protocol) with no `Volume` seam, so a path INSIDE a zip
//! (`/…/foo.zip/inner.txt`) can't be opened directly. Instead the entry is streamed
//! out to a bounded temp file and the viewer opens THAT. Threading a `Volume`
//! byte-source through the whole viewer is a later refactor; this is the deliberately
//! simple bridge (see `docs/specs/archive-browsing-m1b-derivation.md` lead decision 5).
//!
//! Discipline this module owns:
//!
//! - **One temp per open.** Re-opening the same entry re-extracts — simple beats a
//!   dedup cache. The temp is deleted when the viewer session closes (both close
//!   paths funnel through [`super::session::close_session`]).
//! - **Bounded, refuse-before-extract.** The archive index reports the entry's
//!   uncompressed size UP FRONT, so an oversize entry is refused with a typed
//!   [`ViewerError::ExtractTooLarge`] before a single byte is written. That's also
//!   the zip-bomb guard for preview: a streaming byte-cap is the belt-and-suspenders
//!   backstop against a central directory that understates the real size.
//! - **Reaper-friendly, per-instance temps.** Each extraction lives in its own
//!   `.cmdr-viewer-<uuid>/` subdir of a per-instance extract dir (under the app data
//!   dir, so side-by-side dev/prod/worktree instances never reap each other's live
//!   temps). The startup reaper removes any `.cmdr-viewer-*` subdir left by a crash.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::RwLockIgnorePoison;

use super::ViewerError;

/// Max uncompressed bytes to extract for a single preview. Above this the open is
/// refused (typed) before any extraction, which doubles as the zip-bomb guard.
///
/// 256 MiB comfortably covers real preview content (documents, images, PDFs, most
/// media) while bounding the temp write, extraction time, and decompression
/// amplification. Chosen independently of the FE copy-selection ceiling
/// (`COPY_REFUSE_BYTES`, 100 MiB) — that caps a *selection*, this caps a whole-entry
/// materialization.
pub(super) const EXTRACT_CAP_BYTES: u64 = 256 * 1024 * 1024;

/// Prefix on each extraction's subdir. The startup reaper matches on it, and it's the
/// `.cmdr-` family the project uses for recoverable temps.
const EXTRACT_SUBDIR_PREFIX: &str = ".cmdr-viewer-";

/// Fallback extract-dir name under the OS temp dir when [`init_archive_extract_dir`]
/// hasn't run (unit tests, a not-yet-initialized process). Prod always initializes a
/// per-instance dir under the app data dir.
const DEFAULT_EXTRACT_DIRNAME: &str = "cmdr-viewer-extract";

/// The per-instance extract dir, stashed at startup by [`init_archive_extract_dir`].
static EXTRACT_DIR: LazyLock<RwLock<Option<PathBuf>>> = LazyLock::new(|| RwLock::new(None));

/// A successful extraction: the temp file to open, and the subdir to remove on close.
#[derive(Debug)]
pub(super) struct ExtractedEntry {
    /// The extracted entry on local disk, named with the entry's basename so the
    /// viewer shows the right title and classifies media by the right extension.
    pub(super) temp_file: PathBuf,
    /// The `.cmdr-viewer-<uuid>/` subdir wrapping `temp_file`, removed wholesale on
    /// session close (stored on the `ViewerSession`).
    pub(super) cleanup_dir: PathBuf,
}

/// Records the per-instance extract dir and reaps any orphans left in it by a crash.
/// Called once at startup from `lib.rs` with `<app_data_dir>/viewer-extract`.
pub fn init_archive_extract_dir(dir: PathBuf) {
    reap_orphan_extracts(&dir);
    *EXTRACT_DIR.write_ignore_poison() = Some(dir);
}

/// The extract dir: the initialized per-instance dir, or an OS-temp fallback.
fn extract_dir() -> PathBuf {
    EXTRACT_DIR
        .read_ignore_poison()
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(DEFAULT_EXTRACT_DIRNAME))
}

/// Removes every `.cmdr-viewer-*` subdir in `dir` (orphaned extractions from a crash).
/// Best-effort: an unreadable dir or a failed remove is logged-then-ignored, never
/// fatal. The prefix guard means it can only ever touch our own extraction subdirs.
pub(super) fn reap_orphan_extracts(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // dir doesn't exist yet (first run) — nothing to reap.
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if is_orphan_extract_name(&name.to_string_lossy())
            && let Err(e) = std::fs::remove_dir_all(entry.path())
        {
            log::debug!(
                target: "cmdr_lib::file_viewer",
                "reap_orphan_extracts: could not remove {}: {e}",
                entry.path().display()
            );
        }
    }
}

/// Whether `name` is one of our extraction subdirs (the reaper's match predicate).
pub(super) fn is_orphan_extract_name(name: &str) -> bool {
    name.starts_with(EXTRACT_SUBDIR_PREFIX)
}

/// If `requested` crosses into an archive, extract the addressed entry to a bounded
/// temp and return it; otherwise `Ok(None)` (the caller opens `requested` directly).
///
/// Uses the shared [`VolumeManager::resolve`](crate::file_system::VolumeManager::resolve)
/// against `volume_id`'s volume so archive detection, registration, and the LRU stay
/// single-sourced with the listing/copy paths — and a `.zip` on a REMOTE parent
/// (direct SMB / MTP) is pulled through that parent, not a hardcoded `"root"`.
/// Blocking: run it inside `spawn_blocking`, not on the IPC thread.
pub(super) fn extract_if_archive_inner(
    requested: &Path,
    volume_id: &str,
) -> Result<Option<ExtractedEntry>, ViewerError> {
    extract_if_archive_inner_with(requested, volume_id, &extract_dir(), EXTRACT_CAP_BYTES)
}

/// [`extract_if_archive_inner`] with an explicit dir + cap, for tests.
pub(super) fn extract_if_archive_inner_with(
    requested: &Path,
    volume_id: &str,
    dir: &Path,
    cap: u64,
) -> Result<Option<ExtractedEntry>, ViewerError> {
    // Only a path INSIDE the archive is temp-extracted. The `.zip` file ITSELF is a
    // regular file: viewing it shows its raw bytes like any binary file (extracting
    // inner "" would address the archive ROOT — a directory — and error). Pure
    // string pre-filter (no I/O); `resolve` below does the parent-aware confirm, so
    // a mislabeled `.zip` or a remote-only archive is handled there.
    let is_inner_candidate = crate::file_system::volume::backends::archive::archive_boundary_candidate(requested)
        .is_some_and(|(_zip, inner)| !inner.as_os_str().is_empty());
    if !is_inner_candidate {
        return Ok(None);
    }
    let resolved =
        tauri::async_runtime::block_on(crate::file_system::get_volume_manager().resolve(volume_id, requested));
    if !resolved.is_archive {
        return Ok(None);
    }
    let Some(volume) = resolved.volume else {
        // Confirmed an archive boundary but the volume vanished (unmount / evict
        // race). Treat as non-archive; the caller's existence check surfaces NotFound.
        return Ok(None);
    };
    let entry_path = resolved.path;
    tauri::async_runtime::block_on(extract_entry(volume, entry_path, dir, cap)).map(Some)
}

/// Streams one archive entry to a fresh temp subdir under `dir`, refusing an oversize
/// entry before writing anything.
async fn extract_entry(
    volume: std::sync::Arc<dyn Volume>,
    entry_path: PathBuf,
    dir: &Path,
    cap: u64,
) -> Result<ExtractedEntry, ViewerError> {
    // Size + kind come from the central directory (no decompression), so the refusal
    // lands BEFORE we create a temp or stream a byte.
    let meta = volume.get_metadata(&entry_path).await.map_err(map_volume_error)?;
    if meta.is_directory {
        return Err(ViewerError::IsDirectory);
    }
    let declared = meta.size.unwrap_or(0);
    if declared > cap {
        return Err(ViewerError::ExtractTooLarge { size: declared, cap });
    }

    let cleanup_dir = dir.join(format!("{EXTRACT_SUBDIR_PREFIX}{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&cleanup_dir)?;
    let temp_file = cleanup_dir.join(temp_basename(&meta.name));

    // Any failure past this point must not leave the subdir behind.
    match stream_to_file(volume.as_ref(), &entry_path, &temp_file, cap).await {
        Ok(()) => Ok(ExtractedEntry { temp_file, cleanup_dir }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&cleanup_dir);
            Err(e)
        }
    }
}

/// Streams the entry into `temp_file`, enforcing the byte-cap as a backstop against a
/// central directory that understates the real uncompressed size.
async fn stream_to_file(volume: &dyn Volume, entry_path: &Path, temp_file: &Path, cap: u64) -> Result<(), ViewerError> {
    use std::io::Write as _;

    let mut stream = volume.open_read_stream(entry_path).await.map_err(map_volume_error)?;
    let mut file = std::fs::File::create(temp_file)?;
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.map_err(map_volume_error)?;
        written = written.saturating_add(chunk.len() as u64);
        if written > cap {
            return Err(ViewerError::ExtractTooLarge { size: written, cap });
        }
        file.write_all(&chunk)?;
    }
    file.flush()?;
    Ok(())
}

/// A safe single-component filename for the temp, derived from the entry's basename.
/// Empty or separator-bearing names fall back to a fixed name (the subdir already
/// guarantees uniqueness; this only affects the viewer's displayed title + extension).
fn temp_basename(entry_name: &str) -> String {
    let candidate = Path::new(entry_name).file_name().and_then(|n| n.to_str()).unwrap_or("");
    if candidate.is_empty() {
        "preview".to_string()
    } else {
        candidate.to_string()
    }
}

/// Maps a `VolumeError` from the archive read into a typed `ViewerError`. Path-shaped
/// errors keep their twins; everything else (encrypted, corrupt, unsupported codec)
/// becomes `Archive`, which the FE renders without inspecting the message string.
fn map_volume_error(err: VolumeError) -> ViewerError {
    match err {
        VolumeError::NotFound(path) => ViewerError::NotFound { path },
        VolumeError::IsADirectory(_) => ViewerError::IsDirectory,
        other => ViewerError::Archive {
            message: other.to_string(),
        },
    }
}

//! Previewing a file the OS can't open: bounded temp-materialization of one file
//! the viewer can only reach through a `Volume`.
//!
//! Two kinds of path have no `std::fs` file behind them:
//!
//! - a path a ROUTE serves: an archive entry or a blob in a repo's virtual `.git`
//!   trees (`/…/foo.zip/inner.txt`, `/…/.git/branches/main/src/lib.rs`);
//! - a path on a volume whose paths aren't OS-visible
//!   (`Volume::paths_are_os_visible`): a phone over ADB or MTP, an SFTP or WebDAV
//!   server, a direct SMB share whose mount went away (`adb://R58M1/sdcard/a.txt`).
//!
//! The viewer core is 100% `std::fs::File`-based (byte-seek, line-index, encoding,
//! media protocol) with no `Volume` seam, so neither can be opened directly. Instead
//! the file is streamed out to a bounded temp and the viewer opens THAT. Threading a
//! `Volume` byte-source through the whole viewer is a later refactor; this is the
//! deliberately simple bridge.
//!
//! Both kinds go through one code path on purpose: every volume answers
//! `get_metadata` and `open_read_stream`, and a second copy of the temp lifecycle
//! would drift. The only thing that reads the kind is the error mapping, where the
//! archive family has its own frontend copy and nothing else does.
//!
//! [`materialize_for_viewer`] is the viewer's entry and covers both kinds;
//! [`extract_if_routed`] covers routes only, for the agent's `inspect_file`, which
//! refuses a volume the OS can't open before it gets here.
//!
//! Discipline this module owns:
//!
//! - **One temp per open.** Re-opening the same file re-materializes — simple
//!   beats a dedup cache. The temp is deleted when the viewer session closes (both
//!   close paths funnel through [`super::session::close_session`]). The second
//!   caller, the agent's `inspect_file` (`agent/tools/read/inspect/`), owns its
//!   temp for one read and removes it in a `Drop` guard; the contract is the same:
//!   whoever receives an [`MaterializedFile`] removes `cleanup_dir`.
//! - **Bounded, refuse-before-extract.** The volume reports the file's size UP
//!   FRONT (an archive from its central directory, the portal from the blob header,
//!   a phone or server from its stat), so an oversize file is refused with a typed
//!   [`ViewerError::TooLargeToPreview`] before a single byte is written. That's also
//!   the zip-bomb guard for preview: a streaming byte-cap is the belt-and-suspenders
//!   backstop against a reported size that understates the real one.
//! - **A snapshot, not a live view.** The temp is the file as it was at open. No
//!   watcher follows it, so tail mode and reload re-read the temp, never the source.
//! - **Reaper-friendly, per-instance temps.** Each extraction lives in its own
//!   `.cmdr-viewer-<uuid>/` subdir of a per-instance extract dir (under the app data
//!   dir, so side-by-side dev/prod/worktree instances never reap each other's live
//!   temps). The startup reaper removes any `.cmdr-viewer-*` subdir left by a crash.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::RwLockIgnorePoison;

use super::ViewerError;
use super::pending_open::PendingOpen;
use crate::file_system::volume::manager::{RoutedKind, get_volume_manager, path_routes_over_its_parent};

/// Max bytes to materialize for a single preview. Above this the open is refused
/// (typed) before any extraction, which doubles as the zip-bomb guard.
///
/// 256 MiB comfortably covers real preview content (documents, images, PDFs, most
/// media) while bounding the temp write, extraction time, and decompression
/// amplification. Chosen independently of the FE copy-selection ceiling
/// (`COPY_REFUSE_BYTES`, 100 MiB) — that caps a *selection*, this caps a whole-entry
/// materialization.
pub(crate) const PREVIEW_CAP_BYTES: u64 = 256 * 1024 * 1024;

/// Prefix on each extraction's subdir. The startup reaper matches on it, and it's the
/// `.cmdr-` family the project uses for recoverable temps.
const TEMP_SUBDIR_PREFIX: &str = ".cmdr-viewer-";

/// Fallback extract-dir name under the OS temp dir when [`init_materialize_dir`]
/// hasn't run (unit tests, a not-yet-initialized process). Prod always initializes a
/// per-instance dir under the app data dir.
const DEFAULT_EXTRACT_DIRNAME: &str = "cmdr-viewer-extract";

/// The per-instance extract dir, stashed at startup by [`init_materialize_dir`].
static MATERIALIZE_DIR: LazyLock<RwLock<Option<PathBuf>>> = LazyLock::new(|| RwLock::new(None));

/// A successful extraction: the temp file to open, and the subdir to remove on close.
#[derive(Debug)]
pub(crate) struct MaterializedFile {
    /// The extracted file on local disk, named with the source's basename so the
    /// viewer shows the right title and classifies media by the right extension.
    pub(crate) temp_file: PathBuf,
    /// The `.cmdr-viewer-<uuid>/` subdir wrapping `temp_file`, removed wholesale on
    /// session close (stored on the `ViewerSession`).
    pub(crate) cleanup_dir: PathBuf,
}

/// Records the per-instance extract dir and reaps any orphans left in it by a crash.
/// Called once at startup from `lib.rs` with `<app_data_dir>/viewer-extract`.
pub fn init_materialize_dir(dir: PathBuf) {
    reap_orphan_temps(&dir);
    *MATERIALIZE_DIR.write_ignore_poison() = Some(dir);
}

/// The extract dir: the initialized per-instance dir, or an OS-temp fallback.
fn materialize_dir() -> PathBuf {
    MATERIALIZE_DIR
        .read_ignore_poison()
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(DEFAULT_EXTRACT_DIRNAME))
}

/// Removes every `.cmdr-viewer-*` subdir in `dir` (orphaned extractions from a crash).
/// Best-effort: an unreadable dir or a failed remove is logged-then-ignored, never
/// fatal. The prefix guard means it can only ever touch our own extraction subdirs.
pub(super) fn reap_orphan_temps(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // dir doesn't exist yet (first run) — nothing to reap.
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if is_orphan_temp_name(&name.to_string_lossy())
            && let Err(e) = std::fs::remove_dir_all(entry.path())
        {
            log::debug!(
                target: "cmdr_lib::file_viewer",
                "reap_orphan_temps: could not remove {}: {e}",
                entry.path().display()
            );
        }
    }
}

/// Whether `name` is one of our extraction subdirs (the reaper's match predicate).
pub(super) fn is_orphan_temp_name(name: &str) -> bool {
    name.starts_with(TEMP_SUBDIR_PREFIX)
}

/// What the viewer opens for `requested`: a bounded temp copy when the OS can't open
/// the path (a route serves it, or its volume's paths aren't OS-visible), else
/// `Ok(None)` and the caller opens `requested` directly.
///
/// `open` watches the pull: it records progress there and stops once `open` is
/// abandoned. Blocking: run it inside `spawn_blocking`, not on the IPC thread.
pub(crate) fn materialize_for_viewer(
    requested: &Path,
    volume_id: &str,
    open: &PendingOpen,
) -> Result<Option<MaterializedFile>, ViewerError> {
    materialize_for_viewer_with(requested, volume_id, &materialize_dir(), PREVIEW_CAP_BYTES, open)
}

/// Whether opening `requested` may pull it into a temp first, so `viewer_open` can
/// grant the materialization budget. No I/O and no route confirm: it's a heuristic,
/// so over-granting to a mislabeled `.zip` is harmless.
///
/// Reads the registry with `get`, not `resolve`: a path that doesn't route is served
/// by `volume_id`'s own volume, and `resolve` may confirm a route with I/O.
pub(crate) fn open_may_materialize(requested: &Path, volume_id: &str) -> bool {
    path_routes_over_its_parent(requested)
        || get_volume_manager()
            .get(volume_id)
            .is_some_and(|volume| !volume.paths_are_os_visible())
}

/// [`materialize_for_viewer`] with an explicit dir + cap, for tests.
pub(crate) fn materialize_for_viewer_with(
    requested: &Path,
    volume_id: &str,
    dir: &Path,
    cap: u64,
    open: &PendingOpen,
) -> Result<Option<MaterializedFile>, ViewerError> {
    if let Some(entry) = extract_routed(requested, volume_id, dir, cap, open)? {
        return Ok(Some(entry));
    }
    // Locality is `paths_are_os_visible`, ❌ never `supports_local_fs_access`: a
    // direct SMB share answers `false` there, yet its mounted path opens with
    // `std::fs` and keeps tail mode, so it stays a direct open until its mount goes.
    let resolved = tauri::async_runtime::block_on(get_volume_manager().resolve(volume_id, requested));
    let Some(volume) = resolved.volume else {
        // Unregistered (an eject or unmount race): the caller's existence check answers.
        return Ok(None);
    };
    if resolved.routed.is_some() || volume.paths_are_os_visible() {
        return Ok(None);
    }
    tauri::async_runtime::block_on(pull_to_temp(volume, resolved.path, dir, cap, None, open)).map(Some)
}

/// If a ROUTE serves `requested`, stream the addressed entry to a bounded temp and
/// return it; otherwise `Ok(None)` (the caller opens `requested` directly).
///
/// Uses the shared [`VolumeManager::resolve`](crate::file_system::VolumeManager::resolve)
/// against `volume_id`'s volume so route detection, registration, and the LRU stay
/// single-sourced with the listing/copy paths — and a `.zip` on a REMOTE parent
/// (direct SMB / MTP) is pulled through that parent, not a hardcoded `"root"`.
/// Blocking: run it inside `spawn_blocking`, not on the IPC thread.
pub(crate) fn extract_if_routed(requested: &Path, volume_id: &str) -> Result<Option<MaterializedFile>, ViewerError> {
    extract_if_routed_with(requested, volume_id, &materialize_dir(), PREVIEW_CAP_BYTES)
}

/// [`extract_if_routed`] with an explicit dir + cap, for tests.
pub(crate) fn extract_if_routed_with(
    requested: &Path,
    volume_id: &str,
    dir: &Path,
    cap: u64,
) -> Result<Option<MaterializedFile>, ViewerError> {
    extract_routed(requested, volume_id, dir, cap, &PendingOpen::new())
}

/// The route half of [`materialize_for_viewer_with`], watched by `open`.
fn extract_routed(
    requested: &Path,
    volume_id: &str,
    dir: &Path,
    cap: u64,
    open: &PendingOpen,
) -> Result<Option<MaterializedFile>, ViewerError> {
    // Only a path with no file of its own is materialized. The `.zip` file ITSELF
    // is a regular file: viewing it shows its raw bytes like any binary file
    // (extracting inner "" would address the archive ROOT — a directory — and
    // error). Pure string pre-filter (no I/O); `resolve` below does the
    // parent-aware confirm, so a mislabeled `.zip`, a remote-only archive, and a
    // `.git` that isn't a repository are all handled there.
    if !path_routes_over_its_parent(requested) {
        return Ok(None);
    }
    let resolved = tauri::async_runtime::block_on(get_volume_manager().resolve(volume_id, requested));
    let Some(routed) = resolved.routed else {
        return Ok(None);
    };
    let Some(volume) = resolved.volume else {
        // The route confirmed but the volume vanished (unmount / evict race). Treat
        // as unrouted; the caller's existence check surfaces NotFound.
        return Ok(None);
    };
    let entry_path = resolved.path;
    tauri::async_runtime::block_on(pull_to_temp(volume, entry_path, dir, cap, Some(routed), open)).map(Some)
}

/// Streams one file to a fresh temp subdir under `dir`, refusing an oversize file
/// before writing anything. `routed` is the route that minted `volume`, or `None`
/// for a volume the OS can't open.
async fn pull_to_temp(
    volume: std::sync::Arc<dyn Volume>,
    entry_path: PathBuf,
    dir: &Path,
    cap: u64,
    routed: Option<RoutedKind>,
    open: &PendingOpen,
) -> Result<MaterializedFile, ViewerError> {
    if let Some(abandoned) = open.abandoned_error() {
        return Err(abandoned);
    }
    // Size + kind come from the volume's metadata (an archive's central directory,
    // the portal's tree entry, a phone's or server's stat), never a decompression or
    // a content read, so the refusal lands BEFORE we create a temp or stream a byte.
    let meta = volume
        .get_metadata(&entry_path)
        .await
        .map_err(|e| map_volume_error(e, routed))?;
    if meta.is_directory {
        return Err(ViewerError::IsDirectory);
    }
    let declared = meta.size.unwrap_or(0);
    if declared > cap {
        return Err(ViewerError::TooLargeToPreview { size: declared, cap });
    }

    let cleanup_dir = dir.join(format!("{TEMP_SUBDIR_PREFIX}{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&cleanup_dir)?;
    let temp_file = cleanup_dir.join(temp_basename(&meta.name));

    // Any failure past this point must not leave the subdir behind.
    match stream_to_file(volume.as_ref(), &entry_path, &temp_file, cap, meta.size, routed, open).await {
        Ok(()) => Ok(MaterializedFile { temp_file, cleanup_dir }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&cleanup_dir);
            Err(e)
        }
    }
}

/// Streams the file into `temp_file`, enforcing the byte-cap as a backstop against a
/// reported size that understates the real one. Records progress on `open` against
/// `declared_size` after every chunk, and stops once `open` is abandoned.
///
/// The abandon check sits BETWEEN chunks, ❌ never as a race that drops an in-flight
/// `next_chunk`: an MTP window read is a USB transaction, and dropping it mid-flight
/// is what wedges a phone. Returning drops the stream, which is the backend's cancel
/// (a network producer stops on it; see `ChannelReadStream`), so a close costs at most
/// the one chunk already on the wire: 64 KiB on ADB, 8 MiB on MTP.
#[allow(
    clippy::too_many_arguments,
    reason = "one pull's inputs; a struct would exist only to pass them here"
)]
async fn stream_to_file(
    volume: &dyn Volume,
    entry_path: &Path,
    temp_file: &Path,
    cap: u64,
    declared_size: Option<u64>,
    routed: Option<RoutedKind>,
    open: &PendingOpen,
) -> Result<(), ViewerError> {
    use std::io::Write as _;

    open.record_pull(0, declared_size);
    let mut stream = volume
        .open_read_stream(entry_path)
        .await
        .map_err(|e| map_volume_error(e, routed))?;
    let mut file = std::fs::File::create(temp_file)?;
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.map_err(|e| map_volume_error(e, routed))?;
        written = written.saturating_add(chunk.len() as u64);
        if written > cap {
            return Err(ViewerError::TooLargeToPreview { size: written, cap });
        }
        file.write_all(&chunk)?;
        open.record_pull(written, declared_size);
        if let Some(abandoned) = open.abandoned_error() {
            return Err(abandoned);
        }
    }
    file.flush()?;
    Ok(())
}

/// A safe single-component filename for the temp, derived from the source's basename.
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

/// Maps a `VolumeError` from a materializing read into a typed `ViewerError`.
/// Path-shaped errors keep their twins whatever the source; the rest depend on it.
///
/// The archive family (encrypted, corrupt, unsupported codec) has its own frontend
/// copy under `ViewerError::Archive`, which the FE renders without inspecting the
/// message string. A portal read and a plain pull have no such family — a repository
/// that can't be opened or a phone that dropped mid-read is a fault, not a kind of
/// file — so they stay a plain `Io`.
fn map_volume_error(err: VolumeError, routed: Option<RoutedKind>) -> ViewerError {
    match err {
        VolumeError::NotFound(path) => ViewerError::NotFound { path },
        VolumeError::IsADirectory(_) => ViewerError::IsDirectory,
        other => match routed {
            Some(RoutedKind::Archive) => ViewerError::Archive {
                message: other.to_string(),
            },
            Some(RoutedKind::GitPortal) | None => ViewerError::Io {
                message: other.to_string(),
            },
        },
    }
}

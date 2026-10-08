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
//! [`materialize_for_viewer`] is the viewer's entry and [`materialize_for_inspect`]
//! is the agent's; both cover both kinds. [`extract_if_routed_for_inspect`] gives the
//! inspector's route-first pipeline the same cancellable pull.
//!
//! Discipline this module owns:
//!
//! - **One temp per window's file.** A fresh open pulls; a view switch in the same
//!   window shares the copy its session already holds ([`PreviewTemp`], behind an
//!   `Arc`), and the temp goes when the last session holding it closes. The second
//!   caller, the agent's `inspect_file` (`agent/tools/read/inspect/`), owns its
//!   temp for one read and removes it in a `Drop` guard; the contract is the same:
//!   whoever receives a [`MaterializedFile`] removes `cleanup_dir`.
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};

use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::RwLockIgnorePoison;

use super::pending_open::PendingOpen;
use super::{ArchiveFailureKind, ViewerError};
use crate::file_system::volume::manager::{RoutedKind, get_volume_manager, path_routes_over_its_parent};

/// Max bytes to materialize for a single preview. Above this the open is refused
/// (typed) before any extraction, which doubles as the zip-bomb guard.
///
/// 256 MiB comfortably covers real preview content (documents, images, PDFs, most
/// media) while bounding the temp write, extraction time, and decompression
/// amplification. Chosen independently of the FE copy-selection ceiling
/// (`COPY_REFUSE_BYTES`, 100 MB) — that caps a *selection*, this caps a whole-entry
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

/// A successful extraction: the temp file to open, and the subdir to remove when done.
#[derive(Debug)]
pub(crate) struct MaterializedFile {
    /// The extracted file on local disk, named with the source's basename so the
    /// viewer shows the right title and classifies media by the right extension.
    pub(crate) temp_file: PathBuf,
    /// The `.cmdr-viewer-<uuid>/` subdir wrapping `temp_file`. Whoever receives this
    /// removes it: the viewer by wrapping it in a [`PreviewTemp`], the agent's
    /// `inspect_file` in its own `Drop` guard.
    pub(crate) cleanup_dir: PathBuf,
}

/// A viewer's temp copy of one file, shared by every session that previews it.
///
/// A view switch ("View as text", or back to the image) opens a second session for the
/// same window before the first one closes, so both hold this through an `Arc`, and the
/// subdir goes when the LAST of them drops it. It remembers which file it copies, so a
/// switch reuses it only for the same path on the same volume.
#[derive(Debug)]
pub(crate) struct PreviewTemp {
    pub(crate) temp_file: PathBuf,
    cleanup_dir: PathBuf,
    source_path: PathBuf,
    volume_id: String,
}

impl PreviewTemp {
    /// Takes ownership of `materialized`, the copy of `source_path` on `volume_id`.
    pub(crate) fn new(materialized: MaterializedFile, source_path: PathBuf, volume_id: &str) -> Self {
        Self {
            temp_file: materialized.temp_file,
            cleanup_dir: materialized.cleanup_dir,
            source_path,
            volume_id: volume_id.to_string(),
        }
    }

    /// Whether this is a copy of `source_path` on `volume_id`, still on disk.
    pub(crate) fn is_copy_of(&self, source_path: &Path, volume_id: &str) -> bool {
        self.source_path == source_path && self.volume_id == volume_id && self.temp_file.exists()
    }
}

impl Drop for PreviewTemp {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.cleanup_dir) {
            log::debug!(
                target: "cmdr_lib::file_viewer",
                "preview temp cleanup failed for {}: {e}",
                self.cleanup_dir.display()
            );
        }
    }
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
    reap_temps_with_prefix(dir, TEMP_SUBDIR_PREFIX);
}

/// Removes every subdir of `dir` whose name starts with `prefix`. The one reaper body,
/// shared with the open-with temps (`open_with_extract.rs`), which keep their own dir
/// and prefix.
pub(super) fn reap_temps_with_prefix(dir: &Path, prefix: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return; // dir doesn't exist yet (first run) — nothing to reap.
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(prefix)
            && let Err(e) = std::fs::remove_dir_all(entry.path())
        {
            log::debug!(
                target: "cmdr_lib::file_viewer",
                "reap_temps_with_prefix: could not remove {}: {e}",
                entry.path().display()
            );
        }
    }
}

/// Whether `name` is one of our extraction subdirs (the reaper's match predicate).
#[cfg(test)]
pub(super) fn is_orphan_temp_name(name: &str) -> bool {
    name.starts_with(TEMP_SUBDIR_PREFIX)
}

/// Where a pull writes its temp: the dir, and the prefix its per-pull subdir takes,
/// which is what that family's reaper matches on.
#[derive(Clone, Copy)]
pub(super) struct TempSpot<'a> {
    pub(super) dir: &'a Path,
    pub(super) prefix: &'a str,
}

impl<'a> TempSpot<'a> {
    /// The viewer's own family in `dir`.
    fn viewer(dir: &'a Path) -> Self {
        Self {
            dir,
            prefix: TEMP_SUBDIR_PREFIX,
        }
    }
}

/// A routed pull that couldn't be made, and which route served the path, so open-with's
/// toast can say where the file came from.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug)]
pub(super) struct RoutedPullFailure {
    pub(super) routed: RoutedKind,
    pub(super) error: ViewerError,
}

/// [`extract_routed`] into another temp family's spot, unwatched: open-with's pull
/// (`open_with_extract.rs`), which has no window to close and no deadline to honor.
#[cfg(any(target_os = "macos", test))]
pub(super) fn extract_routed_into(
    requested: &Path,
    volume_id: &str,
    spot: TempSpot<'_>,
    cap: u64,
) -> Result<Option<MaterializedFile>, RoutedPullFailure> {
    let Some(route) = resolve_route(requested, volume_id) else {
        return Ok(None);
    };
    let routed = route.kind;
    tauri::async_runtime::block_on(pull_to_temp(
        route.volume,
        route.entry_path,
        spot,
        cap,
        Some(routed),
        &PendingOpen::new(),
        None,
    ))
    .map(Some)
    .map_err(|error| RoutedPullFailure { routed, error })
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
    materialize_with(requested, volume_id, dir, cap, open, None)
}

/// What `inspect_file` reads for `requested`: the same bounded temp the viewer uses,
/// with the inspector's per-path deadline stopping a pull at its next chunk boundary.
pub(crate) fn materialize_for_inspect(
    requested: &Path,
    volume_id: &str,
    cancel: &AtomicBool,
) -> Result<Option<MaterializedFile>, ViewerError> {
    materialize_for_inspect_with(requested, volume_id, &materialize_dir(), PREVIEW_CAP_BYTES, cancel)
}

/// [`materialize_for_inspect`] with an explicit dir + cap, for tests.
pub(crate) fn materialize_for_inspect_with(
    requested: &Path,
    volume_id: &str,
    dir: &Path,
    cap: u64,
    cancel: &AtomicBool,
) -> Result<Option<MaterializedFile>, ViewerError> {
    materialize_with(requested, volume_id, dir, cap, &PendingOpen::new(), Some(cancel))
}

fn materialize_with(
    requested: &Path,
    volume_id: &str,
    dir: &Path,
    cap: u64,
    open: &PendingOpen,
    cancel: Option<&AtomicBool>,
) -> Result<Option<MaterializedFile>, ViewerError> {
    if let Some(entry) = extract_routed(requested, volume_id, TempSpot::viewer(dir), cap, open, cancel)? {
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
    tauri::async_runtime::block_on(pull_to_temp(
        volume,
        resolved.path,
        TempSpot::viewer(dir),
        cap,
        None,
        open,
        cancel,
    ))
    .map(Some)
}

/// The route-only materializer for `inspect_file`, with its per-path cancellation.
/// Uses the shared volume-manager routing, including archives on remote parents.
pub(crate) fn extract_if_routed_for_inspect(
    requested: &Path,
    volume_id: &str,
    cancel: &AtomicBool,
) -> Result<Option<MaterializedFile>, ViewerError> {
    extract_routed(
        requested,
        volume_id,
        TempSpot::viewer(&materialize_dir()),
        PREVIEW_CAP_BYTES,
        &PendingOpen::new(),
        Some(cancel),
    )
}

/// Route extraction with an explicit dir + cap, for tests.
#[cfg(test)]
pub(crate) fn extract_if_routed_with(
    requested: &Path,
    volume_id: &str,
    dir: &Path,
    cap: u64,
) -> Result<Option<MaterializedFile>, ViewerError> {
    extract_routed(
        requested,
        volume_id,
        TempSpot::viewer(dir),
        cap,
        &PendingOpen::new(),
        None,
    )
}

/// The route half of [`materialize_for_viewer_with`], watched by `open`.
fn extract_routed(
    requested: &Path,
    volume_id: &str,
    spot: TempSpot<'_>,
    cap: u64,
    open: &PendingOpen,
    cancel: Option<&AtomicBool>,
) -> Result<Option<MaterializedFile>, ViewerError> {
    let Some(route) = resolve_route(requested, volume_id) else {
        return Ok(None);
    };
    tauri::async_runtime::block_on(pull_to_temp(
        route.volume,
        route.entry_path,
        spot,
        cap,
        Some(route.kind),
        open,
        cancel,
    ))
    .map(Some)
}

/// A path a route serves: the volume the route minted, the path to read on it, and
/// which route it was.
struct Route {
    volume: std::sync::Arc<dyn Volume>,
    entry_path: PathBuf,
    kind: RoutedKind,
}

/// The route serving `requested`, or `None` when it has a file of its own.
fn resolve_route(requested: &Path, volume_id: &str) -> Option<Route> {
    // Only a path with no file of its own is materialized. The `.zip` file ITSELF
    // is a regular file: viewing it shows its raw bytes like any binary file
    // (extracting inner "" would address the archive ROOT — a directory — and
    // error). Pure string pre-filter (no I/O); `resolve` below does the
    // parent-aware confirm, so a mislabeled `.zip`, a remote-only archive, and a
    // `.git` that isn't a repository are all handled there.
    if !path_routes_over_its_parent(requested) {
        return None;
    }
    let resolved = tauri::async_runtime::block_on(get_volume_manager().resolve(volume_id, requested));
    let kind = resolved.routed?;
    // `None`: the route confirmed but the volume vanished (unmount / evict race). Treat
    // as unrouted; the caller's existence check surfaces NotFound.
    let volume = resolved.volume?;
    Some(Route {
        volume,
        entry_path: resolved.path,
        kind,
    })
}

/// Streams one file to a fresh temp subdir in `spot`, refusing an oversize file
/// before writing anything. `routed` is the route that minted `volume`, or `None`
/// for a volume the OS can't open.
async fn pull_to_temp(
    volume: std::sync::Arc<dyn Volume>,
    entry_path: PathBuf,
    spot: TempSpot<'_>,
    cap: u64,
    routed: Option<RoutedKind>,
    open: &PendingOpen,
    cancel: Option<&AtomicBool>,
) -> Result<MaterializedFile, ViewerError> {
    if let Some(stopped) = stopped_error(open, cancel) {
        return Err(stopped);
    }
    // Size + kind come from the volume's metadata (an archive's central directory,
    // the portal's tree entry, a phone's or server's stat), never a decompression or
    // a content read, so the refusal lands BEFORE we create a temp or stream a byte.
    let meta = volume
        .get_metadata(&entry_path)
        .await
        .map_err(|e| map_volume_error(e, routed))?;
    if let Some(stopped) = stopped_error(open, cancel) {
        return Err(stopped);
    }
    if meta.is_directory {
        return Err(ViewerError::IsDirectory);
    }
    let declared = meta.size.unwrap_or(0);
    if declared > cap {
        return Err(ViewerError::TooLargeToPreview { size: declared, cap });
    }

    let cleanup_dir = spot.dir.join(format!("{}{}", spot.prefix, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&cleanup_dir)?;
    let temp_file = cleanup_dir.join(temp_basename(&meta.name));

    // Any failure past this point must not leave the subdir behind.
    match stream_to_file(
        volume.as_ref(),
        &entry_path,
        &temp_file,
        cap,
        meta.size,
        routed,
        open,
        cancel,
    )
    .await
    {
        Ok(()) => Ok(MaterializedFile { temp_file, cleanup_dir }),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&cleanup_dir);
            Err(e)
        }
    }
}

/// Streams the file into `temp_file`, enforcing the byte-cap as a backstop against a
/// reported size that understates the real one. Records progress on `open` against
/// `declared_size` after every chunk, and stops once the viewer open is abandoned or
/// the inspector's per-path deadline flips `cancel`.
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
    cancel: Option<&AtomicBool>,
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
        if let Some(stopped) = stopped_error(open, cancel) {
            return Err(stopped);
        }
    }
    file.flush()?;
    Ok(())
}

fn stopped_error(open: &PendingOpen, cancel: Option<&AtomicBool>) -> Option<ViewerError> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
        Some(ViewerError::Cancelled)
    } else {
        open.abandoned_error()
    }
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
/// The archive family has its own frontend copy under `ViewerError::Archive`, which
/// the FE renders without inspecting the typed failure or message string. The typed
/// failure preserves `NotSupported` for non-UI consumers. A portal read and a plain
/// pull have no such family — a repository that can't be opened or a phone that
/// dropped mid-read is a fault, not a kind of file — so they stay a plain `Io`.
pub(super) fn map_volume_error(err: VolumeError, routed: Option<RoutedKind>) -> ViewerError {
    match err {
        VolumeError::NotFound(path) => ViewerError::NotFound { path },
        VolumeError::IsADirectory(_) => ViewerError::IsDirectory,
        // Whichever route read it: a zip in cold storage is as unreadable as a
        // file there, and the fix is the same restore.
        VolumeError::ColdStorage(_) => ViewerError::ColdStorage,
        other => match routed {
            Some(RoutedKind::Archive) => ViewerError::Archive {
                failure: match other {
                    VolumeError::NotSupported => ArchiveFailureKind::Unsupported,
                    VolumeError::NeedsPassword { .. } => ArchiveFailureKind::NeedsPassword,
                    _ => ArchiveFailureKind::Unreadable,
                },
                message: other.to_string(),
            },
            Some(RoutedKind::GitPortal) | None => ViewerError::Io {
                message: other.to_string(),
            },
        },
    }
}

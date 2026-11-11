//! Tauri commands that ask about a path before anything lists or writes it:
//! the platform's name and path limits, and whether a path exists on its volume.

use std::path::{Path, PathBuf};

use tokio::time::Duration;

use crate::file_system::volume::manager::get_volume_manager;
use cmdr_fs::deadline::{TimedOut, timeout_detached_typed};
use cmdr_fs::validation::{MAX_NAME_BYTES, MAX_PATH_BYTES};
use cmdr_fs::volume::Presence;
use cmdr_operations::held_in_another_spelling;

use super::expand_tilde;

const PATH_EXISTS_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PathLimits {
    pub max_name_bytes: usize,
    pub max_path_bytes: usize,
}

#[tauri::command]
#[specta::specta]
pub fn get_path_limits() -> PathLimits {
    PathLimits {
        max_name_bytes: MAX_NAME_BYTES,
        max_path_bytes: MAX_PATH_BYTES,
    }
}

/// Returns `TimedOut<bool>` so the frontend can distinguish a real "doesn't exist"
/// from "we couldn't tell" (timeout, an SMB volume in `Disconnected` state, or a
/// listed device nobody has dialed, like an ADB phone before its pane connects). Without this
/// distinction, the directory-eviction poll in `FilePane.svelte` evicts users from a
/// network folder on any transient connection blip.
#[tauri::command]
#[specta::specta]
pub async fn path_exists(volume_id: Option<String>, path: String) -> TimedOut<bool> {
    exists_on_volume(volume_id, path, Spelling::Exact).await
}

/// [`path_exists`] for a write's DESTINATION, asked by the transfer and compress
/// dialogs: a name the volume holds in another Unicode spelling counts as there
/// (`cmdr_operations::held_in_another_spelling`), because the write that follows
/// lands on that entry or refuses it. So copy/move don't promise to create a
/// folder they'll merge into, and compress warns before it overwrites.
///
/// Costs one listing of the parent, only after the exact probe missed a non-ASCII
/// name on a volume that matches names byte for byte. A listing that fails reads
/// as "couldn't tell", ❌ never "not there".
#[tauri::command]
#[specta::specta]
pub async fn destination_exists(volume_id: Option<String>, path: String) -> TimedOut<bool> {
    exists_on_volume(volume_id, path, Spelling::AnyForm).await
}

/// Whether a local `stat` error means "not there": only `ENOENT` and `ENOTDIR` do.
/// Anything else (EIO, ETIMEDOUT, EACCES, ENXIO from a struggling FUSE daemon or a
/// dying disk) says nothing about the path, so the caller answers "couldn't tell".
fn local_miss_is_definite(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// Existence on a volume `std::fs` can reach: `Err(())` is "couldn't tell".
///
/// ❗ Not `Volume::exists`, which folds every `stat` error into `false`: that's the
/// right answer for the copy/move conflict checks that call it, and the wrong one
/// here, where a `false` walks a pane off a slow FUSE mount (pCloud's `pcloudfs`,
/// cmdr-reports#4) as if the drive were gone. Same `symlink_metadata`, so a broken
/// symlink still counts as there.
async fn local_exists(path: PathBuf) -> Result<bool, ()> {
    match tokio::task::spawn_blocking(move || std::fs::symlink_metadata(path)).await {
        Ok(Ok(_)) => Ok(true),
        Ok(Err(e)) if local_miss_is_definite(&e) => Ok(false),
        _ => Err(()),
    }
}

/// Which spellings of a path's name an existence check counts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Spelling {
    /// These bytes only. [`path_exists`]: a pane's remembered path and the
    /// directory-eviction poll ask about a path they then list, and a "yes" for a
    /// look-alike would send them to a path that misses.
    Exact,
    /// These bytes, or the name in another Unicode form. [`destination_exists`].
    AnyForm,
}

async fn exists_on_volume(volume_id: Option<String>, path: String, spelling: Spelling) -> TimedOut<bool> {
    let volume_id = volume_id.unwrap_or_else(|| crate::volumes::local_volume_id_for_path(&path));

    // For local volumes, expand tilde
    let expanded_path = if volume_id == "root" { expand_tilde(&path) } else { path };

    // Resolve so an archive-inner path checks existence inside the `.zip`.
    if let Some(volume) = get_volume_manager()
        .resolve(&volume_id, Path::new(&expanded_path))
        .await
        .volume
    {
        // On a remote volume an immediate `Absent` may be the session
        // dying (`clone_session` returns `Err`) rather than the path actually missing.
        // Snapshot whether this volume HAS a session, so the answer can be re-checked
        // against it below.
        let has_session = volume.connection_state().is_some();
        // A live session waits out a busy server's stall (`deadline::io_budget`):
        // a "couldn't tell" here lands a volume switch on the share root and walks
        // a pane past a parent that's still there.
        let budget = cmdr_fs::deadline::io_budget(volume.connection_state(), PATH_EXISTS_TIMEOUT);

        // The transfer dialog asks about its VOLUME-RELATIVE destination box
        // (`/photos`), the panes about absolute paths. Anchoring folds both into
        // what the volume accepts; without it a share answers "doesn't exist"
        // for every subfolder, and the dialog promises to create one that's
        // already there.
        let path_for_check = cmdr_fs::volume::root_anchored(volume.root(), Path::new(&expanded_path));
        // Detached, so a deadline never abandons an MTP transaction mid-flight
        // (`crates/cmdr-fs/src/deadline/CLAUDE.md`). `Err(())` is "couldn't tell": the deadline, or
        // a look-alike listing that failed.
        let probe = {
            let volume = std::sync::Arc::clone(&volume);
            async move {
                let exists = if volume.local_path().is_some() {
                    local_exists(path_for_check.clone()).await?
                } else {
                    // `presence`, ❌ never `exists`: a refusal (a share folder this
                    // account may not read) is "couldn't tell", and `exists` folds
                    // it into a "no" the pane reads as "deleted" (#1018).
                    match volume.presence(&path_for_check).await {
                        Presence::Present => true,
                        Presence::Absent => false,
                        Presence::Unknown => return Err(()),
                    }
                };
                if exists || spelling == Spelling::Exact {
                    return Ok(exists);
                }
                held_in_another_spelling(volume.as_ref(), &path_for_check)
                    .await
                    .map_err(|_| ())
            }
        };
        match timeout_detached_typed(budget, || (), |_| (), probe).await {
            Ok(exists) => {
                // The session dropped while we asked? Then the `false` we got back is
                // meaningless. Surface it as a timeout-equivalent so callers know.
                if !exists && has_session && !volume.connection_state().is_some_and(|s| s.is_live()) {
                    return TimedOut {
                        data: false,
                        timed_out: true,
                    };
                }
                TimedOut {
                    data: exists,
                    timed_out: false,
                }
            }
            Err(_) => TimedOut {
                data: false,
                timed_out: true,
            },
        }
    } else if crate::device_volumes::provider_for_volume_id(&volume_id)
        .await
        .is_some()
        || crate::server_volumes::place_root(&volume_id).is_some()
    {
        // A device its provider lists but nobody has dialed (an ADB phone before its
        // pane's connect lands, or after an eject), or a saved SFTP / WebDAV server
        // nobody has connected. Nothing has looked, so the honest answer is "couldn't
        // tell", ❌ never a `false` that every caller reads as "gone".
        TimedOut {
            data: false,
            timed_out: true,
        }
    } else {
        // Fallback for unknown volumes (shouldn't happen in practice)
        let path_buf = PathBuf::from(expanded_path);
        let result = tokio::time::timeout(
            PATH_EXISTS_TIMEOUT,
            tokio::task::spawn_blocking(move || path_buf.exists()),
        )
        .await;
        match result {
            Ok(Ok(exists)) => TimedOut {
                data: exists,
                timed_out: false,
            },
            _ => TimedOut {
                data: false,
                timed_out: true,
            },
        }
    }
}

#[cfg(test)]
#[path = "destination_exists_test.rs"]
mod destination_exists_test;

#[cfg(test)]
#[path = "path_exists_budget_test.rs"]
mod path_exists_budget_test;

#[cfg(test)]
#[path = "path_exists_errors_test.rs"]
mod path_exists_errors_test;

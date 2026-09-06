//! Where a transfer's volumes and destination path come from, and the three
//! entry points that route one.
//!
//! Every cross-volume transfer starts with the same two questions — which volume
//! owns these sources, and what path does the destination volume actually accept —
//! and then forks on what it found: a `.zip` source extracts out, a `.zip`
//! destination becomes one archive changeset, anything else goes to the
//! cross-volume engine. That whole shape used to live inside the `#[tauri::command]`
//! bodies, which made it reachable only from a window.
//!
//! It has to be reachable from the backend too: an approved suggestion group is
//! started by Rust, with an injected sink, and an EXTRACT is just a copy whose
//! source resolves to an `ArchiveVolume` — so without these helpers it could only
//! be reached by duplicating the routing, and a second copy of a fork this
//! consequential drifts.
//!
//! ⚠️ **The sink is still built at the IPC edge and injected**, exactly as before
//! (`DETAILS.md` § "Key decisions"). Nothing here constructs a `TauriEventSink`;
//! every function takes an `Arc<dyn OperationEventSink>` and the commands are thin
//! wrappers that build one and pass it in.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::archive_edit::{compress_start, route_archive_copy_into};
use super::event_sinks::OperationEventSink;
use super::source_binding::ExpectedSources;
use super::transfer::volume::{copy_between_volumes, move_between_volumes};
use super::types::{ReadOnlySide, VolumeCopyConfig, WriteOperationError, WriteOperationStartResult};
use crate::file_system::volume::Volume;
use crate::file_system::volume::manager::{RoutedKind, get_volume_manager, path_routes_over_its_parent};
use crate::operation_log::types::Initiator;

/// Turns the destination a caller sent into the path the destination volume
/// accepts: home shortcut expanded (local only), then anchored at the volume's
/// root.
///
/// The transfer dialog's destination box is VOLUME-RELATIVE (`/photos`) because
/// the volume is a separate dropdown next to it, while a pane sends the absolute
/// path it displays. `cmdr_fs::volume::root_anchored` folds both into the absolute
/// form, and it's idempotent, so neither caller has to know which one it holds.
/// Skipping it is what made a move into an SMB subfolder fail instantly
/// (ERR-XCP5Q): `SmbVolume` reads `/photos` as an absolute path outside its mount
/// and answers `NotFound` rather than guessing.
///
/// `~` expands for local volumes only, per the "never tilde-expand MTP/network
/// paths" rule: on a share, `~` is an ordinary folder name.
pub(crate) fn resolve_dest_path(dest_volume: &Arc<dyn Volume>, dest_path: String) -> PathBuf {
    let expanded = if dest_volume.local_path().is_some() {
        crate::commands::file_system::expand_tilde(&dest_path)
    } else {
        dest_path
    };
    cmdr_fs::volume::root_anchored(dest_volume.root(), Path::new(&expanded))
}

/// Whether transferring `source_path` to `dest_path` would land it on that very
/// item, which both engines answer by duplicating rather than by asking
/// (`transfer/DETAILS.md` § "Self-collision (duplicating in place)").
///
/// The pre-flight conflict scan asks this AHEAD of the engines, so it has to
/// give the answer the engine that will actually run gives, and the two engines
/// answer identity differently. Same fork as `copy_between_volumes` makes: a
/// both-local transfer is handed to the local engine, whose rule is `dev+ino`
/// (`validation::is_same_file`, which settles a symlinked parent and a
/// case- or NFC/NFD-differing route); everything else stays on the cross-volume
/// engine, whose rule is one volume, the same parent, and a folded leaf
/// (`is_the_same_item`).
/// Answering it here rather than in each backend's `scan_for_conflicts` is what
/// keeps `SourceItemInfo` a name-and-size DTO with no source path in it.
///
/// The local arm anchors both sides the way that fork does: `join` for the
/// source (an absolute clipboard path wins over the volume root) and
/// `root_anchored` for the destination (idempotent, so an already-absolute
/// listing path passes through).
pub(crate) fn transfer_would_land_on_its_source(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
) -> bool {
    match (source_volume.local_path(), dest_volume.local_path()) {
        (Some(source_root), Some(dest_root)) => super::validation::is_same_file(
            &source_root.join(source_path),
            &cmdr_fs::volume::root_anchored(&dest_root, dest_path),
        ),
        _ => super::transfer::volume::is_the_same_item(source_volume, source_path, dest_volume, dest_path),
    }
}

/// Resolves a batch's source volume, routing a source that a ROUTE serves —
/// inside a `.zip`, or inside a repo's virtual `.git` trees — to that read-only
/// volume, and saying which route it was.
///
/// Reading through the routed volume is what makes an extract-out and a copy out
/// of a snapshot ordinary cross-volume copies: both read with `open_read_stream`
/// and walk with `scan_for_copy`, neither has a path `std::fs` could open. A
/// source left on its parent volume would take the local-to-local fast path
/// against a path with no inode and stall the transfer on its scan.
///
/// One `source_volume_id` per batch means no straddle risk — every path shares
/// the same archive, the same repo, or none — so the first path decides.
///
/// [`RoutedKind`] rather than a bool, because the two routes part ways on a
/// MOVE: an archive can delete the source entries afterwards, a snapshot can't.
pub(crate) async fn resolve_source_volume(
    volume_id: &str,
    first_path: Option<&PathBuf>,
) -> Option<(Arc<dyn Volume>, Option<RoutedKind>)> {
    let manager = get_volume_manager();
    let Some(path) = first_path else {
        return manager.get(volume_id).map(|v| (v, None));
    };
    // Pure string gate, so an ordinary local/remote path skips the resolve below.
    if !path_routes_over_its_parent(path) {
        return manager.get(volume_id).map(|v| (v, None));
    }
    // Parent-aware resolve (local `std::fs` OR remote via the parent's own I/O)
    // for the archive half, lexical for the git half. A mislabeled `.zip`, a
    // `.git` that isn't a repository, or a switched-off portal all degrade to the
    // parent volume with no route.
    let resolved = manager.resolve(volume_id, path).await;
    match resolved.volume {
        Some(volume) => Some((volume, resolved.routed)),
        // The volume vanished between the route and the read (an unmount race).
        None => manager.get(volume_id).map(|v| (v, None)),
    }
}

/// Refuses a MOVE whose sources sit on a read-only routed volume that has no
/// delete to give: a copy out would run, and then the source half of the move
/// could never happen. Refusing before anything starts is the honest outcome,
/// and the copy the user actually can do is one keystroke away.
///
/// Named for the source volume and tagged [`ReadOnlySide::Source`], so the
/// frontend renders the sentence about the half that actually refused: "`.git`
/// is read-only. You can copy files out of it, but not move them out." Telling
/// this user to choose a different destination would send them to fix the half
/// that was fine.
fn source_cannot_give_up_its_files(source_volume: &Arc<dyn Volume>) -> WriteOperationError {
    WriteOperationError::ReadOnlyDevice {
        path: source_volume.root().display().to_string(),
        device_name: Some(source_volume.name().to_string()),
        side: ReadOnlySide::Source,
    }
}

/// Refuses a transfer INTO a read-only routed volume (a repo's virtual `.git`
/// trees). A drop onto a snapshot has nowhere to land: every mutation method on
/// the volume answers `NotSupported`, so without this the transfer would start,
/// build its plan, and fail on the first write with a half-drawn dialog.
///
/// A `.zip` destination is NOT this case: it routes to the archive-edit driver,
/// which really does write.
fn destination_takes_no_writes(dest_volume: &Arc<dyn Volume>) -> WriteOperationError {
    WriteOperationError::ReadOnlyDevice {
        path: dest_volume.root().display().to_string(),
        device_name: Some(dest_volume.name().to_string()),
        side: ReadOnlySide::Destination,
    }
}

/// Refuses a transfer whose source binding the route it landed on cannot honour.
///
/// The archive-changeset routes plan their own work from a `WalkDir` rather than
/// the per-source engine, so they have nowhere to apply an `ExpectedSources` yet.
/// Running one unbound is the exact failure a binding exists to prevent, acting
/// on a file nobody reviewed, and dropping it silently is worse than refusing
/// because nothing downstream would ever say so. Never run it unbound instead.
fn route_cannot_hold_a_binding(route: &str) -> WriteOperationError {
    WriteOperationError::IoError {
        path: String::new(),
        message: format!(
            "This transfer runs as an archive changeset ({route}), which can't yet hold its sources to what was reviewed."
        ),
    }
}

fn source_volume_missing(volume_id: &str) -> WriteOperationError {
    WriteOperationError::IoError {
        path: volume_id.to_string(),
        message: format!("Source volume '{}' not found", volume_id),
    }
}

fn dest_volume_missing(volume_id: &str) -> WriteOperationError {
    WriteOperationError::IoError {
        path: volume_id.to_string(),
        message: format!("Destination volume '{}' not found", volume_id),
    }
}

/// Starts a copy across volume types, resolving both ends first.
///
/// **This is also how an EXTRACT runs.** There is no `Extract` in
/// `WriteOperationType` and there should not be one: pulling entries out of a
/// `.zip` is a copy whose source volume happens to be an `ArchiveVolume`, which
/// [`resolve_source_volume`] arranges. `ArchiveSubkind::Extract` is an
/// operation-log label, not a second execution path.
#[allow(
    clippy::too_many_arguments,
    reason = "both volumes travel with their ids, and the source binding is a separate input from the config on purpose (a bound op and a user-started one must share one config); a bag struct would shuffle the same fields"
)]
pub(crate) async fn start_volume_copy(
    events: Arc<dyn OperationEventSink>,
    source_volume_id: String,
    source_paths: Vec<PathBuf>,
    dest_volume_id: String,
    dest_path: String,
    config: VolumeCopyConfig,
    initiator: Initiator,
    expected_sources: Option<ExpectedSources>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Route a source a route serves to its read-only volume: an archive-inner
    // batch to its `ArchiveVolume` (extract-out), a snapshot path to the repo's
    // `GitPortalVolume` (copy out of `.git/branches/<name>/…`). Both then read
    // through the cross-volume engine.
    let (source_volume, _source_route) = resolve_source_volume(&source_volume_id, source_paths.first())
        .await
        .ok_or_else(|| source_volume_missing(&source_volume_id))?;

    // Resolve the destination. A `.zip`-crossing dest routes the whole copy to
    // the managed archive-edit driver (one `{ add }` changeset).
    let dest_resolved = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_path))
        .await;
    let dest_volume = dest_resolved
        .volume
        .ok_or_else(|| dest_volume_missing(&dest_volume_id))?;

    if dest_resolved.routed == Some(RoutedKind::GitPortal) {
        return Err(destination_takes_no_writes(&dest_volume));
    }

    if dest_resolved.routed == Some(RoutedKind::Archive) {
        if expected_sources.is_some() {
            return Err(route_cannot_hold_a_binding("copy into a zip"));
        }
        return route_archive_copy_into(
            events,
            source_volume,
            source_paths,
            PathBuf::from(&dest_path),
            dest_volume_id,
            config.conflict_resolution,
            config.progress_interval_ms,
            false,
            config.compression_level,
            config.preview_id.clone(),
        )
        .await;
    }

    let dest_path = resolve_dest_path(&dest_volume, dest_path);
    copy_between_volumes(
        events,
        source_volume_id,
        source_volume,
        source_paths,
        dest_volume_id,
        dest_volume,
        dest_path,
        config,
        initiator,
        expected_sources,
    )
    .await
}

/// Starts a move across volume types. Handles same-volume (native rename/move),
/// both-local (native move), cross-volume (copy + delete), out of a `.zip` (the
/// compound move-out op), and into one (an archive changeset).
#[allow(
    clippy::too_many_arguments,
    reason = "both volumes travel with their ids, and the source binding is a separate input from the config on purpose (a bound op and a user-started one must share one config); a bag struct would shuffle the same fields"
)]
pub(crate) async fn start_volume_move(
    events: Arc<dyn OperationEventSink>,
    source_volume_id: String,
    source_paths: Vec<PathBuf>,
    dest_volume_id: String,
    dest_path: String,
    config: VolumeCopyConfig,
    initiator: Initiator,
    expected_sources: Option<ExpectedSources>,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // An archive SOURCE routes to the compound move-out op (extract via the copy
    // engine, then a batch `{ delete }` archive rewrite once the extract lands).
    // A git-portal source has no delete to pair with the copy, so it's refused
    // below rather than routed anywhere.
    let (source_volume, source_route) = resolve_source_volume(&source_volume_id, source_paths.first())
        .await
        .ok_or_else(|| source_volume_missing(&source_volume_id))?;

    // A snapshot can be copied out of, never moved out of: `.git/branches/…` has
    // no file to remove once the copy lands. Refused before the destination is
    // even resolved, so nothing is written and nothing is half-done.
    if source_route == Some(RoutedKind::GitPortal) {
        return Err(source_cannot_give_up_its_files(&source_volume));
    }

    let dest_resolved = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_path))
        .await;
    let dest_volume = dest_resolved
        .volume
        .ok_or_else(|| dest_volume_missing(&dest_volume_id))?;

    if dest_resolved.routed == Some(RoutedKind::GitPortal) {
        return Err(destination_takes_no_writes(&dest_volume));
    }

    // Move OUT of a zip. Takes precedence over the dest-archive branch: a
    // zip→zip move extracts out first (the dest-archive case degrades to a copy
    // failure inside the extract, never data loss).
    if source_route == Some(RoutedKind::Archive) {
        if expected_sources.is_some() {
            return Err(route_cannot_hold_a_binding("move out of a zip"));
        }
        let dest_path = resolve_dest_path(&dest_volume, dest_path);
        return super::archive_edit::route_archive_move_out(
            events,
            source_volume_id,
            source_volume,
            source_paths,
            dest_volume_id,
            dest_volume,
            dest_path,
            config,
        )
        .await;
    }

    // A move INTO a zip routes to the managed edit driver as one `{ add }`
    // changeset; the local sources are deleted after the commit (move invariant).
    if dest_resolved.routed == Some(RoutedKind::Archive) {
        if expected_sources.is_some() {
            return Err(route_cannot_hold_a_binding("move into a zip"));
        }
        return route_archive_copy_into(
            events,
            source_volume,
            source_paths,
            PathBuf::from(&dest_path),
            dest_volume_id,
            config.conflict_resolution,
            config.progress_interval_ms,
            true,
            config.compression_level,
            config.preview_id.clone(),
        )
        .await;
    }

    let dest_path = resolve_dest_path(&dest_volume, dest_path);
    move_between_volumes(
        events,
        source_volume_id,
        source_volume,
        source_paths,
        dest_volume_id,
        dest_volume,
        dest_path,
        config,
        initiator,
        expected_sources,
    )
    .await
}

/// Compresses `source_paths` into a NEW zip at `dest_zip_path` on
/// `dest_volume_id`. Seeds a valid empty zip, then copies the sources in as one
/// changeset. The destination may be LOCAL or REMOTE (SMB/MTP).
///
/// ⚠️ **An existing archive at the target is overwritten, deliberately.** The seed
/// is unconditional, and a compress that replaced a prior archive is the one
/// transfer this engine can't reverse (the prior bytes aren't retained). That is a
/// fact to DISCLOSE before someone approves it, ❌ never a refusal here: if a
/// person can do it from the dialog, an operation they approved does the same
/// thing.
pub(crate) async fn start_volume_compress(
    events: Arc<dyn OperationEventSink>,
    source_volume_id: String,
    source_paths: Vec<PathBuf>,
    dest_volume_id: String,
    dest_zip_path: String,
    config: VolumeCopyConfig,
    initiator: Initiator,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    // Route a routed source batch to its read-only volume, so compressing reads
    // through it: from inside a `.zip`, or from a repo's snapshots.
    let (source_volume, _source_route) = resolve_source_volume(&source_volume_id, source_paths.first())
        .await
        .ok_or_else(|| source_volume_missing(&source_volume_id))?;

    // The new `.zip` doesn't exist yet, so `resolve` returns the PARENT drive volume
    // (nothing routes for a non-existent path) — the drive the seed is written to. `compress_start` bypasses the archive-boundary resolve on its own.
    let dest_volume = get_volume_manager()
        .resolve(&dest_volume_id, Path::new(&dest_zip_path))
        .await
        .volume
        .ok_or_else(|| dest_volume_missing(&dest_volume_id))?;

    let dest_zip_path = resolve_dest_path(&dest_volume, dest_zip_path);

    compress_start(
        events,
        source_volume,
        source_paths,
        dest_zip_path,
        dest_volume_id,
        config.conflict_resolution,
        config.progress_interval_ms,
        config.compression_level,
        config.preview_id.clone(),
        initiator,
    )
    .await
}

#[cfg(test)]
mod tests;

//! Translate live MTP PTP change events into per-volume index writes.
//!
//! The MTP event loop (`mtp/connection/event_loop.rs`) already turns PTP events
//! into targeted pane refreshes. This module is the SECOND consumer: it keeps the
//! volume's persisted index in sync so directory sizes stay correct while the
//! device is Fresh, even with no pane open. It's the MTP analogue of
//! `transports/smb/watch.rs`; the same invariants hold (single-writer-per-DB, ids
//! resolved on the writer in queue order, buffer-during-scan), with two
//! MTP-specific twists.
//!
//! ## Pathful adds vs. pathless removals (the core MTP difference)
//!
//! PTP events carry an opaque object handle, not a path:
//!
//! - **`ObjectAdded` / `ObjectInfoChanged`** resolve the handle to a path via the
//!   handle→path resolver (`resolve_handle_to_path`), so the event loop hands us the
//!   resolved storage-relative path plus the object's metadata. We upsert under
//!   the resolved parent, STORING THE HANDLE in the index's `inode` column (the
//!   same column the MTP scan fills) so a later removal can find this row.
//! - **`ObjectRemoved`** can't resolve to a path (the object is gone, so
//!   `GetObjectInfo` fails). Instead we resolve it against the index by the STORED
//!   HANDLE (`find_entry_by_inode`) — the precedent the plan calls out — and
//!   delete that entry (a subtree if it was a directory).
//!
//! ## Path space
//!
//! The MTP index `ROOT_ID` is the storage root (the scan maps the storage root to
//! `ROOT_ID`), and the resolver produces storage-relative paths (`/DCIM/Camera`),
//! so — unlike SMB — there's no mount-prefix to strip here: a resolved path is
//! already in the index's path space. `store::resolve_path` walks it from
//! `ROOT_ID` directly.
//!
//! ## Ordering, coupling, and buffering
//!
//! Identical to SMB: enqueue the change on the volume's writer FIRST (the writer
//! resolves its ids on its own connection, `send_resolved`, so it sees every row
//! an earlier change still holds uncommitted), then
//! `EmitDirUpdated` so the FE re-reads sizes after the write commits; never open a
//! write connection here; BUFFER changes that arrive during a full scan and replay
//! them after, so they aren't lost against the rebuilding index.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use crate::indexing::lifecycle::state;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::ignore_poison::IgnorePoison;

/// A resolved MTP add/change: the object's storage-relative path plus the
/// metadata needed to upsert it, and its object handle (stored in `inode`).
/// Built by the event loop from the handle→path resolver + `ObjectInfo`.
#[derive(Debug, Clone)]
pub(crate) struct MtpUpsert {
    /// Storage-relative path of the changed object (e.g. `/DCIM/Camera/IMG.jpg`).
    /// Already in the index path space (the index root is the storage root).
    pub path: PathBuf,
    /// The PTP object handle, stored in the index `inode` so a later
    /// `ObjectRemoved{handle}` resolves to this row.
    pub handle: u32,
    pub is_directory: bool,
    /// Logical size in bytes (`None` for directories).
    pub size: Option<u64>,
    pub modified_at: Option<u64>,
}

/// Per-volume buffer of MTP changes that arrived DURING a full (re)scan. Same
/// rationale and bound as the SMB buffer (`transports/smb/watch`): a change to an
/// already-walked dir can't be applied against the gutted, mid-rebuild index, so
/// it's stashed and replayed after the scan's aggregation lands.
static SCAN_CHANGE_BUFFER: LazyLock<Mutex<HashMap<String, BufferedVolume>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Cap on buffered mid-scan changes per volume (mirrors SMB). A storm during a
/// long scan trips overflow ⇒ Stale at replay rather than growing unbounded.
const MAX_BUFFERED_CHANGES: usize = 50_000;

/// One MTP volume's mid-scan change buffer plus its overflow flag.
#[derive(Default)]
struct BufferedVolume {
    changes: Vec<BufferedChange>,
    overflowed: bool,
}

/// A buffered MTP change.
///
/// During a scan we buffer the RAW object handle for adds/changes
/// (`UpsertHandle`), NOT a resolved `MtpUpsert` — resolving means a device round
/// trip, and the whole point of buffering during a scan is to keep the contended
/// device free for foreground work (the gate-before-resolve fix). The handle is
/// resolved at replay time, when the scan has ended and the device is idle.
/// Removals already buffer the bare handle (the object is gone, nothing to
/// resolve). The `Upsert` variant carries an already-resolved change for the
/// non-scanning live path, which buffers nothing.
#[derive(Debug, Clone)]
enum BufferedChange {
    /// A resolved add/change (live path; not used while scanning).
    Upsert(MtpUpsert),
    /// An add/change buffered during a scan as a raw `(storage_id, handle)`,
    /// resolved to a path + metadata at replay time.
    UpsertHandle { storage_id: u32, handle: u32 },
    /// A pathless removal handle (resolved against the index by stored handle).
    Remove(u32),
}

/// The single index-write a resolved MTP change maps to. Mirrors `transports/smb/watch`'s
/// `ResolvedWrite`, but the upsert carries the object handle (→ `inode`).
enum ResolvedWrite {
    Upsert {
        parent_id: i64,
        name: String,
        is_directory: bool,
        size: Option<u64>,
        modified_at: Option<u64>,
        handle: u32,
    },
    DeleteFile(i64),
    DeleteSubtree(i64),
}

impl ResolvedWrite {
    fn into_message(self) -> WriteMessage {
        match self {
            ResolvedWrite::Upsert {
                parent_id,
                name,
                is_directory,
                size,
                modified_at,
                handle,
            } => WriteMessage::UpsertEntryV2 {
                parent_id,
                name,
                is_directory,
                is_symlink: false,
                // MTP reports one size; mirror logical into physical so dir_stats'
                // physical totals populate (the scan does the same).
                logical_size: if is_directory { None } else { size },
                physical_size: if is_directory { None } else { size },
                modified_at,
                // Store the object handle so `ObjectRemoved{handle}` resolves here.
                inode: Some(u64::from(handle)),
                nlink: None,
            },
            ResolvedWrite::DeleteFile(id) => WriteMessage::DeleteEntryById(id),
            ResolvedWrite::DeleteSubtree(id) => WriteMessage::DeleteSubtreeById(id),
        }
    }
}

/// Decide whether an MTP `ObjectAdded` / `ObjectInfoChanged` handle should be
/// BUFFERED (the volume is mid-scan) rather than resolved live, and buffer the
/// RAW handle if so. Returns `true` when buffered (the caller must NOT resolve —
/// this is the gate-before-resolve fix: no device round trip during a scan),
/// `false` when the caller should resolve and apply the change live.
///
/// Synchronous and device-free: it only reads the registry's scanning flag and,
/// when scanning, stashes `(storage_id, handle)` for post-scan replay. Returns
/// `false` for an unindexed/absent volume too (nothing to buffer; the live path
/// will then no-op on the missing index).
pub(crate) fn buffer_mtp_handle_if_scanning(volume_id: &str, storage_id: u32, handle: u32) -> bool {
    let scanning = match state::get_writer_and_flux_for(volume_id) {
        Some((_, scanning)) => scanning,
        None => return false,
    };
    if scanning {
        buffer_change_during_scan(volume_id, BufferedChange::UpsertHandle { storage_id, handle });
        true
    } else {
        false
    }
}

/// Apply a resolved MTP `ObjectAdded` / `ObjectInfoChanged` to the volume's
/// index, if the volume has a live (`Running`) index. BUFFERS during a scan.
///
/// `upsert` carries the resolved storage-relative path + metadata + handle.
/// Synchronous: only writer enqueues (the writer resolves the ids).
///
/// The live event path now gates BEFORE resolving (`buffer_mtp_handle_if_scanning`),
/// so this is reached only when the volume is NOT scanning; the in-scan buffer
/// branch here is a defensive belt-and-braces (e.g. a scan that started between
/// the gate check and this call).
pub(crate) fn apply_mtp_added_or_changed(volume_id: &str, upsert: MtpUpsert) {
    let (writer, scanning) = match state::get_writer_and_flux_for(volume_id) {
        Some(pair) => pair,
        None => return,
    };
    if scanning {
        buffer_change_during_scan(volume_id, BufferedChange::Upsert(upsert));
        return;
    }
    apply_upsert(volume_id, &writer, &upsert);
}

/// Apply a pathless MTP `ObjectRemoved{handle}` to the volume's index, if live.
/// Resolves the entry by its STORED handle (`find_entry_by_inode`) — the object
/// is gone, so there's no path to resolve. BUFFERS during a scan. No-op if the
/// handle was never indexed (a removal for an object we never saw).
pub(crate) fn apply_mtp_removed(volume_id: &str, handle: u32) {
    let (writer, scanning) = match state::get_writer_and_flux_for(volume_id) {
        Some(pair) => pair,
        None => return,
    };
    if scanning {
        buffer_change_during_scan(volume_id, BufferedChange::Remove(handle));
        return;
    }
    send_remove(&writer, handle);
}

/// Enqueue an add/change against the live index.
fn apply_upsert(volume_id: &str, writer: &IndexWriter, upsert: &MtpUpsert) {
    send_upsert(writer, upsert.clone());

    // Sequence the FE refresh AFTER the index write (the writer fires
    // `DirsUpdated` only once the upsert commits). Emit for the parent's
    // absolute MTP URL so the FE listing-cache key matches. Sent whether or not
    // the change resolves to a write, which is only known on the writer; a
    // refresh of an unchanged folder costs one sizes read.
    if let Some(parent) = upsert.path.parent() {
        let parent_url = mtp_url_for(volume_id, parent);
        let _ = writer.send(WriteMessage::EmitDirUpdated(vec![parent_url]));
    }
}

/// Enqueue an add/change for the writer to resolve on its OWN connection.
fn send_upsert(writer: &IndexWriter, upsert: MtpUpsert) {
    send_resolved(writer, move |conn| resolve_upsert(conn, &upsert));
}

/// Enqueue a removal for the writer to resolve on its OWN connection.
fn send_remove(writer: &IndexWriter, handle: u32) {
    send_resolved(writer, move |conn| resolve_remove(conn, handle));
}

/// Enqueue `resolve` to run on the writer's connection, at its turn in the
/// queue. ❗ Never against a read connection: one misses every row an earlier
/// change still holds in the writer's uncommitted batch, so a file added right
/// behind its new folder was never indexed, and a removal right behind its add
/// left the row for good.
fn send_resolved(
    writer: &IndexWriter,
    resolve: impl FnOnce(&rusqlite::Connection) -> Option<ResolvedWrite> + Send + 'static,
) {
    let resolver =
        move |conn: &rusqlite::Connection| resolve(conn).map(ResolvedWrite::into_message).into_iter().collect();
    if let Err(e) = writer.send(WriteMessage::ResolveThenWrite(Box::new(resolver))) {
        log::debug!(target: "indexing::transports::mtp::watch", "writer send failed (writer gone): {e}");
    }
}

/// Decide the index write for a resolved add/change, given a read connection.
/// `None` when the parent isn't indexed (nothing to attach to). Pure over the DB
/// state so the mapping is testable against a seeded index.
fn resolve_upsert(conn: &rusqlite::Connection, upsert: &MtpUpsert) -> Option<ResolvedWrite> {
    let parent_rel = upsert
        .path
        .parent()
        .map(path_to_index_str)
        .unwrap_or_else(|| "/".to_string());
    let name = upsert.path.file_name()?.to_string_lossy().into_owned();
    let parent_id = store::resolve_path(conn, &parent_rel).ok().flatten()?;
    Some(ResolvedWrite::Upsert {
        parent_id,
        name,
        is_directory: upsert.is_directory,
        size: upsert.size,
        modified_at: upsert.modified_at,
        handle: upsert.handle,
    })
}

/// Decide the index write for a pathless removal, resolving the entry by its
/// stored handle. `None` if the handle was never indexed (no row to delete) —
/// the stat-verify analogue: a removal for an object we never saw is a no-op.
fn resolve_remove(conn: &rusqlite::Connection, handle: u32) -> Option<ResolvedWrite> {
    let entry_id = IndexStore::find_entry_by_inode(conn, u64::from(handle))
        .ok()
        .flatten()?;
    let is_dir = IndexStore::get_entry_by_id(conn, entry_id)
        .ok()
        .flatten()
        .map(|e| e.is_directory)
        .unwrap_or(false);
    if is_dir {
        Some(ResolvedWrite::DeleteSubtree(entry_id))
    } else {
        Some(ResolvedWrite::DeleteFile(entry_id))
    }
}

/// Turn a storage-relative index path into the index lookup string (`/` rooted).
/// The path is already storage-relative; this just normalizes the leading slash.
fn path_to_index_str(path: &std::path::Path) -> String {
    let s = path.to_string_lossy();
    if s.is_empty() || s == "/" {
        "/".to_string()
    } else if s.starts_with('/') {
        s.into_owned()
    } else {
        format!("/{s}")
    }
}

/// Build the absolute MTP URL (`mtp://{device}/{storage}/inner`) for a
/// storage-relative path, for the FE `DirsUpdated` listing-cache key.
fn mtp_url_for(volume_id: &str, storage_rel: &std::path::Path) -> String {
    let (device_id, storage_id) = cmdr_fs::volume::mtp_ids::split_volume_id(volume_id).unwrap_or((volume_id, 0));
    let inner = storage_rel.to_string_lossy();
    let inner = inner.trim_start_matches('/');
    if inner.is_empty() {
        format!("mtp://{device_id}/{storage_id}")
    } else {
        format!("mtp://{device_id}/{storage_id}/{inner}")
    }
}

/// Stash a change during a full scan for post-scan replay. Bounded; overflow
/// flips a flag that forces Stale at replay (mirrors SMB).
fn buffer_change_during_scan(volume_id: &str, change: BufferedChange) {
    let mut buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
    let entry = buf.entry(volume_id.to_string()).or_default();
    if entry.overflowed {
        return;
    }
    if entry.changes.len() >= MAX_BUFFERED_CHANGES {
        entry.overflowed = true;
        log::warn!(
            target: "indexing::transports::mtp::watch",
            "mid-scan MTP change buffer for '{volume_id}' hit {MAX_BUFFERED_CHANGES}; will mark Stale at replay",
        );
        return;
    }
    entry.changes.push(change);
}

/// Replay the changes buffered during a volume's MTP scan, then clear the buffer.
/// Returns whether the volume should be considered Fresh (true) or was forced
/// Stale by overflow (false). Mirrors `smb_watch::replay_buffered_changes`.
pub(crate) fn replay_buffered_mtp_changes(volume_id: &str) -> bool {
    let buffered = {
        let mut buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        buf.remove(volume_id)
    };
    let Some(buffered) = buffered else {
        return true; // nothing buffered: clean Fresh
    };
    if buffered.overflowed {
        crate::indexing::transports::smb::index::on_smb_overflow(volume_id); // shared OverflowUnrecoverable ⇒ Stale
        return false;
    }
    let Some((writer, _)) = state::get_writer_and_flux_for(volume_id) else {
        return true;
    };
    let count = buffered.changes.len();

    // Synchronous changes (already-resolved upserts, pathless removals) apply now;
    // raw add/change handles need a device round trip to resolve, so they're
    // collected and resolved off-thread (the scan has ended, so the device is
    // idle — exactly when it's cheap to resolve them).
    let mut handles_to_resolve: Vec<(u32, u32)> = Vec::new();
    for change in &buffered.changes {
        match change {
            BufferedChange::Upsert(u) => apply_upsert(volume_id, &writer, u),
            BufferedChange::Remove(h) => send_remove(&writer, *h),
            BufferedChange::UpsertHandle { storage_id, handle } => handles_to_resolve.push((*storage_id, *handle)),
        }
    }
    if count > 0 {
        log::info!(
            target: "indexing::transports::mtp::watch",
            "replaying {count} mid-scan MTP change(s) into the '{volume_id}' index ({} need a post-scan resolve)",
            handles_to_resolve.len(),
        );
    }

    if !handles_to_resolve.is_empty() {
        resolve_and_apply_buffered_handles(volume_id, handles_to_resolve);
    }
    true
}

/// A device reported that one object appeared or changed: fold it into whichever
/// of the device's storages has it indexed.
///
/// **Gate before resolve.** During a walk the device is contended, so a live
/// resolve just to feed the index would cost a USB round trip the walk is about
/// to make anyway — the original livelock. So each indexed storage is asked
/// first, without touching the device: a scanning one buffers the RAW handle for
/// post-scan replay, and only the rest resolve live.
///
/// PTP handles are device-wide while storages are separate namespaces, so a live
/// resolve is attempted against each indexed storage and the first one it
/// resolves on owns the object; a non-matching storage fails cleanly. Runs
/// detached, because resolving does device I/O.
pub(crate) fn on_device_object_changed(device_id: &str, handle: u32) {
    let indexed = state::registered_mtp_volume_ids_for_device(device_id);
    if indexed.is_empty() {
        return;
    }

    let mut to_resolve_live: Vec<(String, u32)> = Vec::new();
    for volume_id in indexed {
        let Some(storage_id) = cmdr_fs::volume::mtp_ids::storage_id_of_volume(&volume_id) else {
            continue;
        };
        if buffer_mtp_handle_if_scanning(&volume_id, storage_id, handle) {
            continue;
        }
        to_resolve_live.push((volume_id, storage_id));
    }
    if to_resolve_live.is_empty() {
        return;
    }

    let device_id = device_id.to_string();
    let volumes = crate::indexing::host::volumes::current();
    crate::indexing::host::runtime::spawn(async move {
        for (volume_id, storage_id) in to_resolve_live {
            match volumes.resolve_mtp_object(&device_id, storage_id, handle).await {
                Ok(obj) => {
                    apply_mtp_added_or_changed(
                        &volume_id,
                        MtpUpsert {
                            path: obj.path,
                            handle,
                            is_directory: obj.is_directory,
                            size: obj.size,
                            modified_at: obj.modified_at,
                        },
                    );
                    // It resolved on this storage, so it can't also live on
                    // another.
                    return;
                }
                Err(e) => log::debug!(
                    target: "indexing::transports::mtp::watch",
                    "live change: handle {handle} on {device_id}:{storage_id} unresolved ({e:?}); trying the next storage",
                ),
            }
        }
    });
}

/// A device reported that one object is gone: drop it from whichever of the
/// device's storages had it.
///
/// Costs no device round trip — the object is gone, so there's no path to
/// resolve and each storage matches on the handle it stored. Only the storage
/// that indexed the object has a row; the rest are no-ops. Synchronous.
pub(crate) fn on_device_object_removed(device_id: &str, handle: u32) {
    for volume_id in state::registered_mtp_volume_ids_for_device(device_id) {
        apply_mtp_removed(&volume_id, handle);
    }
}

/// Resolve raw add/change handles buffered during a scan and apply them once the
/// device is idle (post-scan). Spawns a task because resolution does USB I/O.
/// Each resolve goes through the connection manager at FOREGROUND-equivalent
/// timing (the scan is done, nothing contends), and a failed resolve is dropped:
/// the scan that just completed already captured the object's then-current state,
/// and any later change re-fires its own event.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn resolve_and_apply_buffered_handles(volume_id: &str, handles: Vec<(u32, u32)>) {
    let Some(device_id) = cmdr_fs::volume::mtp_ids::device_id_of_volume(volume_id).map(str::to_owned) else {
        return;
    };
    let volume_id = volume_id.to_string();
    let volumes = crate::indexing::host::volumes::current();
    crate::indexing::host::runtime::spawn(async move {
        for (storage_id, handle) in handles {
            match volumes.resolve_mtp_object(&device_id, storage_id, handle).await {
                Ok(obj) => apply_mtp_added_or_changed(
                    &volume_id,
                    MtpUpsert {
                        path: obj.path,
                        handle,
                        is_directory: obj.is_directory,
                        size: obj.size,
                        modified_at: obj.modified_at,
                    },
                ),
                Err(e) => log::debug!(
                    target: "indexing::transports::mtp::watch",
                    "post-scan replay: handle {handle} on {device_id}:{storage_id} unresolved ({e:?}); skipping",
                ),
            }
        }
    });
}

/// No-op shim on platforms without MTP (the buffer never fills there).
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn resolve_and_apply_buffered_handles(_volume_id: &str, _handles: Vec<(u32, u32)>) {}

/// Discard any buffered mid-scan MTP changes for a volume without replaying them
/// (the D-interrupted path: the partial index is reset to gray).
pub(crate) fn discard_buffered_mtp_changes(volume_id: &str) {
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(volume_id);
}

#[cfg(test)]
mod tests;

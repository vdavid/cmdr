//! Translate live MTP PTP change events into per-volume index writes.
//!
//! The MTP event loop (`mtp/connection/event_loop.rs`) already turns PTP events
//! into targeted pane refreshes. This module is the SECOND consumer: it keeps the
//! volume's persisted index in sync so directory sizes stay correct while the
//! device is Fresh, even with no pane open. It's the MTP analogue of
//! `transports/smb/watch.rs`; the same invariants hold (single-writer-per-DB, reads off the
//! `ReadPool`, buffer-during-scan), with two MTP-specific twists.
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
//! Identical to SMB: enqueue the index write on the volume's writer FIRST, then
//! `EmitDirUpdated` so the FE re-reads sizes after the write commits; never open a
//! write connection here; BUFFER changes that arrive during a full scan and replay
//! them after, so they aren't lost against the rebuilding index.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};

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
    fn send(self, writer: &IndexWriter) {
        let msg = match self {
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
        };
        if let Err(e) = writer.send(msg) {
            log::debug!(target: "indexing::transports::mtp::watch", "writer send failed (writer gone): {e}");
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
    let scanning = match crate::indexing::lifecycle::state::get_writer_and_scanning_for(volume_id) {
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
/// Synchronous: only local DB reads off the `ReadPool` plus writer enqueues.
///
/// The live event path now gates BEFORE resolving (`buffer_mtp_handle_if_scanning`),
/// so this is reached only when the volume is NOT scanning; the in-scan buffer
/// branch here is a defensive belt-and-braces (e.g. a scan that started between
/// the gate check and this call).
pub(crate) fn apply_mtp_added_or_changed(volume_id: &str, upsert: MtpUpsert) {
    let (writer, scanning) = match crate::indexing::lifecycle::state::get_writer_and_scanning_for(volume_id) {
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
    let (writer, scanning) = match crate::indexing::lifecycle::state::get_writer_and_scanning_for(volume_id) {
        Some(pair) => pair,
        None => return,
    };
    if scanning {
        buffer_change_during_scan(volume_id, BufferedChange::Remove(handle));
        return;
    }
    apply_remove(volume_id, &writer, handle);
}

/// Resolve and enqueue an add/change against the live index.
fn apply_upsert(volume_id: &str, writer: &IndexWriter, upsert: &MtpUpsert) {
    use crate::indexing::read::enrichment::get_read_pool_for;

    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => return,
    };

    let resolved: Option<ResolvedWrite> = pool.with_conn(|conn| resolve_upsert(conn, upsert)).unwrap_or(None);
    let Some(write) = resolved else { return };
    write.send(writer);

    // Sequence the FE refresh AFTER the index write (the writer fires
    // `index-dir-updated` only once the upsert commits). Emit for the parent's
    // absolute MTP URL so the FE listing-cache key matches.
    if let Some(parent) = upsert.path.parent() {
        let parent_url = mtp_url_for(volume_id, parent);
        let _ = writer.send(WriteMessage::EmitDirUpdated(vec![parent_url]));
    }
}

/// Resolve and enqueue a removal against the live index.
fn apply_remove(volume_id: &str, writer: &IndexWriter, handle: u32) {
    use crate::indexing::read::enrichment::get_read_pool_for;

    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => return,
    };
    let resolved: Option<ResolvedWrite> = pool.with_conn(|conn| resolve_remove(conn, handle)).unwrap_or(None);
    let Some(write) = resolved else { return };
    write.send(writer);
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
/// storage-relative path, for the FE `index-dir-updated` listing-cache key.
fn mtp_url_for(volume_id: &str, storage_rel: &std::path::Path) -> String {
    let (device_id, storage_id) = crate::mtp::identity::split_volume_id(volume_id).unwrap_or((volume_id, 0));
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
        crate::indexing::on_smb_overflow(volume_id); // shared OverflowUnrecoverable ⇒ Stale
        return false;
    }
    let Some((writer, _)) = crate::indexing::lifecycle::state::get_writer_and_scanning_for(volume_id) else {
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
            BufferedChange::Remove(h) => apply_remove(volume_id, &writer, *h),
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

/// Resolve raw add/change handles buffered during a scan and apply them once the
/// device is idle (post-scan). Spawns a task because resolution does USB I/O.
/// Each resolve goes through the connection manager at FOREGROUND-equivalent
/// timing (the scan is done, nothing contends), and a failed resolve is dropped:
/// the scan that just completed already captured the object's then-current state,
/// and any later change re-fires its own event.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn resolve_and_apply_buffered_handles(volume_id: &str, handles: Vec<(u32, u32)>) {
    let Some(device_id) = crate::mtp::identity::device_id_of_volume(volume_id).map(str::to_owned) else {
        return;
    };
    let volume_id = volume_id.to_string();
    tauri::async_runtime::spawn(async move {
        for (storage_id, handle) in handles {
            match crate::mtp::connection::connection_manager()
                .resolve_object_for_index(&device_id, storage_id, mtp_rs::ObjectHandle(u64::from(handle)))
                .await
            {
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
mod tests {
    use super::*;

    /// Seed a tiny MTP-shaped index: ROOT(1) → "DCIM"(dir, handle=10) →
    /// "IMG.jpg"(file, handle=11), and "top.txt"(file, handle=12) at root.
    /// Returns an open write connection.
    fn seed_index() -> (rusqlite::Connection, tempfile::TempDir) {
        use crate::indexing::store::{EntryRow, ROOT_ID};
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("mtp-watch-test.db");
        let store = IndexStore::open(&db_path).expect("open store");
        drop(store);
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        let rows = vec![
            EntryRow {
                id: 2,
                parent_id: ROOT_ID,
                name: "DCIM".into(),
                is_directory: true,
                is_symlink: false,
                logical_size: None,
                physical_size: None,
                modified_at: None,
                inode: Some(10),
            },
            EntryRow {
                id: 3,
                parent_id: 2,
                name: "IMG.jpg".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(100),
                physical_size: Some(100),
                modified_at: None,
                inode: Some(11),
            },
            EntryRow {
                id: 4,
                parent_id: ROOT_ID,
                name: "top.txt".into(),
                is_directory: false,
                is_symlink: false,
                logical_size: Some(5),
                physical_size: Some(5),
                modified_at: None,
                inode: Some(12),
            },
        ];
        IndexStore::insert_entries_v2_batch(&conn, &rows).expect("seed rows");
        (conn, dir)
    }

    fn upsert(path: &str, handle: u32, is_dir: bool, size: Option<u64>) -> MtpUpsert {
        MtpUpsert {
            path: PathBuf::from(path),
            handle,
            is_directory: is_dir,
            size,
            modified_at: None,
        }
    }

    // ── resolve_upsert: add/change → upsert with the handle in inode ───────

    #[test]
    fn added_file_maps_to_upsert_under_resolved_parent_with_handle() {
        let (conn, _dir) = seed_index();
        // A new photo in /DCIM, handle 20.
        let w = resolve_upsert(&conn, &upsert("/DCIM/NEW.jpg", 20, false, Some(7))).expect("a write");
        match w {
            ResolvedWrite::Upsert {
                parent_id,
                name,
                handle,
                size,
                ..
            } => {
                assert_eq!(parent_id, 2, "resolved under /DCIM (id=2)");
                assert_eq!(name, "NEW.jpg");
                assert_eq!(handle, 20, "the object handle is carried into the upsert (→ inode)");
                assert_eq!(size, Some(7));
            }
            _ => panic!("Added must map to Upsert"),
        }
    }

    #[test]
    fn added_at_storage_root_resolves_under_root_id() {
        let (conn, _dir) = seed_index();
        let w = resolve_upsert(&conn, &upsert("/movie.mp4", 21, false, Some(9))).expect("a write");
        match w {
            ResolvedWrite::Upsert { parent_id, name, .. } => {
                assert_eq!(parent_id, 1, "resolved under the storage root (ROOT_ID=1)");
                assert_eq!(name, "movie.mp4");
            }
            _ => panic!("root-level add must map to Upsert under ROOT_ID"),
        }
    }

    #[test]
    fn change_under_unindexed_parent_is_a_no_op() {
        let (conn, _dir) = seed_index();
        // /Music isn't in the index, so a change inside it can't attach.
        assert!(resolve_upsert(&conn, &upsert("/Music/song.mp3", 30, false, Some(1))).is_none());
    }

    // ── resolve_remove: pathless removal via the STORED handle ─────────────

    #[test]
    fn removed_file_resolves_by_stored_handle_to_delete_file() {
        let (conn, _dir) = seed_index();
        // IMG.jpg was stored with handle 11. ObjectRemoved{11} must delete id=3.
        match resolve_remove(&conn, 11).expect("a write") {
            ResolvedWrite::DeleteFile(id) => assert_eq!(id, 3, "IMG.jpg (handle 11) is id=3"),
            _ => panic!("removing a file handle must map to DeleteFile"),
        }
    }

    #[test]
    fn removed_directory_resolves_by_stored_handle_to_delete_subtree() {
        let (conn, _dir) = seed_index();
        // DCIM was stored with handle 10. ObjectRemoved{10} must delete the subtree.
        match resolve_remove(&conn, 10).expect("a write") {
            ResolvedWrite::DeleteSubtree(id) => assert_eq!(id, 2, "DCIM (handle 10) is id=2"),
            _ => panic!("removing a directory handle must map to DeleteSubtree"),
        }
    }

    #[test]
    fn removed_unknown_handle_is_a_no_op() {
        // A removal for a handle we never indexed (object we never saw) is a
        // no-op — the MTP analogue of the SMB stat-verify "never-indexed" rule.
        let (conn, _dir) = seed_index();
        assert!(resolve_remove(&conn, 9999).is_none());
    }

    // ── path / url helpers ─────────────────────────────────────────────────

    #[test]
    fn path_to_index_str_normalizes_root_and_leading_slash() {
        assert_eq!(path_to_index_str(std::path::Path::new("/")), "/");
        assert_eq!(path_to_index_str(std::path::Path::new("/DCIM")), "/DCIM");
        assert_eq!(path_to_index_str(std::path::Path::new("")), "/");
    }

    #[test]
    fn mtp_url_for_builds_the_listing_cache_key() {
        let vid = "mtp-PIXEL7:65537";
        assert_eq!(mtp_url_for(vid, std::path::Path::new("/")), "mtp://mtp-PIXEL7/65537");
        assert_eq!(
            mtp_url_for(vid, std::path::Path::new("/DCIM")),
            "mtp://mtp-PIXEL7/65537/DCIM"
        );
        // A serial device id containing a colon round-trips into the URL.
        assert_eq!(
            mtp_url_for("mtp-AA:BB:65537", std::path::Path::new("/Music")),
            "mtp://mtp-AA:BB/65537/Music"
        );
    }

    // ── mid-scan buffer mechanics ──────────────────────────────────────────

    #[test]
    fn buffer_accumulates_then_discard_clears_it() {
        let vid = "mtp-buffer-discard-test";
        SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
        for i in 0..3 {
            buffer_change_during_scan(
                vid,
                BufferedChange::Upsert(upsert(&format!("/f{i}.txt"), 100 + i, false, Some(1))),
            );
        }
        buffer_change_during_scan(vid, BufferedChange::Remove(50));
        {
            let buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
            assert_eq!(buf.get(vid).map(|b| b.changes.len()), Some(4), "four buffered");
            assert!(!buf.get(vid).unwrap().overflowed);
        }
        discard_buffered_mtp_changes(vid);
        assert!(
            SCAN_CHANGE_BUFFER.lock_ignore_poison().get(vid).is_none(),
            "discard must clear the buffer",
        );
    }

    #[test]
    fn buffer_holds_raw_upsert_handles_for_post_scan_resolve() {
        // The gate-before-resolve fix buffers a RAW (storage_id, handle), not a
        // resolved upsert — no device round trip during a scan. Confirm the
        // variant accumulates and the replay-partition sees it as a to-resolve.
        let vid = "mtp-buffer-handle-test";
        SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
        buffer_change_during_scan(
            vid,
            BufferedChange::UpsertHandle {
                storage_id: 65537,
                handle: 42,
            },
        );
        buffer_change_during_scan(
            vid,
            BufferedChange::UpsertHandle {
                storage_id: 65537,
                handle: 43,
            },
        );
        buffer_change_during_scan(vid, BufferedChange::Remove(7));
        {
            let buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
            let changes = &buf.get(vid).expect("buffer present").changes;
            assert_eq!(changes.len(), 3);
            let raw_handles: Vec<u32> = changes
                .iter()
                .filter_map(|c| match c {
                    BufferedChange::UpsertHandle { handle, .. } => Some(*handle),
                    _ => None,
                })
                .collect();
            assert_eq!(raw_handles, vec![42, 43], "both raw handles buffered, in order");
        }
        SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
    }

    #[test]
    fn buffer_gate_returns_false_for_an_unregistered_volume() {
        // `buffer_mtp_handle_if_scanning` must NOT buffer (and must return false,
        // so the caller resolves live) when the volume has no Running index — the
        // device-free fast path. A never-registered volume id exercises this.
        let vid = "mtp-never-registered-gate:65537";
        SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
        assert!(
            !buffer_mtp_handle_if_scanning(vid, 65537, 99),
            "no Running index ⇒ don't buffer, let the caller resolve live",
        );
        assert!(
            SCAN_CHANGE_BUFFER.lock_ignore_poison().get(vid).is_none(),
            "nothing buffered for an unregistered volume",
        );
    }

    #[test]
    fn buffer_overflow_sets_the_flag_and_stops_growing() {
        let vid = "mtp-buffer-overflow-test";
        SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
        {
            let mut buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
            let entry = buf.entry(vid.to_string()).or_default();
            entry.changes.reserve(MAX_BUFFERED_CHANGES);
            for _ in 0..MAX_BUFFERED_CHANGES {
                entry.changes.push(BufferedChange::Remove(1));
            }
        }
        buffer_change_during_scan(vid, BufferedChange::Remove(2));
        {
            let buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
            let b = buf.get(vid).expect("buffer present");
            assert!(b.overflowed, "hitting the cap must set overflow");
            assert_eq!(b.changes.len(), MAX_BUFFERED_CHANGES, "must not grow past the cap");
        }
        SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
    }
}

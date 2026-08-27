//! Translate live SMB `CHANGE_NOTIFY` events into per-volume index writes.
//!
//! The SMB watcher (`crates/cmdr-smb/src/volume/watcher.rs`) already turns
//! `CHANGE_NOTIFY` into [`DirectoryChange`]s and feeds them to
//! `caching::notify_directory_changed` for the open pane. This module is the
//! SECOND consumer: it keeps the volume's persisted index in sync with the same
//! changes so directory sizes stay correct while the share is Fresh, even when
//! no pane is showing the affected directory.
//!
//! ## Path space (the load-bearing subtlety)
//!
//! The SMB index's `ROOT_ID` is the volume's **mount root** (the scanner maps
//! the scan root to `ROOT_ID`; see `network_scanner/mod.rs`), and entries are stored
//! by `name` under their parent. The watcher delivers a **mount-absolute**
//! parent path (`/Volumes/share/sub`). So every resolution here first strips the
//! mount root to a **mount-relative** path (`/sub`) before
//! [`store::resolve_path`], which walks component-by-component from `ROOT_ID`.
//! Passing the mount-absolute path straight to `resolve_path` would try to
//! resolve `Volumes` / `share` as children of the share root and always miss.
//!
//! ## Ordering and coupling (plan Architecture §3)
//!
//! `caching::notify_directory_changed` calls [`apply_smb_change`] FIRST (the
//! index write is sequenced ahead of the pane enrich), then emits
//! `index-dir-updated` for the affected directory so the existing FE refresh
//! path (`index-dir-updated` → `refreshIndexSizes` → `getDirStatsBatch`) re-reads
//! the just-written sizes. The coupling is one-directional: the listing layer
//! notifies the indexer, never the reverse.
//!
//! ## Invariants honored
//!
//! - **Single-writer-per-DB**: we only ENQUEUE messages on the volume's existing
//!   writer thread (via `state::get_writer_and_flux_for`); we never open a
//!   write connection here. A change arriving while the volume isn't `Running`
//!   (disabled) is dropped; one arriving mid-scan is BUFFERED and replayed after
//!   the scan, so it isn't lost against the rebuilding index.
//! - **Reads off the registry lock**: id resolution uses the volume's `ReadPool`
//!   (`get_read_pool_for`), never the lifecycle mutex.
//! - **Resolve deletes against the INDEX, not a live stat**: SMB coalescing can
//!   deliver a false `Removed` (an atomic rename's old name, a deleted-then-
//!   recreated path). We deliberately do NOT stat the live volume per delete (that
//!   would add a network round trip per event). Instead we resolve the removed name
//!   against the index: we enqueue a delete only when that name still exists as an
//!   index entry; an unknown name is a no-op, and a recreate heals via the separate
//!   `Added` the watcher fires for it. The index is display-only, so a briefly
//!   stale row is safe. (FSEvents' `item_removed`, on a local disk where a stat is
//!   cheap, DOES stat-verify; SMB intentionally doesn't.)

use std::collections::HashMap;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use crate::indexing::lifecycle::state;
use crate::indexing::network_scanner::is_recursion_excluded_dir;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::DirectoryChange;

/// Per-volume buffer of SMB changes that arrived DURING a full (re)scan.
///
/// A change to an already-walked directory can't be applied straight to the
/// mid-scan index: the scan truncated the DB and is still inserting, so the
/// parent may not be in the read snapshot yet (the upsert would resolve to
/// nothing and be lost). So while a volume is scanning we stash its changes here
/// and [`replay_buffered_changes`] applies them once the scan's aggregation has
/// landed — the SMB analogue of the local arm-watcher-before-snapshot + reconcile
/// flow. The smb2 watcher itself runs continuously and keeps one `CHANGE_NOTIFY`
/// pre-issued, so the pre-arm holds across a mid-scan reconnect respawn too: a
/// respawned watcher feeds this same buffer.
///
/// Bounded: past `MAX_BUFFERED_CHANGES` we stop stashing and mark the volume
/// overflowed, so a storm during a long scan can't grow unbounded; the overflow
/// then forces Stale at replay (honest: we couldn't track every change).
static SCAN_CHANGE_BUFFER: LazyLock<Mutex<HashMap<String, BufferedVolume>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Cap on buffered mid-scan changes per volume. A normal scan sees a handful;
/// this only trips under a genuine storm, where a post-scan Stale + rescan is
/// the honest outcome anyway.
const MAX_BUFFERED_CHANGES: usize = 50_000;

/// One volume's mid-scan change buffer plus its overflow flag.
#[derive(Default)]
struct BufferedVolume {
    changes: Vec<(std::path::PathBuf, DirectoryChange)>,
    overflowed: bool,
}

/// Map a watcher-delivered, mount-absolute path to the path the SMB index stores
/// it under: relative to the mount root, rooted at `/` (the index `ROOT_ID`).
///
/// `mount_root` is the volume's `root()` (e.g. `/Volumes/share`); `abs_path` is
/// what the watcher built via `to_nfd_display_path(mount_path, …)`. Returns
/// `/` for the mount root itself, `/sub/dir` for `/Volumes/share/sub/dir`. Pure
/// and platform-independent so it's unit-testable on every target.
pub fn index_relative_path(mount_root: &str, abs_path: &str) -> Option<String> {
    // Trim a single trailing slash off the root for a clean prefix match
    // (`/Volumes/share/` vs `/Volumes/share`), then require the abs path to sit
    // under it. A path that doesn't start with the mount root isn't on this
    // volume — drop it rather than mis-resolving.
    let root = mount_root.strip_suffix('/').unwrap_or(mount_root);

    if abs_path == root {
        return Some("/".to_string());
    }
    let rest = abs_path.strip_prefix(root)?;
    // `rest` now starts with the separator for any real child
    // (`/Volumes/share` + `/sub` → `/sub`). Guard against a sibling whose name
    // merely starts with the root (e.g. root `/Volumes/sh`, path `/Volumes/share`):
    // a real child's remainder must begin with `/`.
    if rest.is_empty() {
        return Some("/".to_string());
    }
    if !rest.starts_with('/') {
        return None;
    }
    Some(rest.to_string())
}

/// The single index-write a `DirectoryChange` maps to, decided where we still
/// have the live volume and the read connection. Kept as a tiny enum so the
/// per-variant mapping is obvious and the resolved-id sends are funneled through
/// one place. Not all changes produce a write (a `Removed` whose entry was never
/// indexed yields `None`).
enum ResolvedWrite {
    /// Upsert a single entry under `parent_id`. Carries the columns the writer's
    /// `UpsertEntryV2` needs; the writer auto-propagates the size/count delta to
    /// ancestor `dir_stats`, so we never send a separate `PropagateDeltaById`.
    Upsert {
        parent_id: i64,
        name: String,
        is_directory: bool,
        is_symlink: bool,
        logical_size: Option<u64>,
        physical_size: Option<u64>,
        modified_at: Option<u64>,
    },
    /// Delete a single file entry by id (stat-verified gone).
    DeleteFile(i64),
    /// Delete a directory subtree by id (stat-verified gone).
    DeleteSubtree(i64),
}

impl ResolvedWrite {
    /// Build the upsert columns from a watcher-supplied `FileEntry`. SMB reports
    /// one size, so physical mirrors logical (the scanner does the same); a
    /// symlink contributes no size, matching `du`-style omission.
    fn upsert_from_entry(parent_id: i64, entry: &FileEntry) -> Self {
        let (logical_size, physical_size) = if entry.is_symlink {
            (None, None)
        } else {
            (entry.size, entry.physical_size.or(entry.size))
        };
        ResolvedWrite::Upsert {
            parent_id,
            name: entry.name.clone(),
            is_directory: entry.is_directory,
            is_symlink: entry.is_symlink,
            logical_size,
            physical_size,
            modified_at: entry.modified_at,
        }
    }

    /// Enqueue the corresponding writer message. Inode/nlink are `None`: SMB
    /// entries carry no stable inode, so hardlink dedup doesn't apply.
    fn send(self, writer: &IndexWriter) {
        let msg = match self {
            ResolvedWrite::Upsert {
                parent_id,
                name,
                is_directory,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
            } => WriteMessage::UpsertEntryV2 {
                parent_id,
                name,
                is_directory,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
                inode: None,
                nlink: None,
            },
            ResolvedWrite::DeleteFile(id) => WriteMessage::DeleteEntryById(id),
            ResolvedWrite::DeleteSubtree(id) => WriteMessage::DeleteSubtreeById(id),
        };
        if let Err(e) = writer.send(msg) {
            log::debug!(target: "indexing::transports::smb::watch", "writer send failed (writer gone): {e}");
        }
    }
}

/// Apply one SMB `DirectoryChange` to the volume's index, if the volume has a
/// live (`Running`) index. Resolves ids against the volume's `ReadPool`,
/// enqueues the writer message, and (on success) asks the writer to emit
/// `index-dir-updated` for the affected directory so the FE refreshes sizes AFTER
/// the write lands.
///
/// `parent_path` is the watcher's mount-absolute directory path. Synchronous: all
/// work is local DB reads off the `ReadPool` plus enqueues on the writer channel,
/// no network round trip, so it's safe to call inline from the (sync)
/// `notify_directory_changed`. No-op (cheap early return) when the volume isn't a
/// live non-`root` index, so wiring this in costs one `Arc`-clone check for
/// non-indexed shares and for every `root` (local-disk) change.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn apply_smb_change(volume_id: &str, parent_path: &Path, change: &DirectoryChange) {
    // `root` (local disk) feeds its index from FSEvents, not from this listing
    // hook; never double-index it here.
    if volume_id == crate::ROOT_VOLUME_ID {
        return;
    }

    // Cheap gate: only a Running index has a writer. Drop the change otherwise
    // (initializing volumes are mid-scan; absent ones are disabled). Also read
    // whether a full scan is in progress — if so, BUFFER instead of applying, so
    // a change to an already-walked dir isn't lost against the rebuilding index.
    let (writer, scanning) = match state::get_writer_and_flux_for(volume_id) {
        Some(pair) => pair,
        None => return,
    };

    if scanning {
        buffer_change_during_scan(volume_id, parent_path, change);
        return;
    }

    apply_one_change(volume_id, &writer, parent_path, change);
}

/// Stash a change that arrived during a full scan for post-scan replay. Bounded:
/// past `MAX_BUFFERED_CHANGES` we stop stashing and flag overflow (replay then
/// forces Stale).
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn buffer_change_during_scan(volume_id: &str, parent_path: &Path, change: &DirectoryChange) {
    let mut buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
    let entry = buf.entry(volume_id.to_string()).or_default();
    if entry.overflowed {
        return;
    }
    if entry.changes.len() >= MAX_BUFFERED_CHANGES {
        entry.overflowed = true;
        log::warn!(
            target: "indexing::transports::smb::watch",
            "mid-scan change buffer for '{volume_id}' hit {MAX_BUFFERED_CHANGES}; will mark Stale at replay",
        );
        return;
    }
    entry.changes.push((parent_path.to_path_buf(), change.clone()));
}

/// Replay the changes buffered during a volume's full scan, then clear the
/// buffer. Called by the scan-completion handler AFTER aggregation has landed
/// (so resolution sees the full tree). If the buffer overflowed mid-scan, the
/// index may have drifted, so we signal `OverflowUnrecoverable` ⇒ Stale rather
/// than claim Fresh — the honest outcome. Returns whether the volume should be
/// considered Fresh (true) or was forced Stale by overflow (false).
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn replay_buffered_changes(volume_id: &str) -> bool {
    let buffered = {
        let mut buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        buf.remove(volume_id)
    };
    let Some(buffered) = buffered else {
        return true; // nothing buffered: clean Fresh
    };

    if buffered.overflowed {
        // Too many mid-scan changes to track reliably: don't claim Fresh.
        crate::indexing::transports::smb::index::on_smb_overflow(volume_id);
        return false;
    }

    // The volume is `Running` and no longer scanning by the time we replay, so
    // re-fetch the writer and apply each change against the now-complete index.
    let Some((writer, _)) = state::get_writer_and_flux_for(volume_id) else {
        return true;
    };
    let count = buffered.changes.len();
    for (parent_path, change) in &buffered.changes {
        apply_one_change(volume_id, &writer, parent_path, change);
    }
    if count > 0 {
        log::info!(
            target: "indexing::transports::smb::watch",
            "replayed {count} mid-scan change(s) into the '{volume_id}' index",
        );
    }
    true
}

/// Apply one change straight to the index (resolution + writer enqueue + FE
/// refresh emit). The non-buffering path: used live (not scanning) and during
/// replay. Resolves ids off the volume's `ReadPool` (no registry lock, no write
/// connection), then enqueues onto the single per-volume writer.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn apply_one_change(volume_id: &str, writer: &IndexWriter, parent_path: &Path, change: &DirectoryChange) {
    use crate::indexing::read::enrichment::get_read_pool_for;

    let pool = match get_read_pool_for(volume_id) {
        Some(p) => p,
        None => return,
    };

    let mount_root = match crate::indexing::host::volumes::current().get(volume_id) {
        Some(v) => v.root().to_string_lossy().into_owned(),
        None => return, // share unmounted; the freshness layer will flip it Stale
    };
    let parent_abs = parent_path.to_string_lossy();
    let parent_rel = match index_relative_path(&mount_root, &parent_abs) {
        Some(p) => p,
        None => {
            log::debug!(
                target: "indexing::transports::smb::watch",
                "change parent {parent_abs} not under mount root {mount_root}, skipping",
            );
            return;
        }
    };

    // Resolve the parent id (and, for deletes, the target entry) off the read
    // pool — no registry lock, no write connection.
    let resolved: Option<ResolvedWrite> = pool
        .with_conn(|conn| resolve_change(conn, &parent_rel, change))
        .unwrap_or(None);

    let Some(write) = resolved else {
        return;
    };
    write.send(writer);

    // Sequence the FE refresh AFTER the index write: `EmitDirUpdated` rides the
    // same writer channel, so the writer fires `index-dir-updated` only once the
    // upsert/delete above is committed. The FE then re-reads sizes from the
    // just-written index (the existing refreshIndexSizes → getDirStatsBatch path).
    // Emit for the mount-absolute parent path: that's the listing-cache key the
    // FE matches against, and the index read routes back through the volume id.
    let _ = writer.send(WriteMessage::EmitDirUpdated(vec![parent_abs.into_owned()]));
}

/// Resolve a change against an explicit index connection and enqueue the matching
/// writer message onto `writer` — the registry-free core of the watch→index
/// translation. Returns whether a write was enqueued. Exists so the Docker SMB
/// integration test can drive the real translation against a real, freshly
/// SMB-scanned index without an `AppHandle`-bound `Running` registry instance.
#[cfg(any(test, feature = "testing"))]
pub fn resolve_and_send_for_test(
    conn: &rusqlite::Connection,
    writer: &IndexWriter,
    mount_root: &str,
    parent_abs: &str,
    change: &DirectoryChange,
) -> bool {
    let Some(parent_rel) = index_relative_path(mount_root, parent_abs) else {
        return false;
    };
    let Some(write) = resolve_change(conn, &parent_rel, change) else {
        return false;
    };
    write.send(writer);
    true
}

/// Discard any buffered mid-scan changes for a volume without replaying them.
/// Called when a scan is interrupted/discarded (D-interrupted): the partial index
/// is reset to gray, so its buffered changes are meaningless.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn discard_buffered_changes(volume_id: &str) {
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(volume_id);
}

/// Decide the index write for a change, given the index-relative parent path and
/// a read connection. Pure over the DB state (no enqueue, no I/O beyond reads),
/// so the resolution rules are testable against a seeded in-memory index.
///
/// `None` means "nothing to write" (parent not indexed, or a `Removed`/`Renamed`
/// whose target was never an index entry, so there's nothing to delete).
fn resolve_change(conn: &rusqlite::Connection, parent_rel: &str, change: &DirectoryChange) -> Option<ResolvedWrite> {
    // Stay out of the subtrees the scanner refuses to walk. `@eaDir` & co. keep
    // their OWN row, so their path resolves fine and a live event would happily
    // upsert children under them — rows no scan produces and `writer/prune.rs`
    // deletes, so the watcher would re-dirty a freshly pruned index. A change TO
    // the excluded dir itself arrives under its parent and is unaffected.
    if is_under_recursion_excluded_dir(parent_rel) {
        return None;
    }

    let parent_id = store::resolve_path(conn, parent_rel).ok().flatten()?;

    match change {
        DirectoryChange::Added(entry) | DirectoryChange::Modified(entry) => {
            Some(ResolvedWrite::upsert_from_entry(parent_id, entry))
        }
        DirectoryChange::Renamed { new_entry, .. } => {
            // The watcher already resolved the rename to a removed old name + a
            // freshly-stat'd new entry within the same directory. Upsert the new
            // entry; the stale old-name row is cleared by the verifier / a later
            // FullRefresh. (A same-dir rename keeps ancestor totals, so leaving the
            // old row briefly only over-counts within this dir until reconciled.)
            Some(ResolvedWrite::upsert_from_entry(parent_id, new_entry))
        }
        DirectoryChange::Removed(name) => {
            // Resolve the delete against the INDEX, not a live stat. SMB coalescing
            // delivers false removals (atomic-rename old name, delete-then-recreate).
            // We deliberately skip a per-delete live stat (it would add a network
            // round trip per event); instead we delete only when the removed name
            // still exists as an index entry. A never-indexed name is a no-op, and a
            // recreate heals via the separate `Added` the watcher fires for it.
            let entry_id = store::resolve_path(conn, &join_rel(parent_rel, name)).ok().flatten()?;
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
        DirectoryChange::FullRefresh => {
            // Overflow / bulk change. The index can't translate this to a targeted
            // write here; the watcher-lifetime layer handles overflow policy
            // (targeted subtree rescan, see `transports/smb/index`/`manager`). No-op here.
            None
        }
    }
}

/// Whether an index-relative path sits INSIDE a NAS snapshot/system pseudo-dir the
/// `Volume`-trait scanner refuses to walk (`network_scanner/system_dirs.rs`).
///
/// Matches whole path components, so an ordinary `@eaDirectory` is untouched, and
/// checks EVERY component, not only the last: an excluded dir nested below an
/// ordinary one gates its whole subtree too. The excluded dir's own path is
/// deliberately included — a change TO it arrives under its PARENT, which is not
/// excluded, so its own row still updates.
fn is_under_recursion_excluded_dir(path_rel: &str) -> bool {
    path_rel.split('/').any(is_recursion_excluded_dir)
}

/// Join an index-relative parent dir with a child name, normalizing the single
/// `/` between them. `parent_rel` is `/` or `/a/b`; `name` is a bare basename.
fn join_rel(parent_rel: &str, name: &str) -> String {
    if parent_rel == "/" {
        format!("/{name}")
    } else {
        format!("{parent_rel}/{name}")
    }
}

#[cfg(test)]
mod tests;

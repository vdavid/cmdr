//! Event reconciler: buffers FSEvents during scan, replays after scan completes.
//!
//! During the initial full scan, the watcher runs concurrently and buffers events.
//! Once the scan finishes, the reconciler replays only events that arrived *after*
//! the scanner read their affected path (using monotonically increasing event IDs).
//! Events with `event_id <= scan_start_event_id` are discarded because the scan data
//! is newer.
//!
//! After replay, the reconciler switches to live mode where events are processed
//! immediately.
//!
//! ## Integer-keyed resolution (milestone 4)
//!
//! All path resolution uses `store::resolve_path(conn, path)` to convert filesystem
//! paths to integer entry IDs. Write messages use integer-keyed variants:
//! `UpsertEntryV2`, `DeleteEntryById`, `DeleteSubtreeById`, `PropagateDeltaById`.
//! The reconciler holds a read connection (`rusqlite::Connection`) for path resolution.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};

use crate::indexing::DEBUG_STATS;
use crate::indexing::firmlinks;
use crate::indexing::scanner;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::watcher::FsChangeEvent;
use crate::indexing::writer::{IndexWriter, WriteMessage};

// ── EventReconciler ──────────────────────────────────────────────────

/// Maximum number of events the reconciler will buffer during a scan.
/// Beyond this, buffering stops and a full rescan is forced after the
/// current scan completes. The index is a disposable cache, so dropping
/// events is always safe.
const MAX_BUFFER_CAPACITY: usize = 500_000;

/// Buffers FSEvents during the initial scan and replays them after the scan completes.
pub struct EventReconciler {
    /// Events buffered during scan, in arrival order.
    buffer: Vec<FsChangeEvent>,
    /// Whether we're in buffering mode (scan in progress).
    buffering: bool,
    /// Set when the buffer cap is hit. Forces a full rescan after the
    /// current scan completes instead of replaying individual events.
    pub(super) buffer_overflow: bool,
    /// Paths pending MustScanSubDirs rescans, deduplicated.
    pending_rescans: HashSet<PathBuf>,
    /// Whether a MustScanSubDirs rescan is currently running.
    rescan_active: Arc<AtomicBool>,
}

impl EventReconciler {
    /// Create a new reconciler in buffering mode.
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            buffering: true,
            buffer_overflow: false,
            pending_rescans: HashSet::new(),
            rescan_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Buffer an event during scan. If the buffer cap is reached, stops
    /// buffering and sets `buffer_overflow` to force a full rescan.
    pub fn buffer_event(&mut self, event: FsChangeEvent) {
        if !self.buffering || self.buffer_overflow {
            return;
        }
        if self.buffer.len() >= MAX_BUFFER_CAPACITY {
            log::warn!(
                "Reconciler: buffer cap reached ({MAX_BUFFER_CAPACITY} events). \
                 Dropping further events; a full rescan will run after the current scan."
            );
            self.buffer_overflow = true;
            self.buffer.clear();
            self.buffer.shrink_to_fit();
            return;
        }
        self.buffer.push(event);
    }

    /// Replay buffered events after scan completes.
    ///
    /// - Events with `event_id <= scan_start_event_id` are skipped (scan data is newer).
    /// - Events with `event_id > scan_start_event_id` are processed (filesystem changed after scan).
    /// - Returns the last processed event ID.
    pub fn replay(
        &mut self,
        scan_start_event_id: u64,
        conn: &Connection,
        writer: &IndexWriter,
        on_dirs_affected: &mut dyn FnMut(Vec<String>),
    ) -> Result<u64, String> {
        // Sort by event_id to process in order
        self.buffer.sort_by_key(|e| e.event_id);

        let total = self.buffer.len();
        let mut processed = 0u64;
        let mut last_event_id = scan_start_event_id;
        let mut affected_paths: Vec<String> = Vec::new();

        log::info!("Reconciler: replaying {total} buffered events (scan_start_event_id={scan_start_event_id})");

        for event in &self.buffer {
            // Skip events that the scan already covered
            if event.event_id <= scan_start_event_id {
                continue;
            }

            if let Some(paths) = process_fs_event(event, conn, writer) {
                affected_paths.extend(paths);
            }

            last_event_id = event.event_id;
            processed += 1;
        }

        // Notify caller of all affected paths
        if !affected_paths.is_empty() {
            on_dirs_affected(affected_paths);
        }

        // Store last event ID
        if last_event_id > scan_start_event_id {
            let _ = writer.send(WriteMessage::UpdateLastEventId(last_event_id));
        }

        log::info!("Reconciler: replayed {processed}/{total} events (last_event_id={last_event_id})");
        Ok(last_event_id)
    }

    /// Switch from buffering to live mode. Clears the buffer.
    pub fn switch_to_live(&mut self) {
        self.buffering = false;
        self.buffer_overflow = false;
        self.buffer.clear();
        self.buffer.shrink_to_fit();
        log::info!("Reconciler: switched to live mode");
    }

    /// Process a single event in live mode.
    ///
    /// Collects affected directory paths into `pending_paths` for batched
    /// emission by the caller (1s flush interval). Returns the event ID
    /// on success, or `None` if still buffering.
    pub fn process_live_event(
        &mut self,
        event: &FsChangeEvent,
        conn: &Connection,
        writer: &IndexWriter,
        pending_paths: &mut HashSet<String>,
    ) -> Option<u64> {
        if self.buffering {
            self.buffer_event(event.clone());
            return None;
        }

        // Handle MustScanSubDirs
        if event.flags.must_scan_sub_dirs {
            let normalized = firmlinks::normalize_path(&event.path);
            self.queue_must_scan_sub_dirs(PathBuf::from(&normalized), writer);
            return Some(event.event_id);
        }

        if let Some(affected_paths) = process_fs_event(event, conn, writer) {
            pending_paths.extend(affected_paths);
        }

        // UpdateLastEventId is sent once per batch by the caller (process_live_batch)
        // instead of per-event, to reduce writer channel pressure during event storms.

        Some(event.event_id)
    }

    /// Queue a MustScanSubDirs rescan, throttled to max 1 concurrent.
    pub(super) fn queue_must_scan_sub_dirs(&mut self, path: PathBuf, writer: &IndexWriter) {
        DEBUG_STATS.record_must_scan(&path.to_string_lossy());
        self.pending_rescans.insert(path.clone());

        if self.rescan_active.load(Ordering::Relaxed) {
            log::debug!(
                "Reconciler: MustScanSubDirs for {} queued (rescan already active)",
                path.display()
            );
            return;
        }

        self.start_next_rescan(writer);
    }

    /// Start the next pending MustScanSubDirs rescan, if any.
    fn start_next_rescan(&mut self, writer: &IndexWriter) {
        let path = match self.pending_rescans.iter().next().cloned() {
            Some(p) => p,
            None => return,
        };
        self.pending_rescans.remove(&path);
        self.rescan_active.store(true, Ordering::Relaxed);

        let writer = writer.clone();
        let rescan_active = Arc::clone(&self.rescan_active);

        log::info!("MustScanSubDirs: reconcile starting for {}", path.display());

        tokio::task::spawn_blocking(move || {
            let cancelled = AtomicBool::new(false);
            let conn = match IndexStore::open_write_connection(&writer.db_path()) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!(
                        "MustScanSubDirs: couldn't open read connection for {} — {e}",
                        path.display()
                    );
                    rescan_active.store(false, Ordering::Relaxed);
                    return;
                }
            };

            match reconcile_subtree(&path, &conn, &writer, &cancelled) {
                Ok(summary) => {
                    if summary.duration.as_secs() > 10 {
                        log::warn!(
                            "MustScanSubDirs: reconcile slow for {} (+{} -{} ~{}, {}s)",
                            path.display(),
                            summary.added,
                            summary.removed,
                            summary.updated,
                            summary.duration.as_secs(),
                        );
                    } else {
                        log::info!(
                            "MustScanSubDirs: reconcile complete for {} (+{} -{} ~{}, {}ms)",
                            path.display(),
                            summary.added,
                            summary.removed,
                            summary.updated,
                            summary.duration.as_millis(),
                        );
                    }
                }
                Err(e) => {
                    log::warn!("MustScanSubDirs: reconcile failed for {} — {e}", path.display());
                }
            }

            DEBUG_STATS.record_rescan_completed();
            rescan_active.store(false, Ordering::Relaxed);
        });
    }

    /// Whether the reconciler's event buffer overflowed during the scan.
    pub(super) fn did_buffer_overflow(&self) -> bool {
        self.buffer_overflow
    }

    /// Number of buffered events (for diagnostics).
    #[cfg(test)]
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the reconciler is in buffering mode.
    #[cfg(test)]
    pub fn is_buffering(&self) -> bool {
        self.buffering
    }
}

// ── Subtree reconciliation ───────────────────────────────────────────

/// Summary of a subtree reconciliation.
pub(super) struct ReconcileSummary {
    pub added: u64,
    pub removed: u64,
    pub updated: u64,
    pub duration: std::time::Duration,
}

/// Reconcile a subtree by diffing the filesystem against the DB directory-by-directory.
///
/// Unlike `scanner::scan_subtree` which deletes all descendants then re-inserts,
/// this function walks each directory, compares children by name, and only writes
/// the differences. Safe to interrupt at any point — the DB is never in a
/// partially-deleted state.
pub(super) fn reconcile_subtree(
    root: &Path,
    conn: &Connection,
    writer: &IndexWriter,
    cancelled: &AtomicBool,
) -> Result<ReconcileSummary, String> {
    let start = Instant::now();
    let mut added: u64 = 0;
    let mut removed: u64 = 0;
    let mut updated: u64 = 0;

    let root_str = firmlinks::normalize_path(&root.to_string_lossy());
    let root_id = match store::resolve_path(conn, &root_str) {
        Ok(Some(id)) => id,
        Ok(None) => {
            log::debug!("reconcile_subtree: root not in DB, skipping: {root_str}");
            return Ok(ReconcileSummary {
                added: 0,
                removed: 0,
                updated: 0,
                duration: start.elapsed(),
            });
        }
        Err(e) => return Err(format!("resolve_path for root: {e}")),
    };

    let mut queue: VecDeque<(PathBuf, i64)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), root_id));

    // Collect newly-created directories so we can flush the writer, resolve their IDs,
    // and then queue them for recursive processing.
    let mut new_dir_paths: Vec<PathBuf> = Vec::new();

    while let Some((dir_path, dir_id)) = queue.pop_front() {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }

        let fs_children = match read_fs_children(&dir_path) {
            Some(c) => c,
            None => continue,
        };

        let db_children =
            IndexStore::list_children_on(dir_id, conn).map_err(|e| format!("list_children_on({dir_id}): {e}"))?;

        let mut db_by_name: std::collections::HashMap<String, &store::EntryRow> =
            std::collections::HashMap::with_capacity(db_children.len());
        for row in &db_children {
            db_by_name.insert(store::normalize_for_comparison(&row.name), row);
        }

        let mut matched_db_keys: HashSet<String> = HashSet::with_capacity(fs_children.len());

        for (name, meta, is_symlink) in &fs_children {
            let norm_name = store::normalize_for_comparison(name);
            let is_dir = meta.is_dir();

            if let Some(db_row) = db_by_name.get(&norm_name) {
                matched_db_keys.insert(norm_name);

                let changed = if is_dir || *is_symlink {
                    entry_modified_at(meta) != db_row.modified_at
                } else {
                    let (fs_size, fs_mtime) = entry_size_and_mtime(meta);
                    fs_size != db_row.size || fs_mtime != db_row.modified_at
                };

                if changed {
                    // If the entry changed from directory to file (or vice versa),
                    // delete the old subtree first to avoid orphaning children.
                    if db_row.is_directory != is_dir && db_row.is_directory {
                        let _ = writer.send(WriteMessage::DeleteSubtreeById(db_row.id));
                    }

                    let (size, modified_at) = if is_dir || *is_symlink {
                        (None, entry_modified_at(meta))
                    } else {
                        entry_size_and_mtime(meta)
                    };
                    let _ = writer.send(WriteMessage::UpsertEntryV2 {
                        parent_id: dir_id,
                        name: name.clone(),
                        is_directory: is_dir,
                        is_symlink: *is_symlink,
                        size,
                        modified_at,
                    });
                    updated += 1;
                }

                if is_dir && !is_symlink {
                    queue.push_back((dir_path.join(name), db_row.id));
                }
            } else {
                let (size, modified_at) = if is_dir || *is_symlink {
                    (None, entry_modified_at(meta))
                } else {
                    entry_size_and_mtime(meta)
                };
                let _ = writer.send(WriteMessage::UpsertEntryV2 {
                    parent_id: dir_id,
                    name: name.clone(),
                    is_directory: is_dir,
                    is_symlink: *is_symlink,
                    size,
                    modified_at,
                });

                if is_dir {
                    let _ = writer.send(WriteMessage::PropagateDeltaById {
                        entry_id: dir_id,
                        size_delta: 0,
                        file_count_delta: 0,
                        dir_count_delta: 1,
                    });
                } else if let Some(sz) = size {
                    let _ = writer.send(WriteMessage::PropagateDeltaById {
                        entry_id: dir_id,
                        size_delta: sz as i64,
                        file_count_delta: 1,
                        dir_count_delta: 0,
                    });
                }
                added += 1;

                if is_dir && !is_symlink {
                    new_dir_paths.push(dir_path.join(name));
                }
            }
        }

        for row in &db_children {
            let norm_name = store::normalize_for_comparison(&row.name);
            if !matched_db_keys.contains(&norm_name) {
                if row.is_directory {
                    let _ = writer.send(WriteMessage::DeleteSubtreeById(row.id));
                } else {
                    let _ = writer.send(WriteMessage::DeleteEntryById(row.id));
                }
                removed += 1;
            }
        }

        // If we found new directories and the queue is empty (current level done),
        // flush the writer so the read connection can resolve the new IDs.
        if !new_dir_paths.is_empty() && queue.is_empty() {
            if let Err(e) = writer.flush_blocking() {
                log::warn!("reconcile_subtree: flush failed: {e}");
            }
            for new_dir in new_dir_paths.drain(..) {
                let path_str = firmlinks::normalize_path(&new_dir.to_string_lossy());
                if let Ok(Some(id)) = store::resolve_path(conn, &path_str) {
                    queue.push_back((new_dir, id));
                }
            }
        }
    }

    Ok(ReconcileSummary {
        added,
        removed,
        updated,
        duration: start.elapsed(),
    })
}

/// Read and filter filesystem children of a directory.
fn read_fs_children(dir_path: &Path) -> Option<Vec<(String, std::fs::Metadata, bool)>> {
    let read_dir = match std::fs::read_dir(dir_path) {
        Ok(rd) => rd,
        Err(e) => {
            log::debug!("reconcile_subtree: can't read {}: {e}", dir_path.display());
            return None;
        }
    };

    let mut children = Vec::new();
    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().to_string();
        let child_path = dir_path.join(&name);
        let normalized_child = firmlinks::normalize_path(&child_path.to_string_lossy());
        if scanner::should_exclude(&normalized_child) {
            continue;
        }
        if let Ok(meta) = std::fs::symlink_metadata(&child_path) {
            let is_symlink = meta.is_symlink();
            children.push((name, meta, is_symlink));
        }
    }
    Some(children)
}

// ── Event processing ─────────────────────────────────────────────────

/// Process a single filesystem event. Returns affected parent paths for UI notification.
///
/// Shared between replay and live mode. Normalizes paths, checks exclusions,
/// stats the file, resolves paths to integer entry IDs, and sends appropriate
/// integer-keyed write messages (`UpsertEntryV2`, `DeleteEntryById`, etc.).
pub(super) fn process_fs_event(event: &FsChangeEvent, conn: &Connection, writer: &IndexWriter) -> Option<Vec<String>> {
    let normalized = firmlinks::normalize_path(&event.path);

    // Skip excluded paths
    if scanner::should_exclude(&normalized) {
        return None;
    }

    // Skip HistoryDone marker events
    if event.flags.history_done {
        return None;
    }

    let parent_path = compute_parent_path(&normalized);
    let mut affected = collect_ancestor_paths(&normalized);

    if event.flags.item_removed {
        return handle_removal(&normalized, conn, event, writer, affected);
    }

    if event.flags.item_created || event.flags.item_modified || event.flags.item_renamed {
        return handle_creation_or_modification(&normalized, &parent_path, conn, event, writer, &mut affected);
    }

    // For other flag combinations (xattr, owner change, etc.), just stat and update
    if event.flags.item_is_file || event.flags.item_is_dir {
        return handle_creation_or_modification(&normalized, &parent_path, conn, event, writer, &mut affected);
    }

    None
}

/// Handle a file/directory removal event.
///
/// FSEvents can deliver `item_removed` for paths that still exist on disk
/// (e.g., atomic file swaps, coalesced events with OR'd flags). To avoid
/// deleting live entries, we stat the path first: if it exists, delegate to
/// `handle_creation_or_modification` (which upserts). Only delete from the DB
/// when the path is truly gone from the filesystem.
fn handle_removal(
    normalized: &str,
    conn: &Connection,
    event: &FsChangeEvent,
    writer: &IndexWriter,
    mut affected: Vec<String>,
) -> Option<Vec<String>> {
    // Check if the path actually exists on disk before deleting from the DB.
    if Path::new(normalized).symlink_metadata().is_ok() {
        // Path still exists — treat as a modification, not a removal.
        let parent_path = compute_parent_path(normalized);
        return handle_creation_or_modification(normalized, &parent_path, conn, event, writer, &mut affected);
    }

    // Path is truly gone — resolve and delete from DB
    let entry_id = match store::resolve_path(conn, normalized) {
        Ok(Some(id)) => id,
        Ok(None) => {
            log::debug!("Reconciler: removal for unknown path, skipping: {normalized}");
            return Some(affected);
        }
        Err(e) => {
            log::warn!("Reconciler: resolve_path failed for removal {normalized}: {e}");
            return Some(affected);
        }
    };

    if event.flags.item_is_dir {
        let _ = writer.send(WriteMessage::DeleteSubtreeById(entry_id));
    } else {
        let _ = writer.send(WriteMessage::DeleteEntryById(entry_id));
    }

    Some(affected)
}

/// Handle file/directory creation, modification, or rename.
///
/// Resolves the parent path to an integer ID and sends `UpsertEntryV2`.
/// For new entries (create), also sends `PropagateDeltaById` starting
/// from the parent so dir_stats are updated along the ancestor chain.
fn handle_creation_or_modification(
    normalized: &str,
    parent_path: &str,
    conn: &Connection,
    event: &FsChangeEvent,
    writer: &IndexWriter,
    affected: &mut Vec<String>,
) -> Option<Vec<String>> {
    // Stat the file to get current metadata
    let path = Path::new(normalized);
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => {
            // Path doesn't exist (deleted since event was generated).
            // Treat as a removal: resolve to entry ID and send integer-keyed delete.
            // Use DeleteSubtreeById for directories to also remove child entries;
            // journal replay may coalesce child events into a parent dir event,
            // leaving orphaned children without a subtree delete.
            match store::resolve_path(conn, normalized) {
                Ok(Some(id)) => {
                    if event.flags.item_is_dir {
                        let _ = writer.send(WriteMessage::DeleteSubtreeById(id));
                    } else {
                        let _ = writer.send(WriteMessage::DeleteEntryById(id));
                    }
                }
                Ok(None) => {
                    // Not in DB either -- nothing to do
                }
                Err(e) => {
                    log::warn!("Reconciler: resolve_path failed for gone path {normalized}: {e}");
                }
            }
            return Some(affected.clone());
        }
    };

    // Resolve parent path to integer ID
    let parent_id = match store::resolve_path(conn, parent_path) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Parent not in DB -- stale event (intermediate directory missing), skip
            log::debug!("Reconciler: parent path not in DB, skipping event for {normalized} (parent: {parent_path})");
            return Some(affected.clone());
        }
        Err(e) => {
            log::warn!("Reconciler: resolve_path failed for parent {parent_path}: {e}");
            return Some(affected.clone());
        }
    };

    let is_dir = metadata.is_dir();
    let is_symlink = metadata.is_symlink();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let (size, modified_at) = if is_dir || is_symlink {
        (None, entry_modified_at(&metadata))
    } else {
        entry_size_and_mtime(&metadata)
    };

    let _ = writer.send(WriteMessage::UpsertEntryV2 {
        parent_id,
        name,
        is_directory: is_dir,
        is_symlink,
        size,
        modified_at,
    });

    // Propagate delta for newly created entries.
    // Start propagation from the parent directory (parent_id), since that's
    // the first ancestor whose dir_stats need updating.
    if event.flags.item_created {
        if is_dir {
            let _ = writer.send(WriteMessage::PropagateDeltaById {
                entry_id: parent_id,
                size_delta: 0,
                file_count_delta: 0,
                dir_count_delta: 1,
            });
        } else if let Some(sz) = size {
            let _ = writer.send(WriteMessage::PropagateDeltaById {
                entry_id: parent_id,
                size_delta: sz as i64,
                file_count_delta: 1,
                dir_count_delta: 0,
            });
        }

        // For new directories, also add them to affected paths
        if is_dir {
            affected.push(normalized.to_string());
        }
    }

    Some(affected.clone())
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Compute parent path from a normalized path.
fn compute_parent_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => String::new(),
    }
}

/// Collect all ancestor paths from the immediate parent up to "/".
/// Used to notify the frontend that dir_stats changed along the entire ancestor chain
/// (since propagate_delta updates all ancestors, not just the direct parent).
fn collect_ancestor_paths(path: &str) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut current = path.to_string();
    loop {
        let parent = compute_parent_path(&current);
        if parent.is_empty() || parent == current {
            break;
        }
        ancestors.push(parent.clone());
        if parent == "/" {
            break;
        }
        current = parent;
    }
    ancestors
}

/// Get physical file size and modified time from metadata.
#[cfg(unix)]
pub(super) fn entry_size_and_mtime(metadata: &std::fs::Metadata) -> (Option<u64>, Option<u64>) {
    use std::os::unix::fs::MetadataExt;
    let blocks = metadata.blocks();
    let physical_size = if blocks > 0 { blocks * 512 } else { metadata.len() };
    let mtime = metadata.mtime();
    let mtime_u64 = if mtime >= 0 { Some(mtime as u64) } else { None };
    (Some(physical_size), mtime_u64)
}

#[cfg(not(unix))]
pub(super) fn entry_size_and_mtime(metadata: &std::fs::Metadata) -> (Option<u64>, Option<u64>) {
    let size = metadata.len();
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    (Some(size), mtime)
}

/// Get modified time from metadata.
#[cfg(unix)]
pub(super) fn entry_modified_at(metadata: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    let mtime = metadata.mtime();
    if mtime >= 0 { Some(mtime as u64) } else { None }
}

#[cfg(not(unix))]
pub(super) fn entry_modified_at(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Emit an `index-dir-updated` event to the frontend.
pub(super) fn emit_dir_updated(app: &AppHandle, paths: Vec<String>) {
    let _ = app.emit("index-dir-updated", crate::indexing::IndexDirUpdatedEvent { paths });
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::store::{IndexStore, ROOT_ID};
    use crate::indexing::watcher::FsEventFlags;

    fn make_event(path: &str, event_id: u64, flags: FsEventFlags) -> FsChangeEvent {
        FsChangeEvent {
            path: path.to_string(),
            event_id,
            flags,
        }
    }

    fn created_file_flags() -> FsEventFlags {
        FsEventFlags {
            item_created: true,
            item_is_file: true,
            ..Default::default()
        }
    }

    fn removed_file_flags() -> FsEventFlags {
        FsEventFlags {
            item_removed: true,
            item_is_file: true,
            ..Default::default()
        }
    }

    fn modified_file_flags() -> FsEventFlags {
        FsEventFlags {
            item_modified: true,
            item_is_file: true,
            ..Default::default()
        }
    }

    fn created_dir_flags() -> FsEventFlags {
        FsEventFlags {
            item_created: true,
            item_is_dir: true,
            ..Default::default()
        }
    }

    fn removed_dir_flags() -> FsEventFlags {
        FsEventFlags {
            item_removed: true,
            item_is_dir: true,
            ..Default::default()
        }
    }

    fn history_done_flags() -> FsEventFlags {
        FsEventFlags {
            history_done: true,
            ..Default::default()
        }
    }

    // ── Reconciler buffer/replay tests ───────────────────────────────

    #[test]
    fn reconciler_starts_in_buffering_mode() {
        let reconciler = EventReconciler::new();
        assert!(reconciler.is_buffering());
        assert_eq!(reconciler.buffer_len(), 0);
    }

    #[test]
    fn buffer_events_during_scan() {
        let mut reconciler = EventReconciler::new();

        reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
        reconciler.buffer_event(make_event("/test/b.txt", 20, modified_file_flags()));
        reconciler.buffer_event(make_event("/test/c.txt", 30, removed_file_flags()));

        assert_eq!(reconciler.buffer_len(), 3);
    }

    #[test]
    fn switch_to_live_clears_buffer() {
        let mut reconciler = EventReconciler::new();

        reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
        reconciler.buffer_event(make_event("/test/b.txt", 20, created_file_flags()));

        reconciler.switch_to_live();

        assert!(!reconciler.is_buffering());
        assert_eq!(reconciler.buffer_len(), 0);
    }

    #[test]
    fn events_not_buffered_in_live_mode() {
        let mut reconciler = EventReconciler::new();
        reconciler.switch_to_live();

        // In live mode, buffer_event is a no-op
        reconciler.buffer_event(make_event("/test/a.txt", 10, created_file_flags()));
        assert_eq!(reconciler.buffer_len(), 0);
    }

    // ── Event processing tests ───────────────────────────────────────

    #[test]
    fn excluded_paths_are_skipped() {
        // Use a platform-appropriate excluded path
        #[cfg(target_os = "macos")]
        let excluded_path = "/System/Volumes/VM/swapfile0";
        #[cfg(target_os = "linux")]
        let excluded_path = "/proc/1/status";
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let excluded_path = "/dev/null";

        let event = make_event(excluded_path, 1, created_file_flags());
        let (writer, _dir, conn) = setup_test_writer();
        let result = process_fs_event(&event, &conn, &writer);
        assert!(result.is_none());
        writer.shutdown();
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn system_paths_without_firmlink_are_skipped() {
        // /System/foo paths that aren't firmlinked should be excluded
        let event = make_event("/System/Library/Frameworks/foo", 1, created_file_flags());
        let (writer, _dir, conn) = setup_test_writer();
        let result = process_fs_event(&event, &conn, &writer);
        assert!(result.is_none());
        writer.shutdown();
    }

    #[test]
    fn history_done_events_are_skipped() {
        let event = make_event("/test/file.txt", 1, history_done_flags());
        let (writer, _dir, conn) = setup_test_writer();
        let result = process_fs_event(&event, &conn, &writer);
        assert!(result.is_none());
        writer.shutdown();
    }

    #[test]
    fn compute_parent_path_cases() {
        assert_eq!(compute_parent_path("/Users/foo/bar.txt"), "/Users/foo");
        assert_eq!(compute_parent_path("/Users"), "/");
        assert_eq!(compute_parent_path("/"), "/");
    }

    #[tokio::test]
    async fn must_scan_sub_dirs_queued() {
        let mut reconciler = EventReconciler::new();
        reconciler.switch_to_live();

        let (writer, _dir, _conn) = setup_test_writer();
        reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);

        // Should not have any pending rescans after starting one
        // (it was popped from the set and started)
        assert!(reconciler.pending_rescans.is_empty());
        assert!(reconciler.rescan_active.load(Ordering::Relaxed));

        writer.shutdown();
    }

    #[tokio::test]
    async fn must_scan_sub_dirs_deduplication() {
        let mut reconciler = EventReconciler::new();
        reconciler.switch_to_live();

        // Mark rescan as active so new ones get queued
        reconciler.rescan_active.store(true, Ordering::Relaxed);

        let (writer, _dir, _conn) = setup_test_writer();
        reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);
        reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/dir"), &writer);
        reconciler.queue_must_scan_sub_dirs(PathBuf::from("/test/other"), &writer);

        // Deduplication: only 2 unique paths should be queued
        assert_eq!(reconciler.pending_rescans.len(), 2);

        writer.shutdown();
    }

    // ── Event processing with real files ────────────────────────────

    #[test]
    fn process_file_creation_writes_entry() {
        let (writer, dir, conn) = setup_test_writer();

        // Create a real file so stat() works (must be outside excluded paths)
        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("created.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        // Pre-populate DB with the parent directory chain so resolve_path works.
        // In production, the full scan populates all directories before live events.
        let db_path = dir.path().join("test-reconciler.db");
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        let event = make_event(&file_path.to_string_lossy(), 50, created_file_flags());

        let result = process_fs_event(&event, &conn, &writer);
        assert!(result.is_some());

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // Verify the entry was written to DB
        let store = IndexStore::open(&db_path).unwrap();
        let parent = test_dir.path().to_string_lossy().to_string();
        let parent_id = store::resolve_path(store.read_conn(), &parent).unwrap().unwrap();
        let entries = store.list_children(parent_id).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "created.txt");
        assert!(entries[0].size.unwrap_or(0) > 0);
    }

    #[test]
    fn process_file_removal_deletes_entry() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        // Pre-populate the parent dir and entry using integer-keyed inserts
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let gone_id = IndexStore::insert_entry_v2(&wconn, ROOT_ID, "gone", true, false, None, None).unwrap();
            IndexStore::insert_entry_v2(&wconn, gone_id, "deleted.txt", false, false, Some(100), None).unwrap();
        }

        let event = make_event("/gone/deleted.txt", 60, removed_file_flags());
        let result = process_fs_event(&event, &conn, &writer);
        assert!(result.is_some());

        writer.flush_blocking().unwrap();
        writer.shutdown();

        let store = IndexStore::open(&db_path).unwrap();
        let gone_id = store::resolve_path(store.read_conn(), "/gone").unwrap().unwrap();
        let entries = store.list_children(gone_id).unwrap();
        assert!(entries.is_empty(), "deleted entry should be removed from DB");
    }

    #[test]
    fn process_dir_creation_writes_entry_and_propagates() {
        let (writer, dir, conn) = setup_test_writer();

        // Create a real directory (must be outside excluded paths)
        let test_dir = non_excluded_tempdir();
        let new_dir = test_dir.path().join("newdir");
        std::fs::create_dir(&new_dir).unwrap();

        // Pre-populate DB with the parent directory chain
        let db_path = dir.path().join("test-reconciler.db");
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        let event = make_event(&new_dir.to_string_lossy(), 70, created_dir_flags());

        let result = process_fs_event(&event, &conn, &writer);
        assert!(result.is_some());

        // The affected paths should include both the parent and the new dir itself
        let paths = result.unwrap();
        assert!(!paths.is_empty());

        writer.flush_blocking().unwrap();
        writer.shutdown();

        let store = IndexStore::open(&db_path).unwrap();
        let parent = test_dir.path().to_string_lossy().to_string();
        let parent_id = store::resolve_path(store.read_conn(), &parent).unwrap().unwrap();
        let entries = store.list_children(parent_id).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_directory);
        assert_eq!(entries[0].name, "newdir");
    }

    #[test]
    fn process_dir_removal_deletes_subtree() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        // Pre-populate with a directory subtree using integer-keyed inserts
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let parent_id = IndexStore::insert_entry_v2(&wconn, ROOT_ID, "parent", true, false, None, None).unwrap();
            let removed_dir_id =
                IndexStore::insert_entry_v2(&wconn, parent_id, "removed_dir", true, false, None, None).unwrap();
            IndexStore::insert_entry_v2(&wconn, removed_dir_id, "child.txt", false, false, Some(50), None).unwrap();
        }

        let event = make_event("/parent/removed_dir", 80, removed_dir_flags());
        process_fs_event(&event, &conn, &writer);

        writer.flush_blocking().unwrap();
        writer.shutdown();

        let store = IndexStore::open(&db_path).unwrap();
        let parent_id = store::resolve_path(store.read_conn(), "/parent").unwrap().unwrap();
        let children = store.list_children(parent_id).unwrap();
        assert!(children.is_empty(), "directory and its children should be deleted");
    }

    #[test]
    fn process_nonexistent_file_treated_as_removal() {
        let (writer, _dir, conn) = setup_test_writer();

        // Event for a file that was created and immediately deleted
        // Use a path not under any excluded prefix (for example, /tmp/ is excluded on Linux)
        let event = make_event("/nonexistent_cmdr_test_dir/ghost_file.txt", 90, created_file_flags());
        let result = process_fs_event(&event, &conn, &writer);
        // Should still return Some (stat fails, treated as removal)
        assert!(result.is_some());

        writer.shutdown();
    }

    /// Removal event for a path that STILL EXISTS on disk should upsert, not delete.
    /// This is the key regression test for the false-removal bug: FSEvents can deliver
    /// item_removed for paths that were atomically swapped or had coalesced flags.
    #[test]
    fn removal_event_for_existing_path_upserts_instead_of_deleting() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        // Create a real file on disk (must be outside excluded paths)
        let test_dir = non_excluded_tempdir();
        let real_file = test_dir.path().join("still_here.txt");
        std::fs::write(&real_file, "I exist!").unwrap();

        // Pre-populate DB with the parent directory chain + the file
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let parent_id = store::resolve_path(&wconn, &test_dir.path().to_string_lossy())
                .unwrap()
                .unwrap();
            IndexStore::insert_entry_v2(&wconn, parent_id, "still_here.txt", false, false, Some(100), None).unwrap();
        }

        // Send a removal event even though the file exists on disk
        let event = make_event(&real_file.to_string_lossy(), 99, removed_file_flags());
        process_fs_event(&event, &conn, &writer);

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // The file should still be in the DB (upserted, not deleted)
        let store = IndexStore::open(&db_path).unwrap();
        let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
            .unwrap()
            .unwrap();
        let children = store.list_children(parent_id).unwrap();
        assert_eq!(
            children.len(),
            1,
            "file should still be in DB — removal was a false alarm"
        );
        assert_eq!(children[0].name, "still_here.txt");
    }

    // ── Atomic swap: event with both item_removed AND item_created ──

    /// When FSEvents delivers a single event with both item_removed=true and
    /// item_created=true (atomic file swap), the file should be upserted, not
    /// deleted. process_fs_event checks item_removed first, but handle_removal
    /// stats the path: if the file exists on disk, it delegates to upsert.
    #[test]
    fn atomic_swap_event_upserts_existing_file() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("swapped.txt");
        std::fs::write(&file_path, "new content after swap").unwrap();

        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let parent_id = store::resolve_path(&wconn, &test_dir.path().to_string_lossy())
                .unwrap()
                .unwrap();
            IndexStore::insert_entry_v2(&wconn, parent_id, "swapped.txt", false, false, Some(50), Some(1000)).unwrap();
        }

        // Both item_removed and item_created set (atomic swap scenario)
        let flags = FsEventFlags {
            item_removed: true,
            item_created: true,
            item_is_file: true,
            ..Default::default()
        };
        let event = make_event(&file_path.to_string_lossy(), 120, flags);
        let result = process_fs_event(&event, &conn, &writer);
        assert!(result.is_some());

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // The file should still be in the DB (upserted, not deleted)
        let store = IndexStore::open(&db_path).unwrap();
        let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
            .unwrap()
            .unwrap();
        let children = store.list_children(parent_id).unwrap();
        assert_eq!(children.len(), 1, "file should be upserted, not deleted (atomic swap)");
        assert_eq!(children[0].name, "swapped.txt");
    }

    // ── MustScanSubDirs uses reconcile, not destructive reinsert ──

    /// MustScanSubDirs for a directory that exists in the DB with children and
    /// on disk unchanged should preserve all children. reconcile_subtree diffs
    /// the filesystem against the DB rather than deleting and reinserting.
    /// Regression for 31df59e.
    #[test]
    fn must_scan_sub_dirs_preserves_existing_children() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        // Create a directory with children on disk
        let test_dir = non_excluded_tempdir();
        let sub_dir = test_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir).unwrap();
        std::fs::write(sub_dir.join("child1.txt"), "aaa").unwrap();
        std::fs::write(sub_dir.join("child2.txt"), "bbb").unwrap();

        // Populate DB with the directory tree matching disk
        ensure_path_in_db(&db_path, &sub_dir.to_string_lossy());
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let sub_id = store::resolve_path(&wconn, &sub_dir.to_string_lossy())
                .unwrap()
                .unwrap();

            let meta1 = std::fs::symlink_metadata(sub_dir.join("child1.txt")).unwrap();
            let (size1, mtime1) = entry_size_and_mtime(&meta1);
            IndexStore::insert_entry_v2(&wconn, sub_id, "child1.txt", false, false, size1, mtime1).unwrap();

            let meta2 = std::fs::symlink_metadata(sub_dir.join("child2.txt")).unwrap();
            let (size2, mtime2) = entry_size_and_mtime(&meta2);
            IndexStore::insert_entry_v2(&wconn, sub_id, "child2.txt", false, false, size2, mtime2).unwrap();
        }

        // Run reconcile_subtree (what MustScanSubDirs triggers)
        let cancelled = AtomicBool::new(false);
        let result = reconcile_subtree(&sub_dir, &conn, &writer, &cancelled);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.added, 0, "no new entries expected");
        assert_eq!(summary.removed, 0, "no entries should be removed");

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // Verify all children are still in the DB
        let store = IndexStore::open(&db_path).unwrap();
        let sub_id = store::resolve_path(store.read_conn(), &sub_dir.to_string_lossy())
            .unwrap()
            .unwrap();
        let children = store.list_children(sub_id).unwrap();
        assert_eq!(children.len(), 2, "both children should remain after reconcile");
        let names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"child1.txt"));
        assert!(names.contains(&"child2.txt"));
    }

    // ── False removal of a directory ──────────────────────────────

    /// item_removed for a DIRECTORY that still exists on disk should upsert,
    /// not delete. This is more damaging than the file case because
    /// DeleteSubtreeById wipes the entire subtree. Regression for f0c225f.
    #[test]
    fn removal_event_for_existing_directory_upserts_not_deletes() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        // Create a directory with a child on disk
        let test_dir = non_excluded_tempdir();
        let target_dir = test_dir.path().join("still_here");
        std::fs::create_dir(&target_dir).unwrap();
        std::fs::write(target_dir.join("precious.txt"), "don't delete me").unwrap();

        // Populate DB with the directory tree
        ensure_path_in_db(&db_path, &target_dir.to_string_lossy());
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let dir_id = store::resolve_path(&wconn, &target_dir.to_string_lossy())
                .unwrap()
                .unwrap();
            IndexStore::insert_entry_v2(&wconn, dir_id, "precious.txt", false, false, Some(100), Some(1000)).unwrap();
        }

        // Send a false removal event for the directory (item_is_dir)
        let flags = FsEventFlags {
            item_removed: true,
            item_is_dir: true,
            ..Default::default()
        };
        let event = make_event(&target_dir.to_string_lossy(), 150, flags);
        let result = process_fs_event(&event, &conn, &writer);
        assert!(result.is_some());

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // The directory should still be in the DB
        let store = IndexStore::open(&db_path).unwrap();
        let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
            .unwrap()
            .unwrap();
        let parent_children = store.list_children(parent_id).unwrap();
        assert_eq!(
            parent_children.len(),
            1,
            "directory should still exist in DB (false removal, stat-before-delete)"
        );
        assert_eq!(parent_children[0].name, "still_here");
        assert!(parent_children[0].is_directory);

        // The child should also still be in the DB (no subtree wipe)
        let dir_id = store::resolve_path(store.read_conn(), &target_dir.to_string_lossy())
            .unwrap()
            .unwrap();
        let dir_children = store.list_children(dir_id).unwrap();
        assert_eq!(
            dir_children.len(),
            1,
            "child file should survive — DeleteSubtreeById must not have been sent"
        );
        assert_eq!(dir_children[0].name, "precious.txt");
    }

    // ── Subtree reconciliation tests ──────────────────────────────

    #[test]
    fn reconcile_new_file() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("new_file.txt");
        std::fs::write(&file_path, "hello reconcile").unwrap();

        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        let cancelled = AtomicBool::new(false);
        let result = reconcile_subtree(test_dir.path(), &conn, &writer, &cancelled);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.added, 1);
        assert_eq!(summary.removed, 0);

        writer.flush_blocking().unwrap();
        writer.shutdown();

        let store = IndexStore::open(&db_path).unwrap();
        let parent_str = test_dir.path().to_string_lossy().to_string();
        let parent_id = store::resolve_path(store.read_conn(), &parent_str).unwrap().unwrap();
        let entries = store.list_children(parent_id).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "new_file.txt");
        assert!(entries[0].size.unwrap_or(0) > 0);
    }

    #[test]
    fn reconcile_deleted_file() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();

        // Insert the test dir and a file entry into the DB, but don't create the file on disk
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let parent_str = test_dir.path().to_string_lossy().to_string();
            let parent_id = store::resolve_path(&wconn, &parent_str).unwrap().unwrap();
            IndexStore::insert_entry_v2(&wconn, parent_id, "ghost.txt", false, false, Some(42), Some(1000)).unwrap();
        }

        let cancelled = AtomicBool::new(false);
        let result = reconcile_subtree(test_dir.path(), &conn, &writer, &cancelled);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.removed, 1);
        assert_eq!(summary.added, 0);

        writer.flush_blocking().unwrap();
        writer.shutdown();

        let store = IndexStore::open(&db_path).unwrap();
        let parent_str = test_dir.path().to_string_lossy().to_string();
        let parent_id = store::resolve_path(store.read_conn(), &parent_str).unwrap().unwrap();
        let entries = store.list_children(parent_id).unwrap();
        assert!(entries.is_empty(), "ghost entry should be removed from DB");
    }

    #[test]
    fn reconcile_unchanged() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("stable.txt");
        std::fs::write(&file_path, "no changes").unwrap();

        // Insert the directory into the DB
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        // Get the file's actual metadata and insert a matching DB entry
        let meta = std::fs::symlink_metadata(&file_path).unwrap();
        let (size, mtime) = entry_size_and_mtime(&meta);
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let parent_str = test_dir.path().to_string_lossy().to_string();
            let parent_id = store::resolve_path(&wconn, &parent_str).unwrap().unwrap();
            IndexStore::insert_entry_v2(&wconn, parent_id, "stable.txt", false, false, size, mtime).unwrap();
        }

        let cancelled = AtomicBool::new(false);
        let result = reconcile_subtree(test_dir.path(), &conn, &writer, &cancelled);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.added, 0);
        assert_eq!(summary.removed, 0);
        assert_eq!(summary.updated, 0);

        writer.shutdown();
    }

    #[test]
    fn reconcile_modified_file() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("changed.txt");
        std::fs::write(&file_path, "original content").unwrap();

        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        // Insert DB entry with stale metadata (different size)
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let parent_str = test_dir.path().to_string_lossy().to_string();
            let parent_id = store::resolve_path(&wconn, &parent_str).unwrap().unwrap();
            IndexStore::insert_entry_v2(&wconn, parent_id, "changed.txt", false, false, Some(999), Some(0)).unwrap();
        }

        let cancelled = AtomicBool::new(false);
        let result = reconcile_subtree(test_dir.path(), &conn, &writer, &cancelled);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.updated, 1);
        assert_eq!(summary.added, 0);
        assert_eq!(summary.removed, 0);

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // Verify the DB entry was updated with real metadata
        let store = IndexStore::open(&db_path).unwrap();
        let parent_str = test_dir.path().to_string_lossy().to_string();
        let parent_id = store::resolve_path(store.read_conn(), &parent_str).unwrap().unwrap();
        let entries = store.list_children(parent_id).unwrap();
        assert_eq!(entries.len(), 1);
        assert_ne!(entries[0].size, Some(999), "size should have been updated");
        assert_ne!(entries[0].modified_at, Some(0), "mtime should have been updated");
    }

    // ── Nested directory reconciliation tests ──────────────────────

    /// reconcile_subtree with one new nested dir + child tests the flush+re-resolve
    /// cycle: the reconciler must flush the new directory to the writer, then
    /// re-resolve its ID before inserting the child.
    #[test]
    fn reconcile_subtree_new_nested_dir_with_child() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let parent = test_dir.path().join("parent");
        std::fs::create_dir(&parent).unwrap();
        let new_dir = parent.join("new_dir");
        std::fs::create_dir(&new_dir).unwrap();
        std::fs::write(new_dir.join("child.txt"), "nested child").unwrap();

        // DB only knows about /parent/ — new_dir and child.txt are unknown
        ensure_path_in_db(&db_path, &parent.to_string_lossy());

        let cancelled = AtomicBool::new(false);
        let result = reconcile_subtree(&parent, &conn, &writer, &cancelled);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.added, 2, "new_dir and child.txt should both be added");
        assert_eq!(summary.removed, 0);

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // Verify both entries exist with correct parent relationships
        let store = IndexStore::open(&db_path).unwrap();
        let parent_id = store::resolve_path(store.read_conn(), &parent.to_string_lossy())
            .unwrap()
            .unwrap();
        let parent_children = store.list_children(parent_id).unwrap();
        assert_eq!(parent_children.len(), 1);
        assert_eq!(parent_children[0].name, "new_dir");
        assert!(parent_children[0].is_directory);

        let new_dir_id = store::resolve_path(store.read_conn(), &new_dir.to_string_lossy())
            .unwrap()
            .unwrap();
        let new_dir_children = store.list_children(new_dir_id).unwrap();
        assert_eq!(new_dir_children.len(), 1);
        assert_eq!(new_dir_children[0].name, "child.txt");
        assert!(!new_dir_children[0].is_directory);
    }

    /// Directory replaced by a file on disk: the old directory entry should become
    /// a file entry and the old directory's children should be cleaned up.
    ///
    /// This may reveal a latent bug: `reconcile_subtree` compares by normalized
    /// name and detects that `is_directory` changed. When a dir becomes a file,
    /// the reconciler deletes the old subtree before upserting the replacement,
    /// preventing orphaned children.
    #[test]
    fn reconcile_subtree_dir_replaced_by_file() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let parent = test_dir.path().join("parent");
        std::fs::create_dir(&parent).unwrap();

        // On disk: /parent/item is now a regular file
        std::fs::write(parent.join("item"), "I am a file now").unwrap();

        // DB: /parent/item/ is a directory with a child
        ensure_path_in_db(&db_path, &parent.to_string_lossy());
        {
            let wconn = IndexStore::open_write_connection(&db_path).unwrap();
            let parent_id = store::resolve_path(&wconn, &parent.to_string_lossy()).unwrap().unwrap();
            let item_id = IndexStore::insert_entry_v2(&wconn, parent_id, "item", true, false, None, None).unwrap();
            IndexStore::insert_entry_v2(&wconn, item_id, "child.txt", false, false, Some(50), None).unwrap();
        }

        let cancelled = AtomicBool::new(false);
        let result = reconcile_subtree(&parent, &conn, &writer, &cancelled);
        assert!(result.is_ok());
        let summary = result.unwrap();

        // The reconciler should see "item" as matching by name, but changed.
        // It sends an UpsertEntryV2 with is_directory=false. That's 1 update.
        // The old child.txt is never visited because a file has no children to recurse into.
        assert_eq!(summary.updated, 1, "item should be updated (dir -> file)");

        writer.flush_blocking().unwrap();
        writer.shutdown();

        let store = IndexStore::open(&db_path).unwrap();
        let parent_id = store::resolve_path(store.read_conn(), &parent.to_string_lossy())
            .unwrap()
            .unwrap();
        let children = store.list_children(parent_id).unwrap();
        assert_eq!(children.len(), 1, "parent should have exactly one child (item)");
        assert_eq!(children[0].name, "item");

        let item_id = children[0].id;
        let item_children = store.list_children(item_id).unwrap();

        assert!(!children[0].is_directory, "item should now be a file, not a directory");
        assert!(
            item_children.is_empty(),
            "file entry should have no children — old directory's child.txt should be cleaned up"
        );
    }

    /// reconcile_subtree with 3+ levels of new nested directories tests the
    /// multi-level flush cycle: each BFS level must be flushed and re-resolved
    /// before the next level's parents can be resolved.
    #[test]
    fn reconcile_subtree_deep_nested_dirs() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let root_dir = test_dir.path().join("root_dir");
        std::fs::create_dir(&root_dir).unwrap();

        // Create 3 levels of new dirs + a file: root_dir/a/b/c/file.txt
        let dir_a = root_dir.join("a");
        let dir_b = dir_a.join("b");
        let dir_c = dir_b.join("c");
        std::fs::create_dir_all(&dir_c).unwrap();
        std::fs::write(dir_c.join("file.txt"), "deep content").unwrap();

        // DB only knows about /root_dir/ — everything inside is new
        ensure_path_in_db(&db_path, &root_dir.to_string_lossy());

        let cancelled = AtomicBool::new(false);
        let result = reconcile_subtree(&root_dir, &conn, &writer, &cancelled);
        assert!(result.is_ok());
        let summary = result.unwrap();
        assert_eq!(summary.added, 4, "dirs a, b, c and file.txt should all be added");
        assert_eq!(summary.removed, 0);

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // Verify the full path chain exists with correct parent->child relationships
        let store = IndexStore::open(&db_path).unwrap();

        let root_id = store::resolve_path(store.read_conn(), &root_dir.to_string_lossy())
            .unwrap()
            .unwrap();
        let root_children = store.list_children(root_id).unwrap();
        assert_eq!(root_children.len(), 1);
        assert_eq!(root_children[0].name, "a");
        assert!(root_children[0].is_directory);

        let a_id = store::resolve_path(store.read_conn(), &dir_a.to_string_lossy())
            .unwrap()
            .unwrap();
        let a_children = store.list_children(a_id).unwrap();
        assert_eq!(a_children.len(), 1);
        assert_eq!(a_children[0].name, "b");
        assert!(a_children[0].is_directory);

        let b_id = store::resolve_path(store.read_conn(), &dir_b.to_string_lossy())
            .unwrap()
            .unwrap();
        let b_children = store.list_children(b_id).unwrap();
        assert_eq!(b_children.len(), 1);
        assert_eq!(b_children[0].name, "c");
        assert!(b_children[0].is_directory);

        let c_id = store::resolve_path(store.read_conn(), &dir_c.to_string_lossy())
            .unwrap()
            .unwrap();
        let c_children = store.list_children(c_id).unwrap();
        assert_eq!(c_children.len(), 1);
        assert_eq!(c_children[0].name, "file.txt");
        assert!(!c_children[0].is_directory);
    }

    // ── Replay tests ─────────────────────────────────────────────────

    #[test]
    fn replay_skips_events_at_or_before_scan_start() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("old.txt");
        std::fs::write(&file_path, "old").unwrap();
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        let mut reconciler = EventReconciler::new();
        reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 5, created_file_flags()));
        reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 10, created_file_flags()));

        let mut callback_called = false;
        let result = reconciler
            .replay(10, &conn, &writer, &mut |_| callback_called = true)
            .unwrap();

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // All events at or before scan_start_event_id=10 are skipped
        assert_eq!(result, 10);
        assert!(!callback_called);

        // Nothing written to DB
        let store = IndexStore::open(&db_path).unwrap();
        let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
            .unwrap()
            .unwrap();
        let children = store.list_children(parent_id).unwrap();
        assert!(children.is_empty());
    }

    #[test]
    fn replay_processes_events_after_scan_start() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("new.txt");
        std::fs::write(&file_path, "new content").unwrap();
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        let mut reconciler = EventReconciler::new();
        reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 20, created_file_flags()));

        let result = reconciler.replay(10, &conn, &writer, &mut |_| {}).unwrap();

        writer.flush_blocking().unwrap();
        writer.shutdown();

        assert_eq!(result, 20);

        let store = IndexStore::open(&db_path).unwrap();
        let parent_id = store::resolve_path(store.read_conn(), &test_dir.path().to_string_lossy())
            .unwrap()
            .unwrap();
        let children = store.list_children(parent_id).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "new.txt");
    }

    #[test]
    fn replay_sends_update_last_event_id() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_a = test_dir.path().join("a.txt");
        let file_b = test_dir.path().join("b.txt");
        std::fs::write(&file_a, "a").unwrap();
        std::fs::write(&file_b, "b").unwrap();
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        let mut reconciler = EventReconciler::new();
        reconciler.buffer_event(make_event(&file_a.to_string_lossy(), 15, created_file_flags()));
        reconciler.buffer_event(make_event(&file_b.to_string_lossy(), 25, created_file_flags()));

        let result = reconciler.replay(10, &conn, &writer, &mut |_| {}).unwrap();

        writer.flush_blocking().unwrap();
        writer.shutdown();

        // Returns the highest event_id
        assert_eq!(result, 25);

        // Verify last_event_id was persisted to the DB
        let store = IndexStore::open(&db_path).unwrap();
        let stored_id = IndexStore::get_meta(store.read_conn(), "last_event_id").unwrap();
        assert_eq!(stored_id, Some("25".to_string()));
    }

    #[test]
    fn replay_calls_callback_with_affected_paths() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("notify.txt");
        std::fs::write(&file_path, "hi").unwrap();
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        let mut reconciler = EventReconciler::new();
        reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 20, created_file_flags()));

        let mut notified_paths: Vec<String> = Vec::new();
        reconciler
            .replay(10, &conn, &writer, &mut |paths| {
                notified_paths = paths;
            })
            .unwrap();

        writer.shutdown();

        assert!(!notified_paths.is_empty());
        // The parent directory should appear in affected paths
        let parent = test_dir.path().to_string_lossy().to_string();
        assert!(
            notified_paths.iter().any(|p| p == &parent),
            "expected parent dir in affected paths, got: {notified_paths:?}"
        );
    }

    #[test]
    fn replay_empty_buffer_returns_scan_start_unchanged() {
        let (writer, _dir, conn) = setup_test_writer();

        let mut reconciler = EventReconciler::new();
        // No events buffered

        let mut callback_called = false;
        let result = reconciler
            .replay(42, &conn, &writer, &mut |_| callback_called = true)
            .unwrap();

        writer.shutdown();

        assert_eq!(result, 42);
        assert!(!callback_called);
    }

    #[test]
    fn replay_all_events_before_scan_start_returns_unchanged() {
        let (writer, dir, conn) = setup_test_writer();
        let db_path = dir.path().join("test-reconciler.db");

        let test_dir = non_excluded_tempdir();
        let file_path = test_dir.path().join("stale.txt");
        std::fs::write(&file_path, "stale").unwrap();
        ensure_path_in_db(&db_path, &test_dir.path().to_string_lossy());

        let mut reconciler = EventReconciler::new();
        reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 3, created_file_flags()));
        reconciler.buffer_event(make_event(&file_path.to_string_lossy(), 7, modified_file_flags()));

        let mut callback_called = false;
        let result = reconciler
            .replay(100, &conn, &writer, &mut |_| callback_called = true)
            .unwrap();

        writer.shutdown();

        assert_eq!(result, 100);
        assert!(!callback_called);
    }

    // ── Test helpers ─────────────────────────────────────────────────

    /// Set up a writer and a read connection for tests.
    fn setup_test_writer() -> (IndexWriter, tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test-reconciler.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        let conn = IndexStore::open_write_connection(&db_path).expect("open WAL conn for reads");
        (writer, dir, conn)
    }

    /// Ensure all components of an absolute path exist in the DB as directory entries.
    ///
    /// Walks from root downward, inserting each missing component. This simulates
    /// what the full scan does in production: all directories are indexed before
    /// live events arrive.
    fn ensure_path_in_db(db_path: &Path, abs_path: &str) {
        let conn = IndexStore::open_write_connection(db_path).unwrap();
        let components: Vec<&str> = abs_path
            .strip_prefix('/')
            .unwrap_or(abs_path)
            .split('/')
            .filter(|c| !c.is_empty())
            .collect();

        let mut current_id = ROOT_ID;
        for component in components {
            match IndexStore::resolve_component(&conn, current_id, component).unwrap() {
                Some(id) => current_id = id,
                None => {
                    current_id =
                        IndexStore::insert_entry_v2(&conn, current_id, component, true, false, None, None).unwrap();
                }
            }
        }
    }

    /// Create a temp directory outside indexing-excluded paths.
    /// On Linux, `/tmp/` is excluded from indexing; use the current directory instead.
    fn non_excluded_tempdir() -> tempfile::TempDir {
        #[cfg(target_os = "linux")]
        {
            tempfile::Builder::new()
                .prefix("cmdr_test_")
                .tempdir_in(std::env::current_dir().unwrap())
                .expect("tempdir in cwd")
        }
        #[cfg(not(target_os = "linux"))]
        {
            tempfile::tempdir().expect("tempdir")
        }
    }
}

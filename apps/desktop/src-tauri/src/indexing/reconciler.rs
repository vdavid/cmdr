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

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};

use crate::indexing::firmlinks;
use crate::indexing::scanner;
use crate::indexing::store::{self};
use crate::indexing::watcher::FsChangeEvent;
use crate::indexing::writer::{IndexWriter, WriteMessage};

// ── Exclusion check ──────────────────────────────────────────────────

/// Re-use the scanner's exclusion logic for watcher events.
/// Inlined here to avoid making scanner::should_exclude pub.
fn should_exclude(path: &str) -> bool {
    use crate::indexing::scanner::default_exclusions;

    // Check explicit exclusion prefixes
    let exclusions = default_exclusions();
    for prefix in &exclusions {
        if path.starts_with(prefix.as_str()) {
            return true;
        }
        let prefix_no_slash = prefix.trim_end_matches('/');
        if path == prefix_no_slash {
            return true;
        }
    }

    // macOS: /System/ paths -- allow only firmlinked ones
    #[cfg(target_os = "macos")]
    if path.starts_with("/System/") || path == "/System" {
        const FIRMLINKED_SYSTEM_PREFIXES: &[&str] = &[
            "/System/Library/Caches",
            "/System/Library/Assets",
            "/System/Library/PreinstalledAssets",
            "/System/Library/AssetsV2",
            "/System/Library/PreinstalledAssetsV2",
            "/System/Library/CoreServices/CoreTypes.bundle/Contents/Library",
            "/System/Library/Speech",
        ];
        for allowed in FIRMLINKED_SYSTEM_PREFIXES {
            if path.starts_with(allowed) {
                return false;
            }
        }
        return true;
    }

    false
}

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
        app: &AppHandle,
    ) -> Result<u64, String> {
        // Sort by event_id to process in order
        self.buffer.sort_by_key(|e| e.event_id);

        let total = self.buffer.len();
        let mut processed = 0u64;
        let mut last_event_id = scan_start_event_id;
        let mut affected_paths: Vec<String> = Vec::new();

        log::debug!("Reconciler: replaying {total} buffered events (scan_start_event_id={scan_start_event_id})");

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

        // Emit dir-updated for all affected paths
        if !affected_paths.is_empty() {
            emit_dir_updated(app, affected_paths);
        }

        // Store last event ID
        if last_event_id > scan_start_event_id {
            let _ = writer.send(WriteMessage::UpdateLastEventId(last_event_id));
        }

        log::debug!("Reconciler: replayed {processed}/{total} events (last_event_id={last_event_id})");
        Ok(last_event_id)
    }

    /// Switch from buffering to live mode. Clears the buffer.
    pub fn switch_to_live(&mut self) {
        self.buffering = false;
        self.buffer_overflow = false;
        self.buffer.clear();
        self.buffer.shrink_to_fit();
        log::debug!("Reconciler: switched to live mode");
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

        log::debug!("Reconciler: starting MustScanSubDirs rescan for {}", path.display());

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            let cancelled = AtomicBool::new(false);
            match scanner::scan_subtree(&path, &writer, &cancelled) {
                Ok(summary) => {
                    let duration = start.elapsed();
                    if duration.as_secs() > 10 {
                        log::warn!(
                            "Reconciler: MustScanSubDirs rescan for {} took {}s ({} entries)",
                            path.display(),
                            duration.as_secs(),
                            summary.total_entries,
                        );
                    } else {
                        log::debug!(
                            "Reconciler: MustScanSubDirs rescan for {} completed: {} entries, {}ms",
                            path.display(),
                            summary.total_entries,
                            summary.duration_ms,
                        );
                    }
                }
                Err(e) => {
                    log::warn!("Reconciler: MustScanSubDirs rescan for {} failed: {e}", path.display());
                }
            }

            rescan_active.store(false, Ordering::Relaxed);
        });
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

// ── Event processing ─────────────────────────────────────────────────

/// Process a single filesystem event. Returns affected parent paths for UI notification.
///
/// Shared between replay and live mode. Normalizes paths, checks exclusions,
/// stats the file, resolves paths to integer entry IDs, and sends appropriate
/// integer-keyed write messages (`UpsertEntryV2`, `DeleteEntryById`, etc.).
pub(super) fn process_fs_event(event: &FsChangeEvent, conn: &Connection, writer: &IndexWriter) -> Option<Vec<String>> {
    let normalized = firmlinks::normalize_path(&event.path);

    // Skip excluded paths
    if should_exclude(&normalized) {
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
/// Resolves the path to an entry ID and sends `DeleteSubtreeById` (dirs) or
/// `DeleteEntryById` (files) to the writer. If the path can't be resolved
/// (entry already gone from the DB), the event is silently skipped.
/// The writer auto-propagates accurate negative deltas after reading old data from the DB.
fn handle_removal(
    normalized: &str,
    conn: &Connection,
    event: &FsChangeEvent,
    writer: &IndexWriter,
    affected: Vec<String>,
) -> Option<Vec<String>> {
    // Resolve path to entry ID so we can send integer-keyed deletes
    let entry_id = match store::resolve_path(conn, normalized) {
        Ok(Some(id)) => id,
        Ok(None) => {
            // Entry not in DB -- already deleted or never indexed, skip
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

        // Give writer time to process
        std::thread::sleep(std::time::Duration::from_millis(200));
        writer.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(100));

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

        std::thread::sleep(std::time::Duration::from_millis(200));
        writer.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(100));

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

        std::thread::sleep(std::time::Duration::from_millis(200));
        writer.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(100));

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

        std::thread::sleep(std::time::Duration::from_millis(200));
        writer.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(100));

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

    // ── Test helpers ─────────────────────────────────────────────────

    /// Set up a writer and a read connection for tests.
    fn setup_test_writer() -> (IndexWriter, tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test-reconciler.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path).expect("spawn writer");
        let conn = IndexStore::open_write_connection(&db_path).expect("open read conn");
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

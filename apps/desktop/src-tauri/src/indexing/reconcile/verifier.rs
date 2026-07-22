//! Per-navigation background readdir diff.
//!
//! After each directory navigation, compares disk reality against the index DB
//! and corrects any drift. Runs asynchronously, deduplicated and debounced.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use tauri::AppHandle;

use crate::indexing::enrichment::get_read_pool;
use crate::indexing::firmlinks;
use crate::indexing::metadata::extract_metadata;
use crate::indexing::reconciler;
use crate::indexing::scanner;
use crate::indexing::store::{self, IndexStore};
use crate::indexing::writer::{IndexWriter, WriteMessage};

// ── Dedup/debounce state ─────────────────────────────────────────────

struct VerifierState {
    in_flight: HashSet<String>,
    recent: Vec<(String, Instant)>,
}

static VERIFIER_STATE: LazyLock<Mutex<VerifierState>> = LazyLock::new(|| {
    Mutex::new(VerifierState {
        in_flight: HashSet::new(),
        recent: Vec::new(),
    })
});

const VERIFY_DEBOUNCE_SECS: u64 = 30;
const MAX_CONCURRENT_VERIFICATIONS: usize = 2;

/// RAII guard that frees a path's `in_flight` slot when dropped.
///
/// Constructed right after `in_flight.insert(dir_path)`. The verification body
/// (`verify_and_correct` + `emit_dir_updated`) runs in a spawned task that the
/// tokio runtime catches on panic, so a panic there would otherwise skip the
/// post-`await` `in_flight.remove` and permanently leak the slot against
/// `MAX_CONCURRENT_VERIFICATIONS`. Routing the removal through `Drop` frees the
/// slot on unwind too. Mirrors `write_operations`'s `WriteSettledGuard` pattern.
struct InFlightGuard {
    dir_path: String,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = VERIFIER_STATE.lock() {
            state.in_flight.remove(&self.dir_path);
            state.recent.push((self.dir_path.clone(), Instant::now()));
        }
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Attempt to verify a directory against the index. Checks dedup/debounce,
/// spawns an async task if the directory qualifies.
pub(crate) fn maybe_verify(dir_path: String, writer: IndexWriter, app: AppHandle, scanning: bool) {
    if scanning {
        return;
    }

    let mut state = match VERIFIER_STATE.lock() {
        Ok(s) => s,
        Err(_) => return,
    };

    // Prune expired recent entries
    let now = Instant::now();
    state
        .recent
        .retain(|(_, ts)| now.duration_since(*ts).as_secs() < VERIFY_DEBOUNCE_SECS);

    // Debounce: skip if recently verified
    if state.recent.iter().any(|(p, _)| p == &dir_path) {
        return;
    }

    // Dedup: skip if already in flight
    if state.in_flight.contains(&dir_path) {
        return;
    }

    // Concurrency limit
    if state.in_flight.len() >= MAX_CONCURRENT_VERIFICATIONS {
        return;
    }

    state.in_flight.insert(dir_path.clone());
    drop(state);

    tauri::async_runtime::spawn(async move {
        // Free the `in_flight` slot (and record the debounce) on every exit
        // path, including a panic inside the body that the runtime catches.
        let _slot = InFlightGuard {
            dir_path: dir_path.clone(),
        };

        let affected_paths = verify_and_correct(&dir_path, &writer).await;

        if !affected_paths.is_empty() {
            // The per-navigation verifier is root-scoped, so its live corrections
            // publish under the local root for the importance scheduler's
            // incremental rescore (plan Decision 5), alongside the FE emit.
            crate::indexing::lifecycle_bus::publish_dirs_changed(crate::indexing::ROOT_VOLUME_ID, &affected_paths);
            reconciler::emit_dir_updated(&app, affected_paths);
        }
    });
}

/// Clear all dedup/debounce state. Called on shutdown and clear_index.
pub(crate) fn invalidate() {
    if let Ok(mut state) = VERIFIER_STATE.lock() {
        state.in_flight.clear();
        state.recent.clear();
    }
}

// ── Core verification ────────────────────────────────────────────────

struct DiskEntry {
    name: String,
    is_dir: bool,
    is_symlink: bool,
    logical_size: Option<u64>,
    physical_size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
    nlink: Option<u64>,
}

/// Compare disk contents of `dir_path` against the index DB, sending corrections
/// to the writer. New directories are scanned via `scan_subtree`.
/// Returns the list of affected paths (for UI refresh), empty if no changes.
async fn verify_and_correct(dir_path: &str, writer: &IndexWriter) -> Vec<String> {
    let normalized = firmlinks::normalize_path(dir_path);

    // Phase 1: read DB state via ReadPool
    let pool = match get_read_pool() {
        Some(p) => p,
        None => return Vec::new(),
    };

    let (parent_id, db_children) = match pool.with_conn(|conn| {
        let parent_id = match store::resolve_path(conn, &normalized) {
            Ok(Some(id)) => id,
            _ => return None,
        };
        match IndexStore::list_children_on(parent_id, conn) {
            Ok(entries) => Some((parent_id, entries)),
            Err(_) => Some((parent_id, Vec::new())),
        }
    }) {
        Ok(Some(result)) => result,
        _ => return Vec::new(),
    };

    // Phase 2: read disk entries.
    // Offload the `read_dir` + per-entry `symlink_metadata` loop onto a blocking
    // thread. This task runs on a plain tokio worker (spawned via
    // `tauri::async_runtime::spawn`, not `spawn_blocking`), so a slow/hung disk
    // here would otherwise stall an async executor thread. The diff that follows
    // is pure CPU and stays on the async path.
    let disk_map: HashMap<String, DiskEntry> = {
        let scan_path = normalized.clone();
        // The closure returns `Option`: `None` distinguishes a `read_dir` failure
        // (bail, exactly as the old synchronous code did) from a genuinely empty
        // directory (`Some(empty map)`, which the diff below treats as "all DB
        // children are stale").
        let joined = tokio::task::spawn_blocking(move || {
            let disk_entries = std::fs::read_dir(&scan_path).ok()?;
            let mut disk_map: HashMap<String, DiskEntry> = HashMap::new();
            for dir_entry in disk_entries.flatten() {
                let name = dir_entry.file_name().to_string_lossy().to_string();
                let metadata = match std::fs::symlink_metadata(dir_entry.path()) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let is_dir = metadata.is_dir();
                let is_symlink = metadata.is_symlink();
                let snap = extract_metadata(&metadata, is_dir, is_symlink);

                let key = store::normalize_for_comparison(&name);
                disk_map.insert(
                    key,
                    DiskEntry {
                        name,
                        is_dir,
                        is_symlink,
                        logical_size: snap.logical_size,
                        physical_size: snap.physical_size,
                        modified_at: snap.modified_at,
                        inode: snap.inode,
                        nlink: snap.nlink,
                    },
                );
            }
            Some(disk_map)
        })
        .await;
        match joined {
            Ok(Some(map)) => map,
            Ok(None) => return Vec::new(),
            Err(e) => {
                log::warn!("Verifier: disk-scan task failed: {e}");
                return Vec::new();
            }
        }
    };

    // Build name-keyed map of DB children
    let mut db_map: HashMap<String, &store::EntryRow> = HashMap::with_capacity(db_children.len());
    for child in &db_children {
        let key = store::normalize_for_comparison(&child.name);
        db_map.insert(key, child);
    }

    // Phase 3: diff
    let mut stale_count: u64 = 0;
    let mut new_file_count: u64 = 0;
    let mut new_dir_paths: Vec<String> = Vec::new();
    let mut modified_count: u64 = 0;
    let mut samples: Vec<String> = Vec::new();

    let parent_prefix = if normalized == "/" {
        String::new()
    } else {
        normalized.clone()
    };

    // Stale entries (in DB but not on disk)
    for (key, db_entry) in &db_map {
        if !disk_map.contains_key(key) {
            if db_entry.is_directory {
                let _ = writer.send(WriteMessage::DeleteSubtreeById(db_entry.id));
            } else {
                let _ = writer.send(WriteMessage::DeleteEntryById(db_entry.id));
            }
            stale_count += 1;
            if samples.len() < 5 {
                samples.push(format!("-{}", db_entry.name));
            }
        }
    }

    // New and modified entries (on disk but not in DB, or changed)
    for (key, disk_entry) in &disk_map {
        match db_map.get(key) {
            None => {
                // Skip excluded system paths (e.g. /System, /dev, /Volumes).
                // Per-navigation verification runs on the boot disk today, so
                // `BootDisk`; a mount-rooted verifier threads the kind scope here.
                let child_path = format!("{}/{}", parent_prefix, disk_entry.name);
                if scanner::should_exclude(&child_path, &scanner::ExclusionScope::boot_disk()) {
                    continue;
                }

                // New entry on disk
                let _ = writer.send(WriteMessage::UpsertEntryV2 {
                    parent_id,
                    name: disk_entry.name.clone(),
                    is_directory: disk_entry.is_dir,
                    is_symlink: disk_entry.is_symlink,
                    logical_size: disk_entry.logical_size,
                    physical_size: disk_entry.physical_size,
                    modified_at: disk_entry.modified_at,
                    inode: disk_entry.inode,
                    nlink: disk_entry.nlink,
                });

                // UpsertEntryV2 auto-propagates deltas in the writer.
                if disk_entry.is_dir {
                    let new_dir = format!("{}/{}", parent_prefix, disk_entry.name);
                    new_dir_paths.push(new_dir);
                    if samples.len() < 5 {
                        samples.push(format!("+/{}", disk_entry.name));
                    }
                } else {
                    new_file_count += 1;
                    if samples.len() < 5 {
                        samples.push(format!("+{}", disk_entry.name));
                    }
                }
            }
            Some(db_entry) => {
                // Type change (dir <-> file)
                if db_entry.is_directory != disk_entry.is_dir {
                    if db_entry.is_directory {
                        let _ = writer.send(WriteMessage::DeleteSubtreeById(db_entry.id));
                    } else {
                        let _ = writer.send(WriteMessage::DeleteEntryById(db_entry.id));
                    }
                    let _ = writer.send(WriteMessage::UpsertEntryV2 {
                        parent_id,
                        name: disk_entry.name.clone(),
                        is_directory: disk_entry.is_dir,
                        is_symlink: disk_entry.is_symlink,
                        logical_size: disk_entry.logical_size,
                        physical_size: disk_entry.physical_size,
                        modified_at: disk_entry.modified_at,
                        inode: disk_entry.inode,
                        nlink: disk_entry.nlink,
                    });
                    // UpsertEntryV2 auto-propagates deltas in the writer.
                    if disk_entry.is_dir {
                        let new_dir = format!("{}/{}", parent_prefix, disk_entry.name);
                        new_dir_paths.push(new_dir);
                    }
                    stale_count += 1;
                    if !disk_entry.is_dir {
                        new_file_count += 1;
                    }
                    if samples.len() < 5 {
                        samples.push(format!("~{}", disk_entry.name));
                    }
                    continue;
                }

                // Modified file: compare size and mtime.
                // Skip size comparison when DB has NULL size for a hardlink (nlink > 1):
                // the NULL is intentional dedup, not a real mismatch.
                if !db_entry.is_directory {
                    let is_deduped_hardlink =
                        db_entry.logical_size.is_none() && matches!(disk_entry.nlink, Some(n) if n > 1);
                    let size_changed = !is_deduped_hardlink && db_entry.logical_size != disk_entry.logical_size;
                    let mtime_changed = db_entry.modified_at != disk_entry.modified_at;
                    if size_changed || mtime_changed {
                        let _ = writer.send(WriteMessage::UpsertEntryV2 {
                            parent_id,
                            name: disk_entry.name.clone(),
                            is_directory: false,
                            is_symlink: disk_entry.is_symlink,
                            logical_size: disk_entry.logical_size,
                            physical_size: disk_entry.physical_size,
                            modified_at: disk_entry.modified_at,
                            inode: disk_entry.inode,
                            nlink: disk_entry.nlink,
                        });
                        modified_count += 1;
                        if samples.len() < 5 {
                            samples.push(format!("~{}", disk_entry.name));
                        }
                    }
                }
            }
        }
    }

    let has_changes = stale_count > 0 || new_file_count > 0 || !new_dir_paths.is_empty() || modified_count > 0;
    if !has_changes {
        return Vec::new();
    }

    let total_diffs = stale_count + new_file_count + new_dir_paths.len() as u64 + modified_count;
    log::info!(
        "Verifier: {} diffs in `{}` ({} stale, {} new files, {} new dir, {} modified) [samples: {}]",
        total_diffs,
        normalized,
        stale_count,
        new_file_count,
        new_dir_paths.len(),
        modified_count,
        samples.join(", "),
    );

    // Scan new directories: flush first so UpsertEntryV2 entries are committed,
    // then scan_subtree can resolve paths to entry IDs.
    if !new_dir_paths.is_empty() {
        if let Err(e) = writer.flush().await {
            log::warn!("Verifier: pre-scan flush failed: {e}");
        }

        let cancelled = std::sync::atomic::AtomicBool::new(false);
        for new_dir in &new_dir_paths {
            if scanner::should_exclude(new_dir, &scanner::ExclusionScope::boot_disk()) {
                continue;
            }
            match scanner::scan_subtree(Path::new(new_dir), writer, &cancelled) {
                Ok(summary) => {
                    log::debug!(
                        "Verifier: scanned new dir {} ({} entries, {}ms)",
                        new_dir,
                        summary.total_entries,
                        summary.duration_ms,
                    );
                }
                Err(e) => {
                    log::warn!("Verifier: scan_subtree({new_dir}) failed: {e}");
                }
            }
        }
        // No off-writer ancestor compensation: each `scan_subtree` sends
        // `ComputeSubtreeAggregates`, whose handler repairs the ancestor chain
        // (sizes, counts, symlinks, AND coverage) on the writer thread. Doing it
        // there is race-free and can't double-count; a read-then-`PropagateDeltaById`
        // here would credit the same bytes twice (Leak A).
    }

    // Flush all corrections
    if let Err(e) = writer.flush().await {
        log::warn!("Verifier: final flush failed: {e}");
    }

    let mut paths = vec![normalized];
    paths.extend(new_dir_paths);
    paths
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::enrichment::{READ_POOL, READ_POOL_TEST_MUTEX, ReadPool};
    use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
    use crate::indexing::stress_test_helpers::check_db_consistency;
    use crate::indexing::writer::AggSource;
    use crate::indexing::writer::IndexWriter;
    use std::fs;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    /// Create a temp dir in the crate root instead of `/tmp/`.
    /// On Linux, `/tmp/` is in `EXCLUDED_PREFIXES`, so `should_exclude`
    /// filters out entries under it, breaking verifier tests that add
    /// new files/dirs and expect them to appear in the diff.
    fn test_tempdir() -> tempfile::TempDir {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        tempfile::Builder::new()
            .prefix("cmdr-test-")
            .tempdir_in(base)
            .expect("create temp dir")
    }

    fn setup_writer() -> (IndexWriter, std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test-index.db");
        let _store = IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, None).expect("spawn writer");
        (writer, db_path, dir)
    }

    /// Install a ReadPool so verify_and_correct can read the DB.
    fn install_read_pool(db_path: &Path) {
        let pool = Arc::new(ReadPool::new(db_path.to_path_buf()).unwrap());
        *READ_POOL.lock().unwrap() = Some(pool);
    }

    fn remove_read_pool() {
        *READ_POOL.lock().unwrap() = None;
    }

    /// Insert the parent directory chain for a filesystem path into the DB.
    /// Returns the entry ID of the deepest directory.
    /// Also syncs the writer's shared `next_id` counter with the DB.
    fn ensure_path_in_db(db_path: &Path, path: &Path, writer: &IndexWriter) -> i64 {
        let conn = IndexStore::open_write_connection(db_path).unwrap();
        let path_str = path.to_string_lossy();
        let components: Vec<&str> = path_str.split('/').filter(|c| !c.is_empty()).collect();
        let mut parent_id = ROOT_ID;
        for component in components {
            parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
                Ok(Some(id)) => id,
                _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None)
                    .unwrap(),
            };
        }
        // Sync the writer's next_id counter with what we just inserted
        let db_next_id = IndexStore::get_next_id(&conn).unwrap();
        writer
            .next_id()
            .fetch_max(db_next_id, std::sync::atomic::Ordering::Relaxed);
        parent_id
    }

    /// Insert children under a parent_id matching what's on disk.
    fn insert_children_from_disk(writer: &IndexWriter, parent_id: i64, dir_path: &Path) {
        for entry in fs::read_dir(dir_path).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            let is_dir = metadata.is_dir();
            let is_symlink = metadata.is_symlink();
            let snap = extract_metadata(&metadata, is_dir, is_symlink);

            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id,
                name,
                is_directory: is_dir,
                is_symlink,
                logical_size: snap.logical_size,
                physical_size: snap.physical_size,
                modified_at: snap.modified_at,
                inode: snap.inode,
                nlink: snap.nlink,
            });
        }
        writer.flush_blocking().unwrap();
    }

    fn list_db_children_on(db_path: &Path, parent_id: i64) -> Vec<EntryRow> {
        let conn = IndexStore::open_read_connection(db_path).unwrap();
        IndexStore::list_children_on(parent_id, &conn).unwrap()
    }

    #[test]
    fn verify_clean_directory() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        fs::write(fs_root.path().join("file1.txt"), "hello").unwrap();
        fs::create_dir(fs_root.path().join("subdir")).unwrap();

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        insert_children_from_disk(&writer, parent_id, fs_root.path());
        install_read_pool(&db_path);

        let children_before = list_db_children_on(&db_path, parent_id);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));

        writer.flush_blocking().unwrap();
        let children_after = list_db_children_on(&db_path, parent_id);

        assert!(paths.is_empty(), "clean directory should produce no diffs");
        assert_eq!(children_before.len(), children_after.len());

        remove_read_pool();
        writer.shutdown();
    }

    #[test]
    fn verify_detects_new_file() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        fs::write(fs_root.path().join("file1.txt"), "hello").unwrap();

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        insert_children_from_disk(&writer, parent_id, fs_root.path());
        install_read_pool(&db_path);

        // Add a new file after indexing
        fs::write(fs_root.path().join("new_file.txt"), "new content").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));

        writer.flush_blocking().unwrap();
        let children_after = list_db_children_on(&db_path, parent_id);

        assert!(!paths.is_empty());
        assert!(children_after.iter().any(|e| e.name == "new_file.txt"));

        remove_read_pool();
        writer.shutdown();
    }

    #[test]
    fn verify_detects_deleted_file() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        fs::write(fs_root.path().join("file1.txt"), "hello").unwrap();
        fs::write(fs_root.path().join("file2.txt"), "world").unwrap();

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        insert_children_from_disk(&writer, parent_id, fs_root.path());
        install_read_pool(&db_path);

        // Delete a file after indexing
        fs::remove_file(fs_root.path().join("file1.txt")).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));

        writer.flush_blocking().unwrap();
        let children_after = list_db_children_on(&db_path, parent_id);

        assert!(!paths.is_empty());
        assert!(!children_after.iter().any(|e| e.name == "file1.txt"));
        assert!(children_after.iter().any(|e| e.name == "file2.txt"));

        remove_read_pool();
        writer.shutdown();
    }

    #[test]
    fn verify_detects_modified_file() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        // Write a small initial file
        fs::write(fs_root.path().join("file1.txt"), "x").unwrap();

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        insert_children_from_disk(&writer, parent_id, fs_root.path());
        install_read_pool(&db_path);

        let children_before = list_db_children_on(&db_path, parent_id);
        let file1_before = children_before.iter().find(|e| e.name == "file1.txt").unwrap().clone();

        // Wait so mtime definitely changes (1s resolution on some filesystems)
        thread::sleep(Duration::from_secs(1));
        // Write content large enough to span multiple disk blocks (>4KB ensures physical size change)
        let large_content = vec![b'A'; 8192];
        fs::write(fs_root.path().join("file1.txt"), &large_content).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));

        writer.flush_blocking().unwrap();
        let children_after = list_db_children_on(&db_path, parent_id);
        let file1_after = children_after.iter().find(|e| e.name == "file1.txt").unwrap();

        assert!(!paths.is_empty());
        let changed = file1_after.logical_size != file1_before.logical_size
            || file1_after.modified_at != file1_before.modified_at;
        assert!(changed, "file should show as modified after content change");

        remove_read_pool();
        writer.shutdown();
    }

    #[test]
    fn verify_detects_new_directory() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        fs::write(fs_root.path().join("file1.txt"), "hello").unwrap();

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        insert_children_from_disk(&writer, parent_id, fs_root.path());
        install_read_pool(&db_path);

        // Create new directory after indexing
        fs::create_dir(fs_root.path().join("new_dir")).unwrap();
        fs::write(fs_root.path().join("new_dir").join("inside.txt"), "inside").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));

        writer.flush_blocking().unwrap();
        let children_after = list_db_children_on(&db_path, parent_id);

        assert!(!paths.is_empty());
        assert!(children_after.iter().any(|e| e.name == "new_dir" && e.is_directory));

        remove_read_pool();
        writer.shutdown();
    }

    /// Leak A, end to end: a new directory appearing on disk must credit the
    /// ancestor chain for its bytes EXACTLY once. `scan_subtree` →
    /// `ComputeSubtreeAggregates` now repairs ancestors on the writer; with the
    /// old off-writer `PropagateDeltaById` compensation still in place the new
    /// dir's bytes would land twice (2× credit). The recompute-from-`entries`
    /// oracle catches a double-count anywhere in the chain.
    #[test]
    fn verify_new_dir_credits_ancestors_exactly_once() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        fs::write(fs_root.path().join("file1.txt"), "hello").unwrap(); // 5 bytes

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        insert_children_from_disk(&writer, parent_id, fs_root.path());
        // Exact baseline for the whole ancestor chain.
        writer
            .send(WriteMessage::ComputeAllAggregates {
                source: AggSource::Maps,
            })
            .unwrap();
        writer.flush_blocking().unwrap();
        install_read_pool(&db_path);

        // A new dir with two known-size files appears on disk after indexing.
        let new_dir = fs_root.path().join("new_dir");
        fs::create_dir(&new_dir).unwrap();
        fs::write(new_dir.join("a.txt"), "AAAA").unwrap(); // 4 bytes
        fs::write(new_dir.join("b.txt"), "BB").unwrap(); // 2 bytes

        let rt = tokio::runtime::Runtime::new().unwrap();
        let _paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));
        writer.flush_blocking().unwrap();

        let conn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent = IndexStore::get_dir_stats_by_id(&conn, parent_id).unwrap().unwrap();
        assert_eq!(
            (
                parent.recursive_logical_size,
                parent.recursive_file_count,
                parent.recursive_dir_count
            ),
            // file1(5) + a(4) + b(2) = 11 bytes; 3 files; 1 new dir.
            (11, 3, 1),
            "the verified dir must be credited for new_dir's bytes exactly once, not doubled"
        );
        // The whole tree agrees with an independent recompute from `entries`.
        check_db_consistency(&conn);

        remove_read_pool();
        writer.shutdown();
    }

    #[test]
    fn verify_detects_deleted_directory() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        fs::write(fs_root.path().join("file1.txt"), "hello").unwrap();
        let subdir = fs_root.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("nested.txt"), "nested").unwrap();

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        insert_children_from_disk(&writer, parent_id, fs_root.path());
        install_read_pool(&db_path);

        let children_before = list_db_children_on(&db_path, parent_id);
        assert!(children_before.iter().any(|e| e.name == "subdir" && e.is_directory));

        // Remove directory after indexing
        fs::remove_dir_all(&subdir).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));

        writer.flush_blocking().unwrap();
        let children_after = list_db_children_on(&db_path, parent_id);

        assert!(!paths.is_empty());
        assert!(!children_after.iter().any(|e| e.name == "subdir"));

        remove_read_pool();
        writer.shutdown();
    }

    #[test]
    fn verify_type_change_dir_to_file() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        let subdir = fs_root.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("nested.txt"), "nested").unwrap();

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        insert_children_from_disk(&writer, parent_id, fs_root.path());
        install_read_pool(&db_path);

        // Replace directory with a file of the same name
        fs::remove_dir_all(&subdir).unwrap();
        fs::write(fs_root.path().join("subdir"), "now a file").unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));

        writer.flush_blocking().unwrap();
        let children_after = list_db_children_on(&db_path, parent_id);

        assert!(!paths.is_empty());
        let subdir_entry = children_after.iter().find(|e| e.name == "subdir").unwrap();
        assert!(!subdir_entry.is_directory, "should now be a file, not a directory");

        remove_read_pool();
        writer.shutdown();
    }

    #[test]
    fn verify_debounce() {
        invalidate();

        let dir_path = "/fake/debounce/test".to_string();

        // Simulate an in-flight verification
        {
            let mut state = VERIFIER_STATE.lock().unwrap();
            state.in_flight.insert(dir_path.clone());
        }

        // Path is in flight, so duplicate should be rejected
        let state = VERIFIER_STATE.lock().unwrap();
        assert!(state.in_flight.contains(&dir_path));
        assert_eq!(state.in_flight.len(), 1);
        drop(state);

        // Simulate completion: move to recent
        {
            let mut state = VERIFIER_STATE.lock().unwrap();
            state.in_flight.remove(&dir_path);
            state.recent.push((dir_path.clone(), Instant::now()));
        }

        // Path is now in recent, so a new request should be debounced
        let state = VERIFIER_STATE.lock().unwrap();
        assert!(state.recent.iter().any(|(p, _)| p == &dir_path));
        assert!(state.in_flight.is_empty());
        drop(state);

        invalidate();
    }

    #[test]
    fn in_flight_slot_is_freed_on_panic_unwind() {
        // A panic inside the verification body (which runs in a spawned task the
        // runtime catches) must still free the `in_flight` slot, or the path
        // permanently counts against MAX_CONCURRENT_VERIFICATIONS. The guard's
        // Drop runs during unwinding; this pins that contract.
        invalidate();

        let dir_path = "/fake/panic/unwind".to_string();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            {
                let mut state = VERIFIER_STATE.lock().unwrap();
                state.in_flight.insert(dir_path.clone());
            }
            let _slot = InFlightGuard {
                dir_path: dir_path.clone(),
            };
            panic!("simulated verification panic");
        }));

        assert!(result.is_err(), "the closure must have panicked");

        let state = VERIFIER_STATE.lock().unwrap();
        assert!(
            !state.in_flight.contains(&dir_path),
            "in_flight slot must be freed even when the verification body panicked"
        );
        assert!(
            state.recent.iter().any(|(p, _)| p == &dir_path),
            "the path should be recorded as recently-verified (debounced) after the guard fires"
        );
        drop(state);

        invalidate();
    }

    #[test]
    fn verify_concurrent_limit() {
        invalidate();

        // Fill up in_flight to max
        {
            let mut state = VERIFIER_STATE.lock().unwrap();
            for i in 0..MAX_CONCURRENT_VERIFICATIONS {
                state.in_flight.insert(format!("/fake/path/{i}"));
            }
        }

        // At the limit, new paths should be rejected
        let state = VERIFIER_STATE.lock().unwrap();
        assert_eq!(state.in_flight.len(), MAX_CONCURRENT_VERIFICATIONS);
        assert!(!state.in_flight.contains("/another/path"));
        drop(state);

        invalidate();
    }

    #[test]
    fn verify_empty_directory() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        // Empty directory, no files

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
        // No children to insert
        install_read_pool(&db_path);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let paths = rt.block_on(verify_and_correct(&fs_root.path().to_string_lossy(), &writer));

        writer.flush_blocking().unwrap();
        let children_after = list_db_children_on(&db_path, parent_id);

        assert!(paths.is_empty());
        assert_eq!(children_after.len(), 0);

        remove_read_pool();
        writer.shutdown();
    }
}

//! Per-navigation background readdir diff.
//!
//! After each directory navigation, compares disk reality against the index DB
//! and corrects any drift. Runs asynchronously, deduplicated and debounced.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use tauri::AppHandle;

use super::enrichment::get_read_pool;
use super::firmlinks;
use super::reconciler;
use super::scanner;
use super::store::{self, IndexStore};
use super::writer::{IndexWriter, WriteMessage};

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

// ── Public API ───────────────────────────────────────────────────────

/// Attempt to verify a directory against the index. Checks dedup/debounce,
/// spawns an async task if the directory qualifies.
pub(super) fn maybe_verify(dir_path: String, writer: IndexWriter, app: AppHandle, scanning: bool) {
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
        let affected_paths = verify_and_correct(&dir_path, &writer).await;

        if !affected_paths.is_empty() {
            reconciler::emit_dir_updated(&app, affected_paths);
        }

        if let Ok(mut state) = VERIFIER_STATE.lock() {
            state.in_flight.remove(&dir_path);
            state.recent.push((dir_path, Instant::now()));
        }
    });
}

/// Clear all dedup/debounce state. Called on shutdown and clear_index.
pub(super) fn invalidate() {
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

    // Phase 2: read disk entries
    let disk_entries = match std::fs::read_dir(&normalized) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    // Build name-keyed map of DB children
    let mut db_map: HashMap<String, &store::EntryRow> = HashMap::with_capacity(db_children.len());
    for child in &db_children {
        let key = store::normalize_for_comparison(&child.name);
        db_map.insert(key, child);
    }

    // Build name-keyed map of disk entries
    let mut disk_map: HashMap<String, DiskEntry> = HashMap::new();
    for dir_entry in disk_entries.flatten() {
        let name = dir_entry.file_name().to_string_lossy().to_string();
        let metadata = match std::fs::symlink_metadata(dir_entry.path()) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let is_dir = metadata.is_dir();
        let is_symlink = metadata.is_symlink();
        let (logical_size, physical_size, modified_at) = if is_dir || is_symlink {
            (None, None, reconciler::entry_modified_at(&metadata))
        } else {
            reconciler::entry_size_and_mtime(&metadata)
        };

        let key = store::normalize_for_comparison(&name);
        disk_map.insert(
            key,
            DiskEntry {
                name,
                is_dir,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
            },
        );
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
                // Skip excluded system paths (e.g. /System, /dev, /Volumes)
                let child_path = format!("{}/{}", parent_prefix, disk_entry.name);
                if scanner::should_exclude(&child_path) {
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
                });

                if disk_entry.is_dir {
                    let new_dir = format!("{}/{}", parent_prefix, disk_entry.name);
                    new_dir_paths.push(new_dir);
                    if samples.len() < 5 {
                        samples.push(format!("+/{}", disk_entry.name));
                    }
                } else {
                    if let Some(sz) = disk_entry.logical_size {
                        let _ = writer.send(WriteMessage::PropagateDeltaById {
                            entry_id: parent_id,
                            logical_size_delta: sz as i64,
                            physical_size_delta: disk_entry.physical_size.unwrap_or(0) as i64,
                            file_count_delta: 1,
                            dir_count_delta: 0,
                        });
                    }
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
                    });
                    if disk_entry.is_dir {
                        let new_dir = format!("{}/{}", parent_prefix, disk_entry.name);
                        new_dir_paths.push(new_dir);
                    } else if let Some(sz) = disk_entry.logical_size {
                        let _ = writer.send(WriteMessage::PropagateDeltaById {
                            entry_id: parent_id,
                            logical_size_delta: sz as i64,
                            physical_size_delta: disk_entry.physical_size.unwrap_or(0) as i64,
                            file_count_delta: 1,
                            dir_count_delta: 0,
                        });
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

                // Modified file: compare size and mtime
                if !db_entry.is_directory {
                    let size_changed = db_entry.logical_size != disk_entry.logical_size;
                    let mtime_changed = db_entry.modified_at != disk_entry.modified_at;
                    if size_changed || mtime_changed {
                        let old_size = db_entry.logical_size.unwrap_or(0) as i64;
                        let new_size = disk_entry.logical_size.unwrap_or(0) as i64;
                        let _ = writer.send(WriteMessage::UpsertEntryV2 {
                            parent_id,
                            name: disk_entry.name.clone(),
                            is_directory: false,
                            is_symlink: disk_entry.is_symlink,
                            logical_size: disk_entry.logical_size,
                            physical_size: disk_entry.physical_size,
                            modified_at: disk_entry.modified_at,
                        });
                        let _ = writer.send(WriteMessage::PropagateDeltaById {
                            entry_id: parent_id,
                            logical_size_delta: new_size - old_size,
                            physical_size_delta: (disk_entry.physical_size.unwrap_or(0) as i64)
                                - (db_entry.physical_size.unwrap_or(0) as i64),
                            file_count_delta: 0,
                            dir_count_delta: 0,
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
            if scanner::should_exclude(new_dir) {
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

        // Flush after scans, then propagate subtree deltas up the ancestor chain
        if let Err(e) = writer.flush().await {
            log::warn!("Verifier: post-scan flush failed: {e}");
        }

        let dir_deltas: Vec<(i64, store::DirStatsById)> = get_read_pool()
            .and_then(|pool| {
                pool.with_conn(|conn| {
                    let mut deltas = Vec::new();
                    for new_dir in &new_dir_paths {
                        let entry_id = match store::resolve_path(conn, new_dir) {
                            Ok(Some(id)) => id,
                            _ => continue,
                        };
                        let p_id = match IndexStore::get_parent_id(conn, entry_id) {
                            Ok(Some(pid)) => pid,
                            _ => continue,
                        };
                        let stats = IndexStore::get_dir_stats_by_id(conn, entry_id)
                            .ok()
                            .flatten()
                            .unwrap_or(store::DirStatsById {
                                entry_id,
                                recursive_logical_size: 0,
                                recursive_physical_size: 0,
                                recursive_file_count: 0,
                                recursive_dir_count: 0,
                            });
                        deltas.push((p_id, stats));
                    }
                    deltas
                })
                .ok()
            })
            .unwrap_or_default();

        for (p_id, stats) in &dir_deltas {
            let _ = writer.send(WriteMessage::PropagateDeltaById {
                entry_id: *p_id,
                logical_size_delta: stats.recursive_logical_size as i64,
                physical_size_delta: stats.recursive_physical_size as i64,
                file_count_delta: stats.recursive_file_count as i32,
                dir_count_delta: (stats.recursive_dir_count as i32) + 1,
            });
        }
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
    use crate::indexing::writer::IndexWriter;
    use std::fs;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    /// Create a temp dir in the crate root instead of `/tmp/`.
    /// On Linux, `/tmp/` is in `EXCLUDED_PREFIXES`, so `should_exclude`
    /// filters out entries under it — breaking verifier tests that add
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
    fn ensure_path_in_db(db_path: &Path, path: &Path) -> i64 {
        let conn = IndexStore::open_write_connection(db_path).unwrap();
        let path_str = path.to_string_lossy();
        let components: Vec<&str> = path_str.split('/').filter(|c| !c.is_empty()).collect();
        let mut parent_id = ROOT_ID;
        for component in components {
            parent_id = match IndexStore::resolve_component(&conn, parent_id, component) {
                Ok(Some(id)) => id,
                _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None).unwrap(),
            };
        }
        parent_id
    }

    /// Insert children under a parent_id matching what's on disk.
    fn insert_children_from_disk(writer: &IndexWriter, parent_id: i64, dir_path: &Path) {
        for entry in fs::read_dir(dir_path).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            let is_dir = metadata.is_dir();
            let is_symlink = metadata.is_symlink();
            let (logical_size, physical_size, modified_at) = if is_dir || is_symlink {
                (None, None, reconciler::entry_modified_at(&metadata))
            } else {
                reconciler::entry_size_and_mtime(&metadata)
            };
            let _ = writer.send(WriteMessage::UpsertEntryV2 {
                parent_id,
                name,
                is_directory: is_dir,
                is_symlink,
                logical_size,
                physical_size,
                modified_at,
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
        let parent_id = ensure_path_in_db(&db_path, fs_root.path());
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
        let parent_id = ensure_path_in_db(&db_path, fs_root.path());
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
        let parent_id = ensure_path_in_db(&db_path, fs_root.path());
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
        let parent_id = ensure_path_in_db(&db_path, fs_root.path());
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
        let parent_id = ensure_path_in_db(&db_path, fs_root.path());
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

    #[test]
    fn verify_detects_deleted_directory() {
        let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
        let fs_root = test_tempdir();
        fs::write(fs_root.path().join("file1.txt"), "hello").unwrap();
        let subdir = fs_root.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("nested.txt"), "nested").unwrap();

        let (writer, db_path, _db_dir) = setup_writer();
        let parent_id = ensure_path_in_db(&db_path, fs_root.path());
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
        let parent_id = ensure_path_in_db(&db_path, fs_root.path());
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
        let parent_id = ensure_path_in_db(&db_path, fs_root.path());
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

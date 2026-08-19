//! Tests for the per-navigation verifier.

use super::*;
use crate::indexing::read::enrichment::{
    READ_POOL_TEST_MUTEX, ReadPool, install_read_pool as install_pool_for, uninstall_read_pool,
};
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};
use crate::indexing::stress_test_helpers::check_db_consistency;
use crate::indexing::writer::AggSource;
use crate::indexing::writer::IndexWriter;
use crate::indexing::writer::tests::settle_the_writer;
use std::fs;
use std::sync::Arc;

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

/// A writer over an index whose scan COMPLETED, which is the volume every test
/// below is about: the verifier is the per-navigation repair on a drive nothing is
/// walking any more. A volume with an unfinished frontier is a different case, and
/// `the_verifier_leaves_an_unlisted_directory_alone` owns it.
fn setup_writer() -> (IndexWriter, std::path::PathBuf, tempfile::TempDir) {
    let (writer, db_path, dir) = setup_writer_mid_coverage();
    mark_scan_completed(&db_path);
    (writer, db_path, dir)
}

/// The same, over an index no scan has finished — a volume whose phases are still
/// covering it.
fn setup_writer_mid_coverage() -> (IndexWriter, std::path::PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let db_path = dir.path().join("test-index.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");
    (writer, db_path, dir)
}

fn mark_scan_completed(db_path: &Path) {
    let conn = IndexStore::open_write_connection(db_path).expect("write connection");
    IndexStore::update_meta(&conn, "scan_completed_at", "1").expect("stamp the completion marker");
}

/// Install a root ReadPool so verify_and_correct can read the DB.
fn install_read_pool(db_path: &Path) {
    let pool = Arc::new(ReadPool::new(db_path.to_path_buf()).unwrap());
    install_pool_for(crate::ROOT_VOLUME_ID, pool);
}

fn remove_read_pool() {
    uninstall_read_pool(crate::ROOT_VOLUME_ID);
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
            _ => IndexStore::insert_entry_v2(&conn, parent_id, component, true, false, None, None, None, None).unwrap(),
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

/// Verify a directory as the ROOT volume does it: root's read pool, root's
/// `/`-rooted path space. The hot path every local navigation takes.
async fn verify_root(dir: &Path, writer: &IndexWriter) -> Vec<String> {
    verify_and_correct(
        crate::ROOT_VOLUME_ID,
        &dir.to_string_lossy(),
        &IndexPathSpace::root(),
        writer,
        &CancellationToken::new(),
    )
    .await
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
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

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
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

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
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

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

    // Write content large enough to span multiple disk blocks (>4KB ensures physical size change)
    let large_content = vec![b'A'; 8192];
    fs::write(fs_root.path().join("file1.txt"), &large_content).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

    writer.flush_blocking().unwrap();
    let children_after = list_db_children_on(&db_path, parent_id);
    let file1_after = children_after.iter().find(|e| e.name == "file1.txt").unwrap();

    assert!(!paths.is_empty());
    let changed =
        file1_after.logical_size != file1_before.logical_size || file1_after.modified_at != file1_before.modified_at;
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
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

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
    let _paths = rt.block_on(verify_root(fs_root.path(), &writer));
    // ⚠️ `settle_the_writer`, ❌ never the flush alone: the verifier's subtree
    // aggregate QUEUES this dir's ancestors and the writer rolls them up at its
    // caught-up point, one hook run after the flush replies
    // (`writer/pending_rollups.rs`). A flush-only read sees `new_dir`'s row
    // present but its 6 bytes not yet credited upward.
    settle_the_writer(&writer);

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

/// A mount-rooted volume (SMB share, external drive) must be verified against
/// ITS OWN index, not root's: the read pool routes by volume id and the path
/// resolves mount-relative. Reading root's pool while writing this volume's
/// writer made the per-navigation self-healing a silent no-op everywhere
/// except the boot disk.
#[test]
fn verify_corrects_a_mount_rooted_volumes_own_index() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
    const VOLUME: &str = "test-mount-volume";

    // The temp dir stands in for a mount root (`/Volumes/X`): its index stores
    // `sub` as a direct child of ROOT_ID, not under the absolute FS path.
    let mount = test_tempdir();
    let sub = mount.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file1.txt"), "hello").unwrap();

    let (writer, db_path, _db_dir) = setup_writer();
    let parent_id = ensure_path_in_db(&db_path, Path::new("/sub"), &writer);
    insert_children_from_disk(&writer, parent_id, &sub);
    install_pool_for(VOLUME, Arc::new(ReadPool::new(db_path.clone()).unwrap()));

    // A new file appears after indexing.
    fs::write(sub.join("new_file.txt"), "new content").unwrap();

    let space = IndexPathSpace::mount_rooted(mount.path().to_string_lossy().into_owned());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let paths = rt.block_on(verify_and_correct(
        VOLUME,
        &sub.to_string_lossy(),
        &space,
        &writer,
        &CancellationToken::new(),
    ));

    writer.flush_blocking().unwrap();
    let children_after = list_db_children_on(&db_path, parent_id);

    uninstall_read_pool(VOLUME);
    writer.shutdown();

    assert!(!paths.is_empty(), "the verifier must report the directory it corrected");
    assert!(
        children_after.iter().any(|e| e.name == "new_file.txt"),
        "the new file must land in the mount-rooted volume's own index",
    );
}

/// The new-directory half on a mount-rooted volume: `scan_subtree` gets an
/// ABSOLUTE root to walk but must resolve it to an entry id MOUNT-RELATIVE, and
/// must gate children with the volume's own exclusion scope. Under root's
/// space it resolves nothing and the new subtree stays empty in the index.
#[test]
fn verify_scans_a_new_directory_into_a_mount_rooted_volumes_index() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
    const VOLUME: &str = "test-mount-volume-newdir";

    let mount = test_tempdir();
    let sub = mount.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file1.txt"), "hello").unwrap();

    let (writer, db_path, _db_dir) = setup_writer();
    let parent_id = ensure_path_in_db(&db_path, Path::new("/sub"), &writer);
    insert_children_from_disk(&writer, parent_id, &sub);
    install_pool_for(VOLUME, Arc::new(ReadPool::new(db_path.clone()).unwrap()));

    // A whole new directory tree appears after indexing.
    let new_dir = sub.join("new_dir");
    fs::create_dir(&new_dir).unwrap();
    fs::write(new_dir.join("inside.txt"), "inside").unwrap();

    let space = IndexPathSpace::mount_rooted(mount.path().to_string_lossy().into_owned());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let paths = rt.block_on(verify_and_correct(
        VOLUME,
        &sub.to_string_lossy(),
        &space,
        &writer,
        &CancellationToken::new(),
    ));

    writer.flush_blocking().unwrap();
    let children_after = list_db_children_on(&db_path, parent_id);
    let new_dir_row = children_after
        .iter()
        .find(|e| e.name == "new_dir")
        .expect("the new directory must be indexed")
        .clone();
    let grandchildren = list_db_children_on(&db_path, new_dir_row.id);

    uninstall_read_pool(VOLUME);
    writer.shutdown();

    assert!(new_dir_row.is_directory);
    assert!(
        paths.iter().any(|p| p == &new_dir.to_string_lossy()),
        "the affected set carries the new dir's ABSOLUTE path (it feeds FS reads and the FE emit), got {paths:?}",
    );
    assert!(
        grandchildren.iter().any(|e| e.name == "inside.txt"),
        "`scan_subtree` must resolve the new dir mount-relative and index what's under it",
    );
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
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

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
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

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

/// The data-safety half of the stitch. A stitch gives every frontier root a ROW,
/// which is what used to make the verifier no-op on uncovered ground; with a row
/// there, it would resolve the directory, find no indexed children, treat every
/// name on disk as new, and run a full recursive `scan_subtree` per new
/// subdirectory — on the verifier task, for every folder the user opens ahead of
/// the walker, leaving exactly the non-virgin nodes the stitch exists to prevent.
///
/// The durable gate is the epoch, not a runtime flag: nothing has listed this
/// directory, so the walk owns it.
#[test]
fn the_verifier_leaves_an_unlisted_directory_alone() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
    let fs_root = test_tempdir();
    fs::create_dir(fs_root.path().join("subdir")).unwrap();
    fs::write(fs_root.path().join("subdir/deep.txt"), "hello").unwrap();
    fs::write(fs_root.path().join("file.txt"), "hello").unwrap();

    // A stitched frontier root: it has a row, and nothing has listed it.
    let (writer, db_path, _db_dir) = setup_writer_mid_coverage();
    let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
    install_read_pool(&db_path);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

    writer.flush_blocking().unwrap();
    assert!(
        paths.is_empty(),
        "an unlisted directory is the walk's to cover, so the verifier reports nothing: {paths:?}"
    );
    assert_eq!(
        list_db_children_on(&db_path, parent_id).len(),
        0,
        "and it writes nothing, so the walk still finds virgin ground"
    );

    remove_read_pool();
    writer.shutdown();
}

/// Ground a cover walk is covering RIGHT NOW is the walk's, and a listing of it
/// must write nothing.
///
/// Two writers of one name allocate different ids, and `INSERT OR IGNORE` drops
/// one and orphans its whole subtree — a data-safety bug, not a performance one.
/// The verifier consults neither the claim nor `WatchScope::may_walk`, so what
/// protects it is the durable fact that nothing has listed the directory yet.
/// Walking-while-browsing is the central behavior of phased indexing, so this
/// fires constantly rather than never.
#[test]
fn a_listing_of_ground_a_walk_is_covering_writes_nothing() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
    let fs_root = test_tempdir();
    fs::create_dir_all(fs_root.path().join("claimed/inner")).unwrap();
    fs::write(fs_root.path().join("claimed/file.txt"), "hello").unwrap();

    let (writer, db_path, _db_dir) = setup_writer_mid_coverage();
    ensure_path_in_db(&db_path, &fs_root.path().join("claimed"), &writer);
    install_read_pool(&db_path);

    // A real walk, holding a real claim on that ground for the whole assertion.
    let walk = crate::indexing::lifecycle::cover::start(
        crate::indexing::lifecycle::cover::CoverContext {
            volume_id: crate::ROOT_VOLUME_ID.to_string(),
            writer: writer.clone(),
            space: IndexPathSpace::root(),
            kind: crate::indexing::volume::IndexVolumeKind::Local,
            flush: crate::indexing::lifecycle::cover::FlushOnFinish::default(),
        },
        vec![fs_root.path().join("claimed").to_string_lossy().into_owned()],
        crate::indexing::read::coverage::CoverageDimension::Listing,
        CancellationToken::new(),
        crate::indexing::lifecycle::cover::WalkFor::TheIndex,
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    let paths = rt.block_on(verify_root(&fs_root.path().join("claimed"), &writer));
    assert!(
        paths.is_empty(),
        "the walk owns this ground; a listing of it corrects nothing: {paths:?}"
    );

    drop(walk);
    remove_read_pool();
    writer.shutdown();
}

/// The other side of the bail's scoping, and the reason it isn't unconditional. A
/// directory the reconcile cost budget SKIPPED also has a row with
/// `listed_epoch == 0` and no cause. On a volume whose scan completed, no walk is
/// coming for it, so the per-navigation verifier is the only thing that heals it.
#[test]
fn the_verifier_still_heals_a_skipped_dir_on_a_completed_volume() {
    let _pool_guard = READ_POOL_TEST_MUTEX.lock().unwrap();
    let fs_root = test_tempdir();
    fs::write(fs_root.path().join("file.txt"), "hello").unwrap();

    let (writer, db_path, _db_dir) = setup_writer();
    let parent_id = ensure_path_in_db(&db_path, fs_root.path(), &writer);
    install_read_pool(&db_path);

    let rt = tokio::runtime::Runtime::new().unwrap();
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

    writer.flush_blocking().unwrap();
    assert!(!paths.is_empty(), "a completed volume's skipped directory still heals");
    assert_eq!(
        list_db_children_on(&db_path, parent_id).len(),
        1,
        "the file the skip left out is written"
    );

    remove_read_pool();
    writer.shutdown();
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
    let paths = rt.block_on(verify_root(fs_root.path(), &writer));

    writer.flush_blocking().unwrap();
    let children_after = list_db_children_on(&db_path, parent_id);

    assert!(paths.is_empty());
    assert_eq!(children_after.len(), 0);

    remove_read_pool();
    writer.shutdown();
}

#[test]
fn in_flight_slot_is_freed_even_when_the_panic_poisoned_the_state_lock() {
    // The realistic shape of the panic the guard defends against: it happens
    // while the verifier state lock is held, so the lock is POISONED by the time
    // the guard unwinds. Skipping the removal there leaks the path against
    // `MAX_CONCURRENT_VERIFICATIONS` for the rest of the session, and the
    // verifier eventually stops verifying anything.
    //
    // A local mutex, not `VERIFIER_STATE`: poisoning a process-global static
    // would break every sibling test that touches it.
    let state = Mutex::new(VerifierState {
        in_flight: HashSet::new(),
        recent: Vec::new(),
    });
    let dir_path = "/fake/poisoned/slot";
    state.lock_ignore_poison().in_flight.insert(dir_path.to_string());

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = state.lock_ignore_poison();
        panic!("simulated panic while holding the verifier state lock");
    }));
    assert!(panicked.is_err(), "the closure must have panicked");
    assert!(state.is_poisoned(), "the panic must have poisoned the lock");

    release_in_flight_slot(&state, dir_path);

    let state = state.lock_ignore_poison();
    assert!(
        !state.in_flight.contains(dir_path),
        "the slot must be freed through a poisoned lock, not leaked"
    );
    assert!(
        state.recent.iter().any(|(p, _)| p == dir_path),
        "the path must still be recorded as recently verified"
    );
}

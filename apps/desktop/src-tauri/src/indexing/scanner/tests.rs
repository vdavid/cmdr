//! Tests for the drive scanner: path-component handling, exclusion gating, the
//! canonicalization-alias check, and end-to-end scan behavior. Extracted verbatim
//! from the former `scanner.rs` `mod tests`; pure code movement.
use super::*;
use crate::indexing::store::{self, IndexStore, ROOT_ID, ScanContext};
use crate::indexing::writer::IndexWriter;
use std::fs;

/// Create a temp dir for volume-scan tests. On Linux, `/tmp/` is in the exclusion list,
/// so we use the current directory to avoid false rejections.
fn scan_test_tempdir() -> tempfile::TempDir {
    // Create in CWD instead of /tmp/ to avoid:
    // - Linux: /tmp/ is in EXCLUDED_PREFIXES
    // - macOS: /tmp is a symlink to /private/tmp, causing path mismatches with normalize_path() which
    //   resolves /tmp → /private/tmp
    tempfile::Builder::new()
        .prefix("cmdr-scan-test-")
        .tempdir_in(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .expect("failed to create temp dir in cwd")
}

/// Create a temp directory with a known file tree and return the root path.
fn create_test_tree(dir: &Path) {
    let sub = dir.join("subdir");
    fs::create_dir_all(&sub).unwrap();
    fs::write(dir.join("file1.txt"), "hello world").unwrap();
    fs::write(dir.join("file2.txt"), "more content here").unwrap();
    fs::write(sub.join("nested.txt"), "nested file").unwrap();
    fs::create_dir_all(sub.join("deep")).unwrap();
    fs::write(sub.join("deep").join("leaf.txt"), "leaf").unwrap();
}

fn setup_writer() -> (IndexWriter, PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = dir.path().join("test-index.db");
    let _store = IndexStore::open(&db_path).expect("failed to open store");
    let writer = IndexWriter::spawn(&db_path, None).expect("failed to spawn writer");
    (writer, db_path, dir)
}

/// Insert the full parent directory chain for a path into the DB so that
/// `ScanContext::new` can resolve the subtree root for subtree scans.
/// Also syncs the writer's shared `next_id` counter with the DB.
fn ensure_path_in_db(db_path: &Path, path: &Path, writer: &IndexWriter) {
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
    writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
}

#[test]
#[cfg(target_os = "macos")]
fn should_exclude_system_volumes() {
    assert!(should_exclude("/System/Volumes/Data/"));
    assert!(should_exclude("/System/Volumes/Data/Users/foo"));
    assert!(should_exclude("/System/Volumes/VM/"));
    assert!(should_exclude("/System/Volumes/Preboot/"));
    assert!(should_exclude("/dev"));
    assert!(should_exclude("/dev/null"));
    assert!(should_exclude("/proc"));
    assert!(should_exclude("/private/var/"));
    assert!(should_exclude("/private/var/folders/xx"));
}

#[test]
#[cfg(target_os = "macos")]
fn should_exclude_system_except_firmlinked() {
    // Generic /System/ paths should be excluded
    assert!(should_exclude("/System/foo"));
    assert!(should_exclude("/System/Library/Frameworks"));
    assert!(should_exclude("/System"));

    // Firmlinked /System/ paths should NOT be excluded
    assert!(!should_exclude("/System/Library/Caches"));
    assert!(!should_exclude("/System/Library/Caches/com.apple.something"));
    assert!(!should_exclude("/System/Library/Assets"));
    assert!(!should_exclude("/System/Library/Speech"));
    assert!(!should_exclude("/System/Library/Speech/Voices"));
}

#[test]
#[cfg(target_os = "macos")]
fn should_not_exclude_normal_paths() {
    assert!(!should_exclude("/Users/foo"));
    assert!(!should_exclude("/Users/foo/Documents"));
    assert!(!should_exclude("/Applications"));
    assert!(!should_exclude("/tmp"));
    assert!(!should_exclude("/opt/homebrew"));
}

#[test]
fn canonicalization_aliases_are_skipped() {
    // A real path normalizes to itself, so it's never an alias (every platform).
    assert!(!is_canonicalization_alias(
        "/Users/foo",
        &firmlinks::normalize_path("/Users/foo")
    ));

    // macOS: the well-known /private root symlinks (/tmp, /var, /etc) normalize to
    // /private/..., so they're aliases of the real dir and the scanner skips them.
    #[cfg(target_os = "macos")]
    {
        for alias in ["/tmp", "/var", "/etc"] {
            assert!(
                is_canonicalization_alias(alias, &firmlinks::normalize_path(alias)),
                "{alias} should be a canonicalization alias"
            );
        }
        // The real target owns the canonical slot, so it is NOT an alias.
        assert!(!is_canonicalization_alias(
            "/private/tmp",
            &firmlinks::normalize_path("/private/tmp")
        ));
    }
}

#[test]
#[cfg(target_os = "linux")]
fn should_exclude_linux_virtual_filesystems() {
    assert!(should_exclude("/dev"));
    assert!(should_exclude("/dev/null"));
    assert!(should_exclude("/proc"));
    assert!(should_exclude("/proc/1/status"));
    assert!(should_exclude("/sys"));
    assert!(should_exclude("/sys/class/block"));
    assert!(should_exclude("/run"));
    assert!(should_exclude("/run/user/1000"));
    assert!(should_exclude("/snap"));
    assert!(should_exclude("/mnt"));
    assert!(should_exclude("/media"));
    assert!(should_exclude("/boot"));
    assert!(should_exclude("/tmp"));
}

#[test]
#[cfg(target_os = "linux")]
fn should_not_exclude_linux_normal_paths() {
    assert!(!should_exclude("/home/user"));
    assert!(!should_exclude("/home/user/Documents"));
    assert!(!should_exclude("/usr/local/bin"));
    assert!(!should_exclude("/opt/app"));
    assert!(!should_exclude("/etc/config"));
    assert!(!should_exclude("/var/lib"));
}

// E2E scan restriction is tested end-to-end by the indexing E2E tests
// (indexing.spec.ts) which verify that get_dir_stats returns data for
// fixture paths under /tmp on Linux Docker. A unit test here would require
// mutating the env (unsafe set_var) and nextest (OnceLock is per-process).

#[test]
fn scan_temp_directory_tree() {
    let scan_root = scan_test_tempdir();
    create_test_tree(scan_root.path());

    let (writer, db_path, _db_dir) = setup_writer();

    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };

    let (handle, join_handle) = scan_volume(config, &writer).unwrap();
    let summary = join_handle.join().expect("scan thread panicked").unwrap();

    assert!(!summary.was_cancelled);
    // We created: subdir/, file1.txt, file2.txt, subdir/nested.txt, subdir/deep/, subdir/deep/leaf.txt
    assert_eq!(summary.total_entries, 6, "expected 6 entries (2 dirs + 4 files)");
    assert_eq!(summary.total_dirs, 2, "expected 2 directories");
    assert!(summary.duration_ms < 10_000, "scan should complete quickly");

    // Verify progress matches summary
    let snap = handle.progress.snapshot();
    assert_eq!(snap.entries_scanned, summary.total_entries);
    assert_eq!(snap.dirs_found, summary.total_dirs);
    assert_eq!(snap.bytes_scanned, summary.total_physical_bytes);

    // Wait for writer to process all messages + aggregation
    writer.flush_blocking().unwrap();
    writer.shutdown();

    // Verify entries are in the DB using integer-keyed API.
    // The scanner maps the scan root to ROOT_ID, so children are under ROOT_ID.
    let store = IndexStore::open(&db_path).unwrap();
    let children = store.list_children(ROOT_ID).unwrap();
    assert_eq!(
        children.len(),
        3,
        "root should have 3 children: subdir, file1.txt, file2.txt"
    );

    // Verify a file has a non-zero size
    let file1 = children.iter().find(|e| e.name == "file1.txt").unwrap();
    assert!(!file1.is_directory);
    assert!(
        file1.logical_size.unwrap_or(0) > 0,
        "file should have nonzero logical size"
    );
}

/// After a clean local scan, EVERY directory (root + every subdir, all of
/// which jwalk read successfully) has `listed_epoch == current_epoch`. This
/// is the ordering-invariant anchor: a `MarkDirsListed` queued *behind* the
/// final `ComputeAllAggregates` would leave a dir at epoch 0, so this test
/// would catch the "renders incomplete/stale forever" race.
#[test]
fn clean_scan_stamps_every_listed_dir_with_current_epoch() {
    let scan_root = scan_test_tempdir();
    create_test_tree(scan_root.path());

    let (writer, db_path, _db_dir) = setup_writer();

    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };

    let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
    let summary = join_handle.join().expect("scan thread panicked").unwrap();
    assert!(!summary.was_cancelled);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let epoch = IndexStore::read_current_epoch(&conn).unwrap();
    assert_eq!(epoch, 1, "first scan seeds + stamps epoch 1");

    // Every directory row must carry the current epoch. Read all dir rows
    // directly (PK + listed_epoch) and assert none stayed at 0.
    let mut stmt = conn
        .prepare("SELECT id, listed_epoch FROM entries WHERE is_directory = 1")
        .unwrap();
    let rows: Vec<(i64, u64)> = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, u64>(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(rows.len() >= 3, "root + subdir + deep are all directories");
    for (id, listed_epoch) in rows {
        assert_eq!(
            listed_epoch, epoch,
            "dir id={id} should be stamped with the current epoch (mark must precede the final aggregate)",
        );
    }
}

#[test]
fn scan_subtree_only() {
    let scan_root = scan_test_tempdir();
    create_test_tree(scan_root.path());

    let (writer, db_path, _db_dir) = setup_writer();
    let cancelled = AtomicBool::new(false);

    let subtree_root = scan_root.path().join("subdir");

    // Pre-insert the subtree root's parent chain so ScanContext can resolve it
    ensure_path_in_db(&db_path, &subtree_root, &writer);

    let summary = scan_subtree(&subtree_root, &writer, &cancelled).unwrap();

    assert!(!summary.was_cancelled);
    // subdir contains: nested.txt, deep/, deep/leaf.txt
    assert_eq!(summary.total_entries, 3, "expected 3 entries under subdir");
    assert_eq!(summary.total_dirs, 1, "expected 1 directory (deep/)");

    // Wait for writer to process
    writer.flush_blocking().unwrap();
    writer.shutdown();

    // The subtree scan resolves the actual entry ID for the subtree root.
    // Children should be listed under that ID, not ROOT_ID.
    let store = IndexStore::open(&db_path).unwrap();
    let conn = store.read_conn();
    let subtree_id = store::resolve_path(conn, &subtree_root.to_string_lossy())
        .unwrap()
        .expect("subtree root should be in DB");
    let children = store.list_children(subtree_id).unwrap();
    assert_eq!(children.len(), 2, "subdir should have 2 children: nested.txt, deep");
}

#[test]
fn scan_cancellation() {
    let scan_root = scan_test_tempdir();
    create_test_tree(scan_root.path());

    let (writer, _db_path, _db_dir) = setup_writer();

    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 1, // Tiny batch so we check cancellation frequently
        num_threads: 1,
    };

    let (handle, join_handle) = scan_volume(config, &writer).unwrap();
    // Cancel immediately
    handle.cancel();

    let summary = join_handle.join().expect("scan thread panicked").unwrap();
    assert!(summary.was_cancelled);

    writer.shutdown();
}

#[test]
fn scan_empty_directory() {
    let scan_root = scan_test_tempdir();
    let (writer, _db_path, _db_dir) = setup_writer();

    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };

    let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
    let summary = join_handle.join().expect("scan thread panicked").unwrap();

    assert!(!summary.was_cancelled);
    assert_eq!(summary.total_entries, 0);
    assert_eq!(summary.total_dirs, 0);

    writer.shutdown();
}

#[test]
#[cfg(unix)]
fn physical_size_is_captured() {
    let scan_root = scan_test_tempdir();
    // Write a file with known content
    let content = vec![0u8; 8192]; // 8KB, should allocate at least one block
    fs::write(scan_root.path().join("sized.bin"), &content).unwrap();

    let (writer, db_path, _db_dir) = setup_writer();

    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };

    let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
    let _summary = join_handle.join().expect("scan thread panicked").unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let children = store.list_children(ROOT_ID).unwrap();
    let sized = children.iter().find(|e| e.name == "sized.bin").unwrap();

    // Physical size should be >= logical size (and a multiple of 512)
    let phys = sized.physical_size.unwrap();
    assert!(phys >= 8192, "physical size ({phys}) should be >= logical size (8192)");
    assert_eq!(phys % 512, 0, "physical size should be a multiple of 512");

    // Logical size should be exactly 8192
    let logical = sized.logical_size.unwrap();
    assert_eq!(logical, 8192, "logical size should be exactly 8192");
}

#[test]
fn scan_handles_symlinks() {
    let scan_root = scan_test_tempdir();
    fs::write(scan_root.path().join("real.txt"), "real content").unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(scan_root.path().join("real.txt"), scan_root.path().join("link.txt")).unwrap();
    }

    let (writer, db_path, _db_dir) = setup_writer();

    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };

    let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
    let _summary = join_handle.join().expect("scan thread panicked").unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let children = store.list_children(ROOT_ID).unwrap();

    #[cfg(unix)]
    {
        assert_eq!(children.len(), 2);
        let link = children.iter().find(|e| e.name == "link.txt").unwrap();
        assert!(link.is_symlink, "symlink should be marked as symlink");
        assert!(!link.is_directory);
    }
}

#[test]
#[cfg(unix)]
fn scan_sets_recursive_has_symlinks_for_symlink_only_dir() {
    // A directory containing only symlinks should report 0 bytes (matching
    // `du`/Finder behavior) AND have recursive_has_symlinks = true so the UI
    // can surface the "size omits symlinked content" hint.
    let scan_root = scan_test_tempdir();
    let links_dir = scan_root.path().join("links");
    fs::create_dir(&links_dir).unwrap();
    // Two symlinks pointing somewhere; targets don't have to exist for this test
    std::os::unix::fs::symlink("/tmp/does-not-matter-1", links_dir.join("a")).unwrap();
    std::os::unix::fs::symlink("/tmp/does-not-matter-2", links_dir.join("b")).unwrap();
    // A neighboring dir with no symlinks
    let plain = scan_root.path().join("plain");
    fs::create_dir(&plain).unwrap();
    fs::write(plain.join("hi.txt"), "hello").unwrap();

    let (writer, db_path, _db_dir) = setup_writer();
    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };
    let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
    let _summary = join_handle.join().expect("scan thread panicked").unwrap();

    // Trigger aggregation, then flush
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();
    writer.shutdown();

    // The scan maps the scan root to ROOT_ID, so children are under ROOT_ID.
    let store = IndexStore::open(&db_path).unwrap();
    let conn = store.read_conn();
    let links_id = IndexStore::resolve_component(conn, ROOT_ID, "links")
        .unwrap()
        .expect("links dir indexed");
    let plain_id = IndexStore::resolve_component(conn, ROOT_ID, "plain")
        .unwrap()
        .expect("plain dir indexed");

    let links_stats = IndexStore::get_dir_stats_by_id(conn, links_id).unwrap().unwrap();
    assert_eq!(
        links_stats.recursive_logical_size, 0,
        "symlink-only folder reports 0 bytes"
    );
    assert!(
        links_stats.recursive_has_symlinks,
        "symlink-only folder must surface the hint"
    );

    let plain_stats = IndexStore::get_dir_stats_by_id(conn, plain_id).unwrap().unwrap();
    assert!(
        !plain_stats.recursive_has_symlinks,
        "neighbor without symlinks should stay false"
    );
}

#[test]
fn default_exclusions_populated() {
    let exclusions = default_exclusions();
    assert!(!exclusions.is_empty());
    #[cfg(target_os = "macos")]
    assert!(exclusions.iter().any(|e| e.contains("System/Volumes/Data")));
    #[cfg(target_os = "linux")]
    assert!(exclusions.iter().any(|e| e.contains("/proc")));
}

#[test]
fn scan_assigns_integer_ids() {
    // Verify that the scanner correctly assigns integer IDs and parent IDs
    let scan_root = scan_test_tempdir();
    create_test_tree(scan_root.path());

    let (writer, db_path, _db_dir) = setup_writer();

    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };

    let (_handle, join_handle) = scan_volume(config, &writer).unwrap();
    let _summary = join_handle.join().expect("scan thread panicked").unwrap();

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();

    // All top-level entries should have parent_id = ROOT_ID
    let top_children = store.list_children(ROOT_ID).unwrap();
    assert_eq!(top_children.len(), 3); // subdir, file1.txt, file2.txt

    for child in &top_children {
        assert_eq!(child.parent_id, ROOT_ID);
        assert!(child.id > ROOT_ID, "all IDs should be > ROOT_ID");
    }

    // Find the subdir entry and check its children
    let subdir = top_children.iter().find(|e| e.name == "subdir").unwrap();
    assert!(subdir.is_directory);
    let subdir_children = store.list_children(subdir.id).unwrap();
    assert_eq!(subdir_children.len(), 2); // nested.txt, deep

    for child in &subdir_children {
        assert_eq!(child.parent_id, subdir.id, "children should reference parent's ID");
    }

    // Find the deep directory and check its children
    let deep = subdir_children.iter().find(|e| e.name == "deep").unwrap();
    assert!(deep.is_directory);
    let deep_children = store.list_children(deep.id).unwrap();
    assert_eq!(deep_children.len(), 1); // leaf.txt
    assert_eq!(deep_children[0].name, "leaf.txt");
    assert_eq!(deep_children[0].parent_id, deep.id);
}

#[test]
fn scan_context_id_allocation() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicI64;

    // Verify ScanContext properly assigns monotonically increasing IDs
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("test-ctx.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Seed the counter from the DB (root sentinel is id=1, so next_id=2)
    let next_id = Arc::new(AtomicI64::new(IndexStore::get_next_id(&conn).unwrap()));

    let root_path = Path::new("/test/root");
    let mut ctx = ScanContext::new(&conn, root_path, true, next_id).unwrap();

    let id1 = ctx.alloc_id();
    assert!(id1 >= 2);
    let id2 = ctx.alloc_id();
    let id3 = ctx.alloc_id();
    assert_eq!(id2, id1 + 1);
    assert_eq!(id3, id2 + 1);

    // Volume root → maps to ROOT_ID
    assert_eq!(ctx.lookup_parent(root_path), Some(ROOT_ID));

    // Register a directory and look it up
    let dir_path = PathBuf::from("/test/root/mydir");
    ctx.register_dir(dir_path.clone(), id1);
    assert_eq!(ctx.lookup_parent(&dir_path), Some(id1));

    // Unknown path returns None
    assert_eq!(ctx.lookup_parent(Path::new("/unknown")), None);
}

#[test]
fn scan_context_subtree_resolves_actual_id() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicI64;

    // When the subtree root exists in the DB, ScanContext should use its actual ID
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("test-ctx-subtree.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Insert a directory chain: ROOT → Volumes → "NO NAME"
    let volumes_id =
        IndexStore::insert_entry_v2(&conn, ROOT_ID, "Volumes", true, false, None, None, None, None).unwrap();
    let noname_id =
        IndexStore::insert_entry_v2(&conn, volumes_id, "NO NAME", true, false, None, None, None, None).unwrap();
    assert_ne!(noname_id, ROOT_ID);

    // Seed counter from DB after inserts
    let next_id = Arc::new(AtomicI64::new(IndexStore::get_next_id(&conn).unwrap()));

    // Create ScanContext for the subtree root
    let subtree_root = Path::new("/Volumes/NO NAME");
    let ctx = ScanContext::new(&conn, subtree_root, false, next_id).unwrap();

    // Should resolve to the actual entry ID, NOT ROOT_ID
    assert_eq!(ctx.lookup_parent(subtree_root), Some(noname_id));
}

/// Sum every stored row's `physical_size` (NULLs count as 0), matching how the
/// aggregator treats per-entry physical bytes.
fn sum_stored_physical_bytes(db_path: &Path) -> u64 {
    let conn = IndexStore::open_read_connection(db_path).unwrap();
    conn.query_row("SELECT COALESCE(SUM(physical_size), 0) FROM entries", [], |row| {
        row.get::<_, i64>(0)
    })
    .unwrap() as u64
}

/// Build a tree with BOTH plain single-link files AND a hardlink pair. The
/// single-link files are what catch a "bytes increment placed inside the
/// dedup arm" bug: that arm fires only for `nlink > 1`, so single-link files
/// would contribute nothing and near-zero the counter.
#[cfg(unix)]
fn create_tree_with_hardlinks(dir: &Path) {
    // Plain single-link files (the majority).
    fs::write(dir.join("plain1.bin"), vec![0u8; 4096]).unwrap();
    fs::write(dir.join("plain2.bin"), vec![0u8; 12288]).unwrap();
    let sub = dir.join("sub");
    fs::create_dir_all(&sub).unwrap();
    fs::write(sub.join("plain3.bin"), vec![0u8; 8192]).unwrap();

    // A hardlink pair: two directory entries, one inode. Only the first link's
    // size should be counted; the second resolves to None.
    let target = dir.join("linked.bin");
    fs::write(&target, vec![0u8; 16384]).unwrap();
    fs::hard_link(&target, dir.join("linked-alias.bin")).unwrap();
}

#[test]
#[cfg(unix)]
fn bytes_scanned_matches_stored_physical_sum_with_hardlinks() {
    let scan_root = scan_test_tempdir();
    create_tree_with_hardlinks(scan_root.path());

    let (writer, db_path, _db_dir) = setup_writer();
    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };

    let (handle, join_handle) = scan_volume(config, &writer).unwrap();
    let summary = join_handle.join().expect("scan thread panicked").unwrap();
    assert!(!summary.was_cancelled);

    writer.flush_blocking().unwrap();
    writer.shutdown();

    let counter_total = handle.progress.snapshot().bytes_scanned;
    let stored_total = sum_stored_physical_bytes(&db_path);

    // The live counter follows the exact post-dedup rules of the stored rows.
    assert_eq!(
        counter_total, stored_total,
        "bytes_scanned counter must equal the sum of stored physical sizes"
    );
    // Sanity: the plain single-link files alone exceed any single hardlink, so a
    // counter that only ran inside the dedup arm would fall well below this.
    assert!(
        counter_total >= 4096 + 12288 + 8192,
        "counter must include the single-link files, not just the hardlink"
    );
}

#[test]
#[cfg(unix)]
fn scan_summary_total_physical_bytes_equals_final_counter() {
    let scan_root = scan_test_tempdir();
    create_tree_with_hardlinks(scan_root.path());

    let (writer, _db_path, _db_dir) = setup_writer();
    let config = ScanConfig {
        root: scan_root.path().to_path_buf(),
        batch_size: 100,
        num_threads: 1,
    };

    let (handle, join_handle) = scan_volume(config, &writer).unwrap();
    let summary = join_handle.join().expect("scan thread panicked").unwrap();
    writer.shutdown();

    assert_eq!(
        summary.total_physical_bytes,
        handle.progress.snapshot().bytes_scanned,
        "summary.total_physical_bytes must equal the final counter value"
    );
    assert!(summary.total_physical_bytes > 0, "scan should sum some physical bytes");
}

#[test]
fn timed_out_dir_is_not_marked_listed() {
    use crate::indexing::scanner::walker::{RawDirEntry, RawFileType, ReadDirFn};
    use std::collections::HashMap;

    // Mock tree under "/root": "slow" (dir, its read hangs) and "ok" (dir, has a file).
    let root = PathBuf::from("/root");
    let mut dirs: HashMap<PathBuf, Vec<(&str, RawFileType)>> = HashMap::new();
    dirs.insert(root.clone(), vec![("slow", RawFileType::Dir), ("ok", RawFileType::Dir)]);
    dirs.insert(root.join("slow"), vec![("hidden.txt", RawFileType::File)]);
    dirs.insert(root.join("ok"), vec![("seen.txt", RawFileType::File)]);
    let slow = root.join("slow");
    let dirs = Arc::new(dirs);
    let reader: ReadDirFn = {
        let dirs = Arc::clone(&dirs);
        let slow = slow.clone();
        Arc::new(move |p: &Path| {
            if p == slow {
                std::thread::sleep(Duration::from_secs(2)); // hang past the timeout
            }
            match dirs.get(p) {
                Some(children) => Ok(children
                    .iter()
                    .map(|(n, t)| RawDirEntry {
                        path: p.join(n),
                        file_type: *t,
                        stat: None,
                    })
                    .collect()),
                None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no mock dir")),
            }
        })
    };

    let (writer, db_path, _db_dir) = setup_writer();
    let progress = Arc::new(ScanProgress::new());
    let cancelled = AtomicBool::new(false);

    let start = Instant::now();
    let (summary, listed_ids, epoch) = run_scan(
        &root,
        &cancelled,
        &progress,
        &writer,
        100,
        4,
        true,
        reader,
        Duration::from_millis(50), // short timeout so the hang is abandoned fast
    )
    .expect("run_scan");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "must abandon the hang, not wait it out"
    );
    assert!(!summary.was_cancelled);

    // Emit the marks exactly as scan_volume does, then flush.
    send_marks(&listed_ids, epoch, &writer);
    writer.send(WriteMessage::ComputeAllAggregates).unwrap();
    writer.flush_blocking().unwrap();
    writer.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).unwrap();
    let epoch_now = IndexStore::read_current_epoch(&conn).unwrap();

    // Resolve the two child dirs' ids under ROOT_ID.
    let slow_id = IndexStore::resolve_component(&conn, ROOT_ID, "slow")
        .unwrap()
        .expect("slow dir row exists (its parent listed it)");
    let ok_id = IndexStore::resolve_component(&conn, ROOT_ID, "ok")
        .unwrap()
        .expect("ok dir row exists");

    let listed_epoch = |id: i64| -> u64 {
        conn.query_row("SELECT listed_epoch FROM entries WHERE id = ?1", [id], |r| {
            r.get::<_, u64>(0)
        })
        .unwrap()
    };

    // The hung dir is inserted but NOT marked (honest unknown); its subtree is absent.
    assert_eq!(listed_epoch(slow_id), 0, "timed-out dir must stay listed_epoch = 0");
    assert!(
        IndexStore::resolve_component(&conn, slow_id, "hidden.txt")
            .unwrap()
            .is_none(),
        "hung dir's children must be absent",
    );
    // The healthy sibling and root ARE marked at the current epoch.
    assert_eq!(listed_epoch(ok_id), epoch_now, "healthy dir marked at current epoch");
    assert_eq!(listed_epoch(ROOT_ID), epoch_now, "root marked at current epoch");
}

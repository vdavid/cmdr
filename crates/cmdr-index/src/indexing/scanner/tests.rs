//! Tests for the drive scanner: path-component handling, exclusion gating, the
//! canonicalization-alias check, and end-to-end scan behavior. Extracted verbatim
//! from the former `scanner.rs` `mod tests`; pure code movement.
use super::test_fixtures::{create_test_tree, ensure_path_in_db, scan_test_tempdir, setup_writer};
use super::*;
use crate::indexing::store::{self, IndexStore, ROOT_ID, ScanContext};
use cmdr_fs::firmlinks;
use std::fs;

#[test]
#[cfg(target_os = "macos")]
fn should_exclude_system_volumes() {
    let boot_disk = ExclusionScope::boot_disk();
    assert!(should_exclude("/System/Volumes/Data/", &boot_disk));
    assert!(should_exclude("/System/Volumes/Data/Users/foo", &boot_disk));
    assert!(should_exclude("/System/Volumes/VM/", &boot_disk));
    assert!(should_exclude("/System/Volumes/Preboot/", &boot_disk));
    assert!(should_exclude("/dev", &boot_disk));
    assert!(should_exclude("/dev/null", &boot_disk));
    assert!(should_exclude("/proc", &boot_disk));
    assert!(should_exclude("/private/var/", &boot_disk));
    assert!(should_exclude("/private/var/folders/xx", &boot_disk));
}

#[test]
#[cfg(target_os = "macos")]
fn should_exclude_system_except_firmlinked() {
    let boot_disk = ExclusionScope::boot_disk();
    // Generic /System/ paths should be excluded
    assert!(should_exclude("/System/foo", &boot_disk));
    assert!(should_exclude("/System/Library/Frameworks", &boot_disk));
    assert!(should_exclude("/System", &boot_disk));

    // Firmlinked /System/ paths should NOT be excluded
    assert!(!should_exclude("/System/Library/Caches", &boot_disk));
    assert!(!should_exclude(
        "/System/Library/Caches/com.apple.something",
        &boot_disk
    ));
    assert!(!should_exclude("/System/Library/Assets", &boot_disk));
    assert!(!should_exclude("/System/Library/Speech", &boot_disk));
    assert!(!should_exclude("/System/Library/Speech/Voices", &boot_disk));
}

#[test]
#[cfg(target_os = "macos")]
fn should_not_exclude_normal_paths() {
    let boot_disk = ExclusionScope::boot_disk();
    assert!(!should_exclude("/Users/foo", &boot_disk));
    assert!(!should_exclude("/Users/foo/Documents", &boot_disk));
    assert!(!should_exclude("/Applications", &boot_disk));
    assert!(!should_exclude("/tmp", &boot_disk));
    assert!(!should_exclude("/opt/homebrew", &boot_disk));
}

/// A mount-rooted scan (an external drive rooted at `/Volumes/X`, SMB, or MTP)
/// must index everything beneath its mount: the boot-disk `/Volumes/`,
/// `/System/...`, and `/private/var/` prefixes must NOT exclude its children,
/// or the walk emits zero rows and the completion path writes a false Fresh.
#[test]
#[cfg(target_os = "macos")]
fn mount_rooted_scan_indexes_under_volumes() {
    let no_name = ExclusionScope::mount_rooted("/Volumes/NO NAME");
    let backup = ExclusionScope::mount_rooted("/Volumes/My Backup");
    let usb = ExclusionScope::mount_rooted("/Volumes/USB");
    // The exact false-complete case: a child of the external drive's mount root.
    assert!(!should_exclude("/Volumes/NO NAME/photos", &no_name));
    assert!(!should_exclude("/Volumes/NO NAME/photos/img.jpg", &no_name));
    assert!(!should_exclude("/Volumes/My Backup/Documents/report.pdf", &backup));
    // Paths that only look system-ish because the mount happens to be named so.
    assert!(!should_exclude("/Volumes/USB/private/var/data", &usb));
    assert!(!should_exclude("/Volumes/USB/System/thing", &usb));
}

/// The boot-disk scan still keeps off mounted volumes and system trees.
#[test]
#[cfg(target_os = "macos")]
fn boot_disk_scan_still_excludes_volumes_and_system() {
    let boot_disk = ExclusionScope::boot_disk();
    assert!(should_exclude("/Volumes/NO NAME", &boot_disk));
    assert!(should_exclude("/Volumes/NO NAME/photos", &boot_disk));
    assert!(should_exclude("/System/Volumes/Data/Users/foo", &boot_disk));
    assert!(should_exclude("/private/var/folders/xx", &boot_disk));
}

/// Per-volume junk (`.Spotlight-V100`, `.fseventsd`, `.Trashes`,
/// `.TemporaryItems`) is skipped under BOTH scopes — junk is junk on any volume.
/// On the boot disk these once lived only as root-anchored prefixes; on a
/// mount-rooted drive they sit under `/Volumes/X`, so they're matched by basename.
#[test]
#[cfg(target_os = "macos")]
fn junk_basenames_excluded_under_both_scopes() {
    let boot_disk = ExclusionScope::boot_disk();
    let mount_rooted = ExclusionScope::mount_rooted("/Volumes/USB");
    for junk in [".Spotlight-V100", ".fseventsd", ".Trashes", ".TemporaryItems"] {
        assert!(should_exclude(&format!("/{junk}"), &boot_disk), "{junk} on boot root");
        assert!(
            should_exclude(&format!("/Users/foo/{junk}"), &boot_disk),
            "{junk} deep on boot"
        );
        assert!(
            should_exclude(&format!("/Volumes/USB/{junk}"), &mount_rooted),
            "{junk} on mount root"
        );
        assert!(
            should_exclude(&format!("/Volumes/USB/sub/{junk}"), &mount_rooted),
            "{junk} deep on mount"
        );
    }
    // A user folder that merely contains a junk name as a substring is NOT junk.
    assert!(!should_exclude("/Volumes/USB/My .Trashes notes", &mount_rooted));
    assert!(!should_exclude("/Volumes/USB/Spotlight-V100", &mount_rooted));
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
    let boot_disk = ExclusionScope::boot_disk();
    assert!(should_exclude("/dev", &boot_disk));
    assert!(should_exclude("/dev/null", &boot_disk));
    assert!(should_exclude("/proc", &boot_disk));
    assert!(should_exclude("/proc/1/status", &boot_disk));
    assert!(should_exclude("/sys", &boot_disk));
    assert!(should_exclude("/sys/class/block", &boot_disk));
    assert!(should_exclude("/run", &boot_disk));
    assert!(should_exclude("/run/user/1000", &boot_disk));
    assert!(should_exclude("/snap", &boot_disk));
    assert!(should_exclude("/mnt", &boot_disk));
    assert!(should_exclude("/media", &boot_disk));
    assert!(should_exclude("/boot", &boot_disk));
    assert!(should_exclude("/tmp", &boot_disk));
    // A mount-rooted scan under one of these mounts indexes its own subtree.
    assert!(!should_exclude(
        "/mnt/usb/data",
        &ExclusionScope::mount_rooted("/mnt/usb")
    ));
    assert!(!should_exclude(
        "/media/card/photos",
        &ExclusionScope::mount_rooted("/media/card")
    ));
}

#[test]
#[cfg(target_os = "linux")]
fn should_not_exclude_linux_normal_paths() {
    let boot_disk = ExclusionScope::boot_disk();
    assert!(!should_exclude("/home/user", &boot_disk));
    assert!(!should_exclude("/home/user/Documents", &boot_disk));
    assert!(!should_exclude("/usr/local/bin", &boot_disk));
    assert!(!should_exclude("/opt/app", &boot_disk));
    assert!(!should_exclude("/etc/config", &boot_disk));
    assert!(!should_exclude("/var/lib", &boot_disk));
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
        ..ScanConfig::default()
    };

    let (handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
    let summary = join_handle.join().expect("scan thread panicked").unwrap();

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
/// which the guarded walker read successfully) has `listed_epoch == current_epoch`. This
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
        ..ScanConfig::default()
    };

    let (_handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
    join_handle.join().expect("scan thread panicked").unwrap();

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
    let cancelled = CancellationToken::new();

    let subtree_root = scan_root.path().join("subdir");

    // Pre-insert the subtree root's parent chain so ScanContext can resolve it
    ensure_path_in_db(&db_path, &subtree_root, &writer);

    let summary = scan_subtree(&subtree_root, &IndexPathSpace::root(), &writer, &cancelled).unwrap();

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

/// Data safety on the cancel path: `scan_subtree` deletes the subtree's existing
/// descendants BEFORE walking, so bailing out on a cancel without the aggregate
/// would leave the ancestors claiming sizes for rows that no longer exist. The
/// typed `Cancelled` must therefore arrive AFTER the repair, not instead of it.
#[test]
fn a_cancelled_subtree_scan_still_repairs_its_ancestors() {
    let scan_root = scan_test_tempdir();
    create_test_tree(scan_root.path());

    let (writer, db_path, _db_dir) = setup_writer();
    let subtree_root = scan_root.path().join("subdir");
    ensure_path_in_db(&db_path, &subtree_root, &writer);

    // A clean subtree scan first, so the subtree has real rows and dir_stats.
    scan_subtree(
        &subtree_root,
        &IndexPathSpace::root(),
        &writer,
        &CancellationToken::new(),
    )
    .expect("the seeding scan");
    writer.flush_blocking().unwrap();

    let subtree_id = {
        let store = IndexStore::open(&db_path).unwrap();
        let conn = store.read_conn();
        let id = store::resolve_path(conn, &subtree_root.to_string_lossy())
            .unwrap()
            .expect("subtree root indexed");
        assert!(
            !store.list_children(id).unwrap().is_empty(),
            "precondition: the seeded subtree has children"
        );
        let seeded = IndexStore::get_dir_stats_by_id(conn, id).expect("read dir_stats");
        assert_eq!(
            seeded.map(|s| s.recursive_file_count),
            Some(2),
            "precondition: the seeding scan gave the subtree real dir_stats"
        );
        id
    };

    // Now cancel before the walk can put anything back.
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = scan_subtree(&subtree_root, &IndexPathSpace::root(), &writer, &cancel);
    assert!(
        matches!(result, Err(ScanError::Cancelled(_))),
        "a cancelled subtree scan must surface the typed cancellation, got {result:?}"
    );
    writer.flush_blocking().unwrap();
    writer.shutdown();

    let store = IndexStore::open(&db_path).unwrap();
    let conn = store.read_conn();
    assert!(
        store.list_children(subtree_id).unwrap().is_empty(),
        "the pre-walk delete ran, so the subtree is empty"
    );
    let stats = IndexStore::get_dir_stats_by_id(conn, subtree_id).expect("read dir_stats");
    assert_eq!(
        stats.map(|s| s.recursive_file_count),
        Some(0),
        "the aggregate must run on the cancel path too, or the ancestors keep sizes for deleted rows"
    );
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
        ..ScanConfig::default()
    };

    let (handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
    // Cancel immediately
    handle.cancel();

    let result = join_handle.join().expect("scan thread panicked");
    assert!(
        matches!(result, Err(ScanError::Cancelled(_))),
        "a cancelled scan must surface the typed cancellation, got {result:?}"
    );

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
        ..ScanConfig::default()
    };

    let (_handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
    let summary = join_handle.join().expect("scan thread panicked").unwrap();

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
        ..ScanConfig::default()
    };

    let (_handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
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

/// A scan on an inode-untrusted volume (FAT/exFAT) stores `inode: None` for
/// every entry, so the derived, unstable inode can never reach the index and
/// drive the live rename pre-pass. The identical scan on a trusted volume keeps
/// the real inode. This is the write-side half of the FAT/exFAT corruption
/// guard; the temp dir supplies real (trustworthy) inodes so the contrast is
/// observable without a synthetic FAT image.
#[test]
#[cfg(unix)]
fn scan_nulls_inode_on_inode_untrusted_volume() {
    let scan_root = scan_test_tempdir();
    fs::write(scan_root.path().join("file.txt"), "hello").unwrap();

    // Scan the SAME tree twice, toggling only `inodes_trustworthy`, and read back
    // the stored inode for the file.
    let stored_inode = |inodes_trustworthy: bool| -> Option<u64> {
        let (writer, db_path, _db_dir) = setup_writer();
        let config = ScanConfig {
            root: scan_root.path().to_path_buf(),
            batch_size: 100,
            num_threads: 1,
            space: IndexPathSpace::root().with_inodes_trustworthy(inodes_trustworthy),
        };
        let (_handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
        join_handle.join().expect("scan thread panicked").unwrap();
        writer.flush_blocking().unwrap();
        writer.shutdown();

        let store = IndexStore::open(&db_path).unwrap();
        let children = store.list_children(ROOT_ID).unwrap();
        children.iter().find(|e| e.name == "file.txt").unwrap().inode
    };

    // Trusted volume (default, e.g. APFS): the real inode is stored.
    assert!(
        stored_inode(true).is_some(),
        "a trusted volume stores the file's real inode"
    );
    // Untrusted volume (FAT/exFAT): the inode is nulled at write time.
    assert_eq!(
        stored_inode(false),
        None,
        "an inode-untrusted volume must store inode: None so the rename pre-pass stays inert"
    );
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
        ..ScanConfig::default()
    };

    let (_handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
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
        ..ScanConfig::default()
    };
    let (_handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
    let _summary = join_handle.join().expect("scan thread panicked").unwrap();

    // Trigger aggregation, then flush
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
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
        ..ScanConfig::default()
    };

    let (_handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
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
        ..ScanConfig::default()
    };

    let (handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
    join_handle.join().expect("scan thread panicked").unwrap();

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
        ..ScanConfig::default()
    };

    let (handle, join_handle) = scan_volume(config, &writer, CancellationToken::new()).unwrap();
    let summary = join_handle.join().expect("scan thread panicked").unwrap();
    writer.shutdown();

    assert_eq!(
        summary.total_physical_bytes,
        handle.progress.snapshot().bytes_scanned,
        "summary.total_physical_bytes must equal the final counter value"
    );
    assert!(summary.total_physical_bytes > 0, "scan should sum some physical bytes");
}

/// A read that stalls past the watchdog leaves the directory honestly unlisted AND
/// records why, so the coverage frontier stops handing it to every later search.
///
/// Both halves matter. Without the mark the dir stays plain `listed_epoch = 0`,
/// which is indistinguishable from ground nothing has reached yet, so every search
/// scoped above it re-pays the full stall timeout — forever, since a walk that
/// times out again changes nothing. The cause is `Abandoned` rather than `Denied`:
/// a hung mount is not a permission anybody can grant.
#[test]
fn a_timed_out_dir_is_marked_abandoned_and_never_marked_listed() {
    use crate::indexing::scanner::walker::{RawDirEntry, RawFileType, ReadDirFn, ReadProgress};
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
        Arc::new(move |p: &Path, progress: &ReadProgress| {
            if p == slow {
                // allowed-test-sleep: this stub fakes a hung directory read; stalling past the
                // walker's timeout is exactly what the "not marked listed" assertion needs
                std::thread::sleep(Duration::from_secs(2));
            }
            match dirs.get(p) {
                Some(children) => Ok(children
                    .iter()
                    .map(|(n, t)| {
                        progress.record_entries(1);
                        RawDirEntry {
                            path: p.join(n),
                            file_type: *t,
                            stat: None,
                        }
                    })
                    .collect()),
                None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no mock dir")),
            }
        })
    };

    let (writer, db_path, _db_dir) = setup_writer();
    let progress = Arc::new(ScanProgress::new());
    let cancelled = CancellationToken::new();

    let start = Instant::now();
    let outcome = run_scan(
        &root,
        &cancelled,
        &progress,
        &writer,
        100,
        4,
        WalkPolicy::for_walk(ScanRoot::Volume, &IndexPathSpace::root(), &root),
        &IndexPathSpace::root(), // boot-disk scope, trustworthy inodes
        reader,
        Duration::from_millis(50), // short timeout so the hang is abandoned fast
        None,
        None,
    )
    .expect("run_scan");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "must abandon the hang, not wait it out"
    );

    // The marks already rode along with the rows; all `scan_volume` adds on a
    // clean finish is the aggregate.
    let _ = &outcome;
    writer
        .send(WriteMessage::ComputeAllAggregates {
            source: AggSource::Maps,
        })
        .unwrap();
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

    // And the hung dir carries the cause, so the frontier stops offering it.
    let cause = |id: i64| {
        IndexStore::get_unreadable_cause_by_id(&conn, id)
            .expect("read the cause")
            .expect("row")
    };
    assert_eq!(
        cause(slow_id),
        Some(UnreadableCause::Abandoned),
        "a stalled read is ground Cmdr gave up on, ❌ not a permission the user can grant"
    );
    assert_eq!(cause(ok_id), None, "❌ nothing the walk actually read may be condemned");
    assert_eq!(cause(ROOT_ID), None);
}

#[test]
fn volume_root_that_never_lists_surfaces_root_unlistable() {
    use crate::indexing::scanner::walker::{ReadDirFn, ReadProgress};

    // A volume-root scan whose ROOT read FAILS (the mount vanished mid-scan): the
    // reader errors on every path, so the root is never listed (`dirs_read == 0`).
    // `run_scan` must surface the typed `RootUnlistable` instead of silently
    // "completing" with zero entries (which would false-complete an empty index).
    // Distinct from an empty-but-readable root, which lists successfully.
    let root = PathBuf::from("/vanished-volume-root");
    let reader: ReadDirFn = Arc::new(|_p: &Path, _progress: &ReadProgress| {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "mount vanished (test)",
        ))
    });

    let (writer, _db_path, _db_dir) = setup_writer();
    let progress = Arc::new(ScanProgress::new());
    let cancelled = CancellationToken::new();

    let result = run_scan(
        &root,
        &cancelled,
        &progress,
        &writer,
        100,
        4,
        WalkPolicy::for_walk(ScanRoot::Volume, &IndexPathSpace::root(), &root),
        &IndexPathSpace::root(),
        reader,
        Duration::from_millis(50),
        None,
        None,
    );
    writer.shutdown();

    assert!(
        matches!(result, Err(ScanError::RootUnlistable)),
        "an unreadable volume root must surface RootUnlistable, got {result:?}"
    );
}

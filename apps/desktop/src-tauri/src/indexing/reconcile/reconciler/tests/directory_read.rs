//! The batched directory read that feeds the diff must match a per-entry stat,
//! field for field, and tell "couldn't list" apart from "listed, empty".

use super::*;

// ── The reconcile read matches a per-entry stat, field for field ──────

/// A tree under CWD (not `/tmp` — excluded on Linux, a canonicalization alias on
/// macOS, either of which would empty the listing under test).
fn read_test_tree() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("cmdr-read-diff-")
        .tempdir_in(std::env::current_dir().expect("cwd"))
        .expect("temp dir")
}

/// The load-bearing guard on the batched read: `read_fs_children` must return
/// EXACTLY what the portable `read_dir` + per-entry `symlink_metadata` path
/// returns, across the cases that stress the parser. On macOS the two sides are
/// genuinely different readers (`getattrlistbulk` vs one `lstat` per entry), so a
/// mapping or offset bug shows up here as a mismatched field rather than as wrong
/// sizes written into the index; everywhere else the bulk reader doesn't exist and
/// this asserts the portable path against itself.
///
/// The reconcile is where a read bug bites hardest: `diff_dir_against_db` writes
/// the differences it finds, so a wrong size is persisted and a missing child is
/// DELETED from the index.
#[test]
fn the_reconcile_read_matches_a_per_entry_stat() {
    let dir = read_test_tree();
    let root = dir.path();
    std::fs::write(root.join("empty.bin"), b"").unwrap();
    std::fs::write(root.join("small.txt"), b"hello world").unwrap(); // 11 bytes
    std::fs::write(root.join("larger.bin"), vec![7u8; 40_000]).unwrap();
    std::fs::write(root.join("náïve-unïcode.txt"), b"unicode name").unwrap();
    std::fs::create_dir(root.join("subdir")).unwrap();
    std::fs::write(root.join("subdir/nested.txt"), b"nested").unwrap();
    std::fs::create_dir(root.join("empty_dir")).unwrap();
    std::os::unix::fs::symlink(root.join("small.txt"), root.join("link.txt")).unwrap();
    std::os::unix::fs::symlink(root.join("nowhere"), root.join("broken_link")).unwrap();
    // A hardlink pair (nlink == 2): the size of one occurrence must survive.
    std::fs::hard_link(root.join("small.txt"), root.join("small_alias.txt")).unwrap();
    // A plain dotfile (kept) next to an excluded basename (dropped), so the test
    // separates "the reader sees hidden entries" from "the gate drops this one".
    std::fs::write(root.join(".hidden.txt"), b"dot").unwrap();
    std::fs::create_dir(root.join(".Spotlight-V100")).unwrap();
    // A named pipe: a type the bulk reader carries no inline size for, so it must
    // take the per-entry stat fallback and land on the same values.
    let fifo = std::ffi::CString::new(root.join("pipe").as_os_str().as_bytes()).unwrap();
    // SAFETY: `fifo` is a valid NUL-terminated C string that outlives the call, naming
    // a path inside this test's own temp dir; `mkfifo` only reads it and creates the
    // node, returning 0 or -1.
    let mkfifo_rc = unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) };
    assert_eq!(
        mkfifo_rc, 0,
        "the fifo is what covers the no-inline-stat fallback; without it this test silently stops testing it"
    );

    let space = IndexPathSpace::root();
    let bulk = read_fs_children(root, &space).expect("the directory lists");
    let stated = read_fs_children_via_read_dir(root, &space).expect("the directory lists");

    let by_name = |children: Vec<FsChild>| -> std::collections::BTreeMap<String, FsChild> {
        children.into_iter().map(|c| (c.name.clone(), c)).collect()
    };
    let bulk = by_name(bulk);
    let stated = by_name(stated);

    assert_eq!(
        bulk.keys().collect::<Vec<_>>(),
        stated.keys().collect::<Vec<_>>(),
        "both reads must list the same children, including which ones the exclusions drop"
    );
    assert!(
        !bulk.contains_key(".Spotlight-V100"),
        "an excluded basename is dropped by both reads"
    );
    assert!(
        bulk.contains_key(".hidden.txt"),
        "a plain dotfile is listed — only the exclusion gate removes hidden entries"
    );
    assert!(bulk.contains_key("link.txt") && bulk.contains_key("broken_link"));
    assert!(bulk.contains_key("small_alias.txt"), "the hardlink is listed");
    assert!(
        bulk.contains_key("pipe"),
        "an entry with no inline stat is still listed, not dropped"
    );

    for (name, b) in &bulk {
        let s = &stated[name];
        assert_eq!(b.is_dir, s.is_dir, "is_dir mismatch for {name}");
        assert_eq!(b.is_symlink, s.is_symlink, "is_symlink mismatch for {name}");
        assert_eq!(
            b.snap.logical_size, s.snap.logical_size,
            "logical size mismatch for {name}"
        );
        assert_eq!(
            b.snap.physical_size, s.snap.physical_size,
            "physical size mismatch for {name}"
        );
        assert_eq!(b.snap.modified_at, s.snap.modified_at, "mtime mismatch for {name}");
        assert_eq!(b.snap.inode, s.snap.inode, "inode mismatch for {name}");
        assert_eq!(b.snap.nlink, s.snap.nlink, "nlink mismatch for {name}");
    }

    // Spot-check the values themselves, so a read that agreed on being wrong twice
    // (both sides broken the same way) still fails.
    assert_eq!(bulk["small.txt"].snap.logical_size, Some(11));
    assert_eq!(bulk["larger.bin"].snap.logical_size, Some(40_000));
    assert_eq!(
        bulk["small.txt"].snap.nlink,
        Some(2),
        "the hardlink pair reports nlink 2"
    );
    assert!(bulk["subdir"].is_dir && bulk["subdir"].snap.logical_size.is_none());
    assert!(bulk["link.txt"].is_symlink && bulk["link.txt"].snap.inode.is_none());
}

/// A directory that can't be listed reads as `None` on both paths — the signal the
/// walk maps onto "skip it, keep it honestly stale", never onto an empty listing
/// (which the diff would turn into a subtree delete).
#[test]
fn an_unlistable_directory_reads_as_none() {
    let dir = read_test_tree();
    let missing = dir.path().join("not-there");
    let space = IndexPathSpace::root();
    assert!(read_fs_children(&missing, &space).is_none());
    assert!(read_fs_children_via_read_dir(&missing, &space).is_none());
}

/// An empty-but-readable directory is `Some(vec![])`, NOT `None`: the walk has to
/// tell "listed, nothing in it" apart from "couldn't list".
#[test]
fn an_empty_directory_reads_as_an_empty_listing() {
    let dir = read_test_tree();
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let space = IndexPathSpace::root();
    let children = read_fs_children(&empty, &space).expect("an empty dir still lists");
    assert!(children.is_empty());
}

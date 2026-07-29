use super::*;

// ── index_relative_path: the mount-root → index-root mapping ──────────

#[test]
fn relative_path_maps_mount_absolute_to_index_root() {
    // The watcher's mount-absolute parent must become a path rooted at the
    // index ROOT_ID (the mount root). This is the resolution everything else
    // depends on.
    assert_eq!(
        index_relative_path("/Volumes/share", "/Volumes/share"),
        Some("/".into())
    );
    assert_eq!(
        index_relative_path("/Volumes/share", "/Volumes/share/sub"),
        Some("/sub".into())
    );
    assert_eq!(
        index_relative_path("/Volumes/share", "/Volumes/share/sub/deep"),
        Some("/sub/deep".into())
    );
}

#[test]
fn relative_path_tolerates_a_trailing_slash_on_the_root() {
    assert_eq!(
        index_relative_path("/Volumes/share/", "/Volumes/share/sub"),
        Some("/sub".into())
    );
    assert_eq!(
        index_relative_path("/Volumes/share/", "/Volumes/share/"),
        Some("/".into())
    );
}

#[test]
fn relative_path_rejects_paths_outside_the_mount() {
    // A path not under the mount root isn't on this volume: drop it rather
    // than mis-rooting it at ROOT_ID.
    assert_eq!(index_relative_path("/Volumes/share", "/Volumes/other/x"), None);
    // A sibling whose name merely shares the prefix must NOT match (the
    // remainder has to start with `/`).
    assert_eq!(index_relative_path("/Volumes/sh", "/Volumes/share"), None);
}

// ── resolve_change: the change → write mapping against a seeded index ──

/// Build a tiny SMB-shaped index: ROOT(1) → "sub"(dir) → "leaf.txt"(file),
/// "top.txt"(file) at the root, and "@eaDir"(dir) at the root — a
/// recursion-excluded NAS system dir whose OWN row the scanner does index.
/// Returns an open read/write connection.
fn seed_index() -> (rusqlite::Connection, tempfile::TempDir) {
    use crate::indexing::store::{EntryRow, ROOT_ID};
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("smb-watch-test.db");
    // `IndexStore::open` runs the schema init, which inserts the ROOT_ID
    // sentinel; a fresh write connection sees it (WAL, committed).
    let store = IndexStore::open(&db_path).expect("open store");
    drop(store);
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    let rows = vec![
        EntryRow {
            id: 2,
            parent_id: ROOT_ID,
            name: "sub".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 3,
            parent_id: 2,
            name: "leaf.txt".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(11),
            physical_size: Some(11),
            modified_at: None,
            inode: None,
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
            inode: None,
        },
        EntryRow {
            id: 5,
            parent_id: ROOT_ID,
            name: "@eaDir".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
        EntryRow {
            id: 6,
            parent_id: 2,
            name: "@Recycle".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &rows).expect("seed rows");
    (conn, dir)
}

fn file_entry(name: &str, path: &str, size: u64) -> FileEntry {
    FileEntry {
        size: Some(size),
        ..FileEntry::new(name.to_string(), path.to_string(), false, false)
    }
}

#[test]
fn added_maps_to_upsert_under_the_resolved_parent() {
    let (conn, _dir) = seed_index();
    let change = DirectoryChange::Added(file_entry("new.txt", "/Volumes/share/sub/new.txt", 7));
    let w = resolve_change(&conn, "/sub", &change).expect("a write");
    match w {
        ResolvedWrite::Upsert {
            parent_id,
            name,
            logical_size,
            ..
        } => {
            assert_eq!(parent_id, 2, "resolved under /sub (id=2)");
            assert_eq!(name, "new.txt");
            assert_eq!(logical_size, Some(7));
        }
        _ => panic!("Added must map to Upsert"),
    }
}

#[test]
fn modified_maps_to_upsert_with_new_size() {
    let (conn, _dir) = seed_index();
    let change = DirectoryChange::Modified(file_entry("top.txt", "/Volumes/share/top.txt", 99));
    let w = resolve_change(&conn, "/", &change).expect("a write");
    match w {
        ResolvedWrite::Upsert {
            parent_id,
            name,
            logical_size,
            ..
        } => {
            assert_eq!(parent_id, 1, "resolved under the mount root (ROOT_ID=1)");
            assert_eq!(name, "top.txt");
            assert_eq!(logical_size, Some(99));
        }
        _ => panic!("Modified must map to Upsert"),
    }
}

#[test]
fn removed_file_maps_to_delete_file_by_id() {
    let (conn, _dir) = seed_index();
    let change = DirectoryChange::Removed("top.txt".into());
    match resolve_change(&conn, "/", &change).expect("a write") {
        ResolvedWrite::DeleteFile(id) => assert_eq!(id, 4, "top.txt is id=4"),
        _ => panic!("Removed file must map to DeleteFile"),
    }
}

#[test]
fn removed_directory_maps_to_delete_subtree_by_id() {
    let (conn, _dir) = seed_index();
    let change = DirectoryChange::Removed("sub".into());
    match resolve_change(&conn, "/", &change).expect("a write") {
        ResolvedWrite::DeleteSubtree(id) => assert_eq!(id, 2, "sub is id=2"),
        _ => panic!("Removed directory must map to DeleteSubtree"),
    }
}

#[test]
fn removed_never_indexed_name_is_a_no_op() {
    // Stat-verify rule: a Removed for a name the index never had is a false
    // removal (coalesced event for a path we never saw). It must NOT enqueue
    // a delete — no entry id to delete, so resolve_change yields None.
    let (conn, _dir) = seed_index();
    let change = DirectoryChange::Removed("ghost.txt".into());
    assert!(resolve_change(&conn, "/", &change).is_none());
}

#[test]
fn change_under_unindexed_parent_is_a_no_op() {
    // If the parent dir isn't in the index (never scanned, or a path the scan
    // didn't reach), there's nothing to attach the child to: no write.
    let (conn, _dir) = seed_index();
    let change = DirectoryChange::Added(file_entry("x.txt", "/Volumes/share/nope/x.txt", 1));
    assert!(resolve_change(&conn, "/nope", &change).is_none());
}

// ── Recursion-excluded NAS system dirs (the scanner's rule, live-side) ──

#[test]
fn change_inside_a_recursion_excluded_dir_writes_nothing() {
    // The scanner keeps `@eaDir`'s own row but never walks its subtree, so a
    // child row under it is one no scan would ever produce — and `writer/prune.rs`
    // deletes exactly those. Without this gate the live watcher re-creates them
    // right after a prune, because `@eaDir` itself IS indexed and so resolves.
    let (conn, _dir) = seed_index();
    for change in [
        DirectoryChange::Added(file_entry("thumb.jpg", "/Volumes/share/@eaDir/thumb.jpg", 3)),
        DirectoryChange::Modified(file_entry("thumb.jpg", "/Volumes/share/@eaDir/thumb.jpg", 4)),
        DirectoryChange::Renamed {
            old_name: "thumb.jpg".into(),
            new_entry: file_entry("thumb2.jpg", "/Volumes/share/@eaDir/thumb2.jpg", 4),
        },
        DirectoryChange::Removed("thumb.jpg".into()),
    ] {
        assert!(
            resolve_change(&conn, "/@eaDir", &change).is_none(),
            "no write may land inside a recursion-excluded dir",
        );
    }
}

#[test]
fn the_gate_looks_at_every_ancestor_not_only_the_immediate_parent() {
    // An excluded dir nested below an ordinary one is gated the same way: the
    // scanner stops at `@Recycle` wherever it sits, so nothing below it belongs
    // in the index either.
    let (conn, _dir) = seed_index();
    let change = DirectoryChange::Added(file_entry("gone.txt", "/Volumes/share/sub/@Recycle/gone.txt", 1));
    assert!(resolve_change(&conn, "/sub/@Recycle", &change).is_none());
}

#[test]
fn the_gate_matches_components_case_insensitively_and_whole() {
    // The path predicate must follow the scanner's rule exactly: whole
    // components, case-insensitive, at any depth — and never a substring match
    // that would swallow an ordinary folder.
    assert!(is_under_recursion_excluded_dir("/@eaDir"));
    assert!(is_under_recursion_excluded_dir("/@eadir/deep"));
    assert!(is_under_recursion_excluded_dir("/sub/@RECYCLE/x"));
    assert!(!is_under_recursion_excluded_dir("/"));
    assert!(!is_under_recursion_excluded_dir("/sub"));
    assert!(!is_under_recursion_excluded_dir("/@eaDirectory"));
    assert!(!is_under_recursion_excluded_dir("/my@eaDir"));
}

#[test]
fn the_excluded_dir_s_own_row_and_ordinary_siblings_still_update() {
    // The invariant the gate must NOT break: `@eaDir` stays indexed, listed, and
    // navigable. A change TO it (not inside it) arrives under its PARENT, so it
    // passes the gate like any other entry — as do ordinary folders next to it.
    let (conn, _dir) = seed_index();

    let own = FileEntry::new("@eaDir".into(), "/Volumes/share/@eaDir".into(), true, false);
    match resolve_change(&conn, "/", &DirectoryChange::Modified(own)).expect("a write") {
        ResolvedWrite::Upsert {
            name,
            parent_id,
            is_directory,
            ..
        } => {
            assert_eq!(name, "@eaDir", "the excluded dir's own row must still be written");
            assert_eq!(parent_id, 1);
            assert!(is_directory);
        }
        _ => panic!("a change to the excluded dir itself must upsert it"),
    }

    // And deleting it outright is still a real removal of its subtree.
    match resolve_change(&conn, "/", &DirectoryChange::Removed("@eaDir".into())).expect("a write") {
        ResolvedWrite::DeleteSubtree(id) => assert_eq!(id, 5, "@eaDir is id=5"),
        _ => panic!("removing the excluded dir itself must delete its subtree"),
    }

    // An ordinary sibling folder is untouched by the gate.
    let change = DirectoryChange::Added(file_entry("new.txt", "/Volumes/share/sub/new.txt", 7));
    assert!(resolve_change(&conn, "/sub", &change).is_some());
}

#[test]
fn full_refresh_produces_no_targeted_write() {
    // Overflow/bulk: the targeted translator can't express it; the
    // watcher-lifetime layer owns overflow policy. resolve_change is a no-op.
    let (conn, _dir) = seed_index();
    assert!(resolve_change(&conn, "/", &DirectoryChange::FullRefresh).is_none());
}

#[test]
fn renamed_upserts_the_new_entry() {
    let (conn, _dir) = seed_index();
    let change = DirectoryChange::Renamed {
        old_name: "top.txt".into(),
        new_entry: file_entry("renamed.txt", "/Volumes/share/renamed.txt", 5),
    };
    match resolve_change(&conn, "/", &change).expect("a write") {
        ResolvedWrite::Upsert { name, parent_id, .. } => {
            assert_eq!(name, "renamed.txt");
            assert_eq!(parent_id, 1);
        }
        _ => panic!("Renamed must upsert the new entry"),
    }
}

#[test]
fn join_rel_normalizes_the_separator() {
    assert_eq!(join_rel("/", "a.txt"), "/a.txt");
    assert_eq!(join_rel("/sub", "a.txt"), "/sub/a.txt");
}

// ── Mid-scan buffer mechanics (the pre-arm-before-snapshot buffer) ─────

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn buffer_accumulates_then_discard_clears_it() {
    let vid = "smb-buffer-discard-test";
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);

    for i in 0..3 {
        let change = DirectoryChange::Added(file_entry(&format!("f{i}.txt"), &format!("/Volumes/share/f{i}.txt"), 1));
        buffer_change_during_scan(vid, Path::new("/Volumes/share"), &change);
    }
    {
        let buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        assert_eq!(buf.get(vid).map(|b| b.changes.len()), Some(3), "three buffered");
        assert!(!buf.get(vid).unwrap().overflowed, "no overflow under the cap");
    }

    // Discard (the D-interrupted path) drops the buffer entirely.
    discard_buffered_changes(vid);
    assert!(
        SCAN_CHANGE_BUFFER.lock_ignore_poison().get(vid).is_none(),
        "discard must clear the buffer",
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn buffer_overflow_sets_the_flag_and_stops_growing() {
    let vid = "smb-buffer-overflow-test";
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);

    // Pre-fill the buffer to the cap directly so we don't push 50k entries.
    {
        let mut buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        let entry = buf.entry(vid.to_string()).or_default();
        entry.changes.reserve(MAX_BUFFERED_CHANGES);
        for _ in 0..MAX_BUFFERED_CHANGES {
            entry
                .changes
                .push((std::path::PathBuf::from("/Volumes/share"), DirectoryChange::FullRefresh));
        }
    }

    // One more push must trip overflow and NOT grow the buffer past the cap.
    let change = DirectoryChange::Added(file_entry("x.txt", "/Volumes/share/x.txt", 1));
    buffer_change_during_scan(vid, Path::new("/Volumes/share"), &change);
    {
        let buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        let b = buf.get(vid).expect("buffer present");
        assert!(b.overflowed, "hitting the cap must set the overflow flag");
        assert_eq!(b.changes.len(), MAX_BUFFERED_CHANGES, "must not grow past the cap");
    }

    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
}

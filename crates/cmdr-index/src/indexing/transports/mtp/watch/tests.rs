use super::*;

/// Seed a tiny MTP-shaped index: ROOT(1) → "DCIM"(dir, handle=10) →
/// "IMG.jpg"(file, handle=11), and "top.txt"(file, handle=12) at root.
/// Returns an open write connection.
fn seed_index() -> (rusqlite::Connection, tempfile::TempDir) {
    use crate::indexing::store::{EntryRow, ROOT_ID};
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("mtp-watch-test.db");
    let store = IndexStore::open(&db_path).expect("open store");
    drop(store);
    let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
    let rows = vec![
        EntryRow {
            id: 2,
            parent_id: ROOT_ID,
            name: "DCIM".into(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: Some(10),
        },
        EntryRow {
            id: 3,
            parent_id: 2,
            name: "IMG.jpg".into(),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(100),
            physical_size: Some(100),
            modified_at: None,
            inode: Some(11),
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
            inode: Some(12),
        },
    ];
    IndexStore::insert_entries_v2_batch(&conn, &rows).expect("seed rows");
    (conn, dir)
}

fn upsert(path: &str, handle: u32, is_dir: bool, size: Option<u64>) -> MtpUpsert {
    MtpUpsert {
        path: PathBuf::from(path),
        handle,
        is_directory: is_dir,
        size,
        modified_at: None,
    }
}

// ── resolve_upsert: add/change → upsert with the handle in inode ───────

#[test]
fn added_file_maps_to_upsert_under_resolved_parent_with_handle() {
    let (conn, _dir) = seed_index();
    // A new photo in /DCIM, handle 20.
    let w = resolve_upsert(&conn, &upsert("/DCIM/NEW.jpg", 20, false, Some(7))).expect("a write");
    match w {
        ResolvedWrite::Upsert {
            parent_id,
            name,
            handle,
            size,
            ..
        } => {
            assert_eq!(parent_id, 2, "resolved under /DCIM (id=2)");
            assert_eq!(name, "NEW.jpg");
            assert_eq!(handle, 20, "the object handle is carried into the upsert (→ inode)");
            assert_eq!(size, Some(7));
        }
        _ => panic!("Added must map to Upsert"),
    }
}

#[test]
fn added_at_storage_root_resolves_under_root_id() {
    let (conn, _dir) = seed_index();
    let w = resolve_upsert(&conn, &upsert("/movie.mp4", 21, false, Some(9))).expect("a write");
    match w {
        ResolvedWrite::Upsert { parent_id, name, .. } => {
            assert_eq!(parent_id, 1, "resolved under the storage root (ROOT_ID=1)");
            assert_eq!(name, "movie.mp4");
        }
        _ => panic!("root-level add must map to Upsert under ROOT_ID"),
    }
}

#[test]
fn change_under_unindexed_parent_is_a_no_op() {
    let (conn, _dir) = seed_index();
    // /Music isn't in the index, so a change inside it can't attach.
    assert!(resolve_upsert(&conn, &upsert("/Music/song.mp3", 30, false, Some(1))).is_none());
}

// ── resolve_remove: pathless removal via the STORED handle ─────────────

#[test]
fn removed_file_resolves_by_stored_handle_to_delete_file() {
    let (conn, _dir) = seed_index();
    // IMG.jpg was stored with handle 11. ObjectRemoved{11} must delete id=3.
    match resolve_remove(&conn, 11).expect("a write") {
        ResolvedWrite::DeleteFile(id) => assert_eq!(id, 3, "IMG.jpg (handle 11) is id=3"),
        _ => panic!("removing a file handle must map to DeleteFile"),
    }
}

#[test]
fn removed_directory_resolves_by_stored_handle_to_delete_subtree() {
    let (conn, _dir) = seed_index();
    // DCIM was stored with handle 10. ObjectRemoved{10} must delete the subtree.
    match resolve_remove(&conn, 10).expect("a write") {
        ResolvedWrite::DeleteSubtree(id) => assert_eq!(id, 2, "DCIM (handle 10) is id=2"),
        _ => panic!("removing a directory handle must map to DeleteSubtree"),
    }
}

#[test]
fn removed_unknown_handle_is_a_no_op() {
    // A removal for a handle we never indexed (object we never saw) is a
    // no-op — the MTP analogue of the SMB stat-verify "never-indexed" rule.
    let (conn, _dir) = seed_index();
    assert!(resolve_remove(&conn, 9999).is_none());
}

// ── live changes resolve on the writer, in queue order ─────────────────

/// Runs `send` against a live writer on the seeded index while an explicit
/// transaction holds everything uncommitted, which is the window a busy
/// writer's batch leaves open at random, then commits and settles.
fn with_held_batch(send: impl FnOnce(&IndexWriter)) -> (rusqlite::Connection, tempfile::TempDir) {
    let (conn, dir) = seed_index();
    let writer = IndexWriter::spawn(&dir.path().join("mtp-watch-test.db"), crate::NoopEventSink::shared())
        .expect("spawn writer");
    writer.send(WriteMessage::BeginTransaction).expect("begin");
    send(&writer);
    writer.send(WriteMessage::CommitTransaction).expect("commit");
    writer.flush_blocking().expect("flush");
    writer.shutdown();
    (conn, dir)
}

/// Copying a folder onto a phone adds the folder, then each file in it, a
/// few milliseconds apart. Resolved on a read connection, a file's parent
/// lookup missed the folder row still uncommitted in the writer's batch, and
/// the file was never indexed.
#[test]
fn a_file_added_right_behind_its_new_folder_lands_inside_it() {
    let (conn, _dir) = with_held_batch(|writer| {
        send_upsert(writer, upsert("/Music", 40, true, None));
        send_upsert(writer, upsert("/Music/song.mp3", 41, false, Some(3)));
    });
    assert!(
        store::resolve_path(&conn, "/Music/song.mp3")
            .expect("readable")
            .is_some(),
        "the file must be indexed under the folder added just before it"
    );
}

/// A removal right behind its own add (a quick delete after a copy) must find
/// the row the add made. Resolved on a read connection, it missed the
/// uncommitted row and the file stayed in the index for good.
#[test]
fn a_removal_right_behind_its_add_deletes_the_row_the_add_made() {
    let (conn, _dir) = with_held_batch(|writer| {
        send_upsert(writer, upsert("/DCIM/tmp.jpg", 50, false, Some(4)));
        send_remove(writer, 50);
    });
    assert_eq!(
        store::resolve_path(&conn, "/DCIM/tmp.jpg").expect("readable"),
        None,
        "the removed file must not survive as a row"
    );
}

// ── path / url helpers ─────────────────────────────────────────────────

#[test]
fn path_to_index_str_normalizes_root_and_leading_slash() {
    assert_eq!(path_to_index_str(std::path::Path::new("/")), "/");
    assert_eq!(path_to_index_str(std::path::Path::new("/DCIM")), "/DCIM");
    assert_eq!(path_to_index_str(std::path::Path::new("")), "/");
}

#[test]
fn mtp_url_for_builds_the_listing_cache_key() {
    let vid = "mtp-PIXEL7:65537";
    assert_eq!(mtp_url_for(vid, std::path::Path::new("/")), "mtp://mtp-PIXEL7/65537");
    assert_eq!(
        mtp_url_for(vid, std::path::Path::new("/DCIM")),
        "mtp://mtp-PIXEL7/65537/DCIM"
    );
    // A serial device id containing a colon round-trips into the URL.
    assert_eq!(
        mtp_url_for("mtp-AA:BB:65537", std::path::Path::new("/Music")),
        "mtp://mtp-AA:BB/65537/Music"
    );
}

// ── mid-scan buffer mechanics ──────────────────────────────────────────

#[test]
fn buffer_accumulates_then_discard_clears_it() {
    let vid = "mtp-buffer-discard-test";
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
    for i in 0..3 {
        buffer_change_during_scan(
            vid,
            BufferedChange::Upsert(upsert(&format!("/f{i}.txt"), 100 + i, false, Some(1))),
        );
    }
    buffer_change_during_scan(vid, BufferedChange::Remove(50));
    {
        let buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        assert_eq!(buf.get(vid).map(|b| b.changes.len()), Some(4), "four buffered");
        assert!(!buf.get(vid).unwrap().overflowed);
    }
    discard_buffered_mtp_changes(vid);
    assert!(
        SCAN_CHANGE_BUFFER.lock_ignore_poison().get(vid).is_none(),
        "discard must clear the buffer",
    );
}

#[test]
fn buffer_holds_raw_upsert_handles_for_post_scan_resolve() {
    // The gate-before-resolve fix buffers a RAW (storage_id, handle), not a
    // resolved upsert — no device round trip during a scan. Confirm the
    // variant accumulates and the replay-partition sees it as a to-resolve.
    let vid = "mtp-buffer-handle-test";
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
    buffer_change_during_scan(
        vid,
        BufferedChange::UpsertHandle {
            storage_id: 65537,
            handle: 42,
        },
    );
    buffer_change_during_scan(
        vid,
        BufferedChange::UpsertHandle {
            storage_id: 65537,
            handle: 43,
        },
    );
    buffer_change_during_scan(vid, BufferedChange::Remove(7));
    {
        let buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        let changes = &buf.get(vid).expect("buffer present").changes;
        assert_eq!(changes.len(), 3);
        let raw_handles: Vec<u32> = changes
            .iter()
            .filter_map(|c| match c {
                BufferedChange::UpsertHandle { handle, .. } => Some(*handle),
                _ => None,
            })
            .collect();
        assert_eq!(raw_handles, vec![42, 43], "both raw handles buffered, in order");
    }
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
}

#[test]
fn buffer_gate_returns_false_for_an_unregistered_volume() {
    // `buffer_mtp_handle_if_scanning` must NOT buffer (and must return false,
    // so the caller resolves live) when the volume has no Running index — the
    // device-free fast path. A never-registered volume id exercises this.
    let vid = "mtp-never-registered-gate:65537";
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
    assert!(
        !buffer_mtp_handle_if_scanning(vid, 65537, 99),
        "no Running index ⇒ don't buffer, let the caller resolve live",
    );
    assert!(
        SCAN_CHANGE_BUFFER.lock_ignore_poison().get(vid).is_none(),
        "nothing buffered for an unregistered volume",
    );
}

#[test]
fn buffer_overflow_sets_the_flag_and_stops_growing() {
    let vid = "mtp-buffer-overflow-test";
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
    {
        let mut buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        let entry = buf.entry(vid.to_string()).or_default();
        entry.changes.reserve(MAX_BUFFERED_CHANGES);
        for _ in 0..MAX_BUFFERED_CHANGES {
            entry.changes.push(BufferedChange::Remove(1));
        }
    }
    buffer_change_during_scan(vid, BufferedChange::Remove(2));
    {
        let buf = SCAN_CHANGE_BUFFER.lock_ignore_poison();
        let b = buf.get(vid).expect("buffer present");
        assert!(b.overflowed, "hitting the cap must set overflow");
        assert_eq!(b.changes.len(), MAX_BUFFERED_CHANGES, "must not grow past the cap");
    }
    SCAN_CHANGE_BUFFER.lock_ignore_poison().remove(vid);
}

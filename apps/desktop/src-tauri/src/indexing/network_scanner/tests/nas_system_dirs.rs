//! NAS system and snapshot dirs: their subtrees are never walked, rows an older
//! scan left under one can only be shed by a rebuild, and look-alike user
//! folders keep being walked.

use super::*;

/// The recursive size scan must NOT descend into NAS snapshot/system dirs
/// (`@eaDir`, `@Recently-Snapshot`, …): they're hardlinked/huge and recursively
/// sizing them stalled a real first-scan (`@Recently-Snapshot` alone reported 44 TB
/// on a 10 TB volume). The dir's OWN row stays indexed (listed + navigable), but its
/// subtree is never walked — at the share root AND nested inside a normal dir.
#[tokio::test]
async fn skips_recursion_into_nas_system_dirs() {
    use crate::indexing::writer::IndexWriter;

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("vol-scan-skip.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");

    let vol = InMemoryVolume::with_entries(
        "Test",
        vec![
            entry("photos", "/photos", true, None),
            // Synology thumbnail sidecar nested inside a normal dir → skip recursion.
            entry("@eaDir", "/photos/@eaDir", true, None),
            entry("thumb.jpg", "/photos/@eaDir/thumb.jpg", false, Some(999)),
            // Snapshot root at the share root → skip recursion.
            entry("@Recently-Snapshot", "/@Recently-Snapshot", true, None),
            entry(
                "full-copy.bin",
                "/@Recently-Snapshot/full-copy.bin",
                false,
                Some(44_000),
            ),
            entry("keep.txt", "/keep.txt", false, Some(5)),
        ],
    );
    let vol: Arc<dyn Volume> = Arc::new(vol);

    let cancelled = CancellationToken::new();
    scan_volume_via_trait(
        vol,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("scan should complete");

    writer.flush().await.expect("flush");
    writer.shutdown();

    let store = IndexStore::open(&db_path).expect("reopen");

    // The system dirs themselves ARE indexed (visible + navigable).
    let root_children = store.list_children(ROOT_ID).expect("list root");
    let snap = root_children
        .iter()
        .find(|e| e.name == "@Recently-Snapshot")
        .expect("@Recently-Snapshot row present (visible, navigable)");
    let photos = root_children
        .iter()
        .find(|e| e.name == "photos")
        .expect("photos present");

    // …but their subtrees are NOT walked.
    assert_eq!(
        store.list_children(snap.id).expect("list snapshot").len(),
        0,
        "snapshot subtree must not be indexed (no recursive descent)",
    );
    let photos_children = store.list_children(photos.id).expect("list photos");
    let eadir = photos_children
        .iter()
        .find(|e| e.name == "@eaDir")
        .expect("@eaDir row present under photos");
    assert_eq!(
        store.list_children(eadir.id).expect("list eaDir").len(),
        0,
        "@eaDir subtree must not be indexed even nested under a normal dir",
    );
}

// ── Pruning rows left under a now-excluded dir ───────────────────

/// Seed `count` file rows plus one nested dir under `parent_id`, as an index
/// built BEFORE the dir was recursion-excluded would hold them, and give the
/// parent a `listed_epoch` + inflated `dir_stats` the way that older scan did.
/// Returns the nested dir's id.
fn seed_stale_subtree(writer: &IndexWriter, db_path: &Path, parent_id: i64, first_id: i64, count: i64) -> i64 {
    let nested_id = first_id;
    let mut rows = vec![EntryRow {
        id: nested_id,
        parent_id,
        name: "nested".into(),
        is_directory: true,
        is_symlink: false,
        logical_size: None,
        physical_size: None,
        modified_at: None,
        inode: None,
    }];
    for i in 0..count {
        rows.push(EntryRow {
            id: first_id + 1 + i,
            parent_id: nested_id,
            name: format!("stale-{i}.bin"),
            is_directory: false,
            is_symlink: false,
            logical_size: Some(1_000_000),
            physical_size: Some(1_000_000),
            modified_at: None,
            inode: None,
        });
    }
    writer
        .send(WriteMessage::InsertEntriesV2(rows))
        .expect("seed stale rows");
    // The older scan listed both dirs, so both carry a non-zero `listed_epoch`.
    let epoch = {
        let conn = IndexStore::open_read_connection(db_path).expect("read conn");
        IndexStore::read_current_epoch(&conn).expect("epoch")
    };
    writer
        .send(WriteMessage::MarkDirsListed {
            ids: vec![parent_id, nested_id],
            epoch,
        })
        .expect("mark seeded dirs listed");
    writer
        .send(WriteMessage::ComputeAllAggregates { source: AggSource::Sql })
        .expect("aggregate the seeded tree");
    nested_id
}

/// **Why the exclusion-list rebuild exists.** An index built BEFORE a directory
/// was recursion-excluded carries every row of that subtree, and a reconcile can
/// never shed them: it only diffs the dirs it LISTS, and it deliberately doesn't
/// list this one. On a real QNAP that left 10 898 710 out-of-scope rows (80% of a
/// 13.5M-row, 1.88 GB index) and rolled a 10 TB NAS up to 89 TB.
///
/// So a rescan-in-place is NOT a fix for such an index; only a truncate-and-build
/// is (`lifecycle/network_scan.rs`, `NetworkScanMode::Rebuild`). This pins that
/// premise, so nobody re-derives "a reconcile will clean it up".
#[tokio::test]
async fn reconcile_cannot_shed_rows_left_under_a_now_excluded_dir() {
    let tree = vec![
        entry("@Recently-Snapshot", "/@Recently-Snapshot", true, None),
        entry("sub", "/sub", true, None),
        entry("keep.txt", "/sub/keep.txt", false, Some(4)),
        entry("top.txt", "/top.txt", false, Some(5)),
    ];
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", tree));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol)).await;

    // Back-date the index to the pre-exclusion state: rows under the snapshot dir.
    let snapshot_id = {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        resolve_path(&conn, "/@Recently-Snapshot")
            .expect("resolve")
            .expect("snapshot dir indexed")
    };
    let nested_id = seed_stale_subtree(&writer, &db_path, snapshot_id, 9_000, 4);
    writer.flush().await.expect("flush");

    // The seeded state matches production: the snapshot dir claims an exact,
    // inflated recursive size.
    {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert_eq!(
            dir_size(&conn, "/@Recently-Snapshot"),
            4_000_000,
            "test setup: the stale subtree inflates the snapshot dir's size"
        );
    }

    // A continuity break bumps the epoch before a rescan; mirror the manager.
    {
        let wconn = IndexStore::open_write_connection(&db_path).expect("write conn");
        IndexStore::bump_current_epoch(&wconn).expect("bump epoch");
    }

    let cancelled = CancellationToken::new();
    reconcile_volume_via_trait(
        Arc::clone(&vol),
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("reconcile");
    writer.flush().await.expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");

    // The excluded dir's OWN row stays indexed (listed + navigable) — and so does
    // everything an older list left beneath it.
    assert!(
        IndexStore::get_entry_by_id(&conn, snapshot_id)
            .expect("entry")
            .is_some(),
        "the excluded dir's own row must stay indexed so it's listed and navigable"
    );
    assert!(
        IndexStore::get_entry_by_id(&conn, nested_id).expect("entry").is_some(),
        "a reconcile can't reach rows under a dir it never lists: rebuilding is the only fix"
    );
    assert_eq!(
        dir_size(&conn, "/@Recently-Snapshot"),
        4_000_000,
        "and the stale rows keep inflating the roll-up until the index is rebuilt"
    );

    // The in-scope tree is walked normally throughout.
    assert_eq!(dir_size(&conn, "/sub"), 4, "the real tree keeps its size");
    assert!(
        IndexStore::resolve_component(&conn, ROOT_ID, "top.txt")
            .expect("resolve")
            .is_some(),
        "in-scope rows are untouched"
    );

    writer.shutdown();
}

/// The data-safety guard: the exclusion matches ONLY reserved names. A folder
/// whose name merely LOOKS like one (`snapshot`, `eaDir`, `@myfiles`, `System
/// Volume Information Archive`) is real user data the scanner must keep walking,
/// so its subtree and sizes come through a reconcile intact. A matcher bug here
/// would drop a user's folder from the index on the next rebuild.
#[tokio::test]
async fn reconcile_keeps_walking_lookalike_user_folders() {
    let lookalikes = [
        "snapshot",
        "eaDir",
        "recycle",
        "@myfiles",
        "@Recently-Snapshotted",
        "System Volume Information Archive",
        ".snapshot-backup",
    ];
    let mut tree = Vec::new();
    for name in lookalikes {
        tree.push(entry(name, &format!("/{name}"), true, None));
        tree.push(entry("deep", &format!("/{name}/deep"), true, None));
        tree.push(entry("mine.txt", &format!("/{name}/deep/mine.txt"), false, Some(11)));
    }
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", tree));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol)).await;

    let rows_before = {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        entry_count(&conn)
    };

    let cancelled = CancellationToken::new();
    reconcile_volume_via_trait(
        Arc::clone(&vol),
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("reconcile");
    writer.flush().await.expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(
        entry_count(&conn),
        rows_before,
        "a reconcile must not drop a single row of a look-alike user folder"
    );
    for name in lookalikes {
        assert_eq!(
            dir_size(&conn, &format!("/{name}")),
            11,
            "{name} is real user data: its subtree and size must survive"
        );
    }

    writer.shutdown();
}

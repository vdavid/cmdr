//! The rescan-in-place walk: an unchanged tree costs no writes, a changed one
//! lands where a fresh scan would, and an interrupted or glitched one keeps the
//! index it already had.

use super::*;

/// A reconcile rescan over an UNCHANGED tree writes ZERO entry rows (the
/// no-op-cheap property the perf bench relied on): unchanged rows are diffed and
/// skipped, never re-UPSERTed, so the catastrophic INSERT OR REPLACE path is
/// never touched. Coverage still re-stamps to the new epoch.
#[tokio::test]
async fn reconcile_noop_writes_zero_entry_rows() {
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol)).await;

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let rows_before = entry_count(&conn);
    let max_id_before: i64 = conn
        .query_row("SELECT COALESCE(MAX(id), 0) FROM entries", [], |r| r.get(0))
        .unwrap();

    // A continuity break would bump the epoch before a rescan; mirror that.
    let new_epoch = {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap()
    };

    // Reconcile the SAME tree (nothing changed on disk).
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
        "no-op reconcile must not change the entry row count"
    );
    let max_id_after: i64 = conn
        .query_row("SELECT COALESCE(MAX(id), 0) FROM entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        max_id_after, max_id_before,
        "no-op reconcile must not allocate any new ids (zero rows written)"
    );
    // Coverage re-stamped to the new epoch (the single aggregate ran).
    assert_eq!(
        min_epoch(&conn, "/sub"),
        new_epoch,
        "no-op reconcile re-stamps coverage to the new epoch"
    );

    writer.shutdown();
}

/// A reconcile rescan with changes (add / remove / modify) refreshes sizes
/// correctly AND ends byte-identical (entry set + dir sizes) to a
/// fresh-from-scratch scan of the SAME final tree. The 1.83 TB-ghost guard.
#[tokio::test]
async fn reconcile_with_changes_matches_fresh_from_scratch() {
    let vol_before: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_before)).await;

    // Final tree: remove keep.txt, modify mod.txt (4→20 bytes), add new.txt,
    // add a new subdir with a file.
    let final_tree = vec![
        entry("sub", "/sub", true, None),
        entry("mod.txt", "/sub/mod.txt", false, Some(20)),
        entry("new.txt", "/sub/new.txt", false, Some(7)),
        entry("deep", "/sub/deep", true, None),
        entry("d.txt", "/sub/deep/d.txt", false, Some(3)),
        entry("top.txt", "/top.txt", false, Some(5)),
    ];
    let vol_after: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", final_tree.clone()));

    // Bump epoch (continuity break) then reconcile to the final tree.
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap();
    }
    let cancelled = CancellationToken::new();
    reconcile_volume_via_trait(
        Arc::clone(&vol_after),
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await
    .expect("reconcile");
    writer.flush().await.expect("flush");

    // Fresh-from-scratch oracle: scan the final tree into a clean DB.
    let vol_oracle: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", final_tree));
    let (oracle_writer, oracle_db, _odir) = fresh_scan(vol_oracle).await;

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let oconn = IndexStore::open_read_connection(&oracle_db).expect("oracle read conn");

    // keep.txt gone; new.txt + deep/ present.
    assert!(
        resolve_path(&conn, "/sub/keep.txt").unwrap().is_none(),
        "removed file gone"
    );
    assert!(
        resolve_path(&conn, "/sub/new.txt").unwrap().is_some(),
        "added file present"
    );
    assert!(
        resolve_path(&conn, "/sub/deep/d.txt").unwrap().is_some(),
        "new subtree present"
    );

    // Same recursive sizes as a fresh build (no ghosts).
    assert_eq!(
        dir_size(&conn, "/sub"),
        dir_size(&oconn, "/sub"),
        "/sub size matches fresh"
    );
    assert_eq!(dir_size(&conn, "/"), dir_size(&oconn, "/"), "root size matches fresh");
    // mod.txt's new size is reflected: /sub = mod(20) + new(7) + deep/d(3) = 30.
    assert_eq!(dir_size(&conn, "/sub"), 30, "reconciled /sub reflects modify + adds");

    writer.shutdown();
    oracle_writer.shutdown();
}

/// A mid-rescan DISCONNECT leaves the PRIOR complete index intact (now possible
/// — no truncate ran) and surfaces the typed terminal error. The re-listed dirs
/// are stamped at the rescan epoch; unreached dirs keep their prior data. The
/// completion handler (manager) then bumps past the epoch so everything reads
/// stale — here we assert the prior data SURVIVES (the headline reconcile property).
#[tokio::test]
async fn mid_reconcile_disconnect_keeps_prior_index() {
    // Wide tree so the disconnect leaves real dirs unreached.
    let mut before = vec![entry("top.txt", "/top.txt", false, Some(5))];
    for i in 0..20 {
        before.push(entry(&format!("d{i}"), &format!("/d{i}"), true, None));
        before.push(entry("f.txt", &format!("/d{i}/f.txt"), false, Some(10)));
    }
    let vol_before: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", before));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_before)).await;

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let rows_before = entry_count(&conn);
    assert!(rows_before > 20, "prior complete index has all dirs");
    let root_size_before = dir_size(&conn, "/");

    // A disconnecting volume: lists the root + a couple dirs, then drops.
    let calls = Arc::new(AtomicU64::new(0));
    let mut after = vec![entry("top.txt", "/top.txt", false, Some(5))];
    for i in 0..20 {
        after.push(entry(&format!("d{i}"), &format!("/d{i}"), true, None));
        after.push(entry("f.txt", &format!("/d{i}/f.txt"), false, Some(10)));
    }
    let vol_disc: Arc<dyn Volume> = Arc::new(CountingDisconnectVolume {
        inner: InMemoryVolume::with_entries("Test", after),
        fail_after_calls: 4, // root + a few dirs, then disconnect
        calls: Arc::clone(&calls),
        untyped_failure: false,
    });

    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap();
    }
    let cancelled = CancellationToken::new();
    let result = reconcile_volume_via_trait(
        vol_disc,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::Volume(VolumeError::DeviceDisconnected(_))) => {}
        other => panic!("expected typed terminal disconnect, got {other:?}"),
    }
    writer.flush().await.expect("flush");

    // The prior index is INTACT: no truncate ran, all rows still present, sizes
    // unchanged (the unreached dirs were never re-listed, so their data stands).
    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(
        entry_count(&conn),
        rows_before,
        "mid-rescan disconnect must not lose any prior rows (no truncate)"
    );
    assert_eq!(
        dir_size(&conn, "/"),
        root_size_before,
        "prior root size survives a mid-rescan disconnect"
    );

    writer.shutdown();
}

/// First scan (empty DB) is a fresh truncate+build, NOT a reconcile: the manager
/// chooses by entry-count, but at this layer we confirm `scan_volume_via_trait`
/// builds correctly from empty (the precondition the reconcile path relies on:
/// a populated DB). This pins that the two entry points produce the same index.
#[tokio::test]
async fn first_scan_builds_then_reconcile_is_a_no_op() {
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol)).await;

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    let built = entry_count(&conn);
    // 4 tree entries (sub, keep.txt, mod.txt, top.txt) + the ROOT_ID sentinel.
    assert_eq!(built, 5, "first scan built all 4 entries plus the root sentinel");

    // Immediately reconciling the same tree is a no-op (zero new rows).
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap();
    }
    let cancelled = CancellationToken::new();
    reconcile_volume_via_trait(
        vol,
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
    assert_eq!(entry_count(&conn), built, "reconcile after first scan adds no rows");

    writer.shutdown();
}

/// Count entries stamped at exactly `epoch` (the dirs this reconcile pass
/// successfully re-listed). A reconcile that descends the whole tree stamps
/// every dir; one that stops at the root stamps only the root.
fn dirs_listed_at_epoch(conn: &Connection, epoch: u64) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM entries WHERE is_directory = 1 AND listed_epoch = ?1",
        [epoch],
        |r| r.get(0),
    )
    .expect("count listed dirs")
}

/// THE regression test for the reported prod bug: a reconcile over an
/// already-partially-indexed share must DESCEND into every existing child
/// dir, not stop at the root after matching its children by name.
///
/// Setup mirrors prod (`naspi`): the DB knows the root + its top-level dirs
/// from an earlier interrupted scan, but those dirs are EMPTY in the index —
/// their real subtrees were never listed. The live volume has the full tree.
/// A child dir being "unchanged" at the root's level (same mtime → no UPSERT)
/// says NOTHING about whether its own subtree was ever scanned, so the
/// reconcile must recurse into it regardless.
///
/// Pre-fix (recursion gated on a change/upsert) this stamped only the root
/// and left every deep file missing — a green badge over an unscanned share.
#[tokio::test]
async fn reconcile_descends_into_existing_unchanged_child_dirs() {
    // Prior index: root + 3 top-level dirs, each EMPTY (the interrupted-scan
    // state). A fresh scan stamps these at epoch 1 with stable mtimes.
    let shallow = vec![
        entry("a", "/a", true, None),
        entry("b", "/b", true, None),
        entry("c", "/c", true, None),
    ];
    let vol_prior: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", shallow));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&vol_prior)).await;

    // The full live tree: the SAME 3 top dirs (unchanged → no UPSERT at the
    // root), now each holding a subdir with a deep file. 3 top dirs + 3
    // subdirs = 6 dirs total under the root, plus the root itself = 7 dirs.
    let full = vec![
        entry("a", "/a", true, None),
        entry("sub_a", "/a/sub_a", true, None),
        entry("deep_a.txt", "/a/sub_a/deep_a.txt", false, Some(11)),
        entry("b", "/b", true, None),
        entry("sub_b", "/b/sub_b", true, None),
        entry("deep_b.txt", "/b/sub_b/deep_b.txt", false, Some(22)),
        entry("c", "/c", true, None),
        entry("sub_c", "/c/sub_c", true, None),
        entry("deep_c.txt", "/c/sub_c/deep_c.txt", false, Some(33)),
    ];
    let vol_full: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", full));

    // A continuity break bumps the epoch before a rescan; mirror that so the
    // reconcile stamps re-listed dirs at the NEW epoch (distinct from epoch 1).
    let new_epoch = {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap()
    };

    let cancelled = CancellationToken::new();
    reconcile_volume_via_trait(
        vol_full,
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

    // The walk descended into EVERY dir: root + 3 top + 3 sub = 7 dirs, all
    // stamped at the new epoch. Pre-fix only the root (1) was stamped.
    assert_eq!(
        dirs_listed_at_epoch(&conn, new_epoch),
        7,
        "reconcile must re-list every dir (root + 3 top + 3 sub), not stop at the root"
    );

    // The deep files the prior index never had are now present and sized —
    // proof the recursion actually listed the subtrees, not just stamped them.
    for (path, size) in [
        ("/a/sub_a/deep_a.txt", 11u64),
        ("/b/sub_b/deep_b.txt", 22),
        ("/c/sub_c/deep_c.txt", 33),
    ] {
        let id = resolve_path(&conn, path)
            .expect("resolve")
            .unwrap_or_else(|| panic!("{path} should be indexed after reconcile descends"));
        let row = IndexStore::get_entry_by_id(&conn, id).expect("entry").expect("present");
        assert_eq!(row.logical_size, Some(size), "{path} reconciled with its real size");
    }

    // Recursive sizes rolled up through the descended tree: root = 11+22+33.
    assert_eq!(
        dir_size(&conn, "/"),
        66,
        "root recursive size reflects the deep files the reconcile descended to find"
    );

    writer.shutdown();
}

/// A reconcile rescan whose ROOT suddenly lists EMPTY (the share glitched or
/// the session is half-dead) must NOT report a clean completion: it returns
/// the typed `EmptyRoot` error so the prior (stale-but-real) index is kept
/// and never overwritten as falsely-complete-and-empty. Without this guard a
/// transient empty root strands the index as "complete" with zero entries.
#[tokio::test]
async fn reconcile_empty_root_does_not_complete() {
    // Start from a real, fully-scanned tree so the reconcile path runs over a
    // populated index.
    let populated: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", base_tree()));
    let (writer, db_path, _dir) = fresh_scan(Arc::clone(&populated)).await;

    let rows_before = {
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        entry_count(&conn)
    };
    assert!(rows_before > 0, "precondition: the index has data to reconcile against");

    // A continuity break bumps the epoch before a rescan; mirror that.
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap();
    }

    // Now reconcile against a volume whose root lists EMPTY (the glitch).
    let empty: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("Test", vec![]));
    let cancelled = CancellationToken::new();
    let result = reconcile_volume_via_trait(
        empty,
        PathBuf::from("/"),
        writer.clone(),
        progress(),
        cancelled,
        ScanPacer::unpaced(),
    )
    .await;

    match result {
        Err(VolumeScanError::EmptyRoot) => {}
        other => panic!("expected EmptyRoot from a reconcile whose root went empty, got {other:?}"),
    }
    writer.flush().await.expect("flush");

    // The prior index is untouched — reconcile wrote no changes and we bailed
    // before the diff/removal/marks, so the stale-but-real rows survive.
    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
    assert_eq!(
        entry_count(&conn),
        rows_before,
        "a glitched empty-root reconcile must not blank the prior index",
    );

    writer.shutdown();
}

/// THE regression test for the post-Forget SMB enable bug: a reconcile over an
/// EMPTY DB whose scan root is NOT `/` (the real case — an SMB share mounts at
/// `/Volumes/<share>`) must still DESCEND into every newly-discovered child
/// dir, fully indexing the multi-level tree.
///
/// The enable path routes a no-completion-marker DB through the reconcile walk;
/// post-Forget that DB is empty, so EVERY dir is "new". New dirs are resolved
/// after a flush to get their freshly-assigned ids before recursing. Resolving
/// by ABSOLUTE PATH (`/Volumes/naspi/_test`) walks component-by-component from
/// ROOT_ID, but the index root IS `/Volumes/naspi` (mapped to ROOT_ID) — so the
/// walk fails at the first component (`Volumes`) and resolves NOTHING. The
/// reconcile then stops at the root and falsely "completes" with only the
/// top-level entries (badge green, no real scan). Resolving by `(parent_id,
/// name)` is correct for any root. Pre-fix this assertion fails: only the root
/// and its immediate children are indexed, the subtrees are missing.
#[tokio::test]
async fn reconcile_from_empty_db_with_non_root_mount_indexes_full_tree() {
    // An SMB-shaped mount: root is `/Volumes/naspi`, with a multi-level tree.
    let root = PathBuf::from("/Volumes/naspi");
    let tree = vec![
        entry("top", "/Volumes/naspi/top", true, None),
        entry("sub", "/Volumes/naspi/top/sub", true, None),
        entry("deep.txt", "/Volumes/naspi/top/sub/deep.txt", false, Some(42)),
        entry("other", "/Volumes/naspi/other", true, None),
        entry("leaf.txt", "/Volumes/naspi/other/leaf.txt", false, Some(7)),
    ];
    let vol: Arc<dyn Volume> = Arc::new(InMemoryVolume::with_entries("naspi", tree));

    // Empty DB + writer (the post-Forget state). The manager bumps the epoch at
    // the scan-start funnel before spawning the walk; mirror that so listed dirs
    // stamp the bumped epoch.
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("reconcile-empty.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::indexing::NoopEventSink::shared()).expect("spawn writer");
    let new_epoch = {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        IndexStore::bump_current_epoch(&wconn).unwrap()
    };

    let cancelled = CancellationToken::new();
    reconcile_volume_via_trait(vol, root, writer.clone(), progress(), cancelled, ScanPacer::unpaced())
        .await
        .expect("reconcile from empty DB on a non-`/` mount");
    writer.flush().await.expect("flush");

    let conn = IndexStore::open_read_connection(&db_path).expect("read conn");

    // The walk descended into EVERY dir: root + top + top/sub + other = 4 dirs,
    // all stamped at the new epoch. Pre-fix only the root (1) was stamped.
    assert_eq!(
        dirs_listed_at_epoch(&conn, new_epoch),
        4,
        "reconcile must re-list every dir (root + top + top/sub + other), not stop at the root"
    );

    // The deep files prove recursion actually listed the subtrees rather than
    // just stamping the top level. Resolved by (parent_id, name) chains since
    // `resolve_path` from `/` can't reach a `/Volumes/naspi`-rooted index.
    let id_of = |parent: i64, name: &str| -> i64 {
        IndexStore::resolve_component(&conn, parent, name)
            .expect("resolve")
            .unwrap_or_else(|| panic!("{name} should be indexed after reconcile descends"))
    };
    let top = id_of(ROOT_ID, "top");
    let sub = id_of(top, "sub");
    let deep = id_of(sub, "deep.txt");
    let deep_row = IndexStore::get_entry_by_id(&conn, deep)
        .expect("entry")
        .expect("present");
    assert_eq!(
        deep_row.logical_size,
        Some(42),
        "deep.txt reconciled with its real size"
    );

    let other = id_of(ROOT_ID, "other");
    let leaf = id_of(other, "leaf.txt");
    let leaf_row = IndexStore::get_entry_by_id(&conn, leaf)
        .expect("entry")
        .expect("present");
    assert_eq!(leaf_row.logical_size, Some(7), "leaf.txt reconciled with its real size");

    writer.shutdown();
}

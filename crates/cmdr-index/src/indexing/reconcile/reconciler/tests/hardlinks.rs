//! Hardlink dedup: the diff must read the writer's NULL size as converged, or it
//! re-upserts every deduped row on every pass forever.

use super::*;

// ── Hardlink dedup: the diff must agree with the writer ────────
//
// The writer nulls the sizes of every occurrence of a multi-link inode past the
// first, so each inode's bytes count once. The diff has to read that NULL as the
// converged state, or it re-upserts the row on every pass forever (the writer
// re-nulls it, the next pass re-sends it): 393k rows on the production index,
// one WebKit cache directory alone re-walked 49 times in a day.

/// All children of `parent` as `(name, logical_size)`, sorted by name.
fn db_children_sizes(db_path: &Path, parent: &str) -> Vec<(String, Option<u64>)> {
    let store = IndexStore::open(db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), parent).unwrap().unwrap();
    let mut rows: Vec<(String, Option<u64>)> = store
        .list_children(parent_id)
        .unwrap()
        .into_iter()
        .map(|e| (e.name, e.logical_size))
        .collect();
    rows.sort();
    rows
}

/// Create `name` with `contents` plus `link_count - 1` hardlinks named
/// `<name>.link<n>`, and return the file's path.
fn write_hardlinked_file(dir: &Path, name: &str, contents: &str, link_count: usize) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, contents).unwrap();
    for n in 1..link_count {
        std::fs::hard_link(&path, dir.join(format!("{name}.link{n}"))).unwrap();
    }
    path
}

/// A deduped hardlink (DB sizes NULL by design) with an unchanged mtime must
/// produce ZERO writes on a repeat pass. This is the anti-oscillation contract:
/// the second pass over untouched input writes nothing at all.
#[test]
fn reconcile_deduped_hardlink_writes_nothing_on_a_repeat_pass() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().to_string_lossy().to_string();
    write_hardlinked_file(test_dir.path(), "payload.bin", "shared bytes", 2);
    ensure_path_in_db(&db_path, &parent, &writer);

    let cancelled = CancellationToken::new();
    let first = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    )
    .unwrap();
    assert_eq!(first.added, 2, "both links are new");
    writer.flush_blocking().unwrap();

    // The writer keeps the bytes on exactly one of the two rows.
    let rows = db_children_sizes(&db_path, &parent);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter().filter(|(_, size)| size.is_some()).count(),
        1,
        "exactly one occurrence of the inode carries its bytes: {rows:?}"
    );

    let second = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    )
    .unwrap();
    assert_eq!(
        (second.added, second.removed, second.updated),
        (0, 0, 0),
        "an unchanged pair of hardlinks must not be re-written"
    );
    writer.flush_blocking().unwrap();

    let third = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    )
    .unwrap();
    assert_eq!(third.updated, 0, "and it must stay converged");

    writer.flush_blocking().unwrap();
    assert_eq!(
        db_children_sizes(&db_path, &parent),
        rows,
        "repeat passes leave the deduped row exactly as the writer left it"
    );
    writer.shutdown();
}

/// A deduped hardlink whose mtime DID change on disk is still written: mtime is
/// the only signal left once the size comparison is skipped.
#[test]
fn reconcile_deduped_hardlink_with_a_new_mtime_is_written() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().to_string_lossy().to_string();
    write_hardlinked_file(test_dir.path(), "payload.bin", "shared bytes", 2);
    ensure_path_in_db(&db_path, &parent, &writer);

    let cancelled = CancellationToken::new();
    reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    )
    .unwrap();
    writer.flush_blocking().unwrap();

    // Backdate the DB mtime of the deduped (NULL-sized) row.
    let deduped_name = db_children_sizes(&db_path, &parent)
        .into_iter()
        .find(|(_, size)| size.is_none())
        .expect("one row is deduped")
        .0;
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        wconn
            .execute("UPDATE entries SET modified_at = 1 WHERE name = ?1", [&deduped_name])
            .unwrap();
    }

    let summary = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    )
    .unwrap();
    assert_eq!(summary.updated, 1, "a changed mtime still writes the deduped row");
    writer.flush_blocking().unwrap();

    let store = IndexStore::open(&db_path).unwrap();
    let parent_id = store::resolve_path(store.read_conn(), &parent).unwrap().unwrap();
    let row = store
        .list_children(parent_id)
        .unwrap()
        .into_iter()
        .find(|e| e.name == deduped_name)
        .expect("row survives");
    assert_ne!(row.modified_at, Some(1), "the real mtime is back");
    writer.shutdown();
}

/// Self-healing: when the other links go away the survivor is a plain one-link
/// file again, so its NULL row is stale and its real size must come back. Only
/// `nlink` says so; the mtime doesn't move when a link is unlinked.
#[test]
fn reconcile_restores_the_size_when_a_hardlink_drops_to_one_link() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().to_string_lossy().to_string();
    write_hardlinked_file(test_dir.path(), "payload.bin", "shared bytes", 2);
    ensure_path_in_db(&db_path, &parent, &writer);

    let cancelled = CancellationToken::new();
    reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    )
    .unwrap();
    writer.flush_blocking().unwrap();

    // Delete whichever link the writer sized, leaving the deduped one alone on disk.
    let (sized_name, _) = db_children_sizes(&db_path, &parent)
        .into_iter()
        .find(|(_, size)| size.is_some())
        .expect("one row is sized");
    std::fs::remove_file(test_dir.path().join(&sized_name)).unwrap();

    reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    )
    .unwrap();
    writer.flush_blocking().unwrap();

    let rows = db_children_sizes(&db_path, &parent);
    assert_eq!(rows.len(), 1, "only the survivor is left: {rows:?}");
    assert_eq!(
        rows[0].1,
        Some("shared bytes".len() as u64),
        "the survivor's real size is restored: {rows:?}"
    );
    writer.shutdown();
}

/// A multi-link row that DOES carry sizes keeps comparing on size: the NULL is
/// what marks the deduped occurrence, not `nlink` on its own.
#[test]
fn reconcile_sized_hardlink_still_compares_on_size() {
    let (writer, dir, conn) = setup_test_writer();
    let db_path = dir.path().join("test-reconciler.db");

    let test_dir = non_excluded_tempdir();
    let parent = test_dir.path().to_string_lossy().to_string();
    let path = write_hardlinked_file(test_dir.path(), "payload.bin", "shared bytes", 2);
    ensure_path_in_db(&db_path, &parent, &writer);

    // Seed both rows by hand: the first sized but WRONG, the second deduped.
    let meta = std::fs::symlink_metadata(&path).unwrap();
    let snap = extract_metadata(&meta, false, false);
    {
        let wconn = IndexStore::open_write_connection(&db_path).unwrap();
        let parent_id = store::resolve_path(&wconn, &parent).unwrap().unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            parent_id,
            "payload.bin",
            false,
            false,
            Some(999),
            Some(999),
            snap.modified_at,
            snap.inode,
        )
        .unwrap();
        IndexStore::insert_entry_v2(
            &wconn,
            parent_id,
            "payload.bin.link1",
            false,
            false,
            None,
            None,
            snap.modified_at,
            snap.inode,
        )
        .unwrap();
        let db_next_id = IndexStore::get_next_id(&wconn).unwrap();
        writer.next_id().fetch_max(db_next_id, Ordering::Relaxed);
    }

    let cancelled = CancellationToken::new();
    let summary = reconcile_subtree(
        test_dir.path(),
        &IndexPathSpace::root(),
        &conn,
        &writer,
        &cancelled,
        None,
    )
    .unwrap();
    assert_eq!(summary.updated, 1, "only the wrongly-sized row is re-written");
    writer.flush_blocking().unwrap();

    assert_eq!(
        db_children_sizes(&db_path, &parent),
        vec![
            ("payload.bin".to_string(), Some("shared bytes".len() as u64)),
            ("payload.bin.link1".to_string(), None),
        ],
        "the sized row is corrected, the deduped row stays NULL"
    );
    writer.shutdown();
}

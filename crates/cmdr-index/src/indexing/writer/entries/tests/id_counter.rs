//! Entry-ID counter self-healing: an id counter that drifted behind the table
//! resyncs and retries, while a name conflict must not reassign an id.

use super::*;

/// The shared `next_id` counter can fall behind the table's real `MAX(id)`
/// (a second process writing the same DB, a restart racing an in-flight batch).
/// The allocated id then hits `SQLITE_CONSTRAINT_PRIMARYKEY` and the live upsert
/// used to drop the file from the index silently, for every following insert too
/// (one incident: ~9,600 warnings in seconds). The handler must resync from the
/// DB and retry, so the entry lands.
#[test]
fn upsert_heals_an_id_counter_that_drifted_behind_the_table() {
    let (db_path, _dir) = setup_db();
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    IndexStore::insert_entries_v2_batch(&conn, &[seed_row(40, "a.txt"), seed_row(41, "b.txt")]).unwrap();

    // The counter still points at 40: ids 40 and 41 are already taken.
    let next_id = AtomicI64::new(40);
    let mutation_tracker = MutationTracker::new(true);
    let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());

    handle_upsert_entry_v2(
        &conn,
        ROOT_ID,
        "fresh.txt".into(),
        false,
        false,
        Some(7),
        Some(7),
        None,
        None,
        None,
        &next_id,
        &mutation_tracker,
        true,
        &DeferredRepairs::new(),
        &signal,
    );

    let landed = IndexStore::resolve_component(&conn, ROOT_ID, "fresh.txt")
        .unwrap()
        .expect("a new file must land in the index even when the ID counter drifted behind the table");
    assert!(
        landed > 41,
        "the retry must use a fresh id past the table's MAX, got {landed}"
    );
    assert!(
        next_id.load(Ordering::Relaxed) > landed,
        "the counter must end up past the id it just used, so the next insert doesn't collide again"
    );
    assert_eq!(
        IndexStore::get_entry_by_id(&conn, landed).unwrap().unwrap().name,
        "fresh.txt"
    );
}

/// A `(parent_id, name_folded)` UNIQUE conflict (2067) is a genuinely different
/// situation from an id collision (1555): the name is already in the table.
/// Retrying it with a fresh id would insert a duplicate row, so it must NOT heal.
/// Reachable when another writer inserts the name between `resolve_component`
/// and the insert, which is why this drives `upsert_insert_new` directly.
#[test]
fn upsert_does_not_reassign_an_id_on_a_name_conflict() {
    let (db_path, _dir) = setup_db();
    let conn = IndexStore::open_write_connection(&db_path).unwrap();
    IndexStore::insert_entries_v2_batch(&conn, &[seed_row(5, "dup.txt")]).unwrap();

    let next_id = AtomicI64::new(6);
    let signal = IndexFailureSignal::new(crate::NoopEventSink::shared());

    upsert_insert_new(
        &conn,
        ROOT_ID,
        "dup.txt",
        false,
        false,
        Some(9),
        Some(9),
        None,
        None,
        false,
        &next_id,
        true,
        &DeferredRepairs::new(),
        &signal,
    );

    let children = IndexStore::list_children_on(ROOT_ID, &conn).unwrap();
    assert_eq!(
        children.len(),
        1,
        "a name conflict must not produce a second row under a fresh id"
    );
    assert_eq!(children[0].id, 5, "the original row must be the one that stays");
}

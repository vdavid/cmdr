//! The FK actions of the spine, asserted in raw SQL.

use super::migrated_conn;

/// Deleting a conversation NULLs its sweep's link and deletes NOTHING: the decision
/// record outlives the chat that produced it. Asserted in raw SQL, because the FK action
/// is the thing under test, not a Rust wrapper around it.
#[test]
fn deleting_a_conversation_nulls_the_sweep_link_and_deletes_nothing() {
    let conn = migrated_conn();
    conn.execute(
        "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (7, 't', 100, 100)",
        [],
    )
    .expect("conversation");
    conn.execute(
        "INSERT INTO proposal_sets (id, conversation_id, created_at) VALUES (1, 7, 100)",
        [],
    )
    .expect("sweep");
    conn.execute(
        "INSERT INTO proposals (id, set_id, seq, verb, status, source_volume_id, reversible, display_name, created_at)
         VALUES (1, 1, 0, 'trash', 'pending', 'root', 'restore_move', 'three files', 100)",
        [],
    )
    .expect("group");
    conn.execute(
        "INSERT INTO proposal_ops (id, group_id, seq, source_path, status, created_at)
         VALUES (1, 1, 0, '/a/b.txt', 'pending', 100)",
        [],
    )
    .expect("op");

    conn.execute("DELETE FROM conversations WHERE id = 7", [])
        .expect("delete conversation");

    let link: Option<i64> = conn
        .query_row("SELECT conversation_id FROM proposal_sets WHERE id = 1", [], |row| {
            row.get(0)
        })
        .expect("the sweep survives");
    assert_eq!(link, None, "the link is nulled, never cascaded");

    let groups: i64 = conn
        .query_row("SELECT COUNT(*) FROM proposals", [], |row| row.get(0))
        .expect("count groups");
    let ops: i64 = conn
        .query_row("SELECT COUNT(*) FROM proposal_ops", [], |row| row.get(0))
        .expect("count ops");
    assert_eq!((groups, ops), (1, 1), "the group and its ops stay put");
}

/// A group and its ops go with their sweep: the sweep is the unit of provenance, so an
/// orphan group would be a decision record nothing explains.
#[test]
fn deleting_a_sweep_cascades_to_its_groups_and_ops() {
    let conn = migrated_conn();
    conn.execute("INSERT INTO proposal_sets (id, created_at) VALUES (1, 100)", [])
        .expect("sweep");
    conn.execute(
        "INSERT INTO proposals (id, set_id, seq, verb, status, source_volume_id, reversible, display_name, created_at)
         VALUES (1, 1, 0, 'move', 'pending', 'root', 'restore_move', 'five files', 100)",
        [],
    )
    .expect("group");
    conn.execute(
        "INSERT INTO proposal_ops (id, group_id, seq, source_path, status, created_at)
         VALUES (1, 1, 0, '/a/b.txt', 'pending', 100)",
        [],
    )
    .expect("op");

    conn.execute("DELETE FROM proposal_sets WHERE id = 1", [])
        .expect("delete sweep");

    let groups: i64 = conn
        .query_row("SELECT COUNT(*) FROM proposals", [], |row| row.get(0))
        .expect("count groups");
    let ops: i64 = conn
        .query_row("SELECT COUNT(*) FROM proposal_ops", [], |row| row.get(0))
        .expect("count ops");
    assert_eq!((groups, ops), (0, 0), "the whole sweep goes");
}

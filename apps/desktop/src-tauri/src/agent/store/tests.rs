//! Store lifecycle tests: the migration ladder (the risky, template-mirrored logic), the
//! open→recreate/refuse behavior, and the token round-trips. The conversation / message /
//! FTS / cost query round-trips live in `query/tests.rs`.

use std::collections::HashSet;

use rusqlite::{Connection, Transaction};

use super::migrations::read_schema_version;
use super::*;
use crate::agent::llm::types::{AgentRole, ProviderTag};
use crate::agent::types::ConversationOrigin;

// ── Synthetic ladders, so the runner mechanism is tested independent of the production
//    schema. `widget` is a throwaway table these steps evolve. ──

fn v1_create_widget(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("CREATE TABLE widget (id INTEGER PRIMARY KEY, a TEXT NOT NULL);")
}

fn v2_add_column(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("ALTER TABLE widget ADD COLUMN b TEXT NOT NULL DEFAULT 'backfilled';")
}

const LADDER_V1: &[Migration] = &[Migration {
    version: 1,
    description: "create widget",
    up: v1_create_widget,
}];

const LADDER_V2: &[Migration] = &[
    Migration {
        version: 1,
        description: "create widget",
        up: v1_create_widget,
    },
    Migration {
        version: 2,
        description: "add widget.b",
        up: v2_add_column,
    },
];

/// A v1→v2 forward migration bumps the version stepwise AND preserves the rows written
/// under v1 (backfilling the new column) — the whole point of a ladder over
/// delete-and-recreate: the user's chat history survives a schema change.
#[test]
fn forward_migration_preserves_rows_and_bumps_version() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    run_migrations(&conn, LADDER_V1).expect("migrate to v1");
    assert_eq!(read_schema_version(&conn).expect("version"), 1);
    conn.execute("INSERT INTO widget (id, a) VALUES (1, 'hi')", [])
        .expect("insert v1 row");

    run_migrations(&conn, LADDER_V2).expect("migrate to v2");
    assert_eq!(read_schema_version(&conn).expect("version"), 2);

    let (a, b): (String, String) = conn
        .query_row("SELECT a, b FROM widget WHERE id = 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .expect("row survived the migration");
    assert_eq!(a, "hi", "the pre-migration value is preserved");
    assert_eq!(b, "backfilled", "the new column is backfilled by the step");
}

/// Re-running the same ladder is a no-op: the version stays put and nothing re-applies.
#[test]
fn re_running_the_ladder_is_a_noop() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    run_migrations(&conn, LADDER_V2).expect("migrate to v2");
    run_migrations(&conn, LADDER_V2).expect("second run is a no-op");
    assert_eq!(read_schema_version(&conn).expect("version"), 2);
}

/// A stored version NEWER than the ladder (a downgrade) is refused, never destroyed — the
/// newer DB may hold data this build can't represent.
#[test]
fn downgrade_is_refused_not_destroyed() {
    let conn = Connection::open_in_memory().expect("in-memory db");
    run_migrations(&conn, LADDER_V2).expect("migrate to v2");
    conn.execute("INSERT INTO widget (id, a) VALUES (7, 'precious')", [])
        .expect("insert");

    let err = run_migrations(&conn, LADDER_V1).expect_err("a downgrade must be refused");
    assert!(
        matches!(err, AgentStoreError::SchemaDowngrade { found: 2, expected: 1 }),
        "expected a typed SchemaDowngrade(found=2, expected=1), got {err:?}"
    );

    let a: String = conn
        .query_row("SELECT a FROM widget WHERE id = 7", [], |row| row.get(0))
        .expect("row still there");
    assert_eq!(a, "precious", "a refused downgrade must not destroy the DB");
}

/// A genuinely unparseable file (garbage bytes) is deleted and recreated fresh through
/// `AgentStore::open` — the ONLY case where destroy-and-recreate is correct. The recreated
/// DB is at the current schema version with the real tables present.
#[test]
fn unparseable_file_recreates_fresh() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = main_db_path(dir.path());
    std::fs::write(&path, b"this is not a sqlite database at all").expect("write garbage");

    let store = AgentStore::open(&path).expect("open recreates an unparseable file");
    assert_eq!(
        store.schema_version().expect("version"),
        MIGRATIONS.last().expect("ladder non-empty").version,
        "the recreated DB is at the current schema version"
    );
    let count: i64 = store
        .conn()
        .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
        .expect("the conversations table exists and reads");
    assert_eq!(count, 0, "the recreated DB has the real schema and no rows");
}

/// A fresh `AgentStore::open` on a new path builds the full production schema at the
/// current version, including the FTS5 virtual table.
#[test]
fn fresh_open_builds_current_schema() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = main_db_path(dir.path());
    let store = AgentStore::open(&path).expect("open fresh");
    assert_eq!(
        store.schema_version().expect("version"),
        MIGRATIONS.last().expect("ladder non-empty").version
    );
    // The FTS5 virtual table is queryable — a MATCH against the empty index returns no
    // rows rather than erroring, proving the `fts5` feature is compiled in.
    let hits: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'anything'",
            [],
            |row| row.get(0),
        )
        .expect("messages_fts is a live FTS5 table");
    assert_eq!(hits, 0);
}

// ── Token round-trips + uniqueness ─────────────────────────────────────────────

/// Assert every token round-trips through `as_token`/`from_token` and the tokens are
/// distinct — the invariant that keeps the two directions from drifting.
fn assert_token_round_trip<T>(variants: &[T], token_of: impl Fn(&T) -> &'static str, parse: impl Fn(&str) -> Option<T>)
where
    T: std::fmt::Debug + PartialEq,
{
    let mut seen = HashSet::new();
    for variant in variants {
        let token = token_of(variant);
        assert!(seen.insert(token), "duplicate token {token:?}");
        assert_eq!(
            parse(token).as_ref(),
            Some(variant),
            "token {token:?} must round-trip to its variant"
        );
    }
    assert!(parse("nope-not-a-token").is_none(), "an unknown token parses to None");
}

#[test]
fn conversation_origin_tokens_round_trip() {
    assert_token_round_trip(
        &[ConversationOrigin::Notification],
        |o| o.as_token(),
        ConversationOrigin::from_token,
    );
}

#[test]
fn provider_tag_tokens_round_trip_and_are_unique() {
    assert_token_round_trip(&ProviderTag::ALL, |p| p.as_token(), ProviderTag::from_token);
}

#[test]
fn agent_role_tokens_round_trip_and_are_unique() {
    let roles = [
        AgentRole::System,
        AgentRole::User,
        AgentRole::Assistant,
        AgentRole::Tool,
    ];
    assert_token_round_trip(&roles, |r| r.as_token(), AgentRole::from_token);
}

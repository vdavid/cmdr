//! Store lifecycle tests: the migration ladder (the risky, template-mirrored logic), the
//! open→recreate/refuse behavior, and the token round-trips. The conversation / message /
//! FTS / cost query round-trips live in `query/tests.rs`.

use std::collections::HashSet;

use rusqlite::Transaction;

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
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
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
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    run_migrations(&conn, LADDER_V2).expect("migrate to v2");
    run_migrations(&conn, LADDER_V2).expect("second run is a no-op");
    assert_eq!(read_schema_version(&conn).expect("version"), 2);
}

/// A stored version NEWER than the ladder (a downgrade) is refused, never destroyed — the
/// newer DB may hold data this build can't represent.
#[test]
fn downgrade_is_refused_not_destroyed() {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
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

/// A genuinely unparseable file (garbage bytes) is set aside and recreated fresh through
/// `AgentStore::open` — the ONLY case where recreating is correct. The recreated
/// DB is at the current schema version with the real tables present.
#[test]
fn unparseable_file_recreates_fresh() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = main_db_path(dir.path());
    std::fs::write(&path, b"this is not a sqlite database at all").expect("write garbage");

    let notices = crate::state_recovery::testing::capture_recoveries();
    let store = AgentStore::open(&path).expect("open recreates an unparseable file");
    let kept = crate::state_recovery::testing::kept_copies(&path);
    assert_eq!(kept.len(), 1, "the unreadable DB is kept beside the new one: {kept:?}");
    assert_eq!(
        std::fs::read(&kept[0]).expect("read the copy"),
        b"this is not a sqlite database at all"
    );
    let recoveries = notices.recoveries();
    assert_eq!(recoveries.len(), 1, "one notice");
    assert_eq!(recoveries[0].file, crate::state_recovery::StateFile::ChatHistory);
    assert_eq!(
        store.schema_version().expect("version"),
        MIGRATIONS.last().expect("ladder non-empty").version,
        "the recreated DB is at the current schema version"
    );
    let threads = list_conversations(store.conn(), 50, 0, true).expect("the conversations table exists and reads");
    assert!(
        threads.is_empty(),
        "the recreated DB has the real schema and no threads"
    );
    quiet_wakes_conversation(store.conn()).expect("only the reserved cost-ledger rows, which nothing lists");
    about_this_conversation(store.conn()).expect("the verdict's ledger row too");
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

/// A v1 database (the shipped initial schema) with an existing conversation migrates
/// forward to the current version, gaining `last_model` as NULL without touching rows.
#[test]
fn production_v1_db_migrates_forward_gaining_last_model() {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    run_migrations(&conn, &MIGRATIONS[..1]).expect("run v1 only");
    conn.execute(
        "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (1, 'old thread', 10, 10)",
        [],
    )
    .expect("insert v1 row");

    run_migrations(&conn, MIGRATIONS).expect("migrate to current");

    let last_model: Option<String> = conn
        .query_row("SELECT last_model FROM conversations WHERE id = 1", [], |row| {
            row.get(0)
        })
        .expect("last_model column exists after the migration");
    assert_eq!(last_model, None, "pre-existing rows read as no recorded model");
}

/// A v11 database's groups gain `source_folder` once, from their stored paths: one folder for a
/// group that has one, NULL for a group that spans folders, so the review names the right folder
/// without streaming paths on every refresh.
#[test]
fn a_v11_db_backfills_each_groups_shared_source_folder() {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    run_migrations(&conn, &MIGRATIONS[..11]).expect("run up to v11");
    conn.execute_batch(
        "INSERT INTO proposal_sets (id, created_at) VALUES (1, 10);
         INSERT INTO proposals (id, set_id, seq, verb, status, source_volume_id, reversible, display_name, created_at)
             VALUES (1, 1, 0, 'trash', 'pending', 'root', 'restore_move', 'one folder', 10),
                    (2, 1, 1, 'trash', 'pending', 'root', 'restore_move', 'two folders', 10);
         INSERT INTO proposal_ops (group_id, seq, source_path, status, created_at) VALUES
             (1, 0, '/Users/someone/Downloads/a.dmg', 'pending', 10),
             (1, 1, '/Users/someone/Downloads/b.dmg', 'pending', 10),
             (2, 0, '/Users/someone/Downloads/a.dmg', 'pending', 10),
             (2, 1, '/Users/someone/Desktop/b.dmg', 'pending', 10);",
    )
    .expect("insert v11 rows");

    run_migrations(&conn, MIGRATIONS).expect("migrate to current");

    let folder = |id: i64| -> Option<String> {
        conn.query_row("SELECT source_folder FROM proposals WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .expect("source_folder exists")
    };
    assert_eq!(folder(1).as_deref(), Some("/Users/someone/Downloads"));
    assert_eq!(folder(2), None, "a group spanning folders names none");
}

/// A thread reopened after a restart must show its last known context usage rather than an
/// empty gauge, so the figures live with the conversation. An older thread that never
/// recorded one reads as "not measured yet", never as zero usage.
#[test]
fn a_conversations_context_usage_round_trips_and_is_absent_before_a_turn() {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    run_migrations(&conn, MIGRATIONS).expect("migrate to current");
    let id = create_conversation(&conn, "sizing", 10, None).expect("create conversation");

    assert_eq!(
        conversation_context_usage(&conn, id).expect("read usage"),
        None,
        "a thread with no completed turn has no usage to report"
    );

    set_conversation_context_usage(&conn, id, 31_200, 60_000).expect("record usage");

    assert_eq!(
        conversation_context_usage(&conn, id).expect("read usage"),
        Some((31_200, 60_000)),
        "the reopened thread reports the last turn's prompt against its budget"
    );
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
        &[
            ConversationOrigin::Notification,
            ConversationOrigin::QuietWakes,
            ConversationOrigin::AboutThis,
        ],
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

/// A `TurnFinished` row written before turns stored whether they were stopped still loads, as a
/// turn that wasn't: its rows can't tell a stop apart, so the footer claims nothing.
#[test]
fn a_turn_finished_row_from_before_stopped_was_stored_loads_as_not_stopped() {
    let old = r#"{"kind":"turnFinished","stats":{"toolCalls":2,"wallMs":900,"promptTokens":40,"completionTokens":4}}"#;
    let event: ConversationEvent = serde_json::from_str(old).expect("an old row still parses");
    let ConversationEvent::TurnFinished { stats } = event else {
        panic!("expected TurnFinished, got {event:?}");
    };
    assert_eq!((stats.tool_calls, stats.stopped), (2, false));
}

/// Cloud AI consent round-trips through the `meta` table: absent → recorded (version +
/// timestamp) → cleared. A partial/absent record reads as no consent, so the gate stays closed.
#[test]
fn cloud_ai_consent_round_trips() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = AgentStore::open(&main_db_path(dir.path())).expect("open");
    let conn = store.conn();

    assert!(
        get_consent(conn, ConsentRecord::CloudAi).expect("read").is_none(),
        "no consent on a fresh DB"
    );

    set_cloud_ai_consent(conn, 1, 1_760_000_000).expect("record consent");
    let recorded = get_consent(conn, ConsentRecord::CloudAi)
        .expect("read")
        .expect("consent present");
    assert_eq!(recorded.version, 1);
    assert_eq!(recorded.at, 1_760_000_000);

    // Re-accepting a newer copy version overwrites in place.
    set_cloud_ai_consent(conn, 2, 1_760_000_100).expect("re-record");
    assert_eq!(
        get_consent(conn, ConsentRecord::CloudAi)
            .expect("read")
            .expect("present")
            .version,
        2
    );

    clear_cloud_ai_consent(conn).expect("clear");
    assert!(
        get_consent(conn, ConsentRecord::CloudAi).expect("read").is_none(),
        "cleared consent reads absent"
    );
}

/// The legacy Ask Cmdr record is a separate pair of keys that nothing writes any more: recording
/// and clearing cloud consent leaves it exactly as it was, and the legacy read still sees it (the
/// one-time Ask Cmdr on/off mapping depends on that).
#[test]
fn cloud_ai_consent_leaves_the_legacy_ask_cmdr_record_untouched() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = AgentStore::open(&main_db_path(dir.path())).expect("open");
    let conn = store.conn();

    set_legacy_ask_cmdr_consent_for_tests(conn, 4, 1_750_000_000);
    assert!(
        get_consent(conn, ConsentRecord::CloudAi).expect("read").is_none(),
        "an Ask Cmdr opt-in grants no cloud consent"
    );

    set_cloud_ai_consent(conn, 1, 1_760_000_000).expect("record cloud consent");
    clear_cloud_ai_consent(conn).expect("clear cloud consent");

    let legacy = get_consent(conn, ConsentRecord::AskCmdrLegacy)
        .expect("read")
        .expect("legacy record still there");
    assert_eq!(legacy.version, 4);
    assert_eq!(legacy.at, 1_750_000_000);
}

/// The per-conversation cost total sums across days/models, ANDs the priced flag (any
/// unpriced contribution ⇒ not fully priced), and lists the distinct providers — the
/// honest miss-path the footer renders.
#[test]
fn conversation_cost_sums_and_flags_unpriced() {
    use crate::agent::llm::types::ProviderTag;

    let dir = tempfile::tempdir().expect("temp dir");
    let store = AgentStore::open(&main_db_path(dir.path())).expect("open");
    let conn = store.conn();
    let id = create_conversation(conn, "t", 1_760_000_000, None).expect("create");

    // A priced local turn (free) plus an unpriced cloud turn (unknown model).
    record_cost(
        conn,
        &CostRecord {
            day: "2026-07-13".to_string(),
            conversation_id: id,
            provider: ProviderTag::Local,
            model: "local-model".to_string(),
            prompt_tokens: 100,
            completion_tokens: 50,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_micros: 0,
            priced: true,
        },
    )
    .expect("record local");
    record_cost(
        conn,
        &CostRecord {
            day: "2026-07-13".to_string(),
            conversation_id: id,
            provider: ProviderTag::OpenAi,
            model: "some-unpriced-model".to_string(),
            prompt_tokens: 200,
            completion_tokens: 20,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_micros: 0,
            priced: false,
        },
    )
    .expect("record cloud");

    let cost = conversation_cost(conn, id).expect("cost");
    assert_eq!(cost.prompt_tokens, 300, "prompt tokens sum across turns");
    assert_eq!(cost.completion_tokens, 70);
    assert!(
        !cost.fully_priced,
        "an unpriced turn makes the whole thread not fully priced"
    );
    assert!(cost.providers.contains(&ProviderTag::Local));
    assert!(cost.providers.contains(&ProviderTag::OpenAi));

    // A thread with no metered turn reads zeroed and fully priced (nothing unknown yet).
    let empty_id = create_conversation(conn, "empty", 1_760_000_000, None).expect("create empty");
    let empty = conversation_cost(conn, empty_id).expect("cost");
    assert_eq!(empty.prompt_tokens, 0);
    assert!(empty.fully_priced);
    assert!(empty.providers.is_empty());
}

/// Read-only connections open with the smaller page cache, write connections
/// with the bigger one. Both are upper bounds drawn from the process-wide slab;
/// budgets and rationale: `crate::sqlite_util`.
#[test]
fn read_connections_get_a_smaller_page_cache_than_write_connections() {
    use crate::sqlite_util::{READ_PAGE_CACHE_KIB, WRITE_PAGE_CACHE_KIB, page_cache_kib};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = main_db_path(dir.path());
    let write = open_write_connection(&path).expect("write conn");
    let read = open_read_connection(&path).expect("read conn");

    assert_eq!(page_cache_kib(&write), WRITE_PAGE_CACHE_KIB);
    assert_eq!(page_cache_kib(&read), READ_PAGE_CACHE_KIB);
}

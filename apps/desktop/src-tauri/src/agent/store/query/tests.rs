//! Query-layer round-trips: conversation CRUD, message append + seq, the FTS5 trigger
//! sync (insert/update/delete), cross-thread search + ranking, the input sanitizer, and
//! the cost meter's accumulating upsert + per-day rollup.

use rusqlite::Connection;

use super::*;
use crate::agent::llm::types::{AgentPart, AgentRole, AgentToolCall, ProviderTag, ReasoningState, ToolId};
use crate::agent::types::ConversationOrigin;

/// An in-memory `main.db` at the current schema. FTS5 works in-memory, so this exercises
/// the real triggers and query paths without a temp file.
fn migrated_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    super::super::run_migrations(&conn, super::super::MIGRATIONS).expect("migrate");
    conn
}

/// Append a plain user-text message (the common case) with an explicit clock.
fn append_user_text(conn: &Connection, conversation_id: i64, text: &str, now: i64) -> (i64, i64) {
    append_message(
        conn,
        conversation_id,
        AgentRole::User,
        &[AgentPart::Text(text.to_string())],
        text,
        None,
        None,
        now,
    )
    .expect("append message")
}

// ── Conversations ────────────────────────────────────────────────────────────

#[test]
fn conversations_list_newest_activity_first_and_filter_archived() {
    let conn = migrated_conn();
    let a = create_conversation(&conn, "Alpha", 100, None).expect("create a");
    let b = create_conversation(&conn, "Beta", 200, None).expect("create b");

    // Activity on `a` after `b` was created makes `a` the most recent.
    append_user_text(&conn, a, "hi", 300);

    let list = list_conversations(&conn, 50, 0, false).expect("list");
    let ids: Vec<i64> = list.iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![a, b], "newest updated_at first");

    // Archiving `a` drops it from the default (non-archived) list, keeps it when included.
    archive_conversation(&conn, a, true).expect("archive");
    let visible = list_conversations(&conn, 50, 0, false).expect("list visible");
    assert_eq!(visible.iter().map(|c| c.id).collect::<Vec<_>>(), vec![b]);
    let all = list_conversations(&conn, 50, 0, true).expect("list all");
    assert_eq!(all.len(), 2, "archived rows appear when included");
    assert!(all.iter().find(|c| c.id == a).expect("a present").archived);
}

#[test]
fn conversation_origin_round_trips_through_the_column() {
    let conn = migrated_conn();
    let user = create_conversation(&conn, "user thread", 10, None).expect("create");
    let spawned =
        create_conversation(&conn, "spawned", 20, Some(ConversationOrigin::Notification)).expect("create origin");

    let detail = get_conversation(&conn, user, 10, 0).expect("get").expect("present");
    assert_eq!(detail.conversation.origin, None, "user-started ⇒ NULL origin");

    let detail = get_conversation(&conn, spawned, 10, 0).expect("get").expect("present");
    assert_eq!(detail.conversation.origin, Some(ConversationOrigin::Notification));
}

#[test]
fn rename_changes_title_without_reordering() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "old", 100, None).expect("create");
    append_user_text(&conn, id, "hi", 200);
    rename_conversation(&conn, id, "new title").expect("rename");

    let detail = get_conversation(&conn, id, 10, 0).expect("get").expect("present");
    assert_eq!(detail.conversation.title, "new title");
    assert_eq!(detail.conversation.updated_at, 200, "rename must not bump updated_at");
}

// ── Messages ─────────────────────────────────────────────────────────────────

#[test]
fn append_message_assigns_per_conversation_seq_and_bumps_updated_at() {
    let conn = migrated_conn();
    let a = create_conversation(&conn, "A", 1, None).expect("create a");
    let b = create_conversation(&conn, "B", 1, None).expect("create b");

    let (_, s0) = append_user_text(&conn, a, "first", 10);
    let (_, s1) = append_user_text(&conn, a, "second", 20);
    let (_, s0b) = append_user_text(&conn, b, "b-first", 30);
    assert_eq!((s0, s1), (0, 1), "seq is per-conversation and monotonic");
    assert_eq!(s0b, 0, "a second conversation starts its own seq at 0");

    let detail = get_conversation(&conn, a, 50, 0).expect("get").expect("present");
    assert_eq!(
        detail.conversation.updated_at, 20,
        "updated_at follows the latest message"
    );
    assert_eq!(detail.total_messages, 2);
}

#[test]
fn content_blocks_round_trip_the_opaque_reasoning_blob_through_the_db() {
    // A tool call carrying provider reasoning state persists to `content_blocks` JSON and
    // reads back byte-for-byte — the invariant the typed-parts model protects, now proven
    // across the DB boundary (never flatten to content+reasoning strings).
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    let parts = vec![AgentPart::ToolCall(AgentToolCall {
        call_id: "call-1".into(),
        tool: ToolId::AppState,
        arguments: serde_json::json!({ "path": "/Users/x" }),
        reasoning: Some(ReasoningState {
            provider: ProviderTag::Gemini,
            blob: serde_json::json!({ "thought_signatures": ["sig-abc"] }),
        }),
    })];
    append_message(&conn, id, AgentRole::Assistant, &parts, "", Some(12), Some(34), 100).expect("append");

    let detail = get_conversation(&conn, id, 10, 0).expect("get").expect("present");
    let msg = &detail.messages[0];
    assert_eq!(msg.role, AgentRole::Assistant);
    assert_eq!(msg.parts, parts, "parts round-trip through the DB untouched");
    assert_eq!((msg.prompt_tokens, msg.completion_tokens), (Some(12), Some(34)));
}

#[test]
fn large_transcript_pages_without_overlap_or_gap() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "big", 1, None).expect("create");
    for i in 0..250 {
        append_user_text(&conn, id, &format!("message {i}"), 100 + i);
    }
    let detail = get_conversation(&conn, id, 100, 0).expect("get").expect("present");
    assert_eq!(detail.total_messages, 250);
    assert_eq!(detail.messages.len(), 100);

    // Walk the pages by offset = entries so far; seqs must be a contiguous 0..250.
    let mut seqs = Vec::new();
    let mut offset = 0;
    loop {
        let page = list_messages(&conn, id, 100, offset).expect("page");
        if page.is_empty() {
            break;
        }
        seqs.extend(page.iter().map(|m| m.seq));
        offset += page.len() as u32;
    }
    assert_eq!(
        seqs,
        (0..250).collect::<Vec<_>>(),
        "pages tile the transcript with no gap or overlap"
    );
}

// ── FTS5 trigger sync ──────────────────────────────────────────────────────────

fn search_ids(conn: &Connection, query: &str) -> Vec<i64> {
    search_conversations(conn, query, 50, 0)
        .expect("search")
        .into_iter()
        .map(|h| h.conversation_id)
        .collect()
}

#[test]
fn fts_insert_trigger_indexes_new_messages() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    append_user_text(&conn, id, "the quarterly budget spreadsheet", 10);
    assert_eq!(search_ids(&conn, "budget"), vec![id], "insert is indexed");
    assert!(search_ids(&conn, "mortgage").is_empty(), "an unrelated term misses");
}

#[test]
fn fts_update_trigger_reindexes_edited_text() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    let (message_id, _) = append_user_text(&conn, id, "original tomato text", 10);
    assert_eq!(search_ids(&conn, "tomato"), vec![id]);

    // Edit the indexed column directly to exercise the AFTER UPDATE trigger.
    conn.execute(
        "UPDATE messages SET text_for_search = 'replaced potato text' WHERE id = ?1",
        rusqlite::params![message_id],
    )
    .expect("update");
    assert!(
        search_ids(&conn, "tomato").is_empty(),
        "the old term is de-indexed on edit"
    );
    assert_eq!(search_ids(&conn, "potato"), vec![id], "the new term is indexed on edit");
}

/// Count index entries directly in the FTS table (rowid only, so it reads the index, not
/// the deleted content row). This is what actually proves de-indexing: `search_ids` joins
/// the match back to `messages`, so a deleted message can't join and an ORPHAN index row
/// would be silently masked — the exact external-content FTS desync the plan flags as the
/// top DB risk. Asserting on the index directly catches a broken delete trigger.
fn fts_index_hits(conn: &Connection, term: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH ?1",
        rusqlite::params![term],
        |row| row.get(0),
    )
    .expect("count fts index")
}

#[test]
fn fts_delete_trigger_deindexes_removed_messages() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    let (message_id, _) = append_user_text(&conn, id, "ephemeral kumquat note", 10);
    assert_eq!(fts_index_hits(&conn, "kumquat"), 1, "the message is indexed");

    conn.execute("DELETE FROM messages WHERE id = ?1", rusqlite::params![message_id])
        .expect("delete");
    assert_eq!(
        fts_index_hits(&conn, "kumquat"),
        0,
        "a deleted message leaves NO orphan index row (checked on the index itself)"
    );
    // And the user-facing search agrees.
    assert!(search_ids(&conn, "kumquat").is_empty());
}

// ── Search ranking ────────────────────────────────────────────────────────────

#[test]
fn search_returns_matching_threads_recent_first() {
    let conn = migrated_conn();
    let a = create_conversation(&conn, "A", 1, None).expect("create a");
    let b = create_conversation(&conn, "B", 1, None).expect("create b");
    append_user_text(&conn, a, "shared invoice topic", 100);
    append_user_text(&conn, b, "shared invoice topic", 200);
    // `b`'s matching message is newer, so `b` ranks first.
    assert_eq!(search_ids(&conn, "invoice"), vec![b, a]);

    // A newer matching message in `a` flips the order.
    append_user_text(&conn, a, "another invoice mention", 300);
    assert_eq!(
        search_ids(&conn, "invoice"),
        vec![a, b],
        "ranks by most-recent matching message"
    );
}

#[test]
fn search_returns_each_matching_thread_once() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    append_user_text(&conn, id, "receipt one", 10);
    append_user_text(&conn, id, "receipt two", 20);
    assert_eq!(
        search_ids(&conn, "receipt"),
        vec![id],
        "a thread with two hits appears once"
    );
}

// ── The FTS5 sanitizer ─────────────────────────────────────────────────────────

#[test]
fn sanitizer_returns_none_for_empty_or_punctuation_only() {
    assert_eq!(sanitize_fts_query(""), None);
    assert_eq!(sanitize_fts_query("   \t "), None);
    assert_eq!(
        sanitize_fts_query("()  \"  :"),
        None,
        "pure punctuation carries no term"
    );
}

#[test]
fn sanitizer_wraps_tokens_as_prefix_string_literals() {
    assert_eq!(sanitize_fts_query("report"), Some("\"report\"*".to_string()));
    assert_eq!(
        sanitize_fts_query("budget report"),
        Some("\"budget\"* \"report\"*".to_string()),
        "each token becomes its own prefix literal (implicit AND)"
    );
}

/// Every awkward input a filename fragment can carry produces a valid, NON-throwing query.
/// Raw, each of these would throw an fts5 syntax error (parentheses, a `:` column filter,
/// a bareword boolean, an unbalanced quote, pure punctuation) — the whole point of the
/// sanitizer.
#[test]
fn sanitizer_makes_awkward_input_non_throwing() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    append_user_text(&conn, id, "the report(v2) about foo:bar and cats and dogs", 10);

    for query in [
        "report(v2)",
        "foo:bar",
        "cats OR NOT dogs", // bareword booleans become literal terms, never operators
        "foo\"bar",         // an unbalanced embedded quote
        "(((",              // pure punctuation ⇒ no term ⇒ no match, still Ok
    ] {
        assert!(
            search_conversations(&conn, query, 50, 0).is_ok(),
            "query {query:?} must not throw an fts5 syntax error"
        );
    }
    // `id` exists so the seeded row participates; unused binding guard.
    let _ = id;
}

/// The meaningful awkward fragments still find the seeded thread: a prefix, a
/// parenthesized token, a colon-joined pair, a bareword that happens to be in the text,
/// and an embedded-quote fragment all match.
#[test]
fn sanitizer_keeps_awkward_fragments_matching() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    append_user_text(&conn, id, "the report(v2) about foo:bar and cats and dogs", 10);

    for query in ["report(v2)", "foo:bar", "AND", "rep", "foo\"bar", "cats"] {
        assert_eq!(
            search_ids(&conn, query),
            vec![id],
            "query {query:?} must match the seeded message"
        );
    }
}

// ── Cost meter ─────────────────────────────────────────────────────────────────

fn cost(
    day: &str,
    conversation_id: i64,
    provider: ProviderTag,
    prompt: u64,
    completion: u64,
    micros: i64,
) -> CostRecord {
    CostRecord {
        day: day.to_string(),
        conversation_id,
        provider,
        model: "test-model".to_string(),
        prompt_tokens: prompt,
        completion_tokens: completion,
        cost_micros: micros,
        priced: true,
    }
}

#[test]
fn cost_upsert_accumulates_onto_the_same_key() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    record_cost(&conn, &cost("2026-07-12", id, ProviderTag::Anthropic, 100, 20, 500)).expect("record 1");
    record_cost(&conn, &cost("2026-07-12", id, ProviderTag::Anthropic, 50, 5, 250)).expect("record 2");

    // One row (upsert, not duplicate) with summed values — the guard against the
    // NULL-in-PK regression that would insert a second row instead.
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM cost_meter", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rows, 1, "same (day, thread, provider, model) upserts, never duplicates");

    let summary = cost_summary(&conn).expect("summary");
    assert_eq!(summary.days.len(), 1);
    let day = &summary.days[0];
    assert_eq!(
        (day.prompt_tokens, day.completion_tokens, day.cost_micros),
        (150, 25, 750)
    );
    assert!(day.fully_priced);
}

#[test]
fn cost_rollup_sums_across_threads_per_day() {
    let conn = migrated_conn();
    let a = create_conversation(&conn, "A", 1, None).expect("create a");
    let b = create_conversation(&conn, "B", 1, None).expect("create b");
    record_cost(&conn, &cost("2026-07-12", a, ProviderTag::Anthropic, 100, 10, 300)).expect("a");
    record_cost(&conn, &cost("2026-07-12", b, ProviderTag::OpenAi, 200, 20, 600)).expect("b");
    record_cost(&conn, &cost("2026-07-11", a, ProviderTag::Gemini, 5, 1, 10)).expect("earlier day");

    let summary = cost_summary(&conn).expect("summary");
    assert_eq!(
        summary.days.iter().map(|d| d.day.as_str()).collect::<Vec<_>>(),
        vec!["2026-07-12", "2026-07-11"],
        "days are newest-first"
    );
    let today = &summary.days[0];
    assert_eq!(
        (today.prompt_tokens, today.completion_tokens, today.cost_micros),
        (300, 30, 900),
        "the per-day rollup sums across threads and models"
    );
}

#[test]
fn cost_rollup_marks_a_day_unpriced_when_any_row_is_unpriced() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    record_cost(&conn, &cost("2026-07-12", id, ProviderTag::Anthropic, 100, 10, 300)).expect("priced");
    // A local/unpriced model contributes tokens but no known cost.
    let mut unpriced = cost("2026-07-12", id, ProviderTag::Local, 40, 4, 0);
    unpriced.model = "local-llama".to_string();
    unpriced.priced = false;
    record_cost(&conn, &unpriced).expect("unpriced");

    let summary = cost_summary(&conn).expect("summary");
    let day = &summary.days[0];
    assert_eq!(day.prompt_tokens, 140, "tokens still sum");
    assert!(
        !day.fully_priced,
        "a day with any unpriced row reads unpriced, so cost shows 'unknown', never a silent $0"
    );
}

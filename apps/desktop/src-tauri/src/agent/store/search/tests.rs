//! Cross-thread search: which threads match, their order, their excerpts, the input sanitizer,
//! and the wake place a wake thread's row is titled from.

use rusqlite::Connection;

use super::*;
use crate::agent::llm::types::{AgentPart, AgentRole, WakeDigest, WakeDigestFolder, WakeDigestRollup};
use crate::agent::store::wake_place::{home_relative, place_of};
use crate::agent::store::{append_message, create_conversation, rename_conversation};

fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    crate::agent::store::run_migrations(&conn, crate::agent::store::MIGRATIONS).expect("migrate");
    conn
}

fn say(conn: &Connection, conversation_id: i64, role: AgentRole, text: &str, now: i64) {
    append_message(
        conn,
        conversation_id,
        role,
        &[AgentPart::Text(text.to_string())],
        text,
        None,
        None,
        now,
    )
    .expect("append message");
}

fn thread(conn: &Connection, title: &str, messages: &[&str], now: i64) -> i64 {
    let id = create_conversation(conn, title, now, None).expect("create");
    for (i, text) in messages.iter().enumerate() {
        say(conn, id, AgentRole::User, text, now + i as i64);
    }
    id
}

fn search_ids(conn: &Connection, query: &str) -> Vec<i64> {
    search_conversations(conn, query, 50, 0)
        .expect("search")
        .into_iter()
        .map(|hit| hit.conversation_id)
        .collect()
}

fn matched_text(parts: &[SnippetPart]) -> Vec<&str> {
    parts
        .iter()
        .filter(|part| part.matched)
        .map(|part| part.text.as_str())
        .collect()
}

fn plain_text(parts: &[SnippetPart]) -> String {
    parts.iter().map(|part| part.text.as_str()).collect()
}

// ── Ranking ───────────────────────────────────────────────────────────────────

/// A thread that's ABOUT the term beats a newer one that mentions it in passing: bm25 favors
/// the denser match, and recency only breaks ties.
#[test]
fn relevance_outranks_recency() {
    let conn = migrated_conn();
    let focused = thread(&conn, "A", &["mcp server mcp config mcp"], 100);
    let passing = thread(
        &conn,
        "B",
        &["we talked about holidays, groceries, the weather, a long list of things, and mcp once"],
        900,
    );
    assert_eq!(search_ids(&conn, "mcp"), vec![focused, passing]);
}

#[test]
fn equal_relevance_falls_back_to_the_newest_thread() {
    let conn = migrated_conn();
    let older = thread(&conn, "A", &["shared invoice topic"], 100);
    let newer = thread(&conn, "B", &["shared invoice topic"], 200);
    assert_eq!(search_ids(&conn, "invoice"), vec![newer, older]);
}

/// The title is the line the user wrote to find the thread again, so it leads; and a renamed
/// title, which no index holds, still finds its thread.
#[test]
fn a_title_match_leads_and_counts_on_its_own() {
    let conn = migrated_conn();
    let body_only = thread(&conn, "Groceries", &["budget budget budget"], 100);
    let titled = thread(&conn, "Budget review", &["numbers for the year"], 50);
    let renamed = thread(&conn, "Old name", &["nothing relevant here"], 10);
    rename_conversation(&conn, renamed, "Budgeting ideas").expect("rename");

    assert_eq!(search_ids(&conn, "budget"), vec![titled, renamed, body_only]);
}

#[test]
fn every_query_word_must_appear_somewhere() {
    let conn = migrated_conn();
    let both = thread(&conn, "T", &["the quarterly budget spreadsheet"], 100);
    thread(&conn, "U", &["the quarterly report"], 200);
    assert_eq!(search_ids(&conn, "quarterly budget"), vec![both]);
}

#[test]
fn a_thread_with_several_matches_appears_once() {
    let conn = migrated_conn();
    let id = thread(&conn, "T", &["receipt one", "receipt two"], 10);
    assert_eq!(search_ids(&conn, "receipt"), vec![id]);
}

#[test]
fn paging_tiles_the_ranked_list() {
    let conn = migrated_conn();
    let ids: Vec<i64> = (0..5).map(|i| thread(&conn, "T", &["kiwi"], 100 + i)).collect();
    let mut expected = ids.clone();
    expected.reverse();
    let page = |offset| {
        search_conversations(&conn, "kiwi", 2, offset)
            .expect("search")
            .into_iter()
            .map(|hit| hit.conversation_id)
            .collect::<Vec<_>>()
    };
    let tiled: Vec<i64> = [page(0), page(2), page(4)].concat();
    assert_eq!(tiled, expected);
}

#[test]
fn the_reserved_quiet_wakes_row_never_matches() {
    let conn = migrated_conn();
    let reserved = crate::agent::store::quiet_wakes_conversation(&conn).expect("reserved row");
    rename_conversation(&conn, reserved, "kiwi ledger").expect("rename");
    assert!(search_ids(&conn, "kiwi").is_empty());
}

#[test]
fn rank_orders_title_then_score_then_recency_then_id() {
    let candidate = |id, title_match, best, updated_at| Candidate {
        id,
        title_match,
        best,
        updated_at,
    };
    let mut candidates = vec![
        candidate(1, false, Some((-1.0, 10)), 500),
        candidate(2, true, None, 100),
        candidate(3, false, Some((-3.0, 11)), 100),
        candidate(4, true, Some((-0.5, 12)), 50),
        candidate(5, false, Some((-1.0, 13)), 600),
        candidate(6, false, Some((-1.0, 14)), 600),
    ];
    rank(&mut candidates);
    let order: Vec<i64> = candidates.iter().map(|c| c.id).collect();
    assert_eq!(order, vec![4, 2, 3, 6, 5, 1]);
}

#[test]
fn title_matching_is_word_prefix_and_case_blind() {
    let words = |q: &str| query_words(q);
    assert!(title_matches("MCP server setup", &words("mcp")));
    assert!(title_matches("MCP server setup", &words("serv mc")));
    assert!(!title_matches("rmcp crate", &words("mcp")), "mid-word isn't a prefix");
    assert!(title_matches("report(v2) draft", &words("report(v2)")));
    assert!(!title_matches("anything", &words("()")), "no words, no match");
}

// ── Snippets ──────────────────────────────────────────────────────────────────

#[test]
fn a_hit_flags_the_matched_words_in_its_excerpt() {
    let conn = migrated_conn();
    thread(&conn, "T", &["How do I set up the MCP server for Claude?"], 10);
    let hits = search_conversations(&conn, "mcp", 50, 0).expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(matched_text(&hits[0].snippet), vec!["MCP"]);
    assert_eq!(
        plain_text(&hits[0].snippet),
        "How do I set up the MCP server for Claude?"
    );
}

#[test]
fn a_prefix_query_flags_the_whole_word() {
    let conn = migrated_conn();
    thread(&conn, "T", &["the quarterly budget spreadsheet"], 10);
    let hits = search_conversations(&conn, "budg", 50, 0).expect("search");
    assert_eq!(matched_text(&hits[0].snippet), vec!["budget"]);
}

/// The excerpt comes from the thread's BEST message, not its first or newest.
#[test]
fn the_excerpt_comes_from_the_best_matching_message() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "T", 1, None).expect("create");
    say(
        &conn,
        id,
        AgentRole::User,
        "a long aside about many things that ends with kiwi",
        10,
    );
    say(&conn, id, AgentRole::Assistant, "kiwi kiwi", 20);
    say(&conn, id, AgentRole::User, "unrelated follow-up", 30);
    let hits = search_conversations(&conn, "kiwi", 50, 0).expect("search");
    assert_eq!(plain_text(&hits[0].snippet), "kiwi kiwi");
}

#[test]
fn a_title_only_hit_has_no_excerpt() {
    let conn = migrated_conn();
    thread(&conn, "Kiwi plans", &["nothing about fruit"], 10);
    let hits = search_conversations(&conn, "kiwi", 50, 0).expect("search");
    assert!(hits[0].snippet.is_empty());
}

#[test]
fn parse_snippet_splits_on_the_markers_and_flattens_newlines() {
    let raw = format!("…set up the {MATCH_START}MCP{MATCH_END} server\nfor {MATCH_START}mcp{MATCH_END}");
    assert_eq!(
        parse_snippet(&raw),
        vec![
            SnippetPart {
                text: "…set up the ".into(),
                matched: false
            },
            SnippetPart {
                text: "MCP".into(),
                matched: true
            },
            SnippetPart {
                text: " server for ".into(),
                matched: false
            },
            SnippetPart {
                text: "mcp".into(),
                matched: true
            },
        ]
    );
}

#[test]
fn parse_snippet_merges_adjacent_runs_and_drops_empty_ones() {
    let raw = format!("{MATCH_START}a{MATCH_END}{MATCH_START}b{MATCH_END}");
    assert_eq!(
        parse_snippet(&raw),
        vec![SnippetPart {
            text: "ab".into(),
            matched: true
        }]
    );
    assert!(parse_snippet("").is_empty());
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
/// a bareword boolean, an unbalanced quote, pure punctuation): the whole point of the
/// sanitizer.
#[test]
fn sanitizer_makes_awkward_input_non_throwing() {
    let conn = migrated_conn();
    thread(&conn, "T", &["the report(v2) about foo:bar and cats and dogs"], 10);
    for query in ["report(v2)", "foo:bar", "cats OR NOT dogs", "foo\"bar", "((("] {
        assert!(
            search_conversations(&conn, query, 50, 0).is_ok(),
            "query {query:?} must not throw an fts5 syntax error"
        );
    }
}

/// The meaningful awkward fragments still find the seeded thread.
#[test]
fn sanitizer_keeps_awkward_fragments_matching() {
    let conn = migrated_conn();
    let id = thread(&conn, "T", &["the report(v2) about foo:bar and cats and dogs"], 10);
    for query in ["report(v2)", "foo:bar", "AND", "rep", "foo\"bar", "cats"] {
        assert_eq!(search_ids(&conn, query), vec![id], "query {query:?} must match");
    }
}

// ── Who started it, and the wake place ────────────────────────────────────────

/// A search result has to wear the same mark the thread list gives it. Without the origin on
/// the hit, a thread the agent opened for itself looks in search exactly like one the user
/// started and forgot.
#[test]
fn search_hit_says_who_started_the_thread() {
    let conn = migrated_conn();
    let mine = create_conversation(&conn, "Mine", 1, None).expect("create mine");
    let theirs =
        create_conversation(&conn, "Theirs", 1, Some(ConversationOrigin::Notification)).expect("create theirs");
    say(&conn, mine, AgentRole::User, "kumquat harvest", 100);
    say(&conn, theirs, AgentRole::User, "kumquat harvest", 200);

    let hits = search_conversations(&conn, "kumquat", 50, 0).expect("search");
    let origins: Vec<_> = hits.iter().map(|hit| (hit.conversation_id, hit.origin)).collect();
    assert_eq!(
        origins,
        vec![(theirs, Some(ConversationOrigin::Notification)), (mine, None)]
    );
}

fn digest(folders: &[&str], rollups: &[(&str, u32)]) -> WakeDigest {
    WakeDigest {
        folders: folders
            .iter()
            .map(|folder| WakeDigestFolder {
                folder: (*folder).to_string(),
                created: 1,
                modified: 0,
                removed: 0,
                renamed: 0,
            })
            .collect(),
        rollups: rollups
            .iter()
            .map(|(ancestor, folders)| WakeDigestRollup {
                ancestor: (*ancestor).to_string(),
                folders: *folders,
                changes: 3,
            })
            .collect(),
    }
}

#[test]
fn a_wake_hit_carries_where_its_activity_happened() {
    let conn = migrated_conn();
    let id = create_conversation(&conn, "tmp", 1, Some(ConversationOrigin::Notification)).expect("create");
    let digest = digest(&["/private/tmp", "/opt/data"], &[("/var/log", 4)]);
    append_message(
        &conn,
        id,
        AgentRole::User,
        &[AgentPart::WakeDigest(digest)],
        "/private/tmp /opt/data",
        None,
        None,
        10,
    )
    .expect("append digest");

    let hits = search_conversations(&conn, "data", 50, 0).expect("search");
    assert_eq!(
        hits[0].wake_place,
        Some(WakePlace {
            folder: "/private/tmp".into(),
            other_folders: 5
        })
    );
}

#[test]
fn a_renamed_wake_thread_keeps_its_own_title() {
    let d = digest(&["/Users/ada/tmp"], &[]);
    assert!(place_of(&d, "tmp", Some("/Users/ada")).is_some());
    assert_eq!(place_of(&d, "My cleanup", Some("/Users/ada")), None);
}

#[test]
fn a_digest_of_rollups_only_is_placed_at_the_first_ancestor() {
    let d = digest(&[], &[("/Users/ada/Library", 7)]);
    assert_eq!(
        place_of(&d, "Library", Some("/Users/ada")),
        Some(WakePlace {
            folder: "~/Library".into(),
            other_folders: 6
        })
    );
    assert_eq!(place_of(&digest(&[], &[]), "", Some("/Users/ada")), None);
}

#[test]
fn home_relative_writes_the_home_folder_as_a_tilde() {
    let home = Some("/Users/ada");
    assert_eq!(home_relative("/Users/ada", home), "~");
    assert_eq!(home_relative("/Users/ada/tmp", home), "~/tmp");
    assert_eq!(
        home_relative("/Users/adam/tmp", home),
        "/Users/adam/tmp",
        "a sibling isn't home"
    );
    assert_eq!(home_relative("/private/tmp", home), "/private/tmp");
    assert_eq!(home_relative("/Users/ada/tmp", Some("/Users/ada/")), "~/tmp");
    assert_eq!(home_relative("/Users/ada/tmp", None), "/Users/ada/tmp");
}

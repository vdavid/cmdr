//! The wake job, driven end to end against the real turn loop with fake seams.

use futures_util::future::BoxFuture;
use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

use super::super::*;
use crate::agent::chat::context::ContextEnvelope;
use crate::agent::chat::runtime::{ToolDispatchOutcome, ToolDispatcher, TurnResult};
use crate::agent::llm::fake::{FakeAgentLlm, ScriptedTurn};
use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ProviderTag};
use crate::agent::store::{MIGRATIONS, run_migrations};
use crate::agent::types::ConversationOrigin;

fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

/// A dispatcher that answers nothing, since these tests script the model to answer directly
/// rather than to reach for a tool. What the wake proves here is the LOOP, not the toolset.
struct NoTools;

impl ToolDispatcher for NoTools {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        Box::pin(async move {
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: serde_json::json!({}),
                    elided: false,
                },
                proposal: None,
            }
        })
    }
}

fn envelope(now: i64) -> ContextEnvelope {
    ContextEnvelope {
        captured_at: now,
        focused_pane_path: None,
        cursor_item: None,
        selection_count: 0,
        volumes: Vec::new(),
        attachments: Vec::new(),
        denied_names: Vec::new(),
        rename_batch_files: 0,
    }
}

fn params(readiness: WakeReadiness, now: i64, envelope: &ContextEnvelope) -> WakeParams<'_> {
    WakeParams {
        readiness,
        now_secs: now,
        digest_budget_tokens: 2_000,
        envelope,
        tools: &[],
        offset: chrono::FixedOffset::east_opt(0).expect("utc"),
        provider: ProviderTag::Anthropic,
        model: "test-model".to_string(),
        prompt_budget: 16_000,
    }
}

fn arrivals(folder: &str, created: u32, window_start: u64) -> EventBundle {
    EventBundle {
        folder: folder.to_string(),
        counters: ChangeCounters {
            created,
            ..ChangeCounters::default()
        },
        window_start,
        last_event_at: window_start,
    }
}

/// The whole point, end to end: something due wakes the agent, and the turn runs in a
/// conversation the user never started, marked as one the agent opened.
///
/// The `notification` origin has existed in the schema since v1 and this is its first writer.
/// It is what lets the rail tell an agent-opened thread from one the user began, and what
/// lets a sweep point back at the reasoning that produced it.
#[tokio::test]
async fn a_due_inbox_wakes_into_a_thread_marked_as_the_agents_own() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        1_000,
    );
    let due_at = inbox.next_deadline().expect("something waits");

    let llm = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["Four files arrived.".into()])]);
    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(due_at as i64);

    let outcome = run_wake(
        &llm,
        &NoTools,
        &conn,
        &mut inbox,
        params(WakeReadiness::Ready, due_at as i64, &env),
        &sink,
        &CancellationToken::new(),
    )
    .await;

    let WakeOutcome::Ran {
        conversation_id,
        result,
    } = outcome
    else {
        panic!("expected a turn, got {outcome:?}");
    };
    assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");

    let origin: Option<String> = conn
        .query_row(
            "SELECT origin FROM conversations WHERE id = ?1",
            rusqlite::params![conversation_id],
            |row| row.get(0),
        )
        .expect("the conversation exists");
    assert_eq!(origin.as_deref(), Some(ConversationOrigin::Notification.as_token()));

    assert_eq!(inbox.len(), 0, "a wake drains what it reported on");
}

/// A closed gate stops the turn before anything is created. No conversation, no provider
/// call, and the inbox keeps what it was holding.
#[tokio::test]
async fn a_closed_gate_creates_no_thread_and_keeps_the_backlog() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        1_000,
    );

    let llm = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["should not run".into()])]);
    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(9_000);

    let outcome = run_wake(
        &llm,
        &NoTools,
        &conn,
        &mut inbox,
        params(WakeReadiness::NeedsApiKey, 9_000, &env),
        &sink,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(outcome, WakeOutcome::NotReady(WakeReadiness::NeedsApiKey)));
    assert_eq!(inbox.len(), 1, "the backlog waits for the key");
    let threads: i64 = conn
        .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
        .expect("count");
    assert_eq!(threads, 0, "nothing was opened");
}

/// Nothing due is the common case, and it must cost nothing: no thread, no turn, and the
/// rows stay put until their own deadlines land.
#[tokio::test]
async fn nothing_due_costs_no_turn() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        1_000,
    );

    let llm = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["should not run".into()])]);
    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(1_001);

    let outcome = run_wake(
        &llm,
        &NoTools,
        &conn,
        &mut inbox,
        params(WakeReadiness::Ready, 1_001, &env),
        &sink,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(outcome, WakeOutcome::NothingDue));
    assert_eq!(inbox.len(), 1);
}

/// The thread is named for the PLACE, not with an authored sentence: a folder name is data,
/// while a backend-written English title would be untranslated copy in the database, sitting
/// in a list beside threads the user named themselves.
#[test]
fn a_wake_thread_is_named_for_the_busiest_place() {
    let digest = compact(
        &[ScoredBundle {
            bundle: arrivals("/Users/someone/Downloads", 4, 100),
            interest: Interest::of(0.9),
        }],
        2_000,
    );

    assert_eq!(thread_title(&digest), "Downloads");
}

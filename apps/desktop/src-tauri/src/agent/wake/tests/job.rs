//! The wake job, driven end to end against the real turn loop with fake seams.

use futures_util::future::BoxFuture;
use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

use super::super::*;
use crate::agent::chat::context::ContextEnvelope;
use crate::agent::chat::runtime::{ToolDispatchOutcome, ToolDispatcher, TurnResult};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::fake::{FakeAgentLlm, ScriptedTurn};
use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ProviderTag, ToolId};
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

fn run_params<'a>(now: i64, envelope: &'a ContextEnvelope) -> RunWakeParams<'a> {
    RunWakeParams {
        now_secs: now,
        envelope,
        tools: &[],
        offset: chrono::FixedOffset::east_opt(0).expect("utc"),
        provider: ProviderTag::Anthropic,
        model: "test-model".to_string(),
        prompt_budget: 16_000,
    }
}

/// An LLM factory answering with one fixed line, whatever thread it is built for.
fn say(text: &'static str) -> impl Fn(i64) -> Box<dyn AgentLlm> {
    move |_| Box::new(FakeAgentLlm::script(vec![ScriptedTurn::Say(vec![text.to_string()])]))
}

/// How many threads a user would see. ❌ Never a bare `COUNT(*) FROM conversations`: the store
/// reserves one hidden row for what quiet wakes spent, and counting it reads as a thread a wake
/// opened.
fn visible_threads(conn: &Connection) -> i64 {
    crate::agent::store::list_conversations(conn, 100, 0, true)
        .expect("list threads")
        .len() as i64
}

fn no_tools(_conversation_id: i64) -> Box<dyn ToolDispatcher> {
    Box::new(NoTools)
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
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let due_at = inbox.next_deadline().expect("something waits");

    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(due_at as i64);

    let outcome = run_wake(
        &say("Four files arrived."),
        &no_tools,
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
        tier,
        folders,
    } = outcome
    else {
        panic!("expected a turn, got {outcome:?}");
    };
    assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");
    assert_eq!(tier, WakeTier::Hot, "four arrivals in a scored folder is hot");
    assert_eq!(folders, 1);

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
        DEFAULT_HOT_DELAY,
        1_000,
    );

    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(9_000);

    let outcome = run_wake(
        &say("should not run"),
        &no_tools,
        &conn,
        &mut inbox,
        params(WakeReadiness::NeedsApiKey, 9_000, &env),
        &sink,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(outcome, WakeOutcome::NotReady(WakeReadiness::NeedsApiKey)));
    assert_eq!(inbox.len(), 1, "the backlog waits for the key");
    let threads = visible_threads(&conn);
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
        DEFAULT_HOT_DELAY,
        1_000,
    );

    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(1_001);

    let outcome = run_wake(
        &say("should not run"),
        &no_tools,
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

// ── The prepare / run split ───────────────────────────────────────────────────

/// The property `run_wake`'s step order exists for, now that the steps sit on two threads:
/// the rows leave the inbox only once a turn is CERTAIN to run. A digest budget too small to
/// render anything is an ordinary path, not a crash, and it must cost the backlog nothing.
#[test]
fn a_prepare_that_cannot_render_a_digest_keeps_the_backlog() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let due_at = inbox.next_deadline().expect("something waits");
    save_all(&conn, &inbox).expect("write");

    let outcome = prepare_wake(
        &conn,
        &mut inbox,
        &PrepareParams {
            readiness: WakeReadiness::Ready,
            now_secs: due_at as i64,
            // Not enough for a single line, so nothing can be said.
            digest_budget_tokens: 0,
            ignore_deadlines: false,
        },
    );

    assert!(matches!(outcome, PrepareOutcome::NothingDue), "{outcome:?}");
    assert_eq!(inbox.len(), 1, "the backlog is exactly as it was");
    assert_eq!(load(&conn).expect("read").len(), 1, "and so is the table");
    let threads = visible_threads(&conn);
    assert_eq!(threads, 0, "nothing was opened");
}

/// The dev-only force skips the CLOCK and nothing else, so verifying the loop doesn't mean
/// sitting out a cadence that runs up to half an hour.
#[test]
fn a_forced_prepare_acts_on_rows_that_are_not_due_yet() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    save_all(&conn, &inbox).expect("write");
    assert!(!inbox.due_at(1_000), "nothing has come due at the admit instant");

    let outcome = prepare_wake(
        &conn,
        &mut inbox,
        &PrepareParams {
            readiness: WakeReadiness::Ready,
            now_secs: 1_000,
            digest_budget_tokens: 2_000,
            ignore_deadlines: true,
        },
    );

    assert!(matches!(outcome, PrepareOutcome::Ready(_)), "{outcome:?}");
    assert_eq!(inbox.len(), 0, "and it committed, so the rows are out");
}

/// ⚠️ The force skips the timer, ❌ never a gate. A forced wake on an unconsented profile has
/// to spend nothing, or the E2E hook would be a way around the one thing the user agreed to.
#[test]
fn a_forced_prepare_still_obeys_the_gates() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    save_all(&conn, &inbox).expect("write");

    let outcome = prepare_wake(
        &conn,
        &mut inbox,
        &PrepareParams {
            readiness: WakeReadiness::NeedsConsent,
            now_secs: 1_000,
            digest_budget_tokens: 2_000,
            ignore_deadlines: true,
        },
    );

    assert!(
        matches!(outcome, PrepareOutcome::NotReady(WakeReadiness::NeedsConsent)),
        "{outcome:?}"
    );
    let threads = visible_threads(&conn);
    assert_eq!(threads, 0, "nothing was opened");
}

/// An empty inbox has nothing to say however hard it is pushed: a force must not open a
/// thread that reports silence.
#[test]
fn a_forced_prepare_on_an_empty_inbox_opens_nothing() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();

    let outcome = prepare_wake(
        &conn,
        &mut inbox,
        &PrepareParams {
            readiness: WakeReadiness::Ready,
            now_secs: 1_000,
            digest_budget_tokens: 2_000,
            ignore_deadlines: true,
        },
    );

    assert!(matches!(outcome, PrepareOutcome::NothingDue), "{outcome:?}");
    let threads = visible_threads(&conn);
    assert_eq!(threads, 0);
}

/// A closed gate declines before the thread, before the drain, and before the table is
/// touched.
#[test]
fn a_prepare_behind_a_closed_gate_spends_nothing() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    save_all(&conn, &inbox).expect("write");

    let outcome = prepare_wake(
        &conn,
        &mut inbox,
        &PrepareParams {
            readiness: WakeReadiness::NeedsApiKey,
            now_secs: 9_000,
            digest_budget_tokens: 2_000,
            ignore_deadlines: false,
        },
    );

    assert!(
        matches!(outcome, PrepareOutcome::NotReady(WakeReadiness::NeedsApiKey)),
        "{outcome:?}"
    );
    assert_eq!(inbox.len(), 1);
    assert_eq!(load(&conn).expect("read").len(), 1);
}

/// Once every step that can decline has passed, prepare commits: it opens the thread, takes
/// the rows, and clears the table in the same breath. A process that dies mid-turn loses that
/// digest rather than re-delivering it, which is the trade `DETAILS.md` states on purpose.
#[test]
fn a_committed_prepare_opens_a_thread_and_empties_the_inbox() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let due_at = inbox.next_deadline().expect("something waits");
    save_all(&conn, &inbox).expect("write");

    let outcome = prepare_wake(
        &conn,
        &mut inbox,
        &PrepareParams {
            readiness: WakeReadiness::Ready,
            now_secs: due_at as i64,
            digest_budget_tokens: 2_000,
            ignore_deadlines: false,
        },
    );

    let PrepareOutcome::Ready(prepared) = outcome else {
        panic!("expected a prepared wake, got {outcome:?}");
    };
    assert!(prepared.digest.contains("Downloads"), "{}", prepared.digest);
    assert_eq!(prepared.rows.len(), 1);
    assert_eq!(prepared.tier, WakeTier::Hot);
    assert!(inbox.is_empty(), "the rows went with the prepared wake");
    assert!(load(&conn).expect("read").is_empty(), "and left the table");

    let origin: Option<String> = conn
        .query_row(
            "SELECT origin FROM conversations WHERE id = ?1",
            rusqlite::params![prepared.conversation_id],
            |row| row.get(0),
        )
        .expect("the conversation exists");
    assert_eq!(origin.as_deref(), Some(ConversationOrigin::Notification.as_token()));
}

/// The run step is the half that reaches a provider, and it runs against the thread prepare
/// opened rather than opening one of its own. That separation is what lets the writer thread
/// hand off and go straight back to servicing its channel.
#[tokio::test]
async fn a_prepared_wake_runs_its_turn_in_the_thread_prepare_opened() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let due_at = inbox.next_deadline().expect("something waits");

    let PrepareOutcome::Ready(prepared) = prepare_wake(
        &conn,
        &mut inbox,
        &PrepareParams {
            readiness: WakeReadiness::Ready,
            now_secs: due_at as i64,
            digest_budget_tokens: 2_000,
            ignore_deadlines: false,
        },
    ) else {
        panic!("expected a prepared wake");
    };

    let llm = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["Four files arrived.".into()])]);
    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(due_at as i64);

    let result = run_prepared_wake(
        &llm,
        &NoTools,
        &conn,
        &prepared,
        &run_params(due_at as i64, &env),
        &sink,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");
    let messages: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
            rusqlite::params![prepared.conversation_id],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(messages, 2, "the digest and the answer landed in prepare's thread");
}

/// A wake builds its LLM and its dispatcher only once the conversation id exists, because
/// both are SCOPED to it: evidence is what stops a claim in one thread being backed by facts
/// delivered to another, and the LLM log is keyed the same way.
#[tokio::test]
async fn the_llm_and_the_dispatcher_are_built_for_the_thread_the_wake_creates() {
    let conn = migrated_conn();
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let due_at = inbox.next_deadline().expect("something waits");

    let seen: std::sync::Mutex<Vec<i64>> = std::sync::Mutex::new(Vec::new());
    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(due_at as i64);

    let outcome = run_wake(
        &|id| {
            seen.lock().expect("not poisoned").push(id);
            Box::new(FakeAgentLlm::script(vec![ScriptedTurn::Say(vec![
                "Four files.".into(),
            ])]))
        },
        &|id| {
            seen.lock().expect("not poisoned").push(id);
            Box::new(NoTools)
        },
        &conn,
        &mut inbox,
        params(WakeReadiness::Ready, due_at as i64, &env),
        &sink,
        &CancellationToken::new(),
    )
    .await;

    let WakeOutcome::Ran { conversation_id, .. } = outcome else {
        panic!("expected a turn, got {outcome:?}");
    };
    assert_eq!(
        *seen.lock().expect("not poisoned"),
        vec![conversation_id, conversation_id],
        "both factories were handed the thread the wake opened"
    );
}

// ── A wake with nothing to say ────────────────────────────────────────────────

/// An LLM factory that calls `nothing_to_suggest` and then signs off, which is what a wake
/// with nothing worth raising does.
fn says_nothing(reason: &'static str) -> impl Fn(i64) -> Box<dyn AgentLlm> {
    move |_| {
        Box::new(FakeAgentLlm::script(vec![
            ScriptedTurn::CallTools(vec![(
                ToolId::NothingToSuggest,
                serde_json::json!({ "reason": reason }),
            )]),
            ScriptedTurn::Say(vec!["Nothing worth raising.".to_string()]),
        ]))
    }
}

/// The whole point of the tool: a wake that finds nothing leaves the user's session list
/// exactly as it was — no thread, no digest, no "we had a look and it was fine".
///
/// ⚠️ **But the cost survives it.** `cost_meter` cascades on the conversation, so deleting the
/// thread outright would erase what the proactive agent spent from the one place the user can
/// see it. The rows fold onto the reserved quiet-wakes thread first.
#[tokio::test]
async fn a_noop_wake_leaves_no_thread_but_keeps_what_it_spent() {
    let conn = migrated_conn();
    let reserved = crate::agent::store::quiet_wakes_conversation(&conn).expect("the reserved row");
    let mut inbox = Inbox::default();
    inbox.admit(
        arrivals("/Users/someone/Downloads", 4, 100),
        FolderImportance::Scored(0.9),
        DEFAULT_HOT_DELAY,
        1_000,
    );
    let due_at = inbox.next_deadline().expect("something waits");

    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(due_at as i64);

    let outcome = run_wake(
        &says_nothing("all of it is cache churn"),
        &no_tools,
        &conn,
        &mut inbox,
        params(WakeReadiness::Ready, due_at as i64, &env),
        &sink,
        &CancellationToken::new(),
    )
    .await;

    let WakeOutcome::Quiet { tier, folders, reason } = outcome else {
        panic!("expected a quiet wake, got {outcome:?}");
    };
    assert_eq!(tier, WakeTier::Hot);
    assert_eq!(folders, 1);
    assert_eq!(reason.as_deref(), Some("all of it is cache churn"));

    assert_eq!(visible_threads(&conn), 0, "the user's session list is untouched");
    assert_eq!(inbox.len(), 0, "the rows it looked at are still spent");

    let stray: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cost_meter WHERE conversation_id <> ?1",
            rusqlite::params![reserved],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(stray, 0, "no cost row is left pointing at the vanished thread");
    let kept: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cost_meter WHERE conversation_id = ?1",
            rusqlite::params![reserved],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(kept, 1, "what the quiet wake spent moved onto the reserved thread");
}

/// ⚠️ **There is one `agent_tool_view()`, so the RAIL sees this tool too**, and there it must
/// be completely inert. A user's own thread that somehow calls `nothing_to_suggest` keeps its
/// thread, its messages, and its place in the session list: only a wake wraps its dispatcher
/// in the watch that acts on the call.
#[tokio::test]
async fn a_rail_turn_calling_nothing_to_suggest_deletes_nothing() {
    use crate::agent::chat::runtime::{TurnParams, run_turn};

    let conn = migrated_conn();
    let id = crate::agent::store::create_conversation(&conn, "the user's own thread", 1_000, None).expect("create");

    let llm = FakeAgentLlm::script(vec![
        ScriptedTurn::CallTools(vec![(
            ToolId::NothingToSuggest,
            serde_json::json!({ "reason": "confused model" }),
        )]),
        ScriptedTurn::Say(vec!["Nothing to report.".to_string()]),
    ]);
    let (sink, _events) = tokio::sync::mpsc::unbounded_channel();
    let env = envelope(1_000);

    // Exactly the shape the rail runs: `run_turn` against the plain dispatcher, with no wake
    // watch wrapping it.
    let result = run_turn(
        &llm,
        &NoTools,
        &conn,
        &[],
        &TurnParams {
            conversation_id: id,
            user_text: Some("are we good?"),
            cmdr_md: None,
            envelope: &env,
            offset: chrono::FixedOffset::east_opt(0).expect("utc"),
            now_secs: 1_000,
            provider: ProviderTag::Anthropic,
            model: "test-model".to_string(),
            prompt_budget: 16_000,
        },
        &sink,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");
    assert_eq!(visible_threads(&conn), 1, "the user's thread is still listed");
    let detail = crate::agent::store::get_conversation(&conn, id, 10, 0)
        .expect("get")
        .expect("the thread is still there");
    assert!(
        detail.total_messages >= 2,
        "its messages survive too, got {}",
        detail.total_messages
    );
}

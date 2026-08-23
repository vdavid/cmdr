//! Runtime tests: single-flight, the per-message budgets, cancellation at a tool
//! boundary, the crash-safe persistence model, cost metering, and an end-to-end
//! fake-driven multi-tool turn. The wake path has its own file (`wake_tests.rs`)
//! and borrows the fixtures here.
//!
//! Tool dispatch is exercised through a scripted [`ToolDispatcher`] double (there is no
//! in-tree full-Tauri harness for the agent toolset at unit-test scope), and the LLM
//! through a local [`ProgrammableLlm`] that gives per-turn control over text, tool calls,
//! usage, and a mid-stream drop (no `End`) for the crash cases.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::FixedOffset;
use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{self, StreamExt};
use rusqlite::Connection;
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::agent::chat::context::{ContextEnvelope, MAX_TOOL_TURNS, MAX_WALL_TIME};
use crate::agent::llm::AgentDeltaStream;
use crate::agent::llm::types::{
    AgentDelta, AgentLlmError, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolCall, AgentToolResult,
    AgentUsage, ProviderTag, ToolDeclaration, ToolId,
};
use crate::agent::store;
use crate::test_support::wait_until_async;

// ── Fixtures ──────────────────────────────────────────────────────────────────

/// The prompt budget the turn fixtures assemble against: the conservative default, so no
/// test trips budget pressure unless it means to.
const TEST_PROMPT_BUDGET: usize = crate::agent::chat::budget::DEFAULT_PROMPT_TOKEN_BUDGET;

fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    store::run_migrations(&conn, store::MIGRATIONS).expect("migrate");
    conn
}

fn offset() -> FixedOffset {
    FixedOffset::east_opt(2 * 3600).expect("valid offset")
}

fn envelope() -> ContextEnvelope {
    ContextEnvelope {
        captured_at: 1_780_000_000,
        focused_pane_path: Some("~/Documents".to_string()),
        cursor_item: Some("taxes/".to_string()),
        selection_count: 1,
        volumes: vec![],
        attachments: vec![],
        denied_names: vec![],
        rename_batch_files: 101,
    }
}

pub(super) fn params<'a>(conversation_id: i64, user_text: Option<&'a str>) -> TurnParams<'a> {
    TurnParams {
        conversation_id,
        user: user_text.map(UserTurn::Text),
        cmdr_md: None,
        envelope: LEAK_ENVELOPE.get_or_init(envelope),
        offset: offset(),
        now_secs: 1_780_000_000,
        provider: ProviderTag::Local,
        model: "fake-model".to_string(),
        prompt_budget: TEST_PROMPT_BUDGET,
    }
}

// A single leaked envelope so `params` can hand out a `&'a ContextEnvelope` without the
// caller juggling a binding; the value is constant across the test run.
static LEAK_ENVELOPE: std::sync::OnceLock<ContextEnvelope> = std::sync::OnceLock::new();

fn conversation(conn: &Connection) -> i64 {
    store::create_conversation(conn, "Test thread", 1_780_000_000, None).expect("create conversation")
}

fn drain(rx: &mut UnboundedReceiver<AgentChatEvent>) -> Vec<AgentChatEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

/// The transcript role of a persisted row; panics on an event row (these tests assert
/// transcript shape, so an unexpected event row should fail loudly).
fn stored_role(message: &store::StoredMessage) -> AgentRole {
    match &message.content {
        store::StoredContent::Message { role, .. } => *role,
        store::StoredContent::Event(event) => panic!("expected a transcript row, got event {event:?}"),
    }
}

fn leading_text(message: &AgentMessage) -> &str {
    match &message.parts[0] {
        AgentPart::Text(text) => text,
        _ => panic!("expected a leading text part"),
    }
}

// ── A programmable LLM (per-turn text / tools / usage / mid-stream drop) ───────

enum Program {
    /// Stream these text chunks, then a clean `End` (Completed) with this usage.
    Answer { chunks: Vec<String>, usage: AgentUsage },
    /// Emit these tool calls, then a clean `End` (ToolCall) with this usage.
    Tools {
        calls: Vec<(ToolId, Value)>,
        usage: AgentUsage,
    },
    /// Stream text chunks then END THE STREAM with no `End` delta (a mid-stream drop).
    DropAfterText { chunks: Vec<String> },
    /// Stream text chunks then yield a typed stream error (a mid-stream provider problem).
    ErrorAfterText { chunks: Vec<String>, error: AgentLlmError },
}

struct ProgrammableLlm {
    turns: Mutex<VecDeque<Program>>,
    calls_seen: Mutex<Vec<Vec<AgentMessage>>>,
}

impl ProgrammableLlm {
    fn new(programs: Vec<Program>) -> Self {
        Self {
            turns: Mutex::new(programs.into()),
            calls_seen: Mutex::new(Vec::new()),
        }
    }

    fn calls_seen(&self) -> Vec<Vec<AgentMessage>> {
        self.calls_seen.lock().expect("lock").clone()
    }
}

impl AgentLlm for ProgrammableLlm {
    fn respond<'a>(
        &'a self,
        _system: &'a str,
        _tools: &'a [ToolDeclaration],
        messages: &'a [AgentMessage],
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<AgentDeltaStream, AgentLlmError>> {
        async move {
            self.calls_seen.lock().expect("lock").push(messages.to_vec());
            let program = self
                .turns
                .lock()
                .expect("lock")
                .pop_front()
                .ok_or_else(|| AgentLlmError::Provider("programmable: script exhausted".to_string()))?;
            let deltas = program_to_deltas(program);
            let cancel_signal = cancel.clone();
            let stream: AgentDeltaStream = stream::iter(deltas)
                .take_until(async move { cancel_signal.cancelled().await })
                .boxed();
            Ok(stream)
        }
        .boxed()
    }
}

fn program_to_deltas(program: Program) -> Vec<Result<AgentDelta, AgentLlmError>> {
    match program {
        Program::Answer { chunks, usage } => {
            let joined = chunks.concat();
            let mut deltas: Vec<Result<AgentDelta, AgentLlmError>> =
                chunks.into_iter().map(|c| Ok(AgentDelta::Text(c))).collect();
            deltas.push(Ok(AgentDelta::End {
                stop: AgentStopReason::Completed,
                usage,
                message: AgentMessage {
                    role: AgentRole::Assistant,
                    parts: vec![AgentPart::Text(joined)],
                    at: 0,
                },
            }));
            deltas
        }
        Program::Tools { calls, usage } => {
            let mut deltas = Vec::new();
            let mut parts = Vec::new();
            for (index, (tool, arguments)) in calls.into_iter().enumerate() {
                let call_id = format!("call-{index}");
                deltas.push(Ok(AgentDelta::ToolCallStarted {
                    call_id: call_id.clone(),
                    tool: tool.clone(),
                }));
                parts.push(AgentPart::ToolCall(AgentToolCall {
                    call_id,
                    tool,
                    arguments,
                    reasoning: None,
                }));
            }
            deltas.push(Ok(AgentDelta::End {
                stop: AgentStopReason::ToolCall,
                usage,
                message: AgentMessage {
                    role: AgentRole::Assistant,
                    parts,
                    at: 0,
                },
            }));
            deltas
        }
        // No `End`: the stream just ends after the text (a provider drop / crash).
        Program::DropAfterText { chunks } => chunks.into_iter().map(|c| Ok(AgentDelta::Text(c))).collect(),
        Program::ErrorAfterText { chunks, error } => {
            let mut deltas: Vec<Result<AgentDelta, AgentLlmError>> =
                chunks.into_iter().map(|c| Ok(AgentDelta::Text(c))).collect();
            deltas.push(Err(error));
            deltas
        }
    }
}

// ── Scripted dispatchers ──────────────────────────────────────────────────────

/// Returns a successful, structured tool result for every call.
pub(super) struct OkDispatcher;

impl ToolDispatcher for OkDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "looked_at": call.tool.as_wire_name() }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// Sleeps `secs` (virtual, under `start_paused`) before returning — to drive the
/// wall-time budget past its ceiling between respond calls.
struct SleepingDispatcher {
    secs: u64,
}

impl ToolDispatcher for SleepingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            // allowed-test-sleep: this stub's whole job is to burn wall-time budget, and under
            // `start_paused` the runtime advances the clock rather than waiting
            tokio::time::sleep(Duration::from_secs(self.secs)).await;
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "ok": true }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// Fires a cancellation token during dispatch — the user pressing stop while a tool runs.
struct CancellingDispatcher {
    token: CancellationToken,
}

impl ToolDispatcher for CancellingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            self.token.cancel();
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "ok": true }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

// ── Single-flight ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_second_send_queues_and_emits_queued() {
    let locks = Arc::new(ConversationLocks::new());
    let (tx, mut rx) = unbounded_channel();

    // Hold the conversation's lock.
    let first = locks.acquire(7, &tx).await;
    assert!(drain(&mut rx).is_empty(), "the first acquire does not queue");

    // A second acquire for the same thread emits Queued and blocks until released.
    let locks2 = locks.clone();
    let tx2 = tx.clone();
    let waiter = tokio::spawn(async move {
        let _guard = locks2.acquire(7, &tx2).await;
    });
    let mut events = Vec::new();
    wait_until_async(Duration::from_secs(2), "the queued send to signal Queued", || {
        events.extend(drain(&mut rx));
        !events.is_empty()
    })
    .await;
    assert_eq!(events, vec![AgentChatEvent::Queued], "the queued send signals Queued");
    assert!(!waiter.is_finished(), "the second send waits for the lock");

    drop(first);
    waiter.await.expect("the queued send proceeds once released");
}

// ── Budgets ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn max_tool_turns_halts_before_the_ninth_respond() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // More tool turns than the cap allows; the runtime must stop before the 9th respond.
    let programs = (0..MAX_TOOL_TURNS + 4)
        .map(|_| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        })
        .collect();
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("keep going")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::BudgetExhausted));
    assert_eq!(
        llm.calls_seen().len(),
        MAX_TOOL_TURNS,
        "exactly MAX_TOOL_TURNS respond calls fire; the ninth never does"
    );
    assert!(
        drain(&mut rx).contains(&AgentChatEvent::Failed {
            kind: AgentErrorKind::BudgetExhausted,
            detail: None,
        }),
        "the budget-exhausted notice is emitted"
    );
}

#[tokio::test(start_paused = true)]
async fn max_wall_time_halts_the_loop() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let programs = (0..5)
        .map(|_| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        })
        .collect();
    let llm = ProgrammableLlm::new(programs);
    let (tx, _rx) = unbounded_channel();

    // One dispatch crosses the configured wall-time ceiling, so it trips after one tool round.
    let result = run_turn(
        &llm,
        &SleepingDispatcher {
            secs: MAX_WALL_TIME.as_secs() + 1,
        },
        &conn,
        &[],
        &params(id, Some("slow please")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::BudgetExhausted));
    assert_eq!(
        llm.calls_seen().len(),
        1,
        "the wall clock halts the loop after one round"
    );
}

// ── Cancellation ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn cancellation_mid_loop_stops_cleanly_at_a_tool_boundary() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["never reached".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let cancel = CancellationToken::new();
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &CancellingDispatcher { token: cancel.clone() },
        &conn,
        &[],
        &params(id, Some("stop me")),
        &tx,
        &cancel,
    )
    .await;

    assert_eq!(result, TurnResult::Cancelled, "a mid-loop cancel is a clean stop");
    assert_eq!(llm.calls_seen().len(), 1, "the second respond never fires after cancel");
    assert!(
        !drain(&mut rx)
            .iter()
            .any(|e| matches!(e, AgentChatEvent::Failed { .. })),
        "cancellation is not a failure"
    );
}

// ── Crash / persistence semantics ─────────────────────────────────────────────

#[tokio::test]
async fn crash_a_stream_dropped_mid_text_persists_nothing() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // The first (and only) respond drops mid-text: no `End`, so nothing is persisted.
    let llm = ProgrammableLlm::new(vec![Program::DropAfterText {
        chunks: vec!["partial ".to_string(), "answer".to_string()],
    }]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what is big?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::UnfinishedReply));
    let persisted = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(
        persisted.is_empty(),
        "no user row and no assistant row persist when the first call never reached End (crash cases a + b)"
    );
    let events = drain(&mut rx);
    assert!(events.contains(&AgentChatEvent::Failed {
        kind: AgentErrorKind::UnfinishedReply,
        detail: None,
    }));
    assert!(
        !events.iter().any(|e| matches!(e, AgentChatEvent::UserPersisted { .. })),
        "the user message is not persisted on a failed first attempt"
    );
}

#[tokio::test]
async fn crash_c_completed_turns_persist_and_a_retry_resumes_from_them() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // Turn 1 completes with a tool call; turn 2 then drops mid-text.
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
            },
        },
        Program::DropAfterText {
            chunks: vec!["crash".to_string()],
        },
    ]);
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what is big?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(result, TurnResult::Failed(AgentErrorKind::UnfinishedReply));

    // Turn 1's rows survive: user, assistant(tool_call), and the tool result. The
    // crashed turn 2's assistant row is absent.
    let persisted = store::list_messages(&conn, id, 100, 0).expect("list");
    let roles: Vec<AgentRole> = persisted.iter().map(stored_role).collect();
    assert_eq!(
        roles,
        vec![AgentRole::User, AgentRole::Assistant, AgentRole::Tool],
        "only the completed turn's rows persist"
    );

    // A retry issues a FRESH respond from the persisted transcript (user_text: None),
    // and its assembled prompt includes turn 1's completed rows.
    let retry_llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["Movies is the biggest.".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx2, _rx2) = unbounded_channel();
    let retry = run_turn(
        &retry_llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, None),
        &tx2,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(retry, TurnResult::Answered { .. }), "the retry answers");

    let resumed_prompt = &retry_llm.calls_seen()[0];
    let roles_in_prompt: Vec<AgentRole> = resumed_prompt.iter().map(|m| m.role).collect();
    assert_eq!(
        roles_in_prompt,
        vec![AgentRole::User, AgentRole::Assistant, AgentRole::Tool],
        "the retry's assembled prompt includes the completed turn's rows (fresh respond, not a re-send)"
    );
}

// ── Cost metering ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn cost_is_metered_per_completed_respond_call() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // One tool round (10/5) then a final answer (20/10): two completed respond calls, so
    // the meter must accumulate 30 prompt + 15 completion tokens.
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
            },
        },
        Program::Answer {
            chunks: vec!["done".to_string()],
            usage: AgentUsage {
                prompt_tokens: 20,
                completion_tokens: 10,
            },
        },
    ]);
    let (tx, _rx) = unbounded_channel();

    run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what is big?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    let summary = store::cost_summary(&conn).expect("summary");
    assert_eq!(summary.days.len(), 1, "one day of spend");
    assert_eq!(summary.days[0].prompt_tokens, 30, "both completed calls are metered");
    assert_eq!(summary.days[0].completion_tokens, 15);
}

// ── End-to-end multi-tool turn ────────────────────────────────────────────────

#[tokio::test]
async fn end_to_end_multi_tool_turn_dispatches_and_answers() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![
                (ToolId::ListDir, json!({ "path": "/a" })),
                (ToolId::ListVolumes, json!({})),
            ],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["The biggest is ".to_string(), "Movies.".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what is big?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }));

    // Persisted: user, assistant(2 tool calls), 2 tool results, assistant(answer).
    let roles: Vec<AgentRole> = store::list_messages(&conn, id, 100, 0)
        .expect("list")
        .iter()
        .map(stored_role)
        .collect();
    assert_eq!(
        roles,
        vec![
            AgentRole::User,
            AgentRole::Assistant,
            AgentRole::Tool,
            AgentRole::Tool,
            AgentRole::Assistant,
        ]
    );

    let events = drain(&mut rx);
    let tool_finished = events
        .iter()
        .filter(|e| matches!(e, AgentChatEvent::ToolCallFinished { ok: true, .. }))
        .count();
    assert_eq!(tool_finished, 2, "both tool calls dispatched and finished ok");
    assert!(
        events.iter().any(|e| matches!(e, AgentChatEvent::Done { .. })),
        "a final answer"
    );
    assert!(events.contains(&AgentChatEvent::TextDelta {
        text: "The biggest is ".to_string()
    }));

    // Snapshot-at-send: both respond calls of the loop saw a byte-identical envelope on
    // the latest user turn (the runtime holds one captured envelope across the turn).
    let seen = llm.calls_seen();
    assert_eq!(seen.len(), 2, "two respond calls in the loop");
    assert_eq!(
        leading_text(&seen[0][0]),
        leading_text(&seen[1][0]),
        "the envelope must not shift across the turn's respond calls"
    );
}

// ── A context drop is loud ─────────────────────────────────────────────────────

/// Returns one oversized tool result, big enough that keeping it in history blows a tight
/// prompt budget (the `image_facts` shape that started this).
struct HugeDispatcher;

impl ToolDispatcher for HugeDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "text": "y".repeat(60_000) }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// Returns one oversized result AND records every evidence revocation the runtime asks for,
/// so a test can assert that a dropped result loses its standing as evidence.
#[derive(Default)]
struct RevokeRecordingDispatcher {
    revoked: Mutex<Vec<String>>,
}

impl RevokeRecordingDispatcher {
    fn revoked(&self) -> Vec<String> {
        self.revoked.lock().expect("lock").clone()
    }
}

impl ToolDispatcher for RevokeRecordingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "text": "y".repeat(60_000) }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }

    fn revoke_evidence(&self, call_ids: &[String]) {
        self.revoked.lock().expect("lock").extend_from_slice(call_ids);
    }
}

#[tokio::test]
async fn a_dropped_tool_result_loses_its_standing_as_evidence() {
    // The ledger backing rename evidence vouches for `image_facts` results that were
    // DISPATCHED; assembly decides which ones the model actually reads. So every result the
    // prompt drops must be revoked, or a plan could cite content the model never saw — the
    // original bug wearing a badge.
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = RevokeRecordingDispatcher::default();

    let first = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ImageFacts, json!({ "paths": ["/a.png"] }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["named them".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx1, _rx1) = unbounded_channel();
    run_turn(
        &first,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("name these by content")),
        &tx1,
        &CancellationToken::new(),
    )
    .await;
    assert!(
        dispatcher.revoked().is_empty(),
        "the turn that fetched the facts read them: nothing to revoke"
    );

    // A second turn on a tight budget: turn 1's result is history now, so it goes.
    let second = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["and the rest".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx2, _rx2) = unbounded_channel();
    let mut tight = params(id, Some("now the rest"));
    tight.prompt_budget = 8_000;
    let result = run_turn(
        &second,
        &dispatcher,
        &conn,
        &[],
        &tight,
        &tx2,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, TurnResult::Answered { .. }), "the turn still answers");

    let revoked = dispatcher.revoked();
    assert_eq!(revoked.len(), 1, "exactly the dropped result is revoked: {revoked:?}");
    assert!(
        !revoked[0].is_empty(),
        "the revocation carries the call id the result answered"
    );
}

#[tokio::test]
async fn a_budget_forced_context_drop_is_announced_once_per_turn() {
    // Turn 1 puts a huge tool result in history; turn 2 runs against a tight budget, so
    // that result has to go. The user must be TOLD — an unannounced drop is exactly how a
    // reply written without the evidence read like a normal one.
    let conn = migrated_conn();
    let id = conversation(&conn);

    let first = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ImageFacts, json!({ "paths": ["/a.png"] }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["named them".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx1, _rx1) = unbounded_channel();
    let params1 = params(id, Some("name these by content"));
    run_turn(
        &first,
        &HugeDispatcher,
        &conn,
        &[],
        &params1,
        &tx1,
        &CancellationToken::new(),
    )
    .await;

    let second = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ImageFacts, json!({ "paths": ["/b.png"] }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["named those too".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx2, mut rx2) = unbounded_channel();
    let mut params2 = params(id, Some("now the rest"));
    params2.prompt_budget = 8_000;
    let result = run_turn(
        &second,
        &HugeDispatcher,
        &conn,
        &[],
        &params2,
        &tx2,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, TurnResult::Answered { .. }), "the turn still answers");

    let trims: Vec<AgentChatEvent> = drain(&mut rx2)
        .into_iter()
        .filter(|e| matches!(e, AgentChatEvent::ContextTrimmed { .. }))
        .collect();
    assert_eq!(
        trims.len(),
        1,
        "exactly one notice per turn, however many respond calls it took: {trims:?}"
    );
    let AgentChatEvent::ContextTrimmed {
        elided_results,
        approx_tokens,
    } = &trims[0]
    else {
        unreachable!("filtered above")
    };
    assert_eq!(*elided_results, 1, "the older batch is what went");
    assert!(*approx_tokens > 0, "the notice sizes what was dropped");
}

/// The gauge is driven by one event per turn carrying the assembly's own numbers. Two events
/// would make the bar jump mid-turn; zero would leave it stale after a turn that grew the chat.
#[tokio::test]
async fn context_usage_is_reported_once_per_turn_with_the_assemblys_numbers() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ImageFacts, json!({ "paths": ["/a.png"] }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["named it".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx, mut rx) = unbounded_channel();
    let mut turn_params = params(id, Some("name this"));
    turn_params.prompt_budget = 16_000;

    let result = run_turn(
        &llm,
        &HugeDispatcher,
        &conn,
        &[],
        &turn_params,
        &tx,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, TurnResult::Answered { .. }), "the turn answers");

    let reports: Vec<AgentChatEvent> = drain(&mut rx)
        .into_iter()
        .filter(|e| matches!(e, AgentChatEvent::ContextUsage { .. }))
        .collect();
    assert_eq!(
        reports.len(),
        1,
        "exactly one usage report per turn, however many respond calls it took: {reports:?}"
    );
    let AgentChatEvent::ContextUsage {
        estimated_tokens,
        budget_tokens,
        ..
    } = &reports[0]
    else {
        unreachable!("filtered above")
    };
    assert_eq!(
        *budget_tokens, 16_000,
        "the report names the budget it was assembled against"
    );
    assert!(
        *estimated_tokens > 0,
        "a real assembly costs something; a zero would render as an empty bar"
    );

    // And it outlives the turn, so reopening the thread shows the last known figure.
    assert_eq!(
        crate::agent::store::conversation_context_usage(&conn, id).expect("read stored usage"),
        Some((*estimated_tokens, *budget_tokens)),
        "the figure the user saw is the figure the thread remembers"
    );
}

/// A turn that never finishes reports no usage: the user is looking at an error line, and the
/// previous turn's stored figure stays the last thing actually measured.
#[tokio::test]
async fn a_failed_turn_reports_no_usage_and_leaves_the_stored_figure_alone() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    crate::agent::store::set_conversation_context_usage(&conn, id, 4_200, 16_000).expect("seed a prior turn");

    let llm = ProgrammableLlm::new(vec![Program::ErrorAfterText {
        chunks: vec!["thinking".to_string()],
        error: AgentLlmError::Unavailable,
    }]);
    let (tx, mut rx) = unbounded_channel();
    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hello")),
        &tx,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, TurnResult::Failed(_)), "the turn fails");

    assert!(
        !drain(&mut rx)
            .iter()
            .any(|e| matches!(e, AgentChatEvent::ContextUsage { .. })),
        "a failed turn measured nothing worth showing"
    );
    assert_eq!(
        crate::agent::store::conversation_context_usage(&conn, id).expect("read stored usage"),
        Some((4_200, 16_000)),
        "the last real measurement survives a failed turn"
    );
}

#[tokio::test]
async fn a_turn_that_fits_its_budget_announces_nothing() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let events = run_answer_turn(&conn, id, "model-one", "hello").await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentChatEvent::ContextTrimmed { .. })),
        "no notice when nothing was dropped — it would cry wolf on every turn"
    );
}

// ── Model-change events ───────────────────────────────────────────────────────

/// Run one single-answer turn for `id` with the given model, returning the drained events.
async fn run_answer_turn(conn: &Connection, id: i64, model: &str, user_text: &str) -> Vec<AgentChatEvent> {
    let llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["ok".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx, mut rx) = unbounded_channel();
    let mut params = params(id, Some(user_text));
    params.model = model.to_string();
    let result = run_turn(&llm, &OkDispatcher, conn, &[], &params, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }), "turn answers");
    drain(&mut rx)
}

#[tokio::test]
async fn first_turn_stamps_last_model_without_an_event() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let events = run_answer_turn(&conn, id, "model-one", "hello").await;

    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-one".to_string())
    );
    assert!(
        !events.iter().any(|e| matches!(e, AgentChatEvent::ModelChanged { .. })),
        "no event on a conversation's first turn — there is nothing to switch from"
    );
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(
        !rows.iter().any(|m| matches!(m.content, store::StoredContent::Event(_))),
        "no event row persists either"
    );
}

#[tokio::test]
async fn a_model_change_between_turns_logs_an_event_row_before_the_user_message() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn(&conn, id, "model-one", "hello").await;
    let events = run_answer_turn(&conn, id, "model-two", "again").await;

    // The event row sits between the turns: user, assistant, EVENT, user, assistant.
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    let shapes: Vec<&str> = rows
        .iter()
        .map(|m| match &m.content {
            store::StoredContent::Message { role, .. } => match role {
                AgentRole::User => "user",
                AgentRole::Assistant => "assistant",
                _ => "other",
            },
            store::StoredContent::Event(store::ConversationEvent::ModelChanged { .. }) => "event",
        })
        .collect();
    assert_eq!(shapes, vec!["user", "assistant", "event", "user", "assistant"]);
    match &rows[2].content {
        store::StoredContent::Event(store::ConversationEvent::ModelChanged { model }) => {
            assert_eq!(model, "model-two");
        }
        other => panic!("expected the model-change event, got {other:?}"),
    }

    // The live rail heard about it, with the persisted row's identity.
    assert!(events.iter().any(|e| matches!(
        e,
        AgentChatEvent::ModelChanged { message_id, seq, model }
            if *message_id == rows[2].id && *seq == rows[2].seq && model == "model-two"
    )));
    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-two".to_string())
    );
}

#[tokio::test]
async fn the_same_model_between_turns_logs_no_event() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn(&conn, id, "model-one", "hello").await;
    let events = run_answer_turn(&conn, id, "model-one", "again").await;

    assert!(!events.iter().any(|e| matches!(e, AgentChatEvent::ModelChanged { .. })));
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(!rows.iter().any(|m| matches!(m.content, store::StoredContent::Event(_))));
}

#[tokio::test]
async fn a_failed_first_attempt_records_no_event_and_leaves_last_model_untouched() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn(&conn, id, "model-one", "hello").await;

    // A turn with a new model whose respond never opens: nothing persists (crash case
    // b), including the model transition — the next successful turn records it instead.
    let llm = ProgrammableLlm::new(vec![]);
    let (tx, _rx) = unbounded_channel();
    let mut failing = params(id, Some("again"));
    failing.model = "model-two".to_string();
    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &failing,
        &tx,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Provider));

    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(!rows.iter().any(|m| matches!(m.content, store::StoredContent::Event(_))));
    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-one".to_string())
    );
}

// ── The live record path (settings changed while a thread is open) ─────────────

/// A `ChatRuntime` over a temp-dir `main.db` with one conversation stamped to
/// `model-one`, as if one turn had completed.
pub(super) fn runtime_with_stamped_conversation() -> (tempfile::TempDir, ChatRuntime, i64) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = store::main_db_path(dir.path());
    let conn = store::open_write_connection(&db).expect("open");
    let id = store::create_conversation(&conn, "t", 100, None).expect("create");
    store::set_conversation_last_model(&conn, id, "model-one").expect("stamp");
    drop(conn);
    let runtime = ChatRuntime::new(db);
    (dir, runtime, id)
}

#[tokio::test]
async fn record_model_change_appends_an_event_and_stamps() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    let recorded = runtime
        .record_model_change(id, "model-two")
        .await
        .expect("record")
        .expect("an event was recorded");

    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    let rows = store::list_messages(&conn, id, 10, 0).expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, recorded.0);
    assert_eq!(rows[0].seq, recorded.1);
    assert_eq!(
        rows[0].content,
        store::StoredContent::Event(store::ConversationEvent::ModelChanged {
            model: "model-two".to_string()
        })
    );
    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-two".to_string())
    );
}

#[tokio::test]
async fn record_model_change_is_a_noop_when_unchanged_or_unstarted() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    // Same effective model (e.g. the changed setting was masked by the override): no row.
    assert!(
        runtime
            .record_model_change(id, "model-one")
            .await
            .expect("record")
            .is_none()
    );

    // A conversation with no completed turn yet: nothing to switch from, no row.
    let conn = store::open_write_connection(&runtime.db_path).expect("open");
    let fresh = store::create_conversation(&conn, "fresh", 100, None).expect("create");
    drop(conn);
    assert!(
        runtime
            .record_model_change(fresh, "model-two")
            .await
            .expect("record")
            .is_none()
    );
    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    assert!(store::list_messages(&conn, fresh, 10, 0).expect("list").is_empty());
    assert_eq!(
        store::conversation_last_model(&conn, fresh).expect("get"),
        None,
        "the first real turn stamps the model, not the settings change"
    );
}

#[tokio::test]
async fn record_model_change_waits_for_the_in_flight_turn() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    // Hold the conversation's single-flight lock like an in-flight turn does.
    let guard = runtime
        .locks
        .lock_for(id)
        .try_lock_owned()
        .expect("lock is free before the fake turn");

    let fut = runtime.record_model_change(id, "model-two");
    tokio::pin!(fut);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut fut).await.is_err(),
        "the record waits while the turn is in flight"
    );

    drop(guard); // the turn finishes
    let recorded = fut.await.expect("record").expect("event recorded after the turn");
    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    let rows = store::list_messages(&conn, id, 10, 0).expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, recorded.0, "the event landed only after the lock released");
}

// ── Typed error surface ───────────────────────────────────────────────────────

#[test]
fn agent_llm_errors_map_to_typed_kinds() {
    // The seam carries TYPED error kinds, not strings. The user-facing copy (rendered
    // without the words "error"/"failed") is the frontend's job; here we pin the
    // total, variant-to-variant mapping.
    use AgentErrorKind as K;
    use AgentLlmError as E;
    assert_eq!(K::from(E::NoKey), K::NoKey);
    assert_eq!(K::from(E::NotConfigured), K::NotConfigured);
    assert_eq!(K::from(E::Unavailable), K::Unavailable);
    assert_eq!(K::from(E::Timeout), K::Timeout);
    assert_eq!(K::from(E::AuthFailed("bad key".into())), K::AuthFailed);
    assert_eq!(K::from(E::RateLimited("slow down".into())), K::RateLimited);
    assert_eq!(K::from(E::BudgetExhausted), K::BudgetExhausted);
    assert_eq!(K::from(E::Provider("detail".into())), K::Provider);
}

#[tokio::test]
async fn a_pre_stream_provider_error_persists_nothing_and_is_typed() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // The respond call fails before opening a stream (no key). Nothing persists (case b).
    let llm = ProgrammableLlm::new(vec![]); // exhausted script → provider error on first call
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hello")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Provider));
    assert!(store::list_messages(&conn, id, 100, 0).expect("list").is_empty());
    // The event carries the provider's own wording so the UI can show the user what to
    // fix (display only — the frontend still branches on `kind`, never on this string).
    assert!(drain(&mut rx).contains(&AgentChatEvent::Failed {
        kind: AgentErrorKind::Provider,
        detail: Some("programmable: script exhausted".to_string()),
    }));
}

#[tokio::test]
async fn a_mid_stream_provider_error_carries_its_detail() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::ErrorAfterText {
        chunks: vec!["partial".to_string()],
        error: AgentLlmError::Provider("HTTP 404: model gone".to_string()),
    }]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hello")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Provider));
    assert!(drain(&mut rx).contains(&AgentChatEvent::Failed {
        kind: AgentErrorKind::Provider,
        detail: Some("HTTP 404: model gone".to_string()),
    }));
}

// ── Attachments reach the LLM in the envelope (and nothing more) ────────────────

#[tokio::test]
async fn attachments_reach_the_llm_in_the_envelope_and_nothing_more() {
    use crate::agent::chat::context::{AttachmentKind, EnvelopeAttachment};

    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["ok".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx, _rx) = unbounded_channel();

    let env = ContextEnvelope {
        captured_at: 1_780_000_000,
        focused_pane_path: Some("~/Documents".to_string()),
        cursor_item: None,
        selection_count: 0,
        volumes: vec![],
        attachments: vec![EnvelopeAttachment {
            path: "/Users/d/report.pdf".to_string(),
            kind: AttachmentKind::File,
        }],
        denied_names: vec![],
        rename_batch_files: 101,
    };
    let params = TurnParams {
        conversation_id: id,
        user: Some(UserTurn::Text("summarize this")),
        cmdr_md: None,
        envelope: &env,
        offset: offset(),
        now_secs: 1_780_000_000,
        provider: ProviderTag::Local,
        model: "fake-model".to_string(),
        prompt_budget: TEST_PROMPT_BUDGET,
    };

    let result = run_turn(&llm, &OkDispatcher, &conn, &[], &params, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }));

    // The prompt the LLM actually saw carries the attachment on the user turn.
    let seen = llm.calls_seen();
    let messages = &seen[0];
    let user_turn = messages
        .iter()
        .rev()
        .find(|m| m.role == AgentRole::User)
        .expect("a user turn");
    let opening = leading_text(user_turn);
    assert!(
        opening.contains("attached: /Users/d/report.pdf (file)"),
        "the envelope names the attachment path + kind: {opening}"
    );
    // Path + kind and NOTHING else — no size, no bytes, no file contents field.
    let joined: String = messages
        .iter()
        .flat_map(|m| m.parts.iter())
        .filter_map(|p| match p {
            AgentPart::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !joined.to_lowercase().contains("content"),
        "no file contents reach the prompt: {joined}"
    );
}

/// The consent gate is STRUCTURAL: a send with no/stale consent never reaches the LLM.
/// This mirrors `ask_cmdr_send_message`'s control flow — gate on `has_current_consent`,
/// then drive `run_turn` only when it opens — and proves the fake records ZERO calls when
/// the gate refuses, and exactly one when it opens (so the empty case is meaningful).
#[tokio::test]
async fn a_send_without_current_consent_never_calls_the_llm() {
    use crate::agent::consent::{CONSENT_COPY_VERSION, has_current_consent};

    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["hi".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx, _rx) = unbounded_channel();

    // No consent recorded, then a STALE copy version — both keep the gate closed.
    assert!(!has_current_consent(&conn), "no consent record ⇒ gate closed");
    store::set_consent(&conn, CONSENT_COPY_VERSION.wrapping_sub(1), 1_780_000_000).expect("set stale consent");
    assert!(!has_current_consent(&conn), "a stale copy version ⇒ gate closed");

    // The command skips `run_turn` while the gate is closed, so the LLM is never called.
    if has_current_consent(&conn) {
        run_turn(
            &llm,
            &OkDispatcher,
            &conn,
            &[],
            &params(id, Some("hi")),
            &tx,
            &CancellationToken::new(),
        )
        .await;
    }
    assert!(llm.calls_seen().is_empty(), "a refused send makes ZERO LLM calls");

    // Accepting the CURRENT copy opens the gate; the send then drives the LLM once.
    store::set_consent(&conn, CONSENT_COPY_VERSION, 1_780_000_000).expect("set current consent");
    assert!(has_current_consent(&conn), "current consent ⇒ gate open");
    if has_current_consent(&conn) {
        run_turn(
            &llm,
            &OkDispatcher,
            &conn,
            &[],
            &params(id, Some("hi")),
            &tx,
            &CancellationToken::new(),
        )
        .await;
    }
    assert_eq!(
        llm.calls_seen().len(),
        1,
        "with consent, the send drives the LLM exactly once"
    );
}

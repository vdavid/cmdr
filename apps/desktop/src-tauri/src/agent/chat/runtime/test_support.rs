//! Shared fixtures and common re-exports for the chat-runtime test modules: an in-memory
//! store, the [`TurnParams`] every test assembles against, a programmable LLM double, and
//! the scripted dispatchers. Each `*_tests.rs` file (`tests.rs`, `context_budget_tests.rs`,
//! `model_change_tests.rs`, `wake_tests.rs`) does `use super::*;` (the runtime's own items)
//! plus `use super::test_support::*;` (these helpers and the common types), so the test
//! bodies read the same as when they lived in one file.
//!
//! Tool dispatch is exercised through a scripted [`ToolDispatcher`] double (there is no
//! in-tree full-Tauri harness for the agent toolset at unit-test scope), and the LLM
//! through a local [`ProgrammableLlm`] that gives per-turn control over text, tool calls,
//! usage, and a mid-stream drop (no `End`) for the crash cases.

use std::collections::VecDeque;

use futures_util::stream;

use super::*;
use crate::agent::llm::AgentDeltaStream;

// Common re-exports the test bodies reference by bare name (they also serve
// test_support's own helpers below).
pub(super) use std::sync::Mutex;
pub(super) use std::time::Duration;

pub(super) use chrono::FixedOffset;
pub(super) use futures_util::future::{BoxFuture, FutureExt};
pub(super) use futures_util::stream::StreamExt;
pub(super) use rusqlite::Connection;
pub(super) use serde_json::{Value, json};
pub(super) use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
pub(super) use tokio_util::sync::CancellationToken;

pub(super) use crate::agent::chat::context::ContextEnvelope;
pub(super) use crate::agent::llm::types::{
    AgentDelta, AgentLlmError, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolCall, AgentToolResult,
    AgentUsage, ProviderTag, ToolDeclaration, ToolId,
};
pub(super) use crate::agent::store;
pub(super) use crate::agent::types::TurnOrigin;

// ── Fixtures ──────────────────────────────────────────────────────────────────

/// The origin a test's opener stands for. Production callers state it; a test that isn't
/// about origins takes the one its opener implies.
pub(super) fn origin_of(user: Option<&UserTurn<'_>>) -> TurnOrigin {
    match user {
        Some(UserTurn::Text(_)) => TurnOrigin::Text,
        Some(UserTurn::Wake(_)) => TurnOrigin::Wake,
        Some(UserTurn::Outcomes(_)) => TurnOrigin::Outcomes,
        None => TurnOrigin::Resume,
    }
}

/// The prompt budget the turn fixtures assemble against: the conservative default, so no
/// test trips budget pressure unless it means to.
pub(super) const TEST_PROMPT_BUDGET: usize = crate::agent::chat::budget::DEFAULT_PROMPT_TOKEN_BUDGET;

pub(super) fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    store::run_migrations(&conn, store::MIGRATIONS).expect("migrate");
    conn
}

pub(super) fn offset() -> FixedOffset {
    FixedOffset::east_opt(2 * 3600).expect("valid offset")
}

pub(super) fn envelope() -> ContextEnvelope {
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
    let user = user_text.map(UserTurn::Text);
    TurnParams {
        conversation_id,
        origin: origin_of(user.as_ref()),
        user,
        cmdr_md: None,
        memory: &[],
        envelope: LEAK_ENVELOPE.get_or_init(envelope),
        offset: offset(),
        now_secs: 1_780_000_000,
        provider: ProviderTag::Local,
        model: "fake-model".to_string(),
        prompt_budget: TEST_PROMPT_BUDGET,
        chat_memory_tokens: TEST_PROMPT_BUDGET,
        call: crate::agent::llm::CallSettings::default(),
    }
}

// A single leaked envelope so `params` can hand out a `&'a ContextEnvelope` without the
// caller juggling a binding; the value is constant across the test run.
static LEAK_ENVELOPE: std::sync::OnceLock<ContextEnvelope> = std::sync::OnceLock::new();

pub(super) fn conversation(conn: &Connection) -> i64 {
    store::create_conversation(conn, "Test thread", 1_780_000_000, None).expect("create conversation")
}

pub(super) fn drain(rx: &mut UnboundedReceiver<AgentChatEvent>) -> Vec<AgentChatEvent> {
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

/// The transcript roles of a thread's rows, skipping the turn's own `TurnFinished` stats row
/// (`line_events_tests.rs` asserts those) and panicking on any other event row, like
/// [`stored_role`].
pub(super) fn transcript_roles(messages: &[store::StoredMessage]) -> Vec<AgentRole> {
    messages
        .iter()
        .filter(|m| {
            !matches!(
                m.content,
                store::StoredContent::Event(store::ConversationEvent::TurnFinished { .. })
            )
        })
        .map(stored_role)
        .collect()
}

/// The text a message opens with: a user turn's envelope (persisted with it), or plain text.
pub(super) fn leading_text(message: &AgentMessage) -> &str {
    match &message.parts[0] {
        AgentPart::Text(text) | AgentPart::Envelope(text) => text,
        _ => panic!("expected a leading text part"),
    }
}

// ── A programmable LLM (per-turn text / tools / usage / mid-stream drop) ───────

pub(super) enum Program {
    /// Stream these text chunks, then a clean `End` (Completed) with this usage.
    Answer { chunks: Vec<String>, usage: AgentUsage },
    /// Emit these tool calls, then a clean `End` (ToolCall) with this usage.
    Tools {
        calls: Vec<(ToolId, Value)>,
        usage: AgentUsage,
    },
    /// Start a call under `stray_call_id` that the final message doesn't hold (what a nameless
    /// call the provider seam dropped looks like), then emit `calls` as [`Program::Tools`] does.
    ToolsAfterStrayStart {
        stray_call_id: String,
        calls: Vec<(ToolId, Value)>,
        usage: AgentUsage,
    },
    /// Stream text chunks then END THE STREAM with no `End` delta (a mid-stream drop).
    DropAfterText { chunks: Vec<String> },
    /// Stream text chunks then yield a typed stream error (a mid-stream provider problem).
    ErrorAfterText { chunks: Vec<String>, error: AgentLlmError },
}

pub(super) struct ProgrammableLlm {
    turns: Mutex<VecDeque<Program>>,
    calls_seen: Mutex<Vec<Vec<AgentMessage>>>,
    prefixes_seen: Mutex<Vec<(String, Vec<ToolDeclaration>)>>,
}

impl ProgrammableLlm {
    pub(super) fn new(programs: Vec<Program>) -> Self {
        Self {
            turns: Mutex::new(programs.into()),
            calls_seen: Mutex::new(Vec::new()),
            prefixes_seen: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn calls_seen(&self) -> Vec<Vec<AgentMessage>> {
        self.calls_seen.lock().expect("lock").clone()
    }

    /// The `system` string and tool declarations of every call, in call order: the span a
    /// caching provider keys on.
    pub(super) fn prefixes_seen(&self) -> Vec<(String, Vec<ToolDeclaration>)> {
        self.prefixes_seen.lock().expect("lock").clone()
    }
}

impl AgentLlm for ProgrammableLlm {
    fn respond<'a>(
        &'a self,
        system: &'a str,
        tools: &'a [ToolDeclaration],
        messages: &'a [AgentMessage],
        _call: crate::agent::llm::CallSettings,
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<AgentDeltaStream, AgentLlmError>> {
        async move {
            self.calls_seen.lock().expect("lock").push(messages.to_vec());
            self.prefixes_seen
                .lock()
                .expect("lock")
                .push((system.to_string(), tools.to_vec()));
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
        Program::ToolsAfterStrayStart {
            stray_call_id,
            calls,
            usage,
        } => {
            let mut deltas = vec![Ok(AgentDelta::ToolCallStarted {
                call_id: stray_call_id,
                tool: ToolId::Unrecognized(String::new()),
            })];
            deltas.extend(program_to_deltas(Program::Tools { calls, usage }));
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
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
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

/// Answers every call with a typed handler problem, and records what it was actually asked
/// to run. The counterpart to [`OkDispatcher`] for the repeat breaker: what it dispatched is
/// the whole assertion, because a call the breaker stops never reaches it.
#[derive(Default)]
pub(super) struct FailingDispatcher {
    dispatched: Mutex<Vec<(String, Value)>>,
}

impl FailingDispatcher {
    pub(super) fn dispatched(&self) -> Vec<(String, Value)> {
        self.dispatched.lock().expect("lock").clone()
    }
}

impl ToolDispatcher for FailingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            self.dispatched
                .lock()
                .expect("lock")
                .push((call.tool.as_wire_name().to_string(), call.arguments.clone()));
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "problem": "list_dir needs path. It takes limit and path." }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// Records every call it is asked to run and answers each one successfully. Pins that a
/// SUCCESSFUL call re-issued (the re-fetch the system prompt tells the model to make after a
/// result is set aside) is never mistaken for a stuck repeat.
#[derive(Default)]
pub(super) struct CountingOkDispatcher {
    dispatched: Mutex<Vec<(String, Value)>>,
}

impl CountingOkDispatcher {
    pub(super) fn dispatched(&self) -> Vec<(String, Value)> {
        self.dispatched.lock().expect("lock").clone()
    }
}

impl ToolDispatcher for CountingOkDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            self.dispatched
                .lock()
                .expect("lock")
                .push((call.tool.as_wire_name().to_string(), call.arguments.clone()));
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
pub(super) struct SleepingDispatcher {
    pub(super) secs: u64,
}

impl ToolDispatcher for SleepingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
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
pub(super) struct CancellingDispatcher {
    pub(super) token: CancellationToken,
}

impl ToolDispatcher for CancellingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
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

/// Waits for the token the TURN hands it, the way a live `list_dir` read waits on a slow
/// volume: it answers only once Stop fires, naming the folder it was reading. `reached` says
/// the call is in flight.
#[derive(Default)]
pub(super) struct StopAwareDispatcher {
    pub(super) reached: tokio::sync::Notify,
}

impl ToolDispatcher for StopAwareDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            self.reached.notify_one();
            turn.cancel.cancelled().await;
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "miss": "cancelled", "location": { "path": "/Volumes/naspi-1" } }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// Holds its call open and never looks at the token: `reached` says the call is in flight,
/// and `release` lets it answer `answer`, or never when there's none.
pub(super) struct HeldDispatcher {
    pub(super) reached: tokio::sync::Notify,
    pub(super) release: tokio::sync::Notify,
    answer: Option<Value>,
}

impl HeldDispatcher {
    /// A handler stuck for good: a hung mount, a server that never answers.
    pub(super) fn forever() -> Self {
        Self {
            reached: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            answer: None,
        }
    }

    /// A handler mid-write that answers `answer` once released.
    pub(super) fn until_released(answer: Value) -> Self {
        Self {
            answer: Some(answer),
            ..Self::forever()
        }
    }
}

impl ToolDispatcher for HeldDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            self.reached.notify_one();
            let Some(answer) = &self.answer else {
                return std::future::pending().await;
            };
            self.release.notified().await;
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: answer.clone(),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// Records the origin each call was handed, and answers it.
#[derive(Default)]
pub(super) struct OriginRecordingDispatcher {
    seen: Mutex<Vec<TurnOrigin>>,
}

impl OriginRecordingDispatcher {
    pub(super) fn seen(&self) -> Vec<TurnOrigin> {
        self.seen.lock().expect("lock").clone()
    }
}

impl ToolDispatcher for OriginRecordingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            self.seen.lock().expect("lock").push(turn.origin);
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

// ── Turn helpers the subject files share ──────────────────────────────────────

/// Run one single-answer turn for `id` with the given model, returning the drained events.
pub(super) async fn run_answer_turn(conn: &Connection, id: i64, model: &str, user_text: &str) -> Vec<AgentChatEvent> {
    run_answer_turn_sized(conn, id, model, TEST_PROMPT_BUDGET, user_text).await
}

/// The same, with the turn's chat-memory size (its resolved prompt budget) spelled out, for
/// the tests that are about the budget moving between turns.
pub(super) async fn run_answer_turn_sized(
    conn: &Connection,
    id: i64,
    model: &str,
    chat_memory_tokens: usize,
    user_text: &str,
) -> Vec<AgentChatEvent> {
    run_answer_turn_budgeted(conn, id, model, chat_memory_tokens, chat_memory_tokens, user_text).await
}

/// The same, with the actual prompt budget apart from the chat memory the timeline names, for
/// the tests about the budget moving for reasons the user didn't cause (an effort, a late report).
pub(super) async fn run_answer_turn_budgeted(
    conn: &Connection,
    id: i64,
    model: &str,
    prompt_budget: usize,
    chat_memory_tokens: usize,
    user_text: &str,
) -> Vec<AgentChatEvent> {
    let llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["ok".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx, mut rx) = unbounded_channel();
    let mut params = params(id, Some(user_text));
    params.model = model.to_string();
    params.prompt_budget = prompt_budget;
    params.chat_memory_tokens = chat_memory_tokens;
    let result = run_turn(&llm, &OkDispatcher, conn, &[], &params, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }), "turn answers");
    drain(&mut rx)
}

/// The chat-memory size `runtime_with_stamped_conversation` leaves stamped, so the live-path
/// tests have a value to change away from.
pub(super) const STAMPED_CHAT_MEMORY: usize = 60_000;

/// A `ChatRuntime` over a temp-dir `main.db` with one conversation stamped to `model-one`
/// and [`STAMPED_CHAT_MEMORY`], as if one turn had completed.
pub(super) fn runtime_with_stamped_conversation() -> (tempfile::TempDir, ChatRuntime, i64) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = store::main_db_path(dir.path());
    let conn = store::open_write_connection(&db).expect("open");
    let id = store::create_conversation(&conn, "t", 100, None).expect("create");
    store::set_conversation_last_model(&conn, id, "model-one").expect("stamp");
    store::set_conversation_last_chat_memory(&conn, id, STAMPED_CHAT_MEMORY).expect("stamp");
    drop(conn);
    let runtime = ChatRuntime::new(db);
    (dir, runtime, id)
}

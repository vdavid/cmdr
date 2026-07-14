//! The chat runtime: drives one user message to an answer, safely and within budgets.
//!
//! It owns the parts of the flow that must be correct under failure and concurrency:
//! single-flight per thread, the per-message budgets that make a runaway loop
//! impossible by construction, cancellation at tool boundaries plus stream-cancel, the
//! typed error surface, and the crash-safe persistence model. The pure prompt building
//! lives in [`context`]; this module is the I/O-and-time half.
//!
//! ## The event seam (what the IPC layer subscribes to)
//!
//! Progress is emitted as typed [`AgentChatEvent`]s through a plain
//! `UnboundedSender` ([`ChatEventSink`]). The Tauri command is a thin adapter: it makes
//! a channel, spawns a task forwarding each [`AgentChatEvent`] onto a
//! `tauri::ipc::Channel` (mapping to the wire `AskCmdrStreamEvent`), and passes the
//! sender here. Nothing in this module knows about Tauri IPC.
//!
//! ## Crash / mid-stream persistence (spec §2.3)
//!
//! Continuity is through DB state, so partial state must be unambiguous. A message's
//! `content_blocks` are written only on that call's `End`:
//! - (a) assistant text streamed before a non-`End` termination (drop, crash) is
//!   discarded — no assistant row — and the UI gets [`AgentErrorKind::UnfinishedReply`].
//! - (b) a user message whose FIRST `respond` never reached `End` records nothing, so a
//!   re-send assembles byte-identically the same prompt (the user row is written on the
//!   first `End`, not at send).
//! - (c) an interrupted multi-turn loop keeps every completed turn's rows (each written
//!   on its own `End`); a retry resumes with a FRESH `respond` from the persisted
//!   transcript (call [`run_turn`] with `user_text: None`).
//! - (d) cost is metered per completed `respond` `End` (usage folded via `record_cost`),
//!   so completed turns count once and are never lost.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{FixedOffset, TimeZone, Utc};
use futures_util::StreamExt;
use futures_util::future::{BoxFuture, FutureExt};
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{
    AgentDelta, AgentLlmError, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolCall, AgentToolResult,
    AgentUsage, ProviderTag, ToolDeclaration, ToolId,
};
use crate::agent::store::{self, AgentStoreError, CostRecord};
use crate::ignore_poison::IgnorePoison;

use super::context::{self, ContextEnvelope, MAX_TOOL_TURNS, MAX_WALL_TIME, PrefixInputs};
use super::system_prompt::SYSTEM_PROMPT;

const LOG_TARGET: &str = "agent::chat";

// ── The event seam ────────────────────────────────────────────────────────────

/// The sink the runtime emits progress through. A plain unbounded channel; the IPC command forwards
/// it to a Tauri `Channel`. Send failures (a closed receiver, e.g. the rail was closed)
/// are ignored — the turn keeps running to persist its state.
pub type ChatEventSink = UnboundedSender<AgentChatEvent>;

/// One typed progress event, mirroring plan §7's stream events minus the IPC specifics.
/// The frontend gets DISPLAY parts only: no reasoning blob and no provider state ever
/// ride here.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentChatEvent {
    /// A send arrived while this thread's loop was running; it will start once the
    /// running one finishes. Drives the "working… stop?" affordance.
    Queued,
    /// The user's message was persisted (written on the first `respond` `End`).
    UserPersisted { message_id: i64, seq: i64 },
    /// A new assistant turn began streaming. Carries no id by design: `content_blocks`
    /// are written only on `End`, so no row exists yet (crash case a). The final id
    /// arrives with [`AgentChatEvent::Done`].
    AssistantStarted,
    /// A chunk of assistant text arrived.
    TextDelta { text: String },
    /// Opaque reasoning progressed; the UI shows "thinking…", content never surfaced.
    ReasoningTick,
    /// The model started a tool call (surfaced as a collapsible "looked at X" line).
    ToolCallStarted { call_id: String, tool: ToolId },
    /// A tool call finished dispatching. `ok` is false for a refusal or a handler
    /// problem (inspected from OUR OWN typed result shape, not external wording).
    ToolCallFinished { call_id: String, ok: bool },
    /// The turn produced its final answer. Carries the persisted assistant message id.
    Done {
        message_id: i64,
        seq: i64,
        stop: AgentStopReason,
        usage: AgentUsage,
    },
    /// The turn ended without an answer, honestly and typed. Rendered by the frontend
    /// without the words "error"/"failed" (the frontend owns the copy). `detail` is the
    /// source error's own wording — shown under the typed headline so the user sees what
    /// to fix (a retired model slug, a quota reset time); display only, never control flow.
    Failed {
        kind: AgentErrorKind,
        detail: Option<String>,
    },
    /// The conversation's effective model changed since its previous turn; a UI-facing
    /// event row was persisted (its identity rides along). The rail shows it as a small
    /// timeline line before this turn's user message.
    ModelChanged { message_id: i64, seq: i64, model: String },
}

/// The typed reasons a turn can end without an answer. A pure classification the
/// frontend renders honestly; never a matched string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentErrorKind {
    /// No API key configured for the selected provider.
    NoKey,
    /// The provider/model slot is not configured.
    NotConfigured,
    /// The provider was unreachable.
    Unavailable,
    /// The request timed out.
    Timeout,
    /// The provider rejected the API key.
    AuthFailed,
    /// The provider is rate-limiting or out of quota.
    RateLimited,
    /// A per-message budget (tool turns or wall time) was exhausted before an answer.
    BudgetExhausted,
    /// The reply stream ended before completing (a provider drop or a crash mid-stream).
    UnfinishedReply,
    /// Any other provider-side problem; detail is logged, never carried in the type.
    Provider,
}

impl From<AgentLlmError> for AgentErrorKind {
    fn from(error: AgentLlmError) -> Self {
        match error {
            AgentLlmError::NoKey => AgentErrorKind::NoKey,
            AgentLlmError::NotConfigured => AgentErrorKind::NotConfigured,
            AgentLlmError::Unavailable => AgentErrorKind::Unavailable,
            AgentLlmError::Timeout => AgentErrorKind::Timeout,
            AgentLlmError::AuthFailed(_) => AgentErrorKind::AuthFailed,
            AgentLlmError::RateLimited(_) => AgentErrorKind::RateLimited,
            AgentLlmError::BudgetExhausted => AgentErrorKind::BudgetExhausted,
            AgentLlmError::Provider(_) => AgentErrorKind::Provider,
        }
    }
}

fn emit(sink: &ChatEventSink, event: AgentChatEvent) {
    let _ = sink.send(event);
}

// ── The tool-dispatch seam ────────────────────────────────────────────────────

/// How the runtime executes one tool call. The real impl routes through the agent's
/// read-only dispatch view ([`AppHandleDispatcher`]); tests inject a scripted double,
/// so [`run_turn`] needs no Tauri app.
pub trait ToolDispatcher: Send + Sync {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, AgentToolResult>;
}

/// The production dispatcher: every call goes through `agent::tools::view::dispatch`,
/// the read-only choke point (an unknown or write name is refused before `execute_tool`).
pub struct AppHandleDispatcher<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> AppHandleDispatcher<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> ToolDispatcher for AppHandleDispatcher<R> {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, AgentToolResult> {
        async move { crate::agent::tools::view::dispatch(&self.app, call).await }.boxed()
    }
}

/// True when a dispatch result is a real answer rather than a refusal or a handler
/// problem. Reads OUR OWN typed result keys (`available` / `problem`), never external
/// wording — so no `no-string-matching` conflict.
fn dispatch_ok(result: &AgentToolResult) -> bool {
    let refused = result.content.get("available") == Some(&Value::Bool(false));
    let problem = result.content.get("problem").is_some();
    !(refused || problem)
}

// ── Single-flight per thread ──────────────────────────────────────────────────

/// Per-conversation async locks so one thread runs a single loop at a time. A second
/// send for the same conversation emits [`AgentChatEvent::Queued`] and then awaits the
/// lock — it runs once the first finishes (true queuing, not a reject).
#[derive(Default)]
pub struct ConversationLocks {
    locks: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
}

impl ConversationLocks {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_for(&self, conversation_id: i64) -> Arc<tokio::sync::Mutex<()>> {
        self.locks
            .lock_ignore_poison()
            .entry(conversation_id)
            .or_default()
            .clone()
    }

    /// Acquire the conversation's lock. If it is already held, emit `Queued` first, then
    /// wait. The returned guard releases on drop.
    pub async fn acquire(&self, conversation_id: i64, sink: &ChatEventSink) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = self.lock_for(conversation_id);
        if let Ok(guard) = lock.clone().try_lock_owned() {
            return guard;
        }
        emit(sink, AgentChatEvent::Queued);
        lock.lock_owned().await
    }

    /// Acquire the conversation's lock with no event stream (the non-turn callers, like
    /// recording a model change, have no sink to announce queuing on).
    pub async fn acquire_quiet(&self, conversation_id: i64) -> tokio::sync::OwnedMutexGuard<()> {
        self.lock_for(conversation_id).lock_owned().await
    }
}

// ── The turn driver ───────────────────────────────────────────────────────────

/// Everything one turn needs beyond the seams. `user_text` is `Some` for a new user
/// message (appended + persisted on the first `End`) and `None` to RESUME a persisted
/// thread after a crash (fresh `respond` from the persisted transcript — crash case c).
pub struct TurnParams<'a> {
    pub conversation_id: i64,
    pub user_text: Option<&'a str>,
    pub cmdr_md: Option<&'a str>,
    pub envelope: &'a ContextEnvelope,
    pub offset: FixedOffset,
    /// Wall-clock secs stamped on rows written this turn; also the envelope's clock.
    pub now_secs: i64,
    /// The resolved interactive-slot provider + model, for cost metering. Real slot
    /// resolution happens in the command layer; the runtime just records what it was told.
    pub provider: ProviderTag,
    pub model: String,
}

/// How a turn ended, for the caller's bookkeeping. The events already told the
/// frontend everything; this is for logging and the single-flight wrapper.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnResult {
    /// A final answer was produced and persisted.
    Answered { assistant_message_id: i64 },
    /// The turn stopped without an answer, for this typed reason.
    Failed(AgentErrorKind),
    /// The user cancelled at a tool boundary; nothing further was attempted.
    Cancelled,
}

/// Drive one turn to completion, persisting crash-safely and staying within budget.
/// Pure of Tauri: it needs only the seams (`llm`, `dispatcher`), a write `Connection`,
/// and the params — so it is fully unit-testable with a temp DB and fakes.
pub async fn run_turn(
    llm: &dyn AgentLlm,
    dispatcher: &dyn ToolDispatcher,
    conn: &rusqlite::Connection,
    tools: &[ToolDeclaration],
    params: &TurnParams<'_>,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
) -> TurnResult {
    // The working transcript mirrors the durable rows; assembly reads it, persistence
    // writes the DB. Load the persisted history, then (for a new turn) the pending user
    // message — held in memory until the first `End` so a failed first attempt records
    // nothing (crash case b).
    let mut transcript = match load_transcript(conn, params.conversation_id) {
        Ok(history) => history,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "load transcript failed: {e}");
            emit(
                sink,
                AgentChatEvent::Failed {
                    kind: AgentErrorKind::Provider,
                    detail: Some(e.to_string()),
                },
            );
            return TurnResult::Failed(AgentErrorKind::Provider);
        }
    };
    let mut user_needs_persist = false;
    if let Some(text) = params.user_text {
        transcript.push(AgentMessage {
            role: AgentRole::User,
            parts: vec![AgentPart::Text(text.to_string())],
            at: params.now_secs,
        });
        user_needs_persist = true;
    }

    let started = Instant::now();
    let mut tool_turns = 0usize;
    let mut model_recorded = false;

    loop {
        // Cancellation and both budgets are checked at the top, so no `respond` fires
        // once the user cancelled or a budget is spent — a runaway loop is impossible.
        if cancel.is_cancelled() {
            return TurnResult::Cancelled;
        }
        if started.elapsed() >= MAX_WALL_TIME || tool_turns >= MAX_TOOL_TURNS {
            emit(
                sink,
                AgentChatEvent::Failed {
                    kind: AgentErrorKind::BudgetExhausted,
                    detail: None,
                },
            );
            return TurnResult::Failed(AgentErrorKind::BudgetExhausted);
        }

        let prefix = PrefixInputs {
            system_prompt: SYSTEM_PROMPT,
            cmdr_md: params.cmdr_md,
            tools,
        };
        let assembled = context::assemble_prompt(&prefix, &transcript, params.envelope, params.offset);

        let stream = match llm
            .respond(&assembled.system, &assembled.tools, &assembled.messages, cancel.clone())
            .await
        {
            Ok(stream) => stream,
            Err(error) => {
                // The call never opened, so it never reached `End`: nothing is
                // persisted (crash case b). Surface the typed error plus the
                // provider's own wording for display.
                let detail = error.detail().map(str::to_string);
                let kind = AgentErrorKind::from(error);
                emit(sink, AgentChatEvent::Failed { kind, detail });
                return TurnResult::Failed(kind);
            }
        };

        emit(sink, AgentChatEvent::AssistantStarted);
        let StreamOutcome {
            final_message,
            stop,
            usage,
            stream_error,
        } = consume_stream(stream, sink).await;

        let message = match final_message {
            Some(message) => message,
            None => {
                // The stream ended without an `End`: partial assistant text is discarded
                // (crash case a). A user cancel is a clean stop, not a failure.
                if cancel.is_cancelled() {
                    return TurnResult::Cancelled;
                }
                let detail = stream_error.as_ref().and_then(|e| e.detail().map(str::to_string));
                let kind = stream_error
                    .map(AgentErrorKind::from)
                    .unwrap_or(AgentErrorKind::UnfinishedReply);
                emit(sink, AgentChatEvent::Failed { kind, detail });
                return TurnResult::Failed(kind);
            }
        };

        // A completed `respond`: record a model transition (first `End` only, BEFORE the
        // user row so the event line sits between the turns), persist the user row (first
        // `End` only), then the assistant row (content written only now), then meter this
        // call's cost.
        if !model_recorded {
            model_recorded = true;
            record_model_transition(conn, params, sink);
        }
        if user_needs_persist && let Some(text) = params.user_text {
            match store::append_message(
                conn,
                params.conversation_id,
                AgentRole::User,
                &[AgentPart::Text(text.to_string())],
                text,
                None,
                None,
                params.now_secs,
            ) {
                Ok((message_id, seq)) => {
                    user_needs_persist = false;
                    emit(sink, AgentChatEvent::UserPersisted { message_id, seq });
                }
                Err(e) => return persist_failed(sink, e),
            }
        }

        let assistant_search = search_text(&message.parts);
        let (assistant_id, assistant_seq) = match store::append_message(
            conn,
            params.conversation_id,
            AgentRole::Assistant,
            &message.parts,
            &assistant_search,
            Some(usage.prompt_tokens),
            Some(usage.completion_tokens),
            params.now_secs,
        ) {
            Ok(ids) => ids,
            Err(e) => return persist_failed(sink, e),
        };
        transcript.push(message.clone());
        meter_cost(conn, params, usage);

        // Terminal vs. another tool turn.
        let has_tool_calls = message.parts.iter().any(|p| matches!(p, AgentPart::ToolCall(_)));
        if !has_tool_calls {
            emit(
                sink,
                AgentChatEvent::Done {
                    message_id: assistant_id,
                    seq: assistant_seq,
                    stop,
                    usage,
                },
            );
            return TurnResult::Answered {
                assistant_message_id: assistant_id,
            };
        }

        // Dispatch each tool call, persisting its result on its own row, then loop.
        tool_turns += 1;
        for part in &message.parts {
            let AgentPart::ToolCall(call) = part else { continue };
            let result = dispatcher.dispatch(call).await;
            emit(
                sink,
                AgentChatEvent::ToolCallFinished {
                    call_id: call.call_id.clone(),
                    ok: dispatch_ok(&result),
                },
            );
            let tool_message = AgentMessage {
                role: AgentRole::Tool,
                parts: vec![AgentPart::ToolResult(result)],
                at: params.now_secs,
            };
            if let Err(e) = store::append_message(
                conn,
                params.conversation_id,
                AgentRole::Tool,
                &tool_message.parts,
                "",
                None,
                None,
                params.now_secs,
            ) {
                return persist_failed(sink, e);
            }
            transcript.push(tool_message);
        }
    }
}

struct StreamOutcome {
    final_message: Option<AgentMessage>,
    stop: AgentStopReason,
    usage: AgentUsage,
    stream_error: Option<AgentLlmError>,
}

/// Consume one `respond` stream, emitting display events and capturing the final
/// message plus its stop reason and usage (present only on a clean `End`). A stream
/// error or a drop leaves `final_message` `None` so the caller applies the
/// crash-case-a discard.
async fn consume_stream(mut stream: crate::agent::llm::AgentDeltaStream, sink: &ChatEventSink) -> StreamOutcome {
    let mut final_message = None;
    let mut stop = AgentStopReason::Completed;
    let mut usage = AgentUsage::default();
    let mut stream_error = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(AgentDelta::Text(text)) => emit(sink, AgentChatEvent::TextDelta { text }),
            Ok(AgentDelta::ReasoningTick) => emit(sink, AgentChatEvent::ReasoningTick),
            Ok(AgentDelta::ToolCallStarted { call_id, tool }) => {
                emit(sink, AgentChatEvent::ToolCallStarted { call_id, tool })
            }
            Ok(AgentDelta::End {
                message,
                stop: end_stop,
                usage: end_usage,
            }) => {
                stop = end_stop;
                usage = end_usage;
                final_message = Some(message);
            }
            Err(error) => {
                stream_error = Some(error);
                break;
            }
        }
    }
    StreamOutcome {
        final_message,
        stop,
        usage,
        stream_error,
    }
}

fn persist_failed(sink: &ChatEventSink, error: AgentStoreError) -> TurnResult {
    log::warn!(target: LOG_TARGET, "persisting a chat message failed: {error}");
    emit(
        sink,
        AgentChatEvent::Failed {
            kind: AgentErrorKind::Provider,
            detail: Some(error.to_string()),
        },
    );
    TurnResult::Failed(AgentErrorKind::Provider)
}

/// On the turn's first completed `respond`: if the effective model differs from the
/// conversation's last recorded one, persist a UI-facing model-change event row (BEFORE
/// this turn's user row, so the line sits between the turns) and tell the live rail;
/// then stamp `last_model`. Running at the first `End` on purpose: a failed first
/// attempt records nothing (crash case b), and the next successful turn re-runs this
/// comparison — the event is deferred, never lost. A conversation with no recorded model
/// yet (its first turn) only stamps; there is nothing to switch from.
fn record_model_transition(conn: &rusqlite::Connection, params: &TurnParams<'_>, sink: &ChatEventSink) {
    let last = match store::conversation_last_model(conn, params.conversation_id) {
        Ok(last) => last,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "reading the conversation's last model failed: {e}");
            return;
        }
    };
    if last.as_deref() == Some(params.model.as_str()) {
        return;
    }
    if last.is_some() {
        let event = store::ConversationEvent::ModelChanged {
            model: params.model.clone(),
        };
        match store::append_event(conn, params.conversation_id, &event, params.now_secs) {
            Ok((message_id, seq)) => emit(
                sink,
                AgentChatEvent::ModelChanged {
                    message_id,
                    seq,
                    model: params.model.clone(),
                },
            ),
            Err(e) => log::warn!(target: LOG_TARGET, "recording the model-change event failed: {e}"),
        }
    }
    if let Err(e) = store::set_conversation_last_model(conn, params.conversation_id, &params.model) {
        log::warn!(target: LOG_TARGET, "stamping the conversation's model failed: {e}");
    }
}

/// Load a conversation's persisted messages as the working transcript. Event rows are
/// UI-facing timeline entries (a model change), NOT transcript content — they never
/// reach a provider, so they're filtered out here.
fn load_transcript(conn: &rusqlite::Connection, conversation_id: i64) -> Result<Vec<AgentMessage>, AgentStoreError> {
    const ALL: u32 = 10_000;
    let stored = store::list_messages(conn, conversation_id, ALL, 0)?;
    Ok(stored
        .into_iter()
        .filter_map(|m| match m.content {
            store::StoredContent::Message { role, parts } => Some(AgentMessage {
                role,
                parts,
                at: m.created_at,
            }),
            store::StoredContent::Event(_) => None,
        })
        .collect())
}

/// The FTS text for a message: its prose (user + assistant text parts) only, never a
/// tool blob or reasoning state. Extracted at the call site per the store contract.
fn search_text(parts: &[AgentPart]) -> String {
    parts
        .iter()
        .filter_map(|part| match part {
            AgentPart::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fold one completed `respond` call's usage into the cost meter (crash case d: metered
/// per completed `End`, so completed turns count once and are never lost). Cost is priced
/// via the per-model table ([`crate::agent::pricing`]): a local/on-device model is free and
/// priced, a known cloud model gets an honest estimate, and an unknown cloud model records
/// its tokens with cost 0 but `priced = false` — shown "unknown", never a silent $0 (spec
/// §2.4 honesty).
fn meter_cost(conn: &rusqlite::Connection, params: &TurnParams<'_>, usage: AgentUsage) {
    let prompt_tokens = usage.prompt_tokens as u64;
    let completion_tokens = usage.completion_tokens as u64;
    let priced = crate::agent::pricing::price_call(params.provider, &params.model, prompt_tokens, completion_tokens);
    let record = CostRecord {
        day: day_for(params.now_secs, params.offset),
        conversation_id: params.conversation_id,
        provider: params.provider,
        model: params.model.clone(),
        prompt_tokens,
        completion_tokens,
        cost_micros: priced.cost_micros,
        priced: priced.priced,
    };
    if let Err(e) = store::record_cost(conn, &record) {
        log::warn!(target: LOG_TARGET, "metering chat cost failed: {e}");
    }
}

/// The local-day `YYYY-MM-DD` for the cost meter, from the send clock and the captured
/// offset (so the day matches the envelope's local timestamp).
fn day_for(now_secs: i64, offset: FixedOffset) -> String {
    let utc = Utc
        .timestamp_opt(now_secs, 0)
        .single()
        .unwrap_or(chrono::DateTime::<Utc>::UNIX_EPOCH);
    utc.with_timezone(&offset).format("%Y-%m-%d").to_string()
}

// ── The single-flight wrapper (the send command's entry point) ────────────────

/// The managed chat runtime: the `main.db` path plus the per-thread single-flight
/// locks. Registered in state by `agent::start`; the `ask_cmdr_send_message` command
/// grabs it and calls [`ChatRuntime::send_message`].
pub struct ChatRuntime {
    db_path: PathBuf,
    locks: ConversationLocks,
}

impl ChatRuntime {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            locks: ConversationLocks::new(),
        }
    }

    /// Send one user message to a thread and drive it to an answer, single-flight per
    /// thread. `conversation_id` is `None` to lazily create a thread (its id is
    /// returned). The provider/model name the resolved interactive slot for cost
    /// metering. Long work runs on the caller's tokio task; nothing here blocks the main
    /// thread.
    #[allow(
        clippy::too_many_arguments,
        reason = "the send surface; the IPC command is a thin pass-through"
    )]
    pub async fn send_message<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        llm: &dyn AgentLlm,
        provider: ProviderTag,
        model: String,
        conversation_id: Option<i64>,
        text: String,
        envelope: ContextEnvelope,
        offset: FixedOffset,
        sink: ChatEventSink,
        cancel: CancellationToken,
    ) -> Result<i64, AgentStoreError> {
        let conn = store::open_write_connection(&self.db_path)?;
        let now = now_secs();
        let conversation_id = match conversation_id {
            Some(id) => id,
            None => store::create_conversation(&conn, &derive_title(&text), now, None)?,
        };

        // Single-flight: a concurrent send for this thread queues behind this guard.
        let _guard = self.locks.acquire(conversation_id, &sink).await;

        let cmdr_md = read_cmdr_md();
        let tools = crate::agent::tools::agent_tool_declarations();
        let dispatcher = AppHandleDispatcher::new(app.clone());
        let params = TurnParams {
            conversation_id,
            user_text: Some(&text),
            cmdr_md: cmdr_md.as_deref(),
            envelope: &envelope,
            offset,
            now_secs: now,
            provider,
            model,
        };
        run_turn(llm, &dispatcher, &conn, &tools, &params, &sink, &cancel).await;
        Ok(conversation_id)
    }

    /// Record that a settings change switched an open thread's effective model, honoring
    /// the single-flight lock so the event lands only AFTER any in-flight turn finishes
    /// (that turn keeps its already-resolved model; the event marks the boundary). Returns
    /// the persisted event row's `(message_id, seq, created_at)`, or `None` when there is
    /// nothing to record: the conversation has no completed turn yet, or the effective
    /// model is unchanged (for example the interactive override masks the changed shared
    /// model).
    pub async fn record_model_change(
        &self,
        conversation_id: i64,
        model: &str,
    ) -> Result<Option<(i64, i64, i64)>, AgentStoreError> {
        let _guard = self.locks.acquire_quiet(conversation_id).await;
        let conn = store::open_write_connection(&self.db_path)?;
        match store::conversation_last_model(&conn, conversation_id)? {
            None => Ok(None),
            Some(last) if last == model => Ok(None),
            Some(_) => {
                let now = now_secs();
                let event = store::ConversationEvent::ModelChanged {
                    model: model.to_string(),
                };
                let (message_id, seq) = store::append_event(&conn, conversation_id, &event, now)?;
                store::set_conversation_last_model(&conn, conversation_id, model)?;
                Ok(Some((message_id, seq, now)))
            }
        }
    }
}

/// Register the [`ChatRuntime`] in managed state (called from `agent::start`, after the
/// store handle is up). The IPC command reads it back with `app.state::<ChatRuntime>()`.
pub fn register<R: Runtime>(app: &AppHandle<R>, db_path: PathBuf) {
    app.manage(ChatRuntime::new(db_path));
}

/// A thread title from the first line of the user's message, trimmed to a sane length.
/// A user-facing default; renaming stays the user's call. `pub(crate)` so the
/// IPC command, which pre-creates the conversation to learn its id up front (for the
/// cancel registry and the `Started` event), derives the same title the runtime would.
pub(crate) fn derive_title(text: &str) -> String {
    const MAX: usize = 60;
    let first_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if first_line.is_empty() {
        return "New chat".to_string();
    }
    let truncated: String = first_line.chars().take(MAX).collect();
    if first_line.chars().count() > MAX {
        format!("{}…", truncated.trim_end())
    } else {
        truncated
    }
}

/// Read `~/.cmdr/CMDR.md` if it exists, for the stable prefix. Absent or unreadable →
/// `None` (the prefix is just the system prompt). Read-only in v1 (spec §3).
fn read_cmdr_md() -> Option<String> {
    let path = dirs::home_dir()?.join(".cmdr").join("CMDR.md");
    std::fs::read_to_string(path).ok().filter(|s| !s.trim().is_empty())
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;

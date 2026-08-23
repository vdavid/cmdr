//! The chat runtime: drives one user message to an answer, safely and within budgets.
//!
//! It owns the parts of the flow that must be correct under failure and concurrency:
//! single-flight per thread, the per-message budgets that make a runaway loop
//! impossible by construction, cancellation at tool boundaries plus stream-cancel, the
//! typed error surface, and the crash-safe persistence model. The pure prompt building
//! lives in [`context`]; this module is the I/O-and-time half.
//!
//! Four parts, one per file, plus [`ChatRuntime`] (the send command's entry point) here:
//! - [`events`]: the event seam ([`AgentChatEvent`], [`ChatEventSink`]) and the typed
//!   [`AgentErrorKind`].
//! - [`dispatch`]: the [`ToolDispatcher`] seam and its production impl.
//! - [`turn`]: [`run_turn`], the loop everything above serves.
//! - [`cost`]: metering one completed `respond` call.
//!
//! ## The event seam (what the IPC layer subscribes to)
//!
//! Progress is emitted as typed [`AgentChatEvent`]s through a plain
//! `UnboundedSender` ([`ChatEventSink`]). Each caller — the rail's command and a wake —
//! makes the channel, forwards what comes out onto the conversation's stream
//! (`agent::chat::stream`), and passes the sender here. Nothing in this module knows
//! about Tauri IPC.
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

use chrono::FixedOffset;
use tauri::{AppHandle, Manager, Runtime};
use tokio_util::sync::CancellationToken;

use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::ProviderTag;
use crate::agent::store::{self, AgentStoreError};
use crate::ignore_poison::IgnorePoison;

use super::context::{self, ContextEnvelope};
use cmdr_md::read_cmdr_md;

mod cmdr_md;
mod cost;
mod dispatch;
mod events;
mod turn;

pub use dispatch::{AppHandleDispatcher, ToolDispatchOutcome, ToolDispatcher};
pub use events::{AgentChatEvent, AgentErrorKind, ChatEventSink};
pub use turn::{TurnParams, TurnResult, run_turn};

const LOG_TARGET: &str = "agent::chat";

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
        events::emit(sink, AgentChatEvent::Queued);
        lock.lock_owned().await
    }

    /// Acquire the conversation's lock with no event stream (the non-turn callers, like
    /// recording a model change, have no sink to announce queuing on).
    pub async fn acquire_quiet(&self, conversation_id: i64) -> tokio::sync::OwnedMutexGuard<()> {
        self.lock_for(conversation_id).lock_owned().await
    }
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
        prompt_budget: usize,
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
        let dispatcher = AppHandleDispatcher::new(app.clone(), conversation_id);
        let params = TurnParams {
            conversation_id,
            user_text: Some(&text),
            cmdr_md: cmdr_md.as_deref(),
            envelope: &envelope,
            offset,
            now_secs: now,
            provider,
            model,
            prompt_budget,
        };
        run_turn(llm, &dispatcher, &conn, &tools, &params, &sink, &cancel).await;
        Ok(conversation_id)
    }

    /// Run a WAKE's turn: the agent noticed something and is speaking in a thread it opened
    /// for itself.
    ///
    /// ⚠️ **A wake must not bypass `ChatRuntime`.** The rail never calls `run_turn` directly,
    /// and neither may the agent: the write connection and the per-thread single-flight guard
    /// are what stop a user's reply and the wake's own turn running concurrently in one
    /// thread. A wake thread is a real conversation the user can reply to.
    ///
    /// ⚠️ **This is a SECOND write connection to `main.db`**, beside the one the thread that
    /// prepared the wake holds. WAL makes that fine, and the preparing thread's writes are
    /// single-row and never held across an await, so the worst case is a brief wait on the
    /// busy timeout.
    ///
    /// The conversation already exists (the prepare step created it), so nothing is created
    /// here; `params.user_text` is the digest.
    pub async fn wake(
        &self,
        llm: &dyn AgentLlm,
        dispatcher: &dyn ToolDispatcher,
        tools: &[crate::agent::llm::types::ToolDeclaration],
        params: &TurnParams<'_>,
        sink: &ChatEventSink,
        cancel: &CancellationToken,
    ) -> Result<TurnResult, AgentStoreError> {
        let conn = store::open_write_connection(&self.db_path)?;
        let _guard = self.locks.acquire(params.conversation_id, sink).await;
        Ok(run_turn(llm, dispatcher, &conn, tools, params, sink, cancel).await)
    }

    /// Take away the thread a wake opened after it said, through `nothing_to_suggest`, that it
    /// had nothing worth raising — keeping what that turn spent on the reserved quiet-wakes row.
    ///
    /// ⚠️ **Under the single-flight guard**, like every other write to a live thread. A wake
    /// thread is a real conversation, so without the guard a reply the user is typing into it
    /// could be persisting into a thread that is being deleted underneath them.
    ///
    /// The wake path decides WHETHER (`agent/wake/quiet.rs` watches the tool call); this is only
    /// the write, which lives here because the connection and the lock do.
    pub async fn discard_quiet_wake(&self, conversation_id: i64) -> Result<(), AgentStoreError> {
        let _guard = self.locks.acquire_quiet(conversation_id).await;
        let conn = store::open_write_connection(&self.db_path)?;
        store::discard_conversation_keeping_cost(&conn, conversation_id)
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

pub(crate) fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod wake_tests;

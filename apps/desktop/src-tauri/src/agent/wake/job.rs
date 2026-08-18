//! The wake job: what happens when something waiting comes due.
//!
//! This reuses the chat runtime's turn loop rather than growing a second one. Budget
//! enforcement, elision, crash-safe persistence, and cost metering must not differ between
//! the user asking and the agent noticing, and two loops guarantee they eventually will.

use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

use super::{Digest, Inbox, WakeReadiness, compact};
use crate::agent::chat::context::ContextEnvelope;
use crate::agent::chat::runtime::{ChatEventSink, ToolDispatcher, TurnParams, TurnResult, run_turn};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::ToolDeclaration;
use crate::agent::store::create_conversation;
use crate::agent::types::ConversationOrigin;

/// What a wake needs that it cannot work out for itself.
pub struct WakeParams<'a> {
    pub readiness: WakeReadiness,
    /// Unix seconds; the same clock the deadlines were set against.
    pub now_secs: i64,
    /// What the digest may spend.
    pub digest_budget_tokens: usize,
    pub envelope: &'a ContextEnvelope,
    /// The declarations the turn may reach for: the agent view, so a wake can propose.
    pub tools: &'a [ToolDeclaration],
    pub offset: chrono::FixedOffset,
    pub provider: crate::agent::llm::types::ProviderTag,
    pub model: String,
    pub prompt_budget: usize,
}

/// How a wake ended.
#[derive(Debug)]
pub enum WakeOutcome {
    /// A turn ran, in the conversation it created. A caller links a sweep to that thread.
    Ran { conversation_id: i64, result: TurnResult },
    /// A gate is closed; the indicator says which.
    NotReady(WakeReadiness),
    /// Nothing is due, which is the common case and not worth a turn.
    NothingDue,
    /// The store would not take a new thread, so there was nowhere to run. The inbox is
    /// untouched and the next wake tries again.
    Unavailable,
}

/// Drive one wake to completion.
///
/// The ORDER is chosen so that every step which can decline to proceed does so before
/// anything is spent or lost: the gates first, then the deadline, then the digest shaped from
/// the rows WITHOUT draining them, then the thread. The inbox is drained only once a turn is
/// certain to run, so a budget too small to say anything, or a store that will not take a new
/// thread, leaves the backlog exactly as it was.
pub async fn run_wake(
    llm: &dyn AgentLlm,
    dispatcher: &dyn ToolDispatcher,
    conn: &Connection,
    inbox: &mut Inbox,
    params: WakeParams<'_>,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
) -> WakeOutcome {
    if !params.readiness.may_wake() {
        return WakeOutcome::NotReady(params.readiness);
    }
    let now = params.now_secs.max(0) as u64;
    if !inbox.due_at(now) {
        return WakeOutcome::NothingDue;
    }

    let digest = compact(&inbox.scored(), params.digest_budget_tokens);
    let rendered = digest.render();
    if rendered.is_empty() {
        // Nothing fits, so there is nothing to say. Better to wait than to open a thread
        // that reports silence.
        return WakeOutcome::NothingDue;
    }

    let conversation_id = match create_conversation(
        conn,
        &thread_title(&digest),
        params.now_secs,
        Some(ConversationOrigin::Notification),
    ) {
        Ok(id) => id,
        Err(e) => {
            log::warn!(target: "agent::wake", "wake could not open a thread: {e}");
            return WakeOutcome::Unavailable;
        }
    };

    // Committed: from here the rows have been reported on, so they leave the inbox.
    let _ = inbox.drain();

    let turn = TurnParams {
        conversation_id,
        user_text: Some(&rendered),
        cmdr_md: None,
        envelope: params.envelope,
        offset: params.offset,
        now_secs: params.now_secs,
        provider: params.provider,
        model: params.model,
        prompt_budget: params.prompt_budget,
    };
    let result = run_turn(llm, dispatcher, conn, params.tools, &turn, sink, cancel).await;
    WakeOutcome::Ran {
        conversation_id,
        result,
    }
}

/// The title a wake-created thread carries in the rail: the PLACE the activity happened,
/// never an authored sentence.
///
/// A backend-generated English title would be untranslated copy shipped into the database,
/// and this thread sits in a list beside ones the user named themselves. A folder name is
/// data, not voice.
pub fn thread_title(digest: &Digest) -> String {
    digest
        .lines
        .first()
        .map(|line| line.folder.clone())
        .or_else(|| digest.rollups.first().map(|rollup| rollup.ancestor.clone()))
        .map(|path| basename(&path).to_string())
        .unwrap_or_default()
}

fn basename(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(cut) => &trimmed[cut + 1..],
        None => trimmed,
    }
}

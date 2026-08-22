//! The wake job: what happens when something waiting comes due.
//!
//! This reuses the chat runtime's turn loop rather than growing a second one. Budget
//! enforcement, elision, crash-safe persistence, and cost metering must not differ between
//! the user asking and the agent noticing, and two loops guarantee they eventually will.
//!
//! ## Two halves, because they run on two threads
//!
//! [`prepare_wake`] is everything that can DECLINE, plus the commit; [`run_prepared_wake`] is
//! the turn. The writer thread (which owns the inbox and its write connection) prepares and
//! hands off; a dedicated wake thread runs. If one thread did both, the bounded rollup channel
//! would go unserviced for the length of a model call and drop what the tap sent meanwhile.
//!
//! ⚠️ **Nothing is drained until the turn is certain.** Handing the rows over up front would
//! lose them on [`PrepareOutcome::NothingDue`] and [`PrepareOutcome::Unavailable`], which are
//! ordinary paths rather than crashes. So prepare gates, scores, renders, and opens the thread
//! FIRST, and only then drains and clears the table.

use rusqlite::Connection;
use tokio_util::sync::CancellationToken;

use super::{Digest, Inbox, ScoredBundle, WakeReadiness, WakeTier, compact, persist, tier_of};
use crate::agent::chat::context::ContextEnvelope;
use crate::agent::chat::runtime::{ChatEventSink, ToolDispatcher, TurnParams, TurnResult, run_turn};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::ToolDeclaration;
use crate::agent::store::create_conversation;
use crate::agent::types::ConversationOrigin;

const LOG_TARGET: &str = "agent::wake";

/// What the prepare step needs. Everything here is cheap to gather on the writer thread.
pub struct PrepareParams {
    pub readiness: WakeReadiness,
    /// Unix seconds; the same clock the deadlines were set against.
    pub now_secs: i64,
    /// What the digest may spend.
    pub digest_budget_tokens: usize,
}

/// A wake that is going to happen: the thread is open, the rows are out of the inbox, and the
/// table is clear. Everything left is the turn.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedWake {
    pub conversation_id: i64,
    /// The rendered digest, which becomes the thread's user-role message.
    pub digest: String,
    /// What was drained, so the run step can report how much this wake covered.
    pub rows: Vec<ScoredBundle>,
    /// The strongest tier among those rows: what triggered the wake, for the log line.
    pub tier: WakeTier,
}

/// Whether a wake is going to happen, and what it costs if not (nothing, in every case).
#[derive(Debug)]
pub enum PrepareOutcome {
    Ready(PreparedWake),
    /// A gate is closed; the indicator says which.
    NotReady(WakeReadiness),
    /// Nothing is due, or nothing could be said within the budget. Either way not worth a turn.
    NothingDue,
    /// The store would not take a new thread, so there was nowhere to run. The inbox is
    /// untouched and the next wake tries again.
    Unavailable,
}

/// What the run step needs beyond the prepared wake and its seams.
pub struct RunWakeParams<'a> {
    /// Unix seconds; stamped on the rows this turn writes.
    pub now_secs: i64,
    pub envelope: &'a ContextEnvelope,
    /// The declarations the turn may reach for: the agent view, so a wake can propose.
    pub tools: &'a [ToolDeclaration],
    pub offset: chrono::FixedOffset,
    pub provider: crate::agent::llm::types::ProviderTag,
    pub model: String,
    pub prompt_budget: usize,
}

/// What a wake needs that it cannot work out for itself, for the whole job in one call.
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
    Ran {
        conversation_id: i64,
        result: TurnResult,
        /// The strongest tier among the rows the wake reported on.
        tier: WakeTier,
        /// How many folder-windows the digest covered.
        folders: usize,
    },
    /// A gate is closed; the indicator says which.
    NotReady(WakeReadiness),
    /// Nothing is due, which is the common case and not worth a turn.
    NothingDue,
    /// The store would not take a new thread, so there was nowhere to run. The inbox is
    /// untouched and the next wake tries again.
    Unavailable,
}

/// Everything a wake can decline to do, and then the commit.
///
/// The ORDER is the safety property: the gates first, then the deadline, then the digest
/// shaped from the rows WITHOUT draining them, then the thread. A budget too small to say
/// anything, or a store that will not take a new thread, leaves the backlog exactly as it was.
///
/// Once the thread exists the wake is committed, so the rows leave the inbox and the table is
/// cleared in the same breath. A process that dies mid-turn loses that digest rather than
/// re-delivering it on restart: the user would otherwise hear about the same activity twice,
/// and the folder is still there to be looked at again.
pub fn prepare_wake(conn: &Connection, inbox: &mut Inbox, params: &PrepareParams) -> PrepareOutcome {
    if !params.readiness.may_wake() {
        return PrepareOutcome::NotReady(params.readiness);
    }
    let now = params.now_secs.max(0) as u64;
    if !inbox.due_at(now) {
        return PrepareOutcome::NothingDue;
    }

    let scored = inbox.scored();
    let digest = compact(&scored, params.digest_budget_tokens);
    let rendered = digest.render();
    if rendered.is_empty() {
        // Nothing fits, so there is nothing to say. Better to wait than to open a thread
        // that reports silence.
        return PrepareOutcome::NothingDue;
    }

    let conversation_id = match create_conversation(
        conn,
        &thread_title(&digest),
        params.now_secs,
        Some(ConversationOrigin::Notification),
    ) {
        Ok(id) => id,
        Err(e) => {
            log::warn!(target: LOG_TARGET, "wake could not open a thread: {e}");
            return PrepareOutcome::Unavailable;
        }
    };

    // Committed: from here the rows have been reported on, so they leave the inbox and the
    // table. A failed clear is logged rather than fatal — the rows are already gone from
    // memory, and re-reading them after a restart is the honest worst case.
    let rows = inbox.drain();
    if let Err(e) = persist::clear(conn) {
        log::warn!(target: LOG_TARGET, "the drained inbox rows stayed on disk: {e}");
    }
    let tier = strongest_tier(&rows);

    PrepareOutcome::Ready(PreparedWake {
        conversation_id,
        digest: rendered,
        rows,
        tier,
    })
}

/// Run the prepared wake's turn, in the thread [`prepare_wake`] opened.
///
/// This is the half that reaches a provider, and the only half that may block for minutes.
/// It runs on the wake thread, under the conversation's single-flight guard, so a user reply
/// arriving in the same thread queues behind it rather than racing it.
pub async fn run_prepared_wake(
    llm: &dyn AgentLlm,
    dispatcher: &dyn ToolDispatcher,
    conn: &Connection,
    prepared: &PreparedWake,
    params: &RunWakeParams<'_>,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
) -> TurnResult {
    let turn = TurnParams {
        conversation_id: prepared.conversation_id,
        user_text: Some(&prepared.digest),
        cmdr_md: None,
        envelope: params.envelope,
        offset: params.offset,
        now_secs: params.now_secs,
        provider: params.provider,
        model: params.model.clone(),
        prompt_budget: params.prompt_budget,
    };
    run_turn(llm, dispatcher, conn, params.tools, &turn, sink, cancel).await
}

/// Drive one wake to completion on a single thread: [`prepare_wake`] then
/// [`run_prepared_wake`].
///
/// The LLM and the dispatcher arrive as FACTORIES because both are scoped to the conversation
/// this call creates: `AppHandleDispatcher::new(app, id)` scopes evidence to the thread, and
/// `LlmLogContext::agent_chat(id)` keys the LLM log the same way. Evidence scope is what stops
/// a claim in one thread being backed by facts delivered to another; get it wrong and
/// `ImageFactsLedger` refuses every content-citing proposal.
pub async fn run_wake(
    llm_for: &dyn Fn(i64) -> Box<dyn AgentLlm>,
    dispatcher_for: &dyn Fn(i64) -> Box<dyn ToolDispatcher>,
    conn: &Connection,
    inbox: &mut Inbox,
    params: WakeParams<'_>,
    sink: &ChatEventSink,
    cancel: &CancellationToken,
) -> WakeOutcome {
    let prepared = match prepare_wake(
        conn,
        inbox,
        &PrepareParams {
            readiness: params.readiness,
            now_secs: params.now_secs,
            digest_budget_tokens: params.digest_budget_tokens,
        },
    ) {
        PrepareOutcome::Ready(prepared) => prepared,
        PrepareOutcome::NotReady(readiness) => return WakeOutcome::NotReady(readiness),
        PrepareOutcome::NothingDue => return WakeOutcome::NothingDue,
        PrepareOutcome::Unavailable => return WakeOutcome::Unavailable,
    };

    let llm = llm_for(prepared.conversation_id);
    let dispatcher = dispatcher_for(prepared.conversation_id);
    let result = run_prepared_wake(
        llm.as_ref(),
        dispatcher.as_ref(),
        conn,
        &prepared,
        &RunWakeParams {
            now_secs: params.now_secs,
            envelope: params.envelope,
            tools: params.tools,
            offset: params.offset,
            provider: params.provider,
            model: params.model,
            prompt_budget: params.prompt_budget,
        },
        sink,
        cancel,
    )
    .await;

    WakeOutcome::Ran {
        conversation_id: prepared.conversation_id,
        result,
        tier: prepared.tier,
        folders: prepared.rows.len(),
    }
}

/// The strongest claim among the drained rows: what the wake fired FOR. A cold-only drain
/// cannot happen (a cold row sets no deadline), but it answers `Cold` rather than panicking.
fn strongest_tier(rows: &[ScoredBundle]) -> WakeTier {
    rows.iter()
        .map(|row| tier_of(row.interest))
        .max()
        .unwrap_or(WakeTier::Cold)
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

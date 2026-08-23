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

use super::quiet::{QuietWatch, discard_quiet_thread};
use super::{Digest, Inbox, ScoredBundle, WakeReadiness, WakeTier, compact, persist, tier_of};
use crate::agent::chat::context::ContextEnvelope;
use crate::agent::chat::runtime::{ChatEventSink, ToolDispatcher, TurnParams, TurnResult, UserTurn, run_turn};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{ToolDeclaration, WakeDigest};
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
    /// Act on whatever is waiting even if nothing has come due yet.
    ///
    /// ⚠️ Set ONLY by the `playwright-e2e` force-wake command, so a test doesn't have to sit
    /// out a deadline. It skips the clock and nothing else: the gates, the empty-digest bail,
    /// and the drain-only-when-committed order all still apply.
    pub ignore_deadlines: bool,
}

/// A wake that is going to happen: the thread is open, the rows are out of the inbox, and the
/// table is clear. Everything left is the turn.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedWake {
    pub conversation_id: i64,
    /// What the wake found, structured. It becomes the thread's user-role message as DATA:
    /// the English rendering is produced on the way to the provider and never persisted, so
    /// the rail can say the same thing in the user's own language.
    pub digest: WakeDigest,
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
    /// A turn ran and the agent said, through `nothing_to_suggest`, that none of it was worth
    /// raising. The thread it thought in is GONE, so there is no id to hand back; only what it
    /// spent survives, on the reserved quiet-wakes row.
    Quiet {
        /// The strongest tier among the rows it looked at.
        tier: WakeTier,
        /// How many folder-windows it looked at.
        folders: usize,
        /// The short reason the model gave, for the agent's own memory. ⚠️ Never log it: see
        /// `wake/quiet.rs`.
        reason: Option<String>,
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
    if !params.ignore_deadlines && !inbox.due_at(now) {
        return PrepareOutcome::NothingDue;
    }

    let scored = inbox.scored();
    let digest = compact(&scored, params.digest_budget_tokens);
    let wire = digest.to_wire();
    if wire.render().is_empty() {
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
        digest: wire,
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
    let turn = wake_turn_params(prepared, params);
    run_turn(llm, dispatcher, conn, params.tools, &turn, sink, cancel).await
}

/// What a wake's turn looks like: the digest as the user-role message, in the thread prepare
/// opened.
///
/// Written once and shared, because the scheduler does NOT go through
/// [`run_prepared_wake`] — it hands this to `ChatRuntime::wake`, which owns the write
/// connection and the single-flight guard. Two places composing a wake's turn independently
/// is how the two would drift.
pub fn wake_turn_params<'a>(prepared: &'a PreparedWake, params: &'a RunWakeParams<'a>) -> TurnParams<'a> {
    TurnParams {
        conversation_id: prepared.conversation_id,
        user: Some(UserTurn::Wake(&prepared.digest)),
        cmdr_md: None,
        envelope: params.envelope,
        offset: params.offset,
        now_secs: params.now_secs,
        provider: params.provider,
        model: params.model.clone(),
        prompt_budget: params.prompt_budget,
    }
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
            ignore_deadlines: false,
        },
    ) {
        PrepareOutcome::Ready(prepared) => prepared,
        PrepareOutcome::NotReady(readiness) => return WakeOutcome::NotReady(readiness),
        PrepareOutcome::NothingDue => return WakeOutcome::NothingDue,
        PrepareOutcome::Unavailable => return WakeOutcome::Unavailable,
    };

    let llm = llm_for(prepared.conversation_id);
    let dispatcher = dispatcher_for(prepared.conversation_id);
    // The watch is what turns the model's typed `nothing_to_suggest` call into an outcome. It
    // wraps the wake's dispatcher and nothing else's, which is what leaves the tool inert in
    // the rail.
    let watch = QuietWatch::new(dispatcher.as_ref());
    let result = run_prepared_wake(
        llm.as_ref(),
        &watch,
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

    // The delete happens HERE, after the turn, rather than in the tool: the tool is a pure
    // `Access::Read` signal, and one that mutated would be `Write` under the registry's
    // tiebreaker and would reach the rail's threads too.
    if watch.stayed_quiet() {
        discard_quiet_thread(conn, prepared.conversation_id);
        return WakeOutcome::Quiet {
            tier: prepared.tier,
            folders: prepared.rows.len(),
            reason: watch.reason(),
        };
    }

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

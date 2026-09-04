//! The wake thread: the half of a wake that reaches a provider.
//!
//! ⚠️ **A dedicated `std::thread` with a current-thread runtime, ❌ not a tokio task.**
//! `run_turn` holds a rusqlite `Connection` across awaits, which is not `Send`, so its future
//! cannot live on a multi-thread runtime. `ask_cmdr_send_message` solves it the same way and
//! this copies that shape deliberately.
//!
//! The writer thread prepares (gates, digest, thread, drain) and hands off here, then goes
//! straight back to servicing its channel. Nothing on this thread touches the inbox.

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::unbounded_channel;

use super::channel::{WakeCompletion, WakeControl, send_control};
use super::followup::PreparedFollowUp;
use super::indicator::{note_wake_finished, note_wake_started};
use super::staged::announce_staged;
use super::watch::WakeToolWatch;
use super::{PreparedWake, RunWakeParams, WakeTier, turn_params};
use crate::agent::chat::budget;
use crate::agent::chat::cancel;
use crate::agent::chat::runtime::{
    AgentChatEvent, AgentErrorKind, AppHandleDispatcher, ChatRuntime, TurnResult, UserTurn,
};
use crate::agent::chat::session::{capture_envelope, local_offset};
use crate::agent::chat::stream::{AskCmdrStreamEvent, emit_turn_event, forward_to_windows};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::ProviderTag;

const LOG_TARGET: &str = "agent::wake";

/// The resolved slot a prepared wake runs against, gathered on the writer thread before the
/// thread was opened so a wake with nowhere to think declines before spending anything.
pub(super) struct ResolvedSlot {
    pub llm: Box<dyn AgentLlm>,
    pub provider: ProviderTag,
    pub model: String,
    pub prompt_budget: usize,
}

/// The two kinds of turn this thread runs, and everything that differs between them.
///
/// One path rather than two because the machinery around the turn is identical: the same
/// envelope, the same memory, the same transport, the same cancel registration, the same
/// corner. Only the opener, the thread's provenance, and what a quiet answer means differ.
pub(super) enum BackgroundTurn {
    /// The agent noticed something and opened a thread of its own to say it in.
    Wake(PreparedWake),
    /// The user turned a sweep down, and the agent is asking why — in THEIR thread.
    FollowUp(PreparedFollowUp),
}

impl BackgroundTurn {
    fn conversation_id(&self) -> i64 {
        match self {
            BackgroundTurn::Wake(prepared) => prepared.conversation_id,
            BackgroundTurn::FollowUp(prepared) => prepared.conversation_id,
        }
    }

    /// What the turn opens with, persisted as DATA either way: a rendered English sentence
    /// would freeze one locale's copy in `main.db` where no locale pass could reach it.
    fn opener(&self) -> UserTurn<'_> {
        match self {
            BackgroundTurn::Wake(prepared) => UserTurn::Wake(&prepared.digest),
            BackgroundTurn::FollowUp(prepared) => UserTurn::Outcomes(&prepared.outcomes),
        }
    }

    /// The tier and folder count the outcome line reports. A follow-up has neither: it is not
    /// answering the inbox.
    fn scale(&self) -> (Option<WakeTier>, usize) {
        match self {
            BackgroundTurn::Wake(prepared) => (Some(prepared.tier), prepared.rows.len()),
            BackgroundTurn::FollowUp(_) => (None, 0),
        }
    }

    /// The outcome token's prefix, so the two kinds stay separable in the log and in analytics.
    fn token(&self, outcome: &'static str) -> &'static str {
        match self {
            BackgroundTurn::Wake(_) => outcome,
            BackgroundTurn::FollowUp(_) => followup_token(outcome),
        }
    }

    /// What this turn tells the scheduler when nothing about the provider was learned.
    fn completion(&self) -> WakeCompletion {
        match self {
            BackgroundTurn::Wake(_) => WakeCompletion::Wake,
            BackgroundTurn::FollowUp(_) => WakeCompletion::FollowUp,
        }
    }
}

/// ⚠️ Paired by hand because the outcome tokens are `&'static str` all the way into the
/// analytics event, and a formatted string would leak a `String` per turn into a categorical
/// property. A new outcome that forgets its twin here reports as a wake, which is why the
/// fallback is loud rather than silent.
fn followup_token(outcome: &'static str) -> &'static str {
    match outcome {
        "ran" => "followup_ran",
        "cancelled" => "followup_cancelled",
        "failed" => "followup_failed",
        "quiet" => "followup_quiet",
        _ => "followup_unavailable",
    }
}

/// Run one prepared background turn on its own thread. Returns immediately; the thread
/// announces itself finished with a [`WakeControl::WakeFinished`] control message, which is
/// what lets the writer thread prepare the next one.
pub(super) fn spawn(app: AppHandle, slot: ResolvedSlot, turn: BackgroundTurn) {
    let spawned = std::thread::Builder::new()
        .name("agent-wake-turn".to_string())
        .spawn(move || {
            // The corner goes busy before anything slow, and clears on EVERY exit below: a
            // stale `Thinking` would leave a spinner up forever and offer a click into a
            // thread a quiet wake has since deleted.
            note_wake_started(turn.conversation_id());
            let completion = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime.block_on(run(app, slot, &turn)),
                Err(e) => {
                    crate::log_error!(target: LOG_TARGET, "building the background turn's runtime failed: {e}");
                    let (tier, folders) = turn.scale();
                    record_outcome(turn.token("unavailable"), tier, folders, 0);
                    turn.completion()
                }
            };
            note_wake_finished();
            // ⚠️ Whatever happened, the writer thread has to hear that this one is done, or it
            // will never prepare another.
            send_control(WakeControl::WakeFinished(completion));
        });
    if let Err(e) = spawned {
        crate::log_error!(target: LOG_TARGET, "the wake thread did not start: {e}");
        note_wake_finished();
        send_control(WakeControl::WakeFinished(WakeCompletion::Wake));
    }
}

async fn run(app: AppHandle, slot: ResolvedSlot, turn: &BackgroundTurn) -> WakeCompletion {
    let conversation_id = turn.conversation_id();
    let (tier, folders) = turn.scale();
    // A wake's thread was created moments ago, so say so before anything slow: this is what puts
    // it in the session list as it is created rather than on next load. A follow-up speaks in a
    // thread that has been there all along, so announcing one would claim a thread was created
    // that wasn't.
    if matches!(turn, BackgroundTurn::Wake(_)) {
        emit_turn_event(conversation_id, AskCmdrStreamEvent::Started);
    }

    // ⚠️ Captured exactly as the rail captures it. With no main window (a routine-launched app
    // on macOS) `PaneStateStore` is absent and the pane fields come back empty, which is the
    // honest answer rather than a reason to skip the capture.
    let envelope = capture_envelope(
        &app,
        Vec::new(),
        Vec::new(),
        budget::files_per_batch(slot.prompt_budget),
    )
    .await;
    let tools = crate::agent::tools::agent_tool_declarations();
    // A wake reads the same memory the rail does. It is what keeps the agent from proposing
    // again what the user already turned down, which is the whole difference between a
    // colleague and a nag.
    let memory = crate::agent::memory::read_for_turn(&app, slot.prompt_budget);
    let params = RunWakeParams {
        memory: memory.as_deref(),
        now_secs: crate::agent::chat::runtime::now_secs(),
        envelope: &envelope,
        tools: &tools,
        offset: local_offset(),
        provider: slot.provider,
        model: slot.model,
        prompt_budget: slot.prompt_budget,
    };
    let dispatcher = AppHandleDispatcher::new(app.clone(), conversation_id);

    // ⚠️ A real token, registered under this thread's id. A background turn is a multi-second
    // action spending the user's money, and `docs/design-principles.md` requires those be
    // cancelable; the status corner's stop button is `ask_cmdr_cancel` with this id, the same
    // call the rail's stop makes. The guard clears the entry on every exit below.
    let cancel_token = cancel::register_cancel(conversation_id);
    let _cancel_guard = cancel::CancelGuard::new(conversation_id);

    let Some(runtime) = app.try_state::<ChatRuntime>() else {
        log::warn!(target: LOG_TARGET, "the chat runtime is not registered; the turn has nowhere to run");
        record_outcome(turn.token("unavailable"), tier, folders, 0);
        return turn.completion();
    };

    // ⚠️ A background turn owns a plain `UnboundedSender<AgentChatEvent>` and drains it itself,
    // onto the SAME conversation-keyed stream a rail send uses. That is what makes its thread
    // readable while it is still being written.
    let (sink, mut events) = unbounded_channel::<AgentChatEvent>();
    let draining = forward_to_windows(conversation_id, &mut events);
    // The watch wraps this turn's dispatcher and nothing else's, and answers both questions the
    // turn's own result can't: whether it said there was nothing to raise (a pure read whose
    // handler changes nothing, so acting on it belongs here rather than in the tool), and
    // whether it staged anything.
    let watch = WakeToolWatch::new(&dispatcher);
    let driving = async {
        // Moved in so the sender drops when the turn ends; otherwise the drain above never
        // finishes and the join never returns.
        let sink = sink;
        runtime
            .wake(
                slot.llm.as_ref(),
                &watch,
                params.tools,
                &turn_params(conversation_id, turn.opener(), &params),
                &sink,
                &cancel_token,
            )
            .await
    };
    let ((), result) = tokio::join!(draining, driving);
    let proposals = watch.proposals();

    // ⚠️ The reason the model gave is deliberately NOT in the line below. `cmdr.log` ships in
    // auto-dispatched error reports and its redactor is path-shaped, so a sentence about which
    // of the user's folders were boring would travel intact. Log THAT a wake was quiet.
    //
    // ⚠️ **Only a WAKE's thread goes away.** A follow-up speaks in a thread the user owns, and a
    // model reaching for `nothing_to_suggest` there must not take it with them.
    if watch.stayed_quiet() {
        if let BackgroundTurn::Wake(_) = turn {
            if let Err(e) = runtime.discard_quiet_wake(conversation_id).await {
                log::warn!(target: LOG_TARGET, "a quiet wake's thread stayed behind: {e}");
            }
            // ⚠️ Anything subscribed to this conversation is now watching a thread that no
            // longer exists, and re-reading it can't tell them so. Say it.
            emit_turn_event(conversation_id, AskCmdrStreamEvent::Discarded);
        }
        record_outcome(turn.token("quiet"), tier, folders, proposals);
        return turn.completion();
    }

    // ⚠️ Announced whatever the turn ENDED as, and before the outcome line. A cancel or a
    // provider failure after the model already staged a group leaves that group sitting in the
    // store, waiting; staying quiet about it would hide work the user is expected to review.
    // `announce_staged` is a no-op at zero, which is the common case.
    announce_staged(conversation_id, proposals);

    match result {
        Ok(TurnResult::Answered { .. }) => record_outcome(turn.token("ran"), tier, folders, proposals),
        Ok(TurnResult::Cancelled) => record_outcome(turn.token("cancelled"), tier, folders, proposals),
        Ok(TurnResult::Failed(kind)) => {
            log::warn!(target: LOG_TARGET, "the background turn ended without an answer: {kind:?}");
            record_outcome(turn.token("failed"), tier, folders, proposals);
            return completion_for_failure(kind, turn.completion());
        }
        Err(e) => {
            log::warn!(target: LOG_TARGET, "the background turn could not open the store: {e}");
            record_outcome(turn.token("unavailable"), tier, folders, proposals);
        }
    }
    turn.completion()
}

/// What a failed background turn tells the scheduler.
///
/// ❌ Classified by TYPED variant, never by the message the provider sent: `error-string-match`
/// forbids the string match, and a provider's wording changes under us without notice.
fn completion_for_failure(kind: AgentErrorKind, ordinary: WakeCompletion) -> WakeCompletion {
    match kind {
        // A rejected key, no key at all, and a spent quota are all settled facts about the rest
        // of the day rather than something worth retrying at the ordinary pace.
        AgentErrorKind::NoKey | AgentErrorKind::AuthFailed | AgentErrorKind::RateLimited => {
            WakeCompletion::ProviderRefused
        }
        // ❌ Never fold a transient failure in here. An unreachable provider or a dropped stream
        // says nothing about the key, and six hours of silence for one flaky request would be
        // the agent punishing the user for their network. `NotConfigured` is a gate
        // `resolve_slot` refuses ahead of the turn, so it costs nothing to reach again.
        AgentErrorKind::NotConfigured
        | AgentErrorKind::Unavailable
        | AgentErrorKind::Timeout
        | AgentErrorKind::BudgetExhausted
        | AgentErrorKind::RepeatedToolCall
        | AgentErrorKind::UnfinishedReply
        | AgentErrorKind::Provider => ordinary,
    }
}

/// One counted line per wake outcome, plus the matching anonymous event.
///
/// Nothing else reports on the wake loop at all, so without this the two deferred tuning knobs
/// (the unknown-importance weight and the hot/warm thresholds) can only ever be ranked by a
/// support message, and "the agent is twitchy" arrives as a complaint rather than a number.
///
/// ❌ Every property is categorical: an outcome token, a tier token, and coarse count buckets.
/// Never a path, never a folder name, never anything the digest said.
pub(super) fn record_outcome(outcome: &'static str, tier: Option<WakeTier>, folders: usize, proposals: usize) {
    let tier = tier.map_or("none", WakeTier::as_token);
    log::info!(
        target: LOG_TARGET,
        "wake {outcome}: tier {tier}, {folders} folder(s), {proposals} proposal(s)"
    );
    crate::analytics::posthog::capture(
        "agent_wake",
        serde_json::json!({
            "outcome": outcome,
            "tier": tier,
            "folders": crate::analytics::item_count_bucket(folders),
            "proposals": crate::analytics::item_count_bucket(proposals),
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ **A dead or exhausted key is a settled fact, not a transient failure.** After the
    /// 2026-09-03 quota died at 09:06 local the app sent 261 further requests over roughly six
    /// hours, every one answered 403, because nothing carried the provider's answer back to the
    /// scheduler. These three variants are what "it would refuse the next one too" looks like.
    #[test]
    fn a_rejected_key_or_a_spent_quota_tells_the_loop_to_stop_trying() {
        for kind in [
            AgentErrorKind::AuthFailed,
            AgentErrorKind::NoKey,
            AgentErrorKind::RateLimited,
        ] {
            assert_eq!(
                completion_for_failure(kind, WakeCompletion::Wake),
                WakeCompletion::ProviderRefused,
                "{kind:?} would refuse the next turn identically"
            );
        }
    }

    /// A follow-up learns it just as a wake does: the key it could not use is the same key the
    /// next wake would reach for.
    #[test]
    fn a_follow_up_that_hits_a_dead_key_reports_it_too() {
        assert_eq!(
            completion_for_failure(AgentErrorKind::AuthFailed, WakeCompletion::FollowUp),
            WakeCompletion::ProviderRefused
        );
    }

    /// ⚠️ **A transient failure must NOT earn the long backoff.** A dropped connection or a slow
    /// provider says nothing about the key, and six hours of silence for one flaky request would
    /// be the agent punishing the user for the network.
    #[test]
    fn a_transient_failure_leaves_the_ordinary_spacing_in_place() {
        for kind in [
            AgentErrorKind::Unavailable,
            AgentErrorKind::Timeout,
            AgentErrorKind::UnfinishedReply,
            AgentErrorKind::BudgetExhausted,
            AgentErrorKind::Provider,
            AgentErrorKind::NotConfigured,
        ] {
            assert_eq!(
                completion_for_failure(kind, WakeCompletion::Wake),
                WakeCompletion::Wake,
                "{kind:?} is worth trying again at the ordinary pace"
            );
        }
    }
}

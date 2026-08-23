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
use tokio_util::sync::CancellationToken;

use super::channel::{WakeControl, send_control};
use super::{PreparedWake, RunWakeParams, WakeTier, wake_turn_params};
use crate::agent::chat::budget;
use crate::agent::chat::runtime::{AgentChatEvent, AppHandleDispatcher, ChatRuntime, TurnResult};
use crate::agent::chat::session::{capture_envelope, local_offset};
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

/// Run one prepared wake on its own thread. Returns immediately; the thread announces itself
/// finished with a [`WakeControl::WakeFinished`] control message, which is what lets the writer
/// thread prepare the next one.
pub(super) fn spawn(app: AppHandle, slot: ResolvedSlot, prepared: PreparedWake) {
    let spawned = std::thread::Builder::new()
        .name("agent-wake-turn".to_string())
        .spawn(move || {
            match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime.block_on(run(app, slot, prepared)),
                Err(e) => {
                    crate::log_error!(target: LOG_TARGET, "building the wake turn's runtime failed: {e}");
                    record_outcome("unavailable", Some(prepared.tier), prepared.rows.len(), 0);
                }
            }
            // ⚠️ Whatever happened, the writer thread has to hear that this one is done, or it
            // will never prepare another.
            send_control(WakeControl::WakeFinished);
        });
    if let Err(e) = spawned {
        crate::log_error!(target: LOG_TARGET, "the wake thread did not start: {e}");
        send_control(WakeControl::WakeFinished);
    }
}

async fn run(app: AppHandle, slot: ResolvedSlot, prepared: PreparedWake) {
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
    let params = RunWakeParams {
        now_secs: crate::agent::chat::runtime::now_secs(),
        envelope: &envelope,
        tools: &tools,
        offset: local_offset(),
        provider: slot.provider,
        model: slot.model,
        prompt_budget: slot.prompt_budget,
    };
    let dispatcher = AppHandleDispatcher::new(app.clone(), prepared.conversation_id);

    let Some(runtime) = app.try_state::<ChatRuntime>() else {
        log::warn!(target: LOG_TARGET, "the chat runtime is not registered; the wake has nowhere to run");
        record_outcome("unavailable", Some(prepared.tier), prepared.rows.len(), 0);
        return;
    };

    // ⚠️ A wake owns a plain `UnboundedSender<AgentChatEvent>` and drains it itself. In M1 the
    // drain DISCARDS: nobody is watching a rail during a wake, and M2 replaces this with the
    // `tauri_specta::Event` bridge that makes the turn visible live. It counts proposals on the
    // way past, which is the one number the log line needs.
    let (sink, mut events) = unbounded_channel::<AgentChatEvent>();
    let draining = async move {
        let mut proposals = 0usize;
        while let Some(event) = events.recv().await {
            if matches!(event, AgentChatEvent::ProposalReady { .. }) {
                proposals += 1;
            }
        }
        proposals
    };
    let driving = async {
        // Moved in so the sender drops when the turn ends; otherwise the drain above never
        // finishes and the join never returns.
        let sink = sink;
        runtime
            .wake(
                slot.llm.as_ref(),
                &dispatcher,
                params.tools,
                &wake_turn_params(&prepared, &params),
                &sink,
                &CancellationToken::new(),
            )
            .await
    };
    let (proposals, result) = tokio::join!(draining, driving);

    match result {
        Ok(TurnResult::Answered { .. }) => record_outcome("ran", Some(prepared.tier), prepared.rows.len(), proposals),
        Ok(TurnResult::Cancelled) => record_outcome("cancelled", Some(prepared.tier), prepared.rows.len(), proposals),
        Ok(TurnResult::Failed(kind)) => {
            log::warn!(target: LOG_TARGET, "the wake turn ended without an answer: {kind:?}");
            record_outcome("failed", Some(prepared.tier), prepared.rows.len(), proposals);
        }
        Err(e) => {
            log::warn!(target: LOG_TARGET, "the wake turn could not open the store: {e}");
            record_outcome("unavailable", Some(prepared.tier), prepared.rows.len(), proposals);
        }
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

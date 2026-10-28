//! Sending one message and streaming its answer: the send gate (Ask Cmdr's switch, then the
//! slot, which carries cloud consent), the snapshot-at-send envelope, and the cancel registry.
//!
//! ## Streaming
//!
//! Progress does NOT come back through this command. It rides `agent::chat::stream`, the one
//! conversation-keyed event a wake's turn uses too, so a reload re-subscribes instead of
//! losing the turn and the whole surface stays specta-typed. This command only bridges the
//! runtime's [`AgentChatEvent`] seam onto it (`forward_to_windows`).
//!
//! ## What does come back through the command
//!
//! The two answers a stream can't carry: the resolved conversation id, and a refusal decided
//! BEFORE a turn exists. Half of those happen before there IS a conversation to key an event
//! on, so they are the command's `Err`, not an event.
//!
//! ## LLM resolution
//!
//! The slot, the budget, and the envelope all resolve in `agent::chat::session`, which a wake
//! shares: `agent/` sits BELOW this layer, so a wake could not import them from here. This
//! command only calls down and maps the typed refusal onto its own.
//!
//! ## Cancellation
//!
//! Cancel is keyed by `conversation_id`. The command resolves/creates the conversation id up
//! front, emits `Started` on it, and registers the turn's cancel token under that id BEFORE the
//! turn waits for the thread's single-flight lock, so a send queued behind a running turn holds
//! its own registration beside that turn's; [`ask_cmdr_cancel`] trips every one of them. The registry itself is `agent::chat::cancel`, one level
//! down, because a WAKE registers in it too and may not import upward.

use std::path::Path;

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::unbounded_channel;
use tokio_util::sync::CancellationToken;

use super::{
    AskCmdrSendRefusal, AttachmentRef, CancelOutcomeView, LOG_TARGET, MessageBlock, MessageRoleView, MessageView,
    ModelWindowView, now_secs,
};
use crate::agent::AgentDb;
use crate::agent::chat::budget;
use crate::agent::chat::cancel::{self, TurnRun};
use crate::agent::chat::runtime::{AgentChatEvent, ChatRuntime, RecordedSlotEvent};
use crate::agent::chat::session::chat_memory_tokens as resolve_chat_memory_tokens;
use crate::agent::chat::session::{
    AgentSlot, admit_send, capture_envelope, local_offset, provider_and_model, resolve_agent_llm,
    resolve_call_settings, resolve_prompt_budget_for,
};
use crate::agent::chat::slot_status::{SlotStatus, slot_status};
use crate::agent::chat::stream::{AgentErrorKindView, AskCmdrStreamEvent, emit_turn_event, forward_to_windows};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::ProviderTag;
use crate::agent::store;

// ── What a thread's slot would be right now ────────────────────────────────────

/// The model an Ask Cmdr turn would use right now, for the slot-change events: the
/// interactive override when set, else the shared `ai/` model — the same resolution a
/// send performs. `None` when AI is off (nothing will run, so nothing to record).
fn effective_model_for_event(app: &AppHandle) -> Option<String> {
    if crate::test_mode::ask_cmdr_fake_active() {
        return Some("fake".to_string());
    }
    if crate::ai::state::get_provider() == "off" {
        return None;
    }
    let model_override = crate::settings::load_ask_cmdr_interactive_model(app);
    Some(provider_and_model(model_override.as_deref()).1)
}

/// The model Ask Cmdr would send to right now, plus the window we believe it has, so the
/// chat-memory setting can warn when a chosen size is larger than that window. `None` for the
/// window means nothing here knows it (an unrecognized model id), and the UI then shows no
/// warning rather than a guess dressed as one.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_model_window(app: AppHandle) -> ModelWindowView {
    let Some(model) = effective_model_for_event(&app) else {
        return ModelWindowView {
            model: String::new(),
            known_window_tokens: None,
            reply_tokens: 0,
        };
    };
    let (provider, _) = provider_and_model(crate::settings::load_ask_cmdr_interactive_model(&app).as_deref());
    let known_window_tokens = budget::known_window(
        provider,
        &model,
        crate::ai::state::get_local_context_size(),
        crate::agent::chat::session::reported_window_tokens(&app, provider, &model),
    )
    .map(|window| window.tokens as u32);
    let reply_tokens = resolve_call_settings(&app, provider, &model).max_output_tokens;
    ModelWindowView {
        model,
        known_window_tokens,
        reply_tokens,
    }
}

/// What the rail shows before anything is sent: whether a provider is set up (its warning and
/// the Send button's reason), and the floor every call of a new chat costs (the footer's
/// baseline). Off the IPC thread: the prefix reads `CMDR.md` and the memory folder from disk.
/// `None` only when that read panicked; the rail then keeps what it last knew.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_slot_status(app: AppHandle) -> Option<SlotStatus> {
    match tauri::async_runtime::spawn_blocking(move || slot_status(&app)).await {
        Ok(status) => Some(status),
        Err(e) => {
            log::warn!(target: LOG_TARGET, "reading the Ask Cmdr slot status failed: {e}");
            None
        }
    }
}

/// A settings change may have moved an open thread's slot — the model it sends to, the chat
/// memory each message carries, or both: record what actually moved as conversation events
/// once any in-flight turn finishes (that turn keeps what it already resolved; the rows mark
/// the boundary). Returns their display views in timeline order, and an empty list when
/// nothing changed for this thread — AI is off, no turn has run yet, or both facets are the
/// same (for example the interactive override masks the changed shared model).
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_record_slot_change(app: AppHandle, conversation_id: i64) -> Result<Vec<MessageView>, String> {
    let Some(model) = effective_model_for_event(&app) else {
        return Ok(Vec::new());
    };
    let Some(runtime) = app.try_state::<ChatRuntime>() else {
        return Ok(Vec::new());
    };
    // The same resolution a send performs. A refusal (a local window under the floor) means
    // there is no honest size to name, so that facet stays quiet rather than guessing.
    let (provider, _) = provider_and_model(crate::settings::load_ask_cmdr_interactive_model(&app).as_deref());
    let chat_memory_tokens = resolve_chat_memory_tokens(&app, provider, &model).ok();

    match runtime
        .record_slot_change(conversation_id, &model, chat_memory_tokens)
        .await
    {
        Ok(recorded) => Ok(recorded.into_iter().map(slot_event_view).collect()),
        Err(e) => {
            log::warn!(target: LOG_TARGET, "recording a slot change failed: {e}");
            Err(e.to_string())
        }
    }
}

/// Project one recorded slot event onto the rail's message view. The same blocks the history
/// fold produces, so the rail draws one line per change whichever path delivered it.
fn slot_event_view(recorded: RecordedSlotEvent) -> MessageView {
    let blocks = match recorded.event {
        store::ConversationEvent::ModelChanged { model } => vec![MessageBlock::ModelChanged { model }],
        store::ConversationEvent::ChatMemoryChanged { chat_memory_tokens } => {
            vec![MessageBlock::ChatMemoryChanged { chat_memory_tokens }]
        }
        // Only this command's own two events reach here; a decision is written by the
        // proposal path, which has its own view.
        store::ConversationEvent::ProposalDecided { decision } => {
            vec![MessageBlock::ProposalDecisions {
                decisions: vec![decision],
            }]
        }
        // Never recorded here (the turn writes its own stats; `Unknown` is never written); an
        // empty event view if either ever were.
        store::ConversationEvent::TurnFinished { .. } | store::ConversationEvent::Unknown => Vec::new(),
    };
    MessageView {
        id: recorded.message_id,
        seq: recorded.seq,
        role: MessageRoleView::Event,
        blocks,
        prompt_tokens: None,
        completion_tokens: None,
        created_at: recorded.created_at,
        turn_id: recorded.turn_id,
        // Recorded under the thread's quiet lock, so never while a turn runs.
        during_turn: false,
        turn_stats: None,
    }
}

// ── Commands ───────────────────────────────────────────────────────────────────

/// What a send the backend took answers: the thread it runs in, and the turn's run, which every
/// event of that turn carries (`stream::AskCmdrTurn::run`). The rail keys its Stop on the run, so a
/// late terminal of a turn it already ended can't end the one it sent since.
#[derive(Debug, Clone, Copy, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskCmdrSendAccepted {
    pub conversation_id: i64,
    pub run: TurnRun,
}

/// Send one user message to a thread and stream the answer. `conversation_id` is `None`
/// to start a fresh thread; the resolved id comes back here, and every event the turn
/// produces rides `agent::chat::stream` keyed on it.
///
/// An `Err` is a refusal decided before the turn existed (no store, Ask Cmdr off, cloud AI not
/// allowed, no slot, a local window too small, a store that wouldn't take a thread). Those can't be streamed:
/// half of them happen before there IS a conversation to key an event on.
///
/// The turn runs on a dedicated thread with its own current-thread runtime: the chat
/// runtime holds a rusqlite `Connection` (not `Send`) across awaits, so its future can't
/// live on the Tauri command future or a multi-thread tokio task. The command answers the
/// conversation id at once; streaming keeps flowing on that thread.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_send_message(
    app: AppHandle,
    conversation_id: Option<i64>,
    text: String,
    attachments: Vec<AttachmentRef>,
    // Destination names the user turned down in this thread's last rename review, newest
    // first. Names only: a reason would be model-authored, and the next batch would inherit
    // the rationalization instead of the fact.
    denied_names: Vec<String>,
) -> Result<AskCmdrSendAccepted, AskCmdrSendRefusal> {
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        return Err(AskCmdrSendRefusal::of(AgentErrorKindView::NotConfigured));
    };

    // The send gate, enforced structurally and BEFORE a thread or an LLM exists: Ask Cmdr's
    // switch (read fresh, absent reads as off), then the slot, whose resolution enforces cloud
    // consent. The rail's frontend gate is the UX layer; this is what makes it hold even if a
    // caller bypasses the UI.
    let ask_cmdr = crate::settings::load_ask_cmdr_switch(&app);
    let (llm_kind, provider, model) = match admit_send(ask_cmdr, || resolve_agent_llm(&app, AgentSlot::Rail)) {
        Ok(resolved) => resolved,
        Err(gate) => return Err(AskCmdrSendRefusal::gated(gate)),
    };

    // Resolve the budget before a thread exists, so a local server too small to hold one
    // prompt is refused honestly instead of assembled against and rejected by the server. The
    // effort comes first: the reply's ceiling it sets is room the prompt must leave free.
    let call = resolve_call_settings(&app, provider, &model);
    // What the slot-change timeline names; a refusal is the budget's, answered below.
    let chat_memory_tokens = resolve_chat_memory_tokens(&app, provider, &model).ok();
    let prompt_budget = match resolve_prompt_budget_for(&app, provider, &model, call) {
        Ok(tokens) => tokens,
        Err(budget::BudgetRefusal::LocalWindowBelowFloor {
            window_tokens,
            floor_tokens,
        }) => {
            log::warn!(
                target: LOG_TARGET,
                "refusing the send: the local server runs with a {window_tokens}-token window, \
                 under the {floor_tokens}-token floor one turn needs"
            );
            return Err(AskCmdrSendRefusal::of(AgentErrorKindView::LocalWindowTooSmall));
        }
    };

    // Resolve/create the conversation id up front so cancel + the frontend can key on it.
    let conversation_id = match conversation_id {
        Some(id) => id,
        None => match create_conversation_now(&db_path, &text) {
            Ok(id) => id,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "creating a conversation failed: {e}");
                return Err(AskCmdrSendRefusal::detailed(
                    AgentErrorKindView::Provider,
                    e.to_string(),
                ));
            }
        },
    };
    // Register the cancel token before spawning so a stop that arrives immediately hits it, and
    // before `Started` so every event of the turn carries its run. The registration moves onto the
    // turn's thread and unregisters when it drops there, on every exit.
    let cancel = cancel::register_cancel(conversation_id);
    let run = cancel.run();
    emit_turn_event(conversation_id, Some(run), AskCmdrStreamEvent::Started);

    // Now that the conversation id is known, build the LLM so the real backend logs under
    // this thread's `llm-logs/thread-{id}/` directory.
    let llm = llm_kind.into_llm(conversation_id);

    std::thread::spawn(move || {
        // Locals drop in reverse: a panic emits the turn's terminal BEFORE it unregisters, so a
        // Stop that reached it still gets the terminal it waits for.
        let registration = cancel;
        let _terminal_on_panic = TerminalOnPanic { conversation_id, run };
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                crate::log_error!(target: LOG_TARGET, "building the chat turn runtime failed: {e}");
                emit_turn_event(
                    conversation_id,
                    Some(run),
                    AskCmdrStreamEvent::Failed {
                        kind: AgentErrorKindView::Provider,
                        detail: Some(e.to_string()),
                        managed: None,
                        turn: None,
                    },
                );
                return;
            }
        };
        runtime.block_on(drive_turn(
            app,
            llm,
            provider,
            model,
            prompt_budget,
            chat_memory_tokens.unwrap_or(prompt_budget),
            call,
            conversation_id,
            run,
            text,
            attachments,
            denied_names,
            registration.token(),
        ));
    });

    Ok(AskCmdrSendAccepted { conversation_id, run })
}

/// Every turn ends with exactly one terminal event, a panicking one included: `run_turn` emits
/// it on every return, and this emits it when the turn thread unwinds instead. Without it the
/// rail would wait forever on a Stop it sent, and take the thread's NEXT turn for its terminal.
struct TerminalOnPanic {
    conversation_id: i64,
    run: TurnRun,
}

impl Drop for TerminalOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            emit_turn_event(
                self.conversation_id,
                Some(self.run),
                AskCmdrStreamEvent::Failed {
                    kind: AgentErrorKindView::UnfinishedReply,
                    detail: None,
                    managed: None,
                    turn: None,
                },
            );
        }
    }
}

/// Run one turn to completion on the current-thread runtime: capture the envelope, forward
/// the runtime's events onto the conversation's stream, and drive the chat runtime. The cancel
/// registration stays on the turn's thread, which drops it (unregistering) once this returns or
/// unwinds.
#[allow(
    clippy::too_many_arguments,
    reason = "the turn's full input set, moved onto a worker thread"
)]
async fn drive_turn(
    app: AppHandle,
    llm: Box<dyn AgentLlm>,
    provider: ProviderTag,
    model: String,
    // Resolved at send (see `resolve_prompt_budget`) and carried in as a value, so a settings
    // change mid-turn can't move the ceiling this turn assembles against.
    prompt_budget: usize,
    // The chat memory the slot-change timeline names (`session::chat_memory_tokens`).
    chat_memory_tokens: usize,
    // The same, for how hard the model thinks and the reply's ceiling (`session::resolve_call_settings`).
    call: crate::agent::llm::CallSettings,
    conversation_id: i64,
    run: TurnRun,
    text: String,
    attachments: Vec<AttachmentRef>,
    denied_names: Vec<String>,
    cancel: CancellationToken,
) {
    let envelope = capture_envelope(
        &app,
        attachments.iter().map(AttachmentRef::to_envelope).collect(),
        denied_names,
        budget::files_per_batch(prompt_budget),
    )
    .await;
    let offset = local_offset();

    let Some(runtime) = app.try_state::<ChatRuntime>() else {
        emit_turn_event(
            conversation_id,
            Some(run),
            AskCmdrStreamEvent::Failed {
                kind: AgentErrorKindView::Provider,
                detail: None,
                managed: None,
                turn: None,
            },
        );
        return;
    };

    // Forward the runtime's unbounded event channel onto the conversation's stream, until the
    // runtime drops its sender (the turn finished). ⚠️ There is deliberately no "the listener
    // went away" exit: a reload takes the webview, not the thread, and the whole point of a
    // conversation-keyed event is that the reloaded rail picks the same stream back up.
    let (tx, mut rx) = unbounded_channel::<AgentChatEvent>();
    let forward = forward_to_windows(conversation_id, run, &mut rx);
    let drive = runtime.send_message(
        &app,
        llm.as_ref(),
        provider,
        model,
        prompt_budget,
        chat_memory_tokens,
        call,
        Some(conversation_id),
        text,
        envelope,
        offset,
        tx,
        cancel,
    );
    let (_, result) = tokio::join!(forward, drive);
    if let Err(e) = result {
        log::warn!(target: LOG_TARGET, "chat turn failed: {e}");
        emit_turn_event(
            conversation_id,
            Some(run),
            AskCmdrStreamEvent::Failed {
                kind: AgentErrorKindView::Provider,
                detail: Some(e.to_string()),
                managed: None,
                turn: None,
            },
        );
    }
}

/// Create a new conversation off the shared write connection, deriving its title like the
/// runtime would. Scoped so the connection drops before the runtime opens its own.
fn create_conversation_now(db_path: &Path, text: &str) -> Result<i64, store::AgentStoreError> {
    let conn = store::open_write_connection(db_path)?;
    store::create_conversation(
        &conn,
        &crate::agent::chat::runtime::derive_title(text),
        now_secs(),
        None,
    )
}

/// Stop the in-flight turn for a thread. Idempotent: an unknown id (already finished) trips
/// nothing. A clean stop at the next tool boundary or stream chunk, not a hard abort.
///
/// Answers what the Stop reached, so the rail waits for exactly the terminals still coming: a
/// Stop that reached no live turn (it ended, or died) gets none, and a turn queued behind
/// another owes one more.
///
/// One command for both kinds of turn: a wake registers in the same registry, so the status
/// corner's stop button and the rail's are the same call with a different id.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_cancel(conversation_id: i64) -> CancelOutcomeView {
    cancel::cancel_turn(conversation_id).into()
}

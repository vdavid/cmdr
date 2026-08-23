//! Sending one message and streaming its answer: the slot resolution, the consent gate,
//! the snapshot-at-send envelope, and the cancel registry.
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
//! Cancel is keyed by `conversation_id` (single-flight means at most one active turn per
//! thread; the frontend disables the composer while a turn streams, so a thread never has
//! two concurrent sends). The command resolves/creates the conversation id up front, emits
//! `Started` on it, and registers the turn's [`CancellationToken`] under that id;
//! [`ask_cmdr_cancel`] trips it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::unbounded_channel;
use tokio_util::sync::CancellationToken;

use super::{
    AskCmdrSendRefusal, AttachmentRef, LOG_TARGET, MessageBlock, MessageRoleView, MessageView, ModelWindowView,
    now_secs,
};
use crate::agent::AgentDb;
use crate::agent::chat::budget;
use crate::agent::chat::runtime::{AgentChatEvent, ChatRuntime};
use crate::agent::chat::session::{
    AgentSlot, capture_envelope, local_offset, provider_and_model, resolve_agent_llm, resolve_prompt_budget,
};
use crate::agent::chat::stream::{AgentErrorKindView, AskCmdrStreamEvent, emit_turn_event, forward_to_windows};
use crate::agent::consent::has_current_consent;
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::ProviderTag;
use crate::agent::store;
use crate::ignore_poison::IgnorePoison;

// ── Cancellation registry (keyed by conversation id) ───────────────────────────

static CANCELS: LazyLock<Mutex<HashMap<i64, CancellationToken>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a fresh cancel token for a conversation and return a clone the turn owns.
fn register_cancel(conversation_id: i64) -> CancellationToken {
    let token = CancellationToken::new();
    CANCELS.lock_ignore_poison().insert(conversation_id, token.clone());
    token
}

fn unregister_cancel(conversation_id: i64) {
    CANCELS.lock_ignore_poison().remove(&conversation_id);
}

// ── The model a thread would use right now ─────────────────────────────────────

/// The model an Ask Cmdr turn would use right now, for the model-change event: the
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
        };
    };
    let (provider, _) = provider_and_model(crate::settings::load_ask_cmdr_interactive_model(&app).as_deref());
    let known_window_tokens = budget::known_window_tokens(provider, &model, crate::ai::state::get_local_context_size())
        .map(|tokens| tokens as u32);
    ModelWindowView {
        model,
        known_window_tokens,
    }
}

/// A settings change may have switched the model for an open thread: record it as a
/// conversation event once any in-flight turn finishes (the turn keeps its already-resolved
/// model; the event marks the boundary). Returns the persisted event's display view, or
/// `None` when nothing changed for this thread — AI is off, no turn has run yet, or the
/// effective model is the same (for example the interactive override masks the changed
/// shared model).
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_record_model_change(app: AppHandle, conversation_id: i64) -> Result<Option<MessageView>, String> {
    let Some(model) = effective_model_for_event(&app) else {
        return Ok(None);
    };
    let Some(runtime) = app.try_state::<ChatRuntime>() else {
        return Ok(None);
    };
    match runtime.record_model_change(conversation_id, &model).await {
        Ok(Some((id, seq, created_at))) => Ok(Some(MessageView {
            id,
            seq,
            role: MessageRoleView::Event,
            blocks: vec![MessageBlock::ModelChanged { model }],
            prompt_tokens: None,
            completion_tokens: None,
            created_at,
        })),
        Ok(None) => Ok(None),
        Err(e) => {
            log::warn!(target: LOG_TARGET, "recording a model change failed: {e}");
            Err(e.to_string())
        }
    }
}

// ── Commands ───────────────────────────────────────────────────────────────────

/// Send one user message to a thread and stream the answer. `conversation_id` is `None`
/// to start a fresh thread; the resolved id comes back here, and every event the turn
/// produces rides `agent::chat::stream` keyed on it.
///
/// An `Err` is a refusal decided before the turn existed (no store, no consent, no slot, a
/// local window too small, a store that wouldn't take a thread). Those can't be streamed:
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
) -> Result<i64, AskCmdrSendRefusal> {
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        return Err(AskCmdrSendRefusal::of(AgentErrorKindView::NotConfigured));
    };

    // The consent gate, enforced structurally: refuse BEFORE creating a thread or resolving
    // the LLM if the user hasn't accepted the current consent copy. The rail's frontend gate
    // is the UX layer; this is what makes "nothing reaches a provider without consent" true
    // even if a caller bypasses the UI. Fails closed (an unreadable store reads as refused).
    let consented = match store::open_read_connection(&db_path) {
        Ok(conn) => has_current_consent(&conn),
        Err(e) => {
            log::warn!(target: LOG_TARGET, "reading consent failed, refusing the send: {e}");
            false
        }
    };
    if !consented {
        return Err(AskCmdrSendRefusal::of(AgentErrorKindView::NoConsent));
    }

    // Resolve the LLM only after the consent gate: if AI is off/unconfigured, say so and add
    // no thread.
    let (llm_kind, provider, model) = match resolve_agent_llm(&app, AgentSlot::Rail) {
        Ok(resolved) => resolved,
        Err(kind) => return Err(AskCmdrSendRefusal::of(kind.into())),
    };

    // Resolve the budget before a thread exists, so a local server too small to hold one
    // prompt is refused honestly instead of assembled against and rejected by the server.
    let prompt_budget = match resolve_prompt_budget(&app, provider, &model) {
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
                return Err(AskCmdrSendRefusal {
                    kind: AgentErrorKindView::Provider,
                    detail: Some(e.to_string()),
                });
            }
        },
    };
    emit_turn_event(conversation_id, AskCmdrStreamEvent::Started);

    // Now that the conversation id is known, build the LLM so the real backend logs under
    // this thread's `llm-logs/thread-{id}/` directory.
    let llm = llm_kind.into_llm(conversation_id);

    // Register the cancel token before spawning so a stop that arrives immediately hits it.
    let cancel = register_cancel(conversation_id);

    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                crate::log_error!(target: LOG_TARGET, "building the chat turn runtime failed: {e}");
                emit_turn_event(
                    conversation_id,
                    AskCmdrStreamEvent::Failed {
                        kind: AgentErrorKindView::Provider,
                        detail: Some(e.to_string()),
                    },
                );
                unregister_cancel(conversation_id);
                return;
            }
        };
        runtime.block_on(drive_turn(
            app,
            llm,
            provider,
            model,
            prompt_budget,
            conversation_id,
            text,
            attachments,
            denied_names,
            cancel,
        ));
    });

    Ok(conversation_id)
}

/// Run one turn to completion on the current-thread runtime: capture the envelope, forward
/// the runtime's events onto the conversation's stream, drive the chat runtime, and
/// unregister the cancel token when done.
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
    conversation_id: i64,
    text: String,
    attachments: Vec<AttachmentRef>,
    denied_names: Vec<String>,
    cancel: CancellationToken,
) {
    // RAII: drop the registry entry when the turn ends, even on an early return/panic.
    struct CancelGuard(i64);
    impl Drop for CancelGuard {
        fn drop(&mut self) {
            unregister_cancel(self.0);
        }
    }
    let _guard = CancelGuard(conversation_id);

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
            AskCmdrStreamEvent::Failed {
                kind: AgentErrorKindView::Provider,
                detail: None,
            },
        );
        return;
    };

    // Forward the runtime's unbounded event channel onto the conversation's stream, until the
    // runtime drops its sender (the turn finished). ⚠️ There is deliberately no "the listener
    // went away" exit: a reload takes the webview, not the thread, and the whole point of a
    // conversation-keyed event is that the reloaded rail picks the same stream back up.
    let (tx, mut rx) = unbounded_channel::<AgentChatEvent>();
    let forward = forward_to_windows(conversation_id, &mut rx);
    let drive = runtime.send_message(
        &app,
        llm.as_ref(),
        provider,
        model,
        prompt_budget,
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
            AskCmdrStreamEvent::Failed {
                kind: AgentErrorKindView::Provider,
                detail: Some(e.to_string()),
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

/// Stop the in-flight turn for a thread. Idempotent: an unknown id (already finished) is a
/// no-op. A clean stop at the next tool boundary or stream chunk, not a hard abort.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_cancel(conversation_id: i64) {
    if let Some(token) = CANCELS.lock_ignore_poison().get(&conversation_id) {
        token.cancel();
    }
}

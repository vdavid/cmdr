//! Sending one message and streaming its answer: the slot resolution, the consent gate,
//! the snapshot-at-send envelope, the cancel registry, and the `Channel` bridge.
//!
//! ## Streaming
//!
//! [`ask_cmdr_send_message`] carries a Tauri [`Channel<AskCmdrStreamEvent>`], the same
//! shape `stream_folder_suggestions` uses: `Channel<T>` is not specta-friendly yet, so the
//! command rides raw `invoke` on the frontend (with the documented eslint opt-out), and its
//! wire event enum derives only `Serialize`.
//!
//! It adapts the runtime's [`AgentChatEvent`] seam: an `unbounded_channel` of runtime
//! events is forwarded onto the `Channel`, mapped to the wire enum. **No reasoning blob or
//! provider state ever crosses** — the runtime events already exclude them, and
//! [`MessageView`] carries display parts only.
//!
//! ## LLM resolution
//!
//! [`resolve_agent_llm`] resolves the Ask Cmdr interactive slot: a dedicated model choice
//! (`askCmdr.interactiveModel`, read fresh) layered over the shared `ai/` provider config
//! (provider on/off, keys, and base URLs stay single-sourced in `ai/`; only the model is
//! slot-specific), producing a [`GenaiAgentLlm`] at send time. A provider that is off or
//! unconfigured yields a typed `NotConfigured` event.
//!
//! ## Cancellation
//!
//! Cancel is keyed by `conversation_id` (single-flight means at most one active turn per
//! thread; the frontend disables the composer while a turn streams, so a thread never has
//! two concurrent sends). The command resolves/creates the conversation id up front, emits
//! `Started { conversationId }` first, and registers the turn's [`CancellationToken`] under
//! that id; [`ask_cmdr_cancel`] trips it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use chrono::{FixedOffset, Local};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::unbounded_channel;
use tokio_util::sync::CancellationToken;

use super::views::to_wire_event;
use super::{
    AgentErrorKindView, AskCmdrStreamEvent, AttachmentRef, LOG_TARGET, MessageBlock, MessageRoleView, MessageView,
    ModelWindowView, now_secs,
};
use crate::agent::AgentDb;
use crate::agent::chat::budget;
use crate::agent::chat::context::{ContextEnvelope, EnvelopeConnectivity, EnvelopeFreshness, EnvelopeVolume};
use crate::agent::chat::runtime::{AgentChatEvent, AgentErrorKind, ChatRuntime};
use crate::agent::consent::has_current_consent;
use crate::agent::llm::AgentLlm;
use crate::agent::llm::fake::FakeAgentLlm;
use crate::agent::llm::genai_impl::GenaiAgentLlm;
use crate::agent::llm::types::ProviderTag;
use crate::agent::store;
use crate::ai::client::AiBackend;
use crate::ai::llm_log::LlmLogContext;
use crate::ignore_poison::IgnorePoison;
use crate::mcp::PaneStateStore;
use crate::mcp::resources::volumes::{VolumeSummary, snapshot_volumes};

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

// ── Interim LLM + envelope + clock helpers ─────────────────────────────────────

/// A resolved-but-not-yet-built agent LLM. Resolution happens up front (to fail fast before
/// creating a thread), but the concrete `AgentLlm` is built only once the conversation id is
/// known, so the real backend can carry an [`LlmLogContext`] keyed on that conversation.
enum ResolvedAgentLlm {
    /// The real genai-backed LLM over the resolved `ai/` backend.
    Genai(AiBackend),
    /// The deterministic E2E fake (zero network; never logs — the tap is at the genai seam).
    Fake(FakeAgentLlm),
}

impl ResolvedAgentLlm {
    /// Builds the boxed `AgentLlm`, attaching a per-conversation logging context to the real
    /// backend so its requests/responses land under `llm-logs/thread-{id}/` (subject to the
    /// `logLlmCalls` setting). The fake bypasses the genai seam, so it logs nothing.
    fn into_llm(self, conversation_id: i64) -> Box<dyn AgentLlm> {
        match self {
            ResolvedAgentLlm::Genai(backend) => Box::new(GenaiAgentLlm::new(
                backend.with_log_context(LlmLogContext::agent_chat(conversation_id)),
            )),
            ResolvedAgentLlm::Fake(fake) => Box::new(fake),
        }
    }
}

/// Resolve the Ask Cmdr interactive slot into a ready LLM. The slot layers a dedicated
/// model choice (`askCmdr.interactiveModel`, read fresh) OVER the shared `ai/` provider
/// config (agent-spec D43): provider on/off, keys, and base URLs stay single-sourced in
/// `ai/`; only the model is slot-specific, so the bulk slot slots in later with no
/// migration (D49). An empty override uses the model the `ai/` provider is configured with.
/// Returns the backend plus the provider/model the cost meter records, or a typed error
/// when AI is off/unconfigured.
fn resolve_agent_llm(app: &AppHandle) -> Result<(ResolvedAgentLlm, ProviderTag, String), AgentErrorKind> {
    // E2E harness path: drive a deterministic scripted assistant with zero network, so the
    // rail's send-and-render can be tested without a provider. Guarded by an explicit env
    // flag so it never activates in a normal run.
    if crate::test_mode::ask_cmdr_fake_active() {
        return Ok((
            ResolvedAgentLlm::Fake(scripted_fake_llm()),
            ProviderTag::Local,
            "fake".to_string(),
        ));
    }
    let model_override = crate::settings::load_ask_cmdr_interactive_model(app);
    use crate::ai::manager::BackendResolution;
    match crate::ai::manager::resolve_backend_with_model(model_override.as_deref()) {
        BackendResolution::Ready(backend) => {
            let (provider, model) = provider_and_model(model_override.as_deref());
            Ok((ResolvedAgentLlm::Genai(backend), provider, model))
        }
        // "AI off", a blank cloud key, or a stopped local server all read the same to the
        // rail: nothing is configured to talk to. The settings surface disambiguates.
        BackendResolution::Off | BackendResolution::NotConfigured(_) | BackendResolution::UnknownProvider(_) => {
            Err(AgentErrorKind::NotConfigured)
        }
    }
}

/// The scripted turn the E2E fake streams: a short multi-chunk reply, so the test sees
/// streamed text land and a `Done`. Kept trivially deterministic.
fn scripted_fake_llm() -> FakeAgentLlm {
    use crate::agent::llm::fake::ScriptedTurn;
    FakeAgentLlm::script(vec![ScriptedTurn::Say(vec![
        "Hi! ".to_string(),
        "I'm the ".to_string(),
        "test assistant.".to_string(),
    ])])
}

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

/// The provider tag + effective model label for cost metering. The model is the
/// interactive slot's override when set, else the live `ai/` cloud model; a cloud model is
/// tagged by its name prefix, matching `ai::client`'s adapter routing. Local uses its fixed
/// model name.
fn provider_and_model(model_override: Option<&str>) -> (ProviderTag, String) {
    if crate::ai::state::get_provider() == "local" {
        return (
            ProviderTag::Local,
            crate::ai::manager::get_ai_runtime_status().model_name,
        );
    }
    let model = match model_override {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => {
            let (_key, _base, ai_model) = crate::ai::state::get_cloud_config();
            ai_model
        }
    };
    let provider = if model.starts_with("claude-") {
        ProviderTag::Anthropic
    } else if model.starts_with("gemini-") {
        ProviderTag::Gemini
    } else {
        ProviderTag::OpenAi
    };
    (provider, model)
}

/// The assembled-prompt token budget for the resolved slot, gathered here and decided in the
/// pure [`budget`] module. This layer supplies the two values the core may not read itself:
/// the user's `askCmdr.chatMemorySize` choice (read fresh per send, so a change applies to the
/// NEXT message and never to a turn already in flight) and the local server's configured
/// window.
///
/// The resolution's source is logged, so a budget that came from a stale family table is
/// visible in the log rather than silently authoritative.
fn resolve_prompt_budget(app: &AppHandle, provider: ProviderTag, model: &str) -> Result<usize, budget::BudgetRefusal> {
    // The E2E fake answers as a LOCAL provider with no local server behind it, so the real
    // resolution would size its budget from `ai.localContextSize` — a setting the harness has no
    // reason to touch, and whose value would then decide what the usage gauge shows in every E2E
    // run. Give the fake its own realistic budget instead: the harness keeps mirroring a real
    // user's settings, and the gauge under test shows what a normal user's would.
    if crate::test_mode::ask_cmdr_fake_active() {
        return Ok(budget::DEFAULT_PROMPT_TOKEN_BUDGET);
    }
    let resolved = budget::resolve_prompt_budget(budget::BudgetInputs {
        provider,
        model,
        user_choice: crate::settings::load_ask_cmdr_chat_memory_size(app),
        local_context_tokens: crate::ai::state::get_local_context_size(),
    })?;
    log::debug!(
        target: LOG_TARGET,
        "prompt budget for {model}: a {}-token ceiling, from {}",
        resolved.prompt_tokens,
        resolved.source.label()
    );
    if resolved.over_known_window {
        log::info!(
            target: LOG_TARGET,
            "the chat memory size ({} tokens) is above the {}-token window we believe {model} has; \
             using it anyway, the provider decides",
            resolved.prompt_tokens,
            resolved.known_window_tokens.unwrap_or(0)
        );
    }
    Ok(resolved.prompt_tokens)
}

/// Capture the context envelope from live app state (snapshot-at-send). Focused pane path
/// resolves from the focused SIDE's directory; volumes come from `snapshot_volumes`;
/// `attachments` are the references the user attached for this turn (path + kind only).
async fn capture_envelope<R: tauri::Runtime>(
    app: &AppHandle<R>,
    attachments: &[AttachmentRef],
    denied_names: Vec<String>,
    rename_batch_files: usize,
) -> ContextEnvelope {
    let (focused_pane_path, cursor_item, selection_count) = match app.try_state::<PaneStateStore>() {
        Some(store) => {
            let side = store.get_focused_pane();
            let pane = if side == "right" {
                store.get_right()
            } else {
                store.get_left()
            };
            let path = (!pane.path.is_empty()).then(|| pane.path.clone());
            let cursor = pane.files.get(pane.cursor_index).map(|f| f.name.clone());
            (path, cursor, pane.selected_indices.len() as u32)
        }
        None => (None, None, 0),
    };
    let volumes = snapshot_volumes().await.iter().map(to_envelope_volume).collect();
    ContextEnvelope {
        captured_at: now_secs(),
        focused_pane_path,
        cursor_item,
        selection_count,
        volumes,
        attachments: attachments.iter().map(AttachmentRef::to_envelope).collect(),
        denied_names,
        rename_batch_files,
    }
}

/// Map a live volume summary to the envelope's pure mirror. The freshness/connectivity
/// values are OUR OWN stable tokens (the same ones `list_volumes` emits), parsed by exact
/// match like a `from_token` — not error/state-string classification.
fn to_envelope_volume(summary: &VolumeSummary) -> EnvelopeVolume {
    let freshness = match summary.index_status {
        Some("fresh") => EnvelopeFreshness::Fresh,
        Some("scanning") => EnvelopeFreshness::Scanning,
        Some("stale") => EnvelopeFreshness::Stale,
        _ => EnvelopeFreshness::Off,
    };
    let connectivity = match summary.smb_connection_state {
        Some("direct") => Some(EnvelopeConnectivity::Direct),
        Some("os_mount") => Some(EnvelopeConnectivity::OsMount),
        Some("disconnected") => Some(EnvelopeConnectivity::Disconnected),
        _ => None,
    };
    EnvelopeVolume {
        name: summary.name.clone(),
        freshness,
        connectivity,
    }
}

/// The local UTC offset now, for rendering timestamps in the user's timezone.
fn local_offset() -> FixedOffset {
    *Local::now().offset()
}

// ── Commands ───────────────────────────────────────────────────────────────────

/// Send one user message to a thread and stream the answer. `conversation_id` is `None`
/// to start a fresh thread (its id arrives in the first `Started` event and as the
/// resolved return value). All progress rides `on_event`; the `Result` exists only
/// because `#[tauri::command]` requires one, and always resolves `Ok` (failures surface as
/// a typed `Failed` event, per the streaming pattern).
///
/// The turn runs on a dedicated thread with its own current-thread runtime: the chat
/// runtime holds a rusqlite `Connection` (not `Send`) across awaits, so its future can't
/// live on the Tauri command future or a multi-thread tokio task. The command returns the
/// conversation id at once; streaming keeps flowing over `on_event` on that thread.
#[tauri::command]
pub async fn ask_cmdr_send_message(
    app: AppHandle,
    conversation_id: Option<i64>,
    text: String,
    attachments: Vec<AttachmentRef>,
    // Destination names the user turned down in this thread's last rename review, newest
    // first. Names only: a reason would be model-authored, and the next batch would inherit
    // the rationalization instead of the fact.
    denied_names: Vec<String>,
    on_event: Channel<AskCmdrStreamEvent>,
) -> Result<i64, String> {
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        let _ = on_event.send(AskCmdrStreamEvent::Failed {
            kind: AgentErrorKindView::NotConfigured,
            detail: None,
        });
        return Ok(conversation_id.unwrap_or(0));
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
        let _ = on_event.send(AskCmdrStreamEvent::Failed {
            kind: AgentErrorKindView::NoConsent,
            detail: None,
        });
        return Ok(conversation_id.unwrap_or(0));
    }

    // Resolve the LLM only after the consent gate: if AI is off/unconfigured, say so and add
    // no thread.
    let (llm_kind, provider, model) = match resolve_agent_llm(&app) {
        Ok(resolved) => resolved,
        Err(kind) => {
            let _ = on_event.send(AskCmdrStreamEvent::Failed {
                kind: kind.into(),
                detail: None,
            });
            return Ok(conversation_id.unwrap_or(0));
        }
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
            let _ = on_event.send(AskCmdrStreamEvent::Failed {
                kind: AgentErrorKindView::LocalWindowTooSmall,
                detail: None,
            });
            return Ok(conversation_id.unwrap_or(0));
        }
    };

    // Resolve/create the conversation id up front so cancel + the frontend can key on it.
    let conversation_id = match conversation_id {
        Some(id) => id,
        None => match create_conversation_now(&db_path, &text) {
            Ok(id) => id,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "creating a conversation failed: {e}");
                let _ = on_event.send(AskCmdrStreamEvent::Failed {
                    kind: AgentErrorKindView::Provider,
                    detail: Some(e.to_string()),
                });
                return Ok(0);
            }
        },
    };
    let _ = on_event.send(AskCmdrStreamEvent::Started { conversation_id });

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
                let _ = on_event.send(AskCmdrStreamEvent::Failed {
                    kind: AgentErrorKindView::Provider,
                    detail: Some(e.to_string()),
                });
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
            on_event,
            cancel,
        ));
    });

    Ok(conversation_id)
}

/// Run one turn to completion on the current-thread runtime: capture the envelope, bridge
/// the runtime's events onto the `Channel`, drive the chat runtime, and unregister the
/// cancel token when done.
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
    on_event: Channel<AskCmdrStreamEvent>,
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

    let envelope = capture_envelope(&app, &attachments, denied_names, budget::files_per_batch(prompt_budget)).await;
    let offset = local_offset();

    let Some(runtime) = app.try_state::<ChatRuntime>() else {
        let _ = on_event.send(AskCmdrStreamEvent::Failed {
            kind: AgentErrorKindView::Provider,
            detail: None,
        });
        return;
    };

    // Bridge the runtime's unbounded event channel onto the Tauri `Channel`. The forwarder
    // drains until the runtime drops its sender (the turn finished).
    let (tx, mut rx) = unbounded_channel::<AgentChatEvent>();
    let forward = async {
        while let Some(event) = rx.recv().await {
            if on_event.send(to_wire_event(event)).is_err() {
                break; // the webview is gone; the turn keeps running to persist its state
            }
        }
    };
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
        let _ = on_event.send(AskCmdrStreamEvent::Failed {
            kind: AgentErrorKindView::Provider,
            detail: Some(e.to_string()),
        });
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

//! IPC commands for Ask Cmdr, the read-only chat rail (spec `docs/specs/ask-cmdr-spec.md`,
//! plan § M6). Thin pass-throughs: the runtime, store, and context assembly all live in
//! [`crate::agent`]; these commands only bridge the frontend to them.
//!
//! ## Streaming
//!
//! [`ask_cmdr_send_message`] carries a Tauri [`Channel<AskCmdrStreamEvent>`], the same
//! shape `stream_folder_suggestions` uses: `Channel<T>` is not specta-friendly yet, so the
//! command rides raw `invoke` on the frontend (with the documented eslint opt-out), and its
//! wire event enum derives only `Serialize`. The other three commands are plain specta
//! commands invoked through the generated bindings.
//!
//! It adapts the runtime's [`AgentChatEvent`](crate::agent::chat::runtime::AgentChatEvent)
//! seam: an `unbounded_channel` of runtime events is forwarded onto the `Channel`, mapped to
//! the wire enum. **No reasoning blob or provider state ever crosses** — the runtime events
//! already exclude them, and [`MessageView`] carries display parts only.
//!
//! ## Interim LLM resolution
//!
//! The interactive model slot is M8. Until then this resolves the EXISTING `ai/` provider
//! config (`ai.provider` + cloud model / local server) into a [`GenaiAgentLlm`] at send
//! time, so the rail is drivable in dev against the local llama-server with zero new
//! settings. A provider that is off or unconfigured yields a typed `NotConfigured` event.
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
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::unbounded_channel;
use tokio_util::sync::CancellationToken;

use crate::agent::AgentDb;
use crate::agent::chat::context::{
    AttachmentKind, ContextEnvelope, EnvelopeAttachment, EnvelopeConnectivity, EnvelopeFreshness, EnvelopeVolume,
};
use crate::agent::chat::runtime::{AgentChatEvent, AgentErrorKind, ChatRuntime};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::fake::FakeAgentLlm;
use crate::agent::llm::genai_impl::GenaiAgentLlm;
use crate::agent::llm::types::{AgentPart, AgentRole, AgentStopReason, AgentUsage, ProviderTag};
use crate::agent::store::{self, ConversationRow, ConversationSearchHit, StoredMessage};
use crate::ai::client::AiBackend;
use crate::ai::llm_log::LlmLogContext;
use crate::ignore_poison::IgnorePoison;
use crate::mcp::PaneStateStore;
use crate::mcp::resources::volumes::{VolumeSummary, snapshot_volumes};

const LOG_TARGET: &str = "agent::ipc";

// ── The wire event enum (Channel; Serialize only, not specta) ──────────────────

/// A streamed progress event for the rail. `type`-tagged camelCase, mirroring the
/// runtime's [`AgentChatEvent`] minus anything backend-only. Never carries a reasoning
/// blob or provider state.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AskCmdrStreamEvent {
    /// First event: the resolved (possibly newly created) conversation id, so the
    /// frontend can key the stop button and bootstrap the active thread immediately.
    Started { conversation_id: i64 },
    /// The send queued behind this thread's running turn (drives "working… stop?").
    Queued,
    /// The user's message was persisted (on the first `respond` `End`).
    UserPersisted { message_id: i64, seq: i64 },
    /// A new assistant turn began streaming (no id yet — the row lands on `Done`).
    AssistantStarted,
    /// A chunk of assistant text.
    TextDelta { text: String },
    /// Opaque reasoning progressed; the UI shows "thinking…", content never surfaced.
    ReasoningTick,
    /// The model started a tool call (a collapsible "looked at X" line; the label is
    /// built frontend-side from `tool`, never a backend string).
    ToolCallStarted { call_id: String, tool: String },
    /// A tool call finished dispatching (`ok = false` for a refusal or handler problem).
    ToolCallFinished { call_id: String, ok: bool },
    /// The turn produced its final answer, carrying the persisted assistant id.
    Done {
        message_id: i64,
        seq: i64,
        stop: StopReasonView,
        usage: UsageView,
    },
    /// The turn ended without an answer, typed and honest (rendered without the words
    /// "error"/"failed" — the frontend owns the copy).
    Failed { kind: AgentErrorKindView },
}

/// The wire form of [`AgentErrorKind`] — the frontend renders each honestly.
#[derive(Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentErrorKindView {
    NoKey,
    NotConfigured,
    Unavailable,
    Timeout,
    AuthFailed,
    RateLimited,
    BudgetExhausted,
    UnfinishedReply,
    Provider,
}

impl From<AgentErrorKind> for AgentErrorKindView {
    fn from(kind: AgentErrorKind) -> Self {
        match kind {
            AgentErrorKind::NoKey => Self::NoKey,
            AgentErrorKind::NotConfigured => Self::NotConfigured,
            AgentErrorKind::Unavailable => Self::Unavailable,
            AgentErrorKind::Timeout => Self::Timeout,
            AgentErrorKind::AuthFailed => Self::AuthFailed,
            AgentErrorKind::RateLimited => Self::RateLimited,
            AgentErrorKind::BudgetExhausted => Self::BudgetExhausted,
            AgentErrorKind::UnfinishedReply => Self::UnfinishedReply,
            AgentErrorKind::Provider => Self::Provider,
        }
    }
}

/// The wire form of [`AgentStopReason`], collapsed to unit variants (the provider's raw
/// `Other` string is not surfaced).
#[derive(Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StopReasonView {
    Completed,
    ToolCall,
    MaxTokens,
    ContentFilter,
    StopSequence,
    Other,
}

impl From<AgentStopReason> for StopReasonView {
    fn from(stop: AgentStopReason) -> Self {
        match stop {
            AgentStopReason::Completed => Self::Completed,
            AgentStopReason::ToolCall => Self::ToolCall,
            AgentStopReason::MaxTokens => Self::MaxTokens,
            AgentStopReason::ContentFilter => Self::ContentFilter,
            AgentStopReason::StopSequence => Self::StopSequence,
            AgentStopReason::Other(_) => Self::Other,
        }
    }
}

/// Per-turn token usage, camelCase for the wire.
#[derive(Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageView {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

impl From<AgentUsage> for UsageView {
    fn from(usage: AgentUsage) -> Self {
        Self {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
        }
    }
}

/// Map a runtime event to its wire form.
fn to_wire_event(event: AgentChatEvent) -> AskCmdrStreamEvent {
    match event {
        AgentChatEvent::Queued => AskCmdrStreamEvent::Queued,
        AgentChatEvent::UserPersisted { message_id, seq } => AskCmdrStreamEvent::UserPersisted { message_id, seq },
        AgentChatEvent::AssistantStarted => AskCmdrStreamEvent::AssistantStarted,
        AgentChatEvent::TextDelta { text } => AskCmdrStreamEvent::TextDelta { text },
        AgentChatEvent::ReasoningTick => AskCmdrStreamEvent::ReasoningTick,
        AgentChatEvent::ToolCallStarted { call_id, tool } => AskCmdrStreamEvent::ToolCallStarted {
            call_id,
            tool: tool.as_wire_name().to_string(),
        },
        AgentChatEvent::ToolCallFinished { call_id, ok } => AskCmdrStreamEvent::ToolCallFinished { call_id, ok },
        AgentChatEvent::Done {
            message_id,
            seq,
            stop,
            usage,
        } => AskCmdrStreamEvent::Done {
            message_id,
            seq,
            stop: stop.into(),
            usage: usage.into(),
        },
        AgentChatEvent::Failed { kind } => AskCmdrStreamEvent::Failed { kind: kind.into() },
    }
}

// ── Display-only message projection (specta) ───────────────────────────────────

/// A message's role, on the wire.
#[derive(Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MessageRoleView {
    System,
    User,
    Assistant,
    Tool,
}

impl From<AgentRole> for MessageRoleView {
    fn from(role: AgentRole) -> Self {
        match role {
            AgentRole::System => Self::System,
            AgentRole::User => Self::User,
            AgentRole::Assistant => Self::Assistant,
            AgentRole::Tool => Self::Tool,
        }
    }
}

/// One display block of a message. A projection of the stored [`AgentPart`]s that DROPS
/// the reasoning part entirely (the opaque provider blob is backend-only and never
/// crosses IPC — the store's `content_blocks` invariant).
#[derive(Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MessageBlock {
    /// Assistant or user prose (rendered markdown-lite, entity-escaped first).
    Text { text: String },
    /// A tool the model invoked. `arguments` is the raw JSON call arguments as a string
    /// (the frontend `JSON.parse`s it to build a localized "looked at X" label); both
    /// `tool` and any filesystem-derived args render as escaped plain text, never `{@html}`.
    ToolCall {
        call_id: String,
        tool: String,
        arguments: String,
    },
    /// A tool result, reduced to its status (`ok`/`elided`) — the raw content stays
    /// backend-only.
    ToolResult { call_id: String, ok: bool, elided: bool },
}

/// A message as the rail displays it: id/seq/role, its display blocks, and token counts.
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: i64,
    pub seq: i64,
    pub role: MessageRoleView,
    pub blocks: Vec<MessageBlock>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub created_at: i64,
}

/// A conversation header plus a page of its display messages, and the total count so a
/// paged UI knows whether more exist.
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetailView {
    pub conversation: ConversationRow,
    pub messages: Vec<MessageView>,
    pub total_messages: u32,
}

// ── Attachments (by reference; path + kind, never contents) ─────────────────────

/// Whether an attachment references a file or a folder, on the wire.
#[derive(Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKindView {
    File,
    Folder,
}

/// A file/folder the user attached by reference for a turn (dragged onto the composer,
/// or "ask about selection"). Structurally path + kind only — the read-only privacy
/// line means no tool ever reads its contents. Both directions: an input to
/// [`ask_cmdr_send_message`], and the output of the two attachment-resolving commands.
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    pub path: String,
    pub kind: AttachmentKindView,
}

impl AttachmentRef {
    fn to_envelope(&self) -> EnvelopeAttachment {
        EnvelopeAttachment {
            path: self.path.clone(),
            kind: match self.kind {
                AttachmentKindView::File => AttachmentKind::File,
                AttachmentKindView::Folder => AttachmentKind::Folder,
            },
        }
    }
}

/// True when a persisted tool result is a real answer rather than a refusal or a handler
/// problem. Reads OUR OWN typed result keys (`available` / `problem`), never external
/// wording — matching the runtime's `dispatch_ok`.
fn tool_result_ok(content: &Value) -> bool {
    let refused = content.get("available") == Some(&Value::Bool(false));
    let problem = content.get("problem").is_some();
    !(refused || problem)
}

/// Project one stored message into its display form, dropping reasoning parts.
fn to_message_view(message: StoredMessage) -> MessageView {
    let blocks = message
        .parts
        .into_iter()
        .filter_map(|part| match part {
            AgentPart::Text(text) => Some(MessageBlock::Text { text }),
            AgentPart::ToolCall(call) => Some(MessageBlock::ToolCall {
                call_id: call.call_id,
                tool: call.tool.as_wire_name().to_string(),
                arguments: call.arguments.to_string(),
            }),
            AgentPart::ToolResult(result) => Some(MessageBlock::ToolResult {
                ok: tool_result_ok(&result.content),
                call_id: result.call_id,
                elided: result.elided,
            }),
            AgentPart::Reasoning(_) => None,
        })
        .collect();
    MessageView {
        id: message.id,
        seq: message.seq,
        role: message.role.into(),
        blocks,
        prompt_tokens: message.prompt_tokens,
        completion_tokens: message.completion_tokens,
        created_at: message.created_at,
    }
}

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
    if std::env::var("CMDR_E2E_ASK_CMDR_FAKE").is_ok() {
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

/// Capture the context envelope from live app state (snapshot-at-send). Focused pane path
/// resolves from the focused SIDE's directory; volumes come from `snapshot_volumes`;
/// `attachments` are the references the user attached for this turn (path + kind only).
async fn capture_envelope<R: tauri::Runtime>(app: &AppHandle<R>, attachments: &[AttachmentRef]) -> ContextEnvelope {
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

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
    on_event: Channel<AskCmdrStreamEvent>,
) -> Result<i64, String> {
    // Resolve the LLM before touching the DB: if AI is off/unconfigured, say so and add
    // no thread.
    let (llm_kind, provider, model) = match resolve_agent_llm(&app) {
        Ok(resolved) => resolved,
        Err(kind) => {
            let _ = on_event.send(AskCmdrStreamEvent::Failed { kind: kind.into() });
            return Ok(conversation_id.unwrap_or(0));
        }
    };

    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        let _ = on_event.send(AskCmdrStreamEvent::Failed {
            kind: AgentErrorKindView::NotConfigured,
        });
        return Ok(conversation_id.unwrap_or(0));
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
            conversation_id,
            text,
            attachments,
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
    conversation_id: i64,
    text: String,
    attachments: Vec<AttachmentRef>,
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

    let envelope = capture_envelope(&app, &attachments).await;
    let offset = local_offset();

    let Some(runtime) = app.try_state::<ChatRuntime>() else {
        let _ = on_event.send(AskCmdrStreamEvent::Failed {
            kind: AgentErrorKindView::Provider,
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

/// One conversation's header plus a page of its display messages (oldest first). `None`
/// when the thread is absent or the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_get_conversation(
    app: AppHandle,
    id: i64,
    msg_limit: u32,
    msg_offset: u32,
) -> Result<Option<ConversationDetailView>, String> {
    with_read_connection(app, None, move |conn| {
        let Some(detail) = store::get_conversation(conn, id, msg_limit, msg_offset)? else {
            return Ok(None);
        };
        Ok(Some(ConversationDetailView {
            conversation: detail.conversation,
            messages: detail.messages.into_iter().map(to_message_view).collect(),
            total_messages: detail.total_messages,
        }))
    })
    .await
}

/// Conversations newest-activity first, paged. Empty when the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_list_conversations(
    app: AppHandle,
    limit: u32,
    offset: u32,
    include_archived: bool,
) -> Result<Vec<ConversationRow>, String> {
    with_read_connection(app, Vec::new(), move |conn| {
        store::list_conversations(conn, limit, offset, include_archived)
    })
    .await
}

/// Conversations whose messages match `query` (FTS5, sanitized), newest-match first,
/// paged. Each hit carries a plain-text snippet around the match. Empty when the store
/// never opened or the query has no searchable term.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_search_conversations(
    app: AppHandle,
    query: String,
    limit: u32,
    offset: u32,
) -> Result<Vec<ConversationSearchHit>, String> {
    with_read_connection(app, Vec::new(), move |conn| {
        store::search_conversations(conn, &query, limit, offset)
    })
    .await
}

/// Rename a conversation. A no-op when the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_rename_conversation(app: AppHandle, id: i64, title: String) -> Result<(), String> {
    with_write_connection(app, move |conn| store::rename_conversation(conn, id, &title)).await
}

/// Archive or unarchive a conversation (no delete in v1 — the flag filters the list). A
/// no-op when the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_archive_conversation(app: AppHandle, id: i64, archived: bool) -> Result<(), String> {
    with_write_connection(app, move |conn| store::archive_conversation(conn, id, archived)).await
}

/// "Ask about selection": attachment refs for the focused pane's current selection, or
/// its cursor item when nothing is selected. Reads [`PaneStateStore`] — the same live
/// source the envelope uses — so kinds come from known pane state, with no filesystem
/// stat. Empty when no pane state is registered.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_selection_attachments(app: AppHandle) -> Vec<AttachmentRef> {
    let Some(store) = app.try_state::<PaneStateStore>() else {
        return Vec::new();
    };
    let pane = if store.get_focused_pane() == "right" {
        store.get_right()
    } else {
        store.get_left()
    };
    let indices = if pane.selected_indices.is_empty() {
        vec![pane.cursor_index]
    } else {
        pane.selected_indices.clone()
    };
    indices
        .into_iter()
        .filter_map(|i| pane.files.get(i))
        .filter(|entry| !entry.path.is_empty())
        .map(pane_entry_to_attachment)
        .collect()
}

/// Resolve dragged local paths into typed attachment refs. Kinds come from the known
/// pane files (left + right) — no filesystem stat — defaulting to `File` for an unknown
/// path. The frontend only calls this for LOCAL drags; virtual-volume drag paths
/// mis-resolve after the pasteboard round-trip and are not supported in v1.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_resolve_attachments(app: AppHandle, paths: Vec<String>) -> Vec<AttachmentRef> {
    let mut is_dir_by_path: HashMap<String, bool> = HashMap::new();
    if let Some(store) = app.try_state::<PaneStateStore>() {
        for pane in [store.get_left(), store.get_right()] {
            for entry in pane.files {
                is_dir_by_path.insert(entry.path, entry.is_directory);
            }
        }
    }
    paths
        .into_iter()
        .filter(|path| !path.is_empty())
        .map(|path| {
            let is_dir = is_dir_by_path.get(&path).copied().unwrap_or(false);
            AttachmentRef {
                kind: if is_dir {
                    AttachmentKindView::Folder
                } else {
                    AttachmentKindView::File
                },
                path,
            }
        })
        .collect()
}

/// Map a known pane file entry to an attachment ref (kind straight from `is_directory`).
fn pane_entry_to_attachment(entry: &crate::mcp::pane_state::PaneFileEntry) -> AttachmentRef {
    AttachmentRef {
        path: entry.path.clone(),
        kind: if entry.is_directory {
            AttachmentKindView::Folder
        } else {
            AttachmentKindView::File
        },
    }
}

// ── Consent (the opt-in gate; plan §12) ─────────────────────────────────────────

/// The current consent-copy version. **Bump when the consent screen's privacy copy
/// changes materially**, so users re-accept the new wording. The copy itself lives in the
/// frontend catalog (`askCmdr.consent.*`); this integer is the machine-checkable version of
/// that copy, recorded in `main.db` when the user accepts.
pub const CONSENT_COPY_VERSION: u32 = 1;

/// Whether the user has opted into Ask Cmdr, and the audit of what they accepted. The rail
/// gates on `accepted` (the CURRENT copy version): a never-accepted or stale-version record
/// re-shows the consent screen, and nothing is ever sent to a provider without it.
#[derive(Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskCmdrConsentStatus {
    /// True only when the user accepted the CURRENT `current_version`. The one flag the
    /// rail and the settings toggle read.
    pub accepted: bool,
    /// The copy version the user must have accepted to be `accepted`.
    pub current_version: u32,
    /// The version the user last accepted, or `None` if never.
    pub accepted_version: Option<u32>,
    /// When the user last accepted (unix secs), or `None` if never.
    pub accepted_at: Option<i64>,
}

/// The Ask Cmdr consent status: whether the user opted into the CURRENT consent copy, plus
/// the audit of what/when they accepted. Reads `main.db`; a missing store reads as
/// not-accepted, so the gate stays closed rather than failing open.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_consent_status(app: AppHandle) -> Result<AskCmdrConsentStatus, String> {
    let not_accepted = AskCmdrConsentStatus {
        accepted: false,
        current_version: CONSENT_COPY_VERSION,
        accepted_version: None,
        accepted_at: None,
    };
    with_read_connection(app, not_accepted, move |conn| {
        let stored = store::get_consent(conn)?;
        Ok(AskCmdrConsentStatus {
            accepted: stored.map(|c| c.version) == Some(CONSENT_COPY_VERSION),
            current_version: CONSENT_COPY_VERSION,
            accepted_version: stored.map(|c| c.version),
            accepted_at: stored.map(|c| c.at),
        })
    })
    .await
}

/// Record the user's opt-in to the current consent copy (timestamp + copy version), so the
/// rail unlocks. Idempotent.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_accept_consent(app: AppHandle) -> Result<(), String> {
    let now = now_secs();
    with_write_connection(app, move |conn| store::set_consent(conn, CONSENT_COPY_VERSION, now)).await
}

/// Turn Ask Cmdr off by clearing consent (the settings "turn off" path). The next rail
/// open re-shows the consent screen. No delete of chats — history stays.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_revoke_consent(app: AppHandle) -> Result<(), String> {
    with_write_connection(app, store::clear_consent).await
}

// ── Cost visibility (per-thread footer + per-day rollup) ─────────────────────────

/// One conversation's cumulative token + cost total (all days, all models), for the
/// per-thread footer. Zeroed for a thread with no metered turn yet. Empty store ⇒ zeroed.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_conversation_cost(app: AppHandle, id: i64) -> Result<store::ConversationCost, String> {
    let empty = store::ConversationCost {
        prompt_tokens: 0,
        completion_tokens: 0,
        cost_micros: 0,
        fully_priced: true,
        providers: Vec::new(),
    };
    with_read_connection(app, empty, move |conn| store::conversation_cost(conn, id)).await
}

/// The per-day cost rollup across every thread and model, newest day first (the settings
/// spend display). Empty when the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_cost_summary(app: AppHandle) -> Result<store::CostSummary, String> {
    let empty = store::CostSummary { days: Vec::new() };
    with_read_connection(app, empty, store::cost_summary).await
}

/// Open a short-lived WRITE connection to `main.db` off the IPC thread and run `write`
/// (opening a write connection runs the idempotent migration ladder). A missing store
/// (agent start failed) is a silent no-op — there are no conversations to mutate.
async fn with_write_connection<F>(app: AppHandle, write: F) -> Result<(), String>
where
    F: FnOnce(&rusqlite::Connection) -> Result<(), store::AgentStoreError> + Send + 'static,
{
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        return Ok(());
    };
    tauri::async_runtime::spawn_blocking(move || {
        let conn = store::open_write_connection(&db_path).map_err(|e| e.to_string())?;
        write(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Open a short-lived read connection to `main.db` off the IPC thread and run `read`. A
/// missing store (agent start failed) yields the read's `empty` result, so the rail
/// degrades to "no history" rather than surfacing a failure.
async fn with_read_connection<T, F>(app: AppHandle, empty: T, read: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T, store::AgentStoreError> + Send + 'static,
{
    let Some(db_path) = app.try_state::<AgentDb>().map(|db| db.db_path().to_path_buf()) else {
        return Ok(empty);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let conn = store::open_read_connection(&db_path).map_err(|e| e.to_string())?;
        read(&conn).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

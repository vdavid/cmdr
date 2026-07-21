//! IPC commands for Ask Cmdr, the read-only chat rail (spec `docs/specs/ask-cmdr-spec.md`).
//! Thin pass-throughs: the runtime, store, and context assembly all live in
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
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

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
use crate::agent::tools::propose::rename::{
    BulkRenamePreflight, BulkRenamePreflightStatus, RenameProposalStore, RenameSourceFingerprint,
};
use crate::ai::client::AiBackend;
use crate::ai::llm_log::LlmLogContext;
use crate::commands::util::IpcError;
use crate::ignore_poison::IgnorePoison;
use crate::mcp::PaneStateStore;
use crate::mcp::resources::volumes::{VolumeSummary, snapshot_volumes};

const LOG_TARGET: &str = "agent::ipc";
const BULK_RENAME_PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(5);
const BULK_RENAME_APPLY_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// Display-only rename rows for the review surface. The frontend must send
    /// only opaque ids back when a later user action approves them.
    ProposalReady {
        proposal: crate::agent::tools::propose::rename::RenameProposalSnapshot,
    },
    /// The turn produced its final answer, carrying the persisted assistant id.
    Done {
        message_id: i64,
        seq: i64,
        stop: StopReasonView,
        usage: UsageView,
    },
    /// The turn ended without an answer, typed and honest (rendered without the words
    /// "error"/"failed" — the frontend owns the copy). `detail` is the source error's own
    /// wording, shown verbatim under the typed headline so the user sees what to fix;
    /// display only — the frontend branches on `kind`, never on this string.
    Failed {
        kind: AgentErrorKindView,
        detail: Option<String>,
    },
    /// The conversation's effective model changed since its previous turn; the persisted
    /// event row's identity rides along. The rail inserts the line BEFORE this turn's
    /// user bubble (the change happened between the turns).
    ModelChanged { message_id: i64, seq: i64, model: String },
}

/// The wire form of [`AgentErrorKind`] — the frontend renders each honestly.
#[derive(Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentErrorKindView {
    NoKey,
    NotConfigured,
    /// The user hasn't accepted the current consent copy — the backend refuses the send
    /// before touching a provider (the privacy line, enforced structurally, not just in the
    /// rail UI). Distinct from `NotConfigured` so the copy can say so honestly.
    NoConsent,
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
        AgentChatEvent::ProposalReady { proposal } => AskCmdrStreamEvent::ProposalReady { proposal },
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
        AgentChatEvent::Failed { kind, detail } => AskCmdrStreamEvent::Failed {
            kind: kind.into(),
            detail,
        },
        AgentChatEvent::ModelChanged { message_id, seq, model } => {
            AskCmdrStreamEvent::ModelChanged { message_id, seq, model }
        }
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
    /// A UI-facing timeline entry (a model change), not transcript content.
    Event,
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
    /// The conversation's effective model changed between turns; `model` is the new name.
    /// Rendered as a small centered timeline line, escaped plain text (never `{@html}`).
    ModelChanged { model: String },
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

/// Project one stored message into its display form, dropping reasoning parts. Event
/// rows project to `role: Event` with their single typed block.
fn to_message_view(message: StoredMessage) -> MessageView {
    let (role, blocks): (MessageRoleView, Vec<MessageBlock>) = match message.content {
        store::StoredContent::Message { role, parts } => {
            let blocks = parts
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
            (role.into(), blocks)
        }
        store::StoredContent::Event(store::ConversationEvent::ModelChanged { model }) => {
            (MessageRoleView::Event, vec![MessageBlock::ModelChanged { model }])
        }
    };
    MessageView {
        id: message.id,
        seq: message.seq,
        role,
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

/// Revalidates the user-selected subset of a server-owned rename proposal. The
/// frontend supplies opaque ids only, never source paths or destination names.
#[tauri::command]
#[specta::specta]
pub async fn preflight_bulk_rename(
    app: AppHandle,
    proposal_id: String,
    allowed_row_ids: Vec<String>,
) -> Result<BulkRenamePreflight, IpcError> {
    tokio::time::timeout(
        BULK_RENAME_PREFLIGHT_TIMEOUT,
        crate::agent::tools::propose::rename::preflight(&app, proposal_id, allowed_row_ids),
    )
    .await
    .map_err(|_| IpcError::timeout())
}

/// Starts the user-approved subset of a server-owned rename plan. Paths and
/// names never cross this IPC boundary: the frontend submits only opaque ids.
#[tauri::command]
#[specta::specta]
pub async fn apply_bulk_rename(
    app: AppHandle,
    proposal_id: String,
    allowed_row_ids: Vec<String>,
) -> Result<crate::file_system::write_operations::WriteOperationStartResult, IpcError> {
    let Some(store) = app.try_state::<RenameProposalStore>() else {
        return Err(IpcError::from_err(
            "This rename review has expired. Ask Cmdr to prepare it again.",
        ));
    };

    // The normal dialog path always arrives with this exact preflight. A stale
    // client retries the bounded authoritative preflight instead of trusting old
    // rows or accepting a different subset.
    if store.accepted_preflight(&proposal_id, &allowed_row_ids).is_none() {
        let preflight = tokio::time::timeout(
            BULK_RENAME_APPLY_TIMEOUT,
            crate::agent::tools::propose::rename::preflight(&app, proposal_id.clone(), allowed_row_ids.clone()),
        )
        .await
        .map_err(|_| IpcError::timeout())?;
        if preflight.status != BulkRenamePreflightStatus::Ready {
            return Err(IpcError::from_err("Review the rename plan again before applying it."));
        }
    }

    let Some((proposal, accepted)) = store.take_accepted_preflight(&proposal_id, &allowed_row_ids) else {
        return Err(IpcError::from_err(
            "This rename review has expired. Ask Cmdr to prepare it again.",
        ));
    };
    let Some(volume_id) = proposal.rows.first().map(|row| row.volume_id.clone()) else {
        return Err(IpcError::from_err("This rename plan has no rows to apply."));
    };
    if proposal.rows.iter().any(|row| row.volume_id != volume_id) {
        return Err(IpcError::from_err("A rename plan must stay on one volume."));
    }

    let fingerprints: HashMap<_, _> = accepted
        .fingerprints
        .into_iter()
        .map(|fingerprint| (fingerprint_row_id(&fingerprint).to_string(), fingerprint))
        .collect();
    let mut rows = Vec::with_capacity(allowed_row_ids.len());
    for row_id in &allowed_row_ids {
        let Some(proposal_row) = proposal.rows.iter().find(|row| &row.row_id == row_id) else {
            return Err(IpcError::from_err("Review the rename plan again before applying it."));
        };
        let Some(fingerprint) = fingerprints.get(row_id) else {
            return Err(IpcError::from_err("Review the rename plan again before applying it."));
        };
        rows.push(crate::file_system::write_operations::BulkRenameRow {
            row_id: row_id.clone(),
            source: PathBuf::from(&proposal_row.source_path),
            destination: Path::new(&proposal_row.source_path)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(&proposal_row.destination_name),
            expected_fingerprint: map_bulk_rename_fingerprint(fingerprint),
        });
    }

    crate::file_system::write_operations::start_bulk_rename(
        Arc::new(crate::file_system::write_operations::TauriEventSink::new(app)),
        volume_id,
        rows,
        crate::operation_log::types::Initiator::Agent,
    )
    .map_err(IpcError::from_err)
}

fn fingerprint_row_id(fingerprint: &RenameSourceFingerprint) -> &str {
    match fingerprint {
        RenameSourceFingerprint::Local { row_id, .. } | RenameSourceFingerprint::Remote { row_id, .. } => row_id,
    }
}

fn map_bulk_rename_fingerprint(
    fingerprint: &RenameSourceFingerprint,
) -> crate::file_system::write_operations::BulkRenameFingerprint {
    match fingerprint {
        RenameSourceFingerprint::Local {
            device,
            inode,
            size,
            modified_nanos,
            ..
        } => crate::file_system::write_operations::BulkRenameFingerprint::Local {
            device: *device,
            inode: *inode,
            size: *size,
            modified_nanos: *modified_nanos,
        },
        RenameSourceFingerprint::Remote {
            normalized_path,
            size,
            modified,
            ..
        } => crate::file_system::write_operations::BulkRenameFingerprint::Remote {
            normalized_path: normalized_path.clone(),
            size: *size,
            modified: *modified,
        },
    }
}

/// Discards a staged proposal after the user closes its review. There is no
/// agent-controlled approval route: only this user action consumes the plan.
#[tauri::command]
#[specta::specta]
pub fn cancel_bulk_rename_proposal(app: AppHandle, proposal_id: String) {
    if let Some(store) = app.try_state::<RenameProposalStore>() {
        store.consume(&proposal_id);
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

// The copy version + the gate predicate live in `crate::agent::consent` so both the send
// path (structural enforcement) and these status/accept commands share one source.
use crate::agent::consent::{CONSENT_COPY_VERSION, has_current_consent};

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

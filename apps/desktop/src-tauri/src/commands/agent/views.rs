//! The wire shapes the rail sees, and the pure mappings that produce them.
//!
//! Two directions meet here: the streamed [`AskCmdrStreamEvent`] (a `Channel` enum,
//! `Serialize` only) and the specta-typed projections a query command returns
//! ([`MessageView`], [`ConversationDetailView`], [`AttachmentRef`]). Everything is a
//! projection that DROPS backend-only material — no reasoning blob, no provider state, no
//! raw tool-result content ever crosses.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::chat::context::{AttachmentKind, EnvelopeAttachment};
use crate::agent::chat::runtime::{AgentChatEvent, AgentErrorKind};
use crate::agent::llm::types::{AgentPart, AgentRole, AgentStopReason, AgentUsage};
use crate::agent::store::{self, ConversationRow, StoredMessage};

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
    /// The prompt budget pushed earlier tool results out of this turn's context, so the
    /// reply was written with less than the full thread in view. One per turn; the rail
    /// shows it as a timeline line.
    ContextTrimmed {
        elided_results: usize,
        approx_tokens: usize,
    },
    /// What this turn's prompt cost against its budget, once per answered turn, for the rail's
    /// usage gauge. Both figures are `chars/4` estimates and the UI labels them so.
    ContextUsage {
        estimated_tokens: usize,
        budget_tokens: usize,
        elided_results: usize,
    },
}

/// What the chat-memory setting needs to warn honestly: the model the next turn would use,
/// and the context window we believe it has.
///
/// The window knowledge stays in `agent::chat::budget` (one table, not two), and the COMPARISON
/// stays in the settings UI: the user's pick has to warn the moment it's chosen, and the stored
/// value the backend reads lands up to half a second later.
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModelWindowView {
    /// The model Ask Cmdr would send to right now. Empty when AI is off.
    pub model: String,
    /// That model's window in tokens: the local server's configured window, else the family
    /// table. `None` when nothing here knows it, so the UI stays quiet instead of guessing.
    pub known_window_tokens: Option<u32>,
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
    /// The local server runs with a context window too small to hold one prompt, so the send
    /// was refused before it could be assembled against
    /// (`budget::BudgetRefusal::LocalWindowBelowFloor`). View-only: the runtime never produces
    /// it, the command layer refuses ahead of the turn. The copy names the setting to change.
    LocalWindowTooSmall,
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
pub(super) fn to_wire_event(event: AgentChatEvent) -> AskCmdrStreamEvent {
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
        AgentChatEvent::ContextTrimmed {
            elided_results,
            approx_tokens,
        } => AskCmdrStreamEvent::ContextTrimmed {
            elided_results,
            approx_tokens,
        },
        AgentChatEvent::ContextUsage {
            estimated_tokens,
            budget_tokens,
            elided_results,
        } => AskCmdrStreamEvent::ContextUsage {
            estimated_tokens,
            budget_tokens,
            elided_results,
        },
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
    /// What the thread's last measured turn spent, and the budget it spent it against, so a
    /// reopened thread shows its real gauge instead of an empty one. `None` until a turn has
    /// finished: read as "not measured yet", never as zero usage.
    pub last_context_usage: Option<ContextUsageView>,
}

/// A thread's last measured context usage, on the wire. Both figures are `chars/4` estimates.
#[derive(Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContextUsageView {
    pub estimated_tokens: u32,
    pub budget_tokens: u32,
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
/// [`ask_cmdr_send_message`](super::chat::ask_cmdr_send_message), and the output of the two attachment-resolving commands.
#[derive(Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    pub path: String,
    pub kind: AttachmentKindView,
}

impl AttachmentRef {
    pub(super) fn to_envelope(&self) -> EnvelopeAttachment {
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
pub(super) fn to_message_view(message: StoredMessage) -> MessageView {
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

#[cfg(test)]
mod tests;

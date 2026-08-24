//! The one transport every Ask Cmdr turn streams over, keyed by conversation.
//!
//! A turn's progress reaches the frontend as an [`AskCmdrTurn`] event carrying the
//! conversation it belongs to plus one [`AskCmdrStreamEvent`]. Rail sends and wakes share
//! it: a wake has no `invoke` to reply into, and a rail send that outlives its webview has
//! nothing to reply into either.
//!
//! ## Why an event rather than a reply channel
//!
//! - **A reload keeps the stream.** A `Channel<T>` dies with the webview that handed it in,
//!   so a turn would finish into the database while the user watched a dead panel.
//!   Subscribing by conversation just picks the stream back up.
//! - **It stays specta-typed.** `Channel<T>` is not specta-friendly, so the command carrying
//!   one had to ride raw `invoke` and its wire enum could not appear in `bindings.ts`. Both
//!   halves of that opt-out are gone.
//! - **A wake can use it.** `agent/` sits BELOW `commands/`, so the type lives here, next to
//!   the runtime that produces it, and the command layer and the wake runner emit through the
//!   same function.
//!
//! ⚠️ **It reaches every window.** Only the main window hosts the rail, so a subscriber
//! outside it must not subscribe at all, exactly as the operation-failure watch arranges.
//!
//! ❌ **No reasoning blob and no provider state ever ride here.** The runtime's
//! [`AgentChatEvent`] already excludes them and [`to_wire_event`] is a projection, never a
//! widening.

use serde::Serialize;
use tauri_specta::Event;

use super::runtime::{AgentChatEvent, AgentErrorKind};
use crate::agent::llm::types::{AgentStopReason, AgentUsage};

const LOG_TARGET: &str = "agent::chat";

// ── The event ──────────────────────────────────────────────────────────────────

/// One turn event, and the thread it happened in.
///
/// ⚠️ `Serialize` only beside the derive: `tauri_specta::Event` wants `DeserializeOwned`
/// solely for its Rust-side `listen`, which nothing here does, and requiring it would drag
/// `Deserialize` onto the whole rename-proposal snapshot chain for no caller.
#[derive(Debug, Clone, Serialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "ask-cmdr-turn")]
pub struct AskCmdrTurn {
    /// The thread this event belongs to. A subscriber not showing this thread ignores the
    /// event; one that is applies it, whenever it started listening.
    pub conversation_id: i64,
    pub event: AskCmdrStreamEvent,
}

/// A streamed progress event for the rail. `type`-tagged camelCase, mirroring the
/// runtime's [`AgentChatEvent`] minus anything backend-only.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AskCmdrStreamEvent {
    /// First event of a turn: the thread it runs in exists and is about to be worked on. A
    /// wake emits it too, which is how the session list learns a thread was created with
    /// nobody having typed anything.
    Started,
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
    /// The thread this turn ran in is GONE: a wake looked, found nothing worth raising, and
    /// took its thread with it (`agent/wake/quiet.rs`).
    ///
    /// ⚠️ Terminal, and the one event a subscriber can't answer by re-reading the thread —
    /// there is nothing left to read. Whoever is showing that conversation drops it.
    Discarded,
}

/// The wire form of [`AgentErrorKind`] — the frontend renders each honestly.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
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

impl AgentErrorKindView {
    /// The stable snake_case token this kind reports as in analytics, shared with
    /// [`AgentErrorKind::as_token`] for every variant the two enums have in common (a test
    /// pins that). Both feed the SAME `failure` prop on `ask_cmdr_turn`, so a gate that
    /// tokenized differently either side of the turn boundary would be two numbers for one
    /// thing. `NoConsent` and `LocalWindowTooSmall` are view-only: the runtime has no variant
    /// for either, because the command layer refuses ahead of the turn.
    pub fn as_token(self) -> &'static str {
        match self {
            AgentErrorKindView::NoKey => "no_key",
            AgentErrorKindView::NotConfigured => "not_configured",
            AgentErrorKindView::NoConsent => "no_consent",
            AgentErrorKindView::LocalWindowTooSmall => "local_window_too_small",
            AgentErrorKindView::Unavailable => "unavailable",
            AgentErrorKindView::Timeout => "timeout",
            AgentErrorKindView::AuthFailed => "auth_failed",
            AgentErrorKindView::RateLimited => "rate_limited",
            AgentErrorKindView::BudgetExhausted => "budget_exhausted",
            AgentErrorKindView::UnfinishedReply => "unfinished_reply",
            AgentErrorKindView::Provider => "provider",
        }
    }
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
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
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
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
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
pub fn to_wire_event(event: AgentChatEvent) -> AskCmdrStreamEvent {
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

// ── Emitting ───────────────────────────────────────────────────────────────────

/// The app handle the emitter uses, wired once at startup like the suggestions emitter's.
/// `None` before wiring, which is every unit test, so emitting is a silent no-op there.
static TURN_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// Point the turn emitter at the app. Startup only.
pub fn init_turn_event_emitter(app: &tauri::AppHandle) {
    let _ = TURN_APP.set(app.clone());
}

/// Announce one turn event for a thread.
///
/// Fire-and-forget on purpose: nobody listening is the ordinary case (the rail is closed, or
/// the turn is a wake nobody has opened yet), and the turn persists itself either way.
pub fn emit_turn_event(conversation_id: i64, event: AskCmdrStreamEvent) {
    let Some(app) = TURN_APP.get() else {
        return;
    };
    let turn = AskCmdrTurn { conversation_id, event };
    if let Err(e) = turn.emit(app) {
        log::warn!(target: LOG_TARGET, "a turn event didn't reach the windows: {e}");
    }
}

/// Forward every runtime event a turn produces onto the transport, until the turn drops its
/// sender.
///
/// ⚠️ Shared by the rail and by a wake so the two can't drift into streaming different things:
/// one transport is only worth having if there is also one projection.
///
/// ❌ **Don't count proposals from this stream.** Only `propose_rename_plan` emits a
/// `ProposalReady` (it opens a review dialog); the whole `propose_suggestions` half streams
/// nothing, so a count taken here would read zero for most of what an agent actually stages.
/// `agent/wake/watch.rs` counts the tool CALLS instead.
pub async fn forward_to_windows(
    conversation_id: i64,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<AgentChatEvent>,
) {
    while let Some(event) = events.recv().await {
        emit_turn_event(conversation_id, to_wire_event(event));
    }
}

#[cfg(test)]
mod tests;

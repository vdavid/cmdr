//! The event seam the IPC layer subscribes to, and the typed reasons a turn can end
//! without an answer.
//!
//! Progress is emitted as typed [`AgentChatEvent`]s through a plain `UnboundedSender`
//! ([`ChatEventSink`]). Each caller makes the channel, forwards what comes out onto the
//! conversation's stream (`agent::chat::stream`), and passes the sender in. Nothing here
//! knows about Tauri IPC.

use tokio::sync::mpsc::UnboundedSender;

use crate::agent::llm::types::{AgentStopReason, AgentUsage, ToolId};
use crate::agent::tools::propose::rename::RenameProposalSnapshot;

/// The sink the runtime emits progress through. A plain unbounded channel; the IPC command forwards
/// it to a Tauri `Channel`. Send failures (a closed receiver, e.g. the rail was closed)
/// are ignored — the turn keeps running to persist its state.
pub type ChatEventSink = UnboundedSender<AgentChatEvent>;

/// One typed progress event, mirroring plan §7's stream events minus the IPC specifics.
/// The frontend gets DISPLAY parts only: no reasoning blob and no provider state ever
/// ride here.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentChatEvent {
    /// A send arrived while this thread's loop was running; it will start once the
    /// running one finishes. Drives the "working… stop?" affordance.
    Queued,
    /// The user's message was persisted (written on the first `respond` `End`).
    UserPersisted { message_id: i64, seq: i64 },
    /// A new assistant turn began streaming. Carries no id by design: `content_blocks`
    /// are written only on `End`, so no row exists yet (crash case a). The final id
    /// arrives with [`AgentChatEvent::Done`].
    AssistantStarted,
    /// A chunk of assistant text arrived.
    TextDelta { text: String },
    /// Opaque reasoning progressed; the UI shows "thinking…", content never surfaced.
    ReasoningTick,
    /// The model started a tool call (surfaced as a collapsible "looked at X" line).
    ToolCallStarted { call_id: String, tool: ToolId },
    /// A tool call finished dispatching. `ok` is false for a refusal or a handler
    /// problem (inspected from OUR OWN typed result shape, not external wording).
    ToolCallFinished { call_id: String, ok: bool },
    /// A server-owned rename proposal is ready for the review surface. The
    /// snapshot is display-only; later approval uses only opaque row ids.
    ProposalReady { proposal: RenameProposalSnapshot },
    /// The turn produced its final answer. Carries the persisted assistant message id.
    Done {
        message_id: i64,
        seq: i64,
        stop: AgentStopReason,
        usage: AgentUsage,
    },
    /// The turn ended without an answer, honestly and typed. Rendered by the frontend
    /// without the words "error"/"failed" (the frontend owns the copy). `detail` is the
    /// source error's own wording — shown under the typed headline so the user sees what
    /// to fix (a retired model slug, a quota reset time); display only, never control flow.
    Failed {
        kind: AgentErrorKind,
        detail: Option<String>,
    },
    /// The conversation's effective model changed since its previous turn; a UI-facing
    /// event row was persisted (its identity rides along). The rail shows it as a small
    /// timeline line before this turn's user message.
    ModelChanged { message_id: i64, seq: i64, model: String },
    /// The prompt budget forced earlier tool results out of this turn's context. Emitted
    /// once per turn so the user learns that the model answered with less than the full
    /// thread in view, instead of a quiet drop that reads like a normal reply.
    ContextTrimmed {
        /// How many tool results were replaced by a stub.
        elided_results: usize,
        /// Roughly how many tokens of detail that removed.
        approx_tokens: usize,
    },
    /// What this turn's prompt actually cost, against the budget it was assembled for.
    /// Emitted ONCE per answered turn, from the turn's LAST assembly (the largest one, since
    /// each tool result joins the same turn), so the gauge reports the peak the user should
    /// judge "is this chat filling up?" by, not the cheapest moment in the turn.
    ///
    /// Both numbers are `chars/4` ESTIMATES, not a tokenizer's count, and the UI says so.
    ContextUsage {
        /// The assembled prompt's estimated size.
        estimated_tokens: usize,
        /// The budget it was assembled against.
        budget_tokens: usize,
        /// How many tool results this turn's assembly set aside, so the gauge can name the
        /// "set aside" state without a second event.
        elided_results: usize,
    },
}

/// The typed reasons a turn can end without an answer. A pure classification the
/// frontend renders honestly; never a matched string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentErrorKind {
    /// No API key configured for the selected provider.
    NoKey,
    /// The provider/model slot is not configured.
    NotConfigured,
    /// The provider was unreachable.
    Unavailable,
    /// The request timed out.
    Timeout,
    /// The provider rejected the API key.
    AuthFailed,
    /// The provider is rate-limiting or out of quota.
    RateLimited,
    /// A per-message budget (tool turns or wall time) was exhausted before an answer.
    BudgetExhausted,
    /// The reply stream ended before completing (a provider drop or a crash mid-stream).
    UnfinishedReply,
    /// Any other provider-side problem; detail is logged, never carried in the type.
    Provider,
}

impl From<crate::agent::llm::types::AgentLlmError> for AgentErrorKind {
    fn from(error: crate::agent::llm::types::AgentLlmError) -> Self {
        use crate::agent::llm::types::AgentLlmError;
        match error {
            AgentLlmError::NoKey => AgentErrorKind::NoKey,
            AgentLlmError::NotConfigured => AgentErrorKind::NotConfigured,
            AgentLlmError::Unavailable => AgentErrorKind::Unavailable,
            AgentLlmError::Timeout => AgentErrorKind::Timeout,
            AgentLlmError::AuthFailed(_) => AgentErrorKind::AuthFailed,
            AgentLlmError::RateLimited(_) => AgentErrorKind::RateLimited,
            AgentLlmError::BudgetExhausted => AgentErrorKind::BudgetExhausted,
            AgentLlmError::Provider(_) => AgentErrorKind::Provider,
        }
    }
}

pub(super) fn emit(sink: &ChatEventSink, event: AgentChatEvent) {
    let _ = sink.send(event);
}

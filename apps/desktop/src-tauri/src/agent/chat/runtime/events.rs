//! The event seam the IPC layer subscribes to, and the typed reasons a turn can end
//! without an answer.
//!
//! Progress is emitted as typed [`AgentChatEvent`]s through a plain `UnboundedSender`
//! ([`ChatEventSink`]). Each caller makes the channel, forwards what comes out onto the
//! conversation's stream (`agent::chat::stream`), and passes the sender in. Nothing here
//! knows about Tauri IPC.

use tokio::sync::mpsc::UnboundedSender;

use serde::Serialize;

use crate::agent::chat::message_view::MessageView;
use crate::agent::llm::types::{AgentStopReason, AgentUsage, ToolId};
use crate::agent::store::TurnStats;
use crate::agent::tools::line::{LineRef, ToolOutcome, ToolSubject};
use crate::tools::propose::rename::RenameProposalSnapshot;

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
    /// The turn's opener was persisted (written on the first `respond` `End`), as the same view
    /// history shows, so a rail that reloaded before this row landed can still show it.
    UserPersisted { opener: MessageView },
    /// A new assistant turn began streaming. Carries no id by design: `content_blocks`
    /// are written only on `End`, so no row exists yet (crash case a). The final id
    /// arrives with [`AgentChatEvent::Done`].
    AssistantStarted,
    /// A chunk of assistant text arrived.
    TextDelta { text: String },
    /// Opaque reasoning progressed; the UI shows "thinking…", content never surfaced.
    ReasoningTick,
    /// The model started a tool call (surfaced as a collapsible "looked at X" line). Its
    /// arguments are still streaming, so it names only the tool.
    ToolCallStarted { call_id: String, tool: ToolId },
    /// A round's assistant row persisted with these tool calls, before any of them runs: EVERY
    /// call of the final message, in order, with what each looks at. It stamps the row's id onto
    /// the round, adds a call whose `ToolCallStarted` never came, and drops a started call the
    /// final message doesn't hold (a nameless call the provider seam dropped).
    ToolCallsResolved { message_id: i64, calls: Vec<ResolvedCall> },
    /// A tool call's result row persisted (always BEFORE this is emitted, so fetching the result
    /// never races the row). `subject` is refined by the result where the arguments couldn't say
    /// (`list_pane_files`). The line is `(message_id, call_id)`: ids collide across rounds.
    ToolCallFinished {
        message_id: i64,
        call_id: String,
        outcome: ToolOutcome,
        subject: ToolSubject,
    },
    /// A server-owned rename proposal is ready for the review surface. The
    /// snapshot is display-only; later approval uses only opaque row ids.
    ProposalReady { proposal: RenameProposalSnapshot },
    /// The turn produced its final answer. Carries the persisted assistant message id, the LAST
    /// call's usage, and what the whole turn spent when it ran a tool.
    Done {
        message_id: i64,
        seq: i64,
        stop: AgentStopReason,
        usage: AgentUsage,
        turn: Option<TurnStats>,
    },
    /// The turn ended without an answer, honestly and typed. Rendered by the frontend
    /// without the words "error"/"failed" (the frontend owns the copy). `detail` is the
    /// source error's own wording — shown under the typed headline so the user sees what
    /// to fix (a retired model slug, a quota reset time); display only, never control flow.
    /// `turn` is what the turn spent when it ran a tool.
    Failed {
        kind: AgentErrorKind,
        detail: Option<String>,
        turn: Option<TurnStats>,
    },
    /// The user (or the status corner) stopped the turn. Terminal, like `Done` and `Failed`,
    /// and the only thing a cancel emits; `turn` is what it spent when it ran a tool.
    Stopped { turn: Option<TurnStats> },
    /// The conversation's effective model changed since its previous turn; a UI-facing
    /// event row was persisted (its identity rides along). The rail shows it as a small
    /// timeline line before this turn's user message.
    ModelChanged { message_id: i64, seq: i64, model: String },
    /// The conversation's chat-memory size (its resolved prompt budget) changed since its
    /// previous turn; a UI-facing event row was persisted (its identity rides along). The
    /// rail shows it as a small timeline line before this turn's user message.
    ///
    /// Distinct from [`Self::ContextTrimmed`], which reports what ONE assembly had to leave
    /// out. This one reports that the ceiling itself moved, and it outlives the turn.
    ChatMemoryChanged {
        message_id: i64,
        seq: i64,
        chat_memory_tokens: usize,
    },
    /// The prompt budget forced earlier tool results out of this turn's context. Emitted
    /// once per turn so the user learns that the model answered with less than the full
    /// thread in view, instead of a quiet drop that reads like a normal reply.
    ContextTrimmed {
        /// How many tool results were replaced by a stub.
        elided_results: usize,
        /// Roughly how many tokens of detail that removed.
        approx_tokens: usize,
    },
    /// What one model call's prompt cost, against the budget it was assembled for. Emitted
    /// once per COMPLETED call (its `End`), so the gauge moves as each tool result joins the
    /// turn; the turn's last report is its largest assembly, the peak "is this chat filling
    /// up?" is judged by. A call that never reached `End` reports nothing.
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
    /// One completed model call was just folded into the cost meter. Carries no numbers on
    /// purpose: the meter stays the one source of the thread's total, so the rail re-reads it
    /// (`ask_cmdr_conversation_cost`) rather than adding up figures of its own.
    CostMetered,
    /// The turn's estimated spend passed the user's threshold, so it stopped between rounds and
    /// waits for Keep going or Stop (`chat::spend_pause`). `cost_micros` is what the turn has
    /// spent so far, by the same estimate as the cost footer.
    SpendPaused { cost_micros: i64 },
    /// The user pressed Keep going on the spend pause, and the turn runs on.
    SpendResumed,
}

/// One call of a round, as [`AgentChatEvent::ToolCallsResolved`] lists it: which tool, what it
/// looks at (from its arguments alone), and the earlier identical call of this turn it repeats, if
/// any. The tool's wire name rides along so a line whose `ToolCallStarted` never came is still
/// added with its label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCall {
    pub call_id: String,
    pub tool: String,
    pub subject: ToolSubject,
    pub repeat_of: Option<LineRef>,
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
    /// A per-answer limit (a background round cap, or the loop guard) ended the tool loop before an answer.
    BudgetExhausted,
    /// The model kept re-sending a tool call that had already come back with a problem, after
    /// being told plainly that repeating it changes nothing (`repeats.rs`). Distinct from
    /// [`BudgetExhausted`](Self::BudgetExhausted) because the user deserves a truer sentence
    /// than "it hit its limit": nothing was running out, the turn was going in a circle.
    RepeatedToolCall,
    /// The reply stream ended before completing (a provider drop or a crash mid-stream).
    UnfinishedReply,
    /// Any other provider-side problem; detail is logged, never carried in the type.
    Provider,
    /// The organization's managed policy refused a request mid-turn (the LLM client's backstop).
    /// The same wire kind the slot refuses with before a turn; not a failure.
    ManagedByOrganization(crate::managed_policy::ManagedAiRefusal),
}

impl AgentErrorKind {
    /// The stable snake_case token this kind reports as in analytics. Separate from the
    /// serde wire form the frontend renders, so a copy or wire change can't silently
    /// rename a metric that a saved analytics query groups by.
    pub fn as_token(self) -> &'static str {
        match self {
            AgentErrorKind::NoKey => "no_key",
            AgentErrorKind::NotConfigured => "not_configured",
            AgentErrorKind::Unavailable => "unavailable",
            AgentErrorKind::Timeout => "timeout",
            AgentErrorKind::AuthFailed => "auth_failed",
            AgentErrorKind::RateLimited => "rate_limited",
            AgentErrorKind::BudgetExhausted => "budget_exhausted",
            AgentErrorKind::RepeatedToolCall => "repeated_tool_call",
            AgentErrorKind::UnfinishedReply => "unfinished_reply",
            AgentErrorKind::Provider => "provider",
            AgentErrorKind::ManagedByOrganization(_) => "managed_by_organization",
        }
    }

    /// The organization's reason, for `ManagedByOrganization`: the wire kind is a unit variant,
    /// so the reason rides beside it.
    pub fn managed(self) -> Option<crate::managed_policy::ManagedAiRefusal> {
        match self {
            AgentErrorKind::ManagedByOrganization(refusal) => Some(refusal),
            _ => None,
        }
    }
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
            AgentLlmError::Managed(refusal) => AgentErrorKind::ManagedByOrganization(refusal),
        }
    }
}

pub(super) fn emit(sink: &ChatEventSink, event: AgentChatEvent) {
    let _ = sink.send(event);
}

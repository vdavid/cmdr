//! What one turn IS, as data: the opener, the params, the outcome, and the tally.
//!
//! A leaf on purpose. `turn.rs` drives the loop, and `cost.rs` and `analytics.rs` each
//! report on a turn afterwards; all three need this vocabulary, and only `turn` implements
//! the driver. Keeping the two in one file made every reporter import the driver's module
//! and the driver import theirs, which is a circle for no reason: nothing here calls
//! anything. ❌ Don't move a function that DRIVES a turn in here — this module stays free of
//! `super` imports, and that is what keeps the arrows pointing one way.

use chrono::FixedOffset;

use super::events::AgentErrorKind;
use crate::agent::chat::context::ContextEnvelope;
use crate::agent::llm::types::{AgentPart, ProviderTag, WakeDigest};
use crate::agent::types::ProposalOutcomes;

/// What opens a turn: what the person typed, or what a wake noticed.
///
/// ⚠️ **A wake's opener is DATA, not prose.** It is persisted as the thread's user-role
/// message, so a rendered English sentence would freeze one locale's copy in `main.db`
/// where no later locale pass can reach it. The rail localizes the numbers instead, and
/// the provider gets [`WakeDigest::render`] on its way out.
#[derive(Debug, Clone, Copy)]
pub enum UserTurn<'a> {
    /// What the person typed into the composer.
    Text(&'a str),
    /// What a wake found waiting for it.
    Wake(&'a WakeDigest),
    /// What the user did with a sweep the agent proposed. Opens the follow-up turn a rejection
    /// asks for, and carries the same "data, never prose" contract as a digest.
    Outcomes(&'a ProposalOutcomes),
}

impl UserTurn<'_> {
    /// The part this opener is persisted and replayed as.
    pub(super) fn part(&self) -> AgentPart {
        match self {
            UserTurn::Text(text) => AgentPart::Text((*text).to_string()),
            UserTurn::Wake(digest) => AgentPart::WakeDigest((*digest).clone()),
            UserTurn::Outcomes(outcomes) => AgentPart::ProposalOutcomes((*outcomes).clone()),
        }
    }

    /// The FTS text for the row. A wake indexes the PATHS it named: they are the user's own
    /// data, and they are what somebody searching their threads would type.
    pub(super) fn search_text(&self) -> String {
        match self {
            UserTurn::Text(text) => (*text).to_string(),
            UserTurn::Wake(digest) => digest.paths().join(" "),
            UserTurn::Outcomes(outcomes) => outcomes.paths().join(" "),
        }
    }
}

/// Everything one turn needs beyond the seams. `user` is `Some` for a new user
/// message (appended + persisted on the first `End`) and `None` to RESUME a persisted
/// thread after a crash (fresh `respond` from the persisted transcript — crash case c).
pub struct TurnParams<'a> {
    pub conversation_id: i64,
    pub user: Option<UserTurn<'a>>,
    pub cmdr_md: Option<&'a str>,
    /// What the agent wrote about the user, already cut to this turn's share of the budget
    /// (`agent::memory`). A separate field from `cmdr_md` on purpose — see
    /// [`crate::agent::chat::context::PrefixInputs`].
    pub memory: Option<&'a str>,
    pub envelope: &'a ContextEnvelope,
    pub offset: FixedOffset,
    /// Wall-clock secs stamped on rows written this turn; also the envelope's clock.
    pub now_secs: i64,
    /// The resolved interactive-slot provider + model, for cost metering. Real slot
    /// resolution happens in the command layer; the runtime just records what it was told.
    pub provider: ProviderTag,
    pub model: String,
    /// The resolved model's assembled-prompt token budget (`super::budget`). Resolved in the
    /// command layer alongside the model, because a local server's window is a user setting.
    pub prompt_budget: usize,
}

/// How a turn ended, for the caller's bookkeeping. The events already told the
/// frontend everything; this is for logging and the single-flight wrapper.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnResult {
    /// A final answer was produced and persisted.
    Answered { assistant_message_id: i64 },
    /// The turn stopped without an answer, for this typed reason.
    Failed(AgentErrorKind),
    /// The user cancelled at a tool boundary; nothing further was attempted.
    Cancelled,
}

/// What one turn did, counted as it runs so every early return reports the numbers it
/// actually reached rather than zeros.
#[derive(Debug, Default, Clone, Copy)]
pub struct TurnTally {
    /// How many times the loop went back to the provider carrying tool results.
    pub tool_turns: usize,
    /// How many tool calls staged a proposal for the user to review. The funnel's join to
    /// `suggestion_group_proposed`: turns with proposals but no proposal events downstream
    /// would be a real instrumentation bug, and nothing else could tell us.
    pub proposals: usize,
}

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
use crate::agent::llm::types::{AgentPart, CallSettings, MemoryNote, ProviderTag, WakeDigest};
use crate::agent::store::{TurnLimit, TurnStats};
use crate::agent::types::{ProposalOutcomes, TurnOrigin};

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
    /// Who started the turn, stated by the caller rather than read off `user`'s shape: the
    /// dispatch gate reads it per call (`agent::tools::view`).
    pub origin: TurnOrigin,
    pub user: Option<UserTurn<'a>>,
    pub cmdr_md: Option<&'a str>,
    /// What the agent wrote about the user, one entry per memory file, already cut to this
    /// turn's share of the budget (`agent::memory::read_for_turn`). Read at send; the files
    /// that changed since the thread last carried them ride the new user turn
    /// (`context::memory_notes_for_turn`), never the cached prefix.
    pub memory: &'a [MemoryNote],
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
    /// The chat memory the slot-change timeline names (`budget::chat_memory_figure`): the budget
    /// less what an effort or a late provider report moved, so "Chat memory changed" lines mean
    /// the user changed something.
    pub chat_memory_tokens: usize,
    /// How hard the model thinks on every call this turn makes, and the output ceiling that
    /// thinking shares with the reply (`chat::effort`). Rides each call rather than the slot so
    /// the picker can change it between turns without rebuilding the LLM.
    pub call: CallSettings,
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
    /// How many tool calls got a result row, refusals and repeats included (a round can hold
    /// several, so this is not `tool_turns`).
    pub tool_calls: u32,
    /// Summed across the turn's persisted assistant rows: the same per-row figures the cost
    /// meter sums.
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// How many tool calls staged a proposal for the user to review. The funnel's join to
    /// `suggestion_group_proposed`: turns with proposals but no proposal events downstream
    /// would be a real instrumentation bug, and nothing else could tell us.
    pub proposals: usize,
    /// The per-answer ceiling the turn reached, if any. It rides the stats, so the rail names
    /// the limit and its value live and after a reload.
    pub limit: Option<TurnLimit>,
}

impl TurnTally {
    /// What the turn spent, for its footer and its `TurnFinished` row: `None` for a turn that ran
    /// no tool, so a tool-less turn looks the same live and reloaded. `stopped` rides along so the
    /// footer can say a stopped turn was stopped, live and after a reload alike.
    pub fn stats(&self, wall_ms: u64, stopped: bool) -> Option<TurnStats> {
        (self.tool_calls > 0).then_some(TurnStats {
            tool_calls: self.tool_calls,
            wall_ms: Some(wall_ms),
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            stopped,
            limit: self.limit,
        })
    }
}

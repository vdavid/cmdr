//! What a thread's timeline records beside its transcript.
//!
//! ⚠️ **These NEVER enter the LLM transcript.** They are `role = 'event'` message rows: they
//! share the conversation's `seq` so they interleave correctly in the history view, and the
//! transcript loader can't feed one to a provider because the token lives outside `AgentRole`.
//!
//! That is also their limit, and it is the reason M4 needs a second channel. An outcome
//! recorded only here teaches the agent nothing: what it learns from is the memory ring
//! (`../memory/outcomes.rs`) and, for a rejection, the follow-up turn. This half is for the
//! user's eyes.

use rusqlite::Connection;

use super::AgentStoreError;
use super::rows::insert_message_row;
use crate::agent::types::ProposalDecision;

/// A UI-facing event recorded in a conversation's timeline (a `role = 'event'` message
/// row). Events share the conversation's `seq` ordering but NEVER enter the LLM
/// transcript — they exist for the user's eyes (and the history view) only.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ConversationEvent {
    /// The conversation's effective model changed between turns; `model` is the new name.
    ModelChanged { model: String },
    /// How much of the conversation each message carries changed between turns;
    /// `chat_memory_tokens` is the newly resolved prompt-token budget
    /// (`agent::chat::budget`). A number, never a sentence: the rail says it in the user's
    /// own language, so no English is frozen in `main.db`.
    ChatMemoryChanged { chat_memory_tokens: usize },
    /// The user answered a proposal this thread produced.
    ///
    /// ⚠️ **Numbers and the group's own display text, never an authored sentence.** This row
    /// outlives every locale pass, exactly as a wake's digest does, so the rail says it in the
    /// user's language and nothing English is frozen in `main.db`.
    ProposalDecided { decision: ProposalDecision },
    /// A turn that ran at least one tool ended (answered, failed, or stopped), and this is what
    /// it spent. Written after the turn's last row, so its wall time survives a reload: every
    /// row of a turn carries the same clock, so the time can't be derived from them.
    TurnFinished { stats: TurnStats },
    /// A row whose `kind` this build doesn't know: a newer build wrote it, and this one runs on
    /// the same `main.db` after a downgrade. Read-only. It parses so the thread still loads AND
    /// sends (the history page and the transcript loader both read every row), and the row is
    /// kept rather than dropped because paging counts rows. ❌ Never written: [`append_event`]
    /// refuses it. Only an unknown `kind` lands here; a known kind with a broken body is still
    /// a decode error.
    #[serde(other)]
    Unknown,
}

/// What one turn spent, summed across its rounds: the turn line's footer. Numbers only, so the
/// rail words it in the user's language and nothing English is frozen in `main.db`.
///
/// Tokens are each assistant row's own `prompt_tokens` / `completion_tokens`, the same figures
/// the cost meter sums, so a thread's per-turn footers add up to its cost footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnStats {
    /// Tool calls that came back with a result row, refusals and repeats included.
    pub tool_calls: u32,
    /// Wall time from the turn's start to its end. `None` only for a turn recorded before this
    /// was measured, whose stats are rebuilt from its rows.
    pub wall_ms: Option<u64>,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// The user (or the progress watchdog) stopped the turn, so the footer says so after a reload
    /// too. `false` for a turn recorded before this was stored: its rows can't tell a stop apart.
    #[serde(default)]
    pub stopped: bool,
    /// The turn's own ceiling that ended its tool loop, so the rail can name it and its value,
    /// live and after a reload. `None` for a turn no ceiling touched, and for one recorded
    /// before this was stored.
    #[serde(default)]
    pub limit: Option<TurnLimit>,
}

/// Which per-answer ceiling a turn reached (`chat::context::MAX_TOOL_TURNS` /
/// `MAX_WALL_TIME`). Both are fixed in Cmdr: no setting moves them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TurnLimitKind {
    /// The rounds of tool calls one answer may run.
    ToolRounds,
    /// The wall-clock time one answer may take.
    WallTime,
}

/// A ceiling a turn reached, with the value that applied to it. The value is stored rather
/// than read from the constant at render time, so a reloaded thread names the limit that
/// stopped THAT turn even after a build moves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TurnLimit {
    pub kind: TurnLimitKind,
    /// Rounds for [`TurnLimitKind::ToolRounds`], seconds for [`TurnLimitKind::WallTime`].
    pub max: u32,
    /// The model answered with what it had: past the round cap the turn makes ONE more call
    /// with no tool allowed to run. `false` when that call still reached for a tool, and
    /// always for wall time, which makes no further call.
    pub answered: bool,
}

/// The `messages.role` token for event rows. Kept OUTSIDE `AgentRole` on purpose: an
/// event is not an LLM transcript role, so the seam's enum can't represent one and the
/// transcript loader can't accidentally feed one to a provider.
pub(super) const EVENT_ROLE_TOKEN: &str = "event";

/// Append a UI-facing event row (see [`ConversationEvent`]) to a conversation, returning
/// `(message_id, seq)`. Events interleave with messages via the shared per-conversation
/// `seq`, carry no searchable text, and never enter the LLM transcript.
pub fn append_event(
    conn: &Connection,
    conversation_id: i64,
    event: &ConversationEvent,
    now: i64,
) -> Result<(i64, i64), AgentStoreError> {
    debug_assert!(
        !matches!(event, ConversationEvent::Unknown),
        "an Unknown event is read-only: it stands for a kind a newer build wrote"
    );
    let content_blocks = serde_json::to_string(event).map_err(AgentStoreError::ContentBlocks)?;
    insert_message_row(
        conn,
        conversation_id,
        EVENT_ROLE_TOKEN,
        &content_blocks,
        "",
        None,
        None,
        now,
    )
}

/// Append an event row with arbitrary `content_blocks`, the way a newer build writes a kind
/// this one doesn't know. Tests only: production writes go through [`append_event`].
#[cfg(test)]
pub fn append_raw_event_row(
    conn: &Connection,
    conversation_id: i64,
    content_blocks: &str,
    now: i64,
) -> Result<(i64, i64), AgentStoreError> {
    insert_message_row(
        conn,
        conversation_id,
        EVENT_ROLE_TOKEN,
        content_blocks,
        "",
        None,
        None,
        now,
    )
}

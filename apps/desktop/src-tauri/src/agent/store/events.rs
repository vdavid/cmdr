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
    /// The user answered a proposal this thread produced.
    ///
    /// ⚠️ **Numbers and the group's own display text, never an authored sentence.** This row
    /// outlives every locale pass, exactly as a wake's digest does, so the rail says it in the
    /// user's language and nothing English is frozen in `main.db`.
    ProposalDecided { decision: ProposalDecision },
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

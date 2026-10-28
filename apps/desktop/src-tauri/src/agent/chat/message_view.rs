//! The rail's display projection of one stored row ([`MessageView`]), and the pure mapping that
//! builds it. Everything here DROPS backend-only material: no reasoning blob, no provider state,
//! and no raw tool-result content on a page or the stream. That content has ONE door,
//! [`thread::ToolResultDetailView`], fetched for one line only when the user expands it. A history
//! page projects through [`thread`], which reads the rows around each one (its turn) to say what
//! every tool line did and what each turn spent.
//!
//! Below `commands/` because the live stream carries a view too: a persisted user row rides
//! `UserPersisted` as a [`MessageView`], so a rail that reloaded before the row landed can show
//! it, and a wake (which may not import upward) streams through the same transport.

pub mod thread;

use serde::Serialize;

use crate::agent::llm::types::{AgentPart, AgentRole};
use crate::agent::store::{self, StoredMessage, TurnStats};
use crate::agent::tools::line::{LineRef, ToolOutcome, ToolSubject, subject_of};
use crate::agent::types::ProposalDecision;

/// A message's role, on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
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
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MessageBlock {
    /// Assistant or user prose (rendered markdown-lite, entity-escaped first).
    Text { text: String },
    /// A tool the model invoked, as its line says it: what it looked at, what came back (`None`
    /// while no result row exists: still running, or a turn that never finished), and the
    /// turn's earlier identical call it repeats. Typed facts from `tools::line`, never the raw
    /// arguments or result; the line is `(this row's id, call_id)`. Its result rides this block,
    /// so a tool-role row projects to no blocks.
    ToolCall {
        call_id: String,
        tool: String,
        subject: ToolSubject,
        outcome: Option<ToolOutcome>,
        repeat_of: Option<LineRef>,
    },
    /// The conversation's effective model changed between turns; `model` is the new name.
    /// Rendered as a small centered timeline line, escaped plain text (never `{@html}`).
    ModelChanged { model: String },
    /// How much of the conversation each message carries changed between turns, so the
    /// replies after this line saw a different amount of the chat. Rendered as a small
    /// centered timeline line beside the model one.
    ///
    /// ⚠️ **A number, never a sentence.** The row outlives every locale pass, so the rail
    /// formats the count in the user's own language and nothing English is frozen in
    /// `main.db`.
    ChatMemoryChanged { chat_memory_tokens: usize },
    /// What a wake noticed, which is the first message of every thread the agent opened for
    /// itself.
    ///
    /// ⚠️ **Numbers and paths, never a sentence.** The digest the model reads is rendered
    /// English (`agent/wake/compact.rs`) and it is persisted for as long as the thread lives,
    /// so shipping it as prose would freeze one locale's copy in `main.db` where no later
    /// locale pass could reach it. The rail says these counts in the user's own language and
    /// renders them collapsed.
    WakeDigest {
        folders: Vec<WakeDigestFolderView>,
        rollups: Vec<WakeDigestRollupView>,
    },
    /// What the user did with the proposals this thread made: one entry per answered group.
    ///
    /// Two rows reach the rail as this block, and deliberately as the SAME one. An `event`-role
    /// row carries a single decision as it happens, for the user's eyes; a `user`-role row
    /// carries a whole sweep's worth, because it is what opened the follow-up turn the agent
    /// learns from. One shape, one renderer, one set of strings.
    ///
    /// ⚠️ **Verbs, counts, and the group's own display text, never a sentence.** The English
    /// the model reads (`ProposalDecision::render`) is a prompt and stays one.
    ProposalDecisions { decisions: Vec<ProposalDecision> },
}

/// One folder a wake's digest named outright, and what happened in it. Every count is a
/// number: the rail owns every word around them.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WakeDigestFolderView {
    /// An absolute path, rendered as escaped plain text (never `{@html}`).
    pub folder: String,
    pub created: u32,
    pub modified: u32,
    pub removed: u32,
    pub renamed: u32,
}

/// The folders a wake's digest did not have room to name, summarized under a shared
/// ancestor. Present so the collapsed block can admit how much it is not showing.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WakeDigestRollupView {
    /// An absolute path, rendered as escaped plain text (never `{@html}`).
    pub ancestor: String,
    pub folders: u32,
    pub changes: u64,
}

/// A message as the rail displays it: id/seq/role, its display blocks, and token counts, plus
/// where it sits among the thread's turns. Turn edges come from here, never from a frontend
/// guess.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MessageView {
    pub id: i64,
    pub seq: i64,
    pub role: MessageRoleView,
    pub blocks: Vec<MessageBlock>,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub created_at: i64,
    /// The id of the user-role row that opened this row's turn (a user row's own id). `None` for
    /// rows before the thread's first user row.
    pub turn_id: Option<i64>,
    /// An event row recorded while its turn still ran: it sits before the turn's last assistant
    /// row. Always `false` for a transcript row.
    pub during_turn: bool,
    /// What the turn spent, on the turn's LAST assistant view only. `None` elsewhere, for a turn
    /// that ran no tool, and for the turn still running (its stats ride the live terminal event).
    pub turn_stats: Option<TurnStats>,
}

/// What a row's projection needs beyond the row itself: its place among the turns, and one
/// line's facts per tool call, in the row's call order.
#[derive(Debug, Default)]
struct RowFacts {
    turn_id: Option<i64>,
    during_turn: bool,
    turn_stats: Option<TurnStats>,
    calls: Vec<CallFacts>,
}

#[derive(Debug)]
struct CallFacts {
    subject: ToolSubject,
    outcome: Option<ToolOutcome>,
    repeat_of: Option<LineRef>,
}

/// Project one stored row on its own, with no thread around it: a user row is its own turn's
/// opener, and a tool call says only what its arguments do (no outcome, no repeat mark, a memory
/// path as plain text). For the live stream's opener; a history page goes through
/// [`thread::project_page`], which knows the turn.
pub fn to_message_view(message: StoredMessage) -> MessageView {
    let mut facts = RowFacts::default();
    if let store::StoredContent::Message { role, parts } = &message.content {
        if *role == AgentRole::User {
            facts.turn_id = Some(message.id);
        }
        facts.calls = parts
            .iter()
            .filter_map(|part| match part {
                AgentPart::ToolCall(call) => Some(CallFacts {
                    subject: subject_of(call, None, None),
                    outcome: None,
                    repeat_of: None,
                }),
                _ => None,
            })
            .collect();
    }
    project_row(message, facts)
}

/// Project one stored message into its display form, dropping reasoning parts and tool results
/// (their facts ride the calls). Event rows project to `role: Event` with their single typed
/// block.
fn project_row(message: StoredMessage, facts: RowFacts) -> MessageView {
    let mut calls = facts.calls.into_iter();
    let (role, blocks): (MessageRoleView, Vec<MessageBlock>) = match message.content {
        store::StoredContent::Message { role, parts } => {
            let blocks = parts
                .into_iter()
                .filter_map(|part| match part {
                    AgentPart::Text(text) => Some(MessageBlock::Text { text }),
                    AgentPart::ToolCall(call) => {
                        let facts = calls.next().unwrap_or_else(|| CallFacts {
                            subject: subject_of(&call, None, None),
                            outcome: None,
                            repeat_of: None,
                        });
                        Some(MessageBlock::ToolCall {
                            call_id: call.call_id,
                            tool: call.tool.as_wire_name().to_string(),
                            subject: facts.subject,
                            outcome: facts.outcome,
                            repeat_of: facts.repeat_of,
                        })
                    }
                    // The envelope is the model's context, not something the user wrote.
                    AgentPart::ToolResult(_)
                    | AgentPart::Reasoning(_)
                    | AgentPart::Envelope(_)
                    | AgentPart::MemoryNotes(_) => None,
                    AgentPart::WakeDigest(digest) => Some(MessageBlock::WakeDigest {
                        folders: digest
                            .folders
                            .into_iter()
                            .map(|folder| WakeDigestFolderView {
                                folder: folder.folder,
                                created: folder.created,
                                modified: folder.modified,
                                removed: folder.removed,
                                renamed: folder.renamed,
                            })
                            .collect(),
                        rollups: digest
                            .rollups
                            .into_iter()
                            .map(|rollup| WakeDigestRollupView {
                                ancestor: rollup.ancestor,
                                folders: rollup.folders,
                                changes: rollup.changes,
                            })
                            .collect(),
                    }),
                    AgentPart::ProposalOutcomes(outcomes) => Some(MessageBlock::ProposalDecisions {
                        decisions: outcomes.decisions,
                    }),
                })
                .collect();
            (role.into(), blocks)
        }
        store::StoredContent::Event(store::ConversationEvent::ModelChanged { model }) => {
            (MessageRoleView::Event, vec![MessageBlock::ModelChanged { model }])
        }
        store::StoredContent::Event(store::ConversationEvent::ChatMemoryChanged { chat_memory_tokens }) => (
            MessageRoleView::Event,
            vec![MessageBlock::ChatMemoryChanged { chat_memory_tokens }],
        ),
        // One decision, in the same block the follow-up turn's opener uses: the rail draws one
        // line per decision either way, and a second shape would be a second renderer for the
        // same sentence.
        store::StoredContent::Event(store::ConversationEvent::ProposalDecided { decision }) => (
            MessageRoleView::Event,
            vec![MessageBlock::ProposalDecisions {
                decisions: vec![decision],
            }],
        ),
        // A turn's stats ride its last assistant view, not a line of their own; and a kind a
        // newer build wrote has nothing to draw. Both stay an empty event view, so the row's seq
        // still holds its place in the timeline.
        store::StoredContent::Event(
            store::ConversationEvent::TurnFinished { .. } | store::ConversationEvent::Unknown,
        ) => (MessageRoleView::Event, Vec::new()),
    };
    MessageView {
        id: message.id,
        seq: message.seq,
        role,
        blocks,
        prompt_tokens: message.prompt_tokens,
        completion_tokens: message.completion_tokens,
        created_at: message.created_at,
        turn_id: facts.turn_id,
        during_turn: facts.during_turn,
        turn_stats: facts.turn_stats,
    }
}

#[cfg(test)]
mod tests;

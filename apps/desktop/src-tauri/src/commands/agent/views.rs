//! The specta-typed projections a query command returns ([`MessageView`],
//! [`ConversationDetailView`], [`AttachmentRef`]) and the pure mappings that produce them.
//! Everything here DROPS backend-only material — no reasoning blob, no provider state, no
//! raw tool-result content ever crosses.
//!
//! A turn's live progress travels the other way and does not live here: it is one
//! conversation-keyed event, `agent::chat::stream`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::chat::context::{AttachmentKind, EnvelopeAttachment};
use crate::agent::chat::stream::AgentErrorKindView;
use crate::agent::llm::types::{AgentPart, AgentRole};
use crate::agent::store::{self, ConversationRow, StoredMessage};
use crate::agent::types::ProposalDecision;

/// Why a send never started.
///
/// Returned rather than streamed, because every one of these is decided before a turn
/// exists and some before a conversation id does, leaving no thread to key an event on. It
/// carries the same typed kinds a mid-turn failure does, so the rail renders one set of
/// honest copy either way.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskCmdrSendRefusal {
    pub kind: AgentErrorKindView,
    /// The source problem's own wording, when there is one worth showing (a store that
    /// wouldn't open). Display only: the frontend branches on `kind`, never on this.
    pub detail: Option<String>,
}

impl AskCmdrSendRefusal {
    /// A refusal the kind says everything about.
    pub(super) fn of(kind: AgentErrorKindView) -> Self {
        Self { kind, detail: None }
    }
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
#[derive(Clone, Serialize, specta::Type)]
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
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WakeDigestRollupView {
    /// An absolute path, rendered as escaped plain text (never `{@html}`).
    pub ancestor: String,
    pub folders: u32,
    pub changes: u64,
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
        // One decision, in the same block the follow-up turn's opener uses: the rail draws one
        // line per decision either way, and a second shape would be a second renderer for the
        // same sentence.
        store::StoredContent::Event(store::ConversationEvent::ProposalDecided { decision }) => (
            MessageRoleView::Event,
            vec![MessageBlock::ProposalDecisions {
                decisions: vec![decision],
            }],
        ),
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

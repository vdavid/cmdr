//! Tests for the display projection: what the rail is allowed to see, and what must never
//! leave the backend. Pure, so they run with no Tauri app and no DB.

use serde_json::{Value, json};

use super::*;
use crate::agent::line::LinePath;
use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ProviderTag, ReasoningState, ToolId};
use crate::agent::store::ConversationEvent;

fn stored(content: store::StoredContent) -> StoredMessage {
    StoredMessage {
        id: 7,
        seq: 3,
        content,
        text_for_search: String::new(),
        prompt_tokens: Some(120),
        completion_tokens: Some(45),
        created_at: 1_780_000_000,
    }
}

fn message(parts: Vec<AgentPart>) -> StoredMessage {
    stored(store::StoredContent::Message {
        role: AgentRole::Assistant,
        parts,
    })
}

// ── The reasoning blob must never cross ────────────────────────────────────────

/// The store's `content_blocks` invariant, enforced at the one place it can leak: the
/// opaque provider reasoning blob is persisted and replayed, never shown. A projection
/// that carried it would hand the frontend provider state.
#[test]
fn a_reasoning_part_is_dropped_and_leaves_no_block_behind() {
    let view = to_message_view(message(vec![
        AgentPart::Text("Here you go.".into()),
        AgentPart::Reasoning(ReasoningState {
            provider: ProviderTag::Anthropic,
            blob: json!({ "signature": "secret-provider-state" }),
        }),
    ]));

    assert_eq!(view.blocks.len(), 1, "only the prose survives");
    let wire = serde_json::to_string(&view).expect("serializes");
    assert!(
        !wire.contains("secret-provider-state") && !wire.contains("reasoning"),
        "no reasoning state may reach the wire: {wire}"
    );
}

/// A tool result carries no block at all: its facts ride the call it answers, and the raw
/// content stays backend-only. The result blob is exactly what a read tool returned (paths, OCR
/// text), so shipping it would widen what the rail sees far past the line it renders.
#[test]
fn a_tool_result_part_projects_to_no_block_and_never_its_content() {
    let view = to_message_view(stored(store::StoredContent::Message {
        role: AgentRole::Tool,
        parts: vec![AgentPart::ToolResult(AgentToolResult {
            call_id: "call-1".into(),
            content: json!({ "files": ["/private/receipt.png"] }),
            elided: false,
        })],
    }));

    assert!(view.blocks.is_empty());
    let wire = serde_json::to_string(&view).expect("serializes");
    assert!(!wire.contains("receipt.png"), "the result content stays backend-only");
}

// ── Tool calls and event rows ──────────────────────────────────────────────────

/// A lone row knows no result, so its call says only what its arguments do: the wire name
/// (never the Rust variant name) and a typed subject, ❌ never the raw arguments.
#[test]
fn a_lone_tool_call_carries_its_wire_name_and_argument_subject_but_no_outcome() {
    let view = to_message_view(message(vec![AgentPart::ToolCall(AgentToolCall {
        call_id: "call-2".into(),
        tool: ToolId::ListDir,
        arguments: json!({ "path": "/photos" }),
        reasoning: None,
    })]));

    assert_eq!(
        view.blocks[0],
        MessageBlock::ToolCall {
            call_id: "call-2".into(),
            tool: ToolId::ListDir.as_wire_name().to_string(),
            subject: ToolSubject::Folder {
                path: LinePath::of("/photos")
            },
            outcome: None,
            repeat_of: None,
        }
    );
}

/// A user row opens its own turn, so the live stream's opener view already knows its turn.
#[test]
fn a_lone_user_row_is_its_own_turns_opener() {
    let view = to_message_view(stored(store::StoredContent::Message {
        role: AgentRole::User,
        parts: vec![AgentPart::Text("hi".into())],
    }));

    assert_eq!(view.turn_id, Some(7));
    assert!(!view.during_turn);
    assert_eq!(view.turn_stats, None);
}

/// An event row is a UI-facing timeline entry, not transcript content, so it projects to
/// its own role with one typed block rather than looking like an assistant message.
#[test]
fn an_event_row_projects_to_the_event_role_with_its_typed_block() {
    let view = to_message_view(stored(store::StoredContent::Event(ConversationEvent::ModelChanged {
        model: "claude-sonnet-5".into(),
    })));

    let wire = serde_json::to_value(&view).expect("serializes");
    assert_eq!(wire["role"], "event");
    assert_eq!(wire["blocks"][0]["type"], "modelChanged");
    assert_eq!(wire["blocks"][0]["model"], "claude-sonnet-5");
}

/// Identity and token counts ride along untouched: the rail keys rows by id/seq and shows
/// the per-turn token counts under the message.
#[test]
fn identity_seq_and_token_counts_survive_the_projection() {
    let view = to_message_view(message(vec![AgentPart::Text("hi".into())]));

    assert_eq!(view.id, 7);
    assert_eq!(view.seq, 3);
    assert_eq!(view.prompt_tokens, Some(120));
    assert_eq!(view.completion_tokens, Some(45));
    assert_eq!(view.created_at, 1_780_000_000);
}

/// A kind a newer build wrote projects to an event view with no blocks, keeping its id and
/// seq so the timeline (and paging) stay whole after a downgrade.
#[test]
fn an_unknown_event_row_projects_to_an_empty_event_view_at_its_seq() {
    let view = to_message_view(stored(store::StoredContent::Event(ConversationEvent::Unknown)));

    assert!(matches!(view.role, MessageRoleView::Event));
    assert!(view.blocks.is_empty());
    assert_eq!((view.id, view.seq), (7, 3));
}

// ── The `From` impls ───────────────────────────────────────────────────────────

#[test]
fn roles_map_field_for_field() {
    let roles: Vec<Value> = [
        AgentRole::System,
        AgentRole::User,
        AgentRole::Assistant,
        AgentRole::Tool,
    ]
    .into_iter()
    .map(|role| serde_json::to_value(MessageRoleView::from(role)).expect("serializes"))
    .collect();
    assert_eq!(roles, ["system", "user", "assistant", "tool"]);
}

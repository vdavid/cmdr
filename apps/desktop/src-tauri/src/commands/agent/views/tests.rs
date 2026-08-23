//! Tests for the wire mapping layer: what the rail is allowed to see, and what must never
//! leave the backend.
//!
//! These are the cheap, load-bearing half of this module. Everything here is pure, so it
//! runs with no Tauri app and no DB: the async command surface around it is a
//! pass-through and is covered by the runtime and store suites instead.

use serde_json::json;

use super::*;
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

/// A tool result is reduced to its status; the raw content stays backend-only. The result
/// blob is exactly what a read tool returned (paths, OCR text), so shipping it would widen
/// what the rail sees far past the "looked at X" line it renders.
#[test]
fn a_tool_result_block_carries_only_its_status_never_the_content() {
    let view = to_message_view(message(vec![AgentPart::ToolResult(AgentToolResult {
        call_id: "call-1".into(),
        content: json!({ "files": ["/private/receipt.png"] }),
        elided: true,
    })]));

    match &view.blocks[0] {
        MessageBlock::ToolResult { call_id, ok, elided } => {
            assert_eq!(call_id, "call-1");
            assert!(ok);
            assert!(elided, "the rail shows an elided result as elided");
        }
        other => panic!("expected a tool-result block, got {}", json!(other)),
    }
    let wire = serde_json::to_string(&view).expect("serializes");
    assert!(!wire.contains("receipt.png"), "the result content stays backend-only");
}

// ── `tool_result_ok`: our own typed keys, never wording ────────────────────────

/// Reads OUR OWN result keys, so a copy edit to any tool's message can't flip a row's
/// status.
#[test]
fn a_refusal_and_a_handler_problem_both_read_as_not_ok() {
    assert!(tool_result_ok(&json!({ "files": [] })), "a plain answer is ok");
    assert!(
        tool_result_ok(&json!({ "available": true, "files": [] })),
        "an explicit available: true is ok"
    );
    assert!(!tool_result_ok(&json!({ "available": false })), "a refusal is not ok");
    assert!(
        !tool_result_ok(&json!({ "problem": "Pane state isn't available yet" })),
        "a handler problem is not ok"
    );
    assert!(
        !tool_result_ok(&json!({ "available": false, "problem": "both" })),
        "both at once is still not ok"
    );
}

// ── Tool calls and event rows ──────────────────────────────────────────────────

/// The frontend builds its localized "looked at X" label from `tool` + parsed
/// `arguments`, so both must arrive: the wire name (never the Rust variant name) and the
/// raw JSON as a string.
#[test]
fn a_tool_call_block_carries_the_wire_name_and_its_raw_arguments() {
    let view = to_message_view(message(vec![AgentPart::ToolCall(AgentToolCall {
        call_id: "call-2".into(),
        tool: ToolId::ListDir,
        arguments: json!({ "path": "/photos" }),
        reasoning: None,
    })]));

    match &view.blocks[0] {
        MessageBlock::ToolCall {
            call_id,
            tool,
            arguments,
        } => {
            assert_eq!(call_id, "call-2");
            assert_eq!(tool, ToolId::ListDir.as_wire_name());
            assert_eq!(
                serde_json::from_str::<Value>(arguments).expect("arguments are JSON"),
                json!({ "path": "/photos" })
            );
        }
        other => panic!("expected a tool-call block, got {}", json!(other)),
    }
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

/// thing to the model.
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

#[test]
fn an_attachment_maps_to_the_envelope_by_path_and_kind_only() {
    let file = AttachmentRef {
        path: "/photos/one.png".into(),
        kind: AttachmentKindView::File,
    }
    .to_envelope();
    assert_eq!(file.path, "/photos/one.png");
    assert_eq!(file.kind, AttachmentKind::File);

    let folder = AttachmentRef {
        path: "/photos".into(),
        kind: AttachmentKindView::Folder,
    }
    .to_envelope();
    assert_eq!(folder.kind, AttachmentKind::Folder);
}

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

// ── The `From` impls ───────────────────────────────────────────────────────────

/// Every typed failure reaches the frontend as its OWN wire kind. Collapsing two of them
/// would make the rail show the wrong recovery advice ("add a key" for a timeout).
#[test]
fn every_error_kind_maps_to_its_own_wire_kind() {
    let mapped: Vec<Value> = [
        AgentErrorKind::NoKey,
        AgentErrorKind::NotConfigured,
        AgentErrorKind::Unavailable,
        AgentErrorKind::Timeout,
        AgentErrorKind::AuthFailed,
        AgentErrorKind::RateLimited,
        AgentErrorKind::BudgetExhausted,
        AgentErrorKind::UnfinishedReply,
        AgentErrorKind::Provider,
    ]
    .into_iter()
    .map(|kind| serde_json::to_value(AgentErrorKindView::from(kind)).expect("serializes"))
    .collect();

    assert_eq!(
        mapped,
        [
            "noKey",
            "notConfigured",
            "unavailable",
            "timeout",
            "authFailed",
            "rateLimited",
            "budgetExhausted",
            "unfinishedReply",
            "provider",
        ]
    );
}

/// `NoConsent` exists only on the wire: the backend refuses the send before a provider is
/// ever reached, so there's no runtime kind to map from. It must stay distinct from
/// `NotConfigured` so the copy can say what's actually wrong.
#[test]
fn no_consent_is_a_wire_only_kind_distinct_from_not_configured() {
    assert_eq!(
        serde_json::to_value(AgentErrorKindView::NoConsent).expect("serializes"),
        "noConsent"
    );
    assert_ne!(
        serde_json::to_value(AgentErrorKindView::NoConsent).expect("serializes"),
        serde_json::to_value(AgentErrorKindView::from(AgentErrorKind::NotConfigured)).expect("serializes")
    );
}

/// `LocalWindowTooSmall` is the other wire-only kind: the command layer refuses a send whose
/// local server runs with a window too small to hold one prompt, before a turn exists. It
/// carries its own wire value so the copy can name the setting to change, rather than reading
/// as a generic provider problem.
#[test]
fn a_too_small_local_window_is_its_own_wire_kind() {
    assert_eq!(
        serde_json::to_value(AgentErrorKindView::LocalWindowTooSmall).expect("serializes"),
        "localWindowTooSmall"
    );
    assert_ne!(
        serde_json::to_value(AgentErrorKindView::LocalWindowTooSmall).expect("serializes"),
        serde_json::to_value(AgentErrorKindView::from(AgentErrorKind::NotConfigured)).expect("serializes")
    );
}

/// The provider's raw `Other` wording is collapsed to a unit variant: it's untranslated
/// vendor text, and nothing may branch on it.
#[test]
fn stop_reasons_map_across_and_other_never_carries_its_provider_string() {
    let mapped: Vec<Value> = [
        AgentStopReason::Completed,
        AgentStopReason::ToolCall,
        AgentStopReason::MaxTokens,
        AgentStopReason::ContentFilter,
        AgentStopReason::StopSequence,
        AgentStopReason::Other("length_exceeded_by_vendor".into()),
    ]
    .into_iter()
    .map(|stop| serde_json::to_value(StopReasonView::from(stop)).expect("serializes"))
    .collect();

    assert_eq!(
        mapped,
        [
            "completed",
            "toolCall",
            "maxTokens",
            "contentFilter",
            "stopSequence",
            "other",
        ]
    );
}

#[test]
fn usage_and_role_map_field_for_field() {
    let usage = UsageView::from(AgentUsage {
        prompt_tokens: 1_200,
        completion_tokens: 34,
    });
    assert_eq!(usage.prompt_tokens, 1_200);
    assert_eq!(usage.completion_tokens, 34);

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

/// An attachment is path + kind and nothing else, in both directions. The envelope mirror
/// is what actually reaches the model, so a kind flipped here would describe the wrong
/// thing to the model.
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

// ── `to_wire_event` ────────────────────────────────────────────────────────────

fn wire(event: AgentChatEvent) -> Value {
    serde_json::to_value(to_wire_event(event)).expect("serializes")
}

/// Each runtime event becomes its own `type`-tagged wire event with its payload intact.
/// The rail branches on `type`, so a wrong tag silently drops a whole class of progress.
#[test]
fn every_runtime_event_maps_to_its_tagged_wire_event() {
    assert_eq!(wire(AgentChatEvent::Queued), json!({ "type": "queued" }));
    assert_eq!(
        wire(AgentChatEvent::UserPersisted { message_id: 4, seq: 1 }),
        json!({ "type": "userPersisted", "messageId": 4, "seq": 1 })
    );
    assert_eq!(
        wire(AgentChatEvent::AssistantStarted),
        json!({ "type": "assistantStarted" })
    );
    assert_eq!(
        wire(AgentChatEvent::TextDelta { text: "hi".into() }),
        json!({ "type": "textDelta", "text": "hi" })
    );
    assert_eq!(wire(AgentChatEvent::ReasoningTick), json!({ "type": "reasoningTick" }));
    assert_eq!(
        wire(AgentChatEvent::ToolCallFinished {
            call_id: "call-1".into(),
            ok: false,
        }),
        json!({ "type": "toolCallFinished", "callId": "call-1", "ok": false })
    );
    assert_eq!(
        wire(AgentChatEvent::ModelChanged {
            message_id: 9,
            seq: 2,
            model: "claude-sonnet-5".into(),
        }),
        json!({ "type": "modelChanged", "messageId": 9, "seq": 2, "model": "claude-sonnet-5" })
    );
    assert_eq!(
        wire(AgentChatEvent::ContextTrimmed {
            elided_results: 3,
            approx_tokens: 4_200,
        }),
        json!({ "type": "contextTrimmed", "elidedResults": 3, "approxTokens": 4200 })
    );
}

/// The typed `ToolId` becomes its WIRE name, because the frontend's label map is keyed on
/// that. A Rust variant name here shows the generic "Working" fallback instead of "looked
/// at X", costing transparency silently.
#[test]
fn a_started_tool_call_crosses_as_its_wire_name() {
    assert_eq!(
        wire(AgentChatEvent::ToolCallStarted {
            call_id: "call-1".into(),
            tool: ToolId::ListPaneFiles,
        }),
        json!({ "type": "toolCallStarted", "callId": "call-1", "tool": ToolId::ListPaneFiles.as_wire_name() })
    );
}

/// `Done` carries the persisted row's identity plus the turn's usage, both converted.
#[test]
fn done_carries_the_persisted_identity_stop_reason_and_usage() {
    assert_eq!(
        wire(AgentChatEvent::Done {
            message_id: 11,
            seq: 5,
            stop: AgentStopReason::ToolCall,
            usage: AgentUsage {
                prompt_tokens: 900,
                completion_tokens: 120,
            },
        }),
        json!({
            "type": "done",
            "messageId": 11,
            "seq": 5,
            "stop": "toolCall",
            "usage": { "promptTokens": 900, "completionTokens": 120 },
        })
    );
}

/// `detail` is the source error's own wording, shown under the typed headline. It rides
/// verbatim (the frontend branches on `kind`, never on this string) and stays absent when
/// there was none.
#[test]
fn failed_carries_the_typed_kind_and_the_providers_own_wording_verbatim() {
    assert_eq!(
        wire(AgentChatEvent::Failed {
            kind: AgentErrorKind::RateLimited,
            detail: Some("quota resets at 14:00".into()),
        }),
        json!({ "type": "failed", "kind": "rateLimited", "detail": "quota resets at 14:00" })
    );
    assert_eq!(
        wire(AgentChatEvent::Failed {
            kind: AgentErrorKind::BudgetExhausted,
            detail: None,
        }),
        json!({ "type": "failed", "kind": "budgetExhausted", "detail": null })
    );
}

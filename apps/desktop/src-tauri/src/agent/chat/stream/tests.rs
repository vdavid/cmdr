//! What a turn is allowed to stream, and what must never leave the backend on the way.
//!
//! Everything here is pure, so it runs with no Tauri app and no DB: the emit half is a
//! one-line `Event::emit` a unit test can't reach without a running app.

use serde_json::{Value, json};

use super::*;
use crate::agent::llm::types::ToolId;

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
        AgentErrorKind::RepeatedToolCall,
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
            "repeatedToolCall",
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
fn usage_maps_field_for_field() {
    let usage = UsageView::from(AgentUsage {
        prompt_tokens: 1_200,
        completion_tokens: 34,
    });
    assert_eq!(usage.prompt_tokens, 1_200);
    assert_eq!(usage.completion_tokens, 34);
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

// ── The envelope, and the two events no runtime event maps to ──────────────────

/// The conversation id is what a subscriber filters on, so it rides the ENVELOPE rather than
/// one variant's payload: a rail that had to read it out of `started` would have no way to
/// tell whose `textDelta` it was holding.
#[test]
fn every_event_crosses_under_the_conversation_it_belongs_to() {
    let turn = AskCmdrTurn {
        conversation_id: 41,
        event: AskCmdrStreamEvent::TextDelta { text: "hi".into() },
    };
    assert_eq!(
        serde_json::to_value(turn).expect("serializes"),
        json!({ "conversationId": 41, "event": { "type": "textDelta", "text": "hi" } })
    );
}

/// `Started` and `Discarded` bracket a wake: the first is how the session list hears a thread
/// was created with nobody having typed anything, the second how it hears the thread is gone
/// again. Neither maps from an [`AgentChatEvent`] — the runtime doesn't create or delete
/// threads — so they'd be easy to drop in a refactor of `to_wire_event`. Pin their wire tags.
#[test]
fn a_wake_brackets_its_thread_with_started_and_discarded() {
    assert_eq!(
        serde_json::to_value(AskCmdrStreamEvent::Started).expect("serializes"),
        json!({ "type": "started" })
    );
    assert_eq!(
        serde_json::to_value(AskCmdrStreamEvent::Discarded).expect("serializes"),
        json!({ "type": "discarded" })
    );
}

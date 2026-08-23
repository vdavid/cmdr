//! Fixtures shared by the context test modules (`tests`, `stub_tests`, `cost_tests`):
//! transcript builders, a stock envelope, and the budgets they assemble against.
//!
//! Everything here is a plain value. The core is pure, so no fixture needs a tokio
//! runtime, a DB, or app state — and none may introduce one.

use chrono::FixedOffset;
use serde_json::{Value, json};

use crate::agent::chat::budget::DEFAULT_PROMPT_TOKEN_BUDGET;
use crate::agent::chat::context::{
    ContextEnvelope, EnvelopeConnectivity, EnvelopeFreshness, EnvelopeVolume, PrefixInputs,
};
use crate::agent::llm::types::{
    AgentMessage, AgentPart, AgentRole, AgentToolCall, AgentToolResult, ToolDeclaration, ToolId,
};

pub(crate) const SYSTEM: &str = "SYSTEM PROMPT BODY";

/// The budget most tests assemble against: the conservative default, so a test that isn't
/// about budget pressure never accidentally trips it.
pub(crate) const BUDGET: usize = DEFAULT_PROMPT_TOKEN_BUDGET;

/// A deliberately tight budget for the pressure tests, matching the one the interactive
/// slot ran under when a 12-file batch overflowed it.
pub(crate) const TIGHT_BUDGET: usize = 8_000;

pub(crate) fn offset() -> FixedOffset {
    FixedOffset::east_opt(2 * 3600).expect("valid offset")
}

pub(crate) fn user(text: &str, at: i64) -> AgentMessage {
    AgentMessage {
        role: AgentRole::User,
        parts: vec![AgentPart::Text(text.to_string())],
        at,
    }
}

pub(crate) fn assistant_text(text: &str, at: i64) -> AgentMessage {
    AgentMessage {
        role: AgentRole::Assistant,
        parts: vec![AgentPart::Text(text.to_string())],
        at,
    }
}

pub(crate) fn assistant_tool_call(call_id: &str, tool: ToolId, args: Value, at: i64) -> AgentMessage {
    AgentMessage {
        role: AgentRole::Assistant,
        parts: vec![AgentPart::ToolCall(AgentToolCall {
            call_id: call_id.to_string(),
            tool,
            arguments: args,
            reasoning: None,
        })],
        at,
    }
}

pub(crate) fn tool_result(call_id: &str, content: Value, at: i64) -> AgentMessage {
    AgentMessage {
        role: AgentRole::Tool,
        parts: vec![AgentPart::ToolResult(AgentToolResult {
            call_id: call_id.to_string(),
            content,
            elided: false,
        })],
        at,
    }
}

pub(crate) fn declaration(tool: ToolId) -> ToolDeclaration {
    ToolDeclaration {
        name: tool,
        description: "a read tool".to_string(),
        schema: json!({ "type": "object" }),
    }
}

pub(crate) fn prefix<'a>(cmdr_md: Option<&'a str>, tools: &'a [ToolDeclaration]) -> PrefixInputs<'a> {
    PrefixInputs {
        system_prompt: SYSTEM,
        cmdr_md,
        memory: None,
        tools,
    }
}

pub(crate) fn envelope_at(at: i64) -> ContextEnvelope {
    ContextEnvelope {
        captured_at: at,
        focused_pane_path: Some("~/Documents/taxes".to_string()),
        cursor_item: Some("2024/".to_string()),
        selection_count: 2,
        denied_names: vec![],
        rename_batch_files: 101,
        volumes: vec![
            EnvelopeVolume {
                name: "Macintosh HD".to_string(),
                freshness: EnvelopeFreshness::Fresh,
                connectivity: None,
            },
            EnvelopeVolume {
                name: "NAS-home".to_string(),
                freshness: EnvelopeFreshness::Stale,
                connectivity: Some(EnvelopeConnectivity::Direct),
            },
        ],
        attachments: vec![],
    }
}

/// Pull the tool-result part a message carries, elided or not.
pub(crate) fn tool_result_part(message: &AgentMessage) -> &AgentToolResult {
    match &message.parts[0] {
        AgentPart::ToolResult(result) => result,
        _ => panic!("expected a tool-result part"),
    }
}

/// Pull the leading text part of a message (the envelope or timestamp marker the
/// assembly prepends).
pub(crate) fn leading_text(message: &AgentMessage) -> &str {
    match &message.parts[0] {
        AgentPart::Text(text) => text,
        _ => panic!("expected a leading text part"),
    }
}

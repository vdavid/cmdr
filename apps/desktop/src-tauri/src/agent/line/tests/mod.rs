//! Table tests per tool: arguments to subject, result to refined subject, result to outcome,
//! and result to preview. Result fixtures are built from the handler's real result struct where
//! one is reachable from here, and otherwise copy the handler's serde field names.

mod fixtures;
mod outcome_tests;
mod preview_tests;
mod repeat_tests;
mod subject_tests;

use serde_json::Value;

use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ToolId};

fn call(tool: ToolId, arguments: Value) -> AgentToolCall {
    call_with_id("call-1", tool, arguments)
}

fn call_with_id(call_id: &str, tool: ToolId, arguments: Value) -> AgentToolCall {
    AgentToolCall {
        call_id: call_id.to_string(),
        tool,
        arguments,
        reasoning: None,
    }
}

fn result(content: Value) -> AgentToolResult {
    AgentToolResult {
        call_id: "call-1".to_string(),
        content,
        elided: false,
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("a tool result serializes")
}

//! The agent's gated tool-dispatch view: the read-only choke point at runtime.
//!
//! A provider returns each tool call's name as a raw string. The runtime parses it
//! into a typed [`ToolId`] (`from_wire_name`) BEFORE dispatch; a name that isn't a
//! known agent-view tool becomes [`ToolId::Unrecognized`], which [`refuse_unavailable`]
//! turns into a typed "not available" tool-result WITHOUT ever calling
//! `execute_tool`. So read-only-by-construction holds at runtime, not just
//! structurally: the parse step is the gate, backed by a `tool_access` check that
//! refuses anything the registry doesn't classify `Read` even if it entered the view.

use serde_json::json;
use tauri::{AppHandle, Runtime};

use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ToolId};
use crate::mcp::{Access, Consumer, execute_tool, tool_access};

/// The refusal tool-result for a tool the agent can't dispatch, or `None` when
/// `tool` is a known read tool dispatch should execute. Refused: any
/// [`ToolId::Unrecognized`] name (hallucinated, a typo, or a write/non-view tool
/// like `delete`/`copy`), AND — as a runtime backstop — any known name the registry
/// doesn't classify [`Access::Read`]. Returning `Some` here means `execute_tool` is
/// never reached.
pub fn refuse_unavailable(call_id: &str, tool: &ToolId) -> Option<AgentToolResult> {
    let dispatchable = tool.is_known() && tool_access(tool.as_wire_name()) == Some(Access::Read);
    if dispatchable {
        return None;
    }
    Some(AgentToolResult {
        call_id: call_id.to_string(),
        content: json!({
            "available": false,
            "requested": tool.as_wire_name(),
            "reason": "That tool isn't available. Ask Cmdr is read-only: it can look at your files, their metadata, and the app state, but it can't act, change anything, or read file contents.",
        }),
        elided: false,
    })
}

/// Dispatch one tool call through the agent's read-only view. The parse gate is
/// consulted FIRST; only a known read tool reaches `execute_tool` with the
/// [`Consumer::Agent`] identity (which itself refuses any name outside the agent
/// view — a second, structural backstop). A handler error comes back as a typed,
/// non-fatal tool-result the model can relay.
pub async fn dispatch<R: Runtime>(app: &AppHandle<R>, call: &AgentToolCall) -> AgentToolResult {
    if let Some(refusal) = refuse_unavailable(&call.call_id, &call.tool) {
        return refusal;
    }
    match execute_tool(app, Consumer::Agent, call.tool.as_wire_name(), &call.arguments).await {
        Ok(content) => AgentToolResult {
            call_id: call.call_id.clone(),
            content,
            elided: false,
        },
        Err(err) => AgentToolResult {
            call_id: call.call_id.clone(),
            content: json!({ "problem": err.message }),
            elided: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::AgentLlm;
    use crate::agent::llm::fake::{FakeAgentLlm, ScriptedTurn};
    use crate::agent::llm::types::AgentDelta;
    use futures_util::StreamExt;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn refuses_a_write_tool_name_without_dispatching() {
        // A hallucinated write name parses to Unrecognized and is refused: the
        // refusal carries the read-only reason and the requested name, and it
        // returns BEFORE execute_tool could run.
        for raw in ["delete", "copy", "definitely_not_a_tool"] {
            let tool = ToolId::from_wire_name(raw);
            assert!(!tool.is_known(), "{raw} must not be a known agent tool");
            let refusal = refuse_unavailable("call-1", &tool).expect("refused");
            assert_eq!(refusal.content["available"], false);
            assert_eq!(refusal.content["requested"], raw);
        }
    }

    #[test]
    fn passes_a_known_read_tool_through_to_dispatch() {
        // A known read tool is NOT refused — dispatch proceeds to execute_tool.
        for tool in ToolId::KNOWN {
            assert!(
                refuse_unavailable("call-1", &tool).is_none(),
                "{} is a read tool and must reach dispatch",
                tool.as_wire_name()
            );
        }
    }

    #[tokio::test]
    async fn raw_provider_name_delete_is_parsed_and_refused_end_to_end() {
        // The full parse gate: a provider emitting the raw name "delete" (via the
        // fake's CallRawTool) yields a ToolCall whose ToolId is Unrecognized, which
        // the dispatch gate refuses — execute_tool is never involved.
        let fake = FakeAgentLlm::script(vec![ScriptedTurn::CallRawTool("delete".into(), json!({ "path": "/" }))]);
        let stream = fake
            .respond("sys", &[], &[], CancellationToken::new())
            .await
            .expect("turn starts");
        let deltas: Vec<AgentDelta> = stream.map(|d| d.expect("no stream error")).collect().await;

        let AgentDelta::End { message, .. } = deltas.last().expect("an End delta") else {
            panic!("expected End");
        };
        let crate::agent::llm::types::AgentPart::ToolCall(call) = &message.parts[0] else {
            panic!("expected a tool call part");
        };
        assert_eq!(call.tool, ToolId::Unrecognized("delete".into()));

        let refusal = refuse_unavailable(&call.call_id, &call.tool).expect("dispatch refuses the write name");
        assert_eq!(refusal.content["available"], false);
        assert_eq!(refusal.content["requested"], "delete");
    }
}

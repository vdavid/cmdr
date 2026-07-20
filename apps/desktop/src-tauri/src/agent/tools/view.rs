//! The agent's gated tool-dispatch view: the no-write choke point at runtime.
//!
//! A provider returns each tool call's name as a raw string. The runtime parses it
//! into a typed [`ToolId`] (`from_wire_name`) BEFORE dispatch; a name that isn't a
//! known agent-view tool becomes [`ToolId::Unrecognized`], which [`refuse_unavailable`]
//! turns into a typed "not available" tool-result WITHOUT ever calling
//! `execute_tool`. So the boundary holds at runtime, not just structurally: the parse
//! step is the gate, backed by a `tool_access` check that refuses anything the registry
//! doesn't classify [`Access::Read`] or [`Access::Propose`] even if it entered the view.
//!
//! **The agent can propose; only the user can approve.** A [`Access::Propose`] tool
//! stages a proposal and opens a review surface; it mutates nothing. Approval originates
//! in the frontend as a user action, and there is no tool that approves a proposal.

use serde_json::json;
use tauri::{AppHandle, Runtime};

use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ToolId};
use crate::mcp::{Access, Consumer, execute_tool, tool_access};

/// The access axis of the gate: whether a registry access class may dispatch through the
/// agent's view. [`Access::Read`] and [`Access::Propose`] may, [`Access::Write`] never may,
/// and an unclassified name (`None`) never may. Pure, so the widened rule is unit-testable
/// against every variant without an authored tool per variant.
fn access_is_dispatchable(access: Option<Access>) -> bool {
    match access {
        Some(Access::Read | Access::Propose) => true,
        Some(Access::Write) | None => false,
    }
}

/// The refusal tool-result for a tool the agent can't dispatch, or `None` when
/// `tool` is a known read-or-propose tool dispatch should execute. Refused: any
/// [`ToolId::Unrecognized`] name (hallucinated, a typo, or a write/non-view tool
/// like `delete`/`copy`), AND — as a runtime backstop — any known name the registry
/// classifies [`Access::Write`] or doesn't classify at all. Returning `Some` here means
/// `execute_tool` is never reached.
pub fn refuse_unavailable(call_id: &str, tool: &ToolId) -> Option<AgentToolResult> {
    let dispatchable = tool.is_known() && access_is_dispatchable(tool_access(tool.as_wire_name()));
    if dispatchable {
        return None;
    }
    // The refusal reason says "read-only" because zero `Propose` tools are authored today, so
    // it's accurate. The first `Propose` tool has to reword it (the agent can ask, not act).
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

/// Dispatch one tool call through the agent's gated view. The parse gate is
/// consulted FIRST; only a known read-or-propose tool reaches `execute_tool` with the
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
    fn the_access_gate_admits_read_and_propose_and_refuses_everything_else() {
        // The gate's access axis, exercised directly against every `Access` variant.
        // The name-based tests below can only reach the variants some tool is actually
        // authored with, so with no `Propose` tool authored yet they'd cover `Propose`
        // vacuously. This one doesn't: it pins the widened rule itself — the agent may
        // read and may ask, and may never write.
        assert!(access_is_dispatchable(Some(Access::Read)), "a read tool must dispatch");
        assert!(
            access_is_dispatchable(Some(Access::Propose)),
            "a propose tool must dispatch — it stages a proposal for the user, it doesn't act"
        );
        assert!(
            !access_is_dispatchable(Some(Access::Write)),
            "a write tool must never dispatch through the agent view"
        );
        assert!(
            !access_is_dispatchable(None),
            "a name the registry doesn't classify must never dispatch"
        );
    }

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

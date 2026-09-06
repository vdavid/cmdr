//! The agent's gated tool-dispatch view: the no-write choke point at runtime.
//!
//! A provider returns each tool call's name as a raw string. The runtime parses it
//! into a typed [`ToolId`] (`from_wire_name`) BEFORE dispatch; a name that isn't a
//! known agent-view tool becomes [`ToolId::Unrecognized`], which [`refuse_unavailable`]
//! turns into a typed "not available" tool-result WITHOUT ever calling
//! `execute_tool`. So the boundary holds at runtime, not just structurally: the parse
//! step is the gate, backed by a `tool_access` check that refuses anything the registry
//! doesn't classify [`Access::Read`], [`Access::Propose`], or [`Access::Memory`] even if it
//! entered the view.
//!
//! **The agent can propose; only the user can approve.** A [`Access::Propose`] tool
//! stages a proposal and opens a review surface; it mutates nothing. Approval originates
//! in the frontend as a user action, and there is no tool that approves a proposal.
//!
//! **And it writes only its own notes.** [`Access::Memory`] widens the gate by exactly one
//! folder (`agent::memory`'s jail); the promise the app makes is "the agent writes only into
//! its memory folder", which is still structural.

use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};

use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ToolId};
use crate::agent::tools::propose::evidence::EvidenceScope;
use crate::agent::tools::propose::rename::{RenameDispatchOutcome, RenameProposalSnapshot};
use crate::mcp::{Access, Consumer, execute_tool, tool_access, validate_params};

/// The access axis of the gate: whether a registry access class may dispatch through the
/// agent's view. [`Access::Read`], [`Access::Propose`], and [`Access::Memory`] may,
/// [`Access::Write`] never may, and an unclassified name (`None`) never may. Pure, so the
/// widened rule is unit-testable against every variant without an authored tool per variant.
fn access_is_dispatchable(access: Option<Access>) -> bool {
    match access {
        Some(Access::Read | Access::Propose | Access::Memory) => true,
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
    Some(AgentToolResult {
        call_id: call_id.to_string(),
        content: json!({
            "available": false,
            "requested": tool.as_wire_name(),
            "reason": "That tool isn't available. Ask Cmdr can prepare a rename plan, suggest file operations for you to review, and save notes in its own memory folder. It can't touch your files or approve a proposal, and it reads a file's contents only through inspect_file.",
        }),
        elided: false,
    })
}

/// Make a [`Access::Propose`] tool's problem result say that nothing was staged.
///
/// **The invariant: a propose result always answers `readyForReview`** — `true` when it staged,
/// `false` when it didn't. A model told a user their rename plan was waiting in the suggestions
/// panel after the schema gate refused it, having read a result that said only `problem`. That
/// is the one propose answer with nothing to say about whether anything got staged, and the
/// model filled the gap in its own favour.
///
/// It stamps HERE, at dispatch's single exit, rather than at each place a refusal is built.
/// Four construction sites already have to agree — the gate, `execute_tool`'s flattened
/// `ToolError`, `propose_in_thread`'s, and the boundary's own typed refusals — and the fifth
/// nobody has written yet is the one that would go missing. One choke point can't be bypassed
/// by adding a path.
///
/// Both decisions are typed: the registry's [`Access`] says which tools owe a verdict, and
/// [`AgentToolResult::reports_a_problem`] reads our own result keys to tell a problem from an
/// answer. Neither reads the tool's name or the refusal's wording.
///
/// It fails closed but never overwrites: a result that already answered keeps its own verdict,
/// so a plan that DID stage can't be reported as staging nothing. That would be the same
/// dishonesty pointed the other way, and the user's panel would contradict it.
fn ensure_review_verdict(tool: &ToolId, result: &mut AgentToolResult) {
    if tool_access(tool.as_wire_name()) != Some(Access::Propose) || !result.reports_a_problem() {
        return;
    }
    if let Some(map) = result.content.as_object_mut() {
        map.entry("readyForReview").or_insert(json!(false));
    }
}

/// Dispatch one tool call through the agent's gated view. The parse gate is
/// consulted FIRST; only a known read-or-propose tool reaches `execute_tool` with the
/// [`Consumer::Agent`] identity (which itself refuses any name outside the agent
/// view — a second, structural backstop). A handler error comes back as a typed,
/// non-fatal tool-result the model can relay.
pub struct DispatchOutcome {
    pub result: AgentToolResult,
    pub proposal: Option<RenameProposalSnapshot>,
}

pub async fn dispatch<R: Runtime>(app: &AppHandle<R>, scope: EvidenceScope, call: &AgentToolCall) -> DispatchOutcome {
    let mut outcome = route_call(app, scope, call).await;
    // The single exit every path funnels through, so no branch above can answer a propose call
    // without saying whether anything was staged.
    ensure_review_verdict(&call.tool, &mut outcome.result);
    outcome
}

async fn route_call<R: Runtime>(app: &AppHandle<R>, scope: EvidenceScope, call: &AgentToolCall) -> DispatchOutcome {
    if let Some(refusal) = refuse_unavailable(&call.call_id, &call.tool) {
        return DispatchOutcome {
            result: refusal,
            proposal: None,
        };
    }
    // The schema gate, ahead of the branch so it covers every agent call. `execute_tool`
    // runs it too, for its own callers; the two thread-scoped propose tools below never
    // reach `execute_tool`, and a model guesses an argument at them as readily as at a
    // read tool. Checking one small object twice costs nothing next to a provider round
    // trip, and neither dispatch path has to know what the other does.
    if let Err(problem) = validate_params(call.tool.as_wire_name(), &call.arguments) {
        // The one trace a refused call leaves. Without it, a plan that never reached the
        // proposal store is invisible: the log showed the provider round trips and the repeat
        // breaker firing, and nothing at all about WHY the first call died, which is what made
        // one real report a transcript dive. The typed `data` rides along, since that — never
        // the sentence — is what says which properties were wrong.
        log::warn!(
            target: "agent::tools",
            "{} was refused before dispatch by the schema gate: {} ({})",
            call.tool.as_wire_name(),
            problem.message,
            problem.data.as_ref().unwrap_or(&Value::Null)
        );
        return DispatchOutcome {
            result: AgentToolResult {
                call_id: call.call_id.clone(),
                content: json!({ "problem": problem.message }),
                elided: false,
            },
            proposal: None,
        };
    }
    if call.tool == ToolId::ProposeRenamePlan {
        let RenameDispatchOutcome { result, proposal } =
            crate::agent::tools::propose::rename::dispatch(app, scope, &call.call_id, &call.arguments).await;
        return DispatchOutcome { result, proposal };
    }
    // `propose_suggestions` is the one registry tool that needs to know WHICH thread asked:
    // a sweep records the conversation it came out of, and the registry path (an external
    // MCP client) has none. Everything else goes through the plain dispatch.
    let outcome = if call.tool == ToolId::ProposeSuggestions {
        crate::agent::tools::suggestions::propose_in_thread(app, scope.conversation_id(), &call.arguments).await
    } else {
        execute_tool(app, Consumer::Agent, call.tool.as_wire_name(), &call.arguments).await
    };
    let result = match outcome {
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
    };
    // Image facts are the only tool result a later rename plan may cite as evidence, so
    // the ledger records what this call actually handed the model. See
    // `propose/evidence.rs` for the guardrail and its revocation seam.
    if call.tool == ToolId::ImageFacts {
        crate::agent::tools::propose::rename::note_image_facts_delivered(app, scope, &result);
    }
    DispatchOutcome { result, proposal: None }
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
    fn the_access_gate_admits_read_propose_and_memory_and_refuses_everything_else() {
        // The gate's access axis, exercised directly against every `Access` variant. The
        // name-based tests below can only reach the variants some tool is actually authored
        // with, so a variant nothing carries yet would be covered vacuously. This one pins
        // the rule itself — the agent may read, may ask, and may write its own notes, and may
        // never write anything else.
        assert!(access_is_dispatchable(Some(Access::Read)), "a read tool must dispatch");
        assert!(
            access_is_dispatchable(Some(Access::Propose)),
            "a propose tool must dispatch — it stages a proposal for the user, it doesn't act"
        );
        assert!(
            access_is_dispatchable(Some(Access::Memory)),
            "a memory tool must dispatch — the agent writes its own notes, and nothing else"
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

    /// A tool result as it stands just before `dispatch` returns it.
    fn result(content: Value) -> AgentToolResult {
        AgentToolResult {
            call_id: "call-1".to_string(),
            content,
            elided: false,
        }
    }

    #[test]
    fn every_shape_of_propose_problem_leaves_dispatch_saying_nothing_is_waiting_for_review() {
        // The invariant, at the one place it can be held for every propose path at once. A
        // model told a user their rename plan was waiting in the suggestions panel after the
        // schema gate refused it; the result it read said only `problem`, which is the one
        // propose answer with nothing to say about whether anything got staged.
        //
        // Each shape below is a real path that reached the model without a verdict: the gate's
        // refusal, `execute_tool`'s and `propose_in_thread`'s flattened `ToolError` (a closed
        // store, a serialization failure), and a refusal already annotated as a repeat.
        let shapes = [
            json!({ "problem": "propose_rename_plan has no volumeId parameter. It takes renames." }),
            json!({ "problem": "Cmdr's suggestion store isn't open yet." }),
            json!({ "problem": "…", "repeatedCall": true, "guidance": "…" }),
            json!({ "available": false, "requested": "propose_rename_plan", "reason": "…" }),
        ];
        for tool in [ToolId::ProposeRenamePlan, ToolId::ProposeSuggestions] {
            for shape in &shapes {
                let mut answer = result(shape.clone());
                ensure_review_verdict(&tool, &mut answer);
                assert_eq!(
                    answer.content["readyForReview"],
                    false,
                    "{} left {shape} without the verdict, so a model can read it as success",
                    tool.as_wire_name()
                );
            }
        }
    }

    #[test]
    fn a_staged_plan_keeps_its_own_verdict() {
        // The stamp fails closed, so it must never reach a result that DID stage something:
        // telling the user nothing is waiting while a proposal sits in their panel is the same
        // dishonesty pointed the other way.
        let mut staged = result(json!({ "readyForReview": true, "count": 9 }));
        ensure_review_verdict(&ToolId::ProposeRenamePlan, &mut staged);
        assert_eq!(staged.content["readyForReview"], true);
        assert_eq!(staged.content["count"], 9);

        // And a refusal that already answered keeps its own wording rather than being restamped.
        let mut refused = result(json!({ "readyForReview": false, "evidenceRejected": [], "problem": "…" }));
        ensure_review_verdict(&ToolId::ProposeRenamePlan, &mut refused);
        assert_eq!(refused.content["readyForReview"], false);
        assert!(refused.content.get("evidenceRejected").is_some());
    }

    #[test]
    fn a_read_tool_problem_claims_no_review_verdict_it_has_no_business_making() {
        // `readyForReview` is the propose family's contract. Stamping it onto a read refusal
        // would invent a verdict about a review that was never in play.
        for tool in [ToolId::ListDir, ToolId::ImageFacts, ToolId::MemoryWrite] {
            let mut answer = result(json!({ "problem": "list_dir needs path." }));
            ensure_review_verdict(&tool, &mut answer);
            assert!(
                answer.content["readyForReview"].is_null(),
                "{} is not a propose tool and must claim no review verdict",
                tool.as_wire_name()
            );
        }
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

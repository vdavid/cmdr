//! The agent's way into the tools: what a model's call turns into, and the shape its answer takes.
//!
//! A provider returns each tool call's name as a raw string. The runtime parses it into a typed
//! [`ToolId`] (`from_wire_name`) BEFORE dispatch, and [`refuse_unavailable`] turns anything
//! [`admit`] refuses, or a name outside the table, into a typed tool-result without ever calling
//! `execute_tool`. **`admit` decides; this file only shapes.** `execute_tool` asks it again for its
//! own callers, and it's pure, so the two can't disagree.
//!
//! **The agent can start anything. A person finishes anything sensitive.** What it can reach and on
//! which turns: `admit`'s matrix (`tools/DETAILS.md` § The rule). The origin is the turn's own, handed
//! to every call in [`CallContext`], never inferred from what the turn opened with.

use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};
use tokio_util::sync::CancellationToken;

use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ToolId};
use crate::agent::types::TurnOrigin;
use crate::tools::policy::{Refusal, Verdict, admit};
use crate::tools::propose::evidence::EvidenceScope;
use crate::tools::propose::rename::{RenameDispatchOutcome, RenameProposalSnapshot};
use crate::tools::{Access, Caller, ToolCtx, ToolError, execute_tool, tool_access, validate_params};

/// What a tool call knows about the turn it runs in: who started it, for the gate, and the
/// turn's Stop. One value, so a dispatch path can't take one and forget the other.
#[derive(Clone, Copy)]
pub struct CallContext<'a> {
    pub origin: TurnOrigin,
    /// Flips when the user presses Stop. The turn already abandons an in-flight call on it
    /// (`agent::chat::runtime::dispatch`); a handler that holds it can stop its own work and
    /// answer with something more specific first, the way `list_dir`'s live read does.
    pub cancel: &'a CancellationToken,
}

/// The refusal tool-result for a call [`admit`] refuses, or a name outside the table; `None` when
/// dispatch should run it. Returning `Some` here means `execute_tool` is never reached.
pub fn refuse_unavailable(call_id: &str, tool: &ToolId, origin: TurnOrigin) -> Option<AgentToolResult> {
    let Some(access) = tool_access(tool.as_wire_name()) else {
        return Some(unknown_tool(call_id, tool));
    };
    match admit(Caller::Agent { origin }, access) {
        Verdict::Allow | Verdict::Ask => None,
        Verdict::Refuse(refusal) => Some(refused(call_id, tool, refusal)),
    }
}

/// `available: false` like every gate refusal, so it reads as a problem; `refusal` is the token a
/// model can branch on, and `reason` says what's possible instead.
fn refused(call_id: &str, tool: &ToolId, refusal: Refusal) -> AgentToolResult {
    let name = tool.as_wire_name();
    AgentToolResult {
        call_id: call_id.to_string(),
        content: json!({
            "available": false,
            "requested": name,
            "refusal": refusal.token(),
            "reason": refusal.message(name),
        }),
        elided: false,
    }
}

fn unknown_tool(call_id: &str, tool: &ToolId) -> AgentToolResult {
    AgentToolResult {
        call_id: call_id.to_string(),
        content: json!({
            "available": false,
            "requested": tool.as_wire_name(),
            "reason": "Cmdr has no tool by that name. Use a tool from your list.",
        }),
        elided: false,
    }
}

/// A `ToolError` as the model reads it. `data` rides along, since it's the contract and the message
/// only the human half: `blockingDialog` names what's in the way, and `refusal` says a person said no.
///
/// Two errors don't read as problems. [`ToolError::outcome_unknown`] means the action went out and may
/// still run, so it can't say `problem`: the repeat breaker would stop the retry a model needs to
/// check, and the model would tell the user a copy that's running didn't happen. And a question Stop
/// took down answers in the words of any call Stop ended (`dispatch.rs`).
fn error_content(error: &ToolError) -> Value {
    if error.outcome_is_unknown() {
        return json!({ "outcomeUnknown": true, "detail": error.message });
    }
    let refusal = error.data.as_ref().and_then(|data| data.get("refusal"));
    if refusal.and_then(Value::as_str) == Some("cancelled") {
        return json!({ "miss": "cancelled", "detail": error.message });
    }
    match &error.data {
        Some(data) => json!({ "problem": error.message, "data": data }),
        None => json!({ "problem": error.message }),
    }
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

/// One call's answer, and the rename proposal it staged if any. A handler error comes back as a typed,
/// non-fatal tool-result the model can relay.
pub struct DispatchOutcome {
    pub result: AgentToolResult,
    pub proposal: Option<RenameProposalSnapshot>,
}

pub async fn dispatch<R: Runtime>(
    app: &AppHandle<R>,
    scope: EvidenceScope,
    call: &AgentToolCall,
    turn: CallContext<'_>,
) -> DispatchOutcome {
    let mut outcome = route_call(app, scope, call, turn).await;
    // The single exit every path funnels through, so no branch above can answer a propose call
    // without saying whether anything was staged.
    ensure_review_verdict(&call.tool, &mut outcome.result);
    outcome
}

async fn route_call<R: Runtime>(
    app: &AppHandle<R>,
    scope: EvidenceScope,
    call: &AgentToolCall,
    turn: CallContext<'_>,
) -> DispatchOutcome {
    if let Some(refusal) = refuse_unavailable(&call.call_id, &call.tool, turn.origin) {
        return DispatchOutcome {
            result: refusal,
            proposal: None,
        };
    }
    // The schema gate, ahead of the branch so it covers every agent call. `execute_tool`
    // runs it too, for its own callers; `propose_rename_plan` below never reaches `execute_tool`,
    // and a model guesses an argument at it as readily as at a read tool. Checking one small object
    // twice costs nothing next to a provider round trip, and neither dispatch path has to know what
    // the other does.
    if let Err(problem) = validate_params(call.tool.as_wire_name(), &call.arguments) {
        // The one trace a refused call leaves. Without it, a plan that never reached the
        // proposal store is invisible: the log showed the provider round trips and the repeat
        // breaker firing, and nothing at all about WHY the first call died, which is what made
        // one real report a transcript dive. The typed `data` rides along, since that — never
        // the sentence — is what says which properties were wrong.
        log::warn!(
            target: "agent::tools",
            "{} was refused before dispatch by the schema gate: {} ({})",
            call.tool.as_wire_name(), // allowed-log-private: an agent tool's wire name of our own
            problem.message,
            problem.data.as_ref().unwrap_or(&Value::Null)
        );
        return DispatchOutcome {
            result: AgentToolResult {
                call_id: call.call_id.clone(),
                content: error_content(&problem),
                elided: false,
            },
            proposal: None,
        };
    }
    if call.tool == ToolId::ProposeRenamePlan {
        let RenameDispatchOutcome { result, proposal } =
            crate::tools::propose::rename::dispatch(app, scope, &call.call_id, &call.arguments).await;
        return DispatchOutcome { result, proposal };
    }
    // What a handler may want to know about this turn: which thread a proposal records, the origin
    // a server card checks, and the Stop that `list_dir` and `connect_volume` answer in their own
    // vocabulary.
    let ctx = ToolCtx {
        caller: Caller::Agent { origin: turn.origin },
        cancel: turn.cancel.clone(),
        conversation_id: scope.conversation_id(),
        asked: None,
    };
    let outcome = execute_tool(app, &ctx, call.tool.as_wire_name(), &call.arguments).await;
    let result = match outcome {
        Ok(content) => AgentToolResult {
            call_id: call.call_id.clone(),
            content,
            elided: false,
        },
        Err(err) => AgentToolResult {
            call_id: call.call_id.clone(),
            content: error_content(&err),
            elided: false,
        },
    };
    // Image facts are the only tool result a later rename plan may cite as evidence, so
    // the ledger records what this call actually handed the model. See
    // `propose/evidence.rs` for the guardrail and its revocation seam.
    if call.tool == ToolId::ImageFacts {
        crate::tools::propose::rename::note_image_facts_delivered(app, scope, &result);
    }
    // A later rename plan is compared against the listing the model last saw, to count plans
    // built from a cut one (`propose/rename/cut_listing.rs`).
    if call.tool == ToolId::ListPaneFiles {
        crate::tools::propose::rename::note_pane_listing(app, scope, &result);
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

    const ALL_ORIGINS: [TurnOrigin; 4] = [
        TurnOrigin::Text,
        TurnOrigin::Wake,
        TurnOrigin::Outcomes,
        TurnOrigin::Resume,
    ];

    /// Every tool × every origin, against `admit`, the one thing deciding: this file adds no opinion.
    #[test]
    fn every_tool_on_every_origin_answers_what_admit_says() {
        for tool in ToolId::KNOWN {
            let name = tool.as_wire_name();
            let access = tool_access(name).expect("a table entry has a class");
            for origin in ALL_ORIGINS {
                let refusal = refuse_unavailable("call-1", &tool, origin);
                match admit(Caller::Agent { origin }, access) {
                    Verdict::Allow | Verdict::Ask => assert!(refusal.is_none(), "{name} on {origin:?}"),
                    Verdict::Refuse(why) => {
                        let refusal = refusal.unwrap_or_else(|| panic!("{name} on {origin:?} reached dispatch"));
                        assert!(
                            refusal.reports_a_problem(),
                            "{name}: a refusal must not read as an answer"
                        );
                        assert_eq!(refusal.content["available"], false);
                        assert_eq!(refusal.content["requested"], name);
                        assert_eq!(refusal.content["refusal"], why.token());
                    }
                }
            }
        }
    }

    /// The fence, named: nothing that goes past a person reaches dispatch, on any origin.
    #[test]
    fn nothing_past_a_person_reaches_dispatch_on_any_origin() {
        for tool in [
            ToolId::DialogConfirm,
            ToolId::ResolveConflict,
            ToolId::QueueMoveLocked,
            ToolId::ConnectToServer,
            ToolId::AiSearch,
        ] {
            for origin in ALL_ORIGINS {
                let refusal = refuse_unavailable("call-1", &tool, origin).expect("refused");
                assert_eq!(refusal.content["refusal"], "devOnly", "{tool:?} on {origin:?}");
            }
        }
    }

    /// A wake gets Read, ReadContents, Propose, and Memory, and the rest wait for the user.
    #[test]
    fn a_background_turn_gets_only_reads_proposals_and_memory() {
        for tool in ToolId::KNOWN {
            let name = tool.as_wire_name();
            let access = tool_access(name).expect("a table entry has a class");
            for origin in [TurnOrigin::Wake, TurnOrigin::Outcomes] {
                let refusal = refuse_unavailable("call-1", &tool, origin);
                match access {
                    Access::Read | Access::ReadContents | Access::Propose | Access::Memory => {
                        assert!(refusal.is_none(), "{name} on {origin:?}");
                    }
                    Access::DevOnly(_) => {
                        assert_eq!(refusal.expect("refused").content["refusal"], "devOnly", "{name}");
                    }
                    _ => assert_eq!(
                        refusal.expect("refused").content["refusal"],
                        "refusedInBackground",
                        "{name} on {origin:?}"
                    ),
                }
            }
        }
    }

    /// `outcome_unknown` says the copy may be running: reading it as a problem would trip the repeat
    /// breaker and have a model tell someone it never started.
    #[test]
    fn an_unknown_outcome_carries_no_problem_key() {
        let content = error_content(&ToolError::outcome_unknown("Check the queue."));
        assert_eq!(content, json!({ "outcomeUnknown": true, "detail": "Check the queue." }));
        assert!(!result(content).reports_a_problem());
    }

    /// `data` is the contract: `blockingDialog` was lost when this flattened to a sentence.
    #[test]
    fn an_error_keeps_its_typed_data_for_the_model() {
        let error = ToolError::invalid_params("A dialog is open.").with_data(json!({ "blockingDialog": "about" }));
        assert_eq!(
            error_content(&error),
            json!({ "problem": "A dialog is open.", "data": { "blockingDialog": "about" } })
        );
        assert_eq!(
            error_content(&ToolError::internal("Nope.")),
            json!({ "problem": "Nope." })
        );
    }

    /// A question Stop took down ends in the words of any call Stop ended.
    #[test]
    fn a_cancelled_question_reads_as_a_stopped_call() {
        let error = ToolError::invalid_params("Stopped.").with_data(json!({ "refusal": "cancelled" }));
        assert_eq!(
            error_content(&error),
            json!({ "miss": "cancelled", "detail": "Stopped." })
        );
    }

    /// `outcome_of` and the breaker read a decline at `data.refusal`.
    #[test]
    fn a_decline_stays_a_typed_problem() {
        let error = ToolError::invalid_params("No.").with_data(json!({ "refusal": "declined" }));
        let content = error_content(&error);
        assert_eq!(content["data"]["refusal"], "declined");
        assert!(result(content).reports_a_problem());
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
        for tool in [
            ToolId::ProposeRenamePlan,
            ToolId::ProposeSuggestions,
            ToolId::ProposeServer,
        ] {
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
    fn a_name_outside_the_table_is_refused_on_every_origin() {
        let tool = ToolId::from_wire_name("definitely_not_a_tool");
        assert!(!tool.is_known());
        for origin in ALL_ORIGINS {
            let refusal = refuse_unavailable("call-1", &tool, origin).expect("refused");
            assert_eq!(refusal.content["available"], false);
            assert_eq!(refusal.content["requested"], "definitely_not_a_tool");
            assert!(refusal.content.get("refusal").is_none(), "no class to refuse by");
        }
    }

    #[test]
    fn a_dry_run_owes_no_review_verdict_and_a_multi_rename_problem_owes_one() {
        // `outcome_of` reads a dry run as an answer, so the stamp must leave it alone.
        let mut dry_run = result(json!({ "dryRun": true, "rows": [], "total": 0 }));
        ensure_review_verdict(&ToolId::MultiRename, &mut dry_run);
        assert!(dry_run.content.get("readyForReview").is_none());

        let mut problem = result(json!({ "problem": "Nothing is ready." }));
        ensure_review_verdict(&ToolId::MultiRename, &mut problem);
        assert_eq!(problem.content["readyForReview"], false);
    }

    #[test]
    fn connect_volume_waits_for_a_turn_the_user_started() {
        for origin in [TurnOrigin::Wake, TurnOrigin::Outcomes] {
            let refusal = refuse_unavailable("call-1", &ToolId::ConnectVolume, origin).expect("refused");
            assert_eq!(refusal.content["refusal"], "refusedInBackground");
        }
        for origin in [TurnOrigin::Text, TurnOrigin::Resume] {
            assert!(refuse_unavailable("call-1", &ToolId::ConnectVolume, origin).is_none());
        }
    }

    /// A provider's raw name parses to its table entry, and the gate reads the turn it's on: `delete`
    /// opens a dialog on a turn the user started and refuses on a wake, with no dispatch.
    #[tokio::test]
    async fn raw_provider_name_delete_is_parsed_and_judged_by_its_turn() {
        let fake = FakeAgentLlm::script(vec![ScriptedTurn::CallRawTool(
            "delete".into(),
            json!({ "pane": "left" }),
        )]);
        let stream = fake
            .respond(
                "sys",
                &[],
                &[],
                crate::agent::llm::CallSettings::default(),
                CancellationToken::new(),
            )
            .await
            .expect("turn starts");
        let deltas: Vec<AgentDelta> = stream.map(|d| d.expect("no stream error")).collect().await;

        let AgentDelta::End { message, .. } = deltas.last().expect("an End delta") else {
            panic!("expected End");
        };
        let crate::agent::llm::types::AgentPart::ToolCall(call) = &message.parts[0] else {
            panic!("expected a tool call part");
        };
        assert_eq!(call.tool, ToolId::Delete);

        assert!(refuse_unavailable(&call.call_id, &call.tool, TurnOrigin::Text).is_none());
        let refusal = refuse_unavailable(&call.call_id, &call.tool, TurnOrigin::Wake).expect("refused on a wake");
        assert_eq!(refusal.content["refusal"], "refusedInBackground");
        assert_eq!(refusal.content["requested"], "delete");
    }
}

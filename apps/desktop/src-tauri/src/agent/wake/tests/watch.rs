//! What a wake's turn decided, read off the tool calls that went past.
//!
//! The proposal count is what decides whether the user is interrupted at all, so the tests
//! here are about not lying in either direction: no toast for a wake that proposed nothing or
//! whose proposal was refused, and a toast for the `propose_suggestions` half — the one a
//! count taken off the streamed events would have missed entirely.

use futures_util::future::BoxFuture;

use super::super::WakeToolWatch;
use crate::agent::chat::runtime::{ToolDispatchOutcome, ToolDispatcher};
use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ToolId};

/// A dispatcher that answers with whatever content it was built with, whatever it is asked.
struct Answers(serde_json::Value);

impl ToolDispatcher for Answers {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        Box::pin(async move {
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: self.0.clone(),
                    elided: false,
                },
                proposal: None,
            }
        })
    }

    fn revoke_evidence(&self, _call_ids: &[String]) {}
}

fn call(tool: ToolId) -> AgentToolCall {
    AgentToolCall {
        call_id: "c1".to_string(),
        tool,
        arguments: serde_json::json!({}),
        reasoning: None,
    }
}

/// ⚠️ **The count comes from the CALLS, not from the streamed events.** Only
/// `propose_rename_plan` streams a `ProposalReady`; a `propose_suggestions` group — the move,
/// copy, trash, delete, compress, and extract half of what the agent can offer — streams
/// nothing at all. Counting events read zero for most of what a wake actually stages, and the
/// user was never told about any of it.
#[tokio::test]
async fn a_suggestion_group_counts_even_though_it_streams_nothing() {
    let inner = Answers(serde_json::json!({ "staged": 3 }));
    let watch = WakeToolWatch::new(&inner);

    watch.dispatch(&call(ToolId::ProposeSuggestions)).await;

    assert_eq!(watch.proposals(), 1);
    assert!(!watch.stayed_quiet());
}

/// Both proposal tools count, and nothing else does: a wake that only looked around has
/// nothing waiting for the user, so interrupting them would be a lie.
#[tokio::test]
async fn only_the_tools_that_stage_something_count() {
    let inner = Answers(serde_json::json!({ "ok": true }));
    let watch = WakeToolWatch::new(&inner);

    watch.dispatch(&call(ToolId::ListDir)).await;
    watch.dispatch(&call(ToolId::FolderImportance)).await;
    assert_eq!(watch.proposals(), 0);

    watch.dispatch(&call(ToolId::ProposeRenamePlan)).await;
    watch.dispatch(&call(ToolId::ProposeSuggestions)).await;
    assert_eq!(watch.proposals(), 2);
}

/// A refused proposal leaves nothing for the user to review, so a toast about it would send
/// them to an empty list. The refusal is read from OUR OWN typed result keys, never from the
/// model's or the handler's wording.
#[tokio::test]
async fn a_refused_proposal_is_not_something_to_interrupt_over() {
    let refused = Answers(serde_json::json!({ "available": false }));
    let watch = WakeToolWatch::new(&refused);
    watch.dispatch(&call(ToolId::ProposeSuggestions)).await;
    assert_eq!(watch.proposals(), 0);

    let broken = Answers(serde_json::json!({ "problem": "the volume went away" }));
    let watch = WakeToolWatch::new(&broken);
    watch.dispatch(&call(ToolId::ProposeSuggestions)).await;
    assert_eq!(watch.proposals(), 0);
}

/// The quiet signal is typed, and it carries the reason out for the agent's own memory.
#[tokio::test]
async fn the_quiet_signal_is_recorded_with_its_reason() {
    let inner = Answers(serde_json::json!({}));
    let watch = WakeToolWatch::new(&inner);
    let mut quiet = call(ToolId::NothingToSuggest);
    quiet.arguments = serde_json::json!({ "reason": "all of it was build output" });

    watch.dispatch(&quiet).await;

    assert!(watch.stayed_quiet());
    assert_eq!(watch.reason().as_deref(), Some("all of it was build output"));
    assert_eq!(watch.proposals(), 0, "saying nothing is not staging something");
}

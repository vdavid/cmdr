//! The tool-dispatch seam: how a turn executes one tool call, and how it withdraws
//! evidence for results the prompt dropped.
//!
//! The real impl routes through the agent's read-only dispatch view; tests inject a
//! scripted double, so [`run_turn`](super::run_turn) needs no Tauri app.

use futures_util::future::{BoxFuture, FutureExt};
use serde_json::Value;
use tauri::{AppHandle, Runtime};

use super::LOG_TARGET;
use crate::agent::llm::types::{AgentToolCall, AgentToolResult};
use crate::agent::tools::propose::evidence::EvidenceScope;
use crate::agent::tools::propose::rename::RenameProposalSnapshot;

/// How the runtime executes one tool call. The real impl routes through the agent's
/// read-only dispatch view ([`AppHandleDispatcher`]); tests inject a scripted double,
/// so [`run_turn`](super::run_turn) needs no Tauri app.
pub trait ToolDispatcher: Send + Sync {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome>;

    /// Withdraw the named tool results' standing as evidence: assembly dropped them, so the
    /// model never read them, and nothing downstream may cite their contents.
    ///
    /// It rides the DISPATCH seam on purpose — the same seam that delivered those results is
    /// the one that owns app state, and the symmetry (deliver here, revoke here) is what
    /// keeps the two halves from drifting. Default is a no-op, so the seam costs a test
    /// double nothing; the real work is in [`AppHandleDispatcher`].
    fn revoke_evidence(&self, call_ids: &[String]) {
        let _ = call_ids;
    }
}

pub struct ToolDispatchOutcome {
    pub result: AgentToolResult,
    pub proposal: Option<RenameProposalSnapshot>,
}

/// The production dispatcher: every call goes through `agent::tools::view::dispatch`,
/// the read-only choke point (an unknown or write name is refused before `execute_tool`).
pub struct AppHandleDispatcher<R: Runtime> {
    app: AppHandle<R>,
    /// The chat thread this dispatcher serves. Evidence is scoped to it, so facts delivered
    /// here can't back a claim made in another thread.
    scope: EvidenceScope,
}

impl<R: Runtime> AppHandleDispatcher<R> {
    pub fn new(app: AppHandle<R>, conversation_id: i64) -> Self {
        Self {
            app,
            scope: EvidenceScope::Thread(conversation_id),
        }
    }
}

impl<R: Runtime> ToolDispatcher for AppHandleDispatcher<R> {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            let outcome = crate::agent::tools::view::dispatch(&self.app, self.scope, call).await;
            ToolDispatchOutcome {
                result: outcome.result,
                proposal: outcome.proposal,
            }
        }
        .boxed()
    }

    fn revoke_evidence(&self, call_ids: &[String]) {
        if call_ids.is_empty() {
            return;
        }
        log::debug!(
            target: LOG_TARGET,
            "revoking evidence for {} dropped tool result(s): {call_ids:?}",
            call_ids.len()
        );
        crate::agent::tools::propose::rename::revoke_image_facts_evidence(&self.app, call_ids);
    }
}

/// True when a dispatch result is a real answer rather than a refusal or a handler
/// problem. Reads OUR OWN typed result keys (`available` / `problem`), never external
/// wording.
pub fn dispatch_ok(result: &AgentToolResult) -> bool {
    let refused = result.content.get("available") == Some(&Value::Bool(false));
    let problem = result.content.get("problem").is_some();
    !(refused || problem)
}

//! The tool-dispatch seam: how a turn executes one tool call, and how it withdraws
//! evidence for results the prompt dropped.
//!
//! The real impl routes through the agent's read-only dispatch view; tests inject a
//! scripted double, so [`run_turn`](super::run_turn) needs no Tauri app.

use std::path::PathBuf;

use futures_util::future::{BoxFuture, FutureExt};
use serde_json::json;
use tauri::{AppHandle, Runtime};

use super::LOG_TARGET;
use crate::agent::llm::types::{AgentToolCall, AgentToolResult};
pub use crate::agent::tools::view::CallContext;
use crate::tools::propose::evidence::EvidenceScope;
use crate::tools::propose::rename::RenameProposalSnapshot;
use crate::tools::{Access, tool_access};

/// How the runtime executes one tool call. The real impl routes through the agent's
/// read-only dispatch view ([`AppHandleDispatcher`]); tests inject a scripted double,
/// so [`run_turn`](super::run_turn) needs no Tauri app.
pub trait ToolDispatcher: Send + Sync {
    /// Run `call` in `turn`: the turn's origin (the gate reads it) and its Stop. The turn
    /// races every call against Stop itself ([`dispatch_until_stopped`]), so an
    /// implementation needn't watch the token; one that can stop its own work early (a
    /// live `list_dir` read) does, and answers with what it was doing.
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome>;

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

    /// The memory folder, so a tool line can name a `memory_write` path as a place to go
    /// (`tools::line::subject_of`). App state, so it rides this seam beside the evidence. Default
    /// `None`: the path then shows as plain text, which costs a test double nothing. ⚠️ A
    /// wrapping dispatcher must forward it.
    fn memory_root(&self) -> Option<PathBuf> {
        None
    }
}

pub struct ToolDispatchOutcome {
    pub result: AgentToolResult,
    pub proposal: Option<RenameProposalSnapshot>,
}

/// Run one call under the turn's Stop: the ONE place Stop reaches a tool call, whatever the
/// tool.
///
/// A call that hasn't started when Stop lands isn't dispatched. One in flight is abandoned
/// (its future dropped) and answers `cancelled`, so a handler stuck on a slow volume or a
/// hung server never holds the turn. Either way the call keeps a result row, so the thread's
/// call/result pairing stays whole for the next turn.
///
/// ⚠️ **Except a call that may be mid-write.** A `Propose` or `Memory` handler commits on a
/// blocking thread that dropping its future doesn't stop, so abandoning it would record
/// `cancelled` beside a proposal the user can see, or a note the agent will read next turn.
/// Those run to their own bounded end and keep their real answer; the turn stops right after.
pub(super) async fn dispatch_until_stopped(
    dispatcher: &dyn ToolDispatcher,
    call: &AgentToolCall,
    turn: CallContext<'_>,
) -> ToolDispatchOutcome {
    if turn.cancel.is_cancelled() {
        return cancelled(call);
    }
    if !stop_may_abandon(tool_access(call.tool.as_wire_name())) {
        return dispatcher.dispatch(call, turn).await;
    }
    tokio::select! {
        // The call first: one that stops on the token itself answers in the same poll, with
        // something more specific than the generic `cancelled` below.
        biased;
        outcome = dispatcher.dispatch(call, turn) => outcome,
        () = turn.cancel.cancelled() => cancelled(call),
    }
}

/// Whether Stop may drop a call of this access class mid-flight. Exhaustive, so a new class
/// is a decision here.
fn stop_may_abandon(access: Option<Access>) -> bool {
    match access {
        Some(Access::Read | Access::Session | Access::Write) | None => true,
        Some(Access::Propose | Access::Memory) => false,
    }
}

/// The result row of a call Stop ended. `miss` is the token `list_dir` uses for the same
/// end, so a model reads one vocabulary.
fn cancelled(call: &AgentToolCall) -> ToolDispatchOutcome {
    ToolDispatchOutcome {
        result: AgentToolResult {
            call_id: call.call_id.clone(),
            content: json!({
                "miss": "cancelled",
                "detail": "The user stopped this turn before this call finished.",
            }),
            elided: false,
        },
        proposal: None,
    }
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
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            let outcome = crate::agent::tools::view::dispatch(&self.app, self.scope, call, turn).await;
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
        crate::tools::propose::rename::revoke_image_facts_evidence(&self.app, call_ids);
    }

    fn memory_root(&self) -> Option<PathBuf> {
        crate::agent::memory::store_for(&self.app).map(|store| store.root().to_path_buf())
    }
}

/// True when a dispatch result is a real answer rather than a refusal or a handler problem.
///
/// The judgement itself is [`AgentToolResult::reports_a_problem`], on the type, since the tools
/// view asks the same question and importing this module from there would point `agent::tools`
/// back at `agent::chat`.
pub fn dispatch_ok(result: &AgentToolResult) -> bool {
    !result.reports_a_problem()
}

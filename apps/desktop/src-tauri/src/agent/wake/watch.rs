//! What a wake's turn decided, read off its own tool calls.
//!
//! Two questions have to be answered after a wake's turn, and neither can be read from
//! [`TurnResult`](crate::agent::chat::runtime::TurnResult), which carries only
//! `Answered | Failed | Cancelled`:
//!
//! - **Did it say there was nothing worth raising?** Then its thread goes away (`quiet.rs`).
//! - **Did it propose anything?** Then the user has something waiting, and gets told
//!   (`staged.rs`).
//!
//! [`WakeToolWatch`] answers both by watching the wake's own dispatcher. It is a DECORATOR,
//! not a handler: every call goes through unchanged, and all it adds is a typed record of what
//! went past. ❌ Never inferred from the model's wording — `error-string-match` forbids
//! classifying control flow by text, and prose breaks on the first copy edit.
//!
//! ⚠️ **The wake path builds one; the rail never does.** That is what leaves
//! `nothing_to_suggest` inert in a user's chat, where a handler that deleted the conversation
//! would take a thread the user was in the middle of.

use std::sync::Mutex;

use futures_util::future::BoxFuture;

use crate::agent::chat::runtime::{ToolDispatchOutcome, ToolDispatcher, dispatch_ok};
use crate::agent::llm::types::{AgentToolCall, ToolId};
use crate::agent::tools::quiet::reason_of;
use crate::ignore_poison::IgnorePoison;

/// A dispatcher wrapper that notices what a wake's turn decided.
pub struct WakeToolWatch<'a> {
    inner: &'a dyn ToolDispatcher,
    seen: Mutex<Seen>,
}

/// What has gone past so far.
#[derive(Default)]
struct Seen {
    /// `Some(reason)` once `nothing_to_suggest` has been called. The inner `Option` is the
    /// reason, which the model may leave out.
    quiet: Option<Option<String>>,
    /// Proposal calls that actually landed something.
    proposals: usize,
}

impl<'a> WakeToolWatch<'a> {
    pub fn new(inner: &'a dyn ToolDispatcher) -> Self {
        Self {
            inner,
            seen: Mutex::new(Seen::default()),
        }
    }

    /// True once the model has called `nothing_to_suggest` this turn.
    pub fn stayed_quiet(&self) -> bool {
        self.seen.lock_ignore_poison().quiet.is_some()
    }

    /// The short reason the model gave, if it gave one and it called at all. For the agent's
    /// own memory — never a log line (`quiet.rs`).
    pub fn reason(&self) -> Option<String> {
        self.seen.lock_ignore_poison().quiet.clone().flatten()
    }

    /// How many proposal calls landed something this turn.
    ///
    /// ⚠️ **Counted from the CALLS, ❌ not from the streamed `ProposalReady` events.** Only
    /// `propose_rename_plan` streams one of those (it opens a review dialog); a
    /// `propose_suggestions` group — the move, copy, trash, delete, compress, and extract half
    /// of what the agent can offer — streams nothing at all. Counting events would report zero
    /// for most of what a wake actually stages, and the toast would never fire.
    pub fn proposals(&self) -> usize {
        self.seen.lock_ignore_poison().proposals
    }
}

/// The tools that stage something for the user to approve. Typed, so a new proposal tool is a
/// compile-visible addition here rather than a silently uncounted one.
fn is_proposal(tool: &ToolId) -> bool {
    matches!(tool, ToolId::ProposeSuggestions | ToolId::ProposeRenamePlan)
}

impl ToolDispatcher for WakeToolWatch<'_> {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        if call.tool == ToolId::NothingToSuggest {
            self.seen.lock_ignore_poison().quiet = Some(reason_of(&call.arguments));
        }
        let proposal = is_proposal(&call.tool);
        Box::pin(async move {
            let outcome = self.inner.dispatch(call).await;
            // ⚠️ Only a call that LANDED counts. A refused or failed proposal leaves nothing
            // for the user to review, and a toast about it would send them to an empty list.
            if proposal && dispatch_ok(&outcome.result) {
                let mut seen = self.seen.lock_ignore_poison();
                seen.proposals = seen.proposals.saturating_add(1);
            }
            outcome
        })
    }

    fn revoke_evidence(&self, call_ids: &[String]) {
        self.inner.revoke_evidence(call_ids);
    }
}

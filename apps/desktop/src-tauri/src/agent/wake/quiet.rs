//! What happens when a wake decides there is nothing worth raising.
//!
//! The model says so by calling `nothing_to_suggest` (`agent/tools/quiet.rs`), a pure signal
//! whose handler changes nothing. Acting on it lives HERE, on the wake path, for two reasons:
//! the tool is `Access::Read` and must stay that way, and the rail shares the one
//! `agent_tool_view()`, so a tool that deleted its own conversation would delete a user's
//! thread the moment a confused model reached for it in the rail.
//!
//! ## The seam
//!
//! [`QuietWatch`] wraps the wake's real dispatcher and watches the calls go past. It answers
//! nothing itself and changes nothing: it records that the call happened, typed, and forwards.
//! After the turn, [`discard_quiet_thread`] takes the thread away and keeps what it spent.
//!
//! ⚠️ **The `reason` is not for a log.** It rides out of here for the agent's own memory. Log
//! that a wake was quiet, never what it said: `cmdr.log` ships inside error reports, including
//! the auto-dispatched ones the user never previews, and the redactor is path-shaped, so a
//! sentence naming which of the user's folders were boring would travel intact.

use std::sync::Mutex;

use futures_util::future::BoxFuture;
use rusqlite::Connection;

use crate::agent::chat::runtime::{ToolDispatchOutcome, ToolDispatcher};
use crate::agent::llm::types::{AgentToolCall, ToolId};
use crate::agent::tools::quiet::reason_of;
use crate::ignore_poison::IgnorePoison;

const LOG_TARGET: &str = "agent::wake";

/// A dispatcher wrapper that notices the wake saying it has nothing to raise.
///
/// It is a decorator, not a handler: every call goes to the wrapped dispatcher unchanged, and
/// the only thing this adds is a record of whether [`ToolId::NothingToSuggest`] came past. The
/// wake path builds one; the rail never does, which is what makes the tool inert there.
pub struct QuietWatch<'a> {
    inner: &'a dyn ToolDispatcher,
    /// `Some(reason)` once the call has been seen. The inner `Option` is the reason, which the
    /// model may leave out.
    seen: Mutex<Option<Option<String>>>,
}

impl<'a> QuietWatch<'a> {
    pub fn new(inner: &'a dyn ToolDispatcher) -> Self {
        Self {
            inner,
            seen: Mutex::new(None),
        }
    }

    /// True once the model has called `nothing_to_suggest` this turn.
    pub fn stayed_quiet(&self) -> bool {
        self.seen.lock_ignore_poison().is_some()
    }

    /// The short reason the model gave, if it gave one and it called at all. For the agent's
    /// own memory — never a log line (see the module docs).
    pub fn reason(&self) -> Option<String> {
        self.seen.lock_ignore_poison().clone().flatten()
    }
}

impl ToolDispatcher for QuietWatch<'_> {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        if call.tool == ToolId::NothingToSuggest {
            // Typed, never read out of the model's wording: `error-string-match` forbids
            // classifying control flow by text, and prose breaks on the first copy edit.
            *self.seen.lock_ignore_poison() = Some(reason_of(&call.arguments));
        }
        self.inner.dispatch(call)
    }

    fn revoke_evidence(&self, call_ids: &[String]) {
        self.inner.revoke_evidence(call_ids);
    }
}

/// Take away the thread a quiet wake opened, keeping what it spent.
///
/// A failure is logged and swallowed: the wake already happened, and a thread that outlives
/// its delete is a cosmetic problem, while losing the cost record is not. So the fold-then-
/// delete is all-or-nothing (`discard_conversation_keeping_cost` runs both in one
/// transaction), and a failure leaves the thread standing WITH its cost rather than gone
/// without them.
pub fn discard_quiet_thread(conn: &Connection, conversation_id: i64) {
    if let Err(e) = crate::agent::store::discard_conversation_keeping_cost(conn, conversation_id) {
        log::warn!(target: LOG_TARGET, "a quiet wake's thread stayed behind: {e}");
    }
}

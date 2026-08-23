//! What happens when a wake decides there is nothing worth raising.
//!
//! The model says so by calling `nothing_to_suggest` (`agent/tools/quiet.rs`), a pure signal
//! whose handler changes nothing. Acting on it lives HERE, on the wake path, for two reasons:
//! the tool is `Access::Read` and must stay that way, and the rail shares the one
//! `agent_tool_view()`, so a tool that deleted its own conversation would delete a user's
//! thread the moment a confused model reached for it in the rail.
//!
//! Noticing the call is `watch.rs`'s job; this is what follows from it.
//!
//! ⚠️ **The `reason` the model gives is not for a log.** It rides out of the watch for the
//! agent's own memory. Log that a wake was quiet, never what it said: `cmdr.log` ships inside
//! error reports, including the auto-dispatched ones the user never previews, and the redactor
//! is path-shaped, so a sentence naming which of the user's folders were boring would travel
//! intact.

use rusqlite::Connection;

const LOG_TARGET: &str = "agent::wake";

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

//! Which calls of one turn repeat an earlier one.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::LineRef;
use crate::agent::llm::types::AgentToolCall;

/// For one turn's calls, in order (each with the id of the assistant row that made it): every
/// call identical to an EARLIER one, mapped to the FIRST identical call of the turn. "Identical"
/// is [`AgentToolCall::identity_key`], the repeat breaker's own key, so the mark and the breaker
/// can't disagree; call ids play no part, since they collide across rounds.
pub fn repeat_marks(calls: &[(i64, &AgentToolCall)]) -> HashMap<LineRef, LineRef> {
    let mut first_by_key: HashMap<String, LineRef> = HashMap::new();
    let mut marks = HashMap::new();
    for (message_id, call) in calls {
        let line = LineRef {
            message_id: *message_id,
            call_id: call.call_id.clone(),
        };
        match first_by_key.entry(call.identity_key()) {
            // A row naming one call id twice can't tell its two lines apart, so it never marks
            // a line as a repeat of itself.
            Entry::Occupied(first) if *first.get() != line => {
                marks.insert(line, first.get().clone());
            }
            Entry::Occupied(_) => {}
            Entry::Vacant(slot) => {
                slot.insert(line);
            }
        }
    }
    marks
}

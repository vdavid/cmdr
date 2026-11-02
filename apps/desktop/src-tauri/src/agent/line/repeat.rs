//! Which calls of one turn repeat an earlier one.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::LineRef;
use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ToolId};
use crate::tools::{Access, tool_access};

/// Whether this answer changed what's around the model, so the same call could now answer differently:
/// any non-read that came back without a problem. The turn's loop guard, repeat breaker, and repeat
/// marks all start over from here, or "go up three levels" would end as going in circles.
pub fn moves_the_turn_on(tool: &ToolId, result: &AgentToolResult) -> bool {
    let read = matches!(
        tool_access(tool.as_wire_name()),
        Some(Access::Read | Access::ReadContents) | None
    );
    !read && !result.reports_a_problem()
}

/// One call of a turn, for [`repeat_marks`].
pub struct TurnCall<'a> {
    /// The assistant row that made it.
    pub message_id: i64,
    pub call: &'a AgentToolCall,
    /// [`moves_the_turn_on`], false while its result isn't known.
    pub moved_on: bool,
}

/// For one turn's calls, in order: every call identical to an EARLIER one since the turn last moved
/// on, mapped to the FIRST of them. "Identical" is [`AgentToolCall::identity_key`], the repeat breaker's
/// own key; call ids play no part, since they collide across rounds.
///
/// A row's move on takes effect at the next row, unlike the breaker's, since the rail hears a round's
/// marks before any of its results, and a reloaded thread has to draw the same line.
pub fn repeat_marks(calls: &[TurnCall<'_>]) -> HashMap<LineRef, LineRef> {
    let mut first_by_key: HashMap<String, LineRef> = HashMap::new();
    let mut marks = HashMap::new();
    let mut row = None;
    let mut moved_on = false;
    for turn_call in calls {
        let (message_id, call) = (&turn_call.message_id, turn_call.call);
        if row != Some(*message_id) {
            if moved_on {
                first_by_key.clear();
            }
            row = Some(*message_id);
            moved_on = false;
        }
        moved_on |= turn_call.moved_on;
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

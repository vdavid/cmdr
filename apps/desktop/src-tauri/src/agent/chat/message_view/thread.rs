//! A history page's projection, with its turns in view.
//!
//! What a tool line says is turn-scoped (its result sits on a later row, a repeat points at an
//! earlier round, a turn's stats ride its last assistant row), while the history pages by rows.
//! So the page arrives with the rows around it that complete its first and last turns
//! (`store::turn_context`), and everything is derived over that window by the same pure
//! functions the live stream runs (`agent::line`), so a reloaded line says what the live one did.
//!
//! **Pairing.** Results persist in call order (`runtime/turn.rs`), so the k-th call with an id in
//! an assistant row pairs with the k-th result carrying that id after it, before the next
//! assistant or user row. Call ids collide across rounds, so pairing never reaches past the
//! next round.

use std::path::Path;

use serde::Serialize;

use super::{CallFacts, MessageView, RowFacts, project_row};
use crate::agent::line::{LineRef, ToolPreview, outcome_of, preview_of, repeat_marks, subject_of};
use crate::agent::llm::types::{AgentPart, AgentRole, AgentToolCall, AgentToolResult};
use crate::agent::store::{ConversationEvent, StoredContent, StoredMessage, TurnContext, TurnStats};

/// What the page's projection needs beyond the rows.
pub struct PageContext<'a> {
    /// The running turn's seq high-water mark (`chat::cancel::turn_in_flight`): the
    /// turn holding rows after it is still running.
    pub in_flight_since_seq: Option<i64>,
    /// The memory folder, so a memory line's path can be a place to go.
    pub memory_root: Option<&'a Path>,
}

/// Project a page of rows, reading `context`'s rows around it for the turns it cuts. Only the
/// page's own rows come back.
pub fn project_page(context: TurnContext, page: Vec<StoredMessage>, ctx: PageContext<'_>) -> Vec<MessageView> {
    let page_start = context.before.len();
    let page_end = page_start + page.len();
    let mut rows = context.before;
    rows.extend(page);
    rows.extend(context.after);

    let facts = thread_facts(&rows, &ctx);
    rows.into_iter()
        .zip(facts)
        .enumerate()
        .filter(|(index, _)| (page_start..page_end).contains(index))
        .map(|(_, (row, facts))| project_row(row, facts))
        .collect()
}

/// One [`RowFacts`] per row of `rows`.
fn thread_facts(rows: &[StoredMessage], ctx: &PageContext<'_>) -> Vec<RowFacts> {
    let mut facts: Vec<RowFacts> = rows.iter().map(|_| RowFacts::default()).collect();
    for turn in turns(rows) {
        let turn_rows = &rows[turn.clone()];
        let turn_id = turn_rows
            .first()
            .filter(|row| is_role(row, AgentRole::User))
            .map(|row| row.id);
        let last_assistant = turn_rows.iter().rposition(|row| is_role(row, AgentRole::Assistant));

        // Results per call, by row, and the turn's calls in order for the repeat marks.
        let rounds: Vec<Round<'_>> = turn_rows
            .iter()
            .enumerate()
            .filter(|(_, row)| is_role(row, AgentRole::Assistant))
            .map(|(offset, row)| {
                let calls = calls_of(row);
                let results = pair_results(turn_rows, offset, &calls);
                Round { offset, calls, results }
            })
            .collect();
        let ordered: Vec<(i64, &AgentToolCall)> = rounds
            .iter()
            .flat_map(|round| round.calls.iter().map(|call| (turn_rows[round.offset].id, *call)))
            .collect();
        let marks = repeat_marks(&ordered);

        for (offset, row) in turn_rows.iter().enumerate() {
            let row_facts = &mut facts[turn.start + offset];
            row_facts.turn_id = turn_id;
            row_facts.during_turn =
                matches!(row.content, StoredContent::Event(_)) && last_assistant.is_some_and(|last| offset < last);
        }
        for Round { offset, calls, results } in rounds {
            let message_id = turn_rows[offset].id;
            facts[turn.start + offset].calls = calls
                .iter()
                .zip(results)
                .map(|(call, result)| CallFacts {
                    subject: subject_of(call, result.map(|result| &result.content), ctx.memory_root),
                    outcome: result.map(|result| outcome_of(&call.tool, result)),
                    repeat_of: marks
                        .get(&LineRef {
                            message_id,
                            call_id: call.call_id.clone(),
                        })
                        .cloned(),
                })
                .collect();
        }
        if let (Some(_), Some(last)) = (turn_id, last_assistant) {
            facts[turn.start + last].turn_stats = turn_stats(turn_rows, ctx.in_flight_since_seq);
        }
    }
    facts
}

/// What an expanded tool line shows: the curated preview, plus the raw result pretty-printed for
/// Copy. The raw result is the one door to what a call returned (it can hold file text and EXIF
/// incl. GPS), so it crosses only on the user's expand, and ❌ is never logged.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultDetailView {
    pub preview: ToolPreview,
    pub raw_json: String,
}

/// The expanded line for the call `call_id` of `rows[0]`, an assistant row followed by the rest
/// of its turn (`store::row_and_rest_of_turn`). `occurrence` picks among calls the row names with
/// the same id (0 for the first), since a provider can repeat one within a round. Pairs the way
/// the page does, so it reads the same result the line's outcome came from. `None` when the row
/// made no such call, or it has no result row.
pub fn tool_result_detail(rows: Vec<StoredMessage>, call_id: &str, occurrence: usize) -> Option<ToolResultDetailView> {
    let row = rows.first().filter(|row| is_role(row, AgentRole::Assistant))?;
    let calls = calls_of(row);
    let results = pair_results(&rows, 0, &calls);
    let (call, result) = calls
        .iter()
        .zip(results)
        .filter(|(call, _)| call.call_id == call_id)
        .nth(occurrence)?;
    let result = result?;
    Some(ToolResultDetailView {
        preview: preview_of(&call.tool, result),
        raw_json: serde_json::to_string_pretty(&result.content).unwrap_or_default(),
    })
}

/// One assistant row of a turn (at `offset` in it): its calls, and each one's result.
struct Round<'a> {
    offset: usize,
    calls: Vec<&'a AgentToolCall>,
    results: Vec<Option<&'a AgentToolResult>>,
}

/// The thread's turns as index ranges: each starts at a user row and runs to the next one. Rows
/// before the first user row form a range of their own, with no turn.
fn turns(rows: &[StoredMessage]) -> Vec<std::ops::Range<usize>> {
    let mut starts: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| is_role(row, AgentRole::User))
        .map(|(index, _)| index)
        .collect();
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| start..starts.get(i + 1).copied().unwrap_or(rows.len()))
        .filter(|range| !range.is_empty())
        .collect()
}

/// What a turn spent: its `TurnFinished` row's stats when it has one. Without one, a turn written
/// before stats were recorded gets them rebuilt from its rows (every result row a call, tokens
/// summed per row, no wall time), ❌ except the turn still running, whose stats come from the
/// live terminal event. `None` for a turn that got no result back, as live.
fn turn_stats(turn_rows: &[StoredMessage], in_flight_since_seq: Option<i64>) -> Option<TurnStats> {
    let recorded = turn_rows.iter().rev().find_map(|row| match &row.content {
        StoredContent::Event(ConversationEvent::TurnFinished { stats }) => Some(*stats),
        _ => None,
    });
    if recorded.is_some() {
        return recorded;
    }
    if in_flight_since_seq.is_some_and(|mark| turn_rows.iter().any(|row| row.seq > mark)) {
        return None;
    }
    let mut stats = TurnStats::default();
    for row in turn_rows {
        let StoredContent::Message { role, parts } = &row.content else {
            continue;
        };
        match role {
            AgentRole::Assistant => {
                stats.prompt_tokens = stats.prompt_tokens.saturating_add(row.prompt_tokens.unwrap_or(0));
                stats.completion_tokens = stats
                    .completion_tokens
                    .saturating_add(row.completion_tokens.unwrap_or(0));
            }
            AgentRole::Tool => {
                let results = parts
                    .iter()
                    .filter(|part| matches!(part, AgentPart::ToolResult(_)))
                    .count();
                stats.tool_calls = stats
                    .tool_calls
                    .saturating_add(u32::try_from(results).unwrap_or(u32::MAX));
            }
            AgentRole::System | AgentRole::User => {}
        }
    }
    (stats.tool_calls > 0).then_some(stats)
}

/// For the assistant row at `rows[at]`, each call's result (aligned with `calls`): the k-th call
/// with an id takes the k-th result with that id from the tool rows after it, up to the next
/// assistant or user row.
fn pair_results<'a>(
    rows: &'a [StoredMessage],
    at: usize,
    calls: &[&AgentToolCall],
) -> Vec<Option<&'a AgentToolResult>> {
    let mut results: Vec<Option<&AgentToolResult>> = vec![None; calls.len()];
    let following = rows.iter().skip(at + 1);
    let round = following.take_while(|row| !is_role(row, AgentRole::Assistant) && !is_role(row, AgentRole::User));
    for row in round {
        let StoredContent::Message {
            role: AgentRole::Tool,
            parts,
        } = &row.content
        else {
            continue;
        };
        for part in parts {
            let AgentPart::ToolResult(result) = part else {
                continue;
            };
            let slot = calls
                .iter()
                .zip(results.iter_mut())
                .find(|(call, slot)| slot.is_none() && call.call_id == result.call_id);
            if let Some((_, slot)) = slot {
                *slot = Some(result);
            }
        }
    }
    results
}

/// A row's tool calls, in its part order.
fn calls_of(row: &StoredMessage) -> Vec<&AgentToolCall> {
    match &row.content {
        StoredContent::Message { parts, .. } => parts
            .iter()
            .filter_map(|part| match part {
                AgentPart::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect(),
        StoredContent::Event(_) => Vec::new(),
    }
}

fn is_role(row: &StoredMessage, wanted: AgentRole) -> bool {
    matches!(&row.content, StoredContent::Message { role, .. } if *role == wanted)
}

#[cfg(test)]
mod tests;

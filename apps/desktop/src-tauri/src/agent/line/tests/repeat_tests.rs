//! `repeat_marks`: identity by `LineRef`, sameness by the breaker's key.

use serde_json::json;

use super::call_with_id;
use crate::agent::line::{LineRef, TurnCall, moves_the_turn_on, repeat_marks};
use crate::agent::llm::types::{AgentToolResult, ToolId};

fn in_row(message_id: i64, call: &crate::agent::llm::types::AgentToolCall) -> TurnCall<'_> {
    TurnCall {
        message_id,
        call,
        moved_on: false,
    }
}

fn line(message_id: i64, call_id: &str) -> LineRef {
    LineRef {
        message_id,
        call_id: call_id.into(),
    }
}

#[test]
fn repeats_across_rounds_that_reuse_call_ids_mark_the_first_identical_call() {
    // Two rounds, both minting `fake-call-0` (the fake, and Gemini, reset ids per response).
    let round_one = [
        call_with_id("fake-call-0", ToolId::ListVolumes, json!({})),
        call_with_id("fake-call-1", ToolId::ListDir, json!({ "path": "/", "limit": 10 })),
    ];
    let round_two = [
        call_with_id("fake-call-0", ToolId::ListDir, json!({ "limit": 10, "path": "/" })),
        call_with_id("fake-call-1", ToolId::ListVolumes, json!({})),
        call_with_id("fake-call-2", ToolId::ListVolumes, json!({})),
    ];
    let calls: Vec<TurnCall<'_>> = round_one
        .iter()
        .map(|call| in_row(10, call))
        .chain(round_two.iter().map(|call| in_row(12, call)))
        .collect();

    let marks = repeat_marks(&calls);

    assert_eq!(marks.len(), 3);
    assert_eq!(
        marks.get(&line(12, "fake-call-0")),
        Some(&line(10, "fake-call-1")),
        "argument key order doesn't matter"
    );
    assert_eq!(marks.get(&line(12, "fake-call-1")), Some(&line(10, "fake-call-0")));
    assert_eq!(
        marks.get(&line(12, "fake-call-2")),
        Some(&line(10, "fake-call-0")),
        "the FIRST identical call wins, never the previous one"
    );
    assert!(!marks.contains_key(&line(10, "fake-call-0")));
}

#[test]
fn different_arguments_or_tools_are_not_repeats() {
    let calls = [
        call_with_id("a", ToolId::ListDir, json!({ "path": "/", "offset": 0 })),
        call_with_id("b", ToolId::ListDir, json!({ "path": "/", "offset": 50 })),
        call_with_id("c", ToolId::FolderImportance, json!({ "path": "/" })),
    ];
    let pairs: Vec<TurnCall<'_>> = calls.iter().map(|call| in_row(1, call)).collect();
    assert!(repeat_marks(&pairs).is_empty());
}

#[test]
fn a_row_naming_one_call_id_twice_never_marks_a_line_as_its_own_repeat() {
    let calls = [
        call_with_id("dup", ToolId::ListVolumes, json!({})),
        call_with_id("dup", ToolId::ListVolumes, json!({})),
    ];
    let pairs: Vec<TurnCall<'_>> = calls.iter().map(|call| in_row(1, call)).collect();
    assert!(repeat_marks(&pairs).is_empty());
}

/// "Go up three levels" is three steps, not "Same as above" twice.
#[test]
fn a_call_that_moved_the_turn_on_starts_the_marks_over_at_the_next_row() {
    let up = [
        call_with_id("a", ToolId::NavToParent, json!({})),
        call_with_id("b", ToolId::NavToParent, json!({})),
        call_with_id("c", ToolId::ListVolumes, json!({})),
        call_with_id("d", ToolId::ListVolumes, json!({})),
    ];
    let row = |message_id, index: usize, moved_on| TurnCall {
        message_id,
        call: &up[index],
        moved_on,
    };
    let marks = repeat_marks(&[row(1, 0, true), row(2, 1, true), row(3, 2, false), row(4, 3, false)]);
    assert!(!marks.contains_key(&line(2, "b")), "the first step moved the turn on");
    assert_eq!(
        marks.get(&line(4, "d")),
        Some(&line(3, "c")),
        "a read moves nothing, so a repeat of it is still one"
    );
}

/// The rail hears a round's marks before its results, so a reload can't start over inside a row.
#[test]
fn moving_on_never_starts_the_marks_over_inside_a_row() {
    let twice = [
        call_with_id("a", ToolId::NavToParent, json!({})),
        call_with_id("b", ToolId::NavToParent, json!({})),
    ];
    let row = |index: usize, moved_on| TurnCall {
        message_id: 1,
        call: &twice[index],
        moved_on,
    };
    let marks = repeat_marks(&[row(0, true), row(1, true)]);
    assert_eq!(marks.get(&line(1, "b")), Some(&line(1, "a")));
}

#[test]
fn only_a_non_read_that_came_back_without_a_problem_moves_the_turn_on() {
    let result = |content| AgentToolResult {
        call_id: "c".into(),
        content,
        elided: false,
    };
    let ok = result(json!({ "ok": true }));
    assert!(moves_the_turn_on(&ToolId::NavToParent, &ok));
    assert!(moves_the_turn_on(&ToolId::MemoryWrite, &ok));
    assert!(moves_the_turn_on(
        &ToolId::Copy,
        &result(json!({ "awaitingPerson": true }))
    ));
    assert!(moves_the_turn_on(
        &ToolId::Copy,
        &result(json!({ "outcomeUnknown": true })),
    ));
    assert!(!moves_the_turn_on(&ToolId::ListDir, &ok));
    assert!(!moves_the_turn_on(&ToolId::InspectFile, &ok));
    assert!(!moves_the_turn_on(&ToolId::Copy, &result(json!({ "problem": "no" }))));
    assert!(!moves_the_turn_on(
        &ToolId::Copy,
        &result(json!({ "available": false })),
    ));
    assert!(!moves_the_turn_on(&ToolId::Unrecognized("x".into()), &ok));
}

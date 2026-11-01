//! `repeat_marks`: identity by `LineRef`, sameness by the breaker's key.

use serde_json::json;

use super::call_with_id;
use crate::agent::line::{LineRef, repeat_marks};
use crate::agent::llm::types::ToolId;

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
    let calls: Vec<(i64, &_)> = round_one
        .iter()
        .map(|call| (10, call))
        .chain(round_two.iter().map(|call| (12, call)))
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
    let pairs: Vec<(i64, &_)> = calls.iter().map(|call| (1, call)).collect();
    assert!(repeat_marks(&pairs).is_empty());
}

#[test]
fn a_row_naming_one_call_id_twice_never_marks_a_line_as_its_own_repeat() {
    let calls = [
        call_with_id("dup", ToolId::ListVolumes, json!({})),
        call_with_id("dup", ToolId::ListVolumes, json!({})),
    ];
    let pairs: Vec<(i64, &_)> = calls.iter().map(|call| (1, call)).collect();
    assert!(repeat_marks(&pairs).is_empty());
}

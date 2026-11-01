//! Table tests per tool: arguments to subject, result to refined subject, result to outcome,
//! and result to preview. Result fixtures are built from the handler's real result struct where
//! one is reachable from here, and otherwise copy the handler's serde field names.

mod fixtures;
mod outcome_tests;
mod preview_tests;
mod repeat_tests;
mod subject_tests;

use serde_json::Value;

use crate::agent::llm::types::{AgentToolCall, AgentToolResult, ToolId};

fn call(tool: ToolId, arguments: Value) -> AgentToolCall {
    call_with_id("call-1", tool, arguments)
}

fn call_with_id(call_id: &str, tool: ToolId, arguments: Value) -> AgentToolCall {
    AgentToolCall {
        call_id: call_id.to_string(),
        tool,
        arguments,
        reasoning: None,
    }
}

fn result(content: Value) -> AgentToolResult {
    AgentToolResult {
        call_id: "call-1".to_string(),
        content,
        elided: false,
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("a tool result serializes")
}

/// The table's `Custom` entries and the hand arms are the same set, in all three functions, and in
/// both directions: a `Custom` tool never reaches a generic arm, and a hand arm never sits behind a
/// tool the table sends through the generic one. Adding a tool needs no arm here unless it's `Custom`.
#[test]
fn only_custom_tools_have_hand_arms_and_only_other_tools_reach_the_generic_ones() {
    use crate::agent::line::Arm;
    use crate::agent::line::{outcome::hand_answer, preview::hand_rows, subject::hand_subject};
    use crate::tools::SubjectSpec;

    for tool in ToolId::KNOWN {
        let custom = tool.subject_spec() == Some(SubjectSpec::Custom);
        let name = tool.as_wire_name().to_string();
        let empty = Value::Object(serde_json::Map::new());
        let as_call = call(tool.clone(), empty.clone());
        let subject = matches!(hand_subject(&as_call, None, None), Arm::Hand(_));
        let outcome = matches!(hand_answer(&tool, &empty), Arm::Hand(_));
        let preview = matches!(hand_rows(&tool, &empty), Arm::Hand(_));
        assert_eq!(subject, custom, "{name} in subject_of");
        assert_eq!(outcome, custom, "{name} in outcome_of");
        assert_eq!(preview, custom, "{name} in preview_of");
    }
    // The refusal case is the hand arms' too, so a made-up name never needs the table.
    let made_up = ToolId::Unrecognized("format_drive".into());
    let empty = Value::Object(serde_json::Map::new());
    assert!(matches!(
        hand_subject(&call(made_up.clone(), empty.clone()), None, None),
        Arm::Hand(_)
    ));
    assert!(matches!(hand_answer(&made_up, &empty), Arm::Hand(_)));
    assert!(matches!(hand_rows(&made_up, &empty), Arm::Hand(_)));
}

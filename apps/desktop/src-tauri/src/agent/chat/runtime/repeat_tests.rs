//! The repeat breaker (`repeats.rs`): an identical repeat of a FAILED tool call is never
//! dispatched again, the model gets its own problem back plus "repeating changes nothing",
//! and only failures are remembered, so paging and re-reads still run.
//!
//! Fixtures come from `test_support.rs`, shared with `tests.rs` and the other sibling suites.

use super::test_support::*;
use super::*;
use crate::agent::chat::context::MAX_TOOL_TURNS;

/// Script `count` assistant turns, each emitting the same one tool call.
fn repeating_llm(count: usize, tool: ToolId, arguments: Value) -> ProgrammableLlm {
    ProgrammableLlm::new(
        (0..count)
            .map(|_| Program::Tools {
                calls: vec![(tool.clone(), arguments.clone())],
                usage: AgentUsage::default(),
            })
            .collect(),
    )
}

/// From a live transcript: a `propose_rename_plan` call missing `volumeId` was refused, and
/// the model re-sent the byte-identical call eight times, each one a full provider round
/// trip, until `MAX_TOOL_TURNS` ended the turn. About 90 seconds spent re-sending one broken
/// payload, and from the user's side the agent simply stopped answering.
#[tokio::test]
async fn an_identical_call_that_already_failed_is_never_dispatched_a_third_time() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = repeating_llm(MAX_TOOL_TURNS + 4, ToolId::ListDir, json!({ "limit": 10 }));
    let dispatcher = FailingDispatcher::default();
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("find the penguins")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::RepeatedToolCall));
    assert_eq!(
        dispatcher.dispatched().len(),
        1,
        "the identical call runs once; repeating it can only produce the same problem"
    );
    assert_eq!(
        llm.calls_seen().len(),
        3,
        "three round trips, not eight: run it, tell the model repeating won't work, then stop"
    );
    assert!(
        drain(&mut rx).contains(&AgentChatEvent::Failed {
            kind: AgentErrorKind::RepeatedToolCall,
            detail: None,
        }),
        "the user is told the turn got stuck, not that it ran out of room"
    );
}

#[tokio::test]
async fn the_repeat_hands_back_the_original_problem_and_says_repeating_will_not_help() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = repeating_llm(3, ToolId::ListDir, json!({ "limit": 10 }));
    let (tx, _rx) = unbounded_channel();

    run_turn(
        &llm,
        &FailingDispatcher::default(),
        &conn,
        &[],
        &params(id, Some("find the penguins")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    // The THIRD respond carries what the runtime synthesized for the second (repeated) call:
    // the handler's own words, so the model still reads what to fix, plus a typed flag and a
    // sentence telling it that sending this again changes nothing.
    let third = llm.calls_seen().into_iter().nth(2).expect("a third respond");
    let last = third.last().expect("the transcript ends on a tool row");
    let AgentPart::ToolResult(result) = last.parts.first().expect("a tool result part") else {
        panic!("expected a tool result, got {:?}", last.parts.first());
    };
    assert_eq!(
        result.content["problem"], "list_dir needs path. It takes limit and path.",
        "the actionable half of the original refusal survives"
    );
    assert_eq!(result.content["repeatedCall"], true);
    assert!(
        result.content["guidance"].is_string(),
        "the model is told plainly that repeating this call answers the same way"
    );
}

#[tokio::test]
async fn paging_through_a_failing_tool_is_not_a_repeat() {
    // Same tool, different arguments: the breaker keys on both, so paging (`offset`) and any
    // other varied retry still runs. Only the byte-identical call is held back.
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/", "offset": 0 }))],
            usage: AgentUsage::default(),
        },
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/", "offset": 50 }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["here you go".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let dispatcher = FailingDispatcher::default();
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("keep looking")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "the turn still answers");
    assert_eq!(dispatcher.dispatched().len(), 2, "both pages run");
}

#[tokio::test]
async fn re_reading_a_result_the_prompt_set_aside_is_not_a_repeat() {
    // The system prompt tells the model to call a tool AGAIN when its result was elided to
    // make room (`elided_tool_result` carries a `refetch` hint), and every agent tool is an
    // idempotent local read. Only calls that came back with a PROBLEM are remembered, so an
    // identical re-fetch of something that worked is dispatched every time it's asked for.
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/photos" }))],
            usage: AgentUsage::default(),
        },
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/photos" }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["here you go".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let dispatcher = CountingOkDispatcher::default();
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("what's in photos?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "the turn answers");
    assert_eq!(
        dispatcher.dispatched().len(),
        2,
        "a re-fetch of a result that worked runs again, every time"
    );
}

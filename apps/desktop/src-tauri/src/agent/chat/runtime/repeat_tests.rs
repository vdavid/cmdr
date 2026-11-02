//! The repeat breaker (`repeats.rs`): an identical repeat of a FAILED tool call is never
//! dispatched again, the model gets its own problem back plus "repeating changes nothing",
//! and only failures are remembered, so paging and re-reads still run.
//!
//! Fixtures come from `test_support.rs`, shared with `tests.rs` and the other sibling suites.

use std::sync::Mutex;

use futures_util::future::{BoxFuture, FutureExt};

use super::dispatch::{CallContext, ToolDispatchOutcome, ToolDispatcher};
use super::test_support::*;
use super::*;
use crate::agent::llm::types::{AgentToolCall, AgentToolResult};

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
/// trip, until the round cap ended the turn. About 90 seconds spent re-sending one broken
/// payload, and from the user's side the agent simply stopped answering. Now the third
/// sending trips the loop guard, whose wrap-up call asks for an answer; a model that STILL
/// reaches for the tool ends the turn as stuck.
#[tokio::test]
async fn an_identical_call_that_already_failed_is_never_dispatched_a_third_time() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = repeating_llm(8, ToolId::ListDir, json!({ "limit": 10 }));
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
        4,
        "four round trips, not eight: run it, say repeating won't work, the third sending, then the wrap-up"
    );
    assert!(
        drain(&mut rx).iter().any(|e| matches!(
            e,
            AgentChatEvent::Failed {
                kind: AgentErrorKind::RepeatedToolCall,
                detail: None,
                turn: Some(_),
            }
        )),
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

// ── Moving on: the turn changed something, so a repeat isn't a circle ─────────

/// Records every call it's asked to run, and answers from `answer`, which sees the calls so far.
struct Scripted<F> {
    dispatched: Mutex<Vec<String>>,
    answer: F,
}

impl<F: Fn(&str, &[String]) -> Value + Send + Sync> Scripted<F> {
    fn new(answer: F) -> Self {
        Self {
            dispatched: Mutex::new(Vec::new()),
            answer,
        }
    }

    fn dispatched(&self) -> Vec<String> {
        self.dispatched.lock().expect("lock").clone()
    }
}

impl<F: Fn(&str, &[String]) -> Value + Send + Sync> ToolDispatcher for Scripted<F> {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            let name = call.tool.as_wire_name().to_string();
            let content = {
                let mut dispatched = self.dispatched.lock().expect("lock");
                let content = (self.answer)(&name, &dispatched);
                dispatched.push(name);
                content
            };
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content,
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// One round per call, in order, then an answer.
fn rounds_then_answer(rounds: Vec<(ToolId, Value)>) -> ProgrammableLlm {
    let mut programs: Vec<Program> = rounds
        .into_iter()
        .map(|call| Program::Tools {
            calls: vec![call],
            usage: AgentUsage::default(),
        })
        .collect();
    programs.push(Program::Answer {
        chunks: vec!["Done.".to_string()],
        usage: AgentUsage::default(),
    });
    ProgrammableLlm::new(programs)
}

async fn run(llm: &ProgrammableLlm, dispatcher: &dyn ToolDispatcher) -> TurnResult {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let (tx, _rx) = unbounded_channel();
    run_turn(
        llm,
        dispatcher,
        &conn,
        &[],
        &params(id, Some("go on")),
        &tx,
        &CancellationToken::new(),
    )
    .await
}

/// Every tool result the model read in its last prompt, in order.
fn last_results_seen(llm: &ProgrammableLlm) -> Vec<Value> {
    let last_prompt = llm.calls_seen().into_iter().last().expect("a call");
    last_prompt
        .iter()
        .flat_map(|message| &message.parts)
        .filter_map(|part| match part {
            AgentPart::ToolResult(result) => Some(result.content.clone()),
            _ => None,
        })
        .collect()
}

/// `list_pane_files` and `nav_to_parent` never change identity, so this died on the guard's third
/// sending, ordinary as it is.
#[tokio::test]
async fn go_up_three_levels_finishes() {
    let up = || (ToolId::NavToParent, json!({}));
    let look = || (ToolId::ListPaneFiles, json!({}));
    let llm = rounds_then_answer(vec![up(), look(), up(), look(), up(), look()]);
    let dispatcher = Scripted::new(|_: &str, _: &[String]| json!({ "ok": true }));

    assert!(matches!(run(&llm, &dispatcher).await, TurnResult::Answered { .. }));
    assert_eq!(dispatcher.dispatched().len(), 6, "every step ran");
    let last_prompt = llm.calls_seen().into_iter().last().expect("a call");
    assert!(
        !last_prompt
            .iter()
            .flat_map(|message| &message.parts)
            .any(|part| matches!(part, AgentPart::Text(text) if text.starts_with("[Cmdr:"))),
        "no wrap-up: this was never going in circles"
    );
}

/// A read moves nothing, so this is still a circle: only a change buys a repeat.
#[tokio::test]
async fn the_same_read_with_nothing_changed_still_wraps_up() {
    let look = || (ToolId::ListPaneFiles, json!({}));
    let llm = rounds_then_answer(vec![look(), look(), look()]);
    let dispatcher = Scripted::new(|_: &str, _: &[String]| json!({ "ok": true }));

    assert!(matches!(run(&llm, &dispatcher).await, TurnResult::Answered { .. }));
    let last_prompt = llm.calls_seen().into_iter().last().expect("a call");
    let note = context::wrap_up_note(context::WrapUp::RepeatedCall);
    assert!(
        last_prompt
            .iter()
            .flat_map(|message| &message.parts)
            .any(|part| matches!(part, AgentPart::Text(text) if *text == note)),
        "the third sending of one read is the loop guard's"
    );
}

/// Our own refusal asks for exactly this retry, and the breaker used to answer it with "this answers
/// the same way every time".
#[tokio::test]
async fn copy_refused_then_select_then_copy_runs_again() {
    let copy = || (ToolId::Copy, json!({ "pane": "left" }));
    let llm = rounds_then_answer(vec![
        copy(),
        (ToolId::Select, json!({ "pane": "left", "all": true })),
        copy(),
    ]);
    let dispatcher = Scripted::new(|name: &str, before: &[String]| match name {
        "copy" if !before.iter().any(|call| call == "select") => json!({ "problem": "Nothing is selected." }),
        "copy" => json!({ "awaitingPerson": true, "dialog": "transfer-confirmation" }),
        _ => json!({ "ok": true }),
    });

    assert!(matches!(run(&llm, &dispatcher).await, TurnResult::Answered { .. }));
    assert_eq!(dispatcher.dispatched(), ["copy", "select", "copy"]);
    assert_eq!(last_results_seen(&llm)[2]["awaitingPerson"], true);
}

/// The person answered. Navigating and asking again would put the question back in front of them.
#[tokio::test]
async fn a_declined_ask_stays_declined_across_moving_on() {
    let quit = || (ToolId::Quit, json!({}));
    let llm = rounds_then_answer(vec![quit(), (ToolId::NavToParent, json!({})), quit()]);
    let dispatcher = Scripted::new(|name: &str, _: &[String]| match name {
        "quit" => json!({ "problem": "The person said no, so nothing happened.", "data": { "refusal": "declined" } }),
        _ => json!({ "ok": true }),
    });

    assert!(matches!(run(&llm, &dispatcher).await, TurnResult::Answered { .. }));
    assert_eq!(
        dispatcher.dispatched(),
        ["quit", "nav_to_parent"],
        "they were asked once"
    );
    let results = last_results_seen(&llm);
    assert_eq!(results[2]["data"]["refusal"], "declined");
    assert_eq!(results[2]["repeatedCall"], true);
}

/// The copy may be running, so reading it as a failure would stop the model checking on it.
#[tokio::test]
async fn an_unknown_outcome_never_trips_the_breaker() {
    let copy = || (ToolId::Copy, json!({ "pane": "left" }));
    let llm = rounds_then_answer(vec![copy(), copy()]);
    let dispatcher =
        Scripted::new(|_: &str, _: &[String]| json!({ "outcomeUnknown": true, "detail": "Check the queue." }));

    assert!(matches!(run(&llm, &dispatcher).await, TurnResult::Answered { .. }));
    assert_eq!(dispatcher.dispatched().len(), 2);
    assert!(last_results_seen(&llm)[1].get("repeatedCall").is_none());
}

/// Moving on can't buy a model an endless loop on a turn with no round cap.
#[tokio::test]
async fn toggling_forever_still_ends() {
    use crate::agent::chat::context::LOOP_GUARD_CALL_CAP;
    let llm = rounds_then_answer(vec![(ToolId::ToggleHidden, json!({})); LOOP_GUARD_CALL_CAP + 5]);
    let dispatcher = Scripted::new(|_: &str, _: &[String]| json!({ "ok": true }));

    run(&llm, &dispatcher).await;
    assert_eq!(dispatcher.dispatched().len(), LOOP_GUARD_CALL_CAP);
}

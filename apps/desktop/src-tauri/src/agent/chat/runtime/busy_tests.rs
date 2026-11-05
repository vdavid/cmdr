//! An overloaded provider (`retry.rs`): the call goes again while nothing has streamed, the rail
//! hears each wait, Stop ends a wait at once, and a provider that stays busy ends the turn on its
//! own typed kind. All under `start_paused`, so the backoffs cost no real time.

use std::time::Duration;

use super::test_support::*;
use super::*;
use crate::agent::llm::types::AgentLlmError;

fn overloaded(retry_after: Option<Duration>) -> AgentLlmError {
    AgentLlmError::Overloaded {
        detail: "HTTP 503: This model is currently experiencing high demand.".to_string(),
        retry_after,
    }
}

/// What genai hands us for a 503: a stream that opens, then fails before its first chunk.
fn busy_stream() -> Program {
    Program::ErrorAfterText {
        chunks: vec![],
        error: overloaded(None),
    }
}

fn answer(text: &str) -> Program {
    Program::Answer {
        chunks: vec![text.to_string()],
        usage: AgentUsage::default(),
    }
}

fn busy_events(events: &[AgentChatEvent]) -> Vec<(u32, u32)> {
    events
        .iter()
        .filter_map(|event| match event {
            AgentChatEvent::ProviderBusyRetrying { attempt, attempts } => Some((*attempt, *attempts)),
            _ => None,
        })
        .collect()
}

/// ⚠️ The case the issue was filed for: Gemini's 503 arrives as the stream's first error, after
/// the call opened. Nothing streamed, so the same call goes again, the rail hears that the
/// provider is busy, and the user gets the answer.
#[tokio::test(start_paused = true)]
async fn an_overload_before_anything_streamed_is_retried_and_answers() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![busy_stream(), busy_stream(), answer("here you go")]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hi")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    let events = drain(&mut rx);
    assert_eq!(busy_events(&events), vec![(2, 4), (3, 4)]);
    assert_eq!(
        events
            .iter()
            .filter(|event| **event == AgentChatEvent::AssistantStarted)
            .count(),
        1,
        "one reply, started once, however many tries it took"
    );
    let calls = llm.calls_seen();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0], calls[2], "a retry sends the very same prompt");
}

/// A 503 can also come back before the call opens (genai's non-streaming shape), and it's
/// retried the same way.
#[tokio::test(start_paused = true)]
async fn an_overload_that_stops_the_call_opening_is_retried_too() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::FailsToOpen(overloaded(None)), answer("made it")]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hi")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    assert_eq!(busy_events(&drain(&mut rx)), vec![(2, 4)]);
}

/// Four tries, then the turn ends as busy, with the provider's own sentence under the line, and
/// nothing persisted (the user's message is sent again next time, byte for byte).
#[tokio::test(start_paused = true)]
async fn a_provider_busy_through_every_try_ends_the_turn_as_busy() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        busy_stream(),
        busy_stream(),
        busy_stream(),
        busy_stream(),
        answer("never"),
    ]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hi")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::ProviderBusy));
    assert_eq!(llm.calls_seen().len(), 4, "the first try plus three retries");
    let events = drain(&mut rx);
    assert_eq!(busy_events(&events), vec![(2, 4), (3, 4), (4, 4)]);
    assert!(events.iter().any(|event| matches!(
        event,
        AgentChatEvent::Failed { kind: AgentErrorKind::ProviderBusy, detail: Some(detail), .. }
            if detail.contains("high demand")
    )));
    assert!(
        store::list_messages(&conn, id, 100, 0).expect("list").is_empty(),
        "nothing reached `End`, so nothing is stored"
    );
}

/// ❌ A reply that already showed something is never sent again: the rail would show it twice.
/// An overload after text is the turn's end, at once.
#[tokio::test(start_paused = true)]
async fn an_overload_after_text_streamed_is_not_retried() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::ErrorAfterText {
            chunks: vec!["Let me look".to_string()],
            error: overloaded(None),
        },
        answer("never"),
    ]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hi")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::ProviderBusy));
    assert_eq!(llm.calls_seen().len(), 1);
    assert!(busy_events(&drain(&mut rx)).is_empty());
}

/// Other provider problems aren't retried: only an overload is worth a second try.
#[tokio::test(start_paused = true)]
async fn a_plain_provider_problem_is_not_retried() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::ErrorAfterText {
            chunks: vec![],
            error: AgentLlmError::Provider("HTTP 500: boom".to_string()),
        },
        answer("never"),
    ]);
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hi")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Provider));
    assert_eq!(llm.calls_seen().len(), 1);
}

/// The provider's `Retry-After` sets the wait, and so does its cap: a provider asking for ten
/// minutes holds the turn 30 s, not ten minutes.
#[tokio::test(start_paused = true)]
async fn retry_after_sets_the_wait_up_to_its_cap() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::FailsToOpen(overloaded(Some(Duration::from_secs(20)))),
        Program::FailsToOpen(overloaded(Some(Duration::from_secs(600)))),
        answer("there"),
    ]);
    let (tx, _rx) = unbounded_channel();
    let started = tokio::time::Instant::now();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hi")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    assert_eq!(started.elapsed(), Duration::from_secs(20 + 30));
}

/// ⚠️ Stop ends a backoff at once: a user who gave up on a busy provider mustn't wait out the
/// provider's 30 s, and no further call goes out.
#[tokio::test(start_paused = true)]
async fn stop_during_a_backoff_ends_the_turn_and_sends_nothing_more() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::FailsToOpen(overloaded(Some(Duration::from_secs(30)))),
        answer("never"),
    ]);
    let (tx, _rx) = unbounded_channel();
    let cancel = CancellationToken::new();
    let stopper = cancel.clone();
    tokio::spawn(async move {
        // allowed-test-sleep: virtual time under `start_paused`; Stop lands mid-backoff on purpose
        tokio::time::sleep(Duration::from_secs(1)).await;
        stopper.cancel();
    });
    let started = tokio::time::Instant::now();

    let result = run_turn(&llm, &OkDispatcher, &conn, &[], &params(id, Some("hi")), &tx, &cancel).await;

    assert_eq!(result, TurnResult::Cancelled);
    assert_eq!(llm.calls_seen().len(), 1);
    assert!(started.elapsed() < Duration::from_secs(30), "the wait ended with Stop");
}

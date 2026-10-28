//! Runtime tests: single-flight, the per-message budgets, cancellation at a tool
//! boundary, the crash-safe persistence model, cost metering, an end-to-end fake-driven
//! multi-tool turn, and the typed error surface plus the two gates on what reaches the
//! LLM (the envelope's attachments, and the send gate).
//!
//! Four sibling files hold the rest, all borrowing the fixtures in `test_support.rs`:
//! `repeat_tests.rs` (the repeat breaker), `context_budget_tests.rs` (context drops and
//! the fullness gauge), `model_change_tests.rs` (which model a thread is on), and
//! `wake_tests.rs` (the wake path).

use super::test_support::*;
use super::*;
use crate::agent::chat::context::{MAX_TOOL_TURNS, MAX_WALL_TIME, tool_round_wrap_up_note};
use crate::test_support::wait_until_async;

// ── Single-flight ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_second_send_queues_and_emits_queued() {
    let locks = Arc::new(ConversationLocks::new());
    let (tx, mut rx) = unbounded_channel();

    // Hold the conversation's lock.
    let first = locks.acquire(7, &tx).await;
    assert!(drain(&mut rx).is_empty(), "the first acquire does not queue");

    // A second acquire for the same thread emits Queued and blocks until released.
    let locks2 = locks.clone();
    let tx2 = tx.clone();
    let waiter = tokio::spawn(async move {
        let _guard = locks2.acquire(7, &tx2).await;
    });
    let mut events = Vec::new();
    wait_until_async(Duration::from_secs(2), "the queued send to signal Queued", || {
        events.extend(drain(&mut rx));
        !events.is_empty()
    })
    .await;
    assert_eq!(events, vec![AgentChatEvent::Queued], "the queued send signals Queued");
    assert!(!waiter.is_finished(), "the second send waits for the lock");

    drop(first);
    waiter.await.expect("the queued send proceeds once released");
}

// ── Budgets ───────────────────────────────────────────────────────────────────

fn tool_rounds(count: usize) -> Vec<Program> {
    (0..count)
        .map(|_| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        })
        .collect()
}

/// The stats the turn's one terminal event carried.
fn terminal_stats(events: &[AgentChatEvent]) -> Option<store::TurnStats> {
    events.iter().find_map(|event| match event {
        AgentChatEvent::Done { turn, .. } | AgentChatEvent::Failed { turn, .. } | AgentChatEvent::Stopped { turn } => {
            Some(*turn)
        }
        _ => None,
    })?
}

fn carries_wrap_up_note(message: &AgentMessage) -> bool {
    message.role == AgentRole::User
        && message
            .parts
            .iter()
            .any(|part| matches!(part, AgentPart::Text(text) if *text == tool_round_wrap_up_note()))
}

/// Past the cap on tool rounds the turn asks for an answer instead of dropping it: ONE more
/// call goes out carrying the wrap-up note, and its answer ends the turn. Regression anchor: an
/// eight-round Downloads cleanup (14 calls, 54 s, a tenth of its window used) ended on "hit its
/// limit" with nothing answered, because the cap cut the turn before the call that would have
/// read its last results.
#[tokio::test]
async fn past_the_round_cap_one_more_call_asks_for_an_answer_with_what_it_found() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let mut programs = tool_rounds(MAX_TOOL_TURNS);
    programs.push(Program::Answer {
        chunks: vec!["Here's what I found so far.".to_string()],
        usage: AgentUsage::default(),
    });
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("sort my downloads")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    let calls = llm.calls_seen();
    assert_eq!(calls.len(), MAX_TOOL_TURNS + 1, "one call past the cap, and only one");
    let last = calls.last().expect("calls");
    assert!(
        last.last().is_some_and(carries_wrap_up_note),
        "the wrap-up call ends on the note"
    );
    assert!(
        calls[..MAX_TOOL_TURNS]
            .iter()
            .flatten()
            .all(|m| !carries_wrap_up_note(m)),
        "no call before the cap carries it"
    );
    let stored = store::list_messages(&conn, id, 1_000, 0).expect("list");
    let persisted_note = stored.iter().any(|row| match &row.content {
        store::StoredContent::Message { role, parts } => carries_wrap_up_note(&AgentMessage {
            role: *role,
            parts: parts.clone(),
            at: 0,
        }),
        store::StoredContent::Event(_) => false,
    });
    assert!(
        !persisted_note,
        "the note is never persisted: it's this call's, not the thread's"
    );
    assert_eq!(
        terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit),
        Some(store::TurnLimit {
            kind: store::TurnLimitKind::ToolRounds,
            max: MAX_TOOL_TURNS as u32,
            answered: true,
        }),
        "the stats name the limit and its value, so the rail can"
    );
}

/// The wrap-up call is bounded too: a reply that still reaches for a tool ends the turn on the
/// limit, no tool runs, and no tool call is persisted without a result.
#[tokio::test]
async fn a_wrap_up_reply_that_still_calls_a_tool_ends_on_the_limit_without_running_it() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(tool_rounds(MAX_TOOL_TURNS + 4));
    let dispatcher = CountingOkDispatcher::default();
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("keep going")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::BudgetExhausted));
    assert_eq!(llm.calls_seen().len(), MAX_TOOL_TURNS + 1);
    assert_eq!(
        dispatcher.dispatched().len(),
        MAX_TOOL_TURNS,
        "the wrap-up's call never runs"
    );
    let stored = store::list_messages(&conn, id, 1_000, 0).expect("list");
    let last_message = stored
        .iter()
        .rev()
        .find_map(|row| match &row.content {
            store::StoredContent::Message { role, .. } => Some(*role),
            store::StoredContent::Event(_) => None,
        })
        .expect("a message");
    assert_eq!(
        last_message,
        AgentRole::Tool,
        "no dangling tool call for the next turn to load"
    );
    assert_eq!(
        terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit),
        Some(store::TurnLimit {
            kind: store::TurnLimitKind::ToolRounds,
            max: MAX_TOOL_TURNS as u32,
            answered: false,
        })
    );
}

/// What a turn spends across its rounds never ends it: only the context window bounds one call,
/// and only the round and time caps bound the loop. Regression anchor: a footer reading 194,776
/// tokens beside a gauge at 2% looked like a token budget had fired.
#[tokio::test]
async fn tokens_summed_across_rounds_never_end_a_turn() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let heavy = AgentUsage {
        prompt_tokens: 900_000,
        completion_tokens: 20_000,
    };
    let mut programs: Vec<Program> = (0..3)
        .map(|_| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: heavy,
        })
        .collect();
    programs.push(Program::Answer {
        chunks: vec!["done".to_string()],
        usage: heavy,
    });
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("look around")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }), "got {result:?}");
    let stats = terminal_stats(&drain(&mut rx)).expect("stats");
    assert_eq!(stats.prompt_tokens, 3_600_000);
    assert_eq!(stats.limit, None);
}

#[tokio::test(start_paused = true)]
async fn max_wall_time_halts_the_loop() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let programs = (0..5)
        .map(|_| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        })
        .collect();
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    // One dispatch crosses the configured wall-time ceiling, so it trips after one tool round.
    let result = run_turn(
        &llm,
        &SleepingDispatcher {
            secs: MAX_WALL_TIME.as_secs() + 1,
        },
        &conn,
        &[],
        &params(id, Some("slow please")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::BudgetExhausted));
    assert_eq!(
        llm.calls_seen().len(),
        1,
        "the wall clock halts the loop after one round"
    );
    assert_eq!(
        terminal_stats(&drain(&mut rx)).and_then(|stats| stats.limit),
        Some(store::TurnLimit {
            kind: store::TurnLimitKind::WallTime,
            max: MAX_WALL_TIME.as_secs() as u32,
            answered: false,
        }),
        "wall time makes no further call, and says which limit it was"
    );
}

// ── Cancellation ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn cancellation_mid_loop_stops_cleanly_at_a_tool_boundary() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["never reached".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let cancel = CancellationToken::new();
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &CancellingDispatcher { token: cancel.clone() },
        &conn,
        &[],
        &params(id, Some("stop me")),
        &tx,
        &cancel,
    )
    .await;

    assert_eq!(result, TurnResult::Cancelled, "a mid-loop cancel is a clean stop");
    assert_eq!(llm.calls_seen().len(), 1, "the second respond never fires after cancel");
    assert!(
        !drain(&mut rx)
            .iter()
            .any(|e| matches!(e, AgentChatEvent::Failed { .. })),
        "cancellation is not a failure"
    );
}

#[tokio::test]
async fn stop_reaches_a_tool_call_that_is_waiting_on_a_slow_volume() {
    // The turn hands its own token to every dispatch, so a call blocked on a wedged mount
    // answers when the user presses Stop instead of holding the turn until the mount does.
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::Tools {
        calls: vec![(ToolId::ListDir, json!({ "path": "/Volumes/naspi-1" }))],
        usage: AgentUsage::default(),
    }]);
    let cancel = CancellationToken::new();
    let dispatcher = Arc::new(StopAwareDispatcher::default());
    let (tx, _rx) = unbounded_channel();
    let stopper = {
        let (dispatcher, cancel) = (dispatcher.clone(), cancel.clone());
        tokio::spawn(async move {
            dispatcher.reached.notified().await;
            cancel.cancel();
        })
    };

    let turn_params = params(id, Some("list the nas"));
    let turn = run_turn(&llm, dispatcher.as_ref(), &conn, &[], &turn_params, &tx, &cancel);
    let result = tokio::time::timeout(Duration::from_secs(5), turn)
        .await
        .expect("Stop must reach the in-flight call");
    stopper.await.expect("the stopper ran");

    assert_eq!(result, TurnResult::Cancelled);
    // The turn's own Stop race lets a handler that stops itself answer first, so the row
    // names the folder it was reading rather than the generic `cancelled`.
    assert_eq!(
        persisted_tool_results(&conn, id),
        vec![json!({ "miss": "cancelled", "location": { "path": "/Volumes/naspi-1" } })]
    );
}

/// The tool results a thread persisted, in order.
fn persisted_tool_results(conn: &Connection, id: i64) -> Vec<Value> {
    store::list_messages(conn, id, 100, 0)
        .expect("list")
        .into_iter()
        .filter_map(|message| match message.content {
            store::StoredContent::Message { parts, .. } => match parts.into_iter().next() {
                Some(AgentPart::ToolResult(result)) => Some(result.content),
                _ => None,
            },
            store::StoredContent::Event(_) => None,
        })
        .collect()
}

/// Run a one-call turn whose call `dispatcher` holds open, pressing Stop once it's in flight.
async fn stop_mid_call(conn: &Connection, id: i64, tool: ToolId, dispatcher: &HeldDispatcher) -> TurnResult {
    let llm = ProgrammableLlm::new(vec![Program::Tools {
        calls: vec![(tool, json!({}))],
        usage: AgentUsage::default(),
    }]);
    let cancel = CancellationToken::new();
    let (tx, _rx) = unbounded_channel();
    let turn_params = params(id, Some("go"));
    let turn = run_turn(&llm, dispatcher, conn, &[], &turn_params, &tx, &cancel);
    let stop = async {
        dispatcher.reached.notified().await;
        cancel.cancel();
        dispatcher.release.notify_one();
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(turn, stop) })
        .await
        .expect("Stop must end the turn");
    result
}

#[tokio::test]
async fn stop_abandons_an_in_flight_call_of_any_tool_and_records_it_cancelled() {
    // A handler that never looks at the token (here one that would never return on its
    // own) still lets go on Stop: the turn drops the call, and the thread keeps a result
    // row for it so the next turn loads a whole call/result pair.
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = HeldDispatcher::forever();

    let result = stop_mid_call(&conn, id, ToolId::Search, &dispatcher).await;

    assert_eq!(result, TurnResult::Cancelled);
    assert_eq!(
        persisted_tool_results(&conn, id),
        vec![json!({ "miss": "cancelled", "detail": "The user stopped this turn before this call finished." })]
    );
}

#[tokio::test]
async fn stop_lets_a_call_that_may_be_writing_finish_with_its_real_answer() {
    // A proposal or a memory note commits on a blocking thread that dropping the call can't
    // stop. Recording `cancelled` beside a proposal the user can see would be a lie, so these
    // run to their own end, and the turn stops right after.
    for tool in [ToolId::ProposeSuggestions, ToolId::MemoryWrite] {
        let conn = migrated_conn();
        let id = conversation(&conn);
        let dispatcher = HeldDispatcher::until_released(json!({ "readyForReview": true }));

        let result = stop_mid_call(&conn, id, tool.clone(), &dispatcher).await;

        assert_eq!(result, TurnResult::Cancelled, "{}", tool.as_wire_name());
        assert_eq!(
            persisted_tool_results(&conn, id),
            vec![json!({ "readyForReview": true })],
            "{} keeps what it really did",
            tool.as_wire_name()
        );
    }
}

#[tokio::test]
async fn a_call_after_stop_is_never_dispatched() {
    // Stop lands during the first of two calls in one message. The second never starts, even
    // a proposal, and still gets its result row.
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::Tools {
        calls: vec![
            (ToolId::ListDir, json!({ "path": "/" })),
            (ToolId::ProposeSuggestions, json!({})),
        ],
        usage: AgentUsage::default(),
    }]);
    let cancel = CancellationToken::new();
    let dispatcher = CancellingDispatcher { token: cancel.clone() };
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(&llm, &dispatcher, &conn, &[], &params(id, Some("go")), &tx, &cancel).await;

    assert_eq!(result, TurnResult::Cancelled);
    let results = persisted_tool_results(&conn, id);
    assert_eq!(results.len(), 2, "both calls keep a result row");
    assert_eq!(results[0], json!({ "ok": true }), "the first call ran");
    assert_eq!(results[1]["miss"], "cancelled", "the second never started");
}

/// Where a model call can hang: before the provider answers at all, or after it opened the
/// stream but before its next chunk. Neither watches the turn's token, which is the point: the
/// turn itself must hear Stop, whatever an `AgentLlm` does with the token it's handed.
enum Hang {
    BeforeOpening,
    AfterOpening,
}

struct HangingLlm(Hang);

impl AgentLlm for HangingLlm {
    fn respond<'a>(
        &'a self,
        _system: &'a str,
        _tools: &'a [ToolDeclaration],
        _messages: &'a [AgentMessage],
        _call: crate::agent::llm::CallSettings,
        _cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<crate::agent::llm::AgentDeltaStream, AgentLlmError>> {
        match self.0 {
            Hang::BeforeOpening => futures_util::future::pending().boxed(),
            Hang::AfterOpening => async { Ok(futures_util::stream::pending().boxed()) }.boxed(),
        }
    }
}

/// ⚠️ Stop must end the turn wherever the model call is, or the turn keeps spending: a provider
/// that's slow to answer (a long queue, a model thinking before its first token) used to hold the
/// turn until it did, the user's Stop notwithstanding.
#[tokio::test]
async fn stop_ends_a_model_call_that_never_answers() {
    for hang in [Hang::BeforeOpening, Hang::AfterOpening] {
        let conn = migrated_conn();
        let id = conversation(&conn);
        let llm = HangingLlm(hang);
        let cancel = CancellationToken::new();
        let (tx, _rx) = unbounded_channel();
        let turn_params = params(id, Some("go"));
        let turn = run_turn(&llm, &OkDispatcher, &conn, &[], &turn_params, &tx, &cancel);
        let stop = async {
            tokio::task::yield_now().await;
            cancel.cancel();
        };

        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async { tokio::join!(turn, stop) })
            .await
            .expect("Stop must end a model call that never answers");

        assert_eq!(result, TurnResult::Cancelled);
    }
}

#[tokio::test]
async fn every_call_hears_the_origin_its_caller_stated() {
    // The gate reads the origin the CALLER states, never one inferred from the opener: here a
    // background turn that opens with plain text still reaches its calls as a wake.
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListVolumes, json!({}))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["done".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let dispatcher = OriginRecordingDispatcher::default();
    let (tx, _rx) = unbounded_channel();
    let mut turn_params = params(id, Some("looks like the user"));
    turn_params.origin = TurnOrigin::Wake;

    run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &turn_params,
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(dispatcher.seen(), vec![TurnOrigin::Wake]);
}

// ── Crash / persistence semantics ─────────────────────────────────────────────

#[tokio::test]
async fn crash_a_stream_dropped_mid_text_persists_nothing() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // The first (and only) respond drops mid-text: no `End`, so nothing is persisted.
    let llm = ProgrammableLlm::new(vec![Program::DropAfterText {
        chunks: vec!["partial ".to_string(), "answer".to_string()],
    }]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what is big?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::UnfinishedReply));
    let persisted = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(
        persisted.is_empty(),
        "no user row and no assistant row persist when the first call never reached End (crash cases a + b)"
    );
    let events = drain(&mut rx);
    assert!(events.contains(&AgentChatEvent::Failed {
        kind: AgentErrorKind::UnfinishedReply,
        detail: None,
        turn: None,
    }));
    assert!(
        !events.iter().any(|e| matches!(e, AgentChatEvent::UserPersisted { .. })),
        "the user message is not persisted on a failed first attempt"
    );
}

#[tokio::test]
async fn an_assistant_turn_with_nothing_in_it_is_an_unfinished_reply() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // What a degenerate provider turn reduces to once the nameless tool call is dropped
    // on the way in: `End` arrived, but the message carries no text and no call. Ending
    // the turn here would persist a blank bubble and call it an answer.
    let llm = ProgrammableLlm::new(vec![Program::Tools {
        calls: vec![],
        usage: AgentUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
        },
    }]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("summarize this")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::UnfinishedReply));
    let persisted = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(
        persisted.is_empty(),
        "an empty reply persists no rows, so a retry is clean"
    );
    assert!(drain(&mut rx).contains(&AgentChatEvent::Failed {
        kind: AgentErrorKind::UnfinishedReply,
        detail: None,
        turn: None,
    }));
}

#[tokio::test]
async fn crash_c_completed_turns_persist_and_a_retry_resumes_from_them() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // Turn 1 completes with a tool call; turn 2 then drops mid-text.
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
            },
        },
        Program::DropAfterText {
            chunks: vec!["crash".to_string()],
        },
    ]);
    let (tx, _rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what is big?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(result, TurnResult::Failed(AgentErrorKind::UnfinishedReply));

    // Turn 1's rows survive: user, assistant(tool_call), and the tool result. The
    // crashed turn 2's assistant row is absent.
    let persisted = store::list_messages(&conn, id, 100, 0).expect("list");
    let roles = transcript_roles(&persisted);
    assert_eq!(
        roles,
        vec![AgentRole::User, AgentRole::Assistant, AgentRole::Tool],
        "only the completed turn's rows persist"
    );

    // A retry issues a FRESH respond from the persisted transcript (user_text: None),
    // and its assembled prompt includes turn 1's completed rows.
    let retry_llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["Movies is the biggest.".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx2, _rx2) = unbounded_channel();
    let retry = run_turn(
        &retry_llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, None),
        &tx2,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(retry, TurnResult::Answered { .. }), "the retry answers");

    let resumed_prompt = &retry_llm.calls_seen()[0];
    let roles_in_prompt: Vec<AgentRole> = resumed_prompt.iter().map(|m| m.role).collect();
    assert_eq!(
        roles_in_prompt,
        vec![AgentRole::User, AgentRole::Assistant, AgentRole::Tool],
        "the retry's assembled prompt includes the completed turn's rows (fresh respond, not a re-send)"
    );
}

// ── Cost metering ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn cost_is_metered_per_completed_respond_call() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // One tool round (10/5) then a final answer (20/10): two completed respond calls, so
    // the meter must accumulate 30 prompt + 15 completion tokens.
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
            },
        },
        Program::Answer {
            chunks: vec!["done".to_string()],
            usage: AgentUsage {
                prompt_tokens: 20,
                completion_tokens: 10,
            },
        },
    ]);
    let (tx, _rx) = unbounded_channel();

    run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what is big?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    let summary = store::cost_summary(&conn).expect("summary");
    assert_eq!(summary.days.len(), 1, "one day of spend");
    assert_eq!(summary.days[0].prompt_tokens, 30, "both completed calls are metered");
    assert_eq!(summary.days[0].completion_tokens, 15);
}

// ── End-to-end multi-tool turn ────────────────────────────────────────────────

#[tokio::test]
async fn end_to_end_multi_tool_turn_dispatches_and_answers() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![
                (ToolId::ListDir, json!({ "path": "/a" })),
                (ToolId::ListVolumes, json!({})),
            ],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["The biggest is ".to_string(), "Movies.".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("what is big?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert!(matches!(result, TurnResult::Answered { .. }));

    // Persisted: user, assistant(2 tool calls), 2 tool results, assistant(answer).
    let roles = transcript_roles(&store::list_messages(&conn, id, 100, 0).expect("list"));
    assert_eq!(
        roles,
        vec![
            AgentRole::User,
            AgentRole::Assistant,
            AgentRole::Tool,
            AgentRole::Tool,
            AgentRole::Assistant,
        ]
    );

    let events = drain(&mut rx);
    let tool_finished = events
        .iter()
        .filter(|e| matches!(e, AgentChatEvent::ToolCallFinished { outcome, .. } if outcome.ok()))
        .count();
    assert_eq!(tool_finished, 2, "both tool calls dispatched and finished ok");
    assert!(
        events.iter().any(|e| matches!(e, AgentChatEvent::Done { .. })),
        "a final answer"
    );
    assert!(events.contains(&AgentChatEvent::TextDelta {
        text: "The biggest is ".to_string()
    }));

    // Snapshot-at-send: both respond calls of the loop saw a byte-identical envelope on
    // the latest user turn (the runtime holds one captured envelope across the turn).
    let seen = llm.calls_seen();
    assert_eq!(seen.len(), 2, "two respond calls in the loop");
    assert_eq!(
        leading_text(&seen[0][0]),
        leading_text(&seen[1][0]),
        "the envelope must not shift across the turn's respond calls"
    );
}

// ── Typed error surface ───────────────────────────────────────────────────────

#[test]
fn agent_llm_errors_map_to_typed_kinds() {
    // The seam carries TYPED error kinds, not strings. The user-facing copy (rendered
    // without the words "error"/"failed") is the frontend's job; here we pin the
    // total, variant-to-variant mapping.
    use AgentErrorKind as K;
    use AgentLlmError as E;
    assert_eq!(K::from(E::NoKey), K::NoKey);
    assert_eq!(K::from(E::NotConfigured), K::NotConfigured);
    assert_eq!(K::from(E::Unavailable), K::Unavailable);
    assert_eq!(K::from(E::Timeout), K::Timeout);
    assert_eq!(K::from(E::AuthFailed("bad key".into())), K::AuthFailed);
    assert_eq!(K::from(E::RateLimited("slow down".into())), K::RateLimited);
    assert_eq!(K::from(E::BudgetExhausted), K::BudgetExhausted);
    assert_eq!(K::from(E::Provider("detail".into())), K::Provider);
}

#[tokio::test]
async fn a_pre_stream_provider_error_persists_nothing_and_is_typed() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // The respond call fails before opening a stream (no key). Nothing persists (case b).
    let llm = ProgrammableLlm::new(vec![]); // exhausted script → provider error on first call
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hello")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Provider));
    assert!(store::list_messages(&conn, id, 100, 0).expect("list").is_empty());
    // The event carries the provider's own wording so the UI can show the user what to
    // fix (display only — the frontend still branches on `kind`, never on this string).
    assert!(drain(&mut rx).contains(&AgentChatEvent::Failed {
        kind: AgentErrorKind::Provider,
        detail: Some("programmable: script exhausted".to_string()),
        turn: None,
    }));
}

#[tokio::test]
async fn a_mid_stream_provider_error_carries_its_detail() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::ErrorAfterText {
        chunks: vec!["partial".to_string()],
        error: AgentLlmError::Provider("HTTP 404: model gone".to_string()),
    }]);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("hello")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Provider));
    assert!(drain(&mut rx).contains(&AgentChatEvent::Failed {
        kind: AgentErrorKind::Provider,
        detail: Some("HTTP 404: model gone".to_string()),
        turn: None,
    }));
}

// ── Attachments reach the LLM in the envelope (and nothing more) ────────────────

#[tokio::test]
async fn attachments_reach_the_llm_in_the_envelope_and_nothing_more() {
    use crate::agent::chat::context::{AttachmentKind, EnvelopeAttachment};

    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["ok".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx, _rx) = unbounded_channel();

    let env = ContextEnvelope {
        captured_at: 1_780_000_000,
        focused_pane_path: Some("~/Documents".to_string()),
        cursor_item: None,
        selection_count: 0,
        volumes: vec![],
        attachments: vec![EnvelopeAttachment {
            path: "/Users/d/report.pdf".to_string(),
            kind: AttachmentKind::File,
        }],
        denied_names: vec![],
        rename_batch_files: 101,
    };
    let params = TurnParams {
        conversation_id: id,
        origin: TurnOrigin::Text,
        user: Some(UserTurn::Text("summarize this")),
        cmdr_md: None,
        memory: &[],
        envelope: &env,
        offset: offset(),
        now_secs: 1_780_000_000,
        provider: ProviderTag::Local,
        model: "fake-model".to_string(),
        prompt_budget: TEST_PROMPT_BUDGET,
        chat_memory_tokens: TEST_PROMPT_BUDGET,
        call: crate::agent::llm::CallSettings::default(),
    };

    let result = run_turn(&llm, &OkDispatcher, &conn, &[], &params, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }));

    // The prompt the LLM actually saw carries the attachment on the user turn.
    let seen = llm.calls_seen();
    let messages = &seen[0];
    let user_turn = messages
        .iter()
        .rev()
        .find(|m| m.role == AgentRole::User)
        .expect("a user turn");
    let opening = leading_text(user_turn);
    assert!(
        opening.contains("attached: /Users/d/report.pdf (file)"),
        "the envelope names the attachment path + kind: {opening}"
    );
    // Path + kind and NOTHING else — no size, no bytes, no file contents field.
    let joined: String = messages
        .iter()
        .flat_map(|m| m.parts.iter())
        .filter_map(|p| match p {
            AgentPart::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !joined.to_lowercase().contains("content"),
        "no file contents reach the prompt: {joined}"
    );
}

/// The send gate is STRUCTURAL and runs before a thread or an LLM exists. This mirrors
/// `ask_cmdr_send_message`'s control flow: `admit_send` decides, the command creates a thread
/// and drives `run_turn` only on `Ok`. The fake records ZERO calls and the store ZERO threads
/// when the gate refuses, and exactly one call when it opens (so the empty cases mean something).
#[tokio::test]
async fn the_send_gate_refuses_before_any_thread_or_llm_call() {
    use crate::agent::chat::session::{SendGateRefusal, SlotRefusal, admit_send};
    use crate::agent::chat::stream::AgentErrorKindView;
    use crate::settings::AskCmdrSwitch;

    let conn = migrated_conn();
    let llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["hi".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx, _rx) = unbounded_channel();
    let threads = |conn: &Connection| store::list_conversations(conn, 100, 0, true).expect("list").len();

    // Ask Cmdr switched off: refused without even resolving the slot.
    let mut resolved = false;
    let off = admit_send(AskCmdrSwitch::Off, || {
        resolved = true;
        Ok(())
    });
    assert!(matches!(off, Err(SendGateRefusal::AskCmdrOff)));
    assert!(!resolved, "an Ask Cmdr that's off never looks at the provider");

    // On, but pinned off by the organization: the slot names the organization's reason, never
    // "Ask Cmdr is off" (the person didn't turn it off).
    let managed_off = admit_send(AskCmdrSwitch::ManagedOff, || {
        Err::<(), _>(SlotRefusal::Managed(crate::managed_policy::ManagedAiRefusal::AiOff))
    });
    assert!(managed_off.is_err_and(|gate| matches!(gate.view(), AgentErrorKindView::ManagedByOrganization)));

    // Ask Cmdr on, Cloud picked, cloud AI not allowed: the slot's refusal comes through as its own kind.
    let no_cloud = admit_send(AskCmdrSwitch::On, || Err::<(), _>(SlotRefusal::NoCloudConsent));
    assert!(matches!(
        no_cloud,
        Err(SendGateRefusal::Slot(SlotRefusal::NoCloudConsent))
    ));

    for refused in [off.is_ok(), no_cloud.is_ok()] {
        if refused {
            let id = conversation(&conn);
            run_turn(
                &llm,
                &OkDispatcher,
                &conn,
                &[],
                &params(id, Some("hi")),
                &tx,
                &CancellationToken::new(),
            )
            .await;
        }
    }
    assert!(llm.calls_seen().is_empty(), "a refused send makes ZERO LLM calls");
    assert_eq!(threads(&conn), 0, "and adds no thread");

    // Ask Cmdr on with a slot that resolves (Local, or Cloud with consent): the send proceeds.
    if admit_send(AskCmdrSwitch::On, || Ok::<(), SlotRefusal>(())).is_ok() {
        let id = conversation(&conn);
        run_turn(
            &llm,
            &OkDispatcher,
            &conn,
            &[],
            &params(id, Some("hi")),
            &tx,
            &CancellationToken::new(),
        )
        .await;
    }
    assert_eq!(
        llm.calls_seen().len(),
        1,
        "an admitted send drives the LLM exactly once"
    );
}

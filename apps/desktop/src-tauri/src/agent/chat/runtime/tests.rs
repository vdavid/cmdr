//! Runtime tests: single-flight, the per-message budgets, cancellation at a tool
//! boundary, the crash-safe persistence model, cost metering, an end-to-end fake-driven
//! multi-tool turn, and the typed error surface plus the two gates on what reaches the
//! LLM (the envelope's attachments, and consent).
//!
//! Three sibling files hold the rest, all borrowing the fixtures in `test_support.rs`:
//! `context_budget_tests.rs` (context drops and the fullness gauge),
//! `model_change_tests.rs` (which model a thread is on), and `wake_tests.rs` (the wake
//! path).

use super::test_support::*;
use super::*;
use crate::agent::chat::context::{MAX_TOOL_TURNS, MAX_WALL_TIME};
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

#[tokio::test]
async fn max_tool_turns_halts_before_the_ninth_respond() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    // More tool turns than the cap allows; the runtime must stop before the 9th respond.
    let programs = (0..MAX_TOOL_TURNS + 4)
        .map(|_| Program::Tools {
            calls: vec![(ToolId::ListDir, json!({ "path": "/" }))],
            usage: AgentUsage::default(),
        })
        .collect();
    let llm = ProgrammableLlm::new(programs);
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &params(id, Some("keep going")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(result, TurnResult::Failed(AgentErrorKind::BudgetExhausted));
    assert_eq!(
        llm.calls_seen().len(),
        MAX_TOOL_TURNS,
        "exactly MAX_TOOL_TURNS respond calls fire; the ninth never does"
    );
    assert!(
        drain(&mut rx).contains(&AgentChatEvent::Failed {
            kind: AgentErrorKind::BudgetExhausted,
            detail: None,
        }),
        "the budget-exhausted notice is emitted"
    );
}

// ── The repeat breaker ────────────────────────────────────────────────────────

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
    let (tx, _rx) = unbounded_channel();

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
    let roles: Vec<AgentRole> = persisted.iter().map(stored_role).collect();
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
    let roles: Vec<AgentRole> = store::list_messages(&conn, id, 100, 0)
        .expect("list")
        .iter()
        .map(stored_role)
        .collect();
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
        .filter(|e| matches!(e, AgentChatEvent::ToolCallFinished { ok: true, .. }))
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
        user: Some(UserTurn::Text("summarize this")),
        cmdr_md: None,
        memory: None,
        envelope: &env,
        offset: offset(),
        now_secs: 1_780_000_000,
        provider: ProviderTag::Local,
        model: "fake-model".to_string(),
        prompt_budget: TEST_PROMPT_BUDGET,
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

/// The consent gate is STRUCTURAL: a send with no/stale consent never reaches the LLM.
/// This mirrors `ask_cmdr_send_message`'s control flow — gate on `has_current_consent`,
/// then drive `run_turn` only when it opens — and proves the fake records ZERO calls when
/// the gate refuses, and exactly one when it opens (so the empty case is meaningful).
#[tokio::test]
async fn a_send_without_current_consent_never_calls_the_llm() {
    use crate::agent::consent::{CONSENT_COPY_VERSION, has_current_consent};

    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["hi".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx, _rx) = unbounded_channel();

    // No consent recorded, then a STALE copy version — both keep the gate closed.
    assert!(!has_current_consent(&conn), "no consent record ⇒ gate closed");
    store::set_consent(&conn, CONSENT_COPY_VERSION.wrapping_sub(1), 1_780_000_000).expect("set stale consent");
    assert!(!has_current_consent(&conn), "a stale copy version ⇒ gate closed");

    // The command skips `run_turn` while the gate is closed, so the LLM is never called.
    if has_current_consent(&conn) {
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
    assert!(llm.calls_seen().is_empty(), "a refused send makes ZERO LLM calls");

    // Accepting the CURRENT copy opens the gate; the send then drives the LLM once.
    store::set_consent(&conn, CONSENT_COPY_VERSION, 1_780_000_000).expect("set current consent");
    assert!(has_current_consent(&conn), "current consent ⇒ gate open");
    if has_current_consent(&conn) {
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
        "with consent, the send drives the LLM exactly once"
    );
}

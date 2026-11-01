//! What the stream says about each tool line and about the turn as a whole: which calls a round
//! holds (`ToolCallsResolved`), what each one came back with (`ToolCallFinished`, only once its
//! row exists), and what the turn spent (on its one terminal event, and in its `TurnFinished`
//! row). A line is `(message_id, call_id)`: the fake mints `fake-call-{index}` per round, so ids
//! collide across rounds by design, and that is the regression fixture here.

use std::path::PathBuf;

use super::test_support::*;
use super::*;
use crate::agent::line::{LinePath, LineRef, ToolOutcome, ToolSubject};
use crate::agent::llm::fake::{FakeAgentLlm, ScriptedTurn};
use crate::agent::store::TurnStats;

fn usage(prompt_tokens: u32, completion_tokens: u32) -> AgentUsage {
    AgentUsage {
        prompt_tokens,
        completion_tokens,
        ..AgentUsage::default()
    }
}

/// The ids of the thread's assistant rows, in order.
fn assistant_ids(conn: &Connection, id: i64) -> Vec<i64> {
    store::list_messages(conn, id, 100, 0)
        .expect("list")
        .into_iter()
        .filter(|m| {
            matches!(
                &m.content,
                store::StoredContent::Message {
                    role: AgentRole::Assistant,
                    ..
                }
            )
        })
        .map(|m| m.id)
        .collect()
}

/// Every `TurnFinished` row of the thread, in order.
fn turn_finished_rows(conn: &Connection, id: i64) -> Vec<TurnStats> {
    store::list_messages(conn, id, 100, 0)
        .expect("list")
        .into_iter()
        .filter_map(|m| match m.content {
            store::StoredContent::Event(store::ConversationEvent::TurnFinished { stats }) => Some(stats),
            _ => None,
        })
        .collect()
}

/// The terminal event's `turn`, whichever terminal it was.
fn terminal_stats(events: &[AgentChatEvent]) -> Option<TurnStats> {
    match events.last() {
        Some(
            AgentChatEvent::Done { turn, .. } | AgentChatEvent::Failed { turn, .. } | AgentChatEvent::Stopped { turn },
        ) => *turn,
        other => panic!("the turn must end on a terminal event, got {other:?}"),
    }
}

async fn run(llm: &dyn AgentLlm, dispatcher: &dyn ToolDispatcher, conn: &Connection, id: i64) -> Vec<AgentChatEvent> {
    let (tx, mut rx) = unbounded_channel();
    run_turn(
        llm,
        dispatcher,
        conn,
        &[],
        &params(id, Some("look around")),
        &tx,
        &CancellationToken::new(),
    )
    .await;
    drain(&mut rx)
}

/// Two rounds both mint `fake-call-0`. Each finished line must name its OWN round's row, or the
/// rail would settle round 1's line with round 2's answer.
#[tokio::test]
async fn colliding_call_ids_across_rounds_land_on_their_own_rows() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = FakeAgentLlm::script(vec![
        ScriptedTurn::CallTools(vec![(ToolId::ListDir, json!({ "path": "/a" }))]),
        ScriptedTurn::CallTools(vec![(ToolId::ListDir, json!({ "path": "/b" }))]),
        ScriptedTurn::Say(vec!["done".into()]),
    ]);

    let events = run(&llm, &OkDispatcher, &conn, id).await;

    let rows = assistant_ids(&conn, id);
    let finished: Vec<(i64, String, ToolSubject)> = events
        .iter()
        .filter_map(|e| match e {
            AgentChatEvent::ToolCallFinished {
                message_id,
                call_id,
                subject,
                ..
            } => Some((*message_id, call_id.clone(), subject.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        finished,
        vec![
            (
                rows[0],
                "fake-call-0".to_string(),
                ToolSubject::Folder {
                    path: LinePath::of("/a")
                }
            ),
            (
                rows[1],
                "fake-call-0".to_string(),
                ToolSubject::Folder {
                    path: LinePath::of("/b")
                }
            ),
        ]
    );
}

/// The whole order of a multi-tool turn, the repeat mark, and the summed stats.
#[tokio::test]
async fn a_multi_tool_turn_resolves_each_round_then_finishes_each_call_then_answers_with_its_stats() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = FakeAgentLlm::script_with_usage(vec![
        (
            ScriptedTurn::CallTools(vec![(ToolId::AppState, json!({})), (ToolId::ListVolumes, json!({}))]),
            usage(100, 10),
        ),
        (
            ScriptedTurn::CallTools(vec![(ToolId::ListVolumes, json!({}))]),
            usage(200, 20),
        ),
        (ScriptedTurn::Say(vec!["Two drives.".into()]), usage(300, 30)),
    ]);

    let events = run(&llm, &OkDispatcher, &conn, id).await;

    let shape: Vec<&str> = events
        .iter()
        .map(|e| match e {
            AgentChatEvent::UserPersisted { .. } => "user",
            AgentChatEvent::AssistantStarted => "assistant",
            AgentChatEvent::ToolCallStarted { .. } => "started",
            AgentChatEvent::ToolCallsResolved { .. } => "resolved",
            AgentChatEvent::ToolCallFinished { .. } => "finished",
            AgentChatEvent::TextDelta { .. } => "text",
            AgentChatEvent::CostMetered => "metered",
            AgentChatEvent::ContextUsage { .. } => "usage",
            AgentChatEvent::Done { .. } => "done",
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(
        shape,
        [
            "assistant",
            "started",
            "started",
            "user",
            "metered",
            "usage",
            "resolved",
            "finished",
            "finished", //
            "assistant",
            "started",
            "metered",
            "usage",
            "resolved",
            "finished", //
            "assistant",
            "text",
            "metered",
            "usage",
            "done",
        ]
    );

    let rows = assistant_ids(&conn, id);
    let resolved: Vec<&AgentChatEvent> = events
        .iter()
        .filter(|e| matches!(e, AgentChatEvent::ToolCallsResolved { .. }))
        .collect();
    assert_eq!(
        *resolved[0],
        AgentChatEvent::ToolCallsResolved {
            message_id: rows[0],
            calls: vec![
                ResolvedCall {
                    call_id: "fake-call-0".into(),
                    tool: "app_state".into(),
                    subject: ToolSubject::Panes,
                    repeat_of: None,
                },
                ResolvedCall {
                    call_id: "fake-call-1".into(),
                    tool: "list_volumes".into(),
                    subject: ToolSubject::Drives,
                    repeat_of: None,
                },
            ],
        }
    );
    assert_eq!(
        *resolved[1],
        AgentChatEvent::ToolCallsResolved {
            message_id: rows[1],
            calls: vec![ResolvedCall {
                call_id: "fake-call-0".into(),
                tool: "list_volumes".into(),
                subject: ToolSubject::Drives,
                repeat_of: Some(LineRef {
                    message_id: rows[0],
                    call_id: "fake-call-1".into(),
                }),
            }],
        },
        "an identical later call is marked as a repeat of the turn's first one, across rounds and colliding ids"
    );

    let stats = terminal_stats(&events).expect("a tool-running turn reports what it spent");
    assert_eq!(stats.tool_calls, 3);
    assert_eq!(
        (stats.prompt_tokens, stats.completion_tokens),
        (600, 60),
        "summed over every round"
    );
    assert!(stats.wall_ms.is_some());
    assert!(!stats.stopped, "an answered turn wasn't stopped");
    assert_eq!(
        turn_finished_rows(&conn, id),
        vec![stats],
        "the same stats persist after the turn's last row"
    );
    let last = store::list_messages(&conn, id, 100, 0)
        .expect("list")
        .pop()
        .expect("rows");
    assert!(
        matches!(
            last.content,
            store::StoredContent::Event(store::ConversationEvent::TurnFinished { .. })
        ),
        "the stats row comes after the answer"
    );
}

/// The provider seam dropped a nameless call after the rail already heard it start. The round's
/// batch lists only what the final message holds, which is what lets the rail drop the stray.
#[tokio::test]
async fn a_started_call_the_final_message_dropped_is_not_resolved() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::ToolsAfterStrayStart {
            stray_call_id: "nameless".into(),
            calls: vec![(ToolId::ListVolumes, json!({}))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["ok".into()],
            usage: AgentUsage::default(),
        },
    ]);

    let events = run(&llm, &OkDispatcher, &conn, id).await;

    let resolved: Vec<Vec<String>> = events
        .iter()
        .filter_map(|e| match e {
            AgentChatEvent::ToolCallsResolved { calls, .. } => {
                Some(calls.iter().map(|call| call.call_id.clone()).collect())
            }
            _ => None,
        })
        .collect();
    assert_eq!(resolved, vec![vec!["call-0".to_string()]]);
}

/// Deletes the thread while its call runs, so that call's result row can't persist (the
/// conversation it belongs to is gone). Stands in for any persist problem at that moment.
struct DeletesThreadMidDispatch {
    db: PathBuf,
    conversation_id: i64,
}

impl ToolDispatcher for DeletesThreadMidDispatch {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            let other = store::open_write_connection(&self.db).expect("open");
            store::delete_conversation(&other, self.conversation_id).expect("delete");
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "ok": true }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// `ToolCallFinished` is what tells the rail the result can be fetched, so it may only follow a
/// result row that exists: a call whose row didn't persist never announces finishing.
#[tokio::test]
async fn a_call_whose_result_row_never_persisted_never_announces_finishing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = store::main_db_path(dir.path());
    let conn = store::open_write_connection(&db).expect("open");
    let id = conversation(&conn);
    let llm = FakeAgentLlm::script(vec![ScriptedTurn::CallTools(vec![(ToolId::ListVolumes, json!({}))])]);
    let dispatcher = DeletesThreadMidDispatch {
        db,
        conversation_id: id,
    };

    let events = run(&llm, &dispatcher, &conn, id).await;

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentChatEvent::ToolCallFinished { .. })),
        "{events:?}"
    );
    assert!(matches!(events.last(), Some(AgentChatEvent::Failed { .. })));
}

// ── The turn's stats and its `TurnFinished` row, whatever the ending ───────────

#[tokio::test]
async fn a_tool_less_turn_writes_no_stats_row_and_its_done_carries_none() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["hi".into()])]);

    let events = run(&llm, &OkDispatcher, &conn, id).await;

    assert!(matches!(events.last(), Some(AgentChatEvent::Done { turn: None, .. })));
    assert!(turn_finished_rows(&conn, id).is_empty());
}

#[tokio::test]
async fn a_failed_turn_that_ran_tools_still_reports_and_records_what_it_spent() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ListVolumes, json!({}))],
            usage: usage(50, 5),
        },
        Program::ErrorAfterText {
            chunks: vec!["partial".into()],
            error: AgentLlmError::Provider("gone".into()),
        },
    ]);

    let events = run(&llm, &OkDispatcher, &conn, id).await;

    let stats = match events.last() {
        Some(AgentChatEvent::Failed { turn: Some(stats), .. }) => *stats,
        other => panic!("expected a failed terminal with stats, got {other:?}"),
    };
    assert_eq!(
        (stats.tool_calls, stats.prompt_tokens, stats.completion_tokens),
        (1, 50, 5)
    );
    assert!(!stats.stopped, "a failed turn wasn't stopped");
    assert_eq!(turn_finished_rows(&conn, id), vec![stats]);
}

/// Dispatch isn't cancellable, so a Stop mid-round still runs and persists the round's remaining
/// calls; the stats that come with `Stopped` count them, and the stop is the only terminal.
#[tokio::test]
async fn a_cancel_mid_dispatch_still_runs_the_round_and_stops_with_stats_that_count_it() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let cancel = CancellationToken::new();
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::AppState, json!({})), (ToolId::ListVolumes, json!({}))],
            usage: usage(70, 7),
        },
        Program::Answer {
            chunks: vec!["never".into()],
            usage: AgentUsage::default(),
        },
    ]);
    let dispatcher = CancellingDispatcher { token: cancel.clone() };
    let (tx, mut rx) = unbounded_channel();

    let result = run_turn(&llm, &dispatcher, &conn, &[], &params(id, Some("hi")), &tx, &cancel).await;

    assert_eq!(result, TurnResult::Cancelled);
    let events = drain(&mut rx);
    let finished = events
        .iter()
        .filter(|e| matches!(e, AgentChatEvent::ToolCallFinished { .. }))
        .count();
    assert_eq!(finished, 2, "both calls of the round still ran after the stop");
    let terminals = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                AgentChatEvent::Done { .. } | AgentChatEvent::Failed { .. } | AgentChatEvent::Stopped { .. }
            )
        })
        .count();
    assert_eq!(terminals, 1, "a cancel emits exactly one terminal");
    let stats = match events.last() {
        Some(AgentChatEvent::Stopped { turn: Some(stats) }) => *stats,
        other => panic!("expected a stopped terminal with stats, got {other:?}"),
    };
    assert_eq!((stats.tool_calls, stats.prompt_tokens), (2, 70));
    assert!(
        stats.stopped,
        "a stopped turn says so, and the stored row keeps it for a reload"
    );
    assert_eq!(turn_finished_rows(&conn, id), vec![stats]);
}

/// The persisted opener rides `UserPersisted` as the view history would show it.
#[tokio::test]
async fn user_persisted_carries_the_stored_opener() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["hi".into()])]);

    let events = run(&llm, &OkDispatcher, &conn, id).await;

    let opener = events
        .iter()
        .find_map(|e| match e {
            AgentChatEvent::UserPersisted { opener } => Some(opener.clone()),
            _ => None,
        })
        .expect("the opener persisted");
    let stored = store::list_messages(&conn, id, 1, 0).expect("list").remove(0);
    assert_eq!(opener, crate::agent::chat::message_view::to_message_view(stored));
}

/// The outcome on the stream is `outcome_of` over the stored result, so a refusal reads as one.
#[tokio::test]
async fn a_refused_call_finishes_with_a_refused_outcome() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = FakeAgentLlm::script(vec![
        ScriptedTurn::CallTools(vec![(ToolId::ListDir, json!({}))]),
        ScriptedTurn::Say(vec!["sorry".into()]),
    ]);

    let events = run(&llm, &FailingDispatcher::default(), &conn, id).await;

    let outcome = events
        .iter()
        .find_map(|e| match e {
            AgentChatEvent::ToolCallFinished { outcome, .. } => Some(outcome.clone()),
            _ => None,
        })
        .expect("finished");
    assert!(matches!(outcome, ToolOutcome::Refused { .. }), "{outcome:?}");
}

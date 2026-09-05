//! Which model a thread is on, and how a change to it becomes visible: the stamp a first
//! turn leaves, the event row a change between turns writes ahead of the user message, and
//! the live `record_model_change` path the settings screen drives while a thread is open.
//! The fixtures come from the sibling `test_support` module.

use super::test_support::*;
use super::*;

// ── Model-change events ───────────────────────────────────────────────────────

#[tokio::test]
async fn first_turn_stamps_last_model_without_an_event() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let events = run_answer_turn(&conn, id, "model-one", "hello").await;

    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-one".to_string())
    );
    assert!(
        !events.iter().any(|e| matches!(e, AgentChatEvent::ModelChanged { .. })),
        "no event on a conversation's first turn — there is nothing to switch from"
    );
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(
        !rows.iter().any(|m| matches!(m.content, store::StoredContent::Event(_))),
        "no event row persists either"
    );
}

#[tokio::test]
async fn a_model_change_between_turns_logs_an_event_row_before_the_user_message() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn(&conn, id, "model-one", "hello").await;
    let events = run_answer_turn(&conn, id, "model-two", "again").await;

    // The event row sits between the turns: user, assistant, EVENT, user, assistant.
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    let shapes: Vec<&str> = rows
        .iter()
        .map(|m| match &m.content {
            store::StoredContent::Message { role, .. } => match role {
                AgentRole::User => "user",
                AgentRole::Assistant => "assistant",
                _ => "other",
            },
            store::StoredContent::Event(store::ConversationEvent::ModelChanged { .. }) => "event",
            store::StoredContent::Event(store::ConversationEvent::ProposalDecided { .. }) => "decision",
        })
        .collect();
    assert_eq!(shapes, vec!["user", "assistant", "event", "user", "assistant"]);
    match &rows[2].content {
        store::StoredContent::Event(store::ConversationEvent::ModelChanged { model }) => {
            assert_eq!(model, "model-two");
        }
        other => panic!("expected the model-change event, got {other:?}"),
    }

    // The live rail heard about it, with the persisted row's identity.
    assert!(events.iter().any(|e| matches!(
        e,
        AgentChatEvent::ModelChanged { message_id, seq, model }
            if *message_id == rows[2].id && *seq == rows[2].seq && model == "model-two"
    )));
    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-two".to_string())
    );
}

#[tokio::test]
async fn the_same_model_between_turns_logs_no_event() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn(&conn, id, "model-one", "hello").await;
    let events = run_answer_turn(&conn, id, "model-one", "again").await;

    assert!(!events.iter().any(|e| matches!(e, AgentChatEvent::ModelChanged { .. })));
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(!rows.iter().any(|m| matches!(m.content, store::StoredContent::Event(_))));
}

#[tokio::test]
async fn a_failed_first_attempt_records_no_event_and_leaves_last_model_untouched() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn(&conn, id, "model-one", "hello").await;

    // A turn with a new model whose respond never opens: nothing persists (crash case
    // b), including the model transition — the next successful turn records it instead.
    let llm = ProgrammableLlm::new(vec![]);
    let (tx, _rx) = unbounded_channel();
    let mut failing = params(id, Some("again"));
    failing.model = "model-two".to_string();
    let result = run_turn(
        &llm,
        &OkDispatcher,
        &conn,
        &[],
        &failing,
        &tx,
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(result, TurnResult::Failed(AgentErrorKind::Provider));

    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(!rows.iter().any(|m| matches!(m.content, store::StoredContent::Event(_))));
    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-one".to_string())
    );
}

// ── The live record path (settings changed while a thread is open) ─────────────

#[tokio::test]
async fn record_model_change_appends_an_event_and_stamps() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    let recorded = runtime
        .record_model_change(id, "model-two")
        .await
        .expect("record")
        .expect("an event was recorded");

    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    let rows = store::list_messages(&conn, id, 10, 0).expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, recorded.0);
    assert_eq!(rows[0].seq, recorded.1);
    assert_eq!(
        rows[0].content,
        store::StoredContent::Event(store::ConversationEvent::ModelChanged {
            model: "model-two".to_string()
        })
    );
    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-two".to_string())
    );
}

#[tokio::test]
async fn record_model_change_is_a_noop_when_unchanged_or_unstarted() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    // Same effective model (e.g. the changed setting was masked by the override): no row.
    assert!(
        runtime
            .record_model_change(id, "model-one")
            .await
            .expect("record")
            .is_none()
    );

    // A conversation with no completed turn yet: nothing to switch from, no row.
    let conn = store::open_write_connection(&runtime.db_path).expect("open");
    let fresh = store::create_conversation(&conn, "fresh", 100, None).expect("create");
    drop(conn);
    assert!(
        runtime
            .record_model_change(fresh, "model-two")
            .await
            .expect("record")
            .is_none()
    );
    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    assert!(store::list_messages(&conn, fresh, 10, 0).expect("list").is_empty());
    assert_eq!(
        store::conversation_last_model(&conn, fresh).expect("get"),
        None,
        "the first real turn stamps the model, not the settings change"
    );
}

#[tokio::test]
async fn record_model_change_waits_for_the_in_flight_turn() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    // Hold the conversation's single-flight lock like an in-flight turn does.
    let guard = runtime
        .locks
        .lock_for(id)
        .try_lock_owned()
        .expect("lock is free before the fake turn");

    let fut = runtime.record_model_change(id, "model-two");
    tokio::pin!(fut);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut fut).await.is_err(),
        "the record waits while the turn is in flight"
    );

    drop(guard); // the turn finishes
    let recorded = fut.await.expect("record").expect("event recorded after the turn");
    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    let rows = store::list_messages(&conn, id, 10, 0).expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, recorded.0, "the event landed only after the lock released");
}

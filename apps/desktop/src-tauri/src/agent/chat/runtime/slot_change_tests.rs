//! What the interactive slot was on for a thread, and how a change becomes visible: the
//! stamp a first turn leaves, the event row a change between turns writes ahead of the user
//! message, and the live `record_slot_change` path the settings screen drives while a thread
//! is open. Two facets travel this path — the model and the chat memory size — and each is
//! compared and recorded on its own, so one changing doesn't invent a line about the other.
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
    let shapes: Vec<&str> = rows.iter().map(row_shape).collect();
    assert_eq!(shapes, vec!["user", "assistant", "model", "user", "assistant"]);
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

// ── Chat-memory-size events ───────────────────────────────────────────────────

#[tokio::test]
async fn first_turn_stamps_the_chat_memory_size_without_an_event() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let events = run_answer_turn_sized(&conn, id, "model-one", 60_000, "hello").await;

    assert_eq!(
        store::conversation_last_chat_memory(&conn, id).expect("get"),
        Some(60_000)
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentChatEvent::ChatMemoryChanged { .. })),
        "no event on a conversation's first turn — there is nothing to have changed from"
    );
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(!rows.iter().any(|m| matches!(m.content, store::StoredContent::Event(_))));
}

#[tokio::test]
async fn a_smaller_chat_memory_between_turns_logs_an_event_row_before_the_user_message() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn_sized(&conn, id, "model-one", 60_000, "hello").await;
    let events = run_answer_turn_sized(&conn, id, "model-one", 16_000, "again").await;

    // The line sits between the turns: user, assistant, EVENT, user, assistant.
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    let shapes: Vec<&str> = rows.iter().map(row_shape).collect();
    assert_eq!(shapes, vec!["user", "assistant", "memory", "user", "assistant"]);
    assert_eq!(
        rows[2].content,
        store::StoredContent::Event(store::ConversationEvent::ChatMemoryChanged {
            chat_memory_tokens: 16_000
        })
    );

    // The live rail heard about it, with the persisted row's identity.
    assert!(events.iter().any(|e| matches!(
        e,
        AgentChatEvent::ChatMemoryChanged { message_id, seq, chat_memory_tokens }
            if *message_id == rows[2].id && *seq == rows[2].seq && *chat_memory_tokens == 16_000
    )));
    assert_eq!(
        store::conversation_last_chat_memory(&conn, id).expect("get"),
        Some(16_000)
    );
}

#[tokio::test]
async fn the_same_chat_memory_between_turns_logs_no_event() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn_sized(&conn, id, "model-one", 60_000, "hello").await;
    let events = run_answer_turn_sized(&conn, id, "model-one", 60_000, "again").await;

    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentChatEvent::ChatMemoryChanged { .. }))
    );
    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    assert!(!rows.iter().any(|m| matches!(m.content, store::StoredContent::Event(_))));
}

#[tokio::test]
async fn switching_to_a_smaller_model_says_both_what_changed_and_what_it_costs() {
    // A model with a smaller window carries less of the chat, and the two facts are
    // separate lines because they are separate facts: the user chose the model, and the
    // budget is the consequence they can't see anywhere else.
    let conn = migrated_conn();
    let id = conversation(&conn);
    run_answer_turn_sized(&conn, id, "model-one", 60_000, "hello").await;
    run_answer_turn_sized(&conn, id, "model-two", 16_000, "again").await;

    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    let shapes: Vec<&str> = rows.iter().map(row_shape).collect();
    assert_eq!(
        shapes,
        vec!["user", "assistant", "model", "memory", "user", "assistant"],
        "the model line leads, then what it costs the thread"
    );
}

/// A one-word name for what a stored row IS, so a test can assert on the shape of a whole
/// thread rather than on one row at a time.
fn row_shape(message: &store::StoredMessage) -> &'static str {
    match &message.content {
        store::StoredContent::Message { role, .. } => match role {
            AgentRole::User => "user",
            AgentRole::Assistant => "assistant",
            _ => "other",
        },
        store::StoredContent::Event(store::ConversationEvent::ModelChanged { .. }) => "model",
        store::StoredContent::Event(store::ConversationEvent::ChatMemoryChanged { .. }) => "memory",
        store::StoredContent::Event(store::ConversationEvent::ProposalDecided { .. }) => "decision",
    }
}

// ── The live record path (settings changed while a thread is open) ─────────────

#[tokio::test]
async fn record_slot_change_appends_an_event_per_changed_facet_and_stamps() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    let recorded = runtime
        .record_slot_change(id, "model-two", Some(16_000))
        .await
        .expect("record");
    assert_eq!(
        recorded.len(),
        2,
        "the model moved and the memory it buys moved with it"
    );

    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    let rows = store::list_messages(&conn, id, 10, 0).expect("list");
    assert_eq!(rows.len(), 2);
    for (row, event) in rows.iter().zip(&recorded) {
        assert_eq!(row.id, event.message_id);
        assert_eq!(row.seq, event.seq);
    }
    assert_eq!(
        rows[0].content,
        store::StoredContent::Event(store::ConversationEvent::ModelChanged {
            model: "model-two".to_string()
        })
    );
    assert_eq!(
        rows[1].content,
        store::StoredContent::Event(store::ConversationEvent::ChatMemoryChanged {
            chat_memory_tokens: 16_000
        })
    );
    assert_eq!(
        store::conversation_last_model(&conn, id).expect("get"),
        Some("model-two".to_string())
    );
    assert_eq!(
        store::conversation_last_chat_memory(&conn, id).expect("get"),
        Some(16_000)
    );
}

#[tokio::test]
async fn record_slot_change_records_only_the_facet_that_moved() {
    // Changing the chat memory size alone says so, and doesn't invent a model line.
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    let recorded = runtime
        .record_slot_change(id, "model-one", Some(16_000))
        .await
        .expect("record");
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0].event,
        store::ConversationEvent::ChatMemoryChanged {
            chat_memory_tokens: 16_000
        }
    );
}

#[tokio::test]
async fn record_slot_change_is_a_noop_when_unchanged_or_unstarted() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    // Same effective model and size (e.g. the changed setting was masked by the override,
    // or "Automatic" resolves to the same number): no rows.
    assert!(
        runtime
            .record_slot_change(id, "model-one", Some(STAMPED_CHAT_MEMORY))
            .await
            .expect("record")
            .is_empty()
    );

    // A conversation with no completed turn yet: nothing to switch from, no rows.
    let conn = store::open_write_connection(&runtime.db_path).expect("open");
    let fresh = store::create_conversation(&conn, "fresh", 100, None).expect("create");
    drop(conn);
    assert!(
        runtime
            .record_slot_change(fresh, "model-two", Some(16_000))
            .await
            .expect("record")
            .is_empty()
    );
    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    assert!(store::list_messages(&conn, fresh, 10, 0).expect("list").is_empty());
    assert_eq!(
        store::conversation_last_model(&conn, fresh).expect("get"),
        None,
        "the first real turn stamps the slot, not the settings change"
    );
    assert_eq!(store::conversation_last_chat_memory(&conn, fresh).expect("get"), None);
}

#[tokio::test]
async fn record_slot_change_stays_quiet_about_a_size_it_could_not_resolve() {
    // A local server whose window can't hold a turn has no honest budget to name, so the
    // caller passes `None` and the model line still lands on its own.
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    let recorded = runtime.record_slot_change(id, "model-two", None).await.expect("record");
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0].event,
        store::ConversationEvent::ModelChanged {
            model: "model-two".to_string()
        }
    );
    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    assert_eq!(
        store::conversation_last_chat_memory(&conn, id).expect("get"),
        Some(STAMPED_CHAT_MEMORY),
        "an unresolvable size leaves the stamp alone rather than clearing it"
    );
}

#[tokio::test]
async fn record_slot_change_waits_for_the_in_flight_turn() {
    let (_dir, runtime, id) = runtime_with_stamped_conversation();
    // Hold the conversation's single-flight lock like an in-flight turn does.
    let guard = runtime
        .locks
        .lock_for(id)
        .try_lock_owned()
        .expect("lock is free before the fake turn");

    let fut = runtime.record_slot_change(id, "model-two", Some(STAMPED_CHAT_MEMORY));
    tokio::pin!(fut);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut fut).await.is_err(),
        "the record waits while the turn is in flight"
    );

    drop(guard); // the turn finishes
    let recorded = fut.await.expect("record");
    assert_eq!(recorded.len(), 1);
    let conn = store::open_read_connection(&runtime.db_path).expect("open read");
    let rows = store::list_messages(&conn, id, 10, 0).expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].id, recorded[0].message_id,
        "the event landed only after the lock released"
    );
}

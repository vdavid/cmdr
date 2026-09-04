//! What a turn says about its own context: a budget-forced drop is announced once and
//! revokes the dropped result's standing as evidence, and the rail's fullness gauge is fed
//! one usage figure per turn from the assembly's own numbers. The fixtures come from the
//! sibling `test_support` module; the two oversized dispatchers below are used only here.

use super::test_support::*;
use super::*;

// ── A context drop is loud ─────────────────────────────────────────────────────

/// Returns one oversized tool result, big enough that keeping it in history blows a tight
/// prompt budget (the `image_facts` shape that started this).
struct HugeDispatcher;

impl ToolDispatcher for HugeDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "text": "y".repeat(60_000) }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }
}

/// Returns one oversized result AND records every evidence revocation the runtime asks for,
/// so a test can assert that a dropped result loses its standing as evidence.
#[derive(Default)]
struct RevokeRecordingDispatcher {
    revoked: Mutex<Vec<String>>,
}

impl RevokeRecordingDispatcher {
    fn revoked(&self) -> Vec<String> {
        self.revoked.lock().expect("lock").clone()
    }
}

impl ToolDispatcher for RevokeRecordingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            ToolDispatchOutcome {
                result: AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: json!({ "text": "y".repeat(60_000) }),
                    elided: false,
                },
                proposal: None,
            }
        }
        .boxed()
    }

    fn revoke_evidence(&self, call_ids: &[String]) {
        self.revoked.lock().expect("lock").extend_from_slice(call_ids);
    }
}

#[tokio::test]
async fn a_dropped_tool_result_loses_its_standing_as_evidence() {
    // The ledger backing rename evidence vouches for `image_facts` results that were
    // DISPATCHED; assembly decides which ones the model actually reads. So every result the
    // prompt drops must be revoked, or a plan could cite content the model never saw — the
    // original bug wearing a badge.
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = RevokeRecordingDispatcher::default();

    let first = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ImageFacts, json!({ "paths": ["/a.png"] }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["named them".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx1, _rx1) = unbounded_channel();
    run_turn(
        &first,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("name these by content")),
        &tx1,
        &CancellationToken::new(),
    )
    .await;
    assert!(
        dispatcher.revoked().is_empty(),
        "the turn that fetched the facts read them: nothing to revoke"
    );

    // A second turn on a tight budget: turn 1's result is history now, so it goes.
    let second = ProgrammableLlm::new(vec![Program::Answer {
        chunks: vec!["and the rest".to_string()],
        usage: AgentUsage::default(),
    }]);
    let (tx2, _rx2) = unbounded_channel();
    let mut tight = params(id, Some("now the rest"));
    tight.prompt_budget = 8_000;
    let result = run_turn(
        &second,
        &dispatcher,
        &conn,
        &[],
        &tight,
        &tx2,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, TurnResult::Answered { .. }), "the turn still answers");

    let revoked = dispatcher.revoked();
    assert_eq!(revoked.len(), 1, "exactly the dropped result is revoked: {revoked:?}");
    assert!(
        !revoked[0].is_empty(),
        "the revocation carries the call id the result answered"
    );
}

#[tokio::test]
async fn a_budget_forced_context_drop_is_announced_once_per_turn() {
    // Turn 1 puts a huge tool result in history; turn 2 runs against a tight budget, so
    // that result has to go. The user must be TOLD — an unannounced drop is exactly how a
    // reply written without the evidence read like a normal one.
    let conn = migrated_conn();
    let id = conversation(&conn);

    let first = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ImageFacts, json!({ "paths": ["/a.png"] }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["named them".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx1, _rx1) = unbounded_channel();
    let params1 = params(id, Some("name these by content"));
    run_turn(
        &first,
        &HugeDispatcher,
        &conn,
        &[],
        &params1,
        &tx1,
        &CancellationToken::new(),
    )
    .await;

    let second = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ImageFacts, json!({ "paths": ["/b.png"] }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["named those too".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx2, mut rx2) = unbounded_channel();
    let mut params2 = params(id, Some("now the rest"));
    params2.prompt_budget = 8_000;
    let result = run_turn(
        &second,
        &HugeDispatcher,
        &conn,
        &[],
        &params2,
        &tx2,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, TurnResult::Answered { .. }), "the turn still answers");

    let trims: Vec<AgentChatEvent> = drain(&mut rx2)
        .into_iter()
        .filter(|e| matches!(e, AgentChatEvent::ContextTrimmed { .. }))
        .collect();
    assert_eq!(
        trims.len(),
        1,
        "exactly one notice per turn, however many respond calls it took: {trims:?}"
    );
    let AgentChatEvent::ContextTrimmed {
        elided_results,
        approx_tokens,
    } = &trims[0]
    else {
        unreachable!("filtered above")
    };
    assert_eq!(*elided_results, 1, "the older batch is what went");
    assert!(*approx_tokens > 0, "the notice sizes what was dropped");
}

/// The gauge is driven by one event per turn carrying the assembly's own numbers. Two events
/// would make the bar jump mid-turn; zero would leave it stale after a turn that grew the chat.
#[tokio::test]
async fn context_usage_is_reported_once_per_turn_with_the_assemblys_numbers() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ImageFacts, json!({ "paths": ["/a.png"] }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["named it".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx, mut rx) = unbounded_channel();
    let mut turn_params = params(id, Some("name this"));
    turn_params.prompt_budget = 16_000;

    let result = run_turn(
        &llm,
        &HugeDispatcher,
        &conn,
        &[],
        &turn_params,
        &tx,
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, TurnResult::Answered { .. }), "the turn answers");

    let reports: Vec<AgentChatEvent> = drain(&mut rx)
        .into_iter()
        .filter(|e| matches!(e, AgentChatEvent::ContextUsage { .. }))
        .collect();
    assert_eq!(
        reports.len(),
        1,
        "exactly one usage report per turn, however many respond calls it took: {reports:?}"
    );
    let AgentChatEvent::ContextUsage {
        estimated_tokens,
        budget_tokens,
        ..
    } = &reports[0]
    else {
        unreachable!("filtered above")
    };
    assert_eq!(
        *budget_tokens, 16_000,
        "the report names the budget it was assembled against"
    );
    assert!(
        *estimated_tokens > 0,
        "a real assembly costs something; a zero would render as an empty bar"
    );

    // And it outlives the turn, so reopening the thread shows the last known figure.
    assert_eq!(
        store::conversation_context_usage(&conn, id).expect("read stored usage"),
        Some((*estimated_tokens, *budget_tokens)),
        "the figure the user saw is the figure the thread remembers"
    );
}

/// A turn that never finishes reports no usage: the user is looking at an error line, and the
/// previous turn's stored figure stays the last thing actually measured.
#[tokio::test]
async fn a_failed_turn_reports_no_usage_and_leaves_the_stored_figure_alone() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    store::set_conversation_context_usage(&conn, id, 4_200, 16_000).expect("seed a prior turn");

    let llm = ProgrammableLlm::new(vec![Program::ErrorAfterText {
        chunks: vec!["thinking".to_string()],
        error: AgentLlmError::Unavailable,
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
    assert!(matches!(result, TurnResult::Failed(_)), "the turn fails");

    assert!(
        !drain(&mut rx)
            .iter()
            .any(|e| matches!(e, AgentChatEvent::ContextUsage { .. })),
        "a failed turn measured nothing worth showing"
    );
    assert_eq!(
        store::conversation_context_usage(&conn, id).expect("read stored usage"),
        Some((4_200, 16_000)),
        "the last real measurement survives a failed turn"
    );
}

#[tokio::test]
async fn a_turn_that_fits_its_budget_announces_nothing() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let events = run_answer_turn(&conn, id, "model-one", "hello").await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, AgentChatEvent::ContextTrimmed { .. })),
        "no notice when nothing was dropped — it would cry wolf on every turn"
    );
}

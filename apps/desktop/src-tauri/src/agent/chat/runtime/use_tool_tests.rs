//! `use_tool` through a whole turn: what the runtime runs is the tool it names, so `admit`, the schema
//! gate, Stop, the repeat breaker, and the rail never know it happened, and what the provider reads back
//! is the call it sent.

use tauri::test::mock_app;

use super::dispatch::AppHandleDispatcher;
use super::test_support::*;
use super::*;
use crate::agent::chat::message_view::thread::{PageContext, project_page};
use crate::agent::chat::message_view::{MessageBlock, MessageView};
use crate::agent::line::{LinePath, ToolSubject};
use crate::agent::store::TurnContext;
use crate::agent::wake::WakeToolWatch;

fn use_tool(name: &str, params: Value) -> (ToolId, Value) {
    (ToolId::UseTool, json!({ "name": name, "params": params }))
}

/// One tool round per entry, then an answer.
fn llm_of(rounds: Vec<Vec<(ToolId, Value)>>) -> ProgrammableLlm {
    let mut programs: Vec<Program> = rounds
        .into_iter()
        .map(|calls| Program::Tools {
            calls,
            usage: AgentUsage::default(),
        })
        .collect();
    programs.push(Program::Answer {
        chunks: vec!["Done.".to_string()],
        usage: AgentUsage::default(),
    });
    ProgrammableLlm::new(programs)
}

/// What the last call of a turn read back for its last tool result.
fn last_answer(llm: &ProgrammableLlm) -> Value {
    let prompt = llm.calls_seen().into_iter().last().expect("a call");
    let last = prompt.last().expect("the transcript ends on a tool row");
    let AgentPart::ToolResult(answer) = last.parts.first().expect("a part") else {
        panic!("expected a tool result, got {:?}", last.parts.first());
    };
    answer.content.clone()
}

/// Run one turn against the real gate on `origin`, and hand back what the model read.
async fn through_the_gate(call: (ToolId, Value), origin: TurnOrigin) -> Value {
    let app = mock_app();
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = AppHandleDispatcher::new(app.handle().clone(), id);
    let llm = llm_of(vec![vec![call]]);
    let (tx, _rx) = unbounded_channel();
    let mut params = params(id, Some("do it"));
    params.origin = origin;
    let result = run_turn(&llm, &dispatcher, &conn, &[], &params, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");
    last_answer(&llm)
}

// ── The gate ──────────────────────────────────────────────────────────────────

/// `use_tool` is `Read`, so it can't be what `admit` sees: that would let a wake run anything.
#[tokio::test]
async fn admit_judges_the_tool_it_names_on_every_origin() {
    for origin in [TurnOrigin::Wake, TurnOrigin::Outcomes] {
        let answer = through_the_gate(use_tool("delete", json!({ "pane": "left" })), origin).await;
        assert_eq!(answer["refusal"], "refusedInBackground", "{answer}");
        assert_eq!(answer["requested"], "delete", "{answer}");
    }
    for origin in [TurnOrigin::Text, TurnOrigin::Resume, TurnOrigin::Wake] {
        let answer = through_the_gate(use_tool("dialog_confirm", json!({})), origin).await;
        assert_eq!(answer["refusal"], "devOnly", "{answer}");
        assert_eq!(answer["requested"], "dialog_confirm", "{answer}");
    }
}

/// The inner call answers to its own schema, and `use_tool`'s own closed one refuses anything beside
/// `name` and `params`, rather than acting on less than was asked.
#[tokio::test]
async fn the_schema_gate_reads_the_inner_call_and_a_stray_key_refuses_by_name() {
    let answer = through_the_gate(use_tool("tag", json!({ "notAParameter": true })), TurnOrigin::Text).await;
    assert_eq!(
        answer["data"]["unknownProperties"],
        json!(["notAParameter"]),
        "{answer}"
    );

    let flattened = (ToolId::UseTool, json!({ "name": "tag", "pane": "left" }));
    let answer = through_the_gate(flattened, TurnOrigin::Text).await;
    assert_eq!(answer["data"]["unknownProperties"], json!(["pane"]), "{answer}");
}

#[tokio::test]
async fn nesting_refuses_and_runs_nothing() {
    for target in ["use_tool", "tool_details"] {
        let answer = through_the_gate(use_tool(target, json!({ "name": "delete" })), TurnOrigin::Text).await;
        assert!(answer["problem"].as_str().is_some(), "{target}: {answer}");
    }
}

#[tokio::test]
async fn a_malformed_call_refuses_as_a_typed_problem() {
    for arguments in [
        json!({}),
        json!({ "name": "" }),
        json!({ "name": 3 }),
        json!({ "name": "list_dir", "params": "{\"path\":\"/tmp\"}" }),
        json!({ "name": "list_dir", "params": [1] }),
    ] {
        let answer = through_the_gate((ToolId::UseTool, arguments.clone()), TurnOrigin::Text).await;
        assert!(answer["problem"].as_str().is_some(), "{arguments}: {answer}");
    }
}

#[tokio::test]
async fn a_name_outside_the_table_refuses_like_any_other() {
    let answer = through_the_gate(use_tool("format_drive", json!({})), TurnOrigin::Text).await;
    assert_eq!(answer["available"], false, "{answer}");
    assert_eq!(answer["requested"], "format_drive", "{answer}");
}

/// The model can skip the wrapper: a deferred tool parses like any other, and runs the same way.
#[tokio::test]
async fn a_deferred_tool_called_directly_works_too() {
    let answer = through_the_gate((ToolId::Tag, json!({ "notAParameter": true })), TurnOrigin::Text).await;
    assert_eq!(
        answer["data"]["unknownProperties"],
        json!(["notAParameter"]),
        "{answer}"
    );
}

#[tokio::test]
async fn tool_details_runs_through_the_gate_on_a_wake_too() {
    let answer = through_the_gate(
        (
            ToolId::ToolDetails,
            json!({ "names": ["tag", "nope", "dialog_confirm"] }),
        ),
        TurnOrigin::Wake,
    )
    .await;
    assert_eq!(answer["tools"][0]["name"], "tag", "{answer}");
    assert!(answer["tools"][0]["inputSchema"]["properties"].is_object(), "{answer}");
    assert_eq!(answer["unknown"], json!(["nope"]), "{answer}");
    assert_eq!(answer["unavailable"][0]["refusal"], "devOnly", "{answer}");
}

// ── A refusal that teaches ────────────────────────────────────────────────────

/// A weak model never calls `tool_details`, so the first thing it reads about a deferred tool is what
/// it got wrong. That answer holds the schema, so one failed try is enough.
#[tokio::test]
async fn refused_params_on_a_deferred_tool_carry_its_schema() {
    for call in [
        // Missing what it needs, and a name it guessed.
        use_tool("tab", json!({ "whichPane": "right" })),
        // The wrapper skipped.
        (ToolId::Tab, json!({ "whichPane": "right" })),
        // Flattened beside the wrapper's own two keys.
        (
            ToolId::UseTool,
            json!({ "name": "tab", "action": "new", "pane": "left" }),
        ),
        // `params` as a string, which never unwraps.
        (
            ToolId::UseTool,
            json!({ "name": "tab", "params": "{\"action\":\"new\"}" }),
        ),
    ] {
        let answer = through_the_gate(call.clone(), TurnOrigin::Text).await;
        assert!(answer["problem"].is_string(), "{call:?}: {answer}");
        assert_eq!(answer["tool"]["name"], "tab", "{call:?}: {answer}");
        assert!(answer["tool"]["description"].is_string(), "{call:?}: {answer}");
        assert!(
            answer["tool"]["inputSchema"]["properties"]["action"]["enum"]
                .as_array()
                .is_some_and(|actions| actions.contains(&json!("new"))),
            "{call:?}: {answer}"
        );
    }
}

/// The gate reads only keys, so a value it can't judge comes back from the handler. That's the same
/// kind of mistake, and the model needs the same answer: in the running app, `Unknown tab action: open`.
#[tokio::test]
async fn a_wrong_value_only_the_handler_can_judge_carries_the_schema_too() {
    let answer = through_the_gate(
        use_tool("sort", json!({ "pane": "left", "by": "bigness", "order": "asc" })),
        TurnOrigin::Text,
    )
    .await;
    assert!(answer["problem"].is_string(), "{answer}");
    assert_eq!(answer["tool"]["name"], "sort", "{answer}");
}

/// A core tool's schema went out with the call that got it wrong, so repeating it buys nothing.
#[tokio::test]
async fn a_refused_core_tool_carries_no_schema() {
    let answer = through_the_gate((ToolId::ListDir, json!({ "notAParameter": true })), TurnOrigin::Text).await;
    assert!(answer["problem"].is_string(), "{answer}");
    assert!(answer.get("tool").is_none(), "{answer}");
}

/// A background turn hears it can't, in the gate's words, rather than how to try harder.
#[tokio::test]
async fn a_refusal_about_who_is_asking_carries_no_schema() {
    let answer = through_the_gate(use_tool("tab", json!({ "action": "new" })), TurnOrigin::Wake).await;
    assert_eq!(answer["refusal"], "refusedInBackground", "{answer}");
    assert!(answer.get("tool").is_none(), "{answer}");
}

/// A guessed name lands on the tool it was reaching for, schema included, by whichever road.
#[tokio::test]
async fn a_guessed_name_is_pointed_at_the_tool_it_was_reaching_for() {
    for call in [
        use_tool("new_tab", json!({ "pane": "left" })),
        (ToolId::from_wire_name("open_tab"), json!({ "pane": "left" })),
    ] {
        let answer = through_the_gate(call.clone(), TurnOrigin::Text).await;
        assert_eq!(answer["available"], false, "{call:?}: {answer}");
        assert_eq!(answer["closest"][0], "tab", "{call:?}: {answer}");
        assert_eq!(answer["tool"]["name"], "tab", "{call:?}: {answer}");
    }

    let answer = through_the_gate(use_tool("read_logs", json!({})), TurnOrigin::Text).await;
    assert_eq!(answer["closest"][0], "read_resource", "{answer}");
    assert_eq!(answer["tool"]["name"], "read_resource", "{answer}");
}

/// "Open a new tab" reads as a dialog to a weak model, and the refusal sends it to the right tool.
#[tokio::test]
async fn opening_a_tab_as_a_dialog_is_pointed_at_the_tab_tool() {
    let answer = through_the_gate(
        (ToolId::Dialog, json!({ "action": "open", "type": "tab" })),
        TurnOrigin::Text,
    )
    .await;
    assert!(
        answer["problem"]
            .as_str()
            .is_some_and(|problem| problem.contains("the tab tool")),
        "{answer}"
    );
}

/// Nothing close means nothing suggested: a wrong pointer is worse than none.
#[tokio::test]
async fn a_guess_like_nothing_cmdr_has_suggests_nothing() {
    let answer = through_the_gate(use_tool("zzz_qqq", json!({})), TurnOrigin::Text).await;
    assert_eq!(answer["available"], false, "{answer}");
    assert!(answer.get("closest").is_none(), "{answer}");
    assert!(answer.get("tool").is_none(), "{answer}");
}

// ── What the dispatcher sees ──────────────────────────────────────────────────

/// Records exactly what it was asked to run.
#[derive(Default)]
struct RecordingDispatcher {
    seen: Mutex<Vec<AgentToolCall>>,
}

impl ToolDispatcher for RecordingDispatcher {
    fn dispatch<'a>(&'a self, call: &'a AgentToolCall, _turn: CallContext<'a>) -> BoxFuture<'a, ToolDispatchOutcome> {
        async move {
            self.seen.lock().expect("lock").push(call.clone());
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

/// The evidence ledgers and `note_pane_listing` key on `call_id` and branch on `call.tool`, both from
/// what they're handed, so this is what makes them read `image_facts` through `use_tool`.
#[tokio::test]
async fn the_dispatcher_gets_the_inner_call_under_the_providers_call_id() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = RecordingDispatcher::default();
    let llm = llm_of(vec![vec![use_tool("image_facts", json!({ "paths": ["/a.png"] }))]]);
    let (tx, _rx) = unbounded_channel();
    run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("what's in it?")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    let seen = dispatcher.seen.lock().expect("lock").clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].tool, ToolId::ImageFacts);
    assert_eq!(seen[0].arguments, json!({ "paths": ["/a.png"] }));

    let prompt = llm.calls_seen().into_iter().last().expect("a second call");
    let result_call_id = prompt.iter().rev().find_map(|message| {
        message.parts.iter().find_map(|part| match part {
            AgentPart::ToolResult(result) => Some(result.call_id.clone()),
            _ => None,
        })
    });
    assert_eq!(result_call_id.as_deref(), Some(seen[0].call_id.as_str()));
}

/// A wake's sign-off through the wrapper is still a sign-off: the watch sits behind the unwrap.
#[tokio::test]
async fn a_wake_hears_nothing_to_suggest_through_use_tool() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let inner = OkDispatcher;
    let watch = WakeToolWatch::new(&inner);
    let llm = llm_of(vec![vec![use_tool("nothing_to_suggest", json!({ "reason": "churn" }))]]);
    let (tx, _rx) = unbounded_channel();
    let mut params = params(id, None);
    params.origin = TurnOrigin::Wake;
    run_turn(&llm, &watch, &conn, &[], &params, &tx, &CancellationToken::new()).await;
    assert!(watch.stayed_quiet());
    assert_eq!(watch.reason().as_deref(), Some("churn"));
}

// ── Stop ──────────────────────────────────────────────────────────────────────

/// Stop abandons by the class of what runs. `use_tool` is `Read`, which it may abandon, but a call
/// that's writing a note or holding a person's yes has to run to its own end.
#[tokio::test]
async fn stop_reads_the_class_of_the_inner_call() {
    for (inner, abandoned) in [("nav_to_parent", true), ("memory_write", false)] {
        let conn = migrated_conn();
        let id = conversation(&conn);
        let dispatcher = Arc::new(HeldDispatcher::until_released(json!({ "ok": true })));
        let cancel = CancellationToken::new();
        let stopper = {
            let (dispatcher, cancel) = (dispatcher.clone(), cancel.clone());
            tokio::spawn(async move {
                dispatcher.reached.notified().await;
                cancel.cancel();
                // Long enough for an abandoned call to have ended the turn without this.
                tokio::task::yield_now().await;
                dispatcher.release.notify_one();
            })
        };
        let llm = llm_of(vec![vec![use_tool(inner, json!({}))]]);
        let (tx, _rx) = unbounded_channel();
        let params = params(id, Some("go on"));
        run_turn(&llm, dispatcher.as_ref(), &conn, &[], &params, &tx, &cancel).await;
        stopper.await.expect("the stopper ran");

        let stored = store::list_messages(&conn, id, 100, 0).expect("list");
        let result = stored.iter().rev().find_map(|row| match &row.content {
            store::StoredContent::Message { parts, .. } => parts.iter().find_map(|part| match part {
                AgentPart::ToolResult(result) => Some(result.content.clone()),
                _ => None,
            }),
            _ => None,
        });
        let result = result.expect("the call keeps a result row");
        assert_eq!(result.get("miss").is_some(), abandoned, "{inner}: {result}");
    }
}

// ── The repeat breaker and the loop guard ─────────────────────────────────────

/// One failure, however it was sent: otherwise a model alternating the two ways gets past the breaker.
#[tokio::test]
async fn the_repeat_breaker_keys_on_the_inner_call() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = FailingDispatcher::default();
    let llm = llm_of(vec![
        vec![use_tool("tag", json!({ "pane": "left" }))],
        vec![(ToolId::Tag, json!({ "pane": "left" }))],
    ]);
    let (tx, _rx) = unbounded_channel();
    run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("tag it")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(dispatcher.dispatched().len(), 1, "the identical retry never ran");
    assert_eq!(dispatcher.dispatched()[0].0, "tag");
    assert_eq!(last_answer(&llm)["repeatedCall"], true);
}

/// `moves_the_turn_on` reads the inner call's class: `use_tool` itself is `Read`, so reading the
/// wrapper would end "go up three levels" as going in circles.
#[tokio::test]
async fn going_up_three_levels_through_use_tool_finishes() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = OkDispatcher;
    let up = || vec![use_tool("nav_to_parent", json!({}))];
    let llm = llm_of(vec![up(), up(), up(), up()]);
    let (tx, mut rx) = unbounded_channel();
    run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("up four")),
        &tx,
        &CancellationToken::new(),
    )
    .await;
    let events = drain(&mut rx);
    let finished = events
        .iter()
        .filter(|event| matches!(event, AgentChatEvent::ToolCallFinished { .. }))
        .count();
    assert_eq!(finished, 4);
    assert!(
        matches!(events.last(), Some(AgentChatEvent::Done { turn: Some(stats), .. }) if stats.limit.is_none()),
        "{:?}",
        events.last()
    );
}

// ── The rail, live and reloaded ───────────────────────────────────────────────

/// What the rail hears never names the wrapper: no `use_tool` line while its arguments stream, then
/// the tool it ran, with that tool's subject.
#[tokio::test]
async fn the_live_line_names_the_tool_that_ran() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = OkDispatcher;
    let llm = llm_of(vec![vec![use_tool("list_dir", json!({ "path": "/Users/me/work" }))]]);
    let (tx, mut rx) = unbounded_channel();
    run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("look")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    let events = drain(&mut rx);
    assert!(
        !events.iter().any(|event| matches!(
            event,
            AgentChatEvent::ToolCallStarted {
                tool: ToolId::UseTool,
                ..
            }
        )),
        "{events:?}"
    );
    let resolved = events
        .iter()
        .find_map(|event| match event {
            AgentChatEvent::ToolCallsResolved { calls, .. } => Some(calls.clone()),
            _ => None,
        })
        .expect("the round resolved");
    assert_eq!(resolved[0].tool, "list_dir");
    assert_eq!(
        resolved[0].subject,
        ToolSubject::Folder {
            path: LinePath::of("/Users/me/work")
        }
    );
    assert!(events.iter().any(|event| matches!(
        event,
        AgentChatEvent::ToolCallFinished {
            subject: ToolSubject::Folder { .. },
            ..
        }
    )));
}

/// A malformed call keeps its own name, since it ran as nothing else, and reads as a problem.
#[tokio::test]
async fn a_malformed_call_keeps_its_own_line() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = FailingDispatcher::default();
    let llm = llm_of(vec![vec![(ToolId::UseTool, json!({}))]]);
    let (tx, mut rx) = unbounded_channel();
    run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("look")),
        &tx,
        &CancellationToken::new(),
    )
    .await;
    let events = drain(&mut rx);
    let resolved = events
        .iter()
        .find_map(|event| match event {
            AgentChatEvent::ToolCallsResolved { calls, .. } => Some(calls.clone()),
            _ => None,
        })
        .expect("the round resolved");
    assert_eq!(resolved[0].tool, "use_tool");
}

fn tool_blocks(views: &[MessageView]) -> Vec<(String, ToolSubject)> {
    views
        .iter()
        .flat_map(|view| &view.blocks)
        .filter_map(|block| match block {
            MessageBlock::ToolCall { tool, subject, .. } => Some((tool.clone(), subject.clone())),
            _ => None,
        })
        .collect()
}

/// A reloaded thread draws what the live one did, from a row that keeps what the provider sent.
#[tokio::test]
async fn a_reloaded_row_keeps_the_stored_call_and_reads_as_the_tool_that_ran() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = OkDispatcher;
    let sent = use_tool("list_dir", json!({ "path": "/Users/me/work" }));
    let llm = llm_of(vec![vec![sent.clone()]]);
    let (tx, _rx) = unbounded_channel();
    run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("look")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    let rows = store::list_messages(&conn, id, 100, 0).expect("list");
    let stored = rows
        .iter()
        .find_map(|row| match &row.content {
            store::StoredContent::Message {
                role: AgentRole::Assistant,
                parts,
            } => parts.iter().find_map(|part| match part {
                AgentPart::ToolCall(call) => Some(call.clone()),
                _ => None,
            }),
            _ => None,
        })
        .expect("the assistant row holds the call");
    assert_eq!(stored.tool, ToolId::UseTool, "the row keeps what the provider sent");
    assert_eq!(stored.arguments, sent.1);

    let page = project_page(
        TurnContext::default(),
        rows,
        PageContext {
            in_flight_since_seq: None,
            memory_root: None,
        },
    );
    assert_eq!(
        tool_blocks(&page),
        vec![(
            "list_dir".to_string(),
            ToolSubject::Folder {
                path: LinePath::of("/Users/me/work")
            }
        )]
    );
}

/// What the provider reads back on the next call is the call it emitted, byte for byte: reasoning and
/// signatures replay against it, and a provider rejects a thread whose history changed under it.
#[tokio::test]
async fn the_next_call_replays_what_the_provider_sent() {
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = OkDispatcher;
    let sent = use_tool("list_dir", json!({ "path": "/Users/me/work" }));
    let llm = llm_of(vec![vec![sent.clone()]]);
    let (tx, _rx) = unbounded_channel();
    run_turn(
        &llm,
        &dispatcher,
        &conn,
        &[],
        &params(id, Some("look")),
        &tx,
        &CancellationToken::new(),
    )
    .await;

    let prompt = llm.calls_seen().into_iter().last().expect("a second call");
    let replayed = prompt
        .iter()
        .find_map(|message| {
            message.parts.iter().find_map(|part| match part {
                AgentPart::ToolCall(call) => Some(call.clone()),
                _ => None,
            })
        })
        .expect("the call replays");
    assert_eq!(replayed.tool, ToolId::UseTool);
    assert_eq!(
        serde_json::to_string(&replayed.arguments).expect("json"),
        serde_json::to_string(&sent.1).expect("json")
    );
}

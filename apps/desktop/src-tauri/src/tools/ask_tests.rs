//! Tests for `ask.rs`: every answer, the one-at-a-time rule, and the dispatch's gate, against a fake
//! person listening on the event the dialog listens on.

use std::sync::{Arc, Mutex};

use serde_json::json;
use tauri::test::{MockRuntime, mock_app};
use tauri::{App, Listener, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::test_support::wait_until_async;
use crate::tools::Caller;
use crate::tools::table::execute_tool;

fn app_without_window() -> App<MockRuntime> {
    let app = mock_app();
    app.manage(ToolAskRegistry::default());
    app.manage(SoftDialogTracker::new());
    app
}

fn app_with_window() -> App<MockRuntime> {
    let app = app_without_window();
    WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
        .build()
        .expect("a mock window");
    app
}

/// Answers every question with `response`, and keeps what it saw.
fn person_answering(app: &App<MockRuntime>, response: ToolAskResponse) -> Arc<Mutex<Vec<serde_json::Value>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (handle, sink) = (app.handle().clone(), seen.clone());
    app.listen_any("tool-ask-requested", move |event| {
        let request: serde_json::Value = serde_json::from_str(event.payload()).expect("a request");
        sink.lock().expect("no poison in a test").push(request.clone());
        let id = request["requestId"].as_str().expect("an id");
        handle.state::<ToolAskRegistry>().answer(id, response.clone());
    });
    seen
}

async fn ask_quit(app: &App<MockRuntime>, cancel: &CancellationToken, expiry: Duration) -> AskAnswer {
    ask_within(app.handle(), AskFacts::Quit, Initiator::AiClient, cancel, expiry).await
}

const LONG: Duration = Duration::from_secs(30);

#[tokio::test]
async fn each_answer_a_person_gives_comes_back() {
    for (response, expected) in [
        (ToolAskResponse::Allowed, AskAnswer::Allowed),
        (ToolAskResponse::Declined, AskAnswer::Declined),
        (ToolAskResponse::Gone, AskAnswer::NoWindow),
        (
            ToolAskResponse::Blocked { dialog: "about".into() },
            AskAnswer::Blocked("about".into()),
        ),
    ] {
        let app = app_with_window();
        person_answering(&app, response);
        assert_eq!(ask_quit(&app, &CancellationToken::new(), LONG).await, expected);
    }
}

#[tokio::test]
async fn a_question_nobody_answers_expires() {
    let app = app_with_window();
    let answer = ask_quit(&app, &CancellationToken::new(), Duration::from_millis(10)).await;
    assert_eq!(answer, AskAnswer::Expired);
}

#[tokio::test]
async fn stop_while_the_question_is_up_answers_cancelled_and_takes_it_down() {
    let app = app_with_window();
    let dismissed = Arc::new(Mutex::new(0));
    let count = dismissed.clone();
    app.listen_any("tool-ask-dismissed", move |_| {
        *count.lock().expect("no poison in a test") += 1;
    });
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(ask_quit(&app, &cancel, LONG).await, AskAnswer::Cancelled);
    assert_eq!(*dismissed.lock().expect("no poison in a test"), 1);
}

#[tokio::test]
async fn a_dropped_wait_takes_the_question_down_and_frees_the_slot() {
    let app = app_with_window();
    let dismissed = Arc::new(Mutex::new(0));
    let count = dismissed.clone();
    app.listen_any("tool-ask-dismissed", move |_| {
        *count.lock().expect("no poison in a test") += 1;
    });
    let cancel = CancellationToken::new();
    {
        let mut waiting = Box::pin(ask_quit(&app, &cancel, LONG));
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut waiting)
                .await
                .is_err()
        );
    }
    assert_eq!(*dismissed.lock().expect("no poison in a test"), 1);
    person_answering(&app, ToolAskResponse::Declined);
    assert_eq!(ask_quit(&app, &cancel, LONG).await, AskAnswer::Declined);
}

#[tokio::test]
async fn no_main_window_fails_closed() {
    let app = app_without_window();
    let answer = ask_quit(&app, &CancellationToken::new(), LONG).await;
    assert_eq!(answer, AskAnswer::NoWindow);
}

#[tokio::test]
async fn a_reloaded_window_wakes_the_wait_at_once() {
    let app = app_with_window();
    let cancel = CancellationToken::new();
    let waiting = ask_quit(&app, &cancel, LONG);
    let reload = async {
        wait_until_async(LONG, "the question to go up", || {
            app.state::<ToolAskRegistry>().lock().is_some()
        })
        .await;
        window_reloaded(app.handle());
    };
    let (answer, ()) = tokio::join!(waiting, reload);
    assert_eq!(answer, AskAnswer::NoWindow);
}

#[tokio::test]
async fn a_dialog_in_the_way_refuses_with_its_name() {
    let app = app_with_window();
    app.state::<SoftDialogTracker>().open("about".to_string());
    let answer = ask_quit(&app, &CancellationToken::new(), LONG).await;
    assert_eq!(answer, AskAnswer::Blocked("about".into()));
}

#[tokio::test]
async fn only_one_question_at_a_time() {
    let app = app_with_window();
    let cancel = CancellationToken::new();
    let first = ask_quit(&app, &cancel, LONG);
    let second = async {
        wait_until_async(LONG, "the question to go up", || {
            app.state::<ToolAskRegistry>().lock().is_some()
        })
        .await;
        let answer = ask_quit(&app, &cancel, LONG).await;
        cancel.cancel();
        answer
    };
    let (first, second) = tokio::join!(first, second);
    assert_eq!(first, AskAnswer::Cancelled);
    assert_eq!(second, AskAnswer::Blocked(TOOL_ASK_DIALOG.into()));
}

#[tokio::test]
async fn a_late_answer_to_an_old_question_lands_nowhere() {
    let app = app_with_window();
    let cancel = CancellationToken::new();
    let waiting = ask_quit(&app, &cancel, LONG);
    let late = async {
        wait_until_async(LONG, "the question to go up", || {
            app.state::<ToolAskRegistry>().lock().is_some()
        })
        .await;
        app.state::<ToolAskRegistry>()
            .answer("an-old-id", ToolAskResponse::Allowed);
        assert!(
            app.state::<ToolAskRegistry>().lock().is_some(),
            "the open question stays open"
        );
        cancel.cancel();
    };
    let (answer, ()) = tokio::join!(waiting, late);
    assert_eq!(answer, AskAnswer::Cancelled);
}

// ── The gate ────────────────────────────────────────────────────────────────

fn external(direct: bool) -> ToolCtx {
    ToolCtx {
        caller: Caller::External { direct },
        cancel: CancellationToken::new(),
        conversation_id: None,
        asked: None,
    }
}

fn agent() -> ToolCtx {
    ToolCtx {
        caller: Caller::Agent {
            origin: crate::agent::types::TurnOrigin::Text,
        },
        ..external(false)
    }
}

fn eject_facts(name: &str) -> AskFacts {
    AskFacts::Eject {
        volume_id: "disk2".into(),
        name: name.into(),
        whole_disk: false,
    }
}

/// `plan` answers what it's given, in order, then panics: a plan the gate ran too often is a bug.
fn plans(answers: Vec<AskPlan>) -> impl Fn() -> std::future::Ready<AskPlan> {
    let answers = Mutex::new(answers.into_iter());
    move || {
        std::future::ready(
            answers
                .lock()
                .expect("no poison in a test")
                .next()
                .expect("the gate ran the plan too often"),
        )
    }
}

#[tokio::test]
async fn no_ask_runs_straight_through() {
    let app = app_with_window();
    let seen = person_answering(&app, ToolAskResponse::Declined);
    let asked = gate(app.handle(), &agent(), plans(vec![AskPlan::NoAsk]))
        .await
        .expect("no ask");
    assert_eq!(asked, None);
    assert!(seen.lock().expect("no poison in a test").is_empty());
}

#[tokio::test]
async fn a_refusing_plan_refuses_before_anyone_is_asked() {
    let app = app_with_window();
    let seen = person_answering(&app, ToolAskResponse::Allowed);
    let refusal = ToolError::invalid_params("no").with_data(json!({ "refusal": "notSettableOverMcp" }));
    let err = gate(app.handle(), &agent(), plans(vec![AskPlan::Refuse(refusal)]))
        .await
        .expect_err("refused");
    assert_eq!(err.data, Some(json!({ "refusal": "notSettableOverMcp" })));
    assert!(seen.lock().expect("no poison in a test").is_empty());
}

#[tokio::test]
async fn a_yes_on_what_is_still_true_goes_ahead_with_those_facts() {
    let app = app_with_window();
    person_answering(&app, ToolAskResponse::Allowed);
    let asked = gate(
        app.handle(),
        &agent(),
        plans(vec![
            AskPlan::Ask(eject_facts("Backup")),
            AskPlan::Ask(eject_facts("Backup")),
        ]),
    )
    .await
    .expect("allowed");
    assert_eq!(asked, Some(eject_facts("Backup")));
}

#[tokio::test]
async fn a_yes_on_what_changed_refuses() {
    for after in [AskPlan::Ask(eject_facts("Something else")), AskPlan::NoAsk] {
        let app = app_with_window();
        person_answering(&app, ToolAskResponse::Allowed);
        let err = gate(
            app.handle(),
            &agent(),
            plans(vec![AskPlan::Ask(eject_facts("Backup")), after]),
        )
        .await
        .expect_err("changed");
        assert_eq!(err.data, Some(json!({ "refusal": "changedWhileAsking" })));
    }
}

#[tokio::test]
async fn each_other_answer_is_a_typed_refusal() {
    assert_eq!(
        refusal_for(AskAnswer::Declined).data,
        Some(json!({ "refusal": "declined" }))
    );
    assert_eq!(
        refusal_for(AskAnswer::Expired).data,
        Some(json!({ "refusal": "expired" }))
    );
    assert_eq!(
        refusal_for(AskAnswer::Cancelled).data,
        Some(json!({ "refusal": "cancelled" }))
    );
    assert_eq!(
        refusal_for(AskAnswer::NoWindow).data,
        Some(json!({ "refusal": "noWindow" }))
    );
    assert_eq!(
        refusal_for(AskAnswer::Blocked("about".into())).data,
        Some(json!({ "blockingDialog": "about" }))
    );
}

// ── Through the dispatch ────────────────────────────────────────────────────

#[tokio::test]
async fn an_outside_client_off_direct_mode_gets_the_question_for_quit_and_a_decline_is_typed() {
    let app = app_with_window();
    let seen = person_answering(&app, ToolAskResponse::Declined);
    let err = execute_tool(app.handle(), &external(false), "quit", &json!({}))
        .await
        .expect_err("declined");
    assert_eq!(err.data, Some(json!({ "refusal": "declined" })));
    let seen = seen.lock().expect("no poison in a test");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["facts"], json!({ "kind": "quit" }));
    assert_eq!(seen[0]["startedBy"], json!("aiClient"));
}

#[tokio::test]
async fn direct_mode_reaches_the_handler_with_no_question() {
    let app = app_with_window();
    let seen = person_answering(&app, ToolAskResponse::Declined);
    // A name nothing's saved under, so the handler answers and nothing changes.
    let err = execute_tool(
        app.handle(),
        &external(true),
        "remove_manual_server",
        &json!({ "hostId": "nothing-saved" }),
    )
    .await
    .expect_err("the handler's own refusal");
    assert_eq!(err.data, None);
    assert!(seen.lock().expect("no poison in a test").is_empty());
}

#[tokio::test]
async fn a_malformed_call_never_reaches_a_person() {
    let app = app_with_window();
    let seen = person_answering(&app, ToolAskResponse::Allowed);
    let outcome = execute_tool(app.handle(), &external(false), "eject", &json!({})).await;
    assert!(outcome.is_err());
    assert!(seen.lock().expect("no poison in a test").is_empty());
}

// ── Facts as a person reads them ────────────────────────────────────────────

#[test]
fn fact_text_keeps_short_plain_text() {
    assert_eq!(fact_text("report <b>final</b>.pdf"), "report <b>final</b>.pdf");
}

#[test]
fn fact_text_drops_what_would_make_a_name_read_as_something_else() {
    assert_eq!(fact_text("cod\u{202E}fdp.command"), "codfdp.command");
    assert_eq!(fact_text("a\nb\u{2066}c"), "abc");
}

#[test]
fn a_long_fact_keeps_its_end_where_the_extension_lives() {
    let long = format!("{}.command", "a".repeat(10_000));
    let shown = fact_text(&long);
    assert!(shown.chars().count() <= MAX_FACT_CHARS + 1);
    assert!(shown.ends_with(".command"));
    assert!(shown.contains('…'));
}

//! Tests for `dialog_confirm.rs`: the round-trip, foreground and background, and the names.

use super::*;
use tauri::Emitter;

/// A mock app carrying the two stores the ack signals read.
fn app_with_stores() -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_app();
    app.manage(crate::ui_state::pane_state::PaneStateStore::new());
    app.manage(SoftDialogTracker::new());
    app
}

#[tokio::test]
async fn a_confirm_acks_once_its_dialog_is_gone_with_no_pane_change() {
    for dialog_type in ["transfer-confirmation", "delete-confirmation"] {
        let app = app_with_stores();
        let handle = app.handle().clone();
        handle.state::<SoftDialogTracker>().open(dialog_type.to_string());
        answer_confirm(&handle, dialog_type, json!({ "ok": true, "operationId": "op-1" }));

        // Pre-fix this waited for a pane-state push, which a compress or a copy
        // onto a slow volume doesn't make for seconds: the tool answered "not
        // acknowledged" about an operation that had started.
        let outcome = execute_dialog_confirm(&handle, &json!({ "type": dialog_type })).await;
        assert!(outcome.is_ok(), "{dialog_type}: {:?}", outcome.err().map(|e| e.message));
    }
}

#[tokio::test]
async fn a_confirm_reports_the_started_operation_id() {
    for (dialog_type, background) in [
        ("transfer-confirmation", false),
        ("delete-confirmation", false),
        ("transfer-confirmation", true),
        ("delete-confirmation", true),
    ] {
        let app = app_with_stores();
        let handle = app.handle().clone();
        handle.state::<SoftDialogTracker>().open(dialog_type.to_string());
        answer_confirm(&handle, dialog_type, json!({ "ok": true, "operationId": "op-42" }));

        let outcome = execute_dialog_confirm(&handle, &json!({ "type": dialog_type, "background": background }))
            .await
            .unwrap_or_else(|e| panic!("{dialog_type} background={background}: {}", e.message));

        let text = outcome.as_str().expect("an OK line");
        assert!(
            text.contains("(operationId: op-42)"),
            "{dialog_type} background={background}: {text}"
        );
    }
}

#[tokio::test]
async fn a_confirm_whose_start_was_refused_reports_no_operation_id() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open("transfer-confirmation".to_string());
    answer_confirm(
        &handle,
        "transfer-confirmation",
        json!({ "ok": false, "error": "Destination is read-only" }),
    );

    let outcome =
        execute_dialog_confirm(&handle, &json!({ "type": "transfer-confirmation", "background": true })).await;

    let error = outcome.expect_err("a refused start started nothing");
    assert!(error.message.contains("Destination is read-only"), "{}", error.message);
    assert!(!error.message.contains("operationId"), "{}", error.message);
}

#[tokio::test]
async fn a_confirm_the_dialog_did_not_act_on_is_refused_typed() {
    for background in [false, true] {
        let app = app_with_stores();
        let handle = app.handle().clone();
        handle
            .state::<SoftDialogTracker>()
            .open("delete-confirmation".to_string());
        answer_confirm(
            &handle,
            "delete-confirmation",
            json!({ "ok": false, "refusal": "notStarted" }),
        );

        let outcome = execute_dialog_confirm(
            &handle,
            &json!({ "type": "delete-confirmation", "background": background }),
        )
        .await;

        let error = outcome.expect_err("the dialog started nothing");
        assert_eq!(error.code, ToolError::invalid_params("").code);
        assert_eq!(
            error.data,
            Some(json!({ "refusal": "notStarted" })),
            "background={background}"
        );
    }
}

#[tokio::test]
async fn a_foreground_confirm_the_start_gate_refused_names_the_blocking_dialog() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open("transfer-confirmation".to_string());
    answer_confirm(
        &handle,
        "transfer-confirmation",
        json!({ "ok": false, "blockedBy": "transfer-progress", "error": "in the way" }),
    );

    let outcome = execute_dialog_confirm(&handle, &json!({ "type": "transfer-confirmation" })).await;

    let error = outcome.expect_err("the progress slot is taken");
    assert_eq!(error.data, Some(json!({ "blockingDialog": "transfer-progress" })));
}

#[tokio::test]
async fn a_confirm_with_no_such_dialog_open_is_refused_up_front() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let started = std::time::Instant::now();
    let outcome = execute_dialog_confirm(&handle, &json!({ "type": "transfer-confirmation" })).await;
    let error = outcome.expect_err("nothing is open to confirm");
    assert_eq!(error.code, ToolError::invalid_params("").code);
    assert!(
        started.elapsed() < DEFAULT_ACK_TIMEOUT,
        "it must not wait out the ack budget"
    );
}

/// Stands in for the frontend's answer to a confirm: replies on `mcp-response`
/// with what the press, and the start it led to, did, and takes the dialog down
/// when it pressed (a start the gate refused takes it down too). Returns the
/// payloads it saw.
fn answer_confirm(
    app: &AppHandle<tauri::test::MockRuntime>,
    dialog_type: &'static str,
    reply: Value,
) -> std::sync::Arc<std::sync::Mutex<Vec<Value>>> {
    use tauri::Listener;
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_in_listener = seen.clone();
    let handle = app.clone();
    app.listen("mcp-confirm-dialog", move |event| {
        let payload: Value = serde_json::from_str(event.payload()).expect("a JSON payload");
        seen_in_listener.lock().expect("unpoisoned").push(payload.clone());
        let mut response = reply.clone();
        response["requestId"] = payload["requestId"].clone();
        if response["ok"] == json!(true) || response.get("blockedBy").is_some() {
            handle.state::<SoftDialogTracker>().close(dialog_type);
        }
        handle.emit("mcp-response", response).expect("emit the reply");
    });
    seen
}

#[tokio::test]
async fn a_background_confirm_presses_the_background_button_and_acks_once_the_dialog_is_gone() {
    for dialog_type in ["transfer-confirmation", "delete-confirmation"] {
        let app = app_with_stores();
        let handle = app.handle().clone();
        handle.state::<SoftDialogTracker>().open(dialog_type.to_string());
        let seen = answer_confirm(&handle, dialog_type, json!({ "ok": true, "operationId": "op-1" }));

        let outcome = execute_dialog_confirm(&handle, &json!({ "type": dialog_type, "background": true })).await;

        assert!(outcome.is_ok(), "{dialog_type}: {:?}", outcome.err().map(|e| e.message));
        let payloads = seen.lock().expect("unpoisoned");
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["startInBackground"], json!(true), "{dialog_type}");
    }
}

#[tokio::test]
async fn a_background_confirm_of_a_permanent_delete_is_refused_typed() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open("delete-confirmation".to_string());
    answer_confirm(
        &handle,
        "delete-confirmation",
        json!({ "ok": false, "refusal": "permanentDelete" }),
    );

    let outcome = execute_dialog_confirm(&handle, &json!({ "type": "delete-confirmation", "background": true })).await;

    let error = outcome.expect_err("a permanent delete never starts out of sight");
    assert_eq!(error.code, ToolError::invalid_params("").code);
    assert_eq!(error.data, Some(json!({ "refusal": "permanentDelete" })));
    // The dialog is still up for the agent to confirm in the foreground or close.
    assert!(
        handle
            .state::<SoftDialogTracker>()
            .get_open_types()
            .iter()
            .any(|open| open == "delete-confirmation")
    );
}

#[tokio::test]
async fn a_background_confirm_of_a_dialog_with_no_background_is_refused_up_front() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open("quit-confirmation".to_string());

    let outcome = execute_dialog_confirm(&handle, &json!({ "type": "quit-confirmation", "background": true })).await;

    let error = outcome.expect_err("only transfer and trash run in the background");
    assert_eq!(error.code, ToolError::invalid_params("").code);
}

#[test]
fn a_confirm_reply_keeps_its_operation_id_and_typed_refusals() {
    let parse = |payload: Value| parse_confirm_response(&payload.to_string(), "req-1");
    assert_eq!(
        parse(json!({ "requestId": "req-1", "ok": true, "operationId": "op-9" })),
        Some(Ok(ConfirmAck::Started(OperationStartAck::Started {
            operation_id: Some("op-9".to_string())
        })))
    );
    // A move that renamed in place: confirmed, with no operation to name.
    assert_eq!(
        parse(json!({ "requestId": "req-1", "ok": true })),
        Some(Ok(ConfirmAck::Started(OperationStartAck::Started {
            operation_id: None
        })))
    );
    assert_eq!(
        parse(json!({ "requestId": "req-1", "ok": false, "blockedBy": "transfer-progress" })),
        Some(Ok(ConfirmAck::Started(OperationStartAck::Blocked {
            blocking_dialog: "transfer-progress".to_string()
        })))
    );
    assert_eq!(
        parse(json!({ "requestId": "req-1", "ok": false, "refusal": "permanentDelete" })),
        Some(Ok(ConfirmAck::RefusedPermanentDelete))
    );
    assert_eq!(
        parse(json!({ "requestId": "req-1", "ok": false, "refusal": "notStarted" })),
        Some(Ok(ConfirmAck::NotStarted))
    );
    assert_eq!(
        parse(json!({ "requestId": "req-1", "ok": false, "error": "no explorer" })),
        Some(Err("no explorer".to_string()))
    );
    // A missing `ok` is a failure, never a false press.
    assert_eq!(
        parse(json!({ "requestId": "req-1" })),
        Some(Err("Unknown error".to_string()))
    );
    assert_eq!(parse(json!({ "requestId": "someone-else", "ok": true })), None);
}

/// Stands in for a delete dialog whose press waits on a cloud folder's walk: it
/// asks the round-trip to hold for `hold_ms`, and only after `answer_after`
/// (virtual time) does the start answer with `reply`, if at all.
fn answer_confirm_after_a_hold(
    app: &AppHandle<tauri::test::MockRuntime>,
    hold_ms: u64,
    answer_after: std::time::Duration,
    reply: Option<Value>,
) {
    use tauri::Listener;
    let handle = app.clone();
    app.listen("mcp-confirm-dialog", move |event| {
        let payload: Value = serde_json::from_str(event.payload()).expect("a JSON payload");
        let request_id = payload["requestId"].clone();
        handle
            .emit("mcp-response", json!({ "requestId": request_id, "holdMs": hold_ms }))
            .expect("emit the hold");
        let Some(reply) = reply.clone() else { return };
        let handle = handle.clone();
        tokio::spawn(async move {
            // allowed-test-sleep: the walk's answer, in virtual time on a start_paused runtime.
            tokio::time::sleep(answer_after).await;
            let mut response = reply;
            response["requestId"] = request_id;
            handle.state::<SoftDialogTracker>().close("delete-confirmation");
            handle.emit("mcp-response", response).expect("emit the reply");
        });
    });
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_confirm_that_waits_on_a_cloud_walk_still_reports_the_operation_id() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open("delete-confirmation".to_string());
    // Well past the 10 s start budget, well inside the hold the dialog asked for.
    answer_confirm_after_a_hold(
        &handle,
        90_000,
        std::time::Duration::from_secs(60),
        Some(json!({ "ok": true, "operationId": "op-7" })),
    );

    let outcome = execute_dialog_confirm(&handle, &json!({ "type": "delete-confirmation" }))
        .await
        .unwrap_or_else(|e| panic!("{}", e.message));

    let text = outcome.as_str().expect("an OK line");
    assert!(text.contains("(operationId: op-7)"), "{text}");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_held_confirm_that_never_starts_times_out_after_the_hold_plus_the_start_budget() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open("delete-confirmation".to_string());
    answer_confirm_after_a_hold(&handle, 30_000, std::time::Duration::ZERO, None);

    let started = tokio::time::Instant::now();
    let outcome = execute_dialog_confirm(&handle, &json!({ "type": "delete-confirmation" })).await;

    outcome.expect_err("nothing ever started");
    let waited = started.elapsed();
    assert!(waited >= std::time::Duration::from_secs(40), "{waited:?}");
    assert!(waited < std::time::Duration::from_secs(41), "{waited:?}");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_hold_past_the_ceiling_is_capped() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open("delete-confirmation".to_string());
    answer_confirm_after_a_hold(&handle, u64::MAX, std::time::Duration::ZERO, None);

    let started = tokio::time::Instant::now();
    let outcome = execute_dialog_confirm(&handle, &json!({ "type": "delete-confirmation" })).await;

    outcome.expect_err("nothing ever started");
    let ceiling = crate::tools::actions::MAX_ROUND_TRIP_HOLD + std::time::Duration::from_secs(11);
    assert!(started.elapsed() <= ceiling, "{:?}", started.elapsed());
}

#[tokio::test]
async fn no_policy_named_leaves_the_dialog_its_own() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle.state::<SoftDialogTracker>().open(TRANSFER.to_string());
    let seen = answer_confirm(&handle, TRANSFER, json!({ "ok": true, "operationId": "op-1" }));

    execute_dialog_confirm(&handle, &json!({ "type": TRANSFER }))
        .await
        .unwrap_or_else(|e| panic!("{}", e.message));

    let payloads = seen.lock().expect("unpoisoned");
    assert!(
        payloads[0].get("onConflict").is_none(),
        "a default here would override what the dialog shows: {}",
        payloads[0]
    );
}

#[tokio::test]
async fn a_named_policy_rides_the_press() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle.state::<SoftDialogTracker>().open(TRANSFER.to_string());
    let seen = answer_confirm(&handle, TRANSFER, json!({ "ok": true, "operationId": "op-1" }));

    execute_dialog_confirm(&handle, &json!({ "type": TRANSFER, "onConflict": "stop" }))
        .await
        .unwrap_or_else(|e| panic!("{}", e.message));

    assert_eq!(seen.lock().expect("unpoisoned")[0]["onConflict"], json!("stop"));
}

#[tokio::test]
async fn a_bad_policy_refuses_whatever_the_dialog() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    for dialog_type in [TRANSFER, DELETE, RENAME_EDITOR, "quit-confirmation", "no-such-dialog"] {
        handle.state::<SoftDialogTracker>().open(dialog_type.to_string());
        let error = execute_dialog_confirm(&handle, &json!({ "type": dialog_type, "onConflict": "keep_both" }))
            .await
            .expect_err("no such policy");
        assert!(error.message.contains("onConflict"), "{dialog_type}: {}", error.message);
    }
}

#[tokio::test]
async fn a_name_commits_the_editor_or_dialog_and_acks_once_it_is_gone() {
    for dialog_type in [RENAME_EDITOR, NEW_FOLDER, NEW_FILE] {
        let app = app_with_stores();
        let handle = app.handle().clone();
        handle.state::<SoftDialogTracker>().open(dialog_type.to_string());
        let seen = answer_confirm(&handle, dialog_type, json!({ "ok": true, "committed": true }));

        execute_dialog_confirm(&handle, &json!({ "type": dialog_type, "name": "notes.txt" }))
            .await
            .unwrap_or_else(|e| panic!("{dialog_type}: {}", e.message));

        assert_eq!(seen.lock().expect("unpoisoned")[0]["name"], json!("notes.txt"));
    }
}

#[tokio::test]
async fn a_slow_volume_answers_ok_and_says_so_with_the_dialog_still_up() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle.state::<SoftDialogTracker>().open(NEW_FOLDER.to_string());
    answer_confirm_keeping_it_open(&handle, json!({ "ok": true, "committed": true, "stillRunning": true }));
    let started = std::time::Instant::now();

    let outcome = execute_dialog_confirm(&handle, &json!({ "type": NEW_FOLDER, "name": "slow" }))
        .await
        .unwrap_or_else(|e| panic!("{}", e.message));

    assert!(outcome.as_str().expect("an OK line").contains("still running"));
    assert!(
        started.elapsed() < DEFAULT_ACK_TIMEOUT,
        "it must not wait for the dialog"
    );
}

#[tokio::test]
async fn a_refused_name_is_typed_and_leaves_the_editor_open() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle.state::<SoftDialogTracker>().open(RENAME_EDITOR.to_string());
    answer_confirm_keeping_it_open(
        &handle,
        json!({ "ok": false, "refusal": "nameRefused", "reason": "That name is taken." }),
    );

    let error = execute_dialog_confirm(&handle, &json!({ "type": RENAME_EDITOR, "name": "taken.txt" }))
        .await
        .expect_err("the name didn't stick");

    assert_eq!(error.data, Some(json!({ "refusal": "nameRefused" })));
    assert!(error.message.contains("That name is taken."), "{}", error.message);
    let open = handle.state::<SoftDialogTracker>().get_open_types();
    assert!(open.iter().any(|id| id == RENAME_EDITOR));
}

#[tokio::test]
async fn a_rename_that_needs_the_move_dialog_says_so_typed() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle.state::<SoftDialogTracker>().open(RENAME_EDITOR.to_string());
    answer_confirm_keeping_it_open(&handle, json!({ "ok": false, "refusal": "needsMoveDialog" }));

    let error = execute_dialog_confirm(&handle, &json!({ "type": RENAME_EDITOR, "name": "big" }))
        .await
        .expect_err("the Move dialog owns it now");

    assert_eq!(error.data, Some(json!({ "refusal": "needsMoveDialog" })));
}

/// A dispatch keeps the wire shape a poll loop reads, with its `inverseOpId`.
#[tokio::test]
async fn confirming_a_rollback_answers_with_the_dispatch() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open(ROLLBACK_CONFIRMATION.to_string());
    answer_confirm(
        &handle,
        ROLLBACK_CONFIRMATION,
        json!({ "ok": true, "rollback": { "status": "dispatched", "inverseOpId": "op-inverse" } }),
    );

    let answer = execute_dialog_confirm(&handle, &json!({ "type": ROLLBACK_CONFIRMATION }))
        .await
        .unwrap();
    assert_eq!(answer["status"], json!("dispatched"));
    assert_eq!(answer["inverseOpId"], json!("op-inverse"));
    assert!(answer["note"].as_str().unwrap().contains("operations_get"), "{answer}");
}

/// A running operation stops in place, so there's no inverse to poll, and the answer says what to watch.
#[tokio::test]
async fn confirming_a_running_rollback_says_to_await_the_operation() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open(ROLLBACK_CONFIRMATION.to_string());
    answer_confirm(
        &handle,
        ROLLBACK_CONFIRMATION,
        json!({ "ok": true, "rollback": { "status": "dispatched", "inverseOpId": null } }),
    );

    let answer = execute_dialog_confirm(&handle, &json!({ "type": ROLLBACK_CONFIRMATION }))
        .await
        .unwrap();
    assert_eq!(answer["inverseOpId"], Value::Null);
    assert!(
        answer["note"].as_str().unwrap().contains("operation_complete"),
        "{answer}"
    );
}

/// A refusal stays the typed body, ❌ never a sentence and ❌ never a reported success.
#[tokio::test]
async fn a_refused_rollback_keeps_its_typed_body() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open(ROLLBACK_CONFIRMATION.to_string());
    let refused = json!({ "status": "refused", "refusal": { "kind": "alreadyRolledBack" } });
    answer_confirm(
        &handle,
        ROLLBACK_CONFIRMATION,
        json!({ "ok": true, "rollback": refused }),
    );

    let answer = execute_dialog_confirm(&handle, &json!({ "type": ROLLBACK_CONFIRMATION }))
        .await
        .unwrap();
    assert_eq!(answer, refused);
}

#[tokio::test]
async fn confirming_a_rollback_with_none_open_is_refused_up_front() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let error = execute_dialog_confirm(&handle, &json!({ "type": ROLLBACK_CONFIRMATION }))
        .await
        .expect_err("nothing is open");
    assert!(!error.outcome_is_unknown(), "{}", error.message);
}

#[tokio::test]
async fn confirming_a_name_with_nothing_open_is_refused_up_front() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    for dialog_type in [RENAME_EDITOR, NEW_FOLDER, NEW_FILE] {
        let started = std::time::Instant::now();
        execute_dialog_confirm(&handle, &json!({ "type": dialog_type, "name": "x" }))
            .await
            .expect_err("nothing is open");
        assert!(started.elapsed() < DEFAULT_ACK_TIMEOUT, "{dialog_type}");
    }
}

#[tokio::test]
async fn a_name_goes_only_where_a_name_goes_and_must_be_a_name() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle.state::<SoftDialogTracker>().open(DELETE.to_string());
    handle.state::<SoftDialogTracker>().open(NEW_FILE.to_string());
    for params in [
        json!({ "type": DELETE, "name": "x" }),
        json!({ "type": NEW_FILE, "name": "  " }),
        json!({ "type": NEW_FILE, "name": "a/b" }),
    ] {
        let started = std::time::Instant::now();
        execute_dialog_confirm(&handle, &params).await.expect_err("refused");
        assert!(started.elapsed() < DEFAULT_ACK_TIMEOUT, "{params}");
    }
}

#[tokio::test]
async fn archive_password_still_points_to_unlock() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let error = execute_dialog_confirm(&handle, &json!({ "type": "archive-password" }))
        .await
        .expect_err("the unlock tool answers it");
    assert!(error.message.contains("unlock"), "{}", error.message);
}

#[tokio::test]
async fn it_refuses_an_outside_client_off_direct_mode_with_the_typed_reason() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let error = crate::tools::execute_tool(
        &handle,
        &crate::tools::ToolCtx::external(tokio_util::sync::CancellationToken::new()),
        "dialog_confirm",
        &json!({ "type": TRANSFER }),
    )
    .await
    .expect_err("this isn't an E2E run, and nothing asked for direct mode");
    assert_eq!(error.data, Some(json!({ "reason": "directModeOff" })));
}

#[tokio::test]
async fn the_agent_never_reaches_it() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle.state::<SoftDialogTracker>().open(TRANSFER.to_string());
    let error = crate::tools::execute_tool(
        &handle,
        &crate::tools::ToolCtx {
            caller: crate::tools::Caller::Agent {
                origin: crate::agent::types::TurnOrigin::Text,
            },
            cancel: tokio_util::sync::CancellationToken::new(),
            conversation_id: None,
            asked: None,
        },
        "dialog_confirm",
        &json!({ "type": TRANSFER }),
    )
    .await
    .expect_err("it IS the human step");
    assert_eq!(error.data, Some(json!({ "refusal": "devOnly" })));
}

/// Answers a confirm without taking anything down: a refusal, or a volume still working.
fn answer_confirm_keeping_it_open(app: &AppHandle<tauri::test::MockRuntime>, reply: Value) {
    use tauri::Listener;
    let handle = app.clone();
    app.listen("mcp-confirm-dialog", move |event| {
        let payload: Value = serde_json::from_str(event.payload()).expect("a JSON payload");
        let mut response = reply.clone();
        response["requestId"] = payload["requestId"].clone();
        handle.emit("mcp-response", response).expect("emit the reply");
    });
}

#[tokio::test]
async fn a_direct_client_answers_a_tools_question_and_the_ack_waits_for_it_to_come_down() {
    use crate::tools::ask::{AskAnswer, AskFacts, ToolAskRegistry, ask};
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let app = app_with_stores();
    app.manage(ToolAskRegistry::default());
    WebviewWindowBuilder::new(&app, "main", WebviewUrl::default())
        .build()
        .expect("a mock window");
    let handle = app.handle().clone();
    let cancel = tokio_util::sync::CancellationToken::new();
    let waiting = ask(
        &handle,
        AskFacts::Quit,
        cmdr_operation_log::types::Initiator::AiClient,
        &cancel,
    );
    // Polled after `waiting`, so the question is up by the time this looks for it.
    let confirm = async {
        handle.state::<SoftDialogTracker>().open(TOOL_ASK_DIALOG.to_string());
        let close = async { handle.state::<SoftDialogTracker>().close(TOOL_ASK_DIALOG) };
        let params = json!({ "type": TOOL_ASK_DIALOG });
        let (reply, ()) = tokio::join!(execute_dialog_confirm(&handle, &params), close);
        reply
    };
    let (answer, reply) = tokio::join!(waiting, confirm);
    assert_eq!(answer, AskAnswer::Allowed);
    assert!(reply.is_ok(), "{reply:?}");
}

#[tokio::test]
async fn a_tool_ask_confirm_with_nothing_open_refuses() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let outcome = execute_dialog_confirm(&handle, &json!({ "type": TOOL_ASK_DIALOG })).await;
    assert!(outcome.is_err());
}

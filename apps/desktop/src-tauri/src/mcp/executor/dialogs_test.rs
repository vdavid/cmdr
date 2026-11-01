//! Tests for `dialogs.rs`: the confirm round-trip (foreground and background), and focus.

use super::*;
use crate::ui_state::dialog_state::KnownDialog;

fn known(ids: &[&str]) -> Vec<KnownDialog> {
    ids.iter()
        .map(|id| KnownDialog {
            id: (*id).to_string(),
            description: None,
            blocks_operations: true,
        })
        .collect()
}

#[test]
fn is_registered_soft_dialog_matches_only_known_ids() {
    let dialogs = known(&["whats-new", "search", "delete-confirmation"]);
    assert!(is_registered_soft_dialog(&dialogs, "whats-new"));
    assert!(is_registered_soft_dialog(&dialogs, "search"));
    // An id the FE never registered can't be closed generically.
    assert!(!is_registered_soft_dialog(&dialogs, "not-a-dialog"));
    // Empty registry (e.g. FE hasn't registered yet) rejects everything.
    assert!(!is_registered_soft_dialog(&[], "whats-new"));
}

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
        let outcome = execute_dialog_command(&handle, &json!({ "action": "confirm", "type": dialog_type })).await;
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

        let outcome = execute_dialog_command(
            &handle,
            &json!({ "action": "confirm", "type": dialog_type, "background": background }),
        )
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

    let outcome = execute_dialog_command(
        &handle,
        &json!({ "action": "confirm", "type": "transfer-confirmation", "background": true }),
    )
    .await;

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

        let outcome = execute_dialog_command(
            &handle,
            &json!({ "action": "confirm", "type": "delete-confirmation", "background": background }),
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

    let outcome = execute_dialog_command(
        &handle,
        &json!({ "action": "confirm", "type": "transfer-confirmation" }),
    )
    .await;

    let error = outcome.expect_err("the progress slot is taken");
    assert_eq!(error.data, Some(json!({ "blockingDialog": "transfer-progress" })));
}

#[tokio::test]
async fn a_confirm_with_no_such_dialog_open_is_refused_up_front() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let started = std::time::Instant::now();
    let outcome = execute_dialog_command(
        &handle,
        &json!({ "action": "confirm", "type": "transfer-confirmation" }),
    )
    .await;
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

        let outcome = execute_dialog_command(
            &handle,
            &json!({ "action": "confirm", "type": dialog_type, "background": true }),
        )
        .await;

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

    let outcome = execute_dialog_command(
        &handle,
        &json!({ "action": "confirm", "type": "delete-confirmation", "background": true }),
    )
    .await;

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

    let outcome = execute_dialog_command(
        &handle,
        &json!({ "action": "confirm", "type": "quit-confirmation", "background": true }),
    )
    .await;

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

#[tokio::test]
async fn focusing_an_open_settings_window_answers_ok() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    tauri::WebviewWindowBuilder::new(&handle, "settings", tauri::WebviewUrl::default())
        .build()
        .expect("a mock settings window");

    let outcome = execute_dialog_command(&handle, &json!({ "action": "focus", "type": "settings" })).await;
    assert!(outcome.is_ok(), "{:?}", outcome.err().map(|e| e.message));
}

#[tokio::test]
async fn focusing_settings_when_it_isnt_open_is_refused_up_front() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let started = std::time::Instant::now();
    let outcome = execute_dialog_command(&handle, &json!({ "action": "focus", "type": "settings" })).await;
    let error = outcome.expect_err("there's no settings window to focus");
    assert_eq!(error.code, ToolError::invalid_params("").code);
    assert!(
        started.elapsed() < DEFAULT_ACK_TIMEOUT,
        "it must not wait out the ack budget"
    );
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

    let outcome = execute_dialog_command(&handle, &json!({ "action": "confirm", "type": "delete-confirmation" }))
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
    let outcome = execute_dialog_command(&handle, &json!({ "action": "confirm", "type": "delete-confirmation" })).await;

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
    let outcome = execute_dialog_command(&handle, &json!({ "action": "confirm", "type": "delete-confirmation" })).await;

    outcome.expect_err("nothing ever started");
    let ceiling = crate::mcp::executor::MAX_ROUND_TRIP_HOLD + std::time::Duration::from_secs(11);
    assert!(started.elapsed() <= ceiling, "{:?}", started.elapsed());
}

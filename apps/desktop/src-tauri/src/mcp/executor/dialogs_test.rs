//! Tests for `dialogs.rs`: the confirm acks, the background confirm round-trip, and focus.

use super::*;
use crate::mcp::dialog_state::KnownDialog;

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
    app.manage(crate::mcp::pane_state::PaneStateStore::new());
    app.manage(SoftDialogTracker::new());
    app
}

/// Stands in for the frontend: a confirm takes the dialog down, and nothing
/// in either pane changes until the operation's first file lands.
fn close_dialog_on_confirm(app: &AppHandle<tauri::test::MockRuntime>, dialog_type: &'static str) {
    use tauri::Listener;
    let handle = app.clone();
    app.listen("mcp-confirm-dialog", move |_| {
        handle.state::<SoftDialogTracker>().close(dialog_type);
    });
}

#[tokio::test]
async fn a_confirm_acks_once_its_dialog_is_gone_with_no_pane_change() {
    for dialog_type in ["transfer-confirmation", "delete-confirmation"] {
        let app = app_with_stores();
        let handle = app.handle().clone();
        handle.state::<SoftDialogTracker>().open(dialog_type.to_string());
        close_dialog_on_confirm(&handle, dialog_type);

        // Pre-fix this waited for a pane-state push, which a compress or a copy
        // onto a slow volume doesn't make for seconds: the tool answered "not
        // acknowledged" about an operation that had started.
        let outcome = execute_dialog_command(&handle, &json!({ "action": "confirm", "type": dialog_type })).await;
        assert!(outcome.is_ok(), "{dialog_type}: {:?}", outcome.err().map(|e| e.message));
    }
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

/// Stands in for the frontend's answer to a background confirm: replies on
/// `mcp-response` with what the dialog's Background press did, and takes the
/// dialog down when it pressed. Returns the payloads it saw.
fn answer_background_confirm(
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
        if response["ok"] == json!(true) {
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
        let seen = answer_background_confirm(&handle, dialog_type, json!({ "ok": true }));

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
    answer_background_confirm(
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
fn a_background_confirm_reply_keeps_its_typed_refusal() {
    let parse = |payload: Value| parse_background_confirm_response(&payload.to_string(), "req-1");
    assert_eq!(
        parse(json!({ "requestId": "req-1", "ok": true })),
        Some(Ok(BackgroundConfirmAck::Pressed))
    );
    assert_eq!(
        parse(json!({ "requestId": "req-1", "ok": false, "refusal": "permanentDelete" })),
        Some(Ok(BackgroundConfirmAck::RefusedPermanentDelete))
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

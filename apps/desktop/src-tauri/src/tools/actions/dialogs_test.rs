//! Tests for `dialogs.rs`: the generic close, and focus.

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

#[tokio::test]
async fn the_dialog_tool_has_no_confirm() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    handle
        .state::<SoftDialogTracker>()
        .open("delete-confirmation".to_string());

    let outcome = execute_dialog_command(&handle, &json!({ "action": "confirm", "type": "delete-confirmation" })).await;

    outcome.expect_err("dialog_confirm is the one way past a person");
}

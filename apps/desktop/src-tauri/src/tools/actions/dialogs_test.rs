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

const OFF_DIRECT: Caller = Caller::External { direct: false };
const DIRECT: Caller = Caller::External { direct: true };
const AGENT: Caller = Caller::Agent {
    origin: crate::agent::types::TurnOrigin::Text,
};

fn ctx() -> ToolCtx {
    ToolCtx {
        caller: DIRECT,
        cancel: tokio_util::sync::CancellationToken::new(),
        conversation_id: None,
        asked: None,
    }
}

#[test]
fn onboarding_refuses_a_guarded_caller_and_opens_for_direct_mode() {
    for caller in [AGENT, OFF_DIRECT] {
        let refusal = refuse_onboarding_for_guarded_caller(caller).expect_err("it holds the AI setup");
        assert_eq!(refusal.data, Some(json!({ "refusal": "onboardingNeedsPerson" })));
    }
    refuse_onboarding_for_guarded_caller(DIRECT).expect("a person let this client past them");
}

#[tokio::test]
async fn a_guarded_dialog_open_onboarding_never_reaches_the_frontend() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let guarded = ToolCtx {
        caller: OFF_DIRECT,
        ..ctx()
    };
    let error = execute_dialog_command(&handle, &guarded, &json!({ "action": "open", "type": "onboarding" }))
        .await
        .expect_err("refused before any wait");
    assert_eq!(error.data, Some(json!({ "refusal": "onboardingNeedsPerson" })));
}

#[test]
fn a_guarded_search_opens_in_filename_mode_whatever_it_asked_for() {
    for asked in [None, Some("ai"), Some("regex"), Some("filename")] {
        for caller in [AGENT, OFF_DIRECT] {
            let mut payload = serde_json::Map::new();
            if let Some(mode) = asked {
                payload.insert("mode".to_string(), json!(mode));
            }
            assert!(pin_search_mode_for_guarded_caller(caller, &mut payload));
            assert_eq!(payload["mode"], "filename", "{caller:?} asked {asked:?}");
        }
    }
}

#[test]
fn direct_mode_keeps_the_search_mode_it_asked_for() {
    let mut payload = serde_json::Map::new();
    payload.insert("mode".to_string(), json!("ai"));
    assert!(!pin_search_mode_for_guarded_caller(DIRECT, &mut payload));
    assert_eq!(payload["mode"], "ai");

    let mut payload = serde_json::Map::new();
    assert!(!pin_search_mode_for_guarded_caller(DIRECT, &mut payload));
    assert!(!payload.contains_key("mode"), "the frontend picks its default");
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

    let outcome = execute_dialog_command(&handle, &ctx(), &json!({ "action": "focus", "type": "settings" })).await;
    assert!(outcome.is_ok(), "{:?}", outcome.err().map(|e| e.message));
}

#[tokio::test]
async fn focusing_settings_when_it_isnt_open_is_refused_up_front() {
    let app = app_with_stores();
    let handle = app.handle().clone();
    let started = std::time::Instant::now();
    let outcome = execute_dialog_command(&handle, &ctx(), &json!({ "action": "focus", "type": "settings" })).await;
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

    let outcome = execute_dialog_command(
        &handle,
        &ctx(),
        &json!({ "action": "confirm", "type": "delete-confirmation" }),
    )
    .await;

    outcome.expect_err("dialog_confirm is the one way past a person");
}

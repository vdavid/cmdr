//! Unit tests for the pure decision cores of the operation-log MCP handlers:
//! filter/param parsing and the typed refusal wire shape. The handlers themselves
//! (read connection, rollback dispatch) are exercised by the live MCP E2E.

use super::*;
use cmdr_operation_log::types::{
    ExecutionStatus, Initiator, NotRollbackableReason, OpKind, RollbackState, SearchCoverage,
};
use cmdr_operations::testing::RollbackSupport;
use serde_json::json;

fn agent_ctx() -> ToolCtx {
    ToolCtx {
        caller: crate::tools::Caller::Agent {
            origin: crate::agent::types::TurnOrigin::Text,
        },
        cancel: tokio_util::sync::CancellationToken::new(),
        conversation_id: None,
    }
}

#[test]
fn empty_params_parse_to_no_filters() {
    let (filters, limit, offset) = parse_list_filters(&json!({})).expect("parse");
    assert!(filters_are_empty(&filters), "no params ⇒ bare list (recent feed)");
    assert_eq!(limit, DEFAULT_LIST_LIMIT);
    assert_eq!(offset, 0);
}

#[test]
fn filters_parse_to_typed_enums() {
    let params = json!({
        "since": 100,
        "until": 200,
        "kind": "trash",
        "initiator": "aiClient",
        "executionStatus": "done",
        "rollbackState": "rollbackable",
        "limit": 10,
        "offset": 5,
    });
    let (filters, limit, offset) = parse_list_filters(&params).expect("parse");
    assert!(!filters_are_empty(&filters));
    assert_eq!(filters.since, Some(100));
    assert_eq!(filters.until, Some(200));
    assert_eq!(filters.kinds, vec![OpKind::Trash]);
    assert_eq!(filters.initiator, Some(Initiator::AiClient));
    assert_eq!(filters.execution_status, Some(ExecutionStatus::Done));
    assert_eq!(filters.rollback_state, Some(RollbackState::Rollbackable));
    assert_eq!(limit, 10);
    assert_eq!(offset, 5);
}

#[test]
fn camelcase_kind_tokens_match_the_result_serialization() {
    // The input token is the same camelCase form the results serialize, so an
    // agent round-trips a value it read back verbatim (createFolder, not create_folder).
    let (filters, _, _) = parse_list_filters(&json!({ "kind": "createFolder" })).expect("parse");
    assert_eq!(filters.kinds, vec![OpKind::CreateFolder]);
}

#[test]
fn unknown_enum_token_is_a_typed_invalid_params() {
    let err = parse_list_filters(&json!({ "kind": "bogus" })).expect_err("should reject");
    assert_eq!(err.code, crate::tools::error_codes::INVALID_PARAMS);
}

#[test]
fn name_filter_defaults_to_prefix() {
    let filter = parse_name_filter(&json!({ "name": "dog.jpg" }))
        .expect("parse")
        .expect("present");
    assert_eq!(filter.text, "dog.jpg");
    assert_eq!(filter.match_kind, NameMatch::Prefix);
}

#[test]
fn name_filter_honors_exact_match() {
    let filter = parse_name_filter(&json!({ "name": "dog.jpg", "nameMatch": "exact" }))
        .expect("parse")
        .expect("present");
    assert_eq!(filter.match_kind, NameMatch::Exact);
}

#[test]
fn empty_name_is_no_filter() {
    assert!(parse_name_filter(&json!({ "name": "" })).expect("parse").is_none());
    assert!(parse_name_filter(&json!({})).expect("parse").is_none());
}

#[test]
fn bad_name_match_is_rejected() {
    let err = parse_name_filter(&json!({ "name": "x", "nameMatch": "contains" })).expect_err("reject");
    assert_eq!(err.code, crate::tools::error_codes::INVALID_PARAMS);
}

#[test]
fn limit_defaults_and_clamps() {
    assert_eq!(
        parse_limit(&json!({}), "limit", DEFAULT_LIST_LIMIT).expect("default"),
        DEFAULT_LIST_LIMIT
    );
    assert_eq!(
        parse_limit(&json!({ "limit": 5 }), "limit", DEFAULT_LIST_LIMIT).expect("explicit"),
        5
    );
    assert_eq!(
        parse_limit(&json!({ "limit": 99999 }), "limit", DEFAULT_LIST_LIMIT).expect("clamped"),
        MAX_LIMIT
    );
    assert!(parse_limit(&json!({ "limit": "nope" }), "limit", DEFAULT_LIST_LIMIT).is_err());
}

#[test]
fn operation_id_is_required_and_non_empty() {
    assert!(required_operation_id(&json!({ "operationId": "op-1" })).is_ok());
    assert!(required_operation_id(&json!({})).is_err());
    assert!(required_operation_id(&json!({ "operationId": "" })).is_err());
}

#[test]
fn refusal_serializes_as_a_typed_tagged_shape() {
    // The rollback handler returns `{ status: "refused", refusal: <this> }`; the
    // refusal is a typed, tagged enum so an agent branches on `kind`/`detail`, not
    // a message substring.
    let refusal = RollbackRefusal::NotRollbackable(NotRollbackableReason::PermanentDelete);
    let value = serde_json::to_value(&refusal).expect("serialize");
    assert_eq!(value["kind"], "notRollbackable");
    assert_eq!(value["detail"], "permanentDelete");

    let vol = RollbackRefusal::VolumeUnavailable {
        volume_id: "backup".to_string(),
    };
    let value = serde_json::to_value(&vol).expect("serialize");
    assert_eq!(value["kind"], "volumeUnavailable");
    assert_eq!(value["detail"]["volumeId"], "backup");

    let already = RollbackRefusal::AlreadyRollingBack;
    assert_eq!(
        serde_json::to_value(&already).expect("serialize")["kind"],
        "alreadyRollingBack"
    );
}

// ── operations_rollback: what opens a dialog, and what refuses with none ─────────

struct NoVolumes;
impl VolumeLookup for NoVolumes {
    fn get(&self, _volume_id: &str) -> Option<std::sync::Arc<dyn cmdr_fs::volume::Volume>> {
        None
    }
}

fn logged_row(rollback_state: RollbackState) -> OperationRow {
    OperationRow {
        op_id: "op-1".to_string(),
        kind: OpKind::Move,
        archive_subkind: None,
        initiator: Initiator::User,
        execution_status: ExecutionStatus::Done,
        rollback_state,
        not_rollbackable_reason: None,
        rolls_back_op_id: None,
        inverse_op_id: None,
        source_volume_id: None,
        dest_volume_id: None,
        started_at: 0,
        ended_at: Some(1),
        item_count: 1,
        items_done: 1,
        bytes_total: 0,
        search_coverage: SearchCoverage::Full,
        search_coverage_reason: None,
        dev_summary: None,
    }
}

fn live_op(rollback_support: RollbackSupport) -> OperationSnapshot {
    OperationSnapshot {
        operation_id: "op-1".to_string(),
        operation_type: cmdr_operations::WriteOperationType::Copy,
        status: LifecycleStatus::Running,
        source: None,
        destination: None,
        rollback_support,
        reverses: None,
        error: None,
        can_retry: false,
        items: None,
        conflict_policy: None,
        started_at: None,
    }
}

fn refusal_of(result: Result<RollbackQuestion, ToolError>) -> Value {
    result.expect_err("should refuse").data.expect("a typed refusal")
}

#[test]
fn a_finished_operation_asks_in_its_own_terms() {
    assert_eq!(
        logged_question(Some(logged_row(RollbackState::Rollbackable)), &NoVolumes).unwrap(),
        RollbackQuestion::Logged {
            kind: OpKind::Move,
            finishing: false
        }
    );
    assert_eq!(
        logged_question(Some(logged_row(RollbackState::PartiallyRolledBack)), &NoVolumes).unwrap(),
        RollbackQuestion::Logged {
            kind: OpKind::Move,
            finishing: true
        }
    );
}

/// What the journal's gate refuses comes back typed, so nothing opens and nothing parses a sentence.
#[test]
fn what_the_journal_refuses_is_typed_with_no_dialog() {
    assert_eq!(
        refusal_of(logged_question(None, &NoVolumes)),
        json!({ "refusal": "unknownOperation" })
    );
    assert_eq!(
        refusal_of(logged_question(Some(logged_row(RollbackState::RolledBack)), &NoVolumes)),
        json!({ "refusal": "alreadyRolledBack" })
    );
    assert_eq!(
        refusal_of(logged_question(
            Some(logged_row(RollbackState::RollingBack)),
            &NoVolumes
        )),
        json!({ "refusal": "alreadyRollingBack" })
    );

    let mut permanent = logged_row(RollbackState::NotRollbackable);
    permanent.not_rollbackable_reason = Some(NotRollbackableReason::PermanentDelete);
    let data = refusal_of(logged_question(Some(permanent), &NoVolumes));
    assert_eq!(data["refusal"], json!("notRollbackable"));
    assert_eq!(data["detail"], json!("permanentDelete"));

    let mut gone = logged_row(RollbackState::Rollbackable);
    gone.dest_volume_id = Some("backup".to_string());
    let data = refusal_of(logged_question(Some(gone), &NoVolumes));
    assert_eq!(data["refusal"], json!("volumeUnavailable"));
}

#[test]
fn a_running_operation_asks_only_when_its_strategy_can_reverse() {
    assert_eq!(
        live_question(&live_op(RollbackSupport::Supported)).unwrap(),
        RollbackQuestion::Live
    );
    assert_eq!(
        refusal_of(live_question(&live_op(RollbackSupport::CrossVolumeMove))),
        json!({ "refusal": "cantReverse", "detail": "crossVolumeMove" })
    );
    // A reversal offers no rollback of itself: that would re-apply what a person chose to undo.
    assert_eq!(
        refusal_of(live_question(&live_op(RollbackSupport::IsReversal))),
        json!({ "refusal": "cantReverse", "detail": "isReversal" })
    );
}

#[test]
fn the_question_crosses_as_facts_the_frontend_words() {
    assert_eq!(
        serde_json::to_value(RollbackQuestion::Live).unwrap(),
        json!({ "target": "live" })
    );
    assert_eq!(
        serde_json::to_value(RollbackQuestion::Logged {
            kind: OpKind::CreateFolder,
            finishing: true
        })
        .unwrap(),
        json!({ "target": "logged", "kind": "createFolder", "finishing": true })
    );
}

/// The tool opens the dialog and says a person decides. The answer is ❌ never a dispatch.
#[tokio::test]
async fn opening_the_question_answers_that_a_person_decides() {
    use crate::ui_state::dialog_state::SoftDialogTracker;
    use tauri::{Emitter, Listener, Manager};

    let app = tauri::test::mock_app();
    app.manage(SoftDialogTracker::new());
    let handle = app.handle().clone();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let (responder, recorded) = (handle.clone(), seen.clone());
    handle.listen("mcp-rollback-dialog", move |event| {
        let payload: Value = serde_json::from_str(event.payload()).unwrap();
        *recorded.lock().unwrap() = Some(payload.clone());
        responder
            .state::<SoftDialogTracker>()
            .open(ROLLBACK_CONFIRMATION.to_string());
        responder
            .emit("mcp-response", json!({ "requestId": payload["requestId"], "ok": true }))
            .unwrap();
    });

    let answer = open_rollback_question(&handle, &agent_ctx(), "op-9", RollbackQuestion::Live)
        .await
        .unwrap();
    assert_eq!(answer["awaitingPerson"], json!(true));
    assert_eq!(answer["dialog"], json!(ROLLBACK_CONFIRMATION));
    assert!(answer.get("status").is_none(), "nothing was dispatched: {answer}");

    let payload = seen.lock().unwrap().take().expect("the event went out");
    assert_eq!(payload["operationId"], json!("op-9"));
    assert_eq!(payload["target"], json!("live"));
    assert_eq!(payload["initiator"], json!("agent"), "the log credits who asked");
    assert_eq!(payload["guard"], json!(true));
}

/// Another modal up is the frontend's refusal to give, typed, and the question never rises.
#[tokio::test]
async fn a_modal_in_the_way_refuses_with_its_id() {
    use crate::ui_state::dialog_state::SoftDialogTracker;
    use tauri::{Emitter, Listener, Manager};

    let app = tauri::test::mock_app();
    app.manage(SoftDialogTracker::new());
    let handle = app.handle().clone();
    let responder = handle.clone();
    handle.listen("mcp-rollback-dialog", move |event| {
        let payload: Value = serde_json::from_str(event.payload()).unwrap();
        responder
            .emit(
                "mcp-response",
                json!({ "requestId": payload["requestId"], "ok": false, "blockedBy": "operation-log", "error": "x" }),
            )
            .unwrap();
    });

    let error = open_rollback_question(&handle, &agent_ctx(), "op-9", RollbackQuestion::Live)
        .await
        .expect_err("a dialog is in the way");
    assert_eq!(error.data, Some(json!({ "blockingDialog": "operation-log" })));
}

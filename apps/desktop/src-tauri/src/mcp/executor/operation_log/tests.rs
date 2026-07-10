//! Unit tests for the pure decision cores of the operation-log MCP handlers:
//! filter/param parsing and the typed refusal wire shape. The handlers themselves
//! (read connection, rollback dispatch) are exercised by the live MCP E2E.

use super::*;
use crate::operation_log::rollback::RollbackRefusal;
use crate::operation_log::types::{ExecutionStatus, Initiator, NotRollbackableReason, OpKind, RollbackState};
use serde_json::json;

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
    assert_eq!(err.code, crate::mcp::protocol::INVALID_PARAMS);
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
    assert_eq!(err.code, crate::mcp::protocol::INVALID_PARAMS);
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
    // a message substring (`no-string-matching`).
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

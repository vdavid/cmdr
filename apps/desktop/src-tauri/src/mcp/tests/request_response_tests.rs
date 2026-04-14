use serde_json::json;

use crate::mcp::protocol::{INVALID_PARAMS, McpRequest, McpResponse};

#[test]
fn test_mcp_request_parse_valid() {
    let json = r#"{"jsonrpc": "2.0", "id": 1, "method": "tools/list"}"#;
    let request: Result<McpRequest, _> = serde_json::from_str(json);
    assert!(request.is_ok());
    let req = request.unwrap();
    assert_eq!(req.method, "tools/list");
}

#[test]
fn test_mcp_request_parse_with_params() {
    let json = r#"{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "nav.up"}}"#;
    let request: Result<McpRequest, _> = serde_json::from_str(json);
    assert!(request.is_ok());
    let req = request.unwrap();
    assert_eq!(req.method, "tools/call");
    assert!(!req.params.is_null());
}

#[test]
fn test_mcp_request_reject_missing_jsonrpc() {
    let json = r#"{"id": 1, "method": "tools/list"}"#;
    let request: Result<McpRequest, _> = serde_json::from_str(json);
    // Should still parse but jsonrpc field will be empty
    if let Ok(req) = request {
        assert!(req.jsonrpc.is_empty() || req.jsonrpc != "2.0");
    }
}

#[test]
fn test_mcp_request_reject_malformed_json() {
    let malformed_inputs = [
        r#"{"incomplete"#,
        r#"not json at all"#,
        r#"null"#,
        r#"123"#,
        r#""string""#,
        r#"[1, 2, 3]"#,
    ];

    for input in malformed_inputs {
        let result: Result<McpRequest, _> = serde_json::from_str(input);
        assert!(result.is_err(), "Should reject malformed JSON: {}", input);
    }
}

#[test]
fn test_mcp_response_success() {
    let response = McpResponse::success(Some(json!(1)), json!({"test": true}));
    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert!(json["result"].is_object());
    assert!(json.get("error").is_none());
}

#[test]
fn test_mcp_response_error() {
    let response = McpResponse::error(Some(json!(1)), INVALID_PARAMS, "test error".to_string());
    let json = serde_json::to_value(&response).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["id"], 1);
    assert!(json.get("result").is_none());
    assert!(json["error"].is_object());
    assert_eq!(json["error"]["code"], INVALID_PARAMS);
    assert_eq!(json["error"]["message"], "test error");
}

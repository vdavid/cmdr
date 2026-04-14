use serde_json::json;

use crate::mcp::protocol::{
    INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, McpRequest, McpResponse, PARSE_ERROR,
};
use crate::mcp::server::{DEFAULT_PROTOCOL_VERSION, PROTOCOL_VERSION, format_sse_event, prefers_sse};

// =============================================================================
// MCP Spec 2025-11-25 Compliance Tests
// =============================================================================

#[test]
fn test_protocol_version_is_2025_11_25() {
    assert_eq!(PROTOCOL_VERSION, "2025-11-25");
}

#[test]
fn test_default_protocol_version_is_2025_03_26() {
    // Per spec: if no MCP-Protocol-Version header, assume 2025-03-26
    assert_eq!(DEFAULT_PROTOCOL_VERSION, "2025-03-26");
}

#[test]
fn test_server_capabilities_contain_protocol_version() {
    use crate::mcp::protocol::ServerCapabilities;

    let caps = ServerCapabilities::default();
    // The protocol version should be included in capabilities
    assert!(!caps.protocol_version.is_empty());
}

#[test]
fn test_server_capabilities_tools_list_changed_false() {
    use crate::mcp::protocol::ServerCapabilities;

    let caps = ServerCapabilities::default();
    // We don't currently support dynamic tool list changes
    assert!(!caps.capabilities.tools.list_changed);
}

#[test]
fn test_server_info_name_is_cmdr() {
    use crate::mcp::protocol::ServerCapabilities;

    let caps = ServerCapabilities::default();
    assert_eq!(caps.server_info.name, "cmdr");
}

#[test]
fn test_server_info_has_version() {
    use crate::mcp::protocol::ServerCapabilities;

    let caps = ServerCapabilities::default();
    assert!(!caps.server_info.version.is_empty());
}

#[test]
fn test_mcp_response_success_format() {
    let response = McpResponse::success(Some(json!(1)), json!({"data": "test"}));
    let serialized = serde_json::to_value(&response).unwrap();

    // Must have jsonrpc: "2.0"
    assert_eq!(serialized["jsonrpc"], "2.0");
    // Must have id matching request
    assert_eq!(serialized["id"], 1);
    // Must have result
    assert!(serialized.get("result").is_some());
    // Must NOT have error
    assert!(serialized.get("error").is_none());
}

#[test]
fn test_mcp_response_error_format() {
    let response = McpResponse::error(Some(json!(1)), INVALID_REQUEST, "Test error");
    let serialized = serde_json::to_value(&response).unwrap();

    // Must have jsonrpc: "2.0"
    assert_eq!(serialized["jsonrpc"], "2.0");
    // Must have id matching request
    assert_eq!(serialized["id"], 1);
    // Must NOT have result
    assert!(serialized.get("result").is_none());
    // Must have error with code and message
    assert!(serialized.get("error").is_some());
    assert_eq!(serialized["error"]["code"], INVALID_REQUEST);
    assert_eq!(serialized["error"]["message"], "Test error");
}

#[test]
fn test_mcp_response_null_id_allowed() {
    // For notifications and some error responses, id can be null
    let response = McpResponse::error(None, INVALID_REQUEST, "Parse error");
    let serialized = serde_json::to_value(&response).unwrap();

    // id should be omitted (skip_serializing_if)
    assert!(serialized.get("id").is_none());
}

#[test]
fn test_origin_validation_localhost_variants() {
    use crate::mcp::server::validate_origin;
    use axum::http::{HeaderMap, HeaderValue, header};

    // All localhost variants should be allowed
    let localhost_origins = [
        "http://localhost",
        "http://localhost:3000",
        "http://localhost:9224",
        "https://localhost",
        "https://localhost:443",
        "http://127.0.0.1",
        "http://127.0.0.1:9224",
        "https://127.0.0.1",
        "http://[::1]",
        "https://[::1]:9224",
    ];

    for origin in localhost_origins {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        assert!(validate_origin(&headers).is_ok(), "Should allow origin: {}", origin);
    }
}

//noinspection HttpUrlsUsage
#[test]
fn test_origin_validation_rejects_external() {
    use crate::mcp::server::validate_origin;
    use axum::http::{HeaderMap, HeaderValue, header};

    let malicious_origins = [
        "https://evil.com",
        "http://attacker.com",
        "https://localhost.evil.com",
        "http://127.0.0.1.evil.com",
        "https://phishing-site.net",
    ];

    for origin in malicious_origins {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        assert!(validate_origin(&headers).is_err(), "Should reject origin: {}", origin);
    }
}

#[test]
fn test_origin_validation_allows_null() {
    use crate::mcp::server::validate_origin;
    use axum::http::{HeaderMap, HeaderValue, header};

    // null origin is sent by file:// and some non-browser contexts
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, HeaderValue::from_static("null"));
    assert!(validate_origin(&headers).is_ok());
}

#[test]
fn test_origin_validation_allows_tauri() {
    use crate::mcp::server::validate_origin;
    use axum::http::{HeaderMap, HeaderValue, header};

    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, HeaderValue::from_static("tauri://localhost"));
    assert!(validate_origin(&headers).is_ok());
}

#[test]
fn test_origin_validation_allows_no_header() {
    use crate::mcp::server::validate_origin;
    use axum::http::HeaderMap;

    // Non-browser clients typically don't send Origin
    let headers = HeaderMap::new();
    assert!(validate_origin(&headers).is_ok());
}

#[test]
fn test_protocol_version_extraction() {
    use crate::mcp::server::get_protocol_version;
    use axum::http::{HeaderMap, HeaderValue};

    let mut headers = HeaderMap::new();
    headers.insert("mcp-protocol-version", HeaderValue::from_static("2025-11-25"));
    assert_eq!(get_protocol_version(&headers), "2025-11-25");

    // Custom version
    let mut headers2 = HeaderMap::new();
    headers2.insert("mcp-protocol-version", HeaderValue::from_static("2024-11-05"));
    assert_eq!(get_protocol_version(&headers2), "2024-11-05");
}

#[test]
fn test_protocol_version_default_when_missing() {
    use crate::mcp::server::get_protocol_version;
    use axum::http::HeaderMap;

    let headers = HeaderMap::new();
    assert_eq!(get_protocol_version(&headers), DEFAULT_PROTOCOL_VERSION);
}

#[test]
fn test_accept_header_validation() {
    use crate::mcp::server::validate_accept_header;
    use axum::http::{HeaderMap, HeaderValue, header};

    // Proper MCP client Accept header - just validates it doesn't panic
    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    validate_accept_header(&headers);

    // With wildcard
    let mut headers2 = HeaderMap::new();
    headers2.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    validate_accept_header(&headers2);

    // No header (backwards compat)
    let headers3 = HeaderMap::new();
    validate_accept_header(&headers3);
}

#[test]
fn test_json_rpc_error_codes() {
    // JSON-RPC 2.0 standard error codes
    assert_eq!(PARSE_ERROR, -32700);
    assert_eq!(INVALID_REQUEST, -32600);
    assert_eq!(METHOD_NOT_FOUND, -32601);
    assert_eq!(INVALID_PARAMS, -32602);
    assert_eq!(INTERNAL_ERROR, -32603);
}

#[test]
fn test_mcp_request_parses_initialize() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }
    }"#;

    let request: McpRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.method, "initialize");
    assert_eq!(request.params["protocolVersion"], "2025-11-25");
}

#[test]
fn test_mcp_request_parses_tools_call() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 42,
        "method": "tools/call",
        "params": {
            "name": "nav_up",
            "arguments": {}
        }
    }"#;

    let request: McpRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.method, "tools/call");
    assert_eq!(request.params["name"], "nav_up");
}

#[test]
fn test_mcp_request_parses_ping() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 99,
        "method": "ping"
    }"#;

    let request: McpRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.method, "ping");
}

#[test]
fn test_session_id_format() {
    // Session IDs should be valid UUIDs
    let session_id = uuid::Uuid::new_v4().to_string();

    // Must only contain visible ASCII characters (0x21 to 0x7E per spec)
    for c in session_id.chars() {
        assert!(
            c == '-' || c.is_ascii_alphanumeric(),
            "Session ID contains invalid char: {}",
            c
        );
    }

    // UUID v4 format: 8-4-4-4-12
    let parts: Vec<&str> = session_id.split('-').collect();
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[0].len(), 8);
    assert_eq!(parts[1].len(), 4);
    assert_eq!(parts[2].len(), 4);
    assert_eq!(parts[3].len(), 4);
    assert_eq!(parts[4].len(), 12);
}

// =============================================================================
// SSE (Server-Sent Events) tests
// =============================================================================

#[test]
fn test_prefers_sse_with_event_stream() {
    use axum::http::{HeaderMap, HeaderValue, header};

    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("text/event-stream"));
    assert!(prefers_sse(&headers));
}

#[test]
fn test_prefers_sse_with_both_types() {
    use axum::http::{HeaderMap, HeaderValue, header};

    let mut headers = HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream"),
    );
    assert!(prefers_sse(&headers));
}

#[test]
fn test_prefers_sse_with_json_only() {
    use axum::http::{HeaderMap, HeaderValue, header};

    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    assert!(!prefers_sse(&headers));
}

#[test]
fn test_prefers_sse_no_header() {
    use axum::http::HeaderMap;

    let headers = HeaderMap::new();
    assert!(!prefers_sse(&headers));
}

#[test]
fn test_prefers_sse_with_wildcard() {
    use axum::http::{HeaderMap, HeaderValue, header};

    // Wildcard should NOT prefer SSE - we default to JSON for simplicity
    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    assert!(!prefers_sse(&headers));
}

#[test]
fn test_format_sse_event_success_response() {
    let response = McpResponse::success(Some(json!(1)), json!({"status": "ok"}));
    let event = format_sse_event(&response, Some("event-123")).unwrap();

    // Event should be created successfully - axum handles the actual SSE formatting
    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"), "Event should have 'message' event type");
}

#[test]
fn test_format_sse_event_error_response() {
    let response = McpResponse::error(Some(json!(1)), INVALID_REQUEST, "Test error");
    let event = format_sse_event(&response, Some("error-event")).unwrap();

    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"));
}

#[test]
fn test_format_sse_event_without_id() {
    let response = McpResponse::success(Some(json!(1)), json!({"data": "test"}));
    let event = format_sse_event(&response, None).unwrap();

    // Event should be created without an ID
    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"));
}

#[test]
fn test_format_sse_event_with_null_id() {
    // Response with null id (notification response)
    let response = McpResponse::success(None, json!({"acknowledged": true}));
    let event = format_sse_event(&response, Some("notify-event")).unwrap();

    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"));
}

#[test]
fn test_format_sse_event_complex_result() {
    // Test with a complex nested result (like tools/list response)
    let response = McpResponse::success(
        Some(json!(42)),
        json!({
            "tools": [
                {"name": "test_tool", "description": "A test tool"},
                {"name": "another_tool", "description": "Another tool"}
            ]
        }),
    );
    let event = format_sse_event(&response, Some("tools-list")).unwrap();

    let debug_str = format!("{:?}", event);
    assert!(debug_str.contains("message"));
}

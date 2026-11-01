//! The spec rules `rmcp` leaves to us: Origin validation and the JSON-RPC codes tools answer with.

use crate::mcp::auth::validate_origin;
use crate::tools::error_codes::{INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR};
use axum::http::{HeaderMap, HeaderValue, header};

fn headers_with_origin(origin: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::ORIGIN, HeaderValue::from_static(origin));
    headers
}

#[test]
fn test_origin_validation_localhost_variants() {
    for origin in [
        "http://localhost",
        "http://localhost:3000",
        "http://localhost:19224",
        "https://localhost",
        "https://localhost:443",
        "http://127.0.0.1",
        "http://127.0.0.1:19224",
        "https://127.0.0.1",
        "http://[::1]",
        "https://[::1]:19224",
    ] {
        assert!(
            validate_origin(&headers_with_origin(origin)).is_ok(),
            "Should allow origin: {origin}"
        );
    }
}

//noinspection HttpUrlsUsage
#[test]
fn test_origin_validation_rejects_external() {
    for origin in [
        "https://evil.com",
        "http://attacker.com",
        "https://localhost.evil.com",
        "http://127.0.0.1.evil.com",
        "https://phishing-site.net",
    ] {
        assert!(
            validate_origin(&headers_with_origin(origin)).is_err(),
            "Should reject origin: {origin}"
        );
    }
}

/// Opaque browser origins can't be tied to an intended Cmdr client.
#[test]
fn test_origin_validation_rejects_null_and_tauri() {
    assert!(validate_origin(&headers_with_origin("null")).is_err());
    assert!(validate_origin(&headers_with_origin("tauri://localhost")).is_err());
}

#[test]
fn test_origin_validation_allows_no_header() {
    // Non-browser clients typically don't send Origin.
    assert!(validate_origin(&HeaderMap::new()).is_ok());
}

#[test]
fn test_json_rpc_error_codes() {
    assert_eq!(PARSE_ERROR, -32700);
    assert_eq!(INVALID_REQUEST, -32600);
    assert_eq!(METHOD_NOT_FOUND, -32601);
    assert_eq!(INVALID_PARAMS, -32602);
    assert_eq!(INTERNAL_ERROR, -32603);
}

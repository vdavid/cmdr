//! MCP auth and request validation: the per-instance bearer token lifecycle plus the
//! header/origin/protocol validation the HTTP server runs on every request.
//!
//! One-directional dependency: the server (`server.rs`) uses this module; this module has
//! no server dependencies. See `mcp/DETAILS.md` § Authentication for the full model.

use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::Value;
use std::sync::{OnceLock, RwLock};
use tauri::{AppHandle, Runtime};
use uuid::Uuid;

use super::protocol::{INVALID_PARAMS, INVALID_REQUEST, McpResponse};
use super::server::{DEFAULT_PROTOCOL_VERSION, TOKEN_FILE_NAME};

/// The current MCP auth token (None when the server isn't running). Mirrors the
/// `MCP_ACTUAL_PORT` lifecycle: set at start, reset to None on stop/crash. Regenerated
/// fresh on every start so a leaked token from a prior run can't be replayed.
static MCP_TOKEN: OnceLock<RwLock<Option<String>>> = OnceLock::new();

fn mcp_token_slot() -> &'static RwLock<Option<String>> {
    MCP_TOKEN.get_or_init(|| RwLock::new(None))
}

/// Set (or clear) the process-wide MCP token. Used only by the server lifecycle.
pub(super) fn set_mcp_token(token: Option<String>) {
    if let Ok(mut slot) = mcp_token_slot().write() {
        *slot = token;
    }
}

/// Read the current MCP token, or `None` when the server isn't running. Re-exported from
/// `mcp/mod.rs` so `commands::mcp` can serve it over the `get_mcp_token` IPC.
pub fn current_mcp_token() -> Option<String> {
    mcp_token_slot().read().ok().and_then(|slot| slot.clone())
}

/// Generate a fresh CSPRNG token. `Uuid::new_v4` is getrandom-backed (122 random bits);
/// we strip the dashes so the on-the-wire `Authorization: Bearer <token>` is a plain hex
/// string with no special chars to escape in shell/JSON clients.
pub(super) fn generate_token() -> String {
    Uuid::new_v4().simple().to_string()
}

/// Constant-time byte comparison. Returns true iff `a` and `b` are equal. Avoids the early
/// return of `==` so an attacker can't learn the token prefix-by-prefix from response
/// timing. We don't pull in `subtle` for one comparison; this length-checked XOR-accumulate
/// loop is the standard constant-time-compare idiom and the compiler can't short-circuit it
/// because the accumulator is read after the full loop.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Validate Origin header for security (DNS rebinding protection).
/// Per spec: Servers MUST validate the Origin header on all incoming connections.
pub(crate) fn validate_origin(headers: &HeaderMap) -> Result<(), Box<Response>> {
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin_str = origin.to_str().unwrap_or("");

        // Allow null origin (common for file:// or non-browser contexts)
        if origin_str == "null" {
            return Ok(());
        }

        // Allow Tauri origins (tauri://localhost)
        if origin_str.starts_with("tauri://") {
            return Ok(());
        }

        // Parse origin to validate it's actually localhost/127.0.0.1/[::1]
        // Format: scheme://host[:port]
        let is_localhost = is_localhost_origin(origin_str);

        if !is_localhost {
            log::warn!("MCP: Rejected request with invalid Origin: {}", origin_str);
            let error_response = McpResponse::error(None, INVALID_REQUEST, "Invalid Origin header");
            return Err(Box::new((StatusCode::FORBIDDEN, Json(error_response)).into_response()));
        }
    }
    // If no Origin header, allow (non-browser clients typically don't send it)
    Ok(())
}

/// Pure predicate: does this JSON-RPC call bypass the user's in-app confirmation dialog,
/// and therefore require the bearer token? True iff `method == "tools/call"` AND either:
///   - the tool is `delete`/`move`/`copy` with `arguments.autoConfirm == true`, OR
///   - the tool is `dialog` with `arguments.action == "confirm"`, OR
///   - the tool is `set_setting` (config mutation that applies with no user confirmation).
///
/// Everything else (resource reads, nav, search, and destructive ops that STILL pop the
/// dialog) returns false and needs no token. `autoConfirm` and `action` live under
/// `params.arguments`. No I/O, so it's directly unit-testable.
///
/// `set_setting` is gated as a whole tool: it applies any registry setting with no
/// confirmation, so an unauthenticated local process could otherwise silently flip
/// `updates.errorReports`, `network.*`, `developer.mcp*`, etc. That's the same
/// confirmation-bypass class as the auto-confirm file ops, so it belongs in the gated set.
pub(super) fn tool_call_requires_token(method: &str, params: &Value) -> bool {
    if method != "tools/call" {
        return false;
    }
    let Some(name) = params.get("name").and_then(|v| v.as_str()) else {
        return false;
    };
    let args = params.get("arguments");
    match name {
        "delete" | "move" | "copy" => args
            .and_then(|a| a.get("autoConfirm"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        "dialog" => args.and_then(|a| a.get("action")).and_then(|v| v.as_str()) == Some("confirm"),
        "set_setting" => true,
        _ => false,
    }
}

/// Validate the per-instance bearer token. This gates only the calls that bypass the
/// user's in-app confirmation dialog (see `tool_call_requires_token`): the threat is a
/// local non-Cmdr process silently auto-confirming a destructive op. macOS doesn't isolate
/// loopback between processes, so `validate_origin` (browser-CSRF defense) is no barrier to
/// a process that can set any header. The token lives in `<data_dir>/mcp.token` at 0o600,
/// so reading it requires the user's filesystem access.
///
/// Reads `Authorization: Bearer <token>`, compares against the stored token in constant
/// time, and rejects on any miss. Fails closed if the server somehow has no token set
/// (shouldn't happen while serving). The caller wraps the `Err` into the friendly 401.
pub(super) fn validate_token(headers: &HeaderMap) -> Result<(), ()> {
    let Some(expected) = current_mcp_token() else {
        // Fail closed: no token set means we can't authenticate anyone.
        log::warn!(target: "mcp::server", "MCP: rejecting request, no auth token configured");
        return Err(());
    };

    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if presented.is_empty() || !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
        // INFO, not WARN: a token-gated call arriving without the token is expected protocol
        // flow, not an anomaly. Agents reach a gated tool (`set_setting`, auto-confirm
        // delete/move/copy, `dialog confirm`) before reading `<data_dir>/mcp.token`, get the
        // friendly `auto_confirm_token_required_response` telling them where the token lives,
        // and retry. We still log it (not `debug`) so the security-relevant case — a local
        // non-Cmdr process probing the gate — stays visible in terminal and error bundles.
        log::info!(target: "mcp::server", "MCP: rejected request with missing/invalid bearer token");
        return Err(());
    }

    Ok(())
}

/// Build the response returned when a token-gated call (destructive auto-confirm or a
/// programmatic `dialog` confirm) arrives without a valid bearer token. The message tells
/// the caller exactly where to get the token — both the `CMDR_MCP_TOKEN` env var and the
/// resolved `<data_dir>/mcp.token` path. That's safe: the secret is the file's 0o600
/// contents (and the env value), not the path, which is already discoverable. We never
/// echo the token itself. One uniform message for missing-vs-wrong token (no oracle).
///
/// Returns the rejection in the EXACT shape of a normal tool error (the path the
/// `nav_to_path` "path does not exist" error takes), so clients render it as
/// `MCP error -32602: <message>` and resolve their pending call. Two things matter:
///
/// 1. **Echo the request's `id`.** A JSON-RPC response with `id: null` can't be correlated
///    to a pending request, so the client never resolves it and the tool call HANGS. The
///    `id` must be the caller's request id.
/// 2. **HTTP 200 + code `INVALID_PARAMS`, not 401 / `INVALID_REQUEST`.** The MCP
///    Streamable-HTTP spec reserves HTTP 401 for an OAuth challenge (a 401 makes clients
///    launch OAuth discovery and surface "Invalid OAuth error response"), and `-32600`
///    (Invalid Request) signals a malformed request envelope, which clients may treat as a
///    protocol desync. `-32602` at HTTP 200 is the same per-call error shape the executor's
///    `ToolError` uses, which clients handle cleanly. Our bearer gate is not OAuth and the
///    request envelope is valid, so it must look like an ordinary tool error.
pub(super) fn auto_confirm_token_required_response<R: Runtime>(app: &AppHandle<R>, id: Option<Value>) -> Response {
    let token_path = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir.join(TOKEN_FILE_NAME).display().to_string(),
        Err(_) => "<data_dir>/mcp.token".to_string(),
    };
    let message = format!(
        "This tool auto-confirms a destructive file operation, which requires the Cmdr MCP auth token. Send it as an `Authorization: Bearer <token>` header. Get the token from the `CMDR_MCP_TOKEN` environment variable of the running Cmdr, or read `{token_path}` (owner-only). Reads, navigation, search, and destructive ops that prompt you in the app all work without it."
    );
    let error_response = McpResponse::error(id, INVALID_PARAMS, message);
    (StatusCode::OK, Json(error_response)).into_response()
}

/// Check if an origin is a localhost origin (prevents DNS rebinding attacks).
fn is_localhost_origin(origin: &str) -> bool {
    // Extract host from origin (scheme://host[:port])
    let without_scheme = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .unwrap_or("");

    if without_scheme.is_empty() {
        return false;
    }

    // Extract host (remove port if present)
    let host = if without_scheme.starts_with('[') {
        // IPv6: [::1]:port or [::1]
        without_scheme.split(']').next().unwrap_or("").trim_start_matches('[')
    } else {
        // IPv4 or hostname: host:port or host
        without_scheme.split(':').next().unwrap_or("")
    };

    // Only allow exact localhost hosts
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Validate Accept header for MCP compliance.
/// Per spec: The client MUST include an Accept header listing both application/json and
/// text/event-stream. We log a warning but allow requests for backwards compatibility.
pub(crate) fn validate_accept_header(headers: &HeaderMap) {
    if let Some(accept) = headers.get(header::ACCEPT) {
        let accept_str = accept.to_str().unwrap_or("");
        let has_json = accept_str.contains("application/json") || accept_str.contains("*/*");
        let has_sse = accept_str.contains("text/event-stream") || accept_str.contains("*/*");

        if !has_json || !has_sse {
            log::debug!(
                "MCP: Accept header missing required types (got: {}), but allowing for compatibility",
                accept_str
            );
        }
    }
}

/// Check if client prefers SSE over JSON based on Accept header.
pub(crate) fn prefers_sse(headers: &HeaderMap) -> bool {
    if let Some(accept) = headers.get(header::ACCEPT) {
        let accept_str = accept.to_str().unwrap_or("");
        // If explicitly requesting SSE or using wildcard, prefer SSE
        accept_str.contains("text/event-stream")
    } else {
        false
    }
}

/// Validate and extract MCP-Protocol-Version header.
/// Per spec: Client MUST include MCP-Protocol-Version on all subsequent requests.
pub(crate) fn get_protocol_version(headers: &HeaderMap) -> String {
    headers
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_PROTOCOL_VERSION.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use serde_json::json;

    #[test]
    fn test_validate_origin_localhost() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("http://localhost:3000"));
        assert!(validate_origin(&headers).is_ok());

        headers.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1:19224"));
        assert!(validate_origin(&headers).is_ok());
    }

    #[test]
    fn test_validate_origin_null() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("null"));
        assert!(validate_origin(&headers).is_ok());
    }

    #[test]
    fn test_validate_origin_tauri() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("tauri://localhost"));
        assert!(validate_origin(&headers).is_ok());
    }

    #[test]
    fn test_validate_origin_rejects_external() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("https://evil.com"));
        assert!(validate_origin(&headers).is_err());
    }

    #[test]
    fn test_validate_origin_no_header() {
        let headers = HeaderMap::new();
        assert!(validate_origin(&headers).is_ok());
    }

    // ── Token auth ───────────────────────────────────────────────────────────
    // These tests set the process-wide token static, so they can't run truly in
    // parallel against each other. nextest runs each test in its own process, so
    // there's no cross-test contention in CI; under `cargo test` (shared process)
    // they serialize via a mutex to avoid flakiness.

    use std::sync::Mutex as StdMutex;
    static TOKEN_TEST_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn validate_token_rejects_missing_header() {
        let _guard = TOKEN_TEST_LOCK.lock().unwrap();
        set_mcp_token(Some("secret-token-abc".to_string()));
        let headers = HeaderMap::new();
        assert!(validate_token(&headers).is_err());
    }

    #[test]
    fn validate_token_rejects_wrong_token() {
        let _guard = TOKEN_TEST_LOCK.lock().unwrap();
        set_mcp_token(Some("secret-token-abc".to_string()));
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer wrong-token"));
        assert!(validate_token(&headers).is_err());
    }

    #[test]
    fn validate_token_rejects_empty_bearer() {
        let _guard = TOKEN_TEST_LOCK.lock().unwrap();
        set_mcp_token(Some("secret-token-abc".to_string()));
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer "));
        assert!(validate_token(&headers).is_err());
    }

    #[test]
    fn validate_token_accepts_exact_token() {
        let _guard = TOKEN_TEST_LOCK.lock().unwrap();
        set_mcp_token(Some("secret-token-abc".to_string()));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-token-abc"),
        );
        assert!(validate_token(&headers).is_ok());
    }

    #[test]
    fn validate_token_fails_closed_when_no_token_set() {
        let _guard = TOKEN_TEST_LOCK.lock().unwrap();
        set_mcp_token(None);
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer anything"));
        assert!(validate_token(&headers).is_err());
    }

    // ── tool_call_requires_token predicate ───────────────────────────────────
    // The token is required only for calls that bypass the user's in-app confirmation
    // dialog: destructive ops with autoConfirm, and a programmatic dialog confirm.

    fn params_with(name: &str, arguments: Value) -> Value {
        json!({"name": name, "arguments": arguments})
    }

    #[test]
    fn requires_token_delete_autoconfirm() {
        let p = params_with("delete", json!({"autoConfirm": true}));
        assert!(tool_call_requires_token("tools/call", &p));
    }

    #[test]
    fn requires_token_move_autoconfirm() {
        let p = params_with("move", json!({"autoConfirm": true}));
        assert!(tool_call_requires_token("tools/call", &p));
    }

    #[test]
    fn requires_token_copy_autoconfirm() {
        let p = params_with("copy", json!({"autoConfirm": true}));
        assert!(tool_call_requires_token("tools/call", &p));
    }

    #[test]
    fn no_token_delete_without_autoconfirm() {
        let p = params_with("delete", json!({}));
        assert!(!tool_call_requires_token("tools/call", &p));
        let p2 = params_with("delete", json!({"autoConfirm": false}));
        assert!(!tool_call_requires_token("tools/call", &p2));
    }

    #[test]
    fn requires_token_dialog_confirm() {
        let p = params_with("dialog", json!({"action": "confirm"}));
        assert!(tool_call_requires_token("tools/call", &p));
    }

    #[test]
    fn no_token_dialog_open() {
        let p = params_with("dialog", json!({"action": "open"}));
        assert!(!tool_call_requires_token("tools/call", &p));
    }

    #[test]
    fn no_token_read_nav_tools() {
        let nav = params_with("nav_to_path", json!({"pane": "left", "path": "/Users"}));
        assert!(!tool_call_requires_token("tools/call", &nav));
        let search = params_with("search", json!({"pattern": "*.pdf"}));
        assert!(!tool_call_requires_token("tools/call", &search));
    }

    #[test]
    fn no_token_resources_read_method() {
        let p = json!({"uri": "cmdr://state"});
        assert!(!tool_call_requires_token("resources/read", &p));
    }

    #[test]
    fn requires_token_set_setting() {
        // `set_setting` applies any registry setting with no user confirmation
        // (updates.errorReports, network.*, developer.mcp*, …), so it's gated
        // as a whole tool regardless of which setting it targets.
        let p = params_with("set_setting", json!({"id": "updates.errorReports", "value": true}));
        assert!(tool_call_requires_token("tools/call", &p));
        // Even with no arguments at all, the tool itself is gated.
        let bare = json!({"name": "set_setting"});
        assert!(tool_call_requires_token("tools/call", &bare));
    }

    #[test]
    fn no_token_unrelated_tool() {
        // A non-mutating tool stays open even with a set_setting-shaped arg blob.
        let p = params_with("nav_to_path", json!({"id": "updates.errorReports", "value": true}));
        assert!(!tool_call_requires_token("tools/call", &p));
    }

    #[test]
    fn test_get_protocol_version_with_header() {
        let mut headers = HeaderMap::new();
        headers.insert("mcp-protocol-version", HeaderValue::from_static("2025-11-25"));
        assert_eq!(get_protocol_version(&headers), "2025-11-25");
    }

    #[test]
    fn test_get_protocol_version_without_header() {
        let headers = HeaderMap::new();
        assert_eq!(get_protocol_version(&headers), DEFAULT_PROTOCOL_VERSION);
    }

    #[test]
    fn test_prefers_sse_with_sse_header() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static("text/event-stream"));
        assert!(prefers_sse(&headers));
    }

    #[test]
    fn test_prefers_sse_with_both_types() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        assert!(prefers_sse(&headers));
    }

    #[test]
    fn test_prefers_sse_with_json_only() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        assert!(!prefers_sse(&headers));
    }

    #[test]
    fn test_prefers_sse_no_header() {
        let headers = HeaderMap::new();
        assert!(!prefers_sse(&headers));
    }
}

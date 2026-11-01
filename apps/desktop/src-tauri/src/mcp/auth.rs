//! MCP auth and request validation: the per-instance bearer token lifecycle plus [`guard`], the
//! middleware every `/mcp` request passes before the SDK sees it.
//!
//! One-directional dependency: the server (`server.rs`) uses this module; this module has
//! no server dependencies. See `mcp/DETAILS.md` § Authentication for the full model.

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{OnceLock, RwLock};
use tauri::{AppHandle, Runtime};
use uuid::Uuid;

use super::safe_headers::SafeHeaders;
use super::server::TOKEN_FILE_NAME;
use crate::tools::error_codes::{INVALID_PARAMS, INVALID_REQUEST};

/// How much of an unauthenticated POST [`guard`] reads to find its `id`. Past this it can't tell, so
/// it answers as for any other body with no usable id.
const REJECTED_BODY_LIMIT_BYTES: usize = 1024 * 1024;

/// What `rmcp` insists a POST accepts. Its replies here are plain JSON all the same
/// (`with_json_response`).
const ACCEPTS_BOTH_REPLY_TYPES: &str = "application/json, text/event-stream";

/// The current MCP auth token (None when the server isn't running). Mirrors the
/// `MCP_ACTUAL_PORT` lifecycle: set at start, reset to None on stop/crash. Regenerated
/// fresh on every start so a leaked token from a prior run can't be replayed.
static MCP_TOKEN: OnceLock<RwLock<Option<String>>> = OnceLock::new();

#[cfg(test)]
pub(super) static TOKEN_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

/// In front of every `/mcp` request: Origin first, then the token, then what `rmcp` needs from a client
/// that doesn't send it. A request that passes keeps its body unread.
pub(super) async fn guard<R: Runtime>(
    State(app): State<AppHandle<R>>,
    headers: SafeHeaders,
    mut request: Request,
    next: Next,
) -> Response {
    log::debug!("MCP: {} /mcp, headers: {headers:?}", request.method());

    if let Err(response) = validate_origin(&headers) {
        log::warn!("MCP: {} rejected due to Origin validation failure", request.method());
        return *response;
    }
    if validate_token(&headers).is_err() {
        return token_rejection(&app, request).await;
    }
    if request.method() == Method::POST {
        accept_both_reply_types(request.headers_mut());
    }
    next.run(request).await
}

/// `rmcp` answers 406 unless a POST accepts both reply types, and `apps/desktop/test/e2e-shared/mcp-client.ts`
/// sends no `Accept` at all (`fetch` then adds `*/*`, which `rmcp` doesn't read as either).
fn accept_both_reply_types(headers: &mut HeaderMap) {
    let accepts_both = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("application/json") && accept.contains("text/event-stream"));
    if !accepts_both {
        headers.insert(header::ACCEPT, HeaderValue::from_static(ACCEPTS_BOTH_REPLY_TYPES));
    }
}

/// The parts of a refused POST its rejection depends on. Anything else about the envelope is the
/// SDK's to judge, and it never gets to: a body that fails even this much has no id to echo.
#[derive(Deserialize)]
struct RejectedEnvelope {
    id: Option<Value>,
    method: Option<String>,
}

impl RejectedEnvelope {
    /// No response id to correlate, so an in-band error would have nothing to resolve.
    fn is_notification(&self) -> bool {
        self.id.is_none()
            || self
                .method
                .as_deref()
                .is_some_and(|method| method.starts_with("notifications/"))
    }
}

/// The answer to a bad token. An ordinary request gets the in-band error that resolves its pending
/// call. Everything with no id to echo gets an empty 403: a notification, an unreadable body, and
/// any other HTTP method, GET's SSE stream included.
async fn token_rejection<R: Runtime>(app: &AppHandle<R>, request: Request) -> Response {
    if request.method() != Method::POST {
        return StatusCode::FORBIDDEN.into_response();
    }
    let envelope = axum::body::to_bytes(request.into_body(), REJECTED_BODY_LIMIT_BYTES)
        .await
        .ok()
        .and_then(|body| serde_json::from_slice::<RejectedEnvelope>(&body).ok());
    match envelope {
        Some(envelope) if !envelope.is_notification() => authentication_required_response(app, envelope.id),
        _ => StatusCode::FORBIDDEN.into_response(),
    }
}

/// A JSON-RPC error with no `id` key when there's none to echo.
fn json_rpc_error(id: Option<Value>, code: i32, message: &str) -> Value {
    let error = json!({"code": code, "message": message});
    match id {
        Some(id) => json!({"jsonrpc": "2.0", "id": id, "error": error}),
        None => json!({"jsonrpc": "2.0", "error": error}),
    }
}

/// Validate Origin header for security (DNS rebinding protection).
/// Per spec: Servers MUST validate the Origin header on all incoming connections.
pub(crate) fn validate_origin(headers: &HeaderMap) -> Result<(), Box<Response>> {
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin_str = origin.to_str().unwrap_or("");
        if !is_localhost_origin(origin_str) {
            log::warn!("MCP: Rejected request with invalid Origin: {}", origin_str);
            crate::security_log::note_mcp_rejection(crate::security_log::McpRejection::ForeignOrigin);
            let error_response = json_rpc_error(None, INVALID_REQUEST, "Invalid Origin header");
            return Err(Box::new((StatusCode::FORBIDDEN, Json(error_response)).into_response()));
        }
    }
    // If no Origin header, allow (non-browser clients typically don't send it)
    Ok(())
}

/// Why [`validate_token`] turned a request away. The client gets the same response for every
/// one (so the answer isn't an oracle); the reason is for the security log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenRejection {
    /// No `Authorization: Bearer` header, or an empty one: usually a client still being set up.
    Missing,
    /// A token that isn't this run's: usually a client still holding the previous run's.
    Wrong,
    /// The server has no token to compare against (it shouldn't happen while serving).
    NotConfigured,
}

impl From<TokenRejection> for crate::security_log::McpRejection {
    fn from(rejection: TokenRejection) -> Self {
        match rejection {
            TokenRejection::Missing => Self::MissingToken,
            TokenRejection::Wrong => Self::WrongToken,
            TokenRejection::NotConfigured => Self::NoTokenConfigured,
        }
    }
}

/// Validate the per-instance bearer token for the entire MCP HTTP surface. macOS doesn't
/// isolate loopback between processes, so `validate_origin` (browser-CSRF defense) is no
/// barrier to a process that can set any header. The token lives in
/// `<data_dir>/mcp.token` at 0o600, so reading it requires the user's filesystem access.
///
/// Reads `Authorization: Bearer <token>`, compares against the stored token in constant
/// time, and rejects on any miss. Fails closed if the server somehow has no token set
/// (shouldn't happen while serving). The caller chooses the protocol-compatible rejection
/// shape for requests, notifications, and SSE connections. Every rejection is noted in the
/// security log, coalesced per episode.
pub(super) fn validate_token(headers: &HeaderMap) -> Result<(), TokenRejection> {
    let checked = check_token(headers);
    if let Err(rejection) = checked {
        crate::security_log::note_mcp_rejection(rejection.into());
    }
    checked
}

fn check_token(headers: &HeaderMap) -> Result<(), TokenRejection> {
    let Some(expected) = current_mcp_token() else {
        // Fail closed: no token set means we can't authenticate anyone.
        log::warn!(target: "mcp::server", "MCP: rejecting request, no auth token configured");
        return Err(TokenRejection::NotConfigured);
    };

    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if presented.is_empty() || !constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
        // INFO, not WARN: a request arriving without the token is expected setup flow, not an
        // anomaly. Clients may try to initialize before reading `<data_dir>/mcp.token`, receive
        // the actionable rejection, configure the header, and retry. Keep it above `debug` so a
        // local non-Cmdr process probing the boundary stays visible in terminal and bundles.
        log::info!(target: "mcp::server", "MCP: rejected request with missing/invalid bearer token");
        return Err(if presented.is_empty() {
            TokenRejection::Missing
        } else {
            TokenRejection::Wrong
        });
    }

    Ok(())
}

/// Build the response returned when an ordinary JSON-RPC request arrives without a valid
/// bearer token. The message tells the caller exactly where to get the token — both the
/// `CMDR_MCP_TOKEN` env var and the resolved `<data_dir>/mcp.token` path. That's safe: the
/// secret is the file's 0o600 contents (and the env value), not the path, which is already
/// discoverable. We never echo the token itself. One uniform message for missing-vs-wrong
/// token avoids making the response an oracle.
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
pub(super) fn authentication_required_response<R: Runtime>(app: &AppHandle<R>, id: Option<Value>) -> Response {
    let token_path = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => dir.join(TOKEN_FILE_NAME).display().to_string(),
        Err(_) => "<data_dir>/mcp.token".to_string(),
    };
    let message = format!(
        "Cmdr requires its MCP auth token on every request. Send it as an `Authorization: Bearer <token>` header. Get the token from the `CMDR_MCP_TOKEN` environment variable of the running Cmdr, or read `{token_path}` (owner-only)."
    );
    (StatusCode::OK, Json(json_rpc_error(id, INVALID_PARAMS, &message))).into_response()
}

/// Check if an origin is a localhost origin (prevents DNS rebinding attacks).
pub(super) fn is_localhost_origin(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<axum::http::Uri>() else {
        return false;
    };
    let scheme_allowed = matches!(uri.scheme_str(), Some("http" | "https"));
    // `http::Uri` normalizes an absent path to `/`, so accept that canonical root while
    // continuing to reject origins carrying an actual path or query.
    let origin_has_no_path_or_query = uri.path() == "/" && uri.query().is_none();
    scheme_allowed
        && origin_has_no_path_or_query
        && uri
            .authority()
            .is_some_and(|authority| matches!(authority.host(), "localhost" | "127.0.0.1" | "[::1]" | "::1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Never called: it exists so the guard's header parameter has to be `SafeHeaders`. Going back to
    /// a bare `HeaderMap` stops compiling right here. The redaction itself is `SafeHeaders`'s `Debug`
    /// (tested in `safe_headers.rs`), so it covers every log site the guard will have.
    #[expect(dead_code, reason = "a compile-time pin on the guard's signature, never run")]
    async fn the_guard_takes_redacting_headers(
        state: State<AppHandle<tauri::Wry>>,
        headers: SafeHeaders,
        request: Request,
        next: Next,
    ) {
        guard(state, headers, request, next).await;
    }

    #[test]
    fn a_refused_envelope_with_no_id_or_a_notification_method_is_a_notification() {
        let envelope = |json: Value| serde_json::from_value::<RejectedEnvelope>(json).expect("an envelope");

        assert!(envelope(json!({"method": "tools/list"})).is_notification());
        assert!(envelope(json!({"id": null, "method": "ping"})).is_notification());
        assert!(envelope(json!({"id": 1, "method": "notifications/initialized"})).is_notification());
        assert!(!envelope(json!({"id": 1, "method": "ping"})).is_notification());
        assert!(!envelope(json!({"id": 1})).is_notification());
    }

    #[test]
    fn a_client_accepting_less_than_both_reply_types_gets_both_filled_in() {
        for sent in [None, Some("*/*"), Some("application/json"), Some("text/event-stream")] {
            let mut headers = HeaderMap::new();
            if let Some(accept) = sent {
                headers.insert(header::ACCEPT, HeaderValue::from_static(accept));
            }
            accept_both_reply_types(&mut headers);
            assert_eq!(
                headers.get(header::ACCEPT).map(|v| v.as_bytes()),
                Some(ACCEPTS_BOTH_REPLY_TYPES.as_bytes()),
                "{sent:?}"
            );
        }

        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("text/event-stream, application/json"),
        );
        accept_both_reply_types(&mut headers);
        assert_eq!(
            headers.get(header::ACCEPT).map(|v| v.as_bytes()),
            Some("text/event-stream, application/json".as_bytes())
        );
    }

    #[test]
    fn test_validate_origin_localhost() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("http://localhost:3000"));
        assert!(validate_origin(&headers).is_ok());

        headers.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1:19224"));
        assert!(validate_origin(&headers).is_ok());

        headers.insert(header::ORIGIN, HeaderValue::from_static("https://[::1]:8443"));
        assert!(validate_origin(&headers).is_ok());
    }

    #[test]
    fn test_validate_origin_rejects_null() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("null"));
        assert!(validate_origin(&headers).is_err());
    }

    #[test]
    fn test_validate_origin_rejects_tauri() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("tauri://localhost"));
        assert!(validate_origin(&headers).is_err());
    }

    #[test]
    fn test_validate_origin_rejects_external() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("https://evil.com"));
        assert!(validate_origin(&headers).is_err());

        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://localhost.evil.example"),
        );
        assert!(validate_origin(&headers).is_err());

        headers.insert(header::ORIGIN, HeaderValue::from_static("https://127.0.0.1.example"));
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

    #[test]
    fn validate_token_rejects_missing_header() {
        let _guard = TOKEN_TEST_LOCK.blocking_lock();
        set_mcp_token(Some("secret-token-abc".to_string()));
        let headers = HeaderMap::new();
        assert_eq!(validate_token(&headers), Err(TokenRejection::Missing));
    }

    #[test]
    fn validate_token_rejects_wrong_token() {
        let _guard = TOKEN_TEST_LOCK.blocking_lock();
        set_mcp_token(Some("secret-token-abc".to_string()));
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer wrong-token"));
        assert_eq!(validate_token(&headers), Err(TokenRejection::Wrong));
    }

    #[test]
    fn validate_token_rejects_empty_bearer() {
        let _guard = TOKEN_TEST_LOCK.blocking_lock();
        set_mcp_token(Some("secret-token-abc".to_string()));
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer "));
        assert_eq!(validate_token(&headers), Err(TokenRejection::Missing));
    }

    #[test]
    fn validate_token_accepts_exact_token() {
        let _guard = TOKEN_TEST_LOCK.blocking_lock();
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
        let _guard = TOKEN_TEST_LOCK.blocking_lock();
        set_mcp_token(None);
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer anything"));
        assert_eq!(validate_token(&headers), Err(TokenRejection::NotConfigured));
    }

    /// A client retrying with a stale token makes one episode in the security log, not a line
    /// per request, and a valid request records nothing.
    #[test]
    fn token_rejections_reach_the_security_log_once_per_episode() {
        use crate::security_log::{McpRejection, SecurityEvent, testing};
        let _guard = TOKEN_TEST_LOCK.blocking_lock();
        set_mcp_token(Some("secret-token-abc".to_string()));
        let capture = testing::capture_events();
        let mut wrong = HeaderMap::new();
        wrong.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer stale"));
        for _ in 0..50 {
            assert!(validate_token(&wrong).is_err());
        }
        let mut right = HeaderMap::new();
        right.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-token-abc"),
        );
        assert!(validate_token(&right).is_ok());
        assert!(validate_token(&HeaderMap::new()).is_err());
        assert_eq!(
            capture.events(),
            vec![
                SecurityEvent::McpRejectionsStarted {
                    reason: McpRejection::WrongToken
                },
                SecurityEvent::McpRejectionsStarted {
                    reason: McpRejection::MissingToken
                },
            ]
        );
    }

    #[test]
    fn a_refused_origin_reaches_the_security_log() {
        use crate::security_log::{McpRejection, SecurityEvent, testing};
        let capture = testing::capture_events();
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("https://evil.com"));
        assert!(validate_origin(&headers).is_err());
        assert_eq!(
            capture.events(),
            vec![SecurityEvent::McpRejectionsStarted {
                reason: McpRejection::ForeignOrigin
            }]
        );
    }
}

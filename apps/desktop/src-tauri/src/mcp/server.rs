//! MCP HTTP server implementation (spec version 2025-11-25).
//!
//! Implements the Streamable HTTP transport as defined in:
//! https://modelcontextprotocol.io/specification/2025-11-25/basic/transports

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures_util::stream;
use serde_json::{Value, json};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tauri::{AppHandle, Runtime};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use super::config::McpConfig;
use super::executor::execute_tool;
use super::protocol::{INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, McpRequest, McpResponse, ServerCapabilities};
use super::resources::{get_all_resources, read_resource};
use super::tools::get_all_tools;

/// The current MCP protocol version we support.
pub const PROTOCOL_VERSION: &str = "2025-11-25";

/// Default protocol version for backwards compatibility (when no header is sent).
pub const DEFAULT_PROTOCOL_VERSION: &str = "2025-03-26";

/// Shared state for the MCP server.
pub struct McpState<R: Runtime> {
    pub app: AppHandle<R>,
    /// Active session ID (set after initialization).
    pub session_id: RwLock<Option<String>>,
    /// Negotiated protocol version for the session.
    pub negotiated_version: RwLock<Option<String>>,
}

impl<R: Runtime> McpState<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self {
            app,
            session_id: RwLock::new(None),
            negotiated_version: RwLock::new(None),
        }
    }
}

/// Start the MCP server.
pub fn start_mcp_server<R: Runtime + 'static>(app: AppHandle<R>, config: McpConfig) {
    if !config.enabled {
        log::info!("MCP server is disabled");
        return;
    }

    let port = config.port;
    let state = Arc::new(McpState::new(app));

    tauri::async_runtime::spawn(async move {
        let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

        let app = Router::new()
            .route("/mcp", post(handle_mcp_post::<R>))
            .route("/mcp", get(handle_mcp_get))
            .route("/mcp/health", get(health_check))
            .layer(cors)
            .with_state(state);

        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        log::debug!("MCP server attempting to bind on http://{}", addr);

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => {
                log::debug!("MCP server successfully bound to {}", addr);
                l
            }
            Err(e) => {
                log::error!("Failed to bind MCP server to {}: {}", addr, e);
                return;
            }
        };

        if let Err(e) = axum::serve(listener, app).await {
            log::error!("MCP server crashed: {}", e);
        }
    });
}

/// Health check endpoint.
async fn health_check() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

/// Validate Origin header for security (DNS rebinding protection).
/// Per spec: Servers MUST validate the Origin header on all incoming connections.
pub fn validate_origin(headers: &HeaderMap) -> Result<(), Box<Response>> {
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
/// Per spec: The client MUST include an Accept header listing both application/json and text/event-stream.
/// We log a warning but allow requests for backwards compatibility.
pub fn validate_accept_header(headers: &HeaderMap) {
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
pub fn prefers_sse(headers: &HeaderMap) -> bool {
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
pub fn get_protocol_version(headers: &HeaderMap) -> String {
    headers
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_PROTOCOL_VERSION.to_string())
}

/// Handle HTTP GET to MCP endpoint.
/// Per 2024-11-05 spec: Server sends an SSE stream with an 'endpoint' event first.
/// Per 2025-11-25 spec: Server MUST return 405 if it doesn't offer SSE, or start an SSE stream.
async fn handle_mcp_get(headers: HeaderMap) -> Response {
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("none");

    log::debug!(
        "MCP: GET /mcp - SSE connection (user-agent: {}, origin: {})",
        user_agent,
        origin
    );
    log::debug!("MCP: GET headers: {:?}", headers);

    // Validate Origin header (security requirement)
    if let Err(response) = validate_origin(&headers) {
        log::warn!("MCP: GET rejected due to Origin validation failure");
        return *response;
    }

    // For backwards compatibility with 2024-11-05 transport, we send an SSE stream
    // that starts with an 'endpoint' event pointing to the same URL for POST
    let endpoint_event = Event::default().event("endpoint").data("/mcp");

    let sse_stream = stream::once(async move { Ok::<_, Infallible>(endpoint_event) });

    Sse::new(sse_stream)
        .keep_alive(axum::response::sse::KeepAlive::new())
        .into_response()
}

/// Format a JSON-RPC response as an SSE event.
pub fn format_sse_event(response: &McpResponse, event_id: Option<&str>) -> Result<Event, Infallible> {
    let json = serde_json::to_string(response).unwrap_or_else(|_| "{}".to_string());
    let mut event = Event::default().event("message").data(json);
    if let Some(id) = event_id {
        event = event.id(id);
    }
    Ok(event)
}

/// Build SSE response with appropriate headers.
fn build_sse_response(response: McpResponse, new_session_id: Option<String>) -> Response {
    // Generate unique event ID for this response
    let event_id = Uuid::new_v4().to_string();

    // Create a stream that yields the response as an SSE event then completes
    let response_clone = response.clone();
    let event_id_clone = event_id.clone();
    let sse_stream = stream::once(async move { format_sse_event(&response_clone, Some(&event_id_clone)) });

    let sse = Sse::new(sse_stream);
    let mut http_response = sse.into_response();

    // Add MCP-Session-Id header on initialize response
    if let Some(ref session_id) = new_session_id
        && let Ok(session_value) = header::HeaderValue::from_str(session_id)
    {
        http_response.headers_mut().insert("mcp-session-id", session_value);
    }

    http_response
}

/// Build JSON response with appropriate headers.
fn build_json_response(response: McpResponse, new_session_id: Option<String>) -> Response {
    let mut http_response = Json(&response).into_response();

    // Add MCP-Session-Id header on initialize response
    if let Some(ref session_id) = new_session_id
        && let Ok(session_value) = header::HeaderValue::from_str(session_id)
    {
        http_response.headers_mut().insert("mcp-session-id", session_value);
    }

    http_response
}

/// Handle HTTP POST to MCP endpoint (main request handler).
async fn handle_mcp_post<R: Runtime>(
    State(state): State<Arc<McpState<R>>>,
    headers: HeaderMap,
    Json(request): Json<McpRequest>,
) -> Response {
    log::debug!("MCP: POST /mcp - method: {}", request.method);
    log::debug!("MCP: POST headers: {:?}", headers);

    // 1. Validate Origin header (security requirement)
    if let Err(response) = validate_origin(&headers) {
        log::warn!("MCP: POST rejected due to Origin validation failure");
        return *response;
    }

    // 2. Validate Accept header (recommended but we're lenient)
    validate_accept_header(&headers);

    // 3. Get protocol version from header
    let client_version = get_protocol_version(&headers);

    // 4. Check if client prefers SSE
    let use_sse = prefers_sse(&headers);

    // 5. For non-initialize requests, validate session if client provides one
    // Per Streamable HTTP spec: sessions are optional for stateless operations
    if request.method != "initialize" {
        let provided_session = headers
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        // On session mismatch, auto-adopt the client's session ID instead of rejecting.
        // This is a single-user localhost server, so strict session validation adds no
        // security benefit and breaks the workflow when the app restarts during dev.
        if let Some(ref client_session) = provided_session
            && let Ok(session_guard) = state.session_id.read()
            && let Some(ref expected_session) = *session_guard
            && client_session != expected_session
        {
            log::info!(
                "MCP: Session ID mismatch (got: {}, expected: {}), auto-adopting client session",
                client_session,
                expected_session
            );
            drop(session_guard);
            if let Ok(mut session_guard) = state.session_id.write() {
                *session_guard = Some(client_session.clone());
            }
        }

        // Validate protocol version matches negotiated version
        if let Ok(version_guard) = state.negotiated_version.read()
            && let Some(ref negotiated) = *version_guard
            && &client_version != negotiated
            && client_version != DEFAULT_PROTOCOL_VERSION
        {
            log::warn!(
                "MCP: Protocol version mismatch: got {}, expected {}",
                client_version,
                negotiated
            );
        }
    }

    // 6. Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        let error = McpResponse::error(request.id.clone(), INVALID_REQUEST, "Invalid JSON-RPC version");
        return if use_sse {
            build_sse_response(error, None)
        } else {
            (StatusCode::BAD_REQUEST, Json(error)).into_response()
        };
    }

    // 7. Handle notifications (no id) - return 202 Accepted with no body per spec
    // Per MCP spec: "If the input is a JSON-RPC notification: the server MUST return
    // HTTP status code 202 Accepted with no body."
    let is_notification = request.id.is_none() || request.method.starts_with("notifications/");
    if is_notification {
        // Still process the notification for side effects
        let _ = process_request(&state, request, &client_version).await;
        return StatusCode::ACCEPTED.into_response();
    }

    // 8. Process the request
    let (response, new_session_id) = process_request(&state, request, &client_version).await;

    // 9. Build response (SSE or JSON based on Accept header)
    if use_sse {
        build_sse_response(response, new_session_id)
    } else {
        build_json_response(response, new_session_id)
    }
}

/// Process an MCP request and return a response.
/// Returns (response, optional new session ID for initialize).
async fn process_request<R: Runtime>(
    state: &McpState<R>,
    request: McpRequest,
    client_version: &str,
) -> (McpResponse, Option<String>) {
    match request.method.as_str() {
        "initialize" => {
            // Generate new session ID
            let session_id = Uuid::new_v4().to_string();

            // Store session and negotiated version
            if let Ok(mut session_guard) = state.session_id.write() {
                *session_guard = Some(session_id.clone());
            }

            // Negotiate protocol version (use latest supported or client's version if older)
            let negotiated = if client_version == PROTOCOL_VERSION || client_version == DEFAULT_PROTOCOL_VERSION {
                PROTOCOL_VERSION.to_string()
            } else {
                client_version.to_string()
            };

            if let Ok(mut version_guard) = state.negotiated_version.write() {
                *version_guard = Some(negotiated);
            }

            let caps = ServerCapabilities::default();
            (
                McpResponse::success(request.id, serde_json::to_value(caps).unwrap()),
                Some(session_id),
            )
        }

        "notifications/initialized" => {
            // Per spec: notifications return 202 Accepted with no body
            // But we use JSON response for consistency in our implementation
            (McpResponse::success(request.id, json!({"acknowledged": true})), None)
        }

        "tools/list" => {
            let tools = get_all_tools();
            (McpResponse::success(request.id, json!({"tools": tools})), None)
        }

        "resources/list" => {
            let resources = get_all_resources();
            (McpResponse::success(request.id, json!({"resources": resources})), None)
        }

        "resources/read" => {
            let uri = match request.params.get("uri").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => {
                    return (
                        McpResponse::error(request.id, INVALID_PARAMS, "Missing 'uri' parameter"),
                        None,
                    );
                }
            };

            match read_resource(&state.app, uri) {
                Ok(content) => (McpResponse::success(request.id, json!({"contents": [content]})), None),
                Err(e) => (McpResponse::error(request.id, INVALID_PARAMS, e), None),
            }
        }

        "tools/call" => {
            let name = match request.params.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => {
                    return (
                        McpResponse::error(request.id, INVALID_PARAMS, "Missing 'name' parameter"),
                        None,
                    );
                }
            };

            let arguments = request.params.get("arguments").cloned().unwrap_or(json!({}));

            let result = execute_tool(&state.app, name, &arguments);

            match result {
                Ok(value) => (
                    McpResponse::success(
                        request.id,
                        json!({"content": [{"type": "text", "text": format_tool_result(&value)}]}),
                    ),
                    None,
                ),
                Err(e) => (McpResponse::error(request.id, e.code, e.message), None),
            }
        }

        "ping" => (McpResponse::success(request.id, json!({})), None),

        _ => (
            McpResponse::error(
                request.id,
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", request.method),
            ),
            None,
        ),
    }
}

/// Format tool result for MCP content response.
fn format_tool_result(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        s.to_string()
    } else {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_health_response() {
        let response = json!({"status": "ok"});
        assert_eq!(response["status"], "ok");
    }

    #[test]
    fn test_validate_origin_localhost() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("http://localhost:3000"));
        assert!(validate_origin(&headers).is_ok());

        headers.insert(header::ORIGIN, HeaderValue::from_static("http://127.0.0.1:9224"));
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
    fn test_format_tool_result_string() {
        let value = json!("simple string");
        assert_eq!(format_tool_result(&value), "simple string");
    }

    #[test]
    fn test_format_tool_result_object() {
        let value = json!({"key": "value"});
        let result = format_tool_result(&value);
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_protocol_version_constant() {
        assert_eq!(PROTOCOL_VERSION, "2025-11-25");
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

    #[test]
    fn test_format_sse_event_basic() {
        let response = McpResponse::success(Some(json!(1)), json!({"status": "ok"}));
        let event = format_sse_event(&response, Some("event-123")).unwrap();

        // Event should be created successfully
        // The actual SSE formatting is handled by axum
        assert!(format!("{:?}", event).contains("message"));
    }

    #[test]
    fn test_format_sse_event_without_id() {
        let response = McpResponse::success(Some(json!(1)), json!({"status": "ok"}));
        let event = format_sse_event(&response, None).unwrap();

        // Event should be created successfully without an ID
        assert!(format!("{:?}", event).contains("message"));
    }

    #[test]
    fn test_format_sse_event_error_response() {
        let response = McpResponse::error(Some(json!(1)), INVALID_REQUEST, "Test error");
        let event = format_sse_event(&response, Some("error-event")).unwrap();

        assert!(format!("{:?}", event).contains("message"));
    }
}

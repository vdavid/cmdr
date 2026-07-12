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
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Runtime};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use super::auth::{
    auto_confirm_token_required_response, generate_token, get_protocol_version, prefers_sse, set_mcp_token,
    tool_call_requires_token, validate_accept_header, validate_origin, validate_token,
};
use super::config::McpConfig;
use super::port_file::{remove_port_file, write_port_file, write_secret_file};
use super::protocol::{INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, McpRequest, McpResponse, ServerCapabilities};
use super::resources::{get_all_resources, read_resource};
use super::tool_registry::{Consumer, execute_tool};
use super::tools::get_all_tools;

/// File name written under `<data_dir>` so external readers (CLI, E2E fixtures, agent
/// helpers) can discover the actual bound port. See `port_file.rs` for the protocol.
pub const PORT_FILE_NAME: &str = "mcp.port";

/// File name written under `<data_dir>` holding the per-instance bearer token. Written
/// 0o600 (owner-only): an attacker who can read it already has the user's filesystem
/// access, so the token is a real gate against local non-Cmdr processes that bind the
/// loopback. See `mcp/CLAUDE.md` § "Why localhost only?".
pub const TOKEN_FILE_NAME: &str = "mcp.token";

/// Handle to the running MCP server task, if any.
static MCP_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// The port the server is actually listening on (0 when not running).
static MCP_ACTUAL_PORT: AtomicU16 = AtomicU16::new(0);

/// The data dir we last wrote the port file into. Used by `stop_mcp_server` to remove the
/// file on shutdown without needing the AppHandle (the FE-driven stop path doesn't have
/// one to hand). Set once at first successful start; subsequent restarts overwrite it.
static MCP_PORT_FILE_DIR: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

fn port_file_dir_slot() -> &'static Mutex<Option<PathBuf>> {
    MCP_PORT_FILE_DIR.get_or_init(|| Mutex::new(None))
}

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

/// What kind of port the caller asked us to bind. Pure so it can be unit-tested without
/// poking at sockets. The bind strategy is decided once at `start_mcp_server` time and the
/// rest of the function pipes the resolved port through.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BindStrategy {
    /// Caller wants an ephemeral port. We bind `127.0.0.1:0` and ask the kernel.
    Ephemeral,
    /// Caller pinned an explicit port. We bind it; on failure we probe upward (today's
    /// behaviour) so a transient one-shot collision doesn't kill MCP for the session.
    Pinned(u16),
}

/// Pure: turn the loaded `(setting_port, env_port_override)` into a `BindStrategy`. The
/// canonical "0 means ephemeral" convention lives in exactly one place: here.
///
/// Precedence (matches `McpConfig::from_settings_and_env`):
///   1. `env_port_override` (typically from `CMDR_MCP_PORT`); 0 → ephemeral.
///   2. The setting value (already folded into `config.port` by the caller); 0 → ephemeral.
///
/// In practice the caller always passes `config.port` directly because env-then-setting
/// already collapsed to one number. This signature accepts both so a future caller can
/// disambiguate `port=0 because env said so` from `port=0 because setting said so`.
pub fn resolve_bind_strategy(config_port: u16) -> BindStrategy {
    if config_port == 0 {
        BindStrategy::Ephemeral
    } else {
        BindStrategy::Pinned(config_port)
    }
}

/// How to handle a busy port when binding a pinned port.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum BindMode {
    /// Bind the exact resolved port; fail with `BindError::PortInUse` if it's taken. Used
    /// for interactive (settings-driven) starts so the user is told their chosen port is
    /// busy rather than silently landing on a different one.
    Exact,
    /// Bind the resolved port, probing up to 100 ports upward on collision. Used at startup
    /// auto-start where there's no user to prompt: getting *a* server up beats failing.
    ProbeOnCollision,
}

/// Why a bind attempt failed. `PortInUse` is a normal, expected outcome of an interactive
/// (Exact) bind against a busy port and is classified by `std::io::ErrorKind::AddrInUse`
/// (an errno, not a string match). `Other` is everything else.
#[derive(Debug)]
pub enum BindError {
    /// The requested port (Exact mode) was already in use.
    PortInUse(u16),
    /// Any other bind failure (probe exhaustion, permissions, invalid address, …).
    Other(String),
}

/// Result of a server lifecycle transition, returned across the IPC boundary so the
/// frontend can react on a typed `kind` discriminant rather than parsing a message
/// (AGENTS.md § "No string-matching state classification"). Serializes as
/// `{"kind":"running","port":N}` / `{"kind":"stopped"}` / `{"kind":"portInUse","requested":N}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum McpServerOutcome {
    /// Server is running on this port.
    Running { port: u16 },
    /// Server is stopped.
    Stopped,
    /// The requested port was in use; the server's previous state is left unchanged.
    PortInUse { requested: u16 },
}

/// What an interactive (re)bind should do, decided purely so the self-collision guard is
/// unit-testable without sockets.
#[derive(Debug, PartialEq, Eq)]
pub enum RebindAction {
    /// We're already bound to the target pinned port; do nothing, report `Running(port)`.
    /// This is the guard that prevents Exact-binding a port we already hold (the bug where
    /// a restart raced its own still-open socket and "lost" the port).
    NoOp(u16),
    /// Bind a fresh listener, then retire the old server.
    Rebind,
}

/// Pure: given the desired strategy and the port we're currently bound to (0 = not
/// running), decide the interactive-rebind action. A pinned target equal to the live port
/// is a no-op; everything else rebinds (ephemeral always picks a fresh port, so it never
/// self-collides).
pub fn decide_rebind_action(strategy: BindStrategy, actual_port: u16) -> RebindAction {
    match strategy {
        BindStrategy::Pinned(p) if p != 0 && p == actual_port => RebindAction::NoOp(p),
        _ => RebindAction::Rebind,
    }
}

/// Bind a listener for `strategy`, honoring `mode` on collision. The single bind entry
/// point: ephemeral trusts the kernel, pinned either probes upward or fails exact.
async fn bind_listener(strategy: BindStrategy, mode: BindMode) -> Result<(tokio::net::TcpListener, u16), BindError> {
    match strategy {
        BindStrategy::Ephemeral => bind_ephemeral().await.map_err(BindError::Other),
        BindStrategy::Pinned(p) => match mode {
            BindMode::ProbeOnCollision => bind_with_probe(p).await.map_err(BindError::Other),
            BindMode::Exact => {
                let addr = SocketAddr::from(([127, 0, 0, 1], p));
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(listener) => Ok((listener, p)),
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => Err(BindError::PortInUse(p)),
                    Err(e) => Err(BindError::Other(format!("MCP server: failed to bind port {p}: {e}"))),
                }
            }
        },
    }
}

/// Try to bind a tokio TcpListener starting at `start_port`, probing up to 100 ports.
/// Returns the bound listener and the port it's on. Only used for the `Pinned` strategy;
/// ephemeral binds go straight to `127.0.0.1:0` and trust the kernel.
async fn bind_with_probe(start_port: u16) -> Result<(tokio::net::TcpListener, u16), String> {
    for offset in 0u16..100 {
        let port = start_port.saturating_add(offset);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => return Ok((listener, port)),
            Err(e) => {
                log::debug!("MCP server: port {} unavailable ({}), trying next", port, e);
            }
        }
    }
    Err(format!(
        "No available port found starting from {} (tried 100 ports).",
        start_port
    ))
}

/// Bind `127.0.0.1:0` and ask the OS for the assigned port. Single round-trip, no probing.
async fn bind_ephemeral() -> Result<(tokio::net::TcpListener, u16), String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("MCP server: failed to bind ephemeral port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("MCP server: bound but local_addr failed: {e}"))?
        .port();
    Ok((listener, port))
}

/// Start the MCP server (startup auto-start path). Binds the configured port with upward
/// probing on collision (resilience: at launch there's no user to prompt, so getting *a*
/// server up wins), then serves. No-op if already running or disabled. Interactive
/// settings-driven (re)starts go through `rebind_interactive` instead, which is honest
/// about a busy port rather than silently probing.
pub async fn start_mcp_server<R: Runtime + 'static>(app: AppHandle<R>, config: McpConfig) -> Result<(), String> {
    if !config.enabled {
        log::info!("MCP server is disabled");
        return Ok(());
    }

    // Guard against double-start
    if is_mcp_running() {
        log::debug!("MCP server is already running, ignoring start request");
        return Ok(());
    }

    let strategy = resolve_bind_strategy(config.port);

    // Resolve the data dir up front so a write failure on the port file is visible at
    // start time (not when we tear the server down). The dir is what `<data_dir>/mcp.port`
    // lives in; we cache it in `MCP_PORT_FILE_DIR` for the shutdown path.
    let data_dir = crate::config::resolved_app_data_dir(&app)
        .map_err(|e| format!("MCP server: could not resolve data dir for port file: {e}"))?;

    let (listener, port) = bind_listener(strategy, BindMode::ProbeOnCollision)
        .await
        .map_err(|e| match e {
            BindError::PortInUse(p) => format!("MCP server: port {p} is in use"),
            BindError::Other(msg) => msg,
        })?;
    if let BindStrategy::Pinned(p) = strategy
        && p != port
    {
        log::info!("MCP server: port {} is in use, using port {} instead", p, port);
    }

    serve_on(app, listener, port, data_dir);
    Ok(())
}

/// Interactive (settings-driven) (re)bind to `config.port`. Binds the NEW listener BEFORE
/// retiring the old one, so:
///   - a busy port leaves the existing server running and reports `PortInUse` (the user
///     hears "that port's taken" instead of the server silently landing elsewhere),
///   - a successful change never drops an in-flight request (the new listener is up before
///     the old one goes down), and
///   - we never collide with our own still-open socket (the `decide_rebind_action` guard
///     short-circuits a no-change re-apply; otherwise the new port always differs from the
///     live one, so the sync abort of the old server can't contend for it).
///
/// This is the fix for the restart race: the old path aborted the serve task (which only
/// requests cancellation) and immediately re-bound, so a probe could walk into the
/// not-yet-released socket and "lose" the port.
pub async fn rebind_interactive<R: Runtime + 'static>(
    app: AppHandle<R>,
    config: McpConfig,
) -> Result<McpServerOutcome, String> {
    let strategy = resolve_bind_strategy(config.port);

    if let RebindAction::NoOp(port) = decide_rebind_action(strategy, MCP_ACTUAL_PORT.load(Ordering::Relaxed)) {
        log::debug!(target: "mcp::server", "MCP server already bound to port {port}, nothing to rebind");
        return Ok(McpServerOutcome::Running { port });
    }

    let data_dir = crate::config::resolved_app_data_dir(&app)
        .map_err(|e| format!("MCP server: could not resolve data dir for port file: {e}"))?;

    // Bind the new listener first — the old server (if any) is still up on a different port.
    let (listener, port) = match bind_listener(strategy, BindMode::Exact).await {
        Ok(bound) => bound,
        Err(BindError::PortInUse(requested)) => {
            log::info!(target: "mcp::server", "MCP server: requested port {requested} is in use, leaving server unchanged");
            return Ok(McpServerOutcome::PortInUse { requested });
        }
        Err(BindError::Other(msg)) => return Err(msg),
    };

    // New listener is up; retire the old server (sync abort is enough — different port, no
    // contention) and serve on the new listener.
    stop_mcp_server();
    serve_on(app, listener, port, data_dir);
    Ok(McpServerOutcome::Running { port })
}

/// Take a bound listener and bring the server fully online: store the actual port, mint a
/// fresh bearer token, write the port + token files, and spawn the serve task. The back
/// half of a start, shared by the startup and interactive paths.
fn serve_on<R: Runtime + 'static>(app: AppHandle<R>, listener: tokio::net::TcpListener, port: u16, data_dir: PathBuf) {
    let state = Arc::new(McpState::new(app));

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let router = Router::new()
        .route("/mcp", post(handle_mcp_post::<R>))
        .route("/mcp", get(handle_mcp_get))
        .route("/mcp/health", get(health_check))
        .layer(cors)
        .with_state(state);

    log::info!("MCP server listening on http://127.0.0.1:{}", port);

    MCP_ACTUAL_PORT.store(port, Ordering::Relaxed);

    // Generate a fresh per-instance bearer token and store it in-process BEFORE the serve
    // task starts handling requests, so the very first request is already gated. The token
    // is regenerated on every start; a leaked token from a prior run can't be replayed.
    //
    // `CMDR_MCP_TOKEN` overrides the random token with a fixed value. That makes the token
    // stable across restarts, so an external client can pin a static
    // `Authorization: Bearer ${CMDR_MCP_TOKEN}` header (handy for a `.mcp.json` server
    // entry). The tradeoff: a fixed token loses the per-launch replay protection the random
    // token gives, so this is opt-in for the dev workflow, not the default.
    let token = match std::env::var("CMDR_MCP_TOKEN") {
        Ok(env_token) if !env_token.trim().is_empty() => env_token.trim().to_string(),
        _ => generate_token(),
    };
    set_mcp_token(Some(token.clone()));

    // Write the token file 0o600 (owner-only). Unlike the port, the token has no in-process
    // fallback for external readers, so a write failure means external clients (CLI, E2E)
    // can't authenticate — but it's still non-fatal for the server: it's logged and the
    // in-process token (read via `get_mcp_token` IPC) keeps the FE-driven E2E path working.
    if let Err(err) = write_secret_file(&data_dir, TOKEN_FILE_NAME, &token) {
        log::warn!(
            target: "mcp::server",
            "MCP server bound on {port} but could not write token file at {}: {err}",
            data_dir.display(),
        );
    } else {
        log::debug!(target: "mcp::server", "Wrote MCP token file {}/{TOKEN_FILE_NAME}", data_dir.display());
    }

    // Write the port file BEFORE handing the listener to the spawned serve task. Failure
    // here is logged but non-fatal: external readers can fall back to `CMDR_MCP_PORT` or
    // the FE IPC, and the server itself is healthy.
    if let Err(err) = write_port_file(&data_dir, PORT_FILE_NAME, port) {
        log::warn!(
            target: "mcp::server",
            "MCP server bound on {port} but could not write port file at {}: {err}",
            data_dir.display(),
        );
    } else {
        log::debug!(
            target: "mcp::server",
            "Wrote MCP port file {}/{PORT_FILE_NAME} = {port}",
            data_dir.display(),
        );
        if let Ok(mut slot) = port_file_dir_slot().lock() {
            *slot = Some(data_dir);
        }
    }

    let handle = tauri::async_runtime::spawn(async move {
        if let Err(e) = axum::serve(listener, router).await {
            crate::log_error!("MCP server crashed: {}", e);
        }
        // Server exited (crash or graceful shutdown); reset port so
        // is_mcp_running() and get_mcp_actual_port() reflect reality. The on-disk port
        // file is removed in `stop_mcp_server`; for a true crash here we leave it (stale)
        // and trust readers to retry on `ECONNREFUSED`. Clear the in-process token too so
        // `get_mcp_token` reflects "not running" and `validate_token` fails closed.
        MCP_ACTUAL_PORT.store(0, Ordering::Relaxed);
        set_mcp_token(None);
    });

    if let Ok(mut guard) = MCP_HANDLE.lock() {
        *guard = Some(handle);
    }
}

/// Start the MCP server in a fire-and-forget manner (for app startup).
/// Logs errors instead of returning them.
pub fn start_mcp_server_background<R: Runtime + 'static>(app: AppHandle<R>, config: McpConfig) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = start_mcp_server(app, config).await {
            crate::log_error!("MCP server failed to start: {}", e);
        }
    });
}

/// Take the running server's handle (if any) and reset all process-wide state to "stopped":
/// zero the actual port, clear the in-process token (so `validate_token` fails closed), and
/// remove the port + token files. Returns the handle so the caller can decide whether to
/// just `abort()` it (sync stop) or `abort().await` it (wait for the socket to release).
/// Logged but ignored file-removal failures: a stale file is a leftover, not a correctness
/// bug.
fn take_handle_and_reset() -> Option<JoinHandle<()>> {
    let handle = MCP_HANDLE.lock().ok().and_then(|mut guard| guard.take());
    if handle.is_some() {
        MCP_ACTUAL_PORT.store(0, Ordering::Relaxed);
        set_mcp_token(None);
        if let Ok(mut slot) = port_file_dir_slot().lock()
            && let Some(dir) = slot.take()
        {
            remove_port_file(&dir, PORT_FILE_NAME);
            remove_port_file(&dir, TOKEN_FILE_NAME);
        }
    }
    handle
}

/// Stop the MCP server if it's running, without waiting for the serve task to wind down.
/// `abort()` only *requests* cancellation, so the listener socket may linger briefly after
/// this returns. That's fine for the shutdown path (the process is exiting) and for the
/// retire-the-old-server step of an interactive rebind (the new listener is already up on a
/// different port). When a subsequent bind must reuse the same port — re-enabling after a
/// disable — use `stop_mcp_server_and_wait` instead so the socket is released first.
pub fn stop_mcp_server() {
    if let Some(handle) = take_handle_and_reset() {
        handle.abort();
        log::info!("MCP server stopped");
    }
}

/// Stop the MCP server and wait until the serve task is fully gone, guaranteeing the
/// listener socket is released before returning. Used by the interactive disable path so an
/// immediate re-enable on the same port can bind it cleanly instead of racing a
/// not-yet-closed socket.
pub async fn stop_mcp_server_and_wait() {
    if let Some(handle) = take_handle_and_reset() {
        handle.abort();
        // Awaiting an aborted handle resolves once the task (and its `TcpListener`) is
        // dropped. We don't care about the `JoinError::Cancelled` it yields.
        let _ = handle.await;
        log::info!("MCP server stopped");
    }
}

/// Returns whether the MCP server task is currently running.
/// Uses `MCP_ACTUAL_PORT` as the source of truth: the spawned task resets it
/// to 0 when it exits (crash or graceful shutdown), so a non-zero port means running.
pub fn is_mcp_running() -> bool {
    MCP_ACTUAL_PORT.load(Ordering::Relaxed) != 0
}

/// Returns the port the MCP server is actually listening on, or `None` if not running.
pub fn get_mcp_actual_port() -> Option<u16> {
    let port = MCP_ACTUAL_PORT.load(Ordering::Relaxed);
    if port == 0 { None } else { Some(port) }
}

/// Health check endpoint.
async fn health_check() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
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

    log::trace!(
        "MCP: GET /mcp - SSE connection (user-agent: {}, origin: {})",
        user_agent,
        origin
    );
    log::trace!("MCP: GET headers: {:?}", headers);

    // Validate Origin header (browser-CSRF / DNS-rebinding defense)
    if let Err(response) = validate_origin(&headers) {
        log::warn!("MCP: GET rejected due to Origin validation failure");
        return *response;
    }

    // No token gate on the SSE stream: GET carries no tool call, so it can't bypass a
    // confirmation dialog. The token is enforced per-request in the POST handler only.

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

    // 1. Validate Origin header (browser-CSRF / DNS-rebinding defense)
    if let Err(response) = validate_origin(&headers) {
        log::warn!("MCP: POST rejected due to Origin validation failure");
        return *response;
    }

    // 1b. Token gate, but only for calls that bypass the user's confirmation dialog
    // (destructive auto-confirm + programmatic dialog confirm). Reads, nav, search, and
    // destructive ops that still prompt the user all pass without a token.
    if tool_call_requires_token(&request.method, &request.params) && validate_token(&headers).is_err() {
        // Name the gated tool in the rejection (the `name` field of the tools/call
        // params) so the message describes the ACTUAL tool, not a hardcoded
        // "destructive file operation" that's wrong for set_setting / indexing / tag /
        // favorites. Same value for a missing vs a wrong token, so no oracle.
        let tool_name = request.params.get("name").and_then(|v| v.as_str());
        return auto_confirm_token_required_response(&state.app, request.id.clone(), tool_name);
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
            log::debug!(
                "MCP: Protocol version mismatch: got {}, expected {} (accepting per-request; version was negotiated at initialize)",
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

            // Negotiate protocol version (use latest supported or client's version if older).
            // Older clients (2024-11-05 spec) send protocolVersion in the JSON body only,
            // not the HTTP header. Fall back to the body value when the header is absent.
            let effective_version = if client_version == DEFAULT_PROTOCOL_VERSION {
                request
                    .params
                    .get("protocolVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or(client_version)
            } else {
                client_version
            };
            let negotiated = if effective_version == PROTOCOL_VERSION || effective_version == DEFAULT_PROTOCOL_VERSION {
                PROTOCOL_VERSION.to_string()
            } else {
                effective_version.to_string()
            };

            if let Ok(mut version_guard) = state.negotiated_version.write() {
                *version_guard = Some(negotiated);
            }

            let caps = ServerCapabilities::default();
            (
                McpResponse::success(
                    request.id,
                    serde_json::to_value(caps)
                        .expect("ServerCapabilities is a plain serde struct that always serializes"),
                ),
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

            match read_resource(&state.app, uri).await {
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

            if name == "ai_search" {
                log::debug!("MCP ai_search: tools/call received, id={:?}", request.id);
            }

            log::debug!("MCP: executing tool {name}");
            // The HTTP transport is the ai_client consumer: it dispatches only the ai_client
            // view (an agent-only name is refused before dispatch). See `tool_registry`.
            let result = execute_tool(&state.app, Consumer::AiClient, name, &arguments).await;

            match result {
                Ok(ref value) => {
                    let text = format_tool_result(value);
                    log::debug!("MCP: tool {name} succeeded, response length={}", text.len());
                    (
                        McpResponse::success(request.id, json!({"content": [{"type": "text", "text": text}]})),
                        None,
                    )
                }
                Err(e) => {
                    log::warn!("MCP: tool {name} failed, code={}, message={}", e.code, e.message);
                    (McpResponse::error(request.id, e.code, e.message), None)
                }
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

    #[test]
    fn test_health_response() {
        let response = json!({"status": "ok"});
        assert_eq!(response["status"], "ok");
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
    fn resolve_bind_strategy_zero_means_ephemeral() {
        assert_eq!(resolve_bind_strategy(0), BindStrategy::Ephemeral);
    }

    #[test]
    fn resolve_bind_strategy_nonzero_is_pinned() {
        assert_eq!(resolve_bind_strategy(19224), BindStrategy::Pinned(19224));
        assert_eq!(resolve_bind_strategy(1), BindStrategy::Pinned(1));
        assert_eq!(resolve_bind_strategy(65535), BindStrategy::Pinned(65535));
    }

    // ── decide_rebind_action (self-collision guard) ──────────────────────────
    // This is the heart of the restart-race fix: never re-bind a port we already
    // hold. Re-applying the live pinned port is a no-op; everything else rebinds.

    #[test]
    fn rebind_noop_when_target_equals_live_pinned_port() {
        // Server bound to 19225; user re-applies 19225 → no rebind (would otherwise
        // Exact-bind a port we still hold and falsely report it "in use").
        assert_eq!(
            decide_rebind_action(BindStrategy::Pinned(19225), 19225),
            RebindAction::NoOp(19225)
        );
    }

    #[test]
    fn rebind_when_pinned_target_differs_from_live_port() {
        // Bound to 19225, user picks 19226 → rebind. Also covers the startup-probe
        // case where actual (19227) differs from the newly-requested pin.
        assert_eq!(
            decide_rebind_action(BindStrategy::Pinned(19226), 19225),
            RebindAction::Rebind
        );
        assert_eq!(
            decide_rebind_action(BindStrategy::Pinned(19225), 19227),
            RebindAction::Rebind
        );
    }

    #[test]
    fn rebind_when_not_running() {
        // actual_port == 0 means stopped; a pinned start always rebinds.
        assert_eq!(
            decide_rebind_action(BindStrategy::Pinned(19225), 0),
            RebindAction::Rebind
        );
    }

    #[test]
    fn rebind_ephemeral_always_rebinds() {
        // Ephemeral picks a fresh kernel port each time, so it never self-collides;
        // always rebind regardless of the live port.
        assert_eq!(decide_rebind_action(BindStrategy::Ephemeral, 0), RebindAction::Rebind);
        assert_eq!(
            decide_rebind_action(BindStrategy::Ephemeral, 54321),
            RebindAction::Rebind
        );
    }

    // ── bind_listener: Exact is honest about collisions, Probe walks past them ──

    #[tokio::test]
    async fn bind_listener_ephemeral_succeeds() {
        let (listener, port) = bind_listener(BindStrategy::Ephemeral, BindMode::Exact)
            .await
            .expect("ephemeral bind should succeed");
        assert!(port != 0, "kernel must assign a real port");
        drop(listener);
    }

    #[tokio::test]
    async fn bind_listener_exact_reports_port_in_use() {
        // Occupy an ephemeral port and keep it held.
        let (held, port) = bind_ephemeral().await.expect("occupy a port");
        match bind_listener(BindStrategy::Pinned(port), BindMode::Exact).await {
            Err(BindError::PortInUse(p)) => assert_eq!(p, port),
            Ok(_) => panic!("Exact bind on a held port must fail, not succeed on a different port"),
            Err(BindError::Other(msg)) => panic!("expected PortInUse, got Other: {msg}"),
        }
        drop(held);
    }

    #[tokio::test]
    async fn bind_listener_probe_walks_past_a_busy_port() {
        // Occupy a port; ProbeOnCollision must land on a *different*, higher port.
        let (held, port) = bind_ephemeral().await.expect("occupy a port");
        let (probed_listener, probed) = bind_listener(BindStrategy::Pinned(port), BindMode::ProbeOnCollision)
            .await
            .expect("probe should find a free port");
        assert_ne!(probed, port, "probe must not return the busy port");
        assert!(probed > port, "probe walks upward");
        drop(probed_listener);
        drop(held);
    }

    // ── abort + await releases the listener socket ───────────────────────────
    // The property `stop_mcp_server_and_wait` relies on: aborting a serve task and
    // awaiting its handle drops the `TcpListener`, so the very next bind on the same
    // port succeeds. Validated here at the runtime level (the real stop path needs an
    // AppHandle-backed serve task, which isn't available in a unit test).

    #[tokio::test]
    async fn abort_then_await_frees_the_port() {
        let (listener, port) = bind_ephemeral().await.expect("bind a port to hold");
        let addr = SocketAddr::from(([127, 0, 0, 1], port));

        // Hold the listener inside a task that never completes on its own.
        let handle = tokio::spawn(async move {
            let _held = listener;
            std::future::pending::<()>().await;
        });

        // Sanity: while the task holds it, the port is unavailable.
        assert!(
            tokio::net::TcpListener::bind(addr).await.is_err(),
            "port should be busy while the task holds the listener"
        );

        handle.abort();
        let _ = handle.await; // resolves once the task (and its listener) is dropped

        // The socket must be free immediately after the awaited abort.
        tokio::net::TcpListener::bind(addr)
            .await
            .expect("port must be bindable right after abort+await");
    }

    // ── McpServerOutcome wire shape (Rust ↔ TS contract) ─────────────────────
    // The frontend switches on the `kind` discriminant, so pin its serialization.

    #[test]
    fn mcp_server_outcome_json_shape() {
        assert_eq!(
            serde_json::to_value(McpServerOutcome::Running { port: 19225 }).unwrap(),
            json!({"kind": "running", "port": 19225})
        );
        assert_eq!(
            serde_json::to_value(McpServerOutcome::Stopped).unwrap(),
            json!({"kind": "stopped"})
        );
        assert_eq!(
            serde_json::to_value(McpServerOutcome::PortInUse { requested: 19225 }).unwrap(),
            json!({"kind": "portInUse", "requested": 19225})
        );
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

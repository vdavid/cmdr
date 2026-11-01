//! The MCP server's HTTP surface and lifecycle: binding, rebinding, the discovery files, and the
//! route table. The official SDK (`rmcp`) speaks the protocol; `handler.rs` is what it calls, and
//! `auth::guard` runs in front of it.

use axum::{
    Json, Router,
    http::{HeaderName, Method, header},
    middleware,
    response::IntoResponse,
    routing::get,
};
use cmdr_fs::log_private::LogPrivate;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use serde_json::json;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Runtime};
use tower_http::cors::{AllowOrigin, CorsLayer};

use super::auth::{generate_token, guard, is_localhost_origin, set_mcp_token};
use super::config::McpConfig;
use super::handler::CmdrMcpHandler;
use super::port_file::{remove_port_file, write_port_file, write_secret_file};

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
    let router = build_router(app);

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
            LogPrivate::dir_path(&data_dir),
        );
    } else {
        log::debug!(
            target: "mcp::server",
            "Wrote MCP token file {}",
            LogPrivate::file_path(&data_dir.join(TOKEN_FILE_NAME))
        );
    }

    // Write the port file BEFORE handing the listener to the spawned serve task. Failure
    // here is logged but non-fatal: external readers can fall back to `CMDR_MCP_PORT` or
    // the FE IPC, and the server itself is healthy.
    if let Err(err) = write_port_file(&data_dir, PORT_FILE_NAME, port) {
        log::warn!(
            target: "mcp::server",
            "MCP server bound on {port} but could not write port file at {}: {err}",
            LogPrivate::dir_path(&data_dir),
        );
    } else {
        log::debug!(
            target: "mcp::server",
            "Wrote MCP port file {} = {port}",
            LogPrivate::file_path(&data_dir.join(PORT_FILE_NAME)),
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

/// Build the complete HTTP surface. Kept as one function so transport-level tests exercise
/// the exact routes and CORS policy production serves rather than a lookalike test router.
///
/// A fresh service per `serve_on`, and its cancellation token is never cancelled: a rebind retiring
/// this server leaves in-flight requests to finish.
fn build_router<R: Runtime + 'static>(app: AppHandle<R>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            origin.to_str().is_ok_and(is_localhost_origin)
        }))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::ACCEPT,
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            HeaderName::from_static("mcp-protocol-version"),
            HeaderName::from_static("mcp-session-id"),
            HeaderName::from_static("mcp-method"),
            HeaderName::from_static("mcp-name"),
            HeaderName::from_static("last-event-id"),
        ]);

    // Stateless, which keeps "a client restart never breaks". The default `allowed_hosts` stays on;
    // `rmcp`'s own Origin check stays off, since `auth::guard` owns the log and the toast.
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None);
    let guard_app = app.clone();
    let service = StreamableHttpService::new(
        move || Ok(CmdrMcpHandler::new(app.clone())),
        Arc::new(NeverSessionManager::default()),
        config,
    );

    Router::new()
        .route_service("/mcp", service)
        .route_layer(middleware::from_fn_with_state(guard_app, guard::<R>))
        .route("/mcp/health", get(health_check))
        .layer(cors)
}

/// Start the MCP server in a fire-and-forget manner (for app startup).
/// Logs errors instead of returning them.
pub fn start_mcp_server_background<R: Runtime + 'static>(app: AppHandle<R>, config: McpConfig) {
    tauri::async_runtime::spawn(async move {
        let enabled = config.enabled;
        match start_mcp_server(app, config).await {
            // The security log says the server runs from launch, so a log that starts
            // mid-session still knows.
            Ok(()) if enabled && is_mcp_running() => {
                crate::security_log::record(crate::security_log::SecurityEvent::McpServerTurnedOn {
                    by: crate::security_log::Initiator::Launch,
                })
            }
            Ok(()) => {}
            Err(e) => crate::log_error!("MCP server failed to start: {}", e),
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

#[cfg(test)]
#[path = "tests/transport_characterization.rs"]
mod transport_characterization;

#[cfg(test)]
mod tests {
    use super::*;

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
}

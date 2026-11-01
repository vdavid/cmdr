//! What an outside client can see of the MCP transport, pinned black-box over a real listener.
//!
//! This file proved the move to the official SDK, with the same cells before and after it, and it
//! pins the next SDK upgrade the same way. So every assertion reads the wire (status, headers, bytes)
//! or a file on disk, never a handler's internals, and [`TestServer::start`] is the one place that
//! names how a router gets built. Everything here is ours to keep whatever the SDK does;
//! `transport_sdk_answers.rs` holds the answers that are the SDK's own.
//!
//! The toast for a refused Origin can't be seen from here: `security_notices` raises it only for a
//! real `AppHandle`, which a mock app isn't. The cell pins the exact security event instead, and
//! `security_notices`'s `a_web_page_a_refused_reply_and_a_fake_update_reach_the_person` pins that
//! this event is what raises the toast.

use std::net::SocketAddr;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use serde_json::{Value, json};
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{
    BindMode, BindStrategy, McpServerOutcome, PORT_FILE_NAME, TOKEN_FILE_NAME, bind_listener, build_router,
    rebind_interactive, start_mcp_server, stop_mcp_server_and_wait,
};
use crate::managed_policy::testing::{DISABLE_USAGE_STATS, forcing, override_for_test};
use crate::mcp::auth::{TOKEN_TEST_LOCK, current_mcp_token, set_mcp_token};
use crate::mcp::config::McpConfig;
use crate::security_log::{McpRejection, SecurityEvent, testing as security_log_testing};
use crate::test_support::{capture_logs, isolate_secrets, wait_until_async};
use crate::ui_state::dialog_state::SoftDialogTracker;
use crate::ui_state::pane_state::{PaneState, PaneStateStore};

const TOKEN: &str = "transport-characterization-token";
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

type MockApp = tauri::App<tauri::test::MockRuntime>;

#[path = "transport_sdk_answers.rs"]
mod sdk_answers;

/// A mock app holding the two stores the tool gates read.
fn app_with_stores() -> MockApp {
    let app = tauri::test::mock_app();
    app.manage(PaneStateStore::new());
    app.manage(SoftDialogTracker::new());
    app
}

/// A real listener serving the production router, on the caller's runtime.
struct TestServer {
    addr: SocketAddr,
    client: reqwest::Client,
    task: tokio::task::JoinHandle<()>,
}

impl TestServer {
    async fn start(app: &MockApp) -> Self {
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("test server should bind");
        let addr = listener.local_addr().expect("bound listener should have an address");
        let router = build_router(app.handle().clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("test server should serve");
        });
        set_mcp_token(Some(TOKEN.to_string()));
        Self {
            addr,
            client: cmdr_http::client_builder().build().expect("a plain client builds"),
            task,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// An authenticated POST, answered as JSON.
    async fn call(&self, body: Value) -> Value {
        let response = self
            .client
            .post(self.url())
            .bearer_auth(TOKEN)
            .json(&body)
            .send()
            .await
            .expect("request should complete");
        assert_eq!(response.status(), 200, "for {body}");
        response.json().await.expect("the reply should be JSON")
    }

    /// `tools/call` for `name`, answered as JSON.
    async fn tool(&self, name: &str, arguments: Value) -> Value {
        self.call(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":arguments}}))
            .await
    }

    /// One POST with exactly `headers` on the wire, which `reqwest` can't do: it adds `Accept: */*`
    /// by itself, and the `Host` it sends is its own.
    async fn raw_post(&self, headers: &[(&str, &str)], body: &str) -> RawResponse {
        let mut request = format!(
            "POST /mcp HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);

        let mut stream = tokio::net::TcpStream::connect(self.addr)
            .await
            .expect("the server should accept");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("the request should send");
        let mut reply = Vec::new();
        stream.read_to_end(&mut reply).await.expect("the reply should read");
        RawResponse::parse(&String::from_utf8_lossy(&reply))
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
        set_mcp_token(None);
    }
}

struct RawResponse {
    status: u16,
    body: String,
}

impl RawResponse {
    fn parse(reply: &str) -> Self {
        let (head, body) = reply.split_once("\r\n\r\n").expect("a reply has a header block");
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .expect("a reply has a status line");
        Self {
            status,
            body: body.to_string(),
        }
    }
}

// ── Binding and the discovery files ─────────────────────────────────────────

/// A port nothing holds right now, for a cell that needs to name one in advance.
fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("a free port exists")
        .local_addr()
        .expect("bound listener should have an address")
        .port()
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("the file should exist")
        .permissions()
        .mode()
        & 0o777
}

fn port_on_disk(dir: &Path) -> u16 {
    std::fs::read_to_string(dir.join(PORT_FILE_NAME))
        .expect("the port file should exist")
        .trim()
        .parse()
        .expect("the port file should hold a port")
}

fn token_on_disk(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(TOKEN_FILE_NAME)).expect("the token file should exist")
}

/// Whether `port` answers a request carrying `token`, which only the server holding it can.
async fn serves_with_token(port: u16, token: &str) -> bool {
    let reply = cmdr_http::client_builder()
        .build()
        .expect("a plain client builds")
        .post(format!("http://127.0.0.1:{port}/mcp"))
        .bearer_auth(token)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
        .send()
        .await
        .expect("the server should answer");
    let envelope: Value = reply.json().await.expect("the reply should be JSON");
    envelope.get("result").is_some()
}

fn refuses_connections(port: u16) -> bool {
    std::net::TcpStream::connect(("127.0.0.1", port)).is_err()
}

/// The lifecycle cells drive the production start and stop, which spawn on Tauri's runtime, so they
/// run on it too: a listener serves from the runtime that bound it.
fn on_tauri_runtime(cell: impl Future<Output = ()>) {
    let _token_guard = TOKEN_TEST_LOCK.blocking_lock();
    // SAFETY: `set_var` is unsound only under concurrent env access. nextest gives this test its own
    // process, and `TOKEN_TEST_LOCK` is the only way to reach a server start, the one reader.
    unsafe { std::env::remove_var("CMDR_MCP_TOKEN") };
    // allowed-discarded-outcome: the cell's output is `()`; a failed assertion panics through this
    tauri::async_runtime::block_on(cell);
}

#[tokio::test]
async fn every_bind_mode_listens_on_loopback_only() {
    let held = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("occupy a port");
    let busy = held.local_addr().expect("bound").port();
    let free = free_port();
    for (strategy, mode) in [
        (BindStrategy::Ephemeral, BindMode::ProbeOnCollision),
        (BindStrategy::Ephemeral, BindMode::Exact),
        (BindStrategy::Pinned(free), BindMode::Exact),
        (BindStrategy::Pinned(busy), BindMode::ProbeOnCollision),
    ] {
        let (listener, port) = bind_listener(strategy, mode).await.expect("the bind should work");
        let bound = listener.local_addr().expect("bound");
        assert_eq!(bound.ip().to_string(), "127.0.0.1", "{strategy:?} {mode:?}");
        assert_eq!(bound.port(), port, "{strategy:?} {mode:?}");
    }
}

#[test]
fn startup_binds_ephemeral_when_the_port_is_zero_and_writes_owner_only_files() {
    let data_dir = isolate_secrets();
    let app = app_with_stores();
    on_tauri_runtime(async {
        let dir = data_dir.as_ref();
        start_mcp_server(app.handle().clone(), McpConfig { enabled: true, port: 0 })
            .await
            .expect("the server should start");

        let port = port_on_disk(dir);
        assert_ne!(port, 0);
        let token = token_on_disk(dir);
        assert_eq!(token.len(), 32, "a fresh random token, with no trailing newline");
        assert!(serves_with_token(port, &token).await);

        assert_eq!(mode_of(&dir.join(PORT_FILE_NAME)), 0o600);
        assert_eq!(mode_of(&dir.join(TOKEN_FILE_NAME)), 0o600);
        assert_eq!(
            std::fs::read_to_string(dir.join(PORT_FILE_NAME)).expect("the port file"),
            format!("{port}\n")
        );

        // A clean stop takes both files away and fails closed.
        stop_mcp_server_and_wait().await;
        assert!(!dir.join(PORT_FILE_NAME).exists());
        assert!(!dir.join(TOKEN_FILE_NAME).exists());
        assert_eq!(current_mcp_token(), None);
        assert!(refuses_connections(port));
    });
}

#[test]
fn the_discovery_files_are_replaced_atomically_and_never_inherit_a_loose_mode() {
    let data_dir = isolate_secrets();
    let app = app_with_stores();
    on_tauri_runtime(async {
        let dir = data_dir.as_ref();
        // A crash leaves these behind. A write in place would keep both the inode and the mode.
        let mut stale = Vec::new();
        for name in [PORT_FILE_NAME, TOKEN_FILE_NAME] {
            let path = dir.join(name);
            std::fs::write(&path, "stale").expect("seed a leftover");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("loosen it");
            stale.push((path.clone(), std::fs::metadata(&path).expect("seeded").ino()));
        }

        start_mcp_server(app.handle().clone(), McpConfig { enabled: true, port: 0 })
            .await
            .expect("the server should start");

        for (path, stale_inode) in stale {
            assert_ne!(
                std::fs::metadata(&path).expect("written").ino(),
                stale_inode,
                "{path:?}"
            );
            assert_eq!(mode_of(&path), 0o600, "{path:?}");
        }
        let leftovers: Vec<_> = std::fs::read_dir(dir)
            .expect("the data dir reads")
            .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp."))
            .collect();
        assert_eq!(leftovers, Vec::<String>::new(), "no tempfile outlives the rename");

        stop_mcp_server_and_wait().await;
    });
}

#[test]
fn startup_binds_a_free_pinned_port_and_probes_past_a_busy_one() {
    let data_dir = isolate_secrets();
    let app = app_with_stores();
    on_tauri_runtime(async {
        let dir = data_dir.as_ref();
        let free = free_port();
        start_mcp_server(
            app.handle().clone(),
            McpConfig {
                enabled: true,
                port: free,
            },
        )
        .await
        .expect("the server should start");
        assert_eq!(port_on_disk(dir), free);
        stop_mcp_server_and_wait().await;

        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("occupy a port");
        let busy = held.local_addr().expect("bound").port();
        start_mcp_server(
            app.handle().clone(),
            McpConfig {
                enabled: true,
                port: busy,
            },
        )
        .await
        .expect("the probe should find a port");
        let probed = port_on_disk(dir);
        assert!(probed > busy, "it walks upward: {probed} from {busy}");
        assert!(serves_with_token(probed, &token_on_disk(dir)).await);
        stop_mcp_server_and_wait().await;
    });
}

#[test]
fn a_disabled_server_binds_nothing_and_writes_nothing() {
    let data_dir = isolate_secrets();
    let app = app_with_stores();
    on_tauri_runtime(async {
        let dir = data_dir.as_ref();
        start_mcp_server(
            app.handle().clone(),
            McpConfig {
                enabled: false,
                port: 0,
            },
        )
        .await
        .expect("disabled isn't a failure");
        assert!(!dir.join(PORT_FILE_NAME).exists());
        assert!(!dir.join(TOKEN_FILE_NAME).exists());
        assert_eq!(current_mcp_token(), None);
    });
}

#[test]
fn an_interactive_rebind_binds_the_new_port_before_it_retires_the_old_server() {
    let data_dir = isolate_secrets();
    let app = app_with_stores();
    on_tauri_runtime(async {
        let dir = data_dir.as_ref();
        start_mcp_server(app.handle().clone(), McpConfig { enabled: true, port: 0 })
            .await
            .expect("the server should start");
        let first_port = port_on_disk(dir);
        let first_token = token_on_disk(dir);

        // A busy port (`Exact`, so no probing) leaves the running server exactly as it was.
        let held = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("occupy a port");
        let busy = held.local_addr().expect("bound").port();
        let outcome = rebind_interactive(
            app.handle().clone(),
            McpConfig {
                enabled: true,
                port: busy,
            },
        )
        .await
        .expect("a busy port is an outcome, not a failure");
        assert_eq!(outcome, McpServerOutcome::PortInUse { requested: busy });
        assert_eq!(port_on_disk(dir), first_port);
        assert_eq!(token_on_disk(dir), first_token);
        assert!(serves_with_token(first_port, &first_token).await);

        // A free one moves the server, with a token of its own, and the old port goes quiet.
        let target = free_port();
        let outcome = rebind_interactive(
            app.handle().clone(),
            McpConfig {
                enabled: true,
                port: target,
            },
        )
        .await
        .expect("a free port should bind");
        assert_eq!(outcome, McpServerOutcome::Running { port: target });
        assert_eq!(port_on_disk(dir), target);
        let second_token = token_on_disk(dir);
        assert_ne!(second_token, first_token, "a leaked token can't outlive its server");
        assert!(serves_with_token(target, &second_token).await);
        assert!(!serves_with_token(target, &first_token).await);
        wait_until_async(std::time::Duration::from_secs(5), "the old port to close", || {
            refuses_connections(first_port)
        })
        .await;

        // Re-applying the live pin touches nothing, or it would collide with its own socket.
        let outcome = rebind_interactive(
            app.handle().clone(),
            McpConfig {
                enabled: true,
                port: target,
            },
        )
        .await
        .expect("a re-apply should work");
        assert_eq!(outcome, McpServerOutcome::Running { port: target });
        assert_eq!(token_on_disk(dir), second_token);
        assert!(serves_with_token(target, &second_token).await);

        stop_mcp_server_and_wait().await;
    });
}

#[test]
fn the_token_env_var_pins_the_token_across_restarts() {
    let data_dir = isolate_secrets();
    let app = app_with_stores();
    on_tauri_runtime(async {
        let dir = data_dir.as_ref();
        // SAFETY: as in `on_tauri_runtime`: its own process, and the lock it holds serializes the
        // only reader. Nothing else in this test reads the environment concurrently.
        unsafe { std::env::set_var("CMDR_MCP_TOKEN", "  pinned-by-env  ") };
        start_mcp_server(app.handle().clone(), McpConfig { enabled: true, port: 0 })
            .await
            .expect("the server should start");
        assert_eq!(token_on_disk(dir), "pinned-by-env", "trimmed");
        assert_eq!(mode_of(&dir.join(TOKEN_FILE_NAME)), 0o600);
        assert!(serves_with_token(port_on_disk(dir), "pinned-by-env").await);

        rebind_interactive(app.handle().clone(), McpConfig { enabled: true, port: 0 })
            .await
            .expect("a rebind should work");
        assert!(serves_with_token(port_on_disk(dir), "pinned-by-env").await);
        stop_mcp_server_and_wait().await;

        // SAFETY: as above.
        unsafe { std::env::set_var("CMDR_MCP_TOKEN", "   ") };
        start_mcp_server(app.handle().clone(), McpConfig { enabled: true, port: 0 })
            .await
            .expect("the server should start");
        assert_eq!(token_on_disk(dir).len(), 32, "blank means random");
        stop_mcp_server_and_wait().await;

        // SAFETY: as above.
        unsafe { std::env::remove_var("CMDR_MCP_TOKEN") };
    });
}

// ── Origin ──────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn a_foreign_origin_gets_403_a_security_record_and_no_data_even_with_the_token() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    for origin in [
        "https://evil.example",
        "https://localhost.evil.example",
        "https://127.0.0.1.example",
        "http://localhost:3000/path",
        "null",
        "tauri://localhost",
    ] {
        // One episode per origin, so each proves it reached the log by itself.
        let capture = security_log_testing::capture_events();
        let post = server
            .client
            .post(server.url())
            .bearer_auth(TOKEN)
            .header("origin", origin)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
            .send()
            .await
            .expect("the request should complete");
        assert_eq!(post.status(), 403, "POST from {origin}");
        let envelope: Value = post.json().await.expect("the refusal is JSON");
        assert_eq!(envelope["error"]["code"], json!(INVALID_REQUEST), "{origin}");
        assert!(envelope.get("result").is_none(), "{origin}");
        assert_eq!(
            capture.events(),
            vec![SecurityEvent::McpRejectionsStarted {
                reason: McpRejection::ForeignOrigin
            }],
            "{origin}"
        );

        let get = server
            .client
            .get(server.url())
            .bearer_auth(TOKEN)
            .header("origin", origin)
            .send()
            .await
            .expect("the request should complete");
        assert_eq!(get.status(), 403, "GET from {origin}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn the_origin_check_runs_before_the_token_check() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let capture = security_log_testing::capture_events();
    let response = server
        .client
        .post(server.url())
        .header("origin", "https://evil.example")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(response.status(), 403, "the Origin refusal, not the in-band token one");
    assert_eq!(
        capture.events(),
        vec![SecurityEvent::McpRejectionsStarted {
            reason: McpRejection::ForeignOrigin
        }]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_loopback_origin_or_none_at_all_is_allowed() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let ping = json!({"jsonrpc":"2.0","id":1,"method":"ping"});

    for origin in [
        "http://localhost:4312",
        "http://localhost",
        "http://127.0.0.1:19224",
        "https://[::1]:8443",
    ] {
        let response = server
            .client
            .post(server.url())
            .bearer_auth(TOKEN)
            .header("origin", origin)
            .json(&ping)
            .send()
            .await
            .expect("the request should complete");
        assert_eq!(response.status(), 200, "{origin}");
        let envelope: Value = response.json().await.expect("the reply is JSON");
        assert!(envelope.get("result").is_some(), "{origin}");
    }

    assert!(
        server.call(ping).await.get("result").is_some(),
        "no Origin: a native client"
    );
}

// ── The bearer token ────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn every_method_needs_the_token_and_a_rejection_echoes_the_id_at_http_200() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    for (id, method, params) in [
        (json!(11), "initialize", json!({"protocolVersion":"2025-11-25"})),
        (json!(12), "tools/list", json!({})),
        (json!(13), "resources/list", json!({})),
        (json!(14), "resources/read", json!({"uri":"cmdr://state"})),
        (json!(15), "tools/call", json!({"name":"quit","arguments":{}})),
        (json!(16), "ping", json!({})),
        (json!(17), "no/such/method", json!({})),
        (json!("a-string-id"), "ping", json!({})),
    ] {
        let response = server
            .client
            .post(server.url())
            .json(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .send()
            .await
            .expect("the request should complete");
        // A 401 would send a client into OAuth discovery.
        assert_eq!(response.status(), 200, "{method}");
        assert!(!response.headers().contains_key("www-authenticate"), "{method}");
        let envelope: Value = response.json().await.expect("the rejection is JSON");
        assert_eq!(envelope["id"], id, "{method}");
        assert_eq!(envelope["jsonrpc"], json!("2.0"), "{method}");
        assert_eq!(envelope["error"]["code"], json!(INVALID_PARAMS), "{method}");
        assert!(envelope.get("result").is_none(), "{method}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_missing_and_every_kind_of_wrong_token_get_the_same_bytes() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let answer = async |authorization: Option<String>| {
        let mut request = server
            .client
            .post(server.url())
            .json(&json!({"jsonrpc":"2.0","id":19,"method":"tools/list"}));
        if let Some(value) = authorization {
            request = request.header("authorization", value);
        }
        let response = request.send().await.expect("the request should complete");
        (response.status(), response.bytes().await.expect("the body should read"))
    };

    let missing = answer(None).await;
    assert_eq!(missing.0, 200);
    // allowed-error-string-match: this checks that a secret is absent, not error state or control flow
    assert!(!String::from_utf8_lossy(&missing.1).contains(TOKEN));

    // The constant-time compare has no timing to pin from here, so pin what it must not leak: a
    // near miss of every shape answers exactly like no token at all.
    let same_length_wrong = "x".repeat(TOKEN.len());
    let almost: String = TOKEN.chars().take(TOKEN.len() - 1).chain(['x']).collect();
    let prefix = &TOKEN[..TOKEN.len() - 1];
    let longer = format!("{TOKEN}x");
    for authorization in [
        format!("Bearer {same_length_wrong}"),
        format!("Bearer {almost}"),
        format!("Bearer {prefix}"),
        format!("Bearer {longer}"),
        "Bearer ".to_string(),
        format!("bearer {TOKEN}"),
        format!("Basic {TOKEN}"),
        TOKEN.to_string(),
    ] {
        assert_eq!(answer(Some(authorization.clone())).await, missing, "{authorization}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn no_configured_token_fails_closed_even_for_an_empty_bearer() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    set_mcp_token(None);

    for authorization in [format!("Bearer {TOKEN}"), "Bearer ".to_string()] {
        let response = server
            .client
            .post(server.url())
            .header("authorization", authorization)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
            .send()
            .await
            .expect("the request should complete");
        let envelope: Value = response.json().await.expect("the rejection is JSON");
        assert_eq!(envelope["error"]["code"], json!(INVALID_PARAMS));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_token_rejection_reaches_the_security_log_as_one_episode_per_reason() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let capture = security_log_testing::capture_events();
    let ping = json!({"jsonrpc":"2.0","id":1,"method":"ping"});

    for _ in 0..3 {
        server
            .client
            .post(server.url())
            .bearer_auth("stale-token")
            .json(&ping)
            .send()
            .await
            .expect("the request should complete");
    }
    server
        .client
        .post(server.url())
        .json(&ping)
        .send()
        .await
        .expect("the request should complete");

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

#[tokio::test(flavor = "current_thread")]
async fn a_notification_or_get_without_the_token_gets_an_empty_403() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    for body in [
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        // No id makes it a notification whatever the method says, and the method's prefix makes it
        // one whatever the id says.
        json!({"jsonrpc":"2.0","method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":1,"method":"notifications/initialized"}),
    ] {
        let response = server
            .client
            .post(server.url())
            .json(&body)
            .send()
            .await
            .expect("the request should complete");
        assert_eq!(response.status(), 403, "{body}");
        assert!(response.bytes().await.expect("the body reads").is_empty(), "{body}");
    }

    let get = server.client.get(server.url()).send().await.expect("the GET completes");
    assert_eq!(get.status(), 403);
    assert!(get.bytes().await.expect("the body reads").is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn health_stays_open_and_says_nothing_else() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let health = server
        .client
        .get(format!("{}/health", server.url()))
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(health.status(), 200);
    assert_eq!(
        health.json::<Value>().await.expect("health is JSON"),
        json!({"status":"ok"})
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_preflight_stays_open_and_data_free_for_loopback_only() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let preflight = async |origin: &'static str| {
        server
            .client
            .request(reqwest::Method::OPTIONS, server.url())
            .header("origin", origin)
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", "authorization,content-type")
            .send()
            .await
            .expect("the preflight should complete")
    };

    let loopback = preflight("http://localhost:4312").await;
    assert!(loopback.status().is_success());
    assert_eq!(
        loopback
            .headers()
            .get("access-control-allow-origin")
            .map(|v| v.as_bytes()),
        Some("http://localhost:4312".as_bytes())
    );
    let allowed = loopback
        .headers()
        .get("access-control-allow-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    for needed in ["authorization", "content-type", "mcp-protocol-version"] {
        assert!(
            allowed.split(',').any(|name| name.trim() == needed),
            "{needed} in {allowed}"
        );
    }
    assert!(loopback.bytes().await.expect("the body reads").is_empty());

    let foreign = preflight("https://evil.example").await;
    assert!(!foreign.headers().contains_key("access-control-allow-origin"));
    assert!(foreign.bytes().await.expect("the body reads").is_empty());
}

#[test]
fn neither_the_token_nor_a_tool_argument_reaches_any_log_at_trace() {
    let _token_guard = TOKEN_TEST_LOCK.blocking_lock();
    const PASSWORD: &str = "correct-horse-battery-staple";
    let app = app_with_stores();
    // One thread, so `capture_logs` sees the server's records as well as the test's.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a test runtime builds");

    let ((), records) = capture_logs(|| {
        runtime.block_on(async {
            let server = TestServer::start(&app).await;
            server
                .tool("unlock", json!({"path":"/synthetic/a.zip","password":PASSWORD}))
                .await;
            server.tool("no_such_tool", json!({"password":PASSWORD})).await;
            server.call(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).await;
            server
                .client
                .get(server.url())
                .bearer_auth(TOKEN)
                .send()
                .await
                .expect("the GET completes");
            server
                .client
                .post(server.url())
                .bearer_auth("wrong-token-also-private")
                .json(&json!({"jsonrpc":"2.0","id":3,"method":"ping"}))
                .send()
                .await
                .expect("the POST completes");
        });
    });

    // The header dump is what would carry the token, so a capture without one proves nothing.
    assert!(
        // allowed-error-string-match: reads a log line this test expects, not error state
        records.iter().any(|record| record.message.contains("<redacted>")),
        "the header dump should have logged"
    );
    for record in &records {
        // allowed-error-string-match: this checks that a secret is absent, not error state or control flow
        let leaked = [TOKEN, PASSWORD, "wrong-token-also-private"]
            .iter()
            .any(|secret| record.message.contains(secret));
        assert!(!leaked, "{}: {}", record.target, record.message);
    }
}

// ── Dispatch ────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn a_tool_errors_typed_data_reaches_the_client() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let policy = override_for_test(forcing(&[DISABLE_USAGE_STATS]));
    let managed = server
        .tool("set_setting", json!({"id":"analytics.enabled","value":true}))
        .await;
    assert_eq!(managed["error"]["code"], json!(INVALID_PARAMS));
    assert_eq!(managed["error"]["data"], json!({"reason":"managedByOrganization"}));
    assert!(managed.get("result").is_none());
    drop(policy);

    app.state::<SoftDialogTracker>().open("transfer-progress".to_string());
    let blocked = server.tool("copy", json!({"pane":"left"})).await;
    assert_eq!(blocked["error"]["code"], json!(INVALID_PARAMS));
    assert_eq!(blocked["error"]["data"], json!({"blockingDialog":"transfer-progress"}));
}

#[tokio::test(flavor = "current_thread")]
async fn a_dev_only_tool_refuses_off_direct_mode_and_the_setting_toggles_it_live() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let unset_after = isolate_secrets();

    for name in ["resolve_conflict", "connect_to_server", "ai_search", "dialog_confirm"] {
        let refused = server.tool(name, json!({})).await;
        assert_eq!(refused["error"]["code"], json!(INVALID_PARAMS), "{name}");
        assert_eq!(
            refused["error"]["data"],
            json!({"reason":"directModeOff","setting":"developer.mcpDirectMode"}),
            "{name}"
        );
    }

    crate::tools::policy::set_direct_mode(true);
    let past_the_gate = server.tool("resolve_conflict", json!({})).await;
    assert_ne!(
        past_the_gate["error"]["data"]["reason"],
        json!("directModeOff"),
        "{past_the_gate}"
    );

    crate::tools::policy::set_direct_mode(false);
    let refused_again = server.tool("resolve_conflict", json!({})).await;
    assert_eq!(refused_again["error"]["data"]["reason"], json!("directModeOff"));
    drop(unset_after);
}

#[tokio::test(flavor = "current_thread")]
async fn no_reply_shows_a_twins_nul_pick() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let twin = format!("/DCIM/{}/IMG_1.JPG", cmdr_fs::twin_path::pick_segment(4242));
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let store = app.state::<PaneStateStore>();
    store.set_focused_pane("left".to_string());
    store.set_left(PaneState {
        path: twin.clone(),
        ..Default::default()
    });

    // In a result's text, and in a resource's nested text.
    let result = server
        .tool(
            "await",
            json!({"pane":"left","condition":"path","value":twin,"timeout":1}),
        )
        .await;
    let text = result["result"]["content"][0]["text"].as_str().expect("a text result");
    // allowed-error-string-match: reads a path out of a result, not error state
    assert!(text.contains("/DCIM/IMG_1.JPG"), "{text}");
    let resource = server
        .call(json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"cmdr://state"}}))
        .await;
    let text = resource["result"]["contents"][0]["text"]
        .as_str()
        .expect("a text resource");
    // allowed-error-string-match: reads a path out of a result, not error state
    assert!(text.contains("/DCIM/IMG_1.JPG"), "{text}");

    // In a tool's JSON result, where rendering it to text first would write the NUL as the six
    // characters `\u0000`, which nothing could strip afterward.
    let listed = server.tool("list_dir", json!({"path":twin})).await;
    let text = listed["result"]["content"][0]["text"].as_str().expect("a text result");
    let location: Value = serde_json::from_str(text).expect("a JSON result is JSON text");
    assert_eq!(location["location"]["path"], json!("/DCIM/IMG_1.JPG"));

    // In an error's message and its data. The refusal repeats the open dialog's id verbatim, which
    // makes it the one route to both from out here.
    app.state::<SoftDialogTracker>().open(twin);
    let refused = server.tool("copy", json!({"pane":"left"})).await;
    let message = refused["error"]["message"].as_str().expect("an error message");
    // allowed-error-string-match: reads a path out of the message; the code is what's typed
    assert!(message.contains("/DCIM/IMG_1.JPG"), "{message}");
    assert_eq!(refused["error"]["data"]["blockingDialog"], json!("/DCIM/IMG_1.JPG"));
}

#[tokio::test(flavor = "current_thread")]
async fn tools_list_comes_back_in_the_table_order_every_time() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let list = json!({"jsonrpc":"2.0","id":1,"method":"tools/list"});
    let first = server.call(list.clone()).await;
    let second = server.call(list).await;
    assert_eq!(first.to_string(), second.to_string());

    let names = |tools: &Value| -> Vec<String> {
        tools
            .as_array()
            .expect("a tool array")
            .iter()
            .map(|tool| tool["name"].as_str().expect("a tool name").to_string())
            .collect()
    };
    let snapshot: Value = serde_json::from_str(include_str!("../../tools/tests/fixtures/tools_list_snapshot.json"))
        .expect("the snapshot is JSON");
    assert_eq!(names(&first["result"]["tools"]), names(&snapshot["tools"]));
}

#[tokio::test(flavor = "current_thread")]
async fn a_call_with_no_initialize_and_no_accept_header_works() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let authorization = format!("Bearer {TOKEN}");
    let body = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"image_facts","arguments":{"paths":["/synthetic/a.png"]}}})
        .to_string();

    // What `apps/desktop/test/e2e-shared/mcp-client.ts` writes, and the `*/*` that `fetch` adds to it.
    for accept in [None, Some("*/*")] {
        let mut headers = vec![
            ("Host", "127.0.0.1"),
            ("Content-Type", "application/json"),
            ("Authorization", authorization.as_str()),
        ];
        headers.extend(accept.map(|value| ("Accept", value)));
        let response = server.raw_post(&headers, &body).await;
        assert_eq!(response.status, 200, "Accept: {accept:?}");
        let envelope: Value = serde_json::from_str(&response.body).expect("the reply is plain JSON");
        assert_eq!(envelope["id"], json!(7), "Accept: {accept:?}");
        assert!(envelope.get("result").is_some(), "Accept: {accept:?}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_tool_result_is_a_text_content_block() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let reply = server.tool("image_facts", json!({"paths":["/synthetic/a.png"]})).await;
    assert_eq!(reply["id"], json!(1));
    assert!(reply.get("error").is_none());
    assert_eq!(reply["result"]["content"][0]["type"], json!("text"));
    assert!(reply["result"]["content"][0]["text"].is_string());
}

#[tokio::test(flavor = "current_thread")]
async fn resources_list_and_read_and_ping_answer_and_an_unknown_method_is_not_found() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let listed = server
        .call(json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}))
        .await;
    let uris: Vec<&str> = listed["result"]["resources"]
        .as_array()
        .expect("a resource array")
        .iter()
        .map(|resource| resource["uri"].as_str().expect("a uri"))
        .collect();
    assert_eq!(uris.first(), Some(&"cmdr://state"));
    assert!(uris.contains(&"cmdr://dialogs/available"), "{uris:?}");

    let read = server
        .call(json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"cmdr://dialogs/available"}}))
        .await;
    assert_eq!(read["id"], json!(2));
    assert_eq!(read["result"]["contents"][0]["uri"], json!("cmdr://dialogs/available"));
    assert!(read["result"]["contents"][0]["text"].is_string());

    let unknown_uri = server
        .call(json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"cmdr://no-such-thing"}}))
        .await;
    assert_eq!(unknown_uri["error"]["code"], json!(INVALID_PARAMS));

    let ping = server.call(json!({"jsonrpc":"2.0","id":4,"method":"ping"})).await;
    assert_eq!(ping, json!({"jsonrpc":"2.0","id":4,"result":{}}));

    let unknown = server
        .call(json!({"jsonrpc":"2.0","id":5,"method":"no/such/method"}))
        .await;
    assert_eq!(unknown["id"], json!(5));
    assert_eq!(unknown["error"]["code"], json!(METHOD_NOT_FOUND));
}

#[tokio::test(flavor = "current_thread")]
async fn an_authenticated_notification_gets_an_empty_202() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let response = server
        .client
        .post(server.url())
        .bearer_auth(TOKEN)
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(response.status(), 202);
    assert!(response.bytes().await.expect("the body reads").is_empty());
}

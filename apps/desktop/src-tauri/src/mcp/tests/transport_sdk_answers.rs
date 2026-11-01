//! What the official SDK answers on its own, seen from outside: statelessness, the plain-text
//! statuses, the 2026-07-28 revision, and a client that leaves mid-call. An `rmcp` upgrade may
//! legitimately move these, so read each diff; `transport_characterization.rs`, this file's parent,
//! holds what must survive any upgrade.

use serde_json::{Value, json};
use tauri::Manager;

use super::{INVALID_PARAMS, METHOD_NOT_FOUND, TOKEN, TestServer, app_with_stores};
use crate::mcp::auth::TOKEN_TEST_LOCK;
use crate::test_support::wait_until_async;
use crate::ui_state::dialog_state::SoftDialogTracker;
use crate::ui_state::pane_state::{PaneState, PaneStateStore};

/// A plain-text status from the SDK, where a client gets no JSON-RPC envelope to read.
async fn assert_plain_text_status(response: reqwest::Response, status: u16, context: &str) {
    assert_eq!(response.status(), status, "{context}");
    let body = response.text().await.expect("the body reads");
    assert!(serde_json::from_str::<Value>(&body).is_err(), "{context}: {body}");
}

#[tokio::test(flavor = "current_thread")]
async fn initialize_is_stateless_and_reports_who_it_is_and_the_version_it_really_speaks() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    // Every revision that has a handshake is answered in kind, so an old client keeps working.
    for version in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
        let response = server
            .client
            .post(server.url())
            .bearer_auth(TOKEN)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":version,
                "capabilities":{},
                "clientInfo":{"name":"characterization","version":"1"},
            }}))
            .send()
            .await
            .expect("the request should complete");
        assert_eq!(response.status(), 200, "{version}");
        assert!(!response.headers().contains_key("mcp-session-id"), "{version}");
        let envelope: Value = response.json().await.expect("initialize is JSON");
        assert_eq!(envelope["id"], json!(1), "{version}");
        // Stays `cmdr`, and only because the handler says so: rmcp's default is `rmcp`.
        assert_eq!(envelope["result"]["serverInfo"]["name"], json!("cmdr"), "{version}");
        assert_eq!(
            envelope["result"]["capabilities"]["tools"]["listChanged"],
            json!(false),
            "{version}"
        );
        assert!(envelope["result"]["instructions"].is_string(), "{version}");
        assert_eq!(envelope["result"]["protocolVersion"], json!(version));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn an_authenticated_get_is_405_there_being_no_stream_to_offer() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let response = server
        .client
        .get(server.url())
        .bearer_auth(TOKEN)
        .header("accept", "text/event-stream")
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(
        response.headers().get("allow").map(|v| v.as_bytes()),
        Some("POST".as_bytes())
    );
    assert_plain_text_status(response, 405, "GET").await;
}

#[tokio::test(flavor = "current_thread")]
async fn accepting_sse_still_gets_a_plain_json_reply() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let response = server
        .client
        .post(server.url())
        .bearer_auth(TOKEN)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").map(|v| v.as_bytes()),
        Some("application/json".as_bytes())
    );
    assert_eq!(
        response.json::<Value>().await.expect("the reply is JSON"),
        json!({"jsonrpc":"2.0","id":1,"result":{}})
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_foreign_host_gets_403_and_an_unknown_protocol_version_gets_400() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let authorization = format!("Bearer {TOKEN}");
    let body = json!({"jsonrpc":"2.0","id":1,"method":"ping"}).to_string();
    let common = [
        ("Content-Type", "application/json"),
        ("Authorization", authorization.as_str()),
    ];

    let foreign_host = server
        .raw_post(&[&common[..], &[("Host", "evil.example")]].concat(), &body)
        .await;
    assert_eq!(foreign_host.status, 403);

    // The loopback names pass, whatever the port, which is what a client's URL holds.
    for host in ["127.0.0.1:19225", "localhost:19225", "[::1]:19225", "127.0.0.1"] {
        let allowed = server.raw_post(&[&common[..], &[("Host", host)]].concat(), &body).await;
        assert_eq!(allowed.status, 200, "{host}");
    }

    let unknown_version = server
        .raw_post(
            &[
                &common[..],
                &[("Host", "127.0.0.1"), ("MCP-Protocol-Version", "1999-01-01")],
            ]
            .concat(),
            &body,
        )
        .await;
    assert_eq!(unknown_version.status, 400);
}

/// The SDK reads a `tools/call` it can't parse as a method it doesn't know.
#[tokio::test(flavor = "current_thread")]
async fn a_call_missing_what_it_needs_is_method_not_found() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    for (body, method) in [
        (
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"arguments":{}}}),
            "tools/call",
        ),
        (
            json!({"jsonrpc":"2.0","id":1,"method":"resources/read","params":{}}),
            "resources/read",
        ),
        (
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"image_facts","arguments":3}}),
            "tools/call",
        ),
        // Every real client sends both of the fields this leaves out.
        (
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}),
            "initialize",
        ),
    ] {
        let reply = server.call(body.clone()).await;
        assert_eq!(reply["id"], json!(1), "{body}");
        assert_eq!(reply["error"]["code"], json!(METHOD_NOT_FOUND), "{body}");
        assert_eq!(reply["error"]["message"], json!(method), "{body}");
    }

    // The 2026-07-28 path makes the same answer an HTTP 404.
    let on_the_latest_revision = latest_revision_request(&server, "tools/call", None, json!({"arguments":{}}))
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(on_the_latest_revision.status(), 404);
}

#[tokio::test(flavor = "current_thread")]
async fn a_successful_tool_reply_also_says_it_is_not_an_error() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let reply = server.tool("image_facts", json!({"paths":["/synthetic/a.png"]})).await;
    assert_eq!(reply["result"]["isError"], json!(false));
}

#[tokio::test(flavor = "current_thread")]
async fn a_malformed_envelope_is_a_plain_415_with_the_token_and_refused_without() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let authorization = format!("Bearer {TOKEN}");
    let without_token = [("Host", "127.0.0.1"), ("Content-Type", "application/json")];
    let with_token = [
        ("Host", "127.0.0.1"),
        ("Content-Type", "application/json"),
        ("Authorization", authorization.as_str()),
    ];

    // Nothing reaches the SDK past a bad token, not even to be told it's malformed. With no id to
    // read, this is the guard's empty 403.
    let malformed = server.raw_post(&without_token, "{not json").await;
    assert_eq!(malformed.status, 403);
    assert_eq!(malformed.body, "");
    let malformed = server.raw_post(&with_token, "{not json").await;
    assert_eq!(malformed.status, 415);
    assert!(serde_json::from_str::<Value>(&malformed.body).is_err(), "plain text");

    // An id is enough for the in-band rejection, which resolves the caller's pending call.
    let no_method = r#"{"jsonrpc":"2.0","id":1}"#;
    let refused = server.raw_post(&without_token, no_method).await;
    assert_eq!(refused.status, 200);
    let envelope: Value = serde_json::from_str(&refused.body).expect("the rejection is JSON");
    assert_eq!(envelope["id"], json!(1));
    assert_eq!(envelope["error"]["code"], json!(INVALID_PARAMS));
    let no_method = server.raw_post(&with_token, no_method).await;
    assert_eq!(no_method.status, 415);
    assert!(serde_json::from_str::<Value>(&no_method.body).is_err(), "plain text");
}

#[tokio::test(flavor = "current_thread")]
async fn a_wrong_jsonrpc_version_is_a_plain_415_with_the_token_and_in_band_without() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let body = json!({"jsonrpc":"1.0","id":1,"method":"ping"});

    let with_token = server
        .client
        .post(server.url())
        .bearer_auth(TOKEN)
        .json(&body)
        .send()
        .await
        .expect("the request should complete");
    assert_plain_text_status(with_token, 415, "with the token").await;

    // Auth still comes first, and the rejection still echoes the id: the guard reads `id` and
    // `method` off the raw body.
    let without_token = server
        .client
        .post(server.url())
        .json(&body)
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(without_token.status(), 200);
    let envelope: Value = without_token.json().await.expect("the rejection is JSON");
    assert_eq!(envelope["error"]["code"], json!(INVALID_PARAMS));
    assert_eq!(envelope["id"], json!(1));
}

#[tokio::test(flavor = "current_thread")]
async fn a_null_or_float_id_is_a_plain_415() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    for id in [json!(null), json!(1.5)] {
        let response = server
            .client
            .post(server.url())
            .bearer_auth(TOKEN)
            .json(&json!({"jsonrpc":"2.0","id":id,"method":"ping"}))
            .send()
            .await
            .expect("the request should complete");
        assert_plain_text_status(response, 415, &id.to_string()).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_put_or_delete_needs_the_token_too_and_then_gets_405() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    for method in [reqwest::Method::PUT, reqwest::Method::DELETE] {
        for authorization in [None, Some("Bearer wrong-token")] {
            let mut request = server.client.request(method.clone(), server.url());
            if let Some(value) = authorization {
                request = request.header("authorization", value);
            }
            let response = request.send().await.expect("the request should complete");
            assert_eq!(response.status(), 403, "{method} with {authorization:?}");
            assert!(
                response.bytes().await.expect("the body reads").is_empty(),
                "{method} with {authorization:?}"
            );
        }

        let response = server
            .client
            .request(method.clone(), server.url())
            .bearer_auth(TOKEN)
            .send()
            .await
            .expect("the request should complete");
        assert_plain_text_status(response, 405, method.as_str()).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn tools_list_on_the_wire_is_the_snapshot_to_the_key() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let listed = server.call(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})).await;
    let snapshot: Value = serde_json::from_str(include_str!("../../tools/tests/fixtures/tools_list_snapshot.json"))
        .expect("the snapshot is JSON");
    assert_eq!(listed["result"]["tools"], snapshot["tools"]);
}

/// `resultType` belongs to 2026-07-28. An older client's reply has no such key.
#[tokio::test(flavor = "current_thread")]
async fn an_older_client_sees_no_result_type() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    for version in [None, Some("2025-03-26"), Some("2025-11-25")] {
        for body in [
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"image_facts","arguments":{"paths":["/synthetic/a.png"]}}}),
            json!({"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"cmdr://dialogs/available"}}),
        ] {
            let mut request = server.client.post(server.url()).bearer_auth(TOKEN).json(&body);
            if let Some(version) = version {
                request = request.header("mcp-protocol-version", version);
            }
            let reply: Value = request
                .send()
                .await
                .expect("the request should complete")
                .json()
                .await
                .expect("the reply is JSON");
            assert!(reply["result"].is_object(), "{version:?} {body}");
            assert!(reply["result"].get("resultType").is_none(), "{version:?} {body}");
        }
    }
}

// ── The 2026-07-28 revision ─────────────────────────────────────────────────

const LATEST_REVISION: &str = "2026-07-28";

/// A 2026-07-28 request: its version and capabilities in `_meta`, and its method and name in headers
/// too, so a middlebox can route on them.
fn latest_revision_request(
    server: &TestServer,
    method: &str,
    name: Option<&str>,
    params: Value,
) -> reqwest::RequestBuilder {
    let mut params = params.as_object().cloned().unwrap_or_default();
    params.insert(
        "_meta".to_string(),
        json!({
            "io.modelcontextprotocol/protocolVersion": LATEST_REVISION,
            "io.modelcontextprotocol/clientCapabilities": {},
        }),
    );
    let mut request = server
        .client
        .post(server.url())
        .bearer_auth(TOKEN)
        .header("mcp-protocol-version", LATEST_REVISION)
        .header("mcp-method", method)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}));
    if let Some(name) = name {
        request = request.header("mcp-name", name);
    }
    request
}

#[tokio::test(flavor = "current_thread")]
async fn a_latest_revision_client_can_list_call_and_read() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let listed = latest_revision_request(&server, "tools/list", None, json!({}))
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(listed.status(), 200);
    let listed: Value = listed.json().await.expect("the reply is JSON");
    assert!(
        listed["result"]["tools"]
            .as_array()
            .is_some_and(|tools| !tools.is_empty())
    );

    let called = latest_revision_request(
        &server,
        "tools/call",
        Some("image_facts"),
        json!({"name":"image_facts","arguments":{"paths":["/synthetic/a.png"]}}),
    )
    .send()
    .await
    .expect("the request should complete");
    assert_eq!(called.status(), 200);
    let called: Value = called.json().await.expect("the reply is JSON");
    assert_eq!(called["result"]["resultType"], json!("complete"));
    assert_eq!(called["result"]["content"][0]["type"], json!("text"));

    let read = latest_revision_request(
        &server,
        "resources/read",
        Some("cmdr://dialogs/available"),
        json!({"uri":"cmdr://dialogs/available"}),
    )
    .send()
    .await
    .expect("the request should complete");
    assert_eq!(read.status(), 200);
    let read: Value = read.json().await.expect("the reply is JSON");
    assert!(read["result"]["contents"][0]["text"].is_string());
}

#[tokio::test(flavor = "current_thread")]
async fn a_tool_refusal_on_the_latest_revision_stays_in_band_with_its_typed_data() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    app.state::<SoftDialogTracker>().open("transfer-progress".to_string());
    let refused = latest_revision_request(
        &server,
        "tools/call",
        Some("copy"),
        json!({"name":"copy","arguments":{"pane":"left"}}),
    )
    .send()
    .await
    .expect("the request should complete");
    assert_eq!(refused.status(), 200);
    let refused: Value = refused.json().await.expect("the reply is JSON");
    assert_eq!(refused["error"]["code"], json!(INVALID_PARAMS));
    assert_eq!(refused["error"]["data"], json!({"blockingDialog":"transfer-progress"}));
}

#[tokio::test(flavor = "current_thread")]
async fn server_discover_names_cmdr_and_its_revisions() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let response = latest_revision_request(&server, "server/discover", None, json!({}))
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(response.status(), 200);
    let reply: Value = response.json().await.expect("the reply is JSON");
    let versions = reply["result"]["supportedVersions"].as_array().expect("a version list");
    assert!(versions.contains(&json!(LATEST_REVISION)), "{versions:?}");
    assert!(versions.contains(&json!("2025-11-25")), "{versions:?}");
    assert!(reply["result"]["capabilities"]["tools"].is_object(), "{reply}");
    assert!(reply["result"]["instructions"].is_string(), "{reply}");
    // allowed-error-string-match: reads the server's name out of a result, not error state
    assert!(reply["result"].to_string().contains(r#""name":"cmdr""#), "{reply}");
}

#[tokio::test(flavor = "current_thread")]
async fn a_mismatched_mcp_method_header_gets_400() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let response = latest_revision_request(&server, "tools/list", None, json!({}))
        .header("mcp-method", "tools/call")
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(response.status(), 400);
    let reply: Value = response.json().await.expect("the refusal is JSON");
    assert_eq!(reply["id"], json!(1));
    assert!(reply["error"]["code"].is_i64());
}

#[tokio::test(flavor = "current_thread")]
async fn the_token_rejection_is_the_same_in_band_error_on_the_latest_revision() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;

    let response = server
        .client
        .post(server.url())
        .header("mcp-protocol-version", LATEST_REVISION)
        .header("mcp-method", "tools/list")
        .json(&json!({"jsonrpc":"2.0","id":9,"method":"tools/list","params":{"_meta":{
            "io.modelcontextprotocol/protocolVersion": LATEST_REVISION,
            "io.modelcontextprotocol/clientCapabilities": {},
        }}}))
        .send()
        .await
        .expect("the request should complete");
    assert_eq!(response.status(), 200);
    let reply: Value = response.json().await.expect("the rejection is JSON");
    assert_eq!(reply["id"], json!(9));
    assert_eq!(reply["error"]["code"], json!(INVALID_PARAMS));
}

// ── A client that leaves ────────────────────────────────────────────────────

/// `rmcp` runs a handler in a task of its own, so an abandoned `await` would otherwise run on for its
/// whole timeout with nobody left to read the answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_leaving_mid_call_stops_the_handler() {
    use crate::mcp::handler::tests::tool_calls_running;
    use std::time::Duration;

    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = app_with_stores();
    let server = TestServer::start(&app).await;
    let store = app.state::<PaneStateStore>();
    store.set_focused_pane("left".to_string());
    store.set_left(PaneState {
        path: "/here".to_string(),
        ..Default::default()
    });

    let call = server
        .client
        .post(server.url())
        .bearer_auth(TOKEN)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
            "name":"await",
            "arguments":{"pane":"left","condition":"path","value":"/never","timeout":60},
        }}))
        .send();
    let client = tokio::spawn(call);
    wait_until_async(Duration::from_secs(5), "the call to start", || {
        tool_calls_running() == 1
    })
    .await;

    client.abort();
    wait_until_async(Duration::from_secs(5), "the handler to stop", || {
        tool_calls_running() == 0
    })
    .await;
}

use super::*;
use axum::http::HeaderValue;

use crate::mcp::auth::TOKEN_TEST_LOCK;

#[tokio::test(flavor = "current_thread")]
async fn http_surface_requires_auth_before_dispatch_or_session_mutation() {
    let _token_guard = TOKEN_TEST_LOCK.lock().await;
    let app = tauri::test::mock_app();
    let state = Arc::new(McpState::new(app.handle().clone()));
    let inspect_state = state.clone();
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("test server should bind");
    let addr = listener.local_addr().expect("bound listener should have an address");
    let server = tokio::spawn(async move {
        axum::serve(listener, build_router(state))
            .await
            .expect("test server should serve");
    });
    let client = cmdr_http::client_builder().build().expect("a plain client builds");
    let mcp_url = format!("http://{addr}/mcp");
    let health_url = format!("http://{addr}/mcp/health");
    set_mcp_token(Some("transport-test-token".to_string()));

    let health = client
        .get(&health_url)
        .send()
        .await
        .expect("health request should complete");
    assert_eq!(health.status(), StatusCode::OK);
    assert_eq!(
        health.json::<Value>().await.expect("health should be JSON"),
        json!({"status": "ok"})
    );

    let request_cases = [
        json!({"jsonrpc":"2.0","id":11,"method":"initialize","params":{"protocolVersion":PROTOCOL_VERSION}}),
        json!({"jsonrpc":"2.0","id":12,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":13,"method":"resources/list"}),
        json!({"jsonrpc":"2.0","id":14,"method":"resources/read","params":{"uri":"cmdr://state"}}),
        json!({"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"image_facts","arguments":{"paths":["/synthetic/private-scan.png"]}}}),
        json!({"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"quit","arguments":{}}}),
    ];
    for body in request_cases {
        let expected_id = body["id"].clone();
        let response = client
            .post(&mcp_url)
            .json(&body)
            .send()
            .await
            .expect("request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let envelope = response.json::<Value>().await.expect("rejection should be JSON");
        assert_eq!(envelope["id"], expected_id);
        assert!(envelope.get("error").is_some());
        assert_eq!(envelope["error"]["code"], json!(INVALID_PARAMS));
        assert!(envelope.get("result").is_none());
        // allowed-error-string-match: this checks that a secret is absent, not error state or control flow
        assert!(!envelope.to_string().contains("transport-test-token"));
    }
    assert!(inspect_state.session_id.read().unwrap().is_none());

    let wrong = client
        .post(&mcp_url)
        .header(header::AUTHORIZATION, "Bearer wrong-token")
        .json(&json!({"jsonrpc":"2.0","id":17,"method":"tools/list"}))
        .send()
        .await
        .expect("wrong-token request should complete");
    assert_eq!(wrong.status(), StatusCode::OK);
    assert!(
        wrong
            .json::<Value>()
            .await
            .expect("rejection should be JSON")
            .get("error")
            .is_some()
    );

    let notification = client
        .post(&mcp_url)
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .expect("notification should complete");
    assert_eq!(notification.status(), StatusCode::FORBIDDEN);
    assert!(notification.bytes().await.expect("body should read").is_empty());

    let sse = client.get(&mcp_url).send().await.expect("SSE request should complete");
    assert_eq!(sse.status(), StatusCode::FORBIDDEN);

    let preflight = client
        .request(Method::OPTIONS, &mcp_url)
        .header(header::ORIGIN, "http://localhost:4312")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization,content-type")
        .send()
        .await
        .expect("preflight should complete");
    assert!(preflight.status().is_success());
    let actual_after_preflight = client
        .post(&mcp_url)
        .header(header::ORIGIN, "http://localhost:4312")
        .json(&json!({"jsonrpc":"2.0","id":18,"method":"tools/list"}))
        .send()
        .await
        .expect("actual browser request should complete");
    assert!(
        actual_after_preflight
            .json::<Value>()
            .await
            .expect("rejection should be JSON")
            .get("error")
            .is_some()
    );

    let authenticated_initialize = client
        .post(&mcp_url)
        .header(header::AUTHORIZATION, "Bearer transport-test-token")
        .json(&json!({"jsonrpc":"2.0","id":21,"method":"initialize","params":{"protocolVersion":PROTOCOL_VERSION}}))
        .send()
        .await
        .expect("authenticated initialize should complete");
    let initialized = authenticated_initialize
        .json::<Value>()
        .await
        .expect("initialize should be JSON");
    assert_eq!(initialized["id"], json!(21));
    assert!(initialized.get("result").is_some());

    let authenticated_sse = client
        .get(&mcp_url)
        .header(header::AUTHORIZATION, "Bearer transport-test-token")
        .send()
        .await
        .expect("authenticated SSE request should complete");
    assert_eq!(authenticated_sse.status(), StatusCode::OK);

    for body in [
        json!({"jsonrpc":"2.0","id":22,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":23,"method":"resources/list"}),
        json!({"jsonrpc":"2.0","id":24,"method":"resources/read","params":{"uri":"cmdr://dialogs/available"}}),
        json!({"jsonrpc":"2.0","id":25,"method":"tools/call","params":{"name":"image_facts","arguments":{"paths":["/synthetic/private-scan.png"]}}}),
    ] {
        let response = client
            .post(&mcp_url)
            .header(header::AUTHORIZATION, "Bearer transport-test-token")
            .json(&body)
            .send()
            .await
            .expect("authenticated request should complete");
        let envelope = response.json::<Value>().await.expect("response should be JSON");
        assert_eq!(envelope["id"], body["id"]);
        assert!(envelope.get("result").is_some());
        assert!(envelope.get("error").is_none());
    }

    let authenticated_notification = client
        .post(&mcp_url)
        .header(header::AUTHORIZATION, "Bearer transport-test-token")
        .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .send()
        .await
        .expect("authenticated notification should complete");
    assert_eq!(authenticated_notification.status(), StatusCode::ACCEPTED);
    assert!(
        authenticated_notification
            .bytes()
            .await
            .expect("body should read")
            .is_empty()
    );

    let invalid_origin = client
        .post(&mcp_url)
        .header(header::AUTHORIZATION, "Bearer transport-test-token")
        .header(
            header::ORIGIN,
            HeaderValue::from_static("https://localhost.evil.example"),
        )
        .json(&json!({"jsonrpc":"2.0","id":26,"method":"tools/list"}))
        .send()
        .await
        .expect("invalid-origin request should complete");
    assert_eq!(invalid_origin.status(), StatusCode::FORBIDDEN);

    server.abort();
    set_mcp_token(None);
}

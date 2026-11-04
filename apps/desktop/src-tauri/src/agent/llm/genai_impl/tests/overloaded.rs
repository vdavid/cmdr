//! A provider's 503 through the REAL genai Gemini adapter, against a local server: it reaches the
//! seam typed as `Overloaded`, carrying its `Retry-After`, on the shape genai uses for it (the
//! stream opens, then fails before its first chunk). That shape is why the runtime's retry reads
//! the consumed stream rather than only a call that failed to open (`chat/runtime/retry.rs`).

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::super::GenaiAgentLlm;
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{AgentDelta, AgentLlmError, AgentMessage, AgentPart, AgentRole};
use crate::ai::client::AiBackend;

/// What Gemini answers under "high demand" (verified live on `gemini-3.8-flash`, 2026-11-03),
/// plus a `Retry-After`.
async fn overloaded() -> impl IntoResponse {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::CONTENT_TYPE, "application/json"), (header::RETRY_AFTER, "4")],
        r#"{"error":{"code":503,"message":"This model is currently experiencing high demand. Please try again later.","status":"UNAVAILABLE"}}"#,
    )
}

#[tokio::test]
async fn a_503_reaches_the_seam_as_overloaded_with_its_retry_after() {
    let app = Router::new().fallback(overloaded);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    let backend = AiBackend::remote_for_tests(
        "k".into(),
        format!("http://127.0.0.1:{port}/v1beta/"),
        "gemini-3.8-flash".into(),
    );
    let llm = GenaiAgentLlm::new(backend);
    let user = AgentMessage {
        role: AgentRole::User,
        parts: vec![AgentPart::Text("hi".into())],
        at: 1,
    };

    let error = match llm
        .respond(
            "sys",
            &[],
            std::slice::from_ref(&user),
            crate::agent::llm::CallSettings::default(),
            CancellationToken::new(),
        )
        .await
    {
        Err(error) => error,
        // genai's `Start` (a keep-alive here, nothing on screen) can come first, which is why
        // a keep-alive doesn't count as "streamed" for the retry.
        Ok(mut stream) => loop {
            match stream.next().await.expect("the stream yields its failure") {
                Ok(AgentDelta::KeepAlive) => continue,
                Ok(other) => panic!("a 503 streams nothing, got {other:?}"),
                Err(error) => break error,
            }
        },
    };

    match error {
        AgentLlmError::Overloaded { detail, retry_after } => {
            assert_eq!(retry_after, Some(std::time::Duration::from_secs(4)));
            assert!(
                detail.contains("high demand"),
                "the provider's line rides along: {detail}"
            );
        }
        other => panic!("expected Overloaded, got {other:?}"),
    }
}

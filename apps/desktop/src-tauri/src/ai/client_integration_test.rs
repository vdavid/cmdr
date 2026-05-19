//! Integration tests for the AI client.
//!
//! Spins up a `wiremock` server and points an `AiBackend` at it. Asserts request shape
//! (URL path, headers, JSON body) and response parsing without burning real API quota.
//! The point is to lock in behavior we care about per-model:
//! - Regular chat models (`gpt-4o-mini`): `temperature` and `top_p` go on `/v1/chat/completions`.
//! - GPT-5 / pro / codex: routed to `/v1/responses`, no temperature, `reasoning.effort` set.
//! - OpenAI reasoning chat models (`o3-mini` etc.): stay on `/v1/chat/completions` but omit
//!   `temperature` (defense-in-depth heuristic in our client).

use genai::chat::ChatOptions;
use serde_json::{Value, json};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::client::{AiBackend, AiError, chat_completion};

const SYSTEM_PROMPT: &str = "system";
const USER_PROMPT: &str = "hi";

fn opts() -> ChatOptions {
    ChatOptions::default()
        .with_temperature(0.6)
        .with_max_tokens(50)
        .with_top_p(0.95)
}

fn chat_completions_response_body(text: &str) -> Value {
    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 0,
        "model": "test-model",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": text },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    })
}

fn responses_api_response_body(text: &str, model: &str) -> Value {
    // genai's openai_resp adapter requires top-level `model` and reads
    // `output[].content[].text` for items with `type: "output_text"`.
    json!({
        "id": "resp-test",
        "object": "response",
        "status": "completed",
        "model": model,
        "output": [{
            "type": "message",
            "id": "msg-1",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": text }]
        }]
    })
}

#[tokio::test]
async fn regular_openai_chat_model_sends_temperature_on_chat_completions() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chat_completions_response_body("hello")))
        .mount(&server)
        .await;

    let backend = AiBackend::remote(
        String::from("test-key"),
        format!("{}/v1", server.uri()),
        String::from("gpt-4o-mini"),
    );

    let res = chat_completion(&backend, SYSTEM_PROMPT, USER_PROMPT, &opts())
        .await
        .expect("ok response");
    assert_eq!(res, "hello");

    let req = take_only_request(&server).await;
    let body: Value = serde_json::from_slice(&req.body).expect("json body");

    let temperature = body["temperature"].as_f64().expect("temperature set");
    assert!(
        (temperature - 0.6).abs() < 1e-9,
        "regular chat model must receive caller-supplied temperature ~0.6, got {temperature}"
    );
    let top_p = body["top_p"].as_f64().expect("top_p set");
    assert!((top_p - 0.95).abs() < 1e-9, "top_p ~0.95, got {top_p}");
    assert_eq!(body["max_tokens"].as_u64().expect("max_tokens set"), 50);
    assert!(body.get("reasoning").is_none(), "no reasoning for regular chat model");
}

#[tokio::test]
async fn gpt5_routes_to_responses_api_with_reasoning_effort() {
    // genai 0.6+ auto-routes any `gpt-5*` to the Responses API. Plus we layer our
    // own `adjust_for_model` on top to make sure no `temperature` slips through.
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(responses_api_response_body("ok", "gpt-5.5")))
        .mount(&server)
        .await;

    let backend = AiBackend::remote(
        String::from("test-key"),
        format!("{}/v1", server.uri()),
        String::from("gpt-5.5"),
    );

    chat_completion(&backend, SYSTEM_PROMPT, USER_PROMPT, &opts())
        .await
        .expect("ok response");

    let req = take_only_request(&server).await;
    assert_eq!(req.url.path(), "/v1/responses", "gpt-5.5 must hit Responses API");

    let body: Value = serde_json::from_slice(&req.body).expect("json body");
    assert!(
        body.get("temperature").is_none(),
        "gpt-5 family must NOT send temperature (OpenAI rejects with HTTP 400)"
    );
    assert_eq!(
        body["reasoning"]["effort"].as_str().expect("reasoning.effort set"),
        "low",
        "Responses-only models steer via reasoning_effort"
    );
}

#[tokio::test]
async fn gpt_pro_routes_to_responses_api() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(responses_api_response_body("ok", "gpt-5.5-pro-2026-04-23")),
        )
        .mount(&server)
        .await;

    let backend = AiBackend::remote(
        String::from("test-key"),
        format!("{}/v1", server.uri()),
        String::from("gpt-5.5-pro-2026-04-23"),
    );

    chat_completion(&backend, SYSTEM_PROMPT, USER_PROMPT, &opts())
        .await
        .expect("ok response");

    let req = take_only_request(&server).await;
    assert_eq!(req.url.path(), "/v1/responses", "*-pro must hit Responses API");
}

#[tokio::test]
async fn o3_chat_reasoning_model_omits_temperature() {
    // o3 stays on /v1/chat/completions in genai 0.6 (it's not part of the gpt-5 routing
    // rule), but it still rejects custom temperature. Our `is_openai_chat_reasoning_model`
    // heuristic must catch this.
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chat_completions_response_body("ok")))
        .mount(&server)
        .await;

    let backend = AiBackend::remote(
        String::from("test-key"),
        format!("{}/v1", server.uri()),
        String::from("o3-mini"),
    );

    chat_completion(&backend, SYSTEM_PROMPT, USER_PROMPT, &opts())
        .await
        .expect("ok response");

    let req = take_only_request(&server).await;
    let body: Value = serde_json::from_slice(&req.body).expect("json body");
    assert!(
        body.get("temperature").is_none(),
        "o3-* must NOT send temperature (rejected by OpenAI)"
    );
    assert!(body.get("top_p").is_none(), "o3-* must NOT send top_p");
}

#[tokio::test]
async fn local_backend_sends_temperature_and_uses_localhost_endpoint() {
    // Local llama-server speaks OpenAI chat completions and DOES respect temperature.
    // We use wiremock in place of a real llama-server; the AiBackend::local code path
    // forces the OpenAI adapter and pins endpoint to localhost.
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chat_completions_response_body("local-out")))
        .mount(&server)
        .await;

    let port: u16 = server
        .uri()
        .strip_prefix("http://127.0.0.1:")
        .expect("wiremock binds to 127.0.0.1")
        .parse()
        .expect("valid port");

    let backend = AiBackend::local(port);

    let res = chat_completion(&backend, SYSTEM_PROMPT, USER_PROMPT, &opts())
        .await
        .expect("ok response");
    assert_eq!(res, "local-out");

    let req = take_only_request(&server).await;
    let body: Value = serde_json::from_slice(&req.body).expect("json body");
    let temperature = body["temperature"].as_f64().expect("temperature set");
    assert!(
        (temperature - 0.6).abs() < 1e-9,
        "local llama-server gets temperature ~0.6, got {temperature}"
    );
    assert_eq!(body["max_tokens"].as_u64().expect("max_tokens set"), 50);
}

#[tokio::test]
async fn http_500_maps_to_server_error() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let backend = AiBackend::remote(
        String::from("test-key"),
        format!("{}/v1", server.uri()),
        String::from("gpt-4o-mini"),
    );

    let err = chat_completion(&backend, SYSTEM_PROMPT, USER_PROMPT, &opts())
        .await
        .expect_err("expected error");

    let AiError::ServerError(msg) = err else {
        panic!("expected ServerError, got {err:?}");
    };
    assert!(msg.contains("500"), "error should mention HTTP status, got: {msg}");
}

async fn take_only_request(server: &MockServer) -> Request {
    let mut requests = server.received_requests().await.expect("wiremock requests recorded");
    assert_eq!(requests.len(), 1, "expected exactly one request");
    requests.remove(0)
}

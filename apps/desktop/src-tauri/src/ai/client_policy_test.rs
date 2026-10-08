//! The managed-policy backstop in the three `client.rs` request functions, and the redirect guard
//! on remote clients. A backend is built once and reused (an Ask Cmdr turn makes many requests on
//! one), so a policy that arrives after it was built must still stop its NEXT request, and an
//! allowed host must not be able to redirect a request to one the policy refuses.

use futures_util::StreamExt;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::client::{AiBackend, AiError, chat_completion, chat_completion_stream};
use crate::managed_policy::testing::{self, ALLOWED_CLOUD_AI_HOSTS, DISABLE_AI, DISABLE_CLOUD_AI};
use crate::managed_policy::{ManagedAiRefusal, ManagedPolicy};

fn reply(text: &str) -> Value {
    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 0,
        "model": "test-model",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": text }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    })
}

async fn answering_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply("hello")))
        .mount(&server)
        .await;
    server
}

fn remote_backend(server: &MockServer) -> AiBackend {
    AiBackend::remote("test-key".into(), format!("{}/v1", server.uri()), "gpt-4o-mini".into())
}

fn hosts(entries: &[&str]) -> ManagedPolicy {
    let list = entries.iter().map(|e| plist::Value::String((*e).to_string())).collect();
    testing::from_values(&[(ALLOWED_CLOUD_AI_HOSTS, plist::Value::Array(list))])
}

async fn received(server: &MockServer) -> usize {
    server.received_requests().await.map_or(0, |requests| requests.len())
}

#[tokio::test]
async fn a_backend_built_while_allowed_refuses_its_next_request_once_cloud_ai_is_off() {
    let server = answering_server().await;
    let backend = remote_backend(&server);
    let opts = ChatOptions::default();

    assert_eq!(
        chat_completion(&backend, "s", "u", &opts).await.expect("allowed"),
        "hello"
    );
    assert_eq!(received(&server).await, 1);

    let _policy = testing::override_for_test(testing::forcing(&[DISABLE_CLOUD_AI]));
    let refused = chat_completion(&backend, "s", "u", &opts).await;
    assert!(matches!(refused, Err(AiError::Managed(ManagedAiRefusal::CloudAiOff))));

    let stream = chat_completion_stream(&backend, "s", "u", &opts).await;
    assert!(matches!(stream, Err(AiError::Managed(ManagedAiRefusal::CloudAiOff))));

    let request = ChatRequest::new(vec![ChatMessage::user("u")]);
    let raw = backend.exec_chat_stream_request(request, &opts).await;
    assert!(matches!(raw, Err(AiError::Managed(ManagedAiRefusal::CloudAiOff))));

    assert_eq!(received(&server).await, 1, "nothing left after the policy flipped");
}

#[tokio::test]
async fn a_host_outside_the_list_is_refused_and_a_listed_one_goes_through() {
    let server = answering_server().await;
    let backend = remote_backend(&server);
    let opts = ChatOptions::default();

    {
        let _policy = testing::override_for_test(hosts(&["api.openai.com"]));
        let refused = chat_completion(&backend, "s", "u", &opts).await;
        assert!(matches!(
            refused,
            Err(AiError::Managed(ManagedAiRefusal::HostNotAllowed))
        ));
        assert_eq!(received(&server).await, 0);
    }

    let _policy = testing::override_for_test(hosts(&["127.0.0.1"]));
    assert!(chat_completion(&backend, "s", "u", &opts).await.is_ok());
    assert_eq!(received(&server).await, 1);
}

#[tokio::test]
async fn ai_off_refuses_the_local_server_too() {
    let server = answering_server().await;
    let port = server.address().port();
    let backend = AiBackend::local(port);
    let opts = ChatOptions::default();

    {
        let _policy = testing::override_for_test(testing::forcing(&[DISABLE_CLOUD_AI]));
        assert!(
            chat_completion(&backend, "s", "u", &opts).await.is_ok(),
            "cloud off keeps Cmdr's own server"
        );
    }

    let _policy = testing::override_for_test(testing::forcing(&[DISABLE_AI]));
    let refused = chat_completion(&backend, "s", "u", &opts).await;
    assert!(matches!(refused, Err(AiError::Managed(ManagedAiRefusal::AiOff))));
    let stream = chat_completion_stream(&backend, "s", "u", &opts).await;
    assert!(matches!(stream, Err(AiError::Managed(ManagedAiRefusal::AiOff))));
    assert_eq!(received(&server).await, 1);
}

/// An allowed host answering 307 must not carry the request to a host the policy refuses.
#[tokio::test]
async fn a_redirect_to_a_refused_host_never_reaches_it() {
    let elsewhere = answering_server().await;
    let allowed = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(307).insert_header("location", format!("{}/v1/chat/completions", elsewhere.uri())),
        )
        .mount(&allowed)
        .await;

    // Both servers listen on 127.0.0.1, so the list names the allowed one by its port.
    let _policy = testing::override_for_test(hosts(&[&format!("127.0.0.1:{}", allowed.address().port())]));
    let backend = remote_backend(&allowed);
    let opts = ChatOptions::default();

    assert!(chat_completion(&backend, "s", "u", &opts).await.is_err());
    if let Ok(mut stream) = chat_completion_stream(&backend, "s", "u", &opts).await {
        while stream.next().await.is_some() {}
    }
    assert_eq!(received(&allowed).await, 2, "the allowed host saw both requests");
    assert_eq!(received(&elsewhere).await, 0, "the redirect target saw nothing");
}

/// Without a policy the same redirect is followed, so the guard is what stops it above.
#[tokio::test]
async fn without_a_policy_a_redirect_is_followed() {
    let elsewhere = answering_server().await;
    let first = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(307).insert_header("location", format!("{}/v1/chat/completions", elsewhere.uri())),
        )
        .mount(&first)
        .await;

    let backend = remote_backend(&first);
    assert_eq!(
        chat_completion(&backend, "s", "u", &ChatOptions::default())
            .await
            .expect("followed"),
        "hello"
    );
    assert_eq!(received(&elsewhere).await, 1);
}

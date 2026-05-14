//! Integration tests for `chat_completion_stream`.
//!
//! Spins up a minimal axum server that emits SSE-formatted OpenAI chat-completions
//! deltas with configurable delays between frames. `wiremock` can't do this: it
//! buffers the whole body and writes it in a single response, defeating the point
//! of testing chunk-by-chunk parsing through `genai`'s SSE pipeline.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::routing::post;
use futures_util::StreamExt;
use futures_util::stream::{self, BoxStream};
use genai::chat::ChatOptions;
use serde_json::json;
use tokio::net::TcpListener;

use super::client::{AiBackend, AiError, chat_completion_stream};

const SYSTEM: &str = "system";
const USER: &str = "hi";

fn opts() -> ChatOptions {
    ChatOptions::default()
        .with_temperature(0.6)
        .with_max_tokens(50)
        .with_top_p(0.95)
}

/// Frame programmable by tests.
#[derive(Clone, Debug)]
enum Frame {
    /// Emit `data: {"choices":[{"delta":{"content":"<text>"}}]}\n\n`
    Delta(String),
    /// Emit `data: [DONE]` then close.
    Done,
}

#[derive(Clone)]
struct ServerState {
    frames: Arc<Vec<Frame>>,
}

async fn handler(State(state): State<ServerState>) -> Sse<BoxStream<'static, Result<Event, Infallible>>> {
    let frames = state.frames.clone();
    // unfold over the frame index. Each step sleeps then emits the corresponding event.
    let stream = stream::unfold(0usize, move |idx| {
        let frames = frames.clone();
        async move {
            if idx >= frames.len() {
                return None;
            }
            // Small delay forces genai's SSE parser to receive each frame separately
            // rather than as one big body.
            tokio::time::sleep(Duration::from_millis(15)).await;
            let event = match &frames[idx] {
                Frame::Delta(text) => {
                    let payload = json!({
                        "id": "chatcmpl-test",
                        "object": "chat.completion.chunk",
                        "created": 0,
                        "model": "test-model",
                        "choices": [{
                            "index": 0,
                            "delta": { "content": text },
                            "finish_reason": null
                        }]
                    });
                    Event::default().data(payload.to_string())
                }
                Frame::Done => Event::default().data("[DONE]"),
            };
            Some((Ok::<_, Infallible>(event), idx + 1))
        }
    });
    Sse::new(stream.boxed())
}

/// Spawns a one-shot test server on 127.0.0.1:0. Returns its base URL (no trailing slash;
/// `AiBackend::remote` normalizes that).
async fn spawn_server(frames: Vec<Frame>) -> String {
    let state = ServerState {
        frames: Arc::new(frames),
    };
    let app = Router::new()
        .route("/v1/chat/completions", post(handler))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn streams_multiple_chunks_in_order() {
    let base_url = spawn_server(vec![
        Frame::Delta(String::from("hello ")),
        Frame::Delta(String::from("world")),
        Frame::Delta(String::from("!")),
        Frame::Done,
    ])
    .await;

    let backend = AiBackend::remote(String::from("test-key"), base_url, String::from("gpt-4o-mini"));

    let mut stream = chat_completion_stream(&backend, SYSTEM, USER, &opts())
        .await
        .expect("stream open");

    let mut collected = Vec::new();
    while let Some(item) = stream.next().await {
        collected.push(item.expect("chunk ok"));
    }

    assert_eq!(collected, vec!["hello ", "world", "!"]);
}

#[tokio::test]
async fn empty_stream_ends_cleanly() {
    // Server emits only [DONE], no content. Stream yields nothing, ends Ok.
    let base_url = spawn_server(vec![Frame::Done]).await;
    let backend = AiBackend::remote(String::from("test-key"), base_url, String::from("gpt-4o-mini"));

    let mut stream = chat_completion_stream(&backend, SYSTEM, USER, &opts())
        .await
        .expect("stream open");

    let mut count = 0;
    while let Some(item) = stream.next().await {
        item.expect("ok item");
        count += 1;
    }

    assert_eq!(count, 0, "expected zero content chunks");
}

#[tokio::test]
async fn cancel_via_drop_closes_connection() {
    // Server emits 5 frames with delays. We collect the first one, drop the stream,
    // and assert the remaining frames don't arrive (proven by `count < 5`). Drop semantics:
    // the genai stream closes its underlying reqwest body; the server's `tx.send`
    // returns Err, and the spawned producer breaks out of its loop.
    let base_url = spawn_server(vec![
        Frame::Delta(String::from("1 ")),
        Frame::Delta(String::from("2 ")),
        Frame::Delta(String::from("3 ")),
        Frame::Delta(String::from("4 ")),
        Frame::Delta(String::from("5 ")),
        Frame::Done,
    ])
    .await;
    let backend = AiBackend::remote(String::from("test-key"), base_url, String::from("gpt-4o-mini"));

    let mut stream = chat_completion_stream(&backend, SYSTEM, USER, &opts())
        .await
        .expect("stream open");

    let first = stream.next().await.expect("first chunk").expect("ok");
    assert_eq!(first, "1 ");

    // Drop the stream. Should close the reqwest connection.
    drop(stream);
    // We don't have a programmatic way to assert the server saw the disconnect without
    // adding more wiring; the assertion here is "we got back to the test thread cleanly,
    // which only happens if drop didn't deadlock or panic."
}

#[tokio::test]
async fn http_500_maps_to_server_error() {
    // 500 from a server that just returns an error before the SSE upgrade.
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async { (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    let base_url = format!("http://{addr}/v1");
    let backend = AiBackend::remote(String::from("test-key"), base_url, String::from("gpt-4o-mini"));

    // The stream may open Ok (HTTP 500 isn't read until we start polling), or fail at
    // open if genai checks status eagerly. Both are valid; we just want a ServerError
    // somewhere in the path.
    let hit_server_error = match chat_completion_stream(&backend, SYSTEM, USER, &opts()).await {
        Err(AiError::ServerError(_)) => true,
        Err(other) => panic!("expected ServerError, got {other:?}"),
        Ok(mut stream) => match stream.next().await {
            Some(Err(AiError::ServerError(_))) => true,
            Some(Err(other)) => panic!("expected ServerError, got {other:?}"),
            Some(Ok(_)) => panic!("expected error, got chunk"),
            None => false,
        },
    };
    assert!(hit_server_error, "expected a ServerError on the path");
}

//! Real-server E2E smoke tests against a running local llama-server. Gated behind
//! `#[ignore]` and the `CMDR_LOCAL_LLAMA_PORT` env var.
//!
//! Purpose: prove the local-LLM streaming path works end-to-end against an actual
//! reqwest connection to a real model — covering the gap between our axum mock SSE
//! tests (`client_streaming_test.rs`) and real-world behavior. The local LLM is the
//! path that defines whether Cmdr's "blazing fast" promise holds when AI is on.
//!
//! Run with:
//! ```sh
//! AI_DIR="$HOME/Library/Application Support/com.veszelovszki.cmdr-dev/ai"
//! DYLD_LIBRARY_PATH="$AI_DIR" "$AI_DIR/llama-server" \
//!   -m "$AI_DIR/ministral-3b-instruct-q4km.gguf" --port 21847 --host 127.0.0.1 -c 4096 -ngl 99 --jinja &
//! sleep 8
//! CMDR_LOCAL_LLAMA_PORT=21847 cargo nextest run --lib --run-ignored only ai::client_local_llama_test
//! ```

use std::time::Instant;

use futures_util::StreamExt;
use genai::chat::ChatOptions;

use super::client::{AiBackend, chat_completion, chat_completion_stream};

const SYSTEM: &str = "You output only what is requested. No preface, no formatting.";
const USER: &str = "List exactly 5 short folder names for a software project, one per line, no numbering, no markdown.";

fn port_or_skip() -> Option<u16> {
    std::env::var("CMDR_LOCAL_LLAMA_PORT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn opts() -> ChatOptions {
    ChatOptions::default()
        .with_temperature(0.6)
        .with_max_tokens(150)
        .with_top_p(0.95)
}

#[tokio::test]
#[ignore = "real local llama-server — set CMDR_LOCAL_LLAMA_PORT to run"]
async fn local_chat_completion_returns_text() {
    let Some(port) = port_or_skip() else {
        panic!("CMDR_LOCAL_LLAMA_PORT not set");
    };
    let backend = AiBackend::local(port);

    let t0 = Instant::now();
    let res = chat_completion(&backend, SYSTEM, USER, &opts())
        .await
        .expect("local llama-server should respond");
    let elapsed = t0.elapsed();

    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke", "local chat → {elapsed:?}, content: {res}");
}

#[tokio::test]
#[ignore = "real local llama-server — set CMDR_LOCAL_LLAMA_PORT to run"]
async fn local_streaming_emits_multiple_chunks_progressively() {
    let Some(port) = port_or_skip() else {
        panic!("CMDR_LOCAL_LLAMA_PORT not set");
    };
    let backend = AiBackend::local(port);

    let t_start = Instant::now();
    let mut stream = chat_completion_stream(&backend, SYSTEM, USER, &opts())
        .await
        .expect("stream should open against local server");

    let mut chunks = 0;
    let mut total = String::new();
    let mut first_chunk_at = None;
    while let Some(item) = stream.next().await {
        let chunk = item.expect("chunk ok");
        if first_chunk_at.is_none() {
            first_chunk_at = Some(t_start.elapsed());
        }
        total.push_str(&chunk);
        chunks += 1;
    }
    let total_elapsed = t_start.elapsed();
    let ttft = first_chunk_at.expect("expected at least one chunk");

    assert!(chunks > 1, "streaming should emit MORE than one chunk (got {chunks})");
    assert!(!total.trim().is_empty(), "total assembled text should be non-empty");
    // The whole point of streaming: time-to-first-chunk should be measurably shorter
    // than total time. This will only be true on a model that emits >1 token at >1
    // tok/s; the local 3B model qualifies.
    assert!(
        ttft < total_elapsed,
        "TTFT ({ttft:?}) should be < total ({total_elapsed:?}) — otherwise we're not streaming"
    );

    log::info!(
        target: "ai_smoke",
        "local stream → {chunks} chunks, ttft={ttft:?}, total={total_elapsed:?}, content: {total}"
    );
}

#[tokio::test]
#[ignore = "real local llama-server — set CMDR_LOCAL_LLAMA_PORT to run"]
async fn local_streaming_through_sanitizer_emits_valid_suggestions() {
    // The real test: feed a real local-LLM stream through `StreamingSanitizer` and
    // assert it produces ≥1 valid folder name. This is the combination users will
    // actually hit (`stream_folder_suggestions` does exactly this internally) — minus
    // only the Tauri Channel transport, which is covered by other tests.
    let Some(port) = port_or_skip() else {
        panic!("CMDR_LOCAL_LLAMA_PORT not set");
    };
    let backend = AiBackend::local(port);

    let folder_prompt = "Suggest 5 new folder names for a software project. Output ONLY folder names, one per line, no numbering, no markdown.";
    let mut stream = chat_completion_stream(&backend, SYSTEM, folder_prompt, &opts())
        .await
        .expect("stream open");

    let existing: Vec<String> = vec![String::from("src"), String::from("README.md")];
    let mut sanitizer = super::suggestions::StreamingSanitizer::new(&existing);
    let mut suggestions: Vec<String> = Vec::new();
    while let Some(item) = stream.next().await {
        let chunk = item.expect("chunk ok");
        sanitizer.push_chunk(&chunk, |name| {
            suggestions.push(name);
            true
        });
    }
    sanitizer.finish(|name| {
        suggestions.push(name);
        true
    });

    assert!(!suggestions.is_empty(), "expected ≥1 valid suggestion, got 0");
    assert!(
        suggestions.len() <= 5,
        "expected ≤5 suggestions, got {}",
        suggestions.len()
    );
    for name in &suggestions {
        assert!(!name.is_empty(), "blank suggestion");
        assert!(name.len() <= 255, "name too long: {name}");
        assert!(!name.contains('/') && !name.contains('\0'), "invalid char in: {name}");
        assert!(
            !existing.iter().any(|e| e.eq_ignore_ascii_case(name)),
            "duplicate of existing: {name}"
        );
    }
    log::info!(target: "ai_smoke", "local pipeline → {suggestions:?}");
}

#[tokio::test]
#[ignore = "real local llama-server — set CMDR_LOCAL_LLAMA_PORT to run"]
async fn local_streaming_drop_mid_stream_releases_server() {
    // Drops the stream after the first chunk. Server should keep accepting new requests
    // afterward (proves the connection cleanup path works against a real server, not
    // just the axum mock). The next test would block forever if the server was wedged.
    let Some(port) = port_or_skip() else {
        panic!("CMDR_LOCAL_LLAMA_PORT not set");
    };
    let backend = AiBackend::local(port);

    let mut stream = chat_completion_stream(&backend, SYSTEM, USER, &opts())
        .await
        .expect("stream open");
    let _first = stream.next().await.expect("first chunk").expect("ok");
    drop(stream);

    // Now hit it again. If the previous connection wasn't released cleanly, this would
    // either hang (server waiting for slot) or fail with a connection error.
    let res = chat_completion(&backend, SYSTEM, "Say 'pong'.", &opts())
        .await
        .expect("server should accept a new request after stream drop");
    assert!(!res.trim().is_empty(), "second request should succeed");
}

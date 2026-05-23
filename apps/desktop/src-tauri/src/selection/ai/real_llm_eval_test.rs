//! Real-LLM eval for the Selection AI translation pipeline.
//!
//! **Not run in CI**: each test is `#[ignore]`-gated and needs a valid
//! `OPENAI_API_KEY` env var. Run with:
//!
//! ```sh
//! OPENAI_API_KEY=$(security find-generic-password -a "$USER" -s "OPENAI_API_KEY" -w) \
//!   cargo nextest run --lib --run-ignored only selection::ai::real_llm_eval_test
//! ```
//!
//! Cost: a few cents per full run. Purpose: confirm the prompt + parser produce a
//! parseable `SelectionTranslateResult` against a real model. Iterate the prompt
//! here, not in unit tests.
//!
//! The default model is whatever the user has configured in
//! `Settings > AI > Cloud > OpenAI`. For repeatability under CI-like conditions we
//! pin to `gpt-4o-mini` here; David can rerun against his configured model by
//! editing the const below.

use genai::chat::ChatOptions;

use crate::ai::client::{AiBackend, chat_completion};
use crate::selection::ai::{
    SelectionTranslateResult, build_classification_prompt, build_selection_translate_result, parse_selection_response,
};

const BASE_URL: &str = "https://api.openai.com/v1/";
const MODEL: &str = "gpt-4o-mini";

fn api_key_or_skip() -> Option<String> {
    let key = std::env::var("OPENAI_API_KEY").ok()?;
    if key.trim().is_empty() {
        return None;
    }
    Some(key)
}

fn opts() -> ChatOptions {
    ChatOptions::default()
        .with_temperature(0.2)
        .with_max_tokens(300)
        .with_top_p(0.9)
}

/// Runs one end-to-end translation against the real model and returns the parsed
/// result, mirroring what `commands::selection::translate_selection_query` does in
/// production.
async fn translate(prompt: &str, sample: &[&str]) -> SelectionTranslateResult {
    let api_key = api_key_or_skip().expect("OPENAI_API_KEY not set");
    let backend = AiBackend::remote(api_key, BASE_URL.to_string(), MODEL.to_string());

    let sample: Vec<String> = sample.iter().map(|s| (*s).to_string()).collect();
    let system_prompt = build_classification_prompt(&sample);

    let raw = chat_completion(&backend, &system_prompt, prompt, &opts())
        .await
        .expect("real call to OpenAI should succeed");
    log::info!(target: "selection::eval", "prompt={prompt:?} → raw response: {raw}");

    let parsed = parse_selection_response(&raw);
    let result = build_selection_translate_result(&parsed);
    log::info!(target: "selection::eval", "parsed result: {result:?}");
    result
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn eval_all_log_files_returns_log_glob() {
    let sample = ["app.log", "error.log", "notes.txt", "todo.md", "screenshot.png"];
    let r = translate("all log files", &sample).await;
    assert!(r.pattern.is_some(), "expected a pattern");
    let pattern = r.pattern.as_deref().unwrap().to_lowercase();
    assert!(pattern.contains("log"), "pattern {pattern:?} should mention `log`");
    assert!(r.kind.is_some());
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn eval_png_or_jpg_returns_alternation() {
    let sample = ["a.png", "b.jpg", "c.jpeg", "d.gif", "notes.txt"];
    let r = translate("png and jpg images", &sample).await;
    assert!(r.pattern.is_some());
    let pattern = r.pattern.as_deref().unwrap().to_lowercase();
    // Either a regex alternation or a brace glob; both are acceptable for this intent.
    assert!(
        pattern.contains("png") && (pattern.contains("jpg") || pattern.contains("jpeg")),
        "pattern {pattern:?} should mention both png and jpg/jpeg"
    );
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn eval_size_only_intent_returns_pattern_plus_size() {
    let sample = ["movie.mp4", "small.txt", "archive.zip", "doc.pdf"];
    let r = translate("files bigger than 5 MB", &sample).await;
    assert!(r.pattern.is_some(), "should still emit some pattern");
    assert!(r.size_min.is_some(), "size filter must be set for a size intent");
    let min = r.size_min.unwrap();
    // Allow some wiggle room around 5 MiB / 5 MB but reject obvious mistakes.
    assert!(
        (4_000_000..=10_000_000).contains(&min),
        "size_min {min} should be around 5 MB"
    );
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn eval_backups_from_last_week_returns_date_filter() {
    let sample = [
        "project-2026-05-15-backup.zip",
        "project-2026-05-08-backup.zip",
        "report.docx",
    ];
    let r = translate("backups from last week", &sample).await;
    assert!(r.pattern.is_some());
    assert!(
        r.modified_after.is_some(),
        "model should set a modified_after for a `last week` intent"
    );
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn eval_rymd_files_returns_substring_glob() {
    let sample = [
        "rymd-invoice-001.pdf",
        "rymd-invoice-002.pdf",
        "report.docx",
        "screenshot.png",
    ];
    let r = translate("every rymd file", &sample).await;
    assert!(r.pattern.is_some());
    let pattern = r.pattern.as_deref().unwrap().to_lowercase();
    assert!(pattern.contains("rymd"), "pattern {pattern:?} should match the keyword");
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn eval_unfilterable_intent_returns_note_or_caveat() {
    let sample = [
        "Final-Draft-v1.docx",
        "Final-Draft-v2.docx",
        "Final-Notes.md",
        "Scratch.txt",
    ];
    let r = translate("final drafts I haven't shared", &sample).await;
    // Either the model emits a `note:` (which becomes `caveat`) or the matcher
    // produces a `final*` pattern with no caveat. Both are acceptable; the test only
    // confirms the result is parseable and isn't a half-built query.
    assert!(r.pattern.is_some() || r.caveat.is_some());
}

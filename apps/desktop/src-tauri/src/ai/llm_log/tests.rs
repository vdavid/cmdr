//! Tests for the LLM call logger.
//!
//! The pure helpers (slug, counter, file name, redaction round-trip) are tested directly and
//! deterministically. The orchestration tests drive [`record_request`] / [`CallLog::log_response`]
//! against a per-test temp directory: the file write is on a detached thread, so they poll for
//! the files with a bounded timeout. Each temp dir is unique, so the per-session `COUNTERS`
//! cache never collides across parallel tests.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::*;

// ── Pure helpers ────────────────────────────────────────────────────────────────

#[test]
fn sanitize_component_keeps_only_lowercase_alnum_and_dashes() {
    assert_eq!(sanitize_component("Report (v2).final"), "report-v2-final");
    assert_eq!(sanitize_component("  leading/trailing  "), "leading-trailing");
    assert_eq!(sanitize_component("ALL CAPS 123"), "all-caps-123");
    assert_eq!(sanitize_component("émigré"), "migr"); // non-ascii dropped, no separators leak
    assert_eq!(sanitize_component("!!!"), "");
    assert_eq!(sanitize_component(""), "");
}

#[test]
fn build_slug_prefixes_job_and_takes_a_few_words() {
    assert_eq!(
        build_slug(JobKind::AgentChat, "How big is my Downloads folder?"),
        "agent-chat-how-big-is-my-downloads-folder"
    );
    // Empty message → just the job prefix, no trailing dash.
    assert_eq!(build_slug(JobKind::FolderSuggestions, ""), "folder-suggestions");
    assert_eq!(build_slug(JobKind::FolderSuggestions, "!!!"), "folder-suggestions");
    // Word cap: only the first six words survive.
    assert_eq!(
        build_slug(JobKind::TranslateSearch, "one two three four five six seven eight"),
        "translate-search-one-two-three-four-five-six"
    );
}

#[test]
fn build_slug_bounds_length_without_a_trailing_dash() {
    let slug = build_slug(
        JobKind::AgentChat,
        "supercalifragilisticexpialidocious antidisestablishmentarianism",
    );
    assert!(slug.len() <= 48, "slug stays bounded: {slug} ({} chars)", slug.len());
    assert!(!slug.ends_with('-'), "no dangling dash after truncation: {slug}");
    assert!(slug.starts_with("agent-chat-"));
}

#[test]
fn file_name_zero_pads_to_three_digits() {
    assert_eq!(file_name(1, "request", "x"), "001_request_x.json");
    assert_eq!(file_name(42, "response", "y"), "042_response_y.json");
    // Never truncates past three digits.
    assert_eq!(file_name(1234, "request", "z"), "1234_request_z.json");
}

#[test]
fn leading_seq_parses_the_numeric_prefix() {
    assert_eq!(leading_seq("007_request_x.json"), Some(7));
    assert_eq!(leading_seq("012_response_a-b.json"), Some(12));
    assert_eq!(leading_seq("request_x.json"), None);
    assert_eq!(leading_seq(""), None);
}

#[test]
fn build_log_json_wraps_and_redacts() {
    let metadata = json!({ "cmdr.direction": "request" });
    let body = json!({ "headers": { "Authorization": "Bearer sk-secret" }, "model": "claude-x" });
    let out = build_log_json(metadata, body);

    assert_eq!(out["metadata"]["cmdr.direction"], json!("request"));
    assert_eq!(out["body"]["model"], json!("claude-x"));
    // The redaction pass ran over the whole file.
    assert_eq!(out["body"]["headers"]["Authorization"], json!("<redacted>"));
    assert!(!serde_json::to_string(&out).unwrap().contains("sk-secret"));
}

// ── Counter (temp dir; unique key per test) ──────────────────────────────────────

#[test]
fn next_seq_increments_and_continues_across_a_fresh_scan() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("thread-1");
    std::fs::create_dir_all(&session).unwrap();
    // Pre-existing files from an earlier "run": 001 and 003.
    std::fs::write(session.join("001_request_x.json"), "{}").unwrap();
    std::fs::write(session.join("003_request_y.json"), "{}").unwrap();

    // First call in this process scans and continues from max (3) + 1.
    assert_eq!(next_seq(&session), 4);
    assert_eq!(next_seq(&session), 5);
}

#[test]
fn next_seq_starts_at_one_for_a_fresh_session() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("thread-new");
    assert_eq!(next_seq(&session), 1);
    assert_eq!(next_seq(&session), 2);
}

#[test]
fn write_log_file_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("s");
    let value = json!({ "metadata": { "a": 1 }, "body": { "b": [1, 2, 3] } });
    write_log_file(&session, "001_request_x.json", &value).unwrap();

    let text = std::fs::read_to_string(session.join("001_request_x.json")).unwrap();
    let read: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(read, value);
}

// ── Orchestration (record_request / log_response) ────────────────────────────────

fn sample_request_info(user_message: &str) -> RequestInfo {
    RequestInfo {
        provider: "anthropic".into(),
        model: "claude-x".into(),
        adapter_kind: "Anthropic".into(),
        fidelity: Fidelity::RequestStruct,
        user_message: user_message.into(),
    }
}

/// Waits (bounded) for `session_dir` to hold exactly `count` files, then returns their sorted
/// names. The write is on a detached thread, so a short poll avoids flakiness.
fn wait_for_files(session_dir: &Path, count: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let names: Vec<String> = std::fs::read_dir(session_dir)
            .map(|rd| rd.flatten().filter_map(|e| e.file_name().into_string().ok()).collect())
            .unwrap_or_default();
        if names.len() >= count {
            let mut names = names;
            names.sort();
            return names;
        }
        if Instant::now() >= deadline {
            // allowed-pluralize-noun: test timeout panic; count is a fixed test expectation (never 1 in practice)
            panic!("timed out waiting for {count} files in {session_dir:?}; saw {names:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_request_and_response_pair_writes_two_numbered_files() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = LlmLogContext::agent_chat(7);

    let handle = record_request(
        dir.path(),
        &ctx,
        sample_request_info("How big is Downloads?"),
        json!({ "system": "you are read-only", "messages": [] }),
    );
    handle.log_response(
        ResponseInfo {
            prompt_tokens: Some(120),
            completion_tokens: Some(8),
            stop_reason: Some("completed".into()),
            latency_ms: 42,
            fidelity: Fidelity::Assembled,
        },
        json!({ "text": "It's 4.2 GB." }),
    );

    let session_dir = dir.path().join("thread-7");
    let files = wait_for_files(&session_dir, 2);
    assert_eq!(
        files,
        vec![
            "001_request_agent-chat-how-big-is-downloads.json".to_string(),
            "002_response_agent-chat-how-big-is-downloads.json".to_string(),
        ]
    );

    // Request metadata + body.
    let req: Value = read_json(&session_dir, &files[0]);
    assert_eq!(req["metadata"]["gen_ai.request.model"], json!("claude-x"));
    assert_eq!(req["metadata"]["gen_ai.system"], json!("anthropic"));
    assert_eq!(req["metadata"]["cmdr.direction"], json!("request"));
    assert_eq!(req["metadata"]["cmdr.fidelity"], json!("request_struct"));
    assert_eq!(req["metadata"]["cmdr.seq"], json!(1));
    assert_eq!(req["body"]["system"], json!("you are read-only"));

    // Response metadata carries usage + finish reason with OTel-style names.
    let res: Value = read_json(&session_dir, &files[1]);
    assert_eq!(res["metadata"]["gen_ai.usage.input_tokens"], json!(120));
    assert_eq!(res["metadata"]["gen_ai.usage.output_tokens"], json!(8));
    assert_eq!(res["metadata"]["gen_ai.response.finish_reasons"], json!(["completed"]));
    assert_eq!(res["metadata"]["cmdr.latency_ms"], json!(42));
    assert_eq!(res["metadata"]["cmdr.seq"], json!(2));
}

#[test]
fn a_multi_turn_tool_loop_numbers_files_in_call_order() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = LlmLogContext::agent_chat(3);

    // Two respond() calls in one turn's tool loop: request, response, request, response.
    for _ in 0..2 {
        let handle = record_request(dir.path(), &ctx, sample_request_info("list photos"), json!({}));
        handle.log_response(ResponseInfo::default(), json!({}));
    }

    let session_dir = dir.path().join("thread-3");
    let files = wait_for_files(&session_dir, 4);
    let seqs: Vec<&str> = files.iter().map(|n| &n[..12]).collect();
    assert_eq!(
        seqs,
        vec!["001_request_", "002_response", "003_request_", "004_response"]
    );
}

#[test]
fn a_one_shot_helper_groups_under_its_job_directory() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = LlmLogContext::folder_suggestions();
    let handle = record_request(dir.path(), &ctx, sample_request_info("vacation pics"), json!({}));
    handle.log_response(ResponseInfo::default(), json!({}));

    let session_dir = dir.path().join("folder-suggestions");
    let files = wait_for_files(&session_dir, 2);
    assert!(files[0].starts_with("001_request_folder-suggestions"));
}

#[test]
fn an_unwritable_root_never_panics_the_caller() {
    // Point the root at a path under a regular file, so create_dir_all fails. The call path
    // must return normally regardless — a logging problem can't break the LLM call.
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    let bad_root = blocker.join("under-a-file");

    let ctx = LlmLogContext::agent_chat(9);
    let handle = record_request(&bad_root, &ctx, sample_request_info("hi"), json!({}));
    handle.log_response(ResponseInfo::default(), json!({}));
    // Give the detached writer a moment; the assertion is simply that nothing above panicked.
    std::thread::sleep(Duration::from_millis(50));
}

// ── Enabled gate (mutates the process-global ENABLED; nextest isolates each test) ─

#[test]
fn disabled_logging_records_nothing() {
    set_enabled(false);
    // Even with a directory configured, a disabled logger returns None and writes nothing.
    let result = log_request(&LlmLogContext::agent_chat(1), sample_request_info("x"), json!({}));
    assert!(result.is_none(), "a disabled logger yields no handle");
    set_enabled(true); // restore for any same-process follow-up
}

#[test]
fn enabled_flag_round_trips() {
    set_enabled(true);
    assert!(is_enabled());
    set_enabled(false);
    assert!(!is_enabled());
    set_enabled(true);
}

fn read_json(dir: &Path, name: &str) -> Value {
    let text = std::fs::read_to_string(dir.join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

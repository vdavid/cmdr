use serde_json::json;
use tauri::test::mock_app;

use super::*;
use crate::agent::chat::budget::estimate_serialized_tokens;
use crate::tools::resources::logs::{LOG_DEFAULT_LIMIT, LogOptions, log_lines_of_window};

fn log_options() -> LogOptions {
    LogOptions {
        since_iso: None,
        filter: None,
        limit: LOG_DEFAULT_LIMIT,
    }
}

/// Both ways in read one builder: the agent's tool and an outside client's `resources/read`.
#[tokio::test]
async fn the_tool_and_resources_read_give_the_same_string() {
    let app = mock_app();
    let handle = app.handle();
    for uri in ["cmdr://dialogs/available", "cmdr://indexing"] {
        let direct = crate::tools::resources::read_resource(handle, uri)
            .await
            .expect("this view needs nothing but the app");
        let through_the_tool = execute_read_resource(handle, &json!({ "uri": uri }))
            .await
            .expect("the same view");
        assert_eq!(through_the_tool["text"], direct.text, "{uri}");
        assert_eq!(through_the_tool["uri"], uri);
        assert_eq!(through_the_tool["truncated"], false);
        assert_eq!(through_the_tool["totalChars"], direct.text.chars().count());
        assert_eq!(through_the_tool["returnedChars"], direct.text.chars().count());
        assert!(through_the_tool.get("nextOffset").is_none());
    }
}

#[tokio::test]
async fn an_unknown_view_refuses_typed() {
    let app = mock_app();
    let error = execute_read_resource(app.handle(), &json!({ "uri": "cmdr://nope" }))
        .await
        .expect_err("no such view");
    assert_eq!(error.data, Some(json!({ "reason": "unknownResource" })));
}

/// A wake can catch the webview asleep: the answer says so rather than pretending.
#[tokio::test(start_paused = true)]
async fn settings_with_no_window_answering_say_so_typed() {
    let app = mock_app();
    let error = execute_read_resource(app.handle(), &json!({ "uri": "cmdr://settings" }))
        .await
        .expect_err("nothing answers on a mock app");
    assert_eq!(error.data, Some(json!({ "reason": "windowSilent" })));
}

#[test]
fn a_long_view_fits_one_result_and_pages_to_the_end_with_nothing_lost() {
    // Lines that double when escaped, in characters of several widths.
    let line = "  - id: \"é\" 文字 🦀 \\ value\n";
    let text = line.repeat(9_000);

    let mut rebuilt = String::new();
    let mut offset = 0;
    let mut pages = 0;
    loop {
        let window = window_of("cmdr://settings", &text, offset);
        assert!(
            estimate_serialized_tokens(&window) <= MAX_TOOL_RESULT_TOKENS,
            "{} tokens",
            estimate_serialized_tokens(&window)
        );
        assert_eq!(window.total_chars, text.chars().count());
        assert_eq!(window.returned_chars, window.text.chars().count());
        rebuilt.push_str(&window.text);
        pages += 1;
        match window.next_offset {
            Some(next) => {
                assert!(window.truncated);
                assert!(window.text.ends_with('\n'), "a page ends on a whole row");
                offset = next;
            }
            None => {
                assert!(!window.truncated);
                break;
            }
        }
    }
    assert_eq!(rebuilt, text);
    assert!(pages > 1);
}

#[test]
fn one_line_longer_than_a_result_still_moves_on() {
    let text = "x".repeat(MAX_TOOL_RESULT_TOKENS * CHARS_PER_TOKEN_ESTIMATE * 2);
    let first = window_of("cmdr://logs", &text, 0);
    assert!(first.truncated);
    assert!(first.returned_chars > 0);
    assert!(estimate_serialized_tokens(&first) <= MAX_TOOL_RESULT_TOKENS);
}

#[test]
fn an_offset_past_the_end_reads_empty_and_says_it_is_done() {
    let window = window_of("cmdr://logs", "short", 900);
    assert_eq!(window.text, "");
    assert!(!window.truncated);
    assert_eq!(window.next_offset, None);
    assert_eq!(window.total_chars, 5);
}

/// A key's body has no armor to catch, so the check reads the window, not what a query picks.
#[test]
fn a_log_holding_a_private_key_refuses_whatever_the_query_picks() {
    let window = "2026-11-02T08:00:00.000+01:00 INFO -----BEGIN OPENSSH PRIVATE KEY-----\n\
                  b3BlbnNzaC1rZXktdjEAAAAA\n\
                  2026-11-02T08:00:01.000+01:00 INFO listing done\n";
    let narrow = LogOptions {
        since_iso: None,
        filter: Some("listing".to_string()),
        limit: 1,
    };
    for opts in [log_options(), narrow] {
        assert_eq!(log_lines_of_window(window, false, &opts), Err(Protected::PrivateKey));
    }
    let error: ToolError = ResourceError::Protected(Protected::PrivateKey).into_tool_error();
    assert_eq!(
        error.data,
        Some(json!({ "refusal": "protected", "reason": "privateKey" }))
    );
}

#[test]
fn an_ordinary_log_reads_redacted() {
    let window = "2026-11-02T08:00:00.000+01:00 INFO sftp://test_user_1:hunter2@files.example.test/home/test_user_1/report.pdf?token=q1w2\n\
                  2026-11-02T08:00:01.000+01:00 WARN contact test_user_12.doe@example.com at /Users/test_user_8/SecretProject/a.txt\n";
    let lines = log_lines_of_window(window, false, &log_options()).expect("nothing to refuse");
    for leak in [
        "test_user_1",
        "hunter2",
        "q1w2",
        "test_user_12.doe",
        "test_user_8",
        "SecretProject",
        "report",
    ] {
        assert!(!lines.contains(leak), "{leak} leaked: {lines}");
    }
    assert!(lines.contains("INFO"), "{lines}");
}

/// A whole file's first line is whole, and a mid-file read's may be half a line that redaction
/// can't recognize. These two had changed places.
#[test]
fn only_a_mid_file_read_drops_its_first_line() {
    let window = "first\nsecond\n";
    assert_eq!(
        log_lines_of_window(window, false, &log_options()).expect("read"),
        "first\nsecond"
    );
    assert_eq!(
        log_lines_of_window(window, true, &log_options()).expect("read"),
        "second"
    );
}

/// A provider's error text is what could echo a credential: nothing of ours logs one.
#[test]
fn an_authorization_header_and_a_key_in_a_log_line_are_not_handed_on() {
    let window = "2026-11-02T08:00:00.000+01:00 WARN request failed authorization: Bearer abc123def456ghi789\n\
                  2026-11-02T08:00:01.000+01:00 WARN provider said key sk-ant-api03-AbCdEf123456 is wrong\n";
    let lines = log_lines_of_window(window, false, &log_options()).expect("nothing to refuse");
    assert!(!lines.contains("abc123def456ghi789"), "{lines}");
    assert!(!lines.contains("AbCdEf123456"), "{lines}");
}

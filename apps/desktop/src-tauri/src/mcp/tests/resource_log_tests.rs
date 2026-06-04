//! Tests for the `cmdr://logs` builder: option parsing, line selection, the
//! `since` timestamp filter, and the mandatory PII redaction contract.

use crate::mcp::resources::logs::{
    LOG_DEFAULT_LIMIT, LOG_MAX_LIMIT, LogOptions, line_timestamp_passes_since, parse_log_options, select_log_lines,
};

/// The `cmdr://logs` resource must redact PII before returning, matching the
/// crash + error reporters. A loopback caller with no filesystem read
/// shouldn't be able to lift home paths, emails, or SMB URIs out of the log.
#[test]
fn select_log_lines_redacts_pii() {
    let opts = LogOptions {
        since_iso: None,
        filter: None,
        limit: LOG_DEFAULT_LIMIT,
    };
    let raw = "2026-05-31T08:30:02.000+02:00 INFO listing /Users/dorka/SecretProject/budget.pdf\n\
               2026-05-31T08:30:03.000+02:00 WARN contact jane.doe@example.com about smb://nas.local/share/private/file.txt";

    let out = select_log_lines(raw, false, &opts);

    // Raw PII must be gone.
    assert!(!out.contains("/Users/dorka/"), "home path leaked: {out}");
    assert!(!out.contains("SecretProject"), "custom dir name leaked: {out}");
    assert!(!out.contains("jane.doe@example.com"), "email leaked: {out}");
    assert!(!out.contains("/share/private/"), "SMB share tail leaked: {out}");
    // Redaction tokens present (path-shape preserved).
    assert!(out.contains("$HOME/"), "expected redacted home token: {out}");
    assert!(out.contains("<email>"), "expected redacted email token: {out}");
    // Non-PII log structure survives.
    assert!(out.contains("INFO listing"), "log structure dropped: {out}");
    assert!(out.contains("WARN contact"), "log structure dropped: {out}");
}

#[test]
fn parse_log_options_defaults_and_clamping() {
    let opts = parse_log_options(None);
    assert_eq!(opts.limit, LOG_DEFAULT_LIMIT);
    assert!(opts.since_iso.is_none());
    assert!(opts.filter.is_none());

    let opts = parse_log_options(Some("limit=99999"));
    assert_eq!(opts.limit, LOG_MAX_LIMIT, "limit should clamp to max");

    let opts = parse_log_options(Some("limit=0"));
    assert_eq!(opts.limit, 1, "limit should floor at 1 (zero is meaningless)");
}

#[test]
fn parse_log_options_decodes_percent() {
    let opts = parse_log_options(Some("filter=hello%20world&since=2026-05-19T08%3A30%3A00.000%2B02%3A00"));
    assert_eq!(opts.filter.as_deref(), Some("hello world"));
    assert_eq!(opts.since_iso.as_deref(), Some("2026-05-19T08:30:00.000+02:00"));
}

#[test]
fn line_timestamp_passes_since_basic() {
    let line = "2026-05-19T08:30:02.000+02:00 INFO foo";
    assert!(line_timestamp_passes_since(line, "2026-05-19T08:30:01.000+02:00"));
    assert!(!line_timestamp_passes_since(line, "2026-05-19T08:30:02.000+02:00"));
    assert!(!line_timestamp_passes_since(line, "2026-05-19T08:30:03.000+02:00"));
}

#[test]
fn line_timestamp_passes_since_keeps_lines_without_timestamp() {
    // A panic line that doesn't start with a timestamp must not be dropped.
    assert!(line_timestamp_passes_since(
        "thread main panicked at ...",
        "2026-05-19T08:30:00.000+02:00"
    ));
    // Empty line: keep.
    assert!(line_timestamp_passes_since("", "2026-05-19T08:30:00.000+02:00"));
}

//! Unit tests for `find`: params, the search over the viewer's backends, grouping and
//! capping, the snippet cut, the cancel and match-cap flags, and the row shape.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;

use super::find::{FIND_SNIPPET_CHARS, FindHits, MAX_FIND_LINES, find_hits, snippet_around};
use super::tests::assert_text_only;
use super::*;
use crate::file_viewer::headless::open_text_backend;
use crate::file_viewer::{MAX_SEARCH_MATCHES, Matcher, SearchMode};
use crate::test_support::TestDir;

fn literal(query: &str) -> TextAsk {
    TextAsk::Find(Arc::new(
        Matcher::build(
            query,
            SearchMode {
                use_regex: false,
                case_sensitive: false,
            },
        )
        .unwrap(),
    ))
}

fn regex(query: &str) -> TextAsk {
    TextAsk::Find(Arc::new(
        Matcher::build(
            query,
            SearchMode {
                use_regex: true,
                case_sensitive: false,
            },
        )
        .unwrap(),
    ))
}

fn find_in(path: &Path, ask: &TextAsk) -> TextContent {
    match inspect_path(path.to_str().unwrap(), ask, &AtomicBool::new(false)) {
        FileRow::Ok(file) => match file.content {
            Content::Text(text) => text,
            other => panic!("expected a text row, got {other:?}"),
        },
        other => panic!("expected an ok row, got {other:?}"),
    }
}

fn hits_of(text: &TextContent) -> &FindHits {
    text.find.as_ref().expect("a find row carries hits")
}

fn write(dir: &TestDir, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    path
}

/// `lines` numbered lines of 25 bytes each; 50,000 of them is past the FullLoad threshold.
fn write_numbered(dir: &TestDir, name: &str, lines: usize) -> PathBuf {
    let mut body = String::with_capacity(lines * 25);
    for n in 1..=lines {
        body.push_str(&format!("line {n:06} padding........\n"));
    }
    write(dir, name, &body)
}

// ── Params ────────────────────────────────────────────────────────────────────

#[test]
fn find_params_build_one_matcher_with_both_flags_defaulting_to_false() {
    let p = parse_params(&json!({ "paths": ["/a.txt"], "find": { "query": "needle" } })).unwrap();
    assert!(
        matches!(p.ask, TextAsk::Find(ref m) if matches!(**m, Matcher::Literal { case_insensitive: true, .. })),
        "{:?}",
        p.ask
    );

    let p = parse_params(&json!({
        "paths": ["/a.txt"], "find": { "query": "nee.le", "regex": true, "caseSensitive": true }
    }))
    .unwrap();
    assert!(matches!(p.ask, TextAsk::Find(ref m) if matches!(**m, Matcher::Regex(_))));

    // No `find` is the window, as before.
    let p = parse_params(&json!({ "paths": ["/a.txt"], "startLine": 3 })).unwrap();
    assert!(matches!(p.ask, TextAsk::Window(WindowOpts { start_line: 3, .. })));
}

#[test]
fn find_params_reject_a_missing_or_empty_query_and_non_boolean_flags() {
    let code = ToolError::invalid_params("").code;
    for bad in [
        json!({ "paths": ["/a.txt"], "find": "needle" }),
        json!({ "paths": ["/a.txt"], "find": {} }),
        json!({ "paths": ["/a.txt"], "find": { "query": "" } }),
        json!({ "paths": ["/a.txt"], "find": { "query": "x", "regex": "yes" } }),
        json!({ "paths": ["/a.txt"], "find": { "query": "x", "caseSensitive": 1 } }),
    ] {
        let err = parse_params(&bad).unwrap_err();
        assert_eq!(err.code, code, "{bad}");
    }
}

#[test]
fn an_invalid_or_cross_line_regex_is_invalid_params_carrying_the_matchers_reason() {
    let code = ToolError::invalid_params("").code;
    let err =
        parse_params(&json!({ "paths": ["/a.txt"], "find": { "query": "(unclosed", "regex": true } })).unwrap_err();
    assert_eq!(err.code, code);
    let expected = Matcher::build(
        "(unclosed",
        SearchMode {
            use_regex: true,
            case_sensitive: false,
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(
        err.message,
        format!("'find.query': {expected}"),
        "the model gets the matcher's own reason"
    );

    let err = parse_params(&json!({ "paths": ["/a.txt"], "find": { "query": "a\\nb", "regex": true } })).unwrap_err();
    assert_eq!(err.code, code, "a cross-line pattern can never match a streamed line");
}

// ── Hits ──────────────────────────────────────────────────────────────────────

#[test]
fn literal_hits_are_grouped_by_line_with_a_match_count_and_the_window_is_omitted() {
    let dir = TestDir::new("find_grouped");
    let path = write(&dir, "notes.txt", "alpha\nneedle needle\r\nbeta\nNeedle at the end\n");

    let text = find_in(&path, &literal("needle"));
    assert!(text.window.is_none(), "find replaces the window");
    let hits = hits_of(&text);
    assert_eq!(hits.total_matches, 3);
    assert_eq!(hits.lines.len(), 2);
    assert_eq!(hits.lines[0].line, 2, "1-based, as startLine");
    assert_eq!(hits.lines[0].matches, 2);
    assert_eq!(hits.lines[0].text, "needle needle", "the trailing \\r is stripped");
    assert_eq!(hits.lines[1].line, 4);
    assert_eq!(hits.lines[1].matches, 1, "case-insensitive by default");
    assert_eq!(hits.lines[1].text, "Needle at the end");
    assert_eq!((hits.returned_lines, hits.truncated), (2, false));
    assert!(!hits.matches_capped);
    assert!(!hits.scan_incomplete);
    assert_eq!(
        hits.bytes_scanned, None,
        "only an incomplete scan reports where it stopped"
    );
    assert_eq!(text.total_lines, Some(5), "known for a small file, find or not");
    assert!(!text.line_numbers_approximate);
}

#[test]
fn case_sensitive_and_regex_queries_map_onto_the_viewers_search_mode() {
    let dir = TestDir::new("find_modes");
    let path = write(&dir, "log.txt", "ERROR one\nerror two\nwarn 12345\nerr 7\n");

    let sensitive = TextAsk::Find(Arc::new(
        Matcher::build(
            "error",
            SearchMode {
                use_regex: false,
                case_sensitive: true,
            },
        )
        .unwrap(),
    ));
    let hits = hits_of(&find_in(&path, &sensitive)).clone();
    assert_eq!(hits.lines.iter().map(|l| l.line).collect::<Vec<_>>(), [2]);

    let hits = hits_of(&find_in(&path, &regex(r"\d{3,}"))).clone();
    assert_eq!(hits.lines.iter().map(|l| l.line).collect::<Vec<_>>(), [3]);
    assert_eq!(hits.lines[0].text, "warn 12345");
}

#[test]
fn matching_lines_are_capped_at_max_find_lines_while_total_matches_stays_honest() {
    let dir = TestDir::new("find_line_cap");
    let matching_lines = MAX_FIND_LINES + 10;
    let mut body = String::new();
    for n in 0..matching_lines {
        body.push_str(&format!("{n}: needle and needle again\n"));
        body.push_str("filler\n");
    }
    let path = write(&dir, "many.txt", &body);

    let hits = hits_of(&find_in(&path, &literal("needle"))).clone();
    assert_eq!(
        hits.total_matches,
        matching_lines * 2,
        "every match counted, not just the carried lines"
    );
    assert_eq!(hits.lines.len(), MAX_FIND_LINES);
    assert_eq!(hits.returned_lines, MAX_FIND_LINES);
    assert!(hits.truncated, "matching lines exist past the carried ones");
    assert!(!hits.matches_capped, "the viewer's match cap was not hit");
    assert_eq!(hits.lines[0].line, 1);
    assert_eq!(
        hits.lines[MAX_FIND_LINES - 1].line,
        MAX_FIND_LINES * 2 - 1,
        "every other line matches"
    );
}

#[test]
fn a_corpus_past_the_viewers_match_cap_reports_matches_capped() {
    // The shape of `search_cancel_test_support::many_matches_corpus`: 1,000 matches on each
    // of 1,000 lines, two orders of magnitude past `MAX_SEARCH_MATCHES`.
    let dir = TestDir::new("find_match_cap");
    let path = write(&dir, "aaa.txt", &("a".repeat(1_000) + "\n").repeat(1_000));

    let hits = hits_of(&find_in(&path, &literal("a"))).clone();
    assert_eq!(hits.total_matches, MAX_SEARCH_MATCHES);
    assert!(hits.matches_capped, "the model must know the count is a floor");
    assert_eq!(
        hits.lines.len(),
        MAX_SEARCH_MATCHES / 1_000,
        "the cap landed after ten full lines"
    );
    assert!(hits.lines.iter().all(|l| l.matches == 1_000));
    assert!(!hits.truncated, "every matching line the scan saw is carried");
    assert!(
        !hits.scan_incomplete,
        "stopping at the cap is the cap's doing, not an incomplete scan"
    );
}

// ── The snippet ───────────────────────────────────────────────────────────────

#[test]
fn snippet_keeps_a_short_line_whole_and_cuts_a_long_one_around_the_match() {
    assert_eq!(snippet_around("short line", 6, 300), "short line");

    let line = format!("{}needle{}", "x".repeat(1_000), "y".repeat(1_000));
    let snippet = snippet_around(&line, 1_000, 300);
    assert!(snippet.contains("needle"), "{snippet}");
    assert!(snippet.starts_with('…') && snippet.ends_with('…'), "{snippet}");
    assert_eq!(snippet.chars().count(), 300 + 2, "the cap plus the two cut marks");
    assert!(
        snippet.find("needle").unwrap() < snippet.len() / 2,
        "the match sits in the front half: what follows it is usually the evidence"
    );

    // A match near the end: the window slides back so the snippet stays full.
    let line = format!("{}needle", "x".repeat(1_000));
    let snippet = snippet_around(&line, 1_000, 300);
    assert!(snippet.ends_with("needle"), "{snippet}");
    assert!(!snippet.ends_with('…'));
    assert_eq!(snippet.chars().count(), 301);

    // A match at the start: no leading cut mark.
    let line = format!("needle{}", "x".repeat(1_000));
    let snippet = snippet_around(&line, 0, 300);
    assert!(snippet.starts_with("needle"), "{snippet}");
    assert!(snippet.ends_with('…'));
}

#[test]
fn snippet_finds_the_match_by_its_utf16_column_on_a_line_full_of_emoji() {
    // Each emoji is one char, two UTF-16 units, four bytes. Reading the column as a char
    // index would land 400 chars past the match, in the trailing emoji, and cut a snippet
    // without the match in it.
    let dir = TestDir::new("find_emoji");
    let line = format!("{}needle{}", "😀".repeat(400), "😀".repeat(400));
    let path = write(&dir, "emoji.txt", &format!("{line}\n"));

    let hits = hits_of(&find_in(&path, &literal("needle"))).clone();
    assert_eq!(hits.lines.len(), 1);
    let text = &hits.lines[0].text;
    assert!(text.contains("needle"), "{text}");
    assert!(
        text.chars().count() <= FIND_SNIPPET_CHARS + 2,
        "{}",
        text.chars().count()
    );
    assert!(text.starts_with('…') && text.ends_with('…'));
}

// ── Cancel and the backends ───────────────────────────────────────────────────

#[test]
fn a_pre_set_cancel_flag_makes_the_scan_incomplete_and_says_where_it_stopped() {
    let dir = TestDir::new("find_cancelled");
    let small = write(&dir, "small.txt", "needle\n");
    let text = match inspect_path(small.to_str().unwrap(), &literal("needle"), &AtomicBool::new(true)) {
        FileRow::Ok(file) => match file.content {
            Content::Text(text) => text,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    let hits = hits_of(&text);
    assert!(hits.scan_incomplete);
    assert_eq!(hits.total_matches, 0, "nothing was scanned, so nothing was found");
    assert_eq!(hits.bytes_scanned, Some(0));
    assert_eq!(hits.total_bytes, Some(7));
    assert_eq!(hits.total_bytes_human.as_deref(), Some("7 B"));
    assert!(!hits.matches_capped);

    // Past the FullLoad threshold the streaming backend runs; the flag stops it just the same.
    let large = write_numbered(&dir, "large.txt", 50_000);
    let text = match inspect_path(large.to_str().unwrap(), &literal("line 030000"), &AtomicBool::new(true)) {
        FileRow::Ok(file) => match file.content {
            Content::Text(text) => text,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    let hits = hits_of(&text);
    assert!(hits.scan_incomplete);
    assert_eq!(hits.bytes_scanned, Some(0));
    assert!(hits.total_bytes.unwrap() > 1_000_000);
}

#[test]
fn find_on_a_large_file_numbers_lines_exactly_without_building_the_index() {
    let dir = TestDir::new("find_large");
    let path = write_numbered(&dir, "large.txt", 50_000);

    let text = find_in(&path, &literal("line 030000"));
    let hits = hits_of(&text);
    assert_eq!(hits.lines.len(), 1);
    assert_eq!(hits.lines[0].line, 30_000, "the streaming scan counts lines exactly");
    assert_eq!(hits.lines[0].text, "line 030000 padding........");
    assert!(
        !text.line_numbers_approximate,
        "nothing in a find row is estimated, even without an index"
    );
    assert_eq!(
        text.total_lines, None,
        "no index was built, so the line count is honestly unknown"
    );
}

#[test]
fn find_hits_stay_exact_on_the_byte_seek_fallback_the_window_would_call_approximate() {
    // Open the way a deadline-cancelled window read ends up (ByteSeek), then search with a
    // clear flag: the scan streams from byte 0 and numbers lines as it goes, so the hit's
    // line is exact and its text is fetched by byte offset, never by the 80-bytes-a-line guess.
    let dir = TestDir::new("find_fallback");
    let path = write_numbered(&dir, "large.txt", 50_000);
    let cancel = AtomicBool::new(true);
    let opened = open_text_backend(&path, crate::file_viewer::FileEncoding::Utf8, &cancel).unwrap();
    assert!(!opened.line_numbers_exact, "the pre-set flag forced the fallback");
    cancel.store(false, Ordering::Relaxed);

    let matcher = Matcher::build(
        "line 04999",
        SearchMode {
            use_regex: false,
            case_sensitive: true,
        },
    )
    .unwrap();
    let hits = find_hits(opened.backend.as_ref(), &matcher, &cancel).unwrap();
    assert_eq!(hits.total_matches, 10, "049990 through 049999");
    assert_eq!(hits.lines[0].line, 49_990);
    assert_eq!(hits.lines[0].text, "line 049990 padding........");
    assert_eq!(hits.lines[9].line, 49_999);
    assert!(!hits.scan_incomplete);
}

// ── The call ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn one_find_searches_every_text_path_in_the_call_and_leaves_other_kinds_alone() {
    let dir = TestDir::new("find_two_paths");
    let a = write(&dir, "a.txt", "tenant: acme\nrent due\n");
    let b = write(&dir, "b.txt", "nothing here\n");
    let c = write(&dir, "c.txt", "the Tenant moved out\n");
    let png = dir.join("p.png");
    image::RgbaImage::new(1, 1).save(&png).unwrap();

    let paths: Vec<String> = [a, b, c, png]
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let result = run(&json!({ "paths": paths, "find": { "query": "tenant" } }))
        .await
        .unwrap();
    let json = serde_json::to_value(&result).unwrap();
    assert_text_only(&json, "result");
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 4);
    assert_eq!(files[0]["content"]["find"]["totalMatches"], 1);
    assert_eq!(files[0]["content"]["find"]["lines"][0]["line"], 1);
    assert_eq!(files[0]["content"]["find"]["lines"][0]["text"], "tenant: acme");
    assert_eq!(files[1]["content"]["find"]["totalMatches"], 0);
    assert_eq!(files[1]["content"]["find"]["lines"], json!([]));
    assert_eq!(files[2]["content"]["find"]["totalMatches"], 1);
    for file in files.iter().take(3) {
        assert!(file["content"].get("window").is_none(), "{file}");
        assert!(
            file["content"]["find"].get("matchesCapped").is_none(),
            "false flags stay off the wire"
        );
        assert!(file["content"]["find"].get("scanIncomplete").is_none());
    }
    assert_eq!(
        files[3]["content"]["kind"], "image",
        "find leaves a non-text row as it was"
    );
    assert!(files[3]["content"].get("find").is_none());
}

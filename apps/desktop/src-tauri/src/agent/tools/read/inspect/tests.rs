//! Unit tests for `inspect_file`: params, kinds, the window shaper, the runner's timeout
//! policy, the size contract, and the text-only DTO walk.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use super::runner::{RunnerConfig, run_paths};
use super::text::window_from_chunk;
use super::*;
use crate::file_viewer::LineChunk;
use crate::test_support::TestDir;

/// A fresh window request: the defaults the tool uses when the caller says nothing.
fn opts(start_line: usize, max_lines: usize) -> WindowOpts {
    WindowOpts { start_line, max_lines }
}

fn inspect(path: &Path) -> FileRow {
    inspect_path(path.to_str().unwrap(), opts(1, 200), &AtomicBool::new(false))
}

fn inspect_with(path: &Path, window: WindowOpts, cancel: &AtomicBool) -> FileRow {
    inspect_path(path.to_str().unwrap(), window, cancel)
}

fn file_of(row: &FileRow) -> &InspectedFile {
    match row {
        FileRow::Ok(file) => file,
        other => panic!("expected an ok row, got {other:?}"),
    }
}

fn text_of(row: &FileRow) -> &TextContent {
    match &file_of(row).content {
        Content::Text(t) => t,
        other => panic!("expected a text row, got {other:?}"),
    }
}

fn content_of(row: &FileRow) -> &Content {
    &file_of(row).content
}

/// The checked-in encoding fixtures the viewer's own tests use.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../test/fixtures/encodings")
        .join(name)
}

/// `lines` numbered lines of 25 bytes each: 360,000 of them is 9 MB.
fn write_numbered(dir: &TestDir, name: &str, lines: usize) -> PathBuf {
    let path = dir.join(name);
    let mut body = String::with_capacity(lines * 25);
    for n in 1..=lines {
        body.push_str(&format!("line {n:06} padding........\n"));
    }
    std::fs::write(&path, body).unwrap();
    path
}

fn chunk(lines: &[&str], first_line_number: usize, total_lines: Option<usize>) -> LineChunk {
    LineChunk {
        lines: lines.iter().map(|l| l.to_string()).collect(),
        first_line_number,
        byte_offset: 0,
        total_lines,
        total_bytes: 0,
    }
}

// ── Params ────────────────────────────────────────────────────────────────────

#[test]
fn params_default_the_window_and_cap_max_lines() {
    let p = parse_params(&json!({ "paths": ["/a/b.txt"] })).unwrap();
    assert_eq!(p.paths, vec!["/a/b.txt".to_string()]);
    assert_eq!(p.window, opts(1, DEFAULT_MAX_LINES));

    let p = parse_params(&json!({ "paths": ["/a/b.txt"], "startLine": 812, "maxLines": 999_999 })).unwrap();
    assert_eq!(
        p.window,
        opts(812, MAX_MAX_LINES),
        "an oversized ask is clamped, visibly"
    );

    assert!(
        parse_params(&json!({ "paths": ["/x"], "startLine": 0 })).is_err(),
        "lines are 1-based"
    );
    assert!(parse_params(&json!({ "paths": ["/x"], "maxLines": 0 })).is_err());
    assert!(parse_params(&json!({ "paths": ["/x"], "maxLines": "ten" })).is_err());
}

#[test]
fn params_require_a_non_empty_bounded_list_of_paths() {
    assert!(parse_params(&json!({})).is_err());
    assert!(
        parse_params(&json!({ "paths": "/a.txt" })).is_err(),
        "a lone string is not a list"
    );
    assert!(parse_params(&json!({ "paths": [] })).is_err());
    assert!(parse_params(&json!({ "paths": ["  "] })).is_err());
    assert!(parse_params(&json!({ "paths": [1] })).is_err());

    // Over the cap is a hard param error, not a silent cut: a caller asking about 201
    // files must not believe it got answers for all of them.
    let too_many: Vec<String> = (0..=MAX_PATHS).map(|i| format!("/f-{i}.txt")).collect();
    let err = parse_params(&json!({ "paths": too_many })).unwrap_err();
    assert_eq!(
        err.code,
        ToolError::invalid_params("").code,
        "a hard INVALID_PARAMS, never a silent cut"
    );
    let just_enough: Vec<String> = (0..MAX_PATHS).map(|i| format!("/f-{i}.txt")).collect();
    assert!(parse_params(&json!({ "paths": just_enough })).is_ok());
}

#[test]
fn params_expand_a_leading_tilde_and_trim() {
    let p = parse_params(&json!({ "paths": ["~/Documents/a.txt", "  /b.txt "] })).unwrap();
    assert!(
        !p.paths[0].starts_with('~'),
        "a literal ~ never names a file: {}",
        p.paths[0]
    );
    assert!(p.paths[0].ends_with("/Documents/a.txt"));
    assert_eq!(p.paths[1], "/b.txt");
}

// ── Kinds ─────────────────────────────────────────────────────────────────────

#[test]
fn a_utf16_le_file_with_a_bom_is_text_with_its_encoding_named() {
    // The v1 regression: a UTF-8-only text test read every UTF-16 file as binary.
    let row = inspect(&fixture("utf16-le-bom.txt"));
    let text = text_of(&row);
    assert_eq!(text.encoding, "UTF-16 LE");
    assert!(
        text.window.content.starts_with("hello world\nsecond line"),
        "{:?}",
        text.window.content
    );
    assert!(!text.line_numbers_approximate);

    // And the same without leaning on a checked-in file.
    let dir = TestDir::new("inspect_utf16");
    let path = dir.join("be.txt");
    let mut bytes = vec![0xFE, 0xFF];
    for ch in "grüße\nzwei".encode_utf16() {
        bytes.extend_from_slice(&ch.to_be_bytes());
    }
    std::fs::write(&path, bytes).unwrap();
    let text = text_of(&inspect(&path)).clone();
    assert_eq!(text.encoding, "UTF-16 BE");
    assert_eq!(text.window.content, "grüße\nzwei");
    assert_eq!(text.total_lines, Some(2));
}

#[test]
fn a_windows_1252_file_is_text_with_its_encoding_named() {
    let row = inspect(&fixture("windows-1252.txt"));
    let text = text_of(&row);
    assert_eq!(text.encoding, "Western (Windows-1252)");
    assert_eq!(text.window.content, "café\nnaïveté\n");
}

#[test]
fn nul_bytes_make_a_file_binary_and_an_svg_stays_text() {
    let dir = TestDir::new("inspect_kinds");
    let blob = dir.join("blob.dat");
    std::fs::write(&blob, [0u8, 1, 2, 3, 0xFF]).unwrap();
    assert_eq!(content_of(&inspect(&blob)), &Content::Binary {});

    // The viewer renders `.svg` as an image; the model gets more from its markup.
    let svg = dir.join("icon.svg");
    std::fs::write(&svg, "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>\n").unwrap();
    let row = inspect(&svg);
    let text = text_of(&row);
    assert!(text.window.content.starts_with("<svg"));
    assert_eq!(file_of(&row).mime.as_deref(), Some("image/svg+xml"));
}

#[test]
fn an_empty_file_is_empty_and_a_pdf_is_binary() {
    let dir = TestDir::new("inspect_empty_pdf");
    let empty = dir.join("nothing.txt");
    std::fs::write(&empty, b"").unwrap();
    let file = file_of(&inspect(&empty)).clone();
    assert_eq!(file.content, Content::Empty {});
    assert_eq!((file.size_bytes, file.size_human.as_str()), (0, "0 B"));

    let pdf = dir.join("doc.pdf");
    std::fs::write(&pdf, b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n%%EOF\n").unwrap();
    assert_eq!(content_of(&inspect(&pdf)), &Content::Binary {});
}

#[test]
fn a_png_wearing_a_txt_extension_shows_both_the_guess_and_the_truth() {
    let dir = TestDir::new("inspect_lying_ext");
    let path = dir.join("notes.txt");
    image::RgbaImage::new(3, 2)
        .save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    let file = file_of(&inspect(&path)).clone();
    assert_eq!(
        file.mime.as_deref(),
        Some("text/plain"),
        "the extension's guess stays visible"
    );
    assert_eq!(file.extension.as_deref(), Some("txt"));
    let Content::Image(img) = &file.content else {
        panic!("expected an image row, got {:?}", file.content);
    };
    assert_eq!(img.format, "image/png", "the bytes decide the kind");
    assert_eq!((img.width, img.height), (Some(3), Some(2)));
    assert!(img.hint.contains("image_facts"));
}

#[test]
fn an_ok_row_carries_spoken_size_and_date_beside_the_raw_values() {
    let dir = TestDir::new("inspect_meta");
    let path = dir.join("a.md");
    std::fs::write(&path, "# Title\n").unwrap();
    let file = file_of(&inspect(&path)).clone();
    assert_eq!(file.name, "a.md");
    assert_eq!(file.extension.as_deref(), Some("md"));
    assert_eq!((file.size_bytes, file.size_human.as_str()), (8, "8 B"));
    assert_eq!(file.mime.as_deref(), Some("text/markdown"));
    let modified = file.modified.expect("a fresh file has an mtime");
    let human = file.modified_human.expect("and its spoken twin");
    assert!(modified.starts_with(&human), "{modified} should begin with {human}");
    assert!(modified.ends_with('Z'), "RFC 3339 UTC: {modified}");
}

// ── The window shaper ─────────────────────────────────────────────────────────

#[test]
fn window_joins_lines_and_strips_a_trailing_cr_per_line() {
    let lf = window_from_chunk(&chunk(&["a", "b", "c"], 0, Some(3)), opts(1, 3), 100, 100);
    assert_eq!(lf.content, "a\nb\nc");
    assert_eq!(
        (lf.start_line, lf.returned_lines, lf.truncated, lf.lines_cut),
        (1, 3, false, false)
    );

    let crlf = window_from_chunk(&chunk(&["a\r", "b\r", "c"], 0, Some(3)), opts(1, 3), 100, 100);
    assert_eq!(
        crlf.content, "a\nb\nc",
        "the backends keep \\r on CRLF files; the model shouldn't see it"
    );
}

#[test]
fn window_reports_more_lines_exactly_via_the_extra_fetched_line() {
    // The shaper is handed `max_lines + 1` lines: a full extra line means more exist.
    let more = window_from_chunk(&chunk(&["a", "b", "c"], 0, None), opts(1, 2), 100, 100);
    assert_eq!(more.content, "a\nb");
    assert!(more.truncated);
    let last = window_from_chunk(&chunk(&["b", "c"], 1, Some(3)), opts(2, 2), 100, 100);
    assert_eq!(last.content, "b\nc");
    assert_eq!(last.start_line, 2);
    assert!(!last.truncated, "the tail of the file is not truncated");
}

#[test]
fn window_past_eof_is_empty_and_not_truncated() {
    // Exact backends clamp a past-the-end target to the last line; the shaper must not
    // present that last line as line 50.
    let clamped = window_from_chunk(&chunk(&["last"], 2, Some(3)), opts(50, 10), 100, 100);
    assert_eq!(clamped.returned_lines, 0);
    assert_eq!(clamped.content, "");
    assert_eq!(clamped.start_line, 50);
    assert!(!clamped.truncated);
    // The ByteSeek fallback has no total; an empty chunk is its EOF.
    let empty = window_from_chunk(&chunk(&[], 0, None), opts(50, 10), 100, 100);
    assert_eq!((empty.returned_lines, empty.truncated), (0, false));
}

#[test]
fn window_char_cap_stops_the_join_and_flags_truncation() {
    let w = window_from_chunk(&chunk(&["aaaa", "bbbb", "cccc"], 0, Some(3)), opts(1, 3), 9, 100);
    assert_eq!(w.content, "aaaa\nbbbb", "4 + 1 + 4 = 9 fits; the next line would not");
    assert_eq!(w.returned_lines, 2);
    assert!(w.truncated);
    assert!(!w.lines_cut);
}

#[test]
fn window_cuts_a_huge_line_at_the_line_cap_and_says_so() {
    let long = "x".repeat(5_000);
    let w = window_from_chunk(
        &chunk(&[&long, "short"], 0, Some(2)),
        opts(1, 2),
        MAX_WINDOW_CHARS,
        MAX_LINE_CHARS,
    );
    assert!(
        w.lines_cut,
        "a minified bundle is one line; the model must know it was cut"
    );
    assert_eq!(w.content.len(), MAX_LINE_CHARS + 1 + "short".len());
    assert_eq!(w.returned_lines, 2);
    assert!(!w.truncated);

    // Cuts fall on char boundaries, never inside a codepoint.
    let emoji = "😀".repeat(10);
    let w = window_from_chunk(&chunk(&[&emoji], 0, Some(1)), opts(1, 1), 100, 3);
    assert_eq!(w.content, "😀😀😀");
    assert!(w.lines_cut);
}

// ── The backends behind the window ────────────────────────────────────────────

#[test]
fn total_lines_is_known_for_a_small_file_and_absent_on_the_byte_seek_fallback() {
    let dir = TestDir::new("inspect_total_lines");
    let small = write_numbered(&dir, "small.txt", 5);
    let text = text_of(&inspect(&small)).clone();
    assert_eq!(
        text.total_lines,
        Some(6),
        "5 lines plus the trailing empty line, as the viewer counts"
    );
    assert!(!text.line_numbers_approximate);

    // Past 1 MB the line index is built under the cancel flag; pre-setting it drives the
    // ByteSeek fallback, which must still answer and must say its numbers are estimates.
    let large = write_numbered(&dir, "large.txt", 50_000);
    let text = text_of(&inspect_with(&large, opts(1, 3), &AtomicBool::new(true))).clone();
    assert_eq!(text.total_lines, None);
    assert!(text.line_numbers_approximate);
    assert!(text.window.content.starts_with("line 000001"));
    assert_eq!(text.window.returned_lines, 3);
    assert!(text.window.truncated);
}

#[test]
fn a_nine_megabyte_file_reads_from_the_requested_line_and_the_last_page_is_not_truncated() {
    // The v1 bug: past 8 MB the read seeked to 0 while the result claimed the offset,
    // and `truncated` was unconditionally true.
    let dir = TestDir::new("inspect_9mb");
    let lines = 360_000;
    let path = write_numbered(&dir, "big.log", lines);
    assert!(std::fs::metadata(&path).unwrap().len() >= 9 * 1024 * 1024);

    let text = text_of(&inspect_with(&path, opts(100_000, 3), &AtomicBool::new(false))).clone();
    assert_eq!(text.window.start_line, 100_000);
    assert!(
        text.window.content.starts_with("line 100000 padding"),
        "{:?}",
        text.window.content
    );
    assert_eq!(text.window.returned_lines, 3);
    assert!(text.window.truncated);
    assert_eq!(text.total_lines, Some(lines + 1));
    assert!(
        !text.line_numbers_approximate,
        "a 9 MB scan finishes well inside the budget"
    );

    // The last page: the final numbered line and no more. (The line index counts the
    // trailing empty line after the final newline in `total_lines` but never returns it
    // from `get_lines`, unlike FullLoad; the window follows what the backend hands over.)
    let text = text_of(&inspect_with(&path, opts(lines, 200), &AtomicBool::new(false))).clone();
    assert_eq!(text.window.start_line, lines);
    assert_eq!(text.window.content, "line 360000 padding........");
    assert_eq!(text.window.returned_lines, 1);
    assert!(!text.window.truncated);

    // And past the end: empty, not truncated, no phantom last line.
    let text = text_of(&inspect_with(&path, opts(lines + 5, 200), &AtomicBool::new(false))).clone();
    assert_eq!((text.window.returned_lines, text.window.truncated), (0, false));
    assert_eq!(text.window.start_line, lines + 5);
}

// ── Statuses ──────────────────────────────────────────────────────────────────

#[test]
fn folder_missing_and_virtual_paths_answer_typed_statuses() {
    let dir = TestDir::new("inspect_statuses");
    assert!(matches!(inspect(&dir), FileRow::Folder { .. }));
    assert!(matches!(inspect(&dir.join("nope.txt")), FileRow::Missing { .. }));
    // A file used as a directory component: nothing is at that path either.
    let file = dir.join("f.txt");
    std::fs::write(&file, "x").unwrap();
    assert!(matches!(inspect(&file.join("under.txt")), FileRow::Missing { .. }));

    // `mtp://` and direct `smb://` have no local byte path; `missing` would be a lie.
    let cancel = AtomicBool::new(false);
    assert!(matches!(
        inspect_path("mtp://phone/DCIM/a.jpg", opts(1, 1), &cancel),
        FileRow::UnsupportedVolume { .. }
    ));
    assert!(matches!(
        inspect_path("smb://nas/share/a.txt", opts(1, 1), &cancel),
        FileRow::UnsupportedVolume { .. }
    ));
}

#[cfg(unix)]
#[test]
fn a_file_without_read_permission_is_unreadable_for_permission() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new("inspect_perm");
    let path = dir.join("secret.txt");
    std::fs::write(&path, "hidden\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&path).is_ok() {
        // Root reads anything; the case can't be produced here.
        return;
    }
    assert!(matches!(
        inspect(&path),
        FileRow::Unreadable {
            reason: UnreadableReason::Permission,
            ..
        }
    ));
}

// ── The runner ────────────────────────────────────────────────────────────────

fn quick_config() -> RunnerConfig {
    RunnerConfig {
        path_timeout: Duration::from_millis(60),
        cancel_grace: Duration::from_millis(30),
        call_timeout: Duration::from_secs(5),
        concurrency: 4,
    }
}

fn real_inspect() -> InspectFn {
    Arc::new(|path, cancel| inspect_path(path, opts(1, 200), cancel))
}

#[tokio::test]
async fn rows_come_back_in_request_order_with_a_typed_status_each() {
    let dir = TestDir::new("inspect_runner_order");
    let text = dir.join("a.txt");
    std::fs::write(&text, "hello\n").unwrap();
    let paths = vec![
        dir.join("missing.txt").to_string_lossy().into_owned(),
        text.to_string_lossy().into_owned(),
        dir.to_string_lossy().into_owned(),
    ];
    let rows = run_paths(&paths, &RUNNER, real_inspect()).await;
    assert_eq!(rows.len(), 3);
    assert!(matches!(rows[0], Some(FileRow::Missing { .. })));
    assert!(matches!(rows[1], Some(FileRow::Ok(_))));
    assert!(matches!(rows[2], Some(FileRow::Folder { .. })));
    for (row, path) in rows.iter().zip(&paths) {
        assert_eq!(row.as_ref().unwrap().path(), path, "a row answers for its own path");
    }
}

#[tokio::test]
async fn an_expired_call_deadline_launches_nothing_and_leaves_every_path_unanswered() {
    let paths: Vec<String> = (0..3).map(|i| format!("/nowhere/{i}.txt")).collect();
    let cfg = RunnerConfig {
        call_timeout: Duration::ZERO,
        ..quick_config()
    };
    let rows = run_paths(&paths, &cfg, real_inspect()).await;
    assert!(rows.iter().all(Option::is_none), "{rows:?}");

    let InspectResult::Ok {
        files,
        total,
        returned,
        truncated,
        unanswered,
    } = shape_ok(&paths, rows);
    assert!(files.is_empty());
    assert_eq!((total, returned, truncated), (3, 0, true));
    assert_eq!(unanswered, paths, "the model can ask again for exactly these");
}

#[tokio::test]
async fn a_read_that_ignores_the_cancel_flag_is_abandoned_as_unreachable() {
    // A read stuck in a syscall never sees the flag. Stand in for it with a receive that
    // the test only releases after the verdict, so the parked thread can't outlive the
    // runtime (its drop would wait for the blocking pool).
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let release_rx = std::sync::Mutex::new(release_rx);
    let wedged: InspectFn = Arc::new(move |path, _cancel| {
        let _ = release_rx.lock().unwrap().recv();
        FileRow::Missing { path: path.to_string() }
    });
    let paths = vec!["/dead/mount/a.txt".to_string()];
    let rows = run_paths(&paths, &quick_config(), wedged).await;
    assert!(matches!(&rows[0], Some(FileRow::Unreachable { path }) if path == "/dead/mount/a.txt"));
    drop(release_tx);
}

#[tokio::test]
async fn a_read_that_honours_the_cancel_flag_hands_back_its_partial_row() {
    // A slow-but-alive read (a line index still scanning) stops on the flag and answers
    // with an approximate window rather than nothing.
    let cooperative: InspectFn = Arc::new(|path, cancel| {
        while !cancel.load(Ordering::Relaxed) {
            std::thread::yield_now();
        }
        FileRow::Ok(Box::new(InspectedFile {
            path: path.to_string(),
            name: "slow.log".into(),
            extension: Some("log".into()),
            size_bytes: 1,
            size_human: "1 B".into(),
            modified: None,
            modified_human: None,
            mime: None,
            content: Content::Text(TextContent {
                encoding: "UTF-8".into(),
                total_lines: None,
                line_numbers_approximate: true,
                window: TextWindow {
                    start_line: 1,
                    returned_lines: 0,
                    content: String::new(),
                    truncated: false,
                    lines_cut: false,
                },
            }),
        }))
    });
    let paths = vec!["/slow/mount/slow.log".to_string()];
    let rows = run_paths(&paths, &quick_config(), cooperative).await;
    let text = text_of(rows[0].as_ref().unwrap());
    assert!(
        text.line_numbers_approximate,
        "the partial answer is flagged, not silent"
    );
}

// ── The size contract ─────────────────────────────────────────────────────────

/// One text row carrying a full window: the densest row this tool produces.
fn dense_row(index: usize) -> FileRow {
    FileRow::Ok(Box::new(InspectedFile {
        path: format!("/logs/app-{index:03}.log"),
        name: format!("app-{index:03}.log"),
        extension: Some("log".into()),
        size_bytes: 1_000_000,
        size_human: "976.6 KB".into(),
        modified: Some("2026-09-02T08:00:00Z".into()),
        modified_human: Some("2026-09-02".into()),
        mime: None,
        content: Content::Text(TextContent {
            encoding: "UTF-8".into(),
            total_lines: Some(20_000),
            line_numbers_approximate: false,
            window: TextWindow {
                start_line: 1,
                returned_lines: 200,
                content: "y".repeat(MAX_WINDOW_CHARS),
                truncated: true,
                lines_cut: false,
            },
        }),
    }))
}

#[test]
fn two_hundred_dense_rows_page_and_unanswered_names_exactly_the_rows_not_carried() {
    use crate::agent::chat::budget::{MAX_TOOL_RESULT_TOKENS, estimate_serialized_tokens};

    let paths: Vec<String> = (0..MAX_PATHS).map(|i| format!("/logs/app-{i:03}.log")).collect();
    let rows: Vec<Option<FileRow>> = (0..MAX_PATHS).map(|i| Some(dense_row(i))).collect();
    let InspectResult::Ok {
        files,
        total,
        returned,
        truncated,
        unanswered,
    } = shape_ok(&paths, rows);

    assert_eq!(total, MAX_PATHS, "the count asked about is reported in full");
    assert_eq!(returned, files.len());
    assert!(truncated, "the cut must be visible to the model");
    assert!((1..MAX_PATHS).contains(&returned), "returned {returned}");
    assert_eq!(files[0].path(), "/logs/app-000.log", "rows stay in request order");
    assert_eq!(
        unanswered,
        paths[returned..].to_vec(),
        "exactly the rows not carried, in order"
    );
    let spent: usize = files.iter().map(estimate_serialized_tokens).sum();
    assert!(
        spent <= MAX_TOOL_RESULT_TOKENS,
        "the answer must fit the ceiling (spent {spent})"
    );
}

#[test]
fn a_gap_left_by_an_unlaunched_path_is_unanswered_while_the_rest_still_answers() {
    let paths: Vec<String> = ["/a.txt", "/b.txt", "/c.txt"].iter().map(|s| s.to_string()).collect();
    let rows = vec![
        Some(FileRow::Missing { path: "/a.txt".into() }),
        None,
        Some(FileRow::Folder { path: "/c.txt".into() }),
    ];
    let InspectResult::Ok {
        files,
        returned,
        truncated,
        unanswered,
        ..
    } = shape_ok(&paths, rows);
    assert_eq!(files.len(), 2);
    assert_eq!((returned, truncated), (2, true));
    assert_eq!(unanswered, vec!["/b.txt".to_string()]);
}

#[test]
fn a_call_that_fits_is_answered_whole_and_carries_no_unanswered_key() {
    let paths = vec!["/a.txt".to_string()];
    let json = serde_json::to_value(shape_ok(&paths, vec![Some(FileRow::Missing { path: "/a.txt".into() })])).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["total"], 1);
    assert_eq!(json["returned"], 1);
    assert_eq!(json["truncated"], false);
    assert!(
        json.get("unanswered").is_none(),
        "an empty list doesn't clutter the payload"
    );
    assert_eq!(json["files"][0]["status"], "missing");
}

// ── Text-only by construction ─────────────────────────────────────────────────

/// Every leaf of `value` is a string, a number, or a flag: nowhere for bytes to hide.
fn assert_text_only(value: &Value, at: &str) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                assert_text_only(v, &format!("{at}.{key}"));
            }
        }
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                assert_text_only(v, &format!("{at}[{i}]"));
            }
        }
        Value::String(_) | Value::Number(_) | Value::Bool(_) => {}
        Value::Null => panic!("{at} is null: optionals must be skipped, not nulled"),
    }
}

#[test]
fn every_row_shape_is_text_only_no_byte_fields() {
    let dir = TestDir::new("inspect_text_only");
    let text = dir.join("t.txt");
    std::fs::write(&text, "a\r\nb\n").unwrap();
    let png = dir.join("p.png");
    image::RgbaImage::new(1, 1).save(&png).unwrap();
    let bin = dir.join("b.bin");
    std::fs::write(&bin, [0u8, 1]).unwrap();
    let empty = dir.join("e");
    std::fs::write(&empty, b"").unwrap();

    let cancel = AtomicBool::new(false);
    let rows = vec![
        Some(inspect(&text)),
        Some(inspect(&png)),
        Some(inspect(&bin)),
        Some(inspect(&empty)),
        Some(inspect(&dir)),
        Some(inspect(&dir.join("missing"))),
        Some(inspect_path("mtp://x/y", opts(1, 1), &cancel)),
        Some(FileRow::Unreachable { path: "/u".into() }),
        Some(FileRow::Unreadable {
            path: "/r".into(),
            reason: UnreadableReason::Permission,
        }),
    ];
    let paths: Vec<String> = rows.iter().map(|r| r.as_ref().unwrap().path().to_string()).collect();
    let json = serde_json::to_value(shape_ok(&paths, rows)).unwrap();
    assert_text_only(&json, "result");

    let kinds: Vec<&str> = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .take(4)
        .map(|f| f["content"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["text", "image", "binary", "empty"]);
    // FullLoad returns the trailing empty line after the final newline, as the viewer shows it.
    assert_eq!(json["files"][0]["content"]["window"]["content"], "a\nb\n");
    assert_eq!(json["files"][0]["content"]["window"]["returnedLines"], 3);
    assert_eq!(json["files"][6]["status"], "unsupportedVolume");
    assert_eq!(json["files"][8]["reason"], "permission");
}

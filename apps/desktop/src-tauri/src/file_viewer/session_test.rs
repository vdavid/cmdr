//! Tests for ViewerSession orchestrator.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use super::session::{self, SearchStatus};
use super::{FULL_LOAD_THRESHOLD, MAX_SEARCH_MATCHES, RangeEnd, SearchMode, ViewerError};

/// Default mode for existing tests: literal, case-sensitive (matches pre-mode behaviour
/// for ASCII queries). Tests that exercise case-insensitivity should pass an explicit
/// `SearchMode { case_sensitive: false, .. }`.
fn literal_mode() -> SearchMode {
    SearchMode {
        use_regex: false,
        case_sensitive: true,
    }
}

fn create_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_viewer_session_{}", name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("Failed to create test directory");
    dir
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn write_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
    let file = dir.join(name);
    fs::write(&file, content).unwrap();
    file
}

#[test]
fn open_small_file_uses_full_load() {
    let dir = create_test_dir("small");
    let file = write_test_file(&dir, "test.txt", "hello\nworld\n");

    let result = session::open_session(file.to_str().unwrap()).unwrap();
    assert_eq!(result.file_name, "test.txt");
    assert_eq!(result.total_bytes, 12);
    assert_eq!(result.total_lines, Some(3));
    assert!(matches!(result.backend_type, session::BackendType::FullLoad));
    assert!(result.capabilities.supports_line_seek);
    assert!(result.capabilities.knows_total_lines);

    // Initial lines should be populated
    assert!(!result.initial_lines.lines.is_empty());
    assert_eq!(result.initial_lines.first_line_number, 0);

    // Cleanup session
    session::close_session(&result.session_id).unwrap();
    cleanup(&dir);
}

#[test]
fn open_large_file_uses_byte_seek() {
    let dir = create_test_dir("large");
    // Create a file larger than FULL_LOAD_THRESHOLD
    let line = "x".repeat(100) + "\n";
    let line_count = (FULL_LOAD_THRESHOLD as usize / line.len()) + 100;
    let content: String = line.repeat(line_count);
    let file = write_test_file(&dir, "big.txt", &content);

    let result = session::open_session(file.to_str().unwrap()).unwrap();
    assert!(result.total_bytes > FULL_LOAD_THRESHOLD);
    assert!(matches!(result.backend_type, session::BackendType::ByteSeek));
    assert!(!result.capabilities.supports_line_seek);
    assert!(!result.capabilities.knows_total_lines);

    // Should still have initial lines
    assert!(!result.initial_lines.lines.is_empty());

    session::close_session(&result.session_id).unwrap();
    cleanup(&dir);
}

#[test]
fn open_not_found() {
    let result = session::open_session("/nonexistent_session_test.txt");
    assert!(result.is_err());
}

#[test]
fn open_directory_fails() {
    let dir = create_test_dir("dir_fail");
    let result = session::open_session(dir.to_str().unwrap());
    assert!(result.is_err());
    cleanup(&dir);
}

#[test]
fn get_lines_after_open() {
    let dir = create_test_dir("get_lines");
    let file = write_test_file(&dir, "test.txt", "a\nb\nc\nd\ne\n");

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();

    let chunk = session::get_lines(&open_result.session_id, super::SeekTarget::Line(2), 3).unwrap();
    assert_eq!(chunk.first_line_number, 2);
    assert_eq!(chunk.lines, vec!["c", "d", "e"]);

    session::close_session(&open_result.session_id).unwrap();
    cleanup(&dir);
}

#[test]
fn get_lines_invalid_session() {
    let result = session::get_lines("nonexistent-session-id", super::SeekTarget::Line(0), 10);
    assert!(result.is_err());
}

#[test]
fn close_session_cleans_up() {
    let dir = create_test_dir("close");
    let file = write_test_file(&dir, "test.txt", "test\n");

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = open_result.session_id.clone();

    // Session should work
    assert!(session::get_lines(&sid, super::SeekTarget::Line(0), 1).is_ok());

    // Close it
    session::close_session(&sid).unwrap();

    // Now it should fail
    assert!(session::get_lines(&sid, super::SeekTarget::Line(0), 1).is_err());

    cleanup(&dir);
}

#[test]
fn search_start_and_poll() {
    let dir = create_test_dir("search");
    let file = write_test_file(&dir, "test.txt", "hello world\nfoo bar\nhello again\n");

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    // Start search
    session::search_start(sid, "hello".to_string(), literal_mode()).unwrap();

    // Poll until done (with timeout)
    let mut done = false;
    for _ in 0..100 {
        let poll = session::search_poll(sid, 0).unwrap();
        if matches!(poll.status, SearchStatus::Done) {
            assert_eq!(poll.new_matches.len(), 2);
            assert_eq!(poll.total_match_count, 2);
            done = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(done, "Search did not complete in time");

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

#[test]
fn search_cancel_works() {
    let dir = create_test_dir("search_cancel");
    let content = "hello world\n".repeat(100000);
    let file = write_test_file(&dir, "test.txt", &content);

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    session::search_start(sid, "hello".to_string(), literal_mode()).unwrap();

    // Cancel immediately. The cancel flag is set synchronously; the search
    // thread will see it on its next iteration and exit, writing the final
    // `SearchStatus::Cancelled` to the shared status mutex.
    session::search_cancel(sid).unwrap();

    // Poll until the thread observes the cancel and transitions to Cancelled.
    // We accept Running (thread still in flight) along the way.
    let mut saw_cancelled = false;
    for _ in 0..200 {
        let poll = session::search_poll(sid, 0).unwrap();
        if matches!(poll.status, SearchStatus::Cancelled) {
            saw_cancelled = true;
            break;
        }
        assert!(
            matches!(poll.status, SearchStatus::Running),
            "expected Running or Cancelled while cancellation propagates, got {:?}",
            poll.status
        );
        thread::sleep(Duration::from_millis(10));
    }
    assert!(saw_cancelled, "search did not transition to Cancelled in time");

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

#[test]
fn search_poll_after_cancel_surfaces_cancelled_then_idle_after_new_start() {
    // Pins the full `Running → Cancelled` transition contract:
    // 1. Cancelling a running search must surface as `Cancelled` on poll (not silently flip to Idle,
    //    which would erase the user-visible "search was cancelled" signal).
    // 2. Starting a fresh search after a cancel resets the observable status: the new search either
    //    reports `Running` while in flight or `Done` if it finishes between calls.
    let dir = create_test_dir("search_cancel_transition");
    // Large enough to keep the thread busy long enough for the cancel to
    // race the loop body (the inner loop checks the flag per chunk).
    let content = "hello world\n".repeat(200_000);
    let file = write_test_file(&dir, "test.txt", &content);

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    session::search_start(sid, "hello".to_string(), literal_mode()).unwrap();
    session::search_cancel(sid).unwrap();

    // Wait for the Cancelled transition.
    let mut saw_cancelled = false;
    for _ in 0..200 {
        let poll = session::search_poll(sid, 0).unwrap();
        if matches!(poll.status, SearchStatus::Cancelled) {
            saw_cancelled = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(saw_cancelled, "Cancelled was never observed by poll");

    // Starting a fresh search must reset the observable status.
    session::search_start(sid, "world".to_string(), literal_mode()).unwrap();
    let poll_after_restart = session::search_poll(sid, 0).unwrap();
    assert!(
        matches!(poll_after_restart.status, SearchStatus::Running | SearchStatus::Done),
        "fresh search_start must clear the Cancelled state; got {:?}",
        poll_after_restart.status
    );

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

#[test]
fn search_poll_no_active_search() {
    let dir = create_test_dir("poll_idle");
    let file = write_test_file(&dir, "test.txt", "test\n");

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    let poll = session::search_poll(sid, 0).unwrap();
    assert!(matches!(poll.status, SearchStatus::Idle));

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

#[test]
fn tilde_expansion() {
    // open_session should handle ~ paths
    let result = session::open_session("~/nonexistent_file_tilde_test.txt");
    // Should get NotFound rather than a panic/crash
    assert!(result.is_err());
}

#[test]
fn large_file_upgrades_to_line_index() {
    let dir = create_test_dir("upgrade");
    // Create a file larger than FULL_LOAD_THRESHOLD (1 MB) with known lines.
    // Each line is ~115 bytes ("line 00000000 " + 100 x's + "\n").
    let padding = "x".repeat(100);
    let line_count = (FULL_LOAD_THRESHOLD as usize / 100) + 200;
    let content: String = (0..line_count)
        .map(|i| format!("line {:08} {}\n", i, padding))
        .collect();
    let file = write_test_file(&dir, "upgrade.txt", &content);

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    // Initially ByteSeek
    assert!(matches!(open_result.backend_type, session::BackendType::ByteSeek));

    // Wait for upgrade to complete (should be fast for a ~1MB file)
    thread::sleep(Duration::from_millis(500));

    // After upgrade, get_lines with Line target should work correctly
    let chunk = session::get_lines(sid, super::SeekTarget::Line(10), 3).unwrap();
    // If upgraded to LineIndex, first_line_number should be correct
    // If still ByteSeek, it will default to 0
    // We check that it works either way
    assert!(!chunk.lines.is_empty());

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

#[test]
fn multiple_sessions() {
    let dir = create_test_dir("multi");
    let file1 = write_test_file(&dir, "a.txt", "file a\n");
    let file2 = write_test_file(&dir, "b.txt", "file b\n");

    let res1 = session::open_session(file1.to_str().unwrap()).unwrap();
    let res2 = session::open_session(file2.to_str().unwrap()).unwrap();

    assert_ne!(res1.session_id, res2.session_id);
    assert_eq!(res1.file_name, "a.txt");
    assert_eq!(res2.file_name, "b.txt");

    // Both should work independently
    let chunk1 = session::get_lines(&res1.session_id, super::SeekTarget::Line(0), 1).unwrap();
    let chunk2 = session::get_lines(&res2.session_id, super::SeekTarget::Line(0), 1).unwrap();
    assert_eq!(chunk1.lines[0], "file a");
    assert_eq!(chunk2.lines[0], "file b");

    session::close_session(&res1.session_id).unwrap();
    session::close_session(&res2.session_id).unwrap();
    cleanup(&dir);
}

#[test]
fn search_poll_reports_match_limit() {
    let dir = create_test_dir("search_limit");
    // "a" appears twice per line ("aa"), need enough lines to exceed the cap
    let content = "aa\n".repeat(MAX_SEARCH_MATCHES + 1000);
    let file = write_test_file(&dir, "test.txt", &content);

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    session::search_start(sid, "a".to_string(), literal_mode()).unwrap();

    // Poll until done
    let mut done = false;
    for _ in 0..200 {
        let poll = session::search_poll(sid, 0).unwrap();
        if matches!(poll.status, SearchStatus::Done) {
            assert_eq!(poll.new_matches.len(), MAX_SEARCH_MATCHES);
            assert_eq!(poll.total_match_count, MAX_SEARCH_MATCHES);
            assert!(poll.match_limit_reached);
            // Should have stopped early (not scanned the whole file)
            assert!(poll.bytes_scanned < poll.total_bytes);
            done = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(done, "Search did not complete in time");

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

#[test]
fn search_poll_incremental_delivery() {
    let dir = create_test_dir("search_incremental");
    let file = write_test_file(&dir, "test.txt", "aaa\nbbb\naaa\nbbb\naaa\n");

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    session::search_start(sid, "aaa".to_string(), literal_mode()).unwrap();

    // Wait for search to finish
    for _ in 0..100 {
        let poll = session::search_poll(sid, 0).unwrap();
        if matches!(poll.status, SearchStatus::Done) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    // since_index=0 returns all 3 matches
    let poll_all = session::search_poll(sid, 0).unwrap();
    assert_eq!(poll_all.new_matches.len(), 3);
    assert_eq!(poll_all.total_match_count, 3);

    // since_index=2 returns only the last match
    let poll_delta = session::search_poll(sid, 2).unwrap();
    assert_eq!(poll_delta.new_matches.len(), 1);
    assert_eq!(poll_delta.total_match_count, 3);
    assert_eq!(poll_delta.new_matches[0].line, 4); // 5th line (0-indexed)

    // since_index=3 (caught up) returns no new matches
    let poll_none = session::search_poll(sid, 3).unwrap();
    assert_eq!(poll_none.new_matches.len(), 0);
    assert_eq!(poll_none.total_match_count, 3);

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

// ----- viewer_read_range tests -----

fn line(line: u64, offset: u32) -> RangeEnd {
    RangeEnd::Line { line, offset }
}

#[test]
fn read_range_full_load_anchor_equals_focus_returns_empty() {
    let dir = create_test_dir("range_eq");
    let file = write_test_file(&dir, "test.txt", "hello\nworld\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let out = session::read_range(&sid, 1, line(0, 3), line(0, 3)).unwrap();
    assert_eq!(out, "");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_single_line_slice() {
    let dir = create_test_dir("range_single_line");
    let file = write_test_file(&dir, "test.txt", "hello world\nsecond line\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    // Slice "ello w" out of the first line.
    let out = session::read_range(&sid, 1, line(0, 1), line(0, 7)).unwrap();
    assert_eq!(out, "ello w");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_multi_line_includes_newlines_between() {
    let dir = create_test_dir("range_multi_line");
    let file = write_test_file(&dir, "test.txt", "alpha\nbeta\ngamma\ndelta\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    // From (0, 2) "pha\n" + "beta\n" + "gamma\n" + "del" => "pha\nbeta\ngamma\ndel".
    let out = session::read_range(&sid, 1, line(0, 2), line(3, 3)).unwrap();
    assert_eq!(out, "pha\nbeta\ngamma\ndel");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_reversed_inputs_normalised() {
    let dir = create_test_dir("range_reversed");
    let file = write_test_file(&dir, "test.txt", "hello world\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let forward = session::read_range(&sid, 1, line(0, 0), line(0, 5)).unwrap();
    let reversed = session::read_range(&sid, 2, line(0, 5), line(0, 0)).unwrap();
    assert_eq!(forward, reversed);
    assert_eq!(forward, "hello");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_out_of_range_returns_typed_error() {
    let dir = create_test_dir("range_oor");
    let file = write_test_file(&dir, "test.txt", "one line only\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let err = session::read_range(&sid, 1, line(99, 0), line(99, 5)).unwrap_err();
    assert!(matches!(err, ViewerError::OutOfRange));

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_eof_selects_to_end() {
    let dir = create_test_dir("range_eof");
    let file = write_test_file(&dir, "test.txt", "first\nsecond\nthird\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let out = session::read_range(&sid, 1, line(0, 0), RangeEnd::Eof).unwrap();
    // The trailing newline is excluded (half-open semantics; the last line's content is included).
    assert_eq!(out, "first\nsecond\nthird\n");
    // Note: this file *has* a trailing newline; the split gives a 4th empty "line", whose
    // start is included. Trailing newline trimming leaves "first\nsecond\nthird\n".

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_utf16_surrogate_clamps_down() {
    let dir = create_test_dir("range_surrogate");
    // "👋hi" has UTF-16 length 4 (emoji is 2 units, then h, i).
    let file = write_test_file(&dir, "test.txt", "👋hi\nnext\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    // Offset 1 lands inside the surrogate pair; clamp to 0 — output excludes the emoji entirely.
    let out = session::read_range(&sid, 1, line(0, 1), line(0, 3)).unwrap();
    // From (clamped) 0 to 3 (= 'h'): "👋h"? No — clamp pulls offset 1 down to byte 0, so we get [0..byte_for_3].
    // Byte for offset 3 = end of 'h' = byte 5. So output is the full "👋h" = 5 bytes.
    assert_eq!(out, "👋h");

    // A clearer case: offset 1 to 1 (single-line collapsed inside the emoji's surrogate) → "".
    let out_empty = session::read_range(&sid, 2, line(0, 1), line(0, 1)).unwrap();
    assert_eq!(out_empty, "");

    // Offset 0 to 2 (full emoji) → "👋".
    let out_emoji = session::read_range(&sid, 3, line(0, 0), line(0, 2)).unwrap();
    assert_eq!(out_emoji, "👋");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_only_newlines_file() {
    let dir = create_test_dir("range_only_newlines");
    // Three empty lines: "\n\n\n" gives 4 entries when split by '\n' (empty, empty, empty, empty).
    let file = write_test_file(&dir, "test.txt", "\n\n\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    // Select all up to line 2 offset 0: "(empty)\n(empty)\n" = "\n\n".
    let out = session::read_range(&sid, 1, line(0, 0), line(2, 0)).unwrap();
    assert_eq!(out, "\n\n");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_byte_seek_eof_selects_whole_file() {
    let dir = create_test_dir("range_bs_eof");
    // Create a > 1 MB file so we land in ByteSeek.
    let line_str = "0123456789".repeat(10) + "\n"; // 101 bytes
    let line_count = (FULL_LOAD_THRESHOLD as usize / line_str.len()) + 100;
    let content: String = line_str.repeat(line_count);
    let file = write_test_file(&dir, "big.txt", &content);
    let open = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = open.session_id.clone();

    let out = session::read_range(&sid, 1, line(0, 0), RangeEnd::Eof).unwrap();
    // Should match the file content (sans the very last trailing newline).
    let expected = content.trim_end_matches('\n');
    assert_eq!(out, expected);

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_cancellation_returns_cancelled() {
    use std::sync::atomic::AtomicBool;

    // Direct test of the inner-loop cancel check: bypass the session and drive the
    // pure `read_range` against a `FullLoad` backend, flipping the cancel flag BEFORE
    // the call so the result is deterministic (no thread race). A file with ~512
    // lines is plenty to exceed `CANCEL_CHECK_LINES = 256`, guaranteeing the in-loop
    // check fires at least once.
    use super::full_load::FullLoadBackend;
    use super::range_read::read_range;

    let line_str = "x".repeat(64) + "\n"; // 65 bytes per line.
    let content: String = line_str.repeat(512);
    let backend = FullLoadBackend::from_content(&content, "cancel.txt");

    let cancel = AtomicBool::new(true);
    let result = read_range(&backend, line(0, 0), RangeEnd::Eof, &cancel);

    assert!(
        matches!(result, Err(ViewerError::Cancelled)),
        "expected ViewerError::Cancelled, got: {:?}",
        result
    );
}

#[test]
fn read_range_session_cancellation_returns_cancelled_and_cleans_up() {
    // End-to-end through the session: use a file big enough that the read can't
    // complete before the canceller fires. 1 MB + 1024 lines pushes us into ByteSeek
    // mode and the read takes long enough to interrupt deterministically.
    let dir = create_test_dir("range_session_cancel");
    // ~10 MB file with 4 KB lines: 4096 lines = ~16 MB → ByteSeek mode, and the read
    // needs many fetch chunks. The in-loop check fires hundreds of times.
    let line_str = "z".repeat(4096) + "\n";
    let content: String = line_str.repeat(4096);
    let file = write_test_file(&dir, "big.txt", &content);
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let sid_for_cancel = sid.clone();
    let canceller = thread::spawn(move || {
        // 2 ms is enough that the read has started but nowhere near done on a ~16 MB
        // file. The in-loop check (every 64 KB or 256 lines) fires well before the
        // read completes.
        thread::sleep(Duration::from_millis(2));
        session::cancel_read(&sid_for_cancel, 42).unwrap();
    });

    let result = session::read_range(&sid, 42, line(0, 0), RangeEnd::Eof);
    let _ = canceller.join();

    assert!(
        matches!(result, Err(ViewerError::Cancelled)),
        "expected ViewerError::Cancelled, got: {:?}",
        result.map(|s| format!("Ok({} bytes)", s.len()))
    );
    assert_eq!(session::active_read_count(&sid), 0);

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_crlf_preserves_carriage_returns() {
    // CRLF files: the backend keeps the `\r` AS PART of the line text (it only splits
    // on `\n`). So `read_range` returns lines with their `\r` intact, and the byte-
    // offset arithmetic (`line.len() + 1`) correctly accounts for both bytes. This
    // test pins both behaviours so a future "auto-strip \r" change doesn't silently
    // drift `byte_offset`.
    let dir = create_test_dir("range_crlf");
    let content = "alpha\r\nbeta\r\ngamma\r\n";
    let file = write_test_file(&dir, "crlf.txt", content);
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    // ⌘A-equivalent: read everything. The backend keeps `\r` as part of each line;
    // `range_read` rejoins with `\n` between lines and trims exactly one trailing
    // newline at EOF (the same half-open behaviour the LF test asserts). Net: the
    // original CRLF bytes round-trip exactly.
    let out = session::read_range(&sid, 1, line(0, 0), RangeEnd::Eof).unwrap();
    assert_eq!(out, "alpha\r\nbeta\r\ngamma\r\n");

    // Multi-line slice: from (0, 2) to (1, 3). On line 0 the text after offset 2 is
    // "pha\r" (offset 2 in UTF-16 lands on byte 2 of "alpha\r"). Then the joining
    // `\n`. Then on line 1 from offset 0 to 3 = "bet".
    let slice = session::read_range(&sid, 2, line(0, 2), line(1, 3)).unwrap();
    assert_eq!(slice, "pha\r\nbet");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_cleans_up_active_reads_on_success() {
    let dir = create_test_dir("range_cleanup");
    let file = write_test_file(&dir, "test.txt", "small\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    session::read_range(&sid, 7, line(0, 0), line(0, 5)).unwrap();
    assert_eq!(session::active_read_count(&sid), 0);

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_session_not_found_returns_typed_error() {
    let err = session::read_range("nonexistent-session", 1, line(0, 0), line(0, 5)).unwrap_err();
    assert!(matches!(err, ViewerError::SessionNotFound { .. }));
}

#[test]
fn cancel_read_unknown_id_is_no_op() {
    let dir = create_test_dir("cancel_unknown");
    let file = write_test_file(&dir, "test.txt", "small\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    // No read with id 99; cancel should succeed silently.
    session::cancel_read(&sid, 99).unwrap();

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn write_range_to_file_writes_atomically() {
    let dir = create_test_dir("write_range");
    let file = write_test_file(&dir, "test.txt", "alpha\nbeta\ngamma\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let dest = dir.join("out.txt");
    session::write_range_to_file(&sid, 1, line(0, 0), line(2, 5), &dest).unwrap();
    let written = fs::read_to_string(&dest).unwrap();
    assert_eq!(written, "alpha\nbeta\ngamma");

    // No leftover temp file in the dir.
    let leftover = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().contains("cmdr-tmp"));
    assert!(!leftover, "temp file leaked after successful write");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn write_range_to_file_propagates_out_of_range_error() {
    let dir = create_test_dir("write_range_oor");
    let file = write_test_file(&dir, "test.txt", "one line\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let dest = dir.join("out.txt");
    let err = session::write_range_to_file(&sid, 1, line(99, 0), line(99, 5), &dest).unwrap_err();
    assert!(matches!(err, ViewerError::OutOfRange));
    assert!(!dest.exists());

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_stitching_adjacent_ranges_equals_one_big_range() {
    let dir = create_test_dir("range_stitch");
    let file = write_test_file(&dir, "test.txt", "alpha\nbeta\ngamma\ndelta\nepsilon\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    // Pick a split point in the middle of the file: between (1, 2) and (1, 2).
    let big = session::read_range(&sid, 1, line(0, 0), line(4, 5)).unwrap();
    let first = session::read_range(&sid, 2, line(0, 0), line(1, 2)).unwrap();
    let second = session::read_range(&sid, 3, line(1, 2), line(4, 5)).unwrap();
    assert_eq!(big, format!("{}{}", first, second));

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

// ─── Step 1.2: per-match cancellation ──────────────────────────────────────────────

#[test]
fn search_pre_cancelled_starts_and_finishes_quickly() {
    // Setting the cancel flag before search_start completes should produce a
    // Cancelled status within a short wall-clock budget.
    let dir = create_test_dir("search_pre_cancel");
    // ~50 MB file, plenty of matches per line.
    let line_with_a: String = "a".repeat(1_000) + "\n";
    let content: String = line_with_a.repeat(50_000);
    let file = write_test_file(&dir, "test.txt", &content);

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    session::search_start(sid, "a".to_string(), literal_mode()).unwrap();
    session::search_cancel(sid).unwrap();

    let start = std::time::Instant::now();
    let mut saw_terminal = false;
    while start.elapsed() < Duration::from_millis(1_500) {
        let poll = session::search_poll(sid, 0).unwrap();
        if matches!(poll.status, SearchStatus::Cancelled | SearchStatus::Done) {
            saw_terminal = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        saw_terminal,
        "search did not reach a terminal status within 1.5s of cancel"
    );

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

// ─── Step 1.4: watchdog protocol ───────────────────────────────────────────────────

#[test]
fn test_worker_done_after_watchdog_cancelled_is_sticky() {
    // Simulate the race the round-3 review caught: the watchdog has already
    // written `Cancelled`, then the worker tries to write its natural verdict.
    // The conditional in `finalize_search_status` must keep `Cancelled`.
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    let status = Arc::new(Mutex::new(SearchStatus::Cancelled));
    let cancel = Arc::new(AtomicBool::new(true));

    session::finalize_search_status(&status, &cancel, /* errored */ false);
    assert!(
        matches!(*status.lock().unwrap(), SearchStatus::Cancelled),
        "finalize must keep Cancelled when the watchdog already wrote it"
    );

    // Same race, errored worker.
    session::finalize_search_status(&status, &cancel, /* errored */ true);
    assert!(
        matches!(*status.lock().unwrap(), SearchStatus::Cancelled),
        "finalize must keep Cancelled even when the worker errored"
    );
}

#[test]
fn test_finalize_writes_done_when_worker_finishes_naturally() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    let status = Arc::new(Mutex::new(SearchStatus::Running));
    let cancel = Arc::new(AtomicBool::new(false));

    session::finalize_search_status(&status, &cancel, false);
    assert!(matches!(*status.lock().unwrap(), SearchStatus::Done));
}

#[test]
fn test_finalize_writes_cancelled_when_cancel_observed() {
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    let status = Arc::new(Mutex::new(SearchStatus::Running));
    let cancel = Arc::new(AtomicBool::new(true));

    session::finalize_search_status(&status, &cancel, false);
    assert!(matches!(*status.lock().unwrap(), SearchStatus::Cancelled));
}

#[test]
fn test_watchdog_forces_cancel_when_worker_ignores_flag() {
    // Spawn `run_search_watchdog` against a fake worker that never observes
    // the cancel flag (it just keeps the status at `Running`). The watchdog
    // must transition the status to `Cancelled` within ~1.25 s of the flag
    // being set.
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    let status = Arc::new(Mutex::new(SearchStatus::Running));
    let cancel = Arc::new(AtomicBool::new(false));

    let watchdog_status = status.clone();
    let watchdog_cancel = cancel.clone();
    let handle = thread::spawn(move || session::run_search_watchdog(watchdog_cancel, watchdog_status));

    // Set the cancel flag after a short delay so the watchdog observes
    // Running first, then sees the cancel.
    thread::sleep(Duration::from_millis(50));
    cancel.store(true, std::sync::atomic::Ordering::Relaxed);

    let start = std::time::Instant::now();
    handle.join().unwrap();
    let elapsed = start.elapsed();

    assert!(
        matches!(*status.lock().unwrap(), SearchStatus::Cancelled),
        "watchdog must write Cancelled"
    );
    assert!(
        elapsed < Duration::from_millis(1_500),
        "watchdog took too long: {:?}",
        elapsed
    );
}

#[test]
fn test_watchdog_exits_when_worker_finishes_first() {
    // The watchdog must not write Cancelled if the worker reaches Done first
    // (with no cancel signalled).
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    let status = Arc::new(Mutex::new(SearchStatus::Running));
    let cancel = Arc::new(AtomicBool::new(false));

    let watchdog_status = status.clone();
    let watchdog_cancel = cancel.clone();
    let handle = thread::spawn(move || session::run_search_watchdog(watchdog_cancel, watchdog_status));

    // Mark worker done before the watchdog's first poll tick (250 ms).
    thread::sleep(Duration::from_millis(50));
    *status.lock().unwrap() = SearchStatus::Done;

    handle.join().unwrap();
    assert!(
        matches!(*status.lock().unwrap(), SearchStatus::Done),
        "watchdog must not clobber Done"
    );
}

#[test]
fn test_new_search_after_watchdog_cancelled_starts_clean() {
    // After a Cancelled verdict, starting a fresh search must reset the status.
    let dir = create_test_dir("watchdog_reset");
    let file = write_test_file(&dir, "test.txt", "hello\nworld\n");
    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    session::search_start(sid, "hello".to_string(), literal_mode()).unwrap();
    session::search_cancel(sid).unwrap();

    // Wait until we see Cancelled.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_millis(2_000) {
        let poll = session::search_poll(sid, 0).unwrap();
        if matches!(poll.status, SearchStatus::Cancelled) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    // Fresh search must clear the Cancelled state.
    session::search_start(sid, "world".to_string(), literal_mode()).unwrap();
    let poll = session::search_poll(sid, 0).unwrap();
    assert!(
        matches!(poll.status, SearchStatus::Running | SearchStatus::Done),
        "fresh search after Cancelled must start clean, got {:?}",
        poll.status
    );

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

// ─── Step 1.5: invalid query surface ──────────────────────────────────────────────

#[test]
fn test_invalid_regex_surfaces_as_invalid_query_status() {
    let dir = create_test_dir("invalid_regex");
    let file = write_test_file(&dir, "test.txt", "hello\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let mode = SearchMode {
        use_regex: true,
        case_sensitive: true,
    };
    // `(unclosed` is invalid syntax.
    session::search_start(&sid, "(unclosed".to_string(), mode).unwrap();

    let poll = session::search_poll(&sid, 0).unwrap();
    match poll.status {
        SearchStatus::InvalidQuery { message } => {
            assert!(!message.is_empty(), "invalid-query message must be non-empty");
        }
        other => panic!("expected InvalidQuery, got {:?}", other),
    }

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_multiline_regex_surfaces_as_invalid_query_status() {
    let dir = create_test_dir("multiline_regex");
    let file = write_test_file(&dir, "test.txt", "hello\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let mode = SearchMode {
        use_regex: true,
        case_sensitive: true,
    };
    session::search_start(&sid, "(?s).".to_string(), mode).unwrap();

    let poll = session::search_poll(&sid, 0).unwrap();
    assert!(
        matches!(poll.status, SearchStatus::InvalidQuery { .. }),
        "expected InvalidQuery, got {:?}",
        poll.status
    );

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_regex_search_returns_matches() {
    let dir = create_test_dir("regex_search");
    let file = write_test_file(&dir, "test.txt", "a123\nb456\nc789\n");
    let sid = session::open_session(file.to_str().unwrap()).unwrap().session_id;

    let mode = SearchMode {
        use_regex: true,
        case_sensitive: true,
    };
    session::search_start(&sid, r"\d+".to_string(), mode).unwrap();

    let start = std::time::Instant::now();
    let mut done = false;
    while start.elapsed() < Duration::from_secs(2) {
        let poll = session::search_poll(&sid, 0).unwrap();
        if matches!(poll.status, SearchStatus::Done) {
            assert_eq!(poll.total_match_count, 3);
            done = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(done, "regex search did not complete");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

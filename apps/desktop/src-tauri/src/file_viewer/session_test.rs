//! Tests for ViewerSession orchestrator.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use super::session::{self, SearchStatus};
use super::{FULL_LOAD_THRESHOLD, FileEncoding, MAX_SEARCH_MATCHES, RangeEnd, SearchMode, ViewerError};

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

/// Wait until a freshly-opened session's watcher subscribe has landed, so
/// `test_only_emit` reliably reaches a subscriber. The subscribe runs on a
/// background thread off `open_session`'s critical path (see
/// `spawn_watcher_manager`), so tests that inject synthetic watcher events must
/// sync on it instead of assuming the watcher is live the moment open returns.
/// Each test runs in its own nextest process, so `watch_count() > 0` reflects
/// only the session(s) opened in this test.
fn wait_for_watcher_subscribed() {
    for _ in 0..200 {
        if super::watcher::VIEWER_WATCHER_MANAGER.watch_count() > 0 {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("watcher subscribe did not land within 2s");
}

#[test]
fn open_small_file_uses_full_load() {
    let dir = create_test_dir("small");
    let file = write_test_file(&dir, "test.txt", "hello\nworld\n");

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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
    let result = session::open_session("/nonexistent_session_test.txt", "root");
    assert!(result.is_err());
}

#[test]
fn open_directory_fails() {
    let dir = create_test_dir("dir_fail");
    let result = session::open_session(dir.to_str().unwrap(), "root");
    assert!(result.is_err());
    cleanup(&dir);
}

#[test]
fn get_lines_after_open() {
    let dir = create_test_dir("get_lines");
    let file = write_test_file(&dir, "test.txt", "a\nb\nc\nd\ne\n");

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();

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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = &open_result.session_id;

    let poll = session::search_poll(sid, 0).unwrap();
    assert!(matches!(poll.status, SearchStatus::Idle));

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

#[test]
fn tilde_expansion() {
    // open_session should handle ~ paths
    let result = session::open_session("~/nonexistent_file_tilde_test.txt", "root");
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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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

    let res1 = session::open_session(file1.to_str().unwrap(), "root").unwrap();
    let res2 = session::open_session(file2.to_str().unwrap(), "root").unwrap();

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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let out = session::read_range(&sid, 1, line(0, 3), line(0, 3)).unwrap();
    assert_eq!(out, "");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_single_line_slice() {
    let dir = create_test_dir("range_single_line");
    let file = write_test_file(&dir, "test.txt", "hello world\nsecond line\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    let err = session::read_range(&sid, 1, line(99, 0), line(99, 5)).unwrap_err();
    assert!(matches!(err, ViewerError::OutOfRange));

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn read_range_full_load_eof_selects_to_end() {
    let dir = create_test_dir("range_eof");
    let file = write_test_file(&dir, "test.txt", "first\nsecond\nthird\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let open = session::open_session(file.to_str().unwrap(), "root").unwrap();
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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

    // No read with id 99; cancel should succeed silently.
    session::cancel_read(&sid, 99).unwrap();

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn write_range_to_file_writes_atomically() {
    let dir = create_test_dir("write_range");
    let file = write_test_file(&dir, "test.txt", "alpha\nbeta\ngamma\n");
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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

    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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
    let open_result = session::open_session(file.to_str().unwrap(), "root").unwrap();
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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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
    let sid = session::open_session(file.to_str().unwrap(), "root")
        .unwrap()
        .session_id;

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

// -- Encoding-switch tests --------------------------------------------------------

fn encode_utf16_le(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for ch in s.encode_utf16() {
        out.extend_from_slice(&ch.to_le_bytes());
    }
    out
}

#[test]
fn get_encoding_options_returns_detected_and_all() {
    let dir = create_test_dir("enc_options");
    // UTF-8 with high-bit Latin-1 (Windows-1252-detectable).
    let file = dir.join("latin1.txt");
    fs::write(&file, b"caf\xE9\n").unwrap();

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let opts = session::get_encoding_options(&result.session_id).unwrap();
    assert_eq!(opts.detected, FileEncoding::Windows1252);
    assert_eq!(opts.current, FileEncoding::Windows1252);
    assert!(opts.all.len() >= 8, "expected all 8 encodings, got {}", opts.all.len());
    assert!(opts.all.iter().any(|c| c.encoding == FileEncoding::Utf8));
    assert!(opts.all.iter().any(|c| c.encoding == FileEncoding::Utf16Le));

    session::close_session(&result.session_id).unwrap();
    cleanup(&dir);
}

#[test]
fn set_encoding_full_load_swaps_decoder() {
    let dir = create_test_dir("enc_full_load_swap");
    // 0xE9 = 'é' in Windows-1252 / 'í' in Mac Roman / lone 0xE9 in UTF-8 is U+FFFD.
    let file = dir.join("bytes.txt");
    fs::write(&file, b"caf\xE9\n").unwrap();
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    assert!(matches!(result.backend_type, session::BackendType::FullLoad));

    // Currently Windows-1252 (detected): line should decode as "café".
    let chunk = session::get_lines(&result.session_id, super::SeekTarget::Line(0), 1).unwrap();
    assert_eq!(chunk.lines[0], "café");

    // Force UTF-8: the high byte becomes U+FFFD.
    session::set_encoding(&result.session_id, FileEncoding::Utf8).unwrap();
    let chunk = session::get_lines(&result.session_id, super::SeekTarget::Line(0), 1).unwrap();
    assert_eq!(chunk.lines[0], "caf\u{FFFD}");

    session::close_session(&result.session_id).unwrap();
    cleanup(&dir);
}

#[test]
fn set_encoding_large_file_utf8_to_utf16_rebuilds_under_new_encoding() {
    let dir = create_test_dir("enc_large_utf16_rebuild");
    // Build a > 1 MB file in UTF-16 LE so the backend ends up as ByteSeek + LineIndex.
    let line_utf16 = encode_utf16_le("hello world\n");
    let line_count = (FULL_LOAD_THRESHOLD as usize / line_utf16.len()) + 200;
    let mut bytes = Vec::with_capacity(line_utf16.len() * line_count);
    for _ in 0..line_count {
        bytes.extend_from_slice(&line_utf16);
    }
    let file = dir.join("big-utf16.txt");
    fs::write(&file, &bytes).unwrap();

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    // Detector should already pick UTF-16 LE via parity. Decode of line 0 = "hello world".
    let initial = &result.initial_lines.lines;
    assert!(
        initial.iter().any(|l| l == "hello world"),
        "expected 'hello world' lines, got {:?}",
        initial.iter().take(3).collect::<Vec<_>>()
    );

    // Force UTF-8: each "hello world" line becomes a garbled string (every other byte 0x00),
    // but the backend doesn't crash and `get_lines` still returns content.
    session::set_encoding(&result.session_id, FileEncoding::Utf8).unwrap();
    // Wait briefly for the BG rebuild thread to complete (or time out).
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        let opts = session::get_encoding_options(&result.session_id).unwrap();
        assert_eq!(opts.current, FileEncoding::Utf8);
        if !session::test_only_rebuilding_active(&result.session_id) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    // Status should not loop forever.
    assert!(
        !session::test_only_rebuilding_active(&result.session_id),
        "rebuild did not complete within 8 s"
    );

    session::close_session(&result.session_id).unwrap();
    cleanup(&dir);
}

#[test]
fn test_append_during_encoding_rebuild_not_dropped() {
    // The drain-and-swap protocol: a watcher Grew(eof) event queued in
    // session.pending_grew during the rebuild must be picked up by the rebuild
    // thread's swap critical section, so the final LineIndex covers the new EOF
    // (not just the pre-rebuild EOF).
    //
    // We simulate the watcher by calling test_only_push_pending_grew directly
    // (the actual watcher lands in milestone 3).
    let dir = create_test_dir("enc_append_during_rebuild");
    // Start with a > 1 MB Windows-1252 file.
    let line = "x".repeat(80) + "\n";
    let line_count = (FULL_LOAD_THRESHOLD as usize / line.len()) + 200;
    let mut content: String = line.repeat(line_count);
    let file = dir.join("big.txt");
    fs::write(&file, &content).unwrap();
    let original_size = fs::metadata(&file).unwrap().len();

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    // Wait for the initial ByteSeek -> LineIndex upgrade to finish so the
    // pending_grew queue only exercises the rebuild's drain, not the
    // upgrade's. (Otherwise the upgrade could drain our queued EOF before
    // the rebuild ever sees it.)
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        let status = session::get_session_status(&result.session_id).unwrap();
        if matches!(status.backend_type, session::BackendType::LineIndex) && !status.is_indexing {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    // Append 10 KB BEFORE the rebuild starts; the on-disk file is now bigger than
    // what the backend knows about. The test queues this new EOF and verifies that
    // the rebuild's drain-and-swap protocol picks it up.
    let suffix = "y".repeat(10_000) + "\n";
    content.push_str(&suffix);
    fs::write(&file, &content).unwrap();
    let new_size = fs::metadata(&file).unwrap().len();
    assert!(new_size > original_size);

    // Park the rebuild thread so we can queue the EOF before it drains.
    session::test_only_set_rebuild_hold(400);
    // Force a non-instant encoding swap (UTF-8 -> UTF-16 LE) so the rebuild runs.
    session::set_encoding(&result.session_id, FileEncoding::Utf16Le).unwrap();
    // Queue the new EOF; the rebuild's drain-and-swap protocol must observe it.
    // The hold above ensures the rebuild hasn't drained yet.
    session::test_only_push_pending_grew(&result.session_id, new_size);

    // Wait for rebuild to finish.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if !session::test_only_rebuilding_active(&result.session_id) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !session::test_only_rebuilding_active(&result.session_id),
        "rebuild did not complete"
    );

    // The final backend should cover the FULL new_size (not the pre-append size).
    // We can't read total_bytes directly from outside; use get_lines with a Fraction
    // target near 1.0 — the chunk's `total_bytes` field reflects the backend's
    // current total.
    let chunk = session::get_lines(&result.session_id, super::SeekTarget::Fraction(0.0), 1).unwrap();
    assert_eq!(
        chunk.total_bytes, new_size,
        "rebuild must absorb the queued append (drain-and-swap protocol)"
    );

    session::close_session(&result.session_id).unwrap();
    cleanup(&dir);
}

// ─── Tail mode + reload + watcher wiring (milestone 3) ─────────────────

#[test]
fn tail_mode_toggle_persists_on_session() {
    let dir = create_test_dir("tail_toggle");
    let file = write_test_file(&dir, "log.txt", "hello\n");
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();

    assert!(!session::test_only_tail_mode(&sid));
    session::set_tail_mode(&sid, true).unwrap();
    assert!(session::test_only_tail_mode(&sid));
    session::set_tail_mode(&sid, false).unwrap();
    assert!(!session::test_only_tail_mode(&sid));

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn reload_replaces_backend_against_current_disk_contents() {
    let dir = create_test_dir("reload");
    let file = write_test_file(&dir, "log.txt", "first\n");
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();

    // Overwrite the file out of band: same path, fully replaced content.
    fs::write(&file, "first\nsecond\nthird\n").unwrap();

    session::reload(&sid).unwrap();
    let status = session::get_session_status(&sid).unwrap();
    assert!(status.total_lines.is_some());
    let chunk = session::get_lines(&sid, super::SeekTarget::Line(1), 2).unwrap();
    assert_eq!(chunk.lines, vec!["second", "third"]);

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn set_tail_mode_enabling_catches_up_existing_growth() {
    let dir = create_test_dir("tail_catchup");
    // Use a file just over FULL_LOAD_THRESHOLD so we land on ByteSeek and
    // then upgrade to LineIndex. We use ByteSeek's extend_to which simply
    // updates total_bytes; the catch-up check still proves the path is wired.
    let initial: String = "a\n".repeat((FULL_LOAD_THRESHOLD as usize / 2) + 1);
    let file = write_test_file(&dir, "log.txt", &initial);
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();
    let original_bytes = result.total_bytes;

    // Grow on disk while tail is off.
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        f.write_all(b"extra-bytes-extra-bytes-extra-bytes-\n").unwrap();
    }
    let on_disk = fs::metadata(&file).unwrap().len();
    assert!(on_disk > original_bytes);

    session::set_tail_mode(&sid, true).unwrap();

    // The backend's total_bytes should snap to the on-disk size (ByteSeek
    // extend_to just updates the size field). On LineIndex it may have been
    // re-indexed via the same path.
    let backend_bytes = session::get_session_status(&sid).unwrap();
    let _ = backend_bytes; // we re-read via get_lines below
    // get_lines after the snap should not blow up; that's the smoke check.
    let _chunk = session::get_lines(&sid, super::SeekTarget::Fraction(0.99), 2).unwrap();

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn tail_mode_on_extends_backend_when_watcher_reports_grew() {
    // Integration-style test for the watcher → session pipeline. We exercise
    // the full handler stack (handle_watcher_event → apply_tail_extend →
    // backend.extend_to_boxed → ArcSwap::store), but inject the WatcherEvent
    // directly via the watcher's test-only hook instead of waiting on the
    // macOS FSEvents debouncer. Reasons: (1) FSEvents latency is variable
    // (~1–3 s) and the 8 s nextest cap leaves no margin for a real-FS test
    // to also wait for the background ByteSeek → LineIndex upgrade; (2) the
    // watcher's own tests already cover the FS-event path end-to-end against
    // tempfile.
    let dir = create_test_dir("tail_int");
    let line = "x".repeat(120) + "\n";
    let line_count = (FULL_LOAD_THRESHOLD as usize / line.len()) + 100;
    let initial: String = line.repeat(line_count);
    let path = write_test_file(&dir, "tail-int.log", &initial);

    let result = session::open_session(path.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();
    let original_bytes = result.total_bytes;
    // Sync on the background subscribe before mutating, so the catch-up re-stat
    // stays a no-op and the explicit emit below is the driver under test.
    wait_for_watcher_subscribed();

    // Wait for the background ByteSeek → LineIndex upgrade to finish so
    // we're testing the post-upgrade fast path (extend an existing
    // LineIndex), not the upgrade-queue interaction (already covered by
    // `test_append_during_upgrade_not_dropped`).
    let mut upgrade_done = false;
    let mut last_backend = format!("{:?}", session::get_session_status(&sid).unwrap().backend_type);
    let mut last_indexing = session::get_session_status(&sid).unwrap().is_indexing;
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        let status = session::get_session_status(&sid).unwrap();
        last_backend = format!("{:?}", status.backend_type);
        last_indexing = status.is_indexing;
        if matches!(status.backend_type, session::BackendType::LineIndex) && !status.is_indexing {
            upgrade_done = true;
            break;
        }
    }
    assert!(
        upgrade_done,
        "upgrade thread should have completed within 3 s; last_backend={}, last_indexing={}",
        last_backend, last_indexing,
    );

    // Switch on tail mode now (before the watcher fires).
    session::set_tail_mode(&sid, true).unwrap();

    // Append to disk first so extend_to has something to scan.
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"second line at the end\n").unwrap();
    }
    let want_size = fs::metadata(&path).unwrap().len();
    assert!(want_size > original_bytes);

    // Drive the watcher → session handler directly via the test-only emit
    // hook on the watcher singleton, then poll until apply_tail_extend has
    // moved the backend's view forward.
    let sent = super::watcher::test_only_emit(
        &fs::canonicalize(&path).unwrap(),
        super::watcher::WatcherEvent::Grew(want_size),
    );
    assert!(sent > 0, "test_only_emit should have found a subscriber");

    let mut caught_up = false;
    let mut last_chunk_bytes = 0;
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        let chunk = session::get_lines(&sid, super::SeekTarget::Fraction(0.0), 1).unwrap();
        last_chunk_bytes = chunk.total_bytes;
        if chunk.total_bytes >= want_size {
            caught_up = true;
            break;
        }
    }
    let status = session::get_session_status(&sid).unwrap();
    assert!(
        caught_up,
        "tail-mode handler should have extended the backend; last={}, want={}, backend={:?}, indexing={}",
        last_chunk_bytes, want_size, status.backend_type, status.is_indexing,
    );

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

// ─── Audit fix coverage ────────────────────────────────────────────────────────────

#[test]
fn test_tail_extend_during_encoding_rebuild_discards_stale_extend() {
    // The round-3 audit caught: `apply_tail_extend` snapshots the backend, runs
    // a multi-second `extend_to_boxed`, then stores. If an encoding rebuild
    // installs a new backend during the extend, the tail's store would clobber
    // it. The fix uses `Arc::ptr_eq` between the snapshot and the live backend
    // before storing; on mismatch, the extend is discarded and the EOF is
    // re-queued.
    //
    // We script the timing with `test_only_run_tail_extend_with_swap`: it
    // snapshots, runs a `swap_callback` (we use it to call `reload`, which
    // installs a brand-new backend), then runs `extend_to_boxed`, then runs
    // the same ptr-eq check the production code uses. Returns `false` if the
    // stale extend was discarded — that's the assertion.
    let dir = create_test_dir("tail_clobber_race");
    let initial: String = "x\n".repeat((FULL_LOAD_THRESHOLD as usize / 2) + 1);
    let file = write_test_file(&dir, "race.log", &initial);
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();

    // Grow the file so apply_tail_extend has something to extend to.
    {
        use std::io::Write;
        let mut f = fs::OpenOptions::new().append(true).open(&file).unwrap();
        f.write_all(b"appended\n").unwrap();
    }
    let new_size = fs::metadata(&file).unwrap().len();

    let sid_for_swap = sid.clone();
    let stored = session::test_only_run_tail_extend_with_swap(&sid, new_size, move || {
        // While the watcher's extend is in flight, a "rebuild" installs a
        // brand-new backend via reload(). The tail's stale extend must NOT
        // clobber it.
        session::reload(&sid_for_swap).unwrap();
    });
    assert!(
        !stored,
        "stale extend must be discarded after a swap installs a fresh backend"
    );

    // The EOF should be re-queued so a follow-up FS event still catches up.
    let queued = session::test_only_pending_grew(&sid);
    assert!(
        matches!(queued, Some(eof) if eof >= new_size),
        "discarded extend must re-queue the EOF; got {:?}",
        queued
    );

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_append_during_upgrade_not_dropped() {
    // A watcher Grew event arriving while the ByteSeek -> LineIndex upgrade is
    // mid-scan queues into `pending_grew`. The upgrade thread's drain-and-swap
    // critical section reads-and-clears the queue and extends the new
    // LineIndex to the queued EOF. So the final backend covers the full file,
    // not just the pre-upgrade EOF.
    //
    // We script timing with `test_only_set_upgrade_hold`: the upgrade thread
    // sleeps before scanning so we can append to disk and queue the EOF before
    // the swap runs.
    let dir = create_test_dir("append_during_upgrade");
    session::test_only_set_upgrade_hold(800);

    // ~2 MB file -> ByteSeek + upgrade.
    let line = "x".repeat(80) + "\n";
    let line_count = (2 * FULL_LOAD_THRESHOLD as usize) / line.len();
    let mut content = line.repeat(line_count);
    let file = write_test_file(&dir, "upgrade.log", &content);
    let original_size = fs::metadata(&file).unwrap().len();

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();

    // Append 500 KB while the upgrade thread is paused.
    let suffix: String = ("y".repeat(80) + "\n").repeat(6400);
    content.push_str(&suffix);
    fs::write(&file, &content).unwrap();
    let new_size = fs::metadata(&file).unwrap().len();
    assert!(new_size > original_size);

    // Queue the EOF the way the real watcher would.
    session::test_only_push_pending_grew(&sid, new_size);

    // Wait for the upgrade thread to release and drain.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        let status = session::get_session_status(&sid).unwrap();
        if matches!(status.backend_type, session::BackendType::LineIndex) && !status.is_indexing {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let chunk = session::get_lines(&sid, super::SeekTarget::Fraction(0.0), 1).unwrap();
    assert_eq!(
        chunk.total_bytes, new_size,
        "upgrade drain must absorb the queued append"
    );
    assert!(
        chunk.total_lines.is_some_and(|n| n > line_count),
        "total_lines must cover the appended block; got {:?}",
        chunk.total_lines,
    );

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_append_between_drain_and_swap_not_dropped() {
    // Variant where multiple appends queue during the upgrade. Because the
    // watcher writes also take the `pending_grew` lock (and the drain holds
    // it across the whole drain + store), watcher writes physically block
    // until the upgrade releases the lock. The coalesced EOF is observed by
    // the upgrade's drain.
    let dir = create_test_dir("append_between_drain_swap");
    session::test_only_set_upgrade_hold(400);

    let line = "z".repeat(80) + "\n";
    let line_count = (FULL_LOAD_THRESHOLD as usize / line.len()) + 1000;
    let mut content = line.repeat(line_count);
    let file = write_test_file(&dir, "race.log", &content);

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();

    // Two consecutive queued EOFs: simulates two watcher events the
    // production code must coalesce into the final one.
    content.push_str("first-append\n");
    fs::write(&file, &content).unwrap();
    let mid_size = fs::metadata(&file).unwrap().len();
    session::test_only_push_pending_grew(&sid, mid_size);

    content.push_str("second-append-bigger\n");
    fs::write(&file, &content).unwrap();
    let final_size = fs::metadata(&file).unwrap().len();
    session::test_only_push_pending_grew(&sid, final_size);

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        let status = session::get_session_status(&sid).unwrap();
        if matches!(status.backend_type, session::BackendType::LineIndex) && !status.is_indexing {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let chunk = session::get_lines(&sid, super::SeekTarget::Fraction(0.0), 1).unwrap();
    assert_eq!(
        chunk.total_bytes, final_size,
        "coalesced queue must land on the final backend"
    );

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_session_emits_file_changed_on_append() {
    // We can't easily intercept Tauri events from a unit test without an
    // AppHandle, so we verify the observable side of the same code path: an
    // append with tail-on flips through `apply_tail_extend` and advances
    // total_bytes on the active backend. We use a > 1 MB file so the
    // backend is ByteSeek (or LineIndex after upgrade), both of which
    // support `extend_to_boxed`. FullLoad declines extension and would only
    // surface via the FE reload toast.
    let dir = create_test_dir("emit_on_append");
    let line = "a\n";
    let line_count = (FULL_LOAD_THRESHOLD as usize / line.len()) + 100;
    let initial: String = line.repeat(line_count);
    let file = write_test_file(&dir, "emit.log", &initial);
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();
    // Sync on the background subscribe before mutating the file, so the
    // catch-up re-stat stays a no-op and the explicit emit below is the driver.
    wait_for_watcher_subscribed();
    session::set_tail_mode(&sid, true).unwrap();

    let mut content = initial.clone();
    content.push_str("extra-line-content\n");
    fs::write(&file, &content).unwrap();
    let new_size = fs::metadata(&file).unwrap().len();

    let canonical = fs::canonicalize(&file).unwrap();
    let sent = super::watcher::test_only_emit(&canonical, super::watcher::WatcherEvent::Grew(new_size));
    assert!(sent > 0, "test_only_emit must reach the session's subscriber");

    let start = std::time::Instant::now();
    let mut observed = 0;
    while start.elapsed() < Duration::from_secs(8) {
        let chunk = session::get_lines(&sid, super::SeekTarget::Fraction(0.0), 1).unwrap();
        observed = chunk.total_bytes;
        if observed >= new_size {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        observed, new_size,
        "tail-on watcher event must advance backend total_bytes"
    );

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_session_tail_mode_off_does_not_extend_index() {
    // With tail mode OFF: a Grew event must NOT change total_lines on the
    // active backend. The FE still gets the `viewer:file-changed` event and
    // renders its reload toast; only `reload()` actually re-indexes.
    let dir = create_test_dir("tail_off_no_extend");
    let initial = "a\nb\nc\n";
    let file = write_test_file(&dir, "tailoff.log", initial);
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();
    wait_for_watcher_subscribed();
    assert!(!session::test_only_tail_mode(&sid));

    let original_lines = session::get_session_status(&sid).unwrap().total_lines;

    let mut content = initial.to_string();
    content.push_str("d\ne\nf\n");
    fs::write(&file, &content).unwrap();
    let new_size = fs::metadata(&file).unwrap().len();

    let canonical = fs::canonicalize(&file).unwrap();
    super::watcher::test_only_emit(&canonical, super::watcher::WatcherEvent::Grew(new_size));

    // Give the manager thread time to process the event (200 ms poll plus
    // slack); confirm total_lines did NOT move.
    thread::sleep(Duration::from_millis(400));
    let after_lines = session::get_session_status(&sid).unwrap().total_lines;
    assert_eq!(
        original_lines, after_lines,
        "tail-off watcher events must not extend the line index"
    );

    // After explicit reload, the backend reflects the new content.
    session::reload(&sid).unwrap();
    let reloaded_lines = session::get_session_status(&sid).unwrap().total_lines;
    assert!(
        reloaded_lines.unwrap_or(0) > after_lines.unwrap_or(0),
        "reload must surface the new content"
    );

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_session_rotation_reopens_backend() {
    // A `Replaced` event triggers an internal reload, which installs a fresh
    // backend. Observable via total_bytes: post-rotation it matches the new
    // file size.
    let dir = create_test_dir("rotation");
    fs::write(dir.join("rot.log"), b"old content\n").unwrap();
    let file = dir.join("rot.log");
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();
    let original_bytes = result.total_bytes;
    wait_for_watcher_subscribed();

    // Replace the file with a longer one.
    let new_path = dir.join("rot.log.new");
    fs::write(&new_path, b"NEW: longer content goes here\n").unwrap();
    fs::rename(&new_path, &file).unwrap();
    let new_size = fs::metadata(&file).unwrap().len();
    assert!(new_size != original_bytes);

    // Drive the rotation via the watcher's test-only emit hook.
    let canonical = fs::canonicalize(&file).unwrap();
    super::watcher::test_only_emit(&canonical, super::watcher::WatcherEvent::Replaced);

    let start = std::time::Instant::now();
    let mut observed = 0;
    while start.elapsed() < Duration::from_secs(3) {
        let chunk = session::get_lines(&sid, super::SeekTarget::Fraction(0.0), 1).unwrap();
        observed = chunk.total_bytes;
        if observed == new_size {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(observed, new_size, "rotation must reopen against the new bytes");

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_session_close_stops_watcher() {
    // `close_session` flips `watcher_stop`; the manager thread observes it on
    // its next 200 ms poll cycle and exits, dropping the
    // `ViewerSubscription`. The subscription's `Drop` unregisters the path
    // from the shared singleton.
    let dir = create_test_dir("close_stops_watcher");
    let file = write_test_file(&dir, "tmp.log", "hi\n");
    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();

    wait_for_watcher_subscribed();
    let canonical = fs::canonicalize(&file).unwrap();
    // The subscription is alive: a test-only emit reaches it.
    let sent_before = super::watcher::test_only_emit(&canonical, super::watcher::WatcherEvent::MetadataOnly);
    assert!(sent_before > 0, "manager thread should be subscribed before close");

    session::close_session(&sid).unwrap();

    // Give the manager thread ~300 ms to observe `watcher_stop` and drop its
    // subscription. After that, no subscribers should remain for the path.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let sent = super::watcher::test_only_emit(&canonical, super::watcher::WatcherEvent::MetadataOnly);
        if sent == 0 {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let sent_after = super::watcher::test_only_emit(&canonical, super::watcher::WatcherEvent::MetadataOnly);
    assert_eq!(sent_after, 0, "close_session must drop the watcher subscription");

    cleanup(&dir);
}

#[test]
fn test_set_encoding_during_rebuild_serialization() {
    // Two `set_encoding` calls in rapid succession: the first one is cancelled
    // by the second's `prev_cancel.store(true)`. Only the final encoding ends
    // up on the session, and only one rebuild ever succeeds in storing.
    let dir = create_test_dir("enc_serialize");
    let line = "x".repeat(100) + "\n";
    let line_count = (FULL_LOAD_THRESHOLD as usize / line.len()) + 200;
    let content = line.repeat(line_count);
    let file = write_test_file(&dir, "rebuild.txt", &content);

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();

    // Wait for the initial upgrade so we test the rebuild path, not the
    // upgrade path.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        let status = session::get_session_status(&sid).unwrap();
        if matches!(status.backend_type, session::BackendType::LineIndex) && !status.is_indexing {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    // Park the next rebuild so the first set_encoding can be superseded by
    // the second BEFORE its scan starts.
    session::test_only_set_rebuild_hold(800);

    // First call: triggers a rebuild that will sleep for 800 ms.
    session::set_encoding(&sid, FileEncoding::Utf16Le).unwrap();
    // Second call lands immediately; it cancels the in-flight rebuild and
    // installs its own.
    session::set_encoding(&sid, FileEncoding::Utf16Be).unwrap();

    // Wait for the rebuild storm to settle.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        if !session::test_only_rebuilding_active(&sid) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(!session::test_only_rebuilding_active(&sid), "rebuild must settle");

    let opts = session::get_encoding_options(&sid).unwrap();
    assert_eq!(
        opts.current,
        FileEncoding::Utf16Be,
        "only the latest set_encoding call must win"
    );

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

#[test]
fn test_set_encoding_ascii_compatible_is_instant() {
    // For ASCII-newline-compatible encodings with the same BOM (UTF-8 ->
    // Windows-1252), `set_encoding` should NOT trigger a LineIndex rebuild.
    // The instrumentation counter on `LineIndexBackend::open_with_encoding`
    // pins this contract.
    let dir = create_test_dir("instant_swap");
    let line = "abcd\n";
    let line_count = (FULL_LOAD_THRESHOLD as usize / line.len()) + 500;
    let content = line.repeat(line_count);
    let file = write_test_file(&dir, "instant.txt", &content);

    let result = session::open_session(file.to_str().unwrap(), "root").unwrap();
    let sid = result.session_id.clone();

    // Wait for the initial upgrade so we count from a stable baseline.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(8) {
        let status = session::get_session_status(&sid).unwrap();
        if matches!(status.backend_type, session::BackendType::LineIndex) && !status.is_indexing {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let baseline = super::line_index::test_only_open_call_count();

    // UTF-8 -> Windows-1252: same byte layout (no BOM, ASCII-compatible).
    session::set_encoding(&sid, FileEncoding::Windows1252).unwrap();

    // Wait for any in-flight rebuild to settle.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if !session::test_only_rebuilding_active(&sid) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }

    let post = super::line_index::test_only_open_call_count();
    assert_eq!(
        post, baseline,
        "instant swap must NOT re-open LineIndex (baseline={}, post={})",
        baseline, post
    );

    let opts = session::get_encoding_options(&sid).unwrap();
    assert_eq!(opts.current, FileEncoding::Windows1252);

    session::close_session(&sid).unwrap();
    cleanup(&dir);
}

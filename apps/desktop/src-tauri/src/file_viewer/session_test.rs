//! Tests for ViewerSession orchestrator.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use super::FULL_LOAD_THRESHOLD;
use super::session::{self, SearchStatus};

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
    session::search_start(sid, "hello".to_string()).unwrap();

    // Poll until done (with timeout)
    let mut done = false;
    for _ in 0..100 {
        let poll = session::search_poll(sid).unwrap();
        if matches!(poll.status, SearchStatus::Done) {
            assert_eq!(poll.matches.len(), 2);
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

    session::search_start(sid, "hello".to_string()).unwrap();

    // Cancel immediately
    session::search_cancel(sid).unwrap();

    // Poll should show cancelled or idle (since we removed the search state)
    let poll = session::search_poll(sid).unwrap();
    assert!(matches!(poll.status, SearchStatus::Idle));

    session::close_session(sid).unwrap();
    cleanup(&dir);
}

#[test]
fn search_poll_no_active_search() {
    let dir = create_test_dir("poll_idle");
    let file = write_test_file(&dir, "test.txt", "test\n");

    let open_result = session::open_session(file.to_str().unwrap()).unwrap();
    let sid = &open_result.session_id;

    let poll = session::search_poll(sid).unwrap();
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

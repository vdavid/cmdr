//! Tests for ByteSeekBackend.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use super::byte_seek::ByteSeekBackend;
use super::{FileViewerBackend, SearchMatch, SeekTarget};

fn create_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_viewer_byte_{}", name));
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
fn open_succeeds() {
    let dir = create_test_dir("open");
    let file = write_test_file(&dir, "test.txt", "hello world\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    assert_eq!(backend.file_name(), "test.txt");
    assert_eq!(backend.total_bytes(), 12);
    assert_eq!(backend.total_lines(), None); // ByteSeek doesn't know total lines

    cleanup(&dir);
}

#[test]
fn open_not_found() {
    let result = ByteSeekBackend::open(&PathBuf::from("/nonexistent_byte_seek_test.txt"));
    assert!(result.is_err());
}

#[test]
fn open_directory_fails() {
    let dir = create_test_dir("open_dir");
    let result = ByteSeekBackend::open(&dir);
    assert!(result.is_err());
    cleanup(&dir);
}

#[test]
fn get_lines_from_start() {
    let dir = create_test_dir("lines_start");
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\nline 4\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend.get_lines(&SeekTarget::ByteOffset(0), 3).unwrap();

    assert_eq!(chunk.lines, vec!["line 1", "line 2", "line 3"]);
    assert_eq!(chunk.byte_offset, 0);
    assert_eq!(chunk.total_lines, None);

    cleanup(&dir);
}

#[test]
fn get_lines_from_middle_byte_offset() {
    let dir = create_test_dir("lines_mid");
    // "line 1\n" = 7 bytes, so byte 7 starts "line 2"
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\nline 4\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend.get_lines(&SeekTarget::ByteOffset(7), 2).unwrap();

    assert_eq!(chunk.lines, vec!["line 2", "line 3"]);
    assert_eq!(chunk.byte_offset, 7);

    cleanup(&dir);
}

#[test]
fn get_lines_with_backward_scan() {
    let dir = create_test_dir("backward_scan");
    // Seeking to byte 10 (middle of "line 2") should scan back to start of "line 2"
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend.get_lines(&SeekTarget::ByteOffset(10), 2).unwrap();

    // Should find start of "line 2" (byte 7)
    assert_eq!(chunk.byte_offset, 7);
    assert_eq!(chunk.lines[0], "line 2");

    cleanup(&dir);
}

#[test]
fn get_lines_by_fraction() {
    let dir = create_test_dir("fraction");
    let content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
    let file = write_test_file(&dir, "test.txt", content);

    let backend = ByteSeekBackend::open(&file).unwrap();

    // Fraction 0.0 should start at beginning
    let chunk = backend.get_lines(&SeekTarget::Fraction(0.0), 1).unwrap();
    assert_eq!(chunk.byte_offset, 0);
    assert_eq!(chunk.lines[0], "line 1");

    cleanup(&dir);
}

#[test]
fn get_lines_fraction_end() {
    let dir = create_test_dir("fraction_end");
    let content = "line 1\nline 2\nline 3\n";
    let file = write_test_file(&dir, "test.txt", content);

    let backend = ByteSeekBackend::open(&file).unwrap();

    // Fraction 1.0 should go to end (byte 21)
    let chunk = backend.get_lines(&SeekTarget::Fraction(1.0), 1).unwrap();
    // Should find the last line or be at/near end
    assert!(chunk.byte_offset > 0);

    cleanup(&dir);
}

#[test]
fn get_lines_line_target_defaults_to_start() {
    let dir = create_test_dir("line_target");
    let file = write_test_file(&dir, "test.txt", "a\nb\nc\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    // ByteSeek doesn't support line seeking — defaults to start
    let chunk = backend.get_lines(&SeekTarget::Line(5), 2).unwrap();
    assert_eq!(chunk.byte_offset, 0);
    assert_eq!(chunk.lines[0], "a");

    cleanup(&dir);
}

#[test]
fn get_lines_last_line_no_newline() {
    let dir = create_test_dir("no_trailing_nl");
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend.get_lines(&SeekTarget::ByteOffset(0), 10).unwrap();

    assert_eq!(chunk.lines, vec!["line 1", "line 2"]);

    cleanup(&dir);
}

#[test]
fn search_finds_matches() {
    let dir = create_test_dir("search");
    let file = write_test_file(&dir, "test.txt", "hello world\nfoo bar\nhello again\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("hello", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[0].column, 0);
    assert_eq!(matches[1].line, 2);

    cleanup(&dir);
}

#[test]
fn search_case_insensitive() {
    let dir = create_test_dir("search_case");
    let file = write_test_file(&dir, "test.txt", "Hello\nHELLO\nhello\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("hello", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 3);

    cleanup(&dir);
}

#[test]
fn search_cancellation() {
    let dir = create_test_dir("search_cancel");
    let content = "hello world\n".repeat(10000);
    let file = write_test_file(&dir, "test.txt", &content);

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(true); // Pre-cancelled
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("hello", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    // Should stop early
    assert!(matches.len() < 10000);

    cleanup(&dir);
}

#[test]
fn search_no_matches() {
    let dir = create_test_dir("search_none");
    let file = write_test_file(&dir, "test.txt", "abc\ndef\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("xyz", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 0);

    cleanup(&dir);
}

#[test]
fn capabilities_correct() {
    let dir = create_test_dir("caps");
    let file = write_test_file(&dir, "test.txt", "test\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let caps = backend.capabilities();

    assert!(!caps.supports_line_seek);
    assert!(caps.supports_byte_seek);
    assert!(caps.supports_fraction_seek);
    assert!(!caps.knows_total_lines);

    cleanup(&dir);
}

#[test]
fn backward_scan_with_no_newline_caps_at_max() {
    let dir = create_test_dir("no_nl");
    // Write a file with no newlines (simulates binary)
    let content = "x".repeat(20000);
    let file = write_test_file(&dir, "test.bin", &content);

    let backend = ByteSeekBackend::open(&file).unwrap();

    // Seek to byte 15000 — backward scan of 8192 bytes won't find '\n'
    let chunk = backend.get_lines(&SeekTarget::ByteOffset(15000), 1).unwrap();

    // Should fall back to scan_start = 15000 - 8192 = 6808
    assert_eq!(chunk.byte_offset, 15000 - 8192);

    cleanup(&dir);
}

#[test]
fn empty_file() {
    let dir = create_test_dir("empty");
    let file = write_test_file(&dir, "empty.txt", "");

    let backend = ByteSeekBackend::open(&file).unwrap();
    assert_eq!(backend.total_bytes(), 0);

    let chunk = backend.get_lines(&SeekTarget::ByteOffset(0), 10).unwrap();
    // Empty file should produce empty lines
    assert!(chunk.lines.is_empty() || (chunk.lines.len() == 1 && chunk.lines[0].is_empty()));

    cleanup(&dir);
}

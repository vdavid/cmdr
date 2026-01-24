//! Tests for LineIndexBackend.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use super::line_index::LineIndexBackend;
use super::{FileViewerBackend, INDEX_CHECKPOINT_INTERVAL, SearchMatch, SeekTarget};

fn create_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_viewer_lidx_{}", name));
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
fn open_builds_index() {
    let dir = create_test_dir("open");
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\n");

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();

    assert_eq!(backend.total_lines(), Some(4)); // 3 lines + trailing empty
    assert_eq!(backend.file_name(), "test.txt");
    assert_eq!(backend.total_bytes(), 21);

    cleanup(&dir);
}

#[test]
fn open_not_found() {
    let cancel = AtomicBool::new(false);
    let result = LineIndexBackend::open(&PathBuf::from("/nonexistent_lidx_test.txt"), &cancel);
    assert!(result.is_err());
}

#[test]
fn open_directory_fails() {
    let dir = create_test_dir("open_dir");
    let cancel = AtomicBool::new(false);
    let result = LineIndexBackend::open(&dir, &cancel);
    assert!(result.is_err());
    cleanup(&dir);
}

#[test]
fn open_cancellation() {
    let dir = create_test_dir("cancel");
    // Create a file with enough lines to potentially hit the cancel check
    let content = "line\n".repeat(1000);
    let file = write_test_file(&dir, "test.txt", &content);

    let cancel = AtomicBool::new(true); // Pre-cancelled
    let result = LineIndexBackend::open(&file, &cancel);
    assert!(result.is_err());

    cleanup(&dir);
}

#[test]
fn get_lines_from_start() {
    let dir = create_test_dir("lines_start");
    let file = write_test_file(&dir, "test.txt", "alpha\nbeta\ngamma\ndelta\n");

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();

    let chunk = backend.get_lines(&SeekTarget::Line(0), 3).unwrap();
    assert_eq!(chunk.lines, vec!["alpha", "beta", "gamma"]);
    assert_eq!(chunk.first_line_number, 0);
    assert_eq!(chunk.total_lines, Some(5));

    cleanup(&dir);
}

#[test]
fn get_lines_from_middle() {
    let dir = create_test_dir("lines_mid");
    let file = write_test_file(&dir, "test.txt", "a\nb\nc\nd\ne\nf\n");

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();

    let chunk = backend.get_lines(&SeekTarget::Line(3), 2).unwrap();
    assert_eq!(chunk.lines, vec!["d", "e"]);
    assert_eq!(chunk.first_line_number, 3);

    cleanup(&dir);
}

#[test]
fn get_lines_past_end() {
    let dir = create_test_dir("lines_end");
    let file = write_test_file(&dir, "test.txt", "a\nb\nc\n");

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();

    let chunk = backend.get_lines(&SeekTarget::Line(10), 5).unwrap();
    // Should clamp to last line
    assert_eq!(chunk.first_line_number, 3); // 4 lines (including trailing empty), last is index 3

    cleanup(&dir);
}

#[test]
fn get_lines_by_fraction() {
    let dir = create_test_dir("fraction");
    let content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
    let file = write_test_file(&dir, "test.txt", content);

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();

    // Fraction 0.0 = first line
    let chunk = backend.get_lines(&SeekTarget::Fraction(0.0), 1).unwrap();
    assert_eq!(chunk.first_line_number, 0);
    assert_eq!(chunk.lines[0], "line 1");

    cleanup(&dir);
}

#[test]
fn get_lines_no_trailing_newline() {
    let dir = create_test_dir("no_trail");
    let file = write_test_file(&dir, "test.txt", "a\nb\nc");

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();

    let chunk = backend.get_lines(&SeekTarget::Line(0), 10).unwrap();
    assert_eq!(chunk.lines, vec!["a", "b", "c"]);
    assert_eq!(backend.total_lines(), Some(3));

    cleanup(&dir);
}

#[test]
fn sparse_index_checkpoints() {
    let dir = create_test_dir("checkpoints");
    // Create a file with more than INDEX_CHECKPOINT_INTERVAL lines
    let line_count = INDEX_CHECKPOINT_INTERVAL * 3 + 50;
    let content: String = (0..line_count).map(|i| format!("line {:06}\n", i)).collect();
    let file = write_test_file(&dir, "test.txt", &content);

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();

    // Should have total_lines correct
    assert_eq!(backend.total_lines(), Some(line_count + 1)); // +1 for trailing empty

    // Seek to a line past the first checkpoint
    let target_line = INDEX_CHECKPOINT_INTERVAL + 10;
    let chunk = backend.get_lines(&SeekTarget::Line(target_line), 3).unwrap();
    assert_eq!(chunk.first_line_number, target_line);
    assert_eq!(chunk.lines[0], format!("line {:06}", target_line));

    // Seek to a line past the second checkpoint
    let target_line2 = INDEX_CHECKPOINT_INTERVAL * 2 + 5;
    let chunk2 = backend.get_lines(&SeekTarget::Line(target_line2), 2).unwrap();
    assert_eq!(chunk2.first_line_number, target_line2);
    assert_eq!(chunk2.lines[0], format!("line {:06}", target_line2));

    cleanup(&dir);
}

#[test]
fn search_finds_matches() {
    let dir = create_test_dir("search");
    let file = write_test_file(&dir, "test.txt", "hello world\nfoo bar\nhello again\n");

    let cancel_scan = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel_scan).unwrap();

    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("hello", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[1].line, 2);

    cleanup(&dir);
}

#[test]
fn search_case_insensitive() {
    let dir = create_test_dir("search_case");
    let file = write_test_file(&dir, "test.txt", "Hello\nHELLO\nhello\n");

    let cancel_scan = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel_scan).unwrap();

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

    let cancel_scan = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel_scan).unwrap();

    let cancel = AtomicBool::new(true);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("hello", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    assert!(matches.len() < 10000);

    cleanup(&dir);
}

#[test]
fn capabilities_correct() {
    let dir = create_test_dir("caps");
    let file = write_test_file(&dir, "test.txt", "test\n");

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();
    let caps = backend.capabilities();

    assert!(caps.supports_line_seek);
    assert!(caps.supports_byte_seek);
    assert!(caps.supports_fraction_seek);
    assert!(caps.knows_total_lines);

    cleanup(&dir);
}

#[test]
fn empty_file() {
    let dir = create_test_dir("empty");
    let file = write_test_file(&dir, "test.txt", "");

    let cancel = AtomicBool::new(false);
    let backend = LineIndexBackend::open(&file, &cancel).unwrap();

    assert_eq!(backend.total_bytes(), 0);
    assert_eq!(backend.total_lines(), Some(1)); // One empty line

    cleanup(&dir);
}

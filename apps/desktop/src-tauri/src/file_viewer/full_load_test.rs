//! Tests for FullLoadBackend.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use super::full_load::FullLoadBackend;
use super::{FileViewerBackend, SearchMatch, SeekTarget};

fn create_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cmdr_viewer_full_{}", name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("Failed to create test directory");
    dir
}

fn cleanup(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

#[test]
fn open_reads_lines_correctly() {
    let dir = create_test_dir("open_lines");
    let file = dir.join("test.txt");
    fs::write(&file, "line 1\nline 2\nline 3\n").unwrap();

    let backend = FullLoadBackend::open(&file).unwrap();
    assert_eq!(backend.total_lines(), Some(4)); // 3 lines + trailing empty
    assert_eq!(backend.file_name(), "test.txt");
    assert_eq!(backend.total_bytes(), 21);

    cleanup(&dir);
}

#[test]
fn open_empty_file() {
    let dir = create_test_dir("open_empty");
    let file = dir.join("empty.txt");
    fs::write(&file, "").unwrap();

    let backend = FullLoadBackend::open(&file).unwrap();
    assert_eq!(backend.total_lines(), Some(1)); // At least one line
    assert_eq!(backend.total_bytes(), 0);

    cleanup(&dir);
}

#[test]
fn open_not_found() {
    let result = FullLoadBackend::open(&PathBuf::from("/nonexistent_viewer_test_12345.txt"));
    assert!(result.is_err());
}

#[test]
fn open_directory_fails() {
    let dir = create_test_dir("open_dir");
    let result = FullLoadBackend::open(&dir);
    assert!(result.is_err());
    cleanup(&dir);
}

#[test]
fn get_lines_from_start() {
    let backend = FullLoadBackend::from_content("alpha\nbeta\ngamma\ndelta\nepsilon", "test.txt");

    let chunk = backend.get_lines(&SeekTarget::Line(0), 3).unwrap();
    assert_eq!(chunk.lines, vec!["alpha", "beta", "gamma"]);
    assert_eq!(chunk.first_line_number, 0);
    assert_eq!(chunk.total_lines, Some(5));
}

#[test]
fn get_lines_from_middle() {
    let backend = FullLoadBackend::from_content("a\nb\nc\nd\ne\nf\ng", "test.txt");

    let chunk = backend.get_lines(&SeekTarget::Line(3), 2).unwrap();
    assert_eq!(chunk.lines, vec!["d", "e"]);
    assert_eq!(chunk.first_line_number, 3);
}

#[test]
fn get_lines_past_end() {
    let backend = FullLoadBackend::from_content("a\nb\nc", "test.txt");

    let chunk = backend.get_lines(&SeekTarget::Line(10), 5).unwrap();
    // Should clamp to last line
    assert_eq!(chunk.first_line_number, 2);
    assert_eq!(chunk.lines, vec!["c"]);
}

#[test]
fn get_lines_by_byte_offset() {
    let backend = FullLoadBackend::from_content("abc\ndef\nghi", "test.txt");

    // Byte offset 4 is start of "def"
    let chunk = backend.get_lines(&SeekTarget::ByteOffset(4), 2).unwrap();
    assert_eq!(chunk.first_line_number, 1);
    assert_eq!(chunk.lines, vec!["def", "ghi"]);
}

#[test]
fn get_lines_by_fraction() {
    let backend = FullLoadBackend::from_content("a\nb\nc\nd\ne", "test.txt");

    // Fraction 0.5 on 5 lines = line 2 or 3
    let chunk = backend.get_lines(&SeekTarget::Fraction(0.5), 1).unwrap();
    assert!(chunk.first_line_number == 2 || chunk.first_line_number == 3);
}

#[test]
fn get_lines_fraction_zero() {
    let backend = FullLoadBackend::from_content("a\nb\nc", "test.txt");

    let chunk = backend.get_lines(&SeekTarget::Fraction(0.0), 1).unwrap();
    assert_eq!(chunk.first_line_number, 0);
    assert_eq!(chunk.lines, vec!["a"]);
}

#[test]
fn get_lines_fraction_one() {
    let backend = FullLoadBackend::from_content("a\nb\nc", "test.txt");

    let chunk = backend.get_lines(&SeekTarget::Fraction(1.0), 1).unwrap();
    assert_eq!(chunk.first_line_number, 2);
    assert_eq!(chunk.lines, vec!["c"]);
}

#[test]
fn search_finds_matches() {
    let backend = FullLoadBackend::from_content("hello world\nfoo bar\nhello again", "test.txt");

    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    let scanned = backend.search("hello", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[0].column, 0);
    assert_eq!(matches[1].line, 2);
    assert_eq!(matches[1].column, 0);
    assert!(scanned > 0);
}

#[test]
fn search_case_insensitive() {
    let backend = FullLoadBackend::from_content("Hello World\nHELLO\nhello", "test.txt");

    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("hello", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 3);
}

#[test]
fn search_multiple_per_line() {
    let backend = FullLoadBackend::from_content("aaa", "test.txt");

    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("aa", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    // "aaa" contains "aa" at positions 0 and 1
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].column, 0);
    assert_eq!(matches[1].column, 1);
}

#[test]
fn search_cancellation() {
    let backend = FullLoadBackend::from_content(&"line with hello\n".repeat(10000), "test.txt");

    let cancel = AtomicBool::new(true); // Already cancelled
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("hello", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    // Should find zero or very few matches since cancelled immediately
    assert!(matches.len() < 10000);
}

#[test]
fn search_no_matches() {
    let backend = FullLoadBackend::from_content("abc\ndef\nghi", "test.txt");

    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());

    backend.search("xyz", &cancel, &results).unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 0);
}

#[test]
fn capabilities_correct() {
    let backend = FullLoadBackend::from_content("test", "test.txt");
    let caps = backend.capabilities();

    assert!(caps.supports_line_seek);
    assert!(caps.supports_byte_seek);
    assert!(caps.supports_fraction_seek);
    assert!(caps.knows_total_lines);
}

#[test]
fn binary_content_handled() {
    let dir = create_test_dir("binary");
    let file = dir.join("binary.bin");
    fs::write(&file, b"\x00\x01\x02\xff\xfe\n\x03\x04").unwrap();

    let backend = FullLoadBackend::open(&file).unwrap();
    let chunk = backend.get_lines(&SeekTarget::Line(0), 10).unwrap();

    // Should have 2 lines (split on \n)
    assert_eq!(chunk.lines.len(), 2);
    // Binary bytes become replacement characters
    assert!(chunk.lines[0].contains('\u{FFFD}'));

    cleanup(&dir);
}

#[test]
fn single_line_no_newline() {
    let backend = FullLoadBackend::from_content("just one line", "test.txt");

    assert_eq!(backend.total_lines(), Some(1));
    let chunk = backend.get_lines(&SeekTarget::Line(0), 10).unwrap();
    assert_eq!(chunk.lines, vec!["just one line"]);
}

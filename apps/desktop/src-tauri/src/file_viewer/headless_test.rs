//! Unit tests for the headless text-backend opener the agent's `inspect_file` rides.

use std::sync::atomic::{AtomicBool, Ordering};

use super::FULL_LOAD_THRESHOLD;
use super::encoding::FileEncoding;
use super::headless::open_text_backend;
use super::{SeekTarget, ViewerError};
use crate::test_support::TestDir;

/// A file of `lines` numbered lines, each padded so the whole thing lands past the
/// FullLoad threshold when the caller wants it to.
fn write_numbered(dir: &TestDir, name: &str, lines: usize) -> std::path::PathBuf {
    let path = dir.join(name);
    let mut body = String::with_capacity(lines * 24);
    for n in 1..=lines {
        body.push_str(&format!("line {n:06} padding........\n"));
    }
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn a_small_file_opens_full_load_with_exact_lines() {
    let dir = TestDir::new("headless_small");
    let path = write_numbered(&dir, "small.txt", 10);
    let cancel = AtomicBool::new(false);
    let opened = open_text_backend(&path, FileEncoding::Utf8, &cancel).unwrap();
    assert!(opened.line_numbers_exact);
    assert_eq!(
        opened.backend.total_lines(),
        Some(11),
        "10 lines plus the trailing empty one"
    );
    let chunk = opened.backend.get_lines(&SeekTarget::Line(7), 2).unwrap();
    assert_eq!(chunk.first_line_number, 7);
    assert!(chunk.lines[0].starts_with("line 000008"));
}

#[test]
fn a_large_file_opens_line_index_with_exact_lines() {
    let dir = TestDir::new("headless_large");
    // 25 bytes a line, so 50,000 lines is ~1.25 MB: past the FullLoad threshold.
    let path = write_numbered(&dir, "large.txt", 50_000);
    assert!(std::fs::metadata(&path).unwrap().len() > FULL_LOAD_THRESHOLD);
    let cancel = AtomicBool::new(false);
    let opened = open_text_backend(&path, FileEncoding::Utf8, &cancel).unwrap();
    assert!(opened.line_numbers_exact);
    assert_eq!(opened.backend.total_lines(), Some(50_001));
    let chunk = opened.backend.get_lines(&SeekTarget::Line(40_000), 1).unwrap();
    assert_eq!(chunk.first_line_number, 40_000);
    assert!(chunk.lines[0].starts_with("line 040001"), "got {:?}", chunk.lines[0]);
}

#[test]
fn a_cancelled_index_build_falls_back_to_byte_seek_and_says_lines_are_approximate() {
    let dir = TestDir::new("headless_cancel");
    let path = write_numbered(&dir, "large.txt", 50_000);
    // The deadline already passed before the scan started: the index build must bail
    // out and the caller still gets a backend, flagged approximate.
    let cancel = AtomicBool::new(true);
    let opened = open_text_backend(&path, FileEncoding::Utf8, &cancel).unwrap();
    assert!(!opened.line_numbers_exact);
    assert_eq!(opened.backend.total_lines(), None);
    assert!(!opened.backend.capabilities().knows_total_lines);
    let chunk = opened.backend.get_lines(&SeekTarget::Line(0), 2).unwrap();
    assert!(chunk.lines[0].starts_with("line 000001"));
    // The flag is the caller's; the fallback must not clear it.
    assert!(cancel.load(Ordering::Relaxed));
}

#[test]
fn a_missing_file_is_a_typed_not_found() {
    let dir = TestDir::new("headless_missing");
    let cancel = AtomicBool::new(false);
    let Err(err) = open_text_backend(&dir.join("nope.txt"), FileEncoding::Utf8, &cancel) else {
        panic!("a missing file must not open");
    };
    assert!(matches!(err, ViewerError::NotFound { .. }), "got {err:?}");
}

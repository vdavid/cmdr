//! Tests for ByteSeekBackend.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use super::byte_seek::ByteSeekBackend;
use super::search_cancel_test_support::{assert_search_stops_on_per_match_cancel, many_matches_corpus};
use super::search_matcher::{Matcher, SearchMode};
use super::{FileViewerBackend, MAX_SEARCH_MATCHES, SearchMatch, SeekTarget, ViewerError};
use crate::test_support::TestDir;

/// Build a literal matcher for tests. Mirrors the pre-mode "lowercase substring"
/// default but keeps the matcher visible in each call site.
fn literal_matcher(query: &str, case_sensitive: bool) -> Matcher {
    Matcher::build(
        query,
        SearchMode {
            use_regex: false,
            case_sensitive,
        },
    )
    .expect("test query must build")
}

fn create_test_dir(name: &str) -> TestDir {
    TestDir::new(&format!("viewer_byte_{}", name))
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
}

#[test]
fn get_lines_from_start() {
    let dir = create_test_dir("lines_start");
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\nline 4\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(0), 3, &AtomicBool::new(false))
        .unwrap();

    assert_eq!(chunk.texts(), vec!["line 1", "line 2", "line 3"]);
    assert_eq!(chunk.byte_offset, 0);
    // ByteSeek has no index, so its row count is an estimate rather than nothing: the
    // frontend needs a scroll extent from the first fetch.
    assert!(!chunk.total_rows.is_exact());
}

#[test]
fn get_lines_from_middle_byte_offset() {
    let dir = create_test_dir("lines_mid");
    // "line 1\n" = 7 bytes, so byte 7 starts "line 2"
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\nline 4\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(7), 2, &AtomicBool::new(false))
        .unwrap();

    assert_eq!(chunk.texts(), vec!["line 2", "line 3"]);
    assert_eq!(chunk.byte_offset, 7);
}

#[test]
fn get_lines_with_backward_scan() {
    let dir = create_test_dir("backward_scan");
    // Seeking to byte 10 (middle of "line 2") should scan back to start of "line 2"
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(10), 2, &AtomicBool::new(false))
        .unwrap();

    // Should find start of "line 2" (byte 7)
    assert_eq!(chunk.byte_offset, 7);
    assert_eq!(chunk.texts()[0], "line 2");
}

#[test]
fn get_lines_by_fraction() {
    let dir = create_test_dir("fraction");
    let content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
    let file = write_test_file(&dir, "test.txt", content);

    let backend = ByteSeekBackend::open(&file).unwrap();

    // Fraction 0.0 should start at beginning
    let chunk = backend
        .get_lines(&SeekTarget::Fraction(0.0), 1, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(chunk.byte_offset, 0);
    assert_eq!(chunk.texts()[0], "line 1");
}

#[test]
fn get_lines_fraction_end() {
    let dir = create_test_dir("fraction_end");
    let content = "line 1\nline 2\nline 3\n";
    let file = write_test_file(&dir, "test.txt", content);

    let backend = ByteSeekBackend::open(&file).unwrap();

    // Fraction 1.0 should go to end (byte 21)
    let chunk = backend
        .get_lines(&SeekTarget::Fraction(1.0), 1, &AtomicBool::new(false))
        .unwrap();
    // Should find the last line or be at/near end
    assert!(chunk.byte_offset > 0);
}

#[test]
fn get_lines_row_target_rides_the_sampled_bytes_per_row() {
    let dir = create_test_dir("line_target");
    let file = write_test_file(&dir, "test.txt", "a\nb\nc\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::Row(0), 2, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(chunk.byte_offset, 0);
    assert_eq!(chunk.texts(), vec!["a", "b"]);

    // Every row here is 2 bytes, which is what the open-time sample measures, so row 2
    // lands on byte 4 rather than at `2 * 80` past the end of a six-byte file.
    let chunk2 = backend
        .get_lines(&SeekTarget::Row(2), 2, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(chunk2.byte_offset, 4);
    // "c", then the empty row a file ending in a newline carries.
    assert_eq!(chunk2.texts(), vec!["c", ""]);

    // Past the end still clamps to EOF, where only that final empty row is left.
    let chunk3 = backend
        .get_lines(&SeekTarget::Row(50), 2, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(chunk3.byte_offset, 6);
    assert_eq!(chunk3.texts(), vec![""]);
}

#[test]
fn get_lines_last_line_no_newline() {
    let dir = create_test_dir("no_trailing_nl");
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(0), 10, &AtomicBool::new(false))
        .unwrap();

    assert_eq!(chunk.texts(), vec!["line 1", "line 2"]);
}

#[test]
fn search_finds_matches() {
    let dir = create_test_dir("search");
    let file = write_test_file(&dir, "test.txt", "hello world\nfoo bar\nhello again\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    backend
        .search(&literal_matcher("hello", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].row, 0);
    assert_eq!(matches[0].column, 0);
    assert_eq!(matches[0].byte_offset, 0); // First line starts at byte 0
    assert_eq!(matches[1].row, 2);
    // "hello world\n" = 12 bytes, "foo bar\n" = 8 bytes → line 2 starts at byte 20
    assert_eq!(matches[1].byte_offset, 20);

    // Progress should equal total bytes after search completes
    assert_eq!(*progress.lock().unwrap(), backend.total_bytes());
}

#[test]
fn search_case_insensitive() {
    let dir = create_test_dir("search_case");
    let file = write_test_file(&dir, "test.txt", "Hello\nHELLO\nhello\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    backend
        .search(&literal_matcher("hello", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 3);
}

#[test]
fn search_cancellation() {
    let dir = create_test_dir("search_cancel");
    let content = "hello world\n".repeat(10000);
    let file = write_test_file(&dir, "test.txt", &content);

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(true); // Pre-cancelled
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    backend
        .search(&literal_matcher("hello", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    // Should stop early
    assert!(matches.len() < 10000);
}

#[test]
fn search_no_matches() {
    let dir = create_test_dir("search_none");
    let file = write_test_file(&dir, "test.txt", "abc\ndef\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    backend
        .search(&literal_matcher("xyz", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 0);
}

#[test]
fn search_with_multibyte_chars() {
    let dir = create_test_dir("search_multibyte");
    // "café" has a multi-byte 'é' (2 bytes in UTF-8, 1 character)
    let file = write_test_file(&dir, "test.txt", "café latte\nplain text\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    backend
        .search(&literal_matcher("latte", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].row, 0);
    // "café " is 5 characters, not 6 bytes
    assert_eq!(matches[0].column, 5);
    assert_eq!(matches[0].length, 5);
}

#[test]
fn search_with_replacement_chars() {
    let dir = create_test_dir("search_replacement");
    // Write raw bytes: invalid UTF-8 byte followed by "PNG header\n"
    let mut content = vec![0x89u8]; // Invalid UTF-8 start byte
    content.extend_from_slice(b"PNG header\nmore data\n");
    let file = dir.join("test.bin");
    fs::write(&file, &content).unwrap();

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    backend
        .search(&literal_matcher("png", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].row, 0);
    // Column should be 1 (after replacement char), not 3 (byte offset of U+FFFD)
    assert_eq!(matches[0].column, 1);
    assert_eq!(matches[0].length, 3);
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
}

#[test]
fn a_newline_free_file_breaks_on_the_segment_grid() {
    let dir = create_test_dir("no_nl");
    // No newlines anywhere (a minified bundle, or a binary).
    let content = "x".repeat(50_000);
    let file = write_test_file(&dir, "test.bin", &content);

    let backend = ByteSeekBackend::open(&file).unwrap();

    // The old backward scan capped at 8 192 bytes and then called wherever it stopped a
    // line start, which put byte 15 000 at 6 808: an answer that moved with the probe.
    // The row rule puts it on the segment grid, so every probe inside a row agrees.
    for probe in [15_000u64, 20_000, 39_999] {
        let chunk = backend
            .get_lines(&SeekTarget::ByteOffset(probe), 1, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(chunk.byte_offset, probe - probe % super::SEGMENT_BYTES, "probe {probe}");
        assert_eq!(chunk.rows[0].text.len(), super::SEGMENT_BYTES as usize, "probe {probe}");
        // Cmdr made this break, so it carries the marker and no line number.
        assert!(chunk.rows[0].continues, "probe {probe}");
    }

    // And the file's last row runs out at EOF rather than at a boundary.
    let tail = backend
        .get_lines(&SeekTarget::ByteOffset(45_000), 1, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(tail.byte_offset, 40_000);
    assert!(!tail.rows[0].continues);
    assert_eq!(tail.rows[0].text.len(), 10_000);
}

#[test]
fn empty_file() {
    let dir = create_test_dir("empty");
    let file = write_test_file(&dir, "empty.txt", "");

    let backend = ByteSeekBackend::open(&file).unwrap();
    assert_eq!(backend.total_bytes(), 0);

    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(0), 10, &AtomicBool::new(false))
        .unwrap();
    // Empty file should produce empty lines
    assert!(chunk.texts().is_empty() || (chunk.texts().len() == 1 && chunk.texts()[0].is_empty()));
}

#[test]
fn search_caps_at_match_limit() {
    let dir = create_test_dir("search_cap");
    let line_count = MAX_SEARCH_MATCHES + 1000;
    let content = "aa\n".repeat(line_count);
    let file = write_test_file(&dir, "test.txt", &content);

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    let scanned = backend
        .search(&literal_matcher("a", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    // Should cap at exactly MAX_SEARCH_MATCHES
    assert_eq!(matches.len(), MAX_SEARCH_MATCHES);
    // Should stop scanning early (not read the whole file)
    assert!(scanned < backend.total_bytes());
    assert!(scanned > 0);
}

// ─── Multi-byte UTF-8 tests ────────────────────────────────────────────

#[test]
fn seek_mid_multibyte_char_snaps_to_line_start() {
    let dir = create_test_dir("mid_utf8");
    // "café\n" = 6 bytes (c=1, a=1, f=1, é=2, \n=1). Byte 4 lands inside 'é'.
    let file = write_test_file(&dir, "test.txt", "café\nplain\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(4), 2, &AtomicBool::new(false))
        .unwrap();

    // Should backward-scan to byte 0 (start of "café") since byte 4 is mid-char inside first line
    assert_eq!(chunk.byte_offset, 0);
    assert_eq!(chunk.texts()[0], "café");
}

#[test]
fn seek_mid_emoji_snaps_to_line_start() {
    let dir = create_test_dir("mid_emoji");
    // "🦀go\n" = 7 bytes (🦀=4, g=1, o=1, \n=1). Byte 2 lands inside the emoji.
    let file = write_test_file(&dir, "test.txt", "🦀go\nnext\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(2), 2, &AtomicBool::new(false))
        .unwrap();

    assert_eq!(chunk.byte_offset, 0);
    assert_eq!(chunk.texts()[0], "🦀go");
}

#[test]
fn seek_mid_cjk_char_snaps_to_line_start() {
    let dir = create_test_dir("mid_cjk");
    // '漢' = 3 bytes in UTF-8. "漢字\n" = 7 bytes. Byte 1 lands mid-character.
    let file = write_test_file(&dir, "test.txt", "漢字\nnext\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(1), 2, &AtomicBool::new(false))
        .unwrap();

    assert_eq!(chunk.byte_offset, 0);
    assert_eq!(chunk.texts()[0], "漢字");
}

#[test]
fn read_lines_with_mixed_scripts() {
    let dir = create_test_dir("mixed_scripts");
    let content = "hello café\n漢字テスト\n🎉🦀🌍\nplain\n";
    let file = write_test_file(&dir, "test.txt", content);

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(0), 10, &AtomicBool::new(false))
        .unwrap();

    // Five, not four: the file ends in a newline, so it carries a final empty row. That
    // is the one answer all three backends now give, and what makes a whole-file copy
    // byte-identical to the file.
    assert_eq!(chunk.texts(), vec!["hello café", "漢字テスト", "🎉🦀🌍", "plain", ""]);
}

#[test]
fn search_emoji_utf16_column() {
    let dir = create_test_dir("search_emoji");
    // '🦀' = 4 bytes UTF-8, 2 UTF-16 code units
    let file = write_test_file(&dir, "test.txt", "🦀rust is great\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    backend
        .search(&literal_matcher("rust", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].column, 2); // 🦀 = 2 UTF-16 code units
    assert_eq!(matches[0].length, 4); // "rust" = 4 ASCII chars = 4 UTF-16 units
}

#[test]
fn search_cjk_utf16_column() {
    let dir = create_test_dir("search_cjk");
    // CJK chars are 3 bytes UTF-8 but 1 UTF-16 code unit each
    let file = write_test_file(&dir, "test.txt", "漢字test\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress: Mutex<u64> = Mutex::new(0);

    backend
        .search(&literal_matcher("test", false), &cancel, &results, &progress)
        .unwrap();
    let matches = results.lock().unwrap();

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].column, 2); // 2 CJK chars = 2 UTF-16 code units
    assert_eq!(matches[0].length, 4);
}

#[test]
fn read_emoji_only_lines() {
    let dir = create_test_dir("emoji_only");
    let file = write_test_file(&dir, "test.txt", "🎉🎊🎈\n🦀🦞🦐\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(0), 10, &AtomicBool::new(false))
        .unwrap();

    assert_eq!(chunk.texts()[0], "🎉🎊🎈");
    assert_eq!(chunk.texts()[1], "🦀🦞🦐");
}

#[test]
fn test_per_match_cancel_observes_within_100ms() {
    // ByteSeek mirror of the LineIndex test: the cancel flag has to reach `scan_line_with_matcher`,
    // which checks it once per match.
    let dir = create_test_dir("per_match_cancel_bs");
    let path = write_test_file(&dir, "many_matches.txt", &many_matches_corpus());

    let backend = ByteSeekBackend::open(&path).unwrap();
    assert_search_stops_on_per_match_cancel(&backend, &literal_matcher("a", true));
}

#[test]
fn read_file_starting_with_bom() {
    let dir = create_test_dir("bom");
    // UTF-8 BOM is EF BB BF (3 bytes), rendered as U+FEFF
    let mut content = vec![0xEF, 0xBB, 0xBF];
    content.extend_from_slice("hello\nworld\n".as_bytes());
    let file = dir.join("bom.txt");
    fs::write(&file, &content).unwrap();

    let backend = ByteSeekBackend::open(&file).unwrap();
    let chunk = backend
        .get_lines(&SeekTarget::ByteOffset(0), 10, &AtomicBool::new(false))
        .unwrap();

    // The BOM is not content: the first row starts past it, so the user never gets a
    // selectable zero-width `U+FEFF` and all three backends agree on row 0.
    assert_eq!(chunk.byte_offset, 3);
    assert_eq!(chunk.texts()[0], "hello");
    assert_eq!(chunk.texts()[1], "world");
}

/// The IPC deadline flips a fetch's flag; a fetch that sees it stops reading rows
/// rather than finishing for nobody.
#[test]
fn a_fetch_whose_flag_is_set_stops_with_cancelled() {
    let dir = create_test_dir("fetch_cancelled");
    let file = write_test_file(&dir, "test.txt", "line 1\nline 2\nline 3\n");

    let backend = ByteSeekBackend::open(&file).unwrap();
    let result = backend.get_lines(&SeekTarget::ByteOffset(0), 3, &AtomicBool::new(true));
    assert!(matches!(result, Err(ViewerError::Cancelled)), "got {result:?}");
}

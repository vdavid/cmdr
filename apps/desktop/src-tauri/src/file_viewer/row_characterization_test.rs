//! Characterization tests: what the viewer's three text backends do TODAY.
//!
//! The viewer is about to stop serving physical lines and start serving bounded rows
//! (`DETAILS.md` § "The row invariants"). Invariant I6 says every file whose lines are all
//! shorter than the segment size must behave byte-identically afterwards. These tests
//! are the register that claim gets checked against, so the rewrite is measured against
//! reality rather than against the plan's hopes.
//!
//! ❗ **They pin reality, including where reality is wrong.** Several behaviours below
//! are bugs; each says so in its name and its comment and then asserts the wrong answer
//! anyway. A rewrite that changes one has to change it deliberately, which is the point.
//! ❌ Never "fix" a test here by deleting it.
//!
//! Two tests are RED on purpose (`red_*`) and stay red until the rows land. They are the
//! spec's invariant I1 in test form.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use super::byte_seek::ByteSeekBackend;
use super::encoding::FileEncoding;
use super::full_load::FullLoadBackend;
use super::line_index::LineIndexBackend;
use super::range_read::{RangeEnd, read_range};
use super::search_matcher::{Matcher, SearchMode};
use super::session;
use super::{CHUNK_BUDGET_BYTES, ChunkEnd, FileViewerBackend, SearchMatch, SeekTarget, TotalRows};
use crate::pluralize::pluralize;
use crate::test_support::TestDir;

/// The row grid the rewrite introduces (`rows::SEGMENT_BYTES`).
/// Only the `red_*` tests use it; everything else predates rows.
const SEGMENT_BYTES: usize = 20_000;

/// A row's ceiling under the row rule, reached by a newline landing EXACTLY on a
/// multiple: the line then starts one byte past it, the next multiple's window still
/// holds that newline so clause 3 disqualifies it, and the row runs to the one after,
/// 39 999 bytes later.
///
/// ❗ Not "a newline just BEFORE a multiple" — that puts the line start ON the multiple,
/// which is already a boundary by clause 2, and the row after it is exactly one segment.
/// Both cases are pinned in `rows_test`
/// (`a_newline_exactly_on_a_multiple_makes_the_longest_possible_row` and
/// `a_newline_one_byte_before_a_multiple_keeps_the_multiple_as_a_row_start`).
const MAX_ROW_BYTES: usize = 2 * SEGMENT_BYTES;

/// Which backend a file gets depends only on its size in production (under 1 MB →
/// FullLoad, else ByteSeek plus a background LineIndex upgrade). These tests build all
/// three over the same small fixture instead, so the matrix runs in milliseconds and the
/// differences between backends sit side by side rather than behind a size threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    FullLoad,
    ByteSeek,
    LineIndex,
}

const ALL_BACKENDS: [Which; 3] = [Which::FullLoad, Which::ByteSeek, Which::LineIndex];

fn open_backend(which: Which, path: &Path) -> Box<dyn FileViewerBackend> {
    let cancel = AtomicBool::new(false);
    match which {
        Which::FullLoad => Box::new(FullLoadBackend::open(path).expect("FullLoad must open the fixture")),
        Which::ByteSeek => Box::new(ByteSeekBackend::open(path).expect("ByteSeek must open the fixture")),
        Which::LineIndex => Box::new(LineIndexBackend::open(path, &cancel).expect("LineIndex must open the fixture")),
    }
}

/// Open a backend with an explicit encoding, for a fixture whose bytes carry no BOM to
/// detect from.
fn open_backend_with_encoding(which: Which, path: &Path, encoding: FileEncoding) -> Box<dyn FileViewerBackend> {
    let cancel = AtomicBool::new(false);
    match which {
        Which::FullLoad => Box::new(FullLoadBackend::open_with_encoding(path, encoding).expect("FullLoad opens")),
        Which::ByteSeek => Box::new(ByteSeekBackend::open_with_encoding(path, encoding).expect("ByteSeek opens")),
        Which::LineIndex => {
            Box::new(LineIndexBackend::open_with_encoding(path, encoding, &cancel).expect("LineIndex opens"))
        }
    }
}

fn fixture(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let file = dir.join(name);
    fs::write(&file, bytes).expect("fixture write must succeed");
    file
}

/// Byte length of one `uniform_content` line, `\n` included.
const UNIFORM_LINE_BYTES: u64 = 20;
const UNIFORM_LINES: usize = 40;

/// 40 lines of exactly 20 bytes each, `line 0007 abcdefghi\n`.
///
/// Uniform on purpose: ByteSeek has no line index and derives its line numbers from an
/// average line length, so pinning it needs a fixture where that average has exactly one
/// right answer. The byte offset of line `n` is `n * 20`.
fn uniform_content() -> String {
    (0..UNIFORM_LINES).map(|i| format!("line {i:04} abcdefghi\n")).collect()
}

fn literal_matcher(query: &str) -> Matcher {
    Matcher::build(
        query,
        SearchMode {
            use_regex: false,
            case_sensitive: true,
        },
    )
    .expect("test query must build")
}

/// Runs a whole-file search and returns its matches as `(line, column, byte_offset)`.
fn search_hits(backend: &dyn FileViewerBackend, query: &str) -> Vec<(usize, usize, u64)> {
    let cancel = AtomicBool::new(false);
    let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
    let progress = Mutex::new(0u64);
    backend
        .search(&literal_matcher(query), &cancel, &results, &progress)
        .expect("search must succeed");
    let found = results.into_inner().expect("search results must not be poisoned");
    found.iter().map(|m| (m.row, m.column, m.byte_offset)).collect()
}

fn read(backend: &dyn FileViewerBackend, anchor: RangeEnd, focus: RangeEnd) -> String {
    let cancel = AtomicBool::new(false);
    read_range(backend, anchor, focus, &cancel).expect("range read must succeed")
}

fn at(row: u64, offset: u32) -> RangeEnd {
    RangeEnd::Row { row, offset }
}

// ---------------------------------------------------------------------------
// Opening and fetching: an ordinary UTF-8 file with LF line endings
// ---------------------------------------------------------------------------

#[test]
fn totals_agree_except_that_byte_seek_knows_no_line_count() {
    let dir = TestDir::new("viewer_char_totals");
    let file = fixture(&dir, "uniform.txt", uniform_content().as_bytes());

    for which in ALL_BACKENDS {
        let backend = open_backend(which, &file);
        assert_eq!(backend.total_bytes(), 800, "{which:?}");
        // 41, not 40: a file ending in `\n` gets a trailing empty line from every backend
        // that counts lines at all. Everything line-count-derived downstream (⌘A's last
        // line, the scroll scale) is built on that.
        let expected_lines = if which == Which::ByteSeek { None } else { Some(41) };
        assert_eq!(backend.total_lines(), expected_lines, "{which:?}");
    }
}

#[test]
fn fetching_by_line_number_works_on_the_two_backends_that_claim_to_support_it() {
    let dir = TestDir::new("viewer_char_line_target");
    let file = fixture(&dir, "uniform.txt", uniform_content().as_bytes());

    for which in [Which::FullLoad, Which::LineIndex] {
        let backend = open_backend(which, &file);
        assert!(backend.capabilities().supports_line_seek, "{which:?}");
        let chunk = backend
            .get_lines(&SeekTarget::Row(10), 3, &AtomicBool::new(false))
            .expect("line fetch");
        assert_eq!(chunk.first_row_number, 10, "{which:?}");
        assert_eq!(
            chunk.texts(),
            vec![
                "line 0010 abcdefghi".to_string(),
                "line 0011 abcdefghi".to_string(),
                "line 0012 abcdefghi".to_string(),
            ],
            "{which:?}"
        );
        assert_eq!(chunk.total_rows, TotalRows::Exact(41), "{which:?}");
        assert_eq!(chunk.total_bytes, 800, "{which:?}");
    }
}

#[test]
fn line_index_reports_the_target_rows_byte_offset_not_the_checkpoints() {
    // FIXED in milestone 3 (was `bug_pinned_line_index_reports_the_checkpoints_byte_offset_not_the_lines`).
    // `line_index.rs` used to return the CHECKPOINT's offset as `LineChunk.byte_offset`
    // while `first_row_number` was the target, and its own comment called it
    // "approximate". A 40-row file has exactly one checkpoint, so every fetch reported
    // offset 0 however far into the file it started, and anything seeking onward from it
    // landed short. It now walks from the checkpoint to the target and reports the
    // target's own offset. Spec landmine 9.
    let dir = TestDir::new("viewer_char_lidx_offset");
    let file = fixture(&dir, "uniform.txt", uniform_content().as_bytes());

    for which in ALL_BACKENDS {
        let chunk = open_backend(which, &file)
            .get_lines(&SeekTarget::Row(10), 3, &AtomicBool::new(false))
            .expect("row fetch");
        assert_eq!(chunk.first_row_number, 10, "{which:?}");
        assert_eq!(chunk.byte_offset, 10 * UNIFORM_LINE_BYTES, "{which:?}");
        // And the chunk's true source end, which is what the next fetch steers by.
        assert_eq!(chunk.end_byte_offset, 13 * UNIFORM_LINE_BYTES, "{which:?}");
    }
}

#[test]
fn byte_seek_maps_a_row_target_through_the_bytes_per_row_it_sampled() {
    // FIXED in milestone 3 (was `bug_pinned_byte_seek_estimates_a_line_target_at_80_bytes_a_line`).
    // `SeekTarget::Row(n)` used to become `n * 80` bytes, which on a 20-byte-row file
    // overshoots fourfold: row 10 landed at byte 800, which is EOF, so the fetch came
    // back EMPTY carrying a row number the estimate had invented. It now divides by the
    // bytes-per-row it sampled at open, and that sample is the SAME map its byte-to-row
    // answers ride on, so the two agree. On this uniform file it is exactly right.
    let dir = TestDir::new("viewer_char_bs_line");
    let file = fixture(&dir, "uniform.txt", uniform_content().as_bytes());

    let backend = open_backend(Which::ByteSeek, &file);
    assert!(!backend.capabilities().supports_line_seek);
    let chunk = backend
        .get_lines(&SeekTarget::Row(10), 3, &AtomicBool::new(false))
        .expect("row fetch");
    assert_eq!(chunk.byte_offset, 10 * UNIFORM_LINE_BYTES);
    assert_eq!(chunk.first_row_number, 10);
    assert_eq!(
        chunk.texts(),
        vec![
            "line 0010 abcdefghi".to_string(),
            "line 0011 abcdefghi".to_string(),
            "line 0012 abcdefghi".to_string(),
        ]
    );
}

#[test]
fn fetching_by_byte_offset_lands_on_the_containing_row() {
    // Byte 205 is mid-row-10 (every row is 20 bytes). All three resolve it to the row
    // that contains it.
    //
    // LineIndex is the FIX here (was `bug_pinned_line_index_rounds_a_byte_offset_down_to_its_checkpoint`):
    // its byte-offset seek used to binary-search the CHECKPOINT array and stop there, up
    // to 255 rows before the byte asked for, so byte 205 came back as row 0. It now
    // walks from the checkpoint to the row holding the byte. `read_range` steers between
    // chunks by byte offset, which is what made the rounding cost real bytes.
    let dir = TestDir::new("viewer_char_byte_target");
    let file = fixture(&dir, "uniform.txt", uniform_content().as_bytes());

    for which in ALL_BACKENDS {
        let chunk = open_backend(which, &file)
            .get_lines(&SeekTarget::ByteOffset(205), 3, &AtomicBool::new(false))
            .expect("byte fetch");
        assert_eq!(chunk.first_row_number, 10, "{which:?}");
        assert_eq!(chunk.byte_offset, 200, "{which:?}");
        assert_eq!(chunk.texts()[0], "line 0010 abcdefghi", "{which:?}");
    }
}

#[test]
fn fetching_by_fraction_lands_on_the_same_line_in_all_three_backends() {
    // The two line-counting backends multiply the fraction by the line count; ByteSeek
    // multiplies it by the byte count and back-scans. On a uniform file they agree.
    let dir = TestDir::new("viewer_char_fraction");
    let file = fixture(&dir, "uniform.txt", uniform_content().as_bytes());

    for which in ALL_BACKENDS {
        let chunk = open_backend(which, &file)
            .get_lines(&SeekTarget::Fraction(0.5), 2, &AtomicBool::new(false))
            .expect("fraction fetch");
        assert_eq!(chunk.first_row_number, 20, "{which:?}");
        assert_eq!(chunk.texts()[0], "line 0020 abcdefghi", "{which:?}");
    }
}

// ---------------------------------------------------------------------------
// Reading a range
// ---------------------------------------------------------------------------

#[test]
fn range_inside_one_line_is_byte_exact() {
    let dir = TestDir::new("viewer_char_range_one");
    let file = fixture(&dir, "uniform.txt", uniform_content().as_bytes());

    for which in [Which::FullLoad, Which::LineIndex] {
        let got = read(open_backend(which, &file).as_ref(), at(2, 5), at(2, 9));
        assert_eq!(got, "0002", "{which:?}");
    }
}

#[test]
fn range_across_lines_is_byte_exact_and_keeps_the_newlines() {
    let dir = TestDir::new("viewer_char_range_multi");
    let content = uniform_content();
    let file = fixture(&dir, "uniform.txt", content.as_bytes());

    for which in [Which::FullLoad, Which::LineIndex] {
        let got = read(open_backend(which, &file).as_ref(), at(2, 5), at(4, 4));
        assert_eq!(got, &content[45..84], "{which:?}");
        assert_eq!(got, "0002 abcdefghi\nline 0003 abcdefghi\nline", "{which:?}");
    }
}

#[test]
fn range_from_mid_row_to_eof_reaches_the_last_row_in_every_backend() {
    let dir = TestDir::new("viewer_char_range_eof");
    let content = uniform_content();
    let file = fixture(&dir, "uniform.txt", content.as_bytes());

    // ❗ THE TRAILING-NEWLINE ANSWER, picked in milestone 3 and pinned for all three
    // backends: a file ending in a newline has a final EMPTY row, so a read to `Eof`
    // carries that final newline. FullLoad already did this; the other two stopped one
    // byte earlier, which made the same gesture on the same file give two answers
    // depending only on the file's size. FullLoad's answer wins because it is the one a
    // whole-file copy can be byte-identical to the file under.
    for which in ALL_BACKENDS {
        let got = read(open_backend(which, &file).as_ref(), at(37, 5), RangeEnd::Eof);
        assert_eq!(got, &content[745..800], "{which:?}");
    }
}

#[test]
fn selecting_the_whole_file_yields_every_byte_in_every_backend() {
    let dir = TestDir::new("viewer_char_range_all");
    let content = uniform_content();
    let file = fixture(&dir, "uniform.txt", content.as_bytes());

    // ⌘A then ⌘C. All three hand over all 800 bytes, final newline included. Before
    // rows, ByteSeek and LineIndex handed over 799.
    for which in ALL_BACKENDS {
        let got = read(open_backend(which, &file).as_ref(), at(0, 0), RangeEnd::Eof);
        assert_eq!(got, content, "{which:?}");
    }
}

#[test]
fn a_multi_row_range_on_byte_seek_returns_the_rows_that_were_asked_for() {
    // FIXED in milestone 3 (was `bug_pinned_multi_line_range_on_byte_seek_returns_nothing`).
    // `range_read` seeks its FIRST chunk by `SeekTarget::Row(start)`, which ByteSeek
    // used to answer with the 80-bytes-a-line estimate. On a 20-byte-row file that
    // landed four times too far in, so `first_row_number` came back past the range's end
    // row and the loop returned before emitting anything: a partial copy in ByteSeek
    // mode (any file over 1 MB, before the LineIndex upgrade lands) produced an EMPTY
    // clipboard, silently, and a single-row copy produced the WRONG row. Both directions
    // of the map now go through the sampled bytes-per-row.
    let dir = TestDir::new("viewer_char_bs_range");
    let content = uniform_content();
    let file = fixture(&dir, "uniform.txt", content.as_bytes());
    let backend = open_backend(Which::ByteSeek, &file);

    assert_eq!(read(backend.as_ref(), at(2, 5), at(4, 4)), &content[45..84]);
    assert_eq!(read(backend.as_ref(), at(37, 5), RangeEnd::Eof), &content[745..800]);
    assert_eq!(read(backend.as_ref(), at(2, 5), at(2, 9)), "0002");
}

#[test]
fn a_long_range_on_line_index_is_byte_exact_across_every_chunk_seam() {
    // FIXED in milestone 3 (was `bug_pinned_a_long_range_on_line_index_duplicates_a_line_at_each_chunk_seam`).
    // `range_read` fetches 4 096 rows at a time and seeks the next chunk by the byte
    // offset just past the last one. LineIndex used to round that offset down to its
    // previous checkpoint (one every 256 rows), so a range not starting on a checkpoint
    // boundary re-served a row that had already gone out: row 4096 was emitted twice and
    // the copy came out 20 bytes longer than the source. It corrupted every copy or save
    // over 4 096 rows on any file big enough to carry an index. Spec landmine 9.
    let dir = TestDir::new("viewer_char_seam");
    let content: String = (0..5000).map(|i| format!("line {i:04} abcdefghi\n")).collect();
    let file = fixture(&dir, "big.txt", content.as_bytes());

    for which in [Which::LineIndex, Which::ByteSeek] {
        let got = read(open_backend(which, &file).as_ref(), at(1, 0), RangeEnd::Eof);
        assert_eq!(got, &content[20..], "{which:?}");
        assert!(!got.contains("line 4096 abcdefghi\nline 4096 abcdefghi\n"), "{which:?}");
    }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn search_reports_the_same_line_column_and_byte_offset_in_all_three_backends() {
    let dir = TestDir::new("viewer_char_search");
    let file = fixture(&dir, "uniform.txt", uniform_content().as_bytes());

    for which in ALL_BACKENDS {
        let backend = open_backend(which, &file);
        // The first line, a middle line, and the last line that holds text.
        assert_eq!(search_hits(backend.as_ref(), "line 0000"), vec![(0, 0, 0)], "{which:?}");
        assert_eq!(
            search_hits(backend.as_ref(), "line 0020"),
            vec![(20, 0, 400)],
            "{which:?}"
        );
        assert_eq!(
            search_hits(backend.as_ref(), "line 0039"),
            vec![(39, 0, 780)],
            "{which:?}"
        );
        // A match that starts mid-line reports its column within that line.
        let mid = search_hits(backend.as_ref(), "abcdefghi");
        assert_eq!(mid.len(), 40, "{which:?}");
        assert_eq!(mid[7], (7, 10, 140), "{which:?}");
    }
}

#[test]
fn search_counts_a_column_in_utf16_code_units_not_bytes() {
    // `héllo` is 6 UTF-8 bytes and 5 UTF-16 units, and the match after it sits at column
    // 6, which is what JS string indexing wants. `byte_offset` stays a true byte offset.
    let dir = TestDir::new("viewer_char_search_cols");
    let file = fixture(&dir, "wide.txt", "héllo NEEDLE\nplain NEEDLE\n".as_bytes());

    for which in ALL_BACKENDS {
        let hits = search_hits(open_backend(which, &file).as_ref(), "NEEDLE");
        assert_eq!(hits, vec![(0, 6, 0), (1, 6, 14)], "{which:?}");
    }
}

// ---------------------------------------------------------------------------
// A file with no trailing newline
// ---------------------------------------------------------------------------

#[test]
fn a_file_with_no_trailing_newline_has_no_trailing_empty_line() {
    let dir = TestDir::new("viewer_char_no_trailing");
    let content = "alpha\nbeta\ngamma";
    let file = fixture(&dir, "no_trailing.txt", content.as_bytes());

    for which in ALL_BACKENDS {
        let backend = open_backend(which, &file);
        let chunk = backend
            .get_lines(&SeekTarget::ByteOffset(0), 10, &AtomicBool::new(false))
            .expect("fetch");
        assert_eq!(chunk.texts(), vec!["alpha", "beta", "gamma"], "{which:?}");
        let expected_lines = if which == Which::ByteSeek { None } else { Some(3) };
        assert_eq!(backend.total_lines(), expected_lines, "{which:?}");
        // Here all three agree on ⌘A: the whole 16 bytes, with no newline invented.
        assert_eq!(read(backend.as_ref(), at(0, 0), RangeEnd::Eof), content, "{which:?}");
        assert_eq!(search_hits(backend.as_ref(), "gamma"), vec![(2, 0, 11)], "{which:?}");
    }
}

// ---------------------------------------------------------------------------
// CRLF
// ---------------------------------------------------------------------------

#[test]
fn crlf_lines_keep_their_carriage_return_in_the_line_string() {
    // `range_read`'s chunk arithmetic depends on this: the readers split on `\n` only, so
    // `line.len()` already counts the `\r` and the `+ 1` covers the single `\n`. The
    // reasoning in `range_read.rs`'s chunk-end comment is what this pins.
    let dir = TestDir::new("viewer_char_crlf");
    let content = "alpha\r\nbeta\r\ngamma\r\n";
    let file = fixture(&dir, "crlf.txt", content.as_bytes());

    for which in ALL_BACKENDS {
        let backend = open_backend(which, &file);
        let chunk = backend
            .get_lines(&SeekTarget::ByteOffset(0), 10, &AtomicBool::new(false))
            .expect("fetch");
        assert_eq!(&chunk.texts()[..3], ["alpha\r", "beta\r", "gamma\r"], "{which:?}");
    }

    // Copying the whole file round-trips the CRLFs byte for byte, in every backend.
    for which in ALL_BACKENDS {
        let got = read(open_backend(which, &file).as_ref(), at(0, 0), RangeEnd::Eof);
        assert_eq!(got, content, "{which:?}");
    }
}

// ---------------------------------------------------------------------------
// UTF-16
// ---------------------------------------------------------------------------

/// `text` as UTF-16 LE with a BOM, the shape the encoding detector recognises.
fn utf16_le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

/// `text` as UTF-16 with NO BOM, in either byte order.
///
/// ❗ Not a hypothetical shape: `encoding::detect_from_head` reaches UTF-16 without a
/// BOM on purpose, through its parity heuristic, and a manual encoding switch lands a
/// backend here too. Every other UTF-16 fixture in this file writes a BOM, which is
/// exactly how a backend assuming one went unnoticed.
fn utf16_no_bom(text: &str, le: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&if le { unit.to_le_bytes() } else { unit.to_be_bytes() });
    }
    bytes
}

/// ❗ A UTF-16 file with NO BOM must not lose its first character.
///
/// `LineIndexBackend` took `encoding.bom_bytes().len()` as its content start without
/// checking the file actually begins with one, so a BOM-less UTF-16 file lost its first
/// character everywhere the backend is used: viewport, copy, save-as, and `inspect_file`
/// through `headless.rs`. It also put LineIndex's row 0 two bytes off ByteSeek's, so the
/// background upgrade shifted content under a live row cache.
#[test]
fn a_utf16_file_without_a_bom_keeps_its_first_character_in_every_backend() {
    let dir = TestDir::new("viewer_char_utf16_no_bom");
    let text = "alpha\nbeta gamma\ndelta\n";

    for (order, le, encoding) in [
        ("le", true, FileEncoding::Utf16Le),
        ("be", false, FileEncoding::Utf16Be),
    ] {
        let file = fixture(&dir, &format!("no_bom_{order}.txt"), &utf16_no_bom(text, le));
        for which in ALL_BACKENDS {
            let backend = open_backend_with_encoding(which, &file, encoding);
            let chunk = backend
                .get_lines(&SeekTarget::ByteOffset(0), 5, &AtomicBool::new(false))
                .expect("fetch");
            assert_eq!(
                &chunk.texts()[..3],
                ["alpha", "beta gamma", "delta"],
                "{which:?} {order}"
            );
            // No BOM to skip, so row 0 starts at byte 0 in every backend. A backend that
            // skipped two bytes anyway would report 2 here and serve "lpha".
            assert_eq!(chunk.byte_offset, 0, "{which:?} {order}");
            assert_eq!(
                read(backend.as_ref(), at(0, 0), RangeEnd::Eof),
                text,
                "{which:?} {order}"
            );
        }
    }
}

#[test]
fn a_utf16_file_decodes_to_utf8_rows_past_its_bom_in_every_backend() {
    // The BOM half is the FIX from milestone 3 (was
    // `bug_pinned_byte_seek_shows_the_utf16_bom_as_a_character`): ByteSeek read from the
    // raw offset and `decode_line` uses `decode_without_bom_handling`, so the BOM
    // survived as a `U+FEFF` at the head of the first row, a zero-width character the
    // user could select and copy. Every backend's first row now starts past the BOM, so
    // all three agree on row 0 across a reload or a tail escalation.
    let dir = TestDir::new("viewer_char_utf16");
    let file = fixture(&dir, "utf16.txt", &utf16_le_with_bom("alpha\nbeta gamma\ndelta\n"));

    for which in ALL_BACKENDS {
        let backend = open_backend(which, &file);
        let chunk = backend
            .get_lines(&SeekTarget::ByteOffset(0), 5, &AtomicBool::new(false))
            .expect("fetch");
        assert_eq!(&chunk.texts()[..3], ["alpha", "beta gamma", "delta"], "{which:?}");
        assert_eq!(chunk.byte_offset, 2, "{which:?}");
        assert_eq!(backend.total_bytes(), 48, "{which:?}");
        assert_eq!(
            read(backend.as_ref(), at(0, 0), RangeEnd::Eof),
            "alpha\nbeta gamma\ndelta\n",
            "{which:?}"
        );
    }
}

#[test]
fn search_finds_a_needle_in_a_utf16_file_in_every_backend() {
    // FIXED in milestone 3 (was `bug_pinned_search_finds_nothing_in_a_utf16_file_unless_it_is_full_loaded`).
    // ByteSeek's and LineIndex's `search` each carried a copy of the same
    // `memchr(b'\n')` loop over RAW bytes, which is not how UTF-16 is framed: the spans
    // came out misaligned and the needle never matched, so ⌘F on a UTF-16 file over 1 MB
    // reported zero hits rather than saying anything. Both now walk rows through the
    // shared, encoding-aware `row_walk::search_rows`.
    let dir = TestDir::new("viewer_char_utf16_search");
    let file = fixture(&dir, "utf16.txt", &utf16_le_with_bom("alpha\nbeta gamma\ndelta\n"));

    for which in ALL_BACKENDS {
        assert_eq!(
            search_hits(open_backend(which, &file).as_ref(), "gamma"),
            vec![(1, 5, 14)],
            "{which:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Save as
// ---------------------------------------------------------------------------

#[test]
fn save_as_writes_exactly_the_range_that_was_asked_for() {
    let dir = TestDir::new("viewer_char_save_as");
    let content = uniform_content();
    let file = fixture(&dir, "uniform.txt", content.as_bytes());
    let sid = session::open_session(file.to_str().expect("fixture path is utf-8"), "root")
        .expect("session opens")
        .session_id;

    let dest = dir.join("saved.txt");
    session::write_range_to_file(&sid, 1, at(2, 5), at(4, 4), &dest, &session::SaveProgress::new())
        .expect("save succeeds");
    assert_eq!(fs::read_to_string(&dest).expect("saved file reads"), &content[45..84]);

    let whole = dir.join("whole.txt");
    session::write_range_to_file(&sid, 2, at(0, 0), RangeEnd::Eof, &whole, &session::SaveProgress::new())
        .expect("save succeeds");
    assert_eq!(fs::read_to_string(&whole).expect("saved file reads"), content);

    session::close_session(&sid).expect("session closes");
}

// ---------------------------------------------------------------------------
// RED until rows land (spec invariant I1: bounded work)
// ---------------------------------------------------------------------------

/// A file with no newline in it at all, the shape from `ERR-RQ8BY` (a minified JSON).
/// 3 MB rather than the reported 300 MB: the assertions below are structural, so the size
/// only has to make "the whole file" and "one screen of rows" unmistakably different
/// numbers. Keeping it small keeps the suite fast.
const NEWLINE_FREE_BYTES: usize = 3_000_000;

#[test]
fn red_first_fetch_of_a_newline_free_file_must_be_bounded() {
    // ❗ RED until milestone 3. Don't delete it, and don't relax the bound.
    //
    // Why the assertion is structural rather than timed: a wall-clock budget flakes on a
    // loaded CI runner and says nothing about WHY a read was slow. The bytes a backend
    // hands back are a hard floor on the bytes it touched (it cannot return text it never
    // read), so a bounded answer is a necessary condition for a bounded read, it is
    // deterministic, and it is the exact quantity invariant I1 is written about. It is
    // not a sufficient condition: an implementation could return bounded rows while still
    // scanning the file behind them. Catching THAT needs a seam this milestone doesn't
    // add (an injectable reader on the two backends, or a peak-allocation counter beside
    // `test_support::heap_bytes_held`, which measures RETAINED bytes and so nets a
    // transient whole-file buffer to zero).
    //
    // Today both backends return the entire 3 MB file as one "line", which is the 49 s in
    // the report.
    let dir = TestDir::new("viewer_char_red_fetch");
    let file = fixture(&dir, "minified.json", &vec![b'x'; NEWLINE_FREE_BYTES]);
    const ROWS_REQUESTED: usize = 3;

    for which in [Which::ByteSeek, Which::LineIndex] {
        let chunk = open_backend(which, &file)
            .get_lines(&SeekTarget::ByteOffset(0), ROWS_REQUESTED, &AtomicBool::new(false))
            .expect("first fetch");
        let served: usize = chunk.texts().iter().map(|l| l.len()).sum();
        assert!(
            served <= ROWS_REQUESTED * MAX_ROW_BYTES,
            "{which:?}: the first fetch of {} served {}, at most {} expected. A newline-free \
             file still costs the whole file per fetch.",
            pluralize(ROWS_REQUESTED as u64, "row"),
            pluralize(served as u64, "byte"),
            ROWS_REQUESTED * MAX_ROW_BYTES
        );
        assert!(
            chunk.texts().len() > 1,
            "{which:?}: a {NEWLINE_FREE_BYTES}-byte line must arrive as several rows, not one"
        );
    }
}

#[test]
fn red_search_in_a_newline_free_file_must_be_bounded() {
    // ❗ RED until milestone 3. Don't delete it, and don't relax the bound.
    //
    // The same structural argument as above, against the search path: `byte_seek.rs` and
    // `line_index.rs` both decode a newline-free file into ONE `String` and hand it to the
    // matcher, so the match comes back on line 0 at a column 2.5 million wide. A column
    // that fits inside a row is the observable proof the search worked on rows, and the
    // column is what the frontend actually consumes, so the assertion rides a shipped
    // quantity rather than an internal.
    let dir = TestDir::new("viewer_char_red_search");
    let mut content = vec![b'x'; 2_500_000];
    content.extend_from_slice(b"NEEDLE");
    content.extend_from_slice(&vec![b'x'; 500_000]);
    let file = fixture(&dir, "minified.json", &content);

    for which in [Which::ByteSeek, Which::LineIndex] {
        let hits = search_hits(open_backend(which, &file).as_ref(), "NEEDLE");
        assert_eq!(hits.len(), 1, "{which:?}");
        let (row, column, byte_offset) = hits[0];
        assert!(
            column <= MAX_ROW_BYTES,
            "{which:?}: the match came back at column {column} of row {row} (byte offset \
             {byte_offset}), so the search decoded {} of text as one line",
            pluralize(column as u64, "byte")
        );
    }
}

// ---------------------------------------------------------------------------
// What rows guarantee (milestone 3)
// ---------------------------------------------------------------------------

/// ❗ THE data-safety test for rows (spec landmine 1, invariant I3).
///
/// A break Cmdr made is not a newline. If any copy, save, or announcement path joins two
/// rows of the same physical line with a `\n`, the user's clipboard and their saved file
/// hold line breaks the source never had, once every 20 000 bytes, silently. A minified
/// bundle copied out of the viewer would come back mangled and nothing would say so.
#[test]
fn copying_across_a_segment_break_puts_no_newline_in_the_clipboard() {
    let dir = TestDir::new("viewer_rows_no_invented_newline");
    // One line of 90 000 bytes: five rows, four of them ended by Cmdr. Distinct bytes
    // per segment so a mis-stitch shows up as content, not just as a length.
    let long_line: String = (0..90_000).map(|i| char::from(b'a' + (i / 20_000) as u8)).collect();
    let content = format!("head\n{long_line}\ntail\n");
    let file = fixture(&dir, "minified.json", content.as_bytes());

    for which in ALL_BACKENDS {
        let backend = open_backend(which, &file);
        // The long line arrives as four rows, the first three marked. Four rather than
        // five because the `head\n` newline at byte 4 sits inside the segment below
        // 20 000 and disqualifies that multiple (clause 3), so the line's first row runs
        // on to 40 000. That is the rule working, not an off-by-one.
        let chunk = backend
            .get_lines(&SeekTarget::ByteOffset(5), 8, &AtomicBool::new(false))
            .expect("fetch");
        let marked = chunk.rows.iter().filter(|row| row.continues).count();
        assert_eq!(marked, 3, "{which:?}: a 90 000-byte line is four rows");
        assert_eq!(chunk.rows[0].byte_offset, 5, "{which:?}");
        assert_eq!(chunk.rows[1].byte_offset, 40_000, "{which:?}");
        // A continuation row prints NO line number, on every backend: the gutter stays
        // blank down the length of a long line, the usual editor convention.
        assert_eq!(chunk.rows[1].line_number, None, "{which:?}");
        assert_eq!(chunk.rows[2].line_number, None, "{which:?}");
        assert!(chunk.rows[0].line_number.is_some(), "{which:?}");
        if which != Which::ByteSeek {
            // Exact where an index exists. ByteSeek's is an estimate off its
            // bytes-per-row sample, as its line numbers always were.
            assert_eq!(chunk.rows[0].line_number, Some(1), "{which:?}");
        }

        let copied = read(backend.as_ref(), at(0, 0), RangeEnd::Eof);
        assert_eq!(copied, content, "{which:?}: a whole-file copy must be the file");
        assert_eq!(
            copied.matches('\n').count(),
            3,
            "{which:?}: the file has three newlines, and Cmdr's four breaks are not newlines"
        );
    }
}

/// A save of a newline-free file is part of milestone 3's DONE, not a follow-up: the
/// streaming sink appends a whole entry before testing its threshold, so it stayed
/// unbounded until its entries became rows.
#[test]
fn saving_a_newline_free_file_writes_it_back_byte_for_byte() {
    let dir = TestDir::new("viewer_rows_save_newline_free");
    let content: String = (0..120_000).map(|i| char::from(b'0' + (i % 10) as u8)).collect();
    let file = fixture(&dir, "minified.json", content.as_bytes());
    let sid = session::open_session(file.to_str().expect("fixture path is utf-8"), "root")
        .expect("session opens")
        .session_id;

    let dest = dir.join("saved.json");
    session::write_range_to_file(&sid, 1, at(0, 0), RangeEnd::Eof, &dest, &session::SaveProgress::new())
        .expect("save succeeds");
    assert_eq!(fs::read_to_string(&dest).expect("saved file reads"), content);

    session::close_session(&sid).expect("session closes");
}

/// `CHUNK_BUDGET_BYTES` bounds one answer, and the chunk SAYS it stopped early.
///
/// ❗ The saying is the load-bearing half: once a budget exists, "fewer rows than I asked
/// for" no longer means EOF, and a caller reading it that way truncates a copy in
/// silence. `range_read` steers by `ChunkEnd`, which the whole-file copy below proves.
#[test]
fn a_fetch_stops_at_the_chunk_budget_and_says_so() {
    let dir = TestDir::new("viewer_rows_budget");
    // Six MB with no newline: 300 rows of 20 000 bytes, well past the 2 MiB budget.
    let content = vec![b'x'; 6_000_000];
    let file = fixture(&dir, "minified.json", &content);

    for which in ALL_BACKENDS {
        let backend = open_backend(which, &file);
        let chunk = backend
            .get_lines(&SeekTarget::ByteOffset(0), 300, &AtomicBool::new(false))
            .expect("fetch");
        assert_eq!(chunk.end, ChunkEnd::BudgetReached, "{which:?}");
        assert!(
            chunk.rows.len() < 300,
            "{which:?}: the budget must cut the answer short"
        );
        let served: usize = chunk.rows.iter().map(|row| row.text.len()).sum();
        assert!(
            served <= CHUNK_BUDGET_BYTES as usize + MAX_ROW_BYTES,
            "{which:?}: served {} against a {CHUNK_BUDGET_BYTES}-byte budget",
            pluralize(served as u64, "byte")
        );
        // The next fetch starts exactly where this one stopped, from the chunk's own
        // source offset rather than from decoded string lengths.
        assert_eq!(
            chunk.end_byte_offset,
            chunk.byte_offset + served as u64,
            "{which:?}: on this file every row is whole segments, so the two agree"
        );

        // And a copy of the whole file still delivers every byte, over many chunks.
        let copied = read(backend.as_ref(), at(0, 0), RangeEnd::Eof);
        assert_eq!(copied.len(), content.len(), "{which:?}: I3, no silent truncation");
    }
}

/// ❗ A chunk that ENDS on the file's last row must say `EndOfFile`, or the walk asks
/// again from `end_byte_offset`, every streaming seek clamps that back onto the last
/// row, and the copy carries it twice (invariant I3).
///
/// The sizes here are chosen to land a chunk boundary exactly on the last row:
/// `range_read` fetches 4 096 rows at a time, so a file of exactly 4 096 rows fills one
/// chunk and ends on the same row. `FullLoadBackend` never had the bug, because it
/// reports `EndOfFile` whenever it serves its last row; the streaming pair reported
/// `CountReached` and let the walk come round again.
#[test]
fn a_chunk_ending_on_the_last_row_says_so_instead_of_serving_it_twice() {
    let dir = TestDir::new("viewer_rows_chunk_seam_eof");
    // Exactly 4 096 rows, the fetch size `range_read` asks for.
    let with_newline: String = (0..4096).map(|i| format!("line {i:04} abcdefghi\n")).collect();
    // And the same without a trailing newline, where the last row ends at EOF rather
    // than at a boundary, so a re-seek re-serves the whole row instead of an empty one.
    let without_newline = with_newline.trim_end_matches('\n').to_string();

    // Rows in each fixture: the newline-terminated one carries the final empty row, the
    // other doesn't. ❗ `end_byte_offset == total_bytes` is NOT the tell for either: on
    // the first file it is true one row BEFORE the end, because that empty row still
    // follows at the same offset. Only "the walk has no row left" is exact.
    for (name, content, rows) in [("trailing", &with_newline, 4097), ("none", &without_newline, 4096)] {
        let file = fixture(&dir, &format!("seam_{name}.txt"), content.as_bytes());
        for which in ALL_BACKENDS {
            let backend = open_backend(which, &file);
            let got = read(backend.as_ref(), at(0, 0), RangeEnd::Eof);
            assert_eq!(got, *content, "{which:?} {name}: a whole-file copy must be the file");

            // Asking for exactly the rows the file has must come back saying so, rather
            // than leaving the caller to ask once more and be handed the last row again.
            let chunk = backend
                .get_lines(&SeekTarget::Row(0), rows, &AtomicBool::new(false))
                .expect("fetch");
            assert_eq!(chunk.rows.len(), rows, "{which:?} {name}");
            assert_eq!(chunk.end, ChunkEnd::EndOfFile, "{which:?} {name}");
            assert_eq!(chunk.end_byte_offset, backend.total_bytes(), "{which:?} {name}");
        }
    }
}

/// ❗ A range that spans several fetches must end where it was told to, on every
/// backend.
///
/// `range_read` decides where an explicit-end range stops by comparing a row's index
/// against `end_line`, and it took those indices from each chunk. ByteSeek reports the
/// row it was ASKED for on a `Line` target but re-derives the number from
/// `bytes_per_row` on the byte-offset continuation chunks, so on a file whose rows
/// aren't uniform the numbering jumps at a chunk seam and the copy stops at the wrong
/// row. The fixture below is deliberately non-uniform, which is what makes the estimate
/// drift; with uniform rows the bug hides.
#[test]
fn a_multi_chunk_range_stops_where_it_was_told_on_every_backend() {
    let dir = TestDir::new("viewer_rows_multi_chunk_end");
    // Rows of varying length, so `bytes_per_row` is a genuine estimate rather than
    // exact, and more than one `FETCH_CHUNK` of them.
    let content: String = (0..9000)
        .map(|i| format!("line {i:05} {}\n", "z".repeat(i % 37)))
        .collect();
    let file = fixture(&dir, "varied.txt", content.as_bytes());

    // The byte range rows 100..5000 cover, computed from the fixture rather than from
    // any backend's opinion of where a row starts.
    let offsets: Vec<usize> = content
        .match_indices('\n')
        .map(|(i, _)| i + 1)
        .fold(vec![0usize], |mut acc, o| {
            acc.push(o);
            acc
        });
    let expected = &content[offsets[100]..offsets[5000]];

    // The two indexed backends resolve row 100 exactly, so they must be byte-exact.
    for which in [Which::FullLoad, Which::LineIndex] {
        let backend = open_backend(which, &file);
        let got = read(backend.as_ref(), at(100, 0), at(5000, 0));
        assert_eq!(got, expected, "{which:?}");
    }

    // ByteSeek has no index, so WHERE row 100 is remains an estimate; that is its
    // documented nature and not what this pins. What it owes is a well-formed range:
    // exactly the 4 900 rows asked for, contiguous, starting on a row boundary. Before
    // the fix it re-derived the row number from `bytes_per_row` on every continuation
    // chunk, so the count jumped at each seam and the copy ran to the wrong row.
    let got = read(open_backend(Which::ByteSeek, &file).as_ref(), at(100, 0), at(5000, 0));
    assert_eq!(
        got.matches('\n').count(),
        4900,
        "ByteSeek: a 4 900-row range must emit 4 900 rows"
    );
    assert!(content.contains(&got), "ByteSeek: the range must be a contiguous slice");
    let at_row_start = got.starts_with("line ");
    assert!(at_row_start, "ByteSeek: the range must start on a row boundary");
}

/// Search progress counts SOURCE bytes, the same units `total_bytes` is in, on every
/// backend.
///
/// The frontend shows progress as a fraction of the file. `FullLoadBackend` summed
/// decoded UTF-8 lengths, which on a UTF-16 file are about half the source, so the bar
/// stalled near 50%; and it added a byte for the final empty row's delimiter, pushing
/// the total PAST the file size on a newline-terminated one.
#[test]
fn search_progress_is_measured_in_source_bytes_in_every_backend() {
    let dir = TestDir::new("viewer_rows_search_progress");
    let text = "alpha\nbeta gamma\ndelta\n";

    for (name, bytes) in [("utf8", text.as_bytes().to_vec()), ("utf16", utf16_le_with_bom(text))] {
        let file = fixture(&dir, &format!("progress_{name}.txt"), &bytes);
        for which in ALL_BACKENDS {
            let backend = open_backend(which, &file);
            let cancel = AtomicBool::new(false);
            let results: Mutex<Vec<SearchMatch>> = Mutex::new(Vec::new());
            let progress = Mutex::new(0u64);
            let scanned = backend
                .search(&literal_matcher("gamma"), &cancel, &results, &progress)
                .expect("search must succeed");
            let reported = *progress.lock().expect("progress must not be poisoned");
            assert_eq!(scanned, reported, "{which:?} {name}: return value and progress agree");
            assert_eq!(
                scanned,
                backend.total_bytes(),
                "{which:?} {name}: a whole-file search scans the whole file"
            );
        }
    }
}

/// Invariant I1 on the LineIndex backend, deep into a newline-free file.
///
/// Its checkpoints used to count LINES, so a file with one line got exactly one
/// checkpoint and every fetch walked from byte 0 however far in the user had scrolled.
/// Counting rows keeps the walk to one checkpoint interval.
#[test]
fn a_fetch_deep_into_a_newline_free_file_is_bounded_on_line_index() {
    let dir = TestDir::new("viewer_rows_deep_fetch");
    let file = fixture(&dir, "minified.json", &vec![b'x'; NEWLINE_FREE_BYTES]);

    for which in [Which::ByteSeek, Which::LineIndex] {
        let backend = open_backend(which, &file);
        let chunk = backend
            .get_lines(&SeekTarget::ByteOffset(2_800_000), 3, &AtomicBool::new(false))
            .expect("deep fetch");
        assert_eq!(chunk.byte_offset, 2_800_000, "{which:?}: on the segment grid");
        let served: usize = chunk.rows.iter().map(|row| row.text.len()).sum();
        assert!(
            served <= 3 * MAX_ROW_BYTES,
            "{which:?}: served {}",
            pluralize(served as u64, "byte")
        );
        // Row numbers are exact here, even on the backend that has no index: with no
        // newline anywhere, the sampled bytes-per-row IS the segment size.
        assert_eq!(chunk.first_row_number, 140, "{which:?}");
    }
}

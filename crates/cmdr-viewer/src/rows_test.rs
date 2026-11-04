//! Tests for the row boundary rule.
//!
//! Two layers:
//!
//! 1. **Property tests on a tiny grid.** `RowRuler::with_segment` runs the same
//!    rule with a 16-byte segment, so every property can be probed at EVERY offset
//!    of a few dozen crafted files instead of sampled on a handful of 100 KB ones.
//!    A reference boundary set, computed here by whole-file brute force straight
//!    from the definition, is what the windowed implementation is checked against.
//! 2. **Real-grid tests** at the shipping `SEGMENT_BYTES`, for the edge cases whose
//!    arithmetic only exists at that size, and for the bounded-read guarantee.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ops::Bound;
use std::rc::Rc;

use super::ViewerError;
use super::encoding::{FileEncoding, decode_line};
use super::row_walk::{RowReader, RowSpan};
use super::rows::{FileSource, MAX_WINDOW_BYTES, RowRuler, RowSource, SEGMENT_BYTES, SliceSource};
use cmdr_fs::pluralize::pluralize;
use cmdr_fs::testing::TestDir;

/// The tiny grid the property tests run on. Even (UTF-16 code units are 2 bytes)
/// and comfortably longer than the longest character (4 bytes).
const TEST_SEGMENT: u64 = 16;

/// One `RowReader` refill, the granularity its reads round up to. Mirrors
/// `row_walk::READ_CHUNK_BYTES`, which is private; a read bound stated in terms of the
/// answer has to allow for the last partial chunk.
const READ_CHUNK_SLACK: u64 = 64 * 1024;

// ---------------------------------------------------------------------------
// Reference implementation: the rule, brute-forced over the whole file.
// ---------------------------------------------------------------------------

/// Byte offsets of the code unit holding `U+000A`, found without `NewlineScanner`
/// so the implementation isn't checked against itself.
fn reference_newline_units(bytes: &[u8], encoding: FileEncoding) -> Vec<u64> {
    let mut out = Vec::new();
    match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            let le = matches!(encoding, FileEncoding::Utf16Le);
            let mut i = 0usize;
            while i + 1 < bytes.len() {
                let unit = if le {
                    u16::from_le_bytes([bytes[i], bytes[i + 1]])
                } else {
                    u16::from_be_bytes([bytes[i], bytes[i + 1]])
                };
                if unit == 0x000A {
                    out.push(i as u64);
                }
                i += 2;
            }
        }
        _ => {
            for (i, b) in bytes.iter().enumerate() {
                if *b == b'\n' {
                    out.push(i as u64);
                }
            }
        }
    }
    out
}

fn reference_newline_len(encoding: FileEncoding) -> u64 {
    match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => 2,
        _ => 1,
    }
}

/// Where a boundary is allowed to land: never inside a character.
///
/// UTF-16 caps the snap at one code unit, matching the implementation: a run of
/// lone high surrogates is malformed input, and an unbounded snap would break the
/// row length bound.
fn reference_is_char_start(bytes: &[u8], encoding: FileEncoding, pos: u64) -> bool {
    let p = pos as usize;
    match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            if !p.is_multiple_of(2) {
                return false;
            }
            if p < 2 || p > bytes.len() {
                return true;
            }
            let le = matches!(encoding, FileEncoding::Utf16Le);
            let unit = if le {
                u16::from_le_bytes([bytes[p - 2], bytes[p - 1]])
            } else {
                u16::from_be_bytes([bytes[p - 2], bytes[p - 1]])
            };
            !(0xD800..=0xDBFF).contains(&unit)
        }
        FileEncoding::Utf8 | FileEncoding::Utf8WithBom | FileEncoding::UsAscii => {
            p == 0 || p >= bytes.len() || (bytes[p] & 0xC0) != 0x80
        }
        // Every byte is a character in the single-byte Western encodings.
        _ => true,
    }
}

fn reference_snap_back(bytes: &[u8], encoding: FileEncoding, pos: u64) -> u64 {
    // Three steps covers a 4-byte UTF-8 character; UTF-16 needs one code unit.
    for back in 0..=3u64 {
        if back > pos {
            break;
        }
        if reference_is_char_start(bytes, encoding, pos - back) {
            return pos - back;
        }
    }
    pos
}

/// The boundary set, straight from the three clauses, with no windowing at all.
fn reference_boundaries(bytes: &[u8], encoding: FileEncoding, segment: u64) -> BTreeSet<u64> {
    let total = bytes.len() as u64;
    let newlines = reference_newline_units(bytes, encoding);
    let nl_len = reference_newline_len(encoding);

    let mut set = BTreeSet::new();
    set.insert(0u64);
    // `line_start < total`: a file's final newline ends its last row and starts none, so
    // no empty row follows it (the way `less` and editors count lines).
    for p in &newlines {
        let line_start = p + nl_len;
        if line_start < total {
            set.insert(line_start);
        }
    }
    // ❗ `m < total`, not `m <= total`. A multiple sitting exactly ON EOF cannot start a
    // row: there is nothing after it. Admitting it does no harm until the file also ends
    // mid-character, and then the character snap drags it BACK below EOF and invents a
    // boundary, splitting a two-byte row of truncated bytes off the end. `row_start`
    // would then put the file's last byte in that invented row while the forward walk
    // put it in the row a segment earlier: the same byte in two rows, which is I4.
    let mut m = segment;
    while m < total {
        let window_has_newline = newlines.iter().any(|p| *p >= m - segment && *p < m);
        if !window_has_newline {
            set.insert(reference_snap_back(bytes, encoding, m));
        }
        m += segment;
    }
    set
}

fn greatest_boundary_at_or_below(set: &BTreeSet<u64>, offset: u64) -> u64 {
    *set.range(..=offset).next_back().expect("0 is always a boundary")
}

fn least_boundary_above(set: &BTreeSet<u64>, offset: u64, total: u64) -> u64 {
    set.range((Bound::Excluded(offset), Bound::Unbounded))
        .next()
        .copied()
        .unwrap_or(total)
        .min(total)
}

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

struct Case {
    name: String,
    bytes: Vec<u8>,
    encoding: FileEncoding,
}

fn ascii_case(name: &str, bytes: Vec<u8>) -> Case {
    Case {
        name: name.to_string(),
        bytes,
        encoding: FileEncoding::Utf8,
    }
}

fn utf16_bytes(s: &str, le: bool, bom: bool) -> Vec<u8> {
    let mut out = Vec::new();
    if bom {
        out.extend_from_slice(if le { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
    }
    for unit in s.encode_utf16() {
        out.extend_from_slice(&if le { unit.to_le_bytes() } else { unit.to_be_bytes() });
    }
    out
}

/// Files whose every property is checked at every offset, on the tiny grid.
fn tiny_grid_corpus() -> Vec<Case> {
    let mut cases = vec![
        ascii_case("empty", Vec::new()),
        ascii_case("one-byte", b"a".to_vec()),
        ascii_case("one-newline", b"\n".to_vec()),
        ascii_case("newline-at-offset-zero", b"\nabcdef".to_vec()),
        ascii_case("no-newline-short", vec![b'x'; 10]),
        ascii_case("no-newline-long", vec![b'x'; 100]),
        ascii_case("crlf", b"ab\r\ncd\r\nef\r\n".to_vec()),
        ascii_case("crlf-long", b"abcdefghijklmno\r\npqrstuvwxyz012345\r\n".to_vec()),
        ascii_case("trailing-newline", b"abc\ndef\n".to_vec()),
        ascii_case("only-newlines", vec![b'\n'; 40]),
    ];

    // A newline at every position around each of the first few multiples: this is
    // where clause 3's window test either holds or spuriously fires.
    for m in [TEST_SEGMENT, 2 * TEST_SEGMENT, 3 * TEST_SEGMENT] {
        for delta in -2i64..=2 {
            let pos = (m as i64 + delta) as usize;
            let mut bytes = vec![b'x'; 5 * TEST_SEGMENT as usize];
            bytes[pos] = b'\n';
            cases.push(ascii_case(&format!("single-newline-at-{pos}"), bytes));
        }
    }

    // ❗ Files whose SIZE is exactly a multiple and whose last character is truncated,
    // so the character snap has something to grab at EOF. Without these the corpus
    // never exercised a multiple landing on EOF, and the ruler and the forward walk
    // disagreed there unnoticed: the ruler admitted the multiple and snapped it back
    // below EOF, inventing a row of truncated bytes, while the walk rejected it.
    for trailing in 1..=3usize {
        // A 3-byte character cut short `trailing` bytes from its end, placed so the
        // file ends exactly on a multiple with no newline in the last segment.
        let mut bytes = vec![b'x'; 2 * TEST_SEGMENT as usize];
        let cut = bytes.len() - trailing;
        bytes[cut] = 0xE6; // lead byte of a 3-byte sequence
        for b in bytes.iter_mut().skip(cut + 1) {
            *b = 0x9C; // continuation bytes
        }
        cases.push(ascii_case(&format!("utf8-truncated-{trailing}-at-eof-multiple"), bytes));
    }
    // The UTF-16 shape of the same thing: a lone high surrogate closing the file on a
    // multiple, which the snap would pull back by one code unit.
    for (order, le) in [("le", true), ("be", false)] {
        let mut bytes = utf16_bytes(&"a".repeat(TEST_SEGMENT as usize - 1), le, /*bom=*/ false);
        bytes.truncate(2 * TEST_SEGMENT as usize - 2);
        bytes.extend_from_slice(&if le {
            0xD800u16.to_le_bytes()
        } else {
            0xD800u16.to_be_bytes()
        });
        cases.push(Case {
            name: format!("utf16-{order}-lone-high-surrogate-at-eof-multiple"),
            bytes,
            encoding: if le {
                FileEncoding::Utf16Le
            } else {
                FileEncoding::Utf16Be
            },
        });
    }

    // Two newlines straddling a multiple: disqualifies two multiples in a row.
    let mut pair = vec![b'x'; 6 * TEST_SEGMENT as usize];
    pair[TEST_SEGMENT as usize] = b'\n';
    pair[TEST_SEGMENT as usize + 1] = b'\n';
    cases.push(ascii_case("two-newlines-on-a-multiple", pair));

    // Fixed line lengths, every one of them, so line starts hit every residue.
    for len in 1..=(2 * TEST_SEGMENT as usize + 2) {
        let mut bytes = Vec::new();
        while bytes.len() < 6 * TEST_SEGMENT as usize {
            bytes.extend(std::iter::repeat_n(b'x', len));
            bytes.push(b'\n');
        }
        cases.push(ascii_case(&format!("lines-of-{len}"), bytes));
    }

    // Multi-byte UTF-8 straddling a multiple, at every alignment.
    for (label, ch) in [("2-byte", 'é'), ("3-byte", '€'), ("4-byte", '😀')] {
        for pad in 0..6usize {
            let mut text = String::new();
            text.push_str(&"a".repeat(TEST_SEGMENT as usize - 3 + pad));
            text.push(ch);
            text.push_str(&"b".repeat(40));
            cases.push(Case {
                name: format!("utf8-{label}-pad-{pad}"),
                bytes: text.into_bytes(),
                encoding: FileEncoding::Utf8,
            });
        }
    }

    // A UTF-8 BOM shifts every character start by three, so the grid and the
    // character boundaries stop agreeing. Boundaries stay ABSOLUTE regardless.
    let mut bom8 = vec![0xEF, 0xBB, 0xBF];
    bom8.extend_from_slice("aaaa€aaaaaaaa€aaaaaaaaaaaa\nzz€zz".as_bytes());
    cases.push(Case {
        name: "utf8-with-bom".to_string(),
        bytes: bom8,
        encoding: FileEncoding::Utf8WithBom,
    });

    // Western single-byte: no character snapping at all, high bytes everywhere.
    let mut latin = Vec::new();
    for i in 0..(5 * TEST_SEGMENT as usize) {
        latin.push(if i % 37 == 36 { b'\n' } else { 0x80 + (i % 0x40) as u8 });
    }
    cases.push(Case {
        name: "windows-1252".to_string(),
        bytes: latin.clone(),
        encoding: FileEncoding::Windows1252,
    });
    cases.push(Case {
        name: "iso-8859-1".to_string(),
        bytes: latin,
        encoding: FileEncoding::Iso8859_1,
    });

    for le in [true, false] {
        let order = if le { "le" } else { "be" };
        for bom in [false, true] {
            let tag = if bom { "bom" } else { "nobom" };
            cases.push(Case {
                name: format!("utf16-{order}-{tag}-plain"),
                bytes: utf16_bytes("hello world\nsecond line here\nthird\n", le, bom),
                encoding: if le {
                    FileEncoding::Utf16Le
                } else {
                    FileEncoding::Utf16Be
                },
            });
            cases.push(Case {
                name: format!("utf16-{order}-{tag}-no-newline"),
                bytes: utf16_bytes(&"x".repeat(80), le, bom),
                encoding: if le {
                    FileEncoding::Utf16Le
                } else {
                    FileEncoding::Utf16Be
                },
            });
            // A surrogate pair straddling each of the first multiples: the pair
            // occupies 4 bytes, so it straddles `m` when it starts at `m - 2`.
            for m in [TEST_SEGMENT, 2 * TEST_SEGMENT] {
                let bom_units = if bom { 1usize } else { 0 };
                let before = (m as usize / 2).saturating_sub(1).saturating_sub(bom_units);
                let text = format!("{}😀{}", "a".repeat(before), "b".repeat(30));
                cases.push(Case {
                    name: format!("utf16-{order}-{tag}-surrogate-across-{m}"),
                    bytes: utf16_bytes(&text, le, bom),
                    encoding: if le {
                        FileEncoding::Utf16Le
                    } else {
                        FileEncoding::Utf16Be
                    },
                });
            }
            cases.push(Case {
                name: format!("utf16-{order}-{tag}-newline-on-multiple"),
                bytes: utf16_bytes(&format!("{}\n{}", "a".repeat(8), "b".repeat(40)), le, bom),
                encoding: if le {
                    FileEncoding::Utf16Le
                } else {
                    FileEncoding::Utf16Be
                },
            });
        }
    }

    cases
}

/// Walks every row of `bytes` from 0, on the tiny grid.
fn tiny_rows(case: &Case) -> Vec<std::ops::Range<u64>> {
    let mut ruler = RowRuler::with_segment(SliceSource::new(&case.bytes), case.encoding, TEST_SEGMENT);
    let total = case.bytes.len() as u64;
    let mut rows = Vec::new();
    let mut start = 0u64;
    while start < total {
        let end = ruler.row_end(start).expect("slice reads cannot fail");
        assert!(
            end > start,
            "{}: row_end({start}) returned {end}, which would not terminate",
            case.name
        );
        rows.push(start..end);
        start = end;
    }
    rows
}

// ---------------------------------------------------------------------------
// The rule matches its definition, probed at every offset
// ---------------------------------------------------------------------------

#[test]
fn row_start_is_the_greatest_boundary_at_or_below_every_offset() {
    for case in tiny_grid_corpus() {
        let expected = reference_boundaries(&case.bytes, case.encoding, TEST_SEGMENT);
        let mut ruler = RowRuler::with_segment(SliceSource::new(&case.bytes), case.encoding, TEST_SEGMENT);
        for offset in 0..=case.bytes.len() as u64 {
            let got = ruler.row_start(offset).expect("slice reads cannot fail");
            assert_eq!(
                got,
                greatest_boundary_at_or_below(&expected, offset),
                "{}: row_start({offset}) disagrees with the boundary set {expected:?}",
                case.name
            );
        }
    }
}

#[test]
fn row_end_is_the_least_boundary_above_each_row_start() {
    for case in tiny_grid_corpus() {
        let total = case.bytes.len() as u64;
        let expected = reference_boundaries(&case.bytes, case.encoding, TEST_SEGMENT);
        let mut ruler = RowRuler::with_segment(SliceSource::new(&case.bytes), case.encoding, TEST_SEGMENT);
        for &boundary in &expected {
            if boundary >= total {
                continue;
            }
            let got = ruler.row_end(boundary).expect("slice reads cannot fail");
            assert_eq!(
                got,
                least_boundary_above(&expected, boundary, total),
                "{}: row_end({boundary}) disagrees with the boundary set {expected:?}",
                case.name
            );
        }
    }
}

#[test]
fn row_start_is_idempotent_from_every_offset_inside_a_row() {
    for case in tiny_grid_corpus() {
        let rows = tiny_rows(&case);
        let mut ruler = RowRuler::with_segment(SliceSource::new(&case.bytes), case.encoding, TEST_SEGMENT);
        for row in &rows {
            for offset in row.clone() {
                let got = ruler.row_start(offset).expect("slice reads cannot fail");
                assert_eq!(
                    got, row.start,
                    "{}: probing {offset} inside row {row:?} gave row_start {got}",
                    case.name
                );
            }
            let start_again = ruler.row_start(row.start).expect("slice reads cannot fail");
            assert_eq!(start_again, row.start, "{}: row_start is not idempotent", case.name);
        }
    }
}

#[test]
fn rows_partition_the_file_with_no_gap_and_no_overlap() {
    for case in tiny_grid_corpus() {
        let total = case.bytes.len() as u64;
        let rows = tiny_rows(&case);
        if total == 0 {
            assert!(rows.is_empty(), "{}: an empty file has no rows", case.name);
            continue;
        }
        assert_eq!(rows[0].start, 0, "{}: the first row starts at 0", case.name);
        assert_eq!(
            rows[rows.len() - 1].end,
            total,
            "{}: the last row ends at EOF",
            case.name
        );
        for pair in rows.windows(2) {
            assert_eq!(pair[0].end, pair[1].start, "{}: rows {pair:?} leave a seam", case.name);
        }
        let covered: u64 = rows.iter().map(|r| r.end - r.start).sum();
        assert_eq!(covered, total, "{}: rows do not cover every byte once", case.name);
    }
}

#[test]
fn rows_are_shorter_than_two_segments() {
    for case in tiny_grid_corpus() {
        for row in tiny_rows(&case) {
            assert!(
                row.end - row.start < 2 * TEST_SEGMENT,
                "{}: row {row:?} is {} bytes, the bound is {}",
                case.name,
                row.end - row.start,
                2 * TEST_SEGMENT
            );
        }
    }
}

#[test]
fn a_boundary_never_lands_inside_a_character() {
    for case in tiny_grid_corpus() {
        for row in tiny_rows(&case) {
            assert!(
                reference_is_char_start(&case.bytes, case.encoding, row.start),
                "{}: row {row:?} starts inside a character",
                case.name
            );
        }
    }
}

#[test]
fn no_row_decodes_to_a_replacement_character_our_own_boundary_created() {
    // Splitting a file into rows must not introduce a single `U+FFFD` that decoding it
    // whole wouldn't produce.
    //
    // ❗ Stated as a comparison against the file's OWN decode rather than "no row
    // contains `U+FFFD`". The corpus deliberately holds files that end mid-character, so
    // an absolute claim would either fail on them or force them out of the corpus, and
    // they are exactly the fixtures that catch a boundary landing on EOF. What we owe
    // the user is that OUR cuts add nothing, which is what this measures.
    for case in tiny_grid_corpus() {
        let whole = decode_line(&case.bytes, case.encoding);
        let whole_count = whole.matches('\u{FFFD}').count();
        let by_row: usize = tiny_rows(&case)
            .iter()
            .map(|row| {
                decode_line(&case.bytes[row.start as usize..row.end as usize], case.encoding)
                    .matches('\u{FFFD}')
                    .count()
            })
            .sum();
        assert_eq!(
            by_row, whole_count,
            "{}: splitting into rows changed the replacement-character count",
            case.name
        );
    }
}

// ---------------------------------------------------------------------------
// I6: files whose lines are all shorter than a segment are untouched
// ---------------------------------------------------------------------------

#[test]
fn short_line_files_have_rows_exactly_at_line_boundaries() {
    // Every line length below the segment, including the ones that put a line start
    // exactly on a multiple and either side of it.
    for len in 0..(TEST_SEGMENT as usize - 1) {
        for trailing_newline in [true, false] {
            let mut bytes = Vec::new();
            while bytes.len() < 8 * TEST_SEGMENT as usize {
                bytes.extend(std::iter::repeat_n(b'x', len));
                bytes.push(b'\n');
            }
            if !trailing_newline {
                bytes.pop();
                bytes.extend(std::iter::repeat_n(b'y', len.min(3)));
            }
            let case = ascii_case(&format!("short-lines-{len}-trailing-{trailing_newline}"), bytes);

            let mut line_starts = vec![0u64];
            for (i, b) in case.bytes.iter().enumerate() {
                if *b == b'\n' && i + 1 < case.bytes.len() {
                    line_starts.push(i as u64 + 1);
                }
            }
            let row_starts: Vec<u64> = tiny_rows(&case).iter().map(|r| r.start).collect();
            assert_eq!(
                row_starts, line_starts,
                "{}: rows and lines must be one-to-one below the segment size",
                case.name
            );
        }
    }
}

#[test]
fn real_grid_short_line_file_is_one_row_per_line() {
    // 19 999 content bytes plus a newline is exactly `SEGMENT_BYTES` per line, so
    // every line start is a multiple: the case most likely to trip clause 3.
    let mut bytes = Vec::new();
    for _ in 0..5 {
        bytes.extend(std::iter::repeat_n(b'x', SEGMENT_BYTES as usize - 1));
        bytes.push(b'\n');
    }
    let rows = real_rows(&bytes, FileEncoding::Utf8);
    let starts: Vec<u64> = rows.iter().map(|r| r.start).collect();
    assert_eq!(starts, vec![0, 20_000, 40_000, 60_000, 80_000]);
}

// ---------------------------------------------------------------------------
// Real-grid edge cases
// ---------------------------------------------------------------------------

fn real_rows(bytes: &[u8], encoding: FileEncoding) -> Vec<std::ops::Range<u64>> {
    let mut ruler = RowRuler::new(SliceSource::new(bytes), encoding);
    let total = bytes.len() as u64;
    let mut rows = Vec::new();
    let mut start = 0u64;
    while start < total {
        let end = ruler.row_end(start).expect("slice reads cannot fail");
        assert!(end > start, "row_end({start}) returned {end}");
        rows.push(start..end);
        start = end;
    }
    rows
}

#[test]
fn a_newline_free_file_has_rows_exactly_one_segment_apart() {
    let bytes = vec![b'x'; 5 * SEGMENT_BYTES as usize + 137];
    let rows = real_rows(&bytes, FileEncoding::Utf8);
    let starts: Vec<u64> = rows.iter().map(|r| r.start).collect();
    assert_eq!(starts, vec![0, 20_000, 40_000, 60_000, 80_000, 100_000]);
    assert_eq!(rows[rows.len() - 1].end, bytes.len() as u64);

    // Row numbers come out exact on a newline-free file: the arithmetic the
    // ByteSeek backend will lean on.
    let mut ruler = RowRuler::new(SliceSource::new(&bytes), FileEncoding::Utf8);
    for offset in [0u64, 1, 19_999, 20_000, 20_001, 99_999, 100_136] {
        assert_eq!(
            ruler.row_start(offset).expect("slice reads cannot fail"),
            offset - offset % SEGMENT_BYTES,
            "offset {offset}"
        );
    }
}

#[test]
fn a_newline_exactly_on_a_multiple_makes_the_longest_possible_row() {
    // The newline at 20 000 disqualifies the multiple at 40 000 (its window
    // `[20 000, 40 000)` holds the newline), so the row runs to 60 000: 39 999
    // bytes, one short of the `2 × SEGMENT_BYTES` bound.
    let mut bytes = vec![b'x'; 4 * SEGMENT_BYTES as usize];
    bytes[SEGMENT_BYTES as usize] = b'\n';
    let rows = real_rows(&bytes, FileEncoding::Utf8);
    assert_eq!(rows[0], 0..SEGMENT_BYTES);
    assert_eq!(rows[1], SEGMENT_BYTES..SEGMENT_BYTES + 1);
    assert_eq!(rows[2], 20_001..60_000);
    assert_eq!(rows[2].end - rows[2].start, 2 * SEGMENT_BYTES - 1);
    for row in &rows {
        assert!(row.end - row.start < 2 * SEGMENT_BYTES);
    }
}

#[test]
fn a_newline_one_byte_before_a_multiple_keeps_the_multiple_as_a_row_start() {
    // The newline at 19 999 puts the line start exactly on the multiple, so
    // clause 2 supplies the boundary and the row after it is one segment long.
    let mut bytes = vec![b'x'; 4 * SEGMENT_BYTES as usize];
    bytes[SEGMENT_BYTES as usize - 1] = b'\n';
    let rows = real_rows(&bytes, FileEncoding::Utf8);
    assert_eq!(rows[0], 0..SEGMENT_BYTES);
    assert_eq!(rows[1], SEGMENT_BYTES..2 * SEGMENT_BYTES);
}

#[test]
fn a_line_exactly_one_segment_long_is_split_and_a_shorter_one_is_not() {
    // A line of exactly `SEGMENT_BYTES` is NOT "shorter than a segment", so I6
    // doesn't cover it: its window holds no newline, the multiple qualifies, and
    // the line becomes a full row plus a row holding only its newline byte.
    let mut exactly = vec![b'x'; SEGMENT_BYTES as usize];
    exactly.push(b'\n');
    exactly.extend(std::iter::repeat_n(b'y', 10));
    let rows = real_rows(&exactly, FileEncoding::Utf8);
    assert_eq!(rows[0], 0..SEGMENT_BYTES);
    assert_eq!(rows[1], SEGMENT_BYTES..SEGMENT_BYTES + 1);
    assert_eq!(rows[2], SEGMENT_BYTES + 1..SEGMENT_BYTES + 11);

    // One byte shorter and the line survives whole.
    let mut shorter = vec![b'x'; SEGMENT_BYTES as usize - 1];
    shorter.push(b'\n');
    shorter.extend(std::iter::repeat_n(b'y', 10));
    let rows = real_rows(&shorter, FileEncoding::Utf8);
    assert_eq!(rows[0], 0..SEGMENT_BYTES);
    assert_eq!(rows[1], SEGMENT_BYTES..SEGMENT_BYTES + 10);
}

#[test]
fn an_empty_file_has_no_rows() {
    let mut ruler = RowRuler::new(SliceSource::new(&[]), FileEncoding::Utf8);
    assert_eq!(ruler.row_start(0).expect("slice reads cannot fail"), 0);
    assert_eq!(ruler.row_end(0).expect("slice reads cannot fail"), 0);
    assert_eq!(ruler.row_start(999).expect("slice reads cannot fail"), 0);
    assert!(real_rows(&[], FileEncoding::Utf8).is_empty());
}

#[test]
fn a_one_byte_file_is_one_row() {
    assert_eq!(real_rows(b"a", FileEncoding::Utf8), vec![0..1]);
}

#[test]
fn a_utf8_character_straddling_a_multiple_moves_the_boundary_back() {
    // A three-byte `€` covering bytes 19 999-20 001 makes 20 000 an illegal
    // boundary; the row starts at 19 999 instead.
    let mut bytes = vec![b'x'; SEGMENT_BYTES as usize - 1];
    bytes.extend_from_slice("€".as_bytes());
    bytes.extend(std::iter::repeat_n(b'y', SEGMENT_BYTES as usize));
    let rows = real_rows(&bytes, FileEncoding::Utf8);
    assert_eq!(rows[0], 0..SEGMENT_BYTES - 1);
    assert_eq!(rows[1].start, SEGMENT_BYTES - 1);
    assert!(!decode_line(&bytes[..rows[0].end as usize], FileEncoding::Utf8).contains('\u{FFFD}'));
}

#[test]
fn a_utf16_surrogate_pair_straddling_a_multiple_is_never_split() {
    for encoding in [FileEncoding::Utf16Le, FileEncoding::Utf16Be] {
        for bom in [false, true] {
            // Put the pair's first unit at byte 19 998 so the multiple falls
            // between its halves.
            let bom_units = if bom { 1usize } else { 0 };
            let before = (SEGMENT_BYTES as usize / 2 - 1) - bom_units;
            let text = format!("{}😀{}", "a".repeat(before), "b".repeat(20_000));
            let bytes = utf16_bytes(&text, matches!(encoding, FileEncoding::Utf16Le), bom);
            let rows = real_rows(&bytes, encoding);
            assert_eq!(rows[0], 0..SEGMENT_BYTES - 2, "{encoding:?} bom={bom}");
            for row in &rows {
                let text = decode_line(&bytes[row.start as usize..row.end as usize], encoding);
                assert!(
                    !text.contains('\u{FFFD}'),
                    "{encoding:?} bom={bom}: row {row:?} split a surrogate pair"
                );
            }
        }
    }
}

#[test]
fn utf16_boundaries_are_absolute_file_offsets_even_with_a_bom() {
    // FullLoad scans BOM-stripped indices; rows must not. If the grid were anchored
    // past the BOM, a reload or a tail escalation (which swap backends under a live
    // frontend cache) would renumber every row.
    let text = "x".repeat(60_000);
    let with_bom = utf16_bytes(&text, true, true);
    let rows = real_rows(&with_bom, FileEncoding::Utf16Le);
    let starts: Vec<u64> = rows.iter().map(|r| r.start).collect();
    assert_eq!(starts, vec![0, 20_000, 40_000, 60_000, 80_000, 100_000, 120_000]);
}

#[test]
fn crlf_breaks_only_after_the_newline() {
    let mut bytes = Vec::new();
    for _ in 0..4 {
        bytes.extend(std::iter::repeat_n(b'x', 9_998));
        bytes.extend_from_slice(b"\r\n");
    }
    let rows = real_rows(&bytes, FileEncoding::Utf8);
    let starts: Vec<u64> = rows.iter().map(|r| r.start).collect();
    assert_eq!(starts, vec![0, 10_000, 20_000, 30_000]);
    // The `\r` belongs to the row it terminates, exactly as the line readers keep it.
    assert!(bytes[9_998] == b'\r' && rows[0].end == 10_000);
}

// ---------------------------------------------------------------------------
// Bounded work
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ReadLog {
    /// One entry per `read_window` call: (requested length, bytes returned).
    reads: Vec<(u64, u64)>,
}

struct CountingSource<S: RowSource> {
    inner: S,
    log: Rc<RefCell<ReadLog>>,
}

impl<S: RowSource> RowSource for CountingSource<S> {
    fn total_bytes(&self) -> u64 {
        self.inner.total_bytes()
    }

    fn read_window(&mut self, start: u64, buf: &mut [u8]) -> Result<usize, ViewerError> {
        let requested = buf.len() as u64;
        let n = self.inner.read_window(start, buf)?;
        self.log.borrow_mut().reads.push((requested, n as u64));
        Ok(n)
    }
}

#[test]
fn row_start_never_reads_more_than_two_segments() {
    // A 500 KB file with no newline at all: the shape that cost 49 s before rows.
    let bytes = vec![b'x'; 500_000];
    let log = Rc::new(RefCell::new(ReadLog::default()));
    let source = CountingSource {
        inner: SliceSource::new(&bytes),
        log: Rc::clone(&log),
    };
    let mut ruler = RowRuler::new(source, FileEncoding::Utf8);

    for offset in [0u64, 1, 19_999, 20_000, 123_456, 499_999, 500_000] {
        log.borrow_mut().reads.clear();
        let start = ruler.row_start(offset).expect("slice reads cannot fail");
        // No boundary sits at EOF, so a probe there names the last row rather than a
        // row starting where the file ends. 500 000 is 25 segments exactly, so its
        // answer is one segment lower than the grid arithmetic alone would say.
        let expected = if offset == 500_000 {
            500_000 - SEGMENT_BYTES
        } else {
            offset - offset % SEGMENT_BYTES
        };
        assert_eq!(start, expected, "offset {offset}");

        // ❗ ONE read, and it lands inside the file. A byte ceiling alone proves nothing
        // here: `load_window` sizes its own buffer to two segments, so `requested` could
        // not exceed the bound however the rule behaved. What IS falsifiable is the
        // number of reads — a backward scan looking for a newline would show up as many
        // — and the span they touch, which must stay near the probe instead of walking
        // to the file's start.
        let reads = &log.borrow().reads;
        assert_eq!(
            reads.len(),
            1,
            "row_start({offset}) took {} reads, not one",
            reads.len()
        );
        let returned: u64 = reads.iter().map(|(_, n)| *n).sum();
        assert!(
            returned <= MAX_WINDOW_BYTES,
            "row_start({offset}) read {}",
            pluralize(returned, "byte")
        );
        assert!(
            returned < bytes.len() as u64 / 4,
            "row_start({offset}) read {} of a {}-byte file, which is not a bounded read",
            pluralize(returned, "byte"),
            bytes.len()
        );
    }
}

#[test]
fn row_end_never_reads_more_than_two_segments() {
    let bytes = vec![b'x'; 500_000];
    let log = Rc::new(RefCell::new(ReadLog::default()));
    let source = CountingSource {
        inner: SliceSource::new(&bytes),
        log: Rc::clone(&log),
    };
    let mut ruler = RowRuler::new(source, FileEncoding::Utf8);

    for start in [0u64, 20_000, 40_000, 480_000] {
        log.borrow_mut().reads.clear();
        let end = ruler.row_end(start).expect("slice reads cannot fail");
        assert!(end > start);
        let requested: u64 = log.borrow().reads.iter().map(|(r, _)| *r).sum();
        assert!(
            requested <= MAX_WINDOW_BYTES,
            "row_end({start}) asked for {}, the bound is {MAX_WINDOW_BYTES}",
            pluralize(requested, "byte")
        );
    }
}

#[test]
fn the_read_bound_holds_on_a_file_far_larger_than_any_window() {
    // Walking a "huge" file: no call may grow with the file size or the offset.
    let bytes = vec![b'x'; 2_000_000];
    let log = Rc::new(RefCell::new(ReadLog::default()));
    let source = CountingSource {
        inner: SliceSource::new(&bytes),
        log: Rc::clone(&log),
    };
    let mut ruler = RowRuler::new(source, FileEncoding::Utf8);
    let mut start = 0u64;
    while start < 2_000_000 {
        log.borrow_mut().reads.clear();
        start = ruler.row_end(start).expect("slice reads cannot fail");
        for (requested, _) in &log.borrow().reads {
            assert!(
                *requested <= MAX_WINDOW_BYTES,
                "a single read asked for {}",
                pluralize(*requested, "byte")
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The file-backed source agrees with the in-memory one
// ---------------------------------------------------------------------------

#[test]
fn a_file_source_produces_the_same_rows_as_a_slice_source() {
    let dir = TestDir::new("viewer_rows_file_source");
    let path = dir.join("rows.txt");
    let mut bytes = vec![b'x'; 3 * SEGMENT_BYTES as usize];
    bytes[SEGMENT_BYTES as usize] = b'\n';
    bytes[SEGMENT_BYTES as usize * 2 + 5] = b'\n';
    std::fs::write(&path, &bytes).unwrap();

    let expected = real_rows(&bytes, FileEncoding::Utf8);

    let source = super::source::LocalFileSource::shared(&path).unwrap();
    let mut ruler = RowRuler::new(FileSource::new(source, bytes.len() as u64), FileEncoding::Utf8);
    let mut got = Vec::new();
    let mut start = 0u64;
    while start < bytes.len() as u64 {
        let end = ruler.row_end(start).unwrap();
        got.push(start..end);
        start = end;
    }
    assert_eq!(got, expected);

    for row in &expected {
        assert_eq!(ruler.row_start(row.start).unwrap(), row.start);
        assert_eq!(ruler.row_start(row.end - 1).unwrap(), row.start);
    }
}

// ---------------------------------------------------------------------------
// The forward walk reads the SAME boundary set
// ---------------------------------------------------------------------------

/// Every row `RowReader` produces, walking from the file's start.
fn reader_rows(case: &Case, segment: u64) -> Vec<RowSpan> {
    let mut reader = RowReader::with_segment(SliceSource::new(&case.bytes), case.encoding, 0, segment);
    let mut out = Vec::new();
    while let Some(span) = reader.next_span().expect("slice reads cannot fail") {
        assert!(
            span.end > span.start || span.start == case.bytes.len() as u64,
            "{}: zero-length row at {}",
            case.name,
            span.start
        );
        out.push(span);
    }
    out
}

/// ❗ The load-bearing test for the second reading of the rule.
///
/// `RowReader` walks forward off the newline stream instead of probing backward with
/// `RowRuler`, because a two-segment read per row would cost a 4 096-row fetch of an
/// ordinary file 160 MB. That earns it a second implementation ONLY while it provably
/// produces the same boundaries, which is what this asserts over every fixture in the
/// corpus: if it ever goes red, the walk is wrong, not the ruler.
#[test]
fn the_forward_walk_produces_the_same_boundaries_as_the_ruler() {
    for case in tiny_grid_corpus() {
        let expected: Vec<u64> = tiny_rows(&case).iter().map(|r| r.start).collect();
        let walked: Vec<u64> = reader_rows(&case, TEST_SEGMENT)
            .iter()
            .filter(|span| span.start < case.bytes.len() as u64)
            .map(|span| span.start)
            .collect();
        assert_eq!(walked, expected, "{}", case.name);
    }
}

/// The same cross-check at the SHIPPING segment size.
///
/// ❗ The tiny-grid sweep above is exhaustive but runs on a 16-byte grid, so on its own
/// it would let an arithmetic mistake that only bites at 20 000 through: a `u32` that
/// fits 16 and not 20 000, a refill chunk sized against the segment, an off-by-one in
/// the two-segment fill. The fixtures here are the real-grid shapes the rule is most
/// likely to get wrong, so the two readings are bound together at the constant we
/// actually ship, not only at the one that makes the sweep affordable.
#[test]
fn the_forward_walk_matches_the_ruler_at_the_real_segment_size() {
    let mut cases: Vec<Case> = Vec::new();

    // A newline at every position around the first two multiples, where clause 3's
    // window test either holds or spuriously fires.
    for m in [SEGMENT_BYTES, 2 * SEGMENT_BYTES] {
        for delta in -2i64..=2 {
            let mut bytes = vec![b'x'; 3 * SEGMENT_BYTES as usize + 7];
            bytes[(m as i64 + delta) as usize] = b'\n';
            cases.push(ascii_case(&format!("real-newline-at-{m}{delta:+}"), bytes));
        }
    }
    // No newline at all: rows land exactly on the grid.
    cases.push(ascii_case(
        "real-newline-free",
        vec![b'x'; 4 * SEGMENT_BYTES as usize + 137],
    ));
    // Ordinary short lines, which must come out one row per line (invariant I6).
    let mut ordinary = Vec::new();
    while ordinary.len() < 4 * SEGMENT_BYTES as usize {
        ordinary.extend_from_slice(b"a line of perfectly ordinary length\n");
    }
    cases.push(ascii_case("real-ordinary-lines", ordinary));
    // A long line with multibyte characters straddling the grid, in both UTF-8 and
    // UTF-16, so the character-boundary snap is exercised at the real segment too.
    let long_utf8: String = std::iter::repeat_n("→漢🦀", 3 * SEGMENT_BYTES as usize / 4).collect();
    cases.push(ascii_case("real-multibyte-utf8", long_utf8.clone().into_bytes()));
    for (name, le) in [("le", true), ("be", false)] {
        cases.push(Case {
            name: format!("real-multibyte-utf16-{name}"),
            bytes: utf16_bytes(&long_utf8, le, /*bom=*/ false),
            encoding: if le {
                FileEncoding::Utf16Le
            } else {
                FileEncoding::Utf16Be
            },
        });
    }

    for case in cases {
        let expected: Vec<u64> = real_rows(&case.bytes, case.encoding).iter().map(|r| r.start).collect();
        let walked: Vec<u64> = reader_rows(&case, SEGMENT_BYTES)
            .iter()
            .filter(|span| span.start < case.bytes.len() as u64)
            .map(|span| span.start)
            .collect();
        assert_eq!(walked, expected, "{}", case.name);
    }
}

/// ❗ The corpus names its encodings by hand, so a NEW `FileEncoding` variant would
/// silently get no coverage from any property test in this file. This match is
/// exhaustive on purpose: adding a variant stops the build here, and whoever adds it
/// decides in one place whether the rule needs a fixture for it.
///
/// It isn't a runtime assertion because a runtime assertion is one a hurried change can
/// answer with `_ => {}`; a non-exhaustive match is one the compiler answers.
#[test]
fn every_encoding_is_accounted_for_in_the_corpus() {
    fn covered(encoding: FileEncoding) -> bool {
        match encoding {
            // In `tiny_grid_corpus`, each with its own newline framing and character
            // widths, which is what the rule's snaps and `newline_len` turn on.
            FileEncoding::Utf8
            | FileEncoding::Utf8WithBom
            | FileEncoding::Utf16Le
            | FileEncoding::Utf16Be
            | FileEncoding::Windows1252
            | FileEncoding::Iso8859_1 => true,
            // Single-byte encodings the rule cannot tell apart from `Windows1252`: same
            // one-byte newline, same "every byte is a character start", so a fixture
            // would re-run an identical case. Covered by proxy, deliberately.
            FileEncoding::MacRoman | FileEncoding::UsAscii => true,
        }
    }

    for encoding in [
        FileEncoding::Utf8,
        FileEncoding::Utf8WithBom,
        FileEncoding::Utf16Le,
        FileEncoding::Utf16Be,
        FileEncoding::Windows1252,
        FileEncoding::Iso8859_1,
        FileEncoding::MacRoman,
        FileEncoding::UsAscii,
    ] {
        assert!(covered(encoding), "{encoding:?} has no row-rule coverage");
    }
}

/// A row's text is its bytes minus the newline it ended at, and a row Cmdr ended
/// itself keeps every byte. Joining the texts back with the right delimiters has to
/// reproduce the file, which is invariant I3 at the row level.
#[test]
fn walking_the_rows_reproduces_the_file() {
    for case in tiny_grid_corpus() {
        let mut reader = RowReader::with_segment(SliceSource::new(&case.bytes), case.encoding, 0, TEST_SEGMENT);
        let mut rebuilt: Vec<u8> = Vec::new();
        while let Some(span) = reader.next_span().expect("slice reads cannot fail") {
            rebuilt.extend_from_slice(&case.bytes[span.start as usize..span.end as usize]);
        }
        assert_eq!(rebuilt, case.bytes, "{}", case.name);
    }
}

/// Only a row Cmdr ended itself carries the marker. A row that ended at the file's own
/// newline must not, or every copy path puts a break in the clipboard that the file
/// never had.
#[test]
fn only_a_segment_break_marks_a_row_as_continuing() {
    for case in tiny_grid_corpus() {
        let nl_len = reference_newline_len(case.encoding);
        for span in reader_rows(&case, TEST_SEGMENT) {
            if span.start == span.end {
                // An empty file's one row ends nowhere.
                assert!(!span.continues, "{}: an empty file's row must not continue", case.name);
                assert_eq!(span.text_end, span.end, "{}: an empty file's row", case.name);
                continue;
            }
            let ended_at_newline = span.end >= nl_len
                && span.end <= case.bytes.len() as u64
                && reference_newline_units(&case.bytes, case.encoding).contains(&(span.end - nl_len));
            let at_eof = span.end == case.bytes.len() as u64;
            assert_eq!(
                span.continues,
                !ended_at_newline && !at_eof,
                "{}: row {}..{}",
                case.name,
                span.start,
                span.end
            );
            assert_eq!(
                span.text_end,
                if ended_at_newline { span.end - nl_len } else { span.end },
                "{}: row {}..{} text end",
                case.name,
                span.start,
                span.end
            );
        }
    }
}

/// Seeking into the middle of a file lands on the same row the walk from 0 produces,
/// carries the same `starts_line`, and continues identically. A backend seeks per
/// fetch, so a walk that only worked from byte 0 would be no use.
#[test]
fn seeking_lands_on_the_same_rows_as_walking_from_the_start() {
    for case in tiny_grid_corpus() {
        let expected = reader_rows(&case, TEST_SEGMENT);
        let mut reader = RowReader::with_segment(SliceSource::new(&case.bytes), case.encoding, 0, TEST_SEGMENT);
        for (index, row) in expected.iter().enumerate() {
            // Both ends of the row and its middle, rather than every byte of it: the
            // corpus is dozens of files and a full sweep made this the slowest test in
            // the suite, which starved it under a parallel run. The idempotence property
            // itself is `row_start_is_idempotent_from_every_offset_inside_a_row`, which
            // does sweep exhaustively and is cheap because it never walks a row.
            let last = row.end.saturating_sub(1).max(row.start);
            for probe in [row.start, row.start.midpoint(last), last] {
                let landed = reader.seek(probe).expect("slice reads cannot fail");
                assert_eq!(landed, row.start, "{}: probe {probe}", case.name);
                let span = reader
                    .next_span()
                    .expect("slice reads cannot fail")
                    .expect("a row must follow a seek inside the file");
                assert_eq!(span, *row, "{}: probe {probe}", case.name);
                // And the row after it, so a seek's newline evidence is checked too.
                if let Some(next) = expected.get(index + 1) {
                    let after = reader
                        .next_span()
                        .expect("slice reads cannot fail")
                        .expect("a second row must follow");
                    assert_eq!(after, *next, "{}: probe {probe}, second row", case.name);
                }
            }
        }
    }
}

/// Invariant I1 on the walk: a fetch inside a 1 MB single-line file touches kilobytes.
#[test]
fn walking_a_newline_free_file_reads_a_bounded_number_of_bytes() {
    let bytes = vec![b'x'; 1_000_000];
    let log = Rc::new(RefCell::new(ReadLog::default()));
    let source = CountingSource {
        inner: SliceSource::new(&bytes),
        log: Rc::clone(&log),
    };
    let mut reader = RowReader::new(source, FileEncoding::Utf8, 0);
    reader.seek(700_000).expect("slice reads cannot fail");
    let mut rows = Vec::new();
    for _ in 0..3 {
        rows.push(
            reader
                .next_span()
                .expect("slice reads cannot fail")
                .expect("rows follow"),
        );
    }
    assert_eq!(
        rows.iter().map(|r| r.start).collect::<Vec<_>>(),
        vec![700_000, 720_000, 740_000]
    );
    // ❗ Bounded against what the ANSWER costs, not against a number large enough to
    // pass anyway. Three rows are 60 000 bytes of text; the walk is allowed one ruler
    // window to seek and enough refills to cover those rows plus the two-segment
    // lookahead each boundary needs. The old ceiling here was 302 KB for 60 KB of
    // answer, which no plausible regression would have exceeded.
    let read: u64 = log.borrow().reads.iter().map(|(_, got)| got).sum();
    let answer = 3 * SEGMENT_BYTES;
    let allowed = MAX_WINDOW_BYTES + answer + 2 * SEGMENT_BYTES + READ_CHUNK_SLACK;
    assert!(
        read <= allowed,
        "the walk read {} to answer {} of rows from a 1 MB single-line file; the bound \
         is {allowed}",
        pluralize(read, "byte"),
        pluralize(answer, "byte")
    );
    // And the bound has to mean something: it must be far below the file itself, or a
    // walk that read everything from the seek point onward would still pass.
    assert!(
        read < bytes.len() as u64 / 4,
        "the walk read {} of a {}-byte file",
        pluralize(read, "byte"),
        bytes.len()
    );
}

/// A file that ends with a newline has no empty row past it: the newline ends its last
/// row, as `less` and editors count lines. An empty file has exactly one empty row. The
/// last row still says it ended at a newline, which is how a whole-file copy carries it.
#[test]
fn a_file_ending_in_a_newline_has_no_empty_row_past_it() {
    let bytes = b"alpha\nbeta\n".to_vec();
    let case = ascii_case("trailing-newline", bytes.clone());
    let rows = reader_rows(&case, TEST_SEGMENT);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].end, bytes.len() as u64);
    assert_eq!(rows[1].text_bytes(), 4);
    assert!(rows[1].ends_at_newline());

    let none = ascii_case("no-trailing-newline", b"alpha\nbeta".to_vec());
    let rows = reader_rows(&none, TEST_SEGMENT);
    assert_eq!(rows.len(), 2);
    assert!(!rows[1].ends_at_newline());

    let only_newline = ascii_case("only-a-newline", b"\n".to_vec());
    let rows = reader_rows(&only_newline, TEST_SEGMENT);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].ends_at_newline());

    let empty = ascii_case("empty", Vec::new());
    let rows = reader_rows(&empty, TEST_SEGMENT);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].text_bytes(), 0);
}

/// A BOM is not content: the first row starts past it, so all three backends agree on
/// row 0 and none of them hands the user a selectable `U+FEFF`.
#[test]
fn the_first_row_starts_past_a_bom() {
    let bytes = utf16_bytes("alpha\nbeta\n", /*le=*/ true, /*bom=*/ true);
    let mut reader = RowReader::new(SliceSource::new(&bytes), FileEncoding::Utf16Le, 2);
    let (span, text) = reader
        .next_row()
        .expect("slice reads cannot fail")
        .expect("a first row");
    assert_eq!(span.start, 2);
    assert_eq!(text, "alpha");
}

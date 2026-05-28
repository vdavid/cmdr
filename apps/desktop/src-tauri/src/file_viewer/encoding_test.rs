//! Unit + property tests for `encoding.rs`.

use super::encoding::*;

// -- Basic enum metadata --------------------------------------------------------------

#[test]
fn ascii_compatible_excludes_utf16() {
    assert!(FileEncoding::Utf8.is_ascii_newline_compatible());
    assert!(FileEncoding::Utf8WithBom.is_ascii_newline_compatible());
    assert!(FileEncoding::Windows1252.is_ascii_newline_compatible());
    assert!(FileEncoding::Iso8859_1.is_ascii_newline_compatible());
    assert!(FileEncoding::MacRoman.is_ascii_newline_compatible());
    assert!(FileEncoding::UsAscii.is_ascii_newline_compatible());
    assert!(!FileEncoding::Utf16Le.is_ascii_newline_compatible());
    assert!(!FileEncoding::Utf16Be.is_ascii_newline_compatible());
}

#[test]
fn bom_bytes_match_unicode_spec() {
    assert!(FileEncoding::Utf8.bom_bytes().is_empty());
    assert_eq!(FileEncoding::Utf8WithBom.bom_bytes(), &[0xEF, 0xBB, 0xBF]);
    assert_eq!(FileEncoding::Utf16Le.bom_bytes(), &[0xFF, 0xFE]);
    assert_eq!(FileEncoding::Utf16Be.bom_bytes(), &[0xFE, 0xFF]);
    assert!(FileEncoding::Windows1252.bom_bytes().is_empty());
}

// -- same_byte_layout ----------------------------------------------------------------

#[test]
fn same_byte_layout_utf8_to_windows1252_is_instant() {
    assert!(same_byte_layout(FileEncoding::Utf8, FileEncoding::Windows1252));
    assert!(same_byte_layout(FileEncoding::Windows1252, FileEncoding::MacRoman));
    assert!(same_byte_layout(FileEncoding::UsAscii, FileEncoding::Iso8859_1));
}

#[test]
fn same_byte_layout_utf8_to_utf16_is_not_instant() {
    assert!(!same_byte_layout(FileEncoding::Utf8, FileEncoding::Utf16Le));
    assert!(!same_byte_layout(FileEncoding::Utf16Le, FileEncoding::Utf8));
}

#[test]
fn same_byte_layout_utf16_le_be_is_not_instant() {
    // Both are NOT ascii-newline-compatible, so the predicate must return false.
    assert!(!same_byte_layout(FileEncoding::Utf16Le, FileEncoding::Utf16Be));
}

#[test]
fn same_byte_layout_utf8_to_utf8_with_bom_is_not_instant() {
    // BOM bytes differ, so even though both are ASCII-newline-compatible the byte
    // layout shifts by 3 bytes for every codepoint after the BOM. Must NOT be instant.
    assert!(!same_byte_layout(FileEncoding::Utf8, FileEncoding::Utf8WithBom));
}

// -- detect ---------------------------------------------------------------------------

#[test]
fn detect_utf8_bom() {
    let buf = b"\xEF\xBB\xBFhello";
    assert_eq!(detect_from_head(buf), FileEncoding::Utf8WithBom);
}

#[test]
fn detect_utf16_le_bom() {
    let buf = b"\xFF\xFEh\x00e\x00";
    assert_eq!(detect_from_head(buf), FileEncoding::Utf16Le);
}

#[test]
fn detect_utf16_be_bom() {
    let buf = b"\xFE\xFF\x00h\x00e";
    assert_eq!(detect_from_head(buf), FileEncoding::Utf16Be);
}

#[test]
fn detect_plain_ascii_is_utf8() {
    let buf = b"line one\nline two\n";
    assert_eq!(detect_from_head(buf), FileEncoding::Utf8);
}

#[test]
fn detect_valid_utf8_high_codepoints() {
    let buf = "日本語のテキスト".as_bytes();
    assert_eq!(detect_from_head(buf), FileEncoding::Utf8);
}

#[test]
fn detect_high_bit_invalid_utf8_falls_back_to_windows1252() {
    // 0xE4 is "ä" in Windows-1252 / ISO-8859-1, but a lone continuation byte in UTF-8.
    let buf = b"caf\xE9";
    assert_eq!(detect_from_head(buf), FileEncoding::Windows1252);
}

#[test]
fn detect_utf16_le_no_bom_by_parity() {
    // "Hello, world!" in UTF-16 LE without BOM.
    let mut buf = Vec::new();
    for ch in "Hello, world!".encode_utf16() {
        buf.extend_from_slice(&ch.to_le_bytes());
    }
    assert_eq!(detect_from_head(&buf), FileEncoding::Utf16Le);
}

#[test]
fn detect_utf16_be_no_bom_by_parity() {
    let mut buf = Vec::new();
    for ch in "Hello, world!".encode_utf16() {
        buf.extend_from_slice(&ch.to_be_bytes());
    }
    assert_eq!(detect_from_head(&buf), FileEncoding::Utf16Be);
}

#[test]
fn detect_empty_file_is_utf8() {
    assert_eq!(detect_from_head(&[]), FileEncoding::Utf8);
}

// -- detect property test ------------------------------------------------------------

use proptest::prelude::*;

proptest! {
    #[test]
    fn detect_round_trip_utf16_le(s in "[a-zA-Z0-9 ]{32,512}") {
        let mut buf = Vec::new();
        for ch in s.encode_utf16() {
            buf.extend_from_slice(&ch.to_le_bytes());
        }
        prop_assert_eq!(detect_from_head(&buf), FileEncoding::Utf16Le);
    }

    #[test]
    fn detect_round_trip_utf16_be(s in "[a-zA-Z0-9 ]{32,512}") {
        let mut buf = Vec::new();
        for ch in s.encode_utf16() {
            buf.extend_from_slice(&ch.to_be_bytes());
        }
        prop_assert_eq!(detect_from_head(&buf), FileEncoding::Utf16Be);
    }
}

// -- find_newlines (ASCII-compatible fast path) --------------------------------------

#[test]
fn find_newlines_ascii_compatible() {
    let buf = b"line 1\nline 2\nline 3";
    assert_eq!(find_newlines(buf, FileEncoding::Utf8), vec![6, 13]);
    assert_eq!(find_newlines(buf, FileEncoding::Windows1252), vec![6, 13]);
}

proptest! {
    #[test]
    fn find_newlines_utf8_matches_memchr(buf in proptest::collection::vec(any::<u8>(), 0..16_384)) {
        let from_finder = find_newlines(&buf, FileEncoding::Utf8);
        let from_memchr: Vec<usize> = memchr::memchr_iter(b'\n', &buf).collect();
        prop_assert_eq!(from_finder, from_memchr);
    }
}

// -- find_newlines / NewlineScanner (UTF-16) ----------------------------------------

#[test]
fn find_newlines_utf16_le_in_memory() {
    // 'a' '\n' 'b' '\n' in UTF-16 LE: 61 00 0A 00 62 00 0A 00
    let buf = [b'a', 0, b'\n', 0, b'b', 0, b'\n', 0];
    assert_eq!(find_newlines(&buf, FileEncoding::Utf16Le), vec![2, 6]);
}

#[test]
fn find_newlines_utf16_be_in_memory() {
    // 'a' '\n' 'b' '\n' in UTF-16 BE: 00 61 00 0A 00 62 00 0A
    let buf = [0, b'a', 0, b'\n', 0, b'b', 0, b'\n'];
    assert_eq!(find_newlines(&buf, FileEncoding::Utf16Be), vec![3, 7]);
}

#[test]
fn utf16_le_high_byte_0x0a_not_treated_as_newline() {
    // U+010A LATIN CAPITAL LETTER L WITH STROKE in UTF-16 LE = [0x0A, 0x01].
    // The 0x0A byte is the low byte of a pair whose value is 0x010A, NOT 0x000A,
    // so the scanner must not emit.
    let buf = [0x0A, 0x01];
    assert!(find_newlines(&buf, FileEncoding::Utf16Le).is_empty());
}

#[test]
fn utf16_le_misaligned_0x0a_byte_not_a_false_positive() {
    // 'a' (0x61 0x00), U+0A00 (0x00 0x0A): the 0x0A appears as the high byte
    // of the second pair, which doesn't make 0x000A.
    let buf = [0x61, 0x00, 0x00, 0x0A];
    assert!(find_newlines(&buf, FileEncoding::Utf16Le).is_empty());
}

#[test]
fn utf16_le_surrogate_pair_with_0a_byte_not_a_newline() {
    // U+1F40A CROCODILE in UTF-16 LE is the surrogate pair D83D DC0A:
    //   high surrogate D83D in LE = 3D D8
    //   low  surrogate DC0A in LE = 0A DC
    // The low-byte 0x0A of the low surrogate makes pair (0x0A, 0xDC) = 0xDC0A,
    // NOT 0x000A, so it must not be reported.
    let buf = [0x3D, 0xD8, 0x0A, 0xDC];
    assert!(find_newlines(&buf, FileEncoding::Utf16Le).is_empty());
}

#[test]
fn newline_scanner_carry_le_emits_offset_minus_one() {
    // Chunk 1: 'a' 0x00 0x0A. The trailing 0x0A is carried.
    // Chunk 2: 0x00 (the high byte of the U+000A code unit).
    // The 0x0A byte is at absolute offset 2 (= file_offset 3 - 1 when carry is consumed).
    let mut scanner = NewlineScanner::new(FileEncoding::Utf16Le, 0);
    let mut hits: Vec<u64> = Vec::new();
    scanner.feed(&[b'a', 0x00, 0x0A], |off| hits.push(off));
    assert!(hits.is_empty(), "no full pair yet");
    scanner.feed(&[0x00], |off| hits.push(off));
    assert_eq!(hits, vec![2]);
}

#[test]
fn newline_scanner_carry_be_emits_file_offset() {
    // BE: U+000A bytes are 0x00 0x0A (high then low).
    // Chunk 1: 'a' (0x00 0x61) then carry 0x00.
    // Chunk 2: 0x0A '\n' high byte already carried; this 0x0A is the low byte
    // at file_offset 3.
    let mut scanner = NewlineScanner::new(FileEncoding::Utf16Be, 0);
    let mut hits: Vec<u64> = Vec::new();
    scanner.feed(&[0x00, 0x61, 0x00], |off| hits.push(off));
    assert!(hits.is_empty(), "pair incomplete");
    scanner.feed(&[0x0A], |off| hits.push(off));
    assert_eq!(hits, vec![3]);
}

proptest! {
    #[test]
    fn newline_scanner_partitioning_invariant_le(
        buf in proptest::collection::vec(any::<u8>(), 0..2048),
        splits in proptest::collection::vec(0usize..2048, 0..32),
    ) {
        partition_property(&buf, &splits, FileEncoding::Utf16Le)?;
    }

    #[test]
    fn newline_scanner_partitioning_invariant_be(
        buf in proptest::collection::vec(any::<u8>(), 0..2048),
        splits in proptest::collection::vec(0usize..2048, 0..32),
    ) {
        partition_property(&buf, &splits, FileEncoding::Utf16Be)?;
    }
}

fn partition_property(buf: &[u8], splits: &[usize], enc: FileEncoding) -> Result<(), TestCaseError> {
    let expected = find_newlines(buf, enc)
        .into_iter()
        .map(|n| n as u64)
        .collect::<Vec<u64>>();
    let mut scanner = NewlineScanner::new(enc, 0);
    let mut hits: Vec<u64> = Vec::new();
    let mut sorted_splits: Vec<usize> = splits.iter().copied().filter(|s| *s <= buf.len()).collect();
    sorted_splits.sort_unstable();
    sorted_splits.dedup();
    let mut start = 0;
    for split in &sorted_splits {
        let s = *split;
        if s >= start {
            scanner.feed(&buf[start..s], |off| hits.push(off));
            start = s;
        }
    }
    scanner.feed(&buf[start..], |off| hits.push(off));
    prop_assert_eq!(hits, expected);
    Ok(())
}

// -- decode_line ---------------------------------------------------------------------

#[test]
fn decode_line_utf8_passthrough() {
    assert_eq!(decode_line(b"hello", FileEncoding::Utf8), "hello");
}

#[test]
fn decode_line_windows1252_handles_high_bit() {
    // 0xE9 in Windows-1252 is 'é'.
    assert_eq!(decode_line(b"caf\xE9", FileEncoding::Windows1252), "café");
}

#[test]
fn decode_line_utf8_lossy_replaces_invalid() {
    // 0xE9 alone is not valid UTF-8; replaced with U+FFFD.
    assert_eq!(decode_line(b"caf\xE9", FileEncoding::Utf8), "caf\u{FFFD}");
}

#[test]
fn decode_line_utf16_le_hello() {
    let mut buf = Vec::new();
    for ch in "hello".encode_utf16() {
        buf.extend_from_slice(&ch.to_le_bytes());
    }
    assert_eq!(decode_line(&buf, FileEncoding::Utf16Le), "hello");
}

#[test]
fn decode_line_utf16_le_lone_surrogate_replaced() {
    // Lone high surrogate (D83D) without a low surrogate following.
    let buf = [0x3D, 0xD8];
    let decoded = decode_line(&buf, FileEncoding::Utf16Le);
    assert!(
        decoded.contains('\u{FFFD}'),
        "expected replacement char in {:?}",
        decoded
    );
}

#[test]
fn decode_line_empty_returns_empty() {
    assert_eq!(decode_line(b"", FileEncoding::Utf16Le), "");
    assert_eq!(decode_line(b"", FileEncoding::Windows1252), "");
}

#[test]
fn decode_line_mac_roman_smoke() {
    // 0xAE is the registered-trademark sign in Mac Roman.
    let decoded = decode_line(&[0xAE], FileEncoding::MacRoman);
    assert!(!decoded.is_empty(), "Mac Roman 0xAE should decode to something");
}
